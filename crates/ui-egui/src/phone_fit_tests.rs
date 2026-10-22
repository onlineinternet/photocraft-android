//! Every window fits a phone: each menu command is run with a document open on phone-sized
//! screens, upright and sideways, and every window it opens (dialogs, panels, popups) must lie
//! inside the screen (the Android port; a Color Picker was cut off on a phone held upright).

use egui::Order;
use serde_json::json;

use crate::PhotocraftApp;

/// Commands that leave the app or wait on the system, and Cycle Screen Mode: after hundreds of
/// other commands in one session it trips an egui debug check on the canvas's scrollbar (not in
/// release builds; cycling on its own, on any screen, doesn't).
fn skipped(id: &str) -> bool {
    id.contains("quit") || id.contains("exit") || id.starts_with("help.") && id.contains("online") || id == "view.screenMode.cycle"
}

/// Windows opened by clicking rather than from a menu.
type Opener = fn(&mut PhotocraftApp);

const EXTRAS: &[(&str, Opener)] = &[
    ("(foreground color picker)", |app| {
        crate::color_picker_ui::open(app, "foreground");
    }),
    ("(background color picker)", |app| {
        crate::color_picker_ui::open(app, "background");
    }),
];

/// The windows on screen now that reach outside `screen`, as (layer, rect).
fn overflowing(ctx: &egui::Context, screen: egui::Rect) -> Vec<(egui::LayerId, egui::Rect)> {
    ctx.memory(|m| {
        let visible = m.areas().visible_layer_ids();
        m.layer_ids()
            .filter(|l| matches!(l.order, Order::Middle | Order::Foreground) && visible.contains(l))
            .filter_map(|l| m.area_rect(l.id).map(|r| (l, r)))
            .filter(|(_, r)| r.is_positive() && !screen.expand(0.5).contains_rect(*r))
            .collect()
    })
}

fn harness(size: egui::Vec2) -> egui_kittest::Harness<'static, PhotocraftApp> {
    egui_kittest::Harness::builder().with_size(size).with_max_steps(8).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.session.prefs.edit(|p| p.interface.language = "en".into());
        app.touch_ui = true;
        app
    })
}

pub(crate) fn sweep(size: egui::Vec2, only: Option<&str>) -> Vec<String> {
    let mut h = harness(size);
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
    let mut ids: Vec<String> = crate::menus::menu_items(h.state()).into_iter().map(|m| m.id).filter(|id| id != "---" && !id.is_empty()).collect();
    // Windows no menu command opens.
    ids.extend(EXTRAS.iter().map(|(id, _)| id.to_string()));
    let mut bad = Vec::new();
    for id in ids {
        if skipped(&id) || only.is_some_and(|o| !id.starts_with(o)) {
            continue;
        }
        let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = h.state_mut();
            app.ui.dialogs.clear();
            if app.session.active().is_none() {
                let _ = app.run("file.new", json!({"width": 64, "height": 48}));
            }
            let ctx = h.ctx.clone();
            match EXTRAS.iter().find(|(e, _)| *e == id) {
                Some((_, open)) => open(h.state_mut()),
                None => {
                    let _ = crate::menus::invoke(h.state_mut(), &ctx, &id, json!({}));
                }
            }
            h.run_steps(4);
            let found = overflowing(&h.ctx, screen);
            h.state_mut().ui.dialogs.clear();
            h.run_steps(2);
            found
        }));
        match run {
            Ok(found) => {
                for (layer, r) in found {
                    bad.push(format!("{id}: {:?} {r:?}", layer.id));
                }
            }
            Err(_) => {
                bad.push(format!("{id}: PANIC"));
                h = harness(size);
            }
        }
    }
    bad
}

#[test]
#[ignore = "slow: every menu command at two phone sizes; run with --ignored"]
fn every_window_fits_a_phone() {
    let mut bad = Vec::new();
    for size in [egui::vec2(360.0, 740.0), egui::vec2(740.0, 360.0)] {
        bad.extend(sweep(size, None).into_iter().map(|b| format!("{size:?} {b}")));
    }
    assert!(bad.is_empty(), "{} windows don't fit:\n{}", bad.len(), bad.join("\n"));
}

/// The Color Picker fits a phone held upright (it was cut off on the right): its swatches,
/// buttons and number fields go under the colour field.
#[test]
fn the_color_picker_fits_a_phone() {
    for size in [egui::vec2(360.0, 740.0), egui::vec2(412.0, 900.0)] {
        let bad = sweep(size, Some("(foreground color picker)"));
        assert!(bad.is_empty(), "{size:?}: {bad:?}");
        let mut h = harness(size);
        crate::color_picker_ui::open(h.state_mut(), "foreground");
        h.run_steps(4);
        use egui_kittest::kittest::Queryable;
        for label in ["OK", "Cancel", "Add to Swatches"] {
            let r = h.get_by_label(label).rect();
            assert!(r.right() <= size.x && r.bottom() <= size.y, "{label} is off a {size:?} screen at {r:?}");
        }
    }
}
