//! The renderer's layout, inspected without drawing (ADR-0005: layout is tested through glyph
//! positions, not pixels).

use serviceflow_core::song::Slide;
use serviceflow_render::{Align, BoxRole, LineRole, Margins, Renderer, Size, Style, VAlign};

fn slide(lines: &[&str]) -> Slide {
    Slide { section: "V1".into(), lines: lines.iter().map(|l| l.to_string()).collect() }
}

const HD: Size = Size { width: 1920, height: 1080 };

#[test]
fn a_one_line_slide_lays_out_one_centred_body_line() {
    let renderer = Renderer::new();
    let style = Style::default();
    let frame = renderer.layout(&slide(&["Amazing grace"]), &style, HD);

    assert_eq!(frame.size, HD);
    assert_eq!(frame.boxes.len(), 1);
    let body = &frame.boxes[0];
    assert_eq!(body.role, BoxRole::Body);
    assert_eq!(body.lines.len(), 1);
    let line = &body.lines[0];
    assert_eq!(line.role, LineRole::Lyric);
    assert_eq!(line.text, "Amazing grace");
    // "Amazing grace" shapes to one glyph per character.
    assert_eq!(line.glyph_count(), 13);
    let centre = line.x + line.width / 2.0;
    assert!((centre - 960.0).abs() < 0.5, "line centre {centre}");
    assert!(line.width > 100.0 && line.width < 1920.0);
}

fn texts(frame: &serviceflow_render::Frame) -> Vec<&str> {
    frame.boxes[0].lines.iter().map(|l| l.text.as_str()).collect()
}

#[test]
fn a_line_too_wide_for_the_box_wraps_at_spaces_and_stays_inside_the_margins() {
    let renderer = Renderer::new();
    let style = Style::default();
    let long = "When peace like a river attendeth my way, when sorrows like sea billows roll";
    let frame = renderer.layout(&slide(&[long]), &style, HD);

    let body = &frame.boxes[0];
    assert!(body.lines.len() >= 2, "{:?}", texts(&frame));
    assert_eq!(texts(&frame).join(" "), long);
    for line in &body.lines {
        assert!(line.x >= body.rect.x - 0.01 && line.x + line.width <= body.rect.x + body.rect.width + 0.01, "{line:?}");
    }
}

#[test]
fn slide_lines_and_bars_start_new_lines_one_pitch_apart() {
    let renderer = Renderer::new();
    let style = Style::default();
    let frame = renderer.layout(&slide(&["It is well", "with my|soul"]), &style, HD);

    assert_eq!(texts(&frame), ["It is well", "with my", "soul"]);
    let lines = &frame.boxes[0].lines;
    // 72 px at 1080 high, baselines 1.2 em apart.
    for pair in lines.windows(2) {
        assert!((pair[1].baseline - pair[0].baseline - 86.4).abs() < 0.01);
    }
    // The block is centred vertically: the middle line's baseline sits near mid-frame.
    assert!((lines[1].baseline - 540.0).abs() < 40.0, "{}", lines[1].baseline);
}

#[test]
fn margins_and_alignment_place_lines_against_the_box_edges() {
    let renderer = Renderer::new();
    let mut style = Style::default();
    style.body.text.margins = Margins { left: 100.0, top: 50.0, right: 200.0, bottom: 50.0 };
    style.body.text.valign = VAlign::Top;

    style.body.text.align = Align::Left;
    let left = renderer.layout(&slide(&["Grace"]), &style, HD);
    style.body.text.align = Align::Right;
    let right = renderer.layout(&slide(&["Grace"]), &style, HD);

    let (l, r) = (&left.boxes[0].lines[0], &right.boxes[0].lines[0]);
    assert_eq!(l.x, 100.0);
    assert!((r.x + r.width - 1720.0).abs() < 0.01);
    // Top aligned: the first line's ascent starts at the top margin.
    assert!(l.baseline > 50.0 && l.baseline < 50.0 + 86.4);
}

#[test]
fn a_thumbnail_lays_out_like_the_full_frame_scaled_down() {
    let renderer = Renderer::new();
    let style = Style::default();
    let long = "When peace like a river attendeth my way, when sorrows like sea billows roll";
    let full = renderer.layout(&slide(&[long]), &style, HD);
    let thumb = renderer.layout(&slide(&[long]), &style, Size { width: 192, height: 108 });

    assert_eq!(texts(&full), texts(&thumb));
    let (f, t) = (&full.boxes[0].lines[0], &thumb.boxes[0].lines[0]);
    assert!((t.font_size * 10.0 - f.font_size).abs() < 0.01);
    assert!((t.x * 10.0 - f.x).abs() < 1.0);
}

#[test]
fn right_to_left_text_is_shaped_in_visual_order() {
    let renderer = Renderer::new();
    let frame = renderer.layout(&slide(&["abc אבג"]), &Style::default(), HD);

    let runs = &frame.boxes[0].lines[0].runs;
    assert!(runs.iter().any(|r| r.rtl), "{runs:?}");
    // Glyph origins increase left to right across the whole line.
    let xs: Vec<f32> = runs.iter().flat_map(|r| r.glyphs.iter().map(|g| g.x)).collect();
    assert!(xs.windows(2).all(|w| w[0] <= w[1]), "{xs:?}");
}
