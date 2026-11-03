//! Output behaviour imported from OpenSong's settings (opensong.md §13, §14): the logo, the
//! initial output monitor and mode, transitions, and the colours of slide thumbnails.

use std::sync::Arc;

use crate::song::SectionKind;
use crate::style::Rgba;

/// What the output shows when serviceFlow starts (`style/@initial_mode`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InitialMode {
    /// The live slide (`N`; also `W` and `F`, which have no equivalent).
    #[default]
    Normal,
    /// Pure black (`B`).
    Black,
    /// The style's background alone (`H`).
    Blank,
    /// The logo (`L`).
    Logo,
}

/// The logo: base64 text of an image file (PNG or JPEG), as stored in PresentSettings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Logo {
    pub image: Arc<str>,
    /// An optional mask image of the same size (OpenSong's `logo_mask`); kept, not applied yet.
    pub mask: Option<Arc<str>>,
}

/// How the output behaves, from PresentSettings. Read-only import; the operator's own choices
/// (such as the output monitor) win over it once made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputSettings {
    pub initial_mode: InitialMode,
    /// The initial output monitor, 0-based (OpenSong's `monitors/@present` is 1-based).
    pub monitor: Option<usize>,
    /// Insert a blank between set items and at the end (`style/@blanks`).
    pub blanks: bool,
    /// Whether that blank takes the style of the next item (`style/@blank_uses_next`).
    pub blank_uses_next: bool,
    /// Crossfade when the slide changes (`style/@transition`).
    pub transition: bool,
    /// Crossfade time in milliseconds (`style/@transition_time`; 0 means 100 in OpenSong).
    pub transition_ms: u32,
    pub logo: Option<Logo>,
    /// A song's own style beats the style stack (`song_style_preferred`; on unless the file
    /// says otherwise).
    pub song_style_preferred: bool,
}

impl Default for OutputSettings {
    fn default() -> Self {
        Self {
            initial_mode: InitialMode::Normal,
            monitor: None,
            blanks: false,
            blank_uses_next: false,
            transition: false,
            transition_ms: 100,
            logo: None,
            song_style_preferred: true,
        }
    }
}

impl OutputSettings {
    /// How long a slide change crossfades, or `None` when transitions are off.
    pub fn fade_ms(&self) -> Option<u32> {
        (self.transition && self.transition_ms > 0).then_some(self.transition_ms)
    }
}

/// The kinds of thumbnail OpenSong colour-codes (`slide_style_color`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlideKind {
    Verse,
    PreChorus,
    Chorus,
    Bridge,
    Tag,
    Custom,
    Scripture,
    Style,
    Image,
    External,
    Blank,
}

impl SlideKind {
    /// The kind of a song slide from its section tag.
    pub fn of_tag(tag: &str) -> Self {
        match SectionKind::of_tag(tag) {
            SectionKind::Verse => Self::Verse,
            SectionKind::PreChorus => Self::PreChorus,
            SectionKind::Chorus => Self::Chorus,
            SectionKind::Bridge => Self::Bridge,
            SectionKind::Tag => Self::Tag,
        }
    }
}

/// Thumbnail colours by slide kind, imported from MainSettings `slide_style_color`. The defaults
/// are OpenSong's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlideColors {
    /// `slide_style_color/@enabled`: when false, thumbnails are not colour-coded.
    pub enabled: bool,
    pub verse: Rgba,
    pub pre_chorus: Rgba,
    pub chorus: Rgba,
    pub bridge: Rgba,
    pub tag: Rgba,
    pub custom: Rgba,
    pub scripture: Rgba,
    pub style: Rgba,
    pub image: Rgba,
    pub external: Rgba,
    pub blank: Rgba,
}

impl Default for SlideColors {
    fn default() -> Self {
        Self {
            enabled: true,
            verse: Rgba::rgb(213, 213, 255),
            pre_chorus: Rgba::rgb(113, 113, 255),
            chorus: Rgba::rgb(188, 188, 255),
            bridge: Rgba::rgb(138, 138, 255),
            tag: Rgba::rgb(163, 163, 255),
            custom: Rgba::rgb(255, 227, 213),
            scripture: Rgba::rgb(255, 180, 180),
            style: Rgba::rgb(234, 234, 255),
            image: Rgba::rgb(255, 255, 180),
            external: Rgba::rgb(213, 255, 213),
            blank: Rgba::rgb(255, 255, 255),
        }
    }
}

impl SlideColors {
    /// The colour for `kind`, or `None` when colour-coding is switched off.
    pub fn color(&self, kind: SlideKind) -> Option<Rgba> {
        self.enabled.then(|| *self.slot(kind))
    }

    /// The colour for a song slide with section tag `tag`.
    pub fn for_tag(&self, tag: &str) -> Option<Rgba> {
        self.color(SlideKind::of_tag(tag))
    }

    /// Set the colour of one kind.
    pub fn set(&mut self, kind: SlideKind, color: Rgba) {
        *self.slot_mut(kind) = color;
    }

    fn slot(&self, kind: SlideKind) -> &Rgba {
        match kind {
            SlideKind::Verse => &self.verse,
            SlideKind::PreChorus => &self.pre_chorus,
            SlideKind::Chorus => &self.chorus,
            SlideKind::Bridge => &self.bridge,
            SlideKind::Tag => &self.tag,
            SlideKind::Custom => &self.custom,
            SlideKind::Scripture => &self.scripture,
            SlideKind::Style => &self.style,
            SlideKind::Image => &self.image,
            SlideKind::External => &self.external,
            SlideKind::Blank => &self.blank,
        }
    }

    fn slot_mut(&mut self, kind: SlideKind) -> &mut Rgba {
        match kind {
            SlideKind::Verse => &mut self.verse,
            SlideKind::PreChorus => &mut self.pre_chorus,
            SlideKind::Chorus => &mut self.chorus,
            SlideKind::Bridge => &mut self.bridge,
            SlideKind::Tag => &mut self.tag,
            SlideKind::Custom => &mut self.custom,
            SlideKind::Scripture => &mut self.scripture,
            SlideKind::Style => &mut self.style,
            SlideKind::Image => &mut self.image,
            SlideKind::External => &mut self.external,
            SlideKind::Blank => &mut self.blank,
        }
    }
}
