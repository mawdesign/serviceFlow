//! Tidy library in the control window (#21), over a real data-root folder: the operator sees the
//! list before anything is written; applying rewrites the files; a second tidy has nothing to do.
//! The rules are engine-tested (`crates/core/tests/tidy.rs`).
#![cfg(not(target_arch = "wasm32"))]

use std::path::Path;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use serviceflow::ControlWindow;
use serviceflow_runtime::Driver;
use serviceflow_storage_fs::FsStorage;
use serviceflow_testkit::{opensong_data_root, temp_dir};

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn files(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut all = Vec::new();
    for folder in ["Songs", "Sets"] {
        let mut entries: Vec<_> = std::fs::read_dir(root.join(folder)).unwrap().map(|e| e.unwrap().path()).collect();
        entries.sort();
        all.extend(entries.into_iter().filter(|p| p.is_file()).map(|p| (p.display().to_string(), std::fs::read(&p).unwrap())));
    }
    all
}

#[test]
fn tidy_library_shows_the_list_then_rewrites_on_apply_and_a_second_tidy_is_empty() {
    let dir = temp_dir();
    let root = dir.path().to_path_buf();
    for folder in ["Songs", "Sets", "Settings"] {
        copy_dir(&opensong_data_root().join(folder), &root.join(folder));
    }
    let driver = Driver::new(FsStorage::open(&root).unwrap().keeping_alive(dir));
    let mut window = ControlWindow::new(driver);
    let mut harness = Harness::builder().with_size(egui::vec2(1600.0, 900.0)).allow_missing_glyphs().build_ui(move |ui| window.ui(ui));
    harness.run();
    let before = files(&root);

    harness.get_by_label("Tidy library…").click();
    harness.run();
    harness.run();
    harness.get_by_label("These files will be rewritten:");
    assert_eq!(files(&root), before, "the list writes nothing");

    harness.get_by_label("Apply changes").click();
    harness.run();
    harness.run();
    assert_ne!(files(&root), before);
    assert!(harness.query_by_label("Apply changes").is_none());

    let tidied = files(&root);
    harness.get_by_label("Tidy library…").click();
    harness.run();
    harness.run();
    harness.get_by_label("Nothing to tidy: every file is already in canonical form.");
    assert!(harness.query_by_label("Apply changes").is_none());
    harness.get_by_label("Cancel").click();
    harness.run();
    assert_eq!(files(&root), tidied);
}
