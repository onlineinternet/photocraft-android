//! Pixel Art Mode (View › Pixel Art Mode, `Document::pixel_art`): how the shell shows and edits a
//! pixel-art document.
//!
//! - The zoom is a whole number of screen pixels per document pixel (or 1/n below 100 %), so
//!   every pixel is the same size on screen. Pinch and the wheel step through those levels;
//!   Fit on Screen picks the largest one that fits, above 100 % too.
//! - Turning the mode on (or opening a pixel-art document) shows the pixel grid, turns
//!   anti-aliasing off, picks a 1 px Pencil without smoothing (pixel-perfect: `paint.pencil`
//!   defaults `pixelPerfect` from the document) and brings up the Swatches panel.
//! - Free Transform starts with Nearest Neighbor interpolation.
//!
//! The canvas already magnifies with nearest-neighbour sampling, so pixels stay sharp.

use crate::PhotocraftApp;
use crate::state::{Tool, View};

/// `zoom` (screen pixels per document pixel) on the nearest pixel-art level: a whole number at
/// or above 100 %, 1/n below. A zoom that isn't a positive number becomes 100 %.
pub fn snap(zoom: f32) -> f32 {
    if !(zoom.is_finite() && zoom > 0.0) {
        return 1.0;
    }
    if zoom >= 1.0 { zoom.round().max(1.0) } else { 1.0 / (1.0 / zoom).round().max(1.0) }
}

/// The largest pixel-art level at or below `zoom`.
pub fn snap_down(zoom: f32) -> f32 {
    if !(zoom.is_finite() && zoom > 0.0) {
        return 1.0;
    }
    if zoom >= 1.0 { zoom.floor().max(1.0) } else { 1.0 / (1.0 / zoom).ceil().max(1.0) }
}

/// The pixel grid in Pixel Art Mode is this much stronger than Photoshop's, since it also has to
/// show over the checkerboard.
pub const GRID_BOOST: f32 = 1.4;

/// Checker cells in Pixel Art Mode are whole document pixels, lined up with the pixels so every
/// empty pixel is its own square: one pixel per cell when a pixel is at least `min` (the usual
/// checker square) on screen, else the fewest pixels, a power of two, that make a cell that big.
/// `scale` and `min` are in the same unit (screen pixels or points per document pixel).
pub fn checker_cell(scale: f32, min: f32) -> f32 {
    if !(scale.is_finite() && scale > 0.0 && min.is_finite() && min > 0.0) {
        return 1.0;
    }
    let mut k = 1.0;
    while scale * k < min && k < 65536.0 {
        k *= 2.0;
    }
    k
}

/// Whether the active document is pixel art.
pub fn active(app: &PhotocraftApp) -> bool {
    app.session.active().is_some_and(|d| d.doc.pixel_art)
}

/// Free Transform's starting interpolation: Nearest Neighbor for pixel art, else Bicubic.
pub fn interpolation(app: &PhotocraftApp) -> String {
    if active(app) { "nearest".into() } else { "bicubic".into() }
}

/// Continuous zoom (a pinch, a trackpad) in pixel-art mode: the gesture's own zoom is kept
/// unsnapped across frames so small steps add up, and the view shows it snapped.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PinchZoom {
    /// The gesture's unsnapped zoom.
    raw: f32,
    /// What the view was given last; a different view zoom means it was zoomed some other way.
    shown: f32,
}

impl PinchZoom {
    /// The view zoom after scaling by `factor` from `zoom`, snapped, within `clamp`.
    pub fn scale(&mut self, zoom: f32, factor: f32, clamp: impl Fn(f32) -> f32) -> f32 {
        let raw = if self.shown == zoom && self.raw.is_finite() && self.raw > 0.0 { self.raw } else { zoom };
        let raw = clamp(raw * factor);
        let shown = clamp(snap(raw));
        *self = Self { raw, shown };
        shown
    }
}

/// Bring each document's view in line with its pixel-art flag: when a document becomes pixel
/// art (the toggle, undo/redo of it, a pixel-art file opened or created), set up its view and,
/// for the active document, the tools. Runs after every command (`sync_views`).
pub fn sync(app: &mut PhotocraftApp) {
    let flags: Vec<bool> = app.session.documents().iter().map(|d| d.doc.pixel_art).collect();
    let active = app.session.active_index();
    for (i, on) in flags.into_iter().enumerate() {
        let Some(view) = app.ui.views.get_mut(i) else { continue };
        if view.pixel_art == on {
            continue;
        }
        view.pixel_art = on;
        if on {
            enter_view(view);
        }
        if Some(i) == active {
            if on {
                enter_tools(app);
            } else {
                app.ui.tool_options.anti_alias = true;
            }
        }
    }
}

/// A view entering pixel-art mode: refit (on whole pixels) unless a fit is coming anyway.
fn enter_view(view: &mut View) {
    if !view.fill_pending {
        view.fit_pending = true;
    }
}

/// The tools for pixel art: the grid, no anti-aliasing, a 1 px Pencil and the Swatches panel.
fn enter_tools(app: &mut PhotocraftApp) {
    app.ui.view.show.pixel_grid = true;
    app.ui.tool_options.anti_alias = false;
    app.ui.tool = Tool::Pencil;
    crate::paint_mouse::sync_tool_brush(app);
    let brush = &mut app.session.tools.brush;
    brush.size = 1.0;
    brush.smoothing.enabled = false;
    let pro = app.ui.theme.is_pro();
    let group = crate::dock::Group::Color;
    if let Some(i) = group.tabs(pro).iter().position(|t| *t == "Swatches") {
        *group.tab_mut(&mut app.ui.dock_tabs) = i;
    }
    crate::dock::reveal(app, group);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn zoom_snaps_to_whole_pixels() {
        assert_eq!(snap(1.0), 1.0);
        assert_eq!(snap(2.4), 2.0);
        assert_eq!(snap(2.6), 3.0);
        assert_eq!(snap(0.6), 0.5);
        assert_eq!(snap(0.3), 1.0 / 3.0);
        assert_eq!(snap(f32::NAN), 1.0);
        assert_eq!(snap(-2.0), 1.0);
        assert_eq!(snap_down(2.9), 2.0);
        assert_eq!(snap_down(0.9), 0.5);
        assert_eq!(snap_down(0.0), 1.0);
    }

    #[test]
    fn checker_cells_are_whole_pixels() {
        assert_eq!(checker_cell(53.0, 21.0), 1.0, "zoomed in: one pixel per square");
        assert_eq!(checker_cell(5.0, 21.0), 8.0);
        assert_eq!(checker_cell(0.5, 8.0), 16.0);
        assert_eq!(checker_cell(f32::NAN, 8.0), 1.0);
        assert_eq!(checker_cell(1e-9, 8.0), 65536.0, "bounded");
    }

    #[test]
    fn a_slow_pinch_adds_up_to_the_next_level() {
        let mut p = PinchZoom::default();
        let mut zoom = 4.0;
        let mut levels = vec![];
        for _ in 0..40 {
            zoom = p.scale(zoom, 1.02, |z| z.clamp(0.01, 128.0));
            levels.push(zoom);
        }
        assert!(levels.iter().all(|z| z.fract() == 0.0), "{levels:?}");
        assert!(zoom >= 8.0, "a slow pinch still zooms in: {zoom}");
        // Zoomed some other way in between: the pinch starts from the new zoom.
        assert_eq!(p.scale(2.0, 1.0, |z| z), 2.0);
    }

    #[test]
    fn turning_it_on_sets_up_the_tools_and_off_restores_anti_alias() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 32, "height": 32})).unwrap();
        app.sync_views();
        assert!(app.ui.tool_options.anti_alias);
        assert_eq!(interpolation(&app), "bicubic");
        app.ui.views[0].fit_pending = false;
        app.run("view.pixelArtMode", json!({"on": true})).unwrap();
        app.sync_views();
        assert!(app.ui.view.show.pixel_grid && !app.ui.tool_options.anti_alias);
        assert_eq!(app.ui.tool, Tool::Pencil);
        assert_eq!(app.session.tools.brush.size, 1.0);
        assert!(!app.session.tools.brush.smoothing.enabled);
        assert_eq!(interpolation(&app), "nearest");
        assert!(app.ui.views[0].fit_pending, "the view refits on whole pixels");
        app.run("view.pixelArtMode", json!({"on": false})).unwrap();
        app.sync_views();
        assert!(app.ui.tool_options.anti_alias);
        // Undo turns it back on, and the tools follow.
        app.ui.tool_options.anti_alias = true;
        app.run("edit.undo", json!({})).unwrap();
        app.sync_views();
        assert!(!app.ui.tool_options.anti_alias);
    }

    #[test]
    fn a_new_pixel_art_document_starts_set_up() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 16, "height": 16, "pixelArt": true})).unwrap();
        app.sync_views();
        assert_eq!(app.ui.tool, Tool::Pencil);
        assert!(app.ui.views[0].pixel_art);
    }

    #[test]
    fn fit_on_screen_uses_whole_pixels_past_100_percent() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 16, "height": 16})).unwrap();
        let mut doc = (*app.session.active().unwrap().doc).clone();
        doc.pixel_art = true;
        let mut view = View::default();
        crate::canvas::fit_view(&mut view, &doc, egui::vec2(500.0, 400.0), 2.0, 1.0);
        // (400 − 40) / 16 = 22.5 points = 45 device px per pixel.
        assert_eq!(view.zoom, 45.0);
        doc.pixel_art = false;
        crate::canvas::fit_view(&mut view, &doc, egui::vec2(500.0, 400.0), 2.0, 1.0);
        assert_eq!(view.zoom, 1.0, "other documents still fit at 100 % (one image pixel per device pixel) at most");
    }
}
