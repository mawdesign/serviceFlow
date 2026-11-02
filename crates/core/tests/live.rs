//! Seam 1, the engine: going live, moving through a song, and blank/black.

use serviceflow_core::command::Command;
use serviceflow_core::ids::SongId;
use serviceflow_core::state::OutputMode;
use serviceflow_testkit::EngineHarness;

const SONGS: &[(&str, &str)] =
    &[("Three", "<song><lyrics>[V1]\n one\n[V2]\n two\n[C]\n three</lyrics></song>"), ("Other", "<song><lyrics>[V1]\n other</lyrics></song>")];

fn open(name: &str) -> EngineHarness {
    let mut harness = EngineHarness::with_songs(SONGS);
    harness.send(Command::OpenSong(SongId::new(name)));
    harness
}

/// The live slide's first line and index, or `None` when nothing is live.
fn live(harness: &EngineHarness) -> Option<(usize, String)> {
    let state = harness.state();
    state.output.live.as_ref().map(|l| (l.index, l.slide.lines[0].clone()))
}

fn mode(harness: &EngineHarness) -> OutputMode {
    harness.state().output.mode
}

#[test]
fn nothing_is_live_until_a_slide_is_sent_live() {
    let harness = open("Three");
    assert_eq!(live(&harness), None);
    assert_eq!(mode(&harness), OutputMode::Normal);
}

#[test]
fn going_live_shows_that_slide_immediately() {
    let mut harness = open("Three");
    harness.send(Command::GoLive { index: 1 });
    assert_eq!(live(&harness), Some((1, "two".into())));
    harness.send(Command::GoLive { index: 2 });
    assert_eq!(live(&harness), Some((2, "three".into())));
}

#[test]
fn going_live_with_a_slide_that_does_not_exist_changes_nothing() {
    let mut harness = open("Three");
    harness.send(Command::GoLive { index: 0 });
    harness.send(Command::GoLive { index: 3 });
    assert_eq!(live(&harness), Some((0, "one".into())));
    let mut empty = EngineHarness::with_songs(SONGS);
    empty.send(Command::GoLive { index: 0 });
    assert_eq!(live(&empty), None);
}

#[test]
fn next_with_nothing_live_starts_at_the_first_slide() {
    let mut harness = open("Three");
    harness.send(Command::NextSlide);
    assert_eq!(live(&harness), Some((0, "one".into())));
}

#[test]
fn next_and_previous_walk_the_song() {
    let mut harness = open("Three");
    harness.send(Command::GoLive { index: 0 });
    harness.send(Command::NextSlide);
    assert_eq!(live(&harness), Some((1, "two".into())));
    harness.send(Command::NextSlide);
    assert_eq!(live(&harness), Some((2, "three".into())));
    harness.send(Command::PreviousSlide);
    assert_eq!(live(&harness), Some((1, "two".into())));
}

#[test]
fn next_on_the_last_slide_and_previous_on_the_first_stay_put() {
    let mut harness = open("Three");
    harness.send(Command::GoLive { index: 2 });
    harness.send(Command::NextSlide);
    assert_eq!(live(&harness), Some((2, "three".into())));
    harness.send(Command::GoLive { index: 0 });
    harness.send(Command::PreviousSlide);
    assert_eq!(live(&harness), Some((0, "one".into())));
}

#[test]
fn previous_with_nothing_live_does_nothing() {
    let mut harness = open("Three");
    harness.send(Command::PreviousSlide);
    assert_eq!(live(&harness), None);
}

#[test]
fn blank_and_black_toggle_and_replace_each_other() {
    let mut harness = open("Three");
    harness.send(Command::GoLive { index: 1 });
    harness.send(Command::ToggleBlank);
    assert_eq!(mode(&harness), OutputMode::Blank);
    harness.send(Command::ToggleBlack);
    assert_eq!(mode(&harness), OutputMode::Black);
    harness.send(Command::ToggleBlack);
    assert_eq!(mode(&harness), OutputMode::Normal);
    harness.send(Command::ToggleBlank);
    harness.send(Command::ToggleBlank);
    assert_eq!(mode(&harness), OutputMode::Normal);
    // The live slide is kept underneath throughout.
    assert_eq!(live(&harness), Some((1, "two".into())));
}

#[test]
fn moving_to_another_slide_leaves_blank_and_black() {
    let mut harness = open("Three");
    harness.send(Command::GoLive { index: 0 });
    harness.send(Command::ToggleBlack);
    harness.send(Command::NextSlide);
    assert_eq!((mode(&harness), live(&harness)), (OutputMode::Normal, Some((1, "two".into()))));
    harness.send(Command::ToggleBlank);
    harness.send(Command::GoLive { index: 0 });
    assert_eq!(mode(&harness), OutputMode::Normal);
    harness.send(Command::ToggleBlank);
    harness.send(Command::GoLive { index: 9 });
    assert_eq!(mode(&harness), OutputMode::Blank, "a rejected go-live does not unblank");
}

#[test]
fn blank_works_before_anything_is_live() {
    let mut harness = open("Three");
    harness.send(Command::ToggleBlack);
    assert_eq!(mode(&harness), OutputMode::Black);
}

#[test]
fn opening_another_song_keeps_the_live_slide() {
    let mut harness = open("Three");
    harness.send(Command::GoLive { index: 1 });
    harness.send(Command::OpenSong(SongId::new("Other")));
    assert_eq!(live(&harness), Some((1, "two".into())));
    assert_eq!(harness.state().output.live.as_ref().unwrap().song, SongId::new("Three"));
    // Next belongs to the open song, so it starts that song's slides.
    harness.send(Command::NextSlide);
    assert_eq!(live(&harness), Some((0, "other".into())));
}

#[test]
fn live_actions_are_not_undoable() {
    for command in [Command::GoLive { index: 0 }, Command::NextSlide, Command::PreviousSlide, Command::ToggleBlank, Command::ToggleBlack] {
        assert!(command.is_live_action(), "{command:?}");
    }
    assert!(!Command::OpenSong(SongId::new("x")).is_live_action());
}
