//! The library search index (GLOSSARY "Library", "Hymn number"). In memory, rebuilt from the
//! songs; no IO.
//!
//! A song is searchable by title, AKA, author, CCLI number, hymn number, song theme, lyric text
//! and song filename. A query is split on whitespace and every word must match some field
//! (case-insensitively, as a substring). A word `#n` is special: it matches the hymn number
//! exactly, or a filename containing `#n` (not followed by another digit), so `#50` finds
//! "Amazing Grace HOF#50" but not "HOF#500".

use std::collections::BTreeMap;

use crate::ids::SongId;
use crate::song::Song;

/// The searchable text of one song, lower-cased.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    /// The title as written, for display. The filename until the song is read.
    title: String,
    title_lc: String,
    filename: String,
    aka: String,
    author: String,
    ccli: String,
    hymn: String,
    theme: String,
    lyrics: String,
}

impl Entry {
    fn from_id(id: &SongId) -> Self {
        let name = filename(id);
        Self {
            title: name.to_string(),
            title_lc: name.to_lowercase(),
            filename: name.to_lowercase(),
            aka: String::new(),
            author: String::new(),
            ccli: String::new(),
            hymn: String::new(),
            theme: String::new(),
            lyrics: String::new(),
        }
    }

    fn from_song(id: &SongId, song: &Song) -> Self {
        let options = crate::song::SlideOptions::default();
        let lyrics: Vec<String> = song.sections.iter().flat_map(|s| s.slides(&options)).flatten().collect();
        let title = if song.title.trim().is_empty() { filename(id).to_string() } else { song.title.trim().to_string() };
        Self {
            title_lc: title.to_lowercase(),
            title,
            filename: filename(id).to_lowercase(),
            aka: song.aka.to_lowercase(),
            author: song.author.to_lowercase(),
            ccli: song.ccli.to_lowercase(),
            hymn: song.hymn_number.trim().to_lowercase(),
            theme: song.theme.to_lowercase(),
            lyrics: lyrics.join("\n").to_lowercase(),
        }
    }

    /// How well the entry matches (lower is better), or `None`.
    fn score(&self, words: &[Word]) -> Option<u32> {
        let mut total = 0;
        for word in words {
            total += match word {
                Word::Hymn(n) => self.hymn_score(n)?,
                Word::Text(t) => self.text_score(t)?,
            };
        }
        Some(total)
    }

    fn hymn_score(&self, n: &str) -> Option<u32> {
        if self.hymn == n {
            return Some(0);
        }
        let tag = format!("#{n}");
        let mut from = 0;
        while let Some(at) = self.filename[from..].find(&tag) {
            let end = from + at + tag.len();
            if !self.filename[end..].starts_with(|c: char| c.is_ascii_digit()) {
                return Some(1);
            }
            from = end;
        }
        None
    }

    fn text_score(&self, t: &str) -> Option<u32> {
        if self.title_lc.starts_with(t) {
            Some(0)
        } else if self.title_lc.contains(t) {
            Some(1)
        } else if self.aka.contains(t) || self.filename.contains(t) || self.hymn.contains(t) || self.ccli.contains(t) {
            Some(2)
        } else if self.author.contains(t) || self.theme.contains(t) {
            Some(3)
        } else if self.lyrics.contains(t) {
            Some(4)
        } else {
            None
        }
    }
}

enum Word {
    /// `#n`: the digits.
    Hymn(String),
    Text(String),
}

fn words(query: &str) -> Vec<Word> {
    query
        .split_whitespace()
        .map(|w| match w.strip_prefix('#') {
            Some(n) if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => Word::Hymn(n.to_string()),
            _ => Word::Text(w.to_lowercase()),
        })
        .collect()
}

/// The song filename: the last part of the id.
pub fn filename(id: &SongId) -> &str {
    id.as_str().rsplit('/').next().unwrap_or("")
}

/// The category: the folder part of the id (`""` for the top level).
pub fn category(id: &SongId) -> &str {
    id.as_str().rsplit_once('/').map(|(c, _)| c).unwrap_or("")
}

/// The index. Songs not yet read are searchable by filename only.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchIndex {
    entries: BTreeMap<SongId, Entry>,
}

impl SearchIndex {
    /// Replace the index contents with the given songs, known by filename only.
    pub fn reset(&mut self, ids: &[SongId]) {
        self.entries = ids.iter().map(|id| (id.clone(), Entry::from_id(id))).collect();
    }

    /// Index a song that has been read.
    pub fn insert(&mut self, id: &SongId, song: &Song) {
        self.entries.insert(id.clone(), Entry::from_song(id, song));
    }

    /// Forget a song that no longer exists.
    pub fn remove(&mut self, id: &SongId) {
        self.entries.remove(id);
    }

    /// The title to show for a song: its title once read, else its filename.
    pub fn title<'a>(&'a self, id: &'a SongId) -> &'a str {
        self.entries.get(id).map(|e| e.title.as_str()).unwrap_or_else(|| filename(id))
    }

    /// Songs matching `query`, best first, then by title. A blank query matches nothing.
    pub fn search(&self, query: &str) -> Vec<SongId> {
        let words = words(query);
        if words.is_empty() {
            return Vec::new();
        }
        let mut hits: Vec<(u32, &str, &SongId)> =
            self.entries.iter().filter_map(|(id, e)| e.score(&words).map(|s| (s, e.title_lc.as_str(), id))).collect();
        hits.sort();
        hits.into_iter().map(|(_, _, id)| id.clone()).collect()
    }
}
