//! In-memory storage implementation, for tests and seeding from the OpenSong fixtures.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use futures::StreamExt as _;
use futures::channel::mpsc;
use serviceflow_storage::{
    ArchiveImport, AssetId, AssetRepository, ChangeEvent, ChangeEvents, ChangeKind, ChangeStream, ContentHash, DocumentId, Expect, FOLDERS, Key,
    Result, SetId, SetRepository, SettingsId, SettingsRepository, SidecarRepository, SongId, SongRepository, StorageError, Stored, check_path,
    expect_holds, is_ignored_path, key_in_folder, key_path,
};

/// Unpacks a zip file into (`/`-separated path under the data root, bytes).
pub type ArchiveReader = fn(&[u8]) -> std::result::Result<Vec<(String, Vec<u8>)>, String>;

/// Storage held in memory. Every write is announced on [`ChangeEvents::changes`].
#[derive(Default)]
pub struct MemoryStorage {
    documents: Mutex<BTreeMap<Key, Stored>>,
    subscribers: Mutex<Vec<mpsc::UnboundedSender<ChangeEvent>>>,
    archive_reader: Option<ArchiveReader>,
}

impl MemoryStorage {
    /// Empty storage.
    pub fn new() -> Self {
        Self::default()
    }

    /// Accept [`ArchiveImport::import_archive`], unpacking zips with `reader` (this crate has no
    /// zip codec of its own). Without one, imports are refused, as on a real folder.
    pub fn with_archive_reader(mut self, reader: ArchiveReader) -> Self {
        self.archive_reader = Some(reader);
        self
    }

    /// Storage holding a copy of an OpenSong data root: `Songs/` (subfolders are categories),
    /// `Sets/`, `Settings/` and `Backgrounds/`. Names starting with `.` or `_` are skipped.
    pub fn from_data_root(root: &Path) -> std::io::Result<Self> {
        let storage = Self::new();
        {
            let mut documents = storage.lock_documents();
            for (folder, key) in FOLDERS {
                let mut files = Vec::new();
                collect_files(&root.join(folder), "", &mut files)?;
                for (path, bytes) in files {
                    documents.insert(key(path), Stored::new(bytes));
                }
            }
        }
        Ok(storage)
    }

    fn lock_documents(&self) -> std::sync::MutexGuard<'_, BTreeMap<Key, Stored>> {
        self.documents.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn read(&self, key: Key) -> Result<Stored> {
        check_path(key_path(&key))?;
        self.lock_documents().get(&key).cloned().ok_or(StorageError::NotFound(key))
    }

    fn write(&self, key: Key, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash> {
        check_path(key_path(&key))?;
        let stored = Stored::new(bytes);
        let hash = stored.hash;
        {
            let mut documents = self.lock_documents();
            if !expect_holds(expect, documents.get(&key).map(|s| s.hash)) {
                return Err(StorageError::Conflict(key));
            }
            documents.insert(key.clone(), stored);
        }
        self.announce(ChangeEvent { key, kind: ChangeKind::Written });
        Ok(hash)
    }

    /// Delete a document, announcing [`ChangeKind::Removed`] if it existed. Unlike the
    /// trait's delete it has no precondition, for tests and the engine harness.
    pub fn remove(&self, key: &Key) {
        if self.lock_documents().remove(key).is_some() {
            self.announce(ChangeEvent { key: key.clone(), kind: ChangeKind::Removed });
        }
    }

    /// Write a document without a precondition, as if another program had.
    pub fn write_key(&self, key: &Key, bytes: &[u8]) {
        self.write(key.clone(), bytes.to_vec(), Expect::Any).expect("writing a document");
    }

    fn delete(&self, key: Key, expect: Expect) -> Result<()> {
        check_path(key_path(&key))?;
        {
            let mut documents = self.lock_documents();
            let Some(current) = documents.get(&key) else { return Err(StorageError::NotFound(key)) };
            if !expect_holds(expect, Some(current.hash)) {
                return Err(StorageError::Conflict(key));
            }
            documents.remove(&key);
        }
        self.announce(ChangeEvent { key, kind: ChangeKind::Removed });
        Ok(())
    }

    fn rename(&self, from: Key, to: Key, expect: Expect) -> Result<ContentHash> {
        check_path(key_path(&from))?;
        check_path(key_path(&to))?;
        let hash = {
            let mut documents = self.lock_documents();
            let Some(hash) = documents.get(&from).map(|s| s.hash) else { return Err(StorageError::NotFound(from)) };
            if !expect_holds(expect, Some(hash)) {
                return Err(StorageError::Conflict(from));
            }
            if to != from && documents.contains_key(&to) {
                return Err(StorageError::Conflict(to));
            }
            if let Some(stored) = documents.remove(&from) {
                documents.insert(to.clone(), stored);
            }
            hash
        };
        self.announce(ChangeEvent { key: from, kind: ChangeKind::Removed });
        self.announce(ChangeEvent { key: to, kind: ChangeKind::Written });
        Ok(hash)
    }

    fn list<T>(&self, pick: impl Fn(&Key) -> Option<T>) -> Vec<T> {
        // BTreeMap order is id order, so the result is sorted.
        self.lock_documents().keys().filter(|k| !matches!(k, Key::Sidecar(_)) && !is_ignored_path(key_path(k))).filter_map(pick).collect()
    }

    fn announce(&self, event: ChangeEvent) {
        let mut subscribers = self.subscribers.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        subscribers.retain(|s| s.unbounded_send(event.clone()).is_ok());
    }
}

/// Every file under `dir`, as (`/`-separated path relative to `dir`, bytes). A missing folder
/// holds nothing.
fn collect_files(dir: &Path, prefix: &str, out: &mut Vec<(String, Vec<u8>)>) -> std::io::Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(['.', '_']) {
            continue;
        }
        let path = format!("{prefix}{name}");
        if entry.file_type()?.is_dir() {
            collect_files(&entry.path(), &format!("{path}/"), out)?;
        } else {
            out.push((path, std::fs::read(entry.path())?));
        }
    }
    Ok(())
}

impl SongRepository for MemoryStorage {
    async fn list_songs(&self) -> Result<Vec<SongId>> {
        Ok(self.list(|k| match k {
            Key::Song(id) => Some(id.clone()),
            _ => None,
        }))
    }

    async fn read_song(&self, id: &SongId) -> Result<Stored> {
        self.read(Key::Song(id.clone()))
    }

    async fn write_song(&self, id: &SongId, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash> {
        self.write(Key::Song(id.clone()), bytes, expect)
    }

    async fn delete_song(&self, id: &SongId, expect: Expect) -> Result<()> {
        self.delete(Key::Song(id.clone()), expect)
    }

    async fn rename_song(&self, from: &SongId, to: &SongId, expect: Expect) -> Result<ContentHash> {
        self.rename(Key::Song(from.clone()), Key::Song(to.clone()), expect)
    }
}

impl SetRepository for MemoryStorage {
    async fn list_sets(&self) -> Result<Vec<SetId>> {
        Ok(self.list(|k| match k {
            Key::Set(id) => Some(id.clone()),
            _ => None,
        }))
    }

    async fn read_set(&self, id: &SetId) -> Result<Stored> {
        self.read(Key::Set(id.clone()))
    }

    async fn write_set(&self, id: &SetId, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash> {
        self.write(Key::Set(id.clone()), bytes, expect)
    }

    async fn delete_set(&self, id: &SetId, expect: Expect) -> Result<()> {
        self.delete(Key::Set(id.clone()), expect)
    }

    async fn rename_set(&self, from: &SetId, to: &SetId, expect: Expect) -> Result<ContentHash> {
        self.rename(Key::Set(from.clone()), Key::Set(to.clone()), expect)
    }
}

impl AssetRepository for MemoryStorage {
    async fn list_assets(&self) -> Result<Vec<AssetId>> {
        Ok(self.list(|k| match k {
            Key::Asset(id) => Some(id.clone()),
            _ => None,
        }))
    }

    async fn read_asset(&self, id: &AssetId) -> Result<Stored> {
        self.read(Key::Asset(id.clone()))
    }

    async fn write_asset(&self, id: &AssetId, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash> {
        self.write(Key::Asset(id.clone()), bytes, expect)
    }
}

impl SidecarRepository for MemoryStorage {
    async fn read_sidecar(&self, owner: &DocumentId) -> Result<Option<Stored>> {
        let key = Key::Sidecar(owner.clone());
        check_path(key_path(&key))?;
        Ok(self.lock_documents().get(&key).cloned())
    }

    async fn write_sidecar(&self, owner: &DocumentId, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash> {
        self.write(Key::Sidecar(owner.clone()), bytes, expect)
    }

    async fn delete_sidecar(&self, owner: &DocumentId, expect: Expect) -> Result<()> {
        self.delete(Key::Sidecar(owner.clone()), expect)
    }
}

impl SettingsRepository for MemoryStorage {
    async fn read_settings(&self, id: &SettingsId) -> Result<Stored> {
        self.read(Key::Settings(id.clone()))
    }

    async fn write_settings(&self, id: &SettingsId, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash> {
        self.write(Key::Settings(id.clone()), bytes, expect)
    }
}

impl ChangeEvents for MemoryStorage {
    fn changes(&self) -> ChangeStream {
        let (sender, receiver) = mpsc::unbounded();
        self.subscribers.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(sender);
        receiver.boxed_local()
    }
}

impl ArchiveImport for MemoryStorage {
    async fn import_archive(&self, zip: &[u8]) -> Result<usize> {
        let Some(reader) = self.archive_reader else {
            return Err(StorageError::Other("this library cannot be replaced from a zip file".to_owned()));
        };
        let files = reader(zip).map_err(StorageError::Other)?;
        let mut documents = self.lock_documents();
        documents.clear();
        let mut written = 0;
        for (path, bytes) in files {
            let Some((folder, rest)) = path.split_once('/') else { continue };
            let Some(key) = key_in_folder(folder, rest) else { continue };
            if check_path(rest).is_ok() {
                documents.insert(key, Stored::new(bytes));
                written += 1;
            }
        }
        Ok(written)
    }
}
