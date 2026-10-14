//! Telling the pen from fingers, after Clip Studio Paint.
//!
//! winit turns every Android pointer into a touch with a force, so the app saw a finger and the S
//! Pen alike. The vendored winit (`third_party/winit/PATCHED.md`) reports each pointer's tool,
//! pressure, tilt, orientation, barrel button and hover through a hook; [`Pens`] turns that into:
//!
//! - **Pen strokes the way desktop tablets work:** the pen's touch events are taken out of the
//!   input (egui still gets its pointer events) and its samples go to the app's [`StylusFeed`]
//!   with real pressure, tilt and the eraser end, so brushes with tilt dynamics follow the S Pen.
//! - **The barrel button** (the S Pen's side button) holds Alt while pressed: Alt + tap with a
//!   painting tool picks a color, as in Photoshop.
//! - **Hover:** a pen held over the screen moves the pointer, so the brush outline follows it.
//! - **Fingers** either draw (with pressure simulated from speed, since a finger has none: fast
//!   strokes thin out) or, with "Finger draws" off, only navigate: one finger pans the canvas
//!   (Space is held for it, the Hand tool), two pinch to zoom, taps still undo and redo.
//!
//! [`StylusFeed`]: photocraft_ui_egui::stylus::StylusFeed

use egui::{Event, Key, Pos2, TouchPhase};
use photocraft_ui_egui::stylus::{PenSample, StylusFeed};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

/// Which tool a pointer is (mirrors winit's `pointer::Tool`, so this module also builds and tests
/// off Android).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Finger,
    Stylus,
    Eraser,
    Mouse,
    Unknown,
}

impl Tool {
    pub fn is_pen(self) -> bool {
        matches!(self, Tool::Stylus | Tool::Eraser)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Down,
    Move,
    Up,
    Cancel,
    Hover,
    HoverExit,
}

/// One pointer of one Android motion event, as the winit hook reports it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub id: u64,
    pub phase: Phase,
    pub tool: Tool,
    /// Physical pixels.
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
    /// Radians from upright.
    pub tilt: f32,
    /// Radians, the lean's direction (0 = toward the top of the screen, clockwise).
    pub orientation: f32,
    pub barrel_button: bool,
}

/// Samples from the event loop's hook, waiting for the next frame.
pub type Queue = Arc<Mutex<Vec<Sample>>>;

/// Most samples kept between two frames (a stalled frame drops the oldest).
const QUEUE_MAX: usize = 4096;

/// Add a sample from the hook.
pub fn push(queue: &Queue, s: Sample) {
    let mut q = queue.lock().unwrap_or_else(PoisonError::into_inner);
    if q.len() >= QUEUE_MAX {
        q.remove(0);
    }
    q.push(s);
}

/// Android's tilt (from upright) and orientation as the W3C tilt angles PhotoCraft uses: degrees
/// toward +x (right) and +y (down, toward the user).
pub fn tilt_xy(tilt: f32, orientation: f32) -> (f32, f32) {
    if !tilt.is_finite() || !orientation.is_finite() {
        return (0.0, 0.0);
    }
    let t = tilt.clamp(0.0, std::f32::consts::FRAC_PI_2 - 1e-3).tan();
    let x = (orientation.sin() * t).atan().to_degrees();
    let y = (-orientation.cos() * t).atan().to_degrees();
    (x.clamp(-90.0, 90.0), y.clamp(-90.0, 90.0))
}

/// A pen sample for the app's stylus feed.
pub fn pen_sample(s: &Sample) -> PenSample {
    let (tilt_x, tilt_y) = tilt_xy(s.tilt, s.orientation);
    PenSample { pressure: s.pressure, tilt_x, tilt_y, rotation: 0.0, eraser: s.tool == Tool::Eraser }.sanitized()
}

/// Finger pressure from speed (points per second): slow is full, fast thins to a floor.
pub fn speed_pressure(speed: f32) -> f32 {
    const FAST: f32 = 2400.0;
    const FLOOR: f32 = 0.3;
    if !speed.is_finite() {
        return 1.0;
    }
    (1.0 - (speed / FAST)).clamp(FLOOR, 1.0)
}

/// How fingers behave (the edge keyboard's "Finger draws" and "Finger pressure" toggles).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TouchSettings {
    /// A finger paints with the current tool; off, fingers only pan and zoom.
    pub finger_draws: bool,
    /// A painting finger's pressure follows its speed; off, it paints at full pressure.
    pub finger_pressure: bool,
}

impl Default for TouchSettings {
    fn default() -> Self {
        Self { finger_draws: true, finger_pressure: true }
    }
}

impl TouchSettings {
    pub fn to_json(self) -> String {
        serde_json::json!({"fingerDraws": self.finger_draws, "fingerPressure": self.finger_pressure}).to_string()
    }

    pub fn from_json(text: &str) -> Self {
        let d = Self::default();
        let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else { return d };
        Self {
            finger_draws: v.get("fingerDraws").and_then(serde_json::Value::as_bool).unwrap_or(d.finger_draws),
            finger_pressure: v.get("fingerPressure").and_then(serde_json::Value::as_bool).unwrap_or(d.finger_pressure),
        }
    }
}

/// A finger's last position, time and smoothed pressure.
#[derive(Clone, Copy, Debug)]
struct FingerTrack {
    pos: Pos2,
    time: f64,
    pressure: f32,
}

/// What [`Pens::filter`] tells the shell about a frame.
#[derive(Debug, Default, PartialEq)]
pub struct PenFrame {
    /// The barrel button is held with the pen down: add Alt.
    pub barrel_alt: bool,
}

/// What [`Pens::filter`] needs to know about the frame.
#[derive(Clone, Copy, Debug)]
pub struct Moment {
    pub settings: TouchSettings,
    /// Where navigating fingers pan (the canvas).
    pub canvas: egui::Rect,
    /// Pixels per point, to convert the hook's pixels.
    pub ppp: f32,
    /// The frame's time in seconds.
    pub now: f64,
}

#[derive(Default)]
pub struct Pens {
    /// The tool of every pointer seen, by id (Android reuses ids; the latest wins).
    tools: HashMap<u64, Tool>,
    fingers: HashMap<u64, FingerTrack>,
    /// Pen contacts down.
    pens_down: Vec<u64>,
    barrel: bool,
    /// Fingers are navigating (Space held for them) until they all lift.
    navigating: bool,
    fingers_down: Vec<u64>,
}

impl Pens {
    pub fn tool(&self, id: u64) -> Tool {
        self.tools.get(&id).copied().unwrap_or(Tool::Unknown)
    }

    /// One frame: take the hook's `samples`, feed the pen to `feed`, and rewrite `events`: the pen's
    /// touch events out, hover in as pointer moves, finger pressure simulated or full, Space held
    /// while fingers navigate.
    pub fn filter(&mut self, samples: Vec<Sample>, events: &mut Vec<Event>, feed: &StylusFeed, at: Moment) -> PenFrame {
        let Moment { settings, canvas, ppp, now } = at;
        let mut frame = PenFrame::default();
        let ppp = if ppp > 0.0 { ppp } else { 1.0 };
        let mut hover: Option<Option<Pos2>> = None;
        for s in &samples {
            self.tools.insert(s.id, s.tool);
            if !s.tool.is_pen() {
                continue;
            }
            match s.phase {
                Phase::Down | Phase::Move => {
                    if !self.pens_down.contains(&s.id) {
                        self.pens_down.push(s.id);
                    }
                    self.barrel = s.barrel_button;
                    feed.set(Some(pen_sample(s)));
                }
                Phase::Up | Phase::Cancel => {
                    self.pens_down.retain(|id| *id != s.id);
                    if self.pens_down.is_empty() {
                        feed.set(None);
                        self.barrel = false;
                    }
                }
                Phase::Hover => hover = Some(Some(Pos2::new(s.x / ppp, s.y / ppp))),
                Phase::HoverExit => hover = Some(None),
            }
        }
        frame.barrel_alt = self.barrel && !self.pens_down.is_empty();

        let mut out = Vec::with_capacity(events.len() + 2);
        let mut space_down = false;
        let mut space_up = false;
        for e in events.drain(..) {
            let Event::Touch { id, phase, pos, force: _, device_id } = e else {
                out.push(e);
                continue;
            };
            let tool = self.tool(id.0);
            if tool.is_pen() {
                // The pen's samples went to the feed; its pointer events stay.
                continue;
            }
            let force = match phase {
                TouchPhase::Start => {
                    if self.fingers_down.is_empty() && !settings.finger_draws && canvas.contains(pos) {
                        self.navigating = true;
                        space_down = true;
                    }
                    if !self.fingers_down.contains(&id.0) {
                        self.fingers_down.push(id.0);
                    }
                    self.fingers.insert(id.0, FingerTrack { pos, time: now, pressure: 0.6 });
                    self.finger_force(id.0, settings)
                }
                TouchPhase::Move => {
                    if let Some(t) = self.fingers.get_mut(&id.0) {
                        // Several moves can share a frame's time: speed is measured across frames.
                        if now > t.time {
                            let speed = (pos - t.pos).length() / (now - t.time) as f32;
                            t.pressure = t.pressure * 0.7 + speed_pressure(speed) * 0.3;
                            t.time = now;
                            t.pos = pos;
                        }
                    }
                    self.finger_force(id.0, settings)
                }
                TouchPhase::End | TouchPhase::Cancel => {
                    // The lift keeps the stroke's last pressure.
                    let f = self.finger_force(id.0, settings);
                    self.fingers.remove(&id.0);
                    self.fingers_down.retain(|d| *d != id.0);
                    if self.fingers_down.is_empty() && self.navigating {
                        self.navigating = false;
                        space_up = true;
                    }
                    f
                }
            };
            out.push(Event::Touch { device_id, id, phase, pos, force });
        }
        if space_down {
            out.insert(0, Event::Key { key: Key::Space, physical_key: Some(Key::Space), pressed: true, repeat: false, modifiers: egui::Modifiers::NONE });
        }
        if space_up {
            out.push(Event::Key { key: Key::Space, physical_key: Some(Key::Space), pressed: false, repeat: false, modifiers: egui::Modifiers::NONE });
        }
        // A hovering pen moves the pointer (no touch is down, so nothing else drives it).
        if self.pens_down.is_empty() && self.fingers_down.is_empty() {
            match hover {
                Some(Some(p)) => out.push(Event::PointerMoved(p)),
                Some(None) => out.push(Event::PointerGone),
                None => {}
            }
        }
        *events = out;
        frame
    }

    /// A finger's force for the canvas: simulated from speed, or full. (Full is 1, not none: once
    /// the stylus code has seen a force it reads a contact without one as pressure 0.)
    fn finger_force(&self, id: u64, settings: TouchSettings) -> Option<f32> {
        if self.navigating {
            return None;
        }
        if !settings.finger_pressure {
            return Some(1.0);
        }
        self.fingers.get(&id).map(|t| t.pressure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{TouchDeviceId, TouchId, pos2};

    fn sample(id: u64, phase: Phase, tool: Tool) -> Sample {
        Sample { id, phase, tool, x: 20.0, y: 40.0, pressure: 0.42, tilt: 0.5, orientation: std::f32::consts::FRAC_PI_2, barrel_button: false }
    }
    fn touch(id: u64, phase: TouchPhase, x: f32) -> Event {
        Event::Touch { device_id: TouchDeviceId(0), id: TouchId(id), phase, pos: pos2(x, 50.0), force: Some(1.0) }
    }
    fn canvas() -> egui::Rect {
        egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 400.0))
    }

    #[test]
    fn tilt_follows_the_lean() {
        assert_eq!(tilt_xy(0.0, 1.0), (0.0, 0.0), "upright");
        let (x, y) = tilt_xy(0.5, std::f32::consts::FRAC_PI_2);
        assert!(x > 25.0 && y.abs() < 1e-3, "leaning right: {x} {y}");
        let (x, y) = tilt_xy(0.5, 0.0);
        assert!(x.abs() < 1e-3 && y < -25.0, "leaning toward the top: {x} {y}");
        assert_eq!(tilt_xy(f32::NAN, 0.0), (0.0, 0.0));
    }

    #[test]
    fn the_pen_feeds_the_stylus_and_leaves_its_touches_out() {
        let mut pens = Pens::default();
        let feed = StylusFeed::default();
        let mut events = vec![touch(7, TouchPhase::Start, 20.0), Event::PointerMoved(pos2(20.0, 50.0))];
        let f = pens.filter(
            vec![sample(7, Phase::Down, Tool::Stylus)],
            &mut events,
            &feed,
            Moment { settings: TouchSettings::default(), canvas: canvas(), ppp: 2.0, now: 0.0 },
        );
        assert!(!f.barrel_alt);
        assert_eq!(events, vec![Event::PointerMoved(pos2(20.0, 50.0))], "touch out, pointer kept");
        let got = feed.drain();
        assert_eq!(got.len(), 1);
        assert!((got[0].pressure - 0.42).abs() < 1e-6 && got[0].tilt_x > 25.0 && !got[0].eraser);

        let mut events = vec![touch(7, TouchPhase::End, 20.0)];
        pens.filter(
            vec![sample(7, Phase::Up, Tool::Stylus)],
            &mut events,
            &feed,
            Moment { settings: TouchSettings::default(), canvas: canvas(), ppp: 2.0, now: 0.1 },
        );
        assert_eq!(feed.get(), None, "lifted");
    }

    #[test]
    fn the_barrel_button_holds_alt_while_the_pen_is_down() {
        let mut pens = Pens::default();
        let feed = StylusFeed::default();
        let s = Sample { barrel_button: true, ..sample(3, Phase::Down, Tool::Stylus) };
        assert!(pens.filter(vec![s], &mut Vec::new(), &feed, Moment { settings: TouchSettings::default(), canvas: canvas(), ppp: 1.0, now: 0.0 }).barrel_alt);
        let up = Sample { barrel_button: true, ..sample(3, Phase::Up, Tool::Stylus) };
        assert!(!pens.filter(vec![up], &mut Vec::new(), &feed, Moment { settings: TouchSettings::default(), canvas: canvas(), ppp: 1.0, now: 0.1 }).barrel_alt);
    }

    #[test]
    fn a_hovering_pen_moves_the_pointer_in_points() {
        let mut pens = Pens::default();
        let mut events = Vec::new();
        pens.filter(
            vec![sample(1, Phase::Hover, Tool::Stylus)],
            &mut events,
            &StylusFeed::default(),
            Moment { settings: TouchSettings::default(), canvas: canvas(), ppp: 2.0, now: 0.0 },
        );
        assert_eq!(events, vec![Event::PointerMoved(pos2(10.0, 20.0))]);
        let mut events = Vec::new();
        pens.filter(
            vec![sample(1, Phase::HoverExit, Tool::Stylus)],
            &mut events,
            &StylusFeed::default(),
            Moment { settings: TouchSettings::default(), canvas: canvas(), ppp: 2.0, now: 0.1 },
        );
        assert_eq!(events, vec![Event::PointerGone]);
    }

    #[test]
    fn a_fast_finger_paints_lighter_and_without_finger_pressure_at_full() {
        assert_eq!(speed_pressure(0.0), 1.0);
        assert!(speed_pressure(1200.0) < 0.6);
        assert_eq!(speed_pressure(1e9), 0.3);

        let mut pens = Pens::default();
        let feed = StylusFeed::default();
        let on = TouchSettings::default();
        let force = |events: &[Event]| events.iter().find_map(|e| if let Event::Touch { force, .. } = e { Some(*force) } else { None }).flatten();
        let mut e = vec![touch(1, TouchPhase::Start, 0.0)];
        pens.filter(vec![sample(1, Phase::Down, Tool::Finger)], &mut e, &feed, Moment { settings: on, canvas: canvas(), ppp: 1.0, now: 0.0 });
        assert!(force(&e).is_some_and(|f| f < 1.0));
        let mut slow = vec![touch(1, TouchPhase::Move, 1.0)];
        pens.filter(Vec::new(), &mut slow, &feed, Moment { settings: on, canvas: canvas(), ppp: 1.0, now: 0.1 });
        let mut fast = vec![touch(1, TouchPhase::Move, 301.0)];
        pens.filter(Vec::new(), &mut fast, &feed, Moment { settings: on, canvas: canvas(), ppp: 1.0, now: 0.2 });
        assert!(force(&fast) < force(&slow), "{:?} < {:?}", force(&fast), force(&slow));

        let off = TouchSettings { finger_pressure: false, ..on };
        let mut e = vec![touch(1, TouchPhase::Move, 302.0)];
        pens.filter(Vec::new(), &mut e, &feed, Moment { settings: off, canvas: canvas(), ppp: 1.0, now: 0.3 });
        assert_eq!(force(&e), Some(1.0), "full pressure");
    }

    #[test]
    fn navigating_fingers_hold_space_over_the_canvas_only() {
        let nav = TouchSettings { finger_draws: false, ..TouchSettings::default() };
        let feed = StylusFeed::default();
        let space = |events: &[Event], down: bool| events.iter().any(|e| matches!(e, Event::Key { key: Key::Space, pressed, .. } if *pressed == down));

        let mut pens = Pens::default();
        let mut e = vec![touch(1, TouchPhase::Start, 50.0)];
        pens.filter(vec![sample(1, Phase::Down, Tool::Finger)], &mut e, &feed, Moment { settings: nav, canvas: canvas(), ppp: 1.0, now: 0.0 });
        assert!(space(&e, true));
        let mut e = vec![touch(1, TouchPhase::End, 60.0)];
        pens.filter(Vec::new(), &mut e, &feed, Moment { settings: nav, canvas: canvas(), ppp: 1.0, now: 0.1 });
        assert!(space(&e, false), "released when the finger lifts");

        // A finger on a panel outside the canvas is an ordinary tap.
        let mut pens = Pens::default();
        let mut e = vec![touch(2, TouchPhase::Start, 900.0)];
        pens.filter(Vec::new(), &mut e, &feed, Moment { settings: nav, canvas: canvas(), ppp: 1.0, now: 0.0 });
        assert!(!space(&e, true));
    }

    #[test]
    fn settings_round_trip_and_survive_junk() {
        let s = TouchSettings { finger_draws: false, finger_pressure: true };
        assert_eq!(TouchSettings::from_json(&s.to_json()), s);
        assert_eq!(TouchSettings::from_json("not json"), TouchSettings::default());
        assert_eq!(TouchSettings::from_json("{\"fingerDraws\": false}"), TouchSettings { finger_draws: false, finger_pressure: true });
    }
}
