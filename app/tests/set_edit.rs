//! Building and editing a set in the control window (#22). The edits themselves, undo and saving
//! are tested through the engine (`crates/core/tests/set_edit.rs`); these check that the panels
//! send the right commands and show what the state says.

use std::cell::RefCell;
use std::rc::Rc;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{Queryable as _, by};
use serviceflow::ControlWindow;
use serviceflow_core::command::Command;
use serviceflow_core::ids::SetId;
use serviceflow_core::set::ItemKind;
use serviceflow_core::state::OpenSet;
use serviceflow_runtime::Driver;
use serviceflow_storage::SetRepository as _;
use serviceflow_storage_memory::MemoryStorage;
use serviceflow_testkit::opensong_data_root;

type Window = Rc<RefCell<ControlWindow<MemoryStorage>>>;

fn harness() -> (Harness<'static>, Window) {
    let mut driver = Driver::new(MemoryStorage::from_data_root(&opensong_data_root()).unwrap());
    driver.send(Command::OpenSet(SetId::new("Sunday Service")));
    let window: Window = Rc::new(RefCell::new(ControlWindow::new(driver)));
    let inner = Rc::clone(&window);
    let mut harness = Harness::builder().with_size(egui::vec2(1600.0, 1000.0)).allow_missing_glyphs().build_ui(move |ui| inner.borrow_mut().ui(ui));
    harness.run();
    harness.run();
    (harness, window)
}

fn set(window: &Window) -> OpenSet {
    window.borrow().state().set.clone().expect("a set is open")
}

fn names(window: &Window) -> Vec<String> {
    set(window).set.items.iter().map(|i| i.name.clone()).collect()
}

fn saved_bytes(window: &Window, id: &str) -> Option<Vec<u8>> {
    let mut window = window.borrow_mut();
    serviceflow_testkit::block_on(window.driver().storage().read_set(&SetId::new(id))).ok().map(|s| s.bytes)
}

fn click(harness: &mut Harness<'static>, label: &str) {
    harness.get_by_role_and_label(Role::Button, label).click();
    harness.run();
}

#[test]
fn enter_on_a_library_song_adds_it_after_the_selected_item() {
    let (mut harness, window) = harness();
    window.borrow_mut().driver().send(Command::SelectItem(1));
    harness.run();
    harness.run();
    assert_eq!(set(&window).selected, Some(1));
    // Give a library row the keyboard and press Enter.
    let title = window.borrow().state().library.title(&serviceflow_core::ids::SongId::new("Just A Closer Walk With Thee")).to_owned();
    harness.get_by_role_and_label(Role::Button, &title).focus();
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert_eq!(names(&window)[2], "Just A Closer Walk With Thee");
    assert!(set(&window).modified);
    harness.get_by_label("Edited, not saved");
}

#[test]
fn enter_in_the_search_box_adds_the_best_match() {
    let (mut harness, window) = harness();
    let search = harness.get(by().role(Role::TextInput));
    search.focus();
    harness.run();
    harness.get(by().role(Role::TextInput)).type_text("well");
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert_eq!(names(&window).last().unwrap(), "It Is Well With My Soul");
}

#[test]
fn a_library_song_dragged_onto_an_item_goes_before_it() {
    let (mut harness, window) = harness();
    let title = window.borrow().state().library.title(&serviceflow_core::ids::SongId::new("Just A Closer Walk With Thee")).to_owned();
    let from = harness.get_by_role_and_label(Role::Button, &title).rect().center();
    let to = harness.get_by_role_and_label(Role::Button, "Psalm 23 (scripture)").rect().center();
    harness.hover_at(from);
    harness.run();
    harness.drag_at(from);
    harness.run();
    for step in 1..=10 {
        harness.hover_at(from + (to - from) * (step as f32 / 10.0));
        harness.run();
    }
    harness.drop_at(to);
    harness.run();
    assert_eq!(names(&window)[2], "Just A Closer Walk With Thee", "{:?}", names(&window));
}

#[test]
fn items_are_moved_removed_and_the_changes_undone_and_redone() {
    let (mut harness, window) = harness();
    let before = names(&window);
    harness.get_by_role_and_label(Role::Button, "Psalm 23 (scripture)").click();
    harness.run();
    click(&mut harness, "Move up");
    assert_eq!(names(&window)[1], "Psalm 23");
    click(&mut harness, "Remove item");
    assert_eq!(names(&window).len(), before.len() - 1);
    click(&mut harness, "Undo");
    click(&mut harness, "Undo");
    assert_eq!(names(&window), before);
    assert!(!set(&window).modified);
    click(&mut harness, "Redo");
    assert_eq!(names(&window)[1], "Psalm 23");
}

#[test]
fn the_order_override_and_notes_of_the_selected_item_are_edited() {
    let (mut harness, window) = harness();
    harness.get_all_by_role_and_label(Role::Button, "It Is Well With My Soul").next().unwrap().click();
    harness.run();
    let order = harness.get(by().role(Role::TextInput).label("Presentation order"));
    assert_eq!(order.value().as_deref(), Some("V1 C V4 C"));
    order.focus();
    harness.run();
    harness.get(by().role(Role::TextInput).label("Presentation order")).type_text(" V1");
    harness.run();
    let ItemKind::Song(song) = &set(&window).set.items[0].kind else { panic!() };
    assert_eq!(song.presentation, ["V1", "C", "V4", "C", "V1"]);
    // The notes under the slides are editable.
    let notes = harness.get(by().role(Role::MultilineTextInput).label("Notes"));
    assert_eq!(notes.value().as_deref(), Some("Start softly. Band joins at the chorus."));
    notes.focus();
    harness.run();
    harness.get(by().role(Role::MultilineTextInput).label("Notes")).type_text(" Capo 2.");
    harness.run();
    assert_eq!(set(&window).set.items[0].notes, "Start softly. Band joins at the chorus. Capo 2.");
}

#[test]
fn a_custom_slide_item_is_added_and_its_slides_typed() {
    let (mut harness, window) = harness();
    click(&mut harness, "Add custom slides");
    let index = names(&window).len() - 1;
    assert!(matches!(set(&window).set.items[index].kind, ItemKind::Custom(_)));
    harness.get_by_role_and_label(Role::Button, "Custom slides (custom)").click();
    harness.run();
    let slides = harness.get(by().role(Role::MultilineTextInput).label("Slides (separate them with a line of ---)"));
    slides.focus();
    harness.run();
    harness.get(by().role(Role::MultilineTextInput).label("Slides (separate them with a line of ---)")).type_text("Welcome\n---\nTea after");
    harness.run();
    let ItemKind::Custom(custom) = &set(&window).set.items[index].kind else { panic!() };
    assert_eq!(custom.slides, ["Welcome", "Tea after"]);
    assert_eq!(window.borrow().state().song.as_ref().unwrap().slides.len(), 2, "the slide grid follows");
}

#[test]
fn an_image_item_is_added_from_backgrounds_and_style_items_too() {
    let (mut harness, window) = harness();
    click(&mut harness, "Add images…");
    harness.run();
    harness.get(by().role(Role::CheckBox).label("red.png")).click();
    harness.run();
    click(&mut harness, "Add image item");
    click(&mut harness, "Add style change");
    click(&mut harness, "Add style revert");
    let state = window.borrow().state();
    let items = &state.set.as_ref().unwrap().set.items;
    let n = items.len();
    assert!(matches!(&items[n - 3].kind, ItemKind::Image(image) if image.slides.len() == 1));
    assert!(matches!(items[n - 2].kind, ItemKind::Style { action: serviceflow_core::set::StyleAction::New, .. }));
    assert!(matches!(items[n - 1].kind, ItemKind::Style { action: serviceflow_core::set::StyleAction::Revert, .. }));
}

#[test]
fn the_set_is_saved_and_saved_under_a_new_name() {
    let (mut harness, window) = harness();
    let original = saved_bytes(&window, "Sunday Service").unwrap();
    harness.get_by_role_and_label(Role::Button, "Psalm 23 (scripture)").click();
    harness.run();
    click(&mut harness, "Remove item");
    click(&mut harness, "Save set");
    let saved = saved_bytes(&window, "Sunday Service").unwrap();
    assert_ne!(saved, original);
    assert!(!String::from_utf8(saved).unwrap().contains("Psalm 23"));
    assert!(!set(&window).modified);

    click(&mut harness, "Save set as…");
    let name = harness.get(by().role(Role::TextInput).label("Save the set as:"));
    name.focus();
    harness.run();
    harness.get(by().role(Role::TextInput).label("Save the set as:")).type_text(" 2");
    harness.run();
    click(&mut harness, "Save");
    assert!(saved_bytes(&window, "Sunday Service 2").is_some());
    assert_eq!(set(&window).id, SetId::new("Sunday Service 2"));
}
