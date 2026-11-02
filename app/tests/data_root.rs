//! Choosing, remembering and switching the data root (desktop only), through the real
//! filesystem storage and a stubbed folder picker.
#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};

use craft_dialogs::Answer;
use craft_settings::Location;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use serviceflow::data_root::{DesktopApp, RootProblem};
use serviceflow_core::command::Command;
use serviceflow_core::ids::{ContentHash, SongId};
use serviceflow_testkit::{fixture_song, opensong_data_root, temp_dir};

/// Run frames until the app settles.
fn settle(harness: &mut Harness<'_>) {
    for _ in 0..4 {
        harness.step();
    }
}

fn app(harness_ctx: &egui::Context, config: &Path, cli_root: Option<PathBuf>) -> DesktopApp {
    DesktopApp::new(harness_ctx, Location::Dir(config.to_path_buf()), cli_root)
}

/// A data root with one song in it.
fn root_with_song(name: &str) -> (serviceflow_testkit::TempDir, PathBuf) {
    let dir = temp_dir();
    std::fs::create_dir_all(dir.path().join("Songs")).unwrap();
    std::fs::write(dir.path().join("Songs").join(name), "<song><title>x</title></song>").unwrap();
    let path = dir.path().to_path_buf();
    (dir, path)
}

#[test]
fn first_run_asks_for_a_folder_then_remembers_it_for_the_next_launch() {
    let config = temp_dir();
    let (_keep, root) = root_with_song("Only Song");
    let ctx = egui::Context::default();

    let mut first = app(&ctx, config.path(), None);
    let answer = root.clone();
    first.dialogs().set_override(Some(Box::new(move |_| Answer::Paths(vec![answer.clone()]))));
    assert!(first.data_root().is_none());
    let mut harness = Harness::builder().allow_missing_glyphs().build_ui(|ui| first.ui(ui, None));
    settle(&mut harness);
    harness.get_by_label_contains("Data root:");
    drop(harness);
    assert_eq!(first.data_root(), Some(root.as_path()));
    drop(first);

    // A second launch opens straight into it: a picker that was asked would panic.
    let mut second = app(&ctx, config.path(), None);
    second.dialogs().set_override(Some(Box::new(|_| panic!("the folder picker must not open"))));
    let mut harness = Harness::builder().allow_missing_glyphs().build_ui(|ui| second.ui(ui, None));
    settle(&mut harness);
    drop(harness);
    assert_eq!(second.data_root(), Some(root.as_path()));
}

#[test]
fn the_operator_can_switch_to_another_data_root_and_it_is_remembered() {
    let config = temp_dir();
    let (_a, root_a) = root_with_song("Song A");
    let (_b, root_b) = root_with_song("Song B");
    let ctx = egui::Context::default();

    let mut app1 = app(&ctx, config.path(), None);
    let picks = std::cell::RefCell::new(vec![root_b.clone(), root_a.clone()]);
    app1.dialogs().set_override(Some(Box::new(move |_| Answer::Paths(vec![picks.borrow_mut().pop().unwrap()]))));
    let mut harness = Harness::builder().allow_missing_glyphs().build_ui(|ui| app1.ui(ui, None));
    settle(&mut harness);
    harness.get_by_label("Change data root…").click();
    settle(&mut harness);
    drop(harness);
    assert_eq!(app1.data_root(), Some(root_b.as_path()));
    drop(app1);

    let app2 = app(&ctx, config.path(), None);
    assert_eq!(app2.data_root(), Some(root_b.as_path()));
}

#[test]
fn a_folder_without_songs_is_refused_and_the_current_root_stays_open() {
    let config = temp_dir();
    let (_a, root) = root_with_song("Song A");
    let not_root = temp_dir();
    let ctx = egui::Context::default();

    let mut app = app(&ctx, config.path(), Some(root.clone()));
    let bad = not_root.path().to_path_buf();
    app.dialogs().set_override(Some(Box::new(move |_| Answer::Paths(vec![bad.clone()]))));
    app.choose_data_root();
    let mut harness = Harness::builder().allow_missing_glyphs().build_ui(|ui| app.ui(ui, None));
    settle(&mut harness);
    harness.get_by_label_contains("has no Songs folder");
    drop(harness);
    assert_eq!(app.data_root(), Some(root.as_path()));
    assert!(matches!(app.problem(), Some(RootProblem::NotADataRoot(_))));
    // Nothing was remembered.
    drop(app);
    assert!(DesktopApp::new(&ctx, Location::Dir(config.path().to_path_buf()), None).data_root().is_none());
}

#[test]
fn a_remembered_root_that_has_gone_is_reported_and_the_picker_opens_again() {
    let config = temp_dir();
    let (keep, root) = root_with_song("Song A");
    let ctx = egui::Context::default();
    let mut first = app(&ctx, config.path(), None);
    first.use_data_root(root.clone());
    drop(first);
    drop(keep); // the folder disappears

    let mut second = app(&ctx, config.path(), None);
    assert!(matches!(second.problem(), Some(RootProblem::Missing(p)) if *p == root));
    let (_other, other_root) = root_with_song("Song B");
    second.dialogs().set_override(Some(Box::new(move |_| Answer::Paths(vec![other_root.clone()]))));
    let mut harness = Harness::builder().allow_missing_glyphs().build_ui(|ui| second.ui(ui, None));
    settle(&mut harness);
    drop(harness);
    assert!(second.data_root().is_some());
    assert!(second.problem().is_none());
}

#[test]
fn an_opensong_folder_opens_songs_straight_from_disk_and_records_their_hash() {
    let config = temp_dir();
    let ctx = egui::Context::default();
    let mut app = app(&ctx, config.path(), Some(opensong_data_root()));
    let window = app.window().expect("the fixtures folder is a data root");
    window.driver().send(Command::OpenSong(SongId::new("It Is Well With My Soul")));
    let mut harness = Harness::builder().allow_missing_glyphs().build_ui(|ui| app.ui(ui, None));
    settle(&mut harness);
    // The library entry and the open song's heading.
    assert_eq!(harness.get_all_by_label("It Is Well With My Soul").count(), 2);
    drop(harness);
    let state = app.window().unwrap().driver().snapshot();
    let open = state.song.as_ref().expect("the song opened from disk");
    assert_eq!(open.hash, ContentHash::of(&fixture_song("It Is Well With My Soul")));
}
