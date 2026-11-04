//! Seam 1, the engine: blanks between set items (GLOSSARY "Blank"). Which items get one, what it
//! shows, and the order stepping passes through them.

use serviceflow_core::command::{Command, ImportTrigger};
use serviceflow_core::ids::SetId;
use serviceflow_core::state::OutputMode;
use serviceflow_testkit::EngineHarness;

fn style(font: &str) -> String {
    format!(r#"<style><body font="{font}" size="40"/></style>"#)
}

/// A song called `title` with two slides and, optionally, a style of its own.
fn song(title: &str, own_style: Option<&str>) -> (String, String) {
    let style = own_style.map(style).unwrap_or_default();
    (title.to_owned(), format!("<song><title>{title}</title><lyrics>[V1]\n {title} one\n[V2]\n {title} two</lyrics>{style}</song>"))
}

fn song_item(name: &str, own_style: Option<&str>) -> String {
    format!(r#"<slide_group name="{name}" type="song">{}</slide_group>"#, own_style.map(style).unwrap_or_default())
}

fn custom_item(name: &str, own_style: Option<&str>) -> String {
    format!(
        r#"<slide_group name="{name}" type="custom"><title>{name}</title><slides><slide><body>{name} text</body></slide></slides>{}</slide_group>"#,
        own_style.map(style).unwrap_or_default()
    )
}

fn present(blanks: bool, uses_next: bool) -> String {
    format!(
        r#"<present_settings><default_style><body font="DefaultFont" size="40"/></default_style>
<style blanks="{blanks}" blank_uses_next="{uses_next}"/></present_settings>"#
    )
}

fn open(items: &[String], blanks: bool, uses_next: bool) -> EngineHarness {
    let songs: Vec<(String, String)> = ["A", "B", "C"].iter().map(|t| song(t, None)).chain([song("Styled", Some("SongFont"))]).collect();
    let all: Vec<(&str, &str)> = songs.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let set = format!(r#"<set name="S"><slide_groups>{}</slide_groups></set>"#, items.join("\n"));
    let settings = present(blanks, uses_next);
    let mut harness = EngineHarness::with_data(&all, &[("S", &set)], &[("PresentSettings", &settings)], &[]);
    harness.send(Command::ImportOpenSongStyles { trigger: ImportTrigger::FirstRun });
    harness.send(Command::ImportOpenSongSettings { trigger: ImportTrigger::FirstRun });
    harness.send(Command::OpenSet(SetId::new("S")));
    harness.send(Command::SelectItem(0));
    harness
}

fn mode(h: &EngineHarness) -> OutputMode {
    h.state().output.mode
}

/// The text of the live slide, or "(blank)" while the blank between items is shown.
fn shown(h: &EngineHarness) -> String {
    let state = h.state();
    match (state.output.mode, &state.output.live) {
        (OutputMode::Blank, _) => "(blank)".to_owned(),
        (_, Some(live)) => live.slide.lines.join("/"),
        _ => "(nothing)".to_owned(),
    }
}

/// The font of the blank that is shown.
fn blank_font(h: &EngineHarness) -> String {
    let state = h.state();
    let blank = state.output.item_blank.as_ref().expect("the blank between items is shown");
    blank.resolved.style(&state.styles).body.text.font.clone()
}

fn abc() -> Vec<String> {
    vec![song_item("A", None), song_item("B", None), song_item("C", None)]
}

#[test]
fn a_blank_comes_first_when_stepping_past_the_end_of_an_item() {
    let mut h = open(&abc(), true, false);
    h.send(Command::GoLive { index: 0 });
    let mut seen = vec![shown(&h)];
    for _ in 0..6 {
        h.send(Command::NextSlide);
        seen.push(shown(&h));
    }
    assert_eq!(seen, ["A one", "A two", "(blank)", "B one", "B two", "(blank)", "C one"]);
    assert_eq!(h.state().set.as_ref().unwrap().selected, Some(2));
}

#[test]
fn stepping_back_passes_through_the_same_blanks() {
    let mut h = open(&abc(), true, false);
    h.send(Command::SelectItem(2));
    h.send(Command::GoLive { index: 0 });
    let mut seen = vec![shown(&h)];
    for _ in 0..5 {
        h.send(Command::PreviousSlide);
        seen.push(shown(&h));
    }
    assert_eq!(seen, ["C one", "(blank)", "B two", "B one", "(blank)", "A two"]);
}

#[test]
fn previous_from_a_blank_returns_to_the_item_before_and_next_to_the_item_after() {
    let mut h = open(&abc(), true, false);
    h.send(Command::GoLive { index: 1 }).send(Command::NextSlide);
    assert_eq!(shown(&h), "(blank)");
    h.send(Command::PreviousSlide);
    assert_eq!(shown(&h), "A two", "back to the last slide, not the one before");
    h.send(Command::NextSlide).send(Command::NextSlide);
    assert_eq!(shown(&h), "B one");
    h.send(Command::PreviousSlide);
    assert_eq!(shown(&h), "(blank)", "the same blank, reached from the other side");
    h.send(Command::NextSlide);
    assert_eq!(shown(&h), "B one");
}

#[test]
fn there_is_no_blank_before_the_first_item_or_after_the_last() {
    let mut h = open(&abc(), true, false);
    h.send(Command::GoLive { index: 0 }).send(Command::PreviousSlide);
    assert_eq!(shown(&h), "A one", "nothing before the first item");
    h.send(Command::SelectItem(2)).send(Command::GoLive { index: 1 }).send(Command::NextSlide);
    assert_eq!(shown(&h), "C two", "nothing after the last item");
    assert!(h.state().blank_after().is_none());
}

#[test]
fn a_song_opened_from_the_library_has_no_blank() {
    let mut h = open(&abc(), true, false);
    h.send(Command::OpenSong(serviceflow_core::ids::SongId::new("A")));
    h.send(Command::GoLive { index: 1 }).send(Command::NextSlide);
    assert_eq!(shown(&h), "A two");
}

#[test]
fn with_blanks_off_stepping_goes_straight_to_the_next_item() {
    let mut h = open(&abc(), false, false);
    h.send(Command::GoLive { index: 1 }).send(Command::NextSlide);
    assert_eq!(shown(&h), "B one");
    assert!(h.state().blank_after().is_none());
}

#[test]
fn items_with_the_same_name_have_no_blank_between_them() {
    let items = vec![custom_item("Video", None), custom_item("Video", None), song_item("B", None)];
    let mut h = open(&items, true, false);
    h.send(Command::GoLive { index: 0 }).send(Command::NextSlide);
    assert_eq!(shown(&h), "Video text", "straight into the second item of the same name");
    assert_eq!(h.state().set.as_ref().unwrap().selected, Some(1));
    h.send(Command::NextSlide);
    assert_eq!(shown(&h), "(blank)", "but one before an item with another name");
    h.send(Command::NextSlide);
    assert_eq!(shown(&h), "B one");
    // And back again.
    h.send(Command::PreviousSlide).send(Command::PreviousSlide);
    assert_eq!(shown(&h), "Video text");
    h.send(Command::PreviousSlide);
    assert_eq!(h.state().set.as_ref().unwrap().selected, Some(0), "no blank between the two");
    assert_ne!(shown(&h), "(blank)");
}

#[test]
fn the_blank_shows_the_background_of_the_item_before() {
    let items = vec![song_item("A", Some("FontA")), song_item("B", Some("FontB"))];
    let mut h = open(&items, true, false);
    h.send(Command::GoLive { index: 1 }).send(Command::NextSlide);
    assert_eq!(blank_font(&h), "FontA", "the set item's own style");
    // From the other side it is still the item before's.
    h.send(Command::NextSlide).send(Command::PreviousSlide);
    assert_eq!(blank_font(&h), "FontA");
}

#[test]
fn with_blank_uses_next_it_shows_the_background_of_the_item_after() {
    let items = vec![song_item("A", Some("FontA")), song_item("B", Some("FontB"))];
    let mut h = open(&items, true, true);
    h.send(Command::GoLive { index: 1 }).send(Command::NextSlide);
    assert_eq!(blank_font(&h), "FontB", "read ahead from the next song, with the item's style");
    h.send(Command::NextSlide).send(Command::PreviousSlide);
    assert_eq!(blank_font(&h), "FontB");
}

#[test]
fn a_song_style_counts_for_the_blank() {
    // `Styled` has its own style in the song file; the default applies to the others.
    let items = vec![song_item("A", None), song_item("Styled", None)];
    let mut h = open(&items, true, true);
    h.send(Command::GoLive { index: 1 }).send(Command::NextSlide);
    assert_eq!(blank_font(&h), "SongFont");
    let mut h = open(&items, true, false);
    h.send(Command::GoLive { index: 1 }).send(Command::NextSlide);
    assert_eq!(blank_font(&h), "DefaultFont");
}

#[test]
fn custom_items_give_their_style_to_a_blank_without_reading_ahead() {
    let items = vec![song_item("A", None), custom_item("Notices", Some("NoticeFont"))];
    let mut h = open(&items, true, true);
    h.send(Command::GoLive { index: 1 }).send(Command::NextSlide);
    assert_eq!(blank_font(&h), "NoticeFont");
}

#[test]
fn the_helper_lists_the_blank_after_the_item_but_the_set_does_not_hold_it() {
    let mut h = open(&abc(), true, false);
    assert!(h.state().blank_after().is_some());
    assert_eq!(h.state().set.as_ref().unwrap().set.items.len(), 3, "blanks are not set items");
    h.send(Command::GoToItemBlank);
    assert_eq!(mode(&h), OutputMode::Blank);
    assert!(h.state().on_blank_after());
    h.send(Command::SelectItem(2));
    assert!(h.state().blank_after().is_none(), "none after the last item");
    // Saving the set writes no blanks.
    h.send(Command::EditSet(serviceflow_core::set_edit::SetEdit::Remove(2)));
    h.send(Command::SaveSet);
    assert_eq!(h.saved_set("S").unwrap().items.len(), 2);
}

#[test]
fn blank_commands_are_live_actions_and_never_undoable() {
    assert!(Command::GoToItemBlank.is_live_action());
    assert!(Command::Tick { now: std::time::Duration::ZERO }.is_live_action());
    assert!(!Command::GoToItemBlank.is_document_edit());
    let mut h = open(&abc(), true, false);
    h.send(Command::GoLive { index: 1 }).send(Command::NextSlide);
    assert!(!h.state().set.as_ref().unwrap().history.can_undo(), "stepping into a blank is not an edit");
}

#[test]
fn toggling_blank_by_hand_still_works_and_leaves_the_item_blank() {
    let mut h = open(&abc(), true, false);
    h.send(Command::GoLive { index: 1 }).send(Command::NextSlide);
    assert!(h.state().output.item_blank.is_some());
    h.send(Command::ToggleBlank);
    assert_eq!(mode(&h), OutputMode::Normal);
    assert!(h.state().output.item_blank.is_none());
    h.send(Command::ToggleBlank);
    assert_eq!(mode(&h), OutputMode::Blank);
    assert!(h.state().output.item_blank.is_none(), "a hand-made blank is the live slide's own");
}
