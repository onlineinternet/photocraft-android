//! JPEG Options on Save As and Save (the touch app): the quality it asks for, what's written,
//! Cancel, and no prompt where the app doesn't ask (the desktop, scripts).
use super::*;
use crate::{FileDialogAnswer, jpeg_options_ui};
use egui_kittest::kittest::Queryable;
use std::{cell::RefCell, rc::Rc};

/// Each write: the path, and the JPEG quality the export was asked for (as JSON).
type Writes = Rc<RefCell<Vec<(String, Vec<u8>)>>>;

fn asked(quality: Option<u8>) -> Vec<u8> {
    serde_json::to_vec(&quality).unwrap()
}

/// One 8×8 document whose exports record their JPEG quality, in a harness showing JPEG Options.
fn jpeg_app(ask: bool) -> (Prompted, Writes) {
    let mut app = app_with_docs(1);
    app.ask_jpeg_quality = ask;
    app.session.prefs.edit(|p| p.export.jpeg_quality = 85);
    let writes: Writes = Rc::default();
    let written = writes.clone();
    app.services.file_dialog = Some(crate::file_dialog::fake(vec![Some(FileDialogAnswer::SaveTo("unused.jpg".into()))]).0);
    app.services.export = Some(Box::new(|_, _, settings| Ok((asked(settings.jpeg_quality), Vec::new()))));
    app.services.write = Some(Box::new(move |path, bytes| {
        written.borrow_mut().push((path.to_string(), bytes.to_vec()));
        Ok(())
    }));
    let h = egui_kittest::Harness::builder().with_size(egui::vec2(800.0, 600.0)).build_ui_state(
        |ui, app| {
            show(app, ui.ctx());
            jpeg_options_ui::show(app, ui.ctx());
            app.poll_file_dialog(ui.ctx(), None);
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
    (h, writes)
}

/// Save As to a JPEG asks for the quality first, starting at Preferences › Export's, and writes
/// what was chosen; Cancel writes nothing.
#[test]
fn save_as_to_a_jpeg_asks_for_its_quality() {
    let (mut h, writes) = jpeg_app(true);
    h.state_mut().save_as(Some("photo.jpg".into())).unwrap();
    h.run_steps(2);
    assert_eq!(h.state().jpeg_options.as_ref().map(|p| p.quality), Some(85.0));
    assert!(writes.borrow().is_empty(), "nothing is written before the answer");
    assert!(h.state_mut().save_as(Some("other.jpg".into())).is_err(), "one save at a time");
    h.state_mut().jpeg_options.as_mut().unwrap().quality = 42.0;
    h.run_steps(1);
    h.get_by_label("OK").click();
    h.run_steps(3);
    assert_eq!(writes.borrow().as_slice(), &[("photo.jpg".to_string(), asked(Some(42)))]);
    assert!(h.state().jpeg_options.is_none() && h.state().jpeg_answer.is_none());
    assert_eq!(h.state().session.active().unwrap().path.as_deref(), Some("photo.jpg"));

    h.state_mut().save_as(Some("c.jpg".into())).unwrap();
    h.run_steps(2);
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    assert_eq!(writes.borrow().len(), 1, "Cancel wrote nothing");
    assert!(h.state().jpeg_options.is_none());
}

/// Where the app doesn't ask (the desktop), a JPEG is written at once, as before.
#[test]
fn without_asking_a_jpeg_is_written_at_once() {
    let (mut h, writes) = jpeg_app(false);
    h.state_mut().save_as(Some("photo.jpg".into())).unwrap();
    h.run_steps(2);
    assert!(h.state().jpeg_options.is_none());
    assert_eq!(writes.borrow().len(), 1);
}
