//! Seam 2, the OpenSong codec: writing songs in canonical form (ADR-0002, #19).
//!
//! Golden files: the canonical output of every fixture is an insta snapshot (CRLF shown as
//! `\n`; that every line ends in CRLF is checked separately). Round trip: reading the output
//! gives the canonical model of what was read, and writing that again gives the same bytes.

use serviceflow_core::canonical::{canonicalize, has_tag_collision};
use serviceflow_core::song::Song;
use serviceflow_opensong::{read_song, write_song};
use serviceflow_testkit::fixture_song;

const FIXTURES: [&str; 6] = [
    "一件礼物 A Gift",
    "Amazing Grace HOF#50",
    "God Defend NZ - National Anthem",
    "How Deep the Fathers Love + Chinese",
    "It Is Well With My Soul",
    "Just A Closer Walk With Thee",
];

fn song(xml: &str) -> Song {
    read_song(xml.as_bytes()).unwrap()
}

fn written(song: &Song) -> String {
    String::from_utf8(write_song(song)).expect("canonical output is UTF-8")
}

/// The written text with CRLF shown as plain line ends; panics on any bare LF or CR.
fn lines_of(text: &str) -> String {
    let plain = text.replace("\r\n", "\n");
    assert!(!plain.contains('\r'), "a bare CR in {text:?}");
    assert_eq!(plain.matches('\n').count(), text.matches("\r\n").count(), "a bare LF in {text:?}");
    plain
}

#[test]
fn every_fixture_has_a_golden_canonical_form() {
    for name in FIXTURES {
        let out = written(&read_song(&fixture_song(name)).unwrap());
        insta::assert_snapshot!(name.replace(['#', '+', ' '], "_"), lines_of(&out));
    }
}

#[test]
fn reading_the_canonical_output_gives_the_canonical_model() {
    for name in FIXTURES {
        let read = read_song(&fixture_song(name)).unwrap();
        let again = read_song(&write_song(&read)).unwrap();
        assert_eq!(again, canonicalize(&read).song, "{name}");
    }
}

#[test]
fn writing_the_canonical_output_again_changes_nothing() {
    for name in FIXTURES {
        let once = write_song(&read_song(&fixture_song(name)).unwrap());
        let twice = write_song(&read_song(&once).unwrap());
        assert_eq!(String::from_utf8_lossy(&twice), String::from_utf8_lossy(&once), "{name}");
    }
}

#[test]
fn output_is_utf8_with_crlf_line_endings_and_a_final_newline() {
    // The fixture ends `</lyrics></song>` with no newline.
    let out = written(&read_song(&fixture_song("Just A Closer Walk With Thee")).unwrap());
    assert!(out.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<song>\r\n"));
    assert!(out.ends_with("</song>\r\n"));
    lines_of(&out);
    // A UTF-16 file is written as UTF-8.
    let utf16: Vec<u8> = [0xFF, 0xFE].into_iter().chain("<song><title>Ä</title></song>".encode_utf16().flat_map(u16::to_le_bytes)).collect();
    let out = write_song(&read_song(&utf16).unwrap());
    assert!(String::from_utf8(out).unwrap().contains("<title>Ä</title>"));
}

#[test]
fn trailing_whitespace_is_trimmed_from_lyric_lines() {
    let out = written(&song("<song><lyrics>[V1]  \n Amazing grace,   \n \n how sweet\t</lyrics></song>"));
    assert!(out.contains("<lyrics>[V1]\r\n Amazing grace,\r\n\r\n how sweet</lyrics>"), "{out}");
}

#[test]
fn standard_elements_are_written_in_a_fixed_order_including_empty_ones() {
    let out = written(&song("<song><lyrics>[V1]\n a</lyrics><title>T</title><timesig>3/4</timesig></song>"));
    let expected = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<song>\r\n  <title>T</title>\r\n  <author></author>\r\n  <copyright></copyright>\r\n  \
        <hymn_number></hymn_number>\r\n  <presentation></presentation>\r\n  <ccli></ccli>\r\n  <capo></capo>\r\n  <key></key>\r\n  \
        <aka></aka>\r\n  <key_line></key_line>\r\n  <user1></user1>\r\n  <user2></user2>\r\n  <user3></user3>\r\n  <theme></theme>\r\n  \
        <tempo></tempo>\r\n  <time_sig>3/4</time_sig>\r\n  <lyrics>[V1]\r\n a</lyrics>\r\n</song>\r\n";
    assert_eq!(out, expected);
}

#[test]
fn unknown_elements_are_kept_after_the_standard_ones_in_their_order() {
    let xml = "<song><zeta a=\"1\">z &amp; <b>bold</b></zeta><title>T</title><alttheme>Church</alttheme><background/></song>";
    let out = written(&song(xml));
    let tail = "  <lyrics></lyrics>\r\n  <zeta a=\"1\">z &amp; <b>bold</b></zeta>\r\n  <alttheme>Church</alttheme>\r\n  <background/>\r\n</song>\r\n";
    assert!(out.ends_with(tail), "{out}");
    assert_eq!(read_song(out.as_bytes()).unwrap().other_elements, song(xml).other_elements);
}

#[test]
fn known_raw_elements_go_in_their_place_and_survive_a_save() {
    let xml = "<song><style><body font=\"Arial\"/></style><lyrics>[V1]\n a</lyrics><linked_songs><linked_song>X</linked_song></linked_songs>\
        <backgrounds resize=\"body\"><background verse=\"v1\"><filename>sky.jpg</filename></background></backgrounds><title>T</title></song>";
    let read = song(xml);
    let out = written(&read);
    let theme = out.find("<theme>").unwrap();
    let linked = out.find("<linked_songs>").unwrap();
    let tempo = out.find("<tempo>").unwrap();
    let lyrics = out.find("<lyrics>").unwrap();
    let backgrounds = out.find("<backgrounds").unwrap();
    let style = out.find("<style>").unwrap();
    assert!(theme < linked && linked < tempo && lyrics < backgrounds && backgrounds < style, "{out}");
    let again = read_song(out.as_bytes()).unwrap();
    assert_eq!(again, canonicalize(&read).song);
    let names: Vec<_> = again.other_elements.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["linked_songs", "backgrounds", "style"]);
    assert_eq!(again.style, read.style);
    assert_eq!(again.backgrounds, read.backgrounds);
}

#[test]
fn capo_attributes_are_kept() {
    let out = written(&song("<song><capo print=\"true\" sharp=\"false\">3</capo></song>"));
    assert!(out.contains("<capo print=\"true\" sharp=\"false\">3</capo>"), "{out}");
}

#[test]
fn text_is_escaped() {
    let read = song("<song><title>Rock &amp; &lt;Roll&gt; \"x\"</title><lyrics>[V1]\n a &amp; b</lyrics></song>");
    let out = written(&read);
    assert!(out.contains("<title>Rock &amp; &lt;Roll&gt; \"x\"</title>"), "{out}");
    assert_eq!(read_song(out.as_bytes()).unwrap().title, read.title);
}

// ---- Tags and presentation order --------------------------------------------------------------

#[test]
fn bare_numbers_become_verse_tags_and_the_presentation_order_follows() {
    let read = read_song(&fixture_song("God Defend NZ - National Anthem")).unwrap();
    let canonical = read_song(&write_song(&read)).unwrap();
    assert_eq!(canonical.presentation, ["V6", "V1", "V2", "V3", "V4", "V5"]);
    assert!(canonical.lyrics.starts_with("[V1]\n"));
    // The slides are the same, only the tags changed.
    let text = |s: &Song| s.slides(&Default::default()).into_iter().map(|s| s.lines).collect::<Vec<_>>();
    assert_eq!(text(&canonical), text(&read));
}

#[test]
fn legacy_multilanguage_tags_become_the_l_option() {
    let read = song("<song><presentation>V1-L C-L V2L 1</presentation><lyrics>[V1-L]\n a\n b\n[C-L]\n c\n d\n[V2L]\n e\n f</lyrics></song>");
    let canonical = read_song(&write_song(&read)).unwrap();
    assert_eq!(canonical.lyrics, "[V1] (L)\n a\n b\n[C] (L)\n c\n d\n[V2] (L)\n e\n f");
    assert_eq!(canonical.presentation, ["V1", "C", "V2", "V1"]);
    assert!(canonical.sections.iter().all(|s| s.multilanguage));
}

#[test]
fn headers_are_written_in_the_one_canonical_form() {
    let canonical = canonicalize(&song("<song><lyrics>[V1](l)\n a\n b\n[C]  (L, L1=en) trailing\n c\n d\n[Coda] note\n e</lyrics></song>")).song;
    assert_eq!(canonical.lyrics, "[V1] (L)\n a\n b\n[C] (L)\n c\n d\n[Coda]\n e");
}

#[test]
fn an_empty_presentation_order_stays_empty() {
    let canonical = read_song(&write_song(&song("<song><lyrics>[1]\n a\n[2]\n b</lyrics></song>"))).unwrap();
    assert!(canonical.presentation.is_empty());
    assert!(written(&canonical).contains("<presentation></presentation>"));
}

#[test]
fn presentation_tokens_without_a_section_are_kept() {
    let canonical = canonicalize(&song("<song><presentation>1 X 2</presentation><lyrics>[1]\n a\n[2]\n b</lyrics></song>")).song;
    assert_eq!(canonical.presentation, ["V1", "X", "V2"]);
}

#[test]
fn numbered_verse_blocks_stay_compact() {
    let read = read_song(&fixture_song("Just A Closer Walk With Thee")).unwrap();
    let out = written(&read);
    assert!(out.contains("[V]\r\n.G          Em       G        Am     D\r\n1I am       weak but Thou art strong;\r\n2Thro'"), "{out}");
    assert!(out.contains(";Verses 1 &amp; 2 before Chorus"), "comments are kept: {out}");
}

#[test]
fn colliding_tags_are_not_renamed() {
    let read = song("<song><presentation>1 V1</presentation><lyrics>[1]\n one  \n[V1]\n vee one</lyrics></song>");
    assert!(has_tag_collision(&read));
    let canonical = canonicalize(&read);
    assert!(canonical.tags_kept);
    assert_eq!(canonical.song.lyrics, "[1]\n one\n[V1]\n vee one");
    assert_eq!(canonical.song.presentation, ["1", "V1"]);
    assert_eq!(read_song(&write_song(&read)).unwrap(), canonical.song);
}

#[test]
fn canonicalizing_twice_changes_nothing() {
    for name in FIXTURES {
        let once = canonicalize(&read_song(&fixture_song(name)).unwrap());
        assert!(!once.tags_kept, "{name}");
        assert_eq!(canonicalize(&once.song).song, once.song, "{name}");
    }
}
