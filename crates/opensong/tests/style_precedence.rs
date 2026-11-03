//! Seam 2: the parts of songs, sets and settings that decide which style a slide uses.

use serviceflow_core::set::{ItemKind, StyleAction};
use serviceflow_core::style::{ImageSource, Resize, Rgba};
use serviceflow_opensong::{read_output_settings, read_set, read_song};

const STYLE: &str = r##"<style><body font="Arial" size="40" color="#ff000000"/><background color="#00ff0000"/></style>"##;

#[test]
fn a_song_style_is_read() {
    let xml = format!("<song><title>T</title><lyrics>[V1]\n a</lyrics>{STYLE}</song>");
    let song = read_song(xml.as_bytes()).unwrap();
    let style = song.style.expect("song style");
    assert_eq!(style.body.text.font, "Arial");
    assert_eq!(style.body.text.color, Rgba::rgb(255, 0, 0));
    assert_eq!(style.background.color, Rgba::rgb(0, 255, 0));
    // The raw XML is still kept for saving.
    assert!(song.other_elements.iter().any(|e| e.name == "style"));
}

#[test]
fn a_song_without_a_style_has_none() {
    let song = read_song(b"<song><lyrics>[V1]\n a</lyrics></song>").unwrap();
    assert!(song.style.is_none());
    assert!(song.backgrounds.is_none());
}

#[test]
fn song_backgrounds_are_read() {
    let xml = r#"<song><lyrics>[V1]
 a</lyrics><backgrounds resize="body" keep_aspect="true" link="false" background_as_text="true">
  <background verse="v1"><filename>one.png</filename></background>
  <background verse="V1"><image>QUJD
REVG</image></background>
  <background verse="c"><filename></filename></background>
</backgrounds></song>"#;
    let song = read_song(xml.as_bytes()).unwrap();
    let bg = song.backgrounds.expect("backgrounds");
    assert_eq!(bg.resize, Resize::Body);
    assert!(bg.keep_aspect && bg.as_text && !bg.link);
    assert_eq!(bg.items.len(), 2, "an empty background is dropped");
    assert_eq!(bg.items[0].verse, "v1");
    assert_eq!(bg.items[0].image, ImageSource::File("one.png".into()));
    assert_eq!(bg.items[1].image, ImageSource::Embedded("QUJDREVG".into()));
}

#[test]
fn the_nth_background_goes_to_the_nth_slide_and_the_last_repeats() {
    let xml = r#"<song><lyrics>[V1]
 a
 ||b
 ||c</lyrics><backgrounds><background verse="V1"><filename>1.png</filename></background>
<background verse="v1"><filename>2.png</filename></background></backgrounds></song>"#;
    let song = read_song(xml.as_bytes()).unwrap();
    let bg = song.backgrounds.unwrap();
    let names: Vec<_> = (0..3).map(|n| bg.image_for("V1", n).cloned()).collect();
    assert_eq!(names, [Some(ImageSource::File("1.png".into())), Some(ImageSource::File("2.png".into())), Some(ImageSource::File("2.png".into()))]);
    assert_eq!(bg.image_for("C", 0), None);
}

#[test]
fn backgrounds_resize_defaults_to_the_screen_and_empty_means_natural_size() {
    let screen = read_song(b"<song><backgrounds/></song>").unwrap().backgrounds.unwrap();
    assert_eq!((screen.resize, screen.keep_aspect, screen.as_text), (Resize::Screen, false, false));
    let none = read_song(br#"<song><backgrounds resize=""/></song>"#).unwrap().backgrounds.unwrap();
    assert_eq!(none.resize, Resize::None);
}

#[test]
fn style_items_and_set_item_styles_are_read() {
    let xml = format!(
        r#"<set name="S"><slide_groups>
  <slide_group name="Red" type="style" action="new">{STYLE}</slide_group>
  <slide_group name="Alpha" type="song">{STYLE}</slide_group>
  <slide_group name="Back" type="style" action="revert"/>
  <slide_group name="Odd" type="style" action="other"/>
  <slide_group name="Psalm" type="scripture"/>
</slide_groups></set>"#
    );
    let set = read_set(xml.as_bytes()).unwrap();
    let actions: Vec<_> = set
        .items
        .iter()
        .map(|i| match &i.kind {
            ItemKind::Style { action, .. } => Some(*action),
            _ => None,
        })
        .collect();
    assert_eq!(actions, [Some(StyleAction::New), None, Some(StyleAction::Revert), Some(StyleAction::Other), None]);
    assert!(set.items[0].style.is_some(), "a style item carries the style it pushes");
    assert!(set.items[1].style.is_some(), "a song item can have its own style");
    assert!(set.items[2].style.is_none());
    // A style item keeps its XML for saving.
    let ItemKind::Style { xml, .. } = &set.items[0].kind else { panic!() };
    assert!(xml.contains("<body"));
}

#[test]
fn song_style_preferred_defaults_to_on() {
    let on = read_output_settings(b"<present_settings/>").unwrap();
    assert!(on.song_style_preferred);
    let off = read_output_settings(b"<present_settings><song_style_preferred>false</song_style_preferred></present_settings>").unwrap();
    assert!(!off.song_style_preferred);
    let on = read_output_settings(b"<present_settings><song_style_preferred>true</song_style_preferred></present_settings>").unwrap();
    assert!(on.song_style_preferred);
}
