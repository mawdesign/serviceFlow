//! Seam 1, the engine: transposition and capo of a set item (#25). The settings live in the
//! sidecar; only "Save in this key" changes the song.

use proptest::prelude::*;
use serviceflow_core::chord::{Spelling, Symbol, note_name, parse_key, spelling_of_key};
use serviceflow_core::command::Command;
use serviceflow_core::ids::{SetId, SongId};
use serviceflow_core::song::Slide;
use serviceflow_testkit::EngineHarness;

const SET: &str = r#"<set name="Test"><slide_groups>
  <slide_group name="Song" type="song"/>
  <slide_group name="Song" type="song"/>
</slide_groups></set>"#;

fn song_xml(key: &str, capo: &str, lyrics: &str) -> String {
    format!("<song><title>Song</title>{capo}<key>{key}</key><lyrics>{lyrics}</lyrics></song>")
}

fn open(key: &str, lyrics: &str) -> EngineHarness {
    open_with(&song_xml(key, "<capo></capo>", lyrics))
}

fn open_with(xml: &str) -> EngineHarness {
    let mut harness = EngineHarness::with_songs_and_sets(&[("Song", xml)], &[("Test", SET)]);
    harness.send(Command::OpenSet(SetId::new("Test")));
    harness.send(Command::SelectItem(0));
    harness
}

fn slides(harness: &EngineHarness) -> Vec<Slide> {
    harness.state().song.as_ref().expect("a song is open").slides.clone()
}

/// The chord texts above the first line of the first slide.
fn shown(harness: &EngineHarness) -> Vec<String> {
    slides(harness)[0].chords[0].iter().map(|c| c.text.clone()).collect()
}

const LYRICS: &str = "[V1]\n.G     C     D/F#  Em\n la la la la la la la la";

#[test]
fn transposing_changes_the_chord_text_and_leaves_the_offsets_alone() {
    let mut harness = open("G", LYRICS);
    let before = slides(&harness);
    harness.send(Command::SetTranspose(2));
    let after = slides(&harness);
    assert_eq!(shown(&harness), ["A", "D", "E/G#", "F#m"]);
    let offsets = |slides: &[Slide]| slides[0].chords[0].iter().map(|c| c.offset).collect::<Vec<_>>();
    assert_eq!(offsets(&before), offsets(&after));
    assert_eq!(before[0].lines, after[0].lines);
}

#[test]
fn the_target_key_chooses_sharps_or_flats() {
    // G up one is Ab major: flats. G up four is B major: sharps.
    let mut harness = open("G", "[V1]\n.G C Em\n la la la");
    harness.send(Command::SetTranspose(1));
    assert_eq!(shown(&harness), ["Ab", "Db", "Fm"]);
    harness.send(Command::SetTranspose(4));
    assert_eq!(shown(&harness), ["B", "E", "G#m"]);

    let mut harness = open("Em", "[V1]\n.Em A B\n la la la");
    harness.send(Command::SetTranspose(-3));
    assert_eq!(shown(&harness), ["C#m", "F#", "G#"], "C# minor takes sharps");
    harness.send(Command::SetTranspose(-6));
    assert_eq!(shown(&harness), ["Bbm", "Eb", "F"], "Bb minor takes flats");
}

#[test]
fn without_a_key_the_direction_chooses() {
    let mut harness = open("", "[V1]\n.C F\n la la");
    harness.send(Command::SetTranspose(1));
    assert_eq!(shown(&harness), ["C#", "F#"]);
    harness.send(Command::SetTranspose(-1));
    assert_eq!(shown(&harness), ["B", "E"]);
    harness.send(Command::SetTranspose(-2));
    assert_eq!(shown(&harness), ["Bb", "Eb"]);
}

#[test]
fn unknown_tokens_pass_through_and_notation_is_letter_names_only() {
    let mut harness = open("C", "[V1]\n.N.C. Do 4 H Amen x2 % C\n la la la la la la la la la la la la la");
    harness.send(Command::SetTranspose(2));
    assert_eq!(shown(&harness), ["N.C.", "Do", "4", "H", "Amen", "x2", "%", "D"]);
}

#[test]
fn roots_accidentals_qualities_extensions_and_slash_chords_are_understood() {
    let lyrics = "[V1]\n.C#m7b5 Bbmaj7 Fsus4 Gadd9 Cdim7 Eaug A6/9 Dm/F\n la la la la la la la la la la la la la la la la la la la la la la la";
    let mut harness = open("C", lyrics);
    harness.send(Command::SetTranspose(2));
    assert_eq!(shown(&harness), ["D#m7b5", "Cmaj7", "Gsus4", "Aadd9", "Ddim7", "F#aug", "B6/9", "Em/G"]);
}

#[test]
fn the_setting_is_in_the_sidecar_and_never_in_the_song() {
    let mut harness = open("G", LYRICS);
    let song_before = harness.song_bytes(&SongId::new("Song"));
    harness.send(Command::SetTranspose(2));
    harness.send(Command::SetCapo(Some(3)));
    assert_eq!(harness.song_bytes(&SongId::new("Song")), song_before);
    let text = String::from_utf8(harness.sidecar_bytes("Test").unwrap()).unwrap();
    assert!(text.contains("transpose = 2") && text.contains("capo = 3"), "{text}");
    assert!(harness.state().drafts.is_empty());

    // It is restored after a restart: up two, capo 3 is a net step down of one.
    let mut harness = harness.restart();
    harness.send(Command::OpenSet(SetId::new("Test")));
    harness.send(Command::SelectItem(0));
    assert_eq!(shown(&harness), ["F#", "B", "C#/F", "D#m"]);
}

#[test]
fn each_item_has_its_own_transposition_and_a_library_song_has_none() {
    let mut harness = open("G", LYRICS);
    harness.send(Command::SetTranspose(2));
    harness.send(Command::SelectItem(1));
    assert_eq!(shown(&harness), ["G", "C", "D/F#", "Em"], "the same song again is another item");
    harness.send(Command::SelectItem(0));
    assert_eq!(shown(&harness), ["A", "D", "E/G#", "F#m"]);

    harness.send(Command::OpenSong(SongId::new("Song")));
    assert_eq!(shown(&harness), ["G", "C", "D/F#", "Em"]);
    let before = harness.sidecar_bytes("Test");
    harness.send(Command::SetTranspose(5));
    assert_eq!(shown(&harness), ["G", "C", "D/F#", "Em"], "nothing to transpose without a set item");
    assert_eq!(harness.sidecar_bytes("Test"), before);
}

#[test]
fn the_live_slide_follows_the_transposition() {
    let mut harness = open("G", LYRICS);
    harness.send(Command::GoLive { index: 0 });
    harness.send(Command::SetTranspose(2));
    let state = harness.state();
    let live = state.output.live.as_ref().unwrap();
    assert_eq!(live.slide.chords[0].iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), ["A", "D", "E/G#", "F#m"]);
}

#[test]
fn a_capo_shows_shapes_relative_to_it_and_the_sounding_key_on_request() {
    let mut harness = open("Bb", "[V1]\n.Bb Eb F Gm\n la la la la");
    harness.send(Command::SetCapo(Some(3)));
    assert_eq!(shown(&harness), ["G", "C", "D", "Em"]);
    let info = harness.state().key_info().expect("a song item");
    assert_eq!((info.original.as_str(), info.sounding.as_deref(), info.shapes.as_deref(), info.capo), ("Bb", Some("Bb"), Some("G"), 3));

    // Up two with a capo on 2: the shapes are the written chords again, the sound is a tone up.
    harness.send(Command::SetCapo(Some(2)));
    harness.send(Command::SetTranspose(2));
    assert_eq!(shown(&harness), ["Bb", "Eb", "F", "Gm"]);
    let info = harness.state().key_info().unwrap();
    assert_eq!((info.sounding.as_deref(), info.shapes.as_deref()), (Some("C"), Some("Bb")));

    harness.send(Command::SetCapo(Some(0)));
    assert_eq!(shown(&harness), ["C", "F", "G", "Am"]);
}

#[test]
fn a_song_that_prints_its_capo_shows_shapes_until_the_item_says_otherwise() {
    let xml = song_xml("D", r#"<capo print="true">2</capo>"#, "[V1]\n.D G A\n la la la");
    let mut harness = open_with(&xml);
    assert_eq!(shown(&harness), ["C", "F", "G"]);
    harness.send(Command::SetCapo(Some(0)));
    assert_eq!(shown(&harness), ["D", "G", "A"]);
    harness.send(Command::SetCapo(None));
    assert_eq!(shown(&harness), ["C", "F", "G"]);
}

#[test]
fn save_in_this_key_rewrites_chords_and_key_through_a_canonical_save() {
    let lyrics = "[V1]\n;a comment\n.G     C     D/F#  Em\n la la la la la la la la";
    let mut harness = open("G", lyrics);
    harness.send(Command::SetTranspose(2));
    harness.send(Command::SaveInThisKey);

    let bytes = String::from_utf8(harness.song_bytes(&SongId::new("Song")).unwrap()).unwrap().replace("\r\n", "\n");
    assert!(bytes.contains("<key>A</key>"), "{bytes}");
    assert!(bytes.contains("\n.A     D     E/G#  F#m\n"), "{bytes}");
    assert!(bytes.contains(";a comment"), "comments stay");
    assert!(bytes.starts_with("<?xml"), "a canonical save");

    // The item is back at zero: the song has the key now.
    let text = String::from_utf8(harness.sidecar_bytes("Test").unwrap()).unwrap();
    assert!(!text.contains("transpose"), "{text}");
    assert_eq!(shown(&harness), ["A", "D", "E/G#", "F#m"]);
    let state = harness.state();
    assert!(state.draft().is_some_and(|draft| !draft.is_modified()), "saved");
    assert_eq!(state.key_info().unwrap().original, "A");
}

#[test]
fn save_in_this_key_with_no_transposition_does_nothing() {
    let mut harness = open("G", LYRICS);
    let before = harness.song_bytes(&SongId::new("Song"));
    harness.send(Command::SaveInThisKey);
    assert_eq!(harness.song_bytes(&SongId::new("Song")), before);
    assert!(harness.state().drafts.is_empty());
}

#[test]
fn save_in_this_key_keeps_the_item_capo_and_updates_the_live_slide() {
    let mut harness = open("G", LYRICS);
    harness.send(Command::GoLive { index: 0 });
    harness.send(Command::SetCapo(Some(2)));
    harness.send(Command::SetTranspose(2));
    harness.send(Command::SaveInThisKey);
    let state = harness.state();
    let settings = state.set.as_ref().unwrap().item_settings(0);
    assert_eq!((settings.transpose, settings.capo), (0, Some(2)));
    // The song is in A; the capo on 2 shows G shapes again.
    assert_eq!(shown(&harness), ["G", "C", "D/F#", "Em"]);
    let live = state.output.live.as_ref().unwrap();
    assert_eq!(live.slide.chords[0].iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), ["G", "C", "D/F#", "Em"]);
}

// ---- properties ----

const ROOTS: [&str; 17] = ["C", "C#", "Db", "D", "D#", "Eb", "E", "F", "F#", "Gb", "G", "G#", "Ab", "A", "A#", "Bb", "B"];
const SUFFIXES: [&str; 10] = ["", "m", "7", "m7", "maj7", "sus4", "add9", "dim", "m7b5", "6/9"];
const JUNK: [&str; 6] = ["N.C.", "x2", "%", "Amen", "Do", "H7"];
const KEYS: [&str; 19] = ["", "C", "G", "D", "A", "E", "B", "F", "Bb", "Eb", "Ab", "Db", "F#", "Am", "Em", "Bm", "F#m", "Dm", "Gm"];

fn token() -> impl Strategy<Value = String> {
    prop_oneof![
        3 => (0..ROOTS.len(), 0..SUFFIXES.len(), proptest::option::of(0..ROOTS.len()))
            .prop_map(|(r, s, b)| format!("{}{}{}", ROOTS[r], SUFFIXES[s], b.map(|b| format!("/{}", ROOTS[b])).unwrap_or_default())),
        1 => (0..JUNK.len()).prop_map(|j| JUNK[j].to_owned()),
    ]
}

fn lyrics_of(tokens: &[String]) -> String {
    format!("[V1]\n.{}\n {}", tokens.join("        "), "la ".repeat(40))
}

fn semitones() -> impl Strategy<Value = i32> {
    prop_oneof![-11..=-1, 1..=11]
}

/// Whether every accidental in the roots and basses of `chord` is of the `spelling`.
fn accidentals_match(chord: &str, spelling: Spelling) -> bool {
    let wrong = if spelling == Spelling::Sharps { 'b' } else { '#' };
    let letter = |c: char| ('A'..='G').contains(&c);
    let mut parts = chord.splitn(2, '/');
    let root = parts.next().unwrap_or("");
    let bass = parts.next().filter(|b| b.starts_with(letter));
    [Some(root), bass].into_iter().flatten().all(|note| note.chars().nth(1) != Some(wrong))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn transposing_by_n_then_back_gives_the_original(tokens in proptest::collection::vec(token(), 1..6), key in 0..KEYS.len(), n in semitones()) {
        let mut harness = open(KEYS[key], &lyrics_of(&tokens));
        let original = slides(&harness);
        harness.send(Command::SetTranspose(n));
        harness.send(Command::SetTranspose(0));
        prop_assert_eq!(slides(&harness), original);
    }

    #[test]
    fn every_chord_moves_n_semitones_and_everything_else_is_kept(tokens in proptest::collection::vec(token(), 1..6), key in 0..KEYS.len(), n in semitones()) {
        let mut harness = open(KEYS[key], &lyrics_of(&tokens));
        let original = slides(&harness);
        harness.send(Command::SetTranspose(n));
        let moved = slides(&harness);
        prop_assert_eq!(&moved[0].lines, &original[0].lines);
        let (before, after) = (&original[0].chords[0], &moved[0].chords[0]);
        prop_assert_eq!(before.len(), after.len());
        for (was, now) in before.iter().zip(after) {
            prop_assert_eq!(was.offset, now.offset, "positions never change");
            match Symbol::parse(&was.text) {
                None => prop_assert_eq!(&now.text, &was.text, "unknown tokens are kept"),
                Some(was) => {
                    let now = Symbol::parse(&now.text).expect("a chord stays a chord");
                    prop_assert_eq!(now.root, (was.root + n).rem_euclid(12));
                    prop_assert_eq!(now.bass, was.bass.map(|b| (b + n).rem_euclid(12)));
                    prop_assert_eq!(&now.suffix, &was.suffix);
                }
            }
        }
    }

    #[test]
    fn sharps_or_flats_match_the_target_key(tokens in proptest::collection::vec(token(), 1..6), key in 0..KEYS.len(), n in semitones()) {
        let mut harness = open(KEYS[key], &lyrics_of(&tokens));
        harness.send(Command::SetTranspose(n));
        let spelling = match parse_key(KEYS[key]) {
            Some((tonic, minor)) => spelling_of_key(tonic + n, minor),
            None if n > 0 => Spelling::Sharps,
            None => Spelling::Flats,
        };
        for chord in shown(&harness) {
            if Symbol::parse(&chord).is_some() {
                prop_assert!(accidentals_match(&chord, spelling), "{chord} in {:?}", spelling);
            }
        }
        // The note names themselves are from the one table.
        let tonic = parse_key(KEYS[key]).map_or(0, |(t, _)| t);
        prop_assert!(["C", "C#", "Db", "D", "D#", "Eb", "E", "F", "F#", "Gb", "G", "G#", "Ab", "A", "A#", "Bb", "B"].contains(&note_name(tonic + n, spelling)));
    }
}
