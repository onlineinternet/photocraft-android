//! The edge keyboard: a strip that slides in from the right edge with one-tap buttons for the
//! shortcuts a touch screen has no keys for, after Clip Studio Paint's Edge Keyboard.
//!
//! Each button runs a PhotoCraft command directly, so it works whatever the keyboard shortcuts
//! are. **Shift**, **Ctrl** and **Alt** latch: tap one and it stays held (lit) for every tap and
//! stroke until it's tapped again, for the tools that read them (Shift adds to a selection, Alt
//! subtracts or picks a colour, Shift draws straight lines). They're the same latched keys as
//! PhotoCraft's Window › Modifier Keys panel.
//!
//! The strip also has the panel toggles that make a phone screen usable: **Panels** hides the
//! right-hand dock (Shift+Tab) and **Hide UI** hides the toolbar, options bar and dock (Tab).
//!
//! The Clip Studio chrome does not draw this tab. Those actions are labeled buttons in tool
//! properties, and selection options are their own toggles. This type still carries Open Photo,
//! Share and the finger settings for the shell.

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
    /// Run a PhotoCraft command id with these params (JSON).
    Run(&'static str, &'static str),
    /// Flip one of the touch settings.
    Toggle(Setting),
    /// File › Open through Android's photo picker (a grid of your photos) instead of the files.
    OpenPhotos,
    /// Send the image (flattened PNG) to another app through Android's share sheet.
    Share,
}

/// The touch settings the strip can flip ([`crate::pen::TouchSettings`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Setting {
    FingerDraws,
    FingerPressure,
    TouchRotate,
}

/// The strip, top to bottom, two to a row. `None` starts a new group.
pub const BUTTONS: &[Option<(&str, Action)>] = &[
    Some(("Shift", Action::Shift)),
    Some(("Ctrl", Action::Ctrl)),
    Some(("Alt", Action::Alt)),
    None,
    Some(("Undo", Action::Command("edit.undo"))),
    Some(("Redo", Action::Command("edit.redo"))),
    Some(("Copy", Action::Command("edit.copy"))),
    Some(("Paste", Action::Command("edit.paste"))),
    Some(("Open Photo", Action::OpenPhotos)),
    Some(("Share", Action::Share)),
    None,
    Some(("Select All", Action::Command("select.all"))),
    Some(("Deselect", Action::Command("select.deselect"))),
    Some(("Inverse", Action::Command("select.inverse"))),
    Some(("Transform", Action::Command("edit.freeTransform"))),
    None,
    Some(("Size −", Action::Command("tools.decreaseBrushSize"))),
    Some(("Size +", Action::Command("tools.increaseBrushSize"))),
    Some(("Swap", Action::Command("tools.swapColors"))),
    Some(("New Layer", Action::Command("layer.new.layer"))),
    None,
    Some(("Panels", Action::Command("window.toggle.dock"))),
    Some(("Hide UI", Action::Command("window.togglePanels"))),
    Some(("Search", Action::Command("edit.search"))),
    Some(("Pixel Art", Action::Command("view.pixelArtMode"))),
    None,
    Some(("Finger draws", Action::Toggle(Setting::FingerDraws))),
    Some(("Finger pressure", Action::Toggle(Setting::FingerPressure))),
    Some(("Touch rotate", Action::Toggle(Setting::TouchRotate))),
    Some(("Straighten", Action::Run("view.rotateView", r#"{"snap":90}"#))),
];

/// One key on the strip: big enough for a fingertip.
const KEY_SIZE: egui::Vec2 = egui::vec2(96.0, 48.0);
/// The tab that opens and closes the strip.
const TAB_SIZE: egui::Vec2 = egui::vec2(30.0, 88.0);

/// Does a press at `p` close the open strip (anywhere but the tab and the strip itself)?
pub fn closes_strip(p: egui::Pos2, tab: egui::Rect, strip: egui::Rect) -> bool {
    !tab.contains(p) && !strip.contains(p)
}

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
                *changed |= m;
            }
            Event::Key { modifiers, .. } | Event::PointerButton { modifiers, .. } | Event::MouseWheel { modifiers, .. } => *modifiers |= m,
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
    /// Open Photo was tapped: the shell runs File › Open with the photo picker.
    pub open_photos: bool,
    /// Share was tapped: the shell shares the active image.
    pub share: bool,
    pub open: bool,
    pub latched: Latched,
    /// Space changed: send its key event with the next frame's input.
    space_changed: bool,
    /// A command tapped this frame, run once the frame is drawn.
    queued: Vec<(&'static str, serde_json::Value)>,
    /// Keys to press and release with the next frame's input.
    taps: Vec<Key>,
    /// The hardware keyboard's own modifiers, and whether any key was latched last frame.
    real: Modifiers,
    was_latched: bool,
    /// How fingers behave; `settings_changed` until the shell saves them.
    pub settings: crate::pen::TouchSettings,
    pub settings_changed: bool,
    /// Alt held for this frame only (the S Pen's barrel button).
    pub transient_alt: bool,
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
        let mut keys = self.latched;
        keys.alt |= self.transient_alt;
        apply_modifiers(raw, keys, &mut self.real, &mut self.was_latched);
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
        for (id, params) in std::mem::take(&mut self.queued) {
            if let Err(e) = photocraft_ui_egui::menus::invoke(app, ctx, id, params) {
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
            Action::Command(id) => self.queued.push((id, json!({}))),
            Action::Run(id, params) => self.queued.push((id, serde_json::from_str(params).unwrap_or_else(|_| json!({})))),
            Action::OpenPhotos => self.open_photos = true,
            Action::Share => self.share = true,
            Action::Toggle(setting) => {
                let s = &mut self.settings;
                match setting {
                    Setting::FingerDraws => s.finger_draws = !s.finger_draws,
                    Setting::FingerPressure => s.finger_pressure = !s.finger_pressure,
                    Setting::TouchRotate => s.touch_rotate = !s.touch_rotate,
                }
                self.settings_changed = true;
            }
        }
        self.push_to_app(app);
    }

    fn is_on(&self, action: Action) -> bool {
        match action {
            Action::Ctrl => self.latched.ctrl,
            Action::Shift => self.latched.shift,
            Action::Alt => self.latched.alt,
            Action::Space => self.latched.space,
            Action::Toggle(Setting::FingerDraws) => self.settings.finger_draws,
            Action::Toggle(Setting::FingerPressure) => self.settings.finger_pressure,
            Action::Toggle(Setting::TouchRotate) => self.settings.touch_rotate,
            Action::Tap(_) | Action::Command(_) | Action::Run(..) | Action::OpenPhotos | Action::Share => false,
        }
    }

    /// Press and release `key` with the next frame's input.
    pub fn tap(&mut self, key: Key) {
        self.taps.push(key);
    }

    /// Draw the tab on the right edge, and the strip beside it when it's open. They're separate
    /// areas, so the strip takes the height it needs (up to the screen's) instead of the tab's.
    pub fn show(&mut self, ctx: &egui::Context, app: &mut PhotocraftApp) {
        let any_latched = self.latched != Latched::default();
        let tab_area =
            egui::Area::new(egui::Id::new("android_edge_tab")).order(egui::Order::Foreground).anchor(egui::Align2::RIGHT_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                // Lit while a key is latched (from the Modifier Keys panel), so it's never forgotten.
                let tab = egui::Button::new(if self.open { "›" } else { "‹" }).min_size(TAB_SIZE).selected(any_latched && !self.open);
                if ui.add(tab).on_hover_text("Edge keyboard").clicked() {
                    self.open = !self.open;
                }
            });
        if !self.open {
            return;
        }
        let max_h = (ctx.content_rect().height() - 24.0).max(200.0);
        let strip = egui::Area::new(egui::Id::new("android_edge_keys"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::RIGHT_CENTER, [-(TAB_SIZE.x + 6.0), 0.0])
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(max_h).show(ui, |ui| {
                        for (g, group) in BUTTONS.split(Option::is_none).enumerate() {
                            if g > 0 {
                                ui.separator();
                            }
                            egui::Grid::new(("android_edge_group", g)).num_columns(2).spacing([6.0, 6.0]).show(ui, |ui| {
                                for (i, (label, action)) in group.iter().flatten().enumerate() {
                                    // A command key that toggles a mode shows it the way the menu would.
                                    let on = self.is_on(*action)
                                        || matches!(action, Action::Command("view.pixelArtMode") if photocraft_ui_egui::pixel_mode::active(app));
                                    let button = egui::Button::new(*label).min_size(KEY_SIZE).selected(on);
                                    if ui.add(button).clicked() {
                                        self.press(*action, app);
                                        ctx.request_repaint();
                                    }
                                    if i % 2 == 1 {
                                        ui.end_row();
                                    }
                                }
                            });
                        }
                    });
                });
            });
        // A tap anywhere else closes the strip, as a popup would.
        let pressed_at = ctx.input(|i| if i.pointer.any_pressed() { i.pointer.press_origin() } else { None });
        if let Some(p) = pressed_at
            && closes_strip(p, tab_area.response.rect, strip.response.rect)
        {
            self.open = false;
        }
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
        let presses: Vec<bool> = raw
            .events
            .iter()
            .filter_map(|e| match e {
                Event::Key { key: Key::Enter, pressed, .. } => Some(*pressed),
                _ => None,
            })
            .collect();
        assert_eq!(presses, [true, false]);
        let mut raw = egui::RawInput::default();
        keys.raw_input(&mut raw);
        assert!(raw.events.is_empty());
    }

    #[test]
    fn a_press_outside_the_tab_and_strip_closes_it() {
        let tab = egui::Rect::from_min_size(egui::pos2(390.0, 300.0), egui::vec2(30.0, 88.0));
        let strip = egui::Rect::from_min_size(egui::pos2(180.0, 100.0), egui::vec2(200.0, 500.0));
        assert!(closes_strip(egui::pos2(50.0, 300.0), tab, strip));
        assert!(!closes_strip(egui::pos2(400.0, 320.0), tab, strip), "the tab toggles it itself");
        assert!(!closes_strip(egui::pos2(250.0, 300.0), tab, strip), "a key");
    }

    #[test]
    fn the_touch_toggles_flip_their_setting() {
        let mut keys = EdgeKeys::default();
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), photocraft_ui_egui::Services::default());
        keys.press(Action::Toggle(Setting::FingerDraws), &mut app);
        assert!(!keys.settings.finger_draws && keys.settings_changed);
        assert!(!keys.is_on(Action::Toggle(Setting::FingerDraws)));
    }

    #[test]
    fn every_command_button_runs_a_command_photocraft_has() {
        for (label, action) in BUTTONS.iter().flatten() {
            if let Action::Command(id) | Action::Run(id, _) = action {
                assert!(photocraft_ui_egui::menus::is_live(id), "{label}: {id} is no PhotoCraft command");
            }
        }
    }

    #[test]
    fn param_buttons_have_valid_json() {
        for (label, action) in BUTTONS.iter().flatten() {
            if let Action::Run(_, params) = action {
                assert!(serde_json::from_str::<serde_json::Value>(params).is_ok_and(|v| v.is_object()), "{label}");
            }
        }
    }

    #[test]
    fn modifier_keys_latch_until_tapped_again() {
        let mut keys = EdgeKeys::default();
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), photocraft_ui_egui::Services::default());
        keys.press(Action::Shift, &mut app);
        assert!(keys.is_on(Action::Shift) && app.ui.shell.sticky_shift, "held, and the Modifier Keys panel agrees");
        keys.press(Action::Shift, &mut app);
        assert!(!keys.is_on(Action::Shift) && !app.ui.shell.sticky_shift);
    }
}
