//! The style editor in the control window (#30). Editing, undo, saving in OpenSong's units and the
//! default style's own storage are tested through the engine (`crates/core/tests/style_edit.rs`);
//! these drive the editor and check it sends the right commands, shows the state and previews
//! with the real renderer.

use std::cell::RefCell;
use std::rc::Rc;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _, by};
use serviceflow::ControlWindow;
use serviceflow_core::command::Command;
use serviceflow_core::ids::SetId;
use serviceflow_core::set::StyleAction;
use serviceflow_core::set_edit::SetEdit;
use serviceflow_runtime::Driver;
use serviceflow_storage::SetRepository as _;
use serviceflow_storage_memory::MemoryStorage;
use serviceflow_testkit::opensong_data_root;

type Window = Rc<RefCell<ControlWindow<MemoryStorage>>>;

/// The fixture set open, with a pushed-style item added and selected.
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

fn add_and_select_style_item(harness: &mut Harness<'static>, window: &Window) -> usize {
    let at = window.borrow().state().set.as_ref().unwrap().set.items.len();
    window.borrow_mut().driver().send(Command::EditSet(SetEdit::AddStyle { name: "Blue".into(), action: StyleAction::New, at: Some(at) }));
    window.borrow_mut().driver().send(Command::SelectItem(at));
    harness.run();
    harness.run();
    at
}

fn font_field<'h>(harness: &'h Harness<'static>) -> egui_kittest::Node<'h> {
    harness.get(by().role(Role::TextInput).label("Font"))
}

fn type_font(harness: &mut Harness<'static>, text: &str) {
    font_field(harness).focus();
    harness.run();
    font_field(harness).type_text(text);
    harness.run();
}

fn click(harness: &mut Harness<'static>, label: &str) {
    harness.get_by_role_and_label(Role::Button, label).click();
    harness.run();
}

#[test]
fn the_item_style_button_needs_an_item_with_a_style() {
    let (mut harness, window) = harness();
    assert!(harness.get_by_role_and_label(Role::Button, "Edit item style").accesskit_node().is_disabled());
    add_and_select_style_item(&mut harness, &window);
    assert!(!harness.get_by_role_and_label(Role::Button, "Edit item style").accesskit_node().is_disabled());
}

#[test]
fn editing_a_style_items_style_is_a_set_edit_with_undo_and_a_live_preview() {
    let (mut harness, window) = harness();
    let at = add_and_select_style_item(&mut harness, &window);
    click(&mut harness, "Edit item style");
    harness.get_by_label("Style item “Blue”");
    // The preview is the real renderer's picture of the style.
    let drawn = |harness: &Harness<'static>| harness.ctx.tex_manager().read().allocated().filter(|(_, meta)| meta.name == "style preview").count();
    assert_eq!(drawn(&harness), 1);
    let style = |window: &Window| window.borrow().state().set.as_ref().unwrap().set.items[at].style.as_ref().unwrap().as_ref().clone();
    let before = style(&window);

    // Body is the tab shown first: type a font, tick italic.
    type_font(&mut harness, "Zzyzx Sans");
    assert!(style(&window).body.text.font.ends_with("Zzyzx Sans"), "{}", style(&window).body.text.font);
    harness.get(by().role(Role::CheckBox).label("Italic")).click();
    harness.run();
    assert_eq!(style(&window).body.text.italic, !before.body.text.italic);
    assert!(window.borrow().state().set.as_ref().unwrap().modified, "a style edit is a set edit");
    // A font nobody has is flagged right where it is picked (#9).
    assert!(harness.query_all_by_label_contains("Font not installed").count() >= 1);

    // The title tab edits the title box.
    harness.get_by_role_and_label(Role::Button, "Title").click();
    harness.run();
    let underline = style(&window).title.text.underline;
    harness.get(by().role(Role::CheckBox).label("Underline")).click();
    harness.run();
    assert_eq!(style(&window).title.text.underline, !underline);
    assert_eq!(style(&window).body.text.underline, before.body.text.underline);

    // Undo walks back through the set's history, one step per change.
    click(&mut harness, "Undo style");
    assert_eq!(style(&window).title.text.underline, underline);
    click(&mut harness, "Undo style");
    assert_eq!(style(&window).body.text.italic, before.body.text.italic);
    click(&mut harness, "Undo style");
    assert_eq!(style(&window), before);
    click(&mut harness, "Redo style");
    assert!(style(&window).body.text.font.ends_with("Zzyzx Sans"));
}

#[test]
fn saving_the_set_writes_the_item_style_in_opensong_units() {
    let (mut harness, window) = harness();
    add_and_select_style_item(&mut harness, &window);
    click(&mut harness, "Edit item style");
    harness.get_by_role_and_label(Role::Button, "Background").click();
    harness.run();
    harness.get_by_label("Background colour");
    harness.get_by_role_and_label(Role::Button, "Body").click();
    harness.run();
    harness.get(by().role(Role::CheckBox).label("Italic")).click();
    harness.run();
    // Both the set panel and the editor have a Save set button; the editor's is the second.
    window.borrow_mut().driver().send(Command::SaveSet);
    harness.run();
    harness.run();
    let bytes = serviceflow_testkit::block_on(window.borrow_mut().driver().storage().read_set(&SetId::new("Sunday Service"))).unwrap().bytes;
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains(r#"italic="true""#), "{text}");
    // The default style is 108 px in the fixture: 36 in OpenSong's units.
    assert!(text.contains(r#"size="36""#), "{text}");
    assert!(!window.borrow().state().set.as_ref().unwrap().modified);
}

#[test]
fn the_default_style_edit_is_serviceflows_own_and_has_its_own_undo() {
    let (mut harness, window) = harness();
    click(&mut harness, "Edit default style");
    harness.get_by_label("Default style");
    let opensong = window.borrow().state().styles.default.clone();
    let settings_before = serviceflow_testkit::block_on(serviceflow_storage::SettingsRepository::read_settings(
        window.borrow_mut().driver().storage(),
        &serviceflow_storage::SettingsId("PresentSettings".into()),
    ))
    .unwrap()
    .bytes;

    harness.get(by().role(Role::CheckBox).label("Italic")).click();
    harness.run();
    assert!(window.borrow().state().styles.default.body.text.italic);
    // The set is untouched: it is not a set edit.
    assert!(!window.borrow().state().set.as_ref().unwrap().modified);

    // Written beside the library, not into OpenSong's settings.
    let sidecar = serviceflow_testkit::block_on(serviceflow_storage::SidecarRepository::read_sidecar(
        window.borrow_mut().driver().storage(),
        &serviceflow_storage::DocumentId::Styles,
    ))
    .unwrap();
    assert!(sidecar.is_some());
    let settings_after = serviceflow_testkit::block_on(serviceflow_storage::SettingsRepository::read_settings(
        window.borrow_mut().driver().storage(),
        &serviceflow_storage::SettingsId("PresentSettings".into()),
    ))
    .unwrap()
    .bytes;
    assert_eq!(settings_after, settings_before);

    click(&mut harness, "Undo style");
    assert_eq!(window.borrow().state().styles.default, opensong);
    click(&mut harness, "Redo style");
    assert!(window.borrow().state().styles.default.body.text.italic);
}

#[test]
fn the_font_warning_links_to_the_editor() {
    let (mut harness, window) = harness();
    window.borrow_mut().driver().send(Command::FontSubstituted { requested: "Zzyzx Sans".into(), used: "Noto Sans".into() });
    harness.run();
    harness.run();
    assert!(harness.query_by_label("Style editor").is_none());
    click(&mut harness, "Change the font in the style editor");
    harness.get_by_label("Default style");
    harness.get_by_label("Style editor");
}
