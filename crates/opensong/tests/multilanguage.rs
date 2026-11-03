//! Seam 2, the OpenSong codec: multilanguage sections (#11, ADR-0007).
//!
//! Which tags are multilanguage, the warnings for tags that look like they meant to be, the
//! per-slide pairing of primary and translation lines, and `||` / `|` inside pairs.

use serviceflow_core::song::{LanguageMode, SlideOptions, Song, SongWarning};
use serviceflow_opensong::read_song;
use serviceflow_testkit::fixture_song;

fn song(lyrics: &str) -> Song {
    read_song(format!("<song><lyrics>{lyrics}</lyrics></song>").as_bytes()).unwrap()
}

fn multilanguage(song: &Song) -> Vec<(&str, bool)> {
    song.sections.iter().map(|s| (s.tag.as_str(), s.multilanguage)).collect()
}

fn slides(song: &Song, options: &SlideOptions) -> Vec<(bool, Vec<String>)> {
    song.slides(options).into_iter().map(|s| (s.multilanguage, s.lines)).collect()
}

fn both() -> SlideOptions {
    SlideOptions::default()
}

fn lines(l: &[&str]) -> Vec<String> {
    l.iter().map(|s| s.to_string()).collect()
}

// ---- Recognising multilanguage sections -------------------------------------------------------

#[test]
fn the_l_option_marks_a_section_multilanguage_with_or_without_a_space() {
    let song = song("[V1] (L)\n a\n[V2](L)\n b\n[V3] (l)\n c\n[V4] (x=1 L)\n d\n[V5] (en)\n e\n[C]\n f");
    assert_eq!(multilanguage(&song), [("V1", true), ("V2", true), ("V3", true), ("V4", true), ("V5", false), ("C", false)]);
    assert!(song.warnings.is_empty(), "{:?}", song.warnings);
}

#[test]
fn strict_legacy_tags_are_multilanguage_and_keep_their_tag() {
    let song = song("[V1-L]\n a\n[C-L]\n b\n[V2L]\n c\n[B12L]\n d\n[t3-l]\n e");
    assert_eq!(multilanguage(&song), [("V1-L", true), ("C-L", true), ("V2L", true), ("B12L", true), ("t3-l", true)]);
    assert!(song.warnings.is_empty(), "{:?}", song.warnings);
}

#[test]
fn final_and_instrumental_are_not_multilanguage_but_raise_a_warning() {
    let song = song("[Final]\n a\n[Instrumental]\n b\n[CL]\n c\n[X1-L]\n d\n[Coda]\n e\n[L]\n f");
    assert_eq!(multilanguage(&song), [("Final", false), ("Instrumental", false), ("CL", false), ("X1-L", false), ("Coda", false), ("L", false)]);
    assert_eq!(
        song.warnings,
        [
            SongWarning::MaybeMultilanguage { line: 1, tag: "Final".into() },
            SongWarning::MaybeMultilanguage { line: 3, tag: "Instrumental".into() },
            SongWarning::MaybeMultilanguage { line: 5, tag: "CL".into() },
            SongWarning::MaybeMultilanguage { line: 7, tag: "X1-L".into() },
        ]
    );
}

#[test]
fn an_explicit_l_option_silences_the_warning() {
    let song = song("[Final] (L)\n a");
    assert_eq!(multilanguage(&song), [("Final", true)]);
    assert!(song.warnings.is_empty());
}

#[test]
fn the_flag_is_sticky_and_numbered_verses_inherit_it() {
    let song = song("[V3]\n a\n[V3] (L)\n b\n[V1] (L)\n c\n[V1]\n d\n[V] (L)\n1 one\n1 un\n2 two\n2 deux");
    // `V1` from the numbered block continues the earlier V1.
    assert_eq!(multilanguage(&song), [("V3", true), ("V1", true), ("V", true), ("V2", true)]);
}

#[test]
fn a_legacy_token_finds_the_modern_section_and_the_other_way_round() {
    let modern = read_song(b"<song><presentation>V1-L V2L</presentation><lyrics>[V1] (L)\n a\n[V2] (L)\n b</lyrics></song>").unwrap();
    assert_eq!(modern.flow().iter().map(|s| s.tag.as_str()).collect::<Vec<_>>(), ["V1", "V2"]);
    let legacy = read_song(b"<song><presentation>V2 V1</presentation><lyrics>[V1-L]\n a\n[V2L]\n b</lyrics></song>").unwrap();
    assert_eq!(legacy.flow().iter().map(|s| s.tag.as_str()).collect::<Vec<_>>(), ["V2L", "V1-L"]);
}

// ---- Pairing ----------------------------------------------------------------------------------

#[test]
fn multilanguage_slides_alternate_primary_and_translation_lines() {
    let song = song("[V1] (L)\n p1\n t1\n p2\n t2\n[C]\n c");
    assert_eq!(slides(&song, &both()), [(true, lines(&["p1", "t1", "p2", "t2"])), (false, lines(&["c"]))]);
}

#[test]
fn chord_comment_layout_and_blank_lines_do_not_count_in_the_pairing() {
    let song = song("[V1] (L)\n.G   D\n p1\n;comment\n-- page --\n\n t1\n.C\n p2\n t2");
    assert_eq!(slides(&song, &both()), [(true, lines(&["p1", "t1", "p2", "t2"]))]);
}

#[test]
fn pairing_restarts_on_every_slide() {
    let song = song("[V1] (L)\n p1\n t1\n ||\n p2\n t2\n p3 || t3x\n");
    // `p3 || t3x`: the break falls between a primary line and its translation (warned below), so
    // it moves after the translation line.
    assert_eq!(slides(&song, &both()), [(true, lines(&["p1", "t1"])), (true, lines(&["p2", "t2", "p3", "t3x"]))]);
}

#[test]
fn a_break_on_its_own_line_at_a_pair_boundary_is_a_clean_slide_break() {
    let song = song("[V1-L]\n p1\n t1\n ||\n p2\n t2");
    assert_eq!(slides(&song, &both()), [(true, lines(&["p1", "t1"])), (true, lines(&["p2", "t2"]))]);
    assert!(song.warnings.is_empty());
}

#[test]
fn a_break_at_the_end_of_a_translation_line_is_a_clean_slide_break() {
    let song = song("[V1-L]\n p1\n t1 ||\n p2\n t2");
    assert_eq!(slides(&song, &both()), [(true, lines(&["p1", "t1"])), (true, lines(&["p2", "t2"]))]);
    assert!(song.warnings.is_empty());
}

#[test]
fn a_break_between_a_primary_line_and_its_translation_moves_after_the_translation_with_a_warning() {
    let trailing = song("[V1] (L)\n p1 ||\n t1\n p2\n t2");
    assert_eq!(slides(&trailing, &both()), [(true, lines(&["p1", "t1"])), (true, lines(&["p2", "t2"]))]);
    assert_eq!(trailing.warnings, [SongWarning::BreakInsidePair { line: 2 }]);

    let own_line = song("[V1] (L)\n p1\n ||\n t1\n p2\n t2");
    assert_eq!(slides(&own_line, &both()), [(true, lines(&["p1", "t1"])), (true, lines(&["p2", "t2"]))]);
    assert_eq!(own_line.warnings, [SongWarning::BreakInsidePair { line: 3 }]);
}

#[test]
fn a_blank_line_slide_change_inside_a_pair_also_waits_for_the_translation() {
    let song = song("[V1] (L)\n p1\n\n t1\n p2\n t2");
    let options = SlideOptions { blank_is_slide_change: true, ..SlideOptions::default() };
    assert_eq!(slides(&song, &options), [(true, lines(&["p1", "t1"])), (true, lines(&["p2", "t2"]))]);
}

#[test]
fn a_line_break_inside_a_pair_splits_both_lines_and_keeps_them_paired() {
    let song = song("[V1] (L)\n a | b\n x | y\n c | d | e\n z");
    assert_eq!(slides(&song, &both()), [(true, lines(&["a", "x", "b", "y", "c | d | e", "z"]))]);
}

#[test]
fn uneven_line_breaks_pair_up_as_far_as_they_go() {
    let song = song("[V1] (L)\n a | b | c\n x | y");
    assert_eq!(slides(&song, &both()), [(true, lines(&["a", "x", "b | c", "y"]))]);
}

#[test]
fn a_last_primary_line_without_translation_stands_alone() {
    let song = song("[V1] (L)\n p1\n t1\n p2");
    assert_eq!(slides(&song, &both()), [(true, lines(&["p1", "t1", "p2"]))]);
}

// ---- Language mode ----------------------------------------------------------------------------

#[test]
fn primary_only_and_translation_only_show_plain_slides_of_one_language() {
    let song = song("[V1] (L)\n p1\n t1\n p2 | q2\n t2\n p3\n[C]\n c");
    let primary = SlideOptions { language: LanguageMode::Primary, ..SlideOptions::default() };
    let translation = SlideOptions { language: LanguageMode::Translation, ..SlideOptions::default() };
    assert_eq!(slides(&song, &primary), [(false, lines(&["p1", "p2 | q2", "p3"])), (false, lines(&["c"]))]);
    assert_eq!(slides(&song, &translation), [(false, lines(&["t1", "t2"])), (false, lines(&["c"]))]);
}

#[test]
fn a_slide_with_no_translation_line_stays_as_an_empty_slide_in_translation_mode() {
    let song = song("[V1] (L)\n p1\n t1\n ||\n p2");
    let translation = SlideOptions { language: LanguageMode::Translation, ..SlideOptions::default() };
    assert_eq!(slides(&song, &both()), [(true, lines(&["p1", "t1"])), (true, lines(&["p2"]))]);
    assert_eq!(slides(&song, &translation), [(false, lines(&["t1"])), (false, Vec::new())]);
}

// ---- The fixtures -----------------------------------------------------------------------------

#[test]
fn a_gift_has_the_chinese_line_as_primary() {
    let song = read_song(&fixture_song("一件礼物 A Gift")).unwrap();
    let all = slides(&song, &both());
    assert_eq!(all.len(), 4);
    assert!(all.iter().all(|(m, l)| *m && l.len() == 8));
    assert_eq!(all[0].1[..2], lines(&["有一件礼物 你收到没有", "There is a gift, have you received yet?"]));
    assert!(song.warnings.is_empty(), "{:?}", song.warnings);
}

#[test]
fn how_deep_the_fathers_love_splits_cleanly_at_its_own_line_breaks() {
    let song = read_song(&fixture_song("How Deep the Fathers Love + Chinese")).unwrap();
    let all = slides(&song, &both());
    assert_eq!(all.len(), 6);
    assert!(all.iter().all(|(m, l)| *m && l.len() == 8), "{all:#?}");
    assert_eq!(all[1].1[..2], lines(&["How great the pain of searing loss", "离弃之痛是何其深，"]));
    assert_eq!(all[5].1[7], "祂受鞭伤赎我罪债。");
    assert!(song.warnings.is_empty(), "{:?}", song.warnings);
}
