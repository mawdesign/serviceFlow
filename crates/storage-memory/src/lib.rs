//! In-memory storage implementation, for tests and seeding from the OpenSong fixtures.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use futures::StreamExt as _;
use futures::channel::mpsc;
use serviceflow_storage::{
    AssetId, AssetRepository, ChangeEvent, ChangeEvents, ChangeKind, ChangeStream, ContentHash, DocumentId, Expect, Key, Result, SetId,
    SetRepository, SettingsId, SettingsRepository, SidecarRepository, SongId, SongRepository, StorageError, Stored, check_path, is_ignored_path,
    key_path,
};

/// Storage held in memory. Every write is announced on [`ChangeEvents::changes`].
#[derive(Default)]
pub struct MemoryStorage {
    documents: Mutex<BTreeMap<Key, Stored>>,
    subscribers: Mutex<Vec<mpsc::UnboundedSender<ChangeEvent>>>,
}

impl MemoryStorage {
    /// Empty storage.
    pub fn new() -> Self {
        Self::default()
    }

    /// Storage holding a copy of an OpenSong data root: `Songs/` (subfolders are categories),
    /// `Sets/`, `Settings/` and `Backgrounds/`. Names starting with `.` or `_` are skipped.
    pub fn from_data_root(root: &Path) -> std::io::Result<Self> {
        let storage = Self::new();
        {
            let mut documents = storage.lock_documents();
            type MakeKey = fn(String) -> Key;
            let folders: [(&str, MakeKey); 4] = [
                ("Songs", |p| Key::Song(SongId(p))),
                ("Sets", |p| Key::Set(SetId(p))),
                ("Settings", |p| Key::Settings(SettingsId(p))),
                ("Backgrounds", |p| Key::Asset(AssetId(p))),
            ];
            for (folder, key) in folders {
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
            let current = documents.get(&key).map(|s| s.hash);
            let allowed = match expect {
                Expect::Any => true,
                Expect::Absent => current.is_none(),
                Expect::Hash(expected) => current == Some(expected),
            };
            if !allowed {
                return Err(StorageError::Conflict(key));
            }
            documents.insert(key.clone(), stored);
        }
        self.announce(ChangeEvent { key, kind: ChangeKind::Written });
        Ok(hash)
    }

    /// Delete a document, announcing [`ChangeKind::Removed`] if it existed. The storage traits
    /// have no delete yet (#20/#21 add one); tests and the engine harness use this.
    pub fn remove(&self, key: &Key) {
        if self.lock_documents().remove(key).is_some() {
            self.announce(ChangeEvent { key: key.clone(), kind: ChangeKind::Removed });
        }
    }

    /// Write a document without a precondition, as if another program had.
    pub fn write_key(&self, key: &Key, bytes: &[u8]) {
        self.write(key.clone(), bytes.to_vec(), Expect::Any).expect("writing a document");
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
