//! Touch quick actions.
//!
//! A long press on the stage is a right click: it opens the context menu. Beside that menu a
//! small **Q** tab appears; tapping it opens this panel of compact buttons (icons where the
//! timeline has them: Insert Frame, Keyframe, Blank Keyframe, Copy, Delete), so the panel never
//! covers the menu it sits next to.
//!
//! **Pins.** Long-pressing a button here (a right click on it) pins it to the stage as a small
//! draggable square with its icon or name. Tapping the square runs the command, long-pressing
//! it removes it, and the pins are remembered between runs.
//!
//! Buttons run registry commands through `App::invoke`, exactly like the menus and F5 / F6 / F7,
//! and are enabled by the menus' own rule (`menus::command_enabled`).

use crate::app::App;
use crate::icons::{self, Icon};

/// Commands by group, in the order shown. A command this build does not have is skipped.
const GROUPS: &[(&str, &[&str])] = &[
    ("Timeline", &["timeline.insert_keyframe", "timeline.insert_frame", "timeline.insert_blank_keyframe", "timeline.remove_frame", "timeline.clear_keyframe", "timeline.create_tween", "timeline.create_shape_tween", "timeline.remove_tween"]),
    ("Edit", &["edit.undo", "edit.redo", "edit.cut", "edit.copy", "edit.paste", "edit.paste_in_place", "edit.duplicate", "edit.delete", "edit.select_all", "edit.deselect"]),
    ("Modify", &["modify.convert_to_symbol", "modify.break_apart", "modify.group", "modify.ungroup", "modify.rotate_cw", "modify.rotate_ccw", "modify.flip_h", "modify.flip_v", "arrange.bring_to_front", "arrange.send_to_back"]),
];

/// Commands after which the panel stays open.
const STAYS: &[&str] = &["edit.undo", "edit.redo"];

/// The icon a command has, if the timeline or toolbars already draw one.
fn icon_for(id: &str) -> Option<Icon> {
    Some(match id {
        "timeline.insert_frame" => Icon::InsertFrame,
        "timeline.insert_keyframe" => Icon::InsertKeyframe,
        "timeline.insert_blank_keyframe" => Icon::InsertBlankKeyframe,
        "edit.copy" => Icon::Copy,
        "edit.delete" => Icon::Trash,
        _ => return None,
    })
}

/// A pinned shortcut: a command and where its square sits (screen points).
#[derive(Clone, Debug, PartialEq)]
pub struct Pin {
    pub id: String,
    pub pos: egui::Pos2,
}

#[derive(Default)]
pub struct Quick {
    /// A long press on the stage happened here: the Q tab shows beside the context menu.
    offer: Option<egui::Pos2>,
    open: bool,
    pub pins: Vec<Pin>,
    loaded: bool,
    /// The stage area on screen, set by the stage each frame.
    pub canvas: Option<egui::Rect>,
}

impl Quick {
    /// The stage was long-pressed at `at`: show the Q tab.
    pub fn offer(&mut self, at: egui::Pos2) {
        self.offer = Some(at);
        self.open = false;
    }

    /// Put everything away (the stage was tapped).
    pub fn close(&mut self) {
        self.open = false;
        self.offer = None;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn is_offered(&self) -> bool {
        self.offer.is_some()
    }

    fn pinned(&self, id: &str) -> bool {
        self.pins.iter().any(|p| p.id == id)
    }

    /// Pins `id` (at the next free spot of the stage's right edge), or unpins it.
    fn toggle_pin(&mut self, id: &str, canvas: egui::Rect) {
        if self.pinned(id) {
            self.pins.retain(|p| p.id != id);
        } else {
            let n = self.pins.len();
            let (col, row) = ((n / 5) as f32, (n % 5) as f32);
            let pos = egui::pos2((canvas.right() - 56.0 - col * 50.0).max(canvas.left()), canvas.top() + 60.0 + row * 50.0);
            self.pins.push(Pin { id: id.to_string(), pos });
        }
    }
}

/// Draws the Q tab, the panel and the pins; runs whatever was pressed.
pub fn show(app: &mut App, ctx: &egui::Context) {
    if !app.quick.loaded {
        app.quick.loaded = true;
        app.quick.pins = app.prefs.quick_pins.iter().filter(|(_, x, y)| x.is_finite() && y.is_finite()).map(|(id, x, y)| Pin { id: id.clone(), pos: egui::pos2(*x, *y) }).collect();
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.quick.close();
    }
    let before = app.quick.pins.clone();
    show_tab(app, ctx);
    show_panel(app, ctx);
    show_pins(app, ctx);
    if app.quick.pins != before {
        app.prefs.quick_pins = app.quick.pins.iter().map(|p| (p.id.clone(), p.pos.x, p.pos.y)).collect();
        app.save_prefs();
    }
}

/// The Q tab, to the side of the context menu (right of it, or left near the screen's edge).
fn show_tab(app: &mut App, ctx: &egui::Context) {
    let Some(at) = app.quick.offer else { return };
    let screen = ctx.content_rect();
    let x = if at.x + 290.0 <= screen.right() { at.x + 240.0 } else { (at.x - 56.0).max(screen.left()) };
    let y = at.y.min(screen.bottom() - 44.0).max(screen.top());
    let mut open = false;
    egui::Area::new(egui::Id::new("anicraft_quick_q")).order(egui::Order::Foreground).fixed_pos(egui::pos2(x, y)).show(ctx, |ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        egui::Frame::popup(ui.style()).inner_margin(egui::Margin::same(2)).show(ui, |ui| {
            if ui.add(egui::Button::new(egui::RichText::new("Q").strong().size(16.0)).min_size(egui::vec2(40.0, 34.0))).on_hover_text(tr!("Quick actions")).clicked() {
                open = true;
            }
        });
    });
    if open {
        app.quick.open = true;
        app.quick.offer = None;
    }
}

/// One panel button: its icon if it has one, else its name. Returns the click and whether the
/// finger is held (a right click), which pins it.
fn button(ui: &mut egui::Ui, id: &str, label: &str, on: bool, pinned: bool) -> egui::Response {
    let icon = icon_for(id);
    let size = if icon.is_some() { egui::vec2(36.0, 32.0) } else { egui::vec2(0.0, 30.0) };
    let resp = ui
        .add_enabled_ui(on, |ui| {
            if let Some(icon) = icon {
                let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
                let v = ui.visuals();
                let fill = if pinned { v.selection.bg_fill } else if resp.hovered() { v.widgets.hovered.bg_fill } else { v.widgets.inactive.weak_bg_fill };
                let color = if on { v.text_color() } else { v.weak_text_color() };
                ui.painter().rect_filled(rect, 4.0, fill);
                icons::paint(ui.painter(), egui::Rect::from_center_size(rect.center(), egui::vec2(18.0, 18.0)), icon, color);
                resp
            } else {
                ui.add(egui::Button::new(egui::RichText::new(label).size(12.0)).wrap_mode(egui::TextWrapMode::Extend).selected(pinned).min_size(size))
            }
        })
        .inner;
    resp.on_hover_text(label)
}

fn show_panel(app: &mut App, ctx: &egui::Context) {
    if !app.quick.open {
        return;
    }
    let screen = ctx.content_rect();
    let canvas = app.quick.canvas.unwrap_or(screen).intersect(screen);
    let width = (canvas.width() - 24.0).clamp(220.0, 400.0);
    let mut chosen: Option<&'static str> = None;
    let mut pin: Option<&'static str> = None;
    let mut close = false;
    egui::Area::new(egui::Id::new("anicraft_quick_actions")).order(egui::Order::Foreground).pivot(egui::Align2::CENTER_BOTTOM).fixed_pos(egui::pos2(canvas.center().x, canvas.bottom() - 6.0)).constrain_to(screen).show(ctx, |ui| {
        egui::Frame::popup(ui.style()).inner_margin(egui::Margin::same(6)).show(ui, |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
            // Never wrap inside a word. At the end of a row egui hands a widget whatever width is
            // left, and a label or button that wraps in that sliver breaks into one letter a line.
            // Buttons that don't fit move to the next row whole instead (`horizontal_wrapped`).
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(tr!("Quick actions")).strong().size(12.0));
                ui.add(egui::Label::new(egui::RichText::new(tr!("Hold a button to pin it")).weak().size(11.0)).truncate());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new("✕").min_size(egui::vec2(30.0, 26.0))).clicked() {
                        close = true;
                    }
                });
            });
            egui::ScrollArea::vertical().max_height((canvas.height() * 0.45).max(110.0)).show(ui, |ui| {
                for (title, ids) in GROUPS {
                    ui.label(egui::RichText::new(crate::i18n::t(title)).weak().size(11.0));
                    ui.horizontal_wrapped(|ui| {
                        for id in *ids {
                            let Some(spec) = anicraft_engine::find_command(id) else { continue };
                            let label = crate::i18n::t_cmd(spec.id, spec.label).to_string();
                            let on = crate::menus::command_enabled(app, spec.id);
                            let r = button(ui, spec.id, &label, on, app.quick.pinned(spec.id));
                            if r.secondary_clicked() {
                                pin = Some(spec.id);
                            } else if r.clicked() {
                                chosen = Some(spec.id);
                            }
                        }
                    });
                }
            });
        });
    });
    if let Some(id) = pin {
        app.quick.toggle_pin(id, canvas);
    }
    if let Some(id) = chosen {
        app.invoke(id);
        if !STAYS.contains(&id) {
            close = true;
        }
    }
    if close {
        app.quick.close();
    }
}

/// The pinned squares: tap to run, drag to move, hold to remove.
fn show_pins(app: &mut App, ctx: &egui::Context) {
    if app.quick.pins.is_empty() || !app.touch.enabled {
        return;
    }
    let screen = ctx.content_rect();
    let mut run: Option<&'static str> = None;
    let mut remove: Option<String> = None;
    for i in 0..app.quick.pins.len() {
        let (id, pos) = (app.quick.pins[i].id.clone(), app.quick.pins[i].pos);
        let Some(spec) = anicraft_engine::find_command(&id) else { continue };
        let on = crate::menus::command_enabled(app, spec.id);
        let label = crate::i18n::t_cmd(spec.id, spec.label).to_string();
        let out = egui::Area::new(egui::Id::new(("anicraft_quick_pin", id.as_str()))).order(egui::Order::Foreground).default_pos(pos).constrain_to(screen).movable(true).show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(44.0, 44.0), egui::Sense::click());
            let v = ui.visuals();
            let fill = if resp.hovered() { v.widgets.hovered.bg_fill } else { v.window_fill().gamma_multiply(0.9) };
            let color = if on { v.text_color() } else { v.weak_text_color() };
            ui.painter().rect_filled(rect, 8.0, fill);
            ui.painter().rect_stroke(rect, 8.0, v.window_stroke(), egui::StrokeKind::Inside);
            match icon_for(spec.id) {
                Some(icon) => icons::paint(ui.painter(), egui::Rect::from_center_size(rect.center(), egui::vec2(24.0, 24.0)), icon, color),
                None => {
                    let galley = ui.painter().layout(label.clone(), egui::FontId::proportional(10.0), color, 40.0);
                    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
                }
            }
            resp
        });
        if out.inner.secondary_clicked() {
            remove = Some(id.clone());
        } else if out.inner.clicked() && on {
            run = Some(spec.id);
        }
        if out.response.drag_stopped() && out.response.rect.min != pos {
            app.quick.pins[i].pos = out.response.rect.min;
        }
    }
    if let Some(id) = remove {
        app.quick.pins.retain(|p| p.id != id);
    }
    if let Some(id) = run {
        app.invoke(id);
    }
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
    fn the_three_frame_buttons_lead_the_panel_and_have_icons() {
        let first = GROUPS[0].1;
        assert_eq!(&first[..3], ["timeline.insert_keyframe", "timeline.insert_frame", "timeline.insert_blank_keyframe"]);
        assert!(first[..3].iter().all(|id| icon_for(id).is_some()));
    }

    #[test]
    fn the_tab_offers_then_close_puts_everything_away() {
        let mut q = Quick::default();
        q.offer(egui::pos2(10.0, 10.0));
        assert!(q.is_offered() && !q.is_open());
        q.open = true;
        q.close();
        assert!(!q.is_offered() && !q.is_open());
    }

    #[test]
    fn pinning_toggles_and_stacks_down_the_right_edge() {
        let canvas = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 400.0));
        let mut q = Quick::default();
        q.toggle_pin("edit.copy", canvas);
        q.toggle_pin("edit.delete", canvas);
        assert_eq!(q.pins.len(), 2);
        assert!(q.pins[1].pos.y > q.pins[0].pos.y && q.pins[0].pos.x == q.pins[1].pos.x);
        q.toggle_pin("edit.copy", canvas);
        assert!(!q.pinned("edit.copy") && q.pinned("edit.delete"));
    }
}
