//! The renderer's layout, inspected without drawing (ADR-0005: layout is tested through glyph
//! positions, not pixels).

use serviceflow_core::song::Slide;
use serviceflow_render::{Align, BoxRole, LineRole, Renderer, Size, Style};

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
