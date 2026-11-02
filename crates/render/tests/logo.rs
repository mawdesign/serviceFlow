//! Logo mode: the imported logo drawn centred over the style background, shrink-only.

use std::sync::Arc;

use serviceflow_core::command::{Command, ImportTrigger};
use serviceflow_core::output::Logo;
use serviceflow_render::{Renderer, Rgba, Size, Style};
use serviceflow_testkit::EngineHarness;

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// A `side` x `side` PNG of one colour.
fn png(side: u32, rgba: [u8; 4]) -> Logo {
    let image = image::RgbaImage::from_pixel(side, side, image::Rgba(rgba));
    let mut bytes = Vec::new();
    image.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
    Logo { image: Arc::from(base64(&bytes)), mask: None }
}

fn style(background: Rgba) -> Style {
    let mut style = Style::default();
    style.background.color = background;
    style
}

#[test]
fn a_small_logo_is_centred_at_its_own_size() {
    let renderer = Renderer::new();
    let image = renderer.render_logo(&png(20, [255, 0, 0, 255]), &style(Rgba::rgb(0, 0, 255)), Size { width: 100, height: 60 });
    assert_eq!((image.width, image.height), (100, 60));
    assert_eq!(image.pixel(50, 30), [255, 0, 0, 255], "centre is the logo");
    assert_eq!(image.pixel(41, 21), [255, 0, 0, 255], "20 px wide: x 40..60");
    assert_eq!(image.pixel(30, 30), [0, 0, 255, 255], "not enlarged to fill the frame");
    assert_eq!(image.pixel(0, 0), [0, 0, 255, 255]);
}

#[test]
fn a_large_logo_shrinks_to_fit() {
    let renderer = Renderer::new();
    let image = renderer.render_logo(&png(400, [0, 255, 0, 255]), &style(Rgba::rgb(0, 0, 0)), Size { width: 100, height: 60 });
    // 400x400 shrunk to 60x60, centred: x 20..80.
    assert_eq!(image.pixel(50, 1), [0, 255, 0, 255]);
    assert_eq!(image.pixel(50, 58), [0, 255, 0, 255]);
    assert_eq!(image.pixel(10, 30), [0, 0, 0, 255]);
    assert_eq!(image.pixel(90, 30), [0, 0, 0, 255]);
}

#[test]
fn a_transparent_logo_shows_the_background_through() {
    let renderer = Renderer::new();
    let image = renderer.render_logo(&png(20, [255, 255, 255, 0]), &style(Rgba::rgb(10, 20, 30)), Size { width: 40, height: 40 });
    assert_eq!(image.pixel(20, 20), [10, 20, 30, 255]);
}

#[test]
fn an_undecodable_logo_leaves_just_the_background() {
    let renderer = Renderer::new();
    let logo = Logo { image: Arc::from("bm90IGFuIGltYWdl"), mask: None };
    let image = renderer.render_logo(&logo, &style(Rgba::rgb(1, 2, 3)), Size { width: 10, height: 10 });
    assert_eq!(image.pixel(5, 5), [1, 2, 3, 255]);
}

#[test]
fn the_logo_imported_from_the_fixture_settings_is_drawn() {
    let mut harness = EngineHarness::with_fixtures();
    harness.send(Command::ImportOpenSongSettings { trigger: ImportTrigger::FirstRun });
    let state = harness.state();
    let logo = state.output_settings.logo.as_ref().expect("fixture logo");
    let bg = Rgba::rgb(255, 0, 255);
    let image = Renderer::new().render_logo(logo, &style(bg), Size { width: 640, height: 360 });
    assert_eq!(image.pixel(0, 0), [255, 0, 255, 255], "corner shows the background");
    let background_pixels = image.pixels.chunks(4).filter(|p| *p == [255, 0, 255, 255]).count();
    assert!(background_pixels < image.pixels.len() / 4, "the logo covers part of the frame");
    assert!(background_pixels > 0);
}
