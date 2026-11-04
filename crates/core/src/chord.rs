//! Chord symbols, transposition and capo (GLOSSARY "Transposition", "Capo"). Standard letter
//! names only (`A`-`G` with `#` or `b`); anything the parser does not recognise (a rest `N.C.`, a
//! repeat mark, a word) passes through unchanged. Pure text work: positions are never touched.

use crate::song::Song;

/// Whether black-key notes are written with sharps or flats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spelling {
    Sharps,
    Flats,
}

const SHARPS: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const FLATS: [&str; 12] = ["C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B"];

/// The note name of pitch class `pc` (0 = C).
pub fn note_name(pc: i32, spelling: Spelling) -> &'static str {
    let table = if spelling == Spelling::Sharps { &SHARPS } else { &FLATS };
    table[pc.rem_euclid(12) as usize]
}

/// A note name at the start of `text`: its pitch class and how many bytes it takes.
fn note(text: &str) -> Option<(i32, usize)> {
    let mut chars = text.chars();
    let base = match chars.next()? {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    match chars.next() {
        Some('#') => Some(((base + 1) % 12, 2)),
        Some('b') => Some(((base + 11) % 12, 2)),
        _ => Some((base, 1)),
    }
}

/// A chord symbol: root, the quality and extensions after it (kept as written) and an optional
/// slash bass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    /// Pitch class of the root (0 = C).
    pub root: i32,
    /// Everything between the root and the bass: `m7`, `sus4`, `add9`, `b5`, `6/9` ...
    pub suffix: String,
    /// Pitch class of the bass note of a slash chord.
    pub bass: Option<i32>,
}

impl Symbol {
    /// Read a whole token as a chord, or `None` when any of it is not understood.
    pub fn parse(token: &str) -> Option<Symbol> {
        let (root, used) = note(token)?;
        let rest = &token[used..];
        // A trailing `/X` with X a note is the bass; `6/9` and the like stay in the suffix.
        let (suffix, bass) = match rest.rfind('/') {
            Some(at) => match note(&rest[at + 1..]) {
                Some((pc, len)) if at + 1 + len == rest.len() => (&rest[..at], Some(pc)),
                _ => (rest, None),
            },
            None => (rest, None),
        };
        valid_suffix(suffix).then(|| Symbol { root, suffix: suffix.to_owned(), bass })
    }

    /// The chord moved by `semitones`, written with `spelling`.
    pub fn shifted(&self, semitones: i32, spelling: Spelling) -> String {
        let mut out = note_name(self.root + semitones, spelling).to_owned();
        out.push_str(&self.suffix);
        if let Some(bass) = self.bass {
            out.push('/');
            out.push_str(note_name(bass + semitones, spelling));
        }
        out
    }
}

/// Whether `suffix` is made only of qualities and extensions we know.
fn valid_suffix(suffix: &str) -> bool {
    const WORDS: [&str; 11] = ["maj", "min", "dim", "aug", "sus", "add", "omit", "alt", "no", "m", "M"];
    let mut rest = suffix;
    while let Some(c) = rest.chars().next() {
        if let Some(word) = WORDS.iter().find(|w| rest.starts_with(**w)) {
            rest = &rest[word.len()..];
            continue;
        }
        let after = &rest[c.len_utf8()..];
        match c {
            '0'..='9' | '+' | '-' | '(' | ')' | ',' | '°' | 'ø' | 'Δ' | '^' => {}
            // An altered extension (`b5`, `#9`) or the second half of `6/9`.
            '#' | 'b' | '/' if after.starts_with(|n: char| n.is_ascii_digit()) => {}
            _ => return false,
        }
        rest = after;
    }
    true
}

/// `token` moved by `semitones`, or unchanged when it is not a chord.
pub fn transpose_token(token: &str, semitones: i32, spelling: Spelling) -> String {
    match Symbol::parse(token) {
        Some(symbol) => symbol.shifted(semitones, spelling),
        None => token.to_owned(),
    }
}

/// The spelling a key calls for: the usual signature of the major or minor key with this tonic
/// (C and A minor take sharps; F# / Gb major take sharps; Eb minor takes flats).
pub fn spelling_of_key(tonic: i32, minor: bool) -> Spelling {
    use Spelling::{Flats as F, Sharps as S};
    const MAJOR: [Spelling; 12] = [S, F, S, F, S, F, S, S, F, S, F, S];
    const MINOR: [Spelling; 12] = [F, S, F, F, S, F, S, F, S, S, F, S];
    let table = if minor { MINOR } else { MAJOR };
    table[tonic.rem_euclid(12) as usize]
}

/// A song's key as written (`G`, `F#m`, `Bb`): the tonic and whether it is minor.
pub fn parse_key(key: &str) -> Option<(i32, bool)> {
    let symbol = Symbol::parse(key.trim())?;
    let minor = (symbol.suffix.starts_with('m') && !symbol.suffix.starts_with("maj")) || symbol.suffix.starts_with("min");
    Some((symbol.root, minor))
}

/// The key `key` moved by `semitones`, written for its new tonic; `None` when it is not a key.
pub fn shift_key(key: &str, semitones: i32) -> Option<String> {
    let symbol = Symbol::parse(key.trim())?;
    let (tonic, minor) = parse_key(key)?;
    Some(symbol.shifted(semitones, spelling_of_key(tonic + semitones, minor)))
}

/// An item's transposition and capo, as stored in the sidecar (GLOSSARY "Transposition",
/// "Capo"). The default changes nothing (but a song's own printed capo still applies).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Shift {
    /// Semitones up (negative: down). The sound changes.
    pub semitones: i32,
    /// The capo fret; `None` is the song's own capo, `Some(0)` is no capo.
    pub capo: Option<u8>,
}

/// The capo a song asks to have printed (`<capo print="true">3</capo>`), if any.
pub fn song_capo(song: &Song) -> Option<u8> {
    let print = song.capo_attributes.iter().any(|(name, value)| name == "print" && value.eq_ignore_ascii_case("true"));
    print.then(|| song.capo.trim().parse::<u8>().ok()).flatten().filter(|fret| (1..12).contains(fret))
}

impl Shift {
    /// The capo that applies to `song`.
    pub fn capo_for(&self, song: &Song) -> u8 {
        self.capo.or_else(|| song_capo(song)).unwrap_or(0) % 12
    }

    /// What this shift does to the chords of `song`; `None` when they stay as written.
    pub fn applied(&self, song: &Song) -> Option<Applied> {
        let net = self.semitones - i32::from(self.capo_for(song));
        if net.rem_euclid(12) == 0 {
            return None;
        }
        // The shapes are spelled for the key they are played in (the sounding key less the capo).
        let spelling = match parse_key(&song.key) {
            Some((tonic, minor)) => spelling_of_key(tonic + net, minor),
            None if net < 0 => Spelling::Flats,
            None => Spelling::Sharps,
        };
        Some(Applied { semitones: net, spelling })
    }

    /// The keys to show for `song` with this shift.
    pub fn key_info(&self, song: &Song) -> KeyInfo {
        let capo = self.capo_for(song);
        KeyInfo {
            original: song.key.trim().to_owned(),
            sounding: shift_key(&song.key, self.semitones),
            shapes: shift_key(&song.key, self.semitones - i32::from(capo)),
            semitones: self.semitones,
            capo,
        }
    }
}

/// A shift worked out for a song: the net semitones (transposition less capo) and the spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Applied {
    pub semitones: i32,
    pub spelling: Spelling,
}

impl Applied {
    /// The text of one chord as shown.
    pub fn chord(&self, text: &str) -> String {
        transpose_token(text, self.semitones, self.spelling)
    }
}

/// The keys of a song item as shown to the operator (GLOSSARY "Capo": shapes relative to the
/// capo, with the sounding key shown when wanted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyInfo {
    /// The song's key as written (empty when it has none).
    pub original: String,
    /// The key that sounds after transposing (`None` when the song's key is not a key).
    pub sounding: Option<String>,
    /// The key the shapes are played in, with the capo (`None` as above).
    pub shapes: Option<String>,
    pub semitones: i32,
    pub capo: u8,
}

/// Rewrite the chord lines (`.` lines) of `lyrics` moved by `semitones`, for "Save in this key".
/// Each chord keeps its column; one that grew pushes the rest of its line right, keeping a space
/// between chords. Lyrics, comments and everything else are untouched.
pub fn transpose_lyrics(lyrics: &str, semitones: i32, spelling: Spelling) -> String {
    let mut out = String::with_capacity(lyrics.len());
    for line in lyrics.split_inclusive('\n') {
        match line.strip_prefix('.') {
            Some(rest) => {
                let body = rest.trim_end_matches(['\n', '\r']);
                out.push('.');
                out.push_str(&transpose_chord_line(body, semitones, spelling));
                out.push_str(&rest[body.len()..]);
            }
            None => out.push_str(line),
        }
    }
    out
}

fn transpose_chord_line(body: &str, semitones: i32, spelling: Spelling) -> String {
    let mut out = String::new();
    let mut width = 0; // characters written so far
    for (column, token) in crate::song::chord_columns(body) {
        let text = transpose_token(token, semitones, spelling);
        let at = if width == 0 { column } else { column.max(width + 1) };
        out.extend(std::iter::repeat_n(' ', at - width));
        width = at + text.chars().count();
        out.push_str(&text);
    }
    out
}
