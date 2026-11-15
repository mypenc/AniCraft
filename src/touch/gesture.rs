//! Touch gesture recognizer: fingers in, mouse-style actions out.
//!
//! Pure `std` (no egui types), so it is unit-tested on every host. The egui adapter is in
//! `touch/mod.rs`. The recognizer's design follows WordCraft's Android port; the table differs
//! because AniCraft is a drawing tool: one finger is always *the pointer* (it never scrolls),
//! and navigation belongs to two fingers.
//!
//! | Finger gesture                         | Mouse equivalent                              |
//! |----------------------------------------|-----------------------------------------------|
//! | tap                                    | left click                                    |
//! | press, move past the slop              | left press + drag (draw, move, select)        |
//! | press and hold still (long press)      | right click (context menu) + haptic           |
//! | two fingers moving together            | wheel scroll (pan); egui's multi-touch zooms  |
//! | quick two-finger tap                   | undo                                          |
//! | quick three-finger tap                 | redo                                          |
//! | double tap (optional, off by default)  | undo                                          |
//!
//! The primary press is *deferred* until the finger lifts (a tap) or moves past the slop (a
//! drag). That is what lets a hold become a right click with no stray left click before it,
//! and it also means a second finger that lands a moment after the first never leaves a
//! stray dab on the stage. Hover is never emitted for fingers.

/// A point in egui points.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
}

impl Pt {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    fn dist(self, o: Pt) -> f32 {
        ((self.x - o.x).powi(2) + (self.y - o.y).powi(2)).sqrt()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Btn {
    Primary,
    Secondary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Down,
    Move,
    Up,
    Cancel,
}

/// What the host should do. The adapter turns these into egui events, one pointer button
/// event per frame so egui sees press and release as separate frames.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Out {
    Move(Pt),
    Press(Btn, Pt),
    Release(Btn, Pt),
    /// The pointer left (a finger lifted): clears hover state.
    Gone,
    /// Two-finger pan: scroll `delta` points at `at`.
    Scroll { at: Pt, delta: Pt },
    /// Vibrate briefly (the long press fired).
    Haptic,
    Undo,
    Redo,
}

#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Movement (points) before a press becomes a drag. Larger than a mouse: fingers wobble.
    pub slop: f32,
    /// Seconds a still finger must stay down to count as a long press.
    pub long_press: f64,
    /// Longest two- or three-finger tap, in seconds.
    pub multi_tap: f64,
    /// A second tap this soon (seconds) and this near (points) after the first is a double tap.
    pub double_tap_time: f64,
    pub double_tap_dist: f32,
    /// A double tap undoes. Off by default: double-clicking opens symbols, renames layers and
    /// edits text, and a double tap that undoes would swallow all of that.
    pub double_tap_undo: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self { slop: 10.0, long_press: 0.45, multi_tap: 0.30, double_tap_time: 0.30, double_tap_dist: 28.0, double_tap_undo: false }
    }
}

#[derive(Clone, Copy, Debug)]
struct Finger {
    id: u64,
    pos: Pt,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Idle,
    /// One finger down, primary press not sent yet.
    Pending { origin: Pt, t0: f64 },
    /// Primary pressed and dragging.
    Dragging,
    /// The long press fired (or a stray finger is left over); swallow the rest of this touch.
    Spent,
    /// Two or more fingers.
    Multi { t0: f64, max: usize, travel: f32, last: Pt },
}

#[derive(Debug)]
pub struct Recognizer {
    pub cfg: Config,
    fingers: Vec<Finger>,
    mode: Mode,
    /// When and where the last tap lifted (for the optional double-tap undo).
    last_tap: Option<(f64, Pt)>,
}

impl Default for Recognizer {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

impl Recognizer {
    pub fn new(cfg: Config) -> Self {
        Self { cfg, fingers: Vec::new(), mode: Mode::Idle, last_tap: None }
    }

    /// True while any finger is down: the adapter drops egui-winit's own touch-to-mouse events.
    pub fn active(&self) -> bool {
        !self.fingers.is_empty()
    }

    /// How many fingers are down.
    pub fn count(&self) -> usize {
        self.fingers.len()
    }

    /// When `tick` next has something to do (to schedule a repaint), if ever.
    pub fn deadline(&self) -> Option<f64> {
        match self.mode {
            Mode::Pending { t0, .. } => Some(t0 + self.cfg.long_press),
            _ => None,
        }
    }

    /// The average position of the fingers down, `None` with none.
    pub fn centroid(&self) -> Option<Pt> {
        if self.fingers.is_empty() {
            return None;
        }
        let n = self.fingers.len() as f32;
        let (sx, sy) = self.fingers.iter().fold((0.0, 0.0), |a, f| (a.0 + f.pos.x, a.1 + f.pos.y));
        Some(Pt::new(sx / n, sy / n))
    }

    fn centre(&self) -> Pt {
        self.centroid().unwrap_or_default()
    }

    /// Feed one touch event (`t` in seconds).
    pub fn touch(&mut self, id: u64, phase: Phase, pos: Pt, t: f64) -> Vec<Out> {
        let mut out = Vec::new();
        match phase {
            Phase::Down => {
                self.fingers.retain(|f| f.id != id);
                self.fingers.push(Finger { id, pos });
                if self.fingers.len() == 1 {
                    self.mode = Mode::Pending { origin: pos, t0: t };
                    out.push(Out::Move(pos));
                } else {
                    // A second finger ends any one-finger gesture: pan and pinch take over.
                    if self.mode == Mode::Dragging {
                        out.push(Out::Release(Btn::Primary, pos));
                    }
                    let (t0, max, travel) = match self.mode {
                        Mode::Multi { t0, max, travel, .. } => (t0, max, travel),
                        // A first finger that was already held a while is not a quick tap.
                        Mode::Pending { t0, .. } => (t0, 0, 0.0),
                        _ => (t, 0, 0.0),
                    };
                    let max = max.max(self.fingers.len());
                    self.mode = Mode::Multi { t0, max, travel, last: self.centre() };
                    self.last_tap = None;
                }
            }
            Phase::Move => {
                let Some(f) = self.fingers.iter_mut().find(|f| f.id == id) else { return out };
                f.pos = pos;
                match self.mode {
                    Mode::Pending { origin, .. } if pos.dist(origin) > self.cfg.slop => {
                        out.push(Out::Press(Btn::Primary, origin));
                        out.push(Out::Move(pos));
                        self.mode = Mode::Dragging;
                        self.last_tap = None;
                    }
                    Mode::Dragging => out.push(Out::Move(pos)),
                    Mode::Multi { t0, max, travel, last } => {
                        let c = self.centre();
                        let d = Pt::new(c.x - last.x, c.y - last.y);
                        let travel = travel + (d.x * d.x + d.y * d.y).sqrt();
                        if self.fingers.len() == 2 {
                            out.push(Out::Scroll { at: c, delta: d });
                        }
                        self.mode = Mode::Multi { t0, max, travel, last: c };
                    }
                    _ => {}
                }
            }
            Phase::Up | Phase::Cancel => {
                let cancel = phase == Phase::Cancel;
                let was = self.mode;
                self.fingers.retain(|f| f.id != id);
                match was {
                    Mode::Pending { origin, .. } => {
                        let again = self.last_tap.is_some_and(|(t1, p1)| t - t1 <= self.cfg.double_tap_time && origin.dist(p1) <= self.cfg.double_tap_dist);
                        if cancel {
                            // nothing
                        } else if self.cfg.double_tap_undo && again {
                            out.push(Out::Undo);
                            self.last_tap = None;
                        } else {
                            out.push(Out::Press(Btn::Primary, origin));
                            out.push(Out::Release(Btn::Primary, origin));
                            self.last_tap = Some((t, origin));
                        }
                        out.push(Out::Gone);
                    }
                    Mode::Dragging => {
                        out.push(Out::Release(Btn::Primary, pos));
                        out.push(Out::Gone);
                    }
                    Mode::Spent => out.push(Out::Gone),
                    Mode::Multi { t0, max, travel, .. } => {
                        if self.fingers.is_empty() {
                            let quick = t - t0 <= self.cfg.multi_tap && travel <= self.cfg.slop * 2.0;
                            if !cancel && quick {
                                match max {
                                    2 => out.push(Out::Undo),
                                    3 => out.push(Out::Redo),
                                    _ => {}
                                }
                            }
                            out.push(Out::Gone);
                        }
                    }
                    Mode::Idle => {}
                }
                if self.fingers.is_empty() {
                    self.mode = Mode::Idle;
                } else if let Mode::Multi { t0, max, travel, .. } = was {
                    self.mode = Mode::Multi { t0, max, travel, last: self.centre() };
                } else {
                    // A stray finger left over from an odd sequence: ignore it until all lift.
                    self.mode = Mode::Spent;
                }
            }
        }
        out
    }

    /// Call every frame (and at `deadline`): fires the long press.
    pub fn tick(&mut self, t: f64) -> Vec<Out> {
        if let Mode::Pending { origin, t0 } = self.mode
            && t - t0 >= self.cfg.long_press
        {
            self.mode = Mode::Spent;
            self.last_tap = None;
            return vec![Out::Move(origin), Out::Haptic, Out::Press(Btn::Secondary, origin), Out::Release(Btn::Secondary, origin)];
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: Pt = Pt::new(100.0, 100.0);
    const B: Pt = Pt::new(160.0, 100.0);

    fn has(out: &[Out], o: Out) -> bool {
        out.contains(&o)
    }

    #[test]
    fn tap_is_a_left_click_with_no_hover_left_behind() {
        let mut r = Recognizer::default();
        r.touch(1, Phase::Down, A, 0.0);
        let up = r.touch(1, Phase::Up, A, 0.1);
        assert_eq!(up, vec![Out::Press(Btn::Primary, A), Out::Release(Btn::Primary, A), Out::Gone]);
        assert!(!r.active());
    }

    #[test]
    fn nothing_is_pressed_while_a_finger_rests() {
        let mut r = Recognizer::default();
        let down = r.touch(1, Phase::Down, A, 0.0);
        assert!(!down.iter().any(|o| matches!(o, Out::Press(..))));
        assert!(r.tick(0.2).is_empty());
    }

    #[test]
    fn hold_is_a_right_click_and_never_a_left_click() {
        let mut r = Recognizer::default();
        r.touch(1, Phase::Down, A, 0.0);
        assert_eq!(r.deadline(), Some(0.45));
        assert!(r.tick(0.44).is_empty());
        let fired = r.tick(0.46);
        assert!(has(&fired, Out::Haptic));
        assert!(has(&fired, Out::Press(Btn::Secondary, A)));
        assert!(has(&fired, Out::Release(Btn::Secondary, A)));
        let up = r.touch(1, Phase::Up, A, 0.9);
        assert!(!up.iter().any(|o| matches!(o, Out::Press(..) | Out::Release(..))), "lifting after a hold must not click");
        assert!(r.tick(1.5).is_empty(), "it fires once only");
    }

    #[test]
    fn moving_past_the_slop_starts_a_drag_from_the_original_point() {
        let mut r = Recognizer::default();
        r.touch(1, Phase::Down, A, 0.0);
        assert!(r.touch(1, Phase::Move, Pt::new(105.0, 100.0), 0.05).is_empty(), "inside the slop");
        let out = r.touch(1, Phase::Move, Pt::new(130.0, 100.0), 0.1);
        assert_eq!(out[0], Out::Press(Btn::Primary, A), "the press lands where the finger went down");
        assert_eq!(out[1], Out::Move(Pt::new(130.0, 100.0)));
        let end = r.touch(1, Phase::Up, Pt::new(150.0, 100.0), 0.3);
        assert_eq!(end[0], Out::Release(Btn::Primary, Pt::new(150.0, 100.0)));
    }

    #[test]
    fn a_drag_never_becomes_a_long_press() {
        let mut r = Recognizer::default();
        r.touch(1, Phase::Down, A, 0.0);
        r.touch(1, Phase::Move, Pt::new(200.0, 100.0), 0.1);
        assert!(r.tick(2.0).is_empty());
        assert_eq!(r.deadline(), None);
    }

    #[test]
    fn one_finger_never_scrolls() {
        let mut r = Recognizer::default();
        r.touch(1, Phase::Down, A, 0.0);
        let out: Vec<Out> = (1..10).flat_map(|i| r.touch(1, Phase::Move, Pt::new(100.0, 100.0 + 20.0 * i as f32), 0.01 * i as f64)).collect();
        assert!(!out.iter().any(|o| matches!(o, Out::Scroll { .. })), "a finger is the tool, not a scroll bar");
    }

    #[test]
    fn two_finger_drag_scrolls_and_sends_no_clicks() {
        let mut r = Recognizer::default();
        r.touch(1, Phase::Down, A, 0.0);
        r.touch(2, Phase::Down, B, 0.01);
        let out = r.touch(1, Phase::Move, Pt::new(100.0, 140.0), 0.1);
        let Out::Scroll { delta, .. } = out[0] else { panic!("{out:?}") };
        assert_eq!(delta, Pt::new(0.0, 20.0), "the centroid moved half of one finger's travel");
        let end = r.touch(1, Phase::Up, Pt::new(100.0, 140.0), 0.6);
        assert!(!end.iter().any(|o| matches!(o, Out::Press(..) | Out::Undo | Out::Redo)));
    }

    #[test]
    fn second_finger_releases_a_drag_in_progress() {
        let mut r = Recognizer::default();
        r.touch(1, Phase::Down, A, 0.0);
        r.touch(1, Phase::Move, Pt::new(150.0, 100.0), 0.1);
        let out = r.touch(2, Phase::Down, Pt::new(300.0, 300.0), 0.2);
        assert!(matches!(out[0], Out::Release(Btn::Primary, _)));
    }

    #[test]
    fn a_second_finger_soon_after_the_first_leaves_no_dab() {
        let mut r = Recognizer::default();
        let mut all = r.touch(1, Phase::Down, A, 0.0);
        all.extend(r.touch(2, Phase::Down, B, 0.05));
        all.extend(r.touch(1, Phase::Move, Pt::new(100.0, 130.0), 0.1));
        assert!(!all.iter().any(|o| matches!(o, Out::Press(..))), "no press before it is known to be a pinch: {all:?}");
    }

    #[test]
    fn two_finger_tap_is_undo_three_is_redo() {
        let mut r = Recognizer::default();
        r.touch(1, Phase::Down, A, 0.0);
        r.touch(2, Phase::Down, B, 0.02);
        r.touch(1, Phase::Up, A, 0.1);
        let out = r.touch(2, Phase::Up, B, 0.12);
        assert!(has(&out, Out::Undo));

        r.touch(1, Phase::Down, A, 1.0);
        r.touch(2, Phase::Down, B, 1.01);
        r.touch(3, Phase::Down, Pt::new(220.0, 100.0), 1.02);
        r.touch(1, Phase::Up, A, 1.1);
        r.touch(2, Phase::Up, B, 1.11);
        let out = r.touch(3, Phase::Up, Pt::new(220.0, 100.0), 1.12);
        assert!(has(&out, Out::Redo));
    }

    #[test]
    fn a_slow_two_finger_rest_is_not_undo() {
        let mut r = Recognizer::default();
        r.touch(1, Phase::Down, A, 0.0);
        r.touch(2, Phase::Down, B, 0.02);
        r.touch(1, Phase::Up, A, 0.8);
        let out = r.touch(2, Phase::Up, B, 0.82);
        assert!(!has(&out, Out::Undo));
    }

    #[test]
    fn a_long_hold_then_a_second_finger_does_not_undo_on_release() {
        let mut r = Recognizer::default();
        r.touch(1, Phase::Down, A, 0.0);
        r.touch(2, Phase::Down, B, 0.40);
        r.touch(1, Phase::Up, A, 0.42);
        let out = r.touch(2, Phase::Up, B, 0.43);
        assert!(!has(&out, Out::Undo));
    }

    #[test]
    fn cancel_never_clicks() {
        let mut r = Recognizer::default();
        r.touch(1, Phase::Down, A, 0.0);
        assert_eq!(r.touch(1, Phase::Cancel, A, 0.05), vec![Out::Gone]);
    }

    #[test]
    fn two_quick_taps_are_two_clicks_for_egui_to_pair_as_a_double_click() {
        let mut r = Recognizer::default();
        let mut clicks = 0;
        for (i, t) in [0.0, 0.2].into_iter().enumerate() {
            r.touch(i as u64, Phase::Down, A, t);
            let o = r.touch(i as u64, Phase::Up, A, t + 0.05);
            clicks += o.iter().filter(|o| matches!(o, Out::Release(Btn::Primary, _))).count();
        }
        assert_eq!(clicks, 2, "by default a double tap is a double click (symbols open, text edits)");
    }

    #[test]
    fn double_tap_undo_when_switched_on() {
        let mut r = Recognizer::new(Config { double_tap_undo: true, ..Config::default() });
        r.touch(1, Phase::Down, A, 0.0);
        r.touch(1, Phase::Up, A, 0.05);
        r.touch(2, Phase::Down, Pt::new(105.0, 102.0), 0.2);
        let out = r.touch(2, Phase::Up, Pt::new(105.0, 102.0), 0.25);
        assert!(has(&out, Out::Undo));
        assert!(!out.iter().any(|o| matches!(o, Out::Press(..))), "the second tap is not also a click");
        // The pair is used up: a third tap is an ordinary click again.
        r.touch(3, Phase::Down, A, 0.4);
        assert!(r.touch(3, Phase::Up, A, 0.45).iter().any(|o| matches!(o, Out::Release(Btn::Primary, _))));
    }

    #[test]
    fn slow_or_distant_second_taps_are_not_double_taps() {
        let mut r = Recognizer::new(Config { double_tap_undo: true, ..Config::default() });
        r.touch(1, Phase::Down, A, 0.0);
        r.touch(1, Phase::Up, A, 0.05);
        r.touch(2, Phase::Down, A, 0.9);
        assert!(!has(&r.touch(2, Phase::Up, A, 0.95), Out::Undo), "too slow");
        r.touch(3, Phase::Down, A, 1.0);
        r.touch(3, Phase::Up, A, 1.05);
        r.touch(4, Phase::Down, Pt::new(300.0, 300.0), 1.1);
        assert!(!has(&r.touch(4, Phase::Up, Pt::new(300.0, 300.0), 1.15), Out::Undo), "too far");
    }

    #[test]
    fn centroid_follows_the_fingers() {
        let mut r = Recognizer::default();
        assert_eq!(r.centroid(), None);
        r.touch(1, Phase::Down, A, 0.0);
        r.touch(2, Phase::Down, B, 0.01);
        assert_eq!(r.centroid(), Some(Pt::new(130.0, 100.0)));
        assert_eq!(r.count(), 2);
    }
}
