//! AniCraft for Android.
//!
//! The same engine and egui shell as the desktop app, compiled as a `cdylib` that a
//! GameActivity (`android/`) loads. There is no web view: eframe draws with wgpu (Vulkan, with
//! OpenGL ES as the fallback) straight into the activity's surface. Everything here is
//! `cfg(target_os = "android")`; on other targets this library is empty.
//!
//! What differs from the desktop app:
//!
//! * **Input.** Fingers become mouse events in `touch` (tap = click, long press = right click,
//!   two fingers pan, pinch zooms about the fingers, two-finger tap undoes, three-finger tap
//!   redoes) plus an on-screen Shift/Ctrl/Alt/Space bar. Long-pressing the stage also opens
//!   `quick_actions` (Insert Keyframe / Frame / Blank Keyframe and the rest). A mouse and
//!   keyboard reach egui through the patched winit (`android/patches/`).
//! * **Files.** Android hands apps `content://` URIs, not paths: Open and Save go to Kotlin's
//!   system pickers (`platform::android`, `android_glue`).
//! * **No** self-updater (`updates_android`), second windows, ffmpeg or AppImage integration.
#![cfg(target_os = "android")]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// The shell's desktop-only items stay compiled here but are not all reached.
#![allow(dead_code, unused_imports)]

include!("modules.rs");

// The one module that may use `unsafe`: the JNI calls and `android_main` (see Cargo.toml).
#[allow(unsafe_code)]
mod android_glue;
