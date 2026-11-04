//! Zip import and export of a whole OpenSong data root, for browser storage that has no folder
//! of its own (OPFS). Pure functions over paths and bytes, so they are tested natively.
//!
//! Paths are `/`-separated and relative to the data root (`Songs/Hymns/Amazing Grace`,
//! `.serviceflow/Sets/Sunday.sidecar`).

use std::io::{Cursor, Read as _, Write as _};

use serviceflow_storage::check_path;

/// Junk that zip tools on macOS and Windows add, never part of the data.
fn is_junk(path: &str) -> bool {
    path.split('/').any(|part| part == "__MACOSX" || part == ".DS_Store" || part == "Thumbs.db" || part == "desktop.ini")
}

/// Why a zip file could not be imported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveError {
    /// Not a zip file, or a damaged one.
    Unreadable(String),
    /// A zip file without a `Songs` folder at its top or one folder down.
    NotADataRoot,
}

impl std::fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArchiveError::Unreadable(why) => write!(f, "not a readable zip file: {why}"),
            ArchiveError::NotADataRoot => f.write_str("the zip file has no Songs folder"),
        }
    }
}

impl std::error::Error for ArchiveError {}

/// Every file of an OpenSong data root packed in `zip`, as (path under the data root, bytes),
/// sorted by path.
///
/// The data root may be the top of the zip (`Songs/…`) or a single folder inside it
/// (`OpenSong/Songs/…`), as a zip of the folder itself has. Folder entries, macOS and Windows
/// junk and paths that would leave the data root are skipped.
pub fn read_archive(zip: &[u8]) -> Result<Vec<(String, Vec<u8>)>, ArchiveError> {
    let unreadable = |e: zip::result::ZipError| ArchiveError::Unreadable(e.to_string());
    let mut archive = zip::ZipArchive::new(Cursor::new(zip)).map_err(unreadable)?;
    let mut files = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(unreadable)?;
        if entry.is_dir() {
            continue;
        }
        let path = entry.name().map_err(unreadable)?.replace('\\', "/");
        if is_junk(&path) || check_path(&path).is_err() {
            continue;
        }
        let mut bytes = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
        entry.read_to_end(&mut bytes).map_err(|e| ArchiveError::Unreadable(e.to_string()))?;
        files.push((path, bytes));
    }
    let prefix = data_root_prefix(files.iter().map(|(p, _)| p.as_str())).ok_or(ArchiveError::NotADataRoot)?;
    let mut files: Vec<(String, Vec<u8>)> =
        files.into_iter().filter_map(|(path, bytes)| Some((path.strip_prefix(&prefix)?.to_owned(), bytes))).collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

/// Where the data root starts in a zip with these file paths: `""` for the top, `"X/"` for a
/// folder `X` holding `Songs/`. `None` when there is no `Songs` folder in either place.
fn data_root_prefix<'a>(paths: impl Iterator<Item = &'a str> + Clone) -> Option<String> {
    let has_songs = |prefix: &str| paths.clone().any(|p| p.strip_prefix(prefix).is_some_and(|rest| rest.starts_with("Songs/")));
    if has_songs("") {
        return Some(String::new());
    }
    let mut tops: Vec<&str> = paths.clone().filter_map(|p| p.split_once('/').map(|(top, _)| top)).collect();
    tops.sort_unstable();
    tops.dedup();
    tops.into_iter().map(|top| format!("{top}/")).find(|prefix| has_songs(prefix))
}

/// A zip of `files` (path under the data root, bytes), with the data root at the top of the zip.
/// Text compresses well, so every file is deflated.
pub fn write_archive(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut sorted: Vec<&(String, Vec<u8>)> = files.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    for (path, bytes) in sorted {
        // Writing into memory cannot fail except on a duplicate name, which a sorted list of
        // distinct paths does not have.
        writer.start_file(path.as_str(), options).expect("starting a zip entry in memory");
        writer.write_all(bytes).expect("writing a zip entry in memory");
    }
    writer.finish().expect("finishing a zip in memory").into_inner()
}
