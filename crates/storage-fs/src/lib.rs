//! Desktop filesystem storage: an OpenSong data root on disk, with crash-safe writes.
//!
//! Layout (the OpenSong data root, unchanged): `Songs/`, `Sets/`, `Settings/`, `Backgrounds/`.
//! serviceFlow-only sidecars (ADR-0003) live in `.serviceflow/Songs/<id>.sidecar` and
//! `.serviceflow/Sets/<id>.sidecar` at the data root, a dot folder OpenSong never reads.
//!
//! * Every visible file in `Songs/` is a song, with or without an extension. Names starting with
//!   `.` or `_` (files and folders) are ignored, as OpenSong does. Subfolders are categories.
//! * Writes go through [`craft_atomic`]: a staging file beside the target (dot-prefixed, so
//!   ignored) is renamed over it, so a crash leaves the old file or the new one, never half.
//! * Reads return the hash of the bytes read; a write with [`Expect::Hash`] compares it with the
//!   file as it is now and reports [`StorageError::Conflict`] if someone else changed it.
//!   (The check and the rename are not one atomic step: a change landing between them is lost.
//!   OpenSong has no locking either, so this narrows the window rather than closing it.)
//! * File IO is blocking `std::fs` inside `async fn`s: the futures complete on first poll. That
//!   suits one operator on a local disk; move it to a thread if a slow share ever matters.
//! * This storage announces its own writes on [`ChangeEvents::changes`]. Watching the folder for
//!   other programs' changes is a separate piece of work (library auto-refresh).

use std::any::Any;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use futures::StreamExt as _;
use futures::channel::mpsc;
use serviceflow_storage::{
    AssetId, AssetRepository, ChangeEvent, ChangeEvents, ChangeKind, ChangeStream, ContentHash, DocumentId, Expect, Key, Result, SetId,
    SetRepository, SettingsId, SettingsRepository, SidecarRepository, SongId, SongRepository, StorageError, Stored, check_path,
    is_ignored_path, key_path,
};

/// Storage over an OpenSong data root on disk.
pub struct FsStorage {
    root: PathBuf,
    subscribers: Mutex<Vec<mpsc::UnboundedSender<ChangeEvent>>>,
    /// Something to drop together with the storage (a temporary folder in tests).
    _guard: Option<Box<dyn Any>>,
}

impl FsStorage {
    /// Use `root` as the data root. It must be an existing folder; it need not have a `Songs/`
    /// folder yet (that is an empty library). See [`FsStorage::looks_like_data_root`].
    pub fn open(root: &Path) -> std::io::Result<Self> {
        if !root.is_dir() {
            return Err(std::io::Error::new(ErrorKind::NotFound, format!("{} is not a folder", root.display())));
        }
        Ok(Self { root: root.to_path_buf(), subscribers: Mutex::new(Vec::new()), _guard: None })
    }

    /// Drop `guard` when this storage is dropped. For tests that own a temporary folder.
    pub fn keeping_alive(mut self, guard: impl Any) -> Self {
        self._guard = Some(Box::new(guard));
        self
    }

    /// `true` when `path` looks like an OpenSong data root: it holds a `Songs/` folder.
    pub fn looks_like_data_root(path: &Path) -> bool {
        path.join("Songs").is_dir()
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn file(&self, key: &Key) -> Result<PathBuf> {
        let path = key_path(key);
        check_path(path)?;
        let relative = match key {
            Key::Song(_) => Path::new("Songs").join(path),
            Key::Set(_) => Path::new("Sets").join(path),
            Key::Asset(_) => Path::new("Backgrounds").join(path),
            Key::Settings(_) => Path::new("Settings").join(path),
            Key::Sidecar(DocumentId::Song(_)) => Path::new(".serviceflow/Songs").join(format!("{path}.sidecar")),
            Key::Sidecar(DocumentId::Set(_)) => Path::new(".serviceflow/Sets").join(format!("{path}.sidecar")),
        };
        Ok(self.root.join(relative))
    }

    fn read(&self, key: Key) -> Result<Stored> {
        match self.read_if_exists(&key)? {
            Some(stored) => Ok(stored),
            None => Err(StorageError::NotFound(key)),
        }
    }

    fn read_if_exists(&self, key: &Key) -> Result<Option<Stored>> {
        match std::fs::read(self.file(key)?) {
            Ok(bytes) => Ok(Some(Stored::new(bytes))),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io_error(&e)),
        }
    }

    fn write(&self, key: Key, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash> {
        let path = self.file(&key)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| io_error(&e))?;
        }
        let hash = ContentHash::of(&bytes);
        match expect {
            Expect::Any => craft_atomic::atomic_write(&path, &bytes).map_err(|e| io_error(&e))?,
            // `write_new` never replaces an existing file, so this is race-free.
            Expect::Absent => craft_atomic::write_new(&path, &bytes).map_err(|e| match e.kind() {
                ErrorKind::AlreadyExists => StorageError::Conflict(key.clone()),
                _ => io_error(&e),
            })?,
            Expect::Hash(expected) => {
                if self.read_if_exists(&key)?.map(|s| s.hash) != Some(expected) {
                    return Err(StorageError::Conflict(key));
                }
                craft_atomic::atomic_write(&path, &bytes).map_err(|e| io_error(&e))?;
            }
        }
        self.announce(ChangeEvent { key, kind: ChangeKind::Written });
        Ok(hash)
    }

    /// Every visible file under `folder`, as `/`-separated paths relative to it, sorted.
    fn list(&self, folder: &str) -> Result<Vec<String>> {
        let mut found = Vec::new();
        collect(&self.root.join(folder), "", &mut found).map_err(|e| io_error(&e))?;
        found.sort();
        Ok(found)
    }

    fn announce(&self, event: ChangeEvent) {
        let mut subscribers = self.subscribers.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        subscribers.retain(|s| s.unbounded_send(event.clone()).is_ok());
    }
}

fn io_error(e: &std::io::Error) -> StorageError {
    StorageError::Other(e.to_string())
}

/// Visible files below `dir`. A missing folder holds nothing. Names that are not UTF-8 cannot be
/// ids and are skipped, like unreadable entries are not (those fail the listing).
fn collect(dir: &Path, prefix: &str, out: &mut Vec<String>) -> std::io::Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let entry = entry?;
        let Ok(name) = entry.file_name().into_string() else { continue };
        if is_ignored_path(&name) {
            continue;
        }
        let path = format!("{prefix}{name}");
        // `metadata` follows symlinks, as OpenSong does.
        match std::fs::metadata(entry.path()) {
            Ok(m) if m.is_dir() => collect(&entry.path(), &format!("{path}/"), out)?,
            Ok(m) if m.is_file() => out.push(path),
            _ => {}
        }
    }
    Ok(())
}

impl SongRepository for FsStorage {
    async fn list_songs(&self) -> Result<Vec<SongId>> {
        Ok(self.list("Songs")?.into_iter().map(SongId).collect())
    }

    async fn read_song(&self, id: &SongId) -> Result<Stored> {
        self.read(Key::Song(id.clone()))
    }

    async fn write_song(&self, id: &SongId, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash> {
        self.write(Key::Song(id.clone()), bytes, expect)
    }
}

impl SetRepository for FsStorage {
    async fn list_sets(&self) -> Result<Vec<SetId>> {
        Ok(self.list("Sets")?.into_iter().map(SetId).collect())
    }

    async fn read_set(&self, id: &SetId) -> Result<Stored> {
        self.read(Key::Set(id.clone()))
    }

    async fn write_set(&self, id: &SetId, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash> {
        self.write(Key::Set(id.clone()), bytes, expect)
    }
}

impl AssetRepository for FsStorage {
    async fn list_assets(&self) -> Result<Vec<AssetId>> {
        Ok(self.list("Backgrounds")?.into_iter().map(AssetId).collect())
    }

    async fn read_asset(&self, id: &AssetId) -> Result<Stored> {
        self.read(Key::Asset(id.clone()))
    }

    async fn write_asset(&self, id: &AssetId, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash> {
        self.write(Key::Asset(id.clone()), bytes, expect)
    }
}

impl SidecarRepository for FsStorage {
    async fn read_sidecar(&self, owner: &DocumentId) -> Result<Option<Stored>> {
        self.read_if_exists(&Key::Sidecar(owner.clone()))
    }

    async fn write_sidecar(&self, owner: &DocumentId, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash> {
        self.write(Key::Sidecar(owner.clone()), bytes, expect)
    }
}

impl SettingsRepository for FsStorage {
    async fn read_settings(&self, id: &SettingsId) -> Result<Stored> {
        self.read(Key::Settings(id.clone()))
    }

    async fn write_settings(&self, id: &SettingsId, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash> {
        self.write(Key::Settings(id.clone()), bytes, expect)
    }
}

impl ChangeEvents for FsStorage {
    fn changes(&self) -> ChangeStream {
        let (sender, receiver) = mpsc::unbounded();
        self.subscribers.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(sender);
        receiver.boxed_local()
    }
}
