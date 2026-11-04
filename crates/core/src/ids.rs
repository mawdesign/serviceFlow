//! Identifiers of documents in the data root. They are paths, so they mean the same thing to
//! every storage implementation.

/// A song: its path under the data root's `Songs/` folder, with `/` between a category and the
/// song filename (`"It Is Well With My Soul"`, `"Hymns/Amazing Grace"`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SongId(pub String);

/// A set: its filename under the data root's `Sets/` folder.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SetId(pub String);

impl SongId {
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl SongId {
    /// The id of copy `n` (from 1) of this song, beside it: `Hymns/Abide (copy)`, then
    /// `Hymns/Abide (copy 2)`. A `.xml` extension stays at the end.
    pub fn copy(&self, n: usize) -> SongId {
        let suffix = if n <= 1 { " (copy)".to_owned() } else { format!(" (copy {n})") };
        let (dir, name) = self.0.rsplit_once('/').map_or(("", self.0.as_str()), |(d, n)| (d, n));
        let (stem, ext) = match name.len().checked_sub(4).filter(|&i| name.is_char_boundary(i)) {
            Some(i) if name[i..].eq_ignore_ascii_case(".xml") && i > 0 => (&name[..i], &name[i..]),
            _ => (name, ""),
        };
        let file = format!("{stem}{suffix}{ext}");
        SongId(if dir.is_empty() { file } else { format!("{dir}/{file}") })
    }
}

impl SongId {
    /// The id of a new song titled `title` in `category` (`""`: the top level): the title made
    /// safe as a filename, plus `.xml`, with ` (2)`, ` (3)`… before the extension when the name
    /// is taken (compared without regard to case, as Windows and macOS disks do).
    pub fn for_new_song(category: &str, title: &str, taken: &[SongId]) -> SongId {
        let stem = sanitise_filename(title);
        let stem = if stem.is_empty() { "Untitled".to_owned() } else { stem };
        let taken: Vec<String> = taken.iter().map(|id| id.0.to_lowercase()).collect();
        let category = category.trim_matches('/');
        let named = |n: usize| {
            let file = if n == 1 { format!("{stem}.xml") } else { format!("{stem} ({n}).xml") };
            SongId(if category.is_empty() { file } else { format!("{category}/{file}") })
        };
        (1..=taken.len() + 1).map(named).find(|id| !taken.contains(&id.0.to_lowercase())).unwrap_or_else(|| named(taken.len() + 2))
    }
}

/// `text` as a filename every disk and OpenSong accept: no `/ \ : * ? " < > |` or control
/// characters, runs of spaces made one, no leading `.`/`_` (OpenSong hides those) and no
/// trailing `.` (Windows drops it).
pub fn sanitise_filename(text: &str) -> String {
    let cleaned: String =
        text.chars().map(|c| if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { ' ' } else { c }).collect();
    let joined = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    joined.trim_start_matches(['.', '_', ' ']).trim_end_matches(['.', ' ']).to_owned()
}

impl SetId {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SongId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::fmt::Display for SetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A hash of a document's bytes, recorded on read so a later write can detect that the
/// document changed underneath it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentHash(pub u64);

impl ContentHash {
    /// FNV-1a over the bytes. Stable across platforms and runs.
    pub fn of(bytes: &[u8]) -> Self {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for b in bytes {
            hash ^= u64::from(*b);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        Self(hash)
    }
}

/// The condition a write requires of what is currently stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expect {
    /// Write whatever is there.
    Any,
    /// The document must not exist yet.
    Absent,
    /// The document must still have this hash (the one recorded when it was read).
    Hash(ContentHash),
}
