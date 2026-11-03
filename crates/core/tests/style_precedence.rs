//! Seam 1, the engine: which style a slide is drawn with (set item style > song style >
//! style stack > default or scripture style) and the song backgrounds.

use serviceflow_core::command::{Command, ImportTrigger};
use serviceflow_core::ids::{SetId, SongId};
use serviceflow_core::look::{self, ImageFile, StyleChoice};
use serviceflow_core::style::{ImageSource, Resize};
use serviceflow_testkit::EngineHarness;

fn style(font: &str) -> String {
    format!(r#"<style><body font="{font}" size="40"/></style>"#)
}

fn song(title: &str, own_style: Option<&str>) -> (String, String) {
    let style = own_style.map(style).unwrap_or_default();
    (title.to_owned(), format!("<song><title>{title}</title><lyrics>[V1]\n {title} one\n[V2]\n {title} two</lyrics>{style}</song>"))
}

fn present_settings(preferred: Option<bool>) -> String {
    let preferred = preferred.map(|p| format!("<song_style_preferred>{p}</song_style_preferred>")).unwrap_or_default();
    format!(
        r#"<present_settings><default_style><body font="DefaultFont" size="40"/></default_style>
<scripture_style><body font="ScriptureFont" size="40"/></scripture_style>{preferred}</present_settings>"#
    )
}

/// A set of `items` (raw `slide_group` elements) over the songs, with PresentSettings imported.
fn open(items: &[String], preferred: Option<bool>, songs: &[(String, String)]) -> EngineHarness {
    let set = format!(r#"<set name="S"><slide_groups>{}</slide_groups></set>"#, items.join("\n"));
    let all: Vec<(&str, &str)> = songs.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let settings = present_settings(preferred);
    let mut harness = EngineHarness::with_data(&all, &[("S", &set)], &[("PresentSettings", &settings)], &[]);
    harness.send(Command::ImportOpenSongStyles { trigger: ImportTrigger::FirstRun });
    harness.send(Command::ImportOpenSongSettings { trigger: ImportTrigger::FirstRun });
    harness.send(Command::OpenSet(SetId::new("S")));
    harness
}

fn song_item(name: &str, own_style: Option<&str>) -> String {
    format!(r#"<slide_group name="{name}" type="song">{}</slide_group>"#, own_style.map(style).unwrap_or_default())
}

fn style_item(action: &str, font: Option<&str>) -> String {
    format!(r#"<slide_group name="style" type="style" action="{action}">{}</slide_group>"#, font.map(style).unwrap_or_default())
}

/// The font of the style slide 0 of set item `item` is drawn with.
fn font_of_item(harness: &mut EngineHarness, item: usize) -> String {
    harness.send(Command::SelectItem(item));
    let state = harness.state();
    let look = state.open_look(0).expect("a song is open");
    look.style(&state.styles).body.text.font.clone()
}

fn songs() -> Vec<(String, String)> {
    vec![song("Plain", None), song("Styled", Some("SongFont")), song("Other", None)]
}

#[test]
fn with_no_styles_anywhere_the_default_style_is_used() {
    let mut h = open(&[song_item("Plain", None)], None, &songs());
    assert_eq!(font_of_item(&mut h, 0), "DefaultFont");
    assert_eq!(h.state().open_look(0).unwrap().style, StyleChoice::Default);
}

#[test]
fn a_song_opened_from_the_library_uses_its_own_style() {
    let mut h = open(&[], None, &songs());
    h.send(Command::OpenSong(SongId::new("Styled")));
    let state = h.state();
    let look = state.open_look(0).unwrap();
    assert_eq!(look.style(&state.styles).body.text.font, "SongFont");
    h.send(Command::OpenSong(SongId::new("Plain")));
    assert_eq!(h.state().open_look(0).unwrap().style, StyleChoice::Default);
}

#[test]
fn level_1_the_set_items_own_style_beats_everything() {
    let items = [style_item("new", Some("StackFont")), song_item("Styled", Some("ItemFont"))];
    for preferred in [Some(true), Some(false)] {
        let mut h = open(&items, preferred, &songs());
        assert_eq!(font_of_item(&mut h, 1), "ItemFont", "preferred = {preferred:?}");
    }
}

#[test]
fn level_2_the_song_style_beats_the_stack_when_song_style_preferred() {
    let items = [style_item("new", Some("StackFont")), song_item("Styled", None), song_item("Plain", None)];
    for preferred in [None, Some(true)] {
        let mut h = open(&items, preferred, &songs());
        assert_eq!(font_of_item(&mut h, 1), "SongFont", "preferred = {preferred:?}");
        // A song without its own style still gets the stack.
        assert_eq!(font_of_item(&mut h, 2), "StackFont");
    }
}

#[test]
fn level_3_the_stack_beats_the_song_style_when_it_is_not_preferred() {
    let items = [style_item("new", Some("StackFont")), song_item("Styled", None)];
    let mut h = open(&items, Some(false), &songs());
    assert_eq!(font_of_item(&mut h, 1), "StackFont");
}

#[test]
fn the_song_style_still_applies_without_a_style_item_when_not_preferred() {
    // OpenSong: the song style is used when the stack is still at the default style.
    let items = [song_item("Styled", None)];
    let mut h = open(&items, Some(false), &songs());
    assert_eq!(font_of_item(&mut h, 0), "SongFont");
}

#[test]
fn level_4_the_default_style_applies_when_nothing_else_does() {
    let items = [song_item("Plain", None), style_item("new", Some("StackFont")), style_item("revert", None), song_item("Other", None)];
    let mut h = open(&items, None, &songs());
    assert_eq!(font_of_item(&mut h, 0), "DefaultFont");
    assert_eq!(font_of_item(&mut h, 3), "DefaultFont", "after the revert");
}

#[test]
fn style_items_push_and_revert_across_a_set() {
    let items = [
        song_item("Plain", None),       // 0: default
        style_item("new", Some("One")), // 1
        song_item("Plain", None),       // 2: One
        style_item("new", Some("Two")), // 3
        song_item("Plain", None),       // 4: Two
        style_item("revert", None),     // 5
        song_item("Plain", None),       // 6: One
        style_item("revert", None),     // 7
        song_item("Plain", None),       // 8: default
        style_item("revert", None),     // 9: nothing to revert
        song_item("Plain", None),       // 10: default
    ];
    let mut h = open(&items, None, &songs());
    let fonts: Vec<_> = [0, 2, 4, 6, 8, 10].into_iter().map(|i| font_of_item(&mut h, i)).collect();
    assert_eq!(fonts, ["DefaultFont", "One", "Two", "One", "DefaultFont", "DefaultFont"]);
}

#[test]
fn a_new_style_item_without_a_style_keeps_the_current_one_and_still_pushes() {
    let items = [
        style_item("new", Some("One")),
        style_item("new", None),
        song_item("Plain", None),
        style_item("revert", None),
        song_item("Plain", None),
        style_item("revert", None),
        song_item("Plain", None),
    ];
    let mut h = open(&items, None, &songs());
    assert_eq!(font_of_item(&mut h, 2), "One");
    assert_eq!(font_of_item(&mut h, 4), "One");
    assert_eq!(font_of_item(&mut h, 6), "DefaultFont");
}

#[test]
fn the_stack_depth_is_reported() {
    let items = [style_item("new", Some("One")), style_item("new", Some("Two")), style_item("revert", None), song_item("Plain", None)];
    let h = open(&items, None, &songs());
    let state = h.state();
    let set = &state.set.as_ref().unwrap().set;
    let depths: Vec<_> = (0..=4).map(|i| look::stack_depth(set, i)).collect();
    assert_eq!(depths, [0, 1, 2, 1, 1]);
}

#[test]
fn scripture_items_use_the_scripture_style_even_while_a_style_item_is_active() {
    let items = [
        r#"<slide_group name="Psalm" type="scripture"/>"#.to_owned(),
        style_item("new", Some("StackFont")),
        r#"<slide_group name="John" type="scripture"/>"#.to_owned(),
        format!(r#"<slide_group name="Own" type="scripture">{}</slide_group>"#, style("OwnFont")),
        song_item("Plain", None),
    ];
    let h = open(&items, None, &songs());
    let state = h.state();
    let set = &state.set.as_ref().unwrap().set;
    assert_eq!(look::choose(Some((set, 0)), None, true), StyleChoice::Scripture);
    assert_eq!(look::choose(Some((set, 2)), None, true), StyleChoice::Scripture, "the stack does not apply to scripture");
    let StyleChoice::Custom(own) = look::choose(Some((set, 3)), None, true) else { panic!("item style") };
    assert_eq!(own.body.text.font, "OwnFont");
    let scripture = StyleChoice::Scripture;
    let look = look::Look { style: scripture, background: None };
    assert_eq!(look.style(&state.styles).body.text.font, "ScriptureFont");
    // The stack still applies to what follows the scripture item.
    let StyleChoice::Custom(next) = look::choose(Some((set, 4)), None, true) else { panic!("stack") };
    assert_eq!(next.body.text.font, "StackFont");
}

#[test]
fn what_is_live_keeps_its_style_and_the_next_item_brings_its_own() {
    let items = [song_item("Plain", None), style_item("new", Some("StackFont")), song_item("Other", None)];
    let mut h = open(&items, None, &songs());
    h.send(Command::SelectItem(0));
    h.send(Command::GoLive { index: 1 });
    let live_font = |h: &EngineHarness| {
        let state = h.state();
        let live = state.output.live.clone().unwrap();
        live.look.style(&state.styles).body.text.font.clone()
    };
    assert_eq!(live_font(&h), "DefaultFont");
    // Browsing another item does not restyle what is live.
    h.send(Command::SelectItem(2));
    assert_eq!(live_font(&h), "DefaultFont");
    // Stepping past the end of the first item goes live in the next one, with the stack's style.
    h.send(Command::SelectItem(0));
    h.send(Command::GoLive { index: 1 });
    h.send(Command::NextSlide);
    let state = h.state();
    assert_eq!(state.output.live.as_ref().unwrap().song.as_str(), "Other");
    assert_eq!(live_font(&h), "StackFont");
    // And back again.
    h.send(Command::PreviousSlide);
    assert_eq!(live_font(&h), "DefaultFont");
}

#[test]
fn re_importing_the_default_style_restyles_what_is_live() {
    let mut h = open(&[song_item("Plain", None)], None, &songs());
    h.send(Command::SelectItem(0));
    h.send(Command::GoLive { index: 0 });
    let state = h.state();
    assert_eq!(state.output.live.as_ref().unwrap().look.style, StyleChoice::Default);
}

#[test]
fn a_custom_style_uses_the_imported_outline_thickness() {
    let settings = r#"<present_settings><style thickness="3"/><default_style><body font="DefaultFont"/></default_style></present_settings>"#;
    let (title, xml) = song("Styled", Some("SongFont"));
    let mut h = EngineHarness::with_data(&[(&title, &xml)], &[], &[("PresentSettings", settings)], &[]);
    h.send(Command::ImportOpenSongStyles { trigger: ImportTrigger::FirstRun });
    h.send(Command::OpenSong(SongId::new("Styled")));
    let state = h.state();
    let look = state.open_look(0).unwrap();
    assert_eq!(look.style(&state.styles).outline_thickness, 9.0);
}

// Backgrounds

const PICS: &str = r#"<song><title>Pics</title><lyrics>[V1]
 a
 ||b
 ||c
[C]
 chorus</lyrics>
<presentation>V1 C V1</presentation>
<backgrounds resize="body" keep_aspect="true">
<background verse="V1"><filename>one.png</filename></background>
<background verse="V1"><filename>two.png</filename></background>
<background verse="C"><image>QUJD</image></background>
</backgrounds></song>"#;

#[test]
fn song_backgrounds_follow_the_slides_of_each_section() {
    let mut h = EngineHarness::with_data(&[("Pics", PICS)], &[], &[], &[("one.png", b"ONE"), ("two.png", b"TWO")]);
    h.send(Command::OpenSong(SongId::new("Pics")));
    let state = h.state();
    let open = state.song.as_ref().unwrap();
    let images: Vec<_> = (0..open.in_flow).map(|i| state.open_look(i).unwrap().background.map(|b| b.image)).collect();
    let file = |n: &str| Some(ImageSource::File(n.into()));
    // V1 (three slides), C, V1 again: the nth background to the nth slide, the last one repeats,
    // and a repeated section starts over.
    assert_eq!(
        images,
        [
            file("one.png"),
            file("two.png"),
            file("two.png"),
            Some(ImageSource::Embedded("QUJD".into())),
            file("one.png"),
            file("two.png"),
            file("two.png")
        ]
    );
    let bg = state.open_look(0).unwrap().background.unwrap();
    assert_eq!((bg.resize, bg.keep_aspect, bg.as_text), (Resize::Body, true, false));
}

#[test]
fn the_files_songs_name_are_read_from_backgrounds() {
    let mut h = EngineHarness::with_data(&[("Pics", PICS)], &[], &[], &[("one.png", b"ONE")]);
    h.send(Command::OpenSong(SongId::new("Pics")));
    let state = h.state();
    assert_eq!(state.images.get("one.png"), Some(&ImageFile::Loaded(b"ONE".as_slice().into())));
    assert_eq!(state.images.bytes("one.png").map(|b| &b[..]), Some(b"ONE".as_slice()));
    assert_eq!(state.images.get("two.png"), Some(&ImageFile::Missing), "a file that is not there is remembered as missing");
    // It is not asked for again.
    h.send(Command::OpenSong(SongId::new("Pics")));
    assert_eq!(h.state().images.get("two.png"), Some(&ImageFile::Missing));
}

#[test]
fn background_as_text_is_carried_to_the_slide_look() {
    let xml = PICS.replace(r#"keep_aspect="true""#, r#"background_as_text="true""#);
    let mut h = EngineHarness::with_data(&[("Pics", &xml)], &[], &[], &[]);
    h.send(Command::OpenSong(SongId::new("Pics")));
    h.send(Command::GoLive { index: 0 });
    let state = h.state();
    let live = state.output.live.as_ref().unwrap();
    assert!(live.look.background.as_ref().unwrap().as_text);
    assert!(live.look.style(&state.styles).background.as_text);
    assert!(!live.slide.lines.is_empty(), "the lyrics are still the slide's; the renderer leaves them out");
}

#[test]
fn a_style_image_file_is_read_too() {
    let settings = r#"<present_settings><default_style><background filename="bg.jpg"/></default_style></present_settings>"#;
    let mut h = EngineHarness::with_data(&[], &[], &[("PresentSettings", settings)], &[("bg.jpg", b"JPG")]);
    h.send(Command::ImportOpenSongStyles { trigger: ImportTrigger::FirstRun });
    assert_eq!(h.state().images.bytes("bg.jpg").map(|b| &b[..]), Some(b"JPG".as_slice()));
}
