// AI Pixel Studio for the Layerwise Android port of PhotoCraft.
// Licensed under the Apache License, Version 2.0 (LICENSE-APACHE),
// also offered under the MIT License (LICENSE-MIT) like the rest of this tree.

//! PNG encode and decode through the existing codecs crate.

use photocraft_algo::ai_pixel::RgbaImage;
use photocraft_codecs::{ChannelLayout, EncodeOptions, Format, Image};

use crate::error::AiError;

pub fn encode_png(image: &RgbaImage) -> Result<Vec<u8>, AiError> {
    let owned = Image::from_u8(image.width, image.height, ChannelLayout::Rgba, image.pixels.clone())
        .map_err(|e| AiError::other(format!("couldn't encode the layer as PNG ({e})")))?;
    photocraft_codecs::encode(&owned, Format::Png, &EncodeOptions::default()).map_err(|e| AiError::other(format!("couldn't encode the layer as PNG ({e})")))
}

/// RGB PNG with transparent pixels flattened onto white. Retro Diffusion asks for RGB without alpha.
pub fn encode_png_rgb(image: &RgbaImage) -> Result<Vec<u8>, AiError> {
    let mut rgb = Vec::with_capacity((image.width as usize).saturating_mul(image.height as usize).saturating_mul(3));
    let mut i = 0usize;
    while let Some(px) = image.pixels.get(i..i + 4) {
        let a = px[3];
        if a == 255 {
            rgb.extend_from_slice(&px[..3]);
        } else {
            let cover = f32::from(a) / 255.0;
            for src in &px[..3] {
                let mixed = f32::from(*src) * cover + 255.0 * (1.0 - cover);
                rgb.push(mixed.round().clamp(0.0, 255.0) as u8);
            }
        }
        i += 4;
    }
    let owned =
        Image::from_u8(image.width, image.height, ChannelLayout::Rgb, rgb).map_err(|e| AiError::other(format!("couldn't encode the layer as PNG ({e})")))?;
    photocraft_codecs::encode(&owned, Format::Png, &EncodeOptions::default()).map_err(|e| AiError::other(format!("couldn't encode the layer as PNG ({e})")))
}

pub fn decode_png(bytes: &[u8]) -> Result<RgbaImage, AiError> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(AiError::other("the image from the server is larger than 16 MB"));
    }
    let image = photocraft_codecs::decode(bytes).map_err(|e| AiError::other(format!("the server's image couldn't be read ({e})")))?;
    if image.width() > 4096 || image.height() > 4096 {
        return Err(AiError::other("the server's image is larger than 4096 pixels on a side"));
    }
    RgbaImage::new(image.width(), image.height(), image.to_rgba8()).map_err(AiError::other)
}

/// One-pixel-tall RGB PNG of palette colours, for Retro Diffusion's `input_palette`.
pub fn palette_png(palette: &[[u8; 3]]) -> Result<Vec<u8>, AiError> {
    if palette.is_empty() {
        return Err(AiError::bad("the palette is empty"));
    }
    let width = u32::try_from(palette.len()).unwrap_or(256).min(256);
    let mut rgb = Vec::with_capacity((width as usize) * 3);
    for color in palette.iter().take(width as usize) {
        rgb.extend_from_slice(color);
    }
    let owned = Image::from_u8(width, 1, ChannelLayout::Rgb, rgb).map_err(|e| AiError::other(format!("couldn't encode the palette ({e})")))?;
    photocraft_codecs::encode(&owned, Format::Png, &EncodeOptions::default()).map_err(|e| AiError::other(format!("couldn't encode the palette ({e})")))
}
