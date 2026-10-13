//! Finger gestures for a touch screen, the way Clip Studio Paint does them: a two-finger tap
//! undoes, a three-finger tap redoes.
//!
//! egui-winit turns the first finger on the screen into a mouse (a pointer press) as well as a
//! touch, so the second finger of a gesture would otherwise land after the brush already started
//! a dab under the first. [`Gate`] keeps a new touch's pointer events back for a moment
//! ([`HOLD_SECS`]): when a second finger arrives in time, the session is a gesture (a tap, a
//! pinch or a two-finger pan) and its pointer events are dropped, so the canvas never paints or
//! selects; otherwise the held events go through in order, only a few frames late. The touch
//! events themselves always go through, so egui's pinch-zoom and two-finger pan still work.
//!
//! A pen is a touch to winit as well, so a pen stroke starts [`HOLD_SECS`] late too, with none
//! of its points lost.

use egui::{Event, Pos2, TouchPhase};
use std::collections::HashMap;

/// How long a new touch's pointer events wait for a second finger.
pub const HOLD_SECS: f64 = 0.08;
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
    /// Pointer events are being held: run another frame within [`HOLD_SECS`].
    pub holding: bool,
}

#[derive(Default)]
pub struct Gate {
    /// Pointer events of a lone touch, waiting to see whether a second finger follows.
    held: Vec<Event>,
    hold_since: Option<f64>,
    /// Fingers down: where each started and the farthest it has moved.
    down: HashMap<u64, (Pos2, f32)>,
    session_start: f64,
    most_fingers: usize,
    moved: bool,
    /// This session had two or more fingers at once: its pointer events never reach the app.
    multi: bool,
}

fn is_pointer(e: &Event) -> bool {
    matches!(e, Event::PointerMoved(_) | Event::PointerButton { .. } | Event::PointerGone)
}

impl Gate {
    fn flush(&mut self, out: &mut Vec<Event>) {
        out.append(&mut self.held);
        self.hold_since = None;
    }

    /// Pass one frame's input events through the gate at time `now` (seconds).
    pub fn filter(&mut self, events: Vec<Event>, now: f64) -> Filtered {
        let mut f = Filtered::default();
        if self.hold_since.is_some_and(|t| now - t >= HOLD_SECS) {
            self.flush(&mut f.events);
        }
        for event in events {
            match &event {
                Event::Touch { id, phase, pos, .. } => {
                    let (id, phase, pos) = (id.0, *phase, *pos);
                    match phase {
                        TouchPhase::Start => {
                            if self.down.is_empty() {
                                // A new session: hold its pointer events for a moment.
                                self.session_start = now;
                                self.most_fingers = 0;
                                self.moved = false;
                                self.multi = false;
                                self.held.clear();
                                self.hold_since = Some(now);
                                f.session_started = true;
                            }
                            self.down.insert(id, (pos, 0.0));
                            self.most_fingers = self.most_fingers.max(self.down.len());
                            if self.down.len() >= 2 && !self.multi {
                                // A gesture: what the first finger did as a mouse never happened.
                                self.multi = true;
                                self.held.clear();
                                self.hold_since = None;
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
                            if self.down.is_empty() {
                                if self.multi && !self.moved && phase == TouchPhase::End && now - self.session_start <= TAP_SECS {
                                    f.tap = match self.most_fingers {
                                        2 => Some(Tap::Two),
                                        3 => Some(Tap::Three),
                                        _ => None,
                                    };
                                }
                                // A quick single tap: let its press and release through now.
                                if !self.multi {
                                    self.flush(&mut f.events);
                                }
                            }
                        }
                    }
                    f.events.push(event);
                }
                e if is_pointer(e) => {
                    if self.hold_since.is_some() {
                        self.held.push(event);
                    } else if self.multi {
                        // The gesture's emulated mouse: dropped, including its release.
                        if matches!(e, Event::PointerGone) && self.down.is_empty() {
                            self.multi = false;
                        }
                    } else {
                        f.events.push(event);
                    }
                }
                _ => f.events.push(event),
            }
        }
        f.holding = self.hold_since.is_some();
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

    #[test]
    fn a_two_finger_tap_is_undo_and_never_reaches_the_canvas_as_a_press() {
        let mut g = Gate::default();
        let a = g.filter(vec![touch(1, TouchPhase::Start, 10.0, 10.0), moved(10.0), press(true)], 0.0);
        assert!(a.session_started && a.holding);
        assert_eq!(pointers(&a.events), 0, "held");
        let b = g.filter(vec![touch(2, TouchPhase::Start, 60.0, 10.0)], 0.03);
        assert_eq!(pointers(&b.events), 0);
        let c = g.filter(vec![touch(1, TouchPhase::End, 11.0, 10.0), touch(2, TouchPhase::End, 61.0, 10.0), press(false), Event::PointerGone], 0.15);
        assert_eq!(c.tap, Some(Tap::Two));
        assert_eq!(pointers(&c.events), 0, "the release and PointerGone are dropped too");
        assert_eq!(c.events.len(), 2, "touch events still go through");
        // The next single touch is a mouse again.
        let d = g.filter(vec![touch(3, TouchPhase::Start, 5.0, 5.0), press(true)], 1.0);
        let e = g.filter(vec![], 1.0 + HOLD_SECS);
        assert_eq!(pointers(&d.events) + pointers(&e.events), 1);
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
    fn a_lone_stroke_goes_through_in_order_once_the_hold_ends() {
        let mut g = Gate::default();
        let a = g.filter(vec![touch(1, TouchPhase::Start, 10.0, 10.0), moved(10.0), press(true)], 0.0);
        assert_eq!(pointers(&a.events), 0);
        let b = g.filter(vec![touch(1, TouchPhase::Move, 12.0, 10.0), moved(12.0)], 0.03);
        assert_eq!(pointers(&b.events), 0);
        let c = g.filter(vec![touch(1, TouchPhase::Move, 14.0, 10.0), moved(14.0)], HOLD_SECS + 0.01);
        assert!(!c.holding);
        let got: Vec<_> = c.events.iter().filter(|e| is_pointer(e)).cloned().collect();
        assert_eq!(got, vec![moved(10.0), press(true), moved(12.0), moved(14.0)]);
    }

    #[test]
    fn a_quick_single_tap_is_released_at_once() {
        let mut g = Gate::default();
        g.filter(vec![touch(1, TouchPhase::Start, 10.0, 10.0), moved(10.0), press(true)], 0.0);
        let r = g.filter(vec![touch(1, TouchPhase::End, 10.0, 10.0), press(false), Event::PointerGone], 0.04);
        assert_eq!(r.tap, None);
        assert_eq!(pointers(&r.events), 4, "move, press, release, gone");
        assert!(!r.holding);
    }

    #[test]
    fn mouse_input_without_touches_is_untouched() {
        let mut g = Gate::default();
        let r = g.filter(vec![moved(1.0), press(true), press(false)], 0.0);
        assert_eq!(pointers(&r.events), 3);
        assert!(!r.holding);
    }
}
