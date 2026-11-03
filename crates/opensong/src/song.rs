//! Song files (opensong.md §2–§6): metadata, the lyrics line grammar, numbered verse blocks and
//! repeated tags. Unmodelled elements are kept as they were read.

use serviceflow_core::song::{Line, RawElement, Section, Song, SongWarning, legacy_multilanguage_base};

use crate::CodecError;
use crate::xml::{self, Node};

/// Elements read into [`Song`] fields. Everything else ends up in [`Song::other_elements`].
const MODELLED: &[&str] = &[
    "title",
    "author",
    "copyright",
    "hymn_number",
    "presentation",
    "ccli",
    "capo",
    "key",
    "aka",
    "key_line",
    "user1",
    "user2",
    "user3",
    "theme",
    "tempo",
    "time_sig",
    "timesig",
    "lyrics",
];

/// Read an OpenSong song file. Elements may come in any order and may be missing.
pub fn read_song(bytes: &[u8]) -> Result<Song, CodecError> {
    let root = xml::parse(bytes)?;
    if root.name != "song" {
        return Err(CodecError(format!("not a song: root element is <{}>", root.name)));
    }
    let text = |name| root.child_text(name);
    let time_sig = match root.child("time_sig") {
        Some(e) => e.text(),
        // Legacy name, read by OpenSong and dropped on its save.
        None => text("timesig"),
    };
    let other_elements = root
        .children
        .iter()
        .filter_map(|n| match n {
            Node::Element(e) if !MODELLED.contains(&e.name.as_str()) => Some(RawElement { name: e.name.clone(), xml: e.to_xml() }),
            _ => None,
        })
        .collect();
    let (sections, warnings) = parse_lyrics(&text("lyrics"));
    Ok(Song {
        title: text("title"),
        author: text("author"),
        copyright: text("copyright"),
        ccli: text("ccli"),
        hymn_number: text("hymn_number"),
        theme: text("theme"),
        aka: text("aka"),
        key: text("key"),
        capo: text("capo"),
        key_line: text("key_line"),
        tempo: text("tempo"),
        time_sig,
        user1: text("user1"),
        user2: text("user2"),
        user3: text("user3"),
        // ASCII only: a no-break space is part of a token (a tag with a space in it).
        presentation: text("presentation").split_ascii_whitespace().map(str::to_owned).collect(),
        sections,
        other_elements,
        warnings,
    })
}

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
fn parse_lyrics(lyrics: &str) -> (Vec<Section>, Vec<SongWarning>) {
    let mut parser = Parser::default();
    for (number, line) in lyrics.lines().map(str::trim_end).enumerate() {
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
    parser.warnings.sort_by_key(|w| match w {
        SongWarning::StrayFirstCharacter { line, .. } | SongWarning::MaybeMultilanguage { line, .. } | SongWarning::BreakInsidePair { line } => *line,
    });
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
