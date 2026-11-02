//! Seam 2, the OpenSong codec: Settings/PresentSettings to styles.

use serviceflow_core::style::{Align, ImagePosition, Rgba, SubtitleField, VAlign};
use serviceflow_opensong::read_present_settings;
use serviceflow_testkit::opensong_data_root;

fn fixture() -> Vec<u8> {
    std::fs::read(opensong_data_root().join("Settings/PresentSettings")).expect("fixture PresentSettings")
}

#[test]
fn the_default_style_is_read_from_the_fixture() {
    let styles = read_present_settings(&fixture()).unwrap();
    let s = &styles.default;

    // OpenSong sizes are at a 640 px wide reference; ours are at 1080 high (3x for 16:9).
    assert_eq!(s.title.text.font, "Calibri");
    assert_eq!(s.title.text.size, 84.0);
    assert!(s.title.text.bold && s.title.text.underline && !s.title.text.italic);
    assert_eq!(s.title.text.valign, VAlign::Bottom);
    assert_eq!(s.title.text.align, Align::Center);
    assert!(!s.title.include_verse);

    assert_eq!(s.subtitle.text.size, 60.0);
    assert!(s.subtitle.text.italic && !s.subtitle.text.bold);
    assert_eq!(s.subtitle.source, [SubtitleField::Author]);
    assert!(!s.subtitle.descriptive);

    assert_eq!(s.body.text.size, 108.0);
    assert!(s.body.text.bold);
    assert_eq!(s.body.text.valign, VAlign::Top);
    assert!(s.body.auto_scale);
    assert!(!s.body.highlight_chorus);
    assert_eq!(s.body.multilanguage_size, 70);
    assert_eq!(s.body.multilanguage_color, Rgba::rgb(128, 128, 128));
    assert!(!s.body.blank_is_slide_change);

    // Margins include OpenSong's border of a fiftieth of the width (12.8 of 640), times 3.
    assert!((s.body.text.margins.left - 38.4).abs() < 0.01);
    assert!((s.body.text.margins.bottom - 38.4).abs() < 0.01);

    assert!(s.body.text.outline && !s.body.text.shadow && !s.body.text.fill);
    assert_eq!(s.body.text.outline_color, Rgba::rgb(252, 252, 252));
    assert_eq!(s.body.text.shadow_color, Rgba::rgb(192, 192, 192));
    // Fill colour `#FF000000`: red, and alpha `00` is opaque.
    assert_eq!(s.body.text.fill_color, Rgba::rgb(255, 0, 0));

    assert_eq!(s.background.color, Rgba::rgb(255, 255, 255));
    assert_eq!(s.background.position, ImagePosition::Stretch);
    assert_eq!(s.background.image, None);

    // `style/@thickness` is 2 at the 640 reference.
    assert_eq!(s.outline_thickness, 6.0);
}

#[test]
fn the_scripture_style_is_read_separately() {
    let styles = read_present_settings(&fixture()).unwrap();
    let s = &styles.scripture;
    assert_eq!(s.title.text.valign, VAlign::Top);
    assert_eq!(s.body.text.align, Align::Left);
    assert_eq!(s.body.text.size, 102.0);
    assert!(s.subtitle.source.is_empty());
    assert_ne!(s, &styles.default);
}

#[test]
fn colour_alpha_is_inverted() {
    let xml = r##"<present_settings><default_style><body color="#11223300" border_color="#112233FF" shadow_color="#11223380" fill_color="#112233"/></default_style></present_settings>"##;
    let body = read_present_settings(xml.as_bytes()).unwrap().default.body.text;
    assert_eq!(body.color, Rgba { r: 0x11, g: 0x22, b: 0x33, a: 255 });
    assert_eq!(body.outline_color.a, 0);
    assert_eq!(body.shadow_color.a, 127);
    assert_eq!(body.fill_color.a, 255);
}

#[test]
fn a_bad_colour_keeps_the_default() {
    let xml = r##"<present_settings><default_style><body color="red" border_color="#12345"/></default_style></present_settings>"##;
    let styles = read_present_settings(xml.as_bytes()).unwrap();
    let defaults = serviceflow_core::style::Style::default();
    assert_eq!(styles.default.body.text.color, defaults.body.text.color);
    assert_eq!(styles.default.body.text.outline_color, defaults.body.text.outline_color);
}

#[test]
fn blank_is_slide_change_is_global_and_copied_into_both_styles() {
    let xml = r#"<present_settings><style blank_is_slide_change="true"/><default_style/><scripture_style/></present_settings>"#;
    let styles = read_present_settings(xml.as_bytes()).unwrap();
    assert!(styles.default.body.blank_is_slide_change && styles.scripture.body.blank_is_slide_change);
}

#[test]
fn missing_parts_fall_back_to_defaults_and_enabled_defaults_true() {
    let xml = r#"<present_settings><default_style><title enabled="false"/><body auto_scale="false"/></default_style></present_settings>"#;
    let styles = read_present_settings(xml.as_bytes()).unwrap();
    assert!(!styles.default.title.text.enabled);
    assert!(styles.default.subtitle.text.enabled);
    assert!(!styles.default.body.auto_scale);
    assert!(styles.scripture.body.text.enabled);
}

#[test]
fn a_background_image_by_filename_or_inline() {
    use serviceflow_core::style::ImageSource;
    let xml = r#"<present_settings><default_style><background position="2" filename="x/y.jpg"/></default_style>
        <scripture_style><background>QUJD</background></scripture_style></present_settings>"#;
    let styles = read_present_settings(xml.as_bytes()).unwrap();
    assert_eq!(styles.default.background.image, Some(ImageSource::File("x/y.jpg".into())));
    assert_eq!(styles.default.background.position, ImagePosition::Center);
    assert_eq!(styles.scripture.background.image, Some(ImageSource::Embedded("QUJD".into())));
}

#[test]
fn something_that_is_not_present_settings_is_an_error() {
    assert!(read_present_settings(b"<song/>").is_err());
    assert!(read_present_settings(b"nope").is_err());
}
