//! Seam 1, the engine: a set's sidecar follows set edits (#31). Each song item keeps its language
//! mode, transposition and capo when items are moved, removed, inserted, duplicated or relinked
//! (undo and redo too); the sidecar lives in memory with the open set and is written only when
//! the set is saved, or inside the change plan that renames or copies the set or renames a song.

use serviceflow_core::command::Command;
use serviceflow_core::ids::{SetId, SongId};
use serviceflow_core::plan::{PlanAction, PlanFile, PlanStep};
use serviceflow_core::set_edit::SetEdit;
use serviceflow_core::sidecar::{ItemKey, ItemSettings, SetSidecar};
use serviceflow_core::song::LanguageMode;
use serviceflow_core::state::OpenSet;
use serviceflow_testkit::EngineHarness;

const SONGS: &[(&str, &str)] = &[
    ("Gift", "<song><title>Gift</title><key>G</key><lyrics>[V1] (L)\n.G\n 主\n Lord</lyrics></song>"),
    ("Plain", "<song><title>Plain</title><key>C</key><lyrics>[V1]\n.C\n plain</lyrics></song>"),
    ("Other", "<song><title>Other</title><lyrics>[V1]\n other</lyrics></song>"),
];

/// The same song twice, with another song between.
const SET: &str = r#"<set name="Test"><slide_groups>
  <slide_group name="Gift" type="song" path=""/>
  <slide_group name="Plain" type="song" path=""/>
  <slide_group name="Gift" type="song" path=""/>
</slide_groups></set>"#;

/// Another set using the first song.
const SECOND: &str = r#"<set name="Second"><slide_groups>
  <slide_group name="Gift" type="song" path=""/>
</slide_groups></set>"#;

/// First Gift up two, Plain capo 3, second Gift down one and primary only.
const SIDECAR: &[u8] = b"version = 1
[[item]]
song = \"Gift\"
transpose = 2
[[item]]
song = \"Plain\"
capo = 3
[[item]]
song = \"Gift\"
occurrence = 1
transpose = -1
language = \"primary\"
";

const FIRST: ItemSettings = ItemSettings { language: LanguageMode::Both, transpose: 2, capo: None };
const PLAIN: ItemSettings = ItemSettings { language: LanguageMode::Both, transpose: 0, capo: Some(3) };
const THIRD: ItemSettings = ItemSettings { language: LanguageMode::Primary, transpose: -1, capo: None };
const NONE: ItemSettings = ItemSettings { language: LanguageMode::Both, transpose: 0, capo: None };

fn open() -> EngineHarness {
    let mut harness = EngineHarness::with_songs_and_sets(SONGS, &[("Test", SET), ("Second", SECOND)]);
    harness.write_sidecar_file("Test", SIDECAR);
    harness.write_sidecar_file("Second", b"version = 1\n[[item]]\nsong = \"Gift\"\ncapo = 5\n");
    harness.send(Command::RefreshLibrary);
    harness.send(Command::RefreshSets);
    harness.send(Command::OpenSet(SetId::new("Test")));
    harness
}

fn set(harness: &EngineHarness) -> OpenSet {
    harness.state().set.clone().expect("a set is open")
}

/// Every item's settings, in item order.
fn settings(harness: &EngineHarness) -> Vec<ItemSettings> {
    let open = set(harness);
    (0..open.set.items.len()).map(|i| open.item_settings(i)).collect()
}

fn names(harness: &EngineHarness) -> Vec<String> {
    set(harness).set.items.iter().map(|i| i.name.clone()).collect()
}

fn edit(harness: &mut EngineHarness, edit: SetEdit) {
    harness.send(Command::EditSet(edit));
}

fn key(song: &str, occurrence: usize) -> ItemKey {
    ItemKey { song: SongId::new(song), occurrence }
}

fn sidecar(entries: &[(&str, usize, ItemSettings)]) -> SetSidecar {
    let mut sidecar = SetSidecar::default();
    for (song, occurrence, settings) in entries {
        sidecar.set(key(song, *occurrence), *settings);
    }
    sidecar
}

/// The settings each item has after the set is opened again from its files.
fn reopened(harness: &mut EngineHarness, id: &str) -> Vec<ItemSettings> {
    harness.send(Command::OpenSet(SetId::new(id)));
    settings(harness)
}

#[test]
fn moving_an_item_takes_its_settings_along_even_for_the_same_song_and_undo_and_redo_follow() {
    let mut harness = open();
    assert_eq!(settings(&harness), [FIRST, PLAIN, THIRD]);
    edit(&mut harness, SetEdit::Move { from: 2, to: 0 });
    assert_eq!(names(&harness), ["Gift", "Gift", "Plain"]);
    assert_eq!(settings(&harness), [THIRD, FIRST, PLAIN], "the moved Gift is now the first one, with its own settings");
    harness.send(Command::UndoSet);
    assert_eq!(settings(&harness), [FIRST, PLAIN, THIRD]);
    harness.send(Command::RedoSet);
    assert_eq!(settings(&harness), [THIRD, FIRST, PLAIN]);
}

#[test]
fn removing_an_item_keeps_the_other_items_settings_and_undo_brings_its_own_back() {
    let mut harness = open();
    edit(&mut harness, SetEdit::Remove(0));
    assert_eq!(settings(&harness), [PLAIN, THIRD], "the remaining Gift is now its song's first item, still down one");
    harness.send(Command::UndoSet);
    assert_eq!(settings(&harness), [FIRST, PLAIN, THIRD]);
}

#[test]
fn an_inserted_item_starts_at_the_defaults_and_does_not_take_another_items_settings() {
    let mut harness = open();
    edit(&mut harness, SetEdit::AddSong { song: SongId::new("Gift"), at: Some(0) });
    assert_eq!(settings(&harness), [NONE, FIRST, PLAIN, THIRD]);
    harness.send(Command::UndoSet);
    assert_eq!(settings(&harness), [FIRST, PLAIN, THIRD]);
}

#[test]
fn a_duplicated_item_is_a_copy_with_the_same_settings() {
    let mut harness = open();
    edit(&mut harness, SetEdit::Duplicate(1));
    assert_eq!(names(&harness), ["Gift", "Plain", "Plain", "Gift"]);
    assert_eq!(settings(&harness), [FIRST, PLAIN, PLAIN, THIRD]);
    assert_eq!(set(&harness).selected, None, "duplicating does not select");
    // The copy is its own item: changing it leaves the original alone.
    harness.send(Command::SelectItem(2));
    harness.send(Command::SetCapo(Some(1)));
    assert_eq!(settings(&harness)[1], PLAIN);
    assert_eq!(settings(&harness)[2].capo, Some(1));
}

#[test]
fn a_relinked_item_keeps_its_settings_under_its_new_song() {
    let mut harness = open();
    harness.send(Command::StartRelink(2));
    harness.send(Command::RelinkItem { item: 2, song: SongId::new("Other") });
    assert_eq!(names(&harness), ["Gift", "Plain", "Other"]);
    assert_eq!(settings(&harness), [FIRST, PLAIN, THIRD]);
    harness.send(Command::SaveSet);
    assert_eq!(harness.stored_sidecar("Test").unwrap(), sidecar(&[("Gift", 0, FIRST), ("Plain", 0, PLAIN), ("Other", 0, THIRD)]));
}

#[test]
fn settings_are_kept_in_memory_and_written_only_when_the_set_is_saved() {
    let mut harness = open();
    let (set_before, sidecar_before) = (harness.set_bytes("Test"), harness.sidecar_bytes("Test"));
    harness.send(Command::SelectItem(1));
    harness.send(Command::SetTranspose(4));
    harness.send(Command::SelectItem(2));
    harness.send(Command::SetLanguageMode(LanguageMode::Translation));
    assert_eq!(harness.sidecar_bytes("Test"), sidecar_before, "nothing written yet");
    assert!(set(&harness).modified, "the set has unsaved changes");
    assert!(!set(&harness).history.can_undo(), "live actions are not set edits");

    harness.send(Command::SaveSet);
    assert_eq!(harness.set_bytes("Test"), set_before, "only the sidecar changed: the set file is left as it is");
    let plain = ItemSettings { transpose: 4, ..PLAIN };
    let third = ItemSettings { language: LanguageMode::Translation, ..THIRD };
    assert_eq!(harness.stored_sidecar("Test").unwrap(), sidecar(&[("Gift", 0, FIRST), ("Plain", 0, plain), ("Gift", 1, third)]));
    assert!(!set(&harness).modified);
    assert_eq!(reopened(&mut harness, "Test"), [FIRST, plain, third]);
}

#[test]
fn a_saved_reorder_writes_the_set_and_its_sidecar_together() {
    let mut harness = open();
    edit(&mut harness, SetEdit::Move { from: 0, to: 2 });
    assert_eq!(harness.sidecar_bytes("Test").as_deref(), Some(SIDECAR), "nothing written before the save");
    harness.send(Command::SaveSet);
    assert_eq!(harness.saved_set("Test").unwrap().items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["Plain", "Gift", "Gift"]);
    assert_eq!(harness.stored_sidecar("Test").unwrap(), sidecar(&[("Plain", 0, PLAIN), ("Gift", 0, THIRD), ("Gift", 1, FIRST)]));
    assert_eq!(reopened(&mut harness, "Test"), [PLAIN, THIRD, FIRST]);
}

#[test]
fn discarding_the_set_edits_discards_the_settings_changes() {
    let mut harness = open();
    harness.send(Command::SelectItem(0));
    harness.send(Command::SetTranspose(-5));
    edit(&mut harness, SetEdit::Remove(1));
    // Opening the set again (or another one) drops what was not saved.
    assert_eq!(reopened(&mut harness, "Test"), [FIRST, PLAIN, THIRD]);
    assert_eq!(harness.sidecar_bytes("Test").as_deref(), Some(SIDECAR));

    harness.send(Command::SelectItem(2));
    harness.send(Command::SetCapo(Some(7)));
    let mut harness = harness.restart();
    assert_eq!(reopened(&mut harness, "Test"), [FIRST, PLAIN, THIRD], "quitting without saving keeps the files");
}

#[test]
fn undoing_a_set_edit_does_not_undo_a_later_transposition() {
    let mut harness = open();
    edit(&mut harness, SetEdit::Move { from: 2, to: 0 });
    harness.send(Command::SelectItem(0));
    harness.send(Command::SetTranspose(3));
    harness.send(Command::UndoSet);
    assert_eq!(settings(&harness), [FIRST, PLAIN, ItemSettings { transpose: 3, ..THIRD }], "the transposition stays with its item");
}

#[test]
fn a_change_back_to_what_is_saved_leaves_the_set_unmodified() {
    let mut harness = open();
    harness.send(Command::SelectItem(0));
    harness.send(Command::SetTranspose(5));
    assert!(set(&harness).modified);
    harness.send(Command::SetTranspose(2));
    assert!(!set(&harness).modified);
}

#[test]
fn save_as_takes_the_unsaved_settings_and_duplicate_the_saved_ones() {
    let mut harness = open();
    harness.send(Command::SelectItem(1));
    harness.send(Command::SetCapo(None));

    harness.send(Command::DuplicateSet { name: "Copy".into() });
    let files: Vec<PlanFile> = harness.state().plan.as_ref().expect("two files: a plan").steps.iter().map(PlanStep::file).collect();
    assert_eq!(files, [PlanFile::Set(SetId::new("Copy")), PlanFile::Sidecar(SetId::new("Copy"))]);
    harness.send(Command::ConfirmPlan);
    let as_saved = sidecar(&[("Gift", 0, FIRST), ("Plain", 0, PLAIN), ("Gift", 1, THIRD)]);
    assert_eq!(harness.stored_sidecar("Copy").unwrap(), as_saved, "the copy is of the set as saved");

    harness.send(Command::SaveSetAs { name: "New".into() });
    harness.send(Command::ConfirmPlan);
    assert_eq!(harness.stored_sidecar("New").unwrap(), sidecar(&[("Gift", 0, FIRST), ("Gift", 1, THIRD)]));
    assert_eq!(harness.sidecar_bytes("Test").as_deref(), Some(SIDECAR), "the old set's files are as they were");
    assert_eq!(set(&harness).id, SetId::new("New"));
    assert_eq!(settings(&harness), [FIRST, NONE, THIRD]);
}

#[test]
fn renaming_a_set_moves_its_sidecar_in_the_same_plan() {
    let mut harness = open();
    harness.send(Command::SelectItem(0));
    harness.send(Command::SetTranspose(1));
    harness.send(Command::RenameSet { name: "Evening".into() });
    let plan = harness.state().plan.clone().expect("a plan is shown");
    assert_eq!(plan.action, PlanAction::RenameSet { from: SetId::new("Test"), to: SetId::new("Evening") });
    let files: Vec<PlanFile> = plan.steps.iter().map(PlanStep::file).collect();
    assert_eq!(files, [PlanFile::Set(SetId::new("Test")), PlanFile::Sidecar(SetId::new("Evening")), PlanFile::Sidecar(SetId::new("Test"))]);
    assert!(harness.set_bytes("Evening").is_none(), "nothing written before confirming");

    let set_bytes = harness.set_bytes("Test");
    harness.send(Command::ConfirmPlan);
    assert_eq!(harness.set_bytes("Evening"), set_bytes, "the set file is moved as it is");
    assert!(harness.set_bytes("Test").is_none());
    assert!(harness.sidecar_bytes("Test").is_none(), "no sidecar left behind");
    assert_eq!(harness.stored_sidecar("Evening").unwrap(), sidecar(&[("Gift", 0, FIRST), ("Plain", 0, PLAIN), ("Gift", 1, THIRD)]), "as saved");

    // The open set is the renamed one and keeps its unsaved change.
    let open = set(&harness);
    assert_eq!(open.id, SetId::new("Evening"));
    assert!(open.modified);
    assert_eq!(open.item_settings(0).transpose, 1);
    assert!(harness.state().sets.contains(&SetId::new("Evening")));
}

#[test]
fn renaming_a_set_to_a_taken_or_empty_name_is_refused() {
    let mut harness = open();
    harness.send(Command::RenameSet { name: "second".into() });
    harness.send(Command::RenameSet { name: " / ".into() });
    assert!(harness.state().plan.is_none());
    assert_eq!(harness.state().problems.len(), 2);
}

#[test]
fn renaming_a_song_rewrites_the_sidecar_of_every_set_using_it_in_the_same_plan() {
    let mut harness = open();
    // An unsaved change in the open set stays unsaved: the plan rewrites the files as saved.
    harness.send(Command::SelectItem(2));
    harness.send(Command::SetTranspose(6));
    harness.send(Command::RenameSong { from: SongId::new("Gift"), to: SongId::new("Hymns/Gift") });
    let files: Vec<PlanFile> = harness.state().plan.as_ref().expect("a plan").steps.iter().map(PlanStep::file).collect();
    assert_eq!(
        files,
        [
            PlanFile::Set(SetId::new("Second")),
            PlanFile::Sidecar(SetId::new("Second")),
            PlanFile::Set(SetId::new("Test")),
            PlanFile::Sidecar(SetId::new("Test")),
            PlanFile::Song(SongId::new("Gift")),
        ]
    );
    harness.send(Command::ConfirmPlan);
    assert!(harness.state().problems.is_empty(), "{:?}", harness.state().problems);

    assert_eq!(harness.stored_sidecar("Test").unwrap(), sidecar(&[("Hymns/Gift", 0, FIRST), ("Plain", 0, PLAIN), ("Hymns/Gift", 1, THIRD)]));
    assert_eq!(harness.stored_sidecar("Second").unwrap(), sidecar(&[("Hymns/Gift", 0, ItemSettings { capo: Some(5), ..NONE })]));

    let open = set(&harness);
    assert_eq!(settings(&harness), [FIRST, PLAIN, ItemSettings { transpose: 6, ..THIRD }]);
    assert!(open.modified, "the unsaved transposition is still unsaved");
    harness.send(Command::SaveSet);
    assert_eq!(harness.stored_sidecar("Test").unwrap().get(&key("Hymns/Gift", 1)).transpose, 6);
}

#[test]
fn renaming_a_song_to_a_name_a_missing_item_already_has_keeps_each_items_settings() {
    // Item 1 refers to "Lost", which is not in the library; renaming Gift to it makes two items
    // for the same song, so the occurrences change.
    let set_xml = r#"<set name="Mix"><slide_groups>
  <slide_group name="Lost" type="song" path=""/>
  <slide_group name="Gift" type="song" path=""/>
</slide_groups></set>"#;
    let mut harness = EngineHarness::with_songs_and_sets(SONGS, &[("Mix", set_xml)]);
    harness.write_sidecar_file("Mix", b"version = 1\n[[item]]\nsong = \"Lost\"\ncapo = 2\n[[item]]\nsong = \"Gift\"\ntranspose = 4\n");
    harness.send(Command::RenameSong { from: SongId::new("Gift"), to: SongId::new("Lost") });
    harness.send(Command::ConfirmPlan);
    let lost = |occurrence| key("Lost", occurrence);
    let stored = harness.stored_sidecar("Mix").unwrap();
    assert_eq!((stored.get(&lost(0)).capo, stored.get(&lost(1)).transpose), (Some(2), 4));
}
