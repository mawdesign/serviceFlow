//! The set list panel and running a set, through the accessibility tree.

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{Queryable as _, by};
use serviceflow::ControlWindow;
use serviceflow_core::command::Command;
use serviceflow_core::ids::SetId;
use serviceflow_runtime::Driver;
use serviceflow_storage_memory::MemoryStorage;
use serviceflow_testkit::opensong_data_root;

fn window() -> ControlWindow<MemoryStorage> {
    let mut window = ControlWindow::new(Driver::new(MemoryStorage::from_data_root(&opensong_data_root()).unwrap()));
    window.driver().send(Command::OpenSet(SetId::new("Sunday Service")));
    window
}

#[test]
fn the_set_panel_lists_items_and_selecting_one_shows_slides_and_notes() {
    let mut window = window();
    let mut harness = Harness::builder().allow_missing_glyphs().with_size(egui::vec2(1400., 900.)).build_ui(|ui| window.ui(ui));
    harness.run();
    harness.get_by_label("Psalm 23 (scripture)");
    harness.get_by_label("Old Hymn That Was Renamed");
    harness.get_by_role_and_label(Role::Button, "find…");
    // The item's order is "V1 C V4 C" (4 slides in the flow), and its notes are under the grid.
    harness.get_all_by_role_and_label(Role::Button, "It Is Well With My Soul").next().unwrap().click();
    harness.run();
    harness.get_by_label("Start softly. Band joins at the chorus.");
    // The grid shows slides (the override itself is checked at the engine seam).
    assert!(harness.get_all(by().role(Role::Image)).count() >= 4);
}

#[test]
fn placeholders_and_missing_songs_explain_themselves() {
    let mut window = window();
    let mut harness = Harness::builder().allow_missing_glyphs().with_size(egui::vec2(1400., 900.)).build_ui(|ui| window.ui(ui));
    harness.run();
    harness.get_by_label("Psalm 23 (scripture)").click();
    harness.run();
    harness.get_by_label_contains("cannot be shown yet");
    harness.get_by_label("Read by Sam.");
    harness.get_by_label("Old Hymn That Was Renamed").click();
    harness.run();
    harness.get_by_label_contains("The song is missing");
}
