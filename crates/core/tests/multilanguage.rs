//! Seam 1, the engine: multilanguage songs and the language mode toggle (#11).

use serviceflow_core::command::Command;
use serviceflow_core::ids::SongId;
use serviceflow_core::song::{LanguageMode, Slide};
use serviceflow_testkit::EngineHarness;

fn open(harness: &mut EngineHarness, name: &str) {
    harness.send(Command::OpenSong(SongId::new(name)));
}

fn grid(harness: &EngineHarness) -> Vec<Slide> {
    harness.state().song.as_ref().expect("a song is open").slides.clone()
}

fn live(harness: &EngineHarness) -> Slide {
    harness.state().output.live.as_ref().expect("something is live").slide.clone()
}

#[test]
fn a_gift_shows_each_chinese_primary_line_with_its_english_translation() {
    let mut harness = EngineHarness::with_fixtures();
    open(&mut harness, "一件礼物 A Gift");
    let slides = grid(&harness);
    assert_eq!(slides.iter().map(|s| s.section.as_str()).collect::<Vec<_>>(), ["V1", "V2", "V3", "V4"]);
    assert!(slides.iter().all(|s| s.multilanguage && s.lines.len() == 8));
    assert_eq!(slides[2].lines[..2], ["亲爱的朋友 你是否想到", "Dear friend, if have you ever thought..."]);
    assert!(harness.state().song.as_ref().unwrap().song.warnings.is_empty());
}

#[test]
fn how_deep_the_fathers_love_breaks_cleanly_at_each_break_on_its_own_line() {
    let mut harness = EngineHarness::with_fixtures();
    open(&mut harness, "How Deep the Fathers Love + Chinese");
    let slides = grid(&harness);
    let sections: Vec<_> = slides.iter().map(|s| s.section.as_str()).collect();
    assert_eq!(sections, ["V1-L", "V1-L", "V2-L", "V2-L", "V3-L", "V3-L"]);
    assert!(slides.iter().all(|s| s.multilanguage && s.lines.len() == 8));
    // English is the primary line here; every pair stays together across the breaks.
    for slide in &slides {
        for pair in slide.lines.chunks(2) {
            let chinese = |s: &str| s.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c));
            assert!(!chinese(&pair[0]) && chinese(&pair[1]), "{pair:?}");
        }
    }
    assert_eq!(slides[3].lines[..2], ["It was my sin that held Him there", "祂为我罪甘愿受苦，"]);
}

#[test]
fn the_language_mode_switches_the_grid_and_the_live_slide() {
    let mut harness = EngineHarness::with_fixtures();
    open(&mut harness, "一件礼物 A Gift");
    harness.send(Command::GoLive { index: 1 });
    let changes = harness.state().output.changes;

    harness.send(Command::SetLanguageMode(LanguageMode::Primary));
    assert_eq!(harness.state().slide_options.language, LanguageMode::Primary);
    let live_slide = live(&harness);
    assert!(!live_slide.multilanguage);
    assert_eq!(live_slide.lines, ["生命有限 时光也会走", "如果你不珍惜 机会难留", "礼物虽然好 如果你不要", "你怎么能够得到? 怎么能得到?"]);
    assert_eq!(harness.state().output.live.as_ref().unwrap().index, 1);
    assert!(harness.state().output.changes > changes, "the output redraws");
    assert!(grid(&harness).iter().all(|s| s.lines.len() == 4 && !s.multilanguage));

    harness.send(Command::SetLanguageMode(LanguageMode::Translation));
    assert_eq!(live(&harness).lines[0], "Life is short, time will pass, if you don't grasp it,");

    harness.send(Command::SetLanguageMode(LanguageMode::Both));
    assert_eq!(live(&harness), grid(&harness)[1]);
    assert!(live(&harness).multilanguage);
}

#[test]
fn the_language_mode_also_changes_a_live_slide_from_a_song_no_longer_open() {
    let mut harness = EngineHarness::with_fixtures();
    open(&mut harness, "一件礼物 A Gift");
    harness.send(Command::GoLive { index: 0 });
    open(&mut harness, "Amazing Grace HOF#50");
    harness.send(Command::SetLanguageMode(LanguageMode::Translation));
    assert_eq!(live(&harness).lines[0], "There is a gift, have you received yet?");
}

#[test]
fn other_songs_do_not_change_with_the_language_mode() {
    let mut harness = EngineHarness::with_fixtures();
    open(&mut harness, "Amazing Grace HOF#50");
    let before = grid(&harness);
    harness.send(Command::SetLanguageMode(LanguageMode::Translation));
    assert_eq!(grid(&harness), before);
}

#[test]
fn switching_the_language_mode_is_a_live_action() {
    assert!(Command::SetLanguageMode(LanguageMode::Primary).is_live_action());
}
