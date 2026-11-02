//! Drawing a laid-out frame: coarse pixel checks only (layout is tested through snapshots).

use serviceflow_core::song::Slide;
use serviceflow_render::{Renderer, Rgba, Size, Style};

#[test]
fn drawing_fills_the_background_and_paints_text_in_the_body_colour() {
    let renderer = Renderer::new();
    let mut style = Style::default();
    style.background = Rgba::rgb(0, 0, 128);
    style.body.color = Rgba::rgb(255, 255, 0);
    let slide = Slide { section: "V1".into(), lines: vec!["IIIIIIII".into()] };
    let size = Size { width: 1280, height: 720 };

    let frame = renderer.layout(&slide, &style, size);
    let image = renderer.draw(&frame);

    assert_eq!((image.width, image.height), (1280, 720));
    assert_eq!(image.pixels.len(), 1280 * 720 * 4);
    assert_eq!(image.pixel(0, 0), [0, 0, 128, 255]);
    // Some pixel on a row through the capitals, inside an "I" stem, is the text colour.
    let line = &frame.boxes[0].lines[0];
    let y = (line.baseline - line.font_size * 0.3) as u32;
    let yellow = (line.x as u32..(line.x + line.width) as u32).any(|x| image.pixel(x, y) == [255, 255, 0, 255]);
    assert!(yellow, "no text-coloured pixel on row {y}");
}
