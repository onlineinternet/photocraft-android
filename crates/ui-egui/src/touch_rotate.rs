//! Two-finger canvas rotation on a touch screen (Clip Studio Paint, Procreate): twisting two
//! fingers turns the view (`View::rotation`), as long as the touch shell turns it on
//! (`PhotocraftApp::touch_rotate`).
//!
//! - A pinch always twists a little, so the view stays put until the fingers have turned
//!   [`DEAD_ZONE`] degrees; from then on it follows them (less the dead zone, so it doesn't jump).
//! - Letting go within [`SNAP`] degrees of 0°, 90°, 180° or 270° lands exactly on it.
//! - A pixel-art document only turns in 90° steps: the view jumps to the next quarter turn once
//!   the fingers are past halfway to it (`pixel_mode.rs` keeps it on quarter turns).

use crate::rotate_view::{snap_deg, wrap_deg};

/// Degrees the fingers must turn before the view starts to.
pub const DEAD_ZONE: f32 = 12.0;
/// Releasing within this many degrees of a quarter turn snaps to it.
pub const SNAP: f32 = 8.0;

/// One two-finger gesture, kept across frames.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Twist {
    /// Two or more fingers are down.
    active: bool,
    /// The view's rotation when the gesture started.
    start: f32,
    /// How far the fingers have turned since, in degrees (clockwise positive).
    turned: f32,
    /// Past the dead zone: the view follows.
    engaged: bool,
}

impl Twist {
    /// A frame of the gesture: the fingers turned `delta_deg`; the view is at `current`. Returns
    /// the view's new rotation, if it changes. `quarter_turns` is pixel art's 90° lock.
    pub fn turn(&mut self, current: f32, delta_deg: f32, quarter_turns: bool) -> Option<f32> {
        if !self.active {
            *self = Self { active: true, start: current, turned: 0.0, engaged: false };
        }
        if !delta_deg.is_finite() {
            return None;
        }
        self.turned = wrap_deg(self.turned + delta_deg);
        if !self.engaged && self.turned.abs() < DEAD_ZONE {
            return None;
        }
        self.engaged = true;
        let target =
            if quarter_turns { snap_deg(self.start + self.turned, 90.0) } else { wrap_deg(self.start + self.turned - DEAD_ZONE.copysign(self.turned)) };
        (target != current).then_some(target)
    }

    /// The fingers lifted with the view at `current`: the rotation to land on, if it changes.
    pub fn release(&mut self, current: f32) -> Option<f32> {
        let engaged = self.active && self.engaged;
        *self = Self::default();
        if !engaged {
            return None;
        }
        let quarter = snap_deg(current, 90.0);
        (wrap_deg(current - quarter).abs() <= SNAP && quarter != current).then_some(quarter)
    }
}

/// This frame's two-finger rotation of view `key`'s canvas in `rect`: the view's new rotation,
/// if it changes. Off unless `enabled`; a gesture that starts outside `rect` is left alone.
pub fn update(ctx: &egui::Context, key: egui::Id, rect: egui::Rect, current: f32, enabled: bool, quarter_turns: bool) -> Option<f32> {
    let id = key.with("touch-twist");
    let mut twist = ctx.data(|d| d.get_temp::<Twist>(id)).unwrap_or_default();
    let touch = ctx.multi_touch().filter(|t| enabled && t.num_touches >= 2 && rect.contains(t.start_pos));
    let next = match touch {
        Some(t) => twist.turn(current, t.rotation_delta.to_degrees(), quarter_turns),
        None if twist.active => twist.release(current),
        None => return None,
    };
    ctx.data_mut(|d| d.insert_temp(id, twist));
    next
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
    fn a_small_twist_while_pinching_does_nothing() {
        let mut t = Twist::default();
        assert_eq!(run(&mut t, 0.0, &[3.0, -2.0, 4.0, 5.0], false), 0.0);
        assert_eq!(t.release(0.0), None);
    }

    #[test]
    fn past_the_dead_zone_the_view_follows_without_jumping() {
        let mut t = Twist::default();
        let at = run(&mut t, 10.0, &[6.0, 6.0, 2.0], false);
        assert!((at - 12.0).abs() < 1e-4, "2° past the dead zone: {at}");
        let at = run(&mut t, at, &[20.0], false);
        assert!((at - 32.0).abs() < 1e-4);
        assert_eq!(t.release(at), None, "nowhere near a quarter turn");
    }

    #[test]
    fn letting_go_near_a_quarter_turn_snaps_to_it() {
        let mut t = Twist::default();
        let at = run(&mut t, 0.0, &[50.0, 47.0], false);
        assert!((at - 85.0).abs() < 1e-4);
        assert_eq!(t.release(at), Some(90.0));
        // A new gesture starts from where the view is.
        let at = run(&mut t, 90.0, &[-30.0, -73.0], false);
        assert!((at - (-1.0)).abs() < 1e-4);
        assert_eq!(t.release(at), Some(0.0));
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
        assert_eq!(run(&mut t, 0.0, &[30.0], false), 18.0);
    }
}
