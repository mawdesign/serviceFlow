//! Custom slide and image items in the control window: thumbnails, the missing-image
//! placeholder with its "find…" action, and no song editor for items.

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
    window.driver().send(Command::OpenSet(SetId::new("Notices and Pictures")));
    window
}

#[test]
fn custom_slides_show_as_thumbnails_with_their_text() {
    let mut window = window();
    let mut harness = Harness::builder().allow_missing_glyphs().with_size(egui::vec2(1400., 900.)).build_ui(|ui| window.ui(ui));
    harness.run();
    harness.get_by_label("Welcome (custom)").click();
    harness.run();
    harness.get_by_label("Good morning\nand welcome");
    harness.get_by_label("-= Notices =-\nMorning tea after the service");
    assert_eq!(harness.get(by().role(Role::MultilineTextInput).label("Notes")).value().as_deref(), Some("Read slowly."));
    // A custom item is not a song file: there is nothing to edit here (it is edited in the Set panel).
    assert!(harness.query_by_label("Edit song").is_none());
}

#[test]
fn a_missing_image_is_flagged_and_find_offers_the_pictures_of_backgrounds() {
    let mut window = window();
    let mut harness = Harness::builder().allow_missing_glyphs().with_size(egui::vec2(1400., 900.)).build_ui(|ui| window.ui(ui));
    harness.run();
    harness.get_by_label("Pictures (image)").click();
    harness.run();
    harness.get_by_label_contains("The image is missing");
    // The list has a "find…" next to the item and one beside the missing picture.
    assert_eq!(harness.get_all_by_role_and_label(Role::Button, "find…").count(), 2);
    harness.get_all_by_role_and_label(Role::Button, "find…").next().unwrap().click();
    harness.run();
    harness.get_by_label_contains("Pick a replacement from Backgrounds/");
    harness.get_by_label("red.png").click();
    harness.run();
    assert!(harness.query_by_label_contains("The image is missing").is_none(), "relinked to a picture that exists");
    // One thumbnail per picture.
    assert!(harness.get_all(by().role(Role::Image)).count() >= 2);
}
