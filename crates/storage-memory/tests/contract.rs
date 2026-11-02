//! The in-memory storage against the shared storage contract (Seam 3), plus seeding.

use serviceflow_storage::{SettingsId, SettingsRepository as _, SongId, SongRepository as _};
use serviceflow_storage_memory::MemoryStorage;
use serviceflow_testkit::{block_on, fixture_song, opensong_data_root};

serviceflow_testkit::storage_contract!(MemoryStorage::new());

#[test]
fn seeding_from_the_fixtures_folder_lists_every_fixture_song() {
    let storage = MemoryStorage::from_data_root(&opensong_data_root()).unwrap();
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
}

#[test]
fn seeded_documents_hold_the_bytes_of_their_files() {
    let storage = MemoryStorage::from_data_root(&opensong_data_root()).unwrap();
    let song = block_on(storage.read_song(&SongId::new("It Is Well With My Soul"))).unwrap();
    assert_eq!(song.bytes, fixture_song("It Is Well With My Soul"));
    let settings = block_on(storage.read_settings(&SettingsId("PresentSettings".into()))).unwrap();
    assert_eq!(settings.bytes, std::fs::read(opensong_data_root().join("Settings/PresentSettings")).unwrap());
}
