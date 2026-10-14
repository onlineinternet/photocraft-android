//! The edge keyboard: a strip that slides in from the right edge with the keys a touch screen
//! lacks, after Clip Studio Paint's Edge Keyboard.
//!
//! - **Ctrl, Shift, Alt** latch: tap one and it stays held while the pen works (Alt + tap picks a
//!   color, Shift + drag draws a straight line, Ctrl + drag moves the layer), until tapped again.
//!   They go through PhotoCraft's own Modifier Keys panel state (`ShellUi::sticky_*`) and are
//!   added to every input event, so menus and panels see them too.
//! - **Space** latches as a held key: the canvas pans (the Hand tool) while it is on, and with
//!   Ctrl or Alt latched it zooms, as in Photoshop.
//! - The rest run PhotoCraft commands directly, so they work whatever the keyboard shortcuts are.
//!
//! - **Enter and Esc** apply or cancel whatever is waiting for them (a transform, a crop, a
//!   dialog).
//!
//! The strip also has the panel toggles that make a phone screen usable: **Panels** hides the
//! right-hand dock (Shift+Tab) and **Hide UI** hides the toolbar, options bar and dock (Tab).

use egui::{Event, Key, Modifiers};
use photocraft_ui_egui::PhotocraftApp;
use serde_json::json;

/// What a button does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Ctrl,
    Shift,
    Alt,
    Space,
    /// Press and release a key.
    Tap(Key),
    /// Run a PhotoCraft command id.
    Command(&'static str),
}

/// The strip, top to bottom. `None` draws a gap.
pub const BUTTONS: &[Option<(&str, Action)>] = &[
    Some(("Ctrl", Action::Ctrl)),
    Some(("Shift", Action::Shift)),
    Some(("Alt", Action::Alt)),
    Some(("Space", Action::Space)),
    Some(("Enter", Action::Tap(Key::Enter))),
    Some(("Esc", Action::Tap(Key::Escape))),
    None,
    Some(("Undo", Action::Command("edit.undo"))),
    Some(("Redo", Action::Command("edit.redo"))),
    None,
    Some(("Brush", Action::Command("tools.select.brush"))),
    Some(("Eraser", Action::Command("tools.select.eraser"))),
    Some(("Picker", Action::Command("tools.select.eyedropper"))),
    Some(("Move", Action::Command("tools.select.move"))),
    Some(("Lasso", Action::Command("tools.select.lasso"))),
    None,
    Some(("Size −", Action::Command("tools.decreaseBrushSize"))),
    Some(("Size +", Action::Command("tools.increaseBrushSize"))),
    Some(("Swap", Action::Command("tools.swapColors"))),
    None,
    Some(("Transform", Action::Command("edit.freeTransform"))),
    Some(("Deselect", Action::Command("select.deselect"))),
    None,
    Some(("Panels", Action::Command("window.toggle.dock"))),
    Some(("Hide UI", Action::Command("window.togglePanels"))),
    Some(("Search", Action::Command("edit.search"))),
];

/// The latched keys.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Latched {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub space: bool,
}

impl Latched {
    pub fn modifiers(self) -> Modifiers {
        Modifiers { alt: self.alt, ctrl: self.ctrl, shift: self.shift, mac_cmd: false, command: self.ctrl }
    }
}

/// Add the latched modifiers to a frame's input. egui 0.36 tracks modifiers through
/// `Event::ModifiersChanged`, so each one the keyboard sends is widened by the latched keys
/// (`real` remembers the keyboard's own), the frame starts with the combination while anything
/// is latched, and each event that carries modifiers gets them too.
pub fn apply_modifiers(raw: &mut egui::RawInput, latched: Latched, real: &mut Modifiers, was_latched: &mut bool) {
    let m = latched.modifiers();
    for e in &mut raw.events {
        match e {
            Event::ModifiersChanged(changed) => {
                *real = *changed;
                *changed = *changed | m;
            }
            Event::Key { modifiers, .. } | Event::PointerButton { modifiers, .. } | Event::MouseWheel { modifiers, .. } => *modifiers = *modifiers | m,
            _ => {}
        }
    }
    // Every frame while latched (egui forgets modifiers when the window loses focus), and once
    // more when the last key is released, to go back to the keyboard's own.
    if !m.is_none() || *was_latched {
        raw.events.insert(0, Event::ModifiersChanged(*real | m));
    }
    *was_latched = !m.is_none();
}

/// The Space key going down or up.
pub fn space(pressed: bool, modifiers: Modifiers) -> Event {
    Event::Key { key: Key::Space, physical_key: Some(Key::Space), pressed, repeat: false, modifiers }
}

#[derive(Default)]
pub struct EdgeKeys {
    pub open: bool,
    pub latched: Latched,
    /// Space changed: send its key event with the next frame's input.
    space_changed: bool,
    /// A command tapped this frame, run once the frame is drawn.
    queued: Vec<&'static str>,
    /// Keys to press and release with the next frame's input.
    taps: Vec<Key>,
    /// The hardware keyboard's own modifiers, and whether any key was latched last frame.
    real: Modifiers,
    was_latched: bool,
}

impl EdgeKeys {
    /// Before the app sees a frame's input: latched modifiers, and Space going down or up.
    pub fn raw_input(&mut self, raw: &mut egui::RawInput) {
        if self.space_changed {
            self.space_changed = false;
            raw.events.push(space(self.latched.space, self.latched.modifiers()));
        }
        let m = self.latched.modifiers();
        for key in self.taps.drain(..) {
            for pressed in [true, false] {
                raw.events.push(Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers: m });
            }
        }
        apply_modifiers(raw, self.latched, &mut self.real, &mut self.was_latched);
    }

    /// Keep PhotoCraft's Modifier Keys panel in step with the latched keys, both ways: latching a
    /// key there latches it here too.
    pub fn sync(&mut self, app: &mut PhotocraftApp) {
        let s = &mut app.ui.shell;
        (self.latched.ctrl, self.latched.shift, self.latched.alt) = (s.sticky_command, s.sticky_shift, s.sticky_alt);
    }

    fn push_to_app(&self, app: &mut PhotocraftApp) {
        let s = &mut app.ui.shell;
        (s.sticky_command, s.sticky_shift, s.sticky_alt) = (self.latched.ctrl, self.latched.shift, self.latched.alt);
    }

    /// Run the commands tapped in the last frame.
    pub fn run_queued(&mut self, app: &mut PhotocraftApp, ctx: &egui::Context) {
        for id in std::mem::take(&mut self.queued) {
            if let Err(e) = photocraft_ui_egui::menus::invoke(app, ctx, id, json!({})) {
                log::info!("edge keyboard: {id}: {e}");
            }
        }
    }

    fn press(&mut self, action: Action, app: &mut PhotocraftApp) {
        match action {
            Action::Ctrl => self.latched.ctrl = !self.latched.ctrl,
            Action::Shift => self.latched.shift = !self.latched.shift,
            Action::Alt => self.latched.alt = !self.latched.alt,
            Action::Space => {
                self.latched.space = !self.latched.space;
                self.space_changed = true;
            }
            Action::Tap(key) => self.taps.push(key),
            Action::Command(id) => self.queued.push(id),
        }
        self.push_to_app(app);
    }

    fn is_on(&self, action: Action) -> bool {
        match action {
            Action::Ctrl => self.latched.ctrl,
            Action::Shift => self.latched.shift,
            Action::Alt => self.latched.alt,
            Action::Space => self.latched.space,
            Action::Tap(_) | Action::Command(_) => false,
        }
    }

    /// Press and release `key` with the next frame's input.
    pub fn tap(&mut self, key: Key) {
        self.taps.push(key);
    }

    /// Draw the tab on the right edge, and the strip when it's open.
    pub fn show(&mut self, ctx: &egui::Context, app: &mut PhotocraftApp) {
        let any_latched = self.latched != Latched::default();
        egui::Area::new(egui::Id::new("android_edge_keys"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::RIGHT_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    // The tab: always there, lit while a key is latched so it's never forgotten.
                    let tab = egui::Button::new(if self.open { "›" } else { "‹" }).min_size(egui::vec2(22.0, 72.0)).selected(any_latched && !self.open);
                    if ui.add(tab).on_hover_text("Edge keyboard").clicked() {
                        self.open = !self.open;
                    }
                    if !self.open {
                        return;
                    }
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        let max_h = (ctx.content_rect().height() - 24.0).max(200.0);
                        egui::ScrollArea::vertical().max_height(max_h).show(ui, |ui| {
                            ui.set_width(84.0);
                            ui.spacing_mut().item_spacing.y = 4.0;
                            for b in BUTTONS {
                                match b {
                                    None => {
                                        ui.add_space(4.0);
                                        ui.separator();
                                    }
                                    Some((label, action)) => {
                                        let button = egui::Button::new(*label).min_size(egui::vec2(84.0, 40.0)).selected(self.is_on(*action));
                                        if ui.add(button).clicked() {
                                            self.press(*action, app);
                                            ctx.request_repaint();
                                        }
                                    }
                                }
                            }
                        });
                    });
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latched_modifiers_reach_the_frame_and_its_events() {
        let (mut real, mut was) = (Modifiers::NONE, false);
        let mut raw = egui::RawInput::default();
        raw.events.push(Event::PointerButton { pos: egui::pos2(1.0, 1.0), button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
        raw.events.push(Event::Text("a".into()));
        apply_modifiers(&mut raw, Latched { alt: true, ctrl: true, ..Default::default() }, &mut real, &mut was);
        let Event::ModifiersChanged(first) = raw.events[0] else { panic!("{:?}", raw.events) };
        assert!(first.alt && first.ctrl && first.command && !first.shift);
        let Event::PointerButton { modifiers, .. } = raw.events[1] else { panic!() };
        assert!(modifiers.alt && modifiers.command);

        // A keyboard Shift on top keeps the latched keys; unlatching goes back to the keyboard's.
        let mut raw = egui::RawInput::default();
        raw.events.push(Event::ModifiersChanged(Modifiers::SHIFT));
        apply_modifiers(&mut raw, Latched { alt: true, ..Default::default() }, &mut real, &mut was);
        assert!(raw.events.iter().all(|e| matches!(e, Event::ModifiersChanged(m) if m.alt && m.shift)));
        let mut raw = egui::RawInput::default();
        apply_modifiers(&mut raw, Latched::default(), &mut real, &mut was);
        assert_eq!(raw.events, vec![Event::ModifiersChanged(Modifiers::SHIFT)]);
        let mut raw = egui::RawInput::default();
        apply_modifiers(&mut raw, Latched::default(), &mut real, &mut was);
        assert!(raw.events.is_empty(), "nothing latched, nothing added");
    }

    #[test]
    fn space_goes_down_and_up_once_per_change() {
        let mut keys = EdgeKeys::default();
        keys.latched.space = true;
        keys.space_changed = true;
        let mut raw = egui::RawInput::default();
        keys.raw_input(&mut raw);
        assert_eq!(raw.events, vec![space(true, Modifiers::NONE)]);
        let mut raw = egui::RawInput::default();
        keys.raw_input(&mut raw);
        assert!(raw.events.is_empty(), "held, not repeated");
    }

    #[test]
    fn a_tapped_key_goes_down_and_up_in_the_next_frame_only() {
        let mut keys = EdgeKeys::default();
        keys.tap(Key::Enter);
        let mut raw = egui::RawInput::default();
        keys.raw_input(&mut raw);
        let presses: Vec<bool> = raw.events.iter().filter_map(|e| match e {
            Event::Key { key: Key::Enter, pressed, .. } => Some(*pressed),
            _ => None,
        }).collect();
        assert_eq!(presses, [true, false]);
        let mut raw = egui::RawInput::default();
        keys.raw_input(&mut raw);
        assert!(raw.events.is_empty());
    }

    #[test]
    fn every_command_button_runs_a_command_photocraft_has() {
        for (label, action) in BUTTONS.iter().flatten() {
            if let Action::Command(id) = action {
                assert!(photocraft_ui_egui::menus::is_live(id), "{label}: {id} is no PhotoCraft command");
            }
        }
    }
}
