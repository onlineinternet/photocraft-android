// AI Pixel Studio for the Layerwise Android port of PhotoCraft.
// Licensed under the Apache License, Version 2.0 (LICENSE-APACHE),
// also offered under the MIT License (LICENSE-MIT) like the rest of this tree.

//! Providers for AI Pixel Studio. The UI talks to [`AIPixelProvider`]; ComfyUI and Retro Diffusion
//! are two implementations of it. Network access goes through [`transport`], so tests never dial
//! a real server.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod comfy;
pub mod error;
pub mod id;
pub mod pngutil;
pub mod provider;
pub mod retro;
pub mod secrets;
pub mod style;
pub mod transport;
pub mod urlutil;
pub mod workflow;

#[cfg(not(target_arch = "wasm32"))]
pub mod native;

pub use comfy::{AuthKind, ComfyProvider};
pub use error::{AiError, AiErrorKind};
pub use provider::{AIPixelProvider, Control, GenerateOptions, Health, NopControl};
pub use retro::{DEFAULT_ORIGIN, RetroProvider};
pub use style::{RdFamily, Style};
