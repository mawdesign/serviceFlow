//! Background images: placement by `resize`, `keep_aspect` and `position`, text over the image,
//! and `background_as_text`.

use serviceflow_core::resolve::{ImageFile, Images};
use serviceflow_core::song::Slide;
use serviceflow_core::style::{Captions, ImageSource, Resize, Rgba, Style};
use serviceflow_render::{Fit, Renderer, Size};

const RED: [u8; 4] = [255, 0, 0, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];

/// A solid red PNG of `w` x `h`.
fn png(w: u32, h: u32) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(w, h, image::Rgba(RED));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    bytes.into_inner()
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn images(name: &str, bytes: Vec<u8>) -> Images {
    let mut images = Images::default();
    images.set(name.into(), ImageFile::Loaded(bytes.into()));
    images
}

fn style(resize: Option<Resize>, keep_aspect: bool) -> Style {
    let mut style = Style::default();
    style.background.color = Rgba::BLACK;
    style.background.image = Some(ImageSource::File("pic.png".into()));
    style.background.resize = resize;
    style.background.keep_aspect = keep_aspect;
    style
}

fn draw(style: &Style, images: &Images, slide: &Slide, size: u32) -> serviceflow_render::Image {
    Renderer::new().render_with_images(slide, &Captions::default(), style, images, Size { width: size, height: size })
}

#[test]
fn a_screen_background_is_stretched_over_the_whole_frame() {
    let image = draw(&style(Some(Resize::Screen), false), &images("pic.png", png(4, 2)), &Slide::default(), 100);
    for (x, y) in [(0, 0), (99, 0), (0, 99), (99, 99), (50, 50)] {
        assert_eq!(image.pixel(x, y), RED, "({x}, {y})");
    }
}

#[test]
fn keep_aspect_fits_inside_the_screen_and_centres() {
    // A 2:1 picture in a square: 100 x 50, centred.
    let image = draw(&style(Some(Resize::Screen), true), &images("pic.png", png(4, 2)), &Slide::default(), 100);
    assert_eq!(image.pixel(50, 50), RED);
    assert_eq!(image.pixel(5, 30), RED);
    assert_eq!(image.pixel(50, 10), BLACK);
    assert_eq!(image.pixel(50, 90), BLACK);
}

#[test]
fn a_body_background_stays_inside_the_body_area() {
    // 200 px frame: the default 54 px (of 1080) margin is 10 px, so the body starts at 10.
    let image = draw(&style(Some(Resize::Body), false), &images("pic.png", png(4, 2)), &Slide::default(), 200);
    assert_eq!(image.pixel(2, 2), BLACK);
    assert_eq!(image.pixel(197, 100), BLACK);
    assert_eq!(image.pixel(100, 100), RED);
    assert_eq!(image.pixel(12, 12), RED);
}

#[test]
fn a_natural_size_background_is_centred_and_not_scaled() {
    // 1080 px frame is the reference size: a 100 x 100 picture stays 100 x 100.
    let image = draw(&style(Some(Resize::None), false), &images("pic.png", png(100, 100)), &Slide::default(), 1080);
    assert_eq!(image.pixel(540, 540), RED);
    assert_eq!(image.pixel(495, 495), RED);
    assert_eq!(image.pixel(480, 540), BLACK);
    assert_eq!(image.pixel(10, 10), BLACK);
}

#[test]
fn a_style_image_is_placed_by_its_position() {
    use serviceflow_core::style::ImagePosition;
    let pictures = images("pic.png", png(100, 50));
    let mut cover = style(None, false);
    cover.background.position = ImagePosition::Cover;
    let frame = Renderer::new().layout_with_images(&Slide::default(), &Captions::default(), &cover, &pictures, Size { width: 200, height: 200 });
    assert_eq!(frame.image.as_ref().unwrap().fit, Fit::Cover);
    // Cover fills the frame, cropping the sides.
    let image = Renderer::new().draw(&frame);
    assert_eq!((image.pixel(0, 0), image.pixel(199, 199)), (RED, RED));

    let mut tile = style(None, false);
    tile.background.position = ImagePosition::Tile;
    let image = draw(&tile, &pictures, &Slide::default(), 1080);
    assert_eq!((image.pixel(0, 0), image.pixel(1079, 1079), image.pixel(500, 700)), (RED, RED, RED));

    let mut centre = style(None, false);
    centre.background.position = ImagePosition::Center;
    let image = draw(&centre, &pictures, &Slide::default(), 1080);
    assert_eq!((image.pixel(540, 540), image.pixel(10, 10)), (RED, BLACK));
}

#[test]
fn an_embedded_image_needs_no_file() {
    let mut style = style(Some(Resize::Screen), false);
    style.background.image = Some(ImageSource::Embedded(base64(&png(2, 2))));
    let image = draw(&style, &Images::default(), &Slide::default(), 50);
    assert_eq!(image.pixel(25, 25), RED);
}

#[test]
fn text_is_drawn_over_the_image() {
    let slide = Slide { lines: vec!["IIIIIIII".into()], ..Slide::default() };
    let image = draw(&style(Some(Resize::Screen), false), &images("pic.png", png(2, 2)), &slide, 400);
    assert_eq!(image.pixel(1, 1), RED);
    let white = (0..400).flat_map(|y| (0..400).map(move |x| (x, y))).any(|(x, y)| image.pixel(x, y) == [255, 255, 255, 255]);
    assert!(white, "the lyrics are painted over the picture");
}

#[test]
fn background_as_text_shows_the_image_without_the_lyrics() {
    let renderer = Renderer::new();
    let slide = Slide { lines: vec!["Hello".into()], ..Slide::default() };
    let pictures = images("pic.png", png(2, 2));
    let mut style = style(Some(Resize::Screen), false);

    let lines = |frame: &serviceflow_render::Frame| frame.boxes.iter().map(|b| b.lines.len()).sum::<usize>();
    let size = Size { width: 400, height: 400 };
    let shown = renderer.layout_with_images(&slide, &Captions::default(), &style, &pictures, size);
    assert_eq!(lines(&shown), 1);

    style.background.as_text = true;
    let hidden = renderer.layout_with_images(&slide, &Captions::default(), &style, &pictures, size);
    assert_eq!(lines(&hidden), 0, "the lyrics are left out");
    assert!(hidden.image.is_some());

    // Until the image is there the lyrics stay, so a missing file does not blank the slide.
    let missing = renderer.layout_with_images(&slide, &Captions::default(), &style, &Images::default(), size);
    assert_eq!(lines(&missing), 1);
    assert!(missing.image.is_none());
}

#[test]
fn an_undecodable_image_is_ignored() {
    let image = draw(&style(Some(Resize::Screen), false), &images("pic.png", b"not an image".to_vec()), &Slide::default(), 20);
    assert_eq!(image.pixel(10, 10), BLACK);
}
