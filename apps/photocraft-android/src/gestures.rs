//! Finger gestures for a touch screen, the way Clip Studio Paint and Procreate do them: a
//! two-finger tap undoes, a three-finger tap redoes, two fingers pinch and pan.
//!
//! egui-winit turns the first finger on the screen into a mouse (a pointer press) as well as a
//! touch. [`Gate`] lets that mouse through at once, so a painting finger starts with no delay. If
//! a second finger lands, the session becomes a gesture: the rest of its pointer events are
//! dropped, the first finger's press is ended with a release so the stroke doesn't hang open, and
//! when the second finger came quickly ([`CANCEL_SECS`]: it was meant as a gesture from the
//! start) the gate asks the shell to take back the little that was painted ([`Filtered::cancel`]).
//! A second finger that lands later, mid-stroke, ends the stroke and keeps it.
//!
//! The touch events themselves always go through, so egui's pinch-zoom and two-finger pan work.

use egui::{Event, Pos2, TouchPhase};
use std::collections::HashMap;

/// A second finger within this long of the first makes what the first one painted a gesture's
/// accident, taken back; later it's a deliberate stroke, kept.
pub const CANCEL_SECS: f64 = 0.3;
/// A tap is shorter than this.
pub const TAP_SECS: f64 = 0.35;
/// A tap's fingers move less than this (points).
pub const TAP_SLOP: f32 = 18.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tap {
    /// Two fingers: Undo.
    Two,
    /// Three fingers: Redo.
    Three,
}

/// What one frame's events became.
#[derive(Debug, Default, PartialEq)]
pub struct Filtered {
    pub events: Vec<Event>,
    /// A multi-finger tap ended this frame.
    pub tap: Option<Tap>,
    /// A touch session started this frame (the moment to remember what the document was).
    pub session_started: bool,
    /// The session turned into a gesture right after its first finger started painting: undo
    /// what that finger did (its stroke ends with this frame's release).
    pub cancel: bool,
}

#[derive(Default)]
pub struct Gate {
    /// Fingers down: where each started and the farthest it has moved.
    down: HashMap<u64, (Pos2, f32)>,
    session_start: f64,
    most_fingers: usize,
    moved: bool,
    /// This session had two or more fingers at once: its pointer events no longer reach the app.
    multi: bool,
    /// The first finger's press reached the app and hasn't been released yet.
    pressed: bool,
    /// Where the pointer last was, for the release that ends a cut-off stroke.
    last_pos: Option<Pos2>,
}

fn is_pointer(e: &Event) -> bool {
    matches!(e, Event::PointerMoved(_) | Event::PointerButton { .. } | Event::PointerGone)
}

impl Gate {
    /// Pass one frame's input events through the gate at time `now` (seconds).
    pub fn filter(&mut self, events: Vec<Event>, now: f64) -> Filtered {
        let mut f = Filtered::default();
        for event in events {
            match &event {
                Event::Touch { id, phase, pos, .. } => {
                    let (id, phase, pos) = (id.0, *phase, *pos);
                    match phase {
                        TouchPhase::Start => {
                            if self.down.is_empty() {
                                self.session_start = now;
                                self.most_fingers = 0;
                                self.moved = false;
                                self.multi = false;
                                f.session_started = true;
                            }
                            self.down.insert(id, (pos, 0.0));
                            self.most_fingers = self.most_fingers.max(self.down.len());
                            if self.down.len() >= 2 && !self.multi {
                                self.multi = true;
                                if self.pressed {
                                    // End the first finger's stroke now; take it back if it was
                                    // just the start of the gesture.
                                    let at = self.last_pos.unwrap_or(pos);
                                    f.events.push(Event::PointerButton {
                                        pos: at,
                                        button: egui::PointerButton::Primary,
                                        pressed: false,
                                        modifiers: egui::Modifiers::NONE,
                                    });
                                    f.events.push(Event::PointerGone);
                                    self.pressed = false;
                                    f.cancel = now - self.session_start <= CANCEL_SECS;
                                }
                            }
                        }
                        TouchPhase::Move => {
                            if let Some((start, far)) = self.down.get_mut(&id) {
                                *far = far.max(start.distance(pos));
                                if *far > TAP_SLOP {
                                    self.moved = true;
                                }
                            }
                        }
                        TouchPhase::End | TouchPhase::Cancel => {
                            self.down.remove(&id);
                            if self.down.is_empty() && self.multi && !self.moved && phase == TouchPhase::End && now - self.session_start <= TAP_SECS {
                                f.tap = match self.most_fingers {
                                    2 => Some(Tap::Two),
                                    3 => Some(Tap::Three),
                                    _ => None,
                                };
                            }
                        }
                    }
                    f.events.push(event);
                }
                e if is_pointer(e) => {
                    if self.multi {
                        // The gesture's emulated mouse: dropped, including its release.
                        if matches!(e, Event::PointerGone) && self.down.is_empty() {
                            self.multi = false;
                        }
                        continue;
                    }
                    match e {
                        Event::PointerMoved(p) => self.last_pos = Some(*p),
                        Event::PointerButton { pos, pressed, button: egui::PointerButton::Primary, .. } => {
                            self.last_pos = Some(*pos);
                            self.pressed = *pressed;
                        }
                        Event::PointerGone => self.pressed = false,
                        _ => {}
                    }
                    f.events.push(event);
                }
                _ => f.events.push(event),
            }
        }
        f
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Modifiers, PointerButton, TouchDeviceId, TouchId, pos2};

    fn touch(id: u64, phase: TouchPhase, x: f32, y: f32) -> Event {
        Event::Touch { device_id: TouchDeviceId(0), id: TouchId(id), phase, pos: pos2(x, y), force: Some(0.5) }
    }
    fn press(pressed: bool) -> Event {
        Event::PointerButton { pos: pos2(10.0, 10.0), button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }
    }
    fn moved(x: f32) -> Event {
        Event::PointerMoved(pos2(x, 10.0))
    }
    fn pointers(events: &[Event]) -> usize {
        events.iter().filter(|e| is_pointer(e)).count()
    }
    fn releases(events: &[Event]) -> usize {
        events.iter().filter(|e| matches!(e, Event::PointerButton { pressed: false, .. })).count()
    }

    #[test]
    fn a_finger_paints_at_once() {
        let mut g = Gate::default();
        let a = g.filter(vec![touch(1, TouchPhase::Start, 10.0, 10.0), moved(10.0), press(true)], 0.0);
        assert!(a.session_started);
        assert_eq!(pointers(&a.events), 2, "no waiting");
        let b = g.filter(vec![touch(1, TouchPhase::Move, 14.0, 10.0), moved(14.0), touch(1, TouchPhase::End, 14.0, 10.0), press(false), Event::PointerGone], 0.5);
        assert_eq!(pointers(&b.events), 3);
        assert!(!b.cancel && b.tap.is_none());
    }

    #[test]
    fn a_two_finger_tap_ends_the_dab_takes_it_back_and_undoes() {
        let mut g = Gate::default();
        g.filter(vec![touch(1, TouchPhase::Start, 10.0, 10.0), moved(10.0), press(true)], 0.0);
        let b = g.filter(vec![touch(2, TouchPhase::Start, 60.0, 10.0)], 0.03);
        assert!(b.cancel, "the second finger came at once: take the dab back");
        assert_eq!(releases(&b.events), 1, "the stroke is ended, not left hanging");
        let c = g.filter(vec![touch(1, TouchPhase::End, 11.0, 10.0), touch(2, TouchPhase::End, 61.0, 10.0), press(false), Event::PointerGone], 0.15);
        assert_eq!(c.tap, Some(Tap::Two));
        assert_eq!(pointers(&c.events), 0, "the real release and PointerGone are dropped");
        // The next single touch is a mouse again.
        let d = g.filter(vec![touch(3, TouchPhase::Start, 5.0, 5.0), press(true)], 1.0);
        assert_eq!(pointers(&d.events), 1);
    }

    #[test]
    fn a_late_second_finger_ends_the_stroke_and_keeps_it() {
        let mut g = Gate::default();
        g.filter(vec![touch(1, TouchPhase::Start, 10.0, 10.0), press(true)], 0.0);
        g.filter(vec![touch(1, TouchPhase::Move, 200.0, 10.0), moved(200.0)], 0.6);
        let r = g.filter(vec![touch(2, TouchPhase::Start, 60.0, 10.0)], 0.9);
        assert!(!r.cancel, "a deliberate stroke stays");
        assert_eq!(releases(&r.events), 1);
    }

    #[test]
    fn three_fingers_redo_and_a_moving_pair_is_no_tap() {
        let mut g = Gate::default();
        g.filter(vec![touch(1, TouchPhase::Start, 0.0, 0.0), touch(2, TouchPhase::Start, 50.0, 0.0), touch(3, TouchPhase::Start, 100.0, 0.0)], 0.0);
        let r = g.filter(vec![touch(1, TouchPhase::End, 0.0, 0.0), touch(2, TouchPhase::End, 50.0, 0.0), touch(3, TouchPhase::End, 100.0, 0.0)], 0.2);
        assert_eq!(r.tap, Some(Tap::Three));

        let mut g = Gate::default();
        g.filter(vec![touch(1, TouchPhase::Start, 0.0, 0.0), touch(2, TouchPhase::Start, 50.0, 0.0)], 0.0);
        g.filter(vec![touch(1, TouchPhase::Move, 0.0, 80.0), touch(2, TouchPhase::Move, 50.0, 80.0)], 0.1);
        let r = g.filter(vec![touch(1, TouchPhase::End, 0.0, 80.0), touch(2, TouchPhase::End, 50.0, 80.0)], 0.2);
        assert_eq!(r.tap, None, "a two-finger pan");

        let mut g = Gate::default();
        g.filter(vec![touch(1, TouchPhase::Start, 0.0, 0.0), touch(2, TouchPhase::Start, 50.0, 0.0)], 0.0);
        let r = g.filter(vec![touch(1, TouchPhase::End, 0.0, 0.0), touch(2, TouchPhase::End, 50.0, 0.0)], 0.9);
        assert_eq!(r.tap, None, "a long press is no tap");
    }

    #[test]
    fn mouse_input_without_touches_is_untouched() {
        let mut g = Gate::default();
        let r = g.filter(vec![moved(1.0), press(true), press(false)], 0.0);
        assert_eq!(pointers(&r.events), 3);
        assert!(!r.cancel);
    }
}
