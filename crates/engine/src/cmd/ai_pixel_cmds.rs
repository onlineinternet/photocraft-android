// AI Pixel Studio for the Layerwise Android port of PhotoCraft.
// Licensed under the Apache License, Version 2.0 (LICENSE-APACHE),
// also offered under the MIT License (LICENSE-MIT) like the rest of this tree.

//! `ai.pixel.generate` and `ai.pixel.health`. The picture is generated on a background job and
//! inserted as a new layer above the active one. The pixels that were already there are not
//! written. The call is not journaled: replaying it would spend another paid request.

#[cfg(not(target_arch = "wasm32"))]
use photocraft_aipixel::provider::GenerateOptions;
#[cfg(not(target_arch = "wasm32"))]
use photocraft_aipixel::{AIPixelProvider, AiError, AiErrorKind, ComfyProvider, Control, RetroProvider};
use photocraft_aipixel::{AuthKind, RdFamily, Style};
#[cfg(not(target_arch = "wasm32"))]
use photocraft_algo::ai_pixel::{self, PaletteSpace, RgbaImage};
#[cfg(not(target_arch = "wasm32"))]
use photocraft_doc::{DocId, Layer, LayerId};
#[cfg(not(target_arch = "wasm32"))]
use photocraft_geom::Rect;
#[cfg(not(target_arch = "wasm32"))]
use photocraft_raster::{Surface, from_rgba, from_rgba_into};
use serde_json::Value;
#[cfg(not(target_arch = "wasm32"))]
use serde_json::json;

#[cfg(not(target_arch = "wasm32"))]
use crate::jobs::{self, JobCtx};
use crate::{EngineError, Result, Session};

const GENERATE: &str = "ai.pixel.generate";
const HEALTH: &str = "ai.pixel.health";

pub fn specs() -> Vec<super::CommandSpec> {
    vec![
        super::CommandSpec {
            id: GENERATE,
            label: "AI Pixel Studio",
            menu: &[],
            shortcut: None,
            params: r#"{ "mode": "txt2img"|"img2img"|"batch", "provider": "comfy"|"retro", "prompt": "", "negative": "", "style": "gba"|"ds"|"topdown"|"iso"|"gb", "denoise": 0.45, "lockPalette": false, "maintainGrid": true, "removeBg": false, "baseUrl": "", "auth": "none"|"bearer"|"basic", "basicUser": "", "checkpoint": "", "lora": "", "pixelNodes": false, "workflow": "", "family": "pro"|"fast"|"plus"|"core", "seed": null }"#,
            enabled: super::enabled::has_doc,
            run: generate,
            journal: false,
        },
        super::CommandSpec {
            id: HEALTH,
            label: "AI Pixel Studio connection",
            menu: &[],
            shortcut: None,
            params: r#"{ "provider": "comfy"|"retro", "baseUrl": "", "auth": "none"|"bearer"|"basic", "basicUser": "" }"#,
            enabled: super::enabled::always,
            run: health,
            journal: false,
        },
    ]
}

fn bad(cmd: &str, msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: cmd.into(), msg: msg.into() }
}

fn generate(s: &mut Session, p: &Value) -> Result<Value> {
    start_generate(s, parse_generate(p)?)
}

fn health(s: &mut Session, p: &Value) -> Result<Value> {
    start_health(s, parse_health(p)?)
}

#[cfg(target_arch = "wasm32")]
fn start_generate(_s: &mut Session, _request: Request) -> Result<Value> {
    Err(EngineError::Other("AI Pixel Studio isn't available in this build.".into()))
}

#[cfg(target_arch = "wasm32")]
fn start_health(_s: &mut Session, _request: Request) -> Result<Value> {
    Err(EngineError::Other("AI Pixel Studio isn't available in this build.".into()))
}

#[cfg(not(target_arch = "wasm32"))]
fn start_generate(s: &mut Session, request: Request) -> Result<Value> {
    let source = capture(s, &request)?;
    let label = layer_name(&request, &source);
    let history = label.clone();
    jobs::run(s, &history, false, move |ctx| produce(request, source, ctx), move |s, made| insert_layer(s, &label, made))
}

#[cfg(not(target_arch = "wasm32"))]
fn start_health(s: &mut Session, request: Request) -> Result<Value> {
    jobs::run(
        s,
        "AI Pixel Studio",
        false,
        move |ctx| {
            if ctx.cancelled() {
                return Err(EngineError::Cancelled);
            }
            let health = provider(&request)?.check_health_and_credits().map_err(map_err)?;
            Ok(health.message)
        },
        |_s, message| Ok(json!({ "message": message })),
    )
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
struct Request {
    mode: Mode,
    provider: Which,
    prompt: String,
    negative: String,
    style: Style,
    denoise: f32,
    lock_palette: bool,
    maintain_grid: bool,
    remove_bg: bool,
    base_url: String,
    auth: AuthKind,
    basic_user: String,
    checkpoint: String,
    lora: String,
    pixel_nodes: bool,
    workflow: Option<String>,
    family: RdFamily,
    seed: Option<u64>,
}

enum Mode {
    Txt2Img,
    Img2Img,
}

enum Which {
    Comfy,
    Retro,
}

fn parse_generate(p: &Value) -> Result<Request> {
    if !p.is_object() {
        return Err(bad(GENERATE, "parameters must be an object"));
    }
    let mode = match text(p, "mode").as_deref() {
        Some("txt2img") => Mode::Txt2Img,
        Some("img2img") => Mode::Img2Img,
        Some("batch") => return Err(bad(GENERATE, "Batch animation frames are not in this build yet.")),
        _ => return Err(bad(GENERATE, "mode must be txt2img or img2img")),
    };
    let prompt = text(p, "prompt").unwrap_or_default();
    if prompt.chars().count() > 4000 {
        return Err(bad(GENERATE, "the prompt is longer than 4000 characters"));
    }
    if matches!(mode, Mode::Txt2Img) && prompt.trim().is_empty() {
        return Err(bad(GENERATE, "Write a prompt first."));
    }
    let denoise = match p.get("denoise") {
        None | Some(Value::Null) => 0.45,
        Some(v) => {
            let n = v.as_f64().filter(|n| n.is_finite()).ok_or_else(|| bad(GENERATE, "denoise must be a number from 0.10 to 1.00"))?;
            if !(0.099..=1.001).contains(&n) {
                return Err(bad(GENERATE, "denoise must be from 0.10 to 1.00"));
            }
            n as f32
        }
    };
    let workflow = text(p, "workflow").filter(|w| !w.is_empty());
    if let Some(workflow) = &workflow
        && workflow.len() > photocraft_aipixel::workflow::MAX_BYTES
    {
        return Err(bad(GENERATE, "the workflow is larger than 256 KB"));
    }
    let seed = match p.get("seed") {
        None | Some(Value::Null) => None,
        Some(v) => {
            let n = v.as_u64().filter(|n| *n <= u64::from(u32::MAX)).ok_or_else(|| bad(GENERATE, "seed must be an integer from 0 to 4294967295"))?;
            Some(n)
        }
    };
    Ok(Request {
        mode,
        provider: which(p, GENERATE)?,
        prompt,
        negative: text(p, "negative").unwrap_or_default(),
        style: Style::parse(text(p, "style").unwrap_or_default().as_str()),
        denoise,
        lock_palette: flag(p, "lockPalette")?,
        maintain_grid: match p.get("maintainGrid") {
            None | Some(Value::Null) => true,
            Some(_) => flag(p, "maintainGrid")?,
        },
        remove_bg: flag(p, "removeBg")?,
        base_url: text(p, "baseUrl").unwrap_or_default(),
        auth: auth_kind(p)?,
        basic_user: text(p, "basicUser").unwrap_or_default(),
        checkpoint: text(p, "checkpoint").unwrap_or_default(),
        lora: text(p, "lora").unwrap_or_default(),
        pixel_nodes: flag(p, "pixelNodes")?,
        workflow,
        family: RdFamily::parse(text(p, "family").unwrap_or_default().as_str()),
        seed,
    })
}

fn parse_health(p: &Value) -> Result<Request> {
    if !p.is_object() {
        return Err(bad(HEALTH, "parameters must be an object"));
    }
    Ok(Request {
        mode: Mode::Txt2Img,
        provider: which(p, HEALTH)?,
        prompt: String::new(),
        negative: String::new(),
        style: Style::None,
        denoise: 0.45,
        lock_palette: false,
        maintain_grid: true,
        remove_bg: false,
        base_url: text(p, "baseUrl").unwrap_or_default(),
        auth: auth_kind(p)?,
        basic_user: text(p, "basicUser").unwrap_or_default(),
        checkpoint: String::new(),
        lora: String::new(),
        pixel_nodes: false,
        workflow: None,
        family: RdFamily::Pro,
        seed: None,
    })
}

fn which(p: &Value, cmd: &str) -> Result<Which> {
    match text(p, "provider").as_deref() {
        Some("comfy") => Ok(Which::Comfy),
        Some("retro") => Ok(Which::Retro),
        _ => Err(bad(cmd, "provider must be comfy or retro")),
    }
}

fn auth_kind(p: &Value) -> Result<AuthKind> {
    match text(p, "auth").as_deref() {
        None | Some("" | "none") => Ok(AuthKind::None),
        Some("bearer") => Ok(AuthKind::Bearer),
        Some("basic") => Ok(AuthKind::Basic),
        _ => Err(bad(GENERATE, "auth must be none, bearer or basic")),
    }
}

fn text(p: &Value, key: &str) -> Option<String> {
    p.get(key).and_then(Value::as_str).map(str::to_string)
}

fn flag(p: &Value, key: &str) -> Result<bool> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(v) => v.as_bool().ok_or_else(|| bad(GENERATE, format!("`{key}` must be true or false"))),
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct Source {
    rgba: Vec<u8>,
    mask: Option<Vec<u8>>,
    width: u32,
    height: u32,
    origin: (i32, i32),
    anchor: Option<LayerId>,
    doc_id: DocId,
    layer_name: String,
    palette: Vec<[u8; 3]>,
}

#[cfg(not(target_arch = "wasm32"))]
fn capture(s: &Session, request: &Request) -> Result<Source> {
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let bounds = st.doc.bounds();
    let rect = target_rect(&st.doc, bounds)?;
    let (width, height) = rect_size(rect)?;
    let anchor = st.active_layer;
    let layer_name = anchor.and_then(|id| st.doc.layer(id)).map(|l| l.name.clone()).unwrap_or_else(|| "Layer".into());
    let (rgba, mask) = match request.mode {
        Mode::Txt2Img => (Vec::new(), None),
        Mode::Img2Img => {
            let layer = anchor.and_then(|id| st.doc.layer(id)).ok_or_else(|| bad(GENERATE, "no active layer"))?;
            let surface = layer.surface().ok_or_else(|| bad(GENERATE, "the active layer has no pixels to refine"))?;
            (read_rect(surface, rect, width, height)?, selection_mask(&st.doc, rect, width, height)?)
        }
    };
    Ok(Source { rgba, mask, width, height, origin: (rect.x0, rect.y0), anchor, doc_id: st.doc.id, layer_name, palette: palette_u8(&st.doc.palette) })
}

#[cfg(not(target_arch = "wasm32"))]
fn target_rect(doc: &photocraft_doc::Document, bounds: Rect) -> Result<Rect> {
    let rect = doc.selection.as_ref().map(Surface::content_bounds).filter(|r| !r.is_empty()).unwrap_or(bounds).intersect(&bounds);
    if rect.is_empty() {
        return Err(bad(GENERATE, "there's nothing to draw into"));
    }
    Ok(rect)
}

#[cfg(not(target_arch = "wasm32"))]
fn rect_size(rect: Rect) -> Result<(u32, u32)> {
    let width = rect.width();
    let height = rect.height();
    if width == 0 || height == 0 || width > 1024 || height > 1024 {
        return Err(bad(GENERATE, format!("{width}×{height} is outside 1×1 to 1024×1024")));
    }
    Ok((width, height))
}

#[cfg(not(target_arch = "wasm32"))]
fn read_rect(surface: &Surface, rect: Rect, width: u32, height: u32) -> Result<Vec<u8>> {
    let count = (width as usize).checked_mul(height as usize).ok_or_else(|| bad(GENERATE, "that area is too large"))?;
    let mut pixels = vec![[0u8; 4]; count];
    surface.read_rgba8_into(rect, &mut pixels);
    Ok(pixels.into_iter().flatten().collect())
}

#[cfg(not(target_arch = "wasm32"))]
fn selection_mask(doc: &photocraft_doc::Document, rect: Rect, width: u32, height: u32) -> Result<Option<Vec<u8>>> {
    let Some(selection) = doc.selection.as_ref() else { return Ok(None) };
    if selection.content_bounds().is_empty() {
        return Ok(None);
    }
    let count = (width as usize).saturating_mul(height as usize);
    let mut cover = vec![[0u8; 4]; count];
    selection.read_rgba8_into(rect, &mut cover);
    let mut mask = vec![255u8; count.saturating_mul(4)];
    for (i, px) in cover.iter().enumerate() {
        let sum = u16::from(px[0]) + u16::from(px[1]) + u16::from(px[2]);
        let selected = px[0] >= 128 || (px[3] >= 128 && sum > 0);
        let alpha = if selected { 0 } else { 255 };
        let o = i.saturating_mul(4);
        if let Some(slot) = mask.get_mut(o..o + 4) {
            slot.copy_from_slice(&[255, 255, 255, alpha]);
        }
    }
    Ok(Some(mask))
}

#[cfg(not(target_arch = "wasm32"))]
fn palette_u8(palette: &[[f32; 3]]) -> Vec<[u8; 3]> {
    palette.iter().take(256).map(|rgb| rgb.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)).collect()
}

#[cfg(not(target_arch = "wasm32"))]
struct Made {
    image: RgbaImage,
    source: Source,
}

#[cfg(not(target_arch = "wasm32"))]
fn produce(request: Request, source: Source, ctx: &JobCtx) -> Result<Made> {
    let control = CtxControl(ctx.clone());
    if control.cancelled() {
        return Err(EngineError::Cancelled);
    }
    let seed = match request.seed {
        Some(seed) => seed,
        None => photocraft_aipixel::id::random_seed().map_err(map_err)?,
    };
    let workflow = request.workflow.as_deref();
    let options = GenerateOptions {
        seed,
        denoise: request.denoise,
        style: request.style,
        checkpoint: &request.checkpoint,
        lora: &request.lora,
        pixel_nodes: request.pixel_nodes,
        workflow,
        family: request.family,
        remove_bg: request.remove_bg,
        lock_palette: request.lock_palette,
        negative: &request.negative,
        control: &control,
    };
    let provider = provider(&request)?;
    let image = match request.mode {
        Mode::Txt2Img => provider.generate_txt2img(&request.prompt, &request.negative, source.width, source.height, &source.palette, &options),
        Mode::Img2Img => {
            let bitmap = RgbaImage::new(source.width, source.height, source.rgba.clone()).map_err(|m| bad(GENERATE, m))?;
            let mask = match &source.mask {
                Some(bytes) => Some(RgbaImage::new(source.width, source.height, bytes.clone()).map_err(|m| bad(GENERATE, m))?),
                None => None,
            };
            provider.generate_img2img(&bitmap, mask.as_ref(), &request.prompt, request.denoise, &source.palette, &options)
        }
    }
    .map_err(map_err)?;
    let image = finish_pixels(&request, &source, image)?;
    Ok(Made { image, source })
}

#[cfg(not(target_arch = "wasm32"))]
fn finish_pixels(request: &Request, source: &Source, mut image: RgbaImage) -> Result<RgbaImage> {
    if request.remove_bg {
        image = ai_pixel::knock_out_corners(&image).map_err(|m| bad(GENERATE, m))?;
    }
    if image.width != source.width || image.height != source.height {
        image = if request.maintain_grid {
            ai_pixel::downscale_grid(&image, source.width, source.height)
        } else {
            ai_pixel::resize_nearest(&image, source.width, source.height)
        }
        .map_err(|m| bad(GENERATE, m))?;
    }
    if request.lock_palette {
        if source.palette.is_empty() {
            return Err(bad(GENERATE, "Lock to Active Palette needs colours in the document palette."));
        }
        image = ai_pixel::quantize_palette(&image, &source.palette, PaletteSpace::Lab).map_err(|m| bad(GENERATE, m))?;
    }
    Ok(image)
}

#[cfg(not(target_arch = "wasm32"))]
fn provider(request: &Request) -> Result<Box<dyn AIPixelProvider>> {
    match request.provider {
        Which::Comfy => {
            let provider = ComfyProvider::new(request.base_url.clone(), request.auth, request.basic_user.clone()).map_err(map_err)?;
            Ok(Box::new(provider))
        }
        Which::Retro => Ok(Box::new(RetroProvider::new(&request.base_url).map_err(map_err)?)),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn layer_name(request: &Request, source: &Source) -> String {
    let raw = match request.mode {
        Mode::Img2Img => format!("AI Refine - {}", source.layer_name),
        Mode::Txt2Img => format!("AI - {}", snippet(&request.prompt)),
    };
    raw.chars().take(80).collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn snippet(prompt: &str) -> String {
    let flat: String = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    let cut: String = flat.chars().take(24).collect();
    if cut.is_empty() { "prompt".into() } else { cut }
}

#[cfg(not(target_arch = "wasm32"))]
fn insert_layer(s: &mut Session, label: &str, made: Made) -> Result<Value> {
    let Made { image, source } = made;
    let current = s.active().map(|st| st.doc.id);
    if current != Some(source.doc_id) {
        return Err(EngineError::Other("Switch back to the document AI Pixel Studio was drawing into.".into()));
    }
    let id = s.edit(label, |doc, active| {
        let mut layer = Layer::raster(label, doc.pixel_format());
        let rect = Rect::from_xywh(source.origin.0, source.origin.1, image.width, image.height);
        {
            let surface = layer.surface_mut().ok_or_else(|| EngineError::Other("a new layer has no pixels".into()))?;
            if surface.format().alpha {
                let clear = from_rgba(&surface.format(), [0.0, 0.0, 0.0, 0.0]);
                surface.fill_rect(doc.bounds(), &clear);
            }
            write_image(surface, rect, &image)?;
        }
        let id = crate::layer_place::insert_new(doc, source.anchor, layer, crate::layer_place::Place::Above);
        *active = Some(id);
        Ok(id)
    })?;
    Ok(json!({ "message": format!("Added layer {label}"), "layer": id.0 }))
}

#[cfg(not(target_arch = "wasm32"))]
fn write_image(surface: &mut Surface, rect: Rect, image: &RgbaImage) -> Result<()> {
    let width = rect.width();
    let height = rect.height();
    if width != image.width || height != image.height {
        return Err(EngineError::Other("the result doesn't match the layer area".into()));
    }
    let channels = surface.channels();
    if channels == 0 || channels > 8 {
        return Err(EngineError::Other("this document's pixel format isn't supported".into()));
    }
    let count = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(channels))
        .ok_or_else(|| EngineError::Other("the result is too large".into()))?;
    let mut data = vec![0f32; count];
    let mut tmp = [0f32; 8];
    for y in 0..height {
        for x in 0..width {
            let px = image.pixel(x, y).unwrap_or([0, 0, 0, 0]);
            let rgba = [f32::from(px[0]) / 255.0, f32::from(px[1]) / 255.0, f32::from(px[2]) / 255.0, f32::from(px[3]) / 255.0];
            let written = from_rgba_into(&surface.format(), rgba, &mut tmp);
            let i = ((y as usize) * (width as usize) + (x as usize)) * channels;
            if let Some(slot) = data.get_mut(i..i + written) {
                slot.copy_from_slice(&tmp[..written]);
            }
        }
    }
    surface.write_region(rect, &data);
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn map_err(err: AiError) -> EngineError {
    match err.kind {
        AiErrorKind::Cancelled => EngineError::Cancelled,
        AiErrorKind::BadParams | AiErrorKind::Workflow => bad(GENERATE, err.message),
        _ => EngineError::Other(err.message),
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct CtxControl(JobCtx);

#[cfg(not(target_arch = "wasm32"))]
impl Control for CtxControl {
    fn cancelled(&self) -> bool {
        self.0.cancelled()
    }

    fn progress(&self, fraction: f32, message: &str) {
        self.0.progress(fraction, message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use photocraft_aipixel::secrets::{self, RETRO_TOKEN};
    use photocraft_aipixel::transport::{self, HttpResponse, MockTransport};
    use serde_json::json;
    use std::sync::{Mutex, OnceLock, PoisonError};

    fn gate() -> std::sync::MutexGuard<'static, ()> {
        static GATE: OnceLock<Mutex<()>> = OnceLock::new();
        GATE.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn png(color: [u8; 4]) -> Vec<u8> {
        let image = RgbaImage::new(1, 1, color.to_vec()).expect("image");
        photocraft_aipixel::pngutil::encode_png(&image).expect("png")
    }

    fn comfy_script(color: [u8; 4]) -> std::sync::Arc<MockTransport> {
        let ws = vec![
            r#"{"type":"progress","data":{"value":1,"max":1,"prompt_id":"p1"}}"#.into(),
            r#"{"type":"executed","data":{"prompt_id":"p1","output":{"images":[{"filename":"out.png","subfolder":"","type":"output"}]}}}"#.into(),
            r#"{"type":"executing","data":{"node":null,"prompt_id":"p1"}}"#.into(),
        ];
        std::sync::Arc::new(MockTransport::new(
            vec![Ok(HttpResponse { status: 200, body: br#"{"prompt_id":"p1"}"#.to_vec() }), Ok(HttpResponse { status: 200, body: png(color) })],
            ws,
        ))
    }

    fn doc(depth: u32) -> Session {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 16, "height": 16, "depth": depth, "name": "Sprite"})).unwrap();
        s
    }

    fn paint_red(s: &mut Session) {
        s.edit("paint", |doc, active| {
            let bounds = doc.bounds();
            let surface = doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap();
            let px = from_rgba(&surface.format(), [1.0, 0.0, 0.0, 1.0]);
            surface.fill_rect(bounds, &px);
            Ok(())
        })
        .unwrap();
    }

    fn pixel(s: &Session, index_from_bottom: usize) -> [u8; 4] {
        let layer = &s.active().unwrap().doc.layers[index_from_bottom];
        let mut out = [[0u8; 4]; 1];
        layer.surface().unwrap().read_rgba8_into(Rect::from_xywh(0, 0, 1, 1), &mut out);
        out[0]
    }

    #[test]
    fn bad_params_fail_without_a_network_call() {
        let _gate = gate();
        transport::install(Some(std::sync::Arc::new(MockTransport::new(Vec::new(), Vec::new()))));
        let mut s = doc(8);
        assert!(s.execute(GENERATE, json!({})).is_err());
        assert!(s.execute(GENERATE, json!({"mode": ""})).is_err());
        let batch = s.execute(GENERATE, json!({"mode": "batch", "provider": "comfy"})).unwrap_err();
        assert!(batch.to_string().contains("not in this build"), "{batch}");
        assert!(s.execute(GENERATE, json!({"mode": "txt2img", "provider": "comfy", "prompt": "a", "denoise": 0})).is_err());
        assert!(s.execute(GENERATE, json!({"mode": "txt2img", "provider": "comfy", "prompt": "a", "baseUrl": "http://8.8.8.8:8188"})).is_err());
        assert!(s.execute(HEALTH, json!({})).is_err());
        transport::install(None);
    }

    #[test]
    fn a_result_is_a_new_layer_and_undo_removes_only_that() {
        let _gate = gate();
        transport::install(Some(comfy_script([0, 0, 255, 255])));
        let mut s = doc(8);
        paint_red(&mut s);
        let before = s.active().unwrap().doc.layers.len();
        let original = s.active().unwrap().active_layer;
        let result = s
            .execute(
                GENERATE,
                json!({
                    "mode": "txt2img", "provider": "comfy", "prompt": "a blue sword for the knight",
                    "baseUrl": "http://192.168.1.5:8188", "checkpoint": "pixel.safetensors", "denoise": 1.0
                }),
            )
            .expect("generate");
        assert!(result["message"].as_str().unwrap_or("").starts_with("Added layer"));
        let doc = &s.active().unwrap().doc;
        assert_eq!(doc.layers.len(), before + 1);
        let added = doc.layers.last().unwrap();
        assert!(added.name.starts_with("AI - a blue sword"));
        assert_ne!(Some(added.id), original);
        assert_eq!(pixel(&s, 0), [255, 0, 0, 255], "the original layer is untouched");
        assert_eq!(pixel(&s, doc.layers.len() - 1)[2], 255);
        assert!(s.undo());
        assert_eq!(s.active().unwrap().doc.layers.len(), before);
        assert_eq!(pixel(&s, 0), [255, 0, 0, 255]);
        transport::install(None);
    }

    #[test]
    fn img2img_locks_to_the_palette_at_16_bit_too() {
        let _gate = gate();
        secrets::set(RETRO_TOKEN, "test-token").expect("secret");
        let image = RgbaImage::new(1, 1, vec![0, 0, 255, 200]).expect("image");
        let encoded = base64::engine::general_purpose::STANDARD.encode(photocraft_aipixel::pngutil::encode_png(&image).expect("png"));
        let body = format!(r#"{{"base64_images":["{encoded}"]}}"#);
        transport::install(Some(std::sync::Arc::new(MockTransport::new(vec![Ok(HttpResponse { status: 200, body: body.into_bytes() })], Vec::new()))));
        let mut s = doc(16);
        s.edit("palette", |doc, _| {
            doc.palette = vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
            Ok(())
        })
        .unwrap();
        paint_red(&mut s);
        s.execute(
            GENERATE,
            json!({
                "mode": "img2img", "provider": "retro", "prompt": "shade it",
                "lockPalette": true, "maintainGrid": true, "denoise": 0.4
            }),
        )
        .expect("img2img");
        let doc = &s.active().unwrap().doc;
        assert!(doc.layers.last().unwrap().name.starts_with("AI Refine - "));
        let px = pixel(&s, doc.layers.len() - 1);
        assert!([[255, 0, 0], [0, 255, 0], [0, 0, 255]].contains(&[px[0], px[1], px[2]]), "{px:?}");
        assert_eq!(pixel(&s, 0)[0], 255, "original red stays");
        transport::install(None);
        secrets::clear(RETRO_TOKEN).expect("clear");
    }

    #[test]
    fn a_refused_retro_key_is_an_error_and_not_in_the_message() {
        let _gate = gate();
        secrets::set(RETRO_TOKEN, "test-token").expect("secret");
        transport::install(Some(std::sync::Arc::new(MockTransport::new(vec![Ok(HttpResponse { status: 401, body: b"test-token".to_vec() })], Vec::new()))));
        let mut s = doc(8);
        let err = s.execute(HEALTH, json!({"provider": "retro"})).unwrap_err();
        assert!(err.to_string().to_ascii_lowercase().contains("key") || err.to_string().contains("refused"), "{err}");
        assert!(!err.to_string().contains("test-token"));
        transport::install(None);
        secrets::clear(RETRO_TOKEN).expect("clear");
    }
}
