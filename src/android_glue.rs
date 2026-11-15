//! The Android entry point and the JNI bridge to the Kotlin half
//! (`android/app/src/main/kotlin/ai/storyteller/anicraft/`).
//!
//! Rust → Kotlin: methods on `MainActivity` (`openDialog`, `saveDialog`, `haptic`, the keyboard,
//! the clipboard, the loading bar). Kotlin → Rust: the `Java_ai_storyteller_anicraft_NativeBridge_*`
//! functions below. This is the only module in the crate that uses `unsafe`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use android_activity::AndroidApp;
use android_activity::input::{Axis, ImeOptions, InputType, TextInputAction};
use jni::objects::{JClass, JObject, JString, JValue};
use jni::sys::jlong;
use jni::{JNIEnv, JavaVM};

use crate::app::App;
use eframe::App as _;
use crate::platform::android::Hooks;

static APP: OnceLock<AndroidApp> = OnceLock::new();
static CTX: OnceLock<egui::Context> = OnceLock::new();
/// Documents opened from other apps ("Open with"), for the UI thread to open.
static OPENED: Mutex<Vec<String>> = Mutex::new(Vec::new());
static KEYBOARD_SHOWN: AtomicBool = AtomicBool::new(false);

fn wake() {
    if let Some(ctx) = CTX.get() {
        ctx.request_repaint();
    }
}

/// Run `f` on a thread attached to the JVM with the activity as `this`.
fn with_activity<R>(app: &AndroidApp, f: impl FnOnce(&mut JNIEnv, &JObject) -> jni::errors::Result<R>) -> Option<R> {
    // SAFETY: android-activity guarantees both pointers stay valid for the life of the app.
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }.ok()?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
    let mut env = vm.attach_current_thread().ok()?;
    match f(&mut env, &activity) {
        Ok(v) => Some(v),
        Err(e) => {
            log::warn!("JNI call failed: {e}");
            // A pending Java exception would abort the next call.
            let _ = env.exception_clear();
            None
        }
    }
}

fn call_void(app: &AndroidApp, method: &str) {
    with_activity(app, |env, activity| env.call_method(activity, method, "()V", &[]).map(|_| ()));
}

fn set_loading_progress(app: &AndroidApp, percent: i32) {
    with_activity(app, |env, activity| env.call_method(activity, "setLoadingProgress", "(I)V", &[JValue::Int(percent)]).map(|_| ()));
}

fn set_clipboard(app: &AndroidApp, text: &str) {
    with_activity(app, |env, activity| {
        let text: JString = env.new_string(text)?;
        env.call_method(activity, "setClipboardText", "(Ljava/lang/String;)V", &[JValue::Object(&text)]).map(|_| ())
    });
}

/// `platform::android` hook: show a system picker. `true` when Kotlin took the request.
fn show_dialog(id: i64, save: bool, arg: &str) -> bool {
    let Some(app) = APP.get() else { return false };
    with_activity(app, |env, activity| {
        let arg: JString = env.new_string(arg)?;
        let method = if save { "saveDialog" } else { "openDialog" };
        env.call_method(activity, method, "(JLjava/lang/String;)V", &[JValue::Long(id), JValue::Object(&arg)]).map(|_| ())
    })
    .is_some()
}

fn read_string(env: &mut JNIEnv, s: &JString) -> Option<String> {
    if s.as_raw().is_null() {
        return None;
    }
    env.get_string(s).ok().map(String::from)
}

/// `NativeBridge.onDialogAnswer(id: Long, path: String?)`, from a Kotlin worker or the UI thread.
#[unsafe(no_mangle)]
pub extern "system" fn Java_ai_storyteller_anicraft_NativeBridge_onDialogAnswer(mut env: JNIEnv, _class: JClass, id: jlong, path: JString) {
    crate::platform::android::answer(id, read_string(&mut env, &path));
}

/// `NativeBridge.onOpenIntent(path: String)`: a document opened from another app.
#[unsafe(no_mangle)]
pub extern "system" fn Java_ai_storyteller_anicraft_NativeBridge_onOpenIntent(mut env: JNIEnv, _class: JClass, path: JString) {
    if let Some(p) = read_string(&mut env, &path) {
        OPENED.lock().unwrap_or_else(PoisonError::into_inner).push(p);
        wake();
    }
}

fn take_opened() -> Vec<String> {
    std::mem::take(&mut *OPENED.lock().unwrap_or_else(PoisonError::into_inner))
}

/// Entry point: android-activity calls this on its own thread, not the Java UI thread.
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("AniCraft"));
    std::panic::set_hook(Box::new(|info| log::error!("panic: {info}")));
    prepare_environment(&app);
    let _ = APP.set(app.clone());
    crate::platform::android::install(Hooks { dialog: show_dialog, wake });
    set_loading_progress(&app, 15);
    // Mouse wheel axes are off by default in GameActivity.
    app.enable_motion_axis(Axis::Hscroll);
    app.enable_motion_axis(Axis::Vscroll);
    // The embedded interface font joins the text library before anything lays text out.
    anicraft_engine::text::Fonts::register(crate::theme::embedded_fonts());
    set_loading_progress(&app, 35);

    let mut options = eframe::NativeOptions { android_app: Some(app.clone()), renderer: eframe::Renderer::Wgpu, ..Default::default() };
    options.wgpu_options = wgpu_options();
    let host = app.clone();
    let result = eframe::run_native(
        "AniCraft",
        options,
        Box::new(move |cc| {
            log::info!("AniCraft {} for Android", env!("CARGO_PKG_VERSION"));
            match &cc.wgpu_render_state {
                Some(rs) => log::info!("graphics: {:?}", rs.adapter.get_info()),
                None => log::info!("graphics: no wgpu device"),
            }
            let _ = CTX.set(cc.egui_ctx.clone());
            set_loading_progress(&host, 55);
            let mut a = App::new(cc, anicraft_engine::Session::new(anicraft_engine::doc::sample::demo()), None);
            if a.prefs.start_screen {
                crate::start::open(&mut a);
            }
            let h = host.clone();
            a.touch.haptic = Some(Box::new(move || call_void(&h, "haptic")));
            // Single-line fields, no word suggestions (they re-send each word per keystroke).
            host.set_ime_editor_info(InputType::from_bits_retain(0x1 | 0x8_0000), TextInputAction::None, ImeOptions::from_bits_retain(0x4000_0000 | 0x1000_0000));
            set_loading_progress(&host, 85);
            Ok(Box::new(Host { app: a, android: host, loading_hidden: false, last_copied: String::new(), fullscreen: true }))
        }),
    );
    if let Err(e) = result {
        log::error!("AniCraft could not start: {e}");
    }
}

/// `$HOME`, the temp folder and the exports folder must point inside the app's own storage; the
/// desktop defaults (`~/.config`, `/data/local/tmp`) don't exist or aren't writable here. The
/// settings, recovery and plug-in folders then resolve under `<files>/.config/anicraft`.
fn prepare_environment(app: &AndroidApp) {
    let Some(data) = app.internal_data_path() else { return };
    let tmp = data.join("tmp");
    let _ = std::fs::create_dir_all(&tmp);
    let exports = app.external_data_path().unwrap_or_else(|| data.clone()).join("exports");
    let _ = std::fs::create_dir_all(&exports);
    // SAFETY: first thing `android_main` does, before this app spawns any thread of its own.
    unsafe {
        std::env::set_var("HOME", &data);
        std::env::set_var("TMPDIR", &tmp);
        std::env::set_var("ANICRAFT_EXPORTS_DIR", &exports);
        std::env::remove_var("XDG_CONFIG_HOME");
        std::env::remove_var("XDG_DATA_HOME");
    }
}

/// Vulkan first, OpenGL ES as the fallback; the adapter's own texture limit (up to 16384 px) so a
/// modest phone GPU still opens the app, and the desktop's surface-recovery hook (ADR 0092).
fn wgpu_options() -> eframe::egui_wgpu::WgpuConfiguration {
    use eframe::egui_wgpu::WgpuSetup;
    let mut config = eframe::egui_wgpu::WgpuConfiguration::default();
    config.on_surface_status = Arc::new(crate::gpu_watch::on_surface_status);
    if let WgpuSetup::CreateNew(setup) = &mut config.wgpu_setup {
        setup.instance_descriptor.backends = eframe::wgpu::Backends::VULKAN | eframe::wgpu::Backends::GL;
        setup.device_descriptor = Arc::new(|adapter| {
            let limits = adapter.limits();
            eframe::wgpu::DeviceDescriptor {
                label: Some("AniCraft device"),
                required_limits: eframe::wgpu::Limits { max_texture_dimension_2d: limits.max_texture_dimension_2d.min(16384), ..limits },
                ..Default::default()
            }
        });
    }
    config
}

/// The eframe app: the shell plus what only Android needs.
struct Host {
    app: App,
    android: AndroidApp,
    loading_hidden: bool,
    last_copied: String,
    /// What Android was last told for Help › Full Screen (it starts full screen).
    fullscreen: bool,
}

impl eframe::App for Host {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.app.raw_input_hook(ctx, raw);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        for path in take_opened() {
            self.app.request("file.open", serde_json::json!({ "path": path }));
        }
        self.app.ui(ui, frame);
        let ctx = ui.ctx().clone();
        // Help > Full Screen.
        if self.app.prefs.fullscreen != self.fullscreen {
            self.fullscreen = self.app.prefs.fullscreen;
            with_activity(&self.android, |env, activity| env.call_method(activity, "setFullscreen", "(Z)V", &[JValue::Bool(u8::from(self.fullscreen))]).map(|_| ()));
        }
        // The bar's KB button opens the on-screen keyboard by force.
        if std::mem::take(&mut self.app.touch.keyboard_request) {
            call_void(&self.android, "forceShowSoftKeyboard");
            KEYBOARD_SHOWN.store(true, Ordering::Relaxed);
        }
        // The first frame is drawn: take the loading bar away.
        if !self.loading_hidden {
            self.loading_hidden = true;
            set_loading_progress(&self.android, 100);
            call_void(&self.android, "hideLoading");
        }
        // Copy / Cut in a text field -> the system clipboard.
        let copied = ctx.output(|o| {
            o.commands.iter().rev().find_map(|c| match c {
                egui::OutputCommand::CopyText(t) => Some(t.clone()),
                _ => None,
            })
        });
        if let Some(text) = copied.filter(|t| *t != self.last_copied) {
            set_clipboard(&self.android, &text);
            self.last_copied = text;
        }
    }

    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        self.app.on_exit(gl);
    }
}
