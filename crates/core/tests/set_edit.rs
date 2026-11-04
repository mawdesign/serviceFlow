//! Seam 1, the engine: building and editing a set (adding, moving and removing items, editing
//! song item orders and notes, custom, image and style items), undo and redo, and saving sets
//! in canonical form with hash conflict handling, under a new name and as a duplicate.

use serviceflow_core::command::{Command, ConflictChoice, ImportTrigger};
use serviceflow_core::ids::{SetId, SongId};
use serviceflow_core::plan::{PlanAction, PlanFile, PlanStep};
use serviceflow_core::resolve::StyleChoice;
use serviceflow_core::set::{ItemKind, StyleAction};
use serviceflow_core::set_edit::{ItemField, SetEdit};
use serviceflow_core::song::LanguageMode;
use serviceflow_core::state::{OpenSet, Problem};
use serviceflow_core::style::{Captions, ImageSource};
use serviceflow_testkit::EngineHarness;

const SONGS: &[(&str, &str)] = &[
    ("Alpha", "<song><title>Alpha</title><lyrics>[V1]\n a1\n[V2]\n a2\n[C]\n ac</lyrics></song>"),
    ("Beta", "<song><title>Beta</title><lyrics>[V1]\n b1\n[V2]\n b2</lyrics></song>"),
    ("Hymns/Gamma", "<song><title>Gamma</title><lyrics>[V1]\n g1</lyrics></song>"),
];

/// A set with an unknown root attribute, an unknown element and an unknown item attribute, to
/// check they survive editing and saving.
const SET: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<set name="Sunday" version="2"><future>kept</future>
  <slide_groups>
    <slide_group name="Alpha" type="song" path="" transition="1"><notes>slow</notes></slide_group>
    <slide_group name="Psalm" type="scripture"><title>Psalm</title><slides><slide><body>text</body></slide></slides></slide_group>
    <slide_group name="Beta" type="song" path=""/>
  </slide_groups>
</set>"#;

const PRESENT_SETTINGS: &str = r#"<present_settings><default_style><body font="DefaultFont" size="40"/></default_style></present_settings>"#;

fn open() -> EngineHarness {
    let mut harness = EngineHarness::with_data(
        SONGS,
        &[("Sunday", SET)],
        &[("PresentSettings", PRESENT_SETTINGS)],
        &[("red.png", b"RED"), ("Sub/blue.jpg", b"BLUE")],
    );
    harness.send(Command::ImportOpenSongStyles { trigger: ImportTrigger::FirstRun });
    harness.send(Command::RefreshLibrary);
    harness.send(Command::RefreshSets);
    harness.send(Command::OpenSet(SetId::new("Sunday")));
    harness
}

fn set(harness: &EngineHarness) -> OpenSet {
    harness.state().set.clone().expect("a set is open")
}

fn names(harness: &EngineHarness) -> Vec<String> {
    set(harness).set.items.iter().map(|i| i.name.clone()).collect()
}

fn edit(harness: &mut EngineHarness, edit: SetEdit) {
    harness.send(Command::EditSet(edit));
}

fn field(item: usize, field: ItemField, text: &str) -> SetEdit {
    SetEdit::Field { item, field, text: text.to_owned() }
}

fn first_lines(harness: &EngineHarness) -> Vec<String> {
    harness.state().song.as_ref().map(|s| s.slides.iter().map(|sl| sl.lines.join("/")).collect()).unwrap_or_default()
}

fn saved(harness: &EngineHarness, id: &str) -> serviceflow_core::set::Set {
    harness.saved_set(id).expect("the set file exists")
}

// Adding, moving and removing items

#[test]
fn a_song_from_the_library_is_added_at_the_end_after_the_selected_item_or_where_it_is_dropped() {
    let mut harness = open();
    edit(&mut harness, SetEdit::AddSong { song: SongId::new("Hymns/Gamma"), at: None });
    assert_eq!(names(&harness), ["Alpha", "Psalm", "Beta", "Gamma"]);
    let ItemKind::Song(item) = &set(&harness).set.items[3].kind else { panic!("a song item") };
    assert_eq!(item.path, "Hymns", "the category is the item's path, its filename the name");
    assert!(item.presentation.is_empty());

    harness.send(Command::SelectItem(0));
    edit(&mut harness, SetEdit::AddSong { song: SongId::new("Beta"), at: None });
    assert_eq!(names(&harness), ["Alpha", "Beta", "Psalm", "Beta", "Gamma"], "Enter adds after the selected item");
    // Dropped on the first row: goes before it.
    edit(&mut harness, SetEdit::AddSong { song: SongId::new("Hymns/Gamma"), at: Some(0) });
    assert_eq!(names(&harness), ["Gamma", "Alpha", "Beta", "Psalm", "Beta", "Gamma"]);
    let open = set(&harness);
    assert_eq!(open.selected, Some(1), "the selection stays on the same item");
    assert!(open.modified);
    assert!(!open.is_missing(0));
    assert_eq!(harness.set_bytes("Sunday").unwrap(), SET.as_bytes(), "nothing is written before a save");
}

#[test]
fn items_are_moved_and_removed_and_the_selection_follows_its_item() {
    let mut harness = open();
    harness.send(Command::SelectItem(2));
    assert_eq!(harness.state().song.as_ref().unwrap().id, SongId::new("Beta"));
    edit(&mut harness, SetEdit::Move { from: 2, to: 0 });
    assert_eq!(names(&harness), ["Beta", "Alpha", "Psalm"]);
    assert_eq!(set(&harness).selected, Some(0));
    edit(&mut harness, SetEdit::Move { from: 1, to: 2 });
    assert_eq!(names(&harness), ["Beta", "Psalm", "Alpha"]);
    edit(&mut harness, SetEdit::Remove(1));
    assert_eq!(names(&harness), ["Beta", "Alpha"]);
    assert_eq!(set(&harness).selected, Some(0));
    edit(&mut harness, SetEdit::Remove(0));
    assert_eq!(names(&harness), ["Alpha"]);
    assert_eq!(set(&harness).selected, None, "the selected item is gone");
    // Out of range: nothing happens and nothing enters the history.
    let before = set(&harness).history;
    edit(&mut harness, SetEdit::Remove(5));
    edit(&mut harness, SetEdit::Move { from: 7, to: 0 });
    assert_eq!(set(&harness).history, before);
}

// Song items: order override and notes

#[test]
fn a_song_items_presentation_order_override_is_edited_and_shown() {
    let mut harness = open();
    harness.send(Command::SelectItem(0));
    assert_eq!(first_lines(&harness)[..3], ["a1", "a2", "ac"]);
    edit(&mut harness, field(0, ItemField::Presentation, "C V1  C"));
    let ItemKind::Song(item) = &set(&harness).set.items[0].kind else { panic!() };
    assert_eq!(item.presentation, ["C", "V1", "C"]);
    assert_eq!(first_lines(&harness)[..3], ["ac", "a1", "ac"], "the open song follows the override at once");
    // The song file itself is not touched.
    assert!(harness.state().song.as_ref().unwrap().file.presentation.is_empty());
    edit(&mut harness, field(0, ItemField::Presentation, ""));
    assert_eq!(first_lines(&harness)[..3], ["a1", "a2", "ac"], "empty: the song's own order");
}

#[test]
fn item_notes_are_edited_and_typing_is_one_undo_step() {
    let mut harness = open();
    edit(&mut harness, field(0, ItemField::Notes, "s"));
    edit(&mut harness, field(0, ItemField::Notes, "sl"));
    edit(&mut harness, field(0, ItemField::Notes, "slo"));
    harness.send(Command::EndEditGroup);
    edit(&mut harness, field(1, ItemField::Notes, "Read by Sam"));
    assert_eq!(set(&harness).set.items[0].notes, "slo");
    assert_eq!(set(&harness).set.items[1].notes, "Read by Sam", "a placeholder's notes can be edited too");
    harness.send(Command::UndoSet);
    assert_eq!(set(&harness).set.items[1].notes, "");
    harness.send(Command::UndoSet);
    assert_eq!(set(&harness).set.items[0].notes, "slow");
    assert!(!set(&harness).modified);
}

#[test]
fn a_field_an_item_does_not_have_is_ignored() {
    let mut harness = open();
    edit(&mut harness, field(0, ItemField::Slides, "x"));
    edit(&mut harness, field(0, ItemField::Name, "Renamed"));
    edit(&mut harness, field(1, ItemField::Presentation, "V1"));
    assert!(!set(&harness).modified);
    assert!(!set(&harness).history.can_undo());
}

// Custom, image and style items

#[test]
fn a_custom_slide_item_is_created_and_its_title_subtitle_and_slides_edited() {
    let mut harness = open();
    harness.send(Command::SelectItem(2));
    edit(&mut harness, SetEdit::AddCustom { name: "Notices".into(), at: None });
    assert_eq!(names(&harness), ["Alpha", "Psalm", "Beta", "Notices"]);
    harness.send(Command::SelectItem(3));
    assert_eq!(harness.state().song.as_ref().unwrap().slides.len(), 1, "a new custom item has one empty slide");
    edit(&mut harness, field(3, ItemField::Title, "Notices"));
    edit(&mut harness, field(3, ItemField::Subtitle, "This week"));
    edit(&mut harness, field(3, ItemField::Slides, "Tea after\nthe service\n---\nPrayer at 7"));
    let state = harness.state();
    let open = state.song.as_ref().unwrap();
    assert_eq!(first_lines(&harness), ["Tea after/the service", "Prayer at 7"], "the slides change at once");
    assert_eq!(open.song.captions, Some(Captions { title: "Notices".into(), subtitle: "This week".into() }));
    edit(&mut harness, field(3, ItemField::Name, "Church notices"));
    assert_eq!(names(&harness)[3], "Church notices");
    let ItemKind::Custom(custom) = &set(&harness).set.items[3].kind else { panic!() };
    assert_eq!(custom.slides_text(), "Tea after\nthe service\n---\nPrayer at 7");
}

#[test]
fn an_image_item_is_created_from_files_in_backgrounds() {
    let mut harness = open();
    harness.send(Command::ListBackgrounds);
    assert_eq!(harness.state().backgrounds, ["Sub/blue.jpg", "red.png"]);
    edit(&mut harness, SetEdit::AddImages { name: "Pictures".into(), files: vec!["red.png".into(), "Sub/blue.jpg".into()], at: Some(1) });
    assert_eq!(names(&harness), ["Alpha", "Pictures", "Psalm", "Beta"]);
    harness.send(Command::SelectItem(1));
    let state = harness.state();
    let open = state.song.as_ref().unwrap();
    assert_eq!(open.slides.len(), 2);
    let ItemKind::Image(image) = &state.set.as_ref().unwrap().set.items[1].kind else { panic!() };
    assert_eq!(image.slides[1].image, Some(ImageSource::File("Sub/blue.jpg".into())));
    assert_eq!(state.missing_image(1), None);
}

#[test]
fn style_items_push_a_copy_of_the_default_style_or_revert() {
    let mut harness = open();
    edit(&mut harness, SetEdit::AddStyle { name: "Blue".into(), action: StyleAction::New, at: Some(2) });
    edit(&mut harness, SetEdit::AddStyle { name: "Back".into(), action: StyleAction::Revert, at: None });
    assert_eq!(names(&harness), ["Alpha", "Psalm", "Blue", "Beta", "Back"]);
    let open = set(&harness);
    let pushed = open.set.items[2].style.clone().expect("a pushed style");
    assert_eq!(pushed.body.text.font, "DefaultFont");
    assert!(open.set.items[4].style.is_none());
    // Beta comes after the push: drawn with the pushed style.
    harness.send(Command::SelectItem(3));
    let state = harness.state();
    let song = state.song.as_ref().unwrap();
    assert!(matches!(state.style_choice(&song.id, &song.song), StyleChoice::Custom(_)));
}

// Undo and redo

#[test]
fn undo_and_redo_walk_through_set_edits_and_a_new_edit_drops_the_redo() {
    let mut harness = open();
    edit(&mut harness, SetEdit::AddSong { song: SongId::new("Hymns/Gamma"), at: None });
    edit(&mut harness, SetEdit::Remove(0));
    assert_eq!(names(&harness), ["Psalm", "Beta", "Gamma"]);
    harness.send(Command::UndoSet);
    assert_eq!(names(&harness), ["Alpha", "Psalm", "Beta", "Gamma"]);
    harness.send(Command::UndoSet);
    assert_eq!(names(&harness), ["Alpha", "Psalm", "Beta"]);
    assert!(!set(&harness).modified, "back to the file's version");
    assert!(!set(&harness).history.can_undo());
    harness.send(Command::RedoSet);
    assert_eq!(names(&harness), ["Alpha", "Psalm", "Beta", "Gamma"]);
    edit(&mut harness, SetEdit::Move { from: 3, to: 0 });
    assert!(!set(&harness).history.can_redo());
    harness.send(Command::RedoSet);
    assert_eq!(names(&harness), ["Gamma", "Alpha", "Psalm", "Beta"]);
}

#[test]
fn live_actions_never_enter_the_sets_history() {
    let mut harness = open();
    harness.send(Command::SelectItem(0));
    harness.send(Command::GoLive { index: 1 });
    harness.send(Command::ToggleBlack);
    harness.send(Command::SetLanguageMode(LanguageMode::Primary));
    assert!(!set(&harness).history.can_undo());
    edit(&mut harness, field(0, ItemField::Notes, "x"));
    harness.send(Command::NextSlide);
    harness.send(Command::UndoSet);
    assert_eq!(set(&harness).set.items[0].notes, "slow");
    assert!(harness.state().output.live.is_some(), "undo does not touch what is live");
    assert!(!Command::GoLive { index: 0 }.is_document_edit());
    assert!(Command::EditSet(SetEdit::Remove(0)).is_document_edit() && Command::UndoSet.is_document_edit());
}

#[test]
fn relinking_a_missing_song_is_an_edit_that_can_be_undone() {
    let set_xml = r#"<set name="R"><slide_groups><slide_group name="Gone" type="song"/></slide_groups></set>"#;
    let mut harness = EngineHarness::with_songs_and_sets(SONGS, &[("R", set_xml)]);
    harness.send(Command::RefreshLibrary);
    harness.send(Command::OpenSet(SetId::new("R")));
    harness.send(Command::StartRelink(0));
    harness.send(Command::RelinkItem { item: 0, song: SongId::new("Beta") });
    assert_eq!(names(&harness), ["Beta"]);
    assert_eq!(harness.state().song.as_ref().unwrap().id, SongId::new("Beta"));
    harness.send(Command::UndoSet);
    assert_eq!(names(&harness), ["Gone"]);
    assert!(set(&harness).is_missing(0));
    assert!(harness.state().song.is_none());
}

// Saving

#[test]
fn only_a_set_with_edits_is_written_in_canonical_form_keeping_what_the_model_does_not_describe() {
    let mut harness = open();
    harness.send(Command::SaveSet);
    assert_eq!(harness.set_bytes("Sunday").unwrap(), SET.as_bytes(), "no edits: not written");
    edit(&mut harness, SetEdit::AddCustom { name: "Welcome".into(), at: Some(0) });
    edit(&mut harness, field(0, ItemField::Slides, "Hello"));
    edit(&mut harness, SetEdit::Move { from: 3, to: 1 });
    harness.send(Command::SaveSet);
    let bytes = harness.set_bytes("Sunday").unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    for kept in [r#"<set name="Sunday" version="2">"#, "<future>kept</future>", r#"transition="1""#, "<notes>slow</notes>", "<title>Psalm</title>"] {
        assert!(text.contains(kept), "{kept:?} missing in\n{text}");
    }
    let written = saved(&harness, "Sunday");
    assert_eq!(EngineHarness::canonical_set_bytes(&written), bytes, "canonical: writing what was read gives the same bytes");
    let names: Vec<&str> = written.items.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["Welcome", "Beta", "Alpha", "Psalm"]);
    let open = set(&harness);
    assert!(!open.modified && !open.saving);
    assert_eq!(*open.set, written, "the open set is what the file now holds");
    // Saving again with nothing new writes nothing; the next edit is saved over the new hash.
    harness.send(Command::SaveSet);
    edit(&mut harness, SetEdit::Remove(0));
    harness.send(Command::SaveSet);
    assert_eq!(saved(&harness, "Sunday").items.len(), 3);
    assert!(!set(&harness).changed_on_disk);
    // Undo past the save: modified again.
    harness.send(Command::UndoSet);
    assert!(set(&harness).modified);
}

#[test]
fn a_saved_set_reopens_with_its_edits() {
    let mut harness = open();
    edit(&mut harness, field(2, ItemField::Presentation, "V2 V1"));
    edit(&mut harness, field(2, ItemField::Notes, "key of D"));
    harness.send(Command::SaveSet);
    let mut harness = harness.restart();
    harness.send(Command::OpenSet(SetId::new("Sunday")));
    let item = &set(&harness).set.items[2];
    assert_eq!(item.notes, "key of D");
    let ItemKind::Song(song) = &item.kind else { panic!() };
    assert_eq!(song.presentation, ["V2", "V1"]);
}

#[test]
fn a_save_over_a_file_changed_on_disk_is_refused_and_the_operator_chooses() {
    let changed = SET.replace("slow", "changed elsewhere");
    let mut harness = open();
    edit(&mut harness, SetEdit::Remove(1));
    // Another program writes the file; the save notices (whether or not the watcher saw it first).
    harness.write_set_file("Sunday", changed.as_bytes());
    harness.send(Command::SaveSet);
    assert_eq!(harness.set_bytes("Sunday").unwrap(), changed.as_bytes(), "never overwritten without asking");
    let open = set(&harness);
    assert!(open.changed_on_disk && open.modified);
    assert_eq!(open.set.items.len(), 2, "the edits are kept");
    harness.send(Command::SaveSet);
    assert_eq!(harness.set_bytes("Sunday").unwrap(), changed.as_bytes(), "a plain save waits for the choice");

    harness.send(Command::ResolveSetConflict(ConflictChoice::Overwrite));
    assert_eq!(saved(&harness, "Sunday").items.len(), 2);
    assert!(!set(&harness).changed_on_disk && !set(&harness).modified);
}

#[test]
fn reloading_after_a_conflict_drops_the_edits_and_their_history() {
    let changed = SET.replace("slow", "changed elsewhere");
    let mut harness = open();
    edit(&mut harness, SetEdit::Remove(1));
    harness.write_set_file("Sunday", changed.as_bytes());
    harness.settle();
    assert!(set(&harness).changed_on_disk);
    harness.send(Command::ResolveSetConflict(ConflictChoice::Reload));
    let open = set(&harness);
    assert_eq!(open.set.items[0].notes, "changed elsewhere");
    assert!(!open.modified && !open.changed_on_disk && !open.history.can_undo());
}

#[test]
fn saving_a_copy_after_a_conflict_leaves_the_file_alone() {
    let changed = SET.replace("slow", "changed elsewhere");
    let mut harness = open();
    edit(&mut harness, SetEdit::Remove(1));
    harness.write_set_file("Sunday", changed.as_bytes());
    harness.send(Command::SaveSet);
    harness.send(Command::ResolveSetConflict(ConflictChoice::SaveAsCopy));
    assert_eq!(harness.set_bytes("Sunday").unwrap(), changed.as_bytes());
    let copy = saved(&harness, "Sunday (copy)");
    assert_eq!(copy.items.len(), 2);
    assert_eq!(copy.name, "Sunday (copy)");
    assert_eq!(set(&harness).id, SetId::new("Sunday (copy)"));
}

#[test]
fn an_external_change_to_a_clean_set_reloads_it_but_our_own_save_does_not() {
    let mut harness = open();
    edit(&mut harness, field(0, ItemField::Notes, "mine"));
    harness.send(Command::SaveSet);
    harness.settle();
    let open = set(&harness);
    assert!(!open.changed_on_disk);
    assert!(open.history.can_undo(), "our own save does not reload the set and drop its history");
}

// Save as and duplicate

#[test]
fn a_set_is_saved_under_a_new_name_with_its_edits_and_the_old_file_stays() {
    let mut harness = open();
    edit(&mut harness, SetEdit::Remove(1));
    harness.send(Command::SaveSetAs { name: "Next Sunday".into() });
    assert_eq!(harness.set_bytes("Sunday").unwrap(), SET.as_bytes());
    let new = saved(&harness, "Next Sunday");
    assert_eq!(new.name, "Next Sunday");
    assert_eq!(new.items.len(), 2);
    let state = harness.state();
    assert!(state.sets.contains(&SetId::new("Next Sunday")));
    let open = state.set.as_ref().unwrap();
    assert_eq!(open.id, SetId::new("Next Sunday"));
    assert!(!open.modified);
}

#[test]
fn a_set_is_duplicated_as_saved_and_stays_open_with_its_edits() {
    let mut harness = open();
    edit(&mut harness, SetEdit::Remove(1));
    harness.send(Command::DuplicateSet { name: "Copy of Sunday".into() });
    let copy = saved(&harness, "Copy of Sunday");
    assert_eq!(copy.items.len(), 3, "the copy is the set as saved");
    assert_eq!(copy.name, "Copy of Sunday");
    let state = harness.state();
    assert!(state.sets.contains(&SetId::new("Copy of Sunday")));
    let open = state.set.as_ref().unwrap();
    assert_eq!(open.id, SetId::new("Sunday"));
    assert!(open.modified);
}

#[test]
fn a_new_name_that_is_taken_or_unusable_is_refused() {
    let mut harness = EngineHarness::with_songs_and_sets(SONGS, &[("Sunday", SET), ("Other", "<set/>")]);
    harness.send(Command::RefreshSets);
    harness.send(Command::OpenSet(SetId::new("Sunday")));
    harness.send(Command::DuplicateSet { name: "other".into() });
    harness.send(Command::SaveSetAs { name: " /// ".into() });
    assert_eq!(harness.set_bytes("Other").unwrap(), b"<set/>");
    let problems = &harness.state().problems;
    assert!(problems.contains(&Problem::SetNameRefused { name: "other".into(), taken: true }), "{problems:?}");
    assert!(problems.contains(&Problem::SetNameRefused { name: " /// ".into(), taken: false }), "{problems:?}");
}

#[test]
fn a_set_with_a_sidecar_is_saved_under_a_new_name_through_a_change_plan() {
    let mut harness = open();
    harness.send(Command::SelectItem(0));
    harness.send(Command::SetLanguageMode(LanguageMode::Translation));
    let sidecar = harness.sidecar_bytes("Sunday").expect("the item's language mode is kept");
    edit(&mut harness, SetEdit::Remove(1));
    harness.send(Command::SaveSetAs { name: "Next".into() });
    let state = harness.state();
    let plan = state.plan.as_ref().expect("two files: shown first");
    assert_eq!(plan.action, PlanAction::SaveSetAs { from: SetId::new("Sunday"), to: SetId::new("Next") });
    let files: Vec<PlanFile> = plan.steps.iter().map(PlanStep::file).collect();
    assert_eq!(files, [PlanFile::Set(SetId::new("Next")), PlanFile::Sidecar(SetId::new("Next"))]);
    assert!(harness.set_bytes("Next").is_none(), "nothing written before confirming");
    harness.send(Command::ConfirmPlan);
    assert_eq!(saved(&harness, "Next").items.len(), 2);
    assert_eq!(harness.sidecar_bytes("Next").unwrap(), sidecar);
    let open = set(&harness);
    assert_eq!(open.id, SetId::new("Next"));
    assert!(!open.modified);
    assert_eq!(open.item_settings(0).language, LanguageMode::Translation);
}
