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
