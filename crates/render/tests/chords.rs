//! Chord lines (#24): placed above the lyric line they belong to, aligned by character, and
//! snapshots of fixture songs through the engine seam.

use serviceflow_core::command::Command;
use serviceflow_core::ids::SongId;
use serviceflow_core::song::{Chord, Slide};
use serviceflow_core::style::Style;
use serviceflow_render::{LINE_PITCH, Line, LineRole, Renderer, Size};
use serviceflow_testkit::EngineHarness;

const HD: Size = Size { width: 1920, height: 1080 };

fn chord(offset: usize, text: &str) -> Chord {
    Chord { offset, text: text.into() }
}

fn body_lines(slide: &Slide, style: &Style, size: Size) -> Vec<Line> {
    Renderer::new().layout(slide, style, size).boxes.pop().expect("a body box").lines
}

fn roles(lines: &[Line]) -> Vec<LineRole> {
    lines.iter().map(|l| l.role).collect()
}

fn first_x(line: &Line) -> f32 {
    line.runs[0].glyphs[0].x
}

#[test]
fn a_chord_line_goes_above_its_lyric_line_in_the_chord_size_and_colour() {
    let slide = Slide { lines: vec!["Amazing".into(), "grace".into()], chords: vec![vec![chord(0, "G")], vec![]], ..Slide::default() };
    let style = Style { chord_size: 50, ..Style::default() };
    let lines = body_lines(&slide, &style, HD);
    assert_eq!(roles(&lines), [LineRole::Chord, LineRole::Lyric, LineRole::Lyric]);
    let (chords, lyric) = (&lines[0], &lines[1]);
    assert_eq!(chords.font_size, 36.0);
    assert_eq!(chords.runs[0].color, style.chord_color);
    assert!(chords.baseline < lyric.baseline);
    assert_eq!(chords.text, "G");
    // The block is centred as a whole; the chord's pitch is part of it.
    let pitch = |l: &Line| l.font_size * LINE_PITCH;
    assert!((lyric.baseline - chords.baseline) > pitch(chords) * 0.5);
}

#[test]
fn a_chord_sits_over_the_same_x_as_the_character_it_is_anchored_to() {
    // Plain Latin letters, no ligatures: glyph k of the lyric line is character k.
    let slide = Slide { lines: vec!["Amazing grace".into()], chords: vec![vec![chord(0, "G"), chord(8, "Em")]], ..Slide::default() };
    let lines = body_lines(&slide, &Style::default(), HD);
    let (chords, lyric) = (&lines[0], &lines[1]);
    let glyphs = &lyric.runs[0].glyphs;
    assert!((chords.runs[0].glyphs[0].x - glyphs[0].x).abs() < 0.01, "G over A");
    assert!((chords.runs[1].glyphs[0].x - glyphs[8].x).abs() < 0.01, "Em over g");
}

#[test]
fn the_chord_line_moves_with_the_lyric_line_when_it_is_aligned() {
    let slide = Slide { lines: vec!["Hi".into()], chords: vec![vec![chord(0, "G")]], ..Slide::default() };
    let mut style = Style::default();
    style.body.text.align = serviceflow_core::style::Align::Right;
    let lines = body_lines(&slide, &style, HD);
    assert!((first_x(&lines[0]) - first_x(&lines[1])).abs() < 0.01);
    style.body.text.align = serviceflow_core::style::Align::Left;
    let lines = body_lines(&slide, &style, HD);
    assert!((first_x(&lines[0]) - first_x(&lines[1])).abs() < 0.01);
}

#[test]
fn chords_that_would_collide_are_pushed_right() {
    let slide = Slide { lines: vec!["abcdef".into()], chords: vec![vec![chord(0, "Gsus4"), chord(1, "Em7")]], ..Slide::default() };
    let lines = body_lines(&slide, &Style::default(), HD);
    let (first, second) = (&lines[0].runs[0], &lines[0].runs[1]);
    let first_end = first.glyphs.last().unwrap().x;
    assert!(second.glyphs[0].x > first_end, "{} > {first_end}", second.glyphs[0].x);
}

#[test]
fn chords_follow_their_words_when_a_long_line_wraps() {
    let long = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega";
    let slide = Slide { lines: vec![long.into()], chords: vec![vec![chord(0, "G"), chord(long.len() - 5, "D")]], ..Slide::default() };
    let lines = body_lines(&slide, &Style::default(), HD);
    let order = roles(&lines);
    assert!(order.iter().filter(|r| **r == LineRole::Lyric).count() > 1, "the line must wrap");
    // The first chord is above the first wrapped line, the last one above the last.
    assert_eq!(order.first(), Some(&LineRole::Chord));
    assert_eq!(order[order.len() - 2], LineRole::Chord);
    assert_eq!(order.iter().filter(|r| **r == LineRole::Chord).count(), 2);
}

#[test]
fn a_line_split_at_a_bar_keeps_chords_with_their_part() {
    // "one two|three": the bar is character 7, so chord offset 9 is over the 'r' of "three".
    let slide = Slide { lines: vec!["one two|three".into()], chords: vec![vec![chord(9, "C")]], ..Slide::default() };
    let lines = body_lines(&slide, &Style::default(), HD);
    assert_eq!(roles(&lines), [LineRole::Lyric, LineRole::Chord, LineRole::Lyric]);
    assert_eq!(lines[2].text, "three");
    let glyphs = &lines[2].runs[0].glyphs;
    assert!((first_x(&lines[1]) - glyphs[1].x).abs() < 0.01);
}

#[test]
fn multilanguage_slides_have_chord_lines_above_primary_lines_only() {
    let slide = Slide {
        lines: vec!["Primary".into(), "翻译".into(), "Second".into(), "第二".into()],
        chords: vec![vec![chord(0, "G")], vec![chord(0, "X")], vec![chord(3, "D")], vec![]],
        multilanguage: true,
        ..Slide::default()
    };
    let lines = body_lines(&slide, &Style::default(), HD);
    assert_eq!(roles(&lines), [LineRole::Chord, LineRole::Lyric, LineRole::Translation, LineRole::Chord, LineRole::Lyric, LineRole::Translation]);
    assert_eq!(lines[0].text, "G");
    assert_eq!(lines[3].text, "D");
}

#[test]
fn a_slide_without_chords_lays_out_as_before() {
    let slide = Slide { lines: vec!["Amazing".into()], chords: vec![vec![chord(0, "G")]], ..Slide::default() };
    let plain = body_lines(&slide.without_chords(), &Style::default(), HD);
    assert_eq!(roles(&plain), [LineRole::Lyric]);
    let bare = Slide { lines: vec!["Amazing".into()], ..Slide::default() };
    assert_eq!(plain, body_lines(&bare, &Style::default(), HD));
}

#[test]
fn auto_scale_shrinks_chords_with_the_lyrics() {
    let mut style = Style::default();
    style.body.auto_scale = true;
    let lines: Vec<String> = (0..12).map(|i| format!("Lyric line number {i}")).collect();
    let chords = (0..12).map(|_| vec![chord(0, "G")]).collect();
    let slide = Slide { lines, chords, ..Slide::default() };
    let frame = Renderer::new().layout(&slide, &style, HD);
    let body = &frame.boxes[0];
    let lyric = body.lines.iter().find(|l| l.role == LineRole::Lyric).unwrap();
    let chord = body.lines.iter().find(|l| l.role == LineRole::Chord).unwrap();
    assert!(lyric.font_size < 72.0);
    assert_eq!(chord.font_size, (lyric.font_size * 0.6).floor());
    let last = body.lines.last().unwrap();
    assert!(last.baseline <= body.rect.y + body.rect.height + 0.01);
}

fn layout_of(harness: &EngineHarness, size: Size) -> String {
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
fn just_a_closer_walk_with_thee() {
    let mut harness = EngineHarness::with_fixtures();
    harness.send(Command::OpenSong(SongId::new("Just A Closer Walk With Thee")));
    insta::assert_snapshot!(layout_of(&harness, Size { width: 960, height: 540 }));
}

#[test]
fn a_multilanguage_section_with_chords() {
    let xml = "<song><title>Pair</title><lyrics>[V1] (L)\n.G         C\n Amazing grace, how sweet\n 奇异恩典，何等甘甜\n.Em       D\n That saved a wretch like me\n 拯救我这样的罪人</lyrics></song>";
    let mut harness = EngineHarness::with_songs(&[("Pair", xml)]);
    harness.send(Command::OpenSong(SongId::new("Pair")));
    insta::assert_snapshot!(layout_of(&harness, Size { width: 960, height: 540 }));
}
