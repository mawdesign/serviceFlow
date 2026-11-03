//! Style items and song backgrounds in the shell. Which style wins is tested through the engine
//! (`crates/core/tests/style_precedence.rs`) and the drawing in `crates/render/tests/background.rs`.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use serviceflow::ControlWindow;
use serviceflow_core::command::Command;
use serviceflow_core::ids::SetId;
use serviceflow_runtime::Driver;
use serviceflow_storage::{AssetId, AssetRepository as _, Expect, SetRepository as _, SongId, SongRepository as _};
use serviceflow_storage_memory::MemoryStorage;
use serviceflow_testkit::block_on;

const SONG: &str = r#"<song><title>Pic Song</title><lyrics>[V1]
 hello
[V2]
 world</lyrics><backgrounds resize="screen"><background verse="V1"><filename>red.png</filename></background></backgrounds></song>"#;

const SET: &str = r##"<set name="Styled"><slide_groups>
  <slide_group name="Make it red" type="style" action="new"><style><body font="Arial" color="#ff000000"/></style></slide_group>
  <slide_group name="Pic Song" type="song"/>
  <slide_group name="Back to normal" type="style" action="revert"/>
</slide_groups></set>"##;

fn window() -> ControlWindow<MemoryStorage> {
    let storage = MemoryStorage::new();
    block_on(storage.write_song(&SongId::new("Pic Song"), SONG.as_bytes().to_vec(), Expect::Any)).unwrap();
    block_on(storage.write_set(&serviceflow_storage::SetId::new("Styled"), SET.as_bytes().to_vec(), Expect::Any)).unwrap();
    block_on(storage.write_asset(&AssetId("red.png".into()), b"not really a picture".to_vec(), Expect::Any)).unwrap();
    let mut window = ControlWindow::new(Driver::new(storage));
    window.driver().send(Command::OpenSet(SetId::new("Styled")));
    window
}

#[test]
fn style_items_are_listed_and_a_song_item_shows_its_thumbnails_with_the_background() {
    let mut window = window();
    let mut harness = Harness::builder().allow_missing_glyphs().with_size(egui::vec2(1400., 900.)).build_ui(|ui| window.ui(ui));
    harness.run();
    harness.get_by_label("Make it red (style: new)");
    harness.get_by_label("Back to normal (style: revert)");
    harness.get_all_by_label("Pic Song").next().unwrap().click();
    harness.run();
    let textures = harness.ctx.tex_manager();
    let uploaded = textures.read().allocated().filter(|(_, meta)| meta.name.starts_with("slide thumbnail")).count();
    assert_eq!(uploaded, 2);
    drop(harness);
    // The driver read the background file the song names.
    let state = window.state();
    assert!(state.images.bytes("red.png").is_some(), "the background image was read from Backgrounds/");
}
