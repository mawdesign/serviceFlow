//! The faces the renderer can use, how a style's font name finds one, and shaping with HarfRust.
//!
//! A font name resolves in this order (spec "Fonts"):
//!
//! 1. A face added with [`Fonts::add`], or an installed font from the [`SystemFonts`] provider
//!    (the app wires `craft-font-catalog` in; the renderer itself stays free of it).
//! 2. A bundled face that is in the bundle under that name, or is its metric twin (Carlito for
//!    Calibri, Liberation Sans for Arial, Liberation Serif for Times New Roman), so lines wrap the
//!    same as with the original font. The twin is a *substitution*.
//! 3. Noto Sans, also a substitution.
//!
//! On top of that, any character the chosen face has no glyph for is shaped with a Noto face that
//! has one (CJK, Hebrew, Arabic, ...), so text never shows missing-glyph boxes.
//!
//! Every bundled face is licensed under the SIL OFL 1.1; the licence files are in `fonts/` and
//! listed by [`Fonts::bundled`].
//!
//! **Lazy faces.** The CJK faces are large (Noto Sans SC alone is 8 MB), so the browser build does
//! not compile them in. Until the app [`provide`](Fonts::provide)s one, text that needs it is laid
//! out with blank one-em placeholders, the face is listed by [`Fonts::take_wanted`] (for the app
//! to fetch) and [`Fonts::waiting`] is true (for the app to say so). [`Fonts::generation`] changes
//! when a face arrives, so cached pictures can be redrawn. Natively every face is compiled in;
//! [`Fonts::builtin_lazy`] behaves like the browser, for tests.

use std::collections::{BTreeSet, HashMap};
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use harfrust::{Direction, FontRef, ShapeOptions, ShaperData, UnicodeBuffer};
use skrifa::MetadataProvider as _;
use skrifa::instance::{LocationRef, Size};

/// A face in a [`Fonts`] collection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontId(pub u16);

/// A bundled face and its licence.
#[derive(Clone, Copy, Debug)]
pub struct Bundled {
    pub family: &'static str,
    /// The font file's name in `crates/render/fonts/`. The browser build serves lazy faces as
    /// `fonts/<file>` next to the page.
    pub file: &'static str,
    /// SPDX identifier: `OFL-1.1` or `Apache-2.0`.
    pub licence_id: &'static str,
    pub licence_text: &'static str,
    /// Loaded on demand in the browser (module docs, "Lazy faces").
    pub lazy: bool,
    /// The characters a lazy face is fetched for, as inclusive ranges. Empty for other faces.
    covers: &'static [(char, char)],
    /// `None` when the face is not compiled in (lazy faces in the browser build).
    data: Option<&'static [u8]>,
}

macro_rules! bundled {
    ($family:expr, $file:expr, $licence:expr) => {
        Bundled {
            family: $family,
            file: $file,
            licence_id: "OFL-1.1",
            licence_text: include_str!(concat!("../fonts/", $licence)),
            lazy: false,
            covers: &[],
            data: Some(include_bytes!(concat!("../fonts/", $file))),
        }
    };
}

/// A face fetched on demand in the browser; natively it is compiled in like the others.
#[cfg(not(target_arch = "wasm32"))]
macro_rules! lazy_bundled {
    ($family:expr, $file:expr, $licence:expr, $covers:expr) => {
        Bundled { lazy: true, covers: $covers, ..bundled!($family, $file, $licence) }
    };
}

#[cfg(target_arch = "wasm32")]
macro_rules! lazy_bundled {
    ($family:expr, $file:expr, $licence:expr, $covers:expr) => {
        Bundled {
            family: $family,
            file: $file,
            licence_id: "OFL-1.1",
            licence_text: include_str!(concat!("../fonts/", $licence)),
            lazy: true,
            covers: $covers,
            data: None,
        }
    };
}

/// Han (simplified and traditional), kana, bopomofo, CJK punctuation and full-width forms.
const CJK: &[(char, char)] = &[
    ('\u{2E80}', '\u{2FFF}'),
    ('\u{3000}', '\u{312F}'),
    ('\u{31A0}', '\u{4DBF}'),
    ('\u{4E00}', '\u{9FFF}'),
    ('\u{F900}', '\u{FAFF}'),
    ('\u{FE30}', '\u{FE4F}'),
    ('\u{FF00}', '\u{FFEF}'),
    ('\u{20000}', '\u{2FA1F}'),
];

/// Hangul syllables and jamo.
const HANGUL: &[(char, char)] = &[('\u{1100}', '\u{11FF}'), ('\u{3130}', '\u{318F}'), ('\u{A960}', '\u{A97F}'), ('\u{AC00}', '\u{D7FF}')];

/// Index in [`BUNDLED`] is the face's [`FontId`] in every [`Fonts`], so ids are stable.
static BUNDLED: [Bundled; 8] = [
    bundled!("Liberation Sans", "LiberationSans-Regular.ttf", "LiberationSans-LICENSE.txt"),
    bundled!("Liberation Serif", "LiberationSerif-Regular.ttf", "LiberationSans-LICENSE.txt"),
    bundled!("Carlito", "Carlito-Regular.ttf", "Carlito-LICENSE.txt"),
    bundled!("Noto Sans", "NotoSans-Regular.ttf", "Noto-LICENSE.txt"),
    bundled!("Noto Sans Arabic", "NotoSansArabic-Regular.ttf", "Noto-LICENSE.txt"),
    bundled!("Noto Sans Hebrew", "NotoSansHebrew-Regular.ttf", "Noto-LICENSE.txt"),
    // Noto CJK's "SubsetOTF" for Simplified Chinese: hanzi (including traditional), kana, hangul
    // and CJK punctuation. 8 MB; the browser build fetches it when text needs it (#26).
    lazy_bundled!("Noto Sans SC", "NotoSansSC-Regular.otf", "Noto-LICENSE.txt", CJK),
    // The SC face has no hangul. This is Noto Sans KR cut down (pyftsubset) to the 2,350 KS X 1001
    // syllables and jamo, to keep it small.
    lazy_bundled!("Noto Sans KR", "NotoSansKR-Hangul-Subset.otf", "Noto-LICENSE.txt", HANGUL),
];

const LIBERATION_SANS: FontId = FontId(0);
const LIBERATION_SERIF: FontId = FontId(1);
const CARLITO: FontId = FontId(2);
const NOTO_SANS: FontId = FontId(3);
/// Tried in this order for a character the chosen face lacks.
const FALLBACKS: [FontId; 5] = [NOTO_SANS, FontId(4), FontId(5), FontId(6), FontId(7)];
/// Marks text waiting for a lazy face; it is laid out as blank one-em placeholders.
const PLACEHOLDER: FontId = FontId(u16::MAX);

/// Fonts that are not in the bundle but have a bundled face with the same measurements.
const TWINS: &[(&str, FontId)] = &[
    ("calibri", CARLITO),
    ("arial", LIBERATION_SANS),
    ("helvetica", LIBERATION_SANS),
    ("times new roman", LIBERATION_SERIF),
    ("times", LIBERATION_SERIF),
];

/// Installed fonts, provided by the app (the renderer may not depend on `craft-font-catalog`).
pub trait SystemFonts: Send + Sync {
    /// The regular face of `family` if it is installed: the font file and the face's index in it.
    fn load(&self, family: &str) -> Option<(Arc<[u8]>, u32)>;
}

/// A style's font that could not be used as named. Shown to the operator as a warning.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Substitution {
    pub requested: String,
    /// The family drawn instead.
    pub used: String,
}

#[derive(Clone)]
enum Data {
    Static(&'static [u8]),
    Shared(Arc<[u8]>),
}

impl Data {
    fn bytes(&self) -> &[u8] {
        match self {
            Data::Static(b) => b,
            Data::Shared(b) => b,
        }
    }
}

pub(crate) struct Face {
    family: String,
    data: Data,
    index: u32,
    shaper: OnceLock<ShaperData>,
    units_per_em: f32,
    /// Ascent and descent per em (descent is negative).
    ascent: f32,
    descent: f32,
}

impl Face {
    fn new(family: &str, data: Data, index: u32) -> Option<Face> {
        let font = FontRef::from_index(data.bytes(), index).ok()?;
        let metrics = font.metrics(Size::unscaled(), LocationRef::default());
        let upem = f32::from(metrics.units_per_em);
        if upem <= 0.0 {
            return None;
        }
        Some(Face {
            family: family.to_owned(),
            ascent: metrics.ascent / upem,
            descent: metrics.descent / upem,
            units_per_em: upem,
            shaper: OnceLock::new(),
            index,
            data,
        })
    }

    pub(crate) fn font_ref(&self) -> FontRef<'_> {
        FontRef::from_index(self.data.bytes(), self.index).expect("checked when added")
    }

    fn has_glyph(&self, c: char) -> bool {
        self.font_ref().charmap().map(c).is_some()
    }

    fn shape(&self, id: FontId, text: &str, rtl: bool, px: f32, out: &mut Vec<Shaped>) {
        let font = self.font_ref();
        let data = self.shaper.get_or_init(|| ShaperData::new(&font));
        let shaper = data.shaper(&font).build();
        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        buffer.set_direction(if rtl { Direction::RightToLeft } else { Direction::LeftToRight });
        let glyphs = shaper.shape(buffer, ShapeOptions::new());
        let scale = px / self.units_per_em;
        out.extend(glyphs.glyph_infos().iter().zip(glyphs.glyph_positions()).map(|(info, pos)| Shaped {
            font: id,
            id: info.glyph_id,
            advance: pos.x_advance as f32 * scale,
            x_offset: pos.x_offset as f32 * scale,
            y_offset: pos.y_offset as f32 * scale,
        }));
    }
}

/// One shaped glyph, in pixels at the size it was shaped for.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Shaped {
    pub font: FontId,
    pub id: u32,
    pub advance: f32,
    pub x_offset: f32,
    pub y_offset: f32,
}

#[derive(Default)]
struct Substitutions {
    all: Vec<Substitution>,
    reported: usize,
}

#[derive(Default)]
struct Lazy {
    /// Lazy faces some text needed that are not loaded.
    needed: BTreeSet<FontId>,
    /// Those already handed out by `take_wanted`.
    asked: BTreeSet<FontId>,
    /// Faces that could not be fetched: their placeholders stay, nothing waits for them.
    failed: BTreeSet<FontId>,
}

struct Inner {
    /// `None`: a lazy face that is not loaded.
    faces: RwLock<Vec<Option<Arc<Face>>>>,
    lazy: Mutex<Lazy>,
    generation: AtomicU64,
    /// Lower-case requested family -> face.
    resolved: Mutex<HashMap<String, FontId>>,
    substitutions: Mutex<Substitutions>,
    system: Option<Arc<dyn SystemFonts>>,
}

/// The faces available to the layout. Cheap to clone: clones share the faces and the record of
/// substitutions, so thumbnails and the output can use one collection.
#[derive(Clone)]
pub struct Fonts {
    inner: Arc<Inner>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Fonts {
    /// The bundled faces only.
    pub fn builtin() -> Self {
        Self::new(None, false)
    }

    /// The bundled faces without the lazy ones until they are [`provide`](Self::provide)d: what
    /// the browser build has. For tests; in the browser [`builtin`](Self::builtin) is the same.
    pub fn builtin_lazy() -> Self {
        Self::new(None, true)
    }

    /// The bundled faces and the installed fonts `system` knows.
    pub fn with_system_fonts(system: Arc<dyn SystemFonts>) -> Self {
        Self::new(Some(system), false)
    }

    fn new(system: Option<Arc<dyn SystemFonts>>, lazy: bool) -> Self {
        let faces = BUNDLED
            .iter()
            .map(|b| match b.data {
                Some(data) if !(lazy && b.lazy) => Some(Arc::new(Face::new(b.family, Data::Static(data), 0).expect("bundled fonts parse"))),
                _ => None,
            })
            .collect();
        Fonts {
            inner: Arc::new(Inner {
                faces: RwLock::new(faces),
                lazy: Mutex::default(),
                generation: AtomicU64::new(0),
                resolved: Mutex::default(),
                substitutions: Mutex::default(),
                system,
            }),
        }
    }

    fn is_loaded(&self, id: FontId) -> bool {
        self.inner.faces.read().unwrap_or_else(|e| e.into_inner()).get(usize::from(id.0)).is_some_and(Option::is_some)
    }

    /// Note that text needs the lazy face `id`, which is not loaded.
    fn need(&self, id: FontId) {
        let mut lazy = lock(&self.inner.lazy);
        if !lazy.failed.contains(&id) {
            lazy.needed.insert(id);
        }
    }

    /// Lazy faces that text has needed since the last call and that are not loaded: fetch
    /// `fonts/<file>` and [`provide`](Self::provide) it. Each face is listed once.
    pub fn take_wanted(&self) -> Vec<&'static Bundled> {
        let mut lazy = lock(&self.inner.lazy);
        let new: Vec<FontId> = lazy.needed.difference(&lazy.asked).copied().collect();
        lazy.asked.extend(new.iter().copied());
        new.into_iter().map(|id| &BUNDLED[usize::from(id.0)]).collect()
    }

    /// True while some text is drawn with placeholders for a face that is still on its way.
    pub fn waiting(&self) -> bool {
        !lock(&self.inner.lazy).needed.is_empty()
    }

    /// Lazy faces that could not be fetched ([`font_failed`](Self::font_failed)).
    pub fn failed(&self) -> Vec<&'static Bundled> {
        lock(&self.inner.lazy).failed.iter().map(|id| &BUNDLED[usize::from(id.0)]).collect()
    }

    /// Changes whenever a lazy face arrives (or fails), so pictures drawn before can be redrawn.
    pub fn generation(&self) -> u64 {
        self.inner.generation.load(Ordering::Acquire)
    }

    /// Load the lazy face stored in `file` (a [`Bundled::file`]) from its bytes. False if `file`
    /// is no lazy face or the bytes are not a font.
    pub fn provide(&self, file: &str, data: Arc<[u8]>) -> bool {
        let Some(index) = BUNDLED.iter().position(|b| b.lazy && b.file == file) else { return false };
        let Some(face) = Face::new(BUNDLED[index].family, Data::Shared(data), 0) else { return false };
        self.inner.faces.write().unwrap_or_else(|e| e.into_inner())[index] = Some(Arc::new(face));
        let id = FontId(index as u16);
        let mut lazy = lock(&self.inner.lazy);
        lazy.needed.remove(&id);
        lazy.asked.remove(&id);
        lazy.failed.remove(&id);
        drop(lazy);
        self.inner.generation.fetch_add(1, Ordering::AcqRel);
        true
    }

    /// The lazy face stored in `file` could not be fetched: stop waiting for it. Its text keeps
    /// its placeholders.
    pub fn font_failed(&self, file: &str) {
        let Some(index) = BUNDLED.iter().position(|b| b.lazy && b.file == file) else { return };
        let id = FontId(index as u16);
        let mut lazy = lock(&self.inner.lazy);
        lazy.needed.remove(&id);
        lazy.failed.insert(id);
        drop(lazy);
        self.inner.generation.fetch_add(1, Ordering::AcqRel);
    }

    /// The bundled faces with their licences.
    pub fn bundled() -> &'static [Bundled] {
        &BUNDLED
    }

    /// Adds a face from font file bytes; `None` if they are not a font. A later [`Fonts::resolve`]
    /// of `family` finds it before anything else.
    pub fn add(&self, family: &str, data: Arc<[u8]>) -> Option<FontId> {
        self.add_face(family, Data::Shared(data), 0)
    }

    fn add_face(&self, family: &str, data: Data, index: u32) -> Option<FontId> {
        let face = Arc::new(Face::new(family, data, index)?);
        let mut faces = self.inner.faces.write().unwrap_or_else(|e| e.into_inner());
        let id = FontId(u16::try_from(faces.len()).ok().filter(|n| *n != PLACEHOLDER.0)?);
        faces.push(Some(face));
        drop(faces);
        lock(&self.inner.resolved).insert(family.to_ascii_lowercase(), id);
        Some(id)
    }

    /// The face for a family name, following the order in the module docs. Never fails: the last
    /// resort is Noto Sans. A substitution is recorded (once per name) for
    /// [`take_substitutions`](Self::take_substitutions).
    pub fn resolve(&self, family: &str) -> FontId {
        self.resolve_detail(family).0
    }

    fn resolve_detail(&self, family: &str) -> (FontId, bool) {
        let key = family.trim().to_ascii_lowercase();
        if let Some(id) = lock(&self.inner.resolved).get(&key) {
            return (*id, self.is_substitute(*id, &key));
        }
        // A bundled face asked for by its own name is used as is, so the default style looks the
        // same everywhere and never waits for the system font scan.
        let by_name = BUNDLED.iter().position(|b| b.family.eq_ignore_ascii_case(&key));
        // 1. Installed.
        if by_name.is_none()
            && let Some((data, index)) = self.inner.system.as_ref().and_then(|s| s.load(family.trim()))
            && let Some(id) = self.add_face(family.trim(), Data::Shared(data), index)
        {
            return (id, false);
        }
        // A lazy face asked for by name that is not here yet: Noto Sans meanwhile (CJK text gets
        // placeholders), decided again (not cached) once it has arrived.
        if let Some(i) = by_name.filter(|i| !self.is_loaded(FontId(*i as u16))) {
            self.need(FontId(i as u16));
            return (NOTO_SANS, false);
        }
        // 2. In the bundle under that name, or its metric twin.
        let (id, substituted) = match by_name {
            Some(i) => (FontId(i as u16), false),
            None => match TWINS.iter().find(|(name, _)| *name == key) {
                Some((_, id)) => (*id, true),
                // 3. Noto.
                None => (NOTO_SANS, true),
            },
        };
        lock(&self.inner.resolved).insert(key, id);
        if substituted {
            let used = self.family(id).to_owned();
            let mut subs = lock(&self.inner.substitutions);
            let substitution = Substitution { requested: family.trim().to_owned(), used };
            if !subs.all.contains(&substitution) {
                subs.all.push(substitution);
            }
        }
        (id, substituted)
    }

    /// A bundled face reached through a different name than its own is a substitute.
    fn is_substitute(&self, id: FontId, key: &str) -> bool {
        usize::from(id.0) < BUNDLED.len() && !BUNDLED[usize::from(id.0)].family.eq_ignore_ascii_case(key)
    }

    /// What would be drawn instead of `family`, if it cannot be used as named. Does not wait for
    /// anything to be rendered, so a style editor can show the warning as the font is chosen.
    pub fn substitution(&self, family: &str) -> Option<Substitution> {
        let (id, substituted) = self.resolve_detail(family);
        substituted.then(|| Substitution { requested: family.trim().to_owned(), used: self.family(id).to_owned() })
    }

    /// Substitutions made since the last call, to report into the state.
    pub fn take_substitutions(&self) -> Vec<Substitution> {
        let mut subs = lock(&self.inner.substitutions);
        let new = subs.all[subs.reported..].to_vec();
        subs.reported = subs.all.len();
        new
    }

    /// Every substitution made so far.
    pub fn substitutions(&self) -> Vec<Substitution> {
        lock(&self.inner.substitutions).all.clone()
    }

    pub fn family(&self, id: FontId) -> String {
        self.face(id).family.clone()
    }

    /// The face `id`, or the default face for an id that is not loaded.
    pub(crate) fn face(&self, id: FontId) -> Arc<Face> {
        let faces = self.inner.faces.read().unwrap_or_else(|e| e.into_inner());
        match faces.get(usize::from(id.0)) {
            Some(Some(face)) => face.clone(),
            _ => faces[usize::from(LIBERATION_SANS.0)].clone().expect("the default face is always loaded"),
        }
    }

    pub(crate) fn units_per_em(&self, id: FontId) -> f32 {
        self.face(id).units_per_em
    }

    /// Ascent and descent, in pixels at `px` (descent is negative).
    pub fn vertical_metrics(&self, id: FontId, px: f32) -> (f32, f32) {
        let face = self.face(id);
        (face.ascent * px, face.descent * px)
    }

    /// The font to shape each part of `text` with: `primary` where it has the glyph, else the
    /// first Noto face that does (or `primary`, to draw its missing-glyph box, if none does). A
    /// lazy face that is not loaded counts as having the characters it covers; they get
    /// [`PLACEHOLDER`] until it arrives.
    fn segments(&self, primary: FontId, text: &str) -> Vec<(FontId, Range<usize>)> {
        let primary_face = self.face(primary);
        let fallbacks: Vec<(FontId, Option<Arc<Face>>)> = FALLBACKS.iter().map(|id| (*id, self.is_loaded(*id).then(|| self.face(*id)))).collect();
        let mut segments: Vec<(FontId, Range<usize>)> = Vec::new();
        for (at, c) in text.char_indices() {
            let end = at + c.len_utf8();
            let font = if is_ignorable(c) && !segments.is_empty() {
                segments[segments.len() - 1].0
            } else if primary_face.has_glyph(c) {
                primary
            } else {
                let found = fallbacks.iter().find_map(|(id, face)| match face {
                    Some(face) => face.has_glyph(c).then_some(*id),
                    None => BUNDLED[usize::from(id.0)].covers.iter().any(|(lo, hi)| (*lo..=*hi).contains(&c)).then(|| {
                        self.need(*id);
                        PLACEHOLDER
                    }),
                });
                found.unwrap_or(primary)
            };
            match segments.last_mut() {
                Some((last, range)) if *last == font => range.end = end,
                _ => segments.push((font, at..end)),
            }
        }
        segments
    }

    /// Shapes `text` in one direction, at `px` pixels per em, with `primary` and per-character
    /// fallback. Glyphs come out in visual order (left to right on the page).
    pub(crate) fn shape(&self, primary: FontId, text: &str, rtl: bool, px: f32) -> Vec<Shaped> {
        let mut segments = self.segments(primary, text);
        if rtl {
            segments.reverse();
        }
        let mut out = Vec::new();
        for (font, range) in segments {
            if font == PLACEHOLDER {
                placeholders(&self.face(primary), primary, &text[range], px, &mut out);
            } else {
                self.face(font).shape(font, &text[range], rtl, px, &mut out);
            }
        }
        out
    }

    /// The advance width of `text` at `px`, shaped left to right.
    pub(crate) fn measure(&self, id: FontId, text: &str, px: f32) -> f32 {
        self.shape(id, text, false, px).iter().map(|g| g.advance).sum()
    }
}

/// Blank one-em glyphs (the primary face's space) standing in for characters whose face is still
/// loading: about the width a CJK face gives them, so lines barely move when it arrives.
fn placeholders(face: &Face, id: FontId, text: &str, px: f32, out: &mut Vec<Shaped>) {
    let space = face.font_ref().charmap().map(' ').map_or(0, |g| g.to_u32());
    out.extend(text.chars().map(|c| Shaped { font: id, id: space, advance: if is_ignorable(c) { 0.0 } else { px }, x_offset: 0.0, y_offset: 0.0 }));
}

/// Characters that take the font of the character before them (joiners, variation selectors).
fn is_ignorable(c: char) -> bool {
    matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{FE00}'..='\u{FE0F}' | '\u{FEFF}')
}

impl Default for Fonts {
    fn default() -> Self {
        Fonts::builtin()
    }
}
