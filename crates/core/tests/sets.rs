//! Seam 1, the engine: opening a set, moving through its items, overrides, placeholders, relinking.

use serviceflow_core::command::Command;
use serviceflow_core::ids::{SetId, SongId};
use serviceflow_core::set::ItemKind;
use serviceflow_testkit::EngineHarness;

const SONGS: &[(&str, &str)] = &[
    ("Alpha", "<song><title>Alpha</title><lyrics>[V1]\n a1\n[V2]\n a2\n[C]\n ac</lyrics></song>"),
    ("Beta", "<song><title>Beta</title><lyrics>[V1]\n b1\n[V2]\n b2</lyrics></song>"),
    ("Hymns/Gamma New", "<song><title>Gamma</title><lyrics>[V1]\n g1</lyrics></song>"),
];

const SET: &str = r#"<set name="Test"><slide_groups>
  <slide_group name="Alpha" type="song" presentation="C V1"><notes>play it slow</notes></slide_group>
  <slide_group name="Psalm" type="scripture"><title>Psalm</title><slides><slide><body>text</body></slide></slides></slide_group>
  <slide_group name="Gone" type="song" path=""/>
  <slide_group name="Beta" type="song"/>
  <slide_group name="Gamma Old" type="song" path="Hymns/"><notes>relink me</notes></slide_group>
</slide_groups></set>"#;

fn open() -> EngineHarness {
    let mut harness = EngineHarness::with_songs_and_sets(SONGS, &[("Test", SET)]);
    harness.send(Command::RefreshLibrary);
    harness.send(Command::OpenSet(SetId::new("Test")));
    harness
}

fn first_lines(harness: &EngineHarness) -> Vec<String> {
    harness.state().song.as_ref().map(|s| s.slides.iter().map(|sl| sl.lines[0].clone()).collect()).unwrap_or_default()
}

fn live(harness: &EngineHarness) -> Option<String> {
    harness.state().output.live.as_ref().map(|l| l.slide.lines[0].clone())
}

fn selected(harness: &EngineHarness) -> Option<usize> {
    harness.state().set.as_ref().and_then(|s| s.selected)
}

#[test]
fn sets_are_listed_and_opened() {
    let mut harness = EngineHarness::with_songs_and_sets(SONGS, &[("Test", SET), ("Other", "<set/>")]);
    harness.send(Command::RefreshSets);
    assert_eq!(harness.state().sets, [SetId::new("Other"), SetId::new("Test")]);
    harness.send(Command::OpenSet(SetId::new("Test")));
    let state = harness.state();
    let set = state.set.as_ref().unwrap();
    assert_eq!(set.set.items.len(), 5);
    assert_eq!(set.selected, None);
}

#[test]
fn a_broken_set_reports_a_problem_and_keeps_the_open_one() {
    let mut harness = EngineHarness::with_songs_and_sets(SONGS, &[("Test", SET), ("Bad", "<song/>")]);
    harness.send(Command::OpenSet(SetId::new("Test")));
    harness.send(Command::OpenSet(SetId::new("Bad")));
    harness.send(Command::OpenSet(SetId::new("Nope")));
    let state = harness.state();
    assert_eq!(state.set.as_ref().unwrap().id, SetId::new("Test"));
    assert_eq!(state.problems.len(), 2);
}

#[test]
fn selecting_a_song_item_shows_its_slides_with_the_item_presentation_order() {
    let mut harness = open();
    harness.send(Command::SelectItem(0));
    // The item's order "C V1" wins over the file order; V2 is greyed.
    assert_eq!(first_lines(&harness), ["ac", "a1", "a2"]);
    assert_eq!(harness.state().song.as_ref().unwrap().in_flow, 2);
    // The next item has no override: the song's own order.
    harness.send(Command::SelectItem(3));
    assert_eq!(first_lines(&harness), ["b1", "b2"]);
}

#[test]
fn next_slide_continues_into_the_next_song_item_and_previous_comes_back() {
    let mut harness = open();
    harness.send(Command::SelectItem(0));
    harness.send(Command::NextSlide);
    assert_eq!(live(&harness).as_deref(), Some("ac"));
    harness.send(Command::NextSlide);
    assert_eq!(live(&harness).as_deref(), Some("a1"));
    // End of Alpha: the scripture placeholder and the missing song are skipped.
    harness.send(Command::NextSlide);
    assert_eq!(selected(&harness), Some(3));
    assert_eq!(live(&harness).as_deref(), Some("b1"));
    harness.send(Command::PreviousSlide);
    assert_eq!(selected(&harness), Some(0));
    assert_eq!(live(&harness).as_deref(), Some("a1"), "previous lands on the last slide of the flow");
}

#[test]
fn next_slide_stays_put_on_the_last_item_and_the_first_slide() {
    let mut harness = open();
    // Beta is the last item that can be presented (the one after it is a missing song).
    harness.send(Command::SelectItem(3));
    harness.send(Command::NextSlide);
    harness.send(Command::NextSlide);
    harness.send(Command::NextSlide);
    assert_eq!(selected(&harness), Some(3));
    assert_eq!(live(&harness).as_deref(), Some("b2"));
    harness.send(Command::SelectItem(0));
    harness.send(Command::NextSlide);
    harness.send(Command::PreviousSlide);
    assert_eq!(selected(&harness), Some(0));
    assert_eq!(live(&harness).as_deref(), Some("ac"));
}

#[test]
fn next_and_previous_item_step_over_every_kind_of_item() {
    let mut harness = open();
    harness.send(Command::NextItem);
    assert_eq!(selected(&harness), Some(0));
    harness.send(Command::NextItem);
    assert_eq!(selected(&harness), Some(1));
    harness.send(Command::PreviousItem);
    assert_eq!(selected(&harness), Some(0));
    harness.send(Command::PreviousItem);
    assert_eq!(selected(&harness), Some(0));
}

#[test]
fn a_scripture_item_is_a_placeholder_with_no_slides_and_keeps_its_xml() {
    let mut harness = open();
    harness.send(Command::SelectItem(0));
    harness.send(Command::SelectItem(1));
    let state = harness.state();
    assert!(state.song.is_none());
    let set = state.set.as_ref().unwrap();
    let ItemKind::Placeholder { item_type, xml } = &set.set.items[1].kind else { panic!("placeholder") };
    assert_eq!(item_type, "scripture");
    assert!(xml.contains("<body>text</body>"));
}

#[test]
fn a_missing_song_item_is_flagged_and_opens_nothing() {
    let mut harness = open();
    let state = harness.state();
    let set = state.set.as_ref().unwrap();
    assert_eq!((0..5).filter(|&i| set.is_missing(i)).collect::<Vec<_>>(), [2, 4], "Gone and the renamed Gamma are missing");
    harness.send(Command::SelectItem(2));
    assert!(harness.state().song.is_none());
    assert!(harness.state().problems.is_empty());
}

#[test]
fn item_notes_stay_with_the_set_and_are_never_in_the_slides() {
    let mut harness = open();
    harness.send(Command::SelectItem(0));
    let state = harness.state();
    assert_eq!(state.set.as_ref().unwrap().set.items[0].notes, "play it slow");
    assert!(!first_lines(&harness).iter().any(|l| l.contains("slow")));
}

#[test]
fn relinking_a_missing_item_finds_the_song_by_name_and_loads_it() {
    let mut harness = open();
    harness.send(Command::SelectItem(4));
    harness.send(Command::StartRelink(4));
    assert_eq!(harness.state().set.as_ref().unwrap().relinking, Some(4));
    assert_eq!(harness.state().library.query, "Gamma Old");
    // The name is not found as typed; the operator searches on.
    harness.send(Command::Search("Gamma".into()));
    assert_eq!(harness.state().library.results, [SongId::new("Hymns/Gamma New")]);
    harness.send(Command::RelinkItem { item: 4, song: SongId::new("Hymns/Gamma New") });
    let state = harness.state();
    let set = state.set.as_ref().unwrap();
    assert_eq!(set.relinking, None);
    assert!(!set.is_missing(4));
    assert_eq!(set.set.items[4].song_id(), Some(SongId::new("Hymns/Gamma New")));
    assert_eq!(set.set.items[4].notes, "relink me");
    assert_eq!(first_lines(&harness), ["g1"]);
}

#[test]
fn the_example_set_opens_with_every_song_missing_and_nothing_is_lost() {
    let mut harness = EngineHarness::with_fixtures();
    harness.send(Command::OpenSet(SetId::new("Example")));
    let state = harness.state();
    let set = state.set.as_ref().unwrap();
    assert_eq!(set.set.items.len(), 7);
    // "It Is Well With My Soul" exists in the fixtures; the others do not.
    assert_eq!((0..7).filter(|&i| set.is_missing(i)).collect::<Vec<_>>(), [2, 3, 4]);
}

#[test]
fn opening_a_song_from_the_library_ignores_the_item_override() {
    let mut harness = open();
    harness.send(Command::OpenSong(SongId::new("Alpha")));
    assert_eq!(first_lines(&harness), ["a1", "a2", "ac"]);
    assert_eq!(selected(&harness), None);
}
