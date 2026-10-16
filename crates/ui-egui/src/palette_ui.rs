//! Pixel Art Mode's palette (`Document::palette`, the engine's `palette.*` commands), shown at
//! the top of the Swatches panel, and the Pencil's palette inks in its options bar.
//!
//! - Tap a colour to paint with it (it becomes the foreground colour) and select it.
//! - **+** adds the foreground colour, **−** removes the selected one.
//! - **Recolor** changes the selected colour, and every pixel of exactly that colour in the
//!   document, to the foreground colour (Aseprite's palette-linked recolouring).
//! - **Ramp** inserts four steps from the selected colour to the foreground colour.
//! - **From Layer** / **From Swatches** fill the palette from the active layer's colours or the
//!   first Swatches group.
//!
//! The Pencil's **Ink** shades instead of painting (each pixel it passes moves one palette entry
//! up or down), and **Palette only** paints with the nearest palette colour.

use egui::{Color32, Sense, Stroke, StrokeKind, vec2};
use serde_json::json;

use crate::PhotocraftApp;
use crate::preset_panels::run;
use crate::widgets;

/// Side of one palette cell, in points.
const CELL: f32 = 22.0;

fn c32(c: [f32; 3]) -> Color32 {
    let b = |v: f32| (if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 } * 255.0).round() as u8;
    Color32::from_rgb(b(c[0]), b(c[1]), b(c[2]))
}

fn hex(c: [f32; 3]) -> String {
    let b = |v: f32| (if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 } * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
}

/// The selected palette entry of the active document, remembered per document for the session.
fn selected_id(app: &PhotocraftApp) -> egui::Id {
    egui::Id::new(("pc-palette-selected", app.session.active().map(|d| d.doc.id.0)))
}

/// The palette strip at the top of the Swatches panel (Pixel Art Mode only).
pub fn strip(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    if !crate::pixel_mode::active(app) {
        return;
    }
    let Some(palette) = app.session.active().map(|d| d.doc.palette.clone()) else { return };
    let ctx = ui.ctx().clone();
    let sel_id = selected_id(app);
    let mut selected = ctx.data(|d| d.get_temp::<usize>(sel_id)).filter(|i| *i < palette.len());
    let t = crate::theme::Tokens::get(&ctx);
    ui.label(egui::RichText::new(tl!("Palette")).strong());
    if palette.is_empty() {
        ui.label(egui::RichText::new(tl!("No palette yet")).color(t.text_dim));
    }
    let mut picked: Option<usize> = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
        for (i, c) in palette.iter().enumerate() {
            let (r, resp) = ui.allocate_exact_size(vec2(CELL, CELL), Sense::click());
            ui.painter().rect_filled(r, 2.0, c32(*c));
            let on = selected == Some(i);
            let stroke = if on { Stroke::new(2.0, t.text) } else { Stroke::new(1.0, t.card_border) };
            ui.painter().rect_stroke(r, 2.0, stroke, StrokeKind::Inside);
            if resp.on_hover_text(hex(*c)).clicked() {
                picked = Some(i);
            }
        }
    });
    if let Some(i) = picked
        && let Some(c) = palette.get(i)
    {
        selected = Some(i);
        run(app, "tools.setColors", json!({"foreground": hex(*c)}));
    }
    ui.horizontal_wrapped(|ui| {
        if ui.button("+").on_hover_text(tl!("Add the foreground color")).clicked() {
            run(app, "palette.add", json!({}));
        }
        if let Some(i) = selected {
            if ui.button("−").on_hover_text(tl!("Remove the selected color")).clicked() && run(app, "palette.remove", json!({"index": i})).is_some() {
                selected = None;
            }
            if ui.button(tl!("Recolor")).on_hover_text(tl!("Change the selected color, and every pixel of it, to the foreground color")).clicked() {
                run(app, "palette.recolor", json!({"index": i}));
            }
            if ui.button(tl!("Ramp")).on_hover_text(tl!("Add 4 steps from the selected color to the foreground color")).clicked() {
                run(app, "palette.ramp", json!({"index": i, "steps": 4}));
            }
        }
        if ui.button(tl!("From Layer")).clicked() {
            run(app, "palette.fromLayer", json!({}));
        }
        if ui.button(tl!("From Swatches")).clicked() {
            run(app, "palette.fromSwatches", json!({}));
        }
    });
    match selected {
        Some(i) => {
            ctx.data_mut(|d| d.insert_temp(sel_id, i));
        }
        None => ctx.data_mut(|d| d.remove::<usize>(sel_id)),
    }
    ui.separator();
}

/// Add the Pencil's palette inks to its stroke params `p` (Pixel Art Mode with a palette, and not
/// an erasing stroke).
pub fn add_pencil_inks(app: &PhotocraftApp, p: &mut serde_json::Value, erase: bool) {
    if erase || !crate::pixel_mode::active(app) || app.session.active().is_none_or(|d| d.doc.palette.is_empty()) {
        return;
    }
    let o = &app.ui.tool_options;
    if o.pencil_shade != 0 {
        p["shade"] = json!(o.pencil_shade.signum());
    } else if o.pencil_palette_lock {
        p["paletteLock"] = json!(true);
    }
}

/// The Pencil's palette inks in the options bar (Pixel Art Mode with a palette only).
pub fn pencil_options(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let has_palette = crate::pixel_mode::active(app) && app.session.active().is_some_and(|d| !d.doc.palette.is_empty());
    if !has_palette {
        return;
    }
    widgets::vline(ui, 22.0);
    let o = &mut app.ui.tool_options;
    let mut ink = i64::from(o.pencil_shade.signum());
    let opts = [(0i64, tl!("Simple")), (1, tl!("Shade up")), (-1, tl!("Shade down"))];
    ui.label(tl!("Ink"));
    if widgets::dropdown(ui, "pencil-ink", &mut ink, &opts, 110.0) {
        o.pencil_shade = ink.clamp(-1, 1) as i8;
    }
    ui.add_enabled_ui(o.pencil_shade == 0, |ui| widgets::checkbox(ui, &mut o.pencil_palette_lock, tl!("Palette only")));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn app() -> PhotocraftApp {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 8, "height": 8, "background": "transparent", "pixelArt": true})).unwrap();
        app.sync_views();
        app
    }

    #[test]
    fn hex_and_colors_are_clamped() {
        assert_eq!(hex([1.0, 0.5, 0.0]), "#ff8000");
        assert_eq!(hex([f32::NAN, 2.0, -1.0]), "#00ff00");
        assert_eq!(c32([0.0, 0.0, 1.0]), Color32::from_rgb(0, 0, 255));
    }

    #[test]
    fn the_pencil_sends_the_palette_inks_only_in_pixel_art_with_a_palette() {
        let mut app = app();
        app.ui.tool_options.pencil_shade = 1;
        let drag = |app: &PhotocraftApp| {
            let mut p = json!({});
            add_pencil_inks(app, &mut p, false);
            p
        };
        assert!(drag(&app).get("shade").is_none(), "no palette yet");
        app.run("palette.set", json!({"colors": ["#000000", "#ffffff"]})).unwrap();
        assert_eq!(drag(&app)["shade"], 1);
        app.ui.tool_options.pencil_shade = 0;
        app.ui.tool_options.pencil_palette_lock = true;
        let p = drag(&app);
        assert_eq!((p.get("shade"), p["paletteLock"].clone()), (None, Value::Bool(true)));
        let mut erasing = json!({});
        add_pencil_inks(&app, &mut erasing, true);
        assert!(erasing.get("paletteLock").is_none(), "an erasing stroke erases");
        app.run("view.pixelArtMode", json!({"on": false})).unwrap();
        assert!(drag(&app).get("paletteLock").is_none(), "outside Pixel Art Mode the Pencil paints as usual");
    }
}
