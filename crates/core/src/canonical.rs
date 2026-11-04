//! The canonical form of a song (ADR-0002, GLOSSARY "Canonical form"), as a model-to-model step
//! the codec applies before writing.
//!
//! Section tags become `[Vn]` and `[Vn] (L)`: a bare number `n` is written `Vn`, a legacy
//! multilanguage tag (`V1-L`, `V1L`) is written `V1` with the `(L)` option, and header lines
//! lose anything else after the tag. Presentation-order tokens are rewritten to the tags of the
//! sections they name. Trailing whitespace is trimmed from every lyrics line. Everything else
//! in the lyrics (comments, chord lines, numbered verse blocks) stays as written, so numbered
//! verse blocks stay compact.
//!
//! The unmodelled elements are put in the order the file will have them.
//!
//! If renaming would make two sections one (a song with both `[1]` and `[V1]`), no tag is
//! renamed and the presentation order is kept as it is; [`Canonical::tags_kept`] says so.

use crate::lyrics;
use crate::song::{Song, SongWarning, canonical_tag};

/// A song in canonical form.
#[derive(Debug, Clone, PartialEq)]
pub struct Canonical {
    pub song: Song,
    /// Tags were not renamed because some would have collided.
    pub tags_kept: bool,
}

/// Whether the song's tags would collide when renamed to the canonical form.
pub fn has_tag_collision(song: &Song) -> bool {
    lyrics::parse(&song.lyrics).1.iter().any(|w| matches!(w, SongWarning::TagCollision { .. }))
}

/// The song in canonical form. Applying it again changes nothing.
pub fn canonicalize(song: &Song) -> Canonical {
    // The sections as the lyrics read now (tokens are resolved against these).
    let original = Song { sections: lyrics::parse(&song.lyrics).0, ..Song::default() };
    let sections = &original.sections;
    let trimmed = rejoin(&song.lyrics, song.lyrics.lines().map(|l| l.trim_end().to_owned()));
    if !has_tag_collision(song) {
        let renamed = rejoin(
            &song.lyrics,
            song.lyrics.lines().map(|line| {
                let line = line.trim_end();
                let Some(rest) = line.strip_prefix('[') else { return line.to_owned() };
                // As the parser reads it: the tag ends at the first `]` (or the line).
                let tag = rest.split_once(']').map_or(rest, |(tag, _)| tag).trim();
                let multilanguage = sections.iter().find(|s| s.tag.eq_ignore_ascii_case(tag)).is_some_and(|s| s.multilanguage);
                format!("[{}]{}", canonical_tag(tag), if multilanguage { " (L)" } else { "" })
            }),
        );
        let (new_sections, new_warnings) = lyrics::parse(&renamed);
        // Renaming must keep every section apart; anything else keeps the tags as they are.
        if new_sections.len() == sections.len() {
            let presentation = rename_in(&original, &new_sections, &song.presentation);
            let song = Song { lyrics: renamed, sections: new_sections, warnings: new_warnings, presentation, ..song.clone() };
            return Canonical { song: ordered(song), tags_kept: false };
        }
    }
    let (sections, warnings) = lyrics::parse(&trimmed);
    Canonical { song: ordered(Song { lyrics: trimmed, sections, warnings, ..song.clone() }), tags_kept: true }
}

/// Presentation-order `tokens` naming sections of `before` (such as a set item's order), rewritten
/// to the tags those sections have in `after`, the canonical form of `before`. Tokens that name
/// no section are kept. When the tags were kept ([`Canonical::tags_kept`]) nothing changes.
pub fn rename_tokens(before: &Song, after: &Song, tokens: &[String]) -> Vec<String> {
    let original = Song { sections: lyrics::parse(&before.lyrics).0, ..Song::default() };
    if original.sections.len() != after.sections.len() {
        return tokens.to_vec();
    }
    rename_in(&original, &after.sections, tokens)
}

/// Whether saving `song` renames any of its section tags.
pub fn renames_tags(song: &Song) -> bool {
    let canonical = canonicalize(song);
    let before = lyrics::parse(&song.lyrics).0;
    !canonical.tags_kept && before.iter().map(|s| &s.tag).ne(canonical.song.sections.iter().map(|s| &s.tag))
}

fn rename_in(original: &Song, new_sections: &[crate::song::Section], tokens: &[String]) -> Vec<String> {
    tokens
        .iter()
        .map(|token| {
            let Some(section) = original.section(token) else { return token.clone() };
            let index = original.sections.iter().position(|s| std::ptr::eq(s, section));
            match index.and_then(|i| new_sections.get(i)) {
                // Tokens are space-separated: a space in a tag is written as a no-break space.
                Some(new) => new.tag.replace(' ', "\u{a0}"),
                None => token.clone(),
            }
        })
        .collect()
}

/// Unmodelled elements OpenSong knows, in the order the canonical file has them (opensong.md §2):
/// `linked_songs` among the details, `backgrounds` and `style` after the lyrics.
pub const KNOWN_RAW: [&str; 3] = ["linked_songs", "backgrounds", "style"];

/// The song with the first of each [`KNOWN_RAW`] element first, in that order, and every other
/// unmodelled element after them in the order read.
fn ordered(mut song: Song) -> Song {
    let mut rest = std::mem::take(&mut song.other_elements);
    for name in KNOWN_RAW {
        if let Some(i) = rest.iter().position(|e| e.name == name) {
            song.other_elements.push(rest.remove(i));
        }
    }
    song.other_elements.extend(rest);
    song
}

/// Lines joined with `\n`, ending in `\n` when `original` did.
fn rejoin(original: &str, lines: impl Iterator<Item = String>) -> String {
    let mut text = lines.collect::<Vec<_>>().join("\n");
    if original.ends_with('\n') {
        text.push('\n');
    }
    text
}
