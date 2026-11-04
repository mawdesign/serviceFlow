//! Seam 3: the storage contract suite. Written once, run against every storage implementation.
//!
//! Instantiate it in an implementation's integration tests with a factory expression that makes a
//! fresh, empty storage:
//!
//! ```ignore
//! serviceflow_testkit::storage_contract!(serviceflow_storage_memory::MemoryStorage::new());
//! ```
//!
//! Each check is a public `async fn` here taking the storage under test; the macro wraps every
//! one in a `#[test]`. To add a check: write the `async fn`, then add its name to the list in
//! [`storage_contract!`](crate::storage_contract).

use std::time::{Duration, Instant};

use futures::{FutureExt as _, StreamExt as _};
use serviceflow_storage::{
    AssetId, ChangeEvent, ChangeKind, ChangeStream, ContentHash, DocumentId, Expect, Key, SetId, SettingsId, SongId, Storage, StorageError,
};

const SONG: &[u8] = b"<song><title>Contract</title></song>";

pub async fn a_written_song_reads_back_with_the_same_bytes_and_hash(storage: impl Storage) {
    let id = SongId::new("Contract Song");
    let hash = storage.write_song(&id, SONG.to_vec(), Expect::Any).await.unwrap();
    let stored = storage.read_song(&id).await.unwrap();
    assert_eq!(stored.bytes, SONG);
    assert_eq!(stored.hash, hash);
    assert_eq!(hash, ContentHash::of(SONG));
}

pub async fn written_songs_are_listed_sorted_with_their_category(storage: impl Storage) {
    for id in ["Zebra", "Hymns/Amazing Grace", "Abide With Me"] {
        storage.write_song(&SongId::new(id), SONG.to_vec(), Expect::Any).await.unwrap();
    }
    let ids = storage.list_songs().await.unwrap();
    assert_eq!(ids, ["Abide With Me", "Hymns/Amazing Grace", "Zebra"].map(SongId::new));
}

pub async fn reading_a_missing_song_is_not_found(storage: impl Storage) {
    let id = SongId::new("Nowhere");
    assert_eq!(storage.read_song(&id).await, Err(StorageError::NotFound(Key::Song(id))));
}

pub async fn a_write_based_on_a_stale_hash_is_a_conflict_and_changes_nothing(storage: impl Storage) {
    let id = SongId::new("Shared");
    let first = storage.write_song(&id, b"first".to_vec(), Expect::Any).await.unwrap();
    storage.write_song(&id, b"someone else".to_vec(), Expect::Hash(first)).await.unwrap();
    let stale = storage.write_song(&id, b"mine".to_vec(), Expect::Hash(first)).await;
    assert_eq!(stale, Err(StorageError::Conflict(Key::Song(id.clone()))));
    assert_eq!(storage.read_song(&id).await.unwrap().bytes, b"someone else");
}

pub async fn a_write_based_on_the_current_hash_replaces_the_song(storage: impl Storage) {
    let id = SongId::new("Saved");
    let first = storage.write_song(&id, b"first".to_vec(), Expect::Any).await.unwrap();
    let second = storage.write_song(&id, b"second".to_vec(), Expect::Hash(first)).await.unwrap();
    assert_eq!(second, ContentHash::of(b"second"));
    assert_eq!(storage.read_song(&id).await.unwrap().bytes, b"second");
    // And again with the hash the save returned.
    storage.write_song(&id, b"third".to_vec(), Expect::Hash(second)).await.unwrap();
}

pub async fn a_write_based_on_a_hash_of_a_removed_song_is_a_conflict(storage: impl Storage) {
    let id = SongId::new("Gone");
    let result = storage.write_song(&id, SONG.to_vec(), Expect::Hash(ContentHash::of(SONG))).await;
    assert_eq!(result, Err(StorageError::Conflict(Key::Song(id.clone()))));
    assert_eq!(storage.read_song(&id).await, Err(StorageError::NotFound(Key::Song(id))));
}

pub async fn creating_a_song_that_already_exists_is_a_conflict(storage: impl Storage) {
    let id = SongId::new("Taken");
    storage.write_song(&id, SONG.to_vec(), Expect::Absent).await.unwrap();
    assert_eq!(storage.write_song(&id, SONG.to_vec(), Expect::Absent).await, Err(StorageError::Conflict(Key::Song(id))));
}

pub async fn writing_a_song_emits_a_change_event(storage: impl Storage) {
    let mut changes = storage.changes();
    let id = SongId::new("Announced");
    storage.write_song(&id, SONG.to_vec(), Expect::Any).await.unwrap();
    assert_eq!(changes.next().await, Some(ChangeEvent { key: Key::Song(id), kind: ChangeKind::Written }));
}

pub async fn files_with_or_without_an_extension_are_all_songs(storage: impl Storage) {
    for id in ["No Extension", "With.txt", "Hymns/Dotted.name.xml", "Hymns/Plain"] {
        storage.write_song(&SongId::new(id), SONG.to_vec(), Expect::Any).await.unwrap();
    }
    let ids = storage.list_songs().await.unwrap();
    assert_eq!(ids, ["Hymns/Dotted.name.xml", "Hymns/Plain", "No Extension", "With.txt"].map(SongId::new));
}

pub async fn names_starting_with_dot_or_underscore_are_not_songs(storage: impl Storage) {
    for id in [".hidden", "_draft", "Hymns/.DS_Store", "Hymns/_old", "_Archive/Song", ".Trash/Song", "Hymns/Kept", "Kept"] {
        storage.write_song(&SongId::new(id), SONG.to_vec(), Expect::Any).await.unwrap();
    }
    let ids = storage.list_songs().await.unwrap();
    assert_eq!(ids, ["Hymns/Kept", "Kept"].map(SongId::new));
}

pub async fn song_ids_cannot_leave_the_songs_folder(storage: impl Storage) {
    for id in ["../escape", "a/../../escape", "/absolute", "a//b", "", "a\\b", "C:evil"] {
        let result = storage.write_song(&SongId::new(id), SONG.to_vec(), Expect::Any).await;
        assert!(matches!(result, Err(StorageError::Other(_))), "{id:?}: {result:?}");
        assert!(matches!(storage.read_song(&SongId::new(id)).await, Err(StorageError::Other(_))), "{id:?}");
    }
    assert_eq!(storage.list_songs().await.unwrap(), []);
}

pub async fn sets_round_trip_and_list_sorted(storage: impl Storage) {
    let hash = storage.write_set(&SetId::new("Sunday"), b"b".to_vec(), Expect::Any).await.unwrap();
    storage.write_set(&SetId::new("Easter"), b"a".to_vec(), Expect::Any).await.unwrap();
    storage.write_set(&SetId::new(".hidden"), b"x".to_vec(), Expect::Any).await.unwrap();
    assert_eq!(storage.list_sets().await.unwrap(), ["Easter", "Sunday"].map(SetId::new));
    let stored = storage.read_set(&SetId::new("Sunday")).await.unwrap();
    assert_eq!((stored.bytes, stored.hash), (b"b".to_vec(), hash));
    let id = SetId::new("Nope");
    assert_eq!(storage.read_set(&id).await, Err(StorageError::NotFound(Key::Set(id))));
}

pub async fn assets_keep_their_folders_and_stale_writes_conflict(storage: impl Storage) {
    let id = AssetId("Slides/bg.png".into());
    let bytes = vec![0u8, 255, 10, 13, 0];
    let first = storage.write_asset(&id, bytes.clone(), Expect::Any).await.unwrap();
    storage.write_asset(&AssetId("a.jpg".into()), vec![1], Expect::Any).await.unwrap();
    assert_eq!(storage.list_assets().await.unwrap(), vec![id.clone(), AssetId("a.jpg".into())]);
    assert_eq!(storage.read_asset(&id).await.unwrap().bytes, bytes);
    storage.write_asset(&id, vec![2], Expect::Hash(first)).await.unwrap();
    assert_eq!(storage.write_asset(&id, vec![3], Expect::Hash(first)).await, Err(StorageError::Conflict(Key::Asset(id))));
}

pub async fn settings_round_trip_and_a_missing_file_is_not_found(storage: impl Storage) {
    let id = SettingsId("PresentSettings".into());
    assert_eq!(storage.read_settings(&id).await, Err(StorageError::NotFound(Key::Settings(id.clone()))));
    let hash = storage.write_settings(&id, b"<settings/>".to_vec(), Expect::Absent).await.unwrap();
    let stored = storage.read_settings(&id).await.unwrap();
    assert_eq!((stored.bytes, stored.hash), (b"<settings/>".to_vec(), hash));
}

pub async fn sidecars_are_optional_and_never_appear_as_songs_or_sets(storage: impl Storage) {
    let song = DocumentId::Song(SongId::new("Hymns/Amazing Grace"));
    let set = DocumentId::Set(SetId::new("Sunday"));
    assert_eq!(storage.read_sidecar(&song).await, Ok(None));
    let hash = storage.write_sidecar(&song, b"{\"key\":2}".to_vec(), Expect::Absent).await.unwrap();
    storage.write_sidecar(&set, b"{}".to_vec(), Expect::Any).await.unwrap();
    let stored = storage.read_sidecar(&song).await.unwrap().unwrap();
    assert_eq!((stored.bytes, stored.hash), (b"{\"key\":2}".to_vec(), hash));
    assert_eq!(storage.write_sidecar(&song, b"x".to_vec(), Expect::Absent).await, Err(StorageError::Conflict(Key::Sidecar(song))));
    assert_eq!(storage.list_songs().await.unwrap(), []);
    assert_eq!(storage.list_sets().await.unwrap(), []);
}

pub async fn a_sidecar_write_based_on_the_current_hash_replaces_it_and_a_stale_one_conflicts(storage: impl Storage) {
    let set = DocumentId::Set(SetId::new("Sunday"));
    let first = storage.write_sidecar(&set, b"version = 1\n".to_vec(), Expect::Absent).await.unwrap();
    let second = storage.write_sidecar(&set, b"version = 2\n".to_vec(), Expect::Hash(first)).await.unwrap();
    assert_ne!(first, second);
    assert_eq!(storage.read_sidecar(&set).await.unwrap().unwrap().bytes, b"version = 2\n");
    assert_eq!(storage.write_sidecar(&set, b"late".to_vec(), Expect::Hash(first)).await, Err(StorageError::Conflict(Key::Sidecar(set.clone()))));
    assert_eq!(storage.read_sidecar(&set).await.unwrap().unwrap().bytes, b"version = 2\n");
}

pub async fn a_song_sidecar_and_a_set_sidecar_with_the_same_name_are_different_documents(storage: impl Storage) {
    let song = DocumentId::Song(SongId::new("Same"));
    let set = DocumentId::Set(SetId::new("Same"));
    storage.write_sidecar(&song, b"song".to_vec(), Expect::Any).await.unwrap();
    assert_eq!(storage.read_sidecar(&set).await, Ok(None));
    storage.write_sidecar(&set, b"set".to_vec(), Expect::Any).await.unwrap();
    assert_eq!(storage.read_sidecar(&song).await.unwrap().unwrap().bytes, b"song");
    assert_eq!(storage.read_sidecar(&set).await.unwrap().unwrap().bytes, b"set");
}

pub async fn sidecar_owners_cannot_leave_the_sidecar_folder(storage: impl Storage) {
    for id in ["../escape", "a/../../escape", "/absolute", ""] {
        let owner = DocumentId::Set(SetId::new(id));
        assert!(matches!(storage.write_sidecar(&owner, b"x".to_vec(), Expect::Any).await, Err(StorageError::Other(_))), "{id:?}");
        assert!(matches!(storage.read_sidecar(&owner).await, Err(StorageError::Other(_))), "{id:?}");
    }
}

pub async fn writing_a_sidecar_emits_a_change_event(storage: impl Storage) {
    let mut changes = storage.changes();
    let owner = DocumentId::Set(SetId::new("Announced"));
    storage.write_sidecar(&owner, b"x".to_vec(), Expect::Any).await.unwrap();
    assert_eq!(changes.next().await, Some(ChangeEvent { key: Key::Sidecar(owner), kind: ChangeKind::Written }));
}

pub async fn a_song_and_a_set_with_the_same_name_are_different_documents(storage: impl Storage) {
    storage.write_song(&SongId::new("Same"), b"song".to_vec(), Expect::Any).await.unwrap();
    storage.write_set(&SetId::new("Same"), b"set".to_vec(), Expect::Any).await.unwrap();
    assert_eq!(storage.read_song(&SongId::new("Same")).await.unwrap().bytes, b"song");
    assert_eq!(storage.read_set(&SetId::new("Same")).await.unwrap().bytes, b"set");
}

/// How a test changes storage "from outside": another program, another serviceFlow instance or a
/// sync process, without going through the storage API. The filesystem storage edits files with
/// `std::fs`; the memory storage has no outside, so it writes through its own API.
pub struct Outside<S> {
    pub write: fn(&S, &Key, &[u8]),
    pub remove: fn(&S, &Key),
}

/// Wait for `n` change events (any order), giving up after a few seconds.
fn next_changes(changes: &mut ChangeStream, n: usize) -> Vec<ChangeEvent> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut found = Vec::new();
    while found.len() < n {
        match changes.next().now_or_never() {
            Some(Some(event)) => found.push(event),
            Some(None) => panic!("the change stream ended; got {found:?}"),
            None => {
                assert!(Instant::now() < deadline, "waited for {n} change events, got {found:?}");
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
    found.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    found
}

/// Assert that nothing more is announced for a while (longer than any debounce in the tests).
fn no_more_changes(changes: &mut ChangeStream) {
    std::thread::sleep(Duration::from_millis(400));
    let extra = changes.next().now_or_never();
    assert!(extra.is_none(), "unexpected extra change event: {extra:?}");
}

fn written(key: Key) -> ChangeEvent {
    ChangeEvent { key, kind: ChangeKind::Written }
}

fn removed(key: Key) -> ChangeEvent {
    ChangeEvent { key, kind: ChangeKind::Removed }
}

pub async fn a_song_created_modified_and_deleted_from_outside_is_announced_each_time<S: Storage>(storage: S, outside: Outside<S>) {
    let mut changes = storage.changes();
    let key = Key::Song(SongId::new("Hymns/Outside"));
    (outside.write)(&storage, &key, b"<song>one</song>");
    assert_eq!(next_changes(&mut changes, 1), [written(key.clone())]);
    no_more_changes(&mut changes);
    (outside.write)(&storage, &key, b"<song>one, edited</song>");
    assert_eq!(next_changes(&mut changes, 1), [written(key.clone())]);
    assert_eq!(storage.read_song(&SongId::new("Hymns/Outside")).await.unwrap().bytes, b"<song>one, edited</song>");
    (outside.remove)(&storage, &key);
    assert_eq!(next_changes(&mut changes, 1), [removed(key.clone())]);
    no_more_changes(&mut changes);
    assert_eq!(storage.list_songs().await.unwrap(), []);
}

pub async fn a_rename_from_outside_is_a_removal_and_a_creation<S: Storage>(storage: S, outside: Outside<S>) {
    let old = Key::Song(SongId::new("Old Name"));
    let new = Key::Song(SongId::new("New Name"));
    (outside.write)(&storage, &old, b"<song/>");
    let mut changes = storage.changes();
    (outside.remove)(&storage, &old);
    (outside.write)(&storage, &new, b"<song/>");
    let mut expected = vec![removed(old), written(new)];
    expected.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    assert_eq!(next_changes(&mut changes, 2), expected);
}

pub async fn sets_and_settings_changed_from_outside_are_announced<S: Storage>(storage: S, outside: Outside<S>) {
    let mut changes = storage.changes();
    let set = Key::Set(SetId::new("Sunday"));
    let settings = Key::Settings(SettingsId("PresentSettings".into()));
    (outside.write)(&storage, &set, b"<set/>");
    (outside.write)(&storage, &settings, b"<settings/>");
    let mut expected = vec![written(set.clone()), written(settings)];
    expected.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    assert_eq!(next_changes(&mut changes, 2), expected);
    (outside.remove)(&storage, &set);
    assert_eq!(next_changes(&mut changes, 1), [removed(set)]);
}

pub async fn deleting_a_song_removes_it_and_emits_a_removed_event(storage: impl Storage) {
    let id = SongId::new("Hymns/Doomed");
    let hash = storage.write_song(&id, SONG.to_vec(), Expect::Any).await.unwrap();
    storage.write_song(&SongId::new("Kept"), SONG.to_vec(), Expect::Any).await.unwrap();
    let mut changes = storage.changes();
    storage.delete_song(&id, Expect::Hash(hash)).await.unwrap();
    assert_eq!(changes.next().await, Some(ChangeEvent { key: Key::Song(id.clone()), kind: ChangeKind::Removed }));
    assert_eq!(storage.read_song(&id).await, Err(StorageError::NotFound(Key::Song(id))));
    assert_eq!(storage.list_songs().await.unwrap(), [SongId::new("Kept")]);
}

pub async fn deleting_a_song_with_a_stale_hash_is_a_conflict_and_keeps_it(storage: impl Storage) {
    let id = SongId::new("Shared");
    let first = storage.write_song(&id, b"first".to_vec(), Expect::Any).await.unwrap();
    storage.write_song(&id, b"someone else".to_vec(), Expect::Any).await.unwrap();
    assert_eq!(storage.delete_song(&id, Expect::Hash(first)).await, Err(StorageError::Conflict(Key::Song(id.clone()))));
    assert_eq!(storage.delete_song(&id, Expect::Absent).await, Err(StorageError::Conflict(Key::Song(id.clone()))));
    assert_eq!(storage.read_song(&id).await.unwrap().bytes, b"someone else");
}

pub async fn deleting_a_missing_song_is_not_found(storage: impl Storage) {
    let id = SongId::new("Nowhere");
    assert_eq!(storage.delete_song(&id, Expect::Any).await, Err(StorageError::NotFound(Key::Song(id.clone()))));
    assert_eq!(storage.delete_song(&id, Expect::Hash(ContentHash::of(SONG))).await, Err(StorageError::NotFound(Key::Song(id))));
}

pub async fn renaming_a_song_moves_its_bytes_to_another_category(storage: impl Storage) {
    let from = SongId::new("Old Name");
    let to = SongId::new("Hymns/New Name.xml");
    let hash = storage.write_song(&from, SONG.to_vec(), Expect::Any).await.unwrap();
    let mut changes = storage.changes();
    assert_eq!(storage.rename_song(&from, &to, Expect::Hash(hash)).await, Ok(hash));
    assert_eq!(changes.next().await, Some(ChangeEvent { key: Key::Song(from.clone()), kind: ChangeKind::Removed }));
    assert_eq!(changes.next().await, Some(ChangeEvent { key: Key::Song(to.clone()), kind: ChangeKind::Written }));
    assert_eq!(storage.read_song(&from).await, Err(StorageError::NotFound(Key::Song(from))));
    let stored = storage.read_song(&to).await.unwrap();
    assert_eq!((stored.bytes, stored.hash), (SONG.to_vec(), hash));
    assert_eq!(storage.list_songs().await.unwrap(), [to]);
}

pub async fn renaming_a_song_onto_an_existing_one_is_a_conflict_and_changes_nothing(storage: impl Storage) {
    let from = SongId::new("Mine");
    let to = SongId::new("Theirs");
    storage.write_song(&from, b"mine".to_vec(), Expect::Any).await.unwrap();
    storage.write_song(&to, b"theirs".to_vec(), Expect::Any).await.unwrap();
    assert_eq!(storage.rename_song(&from, &to, Expect::Any).await, Err(StorageError::Conflict(Key::Song(to.clone()))));
    assert_eq!(storage.read_song(&from).await.unwrap().bytes, b"mine");
    assert_eq!(storage.read_song(&to).await.unwrap().bytes, b"theirs");
}

pub async fn renaming_a_song_with_a_stale_hash_is_a_conflict_and_a_missing_one_is_not_found(storage: impl Storage) {
    let from = SongId::new("Shared");
    let to = SongId::new("Moved");
    let first = storage.write_song(&from, b"first".to_vec(), Expect::Any).await.unwrap();
    storage.write_song(&from, b"second".to_vec(), Expect::Any).await.unwrap();
    assert_eq!(storage.rename_song(&from, &to, Expect::Hash(first)).await, Err(StorageError::Conflict(Key::Song(from.clone()))));
    assert_eq!(storage.read_song(&to).await, Err(StorageError::NotFound(Key::Song(to.clone()))));
    let missing = SongId::new("Nowhere");
    assert_eq!(storage.rename_song(&missing, &to, Expect::Any).await, Err(StorageError::NotFound(Key::Song(missing))));
    let escape = storage.rename_song(&from, &SongId::new("../escape"), Expect::Any).await;
    assert!(matches!(escape, Err(StorageError::Other(_))), "{escape:?}");
    assert_eq!(storage.list_songs().await.unwrap(), [from]);
}

pub async fn sets_can_be_renamed_and_deleted(storage: impl Storage) {
    let from = SetId::new("Draft");
    let to = SetId::new("Sunday");
    let hash = storage.write_set(&from, b"set".to_vec(), Expect::Any).await.unwrap();
    storage.write_set(&SetId::new("Other"), b"other".to_vec(), Expect::Any).await.unwrap();
    assert_eq!(storage.rename_set(&from, &SetId::new("Other"), Expect::Any).await, Err(StorageError::Conflict(Key::Set(SetId::new("Other")))));
    assert_eq!(storage.rename_set(&from, &to, Expect::Hash(hash)).await, Ok(hash));
    assert_eq!(storage.list_sets().await.unwrap(), [SetId::new("Other"), to.clone()]);
    assert_eq!(storage.delete_set(&to, Expect::Hash(ContentHash::of(b"stale"))).await, Err(StorageError::Conflict(Key::Set(to.clone()))));
    storage.delete_set(&to, Expect::Hash(hash)).await.unwrap();
    assert_eq!(storage.delete_set(&to, Expect::Any).await, Err(StorageError::NotFound(Key::Set(to))));
    assert_eq!(storage.list_sets().await.unwrap(), [SetId::new("Other")]);
}

/// Generate one `#[test]` per contract check, each given a fresh storage from `$make`.
#[macro_export]
macro_rules! storage_contract {
    ($make:expr) => {
        $crate::storage_contract!(@tests $make;
            a_written_song_reads_back_with_the_same_bytes_and_hash,
            a_write_based_on_the_current_hash_replaces_the_song,
            a_write_based_on_a_hash_of_a_removed_song_is_a_conflict,
            written_songs_are_listed_sorted_with_their_category,
            reading_a_missing_song_is_not_found,
            a_write_based_on_a_stale_hash_is_a_conflict_and_changes_nothing,
            creating_a_song_that_already_exists_is_a_conflict,
            writing_a_song_emits_a_change_event,
            files_with_or_without_an_extension_are_all_songs,
            names_starting_with_dot_or_underscore_are_not_songs,
            song_ids_cannot_leave_the_songs_folder,
            sets_round_trip_and_list_sorted,
            assets_keep_their_folders_and_stale_writes_conflict,
            settings_round_trip_and_a_missing_file_is_not_found,
            sidecars_are_optional_and_never_appear_as_songs_or_sets,
            a_song_and_a_set_with_the_same_name_are_different_documents,
            a_sidecar_write_based_on_the_current_hash_replaces_it_and_a_stale_one_conflicts,
            a_song_sidecar_and_a_set_sidecar_with_the_same_name_are_different_documents,
            sidecar_owners_cannot_leave_the_sidecar_folder,
            writing_a_sidecar_emits_a_change_event,
            deleting_a_song_removes_it_and_emits_a_removed_event,
            deleting_a_song_with_a_stale_hash_is_a_conflict_and_keeps_it,
            deleting_a_missing_song_is_not_found,
            renaming_a_song_moves_its_bytes_to_another_category,
            renaming_a_song_onto_an_existing_one_is_a_conflict_and_changes_nothing,
            renaming_a_song_with_a_stale_hash_is_a_conflict_and_a_missing_one_is_not_found,
            sets_can_be_renamed_and_deleted,
        );
    };
    ($make:expr, outside: $write:expr, $remove:expr) => {
        $crate::storage_contract!($make);
        mod storage_contract_outside {
            #[allow(unused_imports)]
            use super::*;
            $crate::storage_contract!(@outside $make, $write, $remove;
                a_song_created_modified_and_deleted_from_outside_is_announced_each_time,
                a_rename_from_outside_is_a_removal_and_a_creation,
                sets_and_settings_changed_from_outside_are_announced,
            );
        }
    };
    (@outside $make:expr, $write:expr, $remove:expr; $($check:ident),* $(,)?) => {
        $(
            #[test]
            fn $check() {
                $crate::block_on($crate::storage_contract::$check(
                    $make,
                    $crate::storage_contract::Outside { write: $write, remove: $remove },
                ));
            }
        )*
    };
    (@tests $make:expr; $($check:ident),* $(,)?) => {
        mod storage_contract {
            #[allow(unused_imports)]
            use super::*;
            $(
                #[test]
                fn $check() {
                    $crate::block_on($crate::storage_contract::$check($make));
                }
            )*
        }
    };
}
