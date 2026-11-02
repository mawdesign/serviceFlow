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
    /// Sections in file order.
    pub sections: Vec<Section>,
}

/// A named part of a song's lyrics (GLOSSARY "Section").
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Section {
    /// The section tag, such as `V1` or `C`. `""` for lyrics before the first tag.
    pub tag: String,
    /// The section split at its slide breaks (`||`): one entry per slide, each a list of lyric
    /// lines.
    pub slides: Vec<Vec<String>>,
}

/// One slide of a song, in presentation order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slide {
    /// The tag of the section the slide comes from.
    pub section: String,
    pub lines: Vec<String>,
}

impl Song {
    /// The song's slides in presentation order (GLOSSARY "Presentation order"). An empty order
    /// means file order. Tags match case-insensitively, as in OpenSong; a tag with no section is
    /// skipped and a repeated tag repeats its slides.
    pub fn slides(&self) -> Vec<Slide> {
        let order: Vec<&Section> = if self.presentation.is_empty() {
            self.sections.iter().collect()
        } else {
            self.presentation.iter().filter_map(|tag| self.sections.iter().find(|s| s.tag.eq_ignore_ascii_case(tag))).collect()
        };
        order
            .into_iter()
            .flat_map(|section| section.slides.iter().map(|lines| Slide { section: section.tag.clone(), lines: lines.clone() }))
            .collect()
    }
}
