//! The song editor in the control window (#18). Editing, undo and the warnings themselves are
//! tested through the engine (`crates/core/tests/song_edit.rs`); these check the view sends the
//! right commands and shows what the state says.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{Queryable as _, by};
use serviceflow::ControlWindow;
use serviceflow_core::command::Command;
use serviceflow_core::edit::SongField;
use serviceflow_core::ids::SongId;
use serviceflow_core::state::State;
use serviceflow_runtime::Driver;
use serviceflow_storage_memory::MemoryStorage;
use serviceflow_testkit::opensong_data_root;

type Seen = Rc<RefCell<Arc<State>>>;

fn harness() -> (Harness<'static>, Seen) {
    let mut driver = Driver::new(MemoryStorage::from_data_root(&opensong_data_root()).unwrap());
    driver.send(Command::OpenSong(SongId::new("Amazing Grace HOF#50")));
    while !driver.is_idle() {
        driver.pump();
    }
    let mut window = ControlWindow::new(driver);
    let seen: Seen = Rc::new(RefCell::new(window.state()));
    let sink = Rc::clone(&seen);
    let mut harness = Harness::builder().with_size(egui::vec2(1600.0, 900.0)).allow_missing_glyphs().build_ui(move |ui| {
        window.ui(ui);
        *sink.borrow_mut() = window.state();
    });
    harness.run();
    harness.get_by_label("Edit song").click();
    harness.run();
    (harness, seen)
}

fn field<'h>(harness: &'h Harness<'static>, label: &'h str) -> egui_kittest::Node<'h> {
    let role = if label == "Lyrics" { Role::MultilineTextInput } else { Role::TextInput };
    harness.get(by().role(role).label(label))
}

#[test]
fn the_form_has_every_detail_and_the_lyrics_editor() {
    let (harness, seen) = harness();
    for label in [
        "Title",
        "Author",
        "Copyright",
        "CCLI number",
        "Hymn number",
        "Key",
        "Capo",
        "AKA",
        "Song theme",
        "User field 1",
        "User field 2",
        "User field 3",
        "Lyrics",
    ] {
        field(&harness, label);
    }
    let state = seen.borrow();
    let song = &state.song.as_ref().unwrap().song;
    assert_eq!(field(&harness, "Title").value().as_deref(), Some(song.title.as_str()));
    assert_eq!(field(&harness, "Lyrics").value().as_deref(), Some(song.lyrics.as_str()));
}

#[test]
fn typing_in_a_field_edits_the_song_and_the_buttons_undo_and_redo() {
    let (mut harness, seen) = harness();
    let original = seen.borrow().song.as_ref().unwrap().song.title.clone();
    field(&harness, "Capo").focus();
    harness.run();
    field(&harness, "Capo").type_text("3");
    harness.run();
    assert_eq!(SongField::Capo.get(&seen.borrow().song.as_ref().unwrap().song), "3");
    assert!(seen.borrow().draft().unwrap().is_modified());

    harness.get_by_label("Undo").click();
    harness.run();
    assert_eq!(SongField::Capo.get(&seen.borrow().song.as_ref().unwrap().song), "");
    assert_eq!(seen.borrow().song.as_ref().unwrap().song.title, original);

    harness.get_by_label("Redo").click();
    harness.run();
    assert_eq!(SongField::Capo.get(&seen.borrow().song.as_ref().unwrap().song), "3");
}

#[test]
fn the_undo_shortcut_is_the_engines_undo() {
    let (mut harness, seen) = harness();
    field(&harness, "Key").focus();
    harness.run();
    field(&harness, "Key").type_text("G");
    harness.run();
    assert_eq!(seen.borrow().song.as_ref().unwrap().song.key, "G");
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    harness.run();
    assert_eq!(seen.borrow().song.as_ref().unwrap().song.key, "");
}

#[test]
fn editing_the_lyrics_shows_warnings_and_updates_the_preview() {
    let (mut harness, seen) = harness();
    let before = harness.ctx.tex_manager().read().allocated().filter(|(_, meta)| meta.name.starts_with("slide thumbnail")).count();
    assert!(before > 0, "the preview draws the slides");
    assert!(harness.query_by_label_contains("Warnings").is_none());

    field(&harness, "Lyrics").focus();
    harness.run();
    field(&harness, "Lyrics").type_text("\nStray line");
    harness.run();

    let state = seen.borrow().clone();
    assert!(state.song.as_ref().unwrap().song.lyrics.ends_with("Stray line"));
    assert_eq!(state.song.as_ref().unwrap().song.warnings.len(), 1);
    harness.get_by_label("Warnings");
    harness.get_by_label_contains("starts with “S”");
}

#[test]
fn done_returns_to_the_slides() {
    let (mut harness, _) = harness();
    harness.get_by_label("Done").click();
    harness.run();
    harness.get_by_label("Edit song");
}
