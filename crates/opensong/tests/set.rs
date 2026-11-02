//! Seam 2: reading sets.

use serviceflow_core::set::{ItemKind, Set};
use serviceflow_opensong::read_set;

fn fixture(name: &str) -> Set {
    let path = serviceflow_testkit::opensong_data_root().join("Sets").join(name);
    read_set(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn opensongs_example_set_reads_every_item() {
    let set = fixture("Example");
    assert_eq!(set.name, "");
    let names: Vec<_> = set.items.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Announcements",
            "Colossians 1:11-14",
            "And Can It Be",
            "He Leadeth Me",
            "What A Friend We Have In Jesus",
            "It Is Well With My Soul",
            "Philippians 4:6-7"
        ]
    );
    let types: Vec<_> = set
        .items
        .iter()
        .map(|i| match &i.kind {
            ItemKind::Song(_) => "song".to_owned(),
            ItemKind::Placeholder { item_type, .. } => item_type.clone(),
        })
        .collect();
    assert_eq!(types, ["custom", "scripture", "song", "song", "song", "song", "scripture"]);
}

#[test]
fn song_items_carry_name_path_order_and_notes() {
    let set = fixture("Sunday Service");
    assert_eq!(set.name, "Sunday Service");
    let item = &set.items[0];
    assert_eq!(item.notes, "Start softly. Band joins at the chorus.");
    let ItemKind::Song(song) = &item.kind else { panic!("song item") };
    assert_eq!(song.presentation, ["V1", "C", "V4", "C"]);
    assert_eq!(item.song_id().unwrap().as_str(), "It Is Well With My Soul");
    let renamed = &set.items[3];
    assert_eq!(renamed.song_id().unwrap().as_str(), "Hymns/Old Hymn That Was Renamed");
    assert_eq!(renamed.notes, "Check the key.");
}

#[test]
fn scripture_and_external_items_are_kept_verbatim_as_placeholders() {
    let set = fixture("Sunday Service");
    let ItemKind::Placeholder { item_type, xml } = &set.items[2].kind else { panic!("placeholder") };
    assert_eq!(item_type, "scripture");
    assert!(xml.contains("The Lord is my shepherd."));
    assert_eq!(set.items[2].notes, "Read by Sam.");
    let ItemKind::Placeholder { item_type, xml } = &set.items[4].kind else { panic!("placeholder") };
    assert_eq!(item_type, "external");
    assert!(xml.contains("talk.pptx"));
}

#[test]
fn paths_are_cleaned_and_odd_items_do_not_stop_the_set() {
    let xml = r#"<set name="x"><slide_groups>
        <slide_group name="A" type="song" path="/Hymns//Old/"/>
        <slide_group name="B"/>
        <slide_group type="SONG"/>
        </slide_groups></set>"#;
    let set = read_set(xml.as_bytes()).unwrap();
    assert_eq!(set.items.len(), 3);
    assert_eq!(set.items[0].song_id().unwrap().as_str(), "Hymns/Old/A");
    assert!(matches!(set.items[1].kind, ItemKind::Placeholder { .. }));
    assert!(set.items[2].song_id().is_some());
}

#[test]
fn an_empty_or_wrong_document_is_handled() {
    assert!(read_set(b"<set/>").unwrap().items.is_empty());
    assert!(read_set(b"<song/>").is_err());
}
