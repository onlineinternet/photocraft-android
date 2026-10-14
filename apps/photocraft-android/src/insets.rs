//! Keeping the interface out from under Android's status bar and navigation bar.
//!
//! A NativeActivity draws into a surface that covers the whole window, system bars included;
//! Android reports the part the user can actually see as the activity's content rect
//! (`AndroidApp::content_rect`, in physical pixels). The shell turns that into egui's safe-area
//! insets, so `Context::content_rect` excludes the bars, and lays the app out inside it.

use egui::epaint::MarginF32;

/// The content rect as Android reports it: left, top, right, bottom, in physical pixels.
pub type PxRect = [i32; 4];

/// Reads the activity's current content rect (`None` before Android has reported one).
pub type ContentRectFn = Box<dyn Fn() -> Option<PxRect>>;

/// The safe-area insets (points) for a window of `window_px` physical pixels whose visible part is
/// `content`. `None` when Android hasn't reported a usable rect yet.
pub fn insets(content: PxRect, window_px: [f32; 2], pixels_per_point: f32) -> Option<MarginF32> {
    let [l, t, r, b] = content.map(|v| v as f32);
    if r <= l || b <= t || pixels_per_point <= 0.0 || window_px[0] <= 0.0 || window_px[1] <= 0.0 {
        return None;
    }
    let p = pixels_per_point;
    Some(MarginF32 {
        left: (l / p).max(0.0),
        top: (t / p).max(0.0),
        right: ((window_px[0] - r) / p).max(0.0),
        bottom: ((window_px[1] - b) / p).max(0.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_and_navigation_bars_become_top_and_bottom_insets() {
        // A 1080 x 2400 phone at 2.625 px/pt: a 105 px status bar and a 63 px gesture bar.
        let m = insets([0, 105, 1080, 2337], [1080.0, 2400.0], 2.625).unwrap();
        assert_eq!((m.left, m.right), (0.0, 0.0));
        assert!((m.top - 40.0).abs() < 1e-3);
        assert!((m.bottom - 24.0).abs() < 1e-3);
    }

    #[test]
    fn an_unreported_rect_is_no_insets() {
        assert_eq!(insets([0, 0, 0, 0], [1080.0, 2400.0], 2.0), None);
        assert_eq!(insets([0, 0, 1080, 2400], [0.0, 0.0], 2.0), None);
    }
}
