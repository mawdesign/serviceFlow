//! Async repository traits for songs, sets, assets, sidecars and settings, plus a change-event
//! stream (ADR-0004). Traits only: implementations live in the `serviceflow-storage-*` crates.
//!
//! Repositories deal in bytes. Parsing is the codec's job (`serviceflow-opensong`), so the same
//! storage serves OpenSong XML, sidecars and images, and tests can check exactly what was
//! written. Every implementation must pass the contract suite in
//! `serviceflow_testkit::storage_contract`.
//!
//! The traits use `async fn` without a `Send` bound on purpose: browser storage futures are not
//! `Send`. Callers drive them on a local executor (see `serviceflow-runtime`).
#![allow(async_fn_in_trait)]

use futures::stream::LocalBoxStream;
pub use serviceflow_core::ids::{ContentHash, Expect, SetId, SongId};

/// An image or other media file: its path under the data root's `Backgrounds/` folder, with `/`
/// between folders.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssetId(pub String);

impl AssetId {
    /// Whether this asset is a picture serviceFlow can show: a `png`, `jpg`, `jpeg`, `gif` or
    /// `bmp` extension, in any case. Other files under `Backgrounds/` (videos, say) are not.
    pub fn is_picture(&self) -> bool {
        const PICTURES: [&str; 5] = ["png", "jpg", "jpeg", "gif", "bmp"];
        self.0.rsplit_once('.').is_some_and(|(_, ext)| PICTURES.iter().any(|p| ext.eq_ignore_ascii_case(p)))
    }
}

/// A settings file: its filename under the data root's `Settings/` folder (`"PresentSettings"`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SettingsId(pub String);

/// The document a sidecar belongs to (ADR-0003).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DocumentId {
    Song(SongId),
    Set(SetId),
}

/// Anything storage holds. Used by change events and errors.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    Song(SongId),
    Set(SetId),
    Asset(AssetId),
    Sidecar(DocumentId),
    Settings(SettingsId),
}

/// A document's bytes as read, with the hash of those bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    pub bytes: Vec<u8>,
    pub hash: ContentHash,
}

impl Stored {
    pub fn new(bytes: Vec<u8>) -> Self {
        let hash = ContentHash::of(&bytes);
        Self { bytes, hash }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    NotFound(Key),
    /// The write's [`Expect`] did not hold: the document changed since it was read.
    Conflict(Key),
    /// Anything else the implementation reports (IO, permissions, browser errors).
    Other(String),
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageError::NotFound(key) => write!(f, "not found: {key:?}"),
            StorageError::Conflict(key) => write!(f, "changed since it was read: {key:?}"),
            StorageError::Other(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for StorageError {}

pub type Result<T> = std::result::Result<T, StorageError>;

/// Songs under `Songs/`. Categories are subfolders.
pub trait SongRepository {
    /// Every song in the library, sorted by id.
    async fn list_songs(&self) -> Result<Vec<SongId>>;
    async fn read_song(&self, id: &SongId) -> Result<Stored>;
    /// Write a song's bytes and return the new hash.
    async fn write_song(&self, id: &SongId, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash>;
    /// Remove a song. `expect` must hold for it ([`Expect::Absent`] never does); a missing song is
    /// [`StorageError::NotFound`]. Announced as [`ChangeKind::Removed`].
    async fn delete_song(&self, id: &SongId, expect: Expect) -> Result<()>;
    /// Move song `from` to `to` (another filename, another category, or both), keeping its bytes,
    /// and return their hash. `expect` must hold for `from`; `to` must not exist
    /// ([`StorageError::Conflict`] of `to`). A crash leaves the song under one name or the other.
    /// Announced as `from` removed, then `to` written.
    async fn rename_song(&self, from: &SongId, to: &SongId, expect: Expect) -> Result<ContentHash>;
}

/// Sets under `Sets/`.
pub trait SetRepository {
    /// Every set, sorted by id.
    async fn list_sets(&self) -> Result<Vec<SetId>>;
    async fn read_set(&self, id: &SetId) -> Result<Stored>;
    async fn write_set(&self, id: &SetId, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash>;
    /// As [`SongRepository::delete_song`].
    async fn delete_set(&self, id: &SetId, expect: Expect) -> Result<()>;
    /// As [`SongRepository::rename_song`].
    async fn rename_set(&self, from: &SetId, to: &SetId, expect: Expect) -> Result<ContentHash>;
}

/// Images and other media under `Backgrounds/`.
pub trait AssetRepository {
    /// Every asset, sorted by id.
    async fn list_assets(&self) -> Result<Vec<AssetId>>;
    async fn read_asset(&self, id: &AssetId) -> Result<Stored>;
    async fn write_asset(&self, id: &AssetId, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash>;
}

/// serviceFlow-only data beside a song or set (ADR-0003). A missing sidecar is normal.
pub trait SidecarRepository {
    async fn read_sidecar(&self, owner: &DocumentId) -> Result<Option<Stored>>;
    async fn write_sidecar(&self, owner: &DocumentId, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash>;
}

/// Settings files under `Settings/`.
pub trait SettingsRepository {
    async fn read_settings(&self, id: &SettingsId) -> Result<Stored>;
    async fn write_settings(&self, id: &SettingsId, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash>;
}

/// Replacing the whole library with an OpenSong data root packed in a zip file. Browser storage
/// does this (its library is not a folder the operator can reach); others refuse, the default.
pub trait ArchiveImport {
    /// Replace everything stored with the data root in `zip` and return the number of files
    /// written. Nothing is removed when the zip cannot be read. No change events are sent: the
    /// caller reads the library again.
    async fn import_archive(&self, zip: &[u8]) -> Result<usize> {
        let _ = zip;
        Err(StorageError::Other("this library cannot be replaced from a zip file".to_owned()))
    }
}

/// What happened to a stored document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeKind {
    /// Created or overwritten.
    Written,
    Removed,
}

/// One change to storage, made by this instance or from outside (another program, another
/// serviceFlow instance, remote sync).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeEvent {
    pub key: Key,
    pub kind: ChangeKind,
}

/// A stream of change events. It ends when the storage is dropped.
pub type ChangeStream = LocalBoxStream<'static, ChangeEvent>;

pub trait ChangeEvents {
    /// Subscribe to every change made after this call.
    fn changes(&self) -> ChangeStream;
}

/// Everything serviceFlow needs from storage. Implemented automatically for any type that
/// implements every repository trait.
pub trait Storage: SongRepository + SetRepository + AssetRepository + SidecarRepository + SettingsRepository + ChangeEvents + ArchiveImport {}

impl<T> Storage for T where T: SongRepository + SetRepository + AssetRepository + SidecarRepository + SettingsRepository + ChangeEvents + ArchiveImport
{}

/// `true` when OpenSong would ignore this path: any `/`-separated part starts with `.` or `_`.
/// Hidden documents are never listed. Sidecars and temporary files rely on this (ADR-0003).
pub fn is_ignored_path(path: &str) -> bool {
    path.split('/').any(|part| part.starts_with('.') || part.starts_with('_'))
}

/// Characters an id may not hold. On Unix (macOS, Linux) `\` and `:` are ordinary file-name
/// characters, and OpenSong libraries made there use them ("Psalm 23: ..."); elsewhere (Windows,
/// and a browser, whose host may be Windows) they are separators or drive and stream markers.
const FORBIDDEN: &[char] = if cfg!(unix) { &['\0'] } else { &['\\', '\0', ':'] };

/// Check that an id is a plain relative path inside its folder: no empty, `.` or `..` parts, no
/// NUL, and outside Unix no backslash or colon, so it cannot name anything outside the data root.
/// It never starts with `:` either: that is how the engine names set items' slides, which are not
/// files. Every implementation applies this to the ids it is given, and lists only names that
/// pass it, so everything listed can be read.
pub fn check_path(path: &str) -> Result<()> {
    let bad = path.is_empty()
        || path.starts_with(':')
        || path.contains(FORBIDDEN)
        || path.split('/').any(|part| part.is_empty() || part == "." || part == "..");
    if bad { Err(StorageError::Other(format!("not a valid path inside the data root: {path:?}"))) } else { Ok(()) }
}

/// The path part of a key, for [`check_path`]. A sidecar is named by its owner's path.
pub fn key_path(key: &Key) -> &str {
    match key {
        Key::Song(SongId(p)) | Key::Set(SetId(p)) | Key::Asset(AssetId(p)) | Key::Settings(SettingsId(p)) => p,
        Key::Sidecar(DocumentId::Song(SongId(p))) | Key::Sidecar(DocumentId::Set(SetId(p))) => p,
    }
}

/// Whether a document currently hashing to `current` (`None`: absent) meets `expect`.
pub fn expect_holds(expect: Expect, current: Option<ContentHash>) -> bool {
    match expect {
        Expect::Any => true,
        Expect::Absent => current.is_none(),
        Expect::Hash(expected) => current == Some(expected),
    }
}

/// Makes the key of a file from its path inside its folder.
pub type MakeKey = fn(String) -> Key;

/// The top-level data-root folders that hold documents, with the key each file in them has.
pub const FOLDERS: [(&str, MakeKey); 4] = [
    ("Songs", |p| Key::Song(SongId(p))),
    ("Sets", |p| Key::Set(SetId(p))),
    ("Settings", |p| Key::Settings(SettingsId(p))),
    ("Backgrounds", |p| Key::Asset(AssetId(p))),
];

/// The key of the file `path` inside top-level `folder` (one of [`FOLDERS`]).
pub fn key_in_folder(folder: &str, path: &str) -> Option<Key> {
    FOLDERS.iter().find(|(name, _)| *name == folder).map(|(_, make)| make(path.to_owned()))
}

/// The `/`-separated file path of a document under the data root: `Songs/<id>`, `Sets/<id>`,
/// `Settings/<id>`, `Backgrounds/<id>`, and sidecars (ADR-0003) in the dot folder OpenSong never
/// reads, `.serviceflow/Songs/<id>.sidecar` and `.serviceflow/Sets/<id>.sidecar`. The path is not
/// checked: call [`check_path`] first.
pub fn key_file(key: &Key) -> String {
    let path = key_path(key);
    match key {
        Key::Song(_) => format!("Songs/{path}"),
        Key::Set(_) => format!("Sets/{path}"),
        Key::Asset(_) => format!("Backgrounds/{path}"),
        Key::Settings(_) => format!("Settings/{path}"),
        Key::Sidecar(DocumentId::Song(_)) => format!(".serviceflow/Songs/{path}.sidecar"),
        Key::Sidecar(DocumentId::Set(_)) => format!(".serviceflow/Sets/{path}.sidecar"),
    }
}

/// The debounce shared by the polling watchers: a difference between `announced` (what listeners
/// were told) and `current` is announced once `previous` (the scan before) shows the same thing,
/// so a file written in pieces is announced once, when it has stopped changing. `announced` is
/// updated to match and the events are returned. `M` is whatever a scan records per file.
pub fn settled_changes<M: PartialEq + Clone>(
    announced: &mut std::collections::BTreeMap<Key, M>,
    current: &std::collections::BTreeMap<Key, M>,
    previous: Option<&std::collections::BTreeMap<Key, M>>,
) -> Vec<ChangeEvent> {
    let keys: std::collections::BTreeSet<Key> = current.keys().chain(announced.keys()).cloned().collect();
    let mut events = Vec::new();
    for key in keys {
        let now = current.get(&key);
        let settled = previous.map(|p| p.get(&key)) == Some(now);
        if announced.get(&key) == now || !settled {
            continue;
        }
        match now {
            Some(meta) => {
                announced.insert(key.clone(), meta.clone());
                events.push(ChangeEvent { key, kind: ChangeKind::Written });
            }
            None => {
                announced.remove(&key);
                events.push(ChangeEvent { key, kind: ChangeKind::Removed });
            }
        }
    }
    events
}
