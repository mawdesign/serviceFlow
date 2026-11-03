//! The laid-out frame: everything the drawing step needs, and what snapshot tests inspect.
//!
//! All positions are absolute, in output pixels, with the origin at the frame's top left and y
//! growing downwards.

use std::fmt::Write as _;
use std::ops::Range;
use std::sync::Arc;

use craft_text::bidi::{Direction, Paragraph};
use craft_text::linebreak::{Item, Params, Rag, break_paragraph};
use craft_text::segment::{Options, line_breaks};
use serviceflow_core::song::Slide;
use serviceflow_core::style::{Align, Captions, ImagePosition, REFERENCE_HEIGHT, Resize, Rgba, Style, TextStyle, VAlign};

use crate::fonts::{FontId, Fonts};
use crate::logo::Picture;

/// Distance between baselines, as a multiple of the font size.
pub const LINE_PITCH: f32 = 1.2;

/// An output size in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// A slide laid out for one output size.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub size: Size,
    pub background: Rgba,
    /// The background image, drawn over `background` and under the text.
    pub image: Option<FrameImage>,
    pub boxes: Vec<TextBox>,
}

/// How a background image is placed on the frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Fit {
    /// Stretched to the area, ignoring proportions.
    Stretch,
    /// As large as fits inside the area, keeping proportions, centred.
    Contain,
    /// As small as covers the area, keeping proportions, centred and cropped.
    Cover,
    /// At natural size (scaled with the output like all lengths), centred.
    Natural,
    /// At natural size, repeated over the whole frame.
    Tile,
}

/// A background image and where it goes.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameImage {
    pub picture: Arc<Picture>,
    pub fit: Fit,
    /// The area it is fitted to, in frame pixels.
    pub area: Rect,
    /// Output height over [`REFERENCE_HEIGHT`], for the natural size.
    pub scale: f32,
}

/// What a text box holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BoxRole {
    /// The slide's lyrics.
    Body,
    /// The song title.
    Title,
    /// The song subtitle (author and so on).
    Subtitle,
}

/// A box of lines placed on the frame.
#[derive(Clone, Debug, PartialEq)]
pub struct TextBox {
    pub role: BoxRole,
    /// The area the lines are laid out in (inside the margins).
    pub rect: Rect,
    pub lines: Vec<Line>,
}

/// What a line holds. Lines of different roles may have different sizes and styles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LineRole {
    /// A lyric line; in a multilanguage slide, a primary line.
    Lyric,
    /// A translation line of a multilanguage slide, beneath its primary line.
    Translation,
    Title,
    Subtitle,
}

/// One line of text as placed.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub role: LineRole,
    /// The line's text in logical order, without trailing spaces.
    pub text: String,
    pub font_size: f32,
    /// Left edge of the line's ink advance box.
    pub x: f32,
    pub baseline: f32,
    /// Total advance width.
    pub width: f32,
    /// Runs in visual order, left to right.
    pub runs: Vec<GlyphRun>,
}

impl Line {
    pub fn glyph_count(&self) -> usize {
        self.runs.iter().map(|r| r.glyphs.len()).sum()
    }
}

/// Glyphs with one font, size and colour.
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphRun {
    pub font: FontId,
    pub font_size: f32,
    pub color: Rgba,
    pub rtl: bool,
    pub effects: Effects,
    pub glyphs: Vec<Glyph>,
}

/// What is drawn around and behind a run's glyphs. Lengths are output pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Effects {
    /// Outline colour and thickness (the stroke grows this far outside the glyph).
    pub outline: Option<(Rgba, f32)>,
    /// Drop shadow colour. Its offset is derived from the font size at draw time.
    pub shadow: Option<Rgba>,
    /// A solid box behind the line.
    pub fill: Option<Rgba>,
    pub underline: bool,
    /// Slanted (synthetic oblique until italic faces are bundled).
    pub italic: bool,
}

/// A glyph's origin (on the baseline).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    pub id: u32,
    pub x: f32,
    pub y: f32,
}

impl Frame {
    /// A stable text dump for snapshot tests: boxes, lines and glyph positions to 0.1 px.
    pub fn snapshot(&self) -> String {
        let mut out = String::new();
        let r = |v: f32| format!("{:.1}", v);
        let _ = writeln!(out, "frame {}x{} background {:?}", self.size.width, self.size.height, self.background);
        if let Some(image) = &self.image {
            let a = image.area;
            let _ = writeln!(
                out,
                "image {}x{} {:?} in ({}, {}) size {}x{}",
                image.picture.width,
                image.picture.height,
                image.fit,
                r(a.x),
                r(a.y),
                r(a.width),
                r(a.height)
            );
        }
        for b in &self.boxes {
            let _ = writeln!(out, "box {:?} at ({}, {}) size {}x{}", b.role, r(b.rect.x), r(b.rect.y), r(b.rect.width), r(b.rect.height));
            for l in &b.lines {
                let _ = writeln!(
                    out,
                    "  line {:?} {:?} size {} x {} baseline {} width {}",
                    l.role,
                    l.text,
                    r(l.font_size),
                    r(l.x),
                    r(l.baseline),
                    r(l.width)
                );
                for run in &l.runs {
                    let glyphs: Vec<String> = run.glyphs.iter().map(|g| format!("{}@{}", g.id, r(g.x))).collect();
                    let fx = if run.effects == Effects::default() { String::new() } else { format!(" fx {:?}", run.effects) };
                    let _ = writeln!(out, "    run font {} rtl {} color {:?}{}: {}", run.font.0, run.rtl, run.color, fx, glyphs.join(" "));
                }
            }
        }
        out
    }
}

/// Lays out `slide` with `style` for a frame of `size`.
///
/// The title and subtitle boxes are placed first, stacked from the top or the bottom of the frame
/// as their vertical alignment says; the body gets what is left. With auto-scale on, the body's
/// text shrinks (never grows) until it fits.
pub(crate) fn layout(fonts: &Fonts, slide: &Slide, captions: &Captions, style: &Style, size: Size, picture: Option<Arc<Picture>>) -> Frame {
    let scale = size.height as f32 / REFERENCE_HEIGHT;
    let (w, h) = (size.width as f32, size.height as f32);
    let thickness = style.outline_thickness * scale;

    let caption_boxes: [(BoxRole, LineRole, &TextStyle, &str); 2] = [
        (BoxRole::Title, LineRole::Title, &style.title.text, &captions.title),
        (BoxRole::Subtitle, LineRole::Subtitle, &style.subtitle.text, &captions.subtitle),
    ];
    // Shape each shown caption; its block height decides where it goes.
    let mut shown: Vec<(BoxRole, &TextStyle, Vec<Line>, f32)> = Vec::new();
    for (role, line_role, text_style, text) in caption_boxes {
        if !text_style.enabled || text.trim().is_empty() {
            continue;
        }
        let m = text_style.margins;
        let width = (w - (m.left + m.right) * scale).max(0.0);
        let lines = shape(fonts, line_role, std::slice::from_ref(&text.to_owned()), text_style, text_style.size * scale, width, thickness);
        let block = lines.len() as f32 * text_style.size * scale * LINE_PITCH;
        shown.push((role, text_style, lines, block));
    }

    let (mut top, mut bottom) = (0.0_f32, h);
    let mut placed: Vec<TextBox> = Vec::new();
    // Top-aligned boxes stack downwards in order; bottom-aligned ones stack upwards, so the
    // last one (the subtitle) sits lowest.
    for (role, st, lines, block) in shown.iter_mut() {
        if st.valign == VAlign::Bottom {
            continue;
        }
        let m = st.margins;
        let y = top + m.top * scale;
        top = y + *block + m.bottom * scale;
        placed.push(finish(fonts, *role, st, std::mem::take(lines), rect_at(m.left * scale, y, w - (m.left + m.right) * scale, *block)));
    }
    for (role, st, lines, block) in shown.iter_mut().rev() {
        if st.valign != VAlign::Bottom {
            continue;
        }
        let m = st.margins;
        let y = bottom - m.bottom * scale - *block;
        bottom = y - m.top * scale;
        placed.push(finish(fonts, *role, st, std::mem::take(lines), rect_at(m.left * scale, y, w - (m.left + m.right) * scale, *block)));
    }
    placed.sort_by_key(|b| b.role != BoxRole::Title);

    // The body: what remains between the captions, inside its own margins.
    let m = style.body.text.margins;
    let top = top.max(m.top * scale);
    let bottom = bottom.min(h - m.bottom * scale);
    let rect = rect_at(m.left * scale, top, w - (m.left + m.right) * scale, (bottom - top).max(0.0));
    // `background_as_text`: the image takes the place of the lyrics (only once it is there).
    let no_lyrics = Slide::default();
    let lyrics = if style.background.as_text && picture.is_some() { &no_lyrics } else { slide };
    placed.push(body_box(fonts, lyrics, style, rect, scale, thickness));
    let image = picture.map(|picture| {
        let frame_area = rect_at(0.0, 0.0, w, h);
        let (fit, area) = match style.background.resize {
            Some(Resize::Screen) => (if style.background.keep_aspect { Fit::Contain } else { Fit::Stretch }, frame_area),
            Some(Resize::Body) => (if style.background.keep_aspect { Fit::Contain } else { Fit::Stretch }, rect),
            Some(Resize::None) => (Fit::Natural, frame_area),
            None => (
                match style.background.position {
                    ImagePosition::Stretch => Fit::Stretch,
                    ImagePosition::Center => Fit::Natural,
                    ImagePosition::Tile => Fit::Tile,
                    ImagePosition::Cover => Fit::Cover,
                },
                frame_area,
            ),
        };
        FrameImage { picture, fit, area, scale }
    });
    Frame { size, background: style.background.color, image, boxes: placed }
}

fn rect_at(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect { x, y, width: width.max(0.0), height: height.max(0.0) }
}

/// The smallest factor auto-scale will shrink text to.
const MIN_AUTO_SCALE: f32 = 0.1;

/// Lay the body's lines out in `rect`, shrinking them to fit when the style auto-scales.
///
/// In a multilanguage slide every other line is a translation line: the body style with italic
/// flipped, `floor(body size x multilanguage size / 100)` and the multilanguage colour; before
/// each primary line after the first goes a gap of `pair_gap` translation lines. Auto-scale
/// fits the whole block, so translation lines shrink with the primary lines.
fn body_box(fonts: &Fonts, slide: &Slide, style: &Style, rect: Rect, scale: f32, thickness: f32) -> TextBox {
    let text = &style.body.text;
    let mut translation = text.clone();
    translation.italic ^= style.translation_italic;
    translation.color = style.body.multilanguage_color;
    let at = |factor: f32| {
        let px = text.size * scale * factor;
        if !slide.multilanguage {
            let lines = shape(fonts, LineRole::Lyric, &slide.lines, text, px, rect.width, thickness);
            let gaps = vec![0.0; lines.len()];
            return (lines, gaps);
        }
        let tpx = (px * style.body.multilanguage_size as f32 / 100.0).floor().max(1.0);
        let gap = style.pair_gap * tpx * LINE_PITCH;
        let (mut lines, mut gaps) = (Vec::new(), Vec::new());
        for (i, line) in slide.lines.iter().enumerate() {
            let shaped = if i % 2 == 0 {
                shape(fonts, LineRole::Lyric, std::slice::from_ref(line), text, px, rect.width, thickness)
            } else {
                shape(fonts, LineRole::Translation, std::slice::from_ref(line), &translation, tpx, rect.width, thickness)
            };
            let first_gap = if i % 2 == 0 && i > 0 { gap } else { 0.0 };
            gaps.extend((0..shaped.len()).map(|j| if j == 0 { first_gap } else { 0.0 }));
            lines.extend(shaped);
        }
        (lines, gaps)
    };
    let fits = |(shaped, gaps): &(Vec<Line>, Vec<f32>)| {
        block_height(shaped, gaps) <= rect.height + 0.01 && shaped.iter().all(|l| l.width <= rect.width + 0.5)
    };
    let mut shaped = at(1.0);
    if style.body.auto_scale && !fits(&shaped) {
        // Binary search for the largest factor that fits (DeckCraft's 12 steps).
        let (mut lo, mut hi) = (MIN_AUTO_SCALE, 1.0_f32);
        let mut best = None;
        for _ in 0..12 {
            let mid = (lo + hi) / 2.0;
            let candidate = at(mid);
            if fits(&candidate) {
                best = Some(candidate);
                lo = mid;
            } else {
                hi = mid;
            }
        }
        shaped = best.unwrap_or_else(|| at(MIN_AUTO_SCALE));
    }
    let (mut lines, gaps) = shaped;
    place_with_gaps(fonts, &mut lines, &gaps, text, rect);
    TextBox { role: BoxRole::Body, rect, lines }
}

/// Height of a block of lines, each `LINE_PITCH` of its own size, with gaps before some.
fn block_height(lines: &[Line], gaps: &[f32]) -> f32 {
    lines.iter().zip(gaps).map(|(l, gap)| gap + l.font_size * LINE_PITCH).sum()
}

/// Place a caption's lines in a rect that is exactly as tall as they are.
fn finish(fonts: &Fonts, role: BoxRole, style: &TextStyle, mut lines: Vec<Line>, rect: Rect) -> TextBox {
    place(fonts, &mut lines, style, rect);
    TextBox { role, rect, lines }
}

/// Breaks and shapes lines at `px`, relative to each line's own origin. `|` forces a break.
fn shape(fonts: &Fonts, role: LineRole, lines: &[String], style: &TextStyle, px: f32, measure: f32, thickness: f32) -> Vec<Line> {
    let font = fonts.resolve(&style.font);
    let effects = Effects {
        outline: style.outline.then_some((style.outline_color, thickness)),
        shadow: style.shadow.then_some(style.shadow_color),
        fill: style.fill.then_some(style.fill_color),
        underline: style.underline,
        italic: style.italic,
    };
    let mut placed = Vec::new();
    for paragraph in lines.iter().flat_map(|l| l.split('|')) {
        for range in break_lines(fonts, font, paragraph, px, measure) {
            placed.push(place_line(fonts, font, paragraph, range, px, style.color, role, effects));
        }
    }
    placed
}

/// Put shaped lines in `rect`: the block by the vertical alignment, each line by the horizontal.
fn place(fonts: &Fonts, placed: &mut [Line], style: &TextStyle, rect: Rect) {
    let gaps = vec![0.0; placed.len()];
    place_with_gaps(fonts, placed, &gaps, style, rect);
}

/// [`place`] with lines of their own sizes (each `LINE_PITCH` of its size) and a gap before each.
fn place_with_gaps(fonts: &Fonts, placed: &mut [Line], gaps: &[f32], style: &TextStyle, rect: Rect) {
    let font = fonts.resolve(&style.font);
    let block = block_height(placed, gaps);
    let mut top = match style.valign {
        VAlign::Top => rect.y,
        VAlign::Middle => rect.y + (rect.height - block) / 2.0,
        VAlign::Bottom => rect.y + rect.height - block,
    };
    for (line, gap) in placed.iter_mut().zip(gaps) {
        let pitch = line.font_size * LINE_PITCH;
        let (ascent, descent) = fonts.vertical_metrics(font, line.font_size);
        let lead = (pitch - (ascent - descent)) / 2.0;
        top += gap;
        let baseline = top + lead + ascent;
        top += pitch;
        let x = match style.align {
            Align::Left => rect.x,
            Align::Center => rect.x + (rect.width - line.width) / 2.0,
            Align::Right => rect.x + rect.width - line.width,
        };
        line.baseline = baseline;
        line.x = x;
        for run in &mut line.runs {
            for g in &mut run.glyphs {
                g.x += x;
                g.y += baseline;
            }
        }
    }
}

/// Breaks one paragraph into lines no wider than `measure` (except single words that are
/// wider), using craft-text's break opportunities and ragged line breaker. Returns byte ranges
/// without trailing spaces. An empty paragraph is one empty line.
fn break_lines(fonts: &Fonts, font: FontId, text: &str, px: f32, measure: f32) -> Vec<Range<usize>> {
    if text.trim().is_empty() {
        return std::iter::once(0..0).collect();
    }
    let mut pieces = Vec::new();
    let mut start = 0;
    for b in line_breaks(text, &Options::default()) {
        if b.offset > start {
            pieces.push(start..b.offset);
            start = b.offset;
        }
    }
    if start < text.len() {
        pieces.push(start..text.len());
    }

    // Items and the byte range each covers.
    let mut items = Vec::new();
    let mut spans: Vec<Range<usize>> = Vec::new();
    let last = pieces.len() - 1;
    for (i, piece) in pieces.into_iter().enumerate() {
        let content_end = piece.start + text[piece.clone()].trim_end().len();
        if content_end > piece.start {
            items.push(Item::boxed(fonts.measure(font, &text[piece.start..content_end], px).into()));
            spans.push(piece.start..content_end);
        }
        if content_end < piece.end {
            items.push(Item::glue(fonts.measure(font, &text[content_end..piece.end], px).into(), 0.0, 0.0));
            spans.push(content_end..piece.end);
        } else if i != last {
            items.push(Item::penalty(0.0, 0.0, false));
            spans.push(piece.end..piece.end);
        }
    }
    items.push(Item::fill());
    items.push(Item::forced());
    spans.extend([text.len()..text.len(), text.len()..text.len()]);

    let ragged = Params { rag: Some(Rag::default()), ..Params::default() };
    let measure = f64::from(measure);
    let breaks = break_paragraph(&items, &|_| measure, &ragged, &[1.0, f64::INFINITY]);
    breaks
        .lines
        .iter()
        .map(|line| {
            let start = spans[line.start].start;
            let end = spans.get(line.end).map_or(text.len(), |s| s.start).max(start);
            start..start + text[start..end].trim_end().len()
        })
        .filter(|r| !r.is_empty())
        .collect()
}

/// Shapes one line in visual order, relative to its own origin (x 0 at the left of the line,
/// y 0 on the baseline).
#[allow(clippy::too_many_arguments)]
fn place_line(fonts: &Fonts, font: FontId, paragraph: &str, range: Range<usize>, px: f32, color: Rgba, role: LineRole, effects: Effects) -> Line {
    let bidi = Paragraph::new(paragraph, Direction::Auto);
    let mut pen = 0.0;
    let mut runs = Vec::new();
    for run in bidi.visual_runs(range.clone()) {
        let rtl = run.is_rtl();
        // Characters the font lacks are shaped with a fallback face, so one bidi run can become
        // several glyph runs, one per face.
        for shaped in fonts.shape(font, &paragraph[run.range], rtl, px) {
            let glyph = Glyph { id: shaped.id, x: pen + shaped.x_offset, y: -shaped.y_offset };
            pen += shaped.advance;
            match runs.last_mut() {
                Some(GlyphRun { font: f, rtl: r, glyphs, .. }) if *f == shaped.font && *r == rtl => glyphs.push(glyph),
                _ => runs.push(GlyphRun { font: shaped.font, font_size: px, color, rtl, effects, glyphs: vec![glyph] }),
            }
        }
    }
    Line { role, text: paragraph[range].to_owned(), font_size: px, x: 0.0, baseline: 0.0, width: pen, runs }
}
