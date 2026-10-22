//! JPEG Options: the quality File › Save and Save As ask for before writing a JPEG, as
//! Photoshop's JPEG Options does (a Low / Medium / High / Maximum preset and a quality slider).
//! It opens at the quality the photo was saved with, or Preferences › Export's JPEG quality for
//! a new one.
//!
//! Only where [`PhotocraftApp::ask_jpeg_quality`] is on (the touch app): elsewhere a save writes
//! at that quality without asking, as before, so scripts and actions never stop on a prompt.
//!
//! The save is parked in [`Prompt`] while the modal is up (egui is immediate-mode) and written
//! once answered, with the answer in force for that one write ([`PhotocraftApp::jpeg_answer`]).
//! Cancel drops the save.

use crate::PhotocraftApp;

/// Which save continues once answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Then {
    /// File › Save over the document's own file.
    Save,
    /// File › Save As to a new path (recorded as `file.saveAs`).
    SaveAs,
}

/// A save waiting on the JPEG Options answer.
pub struct Prompt {
    /// The document that requested the save, even if tabs change while this is open.
    doc: photocraft_doc::DocId,
    /// Where the document goes.
    pub path: String,
    /// The quality chosen, 1–100.
    pub quality: f32,
    then: Then,
}

/// Photoshop's presets, with the qualities they choose.
const PRESETS: [(&str, f32); 4] = [("Low", 30.0), ("Medium", 60.0), ("High", 80.0), ("Maximum", 95.0)];

/// `true` when `path` names a JPEG.
pub fn is_jpeg_path(path: &str) -> bool {
    photocraft_engine::file_cmds::extension(path).and_then(|e| photocraft_codecs::from_extension(&e)) == Some(photocraft_codecs::Format::Jpeg)
}

/// Whether saving the active document to `path` asks for JPEG Options first.
pub fn wants_prompt(app: &PhotocraftApp, path: &str) -> bool {
    app.ask_jpeg_quality && app.jpeg_answer.is_none() && app.session.active().is_some() && is_jpeg_path(path)
}

/// The quality the prompt opens at: the photo's own, else Preferences › Export's.
fn start_quality(app: &PhotocraftApp) -> f32 {
    let own = app.session.active().and_then(|st| st.doc.file_encoding.jpeg_quality);
    own.map(u32::from).unwrap_or(app.session.prefs().export.jpeg_quality).clamp(1, 100) as f32
}

/// Parks the save behind the prompt. A prompt already waiting is never replaced.
pub fn park(app: &mut PhotocraftApp, path: String, then: Then) -> Result<(), String> {
    if app.save_options_open() {
        return Err("Answer the save options before starting another save".into());
    }
    let doc = app.active_doc_id()?;
    let quality = start_quality(app);
    app.jpeg_options = Some(Prompt { doc, path, quality, then });
    Ok(())
}

/// The preset a quality falls under, for the menu.
fn preset_of(quality: f32) -> Option<&'static str> {
    PRESETS.iter().find(|(_, q)| (*q - quality).abs() < 0.5).map(|(name, _)| *name)
}

pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some(p) = &mut app.jpeg_options else { return };
    let (mut ok, mut cancel) = (false, false);
    let mut quality = p.quality;
    let modal = egui::Modal::new(egui::Id::new("jpeg-options")).show(ctx, |ui| {
        ui.set_max_width(320.0_f32.min(ctx.content_rect().width() - 40.0));
        ui.label(egui::RichText::new(tl!("JPEG Options")).font(crate::theme::semibold(15.0)));
        ui.add_space(4.0);
        crate::widgets::hairline(ui);
        ui.add_space(8.0);
        crate::widgets::slider_row(ui, tl!("Quality"), &mut quality, 1.0..=100.0, "%", None);
        // Photoshop's presets; a quality between them reads Custom.
        let mut preset = preset_of(quality).unwrap_or("Custom").to_string();
        let mut options: Vec<(String, &str)> = PRESETS.iter().map(|(name, _)| ((*name).to_string(), tl!(name))).collect();
        if preset == "Custom" {
            options.insert(0, (preset.clone(), tl!("Custom")));
        }
        if crate::widgets::dropdown(ui, "jpeg-quality-preset", &mut preset, &options, 140.0)
            && let Some((_, q)) = PRESETS.iter().find(|(name, _)| *name == preset)
        {
            quality = *q;
        }
        quality = quality.round().clamp(1.0, 100.0);
        ui.add_space(12.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            ok = crate::widgets::primary_button(ui, tl!("OK"), 84.0).clicked();
            cancel = crate::widgets::secondary_button(ui, tl!("Cancel"), 84.0).clicked();
        });
    });
    cancel |= modal.should_close();
    p.quality = quality;
    if ok || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
        let Some(p) = app.jpeg_options.take() else { return };
        let q = p.quality.round().clamp(1.0, 100.0) as u8;
        app.jpeg_answer = Some((p.doc, q));
        let written = app.with_document(p.doc, |app| match p.then {
            Then::Save => app.save_in_place(p.path.clone()),
            Then::SaveAs => app.save_to(p.path.clone(), None),
        });
        app.jpeg_answer = None;
        match written {
            Err(e) => {
                app.ui.status = crate::i18n::fmt(tl!("Save failed: {error}"), &[("error", &e)]);
                app.ui.status_error = true;
            }
            Ok(_) => crate::discard_ui::saved_document(app, ctx, p.doc),
        }
    } else if cancel {
        app.jpeg_options = None;
        app.ui.status = tl!("Save cancelled").into();
        app.ui.status_error = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jpeg_names() {
        for yes in ["a.jpg", "b.JPEG", "dir/c.jpe", "content://x/document/9/Photo.jpg"] {
            assert!(is_jpeg_path(yes), "{yes}");
        }
        for no in ["a.png", "a.jpg.psd", "jpg", ".jpg"] {
            assert!(!is_jpeg_path(no), "{no}");
        }
    }

    #[test]
    fn presets_name_their_qualities() {
        assert_eq!(preset_of(80.0), Some("High"));
        assert_eq!(preset_of(95.0), Some("Maximum"));
        assert_eq!(preset_of(81.0), None);
    }
}
