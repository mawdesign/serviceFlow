//! Seam 1, the engine: chord lines become chords on slide lines, aligned by column (#24), and
//! each output has its own chord toggle.

use serviceflow_core::command::Command;
use serviceflow_core::ids::SongId;
use serviceflow_core::song::{Chord, LanguageMode, Slide};
use serviceflow_core::state::OutputId;
use serviceflow_testkit::EngineHarness;

fn song(lyrics: &str) -> EngineHarness {
    let xml = format!("<song><title>T</title><lyrics>{lyrics}</lyrics></song>");
    let mut harness = EngineHarness::with_songs(&[("T", &xml)]);
    harness.send(Command::OpenSong(SongId::new("T")));
    harness
}

fn slides(harness: &EngineHarness) -> Vec<Slide> {
    harness.state().song.as_ref().expect("a song is open").slides.clone()
}

fn chords(slide: &Slide, line: usize) -> Vec<(usize, &str)> {
    slide.chords[line].iter().map(|c| (c.offset, c.text.as_str())).collect()
}

#[test]
fn a_chord_line_sits_above_the_lyric_line_that_follows_it_by_column() {
    let harness = song("[V1]\n.G   Am     D\n Amazing grace\n Next line");
    let slide = &slides(&harness)[0];
    assert_eq!(slide.lines, ["Amazing grace", "Next line"]);
    // The lyric's leading space is the first-column marker, so column 0 of both lines is 'A'.
    assert_eq!(chords(slide, 0), [(0, "G"), (4, "Am"), (11, "D")]);
    assert!(slide.chords[1].is_empty());
}

#[test]
fn chords_past_the_end_of_the_line_stay_at_its_end() {
    let harness = song("[V1]\n.G       C\n Hi");
    assert_eq!(chords(&slides(&harness)[0], 0), [(0, "G"), (2, "C")]);
}

#[test]
fn trimming_and_spacers_move_chords_with_their_characters() {
    // Three leading spaces trimmed (a chord over them goes to the start); `_` is removed.
    let harness = song("[V1]\n.C  D    E\n    Am_azing");
    let slide = &slides(&harness)[0];
    assert_eq!(slide.lines, ["Amazing"]);
    assert_eq!(chords(slide, 0), [(0, "C"), (0, "D"), (4, "E")]);
}

#[test]
fn a_slide_break_on_a_lyric_line_splits_its_chords_between_the_slides() {
    // Columns: "one two " is 0-7, `||` is 8-9, " three four" starts at 10.
    let harness = song("[V1]\n.G            C\n one two || three four");
    let slides = slides(&harness);
    assert_eq!(slides.len(), 2);
    assert_eq!(slides[0].lines, ["one two"]);
    assert_eq!(chords(&slides[0], 0), [(0, "G")]);
    assert_eq!(slides[1].lines, ["three four"]);
    // Column 13 is three characters into " three four": above the second 'h'... 't','h' -> offset 2.
    assert_eq!(chords(&slides[1], 0), [(2, "C")]);
}

#[test]
fn just_a_closer_walk_numbered_verses_share_the_chord_lines() {
    let mut harness = EngineHarness::with_fixtures();
    harness.send(Command::OpenSong(SongId::new("Just A Closer Walk With Thee")));
    let slides = slides(&harness);
    // V1 V2 C V3 C
    assert_eq!(slides.iter().map(|s| s.section.as_str()).collect::<Vec<_>>(), ["V1", "V2", "C", "V3", "C"]);
    let (v1, v2) = (&slides[0], &slides[1]);
    assert_eq!(v1.lines[0], "I am       weak but Thou art strong;");
    // `G` over column 0 of "I am", `Em` at column 12 of the line (the digit is column 0, so the
    // lyric text starts at column 1 of the file line).
    assert_eq!(chords(v1, 0)[0], (0, "G"));
    assert_eq!(chords(v1, 0)[1], (11, "Em"));
    // Verse 2 has the same chords over different words.
    assert_eq!(v2.lines[0], "Thro' this world of toil and snares,");
    assert_eq!(chords(v2, 0), chords(v1, 0));
}

#[test]
fn chords_sit_above_primary_lines_only_in_multilanguage_sections() {
    let harness = song("[V1] (L)\n.G    C\n Primary one\n 翻译一\n.D\n Primary two\n 翻译二");
    let slide = &slides(&harness)[0];
    assert!(slide.multilanguage);
    assert_eq!(slide.lines, ["Primary one", "翻译一", "Primary two", "翻译二"]);
    assert_eq!(chords(slide, 0), [(0, "G"), (5, "C")]);
    assert!(slide.chords[1].is_empty());
    assert_eq!(chords(slide, 2), [(0, "D")]);
    assert!(slide.chords[3].is_empty());
}

#[test]
fn the_language_modes_keep_chords_on_primary_lines_and_none_on_translations() {
    let mut harness = song("[V1] (L)\n.G\n Primary one\n 翻译一");
    harness.send(Command::SetLanguageMode(LanguageMode::Primary));
    let slide = &slides(&harness)[0];
    assert_eq!(slide.lines, ["Primary one"]);
    assert_eq!(chords(slide, 0), [(0, "G")]);
    harness.send(Command::SetLanguageMode(LanguageMode::Translation));
    let slide = &slides(&harness)[0];
    assert_eq!(slide.lines, ["翻译一"]);
    assert!(!slide.has_chords());
}

#[test]
fn a_pair_split_at_a_bar_keeps_the_chords_with_their_words() {
    let harness = song("[V1] (L)\n.G         C\n one two|three four\n 一二|三四");
    let slide = &slides(&harness)[0];
    assert_eq!(slide.lines, ["one two", "一二", "three four", "三四"]);
    assert_eq!(chords(slide, 0), [(0, "G")]);
    assert_eq!(chords(slide, 2), [(2, "C")]);
}

#[test]
fn chords_are_off_on_every_output_until_toggled() {
    let mut harness = song("[V1]\n.G\n one");
    assert_eq!(harness.state().output.chords, Default::default());
    harness.send(Command::ToggleChords(OutputId::Window));
    let chords = harness.state().output.chords;
    assert!(chords.get(OutputId::Window) && !chords.get(OutputId::Monitor));
    harness.send(Command::ToggleChords(OutputId::Monitor)).send(Command::ToggleChords(OutputId::Window));
    let chords = harness.state().output.chords;
    assert!(!chords.get(OutputId::Window) && chords.get(OutputId::Monitor));
}

#[test]
fn toggling_chords_is_a_live_action_and_leaves_the_live_slide_alone() {
    let mut harness = song("[V1]\n.G\n one");
    harness.send(Command::GoLive { index: 0 });
    let before = harness.state().output.clone();
    harness.send(Command::ToggleChords(OutputId::Window));
    let after = harness.state().output.clone();
    assert_eq!(after.live, before.live);
    assert_eq!(after.changes, before.changes);
    assert!(Command::ToggleChords(OutputId::Window).is_live_action());
    assert!(after.live.unwrap().slide.has_chords(), "the slide keeps its chords; each output decides");
}

#[test]
fn editing_the_lyrics_updates_the_chords_the_editor_preview_draws() {
    use serviceflow_core::edit::SongEdit;
    let mut harness = song("[V1]\n one");
    assert!(!slides(&harness)[0].has_chords());
    harness.send(Command::EditSong(SongEdit::Lyrics("[V1]\n.G  D\n one two".into())));
    assert_eq!(chords(&slides(&harness)[0], 0), [(0, "G"), (3, "D")]);
}

#[test]
fn a_slide_without_chords_drops_them() {
    let slide = Slide { lines: vec!["a".into()], chords: vec![vec![Chord { offset: 0, text: "G".into() }]], ..Slide::default() };
    assert!(slide.has_chords() && !slide.without_chords().has_chords());
}
