// AI Pixel Studio for the Layerwise Android port of PhotoCraft.
// Licensed under the Apache License, Version 2.0 (LICENSE-APACHE),
// also offered under the MIT License (LICENSE-MIT) like the rest of this tree.

//! The seam the UI and the engine call. A job doesn't know whether ComfyUI or Retro Diffusion
//! is on the other side.

use photocraft_algo::ai_pixel::RgbaImage;

use crate::error::AiError;
use crate::style::{RdFamily, Style};

pub struct GenerateOptions<'a> {
    pub seed: u64,
    pub denoise: f32,
    pub style: Style,
    pub checkpoint: &'a str,
    pub lora: &'a str,
    pub pixel_nodes: bool,
    pub workflow: Option<&'a str>,
    pub family: RdFamily,
    pub remove_bg: bool,
    pub lock_palette: bool,
    pub negative: &'a str,
    pub control: &'a dyn Control,
}

#[derive(Clone, Debug)]
pub struct Health {
    pub message: String,
    pub credits: Option<f64>,
}

pub trait Control: Send + Sync {
    fn cancelled(&self) -> bool;
    fn progress(&self, fraction: f32, message: &str);
}

pub struct NopControl;

impl Control for NopControl {
    fn cancelled(&self) -> bool {
        false
    }

    fn progress(&self, _fraction: f32, _message: &str) {}
}

pub fn check(control: &dyn Control) -> Result<(), AiError> {
    if control.cancelled() { Err(AiError::cancelled()) } else { Ok(()) }
}

pub trait AIPixelProvider: Send + Sync {
    fn generate_txt2img(
        &self,
        prompt: &str,
        negative_prompt: &str,
        width: u32,
        height: u32,
        palette: &[[u8; 3]],
        options: &GenerateOptions<'_>,
    ) -> Result<RgbaImage, AiError>;

    fn generate_img2img(
        &self,
        source: &RgbaImage,
        mask: Option<&RgbaImage>,
        prompt: &str,
        strength: f32,
        palette: &[[u8; 3]],
        options: &GenerateOptions<'_>,
    ) -> Result<RgbaImage, AiError>;

    fn check_health_and_credits(&self) -> Result<Health, AiError>;
}
