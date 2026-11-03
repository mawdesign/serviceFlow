//! Operator shortcuts through the egui shell: section keys, set item keys, clicker keys, rebinding
//! and persistence. Jumping itself is tested at the engine seam (`crates/core/tests/jump.rs`).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use craft_keymap::CommandRef;
use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use serviceflow::ControlWindow;
use serviceflow::prefs::Prefs;
use serviceflow::shortcuts::section_jump;
use serviceflow_core::command::Command;
use serviceflow_core::ids::{SetId, SongId};
use serviceflow_core::state::{OutputMode, State};
use serviceflow_runtime::Driver;
use serviceflow_storage_memory::MemoryStorage;
use serviceflow_testkit::opensong_data_root;

type Seen = Rc<RefCell<Arc<State>>>;

fn window(song: &str) -> ControlWindow<MemoryStorage> {
    let mut driver = Driver::new(MemoryStorage::from_data_root(&opensong_data_root()).unwrap());
    driver.send(Command::OpenSong(SongId::new(song)));
    ControlWindow::new(driver)
}

fn harness(mut window: ControlWindow<MemoryStorage>) -> (Harness<'static>, Seen) {
    let seen: Seen = Rc::new(RefCell::new(window.state()));
    let sink = Rc::clone(&seen);
    let harness = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).allow_missing_glyphs().build_ui(move |ui| {
        window.ui(ui);
        *sink.borrow_mut() = window.state();
    });
    (harness, seen)
}

fn live(seen: &Seen) -> Option<(usize, String)> {
    seen.borrow().output.live.as_ref().map(|l| (l.index, l.slide.section.clone()))
}

#[test]
fn a_number_key_and_c_jump_to_that_section() {
    let (mut harness, seen) = harness(window("It Is Well With My Soul"));
    harness.run();
    harness.key_press(Key::Num3);
    harness.run();
    assert_eq!(live(&seen).map(|l| l.1), Some("V3".into()));
    harness.key_press(Key::C);
    harness.run();
    assert_eq!(live(&seen).map(|l| l.1), Some("C".into()));
    // A key for a section the song lacks does nothing.
    harness.key_press(Key::Num9);
    harness.run();
    assert_eq!(live(&seen).map(|l| l.1), Some("C".into()));
}

#[test]
fn a_number_key_reaches_a_greyed_section() {
    let (mut harness, seen) = harness(window("Amazing Grace HOF#50"));
    harness.run();
    harness.key_press(Key::Num3);
    harness.run();
    let (index, section) = live(&seen).unwrap();
    assert_eq!(section, "3");
    assert!(!seen.borrow().song.as_ref().unwrap().is_in_flow(index));
}

#[test]
fn plain_b_is_black_not_the_bridge() {
    let (mut harness, seen) = harness(window("It Is Well With My Soul"));
    harness.run();
    harness.key_press(Key::B);
    harness.run();
    assert_eq!(seen.borrow().output.mode, OutputMode::Black);
    assert_eq!(live(&seen), None);
}

#[test]
fn the_set_item_shortcuts_move_through_the_set() {
    let mut window = window("It Is Well With My Soul");
    window.driver().send(Command::OpenSet(SetId::new("Sunday Service")));
    let (mut harness, seen) = harness(window);
    harness.run();
    harness.get_all_by_role_and_label(Role::Button, "It Is Well With My Soul").next().unwrap().click();
    harness.run();
    let selected = |seen: &Seen| seen.borrow().set.as_ref().and_then(|s| s.selected);
    let first = selected(&seen);
    assert!(first.is_some());
    harness.key_press_modifiers(Modifiers::COMMAND, Key::ArrowRight);
    harness.run();
    assert_eq!(selected(&seen), first.map(|i| i + 1));
    harness.key_press_modifiers(Modifiers::COMMAND, Key::ArrowLeft);
    harness.run();
    assert_eq!(selected(&seen), first);
}

#[test]
fn the_logo_key_toggles_logo_mode() {
    let (mut harness, seen) = harness(window("It Is Well With My Soul"));
    harness.run();
    // The fixture's OpenSong settings start the output on the logo.
    assert_eq!(seen.borrow().output.mode, OutputMode::Logo);
    harness.key_press(Key::L);
    harness.run();
    assert_eq!(seen.borrow().output.mode, OutputMode::Normal);
    harness.key_press(Key::L);
    harness.run();
    assert_eq!(seen.borrow().output.mode, OutputMode::Logo);
}

#[test]
fn page_keys_work_while_the_search_box_has_the_keyboard_but_letters_and_digits_do_not() {
    let (mut harness, seen) = harness(window("It Is Well With My Soul"));
    harness.run();
    let mode = seen.borrow().output.mode;
    harness.get_by_role(Role::TextInput).focus();
    harness.run();
    harness.key_press(Key::Num2);
    harness.key_press(Key::B);
    harness.run();
    assert_eq!(live(&seen), None);
    assert_eq!(seen.borrow().output.mode, mode);
    harness.key_press(Key::PageDown);
    harness.run();
    assert_eq!(live(&seen).map(|l| l.0), Some(0));
    harness.key_press(Key::PageDown);
    harness.run();
    assert_eq!(live(&seen).map(|l| l.0), Some(1));
    harness.key_press(Key::PageUp);
    harness.run();
    assert_eq!(live(&seen).map(|l| l.0), Some(0));
}

#[test]
fn escape_does_nothing_to_the_output_even_while_the_search_box_has_the_keyboard() {
    let (mut harness, seen) = harness(window("It Is Well With My Soul"));
    harness.run();
    harness.key_press(Key::Num2);
    harness.run();
    harness.key_press(Key::W);
    harness.run();
    assert_eq!(seen.borrow().output.mode, OutputMode::Blank);
    harness.key_press(Key::Escape);
    harness.run();
    assert_eq!(seen.borrow().output.mode, OutputMode::Blank);
    assert_eq!(live(&seen).map(|l| l.1), Some("V2".into()));
}

fn prefs_with(command: &str, keys: &[&str]) -> Prefs {
    let mut prefs = Prefs::default();
    prefs.shortcuts.insert(command, keys.iter().map(|k| k.to_string()).collect());
    prefs
}

#[test]
fn remembered_shortcuts_replace_the_defaults() {
    let prefs = prefs_with("slide.next", &["N"]);
    let (mut harness, seen) = harness(window("It Is Well With My Soul").with_prefs(prefs, |_| {}));
    harness.run();
    harness.key_press(Key::ArrowRight);
    harness.run();
    assert_eq!(live(&seen), None, "the old key no longer works");
    harness.key_press(Key::N);
    harness.run();
    assert_eq!(live(&seen).map(|l| l.0), Some(0));
}

#[test]
fn rebinding_saves_the_change_and_a_new_session_starts_with_it() {
    let saved: Rc<RefCell<Vec<Prefs>>> = Rc::default();
    let sink = Rc::clone(&saved);
    let mut first = window("It Is Well With My Soul").with_prefs(Prefs::default(), move |p| sink.borrow_mut().push(p.clone()));
    assert!(first.rebind(&section_jump("C"), &["K"]));
    assert!(!first.rebind(&CommandRef::new("no.such.command"), &["K"]));
    let prefs = saved.borrow().last().cloned().expect("the change is saved");
    assert!(!prefs.shortcuts.is_empty());

    // Through the settings file and back.
    let text = serde_json::to_string(&prefs).unwrap();
    let restored: Prefs = serde_json::from_str(&text).unwrap();
    assert_eq!(restored, prefs);

    let (mut harness, seen) = harness(window("It Is Well With My Soul").with_prefs(restored, |_| {}));
    harness.run();
    harness.key_press(Key::C);
    harness.run();
    assert_eq!(live(&seen), None, "C was taken away from the chorus");
    harness.key_press(Key::K);
    harness.run();
    assert_eq!(live(&seen).map(|l| l.1), Some("C".into()));
}

#[test]
fn the_shortcuts_dialog_lists_the_live_shortcuts() {
    let (mut harness, _seen) = harness(window("It Is Well With My Soul"));
    harness.run();
    harness.get_by_label("Keyboard shortcuts…").click();
    harness.run();
    assert!(harness.get_all_by_label_contains("Next set item").count() >= 1);
}
