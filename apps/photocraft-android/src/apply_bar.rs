//! Apply / Cancel buttons at the bottom of the screen while Free Transform or a crop waits for
//! Enter or Esc, as touch apps do: on a tablet there's no Enter key to reach for, and the
//! options bar's small ✓ and ⊘ can be off the edge of a narrow screen.

use crate::edge_keys::EdgeKeys;
use photocraft_ui_egui::PhotocraftApp;

/// What is waiting to be applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pending {
    Transform,
    Crop,
}

pub fn pending(app: &PhotocraftApp) -> Option<Pending> {
    if app.ui.transform.is_some() {
        Some(Pending::Transform)
    } else if photocraft_ui_egui::crop_ui::pending(app) {
        Some(Pending::Crop)
    } else {
        None
    }
}

/// Draw the bar when something is pending; the buttons apply or cancel it.
pub fn show(ctx: &egui::Context, app: &mut PhotocraftApp, keys: &mut EdgeKeys) {
    let Some(what) = pending(app) else { return };
    let label = match what {
        Pending::Transform => "Transform",
        Pending::Crop => "Crop",
    };
    let mut apply = None;
    egui::Area::new(egui::Id::new("android_apply_bar"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -28.0])
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(label);
                    ui.add_space(8.0);
                    let size = egui::vec2(120.0, 44.0);
                    if ui.add(egui::Button::new("✕  Cancel").min_size(size)).clicked() {
                        apply = Some(false);
                    }
                    if ui.add(egui::Button::new("✓  Apply").min_size(size)).clicked() {
                        apply = Some(true);
                    }
                });
            });
        });
    match (what, apply) {
        (Pending::Transform, Some(true)) => photocraft_ui_egui::transform_tool::commit(app),
        (Pending::Transform, Some(false)) => photocraft_ui_egui::transform_tool::cancel(app),
        (Pending::Crop, Some(true)) => photocraft_ui_egui::canvas::commit_crop(app),
        (Pending::Crop, Some(false)) => keys.tap(egui::Key::Escape),
        (_, None) => {}
    }
    if apply.is_some() {
        ctx.request_repaint();
    }
}
