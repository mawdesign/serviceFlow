//! Seam 1, the engine: editing a song, undo/redo and the editor warnings (#18).

use serviceflow_core::command::Command;
use serviceflow_core::edit::{SongEdit, SongField};
use serviceflow_core::ids::SongId;
use serviceflow_core::song::SongWarning;
use serviceflow_testkit::EngineHarness;

const SONG: &str = "<song><title>Hymn</title><author>A</author><presentation>V1 C</presentation>\
<lyrics>[V1]\n First line\n Second line\n[C]\n Chorus line</lyrics></song>";

fn harness() -> EngineHarness {
    let mut harness = EngineHarness::with_songs(&[("Hymn", SONG), ("Other", SONG)]);
    harness.send(Command::OpenSong(SongId::new("Hymn")));
    harness
}

fn song(harness: &EngineHarness) -> std::sync::Arc<serviceflow_core::song::Song> {
    harness.state().song.as_ref().expect("a song is open").song.clone()
}

fn lyrics(harness: &mut EngineHarness, text: &str) {
    harness.send(Command::EditSong(SongEdit::Lyrics(text.to_owned())));
}

fn warnings(harness: &mut EngineHarness, text: &str) -> Vec<SongWarning> {
    lyrics(harness, text);
    song(harness).warnings.clone()
}

#[test]
fn editing_a_detail_changes_the_open_song_in_memory_only() {
    let mut harness = harness();
    harness.send(Command::EditSong(SongEdit::Field(SongField::Title, "New title".into())));
    harness.send(Command::EditSong(SongEdit::Field(SongField::Capo, "2".into())));
    let edited = song(&harness);
    assert_eq!((edited.title.as_str(), edited.capo.as_str()), ("New title", "2"));
    assert!(harness.state().draft().unwrap().is_modified());
    // Saving is #19: editing asks storage for nothing, so the harness is idle and nothing is written.
}

#[test]
fn every_field_of_the_form_can_be_edited() {
    let mut harness = harness();
    for (i, field) in SongField::ALL.into_iter().enumerate() {
        harness.send(Command::EditSong(SongEdit::Field(field, format!("value {i}"))));
        assert_eq!(field.get(&song(&harness)), format!("value {i}"));
    }
}

#[test]
fn editing_the_lyrics_rebuilds_the_slides() {
    let mut harness = harness();
    lyrics(&mut harness, "[V1]\n One\n ||\n Two\n[C]\n Chorus line");
    let open = harness.state().song.clone().unwrap();
    let lines: Vec<_> = open.slides.iter().map(|s| s.lines.clone()).collect();
    assert_eq!(lines, [vec!["One"], vec!["Two"], vec!["Chorus line"]]);
    assert_eq!(open.song.lyrics, "[V1]\n One\n ||\n Two\n[C]\n Chorus line");
}

#[test]
fn the_lyrics_text_is_kept_with_its_comments() {
    let mut harness = harness();
    lyrics(&mut harness, "[V1]\n;a comment\n One\r\n");
    assert_eq!(song(&harness).lyrics, "[V1]\n;a comment\n One\n");
}

#[test]
fn what_is_live_stays_live_while_the_song_is_edited() {
    let mut harness = harness();
    harness.send(Command::GoLive { index: 0 });
    lyrics(&mut harness, "[V1]\n Changed\n[C]\n Chorus line");
    let state = harness.state();
    assert_eq!(state.output.live.as_ref().unwrap().slide.lines, ["First line", "Second line"]);
    assert_eq!(state.song.as_ref().unwrap().slides[0].lines, ["Changed"]);
}

#[test]
fn undo_and_redo_walk_through_the_edits() {
    let mut harness = harness();
    harness.send(Command::EditSong(SongEdit::Field(SongField::Title, "One".into())));
    harness.send(Command::EndEditGroup);
    harness.send(Command::EditSong(SongEdit::Field(SongField::Title, "Two".into())));
    harness.send(Command::EndEditGroup);
    harness.send(Command::Undo);
    assert_eq!(song(&harness).title, "One");
    assert!(harness.state().draft().unwrap().can_redo());
    harness.send(Command::Undo);
    assert_eq!(song(&harness).title, "Hymn");
    assert!(!harness.state().draft().unwrap().is_modified());
    // Nothing left to undo.
    harness.send(Command::Undo);
    assert_eq!(song(&harness).title, "Hymn");
    harness.send(Command::Redo);
    harness.send(Command::Redo);
    assert_eq!(song(&harness).title, "Two");
    harness.send(Command::Redo);
    assert_eq!(song(&harness).title, "Two");
}

#[test]
fn undo_restores_the_lyrics_and_the_slides() {
    let mut harness = harness();
    lyrics(&mut harness, "[V1]\n Only");
    assert_eq!(harness.state().song.as_ref().unwrap().slides.len(), 1);
    harness.send(Command::Undo);
    assert_eq!(harness.state().song.as_ref().unwrap().slides.len(), 2);
    harness.send(Command::Redo);
    assert_eq!(harness.state().song.as_ref().unwrap().slides.len(), 1);
}

#[test]
fn a_new_edit_clears_what_could_be_redone() {
    let mut harness = harness();
    harness.send(Command::EditSong(SongEdit::Field(SongField::Title, "One".into())));
    harness.send(Command::Undo);
    harness.send(Command::EditSong(SongEdit::Field(SongField::Title, "Two".into())));
    assert!(!harness.state().draft().unwrap().can_redo());
}

#[test]
fn typing_in_one_field_is_one_undo_step_until_the_group_ends() {
    let mut harness = harness();
    for text in ["N", "Ne", "New"] {
        harness.send(Command::EditSong(SongEdit::Field(SongField::Title, text.into())));
    }
    // Another field starts another step without being told.
    harness.send(Command::EditSong(SongEdit::Field(SongField::Author, "B".into())));
    harness.send(Command::Undo);
    assert_eq!((song(&harness).title.as_str(), song(&harness).author.as_str()), ("New", "A"));
    harness.send(Command::Undo);
    assert_eq!(song(&harness).title, "Hymn");
}

#[test]
fn an_edit_that_changes_nothing_is_not_an_undo_step() {
    let mut harness = harness();
    harness.send(Command::EditSong(SongEdit::Field(SongField::Title, "Hymn".into())));
    assert!(harness.state().draft().is_none_or(|d| !d.is_modified()));
}

#[test]
fn each_song_has_its_own_history() {
    let mut harness = harness();
    harness.send(Command::EditSong(SongEdit::Field(SongField::Title, "Edited Hymn".into())));
    harness.send(Command::OpenSong(SongId::new("Other")));
    // Undo in the other song does nothing to the first.
    harness.send(Command::Undo);
    assert_eq!(song(&harness).title, "Hymn");
    harness.send(Command::EditSong(SongEdit::Field(SongField::Title, "Edited Other".into())));
    harness.send(Command::OpenSong(SongId::new("Hymn")));
    assert_eq!(song(&harness).title, "Edited Hymn");
    harness.send(Command::Undo);
    assert_eq!(song(&harness).title, "Hymn");
    harness.send(Command::OpenSong(SongId::new("Other")));
    assert_eq!(song(&harness).title, "Edited Other");
}

#[test]
fn live_actions_are_not_part_of_the_undo_history() {
    let mut harness = harness();
    harness.send(Command::EditSong(SongEdit::Field(SongField::Title, "One".into())));
    harness.send(Command::GoLive { index: 1 });
    harness.send(Command::ToggleBlack);
    harness.send(Command::Undo);
    let state = harness.state();
    // The edit is undone; the live slide and the black output stay as they were.
    assert_eq!(state.song.as_ref().unwrap().song.title, "Hymn");
    assert_eq!(state.output.live.as_ref().unwrap().index, 1);
    assert_eq!(state.output.mode, serviceflow_core::state::OutputMode::Black);
    assert!(Command::GoLive { index: 0 }.is_live_action() && !Command::GoLive { index: 0 }.is_document_edit());
    assert!(Command::Undo.is_document_edit() && !Command::Undo.is_live_action());
}

#[test]
fn editing_with_no_song_open_does_nothing() {
    let mut harness = EngineHarness::with_songs(&[("Hymn", SONG)]);
    harness.send(Command::EditSong(SongEdit::Field(SongField::Title, "x".into())));
    harness.send(Command::Undo);
    assert!(harness.state().drafts.is_empty());
}

#[test]
fn warns_about_a_tag_that_might_have_been_meant_as_multilanguage() {
    let mut harness = harness();
    assert_eq!(warnings(&mut harness, "[V1]\n One\n[Final]\n Two"), [SongWarning::MaybeMultilanguage { line: 3, tag: "Final".into() }]);
    harness.send(Command::EndEditGroup);
    // Fixing it with (L) clears the warning, and undo brings it back.
    assert_eq!(warnings(&mut harness, "[V1]\n One\n[Final] (L)\n Two"), []);
    harness.send(Command::Undo);
    assert_eq!(song(&harness).warnings.len(), 1);
}

#[test]
fn warns_about_a_stray_first_character() {
    let mut harness = harness();
    assert_eq!(warnings(&mut harness, "[V1]\n One\nTwo"), [SongWarning::StrayFirstCharacter { line: 3, character: 'T' }]);
}

#[test]
fn warns_about_a_break_between_a_primary_line_and_its_translation() {
    let mut harness = harness();
    assert_eq!(warnings(&mut harness, "[V1] (L)\n One\n ||\n Un\n Two\n Deux"), [SongWarning::BreakInsidePair { line: 3 }]);
    // The same break after the translation line is fine.
    assert_eq!(warnings(&mut harness, "[V1] (L)\n One\n Un\n ||\n Two\n Deux"), []);
}

#[test]
fn warns_about_tags_that_would_collide_once_canonical() {
    let mut harness = harness();
    assert_eq!(warnings(&mut harness, "[1]\n One\n[V1]\n Uno"), [SongWarning::TagCollision { line: 3, tag: "V1".into(), other: "1".into() }]);
    assert_eq!(
        warnings(&mut harness, "[V1-L]\n One\n Un\n[V1] (L)\n Two\n Deux"),
        [SongWarning::TagCollision { line: 4, tag: "V1".into(), other: "V1-L".into() }]
    );
    // A repeated tag is one section (OpenSong behaviour), and different verses do not collide.
    assert_eq!(warnings(&mut harness, "[V1]\n One\n[V2]\n Two\n[v1]\n More"), []);
}

#[test]
fn the_lyrics_are_highlighted_by_line_syntax() {
    use serviceflow_core::lyrics::{Highlight, highlight};
    let text = "[V1] (L)\n.G   C\n1 Hello|there\n ||\n;note\n-print\nStray_ line\n";
    let spans: Vec<_> = highlight(text).into_iter().map(|(range, kind)| (&text[range], kind)).collect();
    assert_eq!(
        spans,
        [
            ("[V1] (L)", Highlight::Header),
            (".G   C", Highlight::Chords),
            ("1", Highlight::VerseNumber),
            ("|", Highlight::LineBreak),
            ("||", Highlight::SlideBreak),
            (";note", Highlight::Comment),
            ("-print", Highlight::PrintLayout),
            ("S", Highlight::StrayFirstCharacter),
            ("_", Highlight::Spacer),
        ]
    );
}
