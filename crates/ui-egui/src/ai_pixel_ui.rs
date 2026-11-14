// AI Pixel Studio for the Layerwise Android port of PhotoCraft.
// Licensed under the Apache License, Version 2.0 (LICENSE-APACHE),
// also offered under the MIT License (LICENSE-MIT) like the rest of this tree.

//! The AI Pixel Studio drawer. It sends `ai.pixel.generate` and `ai.pixel.health`. Keys are written
//! through [`photocraft_aipixel::secrets`] and are never put in a preference, a command or a label.

use egui::{Id, Rect, RichText, Sense, Stroke, pos2, vec2};
use photocraft_aipixel::secrets::{self, COMFY_BASIC, COMFY_BEARER, RETRO_TOKEN};
use photocraft_aipixel::workflow;
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::theme::Tokens;

const PREF: &str = "ai.pixel";
const STATE: &str = "ai-pixel-studio";

#[derive(Clone)]
struct Studio {
    open: bool,
    settings: bool,
    mode: String,
    provider: String,
    prompt: String,
    negative: String,
    style: String,
    denoise: f32,
    lock_palette: bool,
    maintain_grid: bool,
    remove_bg: bool,
    base_url: String,
    auth: String,
    basic_user: String,
    checkpoint: String,
    lora: String,
    pixel_nodes: bool,
    workflow: String,
    workflow_nonce: u64,
    family: String,
    bearer_entry: String,
    basic_entry: String,
    retro_entry: String,
    loaded: bool,
}

impl Default for Studio {
    fn default() -> Self {
        Self {
            open: false,
            settings: false,
            mode: "txt2img".into(),
            provider: "comfy".into(),
            prompt: String::new(),
            negative: String::new(),
            style: String::new(),
            denoise: 0.45,
            lock_palette: false,
            maintain_grid: true,
            remove_bg: false,
            base_url: String::new(),
            auth: "none".into(),
            basic_user: String::new(),
            checkpoint: String::new(),
            lora: String::new(),
            pixel_nodes: false,
            workflow: String::new(),
            workflow_nonce: 0,
            family: "pro".into(),
            bearer_entry: String::new(),
            basic_entry: String::new(),
            retro_entry: String::new(),
            loaded: false,
        }
    }
}

pub(crate) fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<Studio>(Id::unique(STATE))).is_some_and(|s| s.open)
}

pub(crate) fn toggle(ctx: &egui::Context) {
    ctx.data_mut(|data| {
        let mut studio = data.get_temp::<Studio>(Id::unique(STATE)).unwrap_or_default();
        studio.open = !studio.open;
        data.insert_temp(Id::unique(STATE), studio);
    });
}

pub(crate) fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let mut studio = ctx.data(|data| data.get_temp::<Studio>(Id::unique(STATE))).unwrap_or_default();
    if !studio.loaded {
        load_prefs(app, &mut studio);
        studio.loaded = true;
    }
    pull_workflow(app, &mut studio);
    if !studio.open {
        ctx.data_mut(|data| data.insert_temp(Id::unique(STATE), studio));
        return;
    }
    let hole = ctx.data(|data| data.get_temp::<Rect>(Id::unique("studio-chrome-hole"))).unwrap_or_else(|| ctx.content_rect());
    let t = Tokens::get(ctx);
    let width = hole.width().clamp(0.0, 420.0).min(hole.width());
    let height = (hole.height() * 0.78).clamp(160.0, hole.height());
    let origin = pos2(hole.right() - width, hole.bottom() - height);
    let mut close = false;
    egui::Area::new(Id::unique("ai-pixel-drawer")).order(egui::Order::Foreground).fixed_pos(origin).show(ctx, |ui| {
        egui::Frame::NONE.fill(t.dock).stroke(Stroke::new(1.0, t.card_border)).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
            ui.set_width((width - 16.0).max(0.0));
            ui.set_max_height((height - 16.0).max(0.0));
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            header(ui, &t, &mut close);
            egui::ScrollArea::vertical().id_salt("ai-pixel-scroll").auto_shrink([false, false]).show(ui, |ui| {
                body(app, ui, &t, &mut studio);
            });
        });
    });
    if close {
        studio.open = false;
        persist(app, &studio);
    }
    ctx.data_mut(|data| data.insert_temp(Id::unique(STATE), studio));
}

fn header(ui: &mut egui::Ui, t: &Tokens, close: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(tl!("AI Pixel Studio")).color(t.text).size(16.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(egui::Button::new(tl!("Close")).min_size(vec2(64.0, 40.0))).clicked() {
                *close = true;
            }
        });
    });
}

fn body(app: &mut PhotocraftApp, ui: &mut egui::Ui, t: &Tokens, studio: &mut Studio) {
    mode_row(ui, t, studio);
    provider_row(ui, t, studio);
    ui.add(egui::TextEdit::multiline(&mut studio.prompt).desired_rows(3).desired_width(ui.available_width()).hint_text(tl!("Prompt")));
    ui.add(egui::TextEdit::multiline(&mut studio.negative).desired_rows(2).desired_width(ui.available_width()).hint_text(tl!("Negative prompt")));
    if studio.provider == "retro" {
        ui.label(RichText::new(tl!("Current Retro Diffusion models ignore the negative prompt.")).color(t.text_dim).size(12.0));
    }
    ui.label(RichText::new(tl!("Style")).color(t.text_dim).size(12.0));
    style_row(ui, t, studio);
    ui.add(egui::Slider::new(&mut studio.denoise, 0.10..=1.0).text(tl!("Denoise")));
    ui.label(RichText::new(tl!("Refine works best around 0.30–0.75.")).color(t.text_dim).size(12.0));
    ui.checkbox(&mut studio.lock_palette, tl!("Lock to Active Palette"));
    ui.checkbox(&mut studio.maintain_grid, tl!("Maintain 1x1 Pixel Grid"));
    ui.checkbox(&mut studio.remove_bg, tl!("Remove Background"));
    progress(app, ui, t);
    if app.ui.status_error && !app.ui.status.is_empty() {
        ui.label(RichText::new(&app.ui.status).color(t.danger).size(13.0));
    }
    ui.horizontal_wrapped(|ui| {
        let busy = running(app);
        if ui.add_enabled(!busy, egui::Button::new(tl!("Generate")).min_size(vec2(120.0, 44.0))).clicked() {
            persist(app, studio);
            let params = params_of(studio);
            if let Err(err) = app.run("ai.pixel.generate", params) {
                app.ui.status = err;
                app.ui.status_error = true;
            }
        }
        if ui.add(egui::Button::new(tl!("Settings")).min_size(vec2(96.0, 44.0))).clicked() {
            studio.settings = !studio.settings;
        }
    });
    if studio.settings {
        settings(app, ui, t, studio);
    }
}

fn mode_row(ui: &mut egui::Ui, t: &Tokens, studio: &mut Studio) {
    ui.horizontal_wrapped(|ui| {
        if chip(ui, t, tl!("New layer"), studio.mode == "txt2img") {
            studio.mode = "txt2img".into();
        }
        if chip(ui, t, tl!("Refine"), studio.mode == "img2img") {
            studio.mode = "img2img".into();
        }
        let _ = ui.add_enabled(false, egui::Button::new(tl!("Batch")).min_size(vec2(72.0, 40.0)));
        ui.label(RichText::new(tl!("Not in this build yet")).color(t.text_dim).size(12.0));
    });
}

fn provider_row(ui: &mut egui::Ui, t: &Tokens, studio: &mut Studio) {
    ui.horizontal_wrapped(|ui| {
        if chip(ui, t, tl!("ComfyUI"), studio.provider == "comfy") {
            studio.provider = "comfy".into();
        }
        if chip(ui, t, tl!("Retro Diffusion"), studio.provider == "retro") {
            studio.provider = "retro".into();
        }
    });
}

fn style_row(ui: &mut egui::Ui, t: &Tokens, studio: &mut Studio) {
    let styles = [
        ("gba", tl!("16-Bit GBA")),
        ("ds", tl!("DS Portrait")),
        ("topdown", tl!("Top-Down RPG Tile")),
        ("iso", tl!("Isometric")),
        ("gb", tl!("GameBoy 4-Color")),
    ];
    ui.horizontal_wrapped(|ui| {
        for (id, label) in styles {
            if chip(ui, t, label, studio.style == id) {
                studio.style = if studio.style == id { String::new() } else { id.into() };
            }
        }
    });
}

fn chip(ui: &mut egui::Ui, t: &Tokens, label: &str, on: bool) -> bool {
    let fill = if on { t.accent_soft } else { t.chrome };
    let color = if on { t.accent_text } else { t.text };
    let width = ui.available_width().clamp(72.0, 168.0);
    ui.add(egui::Button::new(RichText::new(label).color(color).size(13.0)).fill(fill).min_size(vec2(width, 40.0))).clicked()
}

fn progress(app: &mut PhotocraftApp, ui: &mut egui::Ui, t: &Tokens) {
    let Some(job) = app.session.jobs().into_iter().find(|job| job.command.starts_with("ai.pixel.") && job.state == "running") else { return };
    let fraction = job.progress.clamp(0.0, 1.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 10.0), Sense::hover());
    ui.painter().rect_filled(rect, 4.0, t.chrome);
    let mut done = rect;
    done.max.x = rect.min.x + rect.width() * fraction;
    ui.painter().rect_filled(done, 4.0, t.accent);
    let message = if job.message.is_empty() { job.label.clone() } else { job.message.clone() };
    ui.label(RichText::new(message).color(t.text_dim).size(12.0));
    if ui.add(egui::Button::new(tl!("Cancel")).min_size(vec2(96.0, 40.0))).clicked() {
        app.session.cancel_job(job.id);
    }
}

fn settings(app: &mut PhotocraftApp, ui: &mut egui::Ui, t: &Tokens, studio: &mut Studio) {
    ui.separator();
    if studio.provider == "comfy" {
        comfy_settings(app, ui, t, studio);
    } else {
        retro_settings(app, ui, t, studio);
    }
}

fn comfy_settings(app: &mut PhotocraftApp, ui: &mut egui::Ui, t: &Tokens, studio: &mut Studio) {
    ui.label(RichText::new(tl!("ComfyUI address")).color(t.text_dim).size(12.0));
    ui.add(egui::TextEdit::singleline(&mut studio.base_url).desired_width(ui.available_width()).hint_text("http://192.168.x.x:8188"));
    ui.label(RichText::new(tl!("Cleartext HTTP is only used for a LAN or Tailscale address.")).color(t.text_dim).size(12.0));
    auth_row(ui, t, studio);
    if studio.auth == "basic" {
        ui.add(egui::TextEdit::singleline(&mut studio.basic_user).desired_width(ui.available_width()).hint_text(tl!("Username")));
        note_secret(app, secret_row(ui, t, tl!("Password"), &mut studio.basic_entry, COMFY_BASIC));
    }
    if studio.auth == "bearer" {
        note_secret(app, secret_row(ui, t, tl!("Bearer token"), &mut studio.bearer_entry, COMFY_BEARER));
    }
    ui.add(egui::TextEdit::singleline(&mut studio.checkpoint).desired_width(ui.available_width()).hint_text(tl!("Checkpoint name")));
    ui.add(egui::TextEdit::singleline(&mut studio.lora).desired_width(ui.available_width()).hint_text(tl!("LoRA name (optional)")));
    ui.checkbox(&mut studio.pixel_nodes, tl!("PixelOE or K-centroid node when the server has one"));
    workflow_row(app, ui, t, studio);
    if ui.add(egui::Button::new(tl!("Test connection")).min_size(vec2(140.0, 40.0))).clicked() {
        persist(app, studio);
        if let Err(err) = app.run("ai.pixel.health", health_params(studio)) {
            app.ui.status = err;
            app.ui.status_error = true;
        }
    }
}

fn retro_settings(app: &mut PhotocraftApp, ui: &mut egui::Ui, t: &Tokens, studio: &mut Studio) {
    note_secret(app, secret_row(ui, t, tl!("API key"), &mut studio.retro_entry, RETRO_TOKEN));
    ui.label(RichText::new(tl!("The key stays in private storage and is never shown again.")).color(t.text_dim).size(12.0));
    let families = [("pro", "RD Pro"), ("fast", "RD Fast"), ("plus", "RD Plus"), ("core", "RD Core")];
    ui.horizontal_wrapped(|ui| {
        for (id, label) in families {
            if chip(ui, t, label, studio.family == id) {
                studio.family = id.into();
            }
        }
    });
    ui.label(RichText::new(tl!("Tileset and animation styles aren't in this build.")).color(t.text_dim).size(12.0));
    if ui.add(egui::Button::new(tl!("Check credits")).min_size(vec2(140.0, 40.0))).clicked() {
        persist(app, studio);
        if let Err(err) = app.run("ai.pixel.health", health_params(studio)) {
            app.ui.status = err;
            app.ui.status_error = true;
        }
    }
}

fn auth_row(ui: &mut egui::Ui, t: &Tokens, studio: &mut Studio) {
    ui.horizontal_wrapped(|ui| {
        for (id, label) in [("none", tl!("No auth")), ("bearer", tl!("Bearer")), ("basic", tl!("Basic"))] {
            if chip(ui, t, label, studio.auth == id) {
                studio.auth = id.into();
            }
        }
    });
}

fn note_secret(app: &mut PhotocraftApp, error: Option<String>) {
    if let Some(error) = error {
        app.ui.status = error;
        app.ui.status_error = true;
    }
}

fn secret_row(ui: &mut egui::Ui, t: &Tokens, label: &str, entry: &mut String, name: &'static str) -> Option<String> {
    ui.label(RichText::new(label).color(t.text_dim).size(12.0));
    if secrets::is_set(name) {
        ui.label(RichText::new(tl!("A key is saved.")).color(t.text).size(13.0));
    }
    ui.add(egui::TextEdit::singleline(entry).password(true).desired_width(ui.available_width()).hint_text(tl!("Paste a new key")));
    ui.horizontal_wrapped(|ui| {
        let mut error = None;
        if ui.add(egui::Button::new(tl!("Save key")).min_size(vec2(96.0, 40.0))).clicked() {
            match secrets::set(name, entry) {
                Ok(()) => entry.clear(),
                Err(err) => error = Some(err),
            }
        }
        if ui.add(egui::Button::new(tl!("Clear")).min_size(vec2(72.0, 40.0))).clicked() {
            if let Err(err) = secrets::clear(name) {
                error = Some(err);
            } else {
                entry.clear();
            }
        }
        error
    })
    .inner
}

fn workflow_row(app: &mut PhotocraftApp, ui: &mut egui::Ui, t: &Tokens, studio: &mut Studio) {
    ui.label(RichText::new(tl!("Custom workflow (optional)")).color(t.text_dim).size(12.0));
    ui.label(RichText::new(tl!("Use @prompt, @seed, @image, @denoise and @negative.")).color(t.text_dim).size(12.0));
    if ui.add(egui::Button::new(tl!("Import workflow")).min_size(vec2(150.0, 40.0))).clicked() {
        let mode = studio.mode.clone();
        match app.ask_file(
            crate::file_dialog::FileDialogRequest::Open { multiple: false, initial_dir: None, extensions: Some(vec!["json".into()]) },
            move |app, answer| import_answer(app, &mode, answer),
        ) {
            Ok(_) => {}
            Err(err) if err == "cancelled" => {
                app.ui.status = tl!("Paste the workflow JSON below, then tap Use workflow.").into();
                app.ui.status_error = false;
            }
            Err(err) => {
                app.ui.status = err;
                app.ui.status_error = true;
            }
        }
    }
    ui.add(egui::TextEdit::multiline(&mut studio.workflow).desired_rows(4).desired_width(ui.available_width()).hint_text(tl!("Or paste workflow JSON")));
    if ui.add(egui::Button::new(tl!("Use workflow")).min_size(vec2(130.0, 40.0))).clicked() {
        match accept_workflow(&studio.mode, &studio.workflow) {
            Ok(()) => {
                persist(app, studio);
                app.ui.status = tl!("Custom workflow ready.").into();
                app.ui.status_error = false;
            }
            Err(err) => {
                app.ui.status = err;
                app.ui.status_error = true;
            }
        }
    }
    if ui.add(egui::Button::new(tl!("Use bundled workflow")).min_size(vec2(160.0, 40.0))).clicked() {
        studio.workflow.clear();
        persist(app, studio);
    }
}

fn import_answer(app: &mut PhotocraftApp, mode: &str, answer: crate::file_dialog::FileDialogAnswer) -> Result<Value, String> {
    let text = workflow_text(answer)?;
    accept_workflow(mode, &text)?;
    let nonce = app.session.prefs().dialogs.get(PREF).and_then(|v| v.get("workflowNonce")).and_then(Value::as_u64).unwrap_or(0).saturating_add(1);
    app.session.edit_prefs(|prefs| {
        let slot = prefs.dialogs.entry(PREF.into()).or_insert_with(|| json!({}));
        if let Some(slot) = slot.as_object_mut() {
            slot.insert("workflow".into(), json!(text));
            slot.insert("workflowNonce".into(), json!(nonce));
        }
    });
    let _ = crate::prefs_ui::save_preferences(app);
    app.ui.status = tl!("Custom workflow ready.").into();
    app.ui.status_error = false;
    Ok(Value::Null)
}

fn workflow_text(answer: crate::file_dialog::FileDialogAnswer) -> Result<String, String> {
    let bytes = match answer {
        crate::file_dialog::FileDialogAnswer::Contents(_, bytes) => bytes,
        crate::file_dialog::FileDialogAnswer::Paths(paths) => read_path(paths.first().map(String::as_str))?,
        _ => return Err(tl!("That wasn't a workflow file.").into()),
    };
    if bytes.len() > workflow::MAX_BYTES {
        return Err(tl!("The workflow is larger than 256 KB.").into());
    }
    String::from_utf8(bytes).map_err(|_| tl!("The workflow file isn't UTF-8 text.").into())
}

#[cfg(not(target_arch = "wasm32"))]
fn read_path(path: Option<&str>) -> Result<Vec<u8>, String> {
    let path = path.ok_or_else(|| tl!("No file was chosen.").to_string())?;
    std::fs::read(path).map_err(|err| format!("{} ({err})", tl!("Couldn't read the workflow")))
}

#[cfg(target_arch = "wasm32")]
fn read_path(_path: Option<&str>) -> Result<Vec<u8>, String> {
    Err(tl!("Paste the workflow JSON in this build.").into())
}

fn accept_workflow(mode: &str, text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err(tl!("The workflow is empty.").into());
    }
    let required: &[&str] = if mode == "img2img" { &["@prompt", "@seed", "@image", "@denoise"] } else { &["@prompt", "@seed"] };
    workflow::require_tags(text, required).map_err(|err| err.to_string())?;
    serde_json::from_str::<Value>(text).map_err(|err| format!("{} ({err})", tl!("This workflow isn't valid JSON")))?;
    Ok(())
}

fn running(app: &PhotocraftApp) -> bool {
    app.session.jobs().iter().any(|job| job.command.starts_with("ai.pixel.") && job.state == "running")
}

fn params_of(studio: &Studio) -> Value {
    json!({
        "mode": studio.mode,
        "provider": studio.provider,
        "prompt": studio.prompt,
        "negative": studio.negative,
        "style": studio.style,
        "denoise": studio.denoise,
        "lockPalette": studio.lock_palette,
        "maintainGrid": studio.maintain_grid,
        "removeBg": studio.remove_bg,
        "baseUrl": studio.base_url,
        "auth": studio.auth,
        "basicUser": studio.basic_user,
        "checkpoint": studio.checkpoint,
        "lora": studio.lora,
        "pixelNodes": studio.pixel_nodes,
        "workflow": if studio.workflow.trim().is_empty() { Value::Null } else { json!(studio.workflow) },
        "family": studio.family,
    })
}

fn health_params(studio: &Studio) -> Value {
    json!({
        "provider": studio.provider,
        "baseUrl": studio.base_url,
        "auth": studio.auth,
        "basicUser": studio.basic_user,
    })
}

fn persist(app: &mut PhotocraftApp, studio: &Studio) {
    let workflow = studio.workflow.clone();
    let nonce = studio.workflow_nonce;
    app.session.edit_prefs(|prefs| {
        prefs.dialogs.insert(
            PREF.into(),
            json!({
                "mode": studio.mode,
                "provider": studio.provider,
                "prompt": studio.prompt,
                "negative": studio.negative,
                "style": studio.style,
                "denoise": studio.denoise,
                "lockPalette": studio.lock_palette,
                "maintainGrid": studio.maintain_grid,
                "removeBg": studio.remove_bg,
                "baseUrl": studio.base_url,
                "auth": studio.auth,
                "basicUser": studio.basic_user,
                "checkpoint": studio.checkpoint,
                "lora": studio.lora,
                "pixelNodes": studio.pixel_nodes,
                "workflow": workflow,
                "workflowNonce": nonce,
                "family": studio.family,
            }),
        );
    });
    let _ = crate::prefs_ui::save_preferences(app);
}

fn load_prefs(app: &PhotocraftApp, studio: &mut Studio) {
    let Some(value) = app.session.prefs().dialogs.get(PREF) else { return };
    let str_field = |key: &str, dest: &mut String| {
        if let Some(text) = value.get(key).and_then(Value::as_str) {
            *dest = text.to_string();
        }
    };
    str_field("mode", &mut studio.mode);
    str_field("provider", &mut studio.provider);
    str_field("prompt", &mut studio.prompt);
    str_field("negative", &mut studio.negative);
    str_field("style", &mut studio.style);
    str_field("baseUrl", &mut studio.base_url);
    str_field("auth", &mut studio.auth);
    str_field("basicUser", &mut studio.basic_user);
    str_field("checkpoint", &mut studio.checkpoint);
    str_field("lora", &mut studio.lora);
    str_field("workflow", &mut studio.workflow);
    str_field("family", &mut studio.family);
    if let Some(denoise) = value.get("denoise").and_then(Value::as_f64)
        && (0.10..=1.0).contains(&denoise)
    {
        studio.denoise = denoise as f32;
    }
    let flag = |key: &str, dest: &mut bool| {
        if let Some(value) = value.get(key).and_then(Value::as_bool) {
            *dest = value;
        }
    };
    flag("lockPalette", &mut studio.lock_palette);
    flag("maintainGrid", &mut studio.maintain_grid);
    flag("removeBg", &mut studio.remove_bg);
    flag("pixelNodes", &mut studio.pixel_nodes);
    if let Some(nonce) = value.get("workflowNonce").and_then(Value::as_u64) {
        studio.workflow_nonce = nonce;
    }
}

fn pull_workflow(app: &PhotocraftApp, studio: &mut Studio) {
    let Some(value) = app.session.prefs().dialogs.get(PREF) else { return };
    let Some(nonce) = value.get("workflowNonce").and_then(Value::as_u64) else { return };
    if nonce != studio.workflow_nonce {
        studio.workflow = value.get("workflow").and_then(Value::as_str).unwrap_or("").to_string();
        studio.workflow_nonce = nonce;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_tag_is_a_validation_error() {
        let err = accept_workflow("img2img", r#"{"3":{"inputs":{"text":"@prompt"}}}"#).expect_err("tags");
        assert!(err.contains("@seed") && err.contains("@image") && err.contains("@denoise"), "{err}");
    }

    #[test]
    fn a_narrow_phone_opens_the_drawer_without_overflowing() {
        use egui_kittest::kittest::Queryable;
        let mut harness = egui_kittest::Harness::builder().with_size(egui::Vec2::new(360.0, 740.0)).with_max_steps(48).build_eframe(|cc| {
            crate::PhotocraftApp::setup_context(&cc.egui_ctx, crate::theme::ThemeKind::Studio);
            let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
            app.studio_chrome = true;
            app.touch_ui = true;
            let _ = app.run("file.new", json!({"width": 64, "height": 64, "name": "Sprite"}));
            app
        });
        harness.run_steps(3);
        harness.get_by_label("AI Pixel Studio").click();
        harness.run_steps(4);
        let _ = harness.get_by_label("New layer");
        let _ = harness.get_by_label("Refine");
        let _ = harness.get_by_label("Batch");
        let _ = harness.get_by_label("Not in this build yet");
        let _ = harness.get_by_label("ComfyUI");
        let _ = harness.get_by_label("Retro Diffusion");
    }
}
