//! The lyrics line grammar (opensong.md §3, §4): text in, sections and warnings out. It lives in
//! core, not in the OpenSong codec, because the song editor re-reads the lyrics on every edit.

use crate::song::{Line, Section, SongWarning, canonical_tag, legacy_multilanguage_base};

/// Split lyrics into sections (OpenSong `SongML.LyricsToSections`). Each line is right-trimmed;
/// its first character decides what it is:
///
/// - `[` a section tag (`[name]`), maybe followed by options in parentheses: `(L)` makes the
///   section multilanguage, as do the strict legacy tags `V1-L`, `C-L`, `V1L` (ADR-0007; other
///   tags ending in `L` get a warning). Once multilanguage, a section stays so, and a numbered
///   verse block's verses inherit it;
/// - `.` a chord line; `;` a comment and `-` a print-layout command (neither kept);
/// - a space: a lyric line of the current section;
/// - `1`–`9`: a lyric line of the numbered verse `<current tag><digit>` (numbered verse block);
/// - anything else: a lyric line of the current section, with a warning (ADR-0007).
///
/// A tag seen before (case-insensitively) continues that section. A chord line goes with the
/// next lyric line; in a numbered verse block it goes to every verse whose lines follow it.
pub fn parse(lyrics: &str) -> (Vec<Section>, Vec<SongWarning>) {
    let mut parser = Parser::default();
    for (number, line) in lyrics.lines().map(str::trim_end).enumerate() {
        parser.line = number;
        let mut chars = line.chars();
        let Some(first) = chars.next() else {
            parser.flush_chords();
            // Empty lines before the first tag or lyric belong to nothing.
            if let Some(current) = parser.current {
                parser.push(current, Line::Blank, number);
            }
            continue;
        };
        let rest = chars.as_str();
        match first {
            '[' => {
                parser.flush_chords();
                let (tag, after) = rest.split_once(']').unwrap_or((rest, ""));
                let tag = tag.trim();
                let explicit = has_l_option(after);
                let legacy = legacy_multilanguage_base(tag).is_some();
                if !explicit && !legacy && tag.chars().count() > 1 && tag.ends_with(['L', 'l']) {
                    parser.warnings.push(SongWarning::MaybeMultilanguage { line: number + 1, tag: tag.to_owned() });
                }
                let section = parser.section(tag);
                parser.sections[section].multilanguage |= explicit || legacy;
                parser.current = Some(section);
            }
            '.' => {
                parser.flush_chords();
                parser.pending = Some(Pending { chords: rest.to_owned(), given_to: Vec::new() });
            }
            ';' | '-' => {}
            '1'..='9' => {
                let tag = format!("{}{first}", parser.current_tag());
                let inherited = parser.current.is_some_and(|i| parser.sections[i].multilanguage);
                let section = parser.section(&tag);
                parser.sections[section].multilanguage |= inherited;
                parser.lyric(section, rest, true, number);
            }
            c if c.is_whitespace() => {
                let section = parser.current();
                parser.lyric(section, rest, false, number);
            }
            c => {
                parser.warnings.push(SongWarning::StrayFirstCharacter { line: number + 1, character: c });
                let section = parser.current();
                parser.lyric(section, line, false, number);
            }
        }
    }
    parser.flush_chords();
    // Slide breaks that would part a pair (known only once every line of a section is in).
    for (section, origins) in parser.sections.iter().zip(&parser.origins) {
        for index in section.breaks_inside_pairs() {
            parser.warnings.push(SongWarning::BreakInsidePair { line: origins[index] + 1 });
        }
    }
    // Tags that would become one section when written in canonical form.
    for (i, section) in parser.sections.iter().enumerate() {
        let canonical = canonical_tag(&section.tag);
        if let Some(other) = parser.sections[..i].iter().find(|s| canonical_tag(&s.tag).eq_ignore_ascii_case(&canonical)) {
            parser.warnings.push(SongWarning::TagCollision { line: parser.first_lines[i] + 1, tag: section.tag.clone(), other: other.tag.clone() });
        }
    }
    parser.warnings.sort_by_key(SongWarning::line);
    (parser.sections, parser.warnings)
}

/// Whether the options after a tag's `]` (the first `(`…`)`) include `L`. Others such as
/// `L1=en` are ignored, as in OpenSong.
fn has_l_option(after: &str) -> bool {
    let Some((_, options)) = after.split_once('(') else { return false };
    let Some((options, _)) = options.split_once(')') else { return false };
    options.split(|c: char| c.is_whitespace() || c == ',').any(|o| o.eq_ignore_ascii_case("L"))
}

#[derive(Default)]
struct Parser {
    sections: Vec<Section>,
    /// For each section, the 0-based lyrics line each of its lines came from.
    origins: Vec<Vec<usize>>,
    /// For each section, the 0-based lyrics line that made it.
    first_lines: Vec<usize>,
    /// The line being read (for sections made on the way).
    line: usize,
    /// Index of the current section (the last tag seen), if any.
    current: Option<usize>,
    pending: Option<Pending>,
    warnings: Vec<SongWarning>,
}

/// A chord line waiting for the lyric line(s) it belongs to.
struct Pending {
    chords: String,
    /// Sections that already got a copy.
    given_to: Vec<usize>,
}

impl Parser {
    fn current_tag(&self) -> &str {
        self.current.map_or("", |i| self.sections[i].tag.as_str())
    }

    /// The current section, making the untagged section if lyrics come before any tag.
    fn current(&mut self) -> usize {
        match self.current {
            Some(i) => i,
            None => {
                let i = self.section("");
                self.current = Some(i);
                i
            }
        }
    }

    /// The section with this tag (case-insensitive), added at the end if it is new.
    fn section(&mut self, tag: &str) -> usize {
        match self.sections.iter().position(|s| s.tag.eq_ignore_ascii_case(tag)) {
            Some(i) => i,
            None => {
                self.sections.push(Section { tag: tag.to_owned(), ..Section::default() });
                self.origins.push(Vec::new());
                self.first_lines.push(self.line);
                self.sections.len() - 1
            }
        }
    }

    /// Add a lyric line to a section, after the pending chord line if it has not had it yet. A
    /// plain lyric line uses up the chord line; numbered lines share it.
    fn lyric(&mut self, section: usize, text: &str, numbered: bool, number: usize) {
        if let Some(pending) = &mut self.pending
            && !pending.given_to.contains(&section)
        {
            pending.given_to.push(section);
            let chords = Line::Chords(pending.chords.clone());
            self.push(section, chords, number);
        }
        if !numbered {
            self.pending = None;
        }
        self.push(section, Line::Lyric(text.to_owned()), number);
    }

    fn push(&mut self, section: usize, line: Line, number: usize) {
        self.sections[section].lines.push(line);
        self.origins[section].push(number);
    }

    /// A chord line that no lyric line took stays in the current section.
    fn flush_chords(&mut self) {
        if let Some(pending) = self.pending.take()
            && pending.given_to.is_empty()
        {
            let current = self.current();
            // Only lyric lines' origins are ever looked up.
            self.push(current, Line::Chords(pending.chords), 0);
        }
    }
}

/// What a stretch of lyrics text is, for syntax highlighting in the song editor. Plain lyrics
/// have no kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Highlight {
    /// A section header line, `[V1] (L)`.
    Header,
    /// A chord line, starting with `.`.
    Chords,
    /// A comment line, starting with `;`.
    Comment,
    /// A print-layout line, starting with `-`.
    PrintLayout,
    /// The digit `1`-`9` that starts a numbered verse line.
    VerseNumber,
    /// A first character that is not a space, digit or line marker: the line is read as lyrics
    /// but is probably a mistake (warned about).
    StrayFirstCharacter,
    /// `||`, a slide break.
    SlideBreak,
    /// `|`, a forced line break.
    LineBreak,
    /// `_`, removed from what is shown.
    Spacer,
}

/// The highlighted stretches of `text` as byte ranges in order, never overlapping. A line is
/// one stretch when its first character makes the whole line a header, chord line, comment or
/// print-layout line; in lyric lines only the first-column marker and the `||`, `|` and `_`
/// markers are picked out.
pub fn highlight(text: &str) -> Vec<(std::ops::Range<usize>, Highlight)> {
    let mut spans = Vec::new();
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        let Some(first) = body.chars().next() else {
            start += line.len();
            continue;
        };
        let whole = match first {
            '[' => Some(Highlight::Header),
            '.' => Some(Highlight::Chords),
            ';' => Some(Highlight::Comment),
            '-' => Some(Highlight::PrintLayout),
            _ => None,
        };
        if let Some(kind) = whole {
            spans.push((start..start + body.len(), kind));
        } else {
            let marker = match first {
                '1'..='9' => Some(Highlight::VerseNumber),
                c if c.is_whitespace() => None,
                _ => Some(Highlight::StrayFirstCharacter),
            };
            if let Some(kind) = marker {
                spans.push((start..start + first.len_utf8(), kind));
            }
            let mut at = first.len_utf8();
            while at < body.len() {
                let rest = &body[at..];
                let (len, kind) = if rest.starts_with("||") {
                    (2, Highlight::SlideBreak)
                } else if rest.starts_with('|') {
                    (1, Highlight::LineBreak)
                } else if rest.starts_with('_') {
                    (1, Highlight::Spacer)
                } else {
                    at += rest.chars().next().map_or(1, char::len_utf8);
                    continue;
                };
                spans.push((start + at..start + at + len, kind));
                at += len;
            }
        }
        start += line.len();
    }
    spans
}
