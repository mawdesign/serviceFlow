//! Seam 1, the engine: custom slide items and image items are presented, styled, navigated and
//! (for images) flagged when a file is missing.

use serviceflow_core::command::{Command, ImportTrigger};
use serviceflow_core::ids::SetId;
use serviceflow_core::output::SlideKind;
use serviceflow_core::resolve::{ImageFile, StyleChoice};
use serviceflow_core::style::{Captions, ImageSource, Resize};
use serviceflow_testkit::EngineHarness;

const SONG: (&str, &str) = ("Alpha", "<song><title>Alpha</title><lyrics>[V1]\n a1\n[V2]\n a2</lyrics></song>");

const SET: &str = r#"<set name="S"><slide_groups>
  <slide_group name="Welcome" type="custom">
    <title>Welcome</title><subtitle>Sunday</subtitle><notes>say hi</notes>
    <slides><slide><body>Good morning
and welcome</body></slide><slide><body>-= Notices =-
Tea after</body></slide></slides>
  </slide_group>
  <slide_group name="Alpha" type="song"/>
  <slide_group name="Pictures" type="image" resize="body" keep_aspect="true">
    <title>Pictures</title><subtitle>Shown</subtitle>
    <slides><slide><filename>red.png</filename></slide><slide><filename>gone.png</filename></slide><slide><image>QUJD</image></slide></slides>
  </slide_group>
</slide_groups></set>"#;

const RED: &[u8] = b"RED";

fn open() -> EngineHarness {
    let mut harness = EngineHarness::with_data(&[SONG], &[("S", SET)], &[], &[("red.png", RED), ("Other/blue.png", b"BLUE"), ("notes.txt", b"x")]);
    harness.send(Command::RefreshLibrary);
    harness.send(Command::OpenSet(SetId::new("S")));
    harness
}

fn live_lines(harness: &EngineHarness) -> Option<Vec<String>> {
    harness.state().output.live.as_ref().map(|l| l.slide.lines.clone())
}

// Custom slides

#[test]
fn selecting_a_custom_item_shows_one_slide_per_body() {
    let mut harness = open();
    harness.send(Command::SelectItem(0));
    let state = harness.state();
    let song = state.song.as_ref().expect("the item's slides are open");
    assert!(song.id.is_set_item());
    assert_eq!(song.item_kind, Some(SlideKind::Custom));
    let lines: Vec<_> = song.slides.iter().map(|s| s.lines.clone()).collect();
    assert_eq!(lines, [vec!["Good morning", "and welcome"], vec!["-= Notices =-", "Tea after"]]);
    assert_eq!(song.in_flow, 2, "every slide of an item is in the flow");
    assert_eq!(state.set.as_ref().unwrap().selected, Some(0));
}

#[test]
fn custom_slides_go_live_with_the_items_title_and_subtitle() {
    let mut harness = open();
    harness.send(Command::SelectItem(0));
    harness.send(Command::GoLive { index: 1 });
    let state = harness.state();
    let live = state.output.live.as_ref().unwrap();
    assert_eq!(live.slide.lines, ["-= Notices =-", "Tea after"]);
    let captions = Captions::for_song(&live.source, &state.styles.default);
    assert_eq!(captions, Captions { title: "Welcome".into(), subtitle: "Sunday".into() });
    assert_eq!(live.resolved.style, StyleChoice::Default, "the current style");
}

#[test]
fn custom_slides_follow_the_style_stack_and_the_items_own_style() {
    let set = r#"<set><slide_groups>
      <slide_group name="push" type="style" action="new"><style><body font="Pushed" size="40"/></style></slide_group>
      <slide_group name="One" type="custom"><slides><slide><body>x</body></slide></slides></slide_group>
      <slide_group name="Two" type="custom"><style><body font="Own" size="40"/></style><slides><slide><body>y</body></slide></slides></slide_group>
      <slide_group name="back" type="style" action="revert"/>
      <slide_group name="Three" type="custom"><slides><slide><body>z</body></slide></slides></slide_group>
    </slide_groups></set>"#;
    let settings = r#"<present_settings><default_style><body font="Default" size="40"/></default_style></present_settings>"#;
    let mut harness = EngineHarness::with_data(&[], &[("S", set)], &[("PresentSettings", settings)], &[]);
    harness.send(Command::ImportOpenSongStyles { trigger: ImportTrigger::FirstRun });
    harness.send(Command::OpenSet(SetId::new("S")));
    let mut fonts = Vec::new();
    for item in [1, 2, 4] {
        harness.send(Command::SelectItem(item));
        let state = harness.state();
        fonts.push(state.open_resolved(0).unwrap().style(&state.styles).body.text.font.clone());
    }
    assert_eq!(fonts, ["Pushed", "Own", "Default"]);
}

#[test]
fn stepping_past_the_end_of_a_song_carries_on_into_the_next_custom_or_image_item() {
    let mut harness = open();
    harness.send(Command::SelectItem(0));
    harness.send(Command::GoLive { index: 1 });
    // Past the last custom slide: the song, its first slide.
    harness.send(Command::NextSlide);
    assert_eq!(harness.state().set.as_ref().unwrap().selected, Some(1));
    assert_eq!(live_lines(&harness), Some(vec!["a1".into()]));
    harness.send(Command::NextSlide);
    harness.send(Command::NextSlide);
    // Past the song: the image item's first slide.
    let state = harness.state();
    assert_eq!(state.set.as_ref().unwrap().selected, Some(2));
    assert_eq!(state.output.live.as_ref().unwrap().index, 0);
    // And back: the last slide of the song, then the last slide of the custom item.
    harness.send(Command::PreviousSlide);
    assert_eq!(live_lines(&harness), Some(vec!["a2".into()]));
    harness.send(Command::PreviousSlide);
    harness.send(Command::PreviousSlide);
    assert_eq!(harness.state().set.as_ref().unwrap().selected, Some(0));
    assert_eq!(live_lines(&harness), Some(vec!["-= Notices =-".into(), "Tea after".into()]));
}

#[test]
fn what_is_live_keeps_its_slide_while_another_item_is_browsed() {
    let mut harness = open();
    harness.send(Command::SelectItem(0));
    harness.send(Command::GoLive { index: 0 });
    harness.send(Command::SelectItem(1));
    assert_eq!(live_lines(&harness), Some(vec!["Good morning".into(), "and welcome".into()]));
}

// Image items

#[test]
fn an_image_item_has_one_slide_per_picture_with_its_fit() {
    let mut harness = open();
    harness.send(Command::SelectItem(2));
    let state = harness.state();
    let song = state.song.as_ref().unwrap();
    assert_eq!(song.item_kind, Some(SlideKind::Image));
    assert_eq!(song.slides.len(), 3);
    let backgrounds: Vec<_> = (0..3).map(|i| state.open_resolved(i).unwrap().background.unwrap()).collect();
    let images: Vec<_> = backgrounds.iter().map(|b| b.image.clone()).collect();
    assert_eq!(images, [ImageSource::File("red.png".into()), ImageSource::File("gone.png".into()), ImageSource::Embedded("QUJD".into())]);
    for bg in &backgrounds {
        assert_eq!((bg.resize, bg.keep_aspect, bg.as_text), (Resize::Body, true, false));
    }
    // A picture is shown on its own: no lines, no title or subtitle.
    assert!(song.slides.iter().all(|s| s.lines.is_empty()));
    harness.send(Command::GoLive { index: 0 });
    let state = harness.state();
    let live = state.output.live.as_ref().unwrap();
    assert_eq!(Captions::for_song(&live.source, &state.styles.default), Captions::default());
    assert_eq!(live.resolved.background.as_ref().unwrap().image, ImageSource::File("red.png".into()));
}

#[test]
fn the_pictures_of_the_set_are_read_from_backgrounds_when_the_set_opens() {
    let harness = open();
    let state = harness.state();
    assert_eq!(state.images.get("red.png"), Some(&ImageFile::Loaded(RED.into())));
    assert_eq!(state.images.get("gone.png"), Some(&ImageFile::Missing));
    // Known without selecting the item, so the set list can flag it.
    assert_eq!(state.missing_image(2), Some(1));
    assert_eq!(state.missing_image(0), None);
    assert_eq!(state.missing_image(1), None);
}

#[test]
fn finding_a_missing_picture_lists_backgrounds_and_relinking_shows_it() {
    let mut harness = open();
    harness.send(Command::FindImage { item: 2, slide: 1 });
    let state = harness.state();
    assert_eq!(state.set.as_ref().unwrap().selected, Some(2));
    assert_eq!(state.set.as_ref().unwrap().finding_image, Some((2, 1)));
    // Only pictures are offered.
    assert_eq!(state.backgrounds, ["Other/blue.png", "red.png"]);
    harness.send(Command::RelinkImage { item: 2, slide: 1, file: "Other/blue.png".into() });
    let state = harness.state();
    assert_eq!(state.set.as_ref().unwrap().finding_image, None);
    assert_eq!(state.missing_image(2), None);
    assert_eq!(state.images.get("Other/blue.png"), Some(&ImageFile::Loaded(b"BLUE".as_slice().into())));
    assert_eq!(state.open_resolved(1).unwrap().background.unwrap().image, ImageSource::File("Other/blue.png".into()));
}

#[test]
fn cancelling_a_find_changes_nothing() {
    let mut harness = open();
    harness.send(Command::FindImage { item: 2, slide: 1 });
    harness.send(Command::CancelFindImage);
    assert_eq!(harness.state().set.as_ref().unwrap().finding_image, None);
    assert_eq!(harness.state().missing_image(2), Some(1));
}

#[test]
fn a_missing_picture_still_opens_the_item_and_a_bad_find_is_ignored() {
    let mut harness = open();
    harness.send(Command::SelectItem(2));
    assert_eq!(harness.state().song.as_ref().unwrap().slides.len(), 3);
    harness.send(Command::FindImage { item: 0, slide: 0 });
    assert_eq!(harness.state().set.as_ref().unwrap().finding_image, None, "a custom item has no pictures to find");
    harness.send(Command::FindImage { item: 2, slide: 9 });
    assert_eq!(harness.state().set.as_ref().unwrap().finding_image, None);
}

#[test]
fn a_set_item_is_not_a_song_file() {
    let mut harness = open();
    harness.send(Command::SelectItem(0));
    let state = harness.state();
    assert!(state.draft().is_none());
    assert!(state.problems.is_empty(), "{:?}", state.problems);
}

#[test]
fn the_fixture_sets_present() {
    let mut harness = EngineHarness::with_fixtures();
    harness.send(Command::OpenSet(SetId::new("Example")));
    harness.send(Command::SelectItem(0));
    let state = harness.state();
    assert_eq!(state.song.as_ref().unwrap().slides.len(), 3);
    harness.send(Command::OpenSet(SetId::new("Notices and Pictures")));
    harness.send(Command::SelectItem(1));
    let state = harness.state();
    assert_eq!(state.images.get("red.png").map(|f| matches!(f, ImageFile::Loaded(_))), Some(true));
    assert_eq!(state.missing_image(1), Some(1));
}
