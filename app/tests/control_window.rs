//! Pixel-free smoke tests of the egui shell, through the accessibility tree.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use serviceflow::ControlWindow;

#[test]
fn the_empty_control_window_shows_the_app_name_and_that_no_data_root_is_open() {
    let mut window = ControlWindow::default();
    let harness = Harness::new_ui(|ui| window.ui(ui));
    harness.get_by_label("serviceFlow");
    harness.get_by_label("No data root is open.");
}

#[test]
fn the_control_window_is_titled_serviceflow() {
    assert_eq!(serviceflow::control_viewport().title.as_deref(), Some("serviceFlow"));
}

#[test]
fn user_facing_strings_are_english_through_craft_i18n() {
    assert_eq!(serviceflow::tl!("Library"), "Library");
}
