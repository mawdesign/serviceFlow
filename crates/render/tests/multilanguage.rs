//! Multilanguage slides (#11): translation lines styled from the body, the pair gap, auto-scale
//! of the combined block, and snapshots of both fixture songs through the engine seam.

use serviceflow_core::command::Command;
use serviceflow_core::ids::SongId;
use serviceflow_core::song::Slide;
use serviceflow_core::style::{Rgba, Style};
use serviceflow_render::{LINE_PITCH, Line, LineRole, Renderer, Size};
use serviceflow_testkit::EngineHarness;

const HD: Size = Size { width: 1920, height: 1080 };

fn multilanguage(lines: &[&str]) -> Slide {
    Slide { section: "V1".into(), lines: lines.iter().map(|l| l.to_string()).collect(), multilanguage: true }
}

fn body_lines(slide: &Slide, style: &Style, size: Size) -> Vec<Line> {
    Renderer::new().layout(slide, style, size).boxes.pop().expect("a body box").lines
}

#[test]
fn translation_lines_are_smaller_flipped_italic_and_in_the_multilanguage_colour() {
    let mut style = Style::default();
    style.body.multilanguage_size = 70;
    style.body.multilanguage_color = Rgba::rgb(200, 0, 0);
    style.body.text.outline = true;
    style.body.text.shadow = true;
    let lines = body_lines(&multilanguage(&["Primary", "Translation"]), &style, HD);
    let roles: Vec<_> = lines.iter().map(|l| l.role).collect();
    assert_eq!(roles, [LineRole::Lyric, LineRole::Translation]);
    let (p, t) = (&lines[0], &lines[1]);
    assert_eq!(p.font_size, 72.0);
    assert_eq!(t.font_size, (72.0_f32 * 0.7).floor());
    assert_eq!(t.runs[0].color, Rgba::rgb(200, 0, 0));
    assert_eq!(p.runs[0].color, style.body.text.color);
    assert!(!p.runs[0].effects.italic);
    assert!(t.runs[0].effects.italic, "the body is upright, so translations are italic");
    assert_eq!(t.runs[0].effects.outline.is_some(), p.runs[0].effects.outline.is_some());
    assert_eq!(t.runs[0].effects.shadow, p.runs[0].effects.shadow);

    style.body.text.italic = true;
    let lines = body_lines(&multilanguage(&["Primary", "Translation"]), &style, HD);
    assert!(lines[0].runs[0].effects.italic && !lines[1].runs[0].effects.italic);
}

#[test]
fn each_primary_line_after_the_first_gets_a_gap_of_a_fifth_of_a_translation_line() {
    let style = Style::default();
    let lines = body_lines(&multilanguage(&["P one", "T one", "P two", "T two"]), &style, HD);
    let t = lines[1].font_size;
    // Line tops: the distance from a translation line to the next primary line is its own
    // pitch plus the gap; from a primary line to its translation, just the primary's pitch.
    let pitch = |l: &Line| l.font_size * LINE_PITCH;
    let tops = line_tops(&lines);
    assert!((tops[1] - tops[0] - pitch(&lines[0])).abs() < 0.01);
    assert!((tops[2] - tops[1] - pitch(&lines[1]) - 0.2 * t * LINE_PITCH).abs() < 0.01, "{tops:?}");
    assert!((tops[3] - tops[2] - pitch(&lines[2])).abs() < 0.01);
}

/// The top of each line's slot, from its baseline (the slot is centred on the font's extent).
fn line_tops(lines: &[Line]) -> Vec<f32> {
    let fonts = serviceflow_render::Fonts::builtin();
    let font = fonts.resolve(&Style::default().body.text.font);
    lines
        .iter()
        .map(|l| {
            let (ascent, descent) = fonts.vertical_metrics(font, l.font_size);
            let lead = (l.font_size * LINE_PITCH - (ascent - descent)) / 2.0;
            l.baseline - ascent - lead
        })
        .collect()
}

#[test]
fn a_wrapped_primary_line_gets_no_gap_inside_it() {
    let long = "A primary line long enough that it has to wrap onto a second line on screen";
    let lines = body_lines(&multilanguage(&[long, "T"]), &Style::default(), HD);
    assert_eq!(lines.iter().map(|l| l.role).collect::<Vec<_>>(), [LineRole::Lyric, LineRole::Lyric, LineRole::Translation]);
    let tops = line_tops(&lines);
    assert!((tops[1] - tops[0] - lines[0].font_size * LINE_PITCH).abs() < 0.01);
}

#[test]
fn auto_scale_fits_the_combined_block_and_translations_shrink_in_proportion() {
    let mut style = Style::default();
    style.body.auto_scale = true;
    let text: Vec<String> = (0..8).flat_map(|i| [format!("Primary line number {i}"), format!("Translation line {i}")]).collect();
    let refs: Vec<&str> = text.iter().map(String::as_str).collect();
    let frame = Renderer::new().layout(&multilanguage(&refs), &style, HD);
    let body = &frame.boxes[0];
    let p = body.lines[0].font_size;
    assert!(p < 72.0, "the block must have been shrunk");
    assert_eq!(body.lines[1].font_size, (p * 0.7).floor());
    let tops = line_tops(&body.lines);
    let last = body.lines.last().unwrap();
    let bottom = tops.last().unwrap() + last.font_size * LINE_PITCH;
    assert!(tops[0] >= body.rect.y - 0.01 && bottom <= body.rect.y + body.rect.height + 0.01, "{} {}", tops[0], bottom);
}

#[test]
fn a_plain_slide_has_no_translation_lines() {
    let slide = Slide { lines: vec!["One".into(), "Two".into()], ..Slide::default() };
    assert!(body_lines(&slide, &Style::default(), HD).iter().all(|l| l.role == LineRole::Lyric));
}

fn song_layout(song: &str, size: Size) -> String {
    let mut harness = EngineHarness::with_fixtures();
    harness.send(Command::OpenSong(SongId::new(song)));
    let state = harness.state();
    let open = state.song.as_ref().expect("the song opened");
    let renderer = Renderer::new();
    let style = Style::default();
    open.slides
        .iter()
        .enumerate()
        .map(|(i, slide)| format!("# slide {} ({})\n{}", i + 1, slide.section, renderer.layout(slide, &style, size).snapshot()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_gift_at_1080p() {
    insta::assert_snapshot!(song_layout("一件礼物 A Gift", HD));
}

#[test]
fn how_deep_the_fathers_love_at_1080p() {
    insta::assert_snapshot!(song_layout("How Deep the Fathers Love + Chinese", HD));
}
