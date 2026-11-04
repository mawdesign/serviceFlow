//! Seam 1, the engine: changes made on disk by other programs reach the engine as commands
//! (#13). The library and search index follow, an open clean document reloads, and an open
//! document with unsaved edits is marked as changed on disk instead of being overwritten.

use serviceflow_core::command::{Command, ConflictChoice};
use serviceflow_core::edit::{SongEdit, SongField};
use serviceflow_core::ids::{SetId, SongId};
use serviceflow_core::state::Problem;
use serviceflow_testkit::EngineHarness;

fn song(title: &str, lyrics: &str) -> String {
    format!("<song><title>{title}</title><lyrics>{lyrics}</lyrics></song>")
}

fn id() -> SongId {
    SongId::new("Hymn")
}

fn songs() -> [(&'static str, String); 2] {
    [("Hymn", song("Hymn", "[V1]\n first line")), ("Other", song("Other", "[V1]\n other line"))]
}

fn harness() -> EngineHarness {
    let songs = songs();
    let songs: Vec<(&str, &str)> = songs.iter().map(|(i, x)| (*i, x.as_str())).collect();
    let mut harness = EngineHarness::with_songs(&songs);
    harness.send(Command::RefreshLibrary);
    harness
}

fn open() -> EngineHarness {
    let mut harness = harness();
    harness.send(Command::OpenSong(id()));
    harness
}

fn first_line(harness: &EngineHarness) -> String {
    harness.state().song.as_ref().unwrap().slides[0].lines[0].clone()
}

fn retitle(harness: &mut EngineHarness, title: &str) {
    harness.send(Command::EditSong(SongEdit::Field(SongField::Title, title.to_owned())));
}

fn search(harness: &mut EngineHarness, query: &str) -> Vec<String> {
    harness.send(Command::Search(query.to_owned()));
    harness.state().library.results.iter().map(ToString::to_string).collect()
}

#[test]
fn a_song_added_on_disk_appears_in_the_library_and_search() {
    let mut harness = harness();
    harness.write_song_file(&SongId::new("Hymns/Brand New"), song("Brand New Song", "[V1]\n shiny words").as_bytes());
    harness.settle();
    let state = harness.state();
    assert_eq!(state.library.songs.len(), 3);
    assert!(state.library.songs.contains(&SongId::new("Hymns/Brand New")));
    assert_eq!(state.library.title(&SongId::new("Hymns/Brand New")), "Brand New Song");
    assert_eq!(search(&mut harness, "shiny"), ["Hymns/Brand New"]);
}

#[test]
fn a_song_edited_on_disk_is_searchable_by_its_new_text() {
    let mut harness = harness();
    harness.write_song_file(&SongId::new("Other"), song("Renamed By Hand", "[V1]\n entirely different").as_bytes());
    harness.settle();
    assert_eq!(harness.state().library.title(&SongId::new("Other")), "Renamed By Hand");
    assert_eq!(search(&mut harness, "entirely"), ["Other"]);
    assert_eq!(search(&mut harness, "other line"), Vec::<String>::new());
}

#[test]
fn a_song_deleted_on_disk_leaves_the_library() {
    let mut harness = harness();
    harness.remove_song_file(&SongId::new("Other"));
    harness.settle();
    assert_eq!(harness.state().library.songs, [id()]);
    assert_eq!(search(&mut harness, "other"), Vec::<String>::new());
}

#[test]
fn a_rename_on_disk_is_a_removal_and_an_addition() {
    let mut harness = harness();
    let bytes = harness.song_bytes(&SongId::new("Other")).unwrap();
    harness.remove_song_file(&SongId::new("Other"));
    harness.write_song_file(&SongId::new("Other Renamed"), &bytes);
    harness.settle();
    assert_eq!(harness.state().library.songs, [id(), SongId::new("Other Renamed")]);
    assert_eq!(search(&mut harness, "other line"), ["Other Renamed"]);
}

#[test]
fn the_open_song_reloads_when_it_changes_on_disk_and_its_undo_history_is_cleared() {
    let mut harness = open();
    // A clean draft with undo history: edited, then undone.
    retitle(&mut harness, "Edited");
    harness.send(Command::Undo);
    assert!(harness.state().draft().is_some());
    harness.write_song_file(&id(), song("Hymn", "[V1]\n written in OpenSong").as_bytes());
    harness.settle();
    assert_eq!(first_line(&harness), "written in OpenSong");
    assert!(harness.state().draft().is_none(), "no draft, so no undo history");
    assert!(harness.state().save_conflicts.is_empty());
    // Undo has nothing to undo.
    harness.send(Command::Undo);
    assert_eq!(first_line(&harness), "written in OpenSong");
}

#[test]
fn a_reload_does_not_change_what_is_live() {
    let mut harness = open();
    harness.send(Command::GoLive { index: 0 });
    harness.write_song_file(&id(), song("Hymn", "[V1]\n written in OpenSong").as_bytes());
    harness.settle();
    assert_eq!(harness.state().output.live.as_ref().unwrap().slide.lines[0], "first line");
    assert_eq!(first_line(&harness), "written in OpenSong");
}

#[test]
fn an_open_song_with_edits_is_marked_changed_on_disk_and_not_overwritten() {
    let mut harness = open();
    retitle(&mut harness, "My edit");
    let theirs = song("Theirs", "[V1]\n their words");
    harness.write_song_file(&id(), theirs.as_bytes());
    harness.settle();
    let state = harness.state();
    assert_eq!(state.save_conflicts, [id()]);
    assert!(state.has_save_conflict());
    // The edits are still there, and the file is theirs.
    assert_eq!(state.draft().unwrap().song.title, "My edit");
    assert_eq!(harness.song_bytes(&id()).unwrap(), theirs.as_bytes());
    // Saving does nothing until the operator chooses.
    harness.send(Command::SaveSong(id()));
    assert_eq!(harness.song_bytes(&id()).unwrap(), theirs.as_bytes());
    // Another change meanwhile is still one pending conflict.
    harness.write_song_file(&id(), song("Theirs again", "[V1]\n more").as_bytes());
    harness.settle();
    assert_eq!(harness.state().save_conflicts, [id()]);
}

#[test]
fn the_operator_can_reload_overwrite_or_save_a_copy_after_such_a_change() {
    let edited = || {
        let mut harness = open();
        retitle(&mut harness, "My edit");
        harness.write_song_file(&id(), song("Theirs", "[V1]\n their words").as_bytes());
        harness.settle();
        harness
    };

    let mut harness = edited();
    harness.send(Command::ResolveSaveConflict { id: id(), choice: ConflictChoice::Reload });
    assert_eq!(first_line(&harness), "their words");
    assert!(harness.state().draft().is_none());

    let mut harness = edited();
    harness.send(Command::ResolveSaveConflict { id: id(), choice: ConflictChoice::Overwrite });
    assert!(String::from_utf8(harness.song_bytes(&id()).unwrap()).unwrap().contains("<title>My edit</title>"));
    assert!(harness.state().save_conflicts.is_empty());
    assert!(!harness.state().draft().unwrap().is_modified());

    let mut harness = edited();
    harness.send(Command::ResolveSaveConflict { id: id(), choice: ConflictChoice::SaveAsCopy });
    assert!(harness.song_ids().contains(&SongId::new("Hymn (copy)")));
    assert!(String::from_utf8(harness.song_bytes(&id()).unwrap()).unwrap().contains("<title>Theirs</title>"));
}

#[test]
fn saving_is_not_mistaken_for_a_change_on_disk() {
    let mut harness = open();
    retitle(&mut harness, "Saved by me");
    harness.send(Command::SaveSong(id()));
    harness.settle();
    let state = harness.state();
    assert!(state.save_conflicts.is_empty());
    assert!(state.problems.is_empty(), "{:?}", state.problems);
    // The history survives: undo past the save is still possible.
    assert!(state.draft().unwrap().can_undo());
}

#[test]
fn a_song_with_edits_that_is_not_open_is_also_marked() {
    let mut harness = open();
    retitle(&mut harness, "My edit");
    harness.send(Command::OpenSong(SongId::new("Other")));
    harness.write_song_file(&id(), song("Theirs", "[V1]\n their words").as_bytes());
    harness.settle();
    assert_eq!(harness.state().save_conflicts, [id()]);
    assert!(harness.state().drafts.contains_key(&id()));
}

#[test]
fn a_clean_draft_of_a_song_that_is_not_open_is_dropped() {
    let mut harness = open();
    retitle(&mut harness, "My edit");
    harness.send(Command::Undo);
    harness.send(Command::OpenSong(SongId::new("Other")));
    harness.write_song_file(&id(), song("Theirs", "[V1]\n their words").as_bytes());
    harness.settle();
    assert!(harness.state().drafts.is_empty());
    harness.send(Command::OpenSong(id()));
    assert_eq!(first_line(&harness), "their words");
}

#[test]
fn the_open_song_deleted_on_disk_closes_when_clean_and_is_marked_when_edited() {
    let mut harness = open();
    harness.remove_song_file(&id());
    harness.settle();
    assert!(harness.state().song.is_none());

    let mut harness = open();
    retitle(&mut harness, "My edit");
    harness.remove_song_file(&id());
    harness.settle();
    let state = harness.state();
    assert_eq!(state.save_conflicts, [id()]);
    assert_eq!(state.song.as_ref().unwrap().id, id());
}

#[test]
fn a_song_that_became_unreadable_is_reported_and_keeps_what_is_open() {
    let mut harness = open();
    harness.write_song_file(&id(), b"<song><title>Half written");
    harness.settle();
    let state = harness.state();
    assert!(state.problems.iter().any(|p| matches!(p, Problem::SongUnreadable { id: i, .. } if *i == id())));
    assert_eq!(first_line(&harness), "first line");
}

const SET: &str = r#"<set name="Test"><slide_groups>
  <slide_group name="Hymn" type="song"/>
  <slide_group name="Other" type="song"/>
</slide_groups></set>"#;

const SET_CHANGED: &str = r#"<set name="Test"><slide_groups>
  <slide_group name="Other" type="song"/>
  <slide_group name="Hymn" type="song"/>
  <slide_group name="Missing" type="song"/>
</slide_groups></set>"#;

fn open_set() -> EngineHarness {
    let songs = songs();
    let songs: Vec<(&str, &str)> = songs.iter().map(|(i, x)| (*i, x.as_str())).collect();
    let mut harness = EngineHarness::with_songs_and_sets(&songs, &[("Test", SET)]);
    harness.send(Command::RefreshLibrary);
    harness.send(Command::RefreshSets);
    harness.send(Command::OpenSet(SetId::new("Test")));
    harness
}

#[test]
fn a_set_added_or_deleted_on_disk_changes_the_set_list() {
    let mut harness = open_set();
    harness.write_set_file(&SetId::new("Easter"), SET.as_bytes());
    harness.settle();
    assert_eq!(harness.state().sets, [SetId::new("Easter"), SetId::new("Test")]);
    harness.remove_set_file(&SetId::new("Easter"));
    harness.settle();
    assert_eq!(harness.state().sets, [SetId::new("Test")]);
}

#[test]
fn the_open_set_reloads_when_it_changes_on_disk() {
    let mut harness = open_set();
    harness.send(Command::SelectItem(1));
    assert_eq!(harness.state().song.as_ref().unwrap().id, SongId::new("Other"));
    harness.write_set_file(&SetId::new("Test"), SET_CHANGED.as_bytes());
    harness.settle();
    let state = harness.state();
    let set = state.set.as_ref().unwrap();
    assert_eq!(set.set.items.len(), 3);
    assert_eq!(set.set.items[0].name, "Other");
    assert!(!set.changed_on_disk);
    // The missing song is noticed, and the selected item shows its own song.
    assert!(set.is_missing(2));
    assert_eq!(set.selected, Some(1));
    assert_eq!(state.song.as_ref().unwrap().id, id());
}

#[test]
fn a_set_with_unsaved_relinking_is_marked_changed_on_disk_instead_of_reloaded() {
    let mut harness = open_set();
    harness.send(Command::RelinkItem { item: 1, song: id() });
    assert!(harness.state().set.as_ref().unwrap().modified);
    harness.write_set_file(&SetId::new("Test"), SET_CHANGED.as_bytes());
    harness.settle();
    let state = harness.state();
    let set = state.set.as_ref().unwrap();
    assert!(set.changed_on_disk);
    assert_eq!(set.set.items.len(), 2, "the operator's version is kept");
    // Opening the set again takes the disk's version and clears the mark.
    harness.send(Command::OpenSet(SetId::new("Test")));
    let state = harness.state();
    let set = state.set.as_ref().unwrap();
    assert_eq!(set.set.items.len(), 3);
    assert!(!set.changed_on_disk && !set.modified);
}

#[test]
fn a_song_added_on_disk_clears_a_missing_marker_in_the_open_set() {
    let mut harness = open_set();
    harness.write_set_file(&SetId::new("Test"), SET_CHANGED.as_bytes());
    harness.settle();
    assert!(harness.state().set.as_ref().unwrap().is_missing(2));
    harness.write_song_file(&SongId::new("Missing"), song("Missing", "[V1]\n found").as_bytes());
    harness.settle();
    assert!(!harness.state().set.as_ref().unwrap().is_missing(2));
}
