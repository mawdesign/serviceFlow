//! Saving from the song editor (#19), over a real data-root folder: the Save button writes the
//! canonical form crash-safely (no staging files left), and a file changed on disk shows the
//! conflict choices instead of being overwritten. Save rules themselves are engine-tested
//! (`crates/core/tests/song_save.rs`).
#![cfg(not(target_arch = "wasm32"))]

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _, by};
use serviceflow::ControlWindow;
use serviceflow_core::command::Command;
use serviceflow_core::ids::SongId;
use serviceflow_core::state::State;
use serviceflow_runtime::Driver;
use serviceflow_storage_fs::FsStorage;
use serviceflow_testkit::{fixture_song, temp_dir};

const NAME: &str = "God Defend NZ - National Anthem";

type Seen = Rc<RefCell<Arc<State>>>;

/// A data root holding one fixture song, open in the editor.
fn harness() -> (Harness<'static>, Seen, PathBuf) {
    let dir = temp_dir();
    let songs = dir.path().join("Songs");
    std::fs::create_dir_all(&songs).unwrap();
    std::fs::write(songs.join(NAME), fixture_song(NAME)).unwrap();
    let path = songs.join(NAME);
    let mut driver = Driver::new(FsStorage::open(dir.path()).unwrap().keeping_alive(dir));
    driver.send(Command::OpenSong(SongId::new(NAME)));
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
    (harness, seen, path)
}

fn type_key(harness: &mut Harness<'static>, text: &str) {
    let key = harness.get(by().role(Role::TextInput).label("Key"));
    key.focus();
    harness.run();
    harness.get(by().role(Role::TextInput).label("Key")).type_text(text);
    harness.run();
}

#[test]
fn save_writes_the_canonical_file_and_leaves_no_staging_files() {
    let (mut harness, seen, path) = harness();
    assert!(harness.get_by_label("Save").accesskit_node().is_disabled(), "nothing to save yet");
    type_key(&mut harness, "G");
    harness.get_by_label("Save").click();
    harness.run();
    harness.run();

    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("<key>G</key>\r\n"), "{written}");
    assert!(written.contains("<presentation>V6 V1 V2 V3 V4 V5</presentation>"), "{written}");
    assert!(written.contains("<lyrics>[V1]\r\n"), "{written}");
    let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(leftovers, [std::ffi::OsString::from(NAME)]);
    assert!(!seen.borrow().draft().unwrap().is_modified());
    assert!(harness.query_by_label("Edited, not saved").is_none());
}

#[test]
fn a_file_changed_on_disk_offers_overwrite_reload_or_copy() {
    let (mut harness, seen, path) = harness();
    type_key(&mut harness, "G");
    std::fs::write(&path, "<song><title>Changed elsewhere</title></song>").unwrap();
    harness.get_by_label("Save").click();
    harness.run();
    harness.run();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "<song><title>Changed elsewhere</title></song>");
    harness.get_by_label_contains("changed on disk");
    harness.get_by_label("Overwrite");
    harness.get_by_label("Save as a copy");

    harness.get_by_label("Reload (discard my edits)").click();
    harness.run();
    harness.run();
    assert_eq!(seen.borrow().song.as_ref().unwrap().song.title, "Changed elsewhere");
    assert!(harness.query_by_label_contains("changed on disk").is_none());
}
