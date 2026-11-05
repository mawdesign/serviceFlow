//! Seam 1, the engine: replacing the library from a zip file (browser storage, #26). The UI only
//! sends the command (ADR-0001); the runtime performs the import over the engine's own storage.

use std::sync::Arc;

use serviceflow_core::command::Command;
use serviceflow_core::edit::{SongEdit, SongField};
use serviceflow_core::ids::{SetId, SongId};
use serviceflow_core::state::{ArchiveImport, Problem};
use serviceflow_testkit::EngineHarness;

const OLD: &str = "<song><title>Old</title><lyrics>[V1]\n old</lyrics></song>";
const NEW: &str = "<song><title>New</title><lyrics>[V1]\n new</lyrics></song>";

#[test]
fn importing_a_zip_replaces_the_library_and_reads_it_again() {
    let mut harness = EngineHarness::with_zip_import(&[("Old", OLD)]);
    harness.send(Command::RefreshLibrary).send(Command::OpenSong(SongId::new("Old")));
    harness.send(Command::EditSong(SongEdit::Field(SongField::Title, "Edited".to_owned())));
    let zip = EngineHarness::zip_of(&[("Songs/New", NEW), ("Sets/Sunday", r#"<set name="Sunday"><slide_groups/></set>"#)]);
    harness.send(Command::ImportArchive(Arc::from(zip)));

    let state = harness.state();
    assert_eq!(state.archive_import, Some(ArchiveImport::Imported(2)));
    assert_eq!(state.library.songs, [SongId::new("New")]);
    assert_eq!(state.sets, [SetId::new("Sunday")]);
    // What was open belonged to the old library.
    assert!(state.song.is_none());
    assert!(state.drafts.is_empty());
    assert_eq!(harness.song_bytes(&SongId::new("Old")), None);
}

#[test]
fn a_storage_that_cannot_import_reports_a_problem_and_keeps_everything() {
    let mut harness = EngineHarness::with_songs(&[("Old", OLD)]);
    harness.send(Command::RefreshLibrary).send(Command::OpenSong(SongId::new("Old")));
    harness.send(Command::ImportArchive(Arc::from(EngineHarness::zip_of(&[("Songs/New", NEW)]))));
    let state = harness.state();
    assert_eq!(state.archive_import, None);
    assert!(matches!(state.problems.last(), Some(Problem::ArchiveUnimported { .. })));
    assert_eq!(state.library.songs, [SongId::new("Old")]);
    assert!(state.song.is_some());
}

#[test]
fn a_zip_that_is_not_an_opensong_folder_changes_nothing() {
    let mut harness = EngineHarness::with_zip_import(&[("Old", OLD)]);
    harness.send(Command::RefreshLibrary);
    harness.send(Command::ImportArchive(Arc::from(EngineHarness::zip_of(&[("notes.txt", "hello")]))));
    assert!(matches!(harness.state().problems.last(), Some(Problem::ArchiveUnimported { .. })));
    assert!(harness.song_bytes(&SongId::new("Old")).is_some());
}
