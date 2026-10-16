use super::*;
use crate::Session;
use photocraft_geom::Rect;

/// A 16×16 transparent pixel-art document; returns the session and its pixel layer.
fn doc() -> (Session, LayerId) {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 16, "height": 16, "background": "transparent", "pixelArt": true})).unwrap();
    let id = s.active().unwrap().active_layer.unwrap();
    (s, id)
}

fn fill(s: &mut Session, id: LayerId, r: Rect, c: [f32; 3]) {
    s.edit("fill", |d, _| {
        d.layer_mut(id).unwrap().surface_mut().unwrap().fill_rect(r, &[c[0], c[1], c[2], 1.0]);
        Ok(())
    })
    .unwrap();
}

fn px(s: &Session, id: LayerId, x: i32, y: i32) -> [f32; 4] {
    s.active().unwrap().doc.layer(id).unwrap().surface().unwrap().rgba(x, y)
}

const RED: [f32; 3] = [1.0, 0.0, 0.0];
const BLUE: [f32; 3] = [0.0, 0.0, 1.0];

#[test]
fn set_add_remove_and_undo() {
    let (mut s, _) = doc();
    let r = s.execute("palette.set", json!({"colors": ["#000000", "#ff0000"]})).unwrap();
    assert_eq!(r["count"], 2);
    s.execute("palette.add", json!({"color": "#0000ff"})).unwrap();
    s.execute("palette.add", json!({"color": "#0000ff"})).unwrap();
    assert_eq!(s.active().unwrap().doc.palette.len(), 3, "a colour already there isn't added twice");
    s.execute("palette.remove", json!({"index": 0})).unwrap();
    assert_eq!(s.active().unwrap().doc.palette, vec![RED, BLUE]);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc.palette.len(), 3);
}

#[test]
fn bad_params_are_errors() {
    let (mut s, _) = doc();
    assert!(s.execute("palette.set", json!({})).is_err());
    assert!(s.execute("palette.set", json!({"colors": ["red"]})).is_err());
    assert!(s.execute("palette.set", json!({"colors": vec!["#000000"; MAX_PALETTE + 1]})).is_err());
    assert!(s.execute("palette.remove", json!({"index": 0})).is_err(), "empty palette");
    assert!(s.execute("palette.recolor", json!({"index": 3})).is_err());
    s.execute("palette.set", json!({"colors": ["#000000"]})).unwrap();
    assert!(s.execute("palette.recolor", json!({"index": 0, "scope": "galaxy"})).is_err());
    assert!(s.execute("palette.ramp", json!({"index": 0, "steps": 0})).is_err());
    assert!(s.execute("palette.ramp", json!({"index": 0, "steps": 99})).is_err());
    assert!(s.execute("palette.fromLayer", json!({})).is_err(), "an empty layer has no colours");
    assert!(Session::new().execute("palette.add", json!({})).is_err());
    let mut f = Session::new();
    f.execute("file.new", json!({"width": 8, "height": 8, "depth": 32})).unwrap();
    assert!(f.execute("palette.add", json!({})).is_err(), "32-bit documents have no exact colours");
}

#[test]
fn recolor_changes_the_entry_and_its_pixels_only() {
    let (mut s, id) = doc();
    fill(&mut s, id, Rect::from_xywh(0, 0, 4, 4), RED);
    fill(&mut s, id, Rect::from_xywh(8, 8, 2, 2), BLUE);
    s.execute("palette.set", json!({"colors": ["#ff0000", "#0000ff"]})).unwrap();
    let r = s.execute("palette.recolor", json!({"index": 0, "color": "#00ff00"})).unwrap();
    assert_eq!(r["pixels"], 16);
    assert_eq!(px(&s, id, 1, 1), [0.0, 1.0, 0.0, 1.0]);
    assert_eq!(px(&s, id, 8, 8), [0.0, 0.0, 1.0, 1.0], "other colours stay");
    assert_eq!(s.active().unwrap().doc.palette[0], [0.0, 1.0, 0.0]);
    assert!(s.undo());
    assert_eq!(px(&s, id, 1, 1), [1.0, 0.0, 0.0, 1.0], "one undo step");
}

#[test]
fn recolor_in_one_layer_leaves_the_others() {
    let (mut s, a) = doc();
    fill(&mut s, a, Rect::from_xywh(0, 0, 2, 2), RED);
    let b = LayerId(s.execute("layer.new.layer", json!({})).unwrap()["layer"].as_u64().unwrap());
    fill(&mut s, b, Rect::from_xywh(0, 0, 2, 2), RED);
    s.execute("palette.set", json!({"colors": ["#ff0000"]})).unwrap();
    s.execute("palette.recolor", json!({"index": 0, "color": "#0000ff", "scope": "layer"})).unwrap();
    assert_eq!(px(&s, b, 0, 0), [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(px(&s, a, 0, 0), [1.0, 0.0, 0.0, 1.0]);
}

#[test]
fn from_layer_and_ramps() {
    let (mut s, id) = doc();
    fill(&mut s, id, Rect::from_xywh(0, 0, 2, 2), [1.0, 1.0, 1.0]);
    fill(&mut s, id, Rect::from_xywh(4, 4, 2, 2), [0.0, 0.0, 0.0]);
    let r = s.execute("palette.fromLayer", json!({})).unwrap();
    assert_eq!(r["colors"], json!(["#000000", "#ffffff"]), "dark to light");
    let r = s.execute("palette.ramp", json!({"index": 0, "color": "#ffffff", "steps": 2})).unwrap();
    assert_eq!(r["colors"], json!(["#000000", "#808080", "#ffffff", "#ffffff"]));
}

#[test]
fn from_swatches_uses_a_group() {
    let (mut s, _) = doc();
    let r = s.execute("palette.fromSwatches", json!({})).unwrap();
    assert!(r["count"].as_u64().unwrap() > 0);
    assert!(s.execute("palette.fromSwatches", json!({"group": "No Such Group"})).is_err());
}

#[test]
fn shading_moves_covered_palette_pixels_one_step() {
    let g = 128.0 / 255.0;
    let palette = [[0.0, 0.0, 0.0], [g, g, g], [1.0, 1.0, 1.0]];
    let fmt = photocraft_doc::PixelFormat::RGBA8;
    let mut pre = Surface::new(fmt);
    pre.fill_rect(Rect::from_xywh(0, 0, 3, 1), &[0.0, 0.0, 0.0, 1.0]);
    pre.fill_rect(Rect::from_xywh(1, 0, 1, 1), &[g, g, g, 1.0]);
    pre.fill_rect(Rect::from_xywh(2, 0, 1, 1), &[1.0, 1.0, 1.0, 1.0]);
    pre.fill_rect(Rect::from_xywh(3, 0, 1, 1), &[0.2, 0.9, 0.1, 1.0]);
    let marker = shade_marker(&palette);
    let mut surf = pre.clone();
    // The stroke covered x = 0..4 (not x = 4, left transparent).
    surf.fill_rect(Rect::from_xywh(0, 0, 4, 1), &[marker[0], marker[1], marker[2], 1.0]);
    shade_pixels(&pre, &mut surf, Rect::from_xywh(0, 0, 5, 1), marker, &palette, 1);
    let got: Vec<[f32; 4]> = (0..5).map(|x| surf.rgba(x, 0)).collect();
    assert!((got[0][0] - 0.5).abs() < 0.01, "black → grey: {got:?}");
    assert_eq!(got[1][0], 1.0, "grey → white");
    assert_eq!(got[2][0], 1.0, "white stays white at the end of the ramp");
    assert!((got[3][1] - 0.9).abs() < 0.01, "a colour not in the palette is left alone");
    assert_eq!(got[4][3], 0.0, "transparent stays transparent");
    assert!(!palette.iter().any(|p| same(*p, marker)));
}

#[test]
fn nearest_entry() {
    let palette = [[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]];
    assert_eq!(nearest(&palette, [0.3, 0.3, 0.3]), Some(0));
    assert_eq!(nearest(&palette, [0.7, 0.6, 0.9]), Some(1));
    assert_eq!(nearest(&[], [0.0; 3]), None);
}

/// A 1-pixel-tall row of the palette's three greys at x = 0, 1, 2.
fn ramp_row() -> (Session, LayerId) {
    let (mut s, id) = doc();
    s.execute("palette.set", json!({"colors": ["#000000", "#808080", "#ffffff"]})).unwrap();
    fill(&mut s, id, Rect::from_xywh(0, 0, 1, 1), [0.0; 3]);
    fill(&mut s, id, Rect::from_xywh(1, 0, 1, 1), [128.0 / 255.0; 3]);
    fill(&mut s, id, Rect::from_xywh(2, 0, 1, 1), [1.0; 3]);
    (s, id)
}

#[test]
fn the_pencil_shades_along_the_palette() {
    let (mut s, id) = ramp_row();
    let p = json!({"points": [[0.5, 0.5], [3.5, 0.5]], "size": 1, "shade": 1});
    s.execute("paint.pencil", p.clone()).unwrap();
    assert!((px(&s, id, 0, 0)[0] - 128.0 / 255.0).abs() < 0.01, "black → grey");
    assert_eq!(px(&s, id, 1, 0)[0], 1.0, "grey → white");
    assert_eq!(px(&s, id, 2, 0)[0], 1.0, "white stays");
    assert_eq!(px(&s, id, 3, 0)[3], 0.0, "empty pixels stay empty");
    // Back the other way.
    s.execute("paint.pencil", json!({"points": [[0.5, 0.5], [1.5, 0.5]], "size": 1, "shade": -1})).unwrap();
    assert_eq!(px(&s, id, 0, 0)[0], 0.0);
    // The live preview shades the same pixels the commit does.
    let (s2, _) = ramp_row();
    let live = crate::brush_cmds::LiveStroke::begin_with(&s2, "paint.pencil", &p).unwrap().into_commit().unwrap();
    let (mut s3, id3) = ramp_row();
    s3.execute("paint.pencil", p).unwrap();
    for x in 0..4 {
        assert_eq!(live.surface.rgba(x, 0), px(&s3, id3, x, 0), "x = {x}");
    }
}

#[test]
fn shading_needs_a_palette_and_a_direction() {
    let (mut s, _) = doc();
    assert!(s.execute("paint.pencil", json!({"points": [[1, 1]], "shade": 1})).is_err(), "no palette");
    s.execute("palette.set", json!({"colors": ["#000000"]})).unwrap();
    assert!(s.execute("paint.pencil", json!({"points": [[1, 1]], "shade": 2})).is_err());
    assert!(s.execute("paint.pencil", json!({"points": [[1, 1]], "shade": "up"})).is_err());
    assert!(s.execute("paint.pencil", json!({"points": [[1, 1]], "shade": 0})).is_ok(), "0 is plain painting");
}

#[test]
fn palette_lock_paints_the_nearest_entry() {
    let (mut s, id) = doc();
    s.execute("palette.set", json!({"colors": ["#000000", "#ff0000"]})).unwrap();
    s.execute("paint.pencil", json!({"points": [[0.5, 0.5]], "size": 1, "color": "#e01010", "paletteLock": true})).unwrap();
    assert_eq!(px(&s, id, 0, 0), [1.0, 0.0, 0.0, 1.0]);
    s.execute("paint.pencil", json!({"points": [[1.5, 0.5]], "size": 1, "color": "#e01010"})).unwrap();
    assert!((px(&s, id, 1, 0)[0] - 224.0 / 255.0).abs() < 0.01, "off, the colour is as picked");
}

#[test]
fn the_palette_is_saved_in_pcraft() {
    let (mut s, _) = doc();
    s.execute("palette.set", json!({"colors": ["#1a1c2c", "#ffcd75"]})).unwrap();
    let d = s.active().unwrap().doc.clone();
    let bytes = photocraft_format::save_to_bytes(&d, &Default::default()).unwrap();
    let back = photocraft_format::load_from_bytes(&bytes).unwrap();
    assert_eq!(back.palette, d.palette);
    assert!(back.pixel_art);
}
