// AI Pixel Studio for the Layerwise Android port of PhotoCraft.
// Licensed under the Apache License, Version 2.0 (LICENSE-APACHE),
// also offered under the MIT License (LICENSE-MIT) like the rest of this tree.

//! Client-side pixel cleanup for AI Pixel Studio.
//!
//! A model is asked for a larger, softer picture. These functions put that picture back on the
//! document's pixel grid and, when asked, on the document palette. They only see bytes: no
//! document, no network.

use photocraft_color::convert::srgb_to_lab;

/// Straight 8-bit RGBA, row-major, length `width * height * 4`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Where nearest-palette distance is measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteSpace {
    /// CIE L*a*b* (D50), the default. Close colours stay close.
    Lab,
    /// Plain sRGB distance.
    Rgb,
}

impl RgbaImage {
    /// Reject a buffer that is not exactly `width * height * 4` bytes, or a side of 0 or above 4096.
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, String> {
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return Err(format!("image size {width}×{height} is outside 1×1 to 4096×4096"));
        }
        let need = (width as usize).checked_mul(height as usize).and_then(|n| n.checked_mul(4)).ok_or_else(|| "image is too large".to_string())?;
        if pixels.len() != need {
            return Err(format!("expected {need} bytes for a {width}×{height} image, got {}", pixels.len()));
        }
        Ok(Self { width, height, pixels })
    }

    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        let i = (y as usize).checked_mul(self.width as usize)?.checked_add(x as usize)?.checked_mul(4)?;
        let p = self.pixels.get(i..i.checked_add(4)?)?;
        Some([p[0], p[1], p[2], p[3]])
    }
}

/// Nearest-neighbour resize. Each output pixel copies the source pixel under its centre.
pub fn resize_nearest(src: &RgbaImage, dst_w: u32, dst_h: u32) -> Result<RgbaImage, String> {
    if dst_w == 0 || dst_h == 0 || dst_w > 4096 || dst_h > 4096 {
        return Err(format!("target size {dst_w}×{dst_h} is outside 1×1 to 4096×4096"));
    }
    if src.width == dst_w && src.height == dst_h {
        return Ok(src.clone());
    }
    let mut pixels = vec![0u8; (dst_w as usize) * (dst_h as usize) * 4];
    for y in 0..dst_h {
        let sy = ((y as u64).saturating_mul(src.height as u64) / dst_h as u64) as u32;
        let sy = sy.min(src.height.saturating_sub(1));
        for x in 0..dst_w {
            let sx = ((x as u64).saturating_mul(src.width as u64) / dst_w as u64) as u32;
            let sx = sx.min(src.width.saturating_sub(1));
            let px = src.pixel(sx, sy).ok_or_else(|| "source pixel missing".to_string())?;
            let i = ((y as usize) * (dst_w as usize) + (x as usize)) * 4;
            if let Some(dst) = pixels.get_mut(i..i + 4) {
                dst.copy_from_slice(&px);
            }
        }
    }
    RgbaImage::new(dst_w, dst_h, pixels)
}

/// Nearest-neighbour upscale by an integer `scale` (each source pixel becomes a `scale`×`scale` block).
pub fn upscale_nearest(src: &RgbaImage, scale: u32) -> Result<RgbaImage, String> {
    if scale == 0 || scale > 64 {
        return Err("upscale factor must be from 1 to 64".into());
    }
    let width = src.width.checked_mul(scale).ok_or_else(|| "upscaled image is too large".to_string())?;
    let height = src.height.checked_mul(scale).ok_or_else(|| "upscaled image is too large".to_string())?;
    if width > 4096 || height > 4096 {
        return Err(format!("upscaled size {width}×{height} is above 4096"));
    }
    let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
    for y in 0..height {
        for x in 0..width {
            let sx = x / scale;
            let sy = y / scale;
            let px = src.pixel(sx, sy).ok_or_else(|| "source pixel missing".to_string())?;
            let i = ((y as usize) * (width as usize) + (x as usize)) * 4;
            if let Some(dst) = pixels.get_mut(i..i + 4) {
                dst.copy_from_slice(&px);
            }
        }
    }
    RgbaImage::new(width, height, pixels)
}

/// Integer nearest-neighbour scale for a small sprite sent to a diffusion model.
///
/// A long side of at most 128 grows so it lands near 512, or near 1024 when an 8× scale would pass
/// 512. Anything larger is sent at 1× (the caller still snaps the sides to a multiple of 8). A side
/// of 0 or above 1024 is refused.
pub fn model_scale(width: u32, height: u32) -> Result<u32, String> {
    if width == 0 || height == 0 || width > 1024 || height > 1024 {
        return Err(format!("{width}×{height} can't be sent (each side must be 1 to 1024)"));
    }
    let long = width.max(height);
    if long > 128 {
        return Ok(1);
    }
    let target: u32 = if long.saturating_mul(8) <= 512 { 512 } else { 1024 };
    Ok((target / long).clamp(1, 64))
}

/// Repeat the last row and column until both sides are multiples of 8, without passing 1024.
pub fn pad_to_multiple_of_8(src: &RgbaImage) -> Result<RgbaImage, String> {
    let width = round_up_8(src.width).min(1024);
    let height = round_up_8(src.height).min(1024);
    if width == src.width && height == src.height {
        return Ok(src.clone());
    }
    if width == 0 || height == 0 {
        return Err("nothing to pad".into());
    }
    let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
    for y in 0..height {
        let sy = y.min(src.height.saturating_sub(1));
        for x in 0..width {
            let sx = x.min(src.width.saturating_sub(1));
            let px = src.pixel(sx, sy).ok_or_else(|| "pad source missing".to_string())?;
            let i = ((y as usize) * (width as usize) + (x as usize)) * 4;
            if let Some(dst) = pixels.get_mut(i..i + 4) {
                dst.copy_from_slice(&px);
            }
        }
    }
    RgbaImage::new(width, height, pixels)
}

fn round_up_8(v: u32) -> u32 {
    let up = v.saturating_add(7) / 8 * 8;
    if up == 0 { 8 } else { up }
}

/// Contrast-aware downscale onto a `dst_w`×`dst_h` grid.
///
/// Each output pixel is the most common colour in its source cell (a 1-centroid / mode), not the
/// average, so a hard edge stays one colour or the other. A cell that is mostly transparent
/// (alpha under 128) becomes fully transparent. Ties keep the colour nearest the cell's top-left
/// sample, which makes an integer nearest-neighbour upscale reverse exactly.
pub fn downscale_grid(src: &RgbaImage, dst_w: u32, dst_h: u32) -> Result<RgbaImage, String> {
    if dst_w == 0 || dst_h == 0 || dst_w > 4096 || dst_h > 4096 {
        return Err(format!("target size {dst_w}×{dst_h} is outside 1×1 to 4096×4096"));
    }
    let mut pixels = vec![0u8; (dst_w as usize) * (dst_h as usize) * 4];
    for y in 0..dst_h {
        let y0 = cell_start(y, src.height, dst_h);
        let y1 = cell_end(y, src.height, dst_h, y0);
        for x in 0..dst_w {
            let x0 = cell_start(x, src.width, dst_w);
            let x1 = cell_end(x, src.width, dst_w, x0);
            let color = cell_color(src, x0, x1, y0, y1)?;
            let i = ((y as usize) * (dst_w as usize) + (x as usize)) * 4;
            if let Some(dst) = pixels.get_mut(i..i + 4) {
                dst.copy_from_slice(&color);
            }
        }
    }
    RgbaImage::new(dst_w, dst_h, pixels)
}

fn cell_start(i: u32, src: u32, dst: u32) -> u32 {
    ((i as u64).saturating_mul(src as u64) / (dst as u64).max(1)) as u32
}

fn cell_end(i: u32, src: u32, dst: u32, start: u32) -> u32 {
    let end = cell_start(i.saturating_add(1), src, dst);
    end.max(start.saturating_add(1)).min(src.max(1))
}

fn cell_color(src: &RgbaImage, x0: u32, x1: u32, y0: u32, y1: u32) -> Result<[u8; 4], String> {
    let anchor = src.pixel(x0, y0).ok_or_else(|| "empty source cell".to_string())?;
    let mut total = 0u32;
    let mut clear = 0u32;
    let mut counts: Vec<([u8; 4], u32)> = Vec::new();
    for y in y0..y1 {
        for x in x0..x1 {
            let px = src.pixel(x, y).ok_or_else(|| "source cell ran outside the image".to_string())?;
            total = total.saturating_add(1);
            if px[3] < 128 {
                clear = clear.saturating_add(1);
            }
            if let Some(slot) = counts.iter_mut().find(|(c, _)| *c == px) {
                slot.1 = slot.1.saturating_add(1);
            } else {
                counts.push((px, 1));
            }
        }
    }
    if total == 0 {
        return Ok(anchor);
    }
    if clear.saturating_mul(2) >= total {
        return Ok([0, 0, 0, 0]);
    }
    let opaque: Vec<([u8; 4], u32)> = counts.iter().copied().filter(|(c, _)| c[3] >= 128).collect();
    let pool = if opaque.is_empty() { &counts } else { &opaque };
    let best = pool.iter().max_by_key(|(c, n)| (*n, distance_rank(anchor, *c))).map(|(c, _)| *c);
    Ok(best.unwrap_or(anchor))
}

/// Higher is better on a tie: the colour closer to `anchor` (chebyshev, inverted).
fn distance_rank(anchor: [u8; 4], color: [u8; 4]) -> u8 {
    let d = anchor.iter().zip(color.iter()).map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0);
    255u8.saturating_sub(d)
}

/// Snap every visible pixel to the nearest palette colour. Alpha is copied unchanged, and a fully
/// transparent pixel is left as it is (it is not forced onto a palette colour).
///
/// An empty palette is an error: locking to "no colours" would have to invent one.
pub fn quantize_palette(src: &RgbaImage, palette: &[[u8; 3]], space: PaletteSpace) -> Result<RgbaImage, String> {
    if palette.is_empty() {
        return Err("the palette is empty".into());
    }
    if palette.len() > 256 {
        return Err("a palette can hold at most 256 colours".into());
    }
    let labs: Vec<[f32; 3]> = palette.iter().map(|rgb| srgb_to_lab(rgb.map(|c| f32::from(c) / 255.0))).collect();
    let mut pixels = src.pixels.clone();
    let count = (src.width as usize).saturating_mul(src.height as usize);
    for n in 0..count {
        let i = n.saturating_mul(4);
        let Some(px) = pixels.get(i..i + 4) else { break };
        if px[3] == 0 {
            continue;
        }
        let rgb = [px[0], px[1], px[2]];
        let nearest = match space {
            PaletteSpace::Rgb => nearest_rgb(rgb, palette),
            PaletteSpace::Lab => nearest_lab(rgb, palette, &labs),
        };
        if let Some(slot) = pixels.get_mut(i..i + 3) {
            slot.copy_from_slice(&nearest);
        }
    }
    RgbaImage::new(src.width, src.height, pixels)
}

fn nearest_rgb(rgb: [u8; 3], palette: &[[u8; 3]]) -> [u8; 3] {
    let mut best = palette.first().copied().unwrap_or(rgb);
    let mut best_d = u32::MAX;
    for entry in palette {
        let d = rgb.iter().zip(entry.iter()).map(|(a, b)| u32::from(a.abs_diff(*b)).saturating_pow(2)).fold(0u32, u32::saturating_add);
        if d < best_d {
            best_d = d;
            best = *entry;
        }
    }
    best
}

fn nearest_lab(rgb: [u8; 3], palette: &[[u8; 3]], labs: &[[f32; 3]]) -> [u8; 3] {
    let lab = srgb_to_lab(rgb.map(|c| f32::from(c) / 255.0));
    let mut best_i = 0usize;
    let mut best_d = f32::MAX;
    for (i, entry) in labs.iter().enumerate() {
        let d = lab.iter().zip(entry.iter()).fold(0.0f32, |acc, (a, b)| {
            let t = a - b;
            if acc.is_finite() && t.is_finite() { acc + t * t } else { f32::MAX }
        });
        if d < best_d {
            best_d = d;
            best_i = i;
        }
    }
    palette.get(best_i).copied().unwrap_or(rgb)
}

/// Turn a flat corner background transparent. The four corners must already agree (within 12
/// levels per channel). Pixels connected to a corner and within that tolerance lose their alpha.
/// A picture whose corners disagree, or that is already transparent there, is returned unchanged.
pub fn knock_out_corners(src: &RgbaImage) -> Result<RgbaImage, String> {
    let w = src.width;
    let h = src.height;
    let corners = [(0, 0), (w.saturating_sub(1), 0), (0, h.saturating_sub(1)), (w.saturating_sub(1), h.saturating_sub(1))];
    let mut samples = Vec::with_capacity(4);
    for (x, y) in corners {
        samples.push(src.pixel(x, y).ok_or_else(|| "corner outside the image".to_string())?);
    }
    let Some(bg) = samples.first().copied() else {
        return Ok(src.clone());
    };
    if bg[3] < 128 || samples.iter().any(|p| !close_color(bg, *p)) {
        return Ok(src.clone());
    }
    let mut pixels = src.pixels.clone();
    let mut seen = vec![false; (w as usize).saturating_mul(h as usize)];
    let mut queue = Vec::new();
    for (x, y) in corners {
        push_clear(&mut queue, &mut seen, w, x, y);
    }
    let mut steps = 0u64;
    let limit = (w as u64).saturating_mul(h as u64).saturating_add(1);
    while let Some((x, y)) = queue.pop() {
        steps = steps.saturating_add(1);
        if steps > limit {
            break;
        }
        let Some(px) = src.pixel(x, y) else { continue };
        if !close_color(bg, px) || px[3] < 128 {
            continue;
        }
        let i = ((y as usize).saturating_mul(w as usize).saturating_add(x as usize)).saturating_mul(4);
        if let Some(slot) = pixels.get_mut(i..i + 4) {
            slot[3] = 0;
        }
        if x > 0 {
            push_clear(&mut queue, &mut seen, w, x - 1, y);
        }
        if y > 0 {
            push_clear(&mut queue, &mut seen, w, x, y - 1);
        }
        if x + 1 < w {
            push_clear(&mut queue, &mut seen, w, x + 1, y);
        }
        if y + 1 < h {
            push_clear(&mut queue, &mut seen, w, x, y + 1);
        }
    }
    RgbaImage::new(w, h, pixels)
}

fn close_color(a: [u8; 4], b: [u8; 4]) -> bool {
    a[0].abs_diff(b[0]) <= 12 && a[1].abs_diff(b[1]) <= 12 && a[2].abs_diff(b[2]) <= 12
}

fn push_clear(queue: &mut Vec<(u32, u32)>, seen: &mut [bool], w: u32, x: u32, y: u32) {
    let i = (y as usize).saturating_mul(w as usize).saturating_add(x as usize);
    if let Some(flag) = seen.get_mut(i)
        && !*flag
    {
        *flag = true;
        queue.push((x, y));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(w: u32, h: u32, pixels: Vec<[u8; 4]>) -> RgbaImage {
        let bytes = pixels.into_iter().flatten().collect();
        RgbaImage::new(w, h, bytes).expect("image")
    }

    #[test]
    fn an_8x_upscale_downscales_exactly_to_the_original() {
        let original = img(2, 2, vec![[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 128], [0, 0, 0, 0]]);
        let up = upscale_nearest(&original, 8).expect("upscale");
        assert_eq!((up.width, up.height), (16, 16));
        let back = downscale_grid(&up, 2, 2).expect("downscale");
        assert_eq!(back, original);
    }

    #[test]
    fn a_soft_edge_keeps_the_majority_colour_instead_of_a_blend() {
        // A 2×1 cell that is three quarters black and one quarter white must stay black, not gray.
        let mut pixels = Vec::new();
        for x in 0..4u32 {
            let color = if x == 3 { [255, 255, 255, 255] } else { [0, 0, 0, 255] };
            pixels.push(color);
        }
        let src = img(4, 1, pixels);
        let out = downscale_grid(&src, 2, 1).expect("downscale");
        assert_eq!(out.pixel(0, 0), Some([0, 0, 0, 255]));
        // The second cell is half and half; the top-left of that cell (x = 2) is black, so the tie
        // stays black rather than flipping to white.
        assert_eq!(out.pixel(1, 0), Some([0, 0, 0, 255]));
    }

    #[test]
    fn a_mostly_transparent_cell_stays_transparent() {
        let src = img(2, 1, vec![[255, 0, 0, 0], [255, 0, 0, 10]]);
        let out = downscale_grid(&src, 1, 1).expect("downscale");
        assert_eq!(out.pixel(0, 0), Some([0, 0, 0, 0]));
    }

    #[test]
    fn the_quantizer_outputs_only_palette_colours_and_keeps_alpha() {
        let palette = [[255, 0, 0], [0, 255, 0], [0, 0, 255]];
        let src = img(3, 1, vec![[200, 30, 20, 255], [30, 180, 40, 200], [1, 2, 3, 0]]);
        for space in [PaletteSpace::Lab, PaletteSpace::Rgb] {
            let out = quantize_palette(&src, &palette, space).expect("quantize");
            assert_eq!(out.pixel(0, 0), Some([255, 0, 0, 255]), "{space:?}");
            assert_eq!(out.pixel(1, 0), Some([0, 255, 0, 200]), "{space:?}");
            assert_eq!(out.pixel(2, 0), Some([1, 2, 3, 0]), "{space:?} transparent pixel is unchanged");
            for n in 0..2 {
                let px = out.pixel(n, 0).expect("pixel");
                assert!(palette.contains(&[px[0], px[1], px[2]]), "{space:?} {px:?}");
            }
        }
    }

    #[test]
    fn an_empty_palette_is_refused() {
        let src = img(1, 1, vec![[1, 2, 3, 255]]);
        assert!(quantize_palette(&src, &[], PaletteSpace::Lab).is_err());
    }

    #[test]
    fn small_sprites_scale_toward_512_or_1024_and_pad_to_a_multiple_of_eight() {
        assert_eq!(model_scale(16, 16).expect("16"), 32);
        assert_eq!(model_scale(64, 64).expect("64"), 8);
        assert_eq!(model_scale(128, 128).expect("128"), 8);
        assert_eq!(model_scale(320, 180).expect("already large"), 1);
        let sprite = img(20, 20, vec![[1, 2, 3, 255]; 400]);
        let scaled = upscale_nearest(&sprite, model_scale(20, 20).expect("scale")).expect("up");
        let padded = pad_to_multiple_of_8(&scaled).expect("pad");
        assert!(padded.width.is_multiple_of(8) && padded.height.is_multiple_of(8));
        assert!(padded.width <= 1024 && padded.height <= 1024);
        assert!(model_scale(0, 8).is_err());
        assert!(model_scale(2000, 8).is_err());
    }

    #[test]
    fn a_flat_corner_background_becomes_transparent_and_the_subject_stays() {
        let mut pixels = vec![[240, 240, 240, 255]; 9];
        pixels[4] = [200, 0, 0, 255];
        let src = img(3, 3, pixels);
        let out = knock_out_corners(&src).expect("knock out");
        assert_eq!(out.pixel(0, 0).expect("corner")[3], 0);
        assert_eq!(out.pixel(1, 1), Some([200, 0, 0, 255]));
        let mixed = img(2, 2, vec![[0, 0, 0, 255], [255, 255, 255, 255], [255, 255, 255, 255], [255, 255, 255, 255]]);
        assert_eq!(knock_out_corners(&mixed).expect("unchanged"), mixed);
    }
}
