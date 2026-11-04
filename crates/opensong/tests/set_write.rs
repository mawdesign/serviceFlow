//! Seam 2: writing sets. A set is written from its model; whatever the model does not describe
//! (scripture, external and custom items, unknown attributes and elements) comes back unchanged.

use std::sync::Arc;

use serviceflow_core::set::{CustomItem, ImageItem, ImageSlide, ItemKind, Set, SetItem, StyleAction};
use serviceflow_core::style::{ImageSource, Resize};
use serviceflow_opensong::{read_present_settings, read_set, write_set};

fn fixture_bytes(name: &str) -> Vec<u8> {
    std::fs::read(serviceflow_testkit::opensong_data_root().join("Sets").join(name)).unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[test]
fn every_fixture_set_reads_back_as_written_and_writing_is_byte_stable() {
    for name in ["Example", "Sunday Service"] {
        let set = read_set(&fixture_bytes(name)).unwrap();
        let written = write_set(&set);
        assert_eq!(read_set(&written).unwrap(), set, "{name}");
        assert_eq!(write_set(&read_set(&written).unwrap()), written, "{name}");
    }
}

#[test]
fn a_set_is_written_with_crlf_two_space_indents_and_items_kept_verbatim() {
    let set = read_set(&fixture_bytes("Sunday Service")).unwrap();
    let expected = [
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        r#"<set name="Sunday Service">"#,
        r#"  <slide_groups>"#,
        r#"    <slide_group name="It Is Well With My Soul" type="song" path="" presentation="V1 C V4 C">"#,
        r#"      <notes>Start softly. Band joins at the chorus.</notes>"#,
        r#"    </slide_group>"#,
        r#"    <slide_group name="Amazing Grace HOF#50" type="song" path=""/>"#,
        r#"    <slide_group name="Psalm 23" type="scripture">"#,
        r#"      <title>Psalm 23</title>"#,
        r#"      <subtitle>NIV</subtitle>"#,
        r#"      <slides><slide><body>The Lord is my shepherd.</body></slide></slides>"#,
        r#"      <notes>Read by Sam.</notes>"#,
        r#"    </slide_group>"#,
        r#"    <slide_group name="Old Hymn That Was Renamed" type="song" path="Hymns/">"#,
        r#"      <notes>Check the key.</notes>"#,
        r#"    </slide_group>"#,
        r#"    <slide_group name="Slides" type="external" application="presentation" filename="talk.pptx"/>"#,
        r#"  </slide_groups>"#,
        r#"</set>"#,
        "",
    ]
    .join("\r\n");
    assert_eq!(text(&write_set(&set)), expected);
}

#[test]
fn content_the_model_does_not_describe_survives_a_write() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<set name="Easter" version="2"><future>kept</future>
  <slide_groups>
    <slide_group name="Abide" type="song" path="Hymns" presentation="V1 C" transition="1" seconds="5">
      <style><title font="Arial"/></style>
      <notes>Line one
line two &amp; more</notes>
      <unknown a="b">x</unknown>
    </slide_group>
    <slide_group name="Welcome" type="custom" seconds="10"><title>Hi</title><slides><slide><body>One</body></slide></slides><notes/></slide_group>
    <slide_group name="Blue" type="style" action="new"><style><body font="Arial"/></style></slide_group>
  </slide_groups>
</set>"#;
    let set = read_set(xml.as_bytes()).unwrap();
    let written = write_set(&set);
    let out = text(&written);
    assert_eq!(read_set(&written).unwrap(), set);
    for kept in [
        r#"<set name="Easter" version="2">"#,
        "<future>kept</future>",
        r#"<slide_group name="Abide" type="song" path="Hymns/" presentation="V1 C" transition="1" seconds="5">"#,
        r#"<style><title font="Arial"/></style>"#,
        "<notes>Line one\r\nline two &amp; more</notes>",
        r#"<unknown a="b">x</unknown>"#,
        r#"<slide_group name="Welcome" type="custom" seconds="10"><title>Hi</title><slides><slide><body>One</body></slide></slides><notes/></slide_group>"#,
        r#"<slide_group name="Blue" type="style" action="new"><style><body font="Arial"/></style></slide_group>"#,
    ] {
        assert!(out.contains(kept), "missing {kept:?} in\n{out}");
    }
}

#[test]
fn a_changed_song_item_is_written_with_its_new_name_path_and_order() {
    let mut set: Set = read_set(&fixture_bytes("Sunday Service")).unwrap();
    let item = &mut set.items[1];
    item.name = "Amazing Grace.xml".into();
    item.notes = "New note".into();
    let ItemKind::Song(song) = &mut item.kind else { panic!("song item") };
    song.path = "Hymns/Old".into();
    song.presentation = vec!["V1".into(), "V2".into()];
    let out = text(&write_set(&set));
    assert!(
        out.contains(
            "    <slide_group name=\"Amazing Grace.xml\" type=\"song\" path=\"Hymns/Old/\" presentation=\"V1 V2\">\r\n      <notes>New note</notes>\r\n    </slide_group>\r\n"
        ),
        "{out}"
    );
}

#[test]
fn a_set_without_a_name_or_items_is_still_a_set() {
    let set = Set::default();
    let written = write_set(&set);
    assert_eq!(text(&written), "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<set>\r\n  <slide_groups/>\r\n</set>\r\n");
    assert_eq!(read_set(&written).unwrap(), set);
}

// Edited sets (#22): items the model describes are written from the model; what it does not
// describe stays.

fn custom_fields(set: &Set, index: usize) -> (String, String, String, Vec<String>, String) {
    let item = &set.items[index];
    let ItemKind::Custom(custom) = &item.kind else { panic!("custom item") };
    (item.name.clone(), custom.title.clone(), custom.subtitle.clone(), custom.slides.clone(), item.notes.clone())
}

fn fixture_styles() -> serviceflow_core::style::Styles {
    read_present_settings(&std::fs::read(serviceflow_testkit::opensong_data_root().join("Settings/PresentSettings")).unwrap()).unwrap()
}

#[test]
fn an_edited_custom_item_is_written_from_its_fields_and_keeps_what_the_model_does_not_describe() {
    let mut set = read_set(&fixture_bytes("Notices and Pictures")).unwrap();
    let item = &mut set.items[0];
    item.name = "Hello".into();
    item.notes = "Smile".into();
    let ItemKind::Custom(custom) = &mut item.kind else { panic!("custom item") };
    custom.title = "Hello & welcome".into();
    custom.subtitle = String::new();
    custom.slides = vec!["One\nTwo".into(), "Three".into(), "Four".into()];
    let written = write_set(&set);
    let out = text(&written);
    assert!(out.contains(r#"<slide_group name="Hello" type="custom" seconds="10" loop="true">"#), "{out}");
    assert!(out.contains("<title>Hello &amp; welcome</title>"), "{out}");
    let again = read_set(&written).unwrap();
    assert_eq!(
        custom_fields(&again, 0),
        ("Hello".into(), "Hello & welcome".into(), String::new(), vec!["One\nTwo".into(), "Three".into(), "Four".into()], "Smile".into())
    );
    // The other items are untouched, byte for byte.
    assert!(out.contains(r#"<slide_group name="Pictures" type="image" resize="screen" keep_aspect="true">"#), "{out}");
    assert_eq!(again.items[1..], read_set(&fixture_bytes("Notices and Pictures")).unwrap().items[1..]);
    assert_eq!(write_set(&again), written, "byte-stable once re-read");
}

#[test]
fn an_edited_image_item_is_written_from_its_fields() {
    let mut set = read_set(&fixture_bytes("Notices and Pictures")).unwrap();
    let ItemKind::Image(image) = &mut set.items[1].kind else { panic!("image item") };
    image.slides[1].image = Some(ImageSource::File("Other/blue.png".into()));
    image.slides.remove(0);
    let written = write_set(&set);
    let again = read_set(&written).unwrap();
    let ItemKind::Image(image) = &again.items[1].kind else { panic!("image item") };
    assert_eq!(image.slides.len(), 1);
    assert_eq!(image.slides[0].image, Some(ImageSource::File("Other/blue.png".into())));
    assert!(image.keep_aspect);
    assert_eq!(image.resize, Resize::Screen);
    assert_eq!(write_set(&again), written);
}

#[test]
fn new_custom_image_and_style_items_are_written_as_opensong_reads_them() {
    let pictures = vec![
        ImageSlide { image: Some(ImageSource::File("red.png".into())), description: String::new() },
        ImageSlide { image: Some(ImageSource::File("Sub/b.jpg".into())), description: "B".into() },
    ];
    let item = |name: &str, kind| SetItem { name: name.into(), notes: String::new(), style: None, kind };
    let mut set = Set { name: "New".into(), ..Set::default() };
    set.items.push(item("Notices", ItemKind::Custom(CustomItem { title: "Notices".into(), slides: vec!["Tea".into()], ..CustomItem::default() })));
    set.items.push(SetItem { notes: "Two pictures".into(), ..item("Pictures", ItemKind::Image(ImageItem { slides: pictures.clone(), ..ImageItem::default() })) });
    set.items.push(SetItem {
        style: Some(Arc::new(fixture_styles().default)),
        ..item("Blue", ItemKind::Style { action: StyleAction::New, xml: String::new() })
    });
    set.items.push(item("Back", ItemKind::Style { action: StyleAction::Revert, xml: String::new() }));
    let written = write_set(&set);
    let out = text(&written);
    assert!(out.contains(r#"<slide_group name="Back" type="style" action="revert"/>"#), "{out}");
    let again = read_set(&written).unwrap();
    assert_eq!(custom_fields(&again, 0), ("Notices".into(), "Notices".into(), String::new(), vec!["Tea".into()], String::new()));
    let ItemKind::Image(image) = &again.items[1].kind else { panic!("image item") };
    assert_eq!(image.slides, pictures);
    assert_eq!(again.items[1].notes, "Two pictures");
    assert!(matches!(again.items[2].kind, ItemKind::Style { action: StyleAction::New, .. }));
    assert!(again.items[2].style.is_some());
    assert!(matches!(again.items[3].kind, ItemKind::Style { action: StyleAction::Revert, .. }));
    assert!(again.items[3].style.is_none());
    assert_eq!(write_set(&again), written, "byte-stable once re-read");
}

#[test]
fn a_pushed_style_is_written_in_opensong_units_and_reads_back_the_same() {
    let pushed = fixture_styles().default;
    let set = Set {
        items: vec![SetItem {
            name: "Style".into(),
            notes: String::new(),
            style: Some(Arc::new(pushed.clone())),
            kind: ItemKind::Style { action: StyleAction::New, xml: String::new() },
        }],
        ..Set::default()
    };
    let out = text(&write_set(&set));
    // Inverted alpha, upper-case hex, as OpenSong writes.
    let c = pushed.body.text.color;
    assert!(out.contains(&format!("color=\"#{:02X}{:02X}{:02X}{:02X}\"", c.r, c.g, c.b, 255 - c.a)), "{out}");
    let read = read_set(out.as_bytes()).unwrap().items[0].style.clone().expect("a style");
    let close = |a: f32, b: f32| (a - b).abs() < 0.05;
    for (a, b) in [(&read.body.text, &pushed.body.text), (&read.title.text, &pushed.title.text), (&read.subtitle.text, &pushed.subtitle.text)] {
        assert_eq!((&a.font, a.color, a.align, a.valign, a.bold, a.outline, a.shadow), (&b.font, b.color, b.align, b.valign, b.bold, b.outline, b.shadow));
        assert!(close(a.size, b.size), "{} vs {}", a.size, b.size);
        for (x, y) in [(a.margins.left, b.margins.left), (a.margins.top, b.margins.top), (a.margins.right, b.margins.right), (a.margins.bottom, b.margins.bottom)] {
            assert!(close(x, y), "{x} vs {y}");
        }
    }
    assert_eq!(read.background.color, pushed.background.color);
    assert_eq!(read.background.image, pushed.background.image);
    assert_eq!(read.subtitle.source, pushed.subtitle.source);
    assert_eq!((read.body.multilanguage_size, read.body.multilanguage_color), (pushed.body.multilanguage_size, pushed.body.multilanguage_color));
}

#[test]
fn notes_of_a_kept_item_are_changed_in_place() {
    let mut set = read_set(&fixture_bytes("Sunday Service")).unwrap();
    set.items[2].notes = "Read by Alex.".into();
    let out = text(&write_set(&set));
    assert!(out.contains(r#"<slide_group name="Psalm 23" type="scripture">"#) && out.contains("<notes>Read by Alex.</notes>"), "{out}");
    assert!(out.contains("<slides><slide><body>The Lord is my shepherd.</body></slide></slides>"), "{out}");
    assert_eq!(read_set(out.as_bytes()).unwrap().items[2].notes, "Read by Alex.");
}

#[test]
fn reordered_and_removed_items_are_written_in_their_new_order() {
    let mut set = read_set(&fixture_bytes("Sunday Service")).unwrap();
    let first = set.items.remove(0);
    set.items.push(first);
    set.items.remove(1);
    let again = read_set(&write_set(&set)).unwrap();
    let names: Vec<&str> = again.items.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["Amazing Grace HOF#50", "Old Hymn That Was Renamed", "Slides", "It Is Well With My Soul"]);
    assert_eq!(again, set);
}
