//! Seam 2, the OpenSong codec: song bytes to model.
//!
//! Style: read a fixture with `fixture_song` (or inline XML for a legacy variant), parse it with
//! `serviceflow_opensong::read_song`, and compare against literals taken from the file itself.

use serviceflow_core::song::{Line, SectionKind, SlideOptions, Song, SongWarning};
use serviceflow_opensong::read_song;
use serviceflow_testkit::fixture_song;

fn lines(slide: &[&str]) -> Vec<String> {
    slide.iter().map(|s| s.to_string()).collect()
}

fn song(xml: &str) -> Song {
    read_song(xml.as_bytes()).unwrap()
}

fn tags(song: &Song) -> Vec<&str> {
    song.sections.iter().map(|s| s.tag.as_str()).collect()
}

/// The section's slides with default options.
fn slides_of(song: &Song, tag: &str) -> Vec<Vec<String>> {
    let section = song.sections.iter().find(|s| s.tag == tag).unwrap_or_else(|| panic!("no section {tag}"));
    section.slides(&SlideOptions::default())
}

/// The song's slides in presentation order, as (section, first line).
fn flow(song: &Song, options: &SlideOptions) -> Vec<(String, String)> {
    song.slides(options).into_iter().map(|s| (s.section, s.lines.first().cloned().unwrap_or_default())).collect()
}

// ---- The six fixtures -------------------------------------------------------------------------

#[test]
fn it_is_well_with_my_soul_reads_its_metadata() {
    let song = read_song(&fixture_song("It Is Well With My Soul")).unwrap();
    assert_eq!(song.title, "It Is Well With My Soul");
    assert_eq!(song.author, "Horatio Spafford and Philip Bliss");
    assert_eq!(song.copyright, "Public Domain");
    assert_eq!(song.ccli, "25376");
    assert_eq!(song.aka, "When Peace Like a River");
    assert_eq!(song.theme, "Fruit: Peace/Comfort; Worship: Assurance/Trust; ");
    assert_eq!(song.hymn_number, "");
    assert_eq!(song.key, "");
    assert_eq!(song.presentation, ["V1", "C", "V2", "C", "V3", "C", "V4", "C"]);
    assert!(song.warnings.is_empty(), "{:?}", song.warnings);
}

#[test]
fn it_is_well_with_my_soul_reads_its_sections_in_file_order() {
    let song = read_song(&fixture_song("It Is Well With My Soul")).unwrap();
    assert_eq!(tags(&song), ["V1", "C", "V2", "V3", "V4"]);
}

#[test]
fn a_slide_break_on_its_own_line_splits_a_section_into_slides() {
    let song = read_song(&fixture_song("It Is Well With My Soul")).unwrap();
    assert_eq!(
        slides_of(&song, "V1"),
        [
            lines(&["When peace like a river", "attendeth my way,", "When sorrows like", "sea billows roll..."]),
            lines(&["Whatever my lot,", "Thou hast taught me to say,", "It is well, it is well", "with my soul..."]),
        ]
    );
    assert_eq!(slides_of(&song, "C"), [lines(&["It is well (it is well)", "With my soul (with my soul)", "It is well,", "It is well with my soul"])]);
}

#[test]
fn god_defend_nz_has_bare_numeric_verse_tags_presented_as_6_1_2_3_4_5() {
    let song = read_song(&fixture_song("God Defend NZ - National Anthem")).unwrap();
    assert_eq!(tags(&song), ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]);
    assert!(song.sections.iter().all(|s| s.kind() == SectionKind::Verse));
    let firsts: Vec<_> = flow(&song, &SlideOptions::default()).into_iter().map(|(tag, first)| format!("{tag}: {first}")).collect();
    assert_eq!(
        firsts,
        [
            "6: E Ihowā Atua,",
            "1: God of Nations at Thy feet,",
            "2: Men of every creed and race,",
            "3: Peace, not war, shall be our boast,",
            "4: Let our love for Thee increase,",
            "5: May our mountains ever be",
        ]
    );
    let unused: Vec<_> = song.unused_slides(&SlideOptions::default()).into_iter().map(|s| s.section).collect();
    assert_eq!(unused, ["7", "8", "9", "10"]);
}

#[test]
fn amazing_grace_skips_verses_3_and_5_and_keeps_its_unknown_order_of_elements() {
    let song = read_song(&fixture_song("Amazing Grace HOF#50")).unwrap();
    assert_eq!(song.title, "Amazing Grace");
    // <hymn_number> and <time_sig> come after <lyrics> in this file.
    assert_eq!(song.hymn_number, "50");
    assert_eq!(song.user2, "1779,19 cent ");
    assert_eq!(song.presentation, ["1", "2", "4", "6"]);
    let sections: Vec<_> = flow(&song, &SlideOptions::default()).into_iter().map(|(tag, _)| tag).collect();
    assert_eq!(sections, ["1", "2", "4", "6"]);
    assert_eq!(slides_of(&song, "1"), [lines(&["Amazing grace,", "how sweet the sound", "That saved a wretch like me,", "I once was lost,", "but now am found,", "Was blind, but now I see...."])]);
    let unused: Vec<_> = song.unused_slides(&SlideOptions::default()).into_iter().map(|s| s.section).collect();
    assert_eq!(unused, ["3", "5"]);
}

#[test]
fn just_a_closer_walk_expands_its_numbered_verse_block_into_verses_sharing_chords() {
    let song = read_song(&fixture_song("Just A Closer Walk With Thee")).unwrap();
    assert_eq!(tags(&song), ["V", "V1", "V2", "V3", "C"]);
    assert_eq!(
        slides_of(&song, "V2"),
        [lines(&["Thro' this world of toil and snares,", "If I faulter, Lord  who  cares?", "Who with me my burden   shares?", "None but Thee, dear   Lord, none but  Thee."])]
    );
    let v1 = &song.sections[1];
    assert_eq!(
        v1.lines[..4],
        [
            Line::Chords("G          Em       G        Am     D".into()),
            Line::Lyric("I am       weak but Thou art strong;".into()),
            Line::Chords("D                   Dsus G".into()),
            Line::Lyric("Jesus keep me from  all  wrong.".into()),
        ]
    );
    // Every verse has the same four chord lines.
    let chords = |tag: &str| -> Vec<Line> {
        song.sections.iter().find(|s| s.tag == tag).unwrap().lines.iter().filter(|l| matches!(l, Line::Chords(_))).cloned().collect()
    };
    assert_eq!(chords("V1").len(), 4);
    assert_eq!(chords("V1"), chords("V2"));
    assert_eq!(chords("V1"), chords("V3"));
    let firsts: Vec<_> = flow(&song, &SlideOptions::default()).into_iter().map(|(tag, _)| tag).collect();
    assert_eq!(firsts, ["V1", "V2", "C", "V3", "C"]);
    assert!(song.warnings.is_empty(), "{:?}", song.warnings);
}

#[test]
fn just_a_closer_walk_keeps_alttheme_and_reads_without_a_final_newline() {
    let bytes = fixture_song("Just A Closer Walk With Thee");
    assert!(bytes.ends_with(b"</song>"));
    let song = read_song(&bytes).unwrap();
    assert_eq!(song.theme, "Worship: Intimacy");
    let names: Vec<_> = song.other_elements.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["alttheme"]);
    assert_eq!(song.other_elements[0].xml, "<alttheme>Church: Fellowship w/ God</alttheme>");
}

#[test]
fn a_gift_reads_multilanguage_tags_written_with_the_l_option() {
    let song = read_song(&fixture_song("一件礼物 A Gift")).unwrap();
    assert_eq!(song.title, "一件礼物 (A Gift)");
    assert_eq!(tags(&song), ["V1", "V2", "V3", "V4"]);
    assert_eq!(slides_of(&song, "V1")[0][..2], lines(&["有一件礼物 你收到没有", "There is a gift, have you received yet?"]));
    // Lyrics come first in this file; the metadata after them still reads.
    assert!(song.presentation.is_empty());
    let names: Vec<_> = song.other_elements.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["linked_songs", "backgrounds"]);
    assert!(song.other_elements[1].xml.contains(r#"resize="screen""#), "{}", song.other_elements[1].xml);
}

#[test]
fn how_deep_the_fathers_love_reads_its_legacy_dash_l_tags_as_written() {
    let song = read_song(&fixture_song("How Deep the Fathers Love + Chinese")).unwrap();
    assert_eq!(song.title, "How Deep the Father's Love 天父的爱何其深长");
    assert_eq!(tags(&song), ["V1-L", "V2-L", "V3-L"]);
    let v1 = slides_of(&song, "V1-L");
    assert_eq!(v1.len(), 2);
    assert_eq!(v1[1][0], "How great the pain of searing loss");
}

// ---- Legacy variants and line grammar ---------------------------------------------------------

#[test]
fn a_slide_break_inside_a_line_splits_the_line() {
    let song = song("<song><title>T</title><lyrics>[V1]\n first || second\n third</lyrics></song>");
    assert_eq!(slides_of(&song, "V1"), [lines(&["first"]), lines(&["second", "third"])]);
}

#[test]
fn chord_comment_and_layout_lines_are_not_shown() {
    let song = song("<song><lyrics>[C]\n.G    D\n;a comment\n-- page --\n Sing\n</lyrics></song>");
    assert_eq!(slides_of(&song, "C"), [lines(&["Sing"])]);
}

#[test]
fn underscores_are_removed_and_line_breaks_are_kept_for_the_renderer() {
    let song = song("<song><lyrics>[V1]\n A__ma_zing | grace\n</lyrics></song>");
    assert_eq!(slides_of(&song, "V1"), [lines(&["Amazing | grace"])]);
}

#[test]
fn lyrics_before_any_section_tag_form_an_untagged_section() {
    let song = song("<song><lyrics> Alone\n[C]\n Chorus</lyrics></song>");
    assert_eq!(tags(&song), ["", "C"]);
}

#[test]
fn an_empty_presentation_order_reads_as_no_tokens() {
    assert!(song("<song><title>T</title><presentation></presentation><lyrics>[V1]\n a</lyrics></song>").presentation.is_empty());
}

#[test]
fn bare_numeric_tags_are_verses_and_n_and_vn_name_the_same_section() {
    let song = song("<song><presentation>V2 1</presentation><lyrics>[1]\n one\n[2]\n two</lyrics></song>");
    assert_eq!(flow(&song, &SlideOptions::default()), [("2".to_string(), "two".to_string()), ("1".to_string(), "one".to_string())]);
    let song = crate::song("<song><presentation>2 1</presentation><lyrics>[V1]\n one\n[V2]\n two</lyrics></song>");
    assert_eq!(flow(&song, &SlideOptions::default()), [("V2".to_string(), "two".to_string()), ("V1".to_string(), "one".to_string())]);
}

#[test]
fn an_exact_tag_wins_over_the_number_alias() {
    let song = song("<song><presentation>1 V1</presentation><lyrics>[1]\n bare\n[V1]\n verse</lyrics></song>");
    assert_eq!(flow(&song, &SlideOptions::default()), [("1".to_string(), "bare".to_string()), ("V1".to_string(), "verse".to_string())]);
}

#[test]
fn only_digits_1_to_9_start_a_numbered_verse() {
    let song = song("<song><lyrics>[V]\n1one\n2two\n0zero\n!bang\nHello\n</lyrics></song>");
    assert_eq!(tags(&song), ["V", "V1", "V2"]);
    assert_eq!(slides_of(&song, "V"), [lines(&["0zero", "!bang", "Hello"])]);
    assert_eq!(
        song.warnings,
        [
            SongWarning::StrayFirstCharacter { line: 4, character: '0' },
            SongWarning::StrayFirstCharacter { line: 5, character: '!' },
            SongWarning::StrayFirstCharacter { line: 6, character: 'H' },
        ]
    );
}

#[test]
fn a_chord_line_before_plain_lyrics_stays_with_its_section() {
    let song = song("<song><lyrics>[C]\n.G\n Sing\n Plain</lyrics></song>");
    assert_eq!(song.sections[0].lines, [Line::Chords("G".into()), Line::Lyric("Sing".into()), Line::Lyric("Plain".into())]);
}

#[test]
fn repeated_tags_have_their_lines_concatenated() {
    let song = song("<song><lyrics>[C]\n first\n[V1]\n verse\n[c]\n second</lyrics></song>");
    assert_eq!(tags(&song), ["C", "V1"]);
    assert_eq!(slides_of(&song, "C"), [lines(&["first", "second"])]);
}

#[test]
fn the_section_type_comes_from_the_first_letter_of_the_tag() {
    let song = song("<song><lyrics>[V1]\n a\n[P]\n a\n[Chorus 2]\n a\n[B]\n a\n[t]\n a\n[Coda]\n a\n[10]\n a</lyrics></song>");
    let kinds: Vec<_> = song.sections.iter().map(|s| (s.tag.as_str(), s.kind())).collect();
    use SectionKind::*;
    assert_eq!(
        kinds,
        [("V1", Verse), ("P", PreChorus), ("Chorus 2", Chorus), ("B", Bridge), ("t", Tag), ("Coda", Chorus), ("10", Verse)]
    );
}

#[test]
fn blank_lines_split_slides_only_when_blank_is_slide_change_is_on() {
    let song = song("<song><lyrics>[V1]\n one\n\n two\n \n\n three\n</lyrics></song>");
    assert_eq!(slides_of(&song, "V1"), [lines(&["one", "two", "three"])]);
    let on = SlideOptions { blank_is_slide_change: true };
    assert_eq!(song.sections[0].slides(&on), [lines(&["one"]), lines(&["two"]), lines(&["three"])]);
}

#[test]
fn tags_missing_from_the_presentation_order_are_skipped() {
    let song = song("<song><presentation>V1 X V9 C</presentation><lyrics>[V1]\n one\n[C]\n chorus</lyrics></song>");
    assert_eq!(flow(&song, &SlideOptions::default()), [("V1".to_string(), "one".to_string()), ("C".to_string(), "chorus".to_string())]);
}

#[test]
fn a_tag_with_spaces_matches_a_token_with_no_break_spaces() {
    let song = song("<song><presentation>Coda\u{a0}2</presentation><lyrics>[Coda 2]\n end</lyrics></song>");
    assert_eq!(flow(&song, &SlideOptions::default()), [("Coda 2".to_string(), "end".to_string())]);
}

// ---- The document -----------------------------------------------------------------------------

#[test]
fn missing_elements_read_as_empty() {
    let song = song("<song><lyrics>[V1]\n a</lyrics></song>");
    assert_eq!(song.title, "");
    assert_eq!(song.ccli, "");
    assert!(song.other_elements.is_empty());
}

#[test]
fn the_legacy_timesig_element_reads_as_the_time_signature() {
    assert_eq!(song("<song><timesig>3/4</timesig></song>").time_sig, "3/4");
    assert_eq!(song("<song><time_sig>4/4</time_sig><timesig>3/4</timesig></song>").time_sig, "4/4");
}

#[test]
fn unknown_elements_keep_their_attributes_children_and_escaping() {
    let song = song(r#"<song><title>T</title><custom a="x &amp; &quot;y&quot;"><inner>1 &lt; 2</inner><empty/></custom></song>"#);
    assert_eq!(song.other_elements.len(), 1);
    assert_eq!(song.other_elements[0].xml, r#"<custom a="x &amp; &quot;y&quot;"><inner>1 &lt; 2</inner><empty/></custom>"#);
}

#[test]
fn crlf_line_endings_read_like_lf() {
    let song = song("<song>\r\n<lyrics>[V1]\r\n one\r\n two\r\n</lyrics>\r\n</song>\r\n");
    assert_eq!(slides_of(&song, "V1"), [lines(&["one", "two"])]);
}

#[test]
fn a_latin_1_song_is_decoded() {
    let mut bytes = b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><song><title>Caf".to_vec();
    bytes.push(0xE9);
    bytes.extend_from_slice(b"</title></song>");
    assert_eq!(read_song(&bytes).unwrap().title, "Café");
}

#[test]
fn undeclared_bytes_that_are_not_utf_8_read_as_windows_1252() {
    let mut bytes = b"<song><title>".to_vec();
    bytes.extend_from_slice(&[0x93, b'H', b'i', 0x94]);
    bytes.extend_from_slice(b"</title></song>");
    assert_eq!(read_song(&bytes).unwrap().title, "\u{201c}Hi\u{201d}");
}

#[test]
fn a_utf_16_song_with_a_bom_is_decoded_even_when_it_claims_utf_8() {
    let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><song><title>一件礼物</title></song>";
    let mut le = vec![0xFF, 0xFE];
    le.extend(xml.encode_utf16().flat_map(u16::to_le_bytes));
    assert_eq!(read_song(&le).unwrap().title, "一件礼物");
    let mut be = vec![0xFE, 0xFF];
    be.extend(xml.encode_utf16().flat_map(u16::to_be_bytes));
    assert_eq!(read_song(&be).unwrap().title, "一件礼物");
}

#[test]
fn a_gbk_song_is_decoded() {
    // "礼物" in GBK.
    let mut bytes = b"<?xml version=\"1.0\" encoding=\"GBK\"?><song><title>".to_vec();
    bytes.extend_from_slice(&[0xC0, 0xF1, 0xCE, 0xEF]);
    bytes.extend_from_slice(b"</title></song>");
    assert_eq!(read_song(&bytes).unwrap().title, "礼物");
}

#[test]
fn a_utf_8_bom_is_ignored() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice("<song><title>Ā</title></song>".as_bytes());
    assert_eq!(read_song(&bytes).unwrap().title, "Ā");
}

#[test]
fn bytes_that_are_not_a_song_are_an_error() {
    assert!(read_song(b"<set/>").is_err());
    assert!(read_song(b"not xml at all <").is_err());
}
