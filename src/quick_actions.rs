//! Touch quick actions: the stage's long press (a right click, see `touch`) opens the context
//! menu, and *beside it* this panel of finger-sized buttons for what is needed most while
//! animating: Insert Keyframe, Insert Frame, Insert Blank Keyframe, then the rest of the
//! frame, edit, modify and arrange commands.
//!
//! The buttons run registry commands through `App::invoke`, exactly as the menus and shortcuts
//! do, and are enabled by the menus' own rule (`menus::command_enabled`), so F5 / F6 / F7
//! behave the same here (at the playhead, or over the frame selection).
//!
//! The panel stays until the user taps the stage again, presses Esc or closes it, so several
//! actions can follow one long press.

use crate::app::App;

/// Commands by group, in the order shown. A command this build does not have is skipped.
const GROUPS: &[(&str, &[&str])] = &[
    // The three people ask for first.
    ("Timeline", &["timeline.insert_keyframe", "timeline.insert_frame", "timeline.insert_blank_keyframe", "timeline.remove_frame", "timeline.clear_keyframe", "timeline.create_tween", "timeline.create_shape_tween", "timeline.remove_tween"]),
    ("Edit", &["edit.undo", "edit.redo", "edit.cut", "edit.copy", "edit.paste", "edit.paste_in_place", "edit.duplicate", "edit.delete", "edit.select_all", "edit.deselect"]),
    ("Modify", &["modify.convert_to_symbol", "modify.break_apart", "modify.group", "modify.ungroup", "modify.rotate_cw", "modify.rotate_ccw", "modify.flip_h", "modify.flip_v", "arrange.bring_to_front", "arrange.send_to_back"]),
];

/// Commands after which the panel stays open (the user is likely to want another go).
const STAYS: &[&str] = &["edit.undo", "edit.redo"];

#[derive(Default)]
pub struct Quick {
    open: bool,
    /// The stage area on screen, set by the stage each frame: the panel sits at its bottom.
    pub canvas: Option<egui::Rect>,
}

impl Quick {
    pub fn open(&mut self) {
        self.open = true;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }
}

/// Draws the panel if it is open and runs the button pressed.
pub fn show(app: &mut App, ctx: &egui::Context) {
    if !app.quick.open {
        return;
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.quick.close();
        return;
    }
    let screen = ctx.content_rect();
    let canvas = app.quick.canvas.unwrap_or(screen).intersect(screen);
    let width = (canvas.width() - 24.0).clamp(240.0, 620.0);
    let mut chosen: Option<&'static str> = None;
    let mut close = false;
    egui::Area::new(egui::Id::new("anicraft_quick_actions"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::CENTER_BOTTOM)
        .fixed_pos(egui::pos2(canvas.center().x, canvas.bottom() - 8.0))
        .constrain_to(screen)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(width);
                ui.horizontal(|ui| {
                    ui.strong(tr!("Quick actions"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new("✕").min_size(egui::vec2(40.0, 34.0))).clicked() {
                            close = true;
                        }
                    });
                });
                egui::ScrollArea::vertical().max_height((canvas.height() * 0.55).max(120.0)).show(ui, |ui| {
                    for (title, ids) in GROUPS {
                        ui.add_space(2.0);
                        ui.label(egui::RichText::new(crate::i18n::t(title)).small().weak());
                        ui.horizontal_wrapped(|ui| {
                            for id in *ids {
                                let Some(spec) = anicraft_engine::find_command(id) else { continue };
                                let label = crate::i18n::t_cmd(spec.id, spec.label).to_string();
                                let on = crate::menus::command_enabled(app, spec.id);
                                if ui.add_enabled(on, egui::Button::new(label).min_size(egui::vec2(0.0, 40.0))).clicked() {
                                    chosen = Some(spec.id);
                                }
                            }
                        });
                    }
                });
            });
        });
    if let Some(id) = chosen {
        app.invoke(id);
        if !STAYS.contains(&id) {
            close = true;
        }
    }
    if close {
        app.quick.close();
    }
    ctx.request_repaint();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every id in the panel is a command the engine has, so no button is silently dropped.
    #[test]
    fn every_quick_action_is_a_real_command() {
        for (title, ids) in GROUPS {
            for id in *ids {
                assert!(anicraft_engine::find_command(id).is_some(), "{title}: {id} is not a registry command");
            }
        }
    }

    #[test]
    fn the_three_frame_buttons_lead_the_panel() {
        let first = GROUPS[0].1;
        assert_eq!(&first[..3], ["timeline.insert_keyframe", "timeline.insert_frame", "timeline.insert_blank_keyframe"]);
    }

    #[test]
    fn open_and_close() {
        let mut q = Quick::default();
        assert!(!q.is_open());
        q.open();
        assert!(q.is_open());
        q.close();
        assert!(!q.is_open());
    }
}
