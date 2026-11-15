//! Updates on Android: there is no self-updater. An APK is updated by installing the newer one
//! (a release download or the store), so the checks the desktop app runs do nothing here.
//! This keeps the shell's calls (`startup`, `request`, `check`, `show`, `finish`) compiling
//! without craft-update, which replaces the running executable and has no place on a phone.

#[derive(Default)]
pub struct Updates;

impl Updates {
    pub fn startup(&mut self) {}

    pub fn request(&mut self) {}

    pub fn check(&mut self, _ctx: &egui::Context) {}

    /// Never takes over the window.
    pub fn show(&mut self, _ctx: &egui::Context, _safe_to_install: bool) -> bool {
        false
    }

    pub fn finish(&mut self) {}
}

/// Nothing installs itself, so there is no moment that is safe or unsafe for it.
pub(crate) fn safe_to_install(_app: &crate::app::App) -> bool {
    false
}
