//! Song files (opensong.md §2–§6). Basic reading: metadata, sections, lyric lines, `||` slide
//! breaks and the presentation order.

use serviceflow_core::song::{Section, Song};

use crate::CodecError;
use crate::xml;

/// Read an OpenSong song file.
pub fn read_song(bytes: &[u8]) -> Result<Song, CodecError> {
    let root = xml::parse(bytes)?;
    if root.name != "song" {
        return Err(CodecError(format!("not a song: root element is <{}>", root.name)));
    }
    let text = |name| root.child_text(name);
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
        time_sig: text("time_sig"),
        user1: text("user1"),
        user2: text("user2"),
        user3: text("user3"),
        presentation: text("presentation").split_whitespace().map(str::to_owned).collect(),
        sections: parse_lyrics(&text("lyrics")),
    })
}

/// Split lyrics into sections (OpenSong `SongML.LyricsToSections`). Each line is right-trimmed;
/// its first character decides what it is: `[` a section tag, `.` a chord line, `;` a comment,
/// `-` a print-layout command (none of these are lyrics); anything else a lyric line. Blank
/// lines are dropped. `||` anywhere starts a new slide.
fn parse_lyrics(lyrics: &str) -> Vec<Section> {
    let mut sections: Vec<Section> = Vec::new();
    for line in lyrics.lines().map(str::trim_end) {
        if let Some(rest) = line.strip_prefix('[') {
            let tag = rest.split(']').next().unwrap_or_default().trim();
            sections.push(Section { tag: tag.to_owned(), slides: vec![Vec::new()] });
            continue;
        }
        if line.is_empty() || line.starts_with(['.', ';', '-']) {
            continue;
        }
        if sections.is_empty() {
            sections.push(Section { tag: String::new(), slides: vec![Vec::new()] });
        }
        let Some(section) = sections.last_mut() else { continue };
        for (i, piece) in line.split("||").enumerate() {
            if i > 0 {
                section.slides.push(Vec::new());
            }
            let piece = piece.trim();
            if !piece.is_empty()
                && let Some(slide) = section.slides.last_mut()
            {
                slide.push(piece.to_owned());
            }
        }
    }
    for section in &mut sections {
        section.slides.retain(|slide| !slide.is_empty());
    }
    sections
}
