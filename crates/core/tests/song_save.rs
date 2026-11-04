//! Seam 1, the engine: canonical save of songs (#19). Only edited songs are written, saves are
//! conditional on the hash read, and a file changed on disk is the operator's choice.

use serviceflow_core::command::{Command, ConflictChoice};
use serviceflow_core::edit::{SongEdit, SongField};
use serviceflow_core::ids::{ContentHash, SongId};
use serviceflow_core::state::Problem;
use serviceflow_testkit::EngineHarness;

const SONG: &str = "<song><title>Hymn</title><presentation>1 C</presentation><zeta>keep me</zeta>\
<lyrics>[1]\n First line   \n[C]\n Chorus line</lyrics></song>";

fn id() -> SongId {
    SongId::new("Hymn")
}

fn harness() -> EngineHarness {
    let mut harness = EngineHarness::with_songs(&[("Hymn", SONG)]);
    harness.send(Command::OpenSong(id()));
    harness
}

fn bytes(harness: &EngineHarness, id: &SongId) -> Vec<u8> {
    harness.song_bytes(id).expect("the song is stored")
}

fn file(harness: &EngineHarness) -> String {
    String::from_utf8(bytes(harness, &id())).unwrap()
}

fn retitle(harness: &mut EngineHarness, title: &str) {
    harness.send(Command::EditSong(SongEdit::Field(SongField::Title, title.to_owned())));
}

/// Someone else writes the file meanwhile.
fn change_on_disk(harness: &EngineHarness, xml: &str) {
    harness.write_song_file(&id(), xml.as_bytes());
}

#[test]
fn opening_songs_never_writes_them() {
    let mut harness = EngineHarness::with_fixtures();
    let ids = harness.song_ids();
    let before: Vec<_> = ids.iter().map(|id| bytes(&harness, id)).collect();
    for id in &ids {
        harness.send(Command::OpenSong(id.clone()));
        harness.send(Command::SaveSong(id.clone()));
    }
    let after: Vec<_> = ids.iter().map(|id| bytes(&harness, id)).collect();
    assert_eq!(after, before);
}

#[test]
fn a_song_without_edits_is_not_written() {
    let mut harness = harness();
    // Edited and undone: nothing to save either.
    retitle(&mut harness, "Changed");
    harness.send(Command::Undo);
    assert!(!harness.state().draft().unwrap().is_modified());
    harness.send(Command::SaveSong(id()));
    assert_eq!(file(&harness), SONG);
}

#[test]
fn saving_writes_the_canonical_form_and_marks_the_song_clean() {
    let mut harness = harness();
    retitle(&mut harness, "Hymn & more");
    harness.send(Command::SaveSong(id()));
    let written = file(&harness);
    assert!(written.contains("<title>Hymn &amp; more</title>\r\n"), "{written}");
    assert!(written.contains("<presentation>V1 C</presentation>"), "{written}");
    assert!(written.contains("<lyrics>[V1]\r\n First line\r\n[C]\r\n Chorus line</lyrics>"), "{written}");
    assert!(written.contains("  <zeta>keep me</zeta>\r\n</song>\r\n"), "unknown elements survive: {written}");

    let state = harness.state();
    let draft = state.draft().unwrap();
    assert!(!draft.is_modified());
    assert!(draft.can_undo(), "the undo history stays");
    let open = state.song.as_ref().unwrap();
    assert_eq!(open.hash, ContentHash::of(written.as_bytes()));
    assert_eq!(open.song.lyrics, "[V1]\n First line\n[C]\n Chorus line", "the editor shows the renamed tags");
    assert_eq!(state.library.title(&id()), "Hymn & more");
    assert!(state.problems.is_empty(), "{:?}", state.problems);
}

#[test]
fn a_second_save_uses_the_hash_of_the_first() {
    let mut harness = harness();
    retitle(&mut harness, "One");
    harness.send(Command::SaveSong(id()));
    retitle(&mut harness, "Two");
    harness.send(Command::SaveSong(id()));
    assert!(file(&harness).contains("<title>Two</title>"));
    assert!(harness.state().save_conflicts.is_empty());
    // Undo past the save: modified again, and saved again.
    harness.send(Command::Undo);
    harness.send(Command::SaveSong(id()));
    assert!(file(&harness).contains("<title>One</title>"));
}

#[test]
fn a_file_changed_on_disk_is_not_overwritten() {
    let mut harness = harness();
    retitle(&mut harness, "Mine");
    change_on_disk(&harness, "<song><title>Theirs</title></song>");
    harness.send(Command::SaveSong(id()));
    assert_eq!(file(&harness), "<song><title>Theirs</title></song>");
    let state = harness.state();
    assert_eq!(state.save_conflicts, [id()]);
    assert!(state.has_save_conflict());
    assert!(state.draft().unwrap().is_modified(), "the edits are kept");
    // Saving again does nothing until the operator chooses.
    harness.send(Command::SaveSong(id()));
    assert_eq!(file(&harness), "<song><title>Theirs</title></song>");
}

#[test]
fn a_conflict_can_be_overwritten() {
    let mut harness = harness();
    retitle(&mut harness, "Mine");
    change_on_disk(&harness, "<song><title>Theirs</title></song>");
    harness.send(Command::SaveSong(id()));
    harness.send(Command::ResolveSaveConflict { id: id(), choice: ConflictChoice::Overwrite });
    assert!(file(&harness).contains("<title>Mine</title>"));
    let state = harness.state();
    assert!(state.save_conflicts.is_empty());
    assert!(!state.draft().unwrap().is_modified());
}

#[test]
fn a_conflict_can_be_reloaded_dropping_the_edits() {
    let mut harness = harness();
    retitle(&mut harness, "Mine");
    change_on_disk(&harness, "<song><title>Theirs</title></song>");
    harness.send(Command::SaveSong(id()));
    harness.send(Command::ResolveSaveConflict { id: id(), choice: ConflictChoice::Reload });
    let state = harness.state();
    assert_eq!(state.song.as_ref().unwrap().song.title, "Theirs");
    assert!(state.draft().is_none());
    assert!(state.save_conflicts.is_empty());
    assert_eq!(file(&harness), "<song><title>Theirs</title></song>");
}

#[test]
fn a_conflict_can_be_saved_as_a_copy() {
    let mut harness = EngineHarness::with_songs(&[("Hymns/Hymn", SONG), ("Hymns/Hymn (copy)", SONG)]);
    let id = SongId::new("Hymns/Hymn");
    harness.send(Command::OpenSong(id.clone()));
    retitle(&mut harness, "Mine");
    harness.write_song_file(&id, b"<song><title>Theirs</title></song>");
    harness.send(Command::SaveSong(id.clone()));
    harness.send(Command::ResolveSaveConflict { id: id.clone(), choice: ConflictChoice::SaveAsCopy });

    let copy = SongId::new("Hymns/Hymn (copy 2)");
    assert!(String::from_utf8(bytes(&harness, &copy)).unwrap().contains("<title>Mine</title>"));
    assert_eq!(bytes(&harness, &id), b"<song><title>Theirs</title></song>");
    assert_eq!(bytes(&harness, &SongId::new("Hymns/Hymn (copy)")), SONG.as_bytes(), "an existing copy is not overwritten");
    let state = harness.state();
    assert_eq!(state.song.as_ref().unwrap().id, copy, "the copy is open");
    assert!(!state.drafts.contains_key(&id));
    assert!(state.library.songs.contains(&copy));
}

#[test]
fn copy_names_keep_the_folder_and_extension() {
    assert_eq!(SongId::new("Abide").copy(1), SongId::new("Abide (copy)"));
    assert_eq!(SongId::new("Hymns/Abide.xml").copy(3), SongId::new("Hymns/Abide (copy 3).xml"));
}

#[test]
fn colliding_tags_are_saved_unrenamed_with_a_warning() {
    let xml = "<song><title>T</title><lyrics>[1]\n one\n[V1]\n vee one</lyrics></song>";
    let mut harness = EngineHarness::with_songs(&[("Hymn", xml)]);
    harness.send(Command::OpenSong(id()));
    retitle(&mut harness, "T2");
    harness.send(Command::SaveSong(id()));
    assert!(file(&harness).contains("<lyrics>[1]\r\n one\r\n[V1]\r\n vee one</lyrics>"));
    assert_eq!(harness.state().problems, [Problem::TagsNotRenamed { id: id() }]);
}

#[test]
fn a_set_items_presentation_order_is_not_saved_into_the_song() {
    let set = r#"<set name="S"><slide_groups><slide_group type="song" name="Hymn" path="" presentation="C C"/></slide_groups></set>"#;
    let mut harness = EngineHarness::with_songs_and_sets(&[("Hymn", SONG)], &[("S", set)]);
    harness.send(Command::OpenSet(serviceflow_core::ids::SetId::new("S")));
    harness.send(Command::SelectItem(0));
    assert_eq!(harness.state().song.as_ref().unwrap().song.presentation, ["C", "C"]);
    retitle(&mut harness, "Edited in a set");
    assert_eq!(harness.state().song.as_ref().unwrap().song.presentation, ["C", "C"], "the item's order still shows");
    harness.send(Command::SaveSong(id()));
    assert!(file(&harness).contains("<presentation>V1 C</presentation>"));
}

#[test]
fn a_stale_clean_draft_gives_way_to_the_file() {
    let mut harness = harness();
    retitle(&mut harness, "Saved");
    harness.send(Command::SaveSong(id()));
    change_on_disk(&harness, "<song><title>Theirs</title></song>");
    harness.send(Command::OpenSong(id()));
    assert_eq!(harness.state().song.as_ref().unwrap().song.title, "Theirs");
}
