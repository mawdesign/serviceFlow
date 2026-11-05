//! Seam 1, the engine: OpenSong song parity (#7). Presentation order with bare numeric tags,
//! numbered verse blocks, sections outside the presentation order, `blank_is_slide_change` and
//! lyric warnings, as they appear in the state snapshot.

use serviceflow_core::command::Command;
use serviceflow_core::ids::SongId;
use serviceflow_core::song::{SlideOptions, SongWarning};
use serviceflow_testkit::EngineHarness;

fn open(harness: &mut EngineHarness, name: &str) {
    harness.send(Command::OpenSong(SongId::new(name)));
}

/// (section, first line) of every slide in the grid, flow slides first.
fn grid(harness: &EngineHarness) -> Vec<(String, String)> {
    let state = harness.state();
    let song = state.song.as_ref().expect("a song is open");
    song.slides.iter().map(|s| (s.section.clone(), s.lines.first().cloned().unwrap_or_default())).collect()
}

fn sections(harness: &EngineHarness) -> Vec<String> {
    grid(harness).into_iter().map(|(s, _)| s).collect()
}

fn in_flow(harness: &EngineHarness) -> usize {
    harness.state().song.as_ref().expect("a song is open").in_flow
}

fn live_section(harness: &EngineHarness) -> Option<String> {
    harness.state().output.live.as_ref().map(|l| l.slide.section.clone())
}

#[test]
fn god_defend_nz_presents_verse_6_first_then_1_to_5_and_greys_the_rest() {
    let mut harness = EngineHarness::with_fixtures();
    open(&mut harness, "God Defend NZ - National Anthem");
    assert_eq!(sections(&harness), ["6", "1", "2", "3", "4", "5", "7", "8", "9", "10"]);
    assert_eq!(in_flow(&harness), 6);
    let state = harness.state();
    let open = state.song.as_ref().unwrap();
    assert!(open.is_in_flow(5));
    assert!(!open.is_in_flow(6));
    assert_eq!(open.slides[0].lines[0], "E Ihowā Atua,");
}

#[test]
fn next_and_previous_skip_sections_outside_the_presentation_order() {
    let mut harness = EngineHarness::with_fixtures();
    open(&mut harness, "Amazing Grace HOF#50");
    // Order 1 2 4 6; 3 and 5 are greyed after them.
    assert_eq!(sections(&harness), ["1", "2", "4", "6", "3", "5"]);
    for _ in 0..6 {
        harness.send(Command::NextSlide);
    }
    assert_eq!(live_section(&harness), Some("6".into()), "next stops at the end of the flow");
}

#[test]
fn a_greyed_slide_can_be_sent_live_by_hand() {
    let mut harness = EngineHarness::with_fixtures();
    open(&mut harness, "God Defend NZ - National Anthem");
    harness.send(Command::GoLive { index: 6 });
    assert_eq!(live_section(&harness), Some("7".into()));
    // From outside the flow, next stays put and previous returns to the end of the flow.
    harness.send(Command::NextSlide);
    assert_eq!(live_section(&harness), Some("7".into()));
    harness.send(Command::PreviousSlide);
    assert_eq!(live_section(&harness), Some("5".into()));
}

#[test]
fn a_numbered_verse_block_presents_as_separate_verses() {
    let mut harness = EngineHarness::with_fixtures();
    open(&mut harness, "Just A Closer Walk With Thee");
    assert_eq!(
        grid(&harness),
        [
            ("V1".to_string(), "I am       weak but Thou art strong;".to_string()),
            ("V2".to_string(), "Thro' this world of toil and snares,".to_string()),
            ("C".to_string(), "Just a closer walk with Thee,".to_string()),
            ("V3".to_string(), "When this  feeble   life is  o'er,".to_string()),
            ("C".to_string(), "Just a closer walk with Thee,".to_string()),
        ]
    );
    assert_eq!(in_flow(&harness), 5);
}

#[test]
fn blank_is_slide_change_splits_the_open_song_at_empty_lines() {
    let mut harness = EngineHarness::with_songs(&[("Blank", "<song><lyrics>[V1]\n one\n\n two</lyrics></song>")]);
    open(&mut harness, "Blank");
    assert_eq!(grid(&harness).len(), 1);
    harness.send(Command::SetSlideOptions(SlideOptions { blank_is_slide_change: true, ..SlideOptions::default() }));
    assert_eq!(grid(&harness), [("V1".to_string(), "one".to_string()), ("V1".to_string(), "two".to_string())]);
    // Songs opened later use the setting too.
    open(&mut harness, "Blank");
    assert_eq!(grid(&harness).len(), 2);
    assert!(!harness.send(Command::SetSlideOptions(SlideOptions::default())).state().slide_options.blank_is_slide_change);
    assert_eq!(grid(&harness).len(), 1);
}

const BLANK_SPLITS: &str = r#"<present_settings><style blank_is_slide_change="true"/><default_style/><scripture_style/></present_settings>"#;

#[test]
fn opensongs_blank_is_slide_change_setting_splits_slides_once_imported() {
    use serviceflow_core::command::ImportTrigger;
    let mut harness =
        EngineHarness::with_files(&[("Blank", "<song><lyrics>[V1]\n one\n\n two</lyrics></song>")], &[("PresentSettings", BLANK_SPLITS)]);
    open(&mut harness, "Blank");
    harness.send(Command::GoLive { index: 0 });
    harness.send(Command::ImportOpenSongStyles { trigger: ImportTrigger::FirstRun });
    assert!(harness.state().slide_options.blank_is_slide_change);
    // The open song and the live slide follow at once.
    assert_eq!(grid(&harness), [("V1".to_string(), "one".to_string()), ("V1".to_string(), "two".to_string())]);
    assert_eq!(harness.state().output.live.as_ref().unwrap().slide.lines, ["one"]);
}

#[test]
fn the_operators_own_blank_is_slide_change_choice_wins_over_the_imported_setting() {
    use serviceflow_core::command::ImportTrigger;
    let mut harness =
        EngineHarness::with_files(&[("Blank", "<song><lyrics>[V1]\n one\n\n two</lyrics></song>")], &[("PresentSettings", BLANK_SPLITS)]);
    open(&mut harness, "Blank");
    harness.send(Command::SetSlideOptions(SlideOptions::default()));
    harness.send(Command::ImportOpenSongStyles { trigger: ImportTrigger::Manual });
    assert!(!harness.state().slide_options.blank_is_slide_change);
    assert_eq!(grid(&harness).len(), 1);
}

#[test]
fn a_stray_first_character_is_shown_as_lyrics_and_recorded_as_a_warning() {
    let mut harness = EngineHarness::with_songs(&[("Typo", "<song><lyrics>[C]\nHello\n</lyrics></song>")]);
    open(&mut harness, "Typo");
    assert_eq!(grid(&harness), [("C".to_string(), "Hello".to_string())]);
    let state = harness.state();
    assert_eq!(state.song.as_ref().unwrap().song.warnings, [SongWarning::StrayFirstCharacter { line: 2, character: 'H' }]);
}

#[test]
fn every_fixture_song_opens() {
    for name in [
        "一件礼物 A Gift",
        "Amazing Grace HOF#50",
        "God Defend NZ - National Anthem",
        "How Deep the Fathers Love + Chinese",
        "It Is Well With My Soul",
        "Just A Closer Walk With Thee",
    ] {
        let mut harness = EngineHarness::with_fixtures();
        open(&mut harness, name);
        let state = harness.state();
        assert!(state.problems.is_empty(), "{name}: {:?}", state.problems);
        assert!(state.song.as_ref().is_some_and(|s| s.in_flow > 0), "{name} has no slides");
    }
}
