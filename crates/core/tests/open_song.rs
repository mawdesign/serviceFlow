//! Seam 1, the engine: opening a song puts its slides into the state snapshot.

use serviceflow_core::command::Command;
use serviceflow_core::ids::SongId;
use serviceflow_core::state::Problem;
use serviceflow_testkit::EngineHarness;

fn texts(harness: &EngineHarness) -> Vec<(String, String)> {
    let state = harness.state();
    let song = state.song.as_ref().expect("a song is open");
    song.slides.iter().map(|s| (s.section.clone(), s.lines.first().cloned().unwrap_or_default())).collect()
}

fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
    expected.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

#[test]
fn opening_it_is_well_with_my_soul_shows_its_slides_in_presentation_order() {
    let mut harness = EngineHarness::with_fixtures();
    harness.send(Command::OpenSong(SongId::new("It Is Well With My Soul")));

    let state = harness.state();
    let open = state.song.as_ref().expect("a song is open");
    assert_eq!(open.id, SongId::new("It Is Well With My Soul"));
    assert_eq!(open.song.title, "It Is Well With My Soul");
    let chorus = ("C", "It is well (it is well)");
    assert_eq!(
        texts(&harness),
        pairs(&[
            ("V1", "When peace like a river"),
            ("V1", "Whatever my lot,"),
            chorus,
            ("V2", "Though Satan should buffet,"),
            ("V2", "That Christ has regarded"),
            chorus,
            ("V3", "My sin, oh the bliss"),
            ("V3", "Is nailed to the cross,"),
            chorus,
            ("V4", "And, Lord, haste the day when my"),
            ("V4", "The trump shall resound,"),
            chorus,
        ])
    );
}

#[test]
fn a_song_without_a_presentation_order_shows_its_sections_in_file_order() {
    let mut harness = EngineHarness::with_songs(&[(
        "No Order",
        "<song><title>No Order</title><presentation></presentation><lyrics>[V1]\n one\n[C]\n chorus\n[V2]\n two</lyrics></song>",
    )]);
    harness.send(Command::OpenSong(SongId::new("No Order")));
    assert_eq!(texts(&harness), pairs(&[("V1", "one"), ("C", "chorus"), ("V2", "two")]));
}

#[test]
fn presentation_order_tags_match_sections_case_insensitively_and_skip_missing_ones() {
    let mut harness =
        EngineHarness::with_songs(&[("Order", "<song><presentation>c v1 X c</presentation><lyrics>[V1]\n one\n[C]\n chorus</lyrics></song>")]);
    harness.send(Command::OpenSong(SongId::new("Order")));
    assert_eq!(texts(&harness), pairs(&[("C", "chorus"), ("V1", "one"), ("C", "chorus")]));
}

#[test]
fn opening_a_missing_song_reports_a_problem_and_opens_nothing() {
    let mut harness = EngineHarness::with_fixtures();
    harness.send(Command::OpenSong(SongId::new("No Such Song")));
    let state = harness.state();
    assert!(state.song.is_none());
    assert!(matches!(&state.problems[..], [Problem::SongUnreadable { id, .. }] if *id == SongId::new("No Such Song")), "{:?}", state.problems);
}

#[test]
fn opening_a_song_that_is_not_valid_xml_reports_a_problem_and_keeps_the_open_song() {
    let mut harness = EngineHarness::with_songs(&[("Good", "<song><lyrics>[V1]\n fine</lyrics></song>"), ("Broken", "<song><lyrics>")]);
    harness.send(Command::OpenSong(SongId::new("Good")));
    harness.send(Command::OpenSong(SongId::new("Broken")));
    let state = harness.state();
    assert_eq!(state.song.as_ref().map(|s| s.id.clone()), Some(SongId::new("Good")));
    assert!(matches!(&state.problems[..], [Problem::SongUnreadable { id, .. }] if *id == SongId::new("Broken")), "{:?}", state.problems);
}

#[test]
fn snapshots_are_immutable_once_taken() {
    let mut harness = EngineHarness::with_fixtures();
    let before = harness.state();
    harness.send(Command::OpenSong(SongId::new("It Is Well With My Soul")));
    assert!(before.song.is_none());
    assert!(harness.state().song.is_some());
}

#[test]
fn opening_a_song_records_the_hash_of_the_bytes_that_were_read() {
    let mut harness = EngineHarness::with_fixtures();
    harness.send(Command::OpenSong(SongId::new("It Is Well With My Soul")));
    let state = harness.state();
    let open = state.song.as_ref().expect("a song is open");
    assert_eq!(open.hash, serviceflow_core::ids::ContentHash::of(&serviceflow_testkit::fixture_song("It Is Well With My Soul")));
}
