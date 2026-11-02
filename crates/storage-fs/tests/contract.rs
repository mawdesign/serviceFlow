//! The filesystem storage against the shared storage contract (Seam 3), plus what only a real
//! folder can show: files changed behind its back, crash leftovers, sidecar placement.

use serviceflow_storage::{DocumentId, Expect, Key, SetId, SongId, StorageError};
use serviceflow_storage::{SetRepository as _, SidecarRepository as _, SongRepository as _};
use serviceflow_storage_fs::FsStorage;
use serviceflow_testkit::{block_on, fixture_song, opensong_data_root, temp_dir};

/// An empty data root in a temporary folder that disappears with the storage.
fn fresh() -> FsStorage {
    let dir = temp_dir();
    FsStorage::open(dir.path()).unwrap().keeping_alive(dir)
}

serviceflow_testkit::storage_contract!(fresh());

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
    assert_eq!(block_on(storage.list_sets()).unwrap(), []);
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
