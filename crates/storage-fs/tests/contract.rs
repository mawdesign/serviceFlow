//! The filesystem storage against the shared storage contract (Seam 3), plus what only a real
//! folder can show: files changed behind its back, crash leftovers, sidecar placement.

use serviceflow_storage::{DocumentId, Expect, Key, SetId, SongId, StorageError};
use serviceflow_storage::{SetRepository as _, SidecarRepository as _, SongRepository as _};
use serviceflow_storage_fs::FsStorage;
use serviceflow_testkit::{block_on, fixture_song, opensong_data_root, temp_dir};

/// An empty data root in a temporary folder that disappears with the storage.
fn fresh() -> FsStorage {
    let dir = temp_dir();
    FsStorage::open(dir.path()).unwrap().with_poll_interval(FAST_POLL).keeping_alive(dir)
}

/// Poll quickly in tests; real use polls about once a second.
const FAST_POLL: std::time::Duration = std::time::Duration::from_millis(20);

/// The file a key lives in, for editing it the way another program would.
fn file_of(storage: &FsStorage, key: &Key) -> std::path::PathBuf {
    let (folder, path) = match key {
        Key::Song(SongId(p)) => ("Songs", p),
        Key::Set(SetId(p)) => ("Sets", p),
        Key::Settings(serviceflow_storage::SettingsId(p)) => ("Settings", p),
        other => panic!("not a key the tests edit: {other:?}"),
    };
    storage.root().join(folder).join(path)
}

serviceflow_testkit::storage_contract!(
    fresh(),
    outside: |s: &FsStorage, key: &Key, bytes: &[u8]| {
        let file = file_of(s, key);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, bytes).unwrap();
    },
    |s: &FsStorage, key: &Key| std::fs::remove_file(file_of(s, key)).unwrap()
);

/// A copy of the OpenSong fixtures data root on disk.
fn fixtures_copy() -> (FsStorage, std::path::PathBuf) {
    fn copy(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                copy(&entry.path(), &to.join(entry.file_name()));
            } else {
                std::fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
            }
        }
    }
    let dir = temp_dir();
    copy(&opensong_data_root(), dir.path());
    let path = dir.path().to_path_buf();
    (FsStorage::open(&path).unwrap().keeping_alive(dir), path)
}

#[test]
fn an_existing_opensong_data_root_lists_its_songs_without_conversion() {
    let storage = FsStorage::open(&opensong_data_root()).unwrap();
    let songs = block_on(storage.list_songs()).unwrap();
    assert_eq!(
        songs,
        [
            "Amazing Grace HOF#50",
            "God Defend NZ - National Anthem",
            "How Deep the Fathers Love + Chinese",
            "It Is Well With My Soul",
            "Just A Closer Walk With Thee",
            "一件礼物 A Gift",
        ]
        .map(SongId::new)
    );
    let stored = block_on(storage.read_song(&SongId::new("It Is Well With My Soul"))).unwrap();
    assert_eq!(stored.bytes, fixture_song("It Is Well With My Soul"));
}

#[test]
fn files_added_behind_its_back_are_listed_and_hidden_ones_are_not() {
    let (storage, root) = fixtures_copy();
    std::fs::write(root.join("Songs/Added Later"), "<song/>").unwrap();
    std::fs::write(root.join("Songs/_notes"), "x").unwrap();
    std::fs::write(root.join("Songs/.DS_Store"), "x").unwrap();
    std::fs::create_dir_all(root.join("Songs/Choruses")).unwrap();
    std::fs::write(root.join("Songs/Choruses/Short.txt"), "<song/>").unwrap();
    std::fs::create_dir_all(root.join("Songs/_Retired")).unwrap();
    std::fs::write(root.join("Songs/_Retired/Old"), "<song/>").unwrap();
    let songs = block_on(storage.list_songs()).unwrap();
    assert!(songs.contains(&SongId::new("Added Later")));
    assert!(songs.contains(&SongId::new("Choruses/Short.txt")));
    assert!(songs.iter().all(|s| !s.as_str().contains("_notes") && !s.as_str().contains(".DS_Store") && !s.as_str().contains("_Retired")));
    assert_eq!(songs.len(), 8);
}

/// Every song listed can be opened, including names OpenSong on macOS or Linux writes with a
/// colon or backslash in them.
#[cfg(unix)]
#[test]
fn files_named_with_a_colon_or_backslash_behind_its_back_are_listed_and_readable() {
    let (storage, root) = fixtures_copy();
    std::fs::write(root.join("Songs/Psalm 23: Shepherd"), "<song/>").unwrap();
    std::fs::write(root.join("Songs/AC\\DC"), "<song/>").unwrap();
    let songs = block_on(storage.list_songs()).unwrap();
    assert!(songs.contains(&SongId::new("Psalm 23: Shepherd")));
    assert!(songs.contains(&SongId::new("AC\\DC")));
    for id in &songs {
        assert!(block_on(storage.read_song(id)).is_ok(), "{id:?} is listed but cannot be read");
    }
}

#[test]
fn a_write_after_the_file_changed_on_disk_is_a_conflict() {
    let (storage, root) = fixtures_copy();
    let id = SongId::new("It Is Well With My Soul");
    let read = block_on(storage.read_song(&id)).unwrap();
    std::fs::write(root.join("Songs/It Is Well With My Soul"), "edited in OpenSong").unwrap();
    let result = block_on(storage.write_song(&id, b"mine".to_vec(), Expect::Hash(read.hash)));
    assert_eq!(result, Err(StorageError::Conflict(Key::Song(id))));
    assert_eq!(std::fs::read(root.join("Songs/It Is Well With My Soul")).unwrap(), b"edited in OpenSong");
}

#[test]
fn writing_replaces_the_file_atomically_and_leaves_no_staging_files() {
    let (storage, root) = fixtures_copy();
    let id = SongId::new("Hymns/New Song");
    block_on(storage.write_song(&id, b"one".to_vec(), Expect::Absent)).unwrap();
    block_on(storage.write_song(&id, b"two".to_vec(), Expect::Any)).unwrap();
    assert_eq!(std::fs::read(root.join("Songs/Hymns/New Song")).unwrap(), b"two");
    let names: Vec<_> = std::fs::read_dir(root.join("Songs/Hymns")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, ["New Song"]);
}

#[test]
fn staging_files_left_by_a_crash_are_not_songs() {
    let (storage, root) = fixtures_copy();
    std::fs::write(root.join("Songs/.It Is Well With My Soul.4242-0.craft_tmp"), "half").unwrap();
    assert_eq!(block_on(storage.list_songs()).unwrap().len(), 6);
}

#[test]
fn sidecars_live_in_a_dot_folder_at_the_data_root() {
    let (storage, root) = fixtures_copy();
    let owner = DocumentId::Song(SongId::new("It Is Well With My Soul"));
    block_on(storage.write_sidecar(&owner, b"{}".to_vec(), Expect::Any)).unwrap();
    assert_eq!(std::fs::read(root.join(".serviceflow/Songs/It Is Well With My Soul.sidecar")).unwrap(), b"{}");
    let set = DocumentId::Set(SetId::new("Sunday"));
    block_on(storage.write_sidecar(&set, b"{}".to_vec(), Expect::Any)).unwrap();
    assert!(root.join(".serviceflow/Sets/Sunday.sidecar").is_file());
    // The sidecar is not a set: only the fixture sets are listed.
    assert_eq!(block_on(storage.list_sets()).unwrap(), [SetId::new("Example"), SetId::new("Notices and Pictures"), SetId::new("Sunday Service")]);
}

#[test]
fn a_missing_songs_folder_is_an_empty_library_and_a_file_is_not_a_data_root() {
    let dir = temp_dir();
    let storage = FsStorage::open(dir.path()).unwrap();
    assert_eq!(block_on(storage.list_songs()).unwrap(), []);
    let file = dir.path().join("plain file");
    std::fs::write(&file, "x").unwrap();
    assert!(FsStorage::open(&file).is_err());
    assert!(FsStorage::open(&dir.path().join("missing")).is_err());
}

#[test]
fn a_data_root_is_recognised_by_its_songs_folder() {
    assert!(FsStorage::looks_like_data_root(&opensong_data_root()));
    assert!(!FsStorage::looks_like_data_root(temp_dir().path()));
}

// Watching (what only a real folder shows).

use futures::{FutureExt as _, StreamExt as _};
use serviceflow_storage::{ChangeEvent, ChangeEvents as _, ChangeKind, ChangeStream};

const MS: fn(u64) -> std::time::Duration = std::time::Duration::from_millis;

/// Events that arrive within `wait`, in order.
fn events_within(changes: &mut ChangeStream, wait: std::time::Duration) -> Vec<ChangeEvent> {
    let deadline = std::time::Instant::now() + wait;
    let mut found = Vec::new();
    while std::time::Instant::now() < deadline {
        while let Some(Some(event)) = changes.next().now_or_never() {
            found.push(event);
        }
        std::thread::sleep(MS(5));
    }
    found
}

#[test]
fn a_burst_of_writes_to_one_file_is_announced_once_when_it_settles() {
    let (storage, root) = fixtures_copy();
    let storage = storage.with_poll_interval(MS(250));
    let mut changes = storage.changes();
    for n in 0..20 {
        std::fs::write(root.join("Songs/Slow Save"), "x".repeat(n + 1)).unwrap();
        std::thread::sleep(MS(3));
    }
    let events = events_within(&mut changes, MS(2000));
    assert_eq!(events, [ChangeEvent { key: Key::Song(SongId::new("Slow Save")), kind: ChangeKind::Written }]);
}

#[test]
fn hidden_files_staging_files_and_other_folders_are_not_announced() {
    let (storage, root) = fixtures_copy();
    let storage = storage.with_poll_interval(FAST_POLL);
    let mut changes = storage.changes();
    std::fs::write(root.join("Songs/.It Is Well With My Soul.1-0.craft_tmp"), "half").unwrap();
    std::fs::write(root.join("Songs/_scratch"), "x").unwrap();
    std::fs::write(root.join("Songs/.DS_Store"), "x").unwrap();
    std::fs::create_dir_all(root.join(".serviceflow/Songs")).unwrap();
    std::fs::write(root.join(".serviceflow/Songs/x.sidecar"), "{}").unwrap();
    std::fs::write(root.join("Notes.txt"), "x").unwrap();
    assert_eq!(events_within(&mut changes, MS(400)), []);
}

#[test]
fn this_storages_own_writes_are_announced_once() {
    let (storage, _root) = fixtures_copy();
    let storage = storage.with_poll_interval(FAST_POLL);
    let mut changes = storage.changes();
    let id = SongId::new("Mine");
    block_on(storage.write_song(&id, b"one".to_vec(), Expect::Any)).unwrap();
    block_on(storage.write_song(&id, b"two!".to_vec(), Expect::Any)).unwrap();
    let events = events_within(&mut changes, MS(400));
    assert_eq!(events, vec![ChangeEvent { key: Key::Song(id), kind: ChangeKind::Written }; 2]);
}

#[test]
fn a_data_root_that_disappears_is_not_a_mass_deletion() {
    let dir = temp_dir();
    std::fs::create_dir_all(dir.path().join("Songs")).unwrap();
    std::fs::write(dir.path().join("Songs/A"), "a").unwrap();
    let storage = FsStorage::open(dir.path()).unwrap().with_poll_interval(FAST_POLL);
    let mut changes = storage.changes();
    let moved = dir.path().with_extension("moved");
    std::fs::rename(dir.path(), &moved).unwrap();
    assert_eq!(events_within(&mut changes, MS(300)), []);
    std::fs::rename(&moved, dir.path()).unwrap();
}

#[test]
fn dropping_the_storage_ends_the_stream() {
    let dir = temp_dir();
    let storage = FsStorage::open(dir.path()).unwrap().with_poll_interval(FAST_POLL);
    let mut changes = storage.changes();
    drop(storage);
    let ended = (0..200).any(|_| {
        std::thread::sleep(MS(10));
        matches!(changes.next().now_or_never(), Some(None))
    });
    assert!(ended);
}

#[test]
fn renaming_moves_the_file_into_a_new_category_folder_and_deleting_removes_it() {
    let (storage, root) = fixtures_copy();
    let from = SongId::new("It Is Well With My Soul");
    let to = SongId::new("Hymns/It Is Well.xml");
    let hash = block_on(storage.rename_song(&from, &to, Expect::Any)).unwrap();
    assert!(!root.join("Songs/It Is Well With My Soul").exists());
    assert_eq!(std::fs::read(root.join("Songs/Hymns/It Is Well.xml")).unwrap(), fixture_song("It Is Well With My Soul"));
    block_on(storage.delete_song(&to, Expect::Hash(hash))).unwrap();
    assert!(!root.join("Songs/Hymns/It Is Well.xml").exists());
}

#[test]
fn a_rename_that_changes_only_letter_case_is_allowed() {
    let (storage, root) = fixtures_copy();
    let from = SongId::new("It Is Well With My Soul");
    let to = SongId::new("IT IS WELL WITH MY SOUL");
    block_on(storage.rename_song(&from, &to, Expect::Any)).unwrap();
    let names: Vec<_> = std::fs::read_dir(root.join("Songs")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert!(names.contains(&"IT IS WELL WITH MY SOUL".to_owned()), "{names:?}");
}
