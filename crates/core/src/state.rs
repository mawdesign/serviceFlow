//! Application state. The engine publishes it as an immutable snapshot (`Arc<State>`); the UI
//! reads a snapshot each frame and never mutates it.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use crate::chord::{KeyInfo, Shift};
use crate::edit::Draft;
use crate::ids::{ContentHash, SetId, SongId};
use crate::output::SlideKind;
use crate::output::{OutputSettings, SlideColors};
use crate::plan::{ChangePlan, PlanAction, PlanFailure, PlanRefusal, PlanRequest};
use crate::resolve::{self, ImageFile, Images, ResolvedStyle};
use crate::search::SearchIndex;
use crate::set::{Advance, ItemKind, ItemTransition, Set};
use crate::set_edit::SetHistory;
use crate::sidecar::{ItemKey, ItemSettings, SetSidecar};
use crate::song::{Slide, SlideOptions, Song};
use crate::style::{ImageSource, Styles};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    /// The song whose slides are shown.
    pub song: Option<OpenSong>,
    /// A song that has been asked for and is still loading.
    pub loading: Option<SongId>,
    /// Songs with edits (not saved), each with its own undo history.
    pub drafts: BTreeMap<SongId, Draft>,
    /// The library: songs, search index and search results.
    pub library: Library,
    /// What the outputs show.
    pub output: Output,
    /// The styles slides are drawn with: imported from OpenSong's PresentSettings on first run
    /// and again on request; serviceFlow's own copy, never written back to OpenSong.
    pub styles: Arc<Styles>,
    /// How the output behaves (logo, initial monitor, transitions, blanks), imported from OpenSong.
    pub output_settings: Arc<OutputSettings>,
    /// Thumbnail colours by slide kind, imported from OpenSong.
    pub slide_colors: Arc<SlideColors>,
    /// Background image files read from `Backgrounds/` for the resolved in use.
    pub images: Arc<Images>,
    /// Whether the initial output mode has been applied (it applies once, at the first import).
    pub(crate) initial_mode_applied: bool,
    /// Problems to show the operator, oldest first.
    pub problems: Vec<Problem>,
    /// How lyrics become slides, for every song opened. `blank_is_slide_change` follows the
    /// imported OpenSong setting until the operator sets it (`SetSlideOptions`).
    pub slide_options: SlideOptions,
    /// The operator set the slide options, so an import no longer changes `blank_is_slide_change`.
    pub(crate) blank_is_slide_change_chosen: bool,
    /// Every set in `Sets/`, sorted.
    pub sets: Vec<SetId>,
    /// The open set (GLOSSARY "Set"), if any.
    pub set: Option<OpenSet>,
    /// A set that has been asked for and is still loading.
    pub loading_set: Option<SetId>,
    /// Songs whose save found the file changed on disk since it was read. The operator chooses
    /// what happens ([`Command::ResolveSaveConflict`](crate::command::Command)); nothing is
    /// written until then.
    pub save_conflicts: Vec<SongId>,
    /// Songs being saved.
    pub saving: Vec<SongId>,
    /// An action whose change plan is being made (storage is being read).
    pub planning: Option<PlanRequest>,
    /// A change plan shown to the operator, waiting for [`Command::ConfirmPlan`](crate::command::Command)
    /// or `CancelPlan`.
    pub plan: Option<Arc<ChangePlan>>,
    /// The confirmed plan is being written.
    pub applying_plan: bool,
    /// The picture files in `Backgrounds/`, listed when the operator looks for a missing image.
    pub backgrounds: Vec<String>,
    /// A zip file replacing the library ([`Command::ImportArchive`](crate::command::Command)):
    /// being imported, or how many files the last import wrote.
    pub archive_import: Option<ArchiveImport>,
    /// The time of the last [`Command::Tick`](crate::command::Command), which the caller measures
    /// (the engine has no clock). Auto-advance is scheduled from it.
    pub(crate) now: Duration,
}

/// Where replacing the library from a zip file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveImport {
    Importing,
    /// Done: this many files were written.
    Imported(usize),
}

/// The open set and where the operator is in it.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenSet {
    pub id: SetId,
    pub set: Arc<Set>,
    /// The item whose slides are shown. Opening a song straight from the library clears it.
    pub selected: Option<usize>,
    /// Songs the set refers to that are not in the library (known once the check has run).
    pub missing: Vec<SongId>,
    /// The song item being relinked: the library search is for its replacement.
    pub relinking: Option<usize>,
    /// The set's serviceFlow-only item settings (ADR-0003), read with the set.
    pub sidecar: Arc<SetSidecar>,
    /// The picture (item, slide) the operator is looking for a file for: choose one of
    /// [`State::backgrounds`].
    pub finding_image: Option<(usize, usize)>,
    /// The set as its file holds it (as read, or as last saved). Edits change [`set`](Self::set) only.
    pub base: Arc<Set>,
    /// The hash of the file's bytes [`base`](Self::base) was read from or saved as. A save writes
    /// only if the file still has it.
    pub hash: ContentHash,
    /// The set differs from its file: a save would write it. Undoing every edit since the last
    /// save makes it unmodified again.
    pub modified: bool,
    /// The set's file changed on disk while it has unsaved changes (or a save found it changed),
    /// so it was not reloaded: the operator overwrites it, reloads it, or saves a copy.
    pub changed_on_disk: bool,
    /// The set is being saved.
    pub saving: bool,
    /// The set's edits, for undo and redo (ADR-0001).
    pub history: SetHistory,
    /// What to send live once the selected item's song has loaded (moving on from the end of
    /// the previous item).
    pub(crate) pending: Option<Pending>,
    /// The songs of the neighbouring song items, read ahead (when blanks are on) to know what a
    /// blank between items looks like.
    pub(crate) peeked: Vec<(SongId, Option<Arc<Song>>)>,
}

/// Which slide to send live when a song loads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pending {
    First,
    Last,
}

impl OpenSet {
    pub(crate) fn new(id: SetId, set: Arc<Set>, hash: ContentHash, sidecar: Arc<SetSidecar>) -> Self {
        Self {
            id,
            base: Arc::clone(&set),
            set,
            hash,
            selected: None,
            missing: Vec::new(),
            relinking: None,
            finding_image: None,
            modified: false,
            changed_on_disk: false,
            saving: false,
            history: SetHistory::default(),
            sidecar,
            pending: None,
            peeked: Vec::new(),
        }
    }

    /// The sidecar key of song item `index` (`None` for any other kind of item).
    pub fn item_key(&self, index: usize) -> Option<ItemKey> {
        let song = self.set.items.get(index)?.song_id()?;
        let occurrence = self.set.items[..index].iter().filter(|item| item.song_id().as_ref() == Some(&song)).count();
        Some(ItemKey { song, occurrence })
    }

    /// The serviceFlow-only settings of item `index` (the defaults for items without any).
    pub fn item_settings(&self, index: usize) -> ItemSettings {
        self.item_key(index).map(|key| self.sidecar.get(&key)).unwrap_or_default()
    }

    /// Whether item `index` is a song item whose song is not in the library.
    pub fn is_missing(&self, index: usize) -> bool {
        self.set.items.get(index).and_then(|item| item.song_id()).is_some_and(|id| self.missing.contains(&id))
    }
}

/// Something that went wrong, for the operator. Structured so the UI words it (through i18n).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// A song could not be read or parsed. `error` is the technical detail.
    SongUnreadable { id: SongId, error: String },
    /// A zip file could not replace the library; nothing was changed unless `error` says the
    /// storage failed while writing.
    ArchiveUnimported { error: String },
    /// The library could not be listed. `error` is the technical detail.
    LibraryUnreadable { error: String },
    /// OpenSong's PresentSettings could not be read when the operator asked for a re-import.
    StylesUnreadable { error: String },
    /// A style names a font that is not installed, so slides use `used` instead (which may wrap
    /// text differently if it does not have the same measurements).
    FontSubstituted { requested: String, used: String },
    /// A set could not be read or parsed.
    SetUnreadable { id: SetId, error: String },
    /// A settings file under `Settings/` could not be imported.
    SettingsUnreadable { file: String, error: String },
    /// A song could not be saved. Its edits are kept.
    SongUnsaved { id: SongId, error: String },
    /// A song was saved, but its section tags were left as written: renaming them to the
    /// canonical form would have made two sections one (ADR-0002).
    TagsNotRenamed { id: SongId },
    /// A set's sidecar (serviceFlow-only item settings) could not be read, so the set opened with
    /// default settings.
    SidecarUnreadable { id: SetId, error: String },
    /// A set's sidecar could not be saved, so its item settings will be lost at the next session.
    SidecarUnsaved { id: SetId, error: String },
    /// No change plan could be made for an action.
    PlanRefused(PlanRefusal),
    /// A confirmed change plan stopped part-way (or, when `failure.written` is empty, before
    /// writing anything). Doing the action again finishes it.
    PlanIncomplete { action: PlanAction, failure: PlanFailure },
    /// A set could not be saved (to `id`). Its edits are kept.
    SetUnsaved { id: SetId, error: String },
    /// A set could not be saved or duplicated under the name `name`: a set with that name exists
    /// (`taken`), or the name is not usable as a file name.
    SetNameRefused { name: String, taken: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpenSong {
    pub id: SongId,
    /// The song shown: the file's song (or its draft), with a set item's presentation order.
    pub song: Arc<Song>,
    /// The song as in its file (or its draft), before any set item's presentation order. Edits
    /// and saves start from this, so an item's order never ends up in the song file.
    pub file: Arc<Song>,
    /// Hash of the file bytes this song was parsed from, recorded on read (conflict detection).
    pub hash: ContentHash,
    /// The slides of the control window's grid: the first [`in_flow`](Self::in_flow) follow the
    /// presentation order; the rest come from sections the order leaves out (shown greyed,
    /// skipped by next/previous, but can be sent live by hand; ADR-0007).
    pub slides: Vec<Slide>,
    /// How many of [`slides`](Self::slides) are in the flow.
    pub in_flow: usize,
    /// `Some` when this is a custom or image item's stand-in song, with the colour kind of its
    /// thumbnails. Its slides are fixed: they do not follow the slide options.
    pub item_kind: Option<SlideKind>,
}

impl State {
    /// Whether the open song is the selected song item of the open set, so that item's
    /// transposition and capo apply to it (and can be changed).
    /// The set item the open song is shown for: the selected item, when the open song is its
    /// (`None`: no set, nothing selected, or a song opened from the library).
    pub fn open_item(&self) -> Option<usize> {
        let (open, set) = (self.song.as_ref()?, self.set.as_ref()?);
        let index = set.selected?;
        (set.set.items.get(index)?.open_id(index).as_ref() == Some(&open.id)).then_some(index)
    }

    /// The index of the live slide among the open song's slides, when it was sent live from this
    /// song shown for this item (the same song elsewhere in the set is another occurrence).
    pub fn open_live_index(&self) -> Option<usize> {
        let live = self.output.live.as_ref()?;
        (self.song.as_ref()?.id == live.song && live.item == self.open_item()).then_some(live.index)
    }

    pub fn is_selected_song_item(&self) -> bool {
        let (Some(open), Some(set)) = (&self.song, &self.set) else { return false };
        set.selected.and_then(|i| set.set.items.get(i)).and_then(|item| item.song_id()).is_some_and(|id| id == open.id)
    }

    /// The keys of the open song as shown for its set item: what it is written in, what sounds
    /// after transposing and the key the shapes are played in with the capo. `None` when the open
    /// song is not a song item of the set.
    pub fn key_info(&self) -> Option<KeyInfo> {
        let open = self.song.as_ref().filter(|_| self.is_selected_song_item())?;
        Some(self.slide_options.shift.key_info(&open.file))
    }

    /// The transposition and capo of the open song (the defaults when it is not a set item).
    pub fn shift(&self) -> Shift {
        self.slide_options.shift
    }

    /// The open song's edits and undo history, once it has been edited.
    pub fn draft(&self) -> Option<&Draft> {
        self.drafts.get(&self.song.as_ref()?.id)
    }

    /// Whether the open song's last save found its file changed on disk (waiting for the
    /// operator to choose).
    pub fn has_save_conflict(&self) -> bool {
        self.song.as_ref().is_some_and(|open| self.save_conflicts.contains(&open.id))
    }

    /// How slide `slide` of song `id` (opened as `song`) is drawn: the style by the precedence of
    /// [`look`](crate::resolve) for the selected set item (if it is this song), plus the song's
    /// background for that slide.
    pub fn resolved_of(&self, id: &SongId, song: &Song, slide: &Slide) -> ResolvedStyle {
        ResolvedStyle { style: self.style_choice(id, song), background: resolve::slide_background(song, slide) }
    }

    /// The style choice for song `id`: from the selected set item when that is this song.
    pub fn style_choice(&self, id: &SongId, song: &Song) -> resolve::StyleChoice {
        let item = self.set.as_ref().and_then(|open| {
            let index = open.selected?;
            (open.set.items.get(index)?.open_id(index).as_ref() == Some(id)).then_some((&*open.set, index))
        });
        resolve::choose(item, Some(song), self.output_settings.song_style_preferred)
    }

    /// How slide `slide` of the item at `index` of the open set is drawn, whatever item is
    /// selected (see [`resolved_of`](Self::resolved_of)).
    pub fn resolved_in_item(&self, index: usize, song: &Song, slide: &Slide) -> ResolvedStyle {
        let item = self.set.as_ref().map(|open| (&*open.set, index));
        ResolvedStyle {
            style: resolve::choose(item, Some(song), self.output_settings.song_style_preferred),
            background: resolve::slide_background(song, slide),
        }
    }

    /// How the open song's item advances and fades (the defaults for songs and for a song opened
    /// from the library).
    pub fn open_advance(&self) -> Advance {
        let item = self.set.as_ref().and_then(|open| {
            let index = open.selected?;
            let item = open.set.items.get(index)?;
            (item.open_id(index).as_ref() == self.song.as_ref().map(|s| &s.id)).then_some(item)
        });
        match item.map(|i| &i.kind) {
            Some(ItemKind::Custom(c)) => c.advance,
            Some(ItemKind::Image(i)) => i.advance,
            _ => Advance::default(),
        }
    }

    /// How long the next change of what is shown crossfades, or `None` for a cut: the output
    /// settings, unless the live item says otherwise (`slide_group/@transition`).
    pub fn fade_ms(&self) -> Option<u32> {
        let settings = &self.output_settings;
        match self.output.live.as_ref().map_or(ItemTransition::Default, |live| live.advance.transition) {
            ItemTransition::Default => settings.fade_ms(),
            ItemTransition::On => Some(settings.transition_ms.max(1)),
            ItemTransition::Off => None,
        }
    }

    /// When the live slide advances by itself, on the clock of [`Command::Tick`](crate::command::Command).
    pub fn auto_advance_due(&self) -> Option<Duration> {
        self.output.auto_due
    }

    /// The first picture of image item `item` whose file is not in `Backgrounds/` (known once it
    /// has been looked for), as its slide index.
    pub fn missing_image(&self, item: usize) -> Option<usize> {
        let ItemKind::Image(image) = &self.set.as_ref()?.set.items.get(item)?.kind else { return None };
        image.slides.iter().position(|slide| match &slide.image {
            Some(ImageSource::File(name)) => matches!(self.images.get(name), Some(ImageFile::Missing)),
            _ => false,
        })
    }

    /// The look of slide `index` of the open song.
    pub fn open_resolved(&self, index: usize) -> Option<ResolvedStyle> {
        let open = self.song.as_ref()?;
        Some(self.resolved_of(&open.id, &open.song, open.slides.get(index)?))
    }
}

impl OpenSong {
    /// Build the grid for `song` with these options.
    pub fn new(id: SongId, song: Arc<Song>, hash: ContentHash, options: &SlideOptions) -> Self {
        let (slides, in_flow) = Self::grid(&song, options);
        Self { id, file: Arc::clone(&song), song, hash, slides, in_flow, item_kind: None }
    }

    /// A custom or image item shown as `slides` (all in the flow). `song` carries the title,
    /// subtitle and backgrounds.
    pub fn of_item(id: SongId, song: Song, slides: Vec<Slide>, kind: SlideKind) -> Self {
        let song = Arc::new(song);
        let in_flow = slides.len();
        Self { id, file: Arc::clone(&song), song, hash: ContentHash::of(&[]), slides, in_flow, item_kind: Some(kind) }
    }

    /// Rebuild the grid with these options, keeping everything else.
    pub fn regrid(&mut self, options: &SlideOptions) {
        if self.item_kind.is_some() {
            return;
        }
        (self.slides, self.in_flow) = Self::grid(&self.song, options);
    }

    /// A song's grid: its flow slides, then the greyed ones, and how many are in the flow.
    pub fn grid(song: &Song, options: &SlideOptions) -> (Vec<Slide>, usize) {
        let mut slides = song.slides(options);
        let in_flow = slides.len();
        slides.extend(song.unused_slides(options));
        (slides, in_flow)
    }

    /// Whether slide `index` follows the presentation order.
    pub fn is_in_flow(&self, index: usize) -> bool {
        index < self.in_flow
    }
}

/// What the outputs show (GLOSSARY "Output", "Live"). Live actions change only this.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Output {
    /// The live slide, if any slide has been sent live.
    pub live: Option<LiveSlide>,
    pub mode: OutputMode,
    /// Counts changes of what is shown, so the output can crossfade exactly when one happens.
    pub changes: u64,
    /// Which outputs show chords (GLOSSARY "Chord line"). All off by default.
    pub chords: OutputChords,
    /// The blank between two items (GLOSSARY "Blank between items") when that is what is shown (then `mode` is
    /// `Blank`). Any other change of what is shown clears it.
    pub item_blank: Option<ItemBlank>,
    /// When the live slide advances by itself (GLOSSARY "Auto-advance"), measured on the clock of
    /// [`Command::Tick`](crate::command::Command). `None`: no auto-advance is running.
    pub auto_due: Option<Duration>,
}

/// The blank between two set items: stepping past the end of item `before`, or back past the
/// start of item `after`, lands on it.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemBlank {
    pub before: usize,
    pub after: usize,
    /// The look of its background: the item before's, or the item after's with `blank_uses_next`.
    pub resolved: ResolvedStyle,
}

/// The outputs a slide is shown on, each with its own chord toggle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputId {
    /// The full-screen output window.
    Window,
    /// The small output monitor in the control window.
    Monitor,
}

/// Whether each output shows chords. Off by default: the congregation sees only lyrics unless the
/// operator chooses. Control-window thumbnails and the editor preview always show chords.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OutputChords {
    pub window: bool,
    pub monitor: bool,
}

impl OutputChords {
    pub fn get(&self, output: OutputId) -> bool {
        match output {
            OutputId::Window => self.window,
            OutputId::Monitor => self.monitor,
        }
    }

    pub fn toggle(&mut self, output: OutputId) {
        match output {
            OutputId::Window => self.window = !self.window,
            OutputId::Monitor => self.monitor = !self.monitor,
        }
    }
}

/// A slide that is live. It keeps its own copy, so opening another song does not change it.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveSlide {
    /// The song it came from.
    pub song: SongId,
    /// The set item it was sent live from (`None`: a song opened from the library). With the
    /// same song twice in a set, this tells the occurrences apart.
    pub item: Option<usize>,
    /// Its position in that song's slides.
    pub index: usize,
    pub slide: Slide,
    /// The song, for the title and subtitle boxes.
    pub source: Arc<Song>,
    /// The style and song background it was sent live with.
    pub resolved: ResolvedStyle,
    /// How its item advances and fades, captured when it went live.
    pub advance: Advance,
    /// The transposition and capo it was drawn with (the item's, when it went live).
    pub shift: Shift,
}

/// How the outputs show the live slide (GLOSSARY "Blank", "Black").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OutputMode {
    /// The live slide.
    #[default]
    Normal,
    /// The style's background with no text.
    Blank,
    /// Pure black.
    Black,
    /// The configured logo image.
    Logo,
}

/// The library (GLOSSARY "Library") and the search over it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Library {
    /// Every song, sorted by id (so by category, then filename).
    pub songs: Vec<SongId>,
    pub index: Arc<SearchIndex>,
    /// `true` while songs are still being read for the index.
    pub indexing: bool,
    /// Songs read so far.
    pub indexed: usize,
    /// The current query, as typed.
    pub query: String,
    /// Songs matching `query`, best first. Empty when the query is blank.
    pub results: Vec<SongId>,
    /// Which refresh the in-flight results belong to; older results are dropped.
    pub generation: u64,
    pub(crate) queue: VecDeque<SongId>,
}

impl Library {
    /// The title to show for a song (its filename until the song has been read).
    pub fn title<'a>(&'a self, id: &'a SongId) -> &'a str {
        self.index.title(id)
    }
}
