//! Blanks between set items (GLOSSARY "Blank"): a slide of only the background that stepping past
//! the end of an item (or back past its start) lands on, so the next item, a video say, does not
//! start unannounced. They exist only while presenting: the set itself never holds them.

use std::sync::Arc;

use super::{auto, go_live, sets};
use crate::command::Effect;
use crate::ids::SongId;
use crate::resolve::ResolvedStyle;
use crate::set::ItemKind;
use crate::song::Song;
use crate::state::{ItemBlank, OpenSong, OutputMode, Pending, State};

/// The items on either side of the blank next to the selected item, going `forward` (after it) or
/// back (before it) as `(before, after)`. `None` when there is no blank there: blanks are off,
/// there is no item that way (no blank before the first item or after the last), or the two have
/// the same name (the operator's way to group items into one).
pub(super) fn across(state: &State, forward: bool) -> Option<(usize, usize)> {
    if !state.output_settings.blanks {
        return None;
    }
    let set = state.set.as_ref()?;
    let current = set.selected?;
    let other = sets::neighbour(set, current, forward)?;
    let (before, after) = if forward { (current, other) } else { (other, current) };
    (set.set.items[before].name != set.set.items[after].name).then_some((before, after))
}

impl State {
    /// The blank after the selected item, as the presentation helper lists it after the item's
    /// last slide: how it looks. `None` when none follows (see [`across`]).
    pub fn blank_after(&self) -> Option<ResolvedStyle> {
        let (before, after) = across(self, true)?;
        Some(background(self, before, after).unwrap_or_default())
    }

    /// Whether the blank after the selected item is the one shown now.
    pub fn on_blank_after(&self) -> bool {
        match (&self.output.item_blank, self.set.as_ref().and_then(|s| s.selected)) {
            (Some(blank), Some(selected)) => self.output.mode == OutputMode::Blank && blank.before == selected,
            _ => false,
        }
    }
}

/// The blank between two items is what the outputs show now.
pub(super) fn on_item_blank(state: &State) -> bool {
    state.output.mode == OutputMode::Blank && state.output.item_blank.is_some()
}

/// Show the blank between items `before` and `after`.
pub(super) fn enter(state: &mut State, before: usize, after: usize) {
    let resolved = background(state, before, after).or_else(|| state.output.live.as_ref().map(|live| live.resolved.clone())).unwrap_or_default();
    state.output.item_blank = Some(ItemBlank { before, after, resolved });
    state.output.mode = OutputMode::Blank;
    state.output.changes += 1;
    auto::schedule(state);
}

/// Step out of the blank into item `to`: its first slide going `forward`, its last going back.
pub(super) fn leave(state: &mut State, to: usize, forward: bool) -> Vec<Effect> {
    let selected = state.set.as_ref().and_then(|set| set.selected);
    match state.song.as_ref() {
        Some(open) if selected == Some(to) => {
            let index = if forward { 0 } else { open.in_flow.saturating_sub(1) };
            go_live(state, index);
            Vec::new()
        }
        _ => sets::select(state, to, Some(if forward { Pending::First } else { Pending::Last })),
    }
}

/// What the blank between `before` and `after` shows: the end of the item before, or with
/// `blank_uses_next` the start of the item after. That item's own style applies, so a style
/// override on the item shows. When the wanted item is not known yet (a song still being read
/// ahead), the other one.
fn background(state: &State, before: usize, after: usize) -> Option<ResolvedStyle> {
    let first_choice = if state.output_settings.blank_uses_next { (after, false) } else { (before, true) };
    let second_choice = if state.output_settings.blank_uses_next { (before, true) } else { (after, false) };
    edge(state, first_choice.0, first_choice.1).or_else(|| edge(state, second_choice.0, second_choice.1))
}

/// How the first (or `last`) slide of the flow of item `index` looks, if its song is at hand.
fn edge(state: &State, index: usize, last: bool) -> Option<ResolvedStyle> {
    let set = state.set.as_ref()?;
    let item = set.set.items.get(index)?;
    let pick = |song: &Song, slides: &[crate::song::Slide], in_flow: usize| {
        let slide = slides.get(if last { in_flow.checked_sub(1)? } else { 0 })?;
        Some(state.resolved_in_item(index, song, slide))
    };
    if let Some(open) = &state.song
        && set.selected == Some(index)
        && item.open_id(index).as_ref() == Some(&open.id)
    {
        return pick(&open.song, &open.slides, open.in_flow);
    }
    match &item.kind {
        ItemKind::Custom(_) | ItemKind::Image(_) => {
            let open = sets::open_item(index, item)?;
            pick(&open.song, &open.slides, open.in_flow)
        }
        ItemKind::Song(_) => {
            let id = item.song_id()?;
            let song = set.peeked.iter().find(|(peeked, _)| *peeked == id)?.1.as_ref()?;
            let song = sets::ordered(item, song);
            let (slides, in_flow) = OpenSong::grid(&song, &state.slide_options);
            pick(&song, &slides, in_flow)
        }
        ItemKind::Style { .. } | ItemKind::Placeholder { .. } => None,
    }
}

/// Which neighbouring songs to read ahead, so a blank next to them knows their background. Asks
/// for each once; forgets those that are no longer neighbours.
pub(super) fn peeks(state: &mut State) -> Vec<Effect> {
    let blanks = state.output_settings.blanks;
    let Some(set) = &mut state.set else { return Vec::new() };
    let wanted: Vec<SongId> = match set.selected {
        Some(current) if blanks => [true, false]
            .into_iter()
            .filter_map(|forward| sets::neighbour(set, current, forward))
            .filter_map(|i| set.set.items[i].song_id())
            .collect(),
        _ => Vec::new(),
    };
    set.peeked.retain(|(id, _)| wanted.contains(id));
    let mut effects = Vec::new();
    for id in wanted {
        if !set.peeked.iter().any(|(peeked, _)| *peeked == id) {
            set.peeked.push((id.clone(), None));
            effects.push(Effect::PeekSong(id));
        }
    }
    effects
}

/// A neighbouring song arrived. If a blank is shown whose background it decides, redraw it.
pub(super) fn peeked(state: &mut State, id: &SongId, song: Option<Arc<Song>>) {
    let Some(set) = &mut state.set else { return };
    let Some(entry) = set.peeked.iter_mut().find(|(peeked, _)| peeked == id) else { return };
    entry.1 = song;
    let Some(ItemBlank { before, after, .. }) = state.output.item_blank.clone() else { return };
    if state.output.mode != OutputMode::Blank {
        return;
    }
    if let Some(resolved) = background(state, before, after)
        && let Some(blank) = &mut state.output.item_blank
        && blank.resolved != resolved
    {
        blank.resolved = resolved;
        state.output.changes += 1;
    }
}
