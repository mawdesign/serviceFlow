//! Seam 2: the sidecar codec.

use serviceflow_core::ids::SongId;
use serviceflow_core::sidecar::{ItemKey, ItemSettings, SetSidecar};
use serviceflow_core::song::LanguageMode;
use serviceflow_sidecar::{read_set_sidecar, write_set_sidecar};

fn key(song: &str, occurrence: usize) -> ItemKey {
    ItemKey { song: SongId::new(song), occurrence }
}

#[test]
fn a_sidecar_round_trips_and_writing_is_stable() {
    let mut sidecar = SetSidecar::default();
    sidecar.set(key("Hymns/Amazing Grace", 0), ItemSettings { language: LanguageMode::Primary, transpose: -2, capo: Some(3) });
    sidecar.set(key("Hymns/Amazing Grace", 1), ItemSettings { language: LanguageMode::Translation, ..ItemSettings::default() });
    sidecar.set(key("一件礼物 A Gift", 0), ItemSettings { transpose: 5, ..ItemSettings::default() });
    let bytes = write_set_sidecar(&sidecar);
    assert_eq!(read_set_sidecar(&bytes), Ok(sidecar.clone()));
    assert_eq!(write_set_sidecar(&read_set_sidecar(&bytes).unwrap()), bytes);
}

#[test]
fn the_written_file_is_plain_readable_toml() {
    let mut sidecar = SetSidecar::default();
    sidecar.set(key("Alpha", 0), ItemSettings { language: LanguageMode::Primary, ..ItemSettings::default() });
    assert_text(&write_set_sidecar(&sidecar), "version = 1\n\n[[item]]\nsong = \"Alpha\"\nlanguage = \"primary\"\n");
}

fn assert_text(bytes: &[u8], expected: &str) {
    assert_eq!(String::from_utf8_lossy(bytes), expected);
}

#[test]
fn an_empty_sidecar_has_no_items() {
    let bytes = write_set_sidecar(&SetSidecar::default());
    assert_eq!(String::from_utf8_lossy(&bytes), "version = 1\n");
    assert_eq!(read_set_sidecar(&bytes), Ok(SetSidecar::default()));
}

#[test]
fn unknown_keys_and_newer_versions_are_tolerated() {
    let text = "version = 7\nfuture = true\n[[item]]\nsong = \"A\"\nlanguage = \"primary\"\nmood = \"calm\"\n";
    let sidecar = read_set_sidecar(text.as_bytes()).unwrap();
    assert_eq!(sidecar.get(&key("A", 0)).language, LanguageMode::Primary);
}

#[test]
fn damaged_files_are_errors() {
    for bad in [
        "this is not toml [",
        "version = \"one\"",
        "version = 1\n[[item]]\nsong = 4",
        "version = 1\n[[item]]\nsong = \"A\"\nlanguage = \"klingon\"",
        "\u{0}",
    ] {
        assert!(read_set_sidecar(bad.as_bytes()).is_err(), "{bad:?}");
    }
    assert!(read_set_sidecar(&[0xff, 0xfe, 0xfd]).is_err());
}
