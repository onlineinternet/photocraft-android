// AI Pixel Studio for the Layerwise Android port of PhotoCraft.
// Licensed under the Apache License, Version 2.0 (LICENSE-APACHE),
// also offered under the MIT License (LICENSE-MIT) like the rest of this tree.

//! Style chips. ComfyUI only sees the prompt prefix. Retro Diffusion also gets a documented
//! `prompt_style` id. There is no public 4-colour Game Boy style, so that chip uses the closest
//! simple / low-res style plus a prompt prefix.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    None,
    Gba,
    DsPortrait,
    TopDown,
    Isometric,
    GameBoy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RdFamily {
    Pro,
    Fast,
    Plus,
    Core,
}

impl Style {
    pub fn parse(text: &str) -> Self {
        match text {
            "gba" => Self::Gba,
            "ds" => Self::DsPortrait,
            "topdown" => Self::TopDown,
            "iso" => Self::Isometric,
            "gb" => Self::GameBoy,
            _ => Self::None,
        }
    }

    pub fn prefix(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Gba => "16-bit GBA pixel art, ",
            Self::DsPortrait => "Nintendo DS character portrait, pixel art, ",
            Self::TopDown => "top-down RPG tile, pixel art, ",
            Self::Isometric => "isometric pixel art, ",
            Self::GameBoy => "4-color Game Boy pixel art, ",
        }
    }
}

impl RdFamily {
    pub fn parse(text: &str) -> Self {
        match text {
            "fast" => Self::Fast,
            "plus" => Self::Plus,
            "core" => Self::Core,
            _ => Self::Pro,
        }
    }
}

pub fn with_prefix(style: Style, prompt: &str) -> String {
    let prefix = style.prefix();
    if prefix.is_empty() || prompt.starts_with(prefix) { prompt.to_string() } else { format!("{prefix}{prompt}") }
}

/// A `prompt_style` id that the public Retro Diffusion docs list for this family and chip.
pub fn prompt_style(family: RdFamily, style: Style) -> &'static str {
    match (family, style) {
        (RdFamily::Fast, Style::DsPortrait) => "rd_fast__portrait",
        (RdFamily::Pro, Style::TopDown) => "rd_pro__topdown",
        (RdFamily::Plus, Style::TopDown) => "rd_plus__topdown_asset",
        (RdFamily::Core, Style::TopDown) => "rd_core__topdown",
        (RdFamily::Pro, Style::Isometric) => "rd_pro__isometric",
        (RdFamily::Plus, Style::Isometric) => "rd_plus__isometric",
        (RdFamily::Core, Style::Isometric) => "rd_core__isometric",
        (RdFamily::Pro, Style::GameBoy) => "rd_pro__simple",
        (RdFamily::Fast, Style::GameBoy) => "rd_fast__low_res",
        (RdFamily::Plus, Style::GameBoy) => "rd_plus__low_res",
        (RdFamily::Core, Style::GameBoy) => "rd_core__simple",
        (RdFamily::Pro, _) => "rd_pro__default",
        (RdFamily::Fast, _) => "rd_fast__default",
        (RdFamily::Plus, _) => "rd_plus__default",
        (RdFamily::Core, _) => "rd_core__default",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chips_map_onto_documented_style_ids() {
        assert_eq!(prompt_style(RdFamily::Pro, Style::TopDown), "rd_pro__topdown");
        assert_eq!(prompt_style(RdFamily::Fast, Style::DsPortrait), "rd_fast__portrait");
        assert_eq!(prompt_style(RdFamily::Core, Style::Isometric), "rd_core__isometric");
        assert_eq!(prompt_style(RdFamily::Plus, Style::GameBoy), "rd_plus__low_res");
        assert_eq!(prompt_style(RdFamily::Fast, Style::Gba), "rd_fast__default");
        assert!(with_prefix(Style::Gba, "a knight").starts_with("16-bit GBA"));
    }
}
