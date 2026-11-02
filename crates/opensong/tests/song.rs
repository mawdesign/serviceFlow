//! Seam 2, the OpenSong codec: song bytes to model.
//!
//! Style: read a fixture with `fixture_song` (or inline XML for a legacy variant), parse it with
//! `serviceflow_opensong::read_song`, and compare against literals taken from the file itself.

use serviceflow_opensong::read_song;
use serviceflow_testkit::fixture_song;

fn lines(slide: &[&str]) -> Vec<String> {
    slide.iter().map(|s| s.to_string()).collect()
}

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
}

#[test]
fn it_is_well_with_my_soul_reads_its_sections_in_file_order() {
    let song = read_song(&fixture_song("It Is Well With My Soul")).unwrap();
    let tags: Vec<_> = song.sections.iter().map(|s| s.tag.as_str()).collect();
    assert_eq!(tags, ["V1", "C", "V2", "V3", "V4"]);
}

#[test]
fn a_slide_break_on_its_own_line_splits_a_section_into_slides() {
    let song = read_song(&fixture_song("It Is Well With My Soul")).unwrap();
    let v1 = &song.sections[0];
    assert_eq!(
        v1.slides,
        [
            lines(&["When peace like a river", "attendeth my way,", "When sorrows like", "sea billows roll..."]),
            lines(&["Whatever my lot,", "Thou hast taught me to say,", "It is well, it is well", "with my soul..."]),
        ]
    );
    let chorus = &song.sections[1];
    assert_eq!(chorus.slides, [lines(&["It is well (it is well)", "With my soul (with my soul)", "It is well,", "It is well with my soul"])]);
}

#[test]
fn a_slide_break_inside_a_line_splits_the_line() {
    let xml = "<song><title>T</title><lyrics>[V1]\n first || second\n third</lyrics></song>";
    let song = read_song(xml.as_bytes()).unwrap();
    assert_eq!(song.sections[0].slides, [lines(&["first"]), lines(&["second", "third"])]);
}

#[test]
fn chord_comment_and_layout_lines_are_not_lyrics() {
    let xml = "<song><lyrics>[C]\n.G    D\n;a comment\n-- page --\n Sing\n</lyrics></song>";
    let song = read_song(xml.as_bytes()).unwrap();
    assert_eq!(song.sections[0].slides, [lines(&["Sing"])]);
}

#[test]
fn lyrics_before_any_section_tag_form_an_untagged_section() {
    let xml = "<song><lyrics> Alone\n[C]\n Chorus</lyrics></song>";
    let song = read_song(xml.as_bytes()).unwrap();
    let tags: Vec<_> = song.sections.iter().map(|s| s.tag.as_str()).collect();
    assert_eq!(tags, ["", "C"]);
}

#[test]
fn an_empty_presentation_order_reads_as_no_tokens() {
    let xml = "<song><title>T</title><presentation></presentation><lyrics>[V1]\n a</lyrics></song>";
    assert!(read_song(xml.as_bytes()).unwrap().presentation.is_empty());
}

#[test]
fn bytes_that_are_not_a_song_are_an_error() {
    assert!(read_song(b"<set/>").is_err());
    assert!(read_song(b"not xml at all <").is_err());
}
