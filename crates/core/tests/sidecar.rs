//! Seam 1, the engine: serviceFlow-only item settings kept in a set's sidecar (#23), written when
//! the set is saved (#31).

use serviceflow_core::command::Command;
use serviceflow_core::ids::SetId;
use serviceflow_core::sidecar::{ItemKey, ItemSettings};
use serviceflow_core::song::LanguageMode;
use serviceflow_core::state::Problem;
use serviceflow_testkit::EngineHarness;

const SONGS: &[(&str, &str)] = &[
    ("Gift", "<song><title>Gift</title><lyrics>[V1] (L)\n 主\n Lord\n[C]\n chorus</lyrics></song>"),
    ("Plain", "<song><title>Plain</title><lyrics>[V1]\n plain</lyrics></song>"),
];

const SET: &str = r#"<set name="Test"><slide_groups>
  <slide_group name="Gift" type="song"/>
  <slide_group name="Plain" type="song"/>
  <slide_group name="Gift" type="song"/>
</slide_groups></set>"#;

fn open() -> EngineHarness {
    let mut harness = EngineHarness::with_songs_and_sets(SONGS, &[("Test", SET)]);
    harness.send(Command::OpenSet(SetId::new("Test")));
    harness
}

fn language(harness: &EngineHarness) -> LanguageMode {
    harness.state().slide_options.language
}

fn grid_lines(harness: &EngineHarness) -> usize {
    harness.state().song.as_ref().expect("a song is open").slides[0].lines.len()
}

#[test]
fn the_language_mode_of_an_item_is_restored_when_the_set_is_opened_again() {
    let mut harness = open();
    harness.send(Command::SelectItem(0));
    assert_eq!(grid_lines(&harness), 2, "both languages by default");
    harness.send(Command::SetLanguageMode(LanguageMode::Primary));
    assert_eq!(grid_lines(&harness), 1);
    harness.send(Command::SaveSet);

    // Restart the app: nothing of the session is left but the files.
    let mut harness = harness.restart();
    assert_eq!(language(&harness), LanguageMode::Both);
    harness.send(Command::OpenSet(SetId::new("Test")));
    harness.send(Command::SelectItem(0));
    assert_eq!(language(&harness), LanguageMode::Primary);
    assert_eq!(grid_lines(&harness), 1);
}

#[test]
fn each_item_keeps_its_own_language_mode_even_for_the_same_song() {
    let mut harness = open();
    harness.send(Command::SelectItem(0));
    harness.send(Command::SetLanguageMode(LanguageMode::Translation));
    harness.send(Command::SelectItem(1));
    assert_eq!(language(&harness), LanguageMode::Both, "another item starts in both languages");
    harness.send(Command::SelectItem(2));
    assert_eq!(language(&harness), LanguageMode::Both, "the same song again is a different item");
    harness.send(Command::SetLanguageMode(LanguageMode::Primary));
    harness.send(Command::SelectItem(0));
    assert_eq!(language(&harness), LanguageMode::Translation);
    harness.send(Command::SelectItem(2));
    assert_eq!(language(&harness), LanguageMode::Primary);
}

#[test]
fn a_song_opened_from_the_library_is_not_a_set_item_and_stores_nothing() {
    let mut harness = open();
    harness.send(Command::OpenSong(serviceflow_core::ids::SongId::new("Gift")));
    harness.send(Command::SetLanguageMode(LanguageMode::Primary));
    assert_eq!(harness.sidecar_bytes("Test"), None);
}

#[test]
fn going_back_to_both_languages_leaves_no_entry() {
    let mut harness = open();
    harness.send(Command::SelectItem(0));
    harness.send(Command::SetLanguageMode(LanguageMode::Primary));
    harness.send(Command::SaveSet);
    harness.send(Command::SetLanguageMode(LanguageMode::Both));
    harness.send(Command::SaveSet);
    let text = String::from_utf8(harness.sidecar_bytes("Test").unwrap()).unwrap();
    assert_eq!(text, "version = 1\n");
}

#[test]
fn the_sidecar_is_toml_beside_the_set_and_the_set_file_is_untouched() {
    let mut harness = open();
    let before = harness.set_bytes("Test");
    harness.send(Command::SelectItem(2));
    harness.send(Command::SetLanguageMode(LanguageMode::Primary));
    harness.send(Command::SaveSet);
    let text = String::from_utf8(harness.sidecar_bytes("Test").unwrap()).unwrap();
    assert_eq!(text, "version = 1\n\n[[item]]\nsong = \"Gift\"\noccurrence = 1\nlanguage = \"primary\"\n");
    assert_eq!(harness.set_bytes("Test"), before);
}

#[test]
fn a_hand_written_sidecar_is_read_with_transposition_and_capo_carried() {
    let mut harness = EngineHarness::with_songs_and_sets(SONGS, &[("Test", SET)]);
    harness.write_sidecar_file("Test", b"version = 1\n[[item]]\nsong = \"Plain\"\ntranspose = -3\ncapo = 2\nlanguage = \"translation\"\n");
    harness.send(Command::OpenSet(SetId::new("Test")));
    let state = harness.state();
    let set = state.set.as_ref().unwrap();
    let key = ItemKey { song: serviceflow_core::ids::SongId::new("Plain"), occurrence: 0 };
    assert_eq!(set.sidecar().get(&key), ItemSettings { language: LanguageMode::Translation, transpose: -3, capo: Some(2) });
    assert_eq!(set.item_settings(1).transpose, -3);
    assert_eq!(set.item_settings(0), ItemSettings::default());
    assert!(state.problems.is_empty());
}

#[test]
fn changing_the_language_keeps_what_else_the_sidecar_holds_for_the_item() {
    let mut harness = EngineHarness::with_songs_and_sets(SONGS, &[("Test", SET)]);
    harness.write_sidecar_file("Test", b"version = 1\n[[item]]\nsong = \"Gift\"\ntranspose = 2\n");
    harness.send(Command::OpenSet(SetId::new("Test")));
    harness.send(Command::SelectItem(0));
    harness.send(Command::SetLanguageMode(LanguageMode::Primary));
    harness.send(Command::SaveSet);
    let restarted = harness.restart();
    let mut harness = restarted;
    harness.send(Command::OpenSet(SetId::new("Test")));
    let state = harness.state();
    assert_eq!(state.set.as_ref().unwrap().item_settings(0), ItemSettings { language: LanguageMode::Primary, transpose: 2, capo: None });
}

#[test]
fn a_missing_sidecar_gives_defaults_without_a_warning() {
    let mut harness = open();
    harness.send(Command::SelectItem(0));
    assert_eq!(language(&harness), LanguageMode::Both);
    assert!(harness.state().problems.is_empty());
}

#[test]
fn a_corrupt_sidecar_gives_defaults_with_a_warning_and_the_set_still_opens() {
    for bad in [&b"this is [not toml"[..], b"version = 1\n[[item]]\nsong = \"Gift\"\nlanguage = \"klingon\"\n", &[0xff, 0xfe, 0x00]] {
        let mut harness = EngineHarness::with_songs_and_sets(SONGS, &[("Test", SET)]);
        harness.write_sidecar_file("Test", bad);
        harness.send(Command::OpenSet(SetId::new("Test")));
        let state = harness.state();
        assert_eq!(state.set.as_ref().expect("the set opened").set.items.len(), 3);
        assert!(matches!(state.problems.as_slice(), [Problem::SidecarUnreadable { id, .. }] if id == &SetId::new("Test")), "{:?}", state.problems);
        assert!(state.set.as_ref().unwrap().sidecar().items.is_empty());

        harness.send(Command::SelectItem(0));
        assert_eq!(grid_lines(&harness), 2, "defaults: both languages");
        // Choosing a mode and saving the set replaces the damaged file with a good one.
        harness.send(Command::SetLanguageMode(LanguageMode::Primary));
        harness.send(Command::SaveSet);
        let mut harness = harness.restart();
        harness.send(Command::OpenSet(SetId::new("Test")));
        assert!(harness.state().problems.is_empty());
        assert_eq!(harness.state().set.as_ref().unwrap().item_settings(0).language, LanguageMode::Primary);
    }
}
