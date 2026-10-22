//! Reject oversized numeric events before layout while preserving DragValue behavior.

use egui::{Event, ImeEvent, Key, Modifiers, accesskit::Role};
use egui_kittest::{Harness, kittest::Queryable};

const LIMIT: usize = super::NUMERIC_INPUT_LIMIT;
type Fields = (f32, String);

fn fields() -> Harness<'static, Fields> {
    let mut h = Harness::builder().with_size(egui::vec2(400.0, 180.0)).build_ui_state(
        |ui, (number, text): &mut Fields| {
            super::value_field(ui, number, 0.01..=12800.0, "%", 90.0);
            ui.add(egui::TextEdit::multiline(text).hint_text("Document text"));
        },
        (100.0, String::new()),
    );
    h.run();
    h
}

fn focused() -> Harness<'static, Fields> {
    let mut h = fields();
    h.get_by_role(Role::SpinButton).click();
    h.run();
    h
}

fn numeric_text(h: &Harness<'_, Fields>) -> String {
    h.query_by_role(Role::TextInput).or_else(|| h.query_by_role(Role::SpinButton)).expect("numeric editor").value().expect("numeric text")
}

fn select_all(h: &mut Harness<'_, Fields>) {
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run();
}

fn assert_bounded(h: &Harness<'_, Fields>) {
    fn longest(shape: &egui::Shape) -> usize {
        match shape {
            egui::Shape::Text(text) => text.galley.job.text.chars().count(),
            egui::Shape::Vec(shapes) => shapes.iter().map(longest).max().unwrap_or(0),
            _ => 0,
        }
    }
    let drawn = h.output().shapes.iter().map(|shape| longest(&shape.shape)).max().unwrap_or(0);
    assert!(drawn <= LIMIT, "numeric input laid out {drawn} characters; expected at most {LIMIT}");
    let retained = numeric_text(h).chars().count();
    assert!(retained <= LIMIT, "numeric editor retained {retained} characters; expected at most {LIMIT}");
}

#[test]
fn oversized_numeric_text_paste_and_ime_are_bounded_before_layout() {
    for (index, event) in [
        Event::Text("9".repeat(16 * 1024)),
        Event::Paste("x".repeat(16 * 1024)),
        Event::Ime(ImeEvent::Preedit { text: "é".repeat(8 * 1024), active_range_chars: None }),
        Event::Ime(ImeEvent::Commit("é".repeat(8 * 1024))),
    ]
    .into_iter()
    .enumerate()
    {
        let mut h = focused();
        select_all(&mut h);
        if matches!(&event, Event::Ime(_)) {
            // egui 0.36 assigns IME events to the focused widget; Enabled is deprecated and
            // ignored. Prove both live composition and commitment reach this field first.
            h.event(Event::Ime(ImeEvent::Preedit { text: "に".into(), active_range_chars: Some(0..1) }));
            h.run();
            assert_eq!(numeric_text(&h), "に", "short IME composition reaches the numeric editor");
            h.event(Event::Ime(ImeEvent::Commit("é".into())));
            h.run();
            assert_eq!(numeric_text(&h), "é", "IME commit replaces its composition");
            select_all(&mut h);
        }
        h.event(event);
        h.run();
        if let Some(dir) = std::env::var_os("CRAFT_NUMERIC_INPUT_FIXTURES") {
            let dir = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&dir).expect("create numeric fixture directory");
            h.render().expect("render numeric fixture").save(dir.join(format!("event-{index}.png"))).expect("save numeric fixture");
        }
        assert_bounded(&h);
        assert_eq!(h.state().0, 100.0, "invalid input leaves the numeric value unchanged");
        h.run();
        assert_bounded(&h);
    }
}

#[test]
fn numeric_replacement_clear_and_undo_preserve_unicode_input() {
    let mut h = focused();
    select_all(&mut h);
    for _ in 0..4 {
        h.event(Event::Text("é".repeat(16)));
        h.run();
        assert_bounded(&h);
    }
    let retained = numeric_text(&h);
    assert!(!retained.is_empty(), "ordinary short inputs still enter the editor");
    // Let egui's text undoer store the stable text before clearing it.
    h.input_mut().time = Some(h.ctx.input(|i| i.time) + 2.0);
    h.run();
    select_all(&mut h);
    h.key_press(Key::Backspace);
    h.run();
    assert_eq!(numeric_text(&h), "");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    assert_eq!(numeric_text(&h), retained, "undo restores only the bounded input");
    assert_bounded(&h);
    select_all(&mut h);
    h.event(Event::Text("−4+6.5".into()));
    h.run();
    assert_eq!(numeric_text(&h), "−4+6.5", "replacement accepts Unicode arithmetic");
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(h.state().0, 2.5);
}

#[test]
fn numeric_input_limit_does_not_truncate_document_text() {
    for switch_from_numeric in [false, true] {
        let mut h = if switch_from_numeric { focused() } else { fields() };
        let text = "é".repeat(8 * 1024);
        h.get_by_role(Role::MultilineTextInput).click();
        h.run();
        h.event(Event::Paste(text.clone()));
        h.run();
        assert_eq!(h.state().1, text, "document paste preserved when switching from numeric: {switch_from_numeric}");
        assert_eq!(h.get_by_role(Role::MultilineTextInput).value().as_deref(), Some(text.as_str()));
        assert_eq!(h.state().0, 100.0);
    }
}

#[test]
fn numeric_event_limit_counts_unicode_characters_and_rejects_without_truncating() {
    let mut h = focused();
    select_all(&mut h);
    let boundary = "é".repeat(LIMIT);
    h.event(Event::Paste(boundary.clone()));
    h.run();
    assert_eq!(numeric_text(&h), boundary);
    select_all(&mut h);
    h.event(Event::Text("9".repeat(LIMIT + 1)));
    h.run();
    assert_eq!(numeric_text(&h), boundary, "oversized input leaves the selection intact");
    h.event(Event::Text("42".into()));
    h.run();
    assert_eq!(h.state().0, 42.0);
}

#[test]
fn numeric_parser_rejects_oversized_accumulated_text_before_normalizing() {
    let mut h = focused();
    select_all(&mut h);
    // Each event is under the cap, but the resulting input is too long. Normalizing first
    // would hide all the spaces and apply 2. DragValue still owns its editable text.
    h.event(Event::Text(" ".repeat(LIMIT / 2)));
    h.run();
    h.event(Event::Text(format!("{}2", " ".repeat(LIMIT / 2))));
    h.run();
    assert_eq!(numeric_text(&h).chars().count(), LIMIT + 1);
    assert_eq!(h.state().0, 100.0, "oversized plain input must not apply live");
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(h.state().0, 100.0, "oversized input must not apply on commit");
}

#[test]
fn tab_and_paste_in_one_frame_filter_only_the_receiving_numeric_field() {
    let mut h = focused();
    let text = "x".repeat(16 * 1024);
    h.key_press(Key::Tab);
    h.event(Event::Paste(text.clone()));
    h.run();
    assert_eq!(h.state().1, text, "Tab to document text preserves the paste");
    // Put the numeric field after a text field to exercise incoming forward Tab too.
    // egui deliberately delays backwards traversal until the next frame.
    let mut h = Harness::builder().with_size(egui::vec2(400.0, 180.0)).build_ui_state(
        |ui, (number, text): &mut Fields| {
            ui.add(egui::TextEdit::multiline(text).hint_text("Document text"));
            super::value_field(ui, number, 0.01..=12800.0, "%", 90.0);
        },
        (100.0, String::new()),
    );
    h.run();
    h.get_by_role(Role::MultilineTextInput).click();
    h.run();
    h.key_press(Key::Tab);
    h.event(Event::Paste("9".repeat(16 * 1024)));
    h.run();
    assert_eq!(h.state().0, 100.0);
    assert!(h.state().1.is_empty());
    assert_bounded(&h);
}

/// A letter that can't be part of a number, a sum or a unit never lands in the field (an
/// on-screen keyboard offers them all); digits, arithmetic and unit names still do.
#[test]
fn number_fields_take_only_what_a_number_is_made_of() {
    let mut h = focused();
    select_all(&mut h);
    for text in ["1", "q", "z", "H", "2", "*", "3"] {
        h.event(Event::Text(text.into()));
        h.run();
    }
    assert_eq!(numeric_text(&h), "12*3");
    h.event(Event::Paste("zq9!".into()));
    h.run();
    assert_eq!(numeric_text(&h), "12*39");
}

#[test]
fn numeric_characters_follow_the_field() {
    use crate::numeric_expression::Units;
    let plain = Units { unit: None, suffix: "", dpi: 72.0, ppi: 72.0 };
    let degrees = Units { suffix: "°", ..plain };
    let px = Units { unit: Some(photocraft_engine::prefs::Unit::Pixels), suffix: "px", ..plain };
    for c in ['0', '9', '.', '-', '/', '(', 'p', 'i', 'e'] {
        assert!(super::numeric_char(c, plain), "{c}");
    }
    for c in ['q', 'z', 'x', 'm', '!'] {
        assert!(!super::numeric_char(c, plain), "{c} in a unitless field");
    }
    assert!(super::numeric_char('°', degrees) && super::numeric_char('１', plain), "an input method's characters are its own");
    for c in ['m', 'c', 'x', 't', 'n'] {
        assert!(super::numeric_char(c, px), "{c} in a px field");
    }
    assert!(!super::numeric_char('q', px));
}

/// A touch shell can tell a number field is being edited (to show the number pad), and only then.
#[test]
fn a_focused_number_field_is_known_as_one() {
    // (number, text, number field focused at the end of the frame)
    type State = (f32, String, bool);
    let mut h = Harness::builder().with_size(egui::vec2(400.0, 180.0)).build_ui_state(
        |ui, (number, text, numeric): &mut State| {
            super::value_field(ui, number, 0.01..=12800.0, "%", 90.0);
            ui.add(egui::TextEdit::singleline(text).hint_text("Name"));
            *numeric = super::numeric_focused(ui.ctx());
        },
        (100.0, String::new(), false),
    );
    h.run();
    assert!(!h.state().2, "nothing is focused");
    h.get_by_role(Role::SpinButton).click();
    h.run();
    assert!(h.state().2, "the number field is being edited");
    h.get_by_role(Role::TextInput).click();
    h.run();
    assert!(!h.state().2, "a text field is not a number field");
}
