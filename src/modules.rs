// The shell's module list, shared by the desktop/web binary (`main.rs`) and the Android
// library (`android.rs`): both crate roots `include!` this file so every module sees the same
// `crate::` tree.

// First, so its `tr!` macros are in scope in every module after it.
#[macro_use]
mod i18n;
mod a11y;
mod actions_panel;
mod aids_dialog;
mod app;
mod autosave;
mod brush_panel;
mod brushtool;
mod colour;
mod colour_panel;
mod envelope;
mod find_panel;
mod publish_panel;
mod erasertool;
mod bind_tool;
mod warp_tool;
mod bone_tool;
mod camera_tool;
mod convert_symbol;
mod layer_depth;
mod layer_props;
mod desktop_ui;
mod dialogs;
mod dock;
mod dock_float;
mod dock_ui;
mod drop;
mod ease_editor;
mod gpu_stage;
#[cfg(not(target_arch = "wasm32"))]
mod gpu_watch;
mod gradient_tool;
mod icons;
mod keymap;
mod library;
mod menus;
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
mod updates;
#[cfg(target_os = "android")]
#[path = "updates_android.rs"]
mod updates;
mod mixed;
mod motion_editor;
mod movie_explorer;
mod panels;
mod motion_path;
mod onion;
mod pathtools;
mod platform;
mod plugin_export_ui;
#[cfg(test)]
mod plugin_fixture_tests;
#[cfg(test)]
mod plugin_ui_tests;
mod plugins_ui;
mod prefs;
mod recent;
mod rulers;
mod scale_grid;
mod shape_envelope;
mod sound;
mod shape_hints;
mod stage;
mod stage_a11y;
mod swatches_panel;
mod start;
mod startup;
mod templates;
mod test_movie;
mod theme;
mod timeline;
#[cfg(target_arch = "wasm32")]
mod web;
mod widthtool;
mod workspaces_window;
mod touch;
mod quick_actions;
