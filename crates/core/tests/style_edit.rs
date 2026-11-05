//! Seam 1, the engine: editing the default style and a style item's style (#30). Item styles are
//! set edits (undo, redo, saved with the set in OpenSong's units); the default style is
//! serviceFlow's own, stored beside the library and never in OpenSong's settings (ADR-0003).

use serviceflow_core::command::{Command, ImportTrigger};
use serviceflow_core::ids::SetId;
use serviceflow_core::set::StyleAction;
use serviceflow_core::set_edit::SetEdit;
use serviceflow_core::state::Problem;
use serviceflow_core::style::{Align, ImagePosition, ImageSource, Rgba, VAlign, snap_margin, snap_size};
use serviceflow_core::style_edit::{Side, StyleChange, StyleEdit, StylePart, StyleTarget};
use serviceflow_testkit::EngineHarness;

const SET: &str = r##"<set name="Sunday"><slide_groups>
    <slide_group name="Blue" type="style" action="new"><style><body font="Arial" size="30" color="#0000FF00"/></style></slide_group>
    <slide_group name="Back" type="style" action="revert"/>
</slide_groups></set>"##;

const PRESENT_SETTINGS: &str =
    r##"<present_settings><default_style><title size="20"/><body font="Calibri" size="36" margin-left="5"/></default_style></present_settings>"##;

fn open() -> EngineHarness {
    let mut harness = EngineHarness::with_data(&[], &[("Sunday", SET)], &[("PresentSettings", PRESENT_SETTINGS)], &[]);
    harness.send(Command::ImportOpenSongStyles { trigger: ImportTrigger::FirstRun });
    harness.send(Command::RefreshSets);
    harness.send(Command::OpenSet(SetId::new("Sunday")));
    harness
}

fn edit(harness: &mut EngineHarness, target: StyleTarget, part: StylePart, change: StyleChange) {
    harness.send(Command::EditStyle { target, edit: StyleEdit::new(part, change) });
}

fn item_style(harness: &EngineHarness, index: usize) -> serviceflow_core::style::Style {
    (**harness.state().set.as_ref().unwrap().set.items[index].style.as_ref().expect("a pushed style")).clone()
}

#[test]
fn every_box_and_the_background_of_an_item_style_can_be_edited() {
    let mut h = open();
    let item = StyleTarget::Item(0);
    edit(&mut h, item, StylePart::Title, StyleChange::Font("Arial".into()));
    edit(&mut h, item, StylePart::Title, StyleChange::Color(Rgba::rgb(255, 0, 0)));
    edit(&mut h, item, StylePart::Subtitle, StyleChange::Align(Align::Right));
    edit(&mut h, item, StylePart::Subtitle, StyleChange::VAlign(VAlign::Top));
    edit(&mut h, item, StylePart::Body, StyleChange::Bold(true));
    edit(&mut h, item, StylePart::Body, StyleChange::Margin(Side::Left, 100.0));
    edit(&mut h, item, StylePart::Body, StyleChange::AutoScale(true));
    edit(&mut h, item, StylePart::Background, StyleChange::Color(Rgba::rgb(0, 0, 128)));
    edit(&mut h, item, StylePart::Background, StyleChange::Image(Some(ImageSource::File("red.png".into()))));
    edit(&mut h, item, StylePart::Background, StyleChange::Position(ImagePosition::Tile));
    let style = item_style(&h, 0);
    assert_eq!(style.title.text.font, "Arial");
    assert_eq!(style.title.text.color, Rgba::rgb(255, 0, 0));
    assert_eq!(style.subtitle.text.align, Align::Right);
    assert_eq!(style.subtitle.text.valign, VAlign::Top);
    assert!(style.body.text.bold);
    assert_eq!(style.body.text.margins.left, snap_margin(100.0));
    assert!(style.body.auto_scale);
    assert_eq!(style.background.color, Rgba::rgb(0, 0, 128));
    assert_eq!(style.background.image, Some(ImageSource::File("red.png".into())));
    assert_eq!(style.background.position, ImagePosition::Tile);
    assert!(h.state().set.as_ref().unwrap().modified, "a style edit is a set edit");
}

#[test]
fn item_style_edits_undo_and_redo_with_the_set_and_join_until_the_group_ends() {
    let mut h = open();
    let item = StyleTarget::Item(0);
    let before = item_style(&h, 0);
    edit(&mut h, item, StylePart::Body, StyleChange::Size(90.0));
    edit(&mut h, item, StylePart::Body, StyleChange::Size(120.0));
    h.send(Command::EndEditGroup);
    edit(&mut h, item, StylePart::Body, StyleChange::Bold(true));
    h.send(Command::UndoSet);
    assert!(!item_style(&h, 0).body.text.bold);
    assert_eq!(item_style(&h, 0).body.text.size, snap_size(120.0));
    h.send(Command::UndoSet);
    assert_eq!(item_style(&h, 0), before, "the two size edits were one step");
    assert!(!h.state().set.as_ref().unwrap().modified);
    h.send(Command::RedoSet).send(Command::RedoSet);
    assert!(item_style(&h, 0).body.text.bold);
}

#[test]
fn an_edit_a_style_does_not_have_is_ignored() {
    let mut h = open();
    // A revert item pushes no style, and a background has no font.
    edit(&mut h, StyleTarget::Item(1), StylePart::Body, StyleChange::Bold(true));
    edit(&mut h, StyleTarget::Item(0), StylePart::Background, StyleChange::Font("Arial".into()));
    edit(&mut h, StyleTarget::Item(9), StylePart::Body, StyleChange::Bold(true));
    assert!(!h.state().set.as_ref().unwrap().modified);
}

#[test]
fn lengths_snap_to_what_opensong_can_hold_and_margins_never_go_below_its_border() {
    let mut h = open();
    let item = StyleTarget::Item(0);
    edit(&mut h, item, StylePart::Body, StyleChange::Size(100.0));
    edit(&mut h, item, StylePart::Body, StyleChange::Margin(Side::Top, 3.0));
    let style = item_style(&h, 0);
    assert_eq!(style.body.text.size, snap_size(100.0));
    assert!((style.body.text.size - 100.0).abs() < 0.02);
    assert!((style.body.text.margins.top - 38.4).abs() < 0.001, "OpenSong's margin 0 plus its border");
}

#[test]
fn a_saved_item_style_is_written_in_opensong_units_and_reads_back_identical() {
    let mut h = open();
    let item = StyleTarget::Item(0);
    edit(&mut h, item, StylePart::Body, StyleChange::Size(100.0));
    edit(&mut h, item, StylePart::Body, StyleChange::Margin(Side::Left, 100.0));
    edit(&mut h, item, StylePart::Title, StyleChange::Color(Rgba { r: 1, g: 2, b: 3, a: 200 }));
    edit(&mut h, item, StylePart::Background, StyleChange::Color(Rgba::rgb(0, 0, 128)));
    let edited = item_style(&h, 0);
    h.send(Command::SaveSet);
    let text = String::from_utf8(h.set_bytes("Sunday").unwrap()).unwrap();
    // 100 px / 3 = 33.33 points, (100 / 3 - 12.8) = 20.53 of OpenSong's margin units.
    assert!(text.contains(r#"size="33.33""#), "{text}");
    assert!(text.contains(r#"margin-left="20.53""#), "{text}");
    assert!(text.contains("#01020337"), "{text}");
    let saved = h.saved_set("Sunday").unwrap();
    let back = (**saved.items[0].style.as_ref().unwrap()).clone();
    assert_eq!(back.body.text.size, edited.body.text.size);
    assert_eq!(back.body.text.margins, edited.body.text.margins);
    assert_eq!(back.title.text.color, edited.title.text.color);
    assert_eq!(back.background.color, edited.background.color);
    // The set the engine now holds is the one read back: nothing is left to save.
    assert!(!h.state().set.as_ref().unwrap().modified);
}

#[test]
fn an_edited_new_style_item_copies_the_edited_default() {
    let mut h = open();
    edit(&mut h, StyleTarget::Default, StylePart::Body, StyleChange::Font("Georgia".into()));
    h.send(Command::EditSet(SetEdit::AddStyle { name: "New".into(), action: StyleAction::New, at: None }));
    let set = h.state().set.clone().unwrap();
    let added = set.set.items.iter().find(|i| i.name == "New").unwrap();
    assert_eq!(added.style.as_ref().unwrap().body.text.font, "Georgia");
}

#[test]
fn editing_the_default_style_changes_what_is_drawn_and_is_kept_in_a_sidecar() {
    let mut h = open();
    let opensong = h.state().styles.default.clone();
    assert_eq!(opensong.body.text.font, "Calibri");
    edit(&mut h, StyleTarget::Default, StylePart::Body, StyleChange::Font("Georgia".into()));
    edit(&mut h, StyleTarget::Default, StylePart::Body, StyleChange::Size(150.0));
    let drawn = h.state().styles.default.clone();
    assert_eq!(drawn.body.text.font, "Georgia");
    assert_eq!(drawn.body.text.size, snap_size(150.0));
    assert!(h.state().default_style.is_edited());
    // Nothing is written until the undo step ends.
    assert_eq!(h.style_sidecar_bytes(), None);
    h.send(Command::EndEditGroup);
    assert!(h.style_sidecar_bytes().is_some());
    assert!(h.state().problems.is_empty());
}

#[test]
fn the_default_style_edit_never_touches_opensongs_settings() {
    let mut h = open();
    let before = h.present_settings().unwrap();
    edit(&mut h, StyleTarget::Default, StylePart::Title, StyleChange::Color(Rgba::rgb(1, 2, 3)));
    h.send(Command::EndEditGroup);
    h.send(Command::ImportOpenSongStyles { trigger: ImportTrigger::Manual });
    h.send(Command::ImportOpenSongSettings { trigger: ImportTrigger::Manual });
    assert_eq!(h.present_settings().unwrap(), before);
}

#[test]
fn the_default_style_edit_survives_a_restart_and_wins_over_the_import() {
    let mut h = open();
    edit(&mut h, StyleTarget::Default, StylePart::Body, StyleChange::Font("Georgia".into()));
    edit(&mut h, StyleTarget::Default, StylePart::Background, StyleChange::Color(Rgba::rgb(10, 20, 30)));
    h.send(Command::EndEditGroup);
    let edited = h.state().styles.default.clone();
    let mut h = h.restart();
    h.send(Command::ImportOpenSongStyles { trigger: ImportTrigger::FirstRun });
    assert_eq!(h.state().styles.default, edited);
    // The scripture style is still OpenSong's.
    assert!(h.state().default_style.is_edited());
}

#[test]
fn undo_and_redo_of_the_default_style_have_their_own_history_and_are_saved() {
    let mut h = open();
    let opensong = h.state().styles.default.clone();
    edit(&mut h, StyleTarget::Default, StylePart::Body, StyleChange::Bold(true));
    h.send(Command::EndEditGroup);
    edit(&mut h, StyleTarget::Default, StylePart::Body, StyleChange::Italic(true));
    edit(&mut h, StyleTarget::Default, StylePart::Body, StyleChange::Underline(true));
    h.send(Command::EndEditGroup);
    assert!(h.state().default_style.history.can_undo());
    h.send(Command::UndoStyle);
    let s = h.state().styles.default.clone();
    assert!(s.body.text.bold && !s.body.text.italic && !s.body.text.underline, "italic and underline were one step");
    h.send(Command::UndoStyle);
    assert_eq!(h.state().styles.default, opensong);
    assert!(!h.state().default_style.history.can_undo());
    assert!(h.state().default_style.history.can_redo());
    h.send(Command::RedoStyle).send(Command::RedoStyle);
    assert!(h.state().styles.default.body.text.underline);
    // The undo is saved too: a restart shows the state the operator left.
    let after = h.state().styles.default.clone();
    let mut h = h.restart();
    h.send(Command::ImportOpenSongStyles { trigger: ImportTrigger::FirstRun });
    assert_eq!(h.state().styles.default, after);
    // A set edit does not undo a style edit, nor the other way round.
    assert!(!h.state().default_style.history.can_undo(), "history is for this run only");
}

#[test]
fn importing_from_opensong_again_drops_the_edit() {
    let mut h = open();
    let opensong = h.state().styles.default.clone();
    edit(&mut h, StyleTarget::Default, StylePart::Body, StyleChange::Font("Georgia".into()));
    h.send(Command::EndEditGroup);
    h.send(Command::ImportOpenSongStyles { trigger: ImportTrigger::Manual });
    assert_eq!(h.state().styles.default, opensong);
    assert!(!h.state().default_style.is_edited());
    let mut h = h.restart();
    h.send(Command::ImportOpenSongStyles { trigger: ImportTrigger::FirstRun });
    assert_eq!(h.state().styles.default, opensong, "the sidecar says there is no edit");
}

#[test]
fn an_unreadable_sidecar_falls_back_to_opensongs_default_and_says_so() {
    let mut h = open();
    h.write_style_sidecar_file(b"this is = not [valid");
    h.send(Command::ImportOpenSongStyles { trigger: ImportTrigger::FirstRun });
    assert_eq!(h.state().styles.default.body.text.font, "Calibri");
    assert!(h.state().problems.iter().any(|p| matches!(p, Problem::DefaultStyleUnreadable { .. })));
}

#[test]
fn the_edit_applies_when_it_is_read_after_the_import_too() {
    // The sidecar read and the OpenSong import finish in either order.
    let mut h = open();
    edit(&mut h, StyleTarget::Default, StylePart::Body, StyleChange::Font("Georgia".into()));
    let edited = h.state().default_style.edited.clone();
    h.send(Command::StylesImported(std::sync::Arc::new(Default::default())));
    assert_eq!(h.state().styles.default.body.text.font, "Georgia");
    h.send(Command::DefaultStyleLoaded(edited));
    assert_eq!(h.state().styles.default.body.text.font, "Georgia");
}
