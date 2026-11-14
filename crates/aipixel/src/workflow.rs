// AI Pixel Studio for the Layerwise Android port of PhotoCraft.
// Licensed under the Apache License, Version 2.0 (LICENSE-APACHE),
// also offered under the MIT License (LICENSE-MIT) like the rest of this tree.

//! API-format ComfyUI workflows. A tag that is the whole string becomes a JSON number when it is
//! a seed, a denoise, or a size. Other tags are substituted inside strings. Keys that start with
//! `_` are documentation and are removed before the graph is posted.

use serde_json::{Number, Value};

use crate::error::AiError;

pub const TXT2IMG: &str = include_str!("../workflows/txt2img_pixel.json");
pub const IMG2IMG: &str = include_str!("../workflows/img2img_pixel.json");

pub const MAX_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug)]
pub struct Tags<'a> {
    pub prompt: &'a str,
    pub negative: &'a str,
    pub seed: u64,
    pub denoise: f32,
    pub image: &'a str,
    pub mask: &'a str,
    pub checkpoint: &'a str,
    pub lora: &'a str,
    pub width: u32,
    pub height: u32,
}

pub fn require_tags(text: &str, required: &[&str]) -> Result<(), AiError> {
    let missing: Vec<&str> = required.iter().copied().filter(|tag| !text.contains(tag)).collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(AiError::workflow(format!("This workflow is missing {}. Put each tag in the input it should fill.", missing.join(", "))))
    }
}

pub fn prepare(text: &str, tags: &Tags<'_>, required: &[&str]) -> Result<Value, AiError> {
    if text.len() > MAX_BYTES {
        return Err(AiError::workflow("the workflow is larger than 256 KB"));
    }
    require_tags(text, required)?;
    let mut value: Value = serde_json::from_str(text).map_err(|e| AiError::workflow(format!("this workflow isn't valid JSON ({e})")))?;
    if !value.is_object() {
        return Err(AiError::workflow("a workflow has to be a JSON object of nodes"));
    }
    substitute(&mut value, tags);
    Ok(value)
}

/// Drop the bundled LoRA node when no LoRA name was given, and point its users back at the checkpoint.
pub fn drop_empty_lora(graph: &mut Value) {
    let Some(nodes) = graph.as_object_mut() else { return };
    let is_lora = nodes.get("10").and_then(|n| n.get("class_type")).and_then(|c| c.as_str()) == Some("LoraLoader");
    if !is_lora {
        return;
    }
    nodes.remove("10");
    for node in nodes.values_mut() {
        rewire_lora(node);
    }
}

fn rewire_lora(node: &mut Value) {
    let Some(inputs) = node.get_mut("inputs").and_then(|v| v.as_object_mut()) else { return };
    for value in inputs.values_mut() {
        let Some(link) = value.as_array() else { continue };
        if link.first().and_then(|v| v.as_str()) != Some("10") {
            continue;
        }
        let slot = link.get(1).and_then(|v| v.as_u64()).unwrap_or(0);
        *value = Value::Array(vec![Value::String("4".into()), Value::Number(Number::from(slot))]);
    }
}

fn substitute(value: &mut Value, tags: &Tags<'_>) {
    match value {
        Value::Object(map) => {
            map.retain(|key, _| !key.starts_with('_'));
            for child in map.values_mut() {
                substitute(child, tags);
            }
        }
        Value::Array(items) => {
            for child in items {
                substitute(child, tags);
            }
        }
        Value::String(text) => {
            if let Some(number) = whole_number(text, tags) {
                *value = Value::Number(number);
            } else if text.contains('@') {
                *text = replace_text(text, tags);
            }
        }
        _ => {}
    }
}

fn whole_number(text: &str, tags: &Tags<'_>) -> Option<Number> {
    match text {
        "@seed" => Some(Number::from(tags.seed)),
        "@width" => Some(Number::from(tags.width)),
        "@height" => Some(Number::from(tags.height)),
        "@denoise" => Number::from_f64(rounded_denoise(tags.denoise)).or_else(|| Number::from_f64(1.0)),
        _ => None,
    }
}

fn rounded_denoise(denoise: f32) -> f64 {
    let hundredths = (f64::from(denoise) * 100.0).round();
    hundredths / 100.0
}

fn replace_text(text: &str, tags: &Tags<'_>) -> String {
    let seed = tags.seed.to_string();
    let width = tags.width.to_string();
    let height = tags.height.to_string();
    let denoise = format!("{:.2}", rounded_denoise(tags.denoise));
    let pairs = [
        ("@prompt", tags.prompt),
        ("@negative", tags.negative),
        ("@image", tags.image),
        ("@mask", tags.mask),
        ("@checkpoint", tags.checkpoint),
        ("@lora", tags.lora),
        ("@seed", seed.as_str()),
        ("@width", width.as_str()),
        ("@height", height.as_str()),
        ("@denoise", denoise.as_str()),
    ];
    let mut out = text.to_string();
    for (tag, value) in pairs {
        if out.contains(tag) {
            out = out.replace(tag, value);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags() -> Tags<'static> {
        Tags {
            prompt: "a sword",
            negative: "blur",
            seed: 7,
            denoise: 0.45,
            image: "in.png",
            mask: "mask.png",
            checkpoint: "pixel.safetensors",
            lora: "",
            width: 64,
            height: 64,
        }
    }

    #[test]
    fn missing_tags_are_named() {
        let err = prepare("{\"3\": {\"inputs\": {\"text\": \"@prompt\"}}}", &tags(), &["@prompt", "@seed", "@image"]).expect_err("missing");
        let text = err.to_string();
        assert!(text.contains("@seed") && text.contains("@image"), "{text}");
        assert!(!text.contains("@prompt,"));
    }

    #[test]
    fn a_bundled_txt2img_graph_fills_numbers_and_drops_an_empty_lora() {
        let mut graph = prepare(TXT2IMG, &tags(), &["@prompt", "@seed", "@negative", "@checkpoint", "@width", "@height", "@denoise"]).expect("prepare");
        drop_empty_lora(&mut graph);
        assert!(graph.get("10").is_none());
        assert!(graph.get("_comment").is_none());
        assert_eq!(graph["6"]["inputs"]["text"], "a sword");
        assert_eq!(graph["3"]["inputs"]["seed"], 7);
        let denoise = graph["3"]["inputs"]["denoise"].as_f64().unwrap_or(0.0);
        assert!((denoise - 0.45).abs() < 0.001, "{denoise}");
        assert_eq!(graph["3"]["inputs"]["model"][0], "4");
    }

    #[test]
    fn malformed_json_is_a_workflow_error() {
        let err = prepare("{", &tags(), &[]).expect_err("json");
        assert!(err.to_string().contains("valid JSON"));
    }
}
