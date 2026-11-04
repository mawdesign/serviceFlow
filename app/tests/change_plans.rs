//! Change plans in the control window (#20), over a real data-root folder: the operator sees
//! every file before anything is written, and applying writes them. The plan rules themselves
//! are engine-tested (`crates/core/tests/change_plans.rs`).
#![cfg(not(target_arch = "wasm32"))]

use std::path::PathBuf;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{Queryable as _, by};
use serviceflow::ControlWindow;
use serviceflow_core::command::Command;
use serviceflow_core::ids::SongId;
use serviceflow_runtime::Driver;
use serviceflow_storage_fs::FsStorage;
use serviceflow_testkit::{fixture_song, opensong_data_root, temp_dir};

const NAME: &str = "It Is Well With My Soul";

/// A data root with one song and the "Sunday Service" set that uses it, the song open.
fn harness() -> (Harness<'static>, PathBuf) {
    let dir = temp_dir();
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("Songs")).unwrap();
    std::fs::create_dir_all(root.join("Sets")).unwrap();
    std::fs::write(root.join("Songs").join(NAME), fixture_song(NAME)).unwrap();
    std::fs::copy(opensong_data_root().join("Sets/Sunday Service"), root.join("Sets/Sunday Service")).unwrap();
    let mut driver = Driver::new(FsStorage::open(&root).unwrap().keeping_alive(dir));
    driver.send(Command::OpenSong(SongId::new(NAME)));
    while !driver.is_idle() {
        driver.pump();
    }
    let mut window = ControlWindow::new(driver);
    let mut harness = Harness::builder().with_size(egui::vec2(1600.0, 900.0)).allow_missing_glyphs().build_ui(move |ui| window.ui(ui));
    harness.run();
    (harness, root)
}

fn names(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

#[test]
fn renaming_a_song_file_lists_the_set_and_writes_only_after_apply() {
    let (mut harness, root) = harness();
    let set_before = std::fs::read(root.join("Sets/Sunday Service")).unwrap();
    harness.get_by_label("Rename or move file…").click();
    harness.run();
    let label = "New file name (folder/name):";
    harness.get(by().role(Role::TextInput).label(label)).focus();
    harness.run();
    harness.get(by().role(Role::TextInput).label(label)).type_text(" (moved)");
    harness.run();
    harness.get_by_label("Rename").click();
    harness.run();
    harness.run();

    harness.get_by_label("• Sets/Sunday Service: update the song item");
    harness.get_by_label(&format!("• Songs/{NAME}: move to Songs/{NAME} (moved)"));
    assert_eq!(names(&root.join("Songs")), [NAME]);
    assert_eq!(std::fs::read(root.join("Sets/Sunday Service")).unwrap(), set_before);

    harness.get_by_label("Apply changes").click();
    harness.run();
    harness.run();
    assert_eq!(names(&root.join("Songs")), [format!("{NAME} (moved)")]);
    assert_eq!(names(&root.join("Sets")), ["Sunday Service"]);
    let set = String::from_utf8(std::fs::read(root.join("Sets/Sunday Service")).unwrap()).unwrap();
    assert!(set.contains(&format!("<slide_group name=\"{NAME} (moved)\" type=\"song\" path=\"\" presentation=\"V1 C V4 C\">")), "{set}");
    assert!(harness.query_by_label("Apply changes").is_none());
}

#[test]
fn a_new_song_shows_its_file_name_before_it_is_created() {
    let (mut harness, root) = harness();
    harness.get_by_label("New song…").click();
    harness.run();
    harness.get(by().role(Role::TextInput).label("Title:")).focus();
    harness.run();
    harness.get(by().role(Role::TextInput).label("Title:")).type_text("Be Still: My Soul");
    harness.run();
    harness.get_by_label("Create").click();
    harness.run();
    harness.run();
    harness.get_by_label("• Songs/Be Still My Soul.xml: create");
    assert_eq!(names(&root.join("Songs")), [NAME]);
    harness.get_by_label("Apply changes").click();
    harness.run();
    harness.run();
    assert_eq!(names(&root.join("Songs")), ["Be Still My Soul.xml", NAME]);
}

#[test]
fn deleting_a_song_a_set_uses_warns_and_cancel_keeps_it() {
    let (mut harness, root) = harness();
    harness.get_by_label("Delete song…").click();
    harness.run();
    harness.run();
    harness.get_by_label("These sets still use the song and will show it as missing: “Sunday Service”");
    harness.get_by_label("Cancel").click();
    harness.run();
    assert_eq!(names(&root.join("Songs")), [NAME]);

    harness.get_by_label("Delete song…").click();
    harness.run();
    harness.run();
    harness.get_by_label("Apply changes").click();
    harness.run();
    harness.run();
    assert!(names(&root.join("Songs")).is_empty());
}
