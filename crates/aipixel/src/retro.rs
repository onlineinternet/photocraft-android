// AI Pixel Studio for the Layerwise Android port of PhotoCraft.
// Licensed under the Apache License, Version 2.0 (LICENSE-APACHE),
// also offered under the MIT License (LICENSE-MIT) like the rest of this tree.

//! Retro Diffusion's public v2 API (`POST /v2/inferences`, `GET /v2/inferences/credits`).
//!
//! The documented public origin is the default. A key is sent only as the `X-RD-Token` header and
//! is never copied into an error. `negative` is sent when present, but current models ignore it.

use std::time::{Duration, Instant};

use base64::Engine;
use photocraft_algo::ai_pixel::RgbaImage;
use serde_json::{Value, json};

use crate::error::{AiError, clip_secret};
use crate::id;
use crate::pngutil;
use crate::provider::{AIPixelProvider, Control, GenerateOptions, Health, check};
use crate::secrets::{self, RETRO_TOKEN};
use crate::style::{prompt_style, with_prefix};
use crate::transport::{HttpRequest, HttpResponse, current};
use crate::urlutil;

/// Documented public API origin. Not a credential.
pub const DEFAULT_ORIGIN: &str = "https://api.retrodiffusion.ai";
const USER_AGENT: &str = "Layerwise-AIPixel/1";

#[derive(Clone, Debug)]
pub struct RetroProvider {
    pub base_url: String,
}

impl RetroProvider {
    pub fn new(base_url: &str) -> Result<Self, AiError> {
        let base = if base_url.trim().is_empty() { DEFAULT_ORIGIN } else { base_url.trim() };
        urlutil::validate_base(base)?;
        Ok(Self { base_url: base.trim_end_matches('/').to_string() })
    }
}

impl AIPixelProvider for RetroProvider {
    fn generate_txt2img(
        &self,
        prompt: &str,
        negative_prompt: &str,
        width: u32,
        height: u32,
        palette: &[[u8; 3]],
        options: &GenerateOptions<'_>,
    ) -> Result<RgbaImage, AiError> {
        check_size(width, height)?;
        let mut body = common_body(prompt, negative_prompt, width, height, options);
        attach_palette(&mut body, palette, options)?;
        self.infer(body, options.control)
    }

    fn generate_img2img(
        &self,
        source: &RgbaImage,
        _mask: Option<&RgbaImage>,
        prompt: &str,
        strength: f32,
        palette: &[[u8; 3]],
        options: &GenerateOptions<'_>,
    ) -> Result<RgbaImage, AiError> {
        check_size(source.width, source.height)?;
        if !(0.0..=1.0).contains(&strength) {
            return Err(AiError::bad("strength has to be from 0 to 1"));
        }
        let mut body = common_body(prompt, options.negative, source.width, source.height, options);
        body["input_image"] = json!(base64::engine::general_purpose::STANDARD.encode(pngutil::encode_png_rgb(source)?));
        body["strength"] = json!((f64::from(strength) * 100.0).round() / 100.0);
        attach_palette(&mut body, palette, options)?;
        self.infer(body, options.control)
    }

    fn check_health_and_credits(&self) -> Result<Health, AiError> {
        let (resp, secret) = self.call("GET", "/v2/inferences/credits", None, false)?;
        let bytes = expect_ok(resp, &secret)?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| AiError::other("the credits response wasn't JSON"))?;
        Ok(health_from(&value))
    }
}

impl RetroProvider {
    fn infer(&self, body: Value, control: &dyn Control) -> Result<RgbaImage, AiError> {
        check(control)?;
        control.progress(0.15, "Sending the prompt…");
        let (resp, secret) = self.call("POST", "/v2/inferences", Some(body.to_string().into_bytes()), true)?;
        let bytes = expect_ok(resp, &secret)?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| AiError::other("the inference response wasn't JSON"))?;
        let result = if value.get("base64_images").and_then(|v| v.as_array()).is_some_and(|a| !a.is_empty()) {
            value
        } else {
            let task_id = value.get("task_id").and_then(|v| v.as_str()).ok_or_else(|| AiError::other("Retro Diffusion didn't return an image or a task id"))?;
            self.poll(task_id, control)?
        };
        let bytes = first_image(&result).ok_or_else(|| AiError::other("Retro Diffusion finished without an image"))?;
        control.progress(0.92, "Reading the result…");
        pngutil::decode_png(&bytes)
    }

    fn poll(&self, task_id: &str, control: &dyn Control) -> Result<Value, AiError> {
        let path = format!("/v2/inferences/tasks/{}", urlutil::query_encode(task_id));
        let started = Instant::now();
        for step in 1..120 {
            check(control)?;
            if started.elapsed() > Duration::from_secs(180) {
                return Err(AiError::timeout("Retro Diffusion didn't finish within 3 minutes."));
            }
            control.progress((0.2 + (step as f32) * 0.005).min(0.85), "Waiting for Retro Diffusion…");
            let (resp, secret) = self.call("GET", &path, None, false)?;
            let bytes = expect_ok(resp, &secret)?;
            let value: Value = serde_json::from_slice(&bytes).map_err(|_| AiError::other("the task response wasn't JSON"))?;
            match value.get("status").and_then(|v| v.as_str()) {
                Some("pending" | "running") => pause(control)?,
                Some("succeeded") => return Ok(value.get("result").cloned().unwrap_or(value)),
                Some("failed") => return Err(failed_task(&value)),
                _ => return Err(AiError::other("Retro Diffusion returned a task without a status")),
            }
        }
        Err(AiError::timeout("Retro Diffusion didn't finish within 3 minutes."))
    }

    fn call(&self, method: &str, path: &str, body: Option<Vec<u8>>, paid: bool) -> Result<(HttpResponse, String), AiError> {
        let token = secrets::get(RETRO_TOKEN).filter(|t| !t.is_empty()).ok_or_else(|| AiError::auth("Enter a Retro Diffusion API key."))?;
        let url = urlutil::join(&self.base_url, path)?;
        let mut headers = vec![("User-Agent".into(), USER_AGENT.into()), ("X-RD-Token".into(), token.clone())];
        if body.is_some() {
            headers.push(("Content-Type".into(), "application/json".into()));
        }
        if paid {
            headers.push(("Idempotency-Key".into(), id::uuid_v4()?));
        }
        let response =
            current().round_trip(&HttpRequest { method: method.into(), url, headers, body: body.unwrap_or_default(), timeout: Duration::from_secs(45) })?;
        Ok((response, token))
    }
}

fn common_body(prompt: &str, negative: &str, width: u32, height: u32, options: &GenerateOptions<'_>) -> Value {
    let mut body = json!({
        "prompt": with_prefix(options.style, prompt),
        "prompt_style": prompt_style(options.family, options.style),
        "width": width,
        "height": height,
        "num_images": 1,
        "seed": options.seed,
        "remove_bg": options.remove_bg,
    });
    if !negative.is_empty() {
        body["negative"] = json!(negative);
    }
    body
}

fn attach_palette(body: &mut Value, palette: &[[u8; 3]], options: &GenerateOptions<'_>) -> Result<(), AiError> {
    if options.lock_palette && !palette.is_empty() {
        let png = pngutil::palette_png(palette)?;
        body["input_palette"] = json!(base64::engine::general_purpose::STANDARD.encode(png));
    }
    Ok(())
}

fn check_size(width: u32, height: u32) -> Result<(), AiError> {
    if (12..=512).contains(&width) && (12..=512).contains(&height) {
        Ok(())
    } else {
        Err(AiError::bad(format!("Retro Diffusion needs each side between 12 and 512. This image is {width}×{height}.")))
    }
}

fn expect_ok(resp: HttpResponse, secret: &str) -> Result<Vec<u8>, AiError> {
    if (200..300).contains(&resp.status) {
        return Ok(resp.body);
    }
    Err(rd_error(resp.status, &resp.body, secret))
}

fn rd_error(status: u16, body: &[u8], secret: &str) -> AiError {
    let detail = server_message(body, secret);
    match status {
        401 => AiError::auth("That Retro Diffusion key was refused. It may be missing, invalid, or expired."),
        403 => AiError::auth("That Retro Diffusion key doesn't have permission for this request."),
        402 => AiError::credits("This Retro Diffusion account is out of credits."),
        408 | 504 => AiError::timeout(format!("Retro Diffusion timed out. {detail}")),
        400 | 422 => AiError::bad(format!("Retro Diffusion rejected the request. {detail}")),
        _ => AiError::other(format!("Retro Diffusion returned HTTP {status}. {detail}")),
    }
}

fn failed_task(value: &Value) -> AiError {
    let code = value.pointer("/error/code").and_then(|v| v.as_str()).unwrap_or("");
    let message = value.pointer("/error/message").and_then(|v| v.as_str()).unwrap_or("the generation failed");
    if code == "not_enough_balance" || value.get("status_code").and_then(|v| v.as_u64()) == Some(402) {
        AiError::credits("This Retro Diffusion account is out of credits.")
    } else {
        AiError::other(format!("Retro Diffusion failed ({code}). {message}"))
    }
}

fn server_message(body: &[u8], secret: &str) -> String {
    let text = String::from_utf8_lossy(body);
    let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    let message = value.pointer("/error/message").and_then(|v| v.as_str()).or_else(|| value.get("detail").and_then(|v| v.as_str())).unwrap_or(text.as_ref());
    clip_secret(message, secret)
}

fn health_from(value: &Value) -> Health {
    let credits = value.get("credits").and_then(|v| v.as_f64());
    let balance = value.get("balance").and_then(|v| v.as_f64());
    let message = match (credits, balance) {
        (Some(credits), Some(balance)) => format!("Retro Diffusion balance {balance:.2} ({credits:.0} credits)."),
        (None, Some(balance)) => format!("Retro Diffusion balance {balance:.2}."),
        (Some(credits), None) => format!("Retro Diffusion credits {credits:.0}."),
        (None, None) => "Retro Diffusion answered, but didn't include a balance.".into(),
    };
    Health { message, credits: balance.or(credits) }
}

fn first_image(value: &Value) -> Option<Vec<u8>> {
    let encoded = value.get("base64_images").and_then(|v| v.as_array()).and_then(|a| a.first()).and_then(|v| v.as_str())?;
    if encoded.len() > 16 * 1024 * 1024 {
        return None;
    }
    base64::engine::general_purpose::STANDARD.decode(encoded).ok()
}

#[cfg(not(target_arch = "wasm32"))]
fn pause(control: &dyn Control) -> Result<(), AiError> {
    for _ in 0..8 {
        check(control)?;
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn pause(control: &dyn Control) -> Result<(), AiError> {
    check(control)
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

    fn options() -> GenerateOptions<'static> {
        GenerateOptions {
            seed: 9,
            denoise: 0.4,
            style: Style::TopDown,
            checkpoint: "",
            lora: "",
            pixel_nodes: false,
            workflow: None,
            family: RdFamily::Pro,
            remove_bg: true,
            lock_palette: true,
            negative: "blur",
            control: &NopControl,
        }
    }

    fn png_b64() -> String {
        let image = RgbaImage::new(1, 1, vec![4, 5, 6, 255]).expect("image");
        let png = pngutil::encode_png(&image).expect("png");
        base64::engine::general_purpose::STANDARD.encode(png)
    }

    #[test]
    fn a_queued_job_is_polled_and_the_token_stays_out_of_errors() {
        let _gate = gate();
        secrets::set(RETRO_TOKEN, "test-token").expect("secret");
        let accepted = br#"{"status":"accepted","task_id":"task-1"}"#.to_vec();
        let pending = br#"{"status":"running"}"#.to_vec();
        let done = format!(r#"{{"status":"succeeded","result":{{"base64_images":["{}"]}}}}"#, png_b64()).into_bytes();
        let mock = std::sync::Arc::new(MockTransport::new(
            vec![
                Ok(HttpResponse { status: 200, body: accepted }),
                Ok(HttpResponse { status: 200, body: pending }),
                Ok(HttpResponse { status: 200, body: done }),
            ],
            Vec::new(),
        ));
        install(Some(mock.clone()));
        let provider = RetroProvider::new("").expect("provider");
        let image = provider.generate_txt2img("a tree", "blur", 32, 32, &[[1, 2, 3]], &options()).expect("image");
        assert_eq!(image.pixels, vec![4, 5, 6, 255]);
        let seen = mock.seen();
        assert!(seen[0].url.ends_with("/v2/inferences") && seen[0].auth == ["rd-token"]);
        assert!(seen.iter().any(|s| s.url.contains("/v2/inferences/tasks/task-1")));
        install(None);
        secrets::clear(RETRO_TOKEN).expect("clear");
    }

    #[test]
    fn a_refused_key_and_an_empty_balance_are_distinct() {
        let _gate = gate();
        secrets::set(RETRO_TOKEN, "test-token").expect("secret");
        let mock = std::sync::Arc::new(MockTransport::new(vec![Ok(HttpResponse { status: 401, body: b"test-token invalid".to_vec() })], Vec::new()));
        install(Some(mock));
        let provider = RetroProvider::new(DEFAULT_ORIGIN).expect("provider");
        let err = provider.check_health_and_credits().expect_err("401");
        assert_eq!(err.kind, crate::error::AiErrorKind::Auth);
        assert!(!err.to_string().contains("test-token"));
        let mock = std::sync::Arc::new(MockTransport::new(
            vec![Ok(HttpResponse { status: 402, body: br#"{"error":{"code":"not_enough_balance","message":"nope"}}"#.to_vec() })],
            Vec::new(),
        ));
        install(Some(mock));
        let err = provider.generate_txt2img("a tree", "", 16, 16, &[], &options()).expect_err("402");
        assert_eq!(err.kind, crate::error::AiErrorKind::Credits);
        assert!(!err.to_string().contains("test-token"));
        install(None);
        secrets::clear(RETRO_TOKEN).expect("clear");
    }

    #[test]
    fn credits_read_the_balance() {
        let _gate = gate();
        secrets::set(RETRO_TOKEN, "test-token").expect("secret");
        let mock =
            std::sync::Arc::new(MockTransport::new(vec![Ok(HttpResponse { status: 200, body: br#"{"credits":0,"balance":12.5}"#.to_vec() })], Vec::new()));
        install(Some(mock.clone()));
        let health = RetroProvider::new("").expect("provider").check_health_and_credits().expect("credits");
        assert_eq!(health.credits, Some(12.5));
        assert!(health.message.contains("12.50"));
        assert!(mock.seen()[0].url.ends_with("/v2/inferences/credits"));
        install(None);
        secrets::clear(RETRO_TOKEN).expect("clear");
    }

    #[test]
    fn a_canvas_under_12_pixels_is_refused() {
        let _gate = gate();
        secrets::set(RETRO_TOKEN, "test-token").expect("secret");
        install(Some(std::sync::Arc::new(MockTransport::new(Vec::new(), Vec::new()))));
        let err = RetroProvider::new("").expect("provider").generate_txt2img("a", "", 8, 8, &[], &options()).expect_err("size");
        assert!(err.to_string().contains("12 and 512"));
        install(None);
        secrets::clear(RETRO_TOKEN).expect("clear");
    }
}
