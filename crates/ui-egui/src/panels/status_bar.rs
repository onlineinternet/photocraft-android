use super::{PhotocraftApp, Tokens, label, vec2, widgets};

// ----------------------------------------------------------------------------- status bar

/// The status bar's height and the height of the controls in it (zoom field, Fit), which leaves
/// them an even margin above and below inside the bar (#2823).
pub fn status_bar_heights(t: &Tokens) -> (f32, f32) {
    if t.pro { (24.0, 18.0) } else { (30.0, 22.0) }
}

fn status_trailing_id() -> egui::Id {
    egui::Id::unique("status-bar-trailing-w")
}

/// The width left on the status bar's row before its trailing group (Fit, job progress) as wide
/// as it was last frame: what's on the left stops short of it instead of running under it on a
/// phone.
pub(crate) fn status_room(ui: &egui::Ui) -> f32 {
    let trailing_w: f32 = ui.data(|d| d.get_temp(status_trailing_id())).unwrap_or(0.0);
    ui.available_width() - trailing_w - 2.0 * ui.spacing().item_spacing.x
}

/// A status bar text no wider than `max_w`, cut with '…' (the whole of it on hover).
pub(crate) fn status_text(ui: &mut egui::Ui, text: &str, color: egui::Color32, size: f32, max_w: f32) {
    if max_w < 24.0 {
        return;
    }
    let galley = crate::tab_strip::elided(ui, text, egui::FontId::proportional(size), color, max_w);
    let cut = galley.elided;
    let resp = ui.add(egui::Label::new(galley));
    if cut {
        resp.on_hover_text(text);
    }
}

/// Draw the status bar's trailing group and remember its width for [`status_room`].
fn status_trailing(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    let before: f32 = ui.data(|d| d.get_temp(status_trailing_id())).unwrap_or(0.0);
    let w = ui
        .with_layout(crate::direction::trailing(), |ui| {
            let right = ui.max_rect().right();
            add(ui);
            (right - ui.min_rect().left()).max(0.0)
        })
        .inner;
    if (w - before).abs() > 0.5 {
        ui.data_mut(|d| d.insert_temp(status_trailing_id(), w));
        ui.ctx().request_repaint();
    }
}

pub fn status_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::bottom("status_bar")
        .exact_size(status_bar_heights(&t).0)
        .frame(egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin::symmetric(10, 0)))
        .show(ui, |ui| {
            let control_h = status_bar_heights(&t).1;
            // Read from the right in a right-to-left UI, with the trailing group at the left.
            crate::direction::horizontal_centered(ui, |ui| {
                if t.pro {
                    crate::chrome_ui::status_bar_pro(app, ui);
                    // Background job progress with Cancel, at the far end (#210).
                    status_trailing(ui, |ui| {
                        crate::jobs_ui::status_progress(app, ui);
                        crate::background_redraw::status(app, ui);
                        crate::chrome_ui::render_path_status(app, ui);
                    });
                    return;
                }
                // A label with its separator, when there's room for the whole of it.
                let fitted = |ui: &mut egui::Ui, text: &str| {
                    let font = egui::TextStyle::Body.resolve(ui.style());
                    let w = ui.fonts_mut(|f| f.layout_no_wrap(text.to_owned(), font, t.text_dim)).size().x;
                    if status_room(ui) >= w + 16.0 {
                        widgets::vline(ui, 16.0);
                        label(ui, text);
                    }
                };
                if let (Some(st), Some(i)) = (app.session.active(), app.session.active_index()) {
                    let (w, h, mode, bits, layers) =
                        (st.doc.size.width, st.doc.size.height, crate::canvas::mode_label(&st.doc), st.doc.depth.bits(), st.doc.layer_count());
                    let mut pct = app.ui.views[i].zoom * 100.0;
                    let field =
                        widgets::compact_wrapping_value_field(ui, &mut pct, crate::zoom_levels::percent_range(&app.ui.views[i]), "%", vec2(78.0, control_h))
                            .accessible_name(tl!("Zoom"));
                    if field.changed() {
                        let animated = app.session.prefs().general.animated_zoom;
                        crate::zoom_levels::set_from_field(&mut app.ui.views[i], field.dragged() || field.drag_started(), pct, animated);
                    }
                    fitted(ui, &crate::i18n::fmt(tl!("{mode} Color · {bits} bit"), &[("mode", mode), ("bits", &bits.to_string())]));
                    fitted(ui, &format!("{w} × {h} px"));
                    fitted(ui, &crate::i18n::trn(crate::i18n::current(), layers as u64, "{n} layer", "{n} layers"));
                } else {
                    label(ui, tl!("No document"));
                }
                // The status message, cut with '…' where the trailing group begins.
                if !app.ui.status.is_empty() && status_room(ui) > 40.0 {
                    widgets::vline(ui, 16.0);
                    let is_err = app.ui.status_error || app.ui.status.starts_with("Couldn");
                    let text = crate::shortcuts::strip_key_hints(&app.ui.status);
                    let size = egui::TextStyle::Body.resolve(ui.style()).size;
                    status_text(ui, &text, if is_err { t.warning } else { t.text_faint }, size, status_room(ui));
                }
                status_trailing(ui, |ui| {
                    if app.session.active().is_some()
                        && !t.pro
                        && widgets::compact_button(ui, tl!("Fit"), control_h).clicked()
                        && let Some(i) = app.session.active_index()
                    {
                        app.ui.views[i].fit_pending = true;
                    }
                    if app.session.has_jobs() {
                        ui.add_space(6.0);
                        crate::jobs_ui::status_progress(app, ui);
                    }
                    crate::background_redraw::status(app, ui);
                    crate::chrome_ui::render_path_status(app, ui);
                });
            });
        });
}

#[cfg(test)]
mod status_bar_phone_tests {
    use egui_kittest::kittest::{Queryable, by};

    /// On a phone the status bar's left side stops short of its right-hand group (a job's
    /// progress): a long message is cut with '…' instead of running under it (two messages were
    /// drawn on top of each other while a photo opened).
    #[test]
    fn status_text_stops_short_of_the_right_hand_group() {
        let width = 360.0;
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(width, 40.0)).build_ui(|ui| {
            ui.horizontal(|ui| {
                ui.label("100%");
                let room = super::status_room(ui);
                super::status_text(ui, "Opened a photo with a very long name from the camera roll.jpg", egui::Color32::GRAY, 12.0, room);
                super::status_trailing(ui, |ui| {
                    ui.label("Opening IMG_2041.jpg 40%");
                });
            });
        });
        h.run_steps(4);
        let progress = h.get_by_label("Opening IMG_2041.jpg 40%").rect();
        let status = h.get(by().label_contains("Opened")).rect();
        assert!(status.right() <= progress.left(), "the status {status:?} runs under the progress {progress:?}");
        assert!(progress.right() <= width);
    }
}
