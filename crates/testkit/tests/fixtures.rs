use serviceflow_testkit::opensong_data_root;

#[test]
fn the_opensong_fixture_data_root_holds_the_songs_and_settings() {
    let root = opensong_data_root();
    for song in [
        "Amazing Grace HOF#50",
        "God Defend NZ - National Anthem",
        "How Deep the Fathers Love + Chinese",
        "It Is Well With My Soul",
        "Just A Closer Walk With Thee",
        "一件礼物 A Gift",
    ] {
        assert!(root.join("Songs").join(song).is_file(), "missing song {song} under {}", root.display());
    }
    for settings in ["MainSettings", "PresentSettings", "PrintSettings"] {
        assert!(root.join("Settings").join(settings).is_file(), "missing {settings}");
    }
}

#[test]
fn the_fixture_folder_is_lowercase_under_tests() {
    let root = opensong_data_root();
    let rel: Vec<_> = root.components().rev().take(3).map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    assert_eq!(rel, ["opensong", "fixtures", "tests"]);
}
