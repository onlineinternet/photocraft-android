//! Two-finger canvas navigation on a touch screen (Clip Studio Paint, Procreate): two fingers
//! pinch to zoom, move to pan and twist to turn the view, all at once and all around the point
//! between the fingers, so the picture stays under them. Twisting needs the touch shell's
//! toggle (`PhotocraftApp::touch_rotate`); zoom and pan always work.
//!
//! - The view follows the fingers exactly: no dead zone and no snapping (both felt jerky on a
//!   tablet). View › Reset View straightens it.
//! - A pixel-art document only turns in 90° steps: the view jumps to the next quarter turn once
//!   the fingers are past halfway to it (`pixel_mode.rs` keeps it on quarter turns).

use crate::rotate_view::{snap_deg, wrap_deg};

/// One two-finger gesture, kept across frames.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Twist {
    /// Two or more fingers are down.
    active: bool,
    /// The view's rotation when the gesture started.
    start: f32,
    /// How far the fingers have turned since, in degrees (clockwise positive).
    turned: f32,
}

impl Twist {
    /// A frame of the gesture: the fingers turned `delta_deg`; the view is at `current`. Returns
    /// the view's new rotation, if it changes. `quarter_turns` is pixel art's 90° lock.
    pub fn turn(&mut self, current: f32, delta_deg: f32, quarter_turns: bool) -> Option<f32> {
        if !self.active {
            *self = Self { active: true, start: current, turned: 0.0 };
        }
        if !delta_deg.is_finite() {
            return None;
        }
        self.turned = wrap_deg(self.turned + delta_deg);
        let target = if quarter_turns { snap_deg(self.start + self.turned, 90.0) } else { wrap_deg(self.start + self.turned) };
        (target != current).then_some(target)
    }

    /// The fingers lifted: the view stays where they left it (no snapping), so this only ends
    /// the gesture. Returns the rotation to land on, which is never a change.
    pub fn release(&mut self, _current: f32) -> Option<f32> {
        *self = Self::default();
        None
    }
}

/// What two fingers did to view `key`'s canvas this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Nav {
    /// The point between the fingers now, and where it was last frame (screen points): the
    /// picture under `from` moves to `to`.
    pub from: egui::Pos2,
    pub to: egui::Pos2,
    /// Zoom by this factor (1 when only turning or panning).
    pub zoom: f32,
    /// The view's new rotation, if it changes.
    pub rotation: Option<f32>,
}

/// Two fingers' midpoint, spread and heading (the angle from the first finger to the second).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Shape {
    center: egui::Pos2,
    spread: f32,
    heading: f32,
}

/// The fingers on the screen and the gesture they make, kept across frames. egui's own
/// `multi_touch` isn't used: it only starts a gesture while there is a pointer, and the touch
/// shell drops the emulated mouse as soon as a second finger lands; and it measures the heading
/// from whichever finger its hash map lists first, which can flip by 180° mid-gesture.
#[derive(Clone, Debug, Default, PartialEq)]
struct Fingers {
    /// Fingers down by touch id (ordered, so the first two are always the same two).
    down: std::collections::BTreeMap<u64, egui::Pos2>,
    /// Last frame's shape of the gesture; `None` right after a finger lands or lifts, so the
    /// average doesn't jump.
    prev: Option<Shape>,
    /// The gesture began on this canvas.
    ours: bool,
    twist: Twist,
    /// Where the fingers last were, for the snap on release.
    last: Option<egui::Pos2>,
}

impl Fingers {
    fn shape(&self) -> Option<Shape> {
        let mut it = self.down.values();
        let (a, b) = (*it.next()?, *it.next()?);
        let center = a + (b - a) * 0.5;
        let d = b - a;
        Some(Shape { center, spread: d.length(), heading: d.y.atan2(d.x) })
    }
}

/// This frame's two-finger navigation of view `key`'s canvas in `rect` (`None` when there is no
/// gesture, or it began outside `rect`). Twisting turns the view only if `rotate`;
/// `quarter_turns` is pixel art's 90° lock; `flip` is View › Flip Horizontal (a mirrored view
/// turns the other way). Letting go can still snap the rotation.
pub fn update(ctx: &egui::Context, key: egui::Id, rect: egui::Rect, current: f32, rotate: bool, quarter_turns: bool, flip: bool) -> Option<Nav> {
    let id = key.with("touch-fingers");
    let mut f = ctx.data(|d| d.get_temp::<Fingers>(id)).unwrap_or_default();
    let was_two = f.down.len() >= 2;
    let mut changed = false;
    ctx.input(|i| {
        for e in &i.events {
            if let egui::Event::Touch { id, phase, pos, .. } = e {
                match phase {
                    egui::TouchPhase::Start => {
                        // A runaway stream of fingers never grows the map without bound.
                        if f.down.len() < 10 {
                            f.down.insert(id.0, *pos);
                        }
                        changed = true;
                    }
                    egui::TouchPhase::Move => {
                        if let Some(p) = f.down.get_mut(&id.0) {
                            *p = *pos;
                        }
                    }
                    egui::TouchPhase::End | egui::TouchPhase::Cancel => {
                        f.down.remove(&id.0);
                        changed = true;
                    }
                }
            }
        }
    });
    let shape = f.shape();
    let result = match shape {
        Some(now) => {
            if !was_two {
                f.ours = rect.contains(now.center);
                f.twist = Twist::default();
            }
            let prev = if changed { None } else { f.prev };
            f.prev = Some(now);
            match prev {
                Some(before) if f.ours => {
                    f.last = Some(now.center);
                    let zoom = if before.spread > 1.0 && now.spread.is_finite() { (now.spread / before.spread).clamp(0.5, 2.0) } else { 1.0 };
                    let turned = crate::rotate_view::screen_delta(wrap_deg((now.heading - before.heading).to_degrees()), flip);
                    let rotation = if rotate { f.twist.turn(current, turned, quarter_turns) } else { None };
                    Some(Nav { from: before.center, to: now.center, zoom, rotation })
                }
                _ => None,
            }
        }
        None => {
            f.prev = None;
            let snap = if was_two && f.ours { f.last.zip(f.twist.release(current)) } else { None };
            if !was_two || f.down.is_empty() {
                f.last = None;
                f.ours = false;
                f.twist = Twist::default();
            }
            snap.map(|(at, r)| Nav { from: at, to: at, zoom: 1.0, rotation: Some(r) })
        }
    };
    ctx.data_mut(|d| d.insert_temp(id, f));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Turn by `steps`, feeding back each new rotation like the canvas does.
    fn run(t: &mut Twist, mut at: f32, steps: &[f32], quarter: bool) -> f32 {
        for &d in steps {
            if let Some(r) = t.turn(at, d, quarter) {
                at = r;
            }
        }
        at
    }

    #[test]
    fn the_view_follows_the_fingers_exactly_and_stays_where_left() {
        let mut t = Twist::default();
        let at = run(&mut t, 10.0, &[3.0, -1.0], false);
        assert!((at - 12.0).abs() < 1e-4, "no dead zone: {at}");
        let at = run(&mut t, at, &[75.0], false);
        assert!((at - 87.0).abs() < 1e-4);
        assert_eq!(t.release(at), None, "no snap to 90°");
        let at = run(&mut t, 87.0, &[-90.0], false);
        assert!((at - (-3.0)).abs() < 1e-4, "a new gesture starts where the view is: {at}");
    }

    #[test]
    fn pixel_art_turns_in_quarter_turns() {
        let mut t = Twist::default();
        assert_eq!(run(&mut t, 0.0, &[20.0, 20.0], true), 0.0, "under halfway");
        assert_eq!(run(&mut t, 0.0, &[10.0], true), 90.0, "past halfway");
        // Same gesture, back the other way: 50° − 120° from the start is past halfway to −90°.
        assert_eq!(run(&mut t, 90.0, &[-60.0, -60.0], true), -90.0);
        assert_eq!(t.release(-90.0), None);
    }

    #[test]
    fn hostile_deltas_are_ignored() {
        let mut t = Twist::default();
        assert_eq!(t.turn(0.0, f32::NAN, false), None);
        assert_eq!(t.turn(0.0, f32::INFINITY, false), None);
        assert_eq!(run(&mut t, 0.0, &[30.0], false), 30.0);
    }
}
