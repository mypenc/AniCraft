# AniCraft for Android

The same engine and egui shell as the desktop app, loaded by a GameActivity (`android/`) as a
`cdylib` (`src/android.rs`). No web view: eframe draws with wgpu (Vulkan, OpenGL ES fallback).
The design follows WordCraft's Android port (`mypenc/wordcraft`).

## Touch

`src/touch/` turns fingers into the mouse events the panels already understand, so every tool,
menu and panel works with a finger and none had to be rewritten.

| Gesture | Does |
|---|---|
| tap | left click |
| press and drag | left drag: draw, move, select (one finger is always the tool) |
| **hold** | **right click** (context menu) + haptic tick |
| hold on the stage | right click **and** the quick-action panel (below) |
| two fingers drag | pan (stage, timeline, panels) |
| **pinch** | zoom about the fingers, panning with them in the same motion |
| **two-finger tap** | **undo** |
| three-finger tap | redo |
| double tap | double click (opens symbols, edits text); the bar's **2×** switch makes it undo instead |

The on-screen bar (drag it anywhere; `<` collapses it) holds Shift, Ctrl, Alt and Space (tap =
next tap only, double-tap = locked), Undo, Redo, Enter, Del, Esc and the keyboard.

### Quick actions

Long-pressing the stage opens the context menu and, separately, a panel of finger-sized buttons:
**Insert Keyframe, Insert Frame, Insert Blank Keyframe**, Remove Frame, Clear Keyframe, tweens,
then Edit (undo, redo, cut, copy, paste, duplicate, delete, select), Modify (symbol, break apart,
group, rotate, flip) and Arrange. They run registry commands exactly like the menus and F5/F6/F7,
and are enabled by the menus' own rule. The panel stays until the stage is tapped, Esc, or ✕.

## Files

Android hands apps `content://` URIs. Open copies the picked file into the app's folder; Save and
Export ask where, stage the file in `Android/data/<app>/files/exports/`, and `Exports.kt` mirrors
every later save to the file you named. A PNG sequence goes to a `frames-<time>` folder there.
Video export (ffmpeg) is unavailable.

## Building

CI (`.github/workflows/android.yml`) builds a signed arm64 APK on every push and attaches it to
`v*` tags. Locally: `android/scripts/build.sh [--debug] [--abi arm64-v8a|x86_64]` (Rust 1.95 with
the Android targets, cargo-ndk, NDK r26, JDK 17, Gradle). `vendor-winit.sh` patches winit 0.30.13
so a mouse, keyboard modifiers and the soft keyboard work.

Set the repository secrets `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD` and
`ANDROID_KEY_ALIAS` for a stable signature; without them each build gets a throwaway key.

## Unsafe

The workspace forbids `unsafe`. The shell package uses `deny` instead so that exactly one module,
`src/android_glue.rs` (JNI calls and `android_main`), can opt in. Everywhere else it is still an
error.
