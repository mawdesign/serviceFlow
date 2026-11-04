//! Songs: metadata, sections, and the slides they turn into.

use std::sync::Arc;

use crate::style::{ImageSource, Resize, Style};

/// A song as read from the library (GLOSSARY "Song").
///
/// Text fields hold the element text exactly as read; an absent element reads as `""`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Song {
    pub title: String,
    pub author: String,
    pub copyright: String,
    pub ccli: String,
    pub hymn_number: String,
    /// `; `-separated song themes, as written (legacy values keep their trailing `;`).
    pub theme: String,
    pub aka: String,
    pub key: String,
    pub capo: String,
    /// The attributes of `<capo>` as written (`print`, `sharp`), kept for saving.
    pub capo_attributes: Vec<(String, String)>,
    pub key_line: String,
    pub tempo: String,
    pub time_sig: String,
    pub user1: String,
    pub user2: String,
    pub user3: String,
    /// The presentation order as section tags. Empty means file order.
    pub presentation: Vec<String>,
    /// The lyrics as written (line endings as LF), which the sections are read from. The song
    /// editor edits this text; comments and everything else the sections drop stay in it.
    pub lyrics: String,
    /// Sections in file order (first appearance; a repeated tag adds to the earlier section).
    pub sections: Vec<Section>,
    /// The song's own `<style>`, parsed from `other_elements` (which keeps the XML for saving).
    pub style: Option<Arc<Style>>,
    /// The song's `<backgrounds>`, parsed from `other_elements` (which keeps the XML for saving).
    pub backgrounds: Option<SongBackgrounds>,
    /// Elements of the file that are not modelled above (such as `<alttheme>`, `<backgrounds>`,
    /// `<style>` or `<linked_songs>`), in file order, so a save can keep them. `style` and
    /// `backgrounds` are parsed views of two of these, for display only.
    pub other_elements: Vec<RawElement>,
    /// Problems in the lyrics worth showing in the editor. The song still reads.
    pub warnings: Vec<SongWarning>,
}

/// A song's background images (OpenSong `<backgrounds>`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SongBackgrounds {
    pub resize: Resize,
    pub keep_aspect: bool,
    pub link: bool,
    /// Show the image without the lyrics (`background_as_text`).
    pub as_text: bool,
    /// In file order.
    pub items: Vec<VerseBackground>,
}

/// One `<background verse="v1">` of a song.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerseBackground {
    /// The section tag it is for, as written (matched case-insensitively).
    pub verse: String,
    pub image: ImageSource,
}

impl SongBackgrounds {
    /// The image for slide `part` (0-based) of a section with tag `section`: the nth background
    /// for that section, and the last one for any slide after that (OpenSong `ToSetML`).
    pub fn image_for(&self, section: &str, part: usize) -> Option<&ImageSource> {
        let mut matching = self.items.iter().filter(|b| b.verse.trim().eq_ignore_ascii_case(section));
        let count = matching.clone().count();
        if count == 0 { None } else { matching.nth(part.min(count - 1)).map(|b| &b.image) }
    }
}

/// An element kept as it was read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawElement {
    pub name: String,
    /// The element as XML text, from its start tag to its end tag.
    pub xml: String,
}

/// Something odd in a song's lyrics (spec "editor warnings").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SongWarning {
    /// A lyric line starts with a character that is neither a space nor a digit 1–9 nor a
    /// line-type marker. OpenSong would make it a sub-section; serviceFlow reads the whole line
    /// as lyrics (ADR-0007). `line` is the line number within the lyrics, from 1.
    StrayFirstCharacter { line: usize, character: char },
    /// A section tag ends in `L` as OpenSong's loose legacy rule would read as multilanguage,
    /// but is not one of the strict forms serviceFlow recognises (ADR-0007). Add ` (L)` after
    /// the tag if it is meant to be multilanguage. `line` is the tag's line.
    MaybeMultilanguage { line: usize, tag: String },
    /// A `||` slide break on `line` of a multilanguage section falls between a primary line and
    /// its translation. The break is shown after the translation line instead (ADR-0007).
    BreakInsidePair { line: usize },
    /// The tag `tag` on `line` names the same section as the earlier tag `other` once both are
    /// written in canonical form (`1` and `V1`; `V1-L` and `V1`), so renaming them to the
    /// canonical form would merge two sections (ADR-0002).
    TagCollision { line: usize, tag: String, other: String },
}

impl SongWarning {
    /// The lyrics line the warning is about, from 1.
    pub fn line(&self) -> usize {
        match self {
            Self::StrayFirstCharacter { line, .. }
            | Self::MaybeMultilanguage { line, .. }
            | Self::BreakInsidePair { line }
            | Self::TagCollision { line, .. } => *line,
        }
    }
}

/// A named part of a song's lyrics (GLOSSARY "Section").
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Section {
    /// The section tag as written, such as `V1`, `C`, `1` or `Coda`. `""` for lyrics before the
    /// first tag. A numbered verse block's verses get the block's tag plus the digit (`V1`).
    pub tag: String,
    /// The section's lines in file order.
    pub lines: Vec<Line>,
    /// A multilanguage section (GLOSSARY): its lyric lines alternate between a primary line and
    /// its translation line, pairing afresh on every slide.
    pub multilanguage: bool,
}

/// One line of a section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// A lyric line without its first-column marker (the leading space or verse digit), so it
    /// lines up column by column with a chord line. It may hold `||` slide breaks, `|` line
    /// breaks and `_` spacers.
    Lyric(String),
    /// A chord line without its leading `.`. A numbered verse block's chord lines are shared:
    /// each verse gets a copy before its lyric line.
    Chords(String),
    /// An empty line. A slide break when `blank_is_slide_change` is on.
    Blank,
}

/// The kind of section, from the first letter of its tag (GLOSSARY "Section type").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum SectionKind {
    #[default]
    Verse,
    PreChorus,
    Chorus,
    Bridge,
    Tag,
}

impl SectionKind {
    /// `V`, `P`, `C`, `B` or `T` (either case); anything else, including digits, is a verse.
    pub fn of_tag(tag: &str) -> Self {
        match tag.chars().next().map(|c| c.to_ascii_uppercase()) {
            Some('P') => Self::PreChorus,
            Some('C') => Self::Chorus,
            Some('B') => Self::Bridge,
            Some('T') => Self::Tag,
            _ => Self::Verse,
        }
    }
}

/// Which lines of multilanguage sections are shown (GLOSSARY "Language mode").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum LanguageMode {
    /// Each primary line with its translation line beneath it.
    #[default]
    Both,
    /// Primary lines only, shown like any other lyrics.
    Primary,
    /// Translation lines only, shown like any other lyrics.
    Translation,
}

/// Settings that change how lyrics become slides.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SlideOptions {
    /// An empty lyric line is a slide break (OpenSong `blank_is_slide_change`). Off: empty lines
    /// are ignored.
    pub blank_is_slide_change: bool,
    /// What multilanguage sections show.
    pub language: LanguageMode,
}

/// One slide of a song, in presentation order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Slide {
    /// The tag of the section the slide comes from.
    pub section: String,
    /// The lines to show. `|` inside a line is a forced line break, left for the renderer.
    pub lines: Vec<String>,
    /// The lines alternate primary line, translation line (even indexes are primary lines; a
    /// last primary line may have no translation). Only set when both languages are shown.
    pub multilanguage: bool,
    /// Which slide of its section this is (0-based, counted afresh each time the presentation
    /// order shows the section). Song backgrounds are chosen by it.
    pub part: usize,
}

impl Section {
    pub fn kind(&self) -> SectionKind {
        SectionKind::of_tag(&self.tag)
    }

    /// The section's slides as the lines to show (OpenSong `ToSetML`/`DeflateString`): chord
    /// lines dropped, split at `||` (and at empty lines when `blank_is_slide_change`), each
    /// line trimmed with `_` removed. Empty slides are left out.
    ///
    /// In a multilanguage section the lines pair up afresh on every slide, and a break that
    /// would part a primary line from its translation waits until after the translation
    /// (ADR-0007). With both languages shown, a `|` in both lines of a pair splits them into
    /// several pairs; otherwise only one language's lines are kept (an empty slide stays, so
    /// slide numbers do not change with the language mode).
    pub fn slides(&self, options: &SlideOptions) -> Vec<Vec<String>> {
        let slides = self.split(options).0;
        if !self.multilanguage {
            return slides;
        }
        slides
            .into_iter()
            .map(|slide| {
                let pairs = slide.chunks(2);
                match options.language {
                    LanguageMode::Both => pairs.flat_map(|pair| split_pair(&pair[0], pair.get(1))).collect(),
                    LanguageMode::Primary => pairs.map(|pair| pair[0].clone()).collect(),
                    LanguageMode::Translation => pairs.filter_map(|pair| pair.get(1).cloned()).collect(),
                }
            })
            .collect()
    }

    /// Indexes into [`lines`](Self::lines) of lyric lines whose `||` falls between a primary
    /// line and its translation (always empty for other sections).
    pub fn breaks_inside_pairs(&self) -> Vec<usize> {
        self.split(&SlideOptions::default()).1
    }

    /// The shown lines per slide (empty slides dropped), and the lines whose break was moved.
    fn split(&self, options: &SlideOptions) -> (Vec<Vec<String>>, Vec<usize>) {
        let mut slides: Vec<Vec<String>> = vec![Vec::new()];
        let mut moved = Vec::new();
        // A break is waiting for the current pair's translation line.
        let mut waiting = false;
        // Start a new slide, unless that would part a pair; then wait. True when it waits.
        let slide_break = |slides: &mut Vec<Vec<String>>, waiting: &mut bool| {
            let open_pair = self.multilanguage && slides.last().is_some_and(|s| s.len() % 2 == 1);
            if open_pair {
                *waiting = true;
            } else {
                slides.push(Vec::new());
            }
            open_pair
        };
        for (index, line) in self.lines.iter().enumerate() {
            match line {
                Line::Lyric(text) => {
                    for (i, piece) in text.split("||").enumerate() {
                        if i > 0 && slide_break(&mut slides, &mut waiting) && !moved.contains(&index) {
                            moved.push(index);
                        }
                        let shown = deflate(piece);
                        if shown.is_empty() {
                            continue;
                        }
                        let Some(slide) = slides.last_mut() else { continue };
                        slide.push(shown);
                        if waiting && slide.len() % 2 == 0 {
                            waiting = false;
                            slides.push(Vec::new());
                        }
                    }
                }
                Line::Blank if options.blank_is_slide_change => {
                    slide_break(&mut slides, &mut waiting);
                }
                Line::Blank | Line::Chords(_) => {}
            }
        }
        slides.retain(|slide| !slide.is_empty());
        (slides, moved)
    }

    /// Whether a presentation-order token names this section exactly (case-insensitively; a
    /// no-break space in a token stands for a space in the tag, as OpenSong writes them).
    fn is_named(&self, token: &str) -> bool {
        let token = token.replace('\u{a0}', " ");
        self.tag.eq_ignore_ascii_case(&token)
    }
}

/// A primary line and its translation as shown one beneath the other. When both hold `|`, the
/// pieces pair up in order and the last pair keeps whatever is left of the longer one.
fn split_pair(primary: &str, translation: Option<&String>) -> Vec<String> {
    let Some(translation) = translation else { return vec![primary.to_owned()] };
    let n = primary.matches('|').count().min(translation.matches('|').count()) + 1;
    let p = primary.splitn(n, '|').map(str::trim);
    let t = translation.splitn(n, '|').map(str::trim);
    p.zip(t).flat_map(|(p, t)| [p.to_owned(), t.to_owned()]).collect()
}

/// The section a strict legacy multilanguage tag (`V1-L`, `C-L`, `V1L`) stands for (`V1`, `C`,
/// `V1`), or `None` for any other tag (ADR-0007). The type letter must be `V`, `P`, `C`, `B` or
/// `T` (either case).
pub fn legacy_multilanguage_base(tag: &str) -> Option<&str> {
    let (base, digits_needed) = match tag.strip_suffix("-L").or_else(|| tag.strip_suffix("-l")) {
        Some(base) => (base, false),
        None => (tag.strip_suffix(['L', 'l'])?, true),
    };
    let mut chars = base.chars();
    let letter = chars.next()?;
    let digits = chars.as_str();
    let typed = matches!(letter.to_ascii_uppercase(), 'V' | 'P' | 'C' | 'B' | 'T');
    let numbered = digits.chars().all(|c| c.is_ascii_digit()) && !(digits_needed && digits.is_empty());
    (typed && numbered).then_some(base)
}

/// The canonical form of a section tag (ADR-0002) for comparing tags: a bare number `n` is `Vn`,
/// a legacy multilanguage tag loses its `-L`/`L`. Compare the results case-insensitively.
pub fn canonical_tag(tag: &str) -> String {
    if !tag.is_empty() && tag.chars().all(|c| c.is_ascii_digit()) {
        return format!("V{tag}");
    }
    legacy_multilanguage_base(tag).unwrap_or(tag).to_owned()
}

/// What a lyric line looks like on screen: trimmed, `_` removed, OpenSong's special spacing
/// characters (U+02CD, U+203F) turned into spaces.
fn deflate(text: &str) -> String {
    text.trim().chars().filter(|&c| c != '_').map(|c| if matches!(c, '\u{2cd}' | '\u{203f}') { ' ' } else { c }).collect()
}

impl Song {
    /// The section a presentation-order token refers to. An exact tag wins; otherwise a bare
    /// number `n` finds verse `Vn`, and `Vn` finds a bare-number section `n`; a legacy
    /// multilanguage token (`V1-L`, `V1L`) finds the multilanguage section `V1`, and `V1` finds a
    /// legacy-tagged one.
    pub fn section(&self, token: &str) -> Option<&Section> {
        let exact = self.sections.iter().find(|s| s.is_named(token));
        exact.or_else(|| self.legacy_alias(token)).or_else(|| {
            let alias = if token.chars().all(|c| c.is_ascii_digit()) {
                format!("V{token}")
            } else {
                let digits = token.strip_prefix(['V', 'v'])?;
                if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
                    return None;
                }
                digits.to_owned()
            };
            self.sections.iter().find(|s| s.is_named(&alias)).or_else(|| self.legacy_alias(&alias))
        })
    }

    fn legacy_alias(&self, token: &str) -> Option<&Section> {
        let base = legacy_multilanguage_base(token).unwrap_or(token);
        self.sections.iter().filter(|s| s.multilanguage).find(|s| legacy_multilanguage_base(&s.tag).unwrap_or(&s.tag).eq_ignore_ascii_case(base))
    }

    /// The sections in presentation order (GLOSSARY "Presentation order"). An empty order means
    /// file order. A token with no section is skipped; a repeated token repeats its section.
    pub fn flow(&self) -> Vec<&Section> {
        if self.presentation.is_empty() {
            self.sections.iter().collect()
        } else {
            self.presentation.iter().filter_map(|token| self.section(token)).collect()
        }
    }

    /// The song's slides in presentation order.
    pub fn slides(&self, options: &SlideOptions) -> Vec<Slide> {
        Self::slides_of(self.flow(), options)
    }

    /// The slides of sections the presentation order leaves out, in file order. They are not
    /// part of the flow but can still be sent live by hand (ADR-0007).
    pub fn unused_slides(&self, options: &SlideOptions) -> Vec<Slide> {
        let flow = self.flow();
        let unused = self.sections.iter().filter(|s| !flow.iter().any(|f| std::ptr::eq(*f, *s)));
        Self::slides_of(unused, options)
    }

    fn slides_of<'a>(sections: impl IntoIterator<Item = &'a Section>, options: &SlideOptions) -> Vec<Slide> {
        sections
            .into_iter()
            .flat_map(|section| {
                section.slides(options).into_iter().enumerate().map(|(part, lines)| Slide {
                    section: section.tag.clone(),
                    lines,
                    multilanguage: section.multilanguage && options.language == LanguageMode::Both,
                    part,
                })
            })
            .collect()
    }
}
