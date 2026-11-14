// AI Pixel Studio for the Layerwise Android port of PhotoCraft.
// Licensed under the Apache License, Version 2.0 (LICENSE-APACHE),
// also offered under the MIT License (LICENSE-MIT) like the rest of this tree.

//! A ComfyUI server on the LAN, on Tailscale, or behind HTTPS.
//!
//! Progress comes from `ws(s)://<host>/ws?clientId=<id>` (`executing`, `progress`, `executed`).
//! The picture is fetched with `GET /view`, and `/history/<prompt_id>` is the fallback when the
//! socket closes without one.

use std::time::{Duration, Instant};

use base64::Engine;
use photocraft_algo::ai_pixel::{RgbaImage, model_scale, pad_to_multiple_of_8, upscale_nearest};
use serde_json::{Value, json};

use crate::error::{AiError, clip_secret};
use crate::pngutil;
use crate::provider::{AIPixelProvider, Control, GenerateOptions, Health, check};
use crate::secrets::{self, COMFY_BASIC, COMFY_BEARER};
use crate::style::with_prefix;
use crate::transport::{HttpRequest, HttpResponse, WsEvent, current};
use crate::workflow::{self, IMG2IMG, TXT2IMG, Tags};
use crate::{id, urlutil};

const USER_AGENT: &str = "Layerwise-AIPixel/1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthKind {
    None,
    Bearer,
    Basic,
}

#[derive(Clone, Debug)]
pub struct ComfyProvider {
    pub base_url: String,
    pub auth: AuthKind,
    pub basic_user: String,
    pub client_id: String,
}

impl ComfyProvider {
    pub fn new(base_url: String, auth: AuthKind, basic_user: String) -> Result<Self, AiError> {
        urlutil::validate_base(base_url.trim())?;
        Ok(Self { base_url: base_url.trim().trim_end_matches('/').to_string(), auth, basic_user, client_id: id::uuid_v4()? })
    }
}

impl AIPixelProvider for ComfyProvider {
    fn generate_txt2img(
        &self,
        prompt: &str,
        negative_prompt: &str,
        width: u32,
        height: u32,
        _palette: &[[u8; 3]],
        options: &GenerateOptions<'_>,
    ) -> Result<RgbaImage, AiError> {
        check(options.control)?;
        let (width, height) = latent_size(width, height)?;
        let graph = self
            .graph(options, GraphInput { img2img: false, prompt, negative: negative_prompt, image: "", mask: "", width, height, denoise: options.denoise })?;
        self.run_graph(&graph, options.control, options.pixel_nodes && options.workflow.is_none())
    }

    fn generate_img2img(
        &self,
        source: &RgbaImage,
        mask: Option<&RgbaImage>,
        prompt: &str,
        strength: f32,
        _palette: &[[u8; 3]],
        options: &GenerateOptions<'_>,
    ) -> Result<RgbaImage, AiError> {
        check(options.control)?;
        let uploaded = self.upload_pair(source, mask, options.control)?;
        let graph = self.graph(
            options,
            GraphInput {
                img2img: true,
                prompt,
                negative: options.negative,
                image: &uploaded.0,
                mask: &uploaded.1,
                width: source.width,
                height: source.height,
                denoise: strength,
            },
        )?;
        self.run_graph(&graph, options.control, options.pixel_nodes && options.workflow.is_none())
    }

    fn check_health_and_credits(&self) -> Result<Health, AiError> {
        let (resp, secret) = self.call("GET", "/system_stats", Vec::new(), None, Duration::from_secs(8))?;
        let _ = ok_body(resp, &secret, "The ComfyUI server")?;
        Ok(Health { message: "Connected to ComfyUI.".into(), credits: None })
    }
}

impl ComfyProvider {
    fn graph(&self, options: &GenerateOptions<'_>, input: GraphInput<'_>) -> Result<Value, AiError> {
        let template = match options.workflow {
            Some(custom) => custom,
            None if input.img2img => IMG2IMG,
            None => TXT2IMG,
        };
        let bundled = options.workflow.is_none();
        if bundled && options.checkpoint.trim().is_empty() {
            return Err(AiError::bad("Enter a checkpoint name. The bundled workflow needs one."));
        }
        let required = required_tags(options.workflow.is_some(), input.img2img);
        let tags = Tags {
            prompt: &with_prefix(options.style, input.prompt),
            negative: input.negative,
            seed: options.seed,
            denoise: input.denoise,
            image: input.image,
            mask: input.mask,
            checkpoint: options.checkpoint,
            lora: options.lora,
            width: input.width,
            height: input.height,
        };
        let mut graph = workflow::prepare(template, &tags, required)?;
        if bundled && options.lora.trim().is_empty() {
            workflow::drop_empty_lora(&mut graph);
        }
        if input.img2img && options.workflow.is_some() && !input.mask.is_empty() && !template.contains("@mask") {
            options.control.progress(0.12, "The custom workflow has no @mask tag, so the selection mask wasn't used.");
        }
        Ok(graph)
    }

    fn upload_pair(&self, source: &RgbaImage, mask: Option<&RgbaImage>, control: &dyn Control) -> Result<(String, String), AiError> {
        if let Some(mask) = mask
            && (mask.width != source.width || mask.height != source.height)
        {
            return Err(AiError::bad("the mask is a different size from the layer"));
        }
        control.progress(0.05, "Preparing the layer…");
        let source_up = for_upload(source)?;
        let mask_image = match mask {
            Some(mask) => for_upload(mask)?,
            None => blank_mask(source_up.width, source_up.height)?,
        };
        control.progress(0.1, "Uploading the layer…");
        let image_name = self.upload_png(&pngutil::encode_png(&source_up)?, "layerwise-in.png")?;
        let mask_name = self.upload_png(&pngutil::encode_png(&mask_image)?, "layerwise-mask.png")?;
        Ok((image_name, mask_name))
    }

    fn upload_png(&self, png: &[u8], filename: &str) -> Result<String, AiError> {
        let boundary = format!("Layerwise{}", &self.client_id.replace('-', ""));
        let (content_type, body) = multipart(&boundary, filename, png);
        let (resp, secret) = self.call("POST", "/upload/image", body, Some(&content_type), Duration::from_secs(60))?;
        let bytes = ok_body(resp, &secret, "The ComfyUI server")?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| AiError::other("the upload response wasn't JSON"))?;
        value
            .get("name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .ok_or_else(|| AiError::other("the server didn't name the uploaded image"))
    }

    fn run_graph(&self, graph: &Value, control: &dyn Control, pixel_nodes: bool) -> Result<RgbaImage, AiError> {
        check(control)?;
        let mut graph = graph.clone();
        if pixel_nodes {
            self.try_splice_pixel(&mut graph, control);
        }
        control.progress(0.2, "Sending the workflow…");
        let prompt_id = self.submit(&graph)?;
        let images = self.watch(&prompt_id, control)?;
        let Some(image) = images.first() else {
            return Err(AiError::other("ComfyUI finished without an image."));
        };
        control.progress(0.92, "Downloading the result…");
        let png = self.view(image)?;
        pngutil::decode_png(&png)
    }

    fn try_splice_pixel(&self, graph: &mut Value, control: &dyn Control) {
        match self.call("GET", "/object_info", Vec::new(), None, Duration::from_secs(20)) {
            Ok((resp, secret)) => match ok_body(resp, &secret, "ComfyUI") {
                Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
                    Ok(info) => {
                        if !splice_pixel(graph, &info) {
                            control.progress(0.18, "No usable PixelOE or K-centroid node on this server. Using the image as returned.");
                        }
                    }
                    Err(_) => control.progress(0.18, "Couldn't read /object_info, so no pixel node was added."),
                },
                Err(_) => control.progress(0.18, "Couldn't read /object_info, so no pixel node was added."),
            },
            Err(_) => control.progress(0.18, "Couldn't read /object_info, so no pixel node was added."),
        }
    }

    fn submit(&self, graph: &Value) -> Result<String, AiError> {
        let body = json!({"prompt": graph, "client_id": self.client_id}).to_string().into_bytes();
        let (resp, secret) = self.call("POST", "/prompt", body, Some("application/json"), Duration::from_secs(30))?;
        let status = resp.status;
        let value: Value = serde_json::from_slice(&resp.body).unwrap_or(Value::Null);
        if let Some(message) = node_error_text(&value) {
            return Err(AiError::workflow(message));
        }
        let bytes = ok_body(HttpResponse { status, body: resp.body }, &secret, "The ComfyUI server")?;
        let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        value
            .get("prompt_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .ok_or_else(|| AiError::other("ComfyUI didn't return a prompt id"))
    }

    fn watch(&self, prompt_id: &str, control: &dyn Control) -> Result<Vec<ImageRef>, AiError> {
        let url = urlutil::ws_url(&self.base_url, &self.client_id)?;
        let headers = self.auth_headers()?.0;
        let mut socket = match current().open_ws(&url, &headers) {
            Ok(socket) => socket,
            Err(_) => return self.history(prompt_id),
        };
        let mut images = Vec::new();
        let started = Instant::now();
        for _ in 0..10_000 {
            check(control)?;
            if started.elapsed() > Duration::from_secs(180) {
                let _ = self.interrupt();
                return Err(AiError::timeout("The ComfyUI job didn't finish within 3 minutes."));
            }
            match socket.recv(Duration::from_secs(2))? {
                WsEvent::Timeout => {}
                WsEvent::Closed => break,
                WsEvent::Text(text) => {
                    if let Some(fraction) = progress_fraction(&text) {
                        control.progress(0.15 + 0.7 * fraction, "ComfyUI is drawing…");
                    }
                    images.extend(executed_images(&text, prompt_id));
                    if execution_done(&text, prompt_id) {
                        break;
                    }
                }
            }
        }
        if images.is_empty() { self.history(prompt_id) } else { Ok(images) }
    }

    fn history(&self, prompt_id: &str) -> Result<Vec<ImageRef>, AiError> {
        let path = format!("/history/{}", urlutil::query_encode(prompt_id));
        let (resp, secret) = self.call("GET", &path, Vec::new(), None, Duration::from_secs(20))?;
        let bytes = ok_body(resp, &secret, "The ComfyUI server")?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| AiError::other("the history response wasn't JSON"))?;
        Ok(history_images(&value, prompt_id))
    }

    fn view(&self, image: &ImageRef) -> Result<Vec<u8>, AiError> {
        let path = format!(
            "/view?filename={}&subfolder={}&type={}",
            urlutil::query_encode(&image.filename),
            urlutil::query_encode(&image.subfolder),
            urlutil::query_encode(&image.kind)
        );
        let (resp, secret) = self.call("GET", &path, Vec::new(), None, Duration::from_secs(60))?;
        ok_body(resp, &secret, "The ComfyUI server")
    }

    fn interrupt(&self) -> Result<(), AiError> {
        let body = json!({"client_id": self.client_id}).to_string().into_bytes();
        let (resp, secret) = self.call("POST", "/interrupt", body, Some("application/json"), Duration::from_secs(5))?;
        let _ = ok_body(resp, &secret, "The ComfyUI server")?;
        Ok(())
    }

    fn call(&self, method: &str, path: &str, body: Vec<u8>, content_type: Option<&str>, timeout: Duration) -> Result<(HttpResponse, String), AiError> {
        let url = urlutil::join(&self.base_url, path)?;
        let (auth, secret) = self.auth_headers()?;
        let mut headers = vec![("User-Agent".into(), USER_AGENT.into())];
        if let Some(content_type) = content_type {
            headers.push(("Content-Type".into(), content_type.to_string()));
        }
        headers.extend(auth);
        let response = current().round_trip(&HttpRequest { method: method.into(), url, headers, body, timeout })?;
        Ok((response, secret))
    }

    fn auth_headers(&self) -> Result<(Vec<(String, String)>, String), AiError> {
        match self.auth {
            AuthKind::None => Ok((Vec::new(), String::new())),
            AuthKind::Bearer => {
                let token = secrets::get(COMFY_BEARER).filter(|t| !t.is_empty()).ok_or_else(|| AiError::auth("Enter a bearer token, or switch auth off."))?;
                Ok((vec![("Authorization".into(), format!("Bearer {token}"))], token))
            }
            AuthKind::Basic => {
                if self.basic_user.is_empty() || self.basic_user.contains(':') {
                    return Err(AiError::auth("Enter a username for basic auth, without a colon."));
                }
                let password = secrets::get(COMFY_BASIC).filter(|t| !t.is_empty()).ok_or_else(|| AiError::auth("Enter the password, or switch auth off."))?;
                let raw = format!("{}:{password}", self.basic_user);
                let encoded = base64::engine::general_purpose::STANDARD.encode(raw.as_bytes());
                Ok((vec![("Authorization".into(), format!("Basic {encoded}"))], password))
            }
        }
    }
}

struct GraphInput<'a> {
    img2img: bool,
    prompt: &'a str,
    negative: &'a str,
    image: &'a str,
    mask: &'a str,
    width: u32,
    height: u32,
    denoise: f32,
}

fn required_tags(custom: bool, img2img: bool) -> &'static [&'static str] {
    match (custom, img2img) {
        (true, true) => &["@prompt", "@seed", "@image", "@denoise"],
        (true, false) => &["@prompt", "@seed"],
        (false, true) => &["@prompt", "@seed", "@image", "@denoise", "@negative", "@checkpoint", "@mask"],
        (false, false) => &["@prompt", "@seed", "@negative", "@checkpoint", "@width", "@height", "@denoise"],
    }
}

fn latent_size(width: u32, height: u32) -> Result<(u32, u32), AiError> {
    let scale = model_scale(width, height).map_err(AiError::bad)?;
    let scaled_w = width.checked_mul(scale).ok_or_else(|| AiError::bad("that size is too large"))?;
    let scaled_h = height.checked_mul(scale).ok_or_else(|| AiError::bad("that size is too large"))?;
    Ok((round_up_8(scaled_w), round_up_8(scaled_h)))
}

fn round_up_8(value: u32) -> u32 {
    let up = value.saturating_add(7) / 8 * 8;
    if up == 0 { 8 } else { up.min(1024) }
}

fn for_upload(src: &RgbaImage) -> Result<RgbaImage, AiError> {
    let scale = model_scale(src.width, src.height).map_err(AiError::bad)?;
    let scaled = if scale <= 1 { src.clone() } else { upscale_nearest(src, scale).map_err(AiError::bad)? };
    pad_to_multiple_of_8(&scaled).map_err(AiError::bad)
}

fn blank_mask(width: u32, height: u32) -> Result<RgbaImage, AiError> {
    RgbaImage::new(width, height, vec![0; (width as usize).saturating_mul(height as usize).saturating_mul(4)]).map_err(AiError::bad)
}

fn multipart(boundary: &str, filename: &str, png: &[u8]) -> (String, Vec<u8>) {
    let mut body = Vec::new();
    body.extend(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"{filename}\"\r\nContent-Type: image/png\r\n\r\n").into_bytes(),
    );
    body.extend_from_slice(png);
    body.extend(format!("\r\n--{boundary}--\r\n").into_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

fn ok_body(resp: HttpResponse, secret: &str, what: &str) -> Result<Vec<u8>, AiError> {
    if resp.status == 401 || resp.status == 403 {
        return Err(AiError::auth(format!("{what} refused the login (HTTP {}). Check the token or the username and password.", resp.status)));
    }
    if !(200..300).contains(&resp.status) {
        let detail = clip_secret(&String::from_utf8_lossy(&resp.body), secret);
        return Err(AiError::other(format!("{what} returned HTTP {}. {detail}", resp.status)));
    }
    Ok(resp.body)
}

fn node_error_text(value: &Value) -> Option<String> {
    let errors = value.get("node_errors")?.as_object()?;
    if errors.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    for (id, node) in errors {
        if let Some(list) = node.get("errors").and_then(|e| e.as_array()) {
            for item in list {
                if let Some(message) = item.get("message").and_then(|m| m.as_str()) {
                    parts.push(format!("node {id}: {message}"));
                }
            }
        }
    }
    let text = if parts.is_empty() { "ComfyUI rejected the workflow.".into() } else { parts.join(" ") };
    Some(clip_secret(&text, ""))
}

struct ImageRef {
    filename: String,
    subfolder: String,
    kind: String,
}

fn executed_images(text: &str, prompt_id: &str) -> Vec<ImageRef> {
    let Ok(value) = serde_json::from_str::<Value>(text) else { return Vec::new() };
    if value.get("type").and_then(|t| t.as_str()) != Some("executed") {
        return Vec::new();
    }
    let data = value.get("data").unwrap_or(&value);
    if data.get("prompt_id").and_then(|v| v.as_str()).is_some_and(|id| id != prompt_id) {
        return Vec::new();
    }
    image_list(data.get("output"))
}

fn execution_done(text: &str, prompt_id: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(text) else { return false };
    if value.get("type").and_then(|t| t.as_str()) != Some("executing") {
        return false;
    }
    let data = value.get("data").unwrap_or(&value);
    data.get("node").map(|n| n.is_null()).unwrap_or(false) && data.get("prompt_id").and_then(|v| v.as_str()) == Some(prompt_id)
}

fn progress_fraction(text: &str) -> Option<f32> {
    let value: Value = serde_json::from_str(text).ok()?;
    if value.get("type").and_then(|t| t.as_str()) != Some("progress") {
        return None;
    }
    let data = value.get("data").unwrap_or(&value);
    let value = data.get("value")?.as_f64()?;
    let max = data.get("max")?.as_f64()?;
    if max <= 0.0 { None } else { Some(((value / max) as f32).clamp(0.0, 1.0)) }
}

fn history_images(value: &Value, prompt_id: &str) -> Vec<ImageRef> {
    let entry = value.get(prompt_id).or_else(|| value.as_object().and_then(|m| m.values().next()));
    let Some(entry) = entry else { return Vec::new() };
    let Some(outputs) = entry.get("outputs").and_then(|v| v.as_object()) else { return Vec::new() };
    let mut images = Vec::new();
    for output in outputs.values() {
        images.extend(image_list(output.get("images").or(Some(output))));
    }
    images
}

fn image_list(value: Option<&Value>) -> Vec<ImageRef> {
    let Some(list) = value.and_then(|v| v.as_array().or_else(|| v.get("images").and_then(|i| i.as_array()))) else {
        return Vec::new();
    };
    list.iter().filter_map(image_ref).collect()
}

fn image_ref(value: &Value) -> Option<ImageRef> {
    Some(ImageRef {
        filename: value.get("filename")?.as_str()?.to_string(),
        subfolder: value.get("subfolder").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        kind: value.get("type").and_then(|v| v.as_str()).unwrap_or("output").to_string(),
    })
}

struct PixelSpec {
    class_name: String,
    image_input: String,
    extras: Vec<(String, Value)>,
    image_output: u64,
}

fn splice_pixel(graph: &mut Value, info: &Value) -> bool {
    let Some(spec) = find_pixel(info) else { return false };
    let Some(nodes) = graph.as_object_mut() else { return false };
    let Some(save) = nodes.get("1") else { return false };
    if save.get("class_type").and_then(|v| v.as_str()) != Some("SaveImage") {
        return false;
    }
    let Some(images) = save.get("inputs").and_then(|v| v.get("images")).cloned() else { return false };
    let mut inputs = serde_json::Map::new();
    inputs.insert(spec.image_input, images);
    for (key, value) in spec.extras {
        inputs.insert(key, value);
    }
    nodes.insert("px1".into(), json!({"class_type": spec.class_name, "inputs": Value::Object(inputs)}));
    let Some(slot) = nodes.get_mut("1").and_then(|n| n.get_mut("inputs")).and_then(|v| v.as_object_mut()) else { return false };
    slot.insert("images".into(), json!(["px1", spec.image_output]));
    true
}

fn find_pixel(info: &Value) -> Option<PixelSpec> {
    for (name, spec) in info.as_object()? {
        let folded = name.to_ascii_lowercase().replace(['-', '_'], "");
        if (folded.contains("pixeloe") || folded.contains("kcentroid"))
            && let Some(found) = usable_pixel(name, spec)
        {
            return Some(found);
        }
    }
    None
}

fn usable_pixel(name: &str, spec: &Value) -> Option<PixelSpec> {
    let required = spec.get("input")?.get("required")?.as_object()?;
    let mut image_input = None;
    let mut extras = Vec::new();
    for (key, defn) in required {
        let kind = defn.as_array()?.first()?.as_str()?;
        if kind == "IMAGE" && image_input.is_none() {
            image_input = Some(key.clone());
            continue;
        }
        extras.push((key.clone(), defn.as_array()?.get(1)?.get("default")?.clone()));
    }
    let outputs = spec.get("output")?.as_array()?;
    Some(PixelSpec {
        class_name: name.to_string(),
        image_input: image_input?,
        extras,
        image_output: outputs.iter().position(|v| v.as_str() == Some("IMAGE"))? as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::NopControl;
    use crate::style::{RdFamily, Style};
    use crate::transport::{HttpResponse, MockTransport, install};
    fn gate() -> std::sync::MutexGuard<'static, ()> {
        crate::transport::test_gate()
    }

    fn options(workflow: Option<&str>) -> GenerateOptions<'_> {
        GenerateOptions {
            seed: 3,
            denoise: 0.4,
            style: Style::None,
            checkpoint: "pixel.safetensors",
            lora: "",
            pixel_nodes: false,
            workflow,
            family: RdFamily::Pro,
            remove_bg: false,
            lock_palette: false,
            negative: "blur",
            control: &NopControl,
        }
    }

    fn png() -> Vec<u8> {
        let image = RgbaImage::new(1, 1, vec![9, 8, 7, 255]).expect("image");
        pngutil::encode_png(&image).expect("png")
    }

    #[test]
    fn txt2img_uses_bearer_auth_without_leaking_the_token() {
        let _gate = gate();
        secrets::set(COMFY_BEARER, "test-token").expect("secret");
        let ws = vec![
            r#"{"type":"progress","data":{"value":1,"max":2,"prompt_id":"p1"}}"#.into(),
            r#"{"type":"executed","data":{"prompt_id":"p1","output":{"images":[{"filename":"out.png","subfolder":"","type":"output"}]}}}"#.into(),
            r#"{"type":"executing","data":{"node":null,"prompt_id":"p1"}}"#.into(),
        ];
        let http = vec![Ok(HttpResponse { status: 200, body: br#"{"prompt_id":"p1"}"#.to_vec() }), Ok(HttpResponse { status: 200, body: png() })];
        let mock = std::sync::Arc::new(MockTransport::new(http, ws));
        install(Some(mock.clone()));
        let provider = ComfyProvider::new("http://192.168.1.5:8188".into(), AuthKind::Bearer, String::new()).expect("provider");
        let image = provider.generate_txt2img("a sword", "blur", 16, 16, &[], &options(None)).expect("generate");
        assert_eq!(image.pixels, vec![9, 8, 7, 255]);
        let seen = mock.seen();
        assert!(seen.iter().any(|s| s.method == "POST" && s.url.ends_with("/prompt") && s.auth == ["bearer"]));
        assert!(seen.iter().any(|s| s.method == "WS" && s.url.starts_with("ws://192.168.1.5:8188/ws?clientId=") && s.auth == ["bearer"]));
        install(None);
        secrets::clear(COMFY_BEARER).expect("clear");
    }

    #[test]
    fn an_auth_failure_does_not_echo_the_token() {
        let _gate = gate();
        secrets::set(COMFY_BASIC, "test-token").expect("secret");
        let mock = std::sync::Arc::new(MockTransport::new(vec![Ok(HttpResponse { status: 401, body: b"nope test-token".to_vec() })], Vec::new()));
        install(Some(mock.clone()));
        let provider = ComfyProvider::new("https://comfy.example".into(), AuthKind::Basic, "artist".into()).expect("provider");
        let err = provider.check_health_and_credits().expect_err("401");
        assert_eq!(err.kind, crate::error::AiErrorKind::Auth);
        assert!(!err.to_string().contains("test-token"), "{}", err);
        assert_eq!(mock.seen()[0].auth, ["basic"]);
        install(None);
        secrets::clear(COMFY_BASIC).expect("clear");
    }

    #[test]
    fn img2img_uploads_then_falls_back_to_history() {
        let _gate = gate();
        let http = vec![
            Ok(HttpResponse { status: 200, body: br#"{"name":"layerwise-in.png"}"#.to_vec() }),
            Ok(HttpResponse { status: 200, body: br#"{"name":"layerwise-mask.png"}"#.to_vec() }),
            Ok(HttpResponse { status: 200, body: br#"{"prompt_id":"p9"}"#.to_vec() }),
            Ok(HttpResponse { status: 200, body: br#"{"p9":{"outputs":{"1":{"images":[{"filename":"out.png","subfolder":"a","type":"output"}]}}}}"#.to_vec() }),
            Ok(HttpResponse { status: 200, body: png() }),
        ];
        let mock = std::sync::Arc::new(MockTransport::new(http, Vec::new()));
        install(Some(mock.clone()));
        let provider = ComfyProvider::new("http://10.1.2.3:8188".into(), AuthKind::None, String::new()).expect("provider");
        let source = RgbaImage::new(16, 16, [1u8, 2, 3, 255].repeat(256)).expect("source");
        let image = provider.generate_img2img(&source, None, "shade", 0.4, &[], &options(None)).expect("img2img");
        assert_eq!(image.width, 1);
        let seen = mock.seen();
        assert!(seen.iter().any(|s| s.url.contains("/upload/image")));
        assert!(seen.iter().any(|s| s.url.contains("/history/p9")));
        assert!(seen.iter().any(|s| s.url.contains("/view?") && s.url.contains("subfolder=a")));
        install(None);
    }

    #[test]
    fn a_public_cleartext_address_is_refused_before_any_request() {
        let err = ComfyProvider::new("http://8.8.8.8:8188".into(), AuthKind::None, String::new()).expect_err("public http");
        assert!(err.to_string().contains("Cleartext"));
    }

    #[test]
    fn a_custom_workflow_without_tags_is_rejected() {
        let _gate = gate();
        install(Some(std::sync::Arc::new(MockTransport::new(Vec::new(), Vec::new()))));
        let provider = ComfyProvider::new("http://127.0.0.1:8188".into(), AuthKind::None, String::new()).expect("provider");
        let err = provider.generate_txt2img("a sword", "", 32, 32, &[], &options(Some(r#"{"1":{"inputs":{"text":"hello"}}}"#))).expect_err("tags");
        assert_eq!(err.kind, crate::error::AiErrorKind::Workflow);
        assert!(err.to_string().contains("@prompt") && err.to_string().contains("@seed"));
        install(None);
    }

    #[test]
    fn a_pixel_node_from_object_info_is_spliced_in_front_of_save() {
        let info = json!({"PixelOE": {"input": {"required": {"image": ["IMAGE"], "mode": ["STRING", {"default": "contrast"}]}}, "output": ["IMAGE"]}});
        let mut graph = workflow::prepare(
            TXT2IMG,
            &workflow::Tags { prompt: "a", negative: "", seed: 1, denoise: 1.0, image: "", mask: "", checkpoint: "c", lora: "", width: 64, height: 64 },
            &["@prompt", "@seed"],
        )
        .expect("graph");
        workflow::drop_empty_lora(&mut graph);
        assert!(splice_pixel(&mut graph, &info));
        assert_eq!(graph["1"]["inputs"]["images"][0], "px1");
        assert_eq!(graph["px1"]["class_type"], "PixelOE");
        assert_eq!(graph["px1"]["inputs"]["mode"], "contrast");
    }
}
