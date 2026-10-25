//! Two fingers on a touch screen zoom, pan and turn the canvas around the point between them
//! (touch_rotate.rs), with no pointer to anchor on (the Android shell drops the emulated mouse
//! during a gesture).

use egui::{Event, Pos2, TouchDeviceId, TouchId, TouchPhase, pos2};
use egui_kittest::Harness;
use serde_json::json;

use crate::PhotocraftApp;
use crate::canvas::ViewXform;
use crate::state::View;

fn harness(pixel_art: bool, rotate: bool) -> Harness<'static, PhotocraftApp> {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.session.execute("file.new", json!({"width": 64, "height": 64, "pixelArt": pixel_art})).unwrap();
    app.sync_views();
    app.touch_ui = true;
    app.touch_rotate = rotate;
    app.ui.extras.rulers = false;
    app.ui.views = vec![View { zoom: 4.0, center: [32.0, 32.0], fit_pending: false, fill_pending: false, doc_size: [64, 64], rotation: 0.0, pixel_art, ..View::default() }];
    let mut h = Harness::builder().with_size(egui::vec2(400.0, 400.0)).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            app.last_canvas_rect = ui.max_rect();
            let v = app.ui.views[0].clone();
            let v = crate::canvas::canvas_view(app, ui, 0, ui.max_rect(), v, true);
            if let Some(slot) = app.ui.views.get_mut(0) {
                *slot = v;
            }
        },
        app,
    );
    h.run_steps(2);
    h
}

fn touch(h: &mut Harness<'_, PhotocraftApp>, id: u64, phase: TouchPhase, p: Pos2) {
    h.event(Event::Touch { device_id: TouchDeviceId(1), id: TouchId(id), phase, pos: p, force: None });
}

fn xf(h: &Harness<'_, PhotocraftApp>) -> ViewXform {
    let app = h.state();
    let v = &app.ui.views[0];
    ViewXform { rect: app.last_canvas_rect, zoom: v.zoom / app.ppp, center: v.center, flip: false, rotation: v.rotation, aspect: 1.0 }
}

/// Put two fingers at `a` and `b`, then move them to `a2` and `b2` in `steps` frames.
fn gesture(h: &mut Harness<'_, PhotocraftApp>, (a, b): (Pos2, Pos2), (a2, b2): (Pos2, Pos2), steps: usize) {
    touch(h, 1, TouchPhase::Start, a);
    touch(h, 2, TouchPhase::Start, b);
    h.run_steps(1);
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        touch(h, 1, TouchPhase::Move, a + (a2 - a) * t);
        touch(h, 2, TouchPhase::Move, b + (b2 - b) * t);
        h.run_steps(1);
    }
}

fn lift(h: &mut Harness<'_, PhotocraftApp>, a: Pos2, b: Pos2) {
    touch(h, 1, TouchPhase::End, a);
    touch(h, 2, TouchPhase::End, b);
    h.run_steps(2);
}

#[test]
fn a_pinch_zooms_around_the_fingers() {
    let mut h = harness(false, true);
    let (a, b) = (pos2(150.0, 250.0), pos2(250.0, 250.0));
    let mid = pos2(200.0, 250.0);
    let under = xf(&h).to_doc(mid);
    gesture(&mut h, (a, b), (pos2(100.0, 250.0), pos2(300.0, 250.0)), 6);
    let zoom = h.state().ui.views[0].zoom;
    assert!(zoom > 7.0, "fingers twice as far apart: about twice the zoom, got {zoom}");
    let now = xf(&h).to_doc(mid);
    assert!((now[0] - under[0]).abs() < 0.5 && (now[1] - under[1]).abs() < 0.5, "{under:?} stayed under the fingers: {now:?}");
    assert_eq!(h.state().ui.views[0].rotation, 0.0, "no twist, no turn");
}

#[test]
fn two_fingers_pan_and_twist_around_their_midpoint() {
    let mut h = harness(false, true);
    let start = (pos2(150.0, 200.0), pos2(250.0, 200.0));
    let mid = pos2(200.0, 200.0);
    let under = xf(&h).to_doc(mid);
    // Turn 30° clockwise about the midpoint and move it 40 points right.
    let turn = |p: Pos2| {
        let (s, c) = 30f32.to_radians().sin_cos();
        let d = p - mid;
        mid + egui::vec2(d.x * c - d.y * s, d.x * s + d.y * c) + egui::vec2(40.0, 0.0)
    };
    gesture(&mut h, start, (turn(start.0), turn(start.1)), 10);
    let r = h.state().ui.views[0].rotation;
    assert!((r - 30.0).abs() < 1.5, "follows the fingers: {r}");
    let now = xf(&h).to_doc(mid + egui::vec2(40.0, 0.0));
    assert!((now[0] - under[0]).abs() < 0.5 && (now[1] - under[1]).abs() < 0.5, "{under:?} followed the fingers: {now:?}");
    lift(&mut h, turn(start.0), turn(start.1));
    assert!((h.state().ui.views[0].rotation - r).abs() < 1e-3, "no snapping on release");
}

#[test]
fn with_touch_rotate_off_a_twist_only_zooms_and_pans() {
    let mut h = harness(false, false);
    let (a, b) = (pos2(150.0, 200.0), pos2(250.0, 200.0));
    gesture(&mut h, (a, b), (pos2(200.0, 150.0), pos2(200.0, 250.0)), 8);
    assert_eq!(h.state().ui.views[0].rotation, 0.0);
}

#[test]
fn pixel_art_turns_by_quarter_turns_and_zooms_by_whole_pixels() {
    let mut h = harness(true, true);
    let mid = pos2(200.0, 200.0);
    let (a, b) = (pos2(150.0, 200.0), pos2(250.0, 200.0));
    // 60°: past halfway to a quarter turn.
    let turn = |p: Pos2, deg: f32| {
        let (s, c) = deg.to_radians().sin_cos();
        let d = p - mid;
        mid + egui::vec2(d.x * c - d.y * s, d.x * s + d.y * c) * 1.3
    };
    gesture(&mut h, (a, b), (turn(a, 60.0), turn(b, 60.0)), 10);
    let v = &h.state().ui.views[0];
    assert_eq!(v.rotation, 90.0);
    assert_eq!(v.zoom.fract(), 0.0, "whole pixels: {}", v.zoom);
}
