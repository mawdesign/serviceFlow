//! Zip import and export of an OpenSong data root (pure functions; the browser side is in
//! `tests/browser.rs`).

use std::io::Write as _;

use serviceflow_storage_web::archive::{ArchiveError, read_archive, write_archive};
use serviceflow_testkit::opensong_data_root;

fn files(list: &[(&str, &[u8])]) -> Vec<(String, Vec<u8>)> {
    list.iter().map(|(p, b)| ((*p).to_owned(), b.to_vec())).collect()
}

/// A zip made by another tool: entries in the given order, folders as their own entries.
fn zip_of(entries: &[(&str, Option<&[u8]>)]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for (name, bytes) in entries {
        match bytes {
            Some(bytes) => {
                writer.start_file(*name, options).unwrap();
                writer.write_all(bytes).unwrap();
            }
            None => writer.add_directory(*name, options).unwrap(),
        }
    }
    writer.finish().unwrap().into_inner()
}

/// Every file of the fixture data root, as the browser storage would export it.
fn fixture_files() -> Vec<(String, Vec<u8>)> {
    fn collect(dir: &std::path::Path, prefix: &str, out: &mut Vec<(String, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = format!("{prefix}{}", entry.file_name().to_string_lossy());
            if entry.file_type().unwrap().is_dir() {
                collect(&entry.path(), &format!("{path}/"), out);
            } else {
                out.push((path, std::fs::read(entry.path()).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    collect(&opensong_data_root(), "", &mut out);
    out.sort();
    out
}

#[test]
fn an_exported_data_root_imports_back_unchanged() {
    let files = fixture_files();
    assert!(files.iter().any(|(p, _)| p.starts_with("Songs/")) && files.iter().any(|(p, _)| p.starts_with("Sets/")));
    let zip = write_archive(&files);
    assert!(zip.len() < files.iter().map(|(_, b)| b.len()).sum::<usize>(), "the export is compressed");
    assert_eq!(read_archive(&zip).unwrap(), files);
}

#[test]
fn sidecars_and_categories_survive_the_round_trip() {
    let files = files(&[
        (".serviceflow/Sets/Sunday.sidecar", b"version = 1\n"),
        ("Sets/Sunday", b"<set/>"),
        ("Songs/Hymns/Amazing Grace", b"<song/>"),
        ("Songs/Plain", b"<song/>"),
    ]);
    assert_eq!(read_archive(&write_archive(&files)).unwrap(), files);
}

#[test]
fn a_zip_of_the_data_root_folder_itself_is_unwrapped() {
    let zip = zip_of(&[
        ("OpenSong/", None),
        ("OpenSong/Songs/", None),
        ("OpenSong/Songs/Abide With Me", Some(b"<song/>")),
        ("OpenSong/Sets/Sunday", Some(b"<set/>")),
    ]);
    assert_eq!(read_archive(&zip).unwrap(), files(&[("Sets/Sunday", b"<set/>"), ("Songs/Abide With Me", b"<song/>")]));
}

#[test]
fn junk_from_zip_tools_and_paths_leaving_the_data_root_are_skipped() {
    let zip = zip_of(&[
        ("__MACOSX/Songs/._Abide With Me", Some(b"junk")),
        ("Songs/.DS_Store", Some(b"junk")),
        ("Songs\\Hymns\\Windows Path", Some(b"<song/>")),
        ("Songs/Abide With Me", Some(b"<song/>")),
        ("Thumbs.db", Some(b"junk")),
    ]);
    assert_eq!(read_archive(&zip).unwrap(), files(&[("Songs/Abide With Me", b"<song/>"), ("Songs/Hymns/Windows Path", b"<song/>")]));
}

#[test]
fn a_zip_without_a_songs_folder_is_not_a_data_root() {
    let zip = zip_of(&[("Documents/letter.txt", Some(b"hello")), ("Sets/Sunday", Some(b"<set/>"))]);
    assert_eq!(read_archive(&zip), Err(ArchiveError::NotADataRoot));
    assert_eq!(read_archive(&zip_of(&[])), Err(ArchiveError::NotADataRoot));
}

#[test]
fn a_file_that_is_not_a_zip_is_unreadable() {
    assert!(matches!(read_archive(b"<song>not a zip</song>"), Err(ArchiveError::Unreadable(_))));
}
