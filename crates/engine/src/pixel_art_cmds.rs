//! View › Pixel Art Mode: marks the active document as pixel art (`Document::pixel_art`).
//!
//! The pixels don't change; the flag tells the shell to show and edit the document for pixel
//! work (whole-number zoom, the pixel grid, no anti-aliasing, nearest-neighbour transforms) and
//! makes the Pencil pixel-perfect (`brush_cmds::pencil_brush`). It is a history step, like any
//! change to the document, and is saved in `.pcraft`.

use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::{EngineError, Result, Session};

const CMD: &str = "view.pixelArtMode";

fn has_doc(s: &Session) -> std::result::Result<(), String> {
    s.active().map(|_| ()).ok_or_else(|| "no document".into())
}

/// Turn the mode on or off (`on`), or flip it (no `on`). Setting what is already set changes
/// nothing and leaves no history step.
fn pixel_art_mode(s: &mut Session, p: &Value) -> Result<Value> {
    let current = s.active().ok_or(EngineError::NoDocument)?.doc.pixel_art;
    let on = match p.get("on") {
        None | Some(Value::Null) => !current,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err(EngineError::BadParams { cmd: CMD.into(), msg: "`on` must be true or false".into() }),
    };
    if on != current {
        s.edit("Pixel Art Mode", |d, _| {
            d.pixel_art = on;
            Ok(())
        })?;
    }
    Ok(json!({ "on": on }))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: CMD,
        label: "Pixel Art Mode",
        menu: &["View"],
        shortcut: None,
        params: r##"{"on":bool? (omit to toggle)}"##,
        enabled: has_doc,
        journal: true,
        run: pixel_art_mode,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 16, "height": 16})).unwrap();
        s
    }

    #[test]
    fn toggles_sets_and_undoes() {
        let mut s = session();
        assert!(!s.active().unwrap().doc.pixel_art);
        assert_eq!(s.execute(CMD, json!({})).unwrap()["on"], true);
        assert!(s.active().unwrap().doc.pixel_art);
        let steps = s.active().unwrap().history.past_len();
        assert_eq!(s.execute(CMD, json!({"on": true})).unwrap()["on"], true);
        assert_eq!(s.active().unwrap().history.past_len(), steps, "setting what is set adds no step");
        assert!(s.undo());
        assert!(!s.active().unwrap().doc.pixel_art, "undo turns it back off");
    }

    #[test]
    fn bad_params_and_no_document_are_errors() {
        let mut s = session();
        assert!(s.execute(CMD, json!({"on": "yes"})).is_err());
        assert!(Session::new().execute(CMD, json!({})).is_err());
    }

    #[test]
    fn file_new_can_start_in_pixel_art_mode() {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 32, "height": 32, "pixelArt": true})).unwrap();
        assert!(s.active().unwrap().doc.pixel_art);
        assert!(s.execute("file.new", json!({"pixelArt": 1})).is_err());
    }

    /// Pixels painted on the active layer of a transparent 16×16 document by a 1 px pencil
    /// zig-zag (right, then down: an L corner at (4, 0)).
    fn zigzag(pixel_art: bool, extra: Value) -> Vec<(i32, i32)> {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 16, "height": 16, "background": "transparent", "pixelArt": pixel_art})).unwrap();
        let mut p = json!({"points": [[0.5, 0.5], [4.5, 0.5], [4.5, 4.5]], "size": 1, "color": "#000000"});
        if let (Some(p), Some(e)) = (p.as_object_mut(), extra.as_object()) {
            p.extend(e.clone());
        }
        s.execute("paint.pencil", p).unwrap();
        let d = &s.active().unwrap().doc;
        let surf = d.layers.iter().find_map(|l| l.surface()).unwrap();
        (0..16).flat_map(|y| (0..16).map(move |x| (x, y))).filter(|&(x, y)| surf.rgba(x, y)[3] > 0.5).collect()
    }

    #[test]
    fn the_pencil_is_pixel_perfect_in_a_pixel_art_document() {
        let plain = zigzag(false, json!({}));
        assert!(plain.contains(&(4, 0)), "a plain pencil keeps the corner: {plain:?}");
        let perfect = zigzag(true, json!({}));
        assert!(!perfect.contains(&(4, 0)), "{perfect:?}");
        assert!(perfect.contains(&(0, 0)) && perfect.contains(&(4, 4)), "the ends stay");
        assert_eq!(zigzag(true, json!({"pixelPerfect": false})), plain, "and it can be turned off");
        assert_eq!(zigzag(false, json!({"pixelPerfect": true})), perfect, "or on anywhere");
    }
}
