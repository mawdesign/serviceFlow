//! The engine: the single writer of [`State`] (ADR-0001).

use std::sync::Arc;

use crate::command::{Command, Effect, ImportTrigger};

mod edit;
mod external;
mod plan;
mod save;
mod sets;

use crate::ids::{ContentHash, SongId};
use crate::output::InitialMode;
use crate::resolve::{self, ImageFile, ResolvedStyle};
use crate::search::SearchIndex;
use crate::song::Song;
use crate::state::{Library, LiveSlide, OpenSong, OutputMode, Problem, State};
use crate::style::ImageSource;

#[derive(Debug, Default)]
pub struct Engine {
    state: Arc<State>,
}

impl Engine {
    pub fn new() -> Self {
        Self::default()
    }

    /// The current state. A snapshot never changes; applying a command publishes a new one.
    pub fn snapshot(&self) -> Arc<State> {
        Arc::clone(&self.state)
    }

    /// Apply one command and return the IO it asks for.
    pub fn apply(&mut self, command: Command) -> Vec<Effect> {
        // Copy-on-write: snapshots handed out earlier keep the old state.
        let state = Arc::make_mut(&mut self.state);
        let mut effects = step(state, command);
        effects.extend(request_images(state));
        effects
    }
}

fn step(state: &mut State, command: Command) -> Vec<Effect> {
    {
        match command {
            Command::OpenSong(id) => {
                // A song opened by hand is not a set item: no override, nothing pending.
                if let Some(set) = &mut state.set {
                    set.selected = None;
                    set.pending = None;
                }
                state.loading = Some(id.clone());
                vec![Effect::LoadSong(id)]
            }
            Command::SongLoaded { id, song, hash } => {
                // A result for a song that is no longer wanted (another was opened since) is dropped.
                if state.loading.as_ref() == Some(&id) {
                    state.loading = None;
                    // An edited song stays as edited. A draft without edits whose file has
                    // changed since is stale: the file wins (ADR-0001).
                    let file = match state.drafts.get(&id) {
                        Some(draft) if draft.is_modified() || draft.hash == hash => {
                            // Edits over a file that has since changed wait for the operator.
                            let stale = draft.hash != hash;
                            let song = Arc::clone(&draft.song);
                            if stale {
                                external::mark_changed_on_disk(state, &id);
                            }
                            song
                        }
                        _ => {
                            state.drafts.remove(&id);
                            song
                        }
                    };
                    show_song(state, id, file, hash);
                    sets::go_pending(state);
                }
                Vec::new()
            }
            Command::SongLoadFailed { id, error } => {
                if state.loading.as_ref() == Some(&id) {
                    state.loading = None;
                    if let Some(set) = &mut state.set {
                        set.pending = None;
                    }
                }
                state.problems.push(Problem::SongUnreadable { id, error });
                Vec::new()
            }
            Command::RefreshLibrary => {
                state.library.generation += 1;
                state.library.queue.clear();
                state.library.indexing = true;
                vec![Effect::ListLibrary { generation: state.library.generation }]
            }
            Command::LibraryListed { generation, mut songs } => {
                let lib = &mut state.library;
                if generation != lib.generation {
                    return Vec::new();
                }
                songs.sort();
                songs.dedup();
                let mut index = SearchIndex::default();
                index.reset(&songs);
                lib.index = Arc::new(index);
                lib.queue = songs.iter().cloned().collect();
                lib.songs = songs;
                lib.indexed = 0;
                refresh_results(lib);
                next_chunk(lib)
            }
            Command::LibraryListFailed { generation, error } => {
                if generation == state.library.generation {
                    state.library.indexing = false;
                    state.problems.push(Problem::LibraryUnreadable { error });
                }
                Vec::new()
            }
            Command::SongsIndexed { generation, songs } => {
                let lib = &mut state.library;
                if generation != lib.generation {
                    return Vec::new();
                }
                let index = Arc::make_mut(&mut lib.index);
                for (id, song) in &songs {
                    if let Some(song) = song {
                        index.insert(id, song);
                    }
                }
                lib.indexed += songs.len();
                refresh_results(lib);
                next_chunk(lib)
            }
            Command::Search(query) => {
                state.library.query = query;
                refresh_results(&mut state.library);
                Vec::new()
            }
            Command::FontSubstituted { requested, used } => {
                let problem = Problem::FontSubstituted { requested, used };
                if !state.problems.contains(&problem) {
                    state.problems.push(problem);
                }
                Vec::new()
            }
            Command::GoLive { index } => {
                go_live(state, index);
                Vec::new()
            }
            Command::JumpToSection { token } => {
                if let Some(index) = section_slide(state, &token) {
                    go_live(state, index);
                }
                Vec::new()
            }
            Command::NextSlide => {
                // Only slides in the flow; a greyed slide is past its end.
                let in_flow = state.song.as_ref().map_or(0, |open| open.in_flow);
                let live = live_index(state);
                let target = live.map_or(0, |i| i + 1);
                if target < in_flow {
                    go_live(state, target);
                    Vec::new()
                } else if state.output_settings.blanks
                    && live_index(state).is_some_and(|i| i + 1 == in_flow)
                    && state.output.mode == OutputMode::Normal
                {
                    // OpenSong puts a blank after each item.
                    toggle(state, OutputMode::Blank);
                    state.output.blank_after_end = true;
                    Vec::new()
                } else if live.is_some() {
                    // Past the end of the item: carry on into the next song item of the set.
                    sets::advance(state, true)
                } else {
                    Vec::new()
                }
            }
            Command::PreviousSlide if state.output.blank_after_end && state.output.mode == OutputMode::Blank => {
                // Back from the blank after the last slide: that slide again.
                toggle(state, OutputMode::Blank);
                Vec::new()
            }
            Command::PreviousSlide => {
                // From a greyed slide, back to the last slide of the flow.
                let in_flow = state.song.as_ref().map_or(0, |open| open.in_flow);
                match live_index(state) {
                    Some(live) => match live.min(in_flow).checked_sub(1) {
                        Some(i) => {
                            go_live(state, i);
                            Vec::new()
                        }
                        // Before the first slide: back into the previous song item of the set.
                        None => sets::advance(state, false),
                    },
                    None => Vec::new(),
                }
            }
            Command::SetSlideOptions(options) => {
                state.slide_options = options;
                if let Some(open) = &mut state.song {
                    open.regrid(&options);
                }
                Vec::new()
            }
            Command::SetLanguageMode(language) => {
                state.slide_options.language = language;
                let options = state.slide_options;
                if let Some(open) = &mut state.song {
                    open.regrid(&options);
                }
                // The live slide is redrawn in the new mode, wherever it came from.
                if let Some(live) = &mut state.output.live
                    && let Some(slide) = OpenSong::grid(&live.source, &options).0.get(live.index)
                    && *slide != live.slide
                {
                    live.slide = slide.clone();
                    live.resolved.background = resolve::slide_background(&live.source, slide);
                    state.output.changes += 1;
                }
                Vec::new()
            }
            Command::ToggleBlank => {
                toggle(state, OutputMode::Blank);
                Vec::new()
            }
            Command::ToggleBlack => {
                toggle(state, OutputMode::Black);
                Vec::new()
            }
            command @ (Command::EditSong(_) | Command::EndEditGroup | Command::Undo | Command::Redo) => edit::apply(state, command),
            command @ (Command::RefreshSets
            | Command::SetsListed(_)
            | Command::SetsListFailed { .. }
            | Command::OpenSet(_)
            | Command::SetLoaded { .. }
            | Command::SetLoadFailed { .. }
            | Command::SongsChecked { .. }
            | Command::SelectItem(_)
            | Command::NextItem
            | Command::PreviousItem
            | Command::StartRelink(_)
            | Command::CancelRelink
            | Command::RelinkItem { .. }) => sets::apply(state, command),
            command @ (Command::DocumentsChanged { .. } | Command::SongReloaded { .. } | Command::SetReloaded { .. }) => {
                external::apply(state, command)
            }
            command @ (Command::SaveSong(_)
            | Command::SongSaved { .. }
            | Command::SongSaveConflict { .. }
            | Command::SongSaveFailed { .. }
            | Command::ResolveSaveConflict { .. }) => save::apply(state, command),
            command @ (Command::RenameSong { .. }
            | Command::DeleteSong(_)
            | Command::NewSong { .. }
            | Command::PlanGathered { .. }
            | Command::ConfirmPlan
            | Command::CancelPlan
            | Command::PlanApplied { .. }) => plan::apply(state, command),
            Command::ToggleLogo => {
                toggle(state, OutputMode::Logo);
                Vec::new()
            }
            Command::ImportOpenSongSettings { trigger } => vec![Effect::ReadOpenSongSettings(trigger)],
            Command::SettingsImported { trigger, output, colors, faults } => {
                if let Some(colors) = colors {
                    state.slide_colors = colors;
                }
                if let Some(output) = output {
                    // The initial mode applies once; after that the operator's choice wins.
                    if !state.initial_mode_applied {
                        state.initial_mode_applied = true;
                        match output.initial_mode {
                            InitialMode::Normal => {}
                            InitialMode::Black => toggle(state, OutputMode::Black),
                            InitialMode::Blank => toggle(state, OutputMode::Blank),
                            InitialMode::Logo => toggle(state, OutputMode::Logo),
                        }
                    }
                    state.output_settings = output;
                }
                for fault in faults {
                    if trigger == ImportTrigger::Manual || !fault.missing {
                        state.problems.push(Problem::SettingsUnreadable { file: fault.file, error: fault.error });
                    }
                }
                Vec::new()
            }
            Command::ImageLoaded { name, bytes } => {
                let file = bytes.map_or(ImageFile::Missing, ImageFile::Loaded);
                Arc::make_mut(&mut state.images).set(name, file);
                Vec::new()
            }
            Command::ImportOpenSongStyles { trigger } => vec![Effect::ReadOpenSongStyles(trigger)],
            Command::StylesImported(styles) => {
                state.styles = styles;
                Vec::new()
            }
            Command::StylesImportFailed { trigger, error, missing } => {
                if trigger == ImportTrigger::Manual || !missing {
                    state.problems.push(Problem::StylesUnreadable { error });
                }
                Vec::new()
            }
        }
    }
}

/// Make `file` (a song as in its file, or its draft) the open song, shown with the selected set
/// item's presentation order when it is that item's song.
fn show_song(state: &mut State, id: SongId, file: Arc<Song>, hash: ContentHash) {
    let shown = sets::with_item_order(state, &id, Arc::clone(&file));
    let mut open = OpenSong::new(id, shown, hash, &state.slide_options);
    open.file = file;
    state.song = Some(open);
}

/// Ask for the background image files the resolved in use need and that nobody has asked for yet.
fn request_images(state: &mut State) -> Vec<Effect> {
    let mut wanted: Vec<String> = Vec::new();
    let mut want = |name: &str| {
        if !wanted.iter().any(|w| w == name) {
            wanted.push(name.to_owned());
        }
    };
    for style in [&state.styles.default, &state.styles.scripture] {
        resolve::file_of(style).into_iter().for_each(&mut want);
    }
    if let Some(open) = &state.song {
        // The style every slide of the song has, then each of its background files.
        let choice = state.style_choice(&open.id, &open.song);
        ResolvedStyle { style: choice, background: None }.files().for_each(&mut want);
        if let Some(backgrounds) = &open.song.backgrounds {
            for item in &backgrounds.items {
                if let ImageSource::File(name) = &item.image {
                    want(name);
                }
            }
        }
    }
    if let Some(live) = &state.output.live {
        live.resolved.files().for_each(&mut want);
    }
    wanted.retain(|name| state.images.get(name).is_none());
    if wanted.is_empty() {
        return Vec::new();
    }
    let images = Arc::make_mut(&mut state.images);
    wanted
        .into_iter()
        .map(|name| {
            images.set(name.clone(), ImageFile::Loading);
            Effect::LoadImage(name)
        })
        .collect()
}

/// The live slide's index, when it belongs to the open song.
fn live_index(state: &State) -> Option<usize> {
    let live = state.output.live.as_ref()?;
    (state.song.as_ref()?.id == live.song).then_some(live.index)
}

/// The grid index of the first slide of the open song's section called `token`.
fn section_slide(state: &State, token: &str) -> Option<usize> {
    let open = state.song.as_ref()?;
    let token = token.trim();
    let section = open.song.section(token).or_else(|| {
        // `C` finds a chorus tagged `C1` when the song has no plain `C`.
        let numbered = format!("{token}1");
        token.chars().all(|c| c.is_ascii_alphabetic()).then(|| open.song.section(&numbered)).flatten()
    })?;
    open.slides.iter().position(|slide| slide.section.eq_ignore_ascii_case(&section.tag))
}

/// Make slide `index` of the open song live and leave blank or black mode. Out of range: no change.
fn go_live(state: &mut State, index: usize) {
    let Some(open) = &state.song else { return };
    let Some(slide) = open.slides.get(index) else { return };
    let resolved = state.resolved_of(&open.id, &open.song, slide);
    state.output.live = Some(LiveSlide { song: open.id.clone(), index, slide: slide.clone(), source: Arc::clone(&open.song), resolved });
    state.output.mode = OutputMode::Normal;
    state.output.blank_after_end = false;
    state.output.changes += 1;
}

/// Switch `mode` on, or back to normal when it is already on. Blank and black replace each other.
fn toggle(state: &mut State, mode: OutputMode) {
    state.output.mode = if state.output.mode == mode { OutputMode::Normal } else { mode };
    state.output.blank_after_end = false;
    state.output.changes += 1;
}

/// Songs read per background step: small enough that the window stays responsive.
const INDEX_CHUNK: usize = 25;

fn next_chunk(lib: &mut Library) -> Vec<Effect> {
    let n = lib.queue.len().min(INDEX_CHUNK);
    if n == 0 {
        lib.indexing = false;
        return Vec::new();
    }
    let ids: Vec<_> = lib.queue.drain(..n).collect();
    vec![Effect::IndexSongs { generation: lib.generation, ids }]
}

fn refresh_results(lib: &mut Library) {
    lib.results = lib.index.search(&lib.query);
}
