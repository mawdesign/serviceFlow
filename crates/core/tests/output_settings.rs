//! Seam 1: output polish imported from OpenSong's settings: logo mode, initial monitor and mode,
//! blanks, slide colours and the crossfade time.

use serviceflow_core::command::{Command, ImportTrigger};
use serviceflow_core::ids::SongId;
use serviceflow_core::output::{InitialMode, SlideKind};
use serviceflow_core::state::{OutputMode, Problem};
use serviceflow_core::style::Rgba;
use serviceflow_testkit::EngineHarness;

const FIRST_RUN: Command = Command::ImportOpenSongSettings { trigger: ImportTrigger::FirstRun };
const MANUAL: Command = Command::ImportOpenSongSettings { trigger: ImportTrigger::Manual };

fn present(style: &str) -> String {
    format!(r#"<present_settings><style {style}/></present_settings>"#)
}

fn with_present(style: &str) -> EngineHarness {
    let mut harness = EngineHarness::with_files(&[], &[("PresentSettings", &present(style))]);
    harness.send(FIRST_RUN);
    harness
}

fn mode(h: &EngineHarness) -> OutputMode {
    h.state().output.mode
}

#[test]
fn the_fixture_logo_and_monitor_are_imported() {
    let mut harness = EngineHarness::with_fixtures();
    harness.send(FIRST_RUN);
    let state = harness.state();
    assert!(state.output_settings.logo.is_some());
    assert_eq!(state.output_settings.monitor, Some(1));
    assert_eq!(state.output_settings.initial_mode, InitialMode::Logo);
    assert!(state.problems.is_empty());
}

#[test]
fn the_initial_mode_applies_once_at_first_import() {
    let mut harness = EngineHarness::with_fixtures();
    assert_eq!(mode(&harness), OutputMode::Normal);
    harness.send(FIRST_RUN);
    assert_eq!(mode(&harness), OutputMode::Logo, "the fixture starts in logo mode");
    // The operator's choice wins from then on, also over a re-import.
    harness.send(Command::ToggleLogo);
    assert_eq!(mode(&harness), OutputMode::Normal);
    harness.send(MANUAL);
    assert_eq!(mode(&harness), OutputMode::Normal);
}

#[test]
fn black_and_blank_initial_modes_are_honoured() {
    assert_eq!(mode(&with_present(r#"initial_mode="B""#)), OutputMode::Black);
    assert_eq!(mode(&with_present(r#"initial_mode="H""#)), OutputMode::Blank);
    assert_eq!(mode(&with_present(r#"initial_mode="N""#)), OutputMode::Normal);
}

#[test]
fn logo_toggles_and_replaces_blank_and_black() {
    let mut harness = EngineHarness::with_songs(&[("A", "<song><lyrics>[V1]\n one\n[V2]\n two</lyrics></song>")]);
    harness.send(Command::ToggleLogo);
    assert_eq!(mode(&harness), OutputMode::Logo);
    harness.send(Command::ToggleBlack);
    assert_eq!(mode(&harness), OutputMode::Black);
    harness.send(Command::ToggleLogo);
    assert_eq!(mode(&harness), OutputMode::Logo);
    harness.send(Command::ToggleBlank);
    assert_eq!(mode(&harness), OutputMode::Blank);
    harness.send(Command::ToggleLogo).send(Command::ToggleLogo);
    assert_eq!(mode(&harness), OutputMode::Normal);
    // A slide change leaves logo mode.
    harness.send(Command::ToggleLogo).send(Command::OpenSong(SongId::new("A"))).send(Command::NextSlide);
    assert_eq!(mode(&harness), OutputMode::Normal);
    assert!(Command::ToggleLogo.is_live_action());
}

#[test]
fn with_blanks_on_going_past_the_last_slide_blanks_and_previous_returns() {
    let song = "<song><lyrics>[V1]\n one\n[V2]\n two</lyrics></song>";
    let mut harness = EngineHarness::with_files(&[("A", song)], &[("PresentSettings", &present(r#"blanks="true""#))]);
    harness.send(FIRST_RUN).send(Command::OpenSong(SongId::new("A")));
    harness.send(Command::NextSlide).send(Command::NextSlide);
    assert_eq!(harness.state().output.live.as_ref().unwrap().index, 1);
    assert_eq!(mode(&harness), OutputMode::Normal);
    harness.send(Command::NextSlide);
    assert_eq!(mode(&harness), OutputMode::Blank, "a blank follows the last slide");
    assert_eq!(harness.state().output.live.as_ref().unwrap().index, 1);
    harness.send(Command::PreviousSlide);
    assert_eq!(mode(&harness), OutputMode::Normal);
    assert_eq!(harness.state().output.live.as_ref().unwrap().index, 1, "back to the last slide, not the one before");
}

#[test]
fn without_blanks_the_last_slide_stays_put() {
    let song = "<song><lyrics>[V1]\n one\n[V2]\n two</lyrics></song>";
    let mut harness = EngineHarness::with_files(&[("A", song)], &[("PresentSettings", &present(r#"blanks="false""#))]);
    harness.send(FIRST_RUN).send(Command::OpenSong(SongId::new("A")));
    harness.send(Command::NextSlide).send(Command::NextSlide).send(Command::NextSlide);
    assert_eq!(mode(&harness), OutputMode::Normal);
}

#[test]
fn the_crossfade_time_comes_from_the_transition_settings() {
    let ms = |style: &str| with_present(style).state().output_settings.fade_ms();
    assert_eq!(ms(r#"transition="false" transition_time="400""#), None);
    assert_eq!(ms(r#"transition="true" transition_time="400""#), Some(400));
    assert_eq!(ms(r#"transition="true" transition_time="0""#), Some(100));
    assert_eq!(EngineHarness::with_fixtures().state().output_settings.fade_ms(), None);
}

#[test]
fn every_change_of_what_is_live_is_counted_for_the_crossfade() {
    let mut harness = EngineHarness::with_songs(&[("A", "<song><lyrics>[V1]\n one\n[V2]\n two</lyrics></song>")]);
    harness.send(Command::OpenSong(SongId::new("A")));
    let changes = |h: &EngineHarness| h.state().output.changes;
    let start = changes(&harness);
    harness.send(Command::NextSlide);
    assert!(changes(&harness) > start);
    let before = changes(&harness);
    harness.send(Command::GoLive { index: 9 });
    assert_eq!(changes(&harness), before, "a rejected change is not a change");
    harness.send(Command::ToggleBlack);
    assert!(changes(&harness) > before);
}

#[test]
fn slide_colours_are_imported_from_main_settings() {
    let mut harness = EngineHarness::with_fixtures();
    assert_eq!(harness.state().slide_colors.color(SlideKind::Verse), Some(Rgba::rgb(213, 213, 255)), "OpenSong's default before import");
    harness.send(FIRST_RUN);
    let colors = harness.state().slide_colors.clone();
    assert_eq!(colors.for_tag("V1"), Some(Rgba::rgb(0xEA, 0xEA, 0xFF)));
    assert_eq!(colors.for_tag("1"), colors.for_tag("V2"), "a numeric tag is a verse");
    assert_eq!(colors.for_tag("C"), Some(Rgba::rgb(0xBC, 0xBC, 0xFF)));
    assert_eq!(colors.for_tag("B"), Some(Rgba::rgb(0x8A, 0x8A, 0xFF)));
    assert_eq!(colors.for_tag("P"), Some(Rgba::rgb(0x71, 0x71, 0xFF)));
    assert_eq!(colors.for_tag("T"), Some(Rgba::rgb(0xA3, 0xA3, 0xFF)));
    assert_eq!(colors.color(SlideKind::Custom), Some(Rgba::rgb(0xFF, 0xE3, 0xD5)));
    assert_eq!(colors.color(SlideKind::Image), Some(Rgba::rgb(0xFF, 0xFF, 0xB4)));
    assert_eq!(colors.color(SlideKind::Blank), Some(Rgba::rgb(0xFF, 0xFF, 0xFF)));
}

#[test]
fn missing_settings_are_silent_on_first_run_and_reported_on_request() {
    let mut harness = EngineHarness::with_songs(&[]);
    harness.send(FIRST_RUN);
    assert!(harness.state().problems.is_empty());
    assert_eq!(*harness.state().output_settings, Default::default());
    harness.send(MANUAL);
    assert!(matches!(harness.state().problems.as_slice(), [Problem::SettingsUnreadable { .. }, ..]));
}

#[test]
fn unreadable_settings_are_reported_even_on_first_run() {
    let mut harness = EngineHarness::with_files(&[], &[("PresentSettings", "<nonsense/>")]);
    harness.send(FIRST_RUN);
    assert!(matches!(harness.state().problems.as_slice(), [Problem::SettingsUnreadable { .. }]));
}
