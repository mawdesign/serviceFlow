//! The style sidecar codec (#30): the edited default style survives a write and a read exactly,
//! the extras OpenSong has no place for included.

use serviceflow_core::style::{Align, ImagePosition, ImageSource, Rgba, Style, SubtitleField, VAlign};
use serviceflow_sidecar::{read_styles_sidecar, write_styles_sidecar};

fn busy() -> Style {
    let mut s = Style::default();
    s.title.text.font = "Times New Roman".into();
    s.title.text.color = Rgba { r: 1, g: 2, b: 3, a: 77 };
    s.title.include_verse = true;
    s.subtitle.text.align = Align::Right;
    s.subtitle.text.valign = VAlign::Top;
    s.subtitle.descriptive = true;
    s.subtitle.source = vec![SubtitleField::Ccli, SubtitleField::Author];
    s.body.text.size = 39.99;
    s.body.text.margins.left = 51.2;
    s.body.text.outline = true;
    s.body.auto_scale = true;
    s.body.multilanguage_size = 65;
    s.background.color = Rgba::rgb(9, 8, 7);
    s.background.image = Some(ImageSource::File("Sub/red.png".into()));
    s.background.position = ImagePosition::Tile;
    s.pair_gap = 0.35;
    s.outline_thickness = 6.0;
    s.chord_size = 55;
    s
}

#[test]
fn a_style_reads_back_as_written_and_writing_is_stable() {
    let style = busy();
    let bytes = write_styles_sidecar(Some(&style));
    assert_eq!(read_styles_sidecar(&bytes).unwrap(), Some(style.clone()));
    assert_eq!(write_styles_sidecar(Some(&style)), bytes);
}

#[test]
fn an_embedded_image_is_kept() {
    let mut style = Style::default();
    style.background.image = Some(ImageSource::Embedded("AAECAw==".into()));
    assert_eq!(read_styles_sidecar(&write_styles_sidecar(Some(&style))).unwrap(), Some(style));
}

#[test]
fn no_edit_is_written_as_no_style() {
    assert_eq!(read_styles_sidecar(&write_styles_sidecar(None)).unwrap(), None);
}

#[test]
fn damage_is_an_error_not_a_style() {
    assert!(read_styles_sidecar(b"version = [").is_err());
    assert!(read_styles_sidecar(&[0xff, 0xfe]).is_err());
    let bad = String::from_utf8(write_styles_sidecar(Some(&Style::default()))).unwrap().replace("#FFFFFFFF", "white");
    assert!(read_styles_sidecar(bad.as_bytes()).is_err());
}
