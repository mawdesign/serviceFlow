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

use std::collections::HashMap;
use std::ops::Range;
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
    /// SPDX identifier: `OFL-1.1` or `Apache-2.0`.
    pub licence_id: &'static str,
    pub licence_text: &'static str,
    data: &'static [u8],
}

macro_rules! bundled {
    ($family:expr, $file:expr, $licence:expr) => {
        Bundled {
            family: $family,
            licence_id: "OFL-1.1",
            licence_text: include_str!(concat!("../fonts/", $licence)),
            data: include_bytes!(concat!("../fonts/", $file)),
        }
    };
}

/// Index in [`BUNDLED`] is the face's [`FontId`] in every [`Fonts`], so ids are stable.
static BUNDLED: [Bundled; 8] = [
    bundled!("Liberation Sans", "LiberationSans-Regular.ttf", "LiberationSans-LICENSE.txt"),
    bundled!("Liberation Serif", "LiberationSerif-Regular.ttf", "LiberationSans-LICENSE.txt"),
    bundled!("Carlito", "Carlito-Regular.ttf", "Carlito-LICENSE.txt"),
    bundled!("Noto Sans", "NotoSans-Regular.ttf", "Noto-LICENSE.txt"),
    bundled!("Noto Sans Arabic", "NotoSansArabic-Regular.ttf", "Noto-LICENSE.txt"),
    bundled!("Noto Sans Hebrew", "NotoSansHebrew-Regular.ttf", "Noto-LICENSE.txt"),
    // Noto CJK's "SubsetOTF" for Simplified Chinese: hanzi (including traditional), kana, hangul
    // and CJK punctuation. 8 MB; the browser build should lazy-load it (#26).
    bundled!("Noto Sans SC", "NotoSansSC-Regular.otf", "Noto-LICENSE.txt"),
    // The SC face has no hangul. This is Noto Sans KR cut down (pyftsubset) to the 2,350 KS X 1001
    // syllables and jamo, to keep it small.
    bundled!("Noto Sans KR", "NotoSansKR-Hangul-Subset.otf", "Noto-LICENSE.txt"),
];

const LIBERATION_SANS: FontId = FontId(0);
const LIBERATION_SERIF: FontId = FontId(1);
const CARLITO: FontId = FontId(2);
const NOTO_SANS: FontId = FontId(3);
/// Tried in this order for a character the chosen face lacks.
const FALLBACKS: [FontId; 5] = [NOTO_SANS, FontId(4), FontId(5), FontId(6), FontId(7)];

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

struct Inner {
    faces: RwLock<Vec<Arc<Face>>>,
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
        Self::new(None)
    }

    /// The bundled faces and the installed fonts `system` knows.
    pub fn with_system_fonts(system: Arc<dyn SystemFonts>) -> Self {
        Self::new(Some(system))
    }

    fn new(system: Option<Arc<dyn SystemFonts>>) -> Self {
        let faces = BUNDLED.iter().map(|b| Arc::new(Face::new(b.family, Data::Static(b.data), 0).expect("bundled fonts parse"))).collect();
        Fonts { inner: Arc::new(Inner { faces: RwLock::new(faces), resolved: Mutex::default(), substitutions: Mutex::default(), system }) }
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
        let id = FontId(u16::try_from(faces.len()).ok()?);
        faces.push(face);
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

    pub(crate) fn face(&self, id: FontId) -> Arc<Face> {
        let faces = self.inner.faces.read().unwrap_or_else(|e| e.into_inner());
        faces.get(usize::from(id.0)).unwrap_or(&faces[0]).clone()
    }

    pub(crate) fn units_per_em(&self, id: FontId) -> f32 {
        self.face(id).units_per_em
    }

    /// Ascent and descent, in pixels at `px` (descent is negative).
    pub(crate) fn vertical_metrics(&self, id: FontId, px: f32) -> (f32, f32) {
        let face = self.face(id);
        (face.ascent * px, face.descent * px)
    }

    /// The font to shape each part of `text` with: `primary` where it has the glyph, else the
    /// first Noto face that does (or `primary`, to draw its missing-glyph box, if none does).
    fn segments(&self, primary: FontId, text: &str) -> Vec<(FontId, Range<usize>)> {
        let primary_face = self.face(primary);
        let fallbacks: Vec<(FontId, Arc<Face>)> = FALLBACKS.iter().map(|id| (*id, self.face(*id))).collect();
        let mut segments: Vec<(FontId, Range<usize>)> = Vec::new();
        for (at, c) in text.char_indices() {
            let end = at + c.len_utf8();
            let font = if is_ignorable(c) && !segments.is_empty() {
                segments[segments.len() - 1].0
            } else if primary_face.has_glyph(c) {
                primary
            } else {
                fallbacks.iter().find(|(_, f)| f.has_glyph(c)).map_or(primary, |(id, _)| *id)
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
            self.face(font).shape(font, &text[range], rtl, px, &mut out);
        }
        out
    }

    /// The advance width of `text` at `px`, shaped left to right.
    pub(crate) fn measure(&self, id: FontId, text: &str, px: f32) -> f32 {
        self.shape(id, text, false, px).iter().map(|g| g.advance).sum()
    }
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
