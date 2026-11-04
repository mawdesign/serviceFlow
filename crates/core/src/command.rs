//! Commands and effects (ADR-0001).
//!
//! A [`Command`] is the only way to change state. The UI, the file watcher, storage results and
//! future automation all send the same commands to the [`Engine`](crate::engine::Engine).
//! Applying a command may ask for IO by returning [`Effect`]s; the runtime performs them and
//! sends the outcome back as another command, so the engine itself never waits.

use std::sync::Arc;

use crate::edit::SongEdit;
use crate::ids::{ContentHash, Expect, SetId, SongId};
use crate::output::{OutputSettings, SlideColors};
use crate::plan::{ChangePlan, GatheredSet, PlanFailure, WrittenSong};
use crate::set::Set;
use crate::song::{LanguageMode, SlideOptions, Song};
use crate::style::Styles;

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Open a song from the library and show its slides.
    OpenSong(SongId),
    /// Storage result: a song requested by [`Effect::LoadSong`] was read and parsed. `hash` is the hash of the
    /// bytes read, kept so a later save can detect that the file changed meanwhile.
    SongLoaded {
        id: SongId,
        song: Arc<Song>,
        hash: ContentHash,
    },
    /// Storage result: a song requested by [`Effect::LoadSong`] could not be read or parsed.
    SongLoadFailed {
        id: SongId,
        error: String,
    },
    /// Read the list of songs and index them in the background (also after the library changed).
    RefreshLibrary,
    /// Storage result: the songs in the library. `generation` identifies the refresh that asked.
    LibraryListed {
        generation: u64,
        songs: Vec<SongId>,
    },
    /// Storage result: the library could not be listed.
    LibraryListFailed {
        generation: u64,
        error: String,
    },
    /// Storage result: a chunk of songs was read for the index; `None` = unreadable (stays
    /// searchable by filename).
    SongsIndexed {
        generation: u64,
        songs: Vec<(SongId, Option<Arc<Song>>)>,
    },
    /// Set the library search query. A blank query clears the results.
    Search(String),
    /// Send slide `index` of the open song live. Ignored when there is no such slide.
    GoLive {
        index: usize,
    },
    /// Send the first slide of the open song's section named `token` live, whether or not the
    /// presentation order lists it (a greyed section). The token is a tag as typed: `3` finds
    /// `V3` or `3`, `C` finds `C` or `C1`; tags match without regard to case. Ignored when the
    /// song has no such section.
    JumpToSection {
        token: String,
    },
    /// Go live with the slide after the live one (the first slide when nothing is live). Stays
    /// put on the last slide.
    NextSlide,
    /// Go live with the slide before the live one. Does nothing on the first slide or when
    /// nothing is live.
    PreviousSlide,
    /// Toggle blank mode (the style's background, no text).
    ToggleBlank,
    /// Toggle black mode (pure black).
    ToggleBlack,
    /// Toggle logo mode (the configured logo image).
    ToggleLogo,
    /// Read the output behaviour (PresentSettings: logo, monitor, initial mode, transitions) and the
    /// thumbnail colours (MainSettings) from OpenSong, read-only. Sent on first run and by the
    /// "re-import from OpenSong" action.
    ImportOpenSongSettings {
        trigger: ImportTrigger,
    },
    /// Storage result of [`Effect::ReadOpenSongSettings`]: what could be read (`None` where the file
    /// is missing or unreadable) and what went wrong.
    SettingsImported {
        trigger: ImportTrigger,
        output: Option<Arc<OutputSettings>>,
        colors: Option<Arc<SlideColors>>,
        faults: Vec<SettingsFault>,
    },
    /// Read the default and scripture styles from OpenSong's PresentSettings (read-only).
    /// Sent on first run and by the "re-import from OpenSong" action.
    ImportOpenSongStyles {
        trigger: ImportTrigger,
    },
    /// Storage result: PresentSettings was read and parsed.
    StylesImported(Arc<Styles>),
    /// Storage result: PresentSettings could not be read. `missing` is true when the file does
    /// not exist.
    StylesImportFailed {
        trigger: ImportTrigger,
        error: String,
        missing: bool,
    },
    /// Storage result: a background image file was read (`None`: it could not be).
    ImageLoaded {
        name: String,
        bytes: Option<Arc<[u8]>>,
    },
    /// The renderer drew `used` instead of the style's font `requested`. Becomes a
    /// [`Problem::FontSubstituted`](crate::state::Problem) the operator sees; repeats are ignored.
    FontSubstituted {
        requested: String,
        used: String,
    },
    /// Change how lyrics become slides (such as OpenSong's `blank_is_slide_change`). The open
    /// song's slides are rebuilt; what is live stays live.
    SetSlideOptions(SlideOptions),
    /// Show both lines of multilanguage sections, primary lines only or translation lines only
    /// (GLOSSARY "Language mode"). The open song's slides and the live slide change at once.
    SetLanguageMode(LanguageMode),
    /// Change the open song in memory (the song editor). Nothing is saved (ADR-0002). Consecutive
    /// edits of the same field, or of the lyrics, are one undo step.
    EditSong(SongEdit),
    /// Stop joining edits: the next edit starts a new undo step (sent when the editor field
    /// loses focus).
    EndEditGroup,
    /// Undo the last edit of the open song. Each song has its own history; live actions are
    /// never in it (ADR-0001).
    Undo,
    Redo,
    /// Read the list of sets.
    RefreshSets,
    /// Storage result: the sets in `Sets/`.
    SetsListed(Vec<SetId>),
    /// Storage result: `Sets/` could not be listed.
    SetsListFailed {
        error: String,
    },
    /// Start looking for a file for picture `slide` of image item `item` (selects the item and
    /// lists `Backgrounds/`): the operator then chooses one with [`Command::RelinkImage`].
    FindImage { item: usize, slide: usize },
    CancelFindImage,
    /// Point picture `slide` of image item `item` at `file` under `Backgrounds/`. Changes the open
    /// set in memory only (saving sets is a later ticket).
    RelinkImage { item: usize, slide: usize, file: String },
    /// Storage result: the picture files in `Backgrounds/`.
    BackgroundsListed(Vec<String>),
    /// Open a set. The current one stays open if this one cannot be read.
    OpenSet(SetId),
    /// Storage result: a set was read and parsed.
    SetLoaded {
        id: SetId,
        set: Arc<Set>,
    },
    /// Storage result: a set could not be read or parsed.
    SetLoadFailed {
        id: SetId,
        error: String,
    },
    /// Storage result: of the songs a set refers to, these are not in the library.
    SongsChecked {
        set: SetId,
        missing: Vec<SongId>,
    },
    /// Show the slides of item `index` of the open set. Nothing goes live.
    SelectItem(usize),
    /// Select the item after / before the selected one (any kind).
    NextItem,
    PreviousItem,
    /// Start looking for a song to replace the missing one of item `index`: the library search
    /// is set to the item's name.
    StartRelink(usize),
    /// Stop relinking without changing anything.
    CancelRelink,
    /// Point song item `item` at `song` (kept in the item's notes, order and style).
    RelinkItem {
        item: usize,
        song: SongId,
    },
    /// Save the song's edits in canonical form (ADR-0002). Nothing is written when the song has
    /// no edits, or while an earlier save waits for a conflict to be resolved. The file is only
    /// written if it is unchanged since it was read.
    SaveSong(SongId),
    /// Storage result: the edits of song `id` were written to `saved_as` (`id` itself, or a new
    /// copy), which now holds `song` (as read back) with `hash`. `tags_kept`: the tags were not
    /// renamed because they would have collided.
    SongSaved {
        id: SongId,
        saved_as: SongId,
        song: Arc<Song>,
        hash: ContentHash,
        tags_kept: bool,
    },
    /// Storage result: the song's file changed on disk since it was read, so nothing was written.
    SongSaveConflict {
        id: SongId,
    },
    /// Storage result: the song could not be written.
    SongSaveFailed {
        id: SongId,
        error: String,
    },
    /// The operator's answer to a save conflict of song `id`.
    ResolveSaveConflict {
        id: SongId,
        choice: ConflictChoice,
    },
    /// Rename or move a song file (another filename and/or category). The song's title is not
    /// changed. Makes a change plan that also rewrites every set item referring to the song;
    /// nothing is written until [`Command::ConfirmPlan`].
    RenameSong {
        from: SongId,
        to: SongId,
    },
    /// Delete a song file. Makes a change plan that lists the sets still using it, as a warning;
    /// nothing is deleted until [`Command::ConfirmPlan`].
    DeleteSong(SongId),
    /// Create a song titled `title` in `category` (`""`: the top level). Its filename is the
    /// sanitised title plus `.xml`, with ` (2)`-style suffixes when taken. Makes a change plan.
    NewSong {
        category: String,
        title: String,
    },
    /// Storage result of [`Effect::GatherForPlan`]: the song list, every readable set with its
    /// hash, the sets that could not be read, and the hash of the plan's song (`None`: missing).
    /// `error`: the library could not be listed.
    PlanGathered {
        songs: Vec<SongId>,
        sets: Vec<GatheredSet>,
        unreadable_sets: Vec<SetId>,
        subject: Option<ContentHash>,
        error: Option<String>,
    },
    /// Apply the change plan the operator has been shown.
    ConfirmPlan,
    /// Drop the change plan; nothing is written.
    CancelPlan,
    /// Storage result of [`Effect::ApplyPlan`]: every step was written (with the songs written,
    /// as read back), or why it stopped.
    PlanApplied {
        plan: Arc<ChangePlan>,
        result: Result<Vec<WrittenSong>, PlanFailure>,
    },
}

/// What to do when a song's file changed on disk since it was read and the song has edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictChoice {
    /// Write the edits over the file anyway.
    Overwrite,
    /// Drop the edits and read the file again.
    Reload,
    /// Write the edits to a new file beside it (`<name> (copy)`), leaving the file alone.
    SaveAsCopy,
}

/// A settings file that could not be imported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsFault {
    /// The file under `Settings/`.
    pub file: String,
    pub error: String,
    /// The file does not exist.
    pub missing: bool,
}

/// Why styles are being imported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportTrigger {
    /// Automatically at start. A data root without PresentSettings is not a problem.
    FirstRun,
    /// The operator asked. Any failure is reported.
    Manual,
}

impl Command {
    /// Document edits are what undo covers: the opposite of live actions.
    pub fn is_document_edit(&self) -> bool {
        matches!(self, Command::EditSong(_) | Command::Undo | Command::Redo)
    }

    /// Live actions are transient: they never enter undo history (ADR-0001). Only document edits
    /// do, and none exist yet.
    pub fn is_live_action(&self) -> bool {
        matches!(
            self,
            Command::GoLive { .. }
                | Command::JumpToSection { .. }
                | Command::NextSlide
                | Command::PreviousSlide
                | Command::ToggleBlank
                | Command::ToggleBlack
                | Command::ToggleLogo
                | Command::SetLanguageMode(_)
        )
    }
}

/// IO the engine asks for. Each effect ends with exactly one result command.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Write `song` in canonical form to song `id` if the file meets `expect`; with `as_copy`,
    /// to the first free copy name of `id` ([`SongId::copy`]) instead. Answer with
    /// [`Command::SongSaved`], [`Command::SongSaveConflict`] (the file changed or vanished) or
    /// [`Command::SongSaveFailed`].
    WriteSong { id: SongId, song: Arc<Song>, expect: Expect, as_copy: bool },
    /// List the songs, read and parse every set and read the hash of song `subject`, to make a
    /// change plan; answer with [`Command::PlanGathered`].
    GatherForPlan { subject: Option<SongId> },
    /// Check every step of the plan against storage, then write them in order; answer with
    /// [`Command::PlanApplied`].
    ApplyPlan(Arc<ChangePlan>),
    /// Read a file of `Backgrounds/`; answer with [`Command::ImageLoaded`].
    LoadImage(String),
    /// Read and parse `Settings/PresentSettings`; answer with [`Command::StylesImported`] or
    /// [`Command::StylesImportFailed`]. Never writes.
    ReadOpenSongStyles(ImportTrigger),
    /// Read `Settings/PresentSettings` and `Settings/MainSettings` for the output behaviour and slide
    /// colours; answer with [`Command::SettingsImported`]. Never writes.
    ReadOpenSongSettings(ImportTrigger),
    /// Read and parse a song; answer with [`Command::SongLoaded`] or [`Command::SongLoadFailed`].
    LoadSong(SongId),
    /// List the library; answer with [`Command::LibraryListed`] or [`Command::LibraryListFailed`].
    ListLibrary { generation: u64 },
    /// Read these songs for the search index; answer with [`Command::SongsIndexed`].
    IndexSongs { generation: u64, ids: Vec<SongId> },
    /// List the sets; answer with [`Command::SetsListed`] or [`Command::SetsListFailed`].
    ListSets,
    /// List the picture files in `Backgrounds/`; answer with [`Command::BackgroundsListed`].
    ListBackgrounds,
    /// Read and parse a set; answer with [`Command::SetLoaded`] or [`Command::SetLoadFailed`].
    LoadSet(SetId),
    /// Find which of these songs are not in the library; answer with [`Command::SongsChecked`].
    CheckSetSongs { set: SetId, ids: Vec<SongId> },
}
