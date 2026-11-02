//! Pixel-free smoke tests of the egui shell, through the accessibility tree.

use egui_kittest::Harness;
use egui_kittest::kittest::{Queryable as _, by};
use serviceflow::ControlWindow;
use serviceflow_core::command::Command;
use serviceflow_core::ids::SongId;
use serviceflow_runtime::Driver;
use serviceflow_storage_memory::MemoryStorage;
use serviceflow_testkit::opensong_data_root;

fn empty_window() -> ControlWindow<MemoryStorage> {
    ControlWindow::new(Driver::new(MemoryStorage::new()))
}

#[test]
fn the_empty_control_window_shows_the_app_name_and_that_no_song_is_open() {
    let mut window = empty_window();
    let harness = Harness::new_ui(|ui| window.ui(ui));
    harness.get_by_label("serviceFlow");
    harness.get_by_label("No song is open.");
}

#[test]
fn the_control_window_shows_the_loaded_songs_slides_as_rendered_thumbnails() {
    let mut driver = Driver::new(MemoryStorage::from_data_root(&opensong_data_root()).unwrap());
    driver.send(Command::OpenSong(SongId::new("It Is Well With My Soul")));
    let mut window = ControlWindow::new(driver);
    let harness = Harness::new_ui(|ui| window.ui(ui));

    harness.get_by_label("It Is Well With My Soul");
    // Twelve slides in presentation order: V1 has two, the chorus appears four times. Each
    // thumbnail is an image whose alternative text is the slide's lyrics.
    assert_eq!(harness.get_all_by_label_contains("It is well (it is well)").count(), 4);
    assert_eq!(harness.get_all_by_label_contains("When peace like a river").count(), 1);
    assert_eq!(harness.get_all_by_label_contains("Even so, it is well with my soul...").count(), 1);
    let chorus_images = by().role(egui::accesskit::Role::Image).label_contains("It is well (it is well)");
    assert_eq!(harness.get_all(chorus_images).count(), 4);
    // The thumbnails are frames drawn by the slide renderer, uploaded as textures.
    let textures = harness.ctx.tex_manager();
    let uploaded = textures.read().allocated().filter(|(_, meta)| meta.name.starts_with("slide thumbnail")).count();
    assert_eq!(uploaded, 12);
}

#[test]
fn the_control_window_is_titled_serviceflow() {
    assert_eq!(serviceflow::control_viewport().title.as_deref(), Some("serviceFlow"));
}

#[test]
fn user_facing_strings_are_english_through_craft_i18n() {
    assert_eq!(serviceflow::tl!("Library"), "Library");
}
