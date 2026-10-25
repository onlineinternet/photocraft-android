//! The pixel-art palette (`Document::palette`), like Aseprite's: an ordered list of up to
//! [`MAX_PALETTE`] colours kept with the document.
//!
//! - `palette.set`, `palette.add`, `palette.remove`: edit the list.
//! - `palette.fromSwatches`, `palette.fromLayer`: fill it from a Swatches group or from the
//!   colours a layer already uses.
//! - `palette.recolor`: change an entry, and every pixel of exactly its colour with it, in the
//!   layer or the whole document (palette-linked recolouring without indexed colour).
//! - `palette.ramp`: insert the steps between an entry and another colour, for shading ramps.
//!
//! The Pencil reads the palette too (`brush_cmds`): `"paletteLock"` paints with the nearest
//! entry, and `"shade"` moves each pixel it passes one entry along the palette ([`shade_pixels`]).
//! Every change is one undo step. Palettes work on 8- and 16-bit RGB documents.

use photocraft_doc::{ColorMode, Document, LayerId, MAX_PALETTE, SampleType, Surface};
use photocraft_geom::Rect;
use serde_json::{Value, json};

use crate::commands::{CommandSpec, color_param, parse_hex};
use crate::{EngineError, Result, Session};

fn bad(cmd: &str, msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: cmd.into(), msg: msg.into() }
}

fn has_doc(s: &Session) -> std::result::Result<(), String> {
    s.active().map(|_| ()).ok_or_else(|| "no document".into())
}

/// `#rrggbb` for a palette colour.
pub fn hex(c: [f32; 3]) -> String {
    crate::presets::swatches::hex(c)
}

/// Two colours are the same palette colour when they agree to within about half an 8-bit step.
pub fn same(a: [f32; 3], b: [f32; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 0.6 / 255.0)
}

/// The palette entry nearest `c` (squared RGB distance), if there is a palette.
pub fn nearest(palette: &[[f32; 3]], c: [f32; 3]) -> Option<usize> {
    let d = |p: &[f32; 3]| (0..3).map(|i| (p[i] - c[i]).powi(2)).sum::<f32>();
    palette.iter().enumerate().min_by(|a, b| d(a.1).total_cmp(&d(b.1))).map(|(i, _)| i)
}

fn rgb(c: [f32; 4]) -> [f32; 3] {
    [c[0], c[1], c[2]].map(|v| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 })
}

fn palette_json(p: &[[f32; 3]]) -> Value {
    json!({ "count": p.len(), "colors": p.iter().map(|c| hex(*c)).collect::<Vec<_>>() })
}

/// Palettes need RGB pixels stored as integers (exact colours to match).
fn check_doc(cmd: &str, d: &Document) -> Result<()> {
    if d.mode != ColorMode::Rgb {
        return Err(bad(cmd, "pixel-art palettes need an RGB document"));
    }
    if d.depth == SampleType::F32 {
        return Err(bad(cmd, "pixel-art palettes work on 8- and 16-bit documents"));
    }
    Ok(())
}

fn index_param(cmd: &str, p: &Value, len: usize) -> Result<usize> {
    let i = p.get("index").and_then(Value::as_u64).ok_or_else(|| bad(cmd, "`index` must be a palette index"))?;
    usize::try_from(i).ok().filter(|i| *i < len).ok_or_else(|| bad(cmd, format!("no palette entry {i} (the palette has {len})")))
}

/// Replace every pixel of exactly colour `from` with `to` (alpha kept) on `surf`; returns how
/// many changed and the area they cover.
pub fn replace_exact(surf: &mut Surface, from: [f32; 3], to: [f32; 3]) -> (usize, Rect) {
    if surf.channels() < 4 {
        return (0, Rect::EMPTY);
    }
    let coords: Vec<_> = surf.tiles().map(|(c, _)| *c).collect();
    let (mut n, mut area) = (0usize, Rect::EMPTY);
    for c in coords {
        let r = c.rect();
        let mut px = surf.read_region(r);
        let mut hit = false;
        for p in px.chunks_exact_mut(4) {
            if p[3] > 0.0 && same([p[0], p[1], p[2]], from) {
                p[..3].copy_from_slice(&to);
                n += 1;
                hit = true;
            }
        }
        if hit {
            surf.write_region(r, &px);
            area = area.union(&r);
        }
    }
    (n, area)
}

/// The Pencil's shading ink: inside `area`, every pixel the stroke covered (it shows the
/// `marker` colour the stroke painted with) goes back to what was under it (`pre`), moved `step`
/// entries along `palette` if that was a palette colour (stopping at the ends).
pub fn shade_pixels(pre: &Surface, surf: &mut Surface, area: Rect, marker: [f32; 3], palette: &[[f32; 3]], step: i32) {
    if area.is_empty() || surf.channels() < 4 || pre.channels() < 4 {
        return;
    }
    let before = pre.read_region(area);
    let mut px = surf.read_region(area);
    let mut changed = false;
    for (p, b) in px.chunks_exact_mut(4).zip(before.chunks_exact(4)) {
        if p[3] <= 0.0 || !same([p[0], p[1], p[2]], marker) {
            continue;
        }
        changed = true;
        p.copy_from_slice(b);
        if b[3] <= 0.0 {
            continue;
        }
        if let Some(k) = palette.iter().position(|c| same(*c, [b[0], b[1], b[2]])) {
            let len = i64::try_from(palette.len()).unwrap_or(0);
            let to = (i64::try_from(k).unwrap_or(0) + i64::from(step)).clamp(0, len - 1);
            if let Some(c) = usize::try_from(to).ok().and_then(|t| palette.get(t)) {
                p[..3].copy_from_slice(c);
            }
        }
    }
    if changed {
        surf.write_region(area, &px);
    }
}

/// A colour for the shading stroke to paint with that is not in `palette`, so the pixels it
/// covers can be told apart afterwards.
pub fn shade_marker(palette: &[[f32; 3]]) -> [f32; 3] {
    let candidates = [[1.0, 0.0, 1.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 1.0, 0.0], [0.0, 1.0, 1.0], [1.0, 0.0, 0.0]];
    for c in candidates {
        if !palette.iter().any(|p| same(*p, c)) {
            return c;
        }
    }
    // Every candidate is in the palette: walk the 8-bit cube for one that isn't.
    (0..=255u8).map(|v| [f32::from(v) / 255.0, 0.5, f32::from(255 - v) / 255.0]).find(|c| !palette.iter().any(|p| same(*p, *c))).unwrap_or([0.5, 0.25, 0.75])
}

/// Set the active document's palette as one undo step.
fn set_palette(s: &mut Session, label: &str, cmd: &str, palette: Vec<[f32; 3]>) -> Result<Value> {
    if palette.len() > MAX_PALETTE {
        return Err(bad(cmd, format!("a palette holds at most {MAX_PALETTE} colours")));
    }
    let st = s.active().ok_or(EngineError::NoDocument)?;
    check_doc(cmd, &st.doc)?;
    if st.doc.palette != palette {
        let p = palette.clone();
        s.edit(label, move |d, _| {
            d.palette = p;
            Ok(())
        })?;
    }
    Ok(palette_json(&palette))
}

fn current(s: &Session) -> Result<Vec<[f32; 3]>> {
    Ok(s.active().ok_or(EngineError::NoDocument)?.doc.palette.clone())
}

fn palette_set(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "palette.set";
    let colors = p.get("colors").and_then(Value::as_array).ok_or_else(|| bad(CMD, "`colors` must be a list of \"#rrggbb\""))?;
    if colors.len() > MAX_PALETTE {
        return Err(bad(CMD, format!("a palette holds at most {MAX_PALETTE} colours")));
    }
    let palette = colors
        .iter()
        .map(|c| c.as_str().and_then(parse_hex).map(rgb).ok_or_else(|| bad(CMD, format!("{c} is not a \"#rrggbb\" colour"))))
        .collect::<Result<Vec<_>>>()?;
    set_palette(s, "Set Palette", CMD, palette)
}

fn palette_add(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "palette.add";
    let c = rgb(color_param(CMD, p, "color", s.tools.foreground)?);
    let mut palette = current(s)?;
    if palette.iter().any(|e| same(*e, c)) {
        return Ok(palette_json(&palette));
    }
    palette.push(c);
    set_palette(s, "Add Palette Color", CMD, palette)
}

fn palette_remove(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "palette.remove";
    let mut palette = current(s)?;
    let i = index_param(CMD, p, palette.len())?;
    palette.remove(i);
    set_palette(s, "Remove Palette Color", CMD, palette)
}

fn from_swatches(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "palette.fromSwatches";
    let conv = crate::presets::swatches::SwatchConv::new(s);
    let groups = &s.presets.swatches;
    let group = match p.get("group") {
        None => groups.first(),
        Some(Value::String(g)) => groups.iter().find(|x| &x.name == g),
        Some(_) => return Err(bad(CMD, "`group` must be a swatch group name")),
    }
    .ok_or_else(|| bad(CMD, "no such swatch group"))?;
    let mut palette: Vec<[f32; 3]> = Vec::new();
    for sw in &group.items {
        let c = conv.rgb(&sw.color);
        if !palette.iter().any(|e| same(*e, c)) {
            palette.push(c);
        }
        if palette.len() > MAX_PALETTE {
            return Err(bad(CMD, format!("the group has more than {MAX_PALETTE} colours")));
        }
    }
    if palette.is_empty() {
        return Err(bad(CMD, "the group has no swatches"));
    }
    set_palette(s, "Palette from Swatches", CMD, palette)
}

fn from_layer(s: &mut Session, _p: &Value) -> Result<Value> {
    const CMD: &str = "palette.fromLayer";
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let id = st.active_layer.ok_or_else(|| bad(CMD, "no active layer"))?;
    let surf = st.doc.layer(id).and_then(|l| l.surface()).ok_or_else(|| bad(CMD, "the active layer has no pixels"))?;
    let mut palette: Vec<[f32; 3]> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (c, _) in surf.tiles() {
        let px = surf.read_region(c.rect());
        for p in px.chunks_exact(4) {
            if p[3] <= 0.0 {
                continue;
            }
            let key = [p[0], p[1], p[2]].map(|v| (v * 255.0).round() as u8);
            if seen.insert(key) {
                if seen.len() > MAX_PALETTE {
                    return Err(bad(CMD, format!("the layer uses more than {MAX_PALETTE} colours")));
                }
                palette.push(key.map(|v| f32::from(v) / 255.0));
            }
        }
    }
    if palette.is_empty() {
        return Err(bad(CMD, "the layer is empty"));
    }
    // Dark to light, so ramps read in order.
    palette.sort_by(|a, b| luma(*a).total_cmp(&luma(*b)));
    set_palette(s, "Palette from Layer", CMD, palette)
}

fn luma(c: [f32; 3]) -> f32 {
    0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]
}

fn recolor(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "palette.recolor";
    let st = s.active().ok_or(EngineError::NoDocument)?;
    check_doc(CMD, &st.doc)?;
    let mut palette = st.doc.palette.clone();
    let i = index_param(CMD, p, palette.len())?;
    let to = rgb(color_param(CMD, p, "color", s.tools.foreground)?);
    let whole = match p.get("scope").and_then(Value::as_str).unwrap_or("document") {
        "document" => true,
        "layer" => false,
        other => return Err(bad(CMD, format!("unknown scope `{other}` (document|layer)"))),
    };
    let from = palette[i];
    palette[i] = to;
    let pixels = s.edit("Recolor Palette Color", move |d, active| {
        d.palette = palette;
        let ids: Vec<LayerId> = if whole { d.layers.iter().map(|l| l.id).collect() } else { active.iter().copied().collect() };
        let mut n = 0;
        for id in ids {
            if let Some(surf) = d.layer_mut(id).and_then(|l| l.surface_mut()) {
                n += replace_exact(surf, from, to).0;
            }
        }
        Ok(n)
    })?;
    Ok(json!({ "pixels": pixels, "color": hex(to) }))
}

fn ramp(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "palette.ramp";
    let mut palette = current(s)?;
    let i = index_param(CMD, p, palette.len())?;
    let to = rgb(color_param(CMD, p, "color", s.tools.foreground)?);
    let steps = match p.get("steps") {
        None => 4,
        Some(v) => v.as_u64().filter(|n| (1..=32).contains(n)).ok_or_else(|| bad(CMD, "`steps` must be 1..32"))? as usize,
    };
    if palette.len() + steps > MAX_PALETTE {
        return Err(bad(CMD, format!("a palette holds at most {MAX_PALETTE} colours")));
    }
    let from = palette[i];
    // `steps` colours from just past `from` up to `to` itself, evenly apart.
    let ramp = (1..=steps).map(|k| {
        let t = k as f32 / steps as f32;
        [0, 1, 2].map(|c| ((from[c] + (to[c] - from[c]) * t) * 255.0).round() / 255.0)
    });
    let at = i + 1;
    palette.splice(at..at, ramp);
    set_palette(s, "Palette Ramp", CMD, palette)
}

pub fn specs() -> Vec<CommandSpec> {
    fn spec(id: &'static str, label: &'static str, params: &'static str, run: fn(&mut Session, &Value) -> Result<Value>) -> CommandSpec {
        CommandSpec { id, label, menu: &[], shortcut: None, params, enabled: has_doc, journal: true, run }
    }
    vec![
        spec("palette.set", "Set Palette", r##"{"colors":["#rrggbb",…] (at most 256)} → {count,colors}"##, palette_set),
        spec("palette.add", "Add Palette Color", r##"{"color":"#rrggbb"?=foreground} → {count,colors}"##, palette_add),
        spec("palette.remove", "Remove Palette Color", r##"{"index":n} → {count,colors}"##, palette_remove),
        spec("palette.fromSwatches", "Palette from Swatches", r##"{"group":name?=the first group} → {count,colors}"##, from_swatches),
        spec("palette.fromLayer", "Palette from Layer", r##"{} (the active layer's colours, dark to light) → {count,colors}"##, from_layer),
        spec(
            "palette.recolor",
            "Recolor Palette Color",
            r##"{"index":n,"color":"#rrggbb"?=foreground,"scope":"document|layer"="document"} (the entry and every pixel of exactly its colour) → {pixels,color}"##,
            recolor,
        ),
        spec(
            "palette.ramp",
            "Palette Ramp",
            r##"{"index":n,"color":"#rrggbb"?=foreground,"steps":1..32=4} (inserts the steps after the entry, ending at the colour) → {count,colors}"##,
            ramp,
        ),
    ]
}

#[cfg(test)]
mod tests;
