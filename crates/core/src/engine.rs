//! The engine: the single writer of [`State`] (ADR-0001).

use std::sync::Arc;
use std::time::Duration;

use crate::command::{Command, Effect, ImportTrigger};

mod auto;
mod blanks;
mod edit;
mod external;
mod key;
mod plan;
mod save;
mod set_edit;
mod sets;
mod tidy;

use crate::ids::{ContentHash, SongId};
use crate::output::InitialMode;
use crate::resolve::{self, ImageFile, ResolvedStyle};
use crate::search::SearchIndex;
use crate::set::ItemKind;
use crate::song::{SlideOptions, Song};
use crate::state::{ArchiveImport, Library, LiveSlide, OpenSong, OutputMode, Problem, State};
use crate::style::ImageSource;

#[derive(Debug, Default)]
pub struct Engine {
    state: Arc<State>,
    /// The time of the last tick; not in the state, so a tick that changes nothing copies nothing.
    now: Duration,
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
        if let Command::Tick { now } = command {
            self.now = now;
            // Most ticks find nothing due: leave the state (and the snapshots of it) alone.
            if !self.state.output.auto_due.is_some_and(|due| due <= now) {
                return Vec::new();
            }
        }
        // Copy-on-write: snapshots handed out earlier keep the old state.
        let state = Arc::make_mut(&mut self.state);
        state.now = self.now;
        let mut effects = step(state, command);
        effects.extend(request_images(state));
        effects.extend(blanks::peeks(state));
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
            Command::Tick { now } => {
                auto::tick(state, now);
                Vec::new()
            }
            Command::SongPeeked { id, song } => {
                blanks::peeked(state, &id, song);
                Vec::new()
            }
            Command::GoToItemBlank => {
                if let Some((before, after)) = blanks::across(state, true) {
                    blanks::enter(state, before, after);
                }
                Vec::new()
            }
            Command::NextSlide if blanks::on_item_blank(state) => {
                // From the blank between two items into the later one.
                let after = state.output.item_blank.as_ref().map_or(0, |b| b.after);
                blanks::leave(state, after, true)
            }
            Command::NextSlide => {
                // Only slides in the flow; a greyed slide is past its end.
                let in_flow = state.song.as_ref().map_or(0, |open| open.in_flow);
                let live = live_index(state);
                let target = live.map_or(0, |i| i + 1);
                if target < in_flow {
                    go_live(state, target);
                    Vec::new()
                } else if let Some((before, after)) = blanks::across(state, true)
                    && live.is_some_and(|i| i + 1 == in_flow)
                    && state.output.mode == OutputMode::Normal
                {
                    // A blank between this item and the next.
                    blanks::enter(state, before, after);
                    Vec::new()
                } else if live.is_some() {
                    // Past the end of the item: carry on into the next song item of the set.
                    sets::advance(state, true)
                } else {
                    Vec::new()
                }
            }
            Command::PreviousSlide if blanks::on_item_blank(state) => {
                // Back from the blank between two items into the earlier one.
                let before = state.output.item_blank.as_ref().map_or(0, |b| b.before);
                blanks::leave(state, before, false)
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
                        // Before the first slide: the blank before this item, else the previous item.
                        None => match blanks::across(state, false) {
                            Some((before, after)) => {
                                blanks::enter(state, before, after);
                                Vec::new()
                            }
                            None => sets::advance(state, false),
                        },
                    },
                    None => Vec::new(),
                }
            }
            Command::SetSlideOptions(options) => {
                // The item's transposition is not the caller's to set (SetTranspose/SetCapo).
                state.slide_options = SlideOptions { shift: state.slide_options.shift, ..options };
                // From now on the operator's choice wins over OpenSong's setting.
                state.blank_is_slide_change_chosen = true;
                let options = state.slide_options;
                if let Some(open) = &mut state.song {
                    open.regrid(&options);
                }
                Vec::new()
            }
            Command::SetLanguageMode(language) => {
                state.slide_options.language = language;
                let effects = sets::remember_language(state, language);
                regrid(state);
                effects
            }
            command @ (Command::SetTranspose(_) | Command::SetCapo(_) | Command::SaveInThisKey) => key::apply(state, command),
            Command::ToggleBlank => {
                toggle(state, OutputMode::Blank);
                Vec::new()
            }
            Command::ToggleBlack => {
                toggle(state, OutputMode::Black);
                Vec::new()
            }
            Command::EndEditGroup => {
                set_edit::end_group(state);
                edit::apply(state, Command::EndEditGroup)
            }
            command @ (Command::EditSong(_) | Command::Undo | Command::Redo) => edit::apply(state, command),
            command @ (Command::EditSet(_)
            | Command::UndoSet
            | Command::RedoSet
            | Command::SaveSet
            | Command::SaveSetAs { .. }
            | Command::DuplicateSet { .. }
            | Command::ResolveSetConflict(_)
            | Command::SetSaved { .. }
            | Command::SetSaveConflict { .. }
            | Command::SetSaveFailed { .. }
            | Command::ListBackgrounds) => set_edit::apply(state, command),
            command @ (Command::RefreshSets
            | Command::SetsListed(_)
            | Command::SetsListFailed { .. }
            | Command::OpenSet(_)
            | Command::SetLoaded { .. }
            | Command::SidecarSaved { .. }
            | Command::SidecarSaveFailed { .. }
            | Command::SetLoadFailed { .. }
            | Command::SongsChecked { .. }
            | Command::SelectItem(_)
            | Command::NextItem
            | Command::PreviousItem
            | Command::StartRelink(_)
            | Command::CancelRelink
            | Command::RelinkItem { .. }
            | Command::FindImage { .. }
            | Command::CancelFindImage
            | Command::RelinkImage { .. }
            | Command::BackgroundsListed(_)) => sets::apply(state, command),
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
            | Command::TidyLibrary
            | Command::TidyGathered { .. }
            | Command::ConfirmPlan
            | Command::CancelPlan
            | Command::PlanApplied { .. }) => plan::apply(state, command),
            Command::ImportArchive(zip) => {
                if state.archive_import == Some(ArchiveImport::Importing) {
                    return Vec::new();
                }
                state.archive_import = Some(ArchiveImport::Importing);
                vec![Effect::ImportArchive(zip)]
            }
            Command::ArchiveImported(result) => archive_imported(state, result),
            Command::ToggleChords(output) => {
                state.output.chords.toggle(output);
                Vec::new()
            }
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
                // OpenSong's `blank_is_slide_change` is global (copied into every style); it
                // applies unless the operator chose otherwise (SetSlideOptions).
                let blank = styles.default.body.blank_is_slide_change;
                state.styles = styles;
                if !state.blank_is_slide_change_chosen && state.slide_options.blank_is_slide_change != blank {
                    state.slide_options.blank_is_slide_change = blank;
                    regrid(state);
                }
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

/// Every file was replaced by an imported zip (or nothing was). Storage sent no change events,
/// so what was open is closed and everything is read again; the live slide stays on the output.
fn archive_imported(state: &mut State, result: Result<usize, String>) -> Vec<Effect> {
    match result {
        Ok(count) => {
            state.archive_import = Some(ArchiveImport::Imported(count));
            state.song = None;
            state.loading = None;
            state.drafts.clear();
            state.set = None;
            state.loading_set = None;
            state.save_conflicts.clear();
            state.saving.clear();
            state.images = Arc::default();
            let mut effects = step(state, Command::RefreshLibrary);
            effects.extend(step(state, Command::RefreshSets));
            effects.extend(step(state, Command::ImportOpenSongStyles { trigger: ImportTrigger::FirstRun }));
            effects.extend(step(state, Command::ImportOpenSongSettings { trigger: ImportTrigger::FirstRun }));
            effects
        }
        Err(error) => {
            state.archive_import = None;
            state.problems.push(Problem::ArchiveUnimported { error });
            Vec::new()
        }
    }
}

/// Make `file` (a song as in its file, or its draft) the open song, shown with the selected set
/// item's presentation order when it is that item's song.
fn show_song(state: &mut State, id: SongId, file: Arc<Song>, hash: ContentHash) {
    let shown = sets::with_item_order(state, &id, Arc::clone(&file));
    state.slide_options.shift = sets::item_shift(state, &id);
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
    // Every picture of the open set's image items, so a missing one is known (and flagged) before
    // the operator gets to it.
    if let Some(open) = &state.set {
        for item in &open.set.items {
            if let ItemKind::Image(image) = &item.kind {
                for slide in &image.slides {
                    if let Some(ImageSource::File(name)) = &slide.image {
                        want(name);
                    }
                }
            }
        }
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

/// The live slide's index, when it belongs to the open song shown for the same set item.
fn live_index(state: &State) -> Option<usize> {
    state.open_live_index()
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
    let advance = state.open_advance();
    let shift = state.slide_options.shift;
    let item = state.open_item();
    state.output.live =
        Some(LiveSlide { song: open.id.clone(), item, index, slide: slide.clone(), source: Arc::clone(&open.song), resolved, advance, shift });
    state.output.mode = OutputMode::Normal;
    state.output.item_blank = None;
    state.output.changes += 1;
    auto::schedule(state);
}

/// The slide options changed: rebuild the open song's grid and redraw the live slide in them,
/// wherever it came from. The open song's transposition goes to a live slide of the same song
/// and set item; another song's (or occurrence's) live slide keeps its own.
fn regrid(state: &mut State) {
    let options = state.slide_options;
    if let Some(open) = &mut state.song {
        open.regrid(&options);
    }
    let live_is_open = state.open_live_index().is_some();
    if let Some(live) = &mut state.output.live {
        if live_is_open {
            live.shift = options.shift;
        }
        let options = SlideOptions { shift: live.shift, ..options };
        if let Some(slide) = OpenSong::grid(&live.source, &options).0.get(live.index)
            && *slide != live.slide
        {
            live.slide = slide.clone();
            live.resolved.background = resolve::slide_background(&live.source, slide);
            state.output.changes += 1;
        }
    }
}

/// Switch `mode` on, or back to normal when it is already on. Blank and black replace each other.
fn toggle(state: &mut State, mode: OutputMode) {
    state.output.mode = if state.output.mode == mode { OutputMode::Normal } else { mode };
    state.output.item_blank = None;
    state.output.changes += 1;
    // Blank, black and logo stop auto-advance; showing the slide again starts it afresh.
    auto::schedule(state);
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
