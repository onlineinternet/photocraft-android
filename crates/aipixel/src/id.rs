// AI Pixel Studio for the Layerwise Android port of PhotoCraft.
// Licensed under the Apache License, Version 2.0 (LICENSE-APACHE),
// also offered under the MIT License (LICENSE-MIT) like the rest of this tree.

//! Random ids. Native builds ask the operating system; the browser build has no generator because
//! the providers themselves aren't available there.

use crate::error::AiError;

pub fn uuid_v4() -> Result<String, AiError> {
    let mut bytes = [0u8; 16];
    fill(&mut bytes)?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    ))
}

/// A seed that fits in a JSON integer without rounding.
pub fn random_seed() -> Result<u64, AiError> {
    let mut bytes = [0u8; 4];
    fill(&mut bytes)?;
    Ok(u64::from(u32::from_le_bytes(bytes)))
}

#[cfg(not(target_arch = "wasm32"))]
fn fill(buf: &mut [u8]) -> Result<(), AiError> {
    getrandom::fill(buf).map_err(|_| AiError::other("the system couldn't produce a random id"))
}

#[cfg(target_arch = "wasm32")]
fn fill(_buf: &mut [u8]) -> Result<(), AiError> {
    Err(AiError::unsupported("AI Pixel Studio isn't available in this build."))
}
