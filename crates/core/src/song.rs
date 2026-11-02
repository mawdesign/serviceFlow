//! Songs: metadata, sections, and the slides they turn into.

/// A song as read from the library (GLOSSARY "Song").
///
/// Text fields hold the element text exactly as read; an absent element reads as `""`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
    pub key_line: String,
    pub tempo: String,
    pub time_sig: String,
    pub user1: String,
    pub user2: String,
    pub user3: String,
    /// The presentation order as section tags. Empty means file order.
    pub presentation: Vec<String>,
    /// Sections in file order (first appearance; a repeated tag adds to the earlier section).
    pub sections: Vec<Section>,
    /// Elements of the file that are not modelled above (such as `<alttheme>`, `<backgrounds>`
    /// or `<linked_songs>`), in file order, so a save can keep them.
    pub other_elements: Vec<RawElement>,
    /// Problems in the lyrics worth showing in the editor. The song still reads.
    pub warnings: Vec<SongWarning>,
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
}

/// A named part of a song's lyrics (GLOSSARY "Section").
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Section {
    /// The section tag as written, such as `V1`, `C`, `1` or `Coda`. `""` for lyrics before the
    /// first tag. A numbered verse block's verses get the block's tag plus the digit (`V1`).
    pub tag: String,
    /// The section's lines in file order.
    pub lines: Vec<Line>,
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

/// Settings that change how lyrics become slides.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SlideOptions {
    /// An empty lyric line is a slide break (OpenSong `blank_is_slide_change`). Off: empty lines
    /// are ignored.
    pub blank_is_slide_change: bool,
}

/// One slide of a song, in presentation order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slide {
    /// The tag of the section the slide comes from.
    pub section: String,
    /// The lines to show. `|` inside a line is a forced line break, left for the renderer.
    pub lines: Vec<String>,
}

impl Section {
    pub fn kind(&self) -> SectionKind {
        SectionKind::of_tag(&self.tag)
    }

    /// The section's slides as the lines to show (OpenSong `ToSetML`/`DeflateString`): chord
    /// lines dropped, split at `||` (and at empty lines when `blank_is_slide_change`), each
    /// line trimmed with `_` removed. Empty slides are left out.
    pub fn slides(&self, options: &SlideOptions) -> Vec<Vec<String>> {
        let mut slides: Vec<Vec<String>> = vec![Vec::new()];
        for line in &self.lines {
            match line {
                Line::Lyric(text) => {
                    for (i, piece) in text.split("||").enumerate() {
                        if i > 0 {
                            slides.push(Vec::new());
                        }
                        let shown = deflate(piece);
                        if !shown.is_empty()
                            && let Some(slide) = slides.last_mut()
                        {
                            slide.push(shown);
                        }
                    }
                }
                Line::Blank if options.blank_is_slide_change => slides.push(Vec::new()),
                Line::Blank | Line::Chords(_) => {}
            }
        }
        slides.retain(|slide| !slide.is_empty());
        slides
    }

    /// Whether a presentation-order token names this section exactly (case-insensitively; a
    /// no-break space in a token stands for a space in the tag, as OpenSong writes them).
    fn is_named(&self, token: &str) -> bool {
        let token = token.replace('\u{a0}', " ");
        self.tag.eq_ignore_ascii_case(&token)
    }
}

/// What a lyric line looks like on screen: trimmed, `_` removed, OpenSong's special spacing
/// characters (U+02CD, U+203F) turned into spaces.
fn deflate(text: &str) -> String {
    text.trim().chars().filter(|&c| c != '_').map(|c| if matches!(c, '\u{2cd}' | '\u{203f}') { ' ' } else { c }).collect()
}

impl Song {
    /// The section a presentation-order token refers to. An exact tag wins; otherwise a bare
    /// number `n` finds verse `Vn`, and `Vn` finds a bare-number section `n`.
    pub fn section(&self, token: &str) -> Option<&Section> {
        let exact = self.sections.iter().find(|s| s.is_named(token));
        exact.or_else(|| {
            let alias = if token.chars().all(|c| c.is_ascii_digit()) {
                format!("V{token}")
            } else {
                let digits = token.strip_prefix(['V', 'v'])?;
                if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
                    return None;
                }
                digits.to_owned()
            };
            self.sections.iter().find(|s| s.is_named(&alias))
        })
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
            .flat_map(|section| section.slides(options).into_iter().map(|lines| Slide { section: section.tag.clone(), lines }))
            .collect()
    }
}
