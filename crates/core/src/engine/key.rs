//! Transposition and capo of a set item (GLOSSARY "Transposition", "Capo"). The settings live in
//! the set's sidecar and only change how the chords are shown; "Save in this key" is the one way
//! they reach the song, through a normal canonical save.

use std::sync::Arc;

use super::{regrid, save, show_song};
use crate::chord::{self, Shift};
use crate::command::{Command, Effect};
use crate::ids::Expect;
use crate::sidecar::ItemSettings;
use crate::state::State;

pub(super) fn apply(state: &mut State, command: Command) -> Vec<Effect> {
    match command {
        Command::SetTranspose(semitones) => change(state, |settings| settings.transpose = semitones.clamp(-11, 11)),
        Command::SetCapo(capo) => change(state, |settings| settings.capo = capo.map(|fret| fret.min(11))),
        Command::SaveInThisKey => save_in_this_key(state),
        _ => Vec::new(),
    }
}

/// Change the selected song item's settings, keep them in the sidecar and redraw.
fn change(state: &mut State, edit: impl FnOnce(&mut ItemSettings)) -> Vec<Effect> {
    if !state.is_selected_song_item() {
        return Vec::new();
    }
    let Some(set) = &mut state.set else { return Vec::new() };
    let Some(index) = set.selected else { return Vec::new() };
    let Some(key) = set.item_key(index) else { return Vec::new() };
    let before = set.sidecar.get(&key);
    let mut settings = before;
    edit(&mut settings);
    if settings == before {
        return Vec::new();
    }
    Arc::make_mut(&mut set.sidecar).set(key, settings);
    let effects = vec![Effect::WriteSidecar { set: set.id.clone(), sidecar: Arc::clone(&set.sidecar) }];
    state.slide_options.shift = Shift { semitones: settings.transpose, capo: settings.capo };
    regrid(state);
    effects
}

/// Rewrite the chords and key of the open song as transposed for its item, put that in the song's
/// draft (one undo step), return the item to zero and save.
fn save_in_this_key(state: &mut State) -> Vec<Effect> {
    if !state.is_selected_song_item() {
        return Vec::new();
    }
    let semitones = state.slide_options.shift.semitones;
    if semitones.rem_euclid(12) == 0 {
        return Vec::new();
    }
    let Some(open) = &state.song else { return Vec::new() };
    let (id, hash) = (open.id.clone(), open.hash);
    let mut song = (*open.file).clone();
    let spelling = match chord::parse_key(&song.key) {
        Some((tonic, minor)) => chord::spelling_of_key(tonic + semitones, minor),
        None if semitones < 0 => chord::Spelling::Flats,
        None => chord::Spelling::Sharps,
    };
    let lyrics = chord::transpose_lyrics(&song.lyrics, semitones, spelling);
    (song.sections, song.warnings) = crate::lyrics::parse(&lyrics);
    song.lyrics = lyrics;
    if let Some(key) = chord::shift_key(&song.key, semitones) {
        song.key = key;
    }
    state.drafts.entry(id.clone()).or_insert_with(|| crate::edit::Draft::new(Arc::clone(&open.file), hash)).replace(song);

    // The song now has the new key: the item goes back to zero (its capo stays).
    let mut effects = change(state, |settings| settings.transpose = 0);
    if let Some(song) = state.drafts.get(&id).map(|draft| Arc::clone(&draft.song)) {
        show_song(state, id.clone(), song, hash);
        // What is live from this song is redrawn from the song as it now is.
        let shown = state.song.as_ref().map(|open| Arc::clone(&open.song));
        if let (Some(live), Some(shown)) = (&mut state.output.live, shown)
            && live.song == id
        {
            live.source = shown;
        }
        regrid(state);
    }
    if !state.saving.contains(&id) && !state.save_conflicts.contains(&id) {
        let expect = Expect::Hash(state.drafts.get(&id).map_or(hash, |draft| draft.hash));
        effects.extend(save::save(state, id, expect));
    }
    effects
}
