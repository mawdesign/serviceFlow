//! Set commands: opening a set, selecting and stepping through items, relinking.

use std::sync::Arc;

use super::{go_live, refresh_results, set_edit};
use crate::chord::Shift;
use crate::command::{Command, Effect};
use crate::ids::SongId;
use crate::output::SlideKind;
use crate::search::{category, filename};
use crate::set::{ItemKind, SetItem};
use crate::set_edit::Change;
use crate::song::{LanguageMode, Slide, Song, SongBackgrounds, VerseBackground};
use crate::state::{OpenSet, OpenSong, Pending, Problem, State};
use crate::style::Captions;

pub(super) fn apply(state: &mut State, command: Command) -> Vec<Effect> {
    match command {
        Command::RefreshSets => vec![Effect::ListSets],
        Command::SetsListed(mut sets) => {
            sets.sort();
            sets.dedup();
            state.sets = sets;
            Vec::new()
        }
        Command::SetsListFailed { error } => {
            state.problems.push(Problem::LibraryUnreadable { error });
            Vec::new()
        }
        Command::OpenSet(id) => {
            state.loading_set = Some(id.clone());
            vec![Effect::LoadSet(id)]
        }
        Command::SetLoaded { id, set, hash, sidecar, sidecar_error } => {
            if state.loading_set.as_ref() != Some(&id) {
                return Vec::new();
            }
            state.loading_set = None;
            let mut ids: Vec<SongId> = set.items.iter().filter_map(|item| item.song_id()).collect();
            ids.sort();
            ids.dedup();
            if let Some(error) = sidecar_error {
                state.problems.push(Problem::SidecarUnreadable { id: id.clone(), error });
            }
            state.set = Some(OpenSet::new(id.clone(), set, hash, sidecar));
            if ids.is_empty() { Vec::new() } else { vec![Effect::CheckSetSongs { set: id, ids }] }
        }
        Command::SidecarSaved { set, sidecar } => {
            if let Some(open) = state.set.as_mut().filter(|open| open.id == set) {
                open.saving = false;
                open.saved_sidecar = sidecar;
                open.refresh_modified();
            }
            Vec::new()
        }
        Command::SidecarSaveFailed { set, error } => {
            if let Some(open) = state.set.as_mut().filter(|open| open.id == set) {
                open.saving = false;
            }
            state.problems.push(Problem::SidecarUnsaved { id: set, error });
            Vec::new()
        }
        Command::SetLoadFailed { id, error } => {
            if state.loading_set.as_ref() == Some(&id) {
                state.loading_set = None;
                state.problems.push(Problem::SetUnreadable { id, error });
            }
            Vec::new()
        }
        Command::SongsChecked { set, missing } => {
            if let Some(open) = state.set.as_mut().filter(|open| open.id == set) {
                open.missing = missing;
            }
            Vec::new()
        }
        Command::SelectItem(index) => select(state, index, None),
        Command::NextItem => {
            let target = state.set.as_ref().map(|set| set.selected.map_or(0, |i| i + 1));
            target.map_or_else(Vec::new, |i| select(state, i, None))
        }
        Command::PreviousItem => {
            let target = state.set.as_ref().and_then(|set| set.selected.and_then(|i| i.checked_sub(1)));
            target.map_or_else(Vec::new, |i| select(state, i, None))
        }
        Command::StartRelink(index) => {
            let Some(set) = &mut state.set else { return Vec::new() };
            let Some(item) = set.set.items.get(index).filter(|item| item.song_id().is_some()) else { return Vec::new() };
            let name = item.name.clone();
            set.relinking = Some(index);
            state.library.query = name;
            refresh_results(&mut state.library);
            Vec::new()
        }
        Command::CancelRelink => {
            if let Some(set) = &mut state.set {
                set.relinking = None;
            }
            Vec::new()
        }
        Command::RelinkItem { item, song } => {
            // An edit of the set like any other: undoable, saved with the set.
            let mut effects = set_edit::change(state, None, |set| {
                let mut set = set.clone();
                let target = set.items.get_mut(item)?;
                let ItemKind::Song(song_item) = &mut target.kind else { return None };
                target.name = filename(&song).to_owned();
                song_item.path = category(&song).to_owned();
                Some((set, Change::InPlace))
            });
            if let Some(set) = &mut state.set {
                set.relinking = None;
            }
            // Show it, unless the edit already did.
            if state.set.as_ref().and_then(|s| s.selected) != Some(item) {
                effects.extend(select(state, item, None));
            }
            effects
        }
        Command::FindImage { item, slide } => {
            let mut effects = select(state, item, None);
            if let Some(set) = &mut state.set
                && matches!(set.set.items.get(item).map(|i| &i.kind), Some(ItemKind::Image(image)) if slide < image.slides.len())
            {
                set.finding_image = Some((item, slide));
                effects.push(Effect::ListBackgrounds);
            }
            effects
        }
        Command::CancelFindImage => {
            if let Some(set) = &mut state.set {
                set.finding_image = None;
            }
            Vec::new()
        }
        Command::BackgroundsListed(files) => {
            state.backgrounds = files;
            Vec::new()
        }
        Command::RelinkImage { item, slide, file } => {
            let effects = set_edit::change(state, None, |set| {
                let mut set = set.clone();
                let ItemKind::Image(image) = &mut set.items.get_mut(item)?.kind else { return None };
                image.slides.get_mut(slide)?.image = Some(crate::style::ImageSource::File(file));
                Some((set, Change::InPlace))
            });
            if let Some(set) = &mut state.set {
                set.finding_image = None;
            }
            let mut effects = effects;
            if state.set.as_ref().and_then(|s| s.selected) != Some(item) {
                effects.extend(select(state, item, None));
            }
            effects
        }
        // Handled by the engine itself.
        _ => super::unrouted(&command),
    }
}

/// Show item `index`. A song item loads its song; anything else has no slides. `pending` says
/// what to send live once the song has loaded.
pub(super) fn select(state: &mut State, index: usize, pending: Option<Pending>) -> Vec<Effect> {
    let Some(set) = &mut state.set else { return Vec::new() };
    let Some(item) = set.set.items.get(index) else { return Vec::new() };
    let id = item.song_id();
    set.selected = Some(index);
    // The item's own language mode replaces whatever the last item was shown in.
    state.slide_options.language = set.item_settings(index).language;
    state.song = None;
    state.loading = None;
    if let Some(open) = open_item(index, item) {
        set.pending = pending;
        state.song = Some(open);
        go_pending(state);
        return Vec::new();
    }
    match id {
        Some(id) if !set.is_missing(index) => {
            set.pending = pending;
            state.loading = Some(id.clone());
            vec![Effect::LoadSong(id)]
        }
        _ => {
            set.pending = None;
            Vec::new()
        }
    }
}

/// Ask which songs the open set refers to are not in the library.
pub(super) fn check_songs(state: &State) -> Vec<Effect> {
    let Some(open) = &state.set else { return Vec::new() };
    let mut ids: Vec<SongId> = open.set.items.iter().filter_map(|item| item.song_id()).collect();
    ids.sort();
    ids.dedup();
    if ids.is_empty() { Vec::new() } else { vec![Effect::CheckSetSongs { set: open.id.clone(), ids }] }
}

/// Move on from the end (`forward`) or the start of the selected item to the nearest song
/// item that can be presented. Placeholders and missing songs have nothing to project, so they
/// are passed over; with none left, nothing changes.
pub(super) fn advance(state: &mut State, forward: bool) -> Vec<Effect> {
    let Some(set) = &state.set else { return Vec::new() };
    let Some(current) = set.selected else { return Vec::new() };
    match neighbour(set, current, forward) {
        Some(i) => select(state, i, Some(if forward { Pending::First } else { Pending::Last })),
        None => Vec::new(),
    }
}

/// The nearest item after (`forward`) or before item `current` that can be presented.
pub(super) fn neighbour(set: &OpenSet, current: usize, forward: bool) -> Option<usize> {
    let presentable = |i: &usize| {
        let item = &set.set.items[*i];
        matches!(item.kind, ItemKind::Custom(_) | ItemKind::Image(_)) || (item.song_id().is_some() && !set.is_missing(*i))
    };
    if forward { (current + 1..set.set.items.len()).find(presentable) } else { (0..current).rev().find(presentable) }
}

/// A song as song item `item` shows it: with the item's presentation order, if it has one.
pub(super) fn ordered(item: &SetItem, song: &Arc<Song>) -> Arc<Song> {
    match &item.kind {
        ItemKind::Song(s) if !s.presentation.is_empty() => Arc::new(Song { presentation: s.presentation.clone(), ..(**song).clone() }),
        _ => Arc::clone(song),
    }
}

/// The song `id` of the selected item gets that item's presentation order, if it has one.
pub(super) fn with_item_order(state: &State, id: &SongId, song: Arc<Song>) -> Arc<Song> {
    let order =
        state.set.as_ref().and_then(|set| set.set.items.get(set.selected?)).filter(|item| item.song_id().as_ref() == Some(id)).and_then(|item| {
            match &item.kind {
                ItemKind::Song(s) if !s.presentation.is_empty() => Some(s.presentation.clone()),
                _ => None,
            }
        });
    match order {
        Some(presentation) => Arc::new(Song { presentation, ..(*song).clone() }),
        None => song,
    }
}

/// After a song loaded for a step into a new item: send its first or last slide live.
pub(super) fn go_pending(state: &mut State) {
    let Some(pending) = state.set.as_mut().and_then(|set| set.pending.take()) else { return };
    let in_flow = state.song.as_ref().map_or(0, |open| open.in_flow);
    match pending {
        Pending::First => go_live(state, 0),
        Pending::Last => go_live(state, in_flow.saturating_sub(1)),
    }
}

/// The operator chose a language mode: if a song item of the open set is selected, keep the choice
/// for that item in the set's item settings (in memory: saved with the set; a song opened from
/// the library has no item to keep it for).
pub(super) fn remember_language(state: &mut State, language: LanguageMode) {
    let Some(set) = &mut state.set else { return };
    let Some(index) = set.selected else { return };
    let mut settings = set.item_settings(index);
    if settings.language != language {
        settings.language = language;
        set.set_item_settings(index, settings);
    }
}

/// A custom or image item as the stand-in song its slides are shown from. Its title and subtitle
/// (custom items only: a picture is shown on its own) go to the title and subtitle boxes; its
/// pictures become the slides' backgrounds. `None` for other items.
pub(super) fn open_item(index: usize, item: &SetItem) -> Option<OpenSong> {
    let id = SongId::set_item(index);
    match &item.kind {
        ItemKind::Custom(custom) => {
            let song = Song {
                title: item.name.clone(),
                captions: Some(Captions { title: custom.title.clone(), subtitle: custom.subtitle.clone() }),
                ..Song::default()
            };
            let slides = custom
                .slides
                .iter()
                .enumerate()
                .map(|(n, body)| Slide { section: (n + 1).to_string(), lines: body.lines().map(str::to_owned).collect(), ..Slide::default() })
                .collect();
            Some(OpenSong::of_item(id, song, slides, SlideKind::Custom))
        }
        ItemKind::Image(image) => {
            let items = image
                .slides
                .iter()
                .enumerate()
                .filter_map(|(n, slide)| Some(VerseBackground { verse: (n + 1).to_string(), image: slide.image.clone()? }))
                .collect();
            let song = Song {
                title: item.name.clone(),
                captions: Some(Captions::default()),
                backgrounds: Some(SongBackgrounds { resize: image.resize, keep_aspect: image.keep_aspect, items, ..SongBackgrounds::default() }),
                ..Song::default()
            };
            let slides = (0..image.slides.len()).map(|n| Slide { section: (n + 1).to_string(), ..Slide::default() }).collect();
            Some(OpenSong::of_item(id, song, slides, SlideKind::Image))
        }
        _ => None,
    }
}

/// The transposition and capo of the selected item when `id` is its song (else none).
pub(super) fn item_shift(state: &State, id: &SongId) -> Shift {
    let Some(set) = &state.set else { return Shift::default() };
    let Some(index) = set.selected else { return Shift::default() };
    if set.set.items.get(index).and_then(|item| item.song_id()).as_ref() != Some(id) {
        return Shift::default();
    }
    let settings = set.item_settings(index);
    Shift { semitones: settings.transpose, capo: settings.capo }
}
