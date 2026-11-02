//! Seam 2, the OpenSong codec: output behaviour in PresentSettings and slide colours in
//! MainSettings.

use serviceflow_core::output::{InitialMode, SlideKind};
use serviceflow_core::style::Rgba;
use serviceflow_opensong::{read_output_settings, read_slide_colors};
use serviceflow_testkit::opensong_data_root;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(opensong_data_root().join("Settings").join(name)).expect("fixture settings")
}

#[test]
fn the_fixture_output_settings_are_read() {
    let s = read_output_settings(&fixture("PresentSettings")).unwrap();
    assert_eq!(s.initial_mode, InitialMode::Logo);
    // `monitors/@present` is 2, 1-based.
    assert_eq!(s.monitor, Some(1));
    assert!(s.blanks && !s.blank_uses_next);
    assert!(!s.transition);
    assert_eq!(s.transition_ms, 100);
    let logo = s.logo.expect("the embedded logo");
    assert!(logo.image.starts_with("/9j/"), "a base64 JPEG");
    assert!(!logo.image.contains(char::is_whitespace));
    assert_eq!(logo.mask, None, "the fixture's mask is empty");
}

#[test]
fn initial_modes_map_to_serviceflow_modes() {
    let mode = |m: &str| {
        let xml = format!(r#"<present_settings><style initial_mode="{m}"/></present_settings>"#);
        read_output_settings(xml.as_bytes()).unwrap().initial_mode
    };
    assert_eq!(mode("N"), InitialMode::Normal);
    assert_eq!(mode("B"), InitialMode::Black);
    assert_eq!(mode("H"), InitialMode::Blank);
    assert_eq!(mode("L"), InitialMode::Logo);
    // White and frozen have no Phase 1 equivalent; anything unknown is normal.
    assert_eq!(mode("W"), InitialMode::Normal);
    assert_eq!(mode("F"), InitialMode::Normal);
    assert_eq!(mode("?"), InitialMode::Normal);
}

#[test]
fn missing_output_settings_keep_the_defaults() {
    let s = read_output_settings(b"<present_settings/>").unwrap();
    assert_eq!(s, Default::default());
    assert_eq!(s.transition_ms, 100);
}

#[test]
fn transition_time_zero_means_a_tenth_of_a_second() {
    let xml = r#"<present_settings><style transition="true" transition_time="0"/><monitors present="0"/></present_settings>"#;
    let s = read_output_settings(xml.as_bytes()).unwrap();
    assert!(s.transition);
    assert_eq!(s.transition_ms, 100);
    assert_eq!(s.monitor, None, "monitor 0 is not a screen number");
    let xml = r#"<present_settings><style transition="true" transition_time="400"/></present_settings>"#;
    assert_eq!(read_output_settings(xml.as_bytes()).unwrap().transition_ms, 400);
}

#[test]
fn a_non_settings_document_is_an_error() {
    assert!(read_output_settings(b"<song/>").is_err());
    assert!(read_slide_colors(b"<song/>").is_err());
}

#[test]
fn the_fixture_slide_colours_are_read() {
    let c = read_slide_colors(&fixture("MainSettings")).unwrap();
    assert!(c.enabled);
    assert_eq!(c.color(SlideKind::Verse), Some(Rgba::rgb(0xEA, 0xEA, 0xFF)));
    assert_eq!(c.color(SlideKind::Bridge), Some(Rgba::rgb(0x8A, 0x8A, 0xFF)));
    assert_eq!(c.color(SlideKind::PreChorus), Some(Rgba::rgb(0x71, 0x71, 0xFF)));
    assert_eq!(c.color(SlideKind::Chorus), Some(Rgba::rgb(0xBC, 0xBC, 0xFF)));
    assert_eq!(c.color(SlideKind::Tag), Some(Rgba::rgb(0xA3, 0xA3, 0xFF)));
    assert_eq!(c.color(SlideKind::Custom), Some(Rgba::rgb(0xFF, 0xE3, 0xD5)));
    assert_eq!(c.color(SlideKind::Image), Some(Rgba::rgb(0xFF, 0xFF, 0xB4)));
    assert_eq!(c.color(SlideKind::Blank), Some(Rgba::rgb(0xFF, 0xFF, 0xFF)));
}

#[test]
fn missing_slide_colours_use_opensongs_defaults_and_can_be_disabled() {
    let xml = r##"<main_settings><slide_style_color enabled="false"><chorus color="#112233FF"/></slide_style_color></main_settings>"##;
    let c = read_slide_colors(xml.as_bytes()).unwrap();
    assert!(!c.enabled);
    assert_eq!(c.color(SlideKind::Verse), None, "colour-coding is off");
    assert_eq!(c.chorus, Rgba { r: 0x11, g: 0x22, b: 0x33, a: 0 }, "inverted alpha");
    assert_eq!(c.verse, Rgba::rgb(213, 213, 255), "default");
    assert_eq!(read_slide_colors(b"<main_settings/>").unwrap(), Default::default());
}
