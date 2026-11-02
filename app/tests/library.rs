//! The library panel, through the accessibility tree.

use egui_kittest::Harness;
use egui_kittest::kittest::{Queryable as _, by};
use serviceflow::ControlWindow;
use serviceflow_runtime::Driver;
use serviceflow_storage_memory::MemoryStorage;
use serviceflow_testkit::opensong_data_root;

fn window() -> ControlWindow<MemoryStorage> {
    ControlWindow::new(Driver::new(MemoryStorage::from_data_root(&opensong_data_root()).unwrap()))
}

#[test]
fn the_library_panel_lists_song_titles_not_filenames() {
    let mut window = window();
    let mut harness = Harness::builder().allow_missing_glyphs().build_ui(|ui| window.ui(ui));
    harness.run();
    harness.get_by_label("Library");
    // "Amazing Grace HOF#50" is a filename; the panel shows the title.
    harness.get_by_label("Amazing Grace");
    assert!(harness.query_by_label("Amazing Grace HOF#50").is_none());
    harness.get_by_label("How Deep the Father's Love 天父的爱何其深长");
}

#[test]
fn typing_a_hymn_number_finds_the_song_and_clicking_it_opens_it() {
    let mut window = window();
    let mut harness = Harness::builder().allow_missing_glyphs().build_ui(|ui| window.ui(ui));
    harness.run();
    let search = harness.get_by(|n| n.role() == egui::accesskit::Role::TextInput);
    search.focus();
    search.type_text("#50");
    harness.run();
    assert!(harness.query_by_label("It Is Well With My Soul").is_none());
    harness.get_by_role_and_label(egui::accesskit::Role::Button, "Amazing Grace").click();
    harness.run();
    harness.get(by().role(egui::accesskit::Role::Image).label_contains("Amazing grace"));
}
