//! Clip Studio-style touch chrome for a phone or tablet.
//!
//! The desktop menu bar, toolbar, options bar and dock stay in place for a mouse. When
//! [`crate::PhotocraftApp::studio_chrome`] is set (the Android shell), those bars are not drawn
//! and this module lays out:
//!
//! - a top command bar (menu, file actions, undo, redo, zoom);
//! - a left tool rail, with a sub-tool list beside it (docked on a landscape tablet, a sheet on a phone);
//! - a bottom strip with the foreground colour and a brush-size slider;
//! - palettes for layers, the colour wheel or sliders, and tool properties, docked on a tablet and
//!   sliding over the canvas on a small screen.
//!
//! Every control runs an existing command or edits the session brush. Rows that have no engine
//! yet are drawn disabled and labelled as not in this build (onion skin, frame tags, materials,
//! a per-document rotation lock).

use egui::{Align, Align2, Color32, FontId, Id, Layout, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};
use photocraft_engine::BrushSettings;
use serde_json::json;

use crate::state::{ColorPanelMode, Tool};
use crate::theme::Tokens;
use crate::{PhotocraftApp, icons};

/// Height of the bottom colour and brush strip.
pub const QUICK_H: f32 = 56.0;
/// Width of the tool rail, including its padding.
const TOOL_W: f32 = 64.0;
/// Width of the sub-tool column.
const SUB_W: f32 = 220.0;
/// Width of a docked palette on a tablet.
const PALETTE_W: f32 = 320.0;
/// A finger has to rest this long, without moving, before the canvas samples a colour.
const HOLD_SECS: f64 = 0.45;
/// Movement (points) past this is a stroke, not a long-press sample.
const HOLD_SLOP: f32 = 14.0;

/// Phone sheets, or a docked tablet layout. A landscape tablet is wide and tall enough that the
/// sub-tools and one palette can sit beside the canvas; a phone (including sideways) keeps them
/// as sheets so the picture stays large.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutKind {
    Phone,
    Tablet,
}

pub fn layout_kind(size: Vec2) -> LayoutKind {
    if size.x >= 960.0 && size.y >= 600.0 && size.x >= size.y { LayoutKind::Tablet } else { LayoutKind::Phone }
}

/// A long-press should sample instead of painting.
pub(crate) fn hold_sample_ready(elapsed_secs: f64, travel: f32, brushlike: bool, blocked: bool) -> bool {
    brushlike && !blocked && travel.is_finite() && travel <= HOLD_SLOP && elapsed_secs >= HOLD_SECS
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Palette {
    Layers,
    Color,
    Properties,
}

#[derive(Clone, Copy, Debug, Default)]
struct Chrome {
    subtools: bool,
    palette: Option<Palette>,
    ready: bool,
    /// The colour palette has already been shown, so its mode is the user's choice.
    color_primed: bool,
}

struct ToolGroup {
    label: &'static str,
    tools: &'static [Tool],
    /// List brush presets under the tool variants.
    presets: bool,
}

const MOVE: &[Tool] = &[Tool::Move, Tool::Artboard, Tool::Hand, Tool::RotateView, Tool::Zoom];
const SELECT: &[Tool] = &[
    Tool::RectMarquee,
    Tool::EllipseMarquee,
    Tool::SingleRowMarquee,
    Tool::SingleColumnMarquee,
    Tool::Lasso,
    Tool::PolygonLasso,
    Tool::MagneticLasso,
    Tool::MagicWand,
    Tool::QuickSelection,
    Tool::ObjectSelection,
];
const CROP: &[Tool] = &[Tool::Crop, Tool::PerspectiveCrop, Tool::Slice, Tool::SliceSelect, Tool::Frame];
const SAMPLE: &[Tool] = &[Tool::Eyedropper, Tool::ColorSampler, Tool::Ruler, Tool::Note, Tool::Count];
const BRUSH: &[Tool] = &[Tool::Brush, Tool::Pencil, Tool::MixerBrush, Tool::ColorReplacement];
const ERASER: &[Tool] = &[Tool::Eraser, Tool::BackgroundEraser, Tool::MagicEraser];
const FILL: &[Tool] = &[Tool::PaintBucket, Tool::Gradient];
const RETOUCH: &[Tool] = &[
    Tool::Remove,
    Tool::SpotHealing,
    Tool::Healing,
    Tool::Patch,
    Tool::ContentAwareMove,
    Tool::RedEye,
    Tool::CloneStamp,
    Tool::PatternStamp,
    Tool::HistoryBrush,
    Tool::Blur,
    Tool::Sharpen,
    Tool::Smudge,
    Tool::Dodge,
    Tool::Burn,
    Tool::Sponge,
];
const PEN: &[Tool] = &[
    Tool::Pen,
    Tool::FreeformPen,
    Tool::CurvaturePen,
    Tool::AddAnchorPoint,
    Tool::DeleteAnchorPoint,
    Tool::ConvertPoint,
    Tool::PathSelection,
    Tool::DirectSelection,
];
const TYPE: &[Tool] = &[Tool::Type, Tool::VerticalType, Tool::HorizontalTypeMask, Tool::VerticalTypeMask];
const SHAPE: &[Tool] = &[Tool::Rectangle, Tool::EllipseShape, Tool::Triangle, Tool::Polygon, Tool::Line, Tool::CustomShape];

fn groups() -> &'static [ToolGroup] {
    const GROUPS: &[ToolGroup] = &[
        ToolGroup { label: "Move", tools: MOVE, presets: false },
        ToolGroup { label: "Select", tools: SELECT, presets: false },
        ToolGroup { label: "Crop", tools: CROP, presets: false },
        ToolGroup { label: "Eyedropper", tools: SAMPLE, presets: false },
        ToolGroup { label: "Brush", tools: BRUSH, presets: true },
        ToolGroup { label: "Eraser", tools: ERASER, presets: true },
        ToolGroup { label: "Fill", tools: FILL, presets: false },
        ToolGroup { label: "Retouch", tools: RETOUCH, presets: false },
        ToolGroup { label: "Pen", tools: PEN, presets: false },
        ToolGroup { label: "Text", tools: TYPE, presets: false },
        ToolGroup { label: "Shape", tools: SHAPE, presets: false },
    ];
    GROUPS
}

fn group_of(tool: Tool) -> Option<usize> {
    groups().iter().position(|g| g.tools.contains(&tool))
}

fn chrome_id() -> Id {
    Id::new("studio-chrome")
}

fn load_chrome(ctx: &egui::Context) -> Chrome {
    ctx.data(|d| d.get_temp(chrome_id())).unwrap_or_default()
}

fn save_chrome(ctx: &egui::Context, chrome: Chrome) {
    ctx.data_mut(|d| d.insert_temp(chrome_id(), chrome));
}

fn hole_id() -> Id {
    Id::new("studio-chrome-hole")
}

/// Ask the next frame's chrome to open a palette (`layers`, `color`, `properties`, `subtools`, `none`).
/// Used by the offscreen snapshot.
pub fn request_open(ctx: &egui::Context, what: &str) {
    ctx.data_mut(|d| d.insert_temp(Id::new("studio-open-request"), what.to_string()));
}

fn init_chrome(ctx: &egui::Context, kind: LayoutKind) -> Chrome {
    let mut chrome =
        Chrome { subtools: kind == LayoutKind::Tablet, palette: (kind == LayoutKind::Tablet).then_some(Palette::Layers), ready: true, color_primed: false };
    if let Some(req) = ctx.data_mut(|d| d.remove_temp::<String>(Id::new("studio-open-request"))) {
        chrome.subtools = false;
        chrome.palette = None;
        match req.as_str() {
            "layers" => chrome.palette = Some(Palette::Layers),
            "color" => chrome.palette = Some(Palette::Color),
            "properties" => chrome.palette = Some(Palette::Properties),
            "subtools" => chrome.subtools = true,
            "none" => {}
            _ => {}
        }
    }
    chrome
}

fn toggle_palette(chrome: &mut Chrome, palette: Palette, kind: LayoutKind) {
    if chrome.palette == Some(palette) {
        chrome.palette = None;
    } else {
        chrome.palette = Some(palette);
        if kind == LayoutKind::Phone {
            chrome.subtools = false;
        }
    }
}

fn shown_tool(ui: &egui::Ui, index: usize, current: Tool) -> Tool {
    let group = &groups()[index];
    if group.tools.contains(&current) {
        return current;
    }
    ui.data(|d| d.get_temp::<Tool>(Id::new(("studio-slot", index)))).filter(|t| group.tools.contains(t)).unwrap_or(group.tools[0])
}

fn remember_tool(ui: &egui::Ui, tool: Tool) {
    if let Some(index) = group_of(tool) {
        ui.data_mut(|d| d.insert_temp(Id::new(("studio-slot", index)), tool));
    }
}

fn run(app: &mut PhotocraftApp, ctx: &egui::Context, id: &str) {
    if let Err(e) = crate::menus::invoke(app, ctx, id, json!({})) {
        app.ui.status = e;
        app.ui.status_error = true;
    }
}

fn color32(c: [f32; 4]) -> Color32 {
    let q = |v: f32| {
        let v = if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
        (v * 255.0).round() as u8
    };
    Color32::from_rgba_unmultiplied(q(c[0]), q(c[1]), q(c[2]), q(c[3]))
}

/// A finger-sized hit target. `label` is the accessible name.
fn hit_button(ui: &mut egui::Ui, width: f32, label: &str, selected: bool, draw: impl FnOnce(&egui::Ui, Rect, Color32)) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 44.0), Sense::click());
    let fill = if selected {
        Some(t.accent_soft)
    } else if resp.is_pointer_button_down_on() {
        Some(t.pressed)
    } else if resp.hovered() {
        Some(t.hover)
    } else {
        None
    };
    if let Some(fill) = fill {
        ui.painter().rect_filled(rect, t.radius_sm, fill);
    }
    let color = if selected { t.accent_text } else { t.text };
    draw(ui, rect, color);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, label));
    resp.on_hover_text(label)
}

fn paint_hook(ui: &egui::Ui, rect: Rect, color: Color32, flip: bool) {
    let c = rect.center();
    let s = if flip { -1.0 } else { 1.0 };
    let p = |x: f32, y: f32| c + vec2(x * s, y);
    let pts = [p(7.0, -7.0), p(-2.0, -7.0), p(-8.0, -1.0), p(-2.0, 5.0), p(8.0, 5.0)];
    let stroke = Stroke::new(2.0, color);
    for w in pts.windows(2) {
        ui.painter().line_segment([w[0], w[1]], stroke);
    }
    let tip = pts[4];
    ui.painter().line_segment([tip, tip + vec2(-6.0 * s, -4.0)], stroke);
    ui.painter().line_segment([tip, tip + vec2(-6.0 * s, 4.0)], stroke);
}

/// A wide touch slider. `caption` is drawn on the track (the value the finger is setting).
fn touch_slider(ui: &mut egui::Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, caption: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let width = ui.available_width().max(48.0);
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width, 40.0), Sense::click_and_drag());
    let (lo, hi) = (*range.start(), *range.end());
    if !value.is_finite() {
        *value = lo;
    }
    let span = (hi - lo).abs().max(f32::EPSILON);
    let track = Rect::from_min_max(pos2(rect.left() + 8.0, rect.center().y - 3.0), pos2(rect.right() - 8.0, rect.center().y + 3.0));
    if let Some(p) = resp.interact_pointer_pos()
        && (resp.dragged() || resp.clicked())
    {
        let f = ((p.x - track.left()) / track.width().max(1.0)).clamp(0.0, 1.0);
        let nv = lo + f * (hi - lo);
        if (nv - *value).abs() > f32::EPSILON {
            *value = nv;
            resp.mark_changed();
        }
    }
    let f = ((*value - lo) / span).clamp(0.0, 1.0);
    ui.painter().rect_filled(track, 3.0, t.field);
    let filled = Rect::from_min_max(track.left_top(), pos2(track.left() + track.width() * f, track.bottom()));
    ui.painter().rect_filled(filled, 3.0, t.accent);
    let knob = pos2(track.left() + track.width() * f, track.center().y);
    ui.painter().circle_filled(knob, 11.0, t.text);
    ui.painter().text(rect.left_center() + vec2(14.0, 0.0), Align2::LEFT_CENTER, caption, FontId::proportional(13.0), t.primary_text);
    resp
}

/// The bars that shrink the canvas: command bar, tool rail, quick strip, and on a tablet the
/// docked sub-tools and palette.
pub(crate) fn bars(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let kind = layout_kind(ctx.content_rect().size());
    let mut chrome = load_chrome(&ctx);
    if !chrome.ready {
        chrome = init_chrome(&ctx, kind);
    }
    prime_color(app, &mut chrome);
    let wide = ctx.content_rect().width() >= 720.0;
    let t = Tokens::get(&ctx);
    let frame = egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin::symmetric(8, 6));

    let command = egui::Panel::top("studio-command").resizable(false).frame(frame).show(ui, |ui| {
        command_bar(app, ui, wide);
    });
    let quick = egui::Panel::bottom("studio-quick").resizable(false).exact_size(QUICK_H).frame(frame).show(ui, |ui| {
        quick_strip(app, ui, &mut chrome, kind);
    });
    let tools = crate::direction::start_panel("studio-tools").resizable(false).exact_size(TOOL_W).frame(frame).show(ui, |ui| {
        tool_rail(app, ui, &mut chrome, kind);
    });
    if kind == LayoutKind::Tablet && chrome.subtools {
        let sub = egui::Frame::NONE.fill(t.dock).inner_margin(egui::Margin::symmetric(8, 8));
        crate::direction::start_panel("studio-subtools").resizable(false).exact_size(SUB_W).frame(sub).show(ui, |ui| {
            if subtool_list(app, ui) {
                chrome.subtools = false;
            }
        });
    }
    if kind == LayoutKind::Tablet
        && let Some(palette) = chrome.palette
    {
        let sub = egui::Frame::NONE.fill(t.dock).inner_margin(egui::Margin::same(8));
        let mut close = false;
        crate::direction::end_panel("studio-palette").resizable(false).exact_size(PALETTE_W).frame(sub).show(ui, |ui| {
            let h = ui.available_height();
            palette_body(app, ui, palette, h, &mut close);
        });
        if close {
            chrome.palette = None;
        }
    }

    let content = ctx.content_rect();
    let (left, right) = if crate::direction::is_rtl() { (content.left(), tools.response.rect.left()) } else { (tools.response.rect.right(), content.right()) };
    let hole = Rect::from_min_max(pos2(left, command.response.rect.bottom()), pos2(right, quick.response.rect.top()));
    ctx.data_mut(|d| d.insert_temp(hole_id(), hole));
    save_chrome(&ctx, chrome);
}

/// Sheets over the canvas on a phone, and the long-press colour ring on every layout.
pub(crate) fn overlays(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let kind = layout_kind(ctx.content_rect().size());
    if kind == LayoutKind::Phone {
        phone_sheets(app, ctx);
    }
    sample_ring(ctx);
}

fn phone_sheets(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let mut chrome = load_chrome(ctx);
    prime_color(app, &mut chrome);
    let t = Tokens::get(ctx);
    let hole = ctx.data(|d| d.get_temp::<Rect>(hole_id())).unwrap_or(ctx.content_rect());
    let sub_t = ctx.animate_bool_with_time(Id::new("studio-sub-anim"), chrome.subtools, 0.16);
    let pal_t = ctx.animate_bool_with_time(Id::new("studio-pal-anim"), chrome.palette.is_some(), 0.16);
    let scrim_t = sub_t.max(pal_t);
    if scrim_t > 0.02 && hole.width() > 8.0 && hole.height() > 8.0 {
        let alpha = (t.scrim.a() as f32 * scrim_t).round() as u8;
        let scrim = Color32::from_black_alpha(alpha);
        let clicked = egui::Area::new(Id::new("studio-scrim")).order(egui::Order::Foreground).fixed_pos(hole.min).interactable(true).show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(hole.size(), Sense::click());
            ui.painter().rect_filled(rect, 0.0, scrim);
            resp.clicked()
        });
        if clicked.inner {
            chrome.subtools = false;
            chrome.palette = None;
        }
    }
    if sub_t > 0.02 {
        let w = SUB_W.min(hole.width() * 0.8).max(160.0);
        let x = hole.left() - (1.0 - sub_t) * w;
        let mut close = false;
        egui::Area::new(Id::new("studio-sub-sheet")).order(egui::Order::Foreground).fixed_pos(pos2(x, hole.top())).show(ctx, |ui| {
            sheet_frame(ui, t.dock, w, hole.height(), |ui| {
                if subtool_list(app, ui) {
                    close = true;
                }
            });
        });
        if close {
            chrome.subtools = false;
        }
    }
    if pal_t > 0.02
        && let Some(palette) = chrome.palette
    {
        let w = PALETTE_W.min(hole.width() * 0.92).max(200.0);
        let x = hole.right() - w * pal_t;
        let mut close = false;
        egui::Area::new(Id::new("studio-pal-sheet")).order(egui::Order::Foreground).fixed_pos(pos2(x, hole.top())).show(ctx, |ui| {
            sheet_frame(ui, t.dock, w, hole.height(), |ui| {
                palette_body(app, ui, palette, hole.height() - 16.0, &mut close);
            });
        });
        if close {
            chrome.palette = None;
        }
    }
    save_chrome(ctx, chrome);
}

fn sheet_frame(ui: &mut egui::Ui, fill: Color32, width: f32, height: f32, body: impl FnOnce(&mut egui::Ui)) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::NONE.fill(fill).stroke(Stroke::new(1.0, t.card_border)).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
        ui.set_width(width - 16.0);
        ui.set_min_height((height - 16.0).max(40.0));
        body(ui);
    });
}

fn sample_ring(ctx: &egui::Context) {
    let Some(at) = ctx.data(|d| d.get_temp::<Pos2>(Id::new("studio-sample-pos"))) else { return };
    let t = Tokens::get(ctx);
    egui::Area::new(Id::new("studio-sample-ring")).order(egui::Order::Tooltip).fixed_pos(at - vec2(20.0, 20.0)).interactable(false).show(ctx, |ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
        ui.painter().circle_stroke(rect.center(), 16.0, Stroke::new(3.0, t.text));
        ui.painter().circle_stroke(rect.center(), 18.0, Stroke::new(1.0, t.chrome));
    });
}

fn command_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui, wide: bool) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        crate::menus::touch_menu_button(app, ui);
        if wide {
            for (label, id) in [(tl!("New"), "file.new"), (tl!("Open"), "file.open")] {
                if hit_button(ui, 58.0, label, false, |ui, rect, color| {
                    ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(15.0), color);
                })
                .clicked()
                {
                    run(app, ui.ctx(), id);
                }
            }
        }
        let save = tl!("Save");
        if hit_button(ui, 58.0, save, false, |ui, rect, color| {
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, save, FontId::proportional(15.0), color);
        })
        .clicked()
        {
            run(app, ui.ctx(), "file.save");
        }
        let undo = tl!("Undo");
        if hit_button(ui, 44.0, undo, false, |ui, rect, color| paint_hook(ui, rect, color, false)).clicked() {
            run(app, ui.ctx(), "edit.undo");
        }
        let redo = tl!("Redo");
        if hit_button(ui, 44.0, redo, false, |ui, rect, color| paint_hook(ui, rect, color, true)).clicked() {
            run(app, ui.ctx(), "edit.redo");
        }

        let name = app.session.active().map(|d| d.doc.name.clone()).unwrap_or_else(|| tl!("PhotoCraft").to_string());
        let pct = app.session.active_index().and_then(|i| app.ui.views.get(i)).map(|v| (v.zoom * 100.0).round() as i32).filter(|z| *z > 0).unwrap_or(100);
        let zoom = format!("{pct}%");
        let fit = tl!("Fit on screen");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if wide {
                let on = crate::pixel_mode::active(app);
                let px = tl!("Pixel art");
                if hit_button(ui, 44.0, px, on, |ui, rect, color| {
                    ui.painter().text(rect.center(), Align2::CENTER_CENTER, "Px", FontId::proportional(14.0), color);
                })
                .clicked()
                {
                    run(app, ui.ctx(), "view.pixelArtMode");
                }
            }
            if hit_button(ui, 64.0, fit, false, |ui, rect, color| {
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, &zoom, FontId::proportional(14.0), color);
            })
            .clicked()
            {
                run(app, ui.ctx(), "view.fitOnScreen");
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                let color = if app.ui.status_error { t.danger } else { t.text };
                let shown = if app.ui.status.is_empty() { name } else { app.ui.status.clone() };
                ui.add(egui::Label::new(egui::RichText::new(shown).color(color).size(15.0)).truncate());
            });
        });
    });
}

fn tool_rail(app: &mut PhotocraftApp, ui: &mut egui::Ui, chrome: &mut Chrome, kind: LayoutKind) {
    ui.spacing_mut().item_spacing.y = 4.0;
    let current = app.ui.tool;
    let mut pick: Option<Tool> = None;
    let mut toggle: Option<usize> = None;
    egui::ScrollArea::vertical().id_salt("studio-tool-rail").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        for (index, _) in groups().iter().enumerate() {
            let shown = shown_tool(ui, index, current);
            let selected = group_of(current) == Some(index);
            let resp = icons::button(ui, icons::tool_icon(shown), 48.0, selected, shown.label());
            if resp.clicked() {
                if selected {
                    toggle = Some(index);
                } else {
                    pick = Some(shown);
                }
            }
        }
    });
    if let Some(tool) = pick {
        crate::tool_memory::select(app, tool);
        remember_tool(ui, tool);
        chrome.subtools = true;
        if kind == LayoutKind::Phone {
            chrome.palette = None;
        }
    }
    if toggle.is_some() {
        chrome.subtools = !chrome.subtools;
        if chrome.subtools && kind == LayoutKind::Phone {
            chrome.palette = None;
        }
    }
}

/// The sub-tool list. Returns true when its close button was pressed.
fn subtool_list(app: &mut PhotocraftApp, ui: &mut egui::Ui) -> bool {
    let Some(index) = group_of(app.ui.tool) else {
        ui.label(tl!("This tool is not on the rail. Choose one there, or from the menu."));
        return false;
    };
    let group = &groups()[index];
    let mut close = false;
    let mut pick_tool: Option<Tool> = None;
    let mut pick_preset: Option<String> = None;
    let mut brush_settings = false;
    egui::ScrollArea::vertical().id_salt("studio-subtools").auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.strong(tl!(group.label));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if hit_button(ui, 36.0, tl!("Close"), false, |ui, rect, color| {
                    ui.painter().text(rect.center(), Align2::CENTER_CENTER, "×", FontId::proportional(18.0), color);
                })
                .clicked()
                {
                    close = true;
                }
            });
        });
        ui.add_space(4.0);
        for tool in group.tools {
            if tool_row(ui, *tool, app.ui.tool == *tool).clicked() {
                pick_tool = Some(*tool);
            }
        }
        if group.presets {
            ui.add_space(8.0);
            ui.separator();
            ui.label(tl!("Brushes"));
            let current = app.session.tools.current_preset.as_deref();
            if app.session.tools.presets.is_empty() {
                ui.weak(tl!("No brush presets yet"));
            }
            for preset in &app.session.tools.presets {
                if text_row(ui, &preset.name, current == Some(preset.name.as_str())).clicked() {
                    pick_preset = Some(preset.name.clone());
                }
            }
            ui.add_space(6.0);
            if text_row(ui, tl!("Brush settings…"), false).clicked() {
                brush_settings = true;
            }
        }
    });
    if let Some(tool) = pick_tool {
        crate::tool_memory::select(app, tool);
        remember_tool(ui, tool);
    }
    if let Some(name) = pick_preset
        && let Err(e) = app.run("tools.setBrush", json!({ "preset": name }))
    {
        app.ui.status = e;
        app.ui.status_error = true;
    }
    if brush_settings {
        app.ui.panels.brush_settings = true;
    }
    close
}

fn tool_row(ui: &mut egui::Ui, tool: Tool, selected: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let width = ui.available_width().max(48.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 44.0), Sense::click());
    if selected {
        ui.painter().rect_filled(rect, t.radius_sm, t.accent_soft);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, t.radius_sm, t.hover);
    }
    let icon = Rect::from_min_size(rect.min + vec2(6.0, 6.0), vec2(32.0, 32.0));
    let tint = if selected { t.accent_text } else { t.icon };
    icons::paint(ui, icon, icons::tool_icon(tool), 22.0, tint);
    let label = tl!(tool.label());
    ui.painter().text(icon.right_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, label, FontId::proportional(15.0), t.text);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, label));
    resp
}

fn text_row(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let width = ui.available_width().max(48.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 40.0), Sense::click());
    if selected {
        ui.painter().rect_filled(rect, t.radius_sm, t.accent_soft);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, t.radius_sm, t.hover);
    }
    let color = if selected { t.accent_text } else { t.text };
    ui.painter().text(rect.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, label, FontId::proportional(14.0), color);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, label));
    resp
}

fn quick_strip(app: &mut PhotocraftApp, ui: &mut egui::Ui, chrome: &mut Chrome, kind: LayoutKind) {
    let wide = ui.available_width() >= 640.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        color_chips(app, ui, chrome, kind);
        // Leave room for the Layers and Tool properties buttons, which follow.
        let reserved = 96.0;
        let rest = (ui.available_width() - reserved).max(48.0);
        if app.ui.tool.is_brushlike() {
            let before = app.session.tools.brush.clone();
            let mut after = before.clone();
            let each = if wide { rest * 0.5 } else { rest };
            ui.allocate_ui_with_layout(vec2(each, 40.0), Layout::left_to_right(Align::Center), |ui| size_slider(ui, &mut after));
            if wide {
                let caption = pct(after.opacity);
                ui.allocate_ui_with_layout(vec2(each, 40.0), Layout::left_to_right(Align::Center), |ui| {
                    unit_slider(ui, &mut after.opacity, 0.0..=1.0, &caption);
                });
            }
            crate::brush_panel::commit_gesture(app, ui.ctx(), &before, &after);
        } else {
            ui.allocate_ui_with_layout(vec2(rest, 40.0), Layout::left_to_right(Align::Center), |ui| {
                ui.label(tl!(app.ui.tool.label()));
            });
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if icons::button(ui, "settings", 40.0, chrome.palette == Some(Palette::Properties), "Tool properties").clicked() {
                toggle_palette(chrome, Palette::Properties, kind);
            }
            if icons::button(ui, "layers", 40.0, chrome.palette == Some(Palette::Layers), "Layers").clicked() {
                toggle_palette(chrome, Palette::Layers, kind);
            }
        });
    });
}

fn color_chips(app: &mut PhotocraftApp, ui: &mut egui::Ui, chrome: &mut Chrome, kind: LayoutKind) {
    let fg = tl!("Foreground color");
    if color_chip(ui, app.session.tools.foreground, 40.0, !app.ui.color_panel.background && chrome.palette == Some(Palette::Color), fg).clicked() {
        pick_color_target(app, chrome, false, kind);
    }
    let bg = tl!("Background color");
    if color_chip(ui, app.session.tools.background, 28.0, app.ui.color_panel.background && chrome.palette == Some(Palette::Color), bg).clicked() {
        pick_color_target(app, chrome, true, kind);
    }
    if icons::button(ui, "arrow-left-right", 36.0, false, "Swap colors").clicked() {
        run(app, ui.ctx(), "tools.swapColors");
    }
}

fn pick_color_target(app: &mut PhotocraftApp, chrome: &mut Chrome, background: bool, kind: LayoutKind) {
    let same = chrome.palette == Some(Palette::Color) && app.ui.color_panel.background == background;
    app.ui.color_panel.background = background;
    if same {
        chrome.palette = None;
    } else {
        chrome.palette = Some(Palette::Color);
        if kind == LayoutKind::Phone {
            chrome.subtools = false;
        }
    }
}

fn color_chip(ui: &mut egui::Ui, color: [f32; 4], size: f32, selected: bool, label: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(size, 44.0), Sense::click());
    let radius = (size * 0.36).min(16.0);
    ui.painter().circle_filled(rect.center(), radius, color32(color));
    let ring = if selected { t.accent } else { t.text };
    ui.painter().circle_stroke(rect.center(), radius + 1.5, Stroke::new(2.0, ring));
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, label));
    resp.on_hover_text(label)
}

fn size_slider(ui: &mut egui::Ui, brush: &mut BrushSettings) {
    let max = photocraft_engine::paint::MAX_BRUSH_SIZE;
    let size = brush.size.clamp(1.0, max);
    let mut lv = size.ln();
    let caption = format!("{:.0} px", size);
    if touch_slider(ui, &mut lv, 0.0..=max.ln(), &caption).changed() {
        brush.resize_with_dual_brush(lv.exp().clamp(1.0, max));
    }
}

fn pct(v: f32) -> String {
    let v = if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
    format!("{}%", (v * 100.0).round())
}

fn unit_slider(ui: &mut egui::Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, caption: &str) {
    let mut v = if value.is_finite() { *value } else { 0.0 };
    if touch_slider(ui, &mut v, range, caption).changed() {
        *value = v;
    }
}

fn prime_color(app: &mut PhotocraftApp, chrome: &mut Chrome) {
    if chrome.palette == Some(Palette::Color) && !chrome.color_primed {
        chrome.color_primed = true;
        if app.ui.color_panel.mode == ColorPanelMode::HueCube {
            app.ui.color_panel.mode = ColorPanelMode::ColorWheel;
        }
    }
}

fn palette_body(app: &mut PhotocraftApp, ui: &mut egui::Ui, palette: Palette, max_h: f32, close: &mut bool) {
    let title = match palette {
        Palette::Layers => tl!("Layers"),
        Palette::Color => tl!("Color"),
        Palette::Properties => tl!("Tool properties"),
    };
    ui.horizontal(|ui| {
        ui.strong(title);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if hit_button(ui, 36.0, tl!("Close"), false, |ui, rect, color| {
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, "×", FontId::proportional(18.0), color);
            })
            .clicked()
            {
                *close = true;
            }
            if palette == Palette::Layers
                && hit_button(ui, 36.0, tl!("New Layer"), false, |ui, rect, color| {
                    ui.painter().text(rect.center(), Align2::CENTER_CENTER, "+", FontId::proportional(18.0), color);
                })
                .clicked()
            {
                run(app, ui.ctx(), "layer.new.layer");
            }
        });
    });
    ui.add_space(4.0);
    let body_h = (max_h - 48.0).max(80.0);
    match palette {
        Palette::Layers => {
            ui.allocate_ui_with_layout(vec2(ui.available_width(), body_h), Layout::top_down(Align::Min), |ui| {
                crate::panels::dock_body(app, ui, crate::dock::Group::Layers, 0);
            });
        }
        Palette::Color => {
            color_mode_row(app, ui);
            egui::ScrollArea::vertical().id_salt("studio-color").max_height(body_h - 36.0).show(ui, |ui| {
                crate::color_panel_ui::panel(app, ui);
            });
        }
        Palette::Properties => {
            egui::ScrollArea::vertical().id_salt("studio-props").max_height(body_h).show(ui, |ui| {
                tool_properties(app, ui);
            });
        }
    }
}

fn color_mode_row(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let mode = app.ui.color_panel.mode;
        for (label, next) in [(tl!("Wheel"), ColorPanelMode::ColorWheel), (tl!("Sliders"), ColorPanelMode::HsbSliders), (tl!("Cube"), ColorPanelMode::HueCube)]
        {
            if hit_button(ui, 72.0, label, mode == next, |ui, rect, color| {
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(14.0), color);
            })
            .clicked()
            {
                app.ui.color_panel.mode = next;
            }
        }
    });
}

fn tool_properties(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    ui.label(egui::RichText::new(tl!(app.ui.tool.label())).strong().color(t.text));
    ui.add_space(6.0);
    if app.ui.tool.is_brushlike() {
        let before = app.session.tools.brush.clone();
        let mut after = before.clone();
        ui.label(tl!("Size"));
        size_slider(ui, &mut after);
        ui.label(tl!("Hardness"));
        let hardness = pct(after.hardness);
        unit_slider(ui, &mut after.hardness, 0.0..=1.0, &hardness);
        ui.label(tl!("Opacity"));
        let opacity = pct(after.opacity);
        unit_slider(ui, &mut after.opacity, 0.0..=1.0, &opacity);
        ui.label(tl!("Flow"));
        let flow = pct(after.flow);
        unit_slider(ui, &mut after.flow, 0.0..=1.0, &flow);
        ui.label(tl!("Smoothing"));
        let mut smooth = after.smoothing.amount.clamp(0.0, 1.0);
        let smooth_label = pct(smooth);
        if touch_slider(ui, &mut smooth, 0.0..=1.0, &smooth_label).changed() {
            after.smoothing.amount = smooth;
            after.smoothing.enabled = smooth > 0.0;
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if flag_button(ui, tl!("Pen size"), after.pressure_size).clicked() {
                after.pressure_size = !after.pressure_size;
            }
            if flag_button(ui, tl!("Pen opacity"), after.pressure_opacity).clicked() {
                after.pressure_opacity = !after.pressure_opacity;
            }
        });
        crate::brush_panel::commit_gesture(app, ui.ctx(), &before, &after);
        ui.add_space(8.0);
        if text_row(ui, tl!("Brush settings…"), false).clicked() {
            app.ui.panels.brush_settings = true;
        }
    } else {
        ui.weak(tl!("Size, opacity and brush presets follow a brush, pencil or eraser."));
    }
    ui.add_space(10.0);
    ui.separator();
    let pixel = crate::pixel_mode::active(app);
    if flag_button(ui, tl!("Pixel art"), pixel).clicked() {
        run(app, ui.ctx(), "view.pixelArtMode");
    }
    if text_row(ui, tl!("Timeline"), false).clicked() {
        run(app, ui.ctx(), "window.panel.timeline");
    }
    ui.add_space(12.0);
    ui.label(egui::RichText::new(tl!("Not in this build")).color(t.text_dim));
    ui.weak(tl!("These Clip Studio and Aseprite tools have no command yet. They do nothing."));
    for label in [tl!("Onion skin"), tl!("Frame tags"), tl!("Materials"), tl!("Lock rotation for this document")] {
        let _ = ui.add_enabled(false, egui::Button::new(label).min_size(vec2(ui.available_width().max(48.0), 36.0)));
    }
    ui.weak(tl!("Two-finger rotation is the Touch rotate key on the edge keyboard, for every document."));
}

fn flag_button(ui: &mut egui::Ui, label: &str, on: bool) -> egui::Response {
    hit_button(ui, 120.0, label, on, |ui, rect, color| {
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(13.0), color);
    })
}

/// While a brush tool's finger rests on the canvas, drop the paint preview and sample the colour
/// under it. Returns true when this frame must not paint or commit that stroke.
pub(crate) fn hold_eyedropper(app: &mut PhotocraftApp, ui: &egui::Ui, xf: &crate::canvas::ViewXform, canvas: Rect) -> bool {
    let (down, pressed, released, origin, pos, time) = ui.input(|i| {
        (
            i.pointer.primary_down(),
            i.pointer.primary_pressed(),
            i.pointer.primary_released(),
            i.pointer.press_origin(),
            i.pointer.hover_pos().or(i.pointer.latest_pos()),
            i.time,
        )
    });
    let on_canvas = origin.is_some_and(|p| canvas.contains(p));
    if !on_canvas || (!down && !released) {
        ui.ctx().data_mut(|d| {
            d.remove::<Hold>(Id::new("studio-hold"));
            d.remove::<Pos2>(Id::new("studio-sample-pos"));
        });
        return false;
    }
    let Some(origin) = origin else { return false };
    let pos = pos.unwrap_or(origin);
    let mut hold = if pressed {
        Hold { travel: 0.0, t0: time, sampling: false }
    } else {
        ui.ctx().data(|d| d.get_temp::<Hold>(Id::new("studio-hold"))).unwrap_or(Hold { travel: 0.0, t0: time, sampling: false })
    };
    hold.travel = hold.travel.max(origin.distance(pos));
    let blocked = app.ui.transform.is_some() || crate::crop_ui::pending(app);
    if hold_sample_ready(time - hold.t0, hold.travel, app.ui.tool.is_brushlike(), blocked) {
        hold.sampling = true;
    }
    if down && !hold.sampling && !hold.travel.is_nan() && hold.travel <= HOLD_SLOP {
        let left = HOLD_SECS - (time - hold.t0);
        if left > 0.0 {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(left));
        }
    }
    if hold.sampling {
        app.live_stroke = None;
        app.drag = None;
        app.press_stroke = false;
        app.alt_sampling = false;
        let [x, y] = xf.to_doc(pos);
        if let Some([r, g, b]) = crate::canvas::eyedropper_color(app, x, y) {
            let _ = app.run("tools.setColors", json!({ "foreground": [r, g, b, 1.0] }));
        }
        ui.ctx().data_mut(|d| d.insert_temp(Id::new("studio-sample-pos"), pos));
    }
    let sampling = hold.sampling;
    if released {
        ui.ctx().data_mut(|d| {
            d.remove::<Hold>(Id::new("studio-hold"));
            d.remove::<Pos2>(Id::new("studio-sample-pos"));
        });
    } else {
        ui.ctx().data_mut(|d| d.insert_temp(Id::new("studio-hold"), hold));
    }
    sampling
}

#[derive(Clone, Copy)]
struct Hold {
    travel: f32,
    t0: f64,
    sampling: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phone_and_tablet_layouts() {
        assert_eq!(layout_kind(vec2(390.0, 844.0)), LayoutKind::Phone);
        assert_eq!(layout_kind(vec2(844.0, 390.0)), LayoutKind::Phone, "a phone on its side stays a sheet layout");
        assert_eq!(layout_kind(vec2(1280.0, 800.0)), LayoutKind::Tablet);
        assert_eq!(layout_kind(vec2(1024.0, 1366.0)), LayoutKind::Phone, "a tablet held upright uses sheets");
    }

    #[test]
    fn every_tool_is_in_one_group() {
        for tool in Tool::ALL {
            let n = groups().iter().filter(|g| g.tools.contains(&tool)).count();
            assert_eq!(n, 1, "{}", tool.label());
        }
    }

    #[test]
    fn a_long_press_samples_only_when_the_finger_rests() {
        assert!(!hold_sample_ready(0.2, 0.0, true, false));
        assert!(hold_sample_ready(0.45, 0.0, true, false));
        assert!(hold_sample_ready(0.8, HOLD_SLOP, true, false));
        assert!(!hold_sample_ready(0.8, HOLD_SLOP + 1.0, true, false), "a stroke is not a sample");
        assert!(!hold_sample_ready(0.8, 0.0, false, false), "selection tools keep their press");
        assert!(!hold_sample_ready(0.8, 0.0, true, true), "a transform or crop owns the press");
        assert!(!hold_sample_ready(f64::NAN, 0.0, true, false));
    }

    #[test]
    fn studio_chrome_replaces_the_desktop_bars() {
        let phone = harness(vec2(390.0, 844.0));
        assert!(drawn(&phone, "studio-command"));
        assert!(drawn(&phone, "studio-tools"));
        assert!(drawn(&phone, "studio-quick"));
        assert!(!drawn(&phone, "studio-subtools"), "a phone keeps sub-tools in a sheet");
        assert!(!drawn(&phone, "studio-palette"));
        assert!(!drawn(&phone, "toolbar"));
        assert!(!drawn(&phone, "options_bar"));
        assert!(!drawn(&phone, "dock"));

        let tablet = harness(vec2(1280.0, 800.0));
        assert!(drawn(&tablet, "studio-command"));
        assert!(drawn(&tablet, "studio-subtools"));
        assert!(drawn(&tablet, "studio-palette"));
        assert!(!drawn(&tablet, "toolbar"));
    }

    fn harness(size: Vec2) -> egui_kittest::Harness<'static, PhotocraftApp> {
        let mut h = egui_kittest::Harness::builder().with_size(size).with_max_steps(32).build_eframe(move |cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, crate::theme::ThemeKind::Studio);
            let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
            app.studio_chrome = true;
            app.touch_ui = true;
            let _ = app.run("prefs.set", json!({"values": {"interface.appearanceMode": "dark", "interface.darkTheme": "studio", "interface.theme": "studio"}}));
            let _ = app.run("file.new", json!({"width": 320, "height": 240, "name": "Sketch"}));
            app
        });
        h.run_steps(3);
        h
    }

    fn drawn(h: &egui_kittest::Harness<'_, PhotocraftApp>, panel: &str) -> bool {
        egui::containers::panel::PanelState::load(&h.ctx, Id::new(panel)).is_some()
    }
}
