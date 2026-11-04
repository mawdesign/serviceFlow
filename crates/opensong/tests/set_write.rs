//! Seam 2: writing sets. A set is written from its model; whatever the model does not describe
//! (scripture, external and custom items, unknown attributes and elements) comes back unchanged.

use serviceflow_core::set::{ItemKind, Set};
use serviceflow_opensong::{read_set, write_set};

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
