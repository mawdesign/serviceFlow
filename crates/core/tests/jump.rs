//! Seam 1, the engine: jumping to a section by its tag (#17).

use serviceflow_core::command::Command;
use serviceflow_core::ids::SongId;
use serviceflow_core::song::LanguageMode;
use serviceflow_core::state::OutputMode;
use serviceflow_testkit::EngineHarness;

fn open(name: &str) -> EngineHarness {
    let mut harness = EngineHarness::with_fixtures();
    harness.send(Command::OpenSong(SongId::new(name)));
    harness
}

fn jump(harness: &mut EngineHarness, token: &str) {
    harness.send(Command::JumpToSection { token: token.into() });
}

fn live(harness: &EngineHarness) -> Option<(usize, String)> {
    harness.state().output.live.as_ref().map(|l| (l.index, l.slide.section.clone()))
}

/// A harness with one song `S` holding `lyrics`, opened.
fn song(lyrics: &str) -> EngineHarness {
    let xml = format!("<?xml version=\"1.0\"?><song><title>S</title><lyrics>{lyrics}</lyrics></song>");
    let mut harness = EngineHarness::with_songs(&[("S", xml.as_str())]);
    harness.send(Command::OpenSong(SongId::new("S")));
    harness
}

#[test]
fn a_number_jumps_to_that_verse_and_c_to_the_chorus() {
    let mut harness = song("[V1]\n one\n[V2]\n two\n[C]\n chorus\n[V3]\n three");
    jump(&mut harness, "3");
    assert_eq!(live(&harness), Some((3, "V3".into())));
    jump(&mut harness, "C");
    assert_eq!(live(&harness), Some((2, "C".into())));
    jump(&mut harness, "v1");
    assert_eq!(live(&harness), Some((0, "V1".into())));
}

#[test]
fn a_jump_goes_to_the_first_slide_of_the_section() {
    let mut harness = song("[V1]\n one\n[C]\n chorus a\n ||\n chorus b\n[V2]\n two");
    harness.send(Command::GoLive { index: 3 });
    jump(&mut harness, "C");
    assert_eq!(live(&harness), Some((1, "C".into())));
}

#[test]
fn a_chorus_numbered_c1_answers_to_c() {
    let mut harness = song("[V1]\n one\n[C1]\n chorus");
    jump(&mut harness, "C");
    assert_eq!(live(&harness), Some((1, "C1".into())));
}

#[test]
fn a_jump_leaves_blank_mode_and_goes_live() {
    let mut harness = song("[V1]\n one\n[V2]\n two");
    harness.send(Command::ToggleBlank);
    jump(&mut harness, "2");
    assert_eq!(harness.state().output.mode, OutputMode::Normal);
    assert_eq!(live(&harness), Some((1, "V2".into())));
}

#[test]
fn a_tag_the_song_lacks_changes_nothing() {
    let mut harness = song("[V1]\n one\n[V2]\n two");
    jump(&mut harness, "2");
    let changes = harness.state().output.changes;
    jump(&mut harness, "C");
    jump(&mut harness, "9");
    assert_eq!(live(&harness), Some((1, "V2".into())));
    assert_eq!(harness.state().output.changes, changes);
    let mut none = EngineHarness::with_fixtures();
    jump(&mut none, "1");
    assert_eq!(live(&none), None);
}

#[test]
fn a_section_the_presentation_order_leaves_out_can_be_jumped_to() {
    // Amazing Grace is ordered `1 2 4 6`; verses 3 and 5 are greyed thumbnails after the flow.
    let mut harness = open("Amazing Grace HOF#50");
    let state = harness.state();
    let open = state.song.as_ref().unwrap();
    let greyed3 = open.slides.iter().position(|s| s.section == "3").unwrap();
    assert!(greyed3 >= open.in_flow);
    jump(&mut harness, "3");
    assert_eq!(live(&harness), Some((greyed3, "3".into())));
    // Next does not walk on from a greyed slide.
    harness.send(Command::NextSlide);
    assert_eq!(live(&harness), Some((greyed3, "3".into())));
    jump(&mut harness, "4");
    assert_eq!(live(&harness).map(|l| l.1), Some("4".into()));
}

#[test]
fn a_number_finds_a_multilanguage_section() {
    let mut harness = open("一件礼物 A Gift");
    jump(&mut harness, "3");
    assert_eq!(live(&harness), Some((2, "V3".into())));
    assert!(harness.state().output.live.as_ref().unwrap().slide.multilanguage);
}

#[test]
fn a_number_finds_a_legacy_tagged_multilanguage_section() {
    let mut harness = open("How Deep the Fathers Love + Chinese");
    jump(&mut harness, "2");
    let (index, section) = live(&harness).unwrap();
    assert_eq!(section, "V2-L");
    assert_eq!(index, 2);
    jump(&mut harness, "V3");
    assert_eq!(live(&harness).map(|l| l.1), Some("V3-L".into()));
}

#[test]
fn a_jump_keeps_the_language_mode() {
    let mut harness = open("一件礼物 A Gift");
    harness.send(Command::SetLanguageMode(LanguageMode::Translation));
    jump(&mut harness, "2");
    let state = harness.state();
    let slide = &state.output.live.as_ref().unwrap().slide;
    assert_eq!(slide.section, "V2");
    assert!(!slide.multilanguage);
}

#[test]
fn jumping_is_a_live_action() {
    assert!(Command::JumpToSection { token: "1".into() }.is_live_action());
}
