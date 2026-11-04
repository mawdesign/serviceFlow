//! Blanks between set items in the control window: a blank thumbnail after the item's last slide
//! only when `blanks` is on, clickable, and never in the set panel.

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{Queryable as _, by};
use serviceflow::ControlWindow;
use serviceflow_core::command::{Command, ImportTrigger};
use serviceflow_core::ids::SetId;
use serviceflow_core::state::OutputMode;
use serviceflow_runtime::Driver;
use serviceflow_storage::{Expect, SetRepository as _, SettingsId, SettingsRepository as _, SongId, SongRepository as _};
use serviceflow_storage_memory::MemoryStorage;
use serviceflow_testkit::block_on;

const SONG: &str = "<song><lyrics>[V1]\n first\n[V2]\n second</lyrics></song>";
const SET: &str = r#"<set name="S"><slide_groups><slide_group name="A" type="song"/><slide_group name="B" type="song"/></slide_groups></set>"#;

fn window(blanks: bool) -> ControlWindow<MemoryStorage> {
    let storage = MemoryStorage::new();
    for name in ["A", "B"] {
        block_on(storage.write_song(&SongId::new(name), SONG.as_bytes().to_vec(), Expect::Any)).unwrap();
    }
    block_on(storage.write_set(&SetId::new("S"), SET.as_bytes().to_vec(), Expect::Any)).unwrap();
    let present = format!(r#"<present_settings><style blanks="{blanks}"/></present_settings>"#);
    block_on(storage.write_settings(&SettingsId("PresentSettings".into()), present.into_bytes(), Expect::Any)).unwrap();
    let mut window = ControlWindow::new(Driver::new(storage));
    window.driver().send(Command::ImportOpenSongSettings { trigger: ImportTrigger::FirstRun });
    window.driver().send(Command::OpenSet(SetId::new("S")));
    window.driver().pump();
    window.driver().send(Command::SelectItem(0));
    window.driver().pump();
    window
}

fn blank_thumbnails(harness: &Harness<'_>) -> usize {
    harness.ctx.tex_manager().read().allocated().filter(|(_, meta)| meta.name == "blank thumbnail").count()
}

#[test]
fn a_blank_thumbnail_follows_the_last_slide_and_clicking_it_shows_the_blank() {
    let mut window = window(true);
    let mut harness = Harness::builder().allow_missing_glyphs().with_size(egui::vec2(1400., 900.)).build_ui(|ui| window.ui(ui));
    harness.run();
    assert_eq!(blank_thumbnails(&harness), 1);
    harness.get(by().role(Role::Image).label("Blank")).click();
    harness.run();
    drop(harness);
    let state = window.state();
    assert_eq!(state.output.mode, OutputMode::Blank);
    assert!(state.on_blank_after());
    // The set itself still has two items and no blank among them.
    assert_eq!(state.set.as_ref().unwrap().set.items.len(), 2);
}

#[test]
fn there_is_no_blank_thumbnail_with_blanks_off_or_after_the_last_item() {
    let mut off = window(false);
    let mut harness = Harness::builder().allow_missing_glyphs().with_size(egui::vec2(1400., 900.)).build_ui(|ui| off.ui(ui));
    harness.run();
    assert_eq!(blank_thumbnails(&harness), 0);
    drop(harness);

    let mut last = window(true);
    last.driver().send(Command::SelectItem(1));
    last.driver().pump();
    let mut harness = Harness::builder().allow_missing_glyphs().with_size(egui::vec2(1400., 900.)).build_ui(|ui| last.ui(ui));
    harness.run();
    assert_eq!(blank_thumbnails(&harness), 0);
}
