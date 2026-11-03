//! Song files (opensong.md §2–§6): metadata, the lyrics line grammar, numbered verse blocks and
//! repeated tags. Unmodelled elements are kept as they were read.

use std::sync::Arc;

use serviceflow_core::lyrics;
use serviceflow_core::song::{RawElement, Song, SongBackgrounds, VerseBackground};
use serviceflow_core::style::{ImageSource, Resize};

use crate::CodecError;
use crate::xml::{self, Node};

/// Elements read into [`Song`] fields. Everything else ends up in [`Song::other_elements`].
const MODELLED: &[&str] = &[
    "title",
    "author",
    "copyright",
    "hymn_number",
    "presentation",
    "ccli",
    "capo",
    "key",
    "aka",
    "key_line",
    "user1",
    "user2",
    "user3",
    "theme",
    "tempo",
    "time_sig",
    "timesig",
    "lyrics",
];

/// Read an OpenSong song file. Elements may come in any order and may be missing.
pub fn read_song(bytes: &[u8]) -> Result<Song, CodecError> {
    let root = xml::parse(bytes)?;
    if root.name != "song" {
        return Err(CodecError(format!("not a song: root element is <{}>", root.name)));
    }
    let text = |name| root.child_text(name);
    let time_sig = match root.child("time_sig") {
        Some(e) => e.text(),
        // Legacy name, read by OpenSong and dropped on its save.
        None => text("timesig"),
    };
    let other_elements = root
        .children
        .iter()
        .filter_map(|n| match n {
            Node::Element(e) if !MODELLED.contains(&e.name.as_str()) => Some(RawElement { name: e.name.clone(), xml: e.to_xml() }),
            _ => None,
        })
        .collect();
    let lyrics_text = text("lyrics");
    let (sections, warnings) = lyrics::parse(&lyrics_text);
    Ok(Song {
        title: text("title"),
        author: text("author"),
        copyright: text("copyright"),
        ccli: text("ccli"),
        hymn_number: text("hymn_number"),
        theme: text("theme"),
        aka: text("aka"),
        key: text("key"),
        capo: text("capo"),
        key_line: text("key_line"),
        tempo: text("tempo"),
        time_sig,
        user1: text("user1"),
        user2: text("user2"),
        user3: text("user3"),
        // ASCII only: a no-break space is part of a token (a tag with a space in it).
        presentation: text("presentation").split_ascii_whitespace().map(str::to_owned).collect(),
        lyrics: lyrics_text,
        sections,
        style: root.child("style").map(|e| Arc::new(crate::settings::style(e))),
        backgrounds: root.child("backgrounds").map(read_backgrounds),
        other_elements,
        warnings,
    })
}

/// `<backgrounds resize keep_aspect link background_as_text>` with one `<background verse>` per
/// image, which is `<filename>` (under `Backgrounds/`) or `<image>` (base64).
fn read_backgrounds(e: &xml::Element) -> SongBackgrounds {
    let flag = |name| crate::settings::flag(e, name, false);
    let resize = match e.attr("resize").map(|r| r.trim().to_ascii_lowercase()).as_deref() {
        None | Some("screen") => Resize::Screen,
        Some("body") => Resize::Body,
        Some(_) => Resize::None,
    };
    let items = e
        .children
        .iter()
        .filter_map(|n| match n {
            Node::Element(b) if b.name == "background" => {
                let file = b.child_text("filename");
                let data: String = b.child_text("image").split_whitespace().collect();
                let image = match (file.trim(), data.is_empty()) {
                    (name, _) if !name.is_empty() => ImageSource::File(name.to_owned()),
                    (_, false) => ImageSource::Embedded(data),
                    _ => return None,
                };
                Some(VerseBackground { verse: b.attr("verse").unwrap_or_default().to_owned(), image })
            }
            _ => None,
        })
        .collect();
    SongBackgrounds { resize, keep_aspect: flag("keep_aspect"), link: flag("link"), as_text: flag("background_as_text"), items }
}
