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
            ItemKind::Style { .. } => "style".to_owned(),
            ItemKind::Custom(_) => "custom".to_owned(),
            ItemKind::Image(_) => "image".to_owned(),
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

// Custom and image items

#[test]
fn a_custom_item_reads_its_title_subtitle_and_slides() {
    let set = fixture("Notices and Pictures");
    let item = &set.items[0];
    assert_eq!(item.name, "Welcome");
    assert_eq!(item.notes, "Read slowly.");
    let ItemKind::Custom(custom) = &item.kind else { panic!("custom item") };
    assert_eq!(custom.title, "Welcome");
    assert_eq!(custom.subtitle, "Sunday 4 November");
    assert_eq!(custom.slides, ["Good morning\nand welcome", "-= Notices =-\nMorning tea after the service"]);
    assert!(custom.xml.contains("seconds=\"10\""), "attributes that are not modelled stay in the kept XML");
}

#[test]
fn the_example_sets_announcements_are_custom_slides() {
    let set = fixture("Example");
    let ItemKind::Custom(custom) = &set.items[0].kind else { panic!("custom item") };
    assert_eq!(custom.subtitle, "For April 26th, 2004");
    assert_eq!(custom.slides.len(), 3);
    assert_eq!(custom.slides[0], "-= Monday =-\n12-1 PM: Prayer Meeting");
}

#[test]
fn an_image_item_reads_its_pictures_and_fit() {
    use serviceflow_core::style::{ImageSource, Resize};
    let set = fixture("Notices and Pictures");
    let ItemKind::Image(image) = &set.items[1].kind else { panic!("image item") };
    assert_eq!(image.resize, Resize::Screen);
    assert!(image.keep_aspect);
    let files: Vec<_> = image.slides.iter().map(|s| s.image.clone()).collect();
    assert_eq!(files, [Some(ImageSource::File("red.png".into())), Some(ImageSource::File("gone.jpg".into()))]);
    assert_eq!(image.slides[0].description, "A red picture");
}

#[test]
fn image_variants_embedded_pictures_empty_resize_and_windows_paths() {
    use serviceflow_core::style::{ImageSource, Resize};
    let xml = r#"<set><slide_groups>
      <slide_group name="a" type="image" resize="" keep_aspect="false"><slides>
        <slide><image>AAAA
        BBBB</image></slide>
        <slide><filename>Folder\pic.png</filename></slide>
        <slide><description>nothing</description></slide>
      </slides></slide_group>
      <slide_group name="b" type="image" resize="body"/></slide_groups></set>"#;
    let set = serviceflow_opensong::read_set(xml.as_bytes()).unwrap();
    let ItemKind::Image(a) = &set.items[0].kind else { panic!("image") };
    assert_eq!(a.resize, Resize::None);
    assert!(!a.keep_aspect);
    assert_eq!(a.slides[0].image, Some(ImageSource::Embedded("AAAABBBB".into())));
    assert_eq!(a.slides[1].image, Some(ImageSource::File("Folder/pic.png".into())));
    assert_eq!(a.slides[2].image, None);
    let ItemKind::Image(b) = &set.items[1].kind else { panic!("image") };
    assert_eq!(b.resize, Resize::Body);
    assert!(b.slides.is_empty());
}
