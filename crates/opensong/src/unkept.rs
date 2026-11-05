//! What a rewrite of a song or set file would drop: the parts of the file the readers do not
//! keep. Tidy library leaves such files alone (ADR-0002: a rewrite never loses information).

use serviceflow_core::plan::Unkept;

use crate::CodecError;
use crate::song::MODELLED;
use crate::xml::{self, Element, Node};

/// What [`write_song`](crate::write_song) of this song file would not keep. Empty: rewriting it
/// loses nothing beyond layout.
pub fn unkept_in_song(bytes: &[u8]) -> Result<Vec<Unkept>, CodecError> {
    let root = xml::parse(bytes)?;
    let mut unkept = common(bytes, &root)?;
    let mut seen: Vec<&str> = Vec::new();
    for e in elements(&root) {
        if !MODELLED.contains(&e.name.as_str()) {
            // Kept whole, attributes and all.
            continue;
        }
        if seen.contains(&e.name.as_str()) {
            unkept.push(Unkept::Repeated(e.name.clone()));
        }
        seen.push(&e.name);
        if e.name != "capo" && !e.attributes.is_empty() {
            unkept.push(Unkept::Attributes(e.name.clone()));
        }
        if elements(e).next().is_some() {
            unkept.push(Unkept::Elements(e.name.clone()));
        }
    }
    if seen.contains(&"timesig") && seen.contains(&"time_sig") {
        unkept.push(Unkept::OldTimeSignature);
    }
    Ok(tidy(unkept))
}

/// What [`write_set`](crate::write_set) of this set file would not keep. Empty: rewriting it
/// loses nothing beyond layout.
pub fn unkept_in_set(bytes: &[u8]) -> Result<Vec<Unkept>, CodecError> {
    let root = xml::parse(bytes)?;
    let mut unkept = common(bytes, &root)?;
    // The root's attributes and other elements are kept; of `slide_groups` only its items.
    let mut groups = elements(&root).filter(|e| e.name == "slide_groups");
    if let Some(groups) = groups.next() {
        if !groups.attributes.is_empty() {
            unkept.push(Unkept::Attributes(groups.name.clone()));
        }
        if has_text(groups) {
            unkept.push(Unkept::Text(groups.name.clone()));
        }
        for e in elements(groups) {
            if e.name != "slide_group" {
                unkept.push(Unkept::Elements(groups.name.clone()));
            } else if e.attr("type").is_some_and(|t| t.eq_ignore_ascii_case("song")) {
                song_item(e, &mut unkept);
            }
            // Other items are kept as their XML.
        }
    }
    if groups.next().is_some() {
        unkept.push(Unkept::Repeated("slide_groups".to_owned()));
    }
    Ok(tidy(unkept))
}

/// A song item keeps its attributes and every child but `notes`, of which only the text of the
/// first is read.
fn song_item(item: &Element, unkept: &mut Vec<Unkept>) {
    if has_text(item) {
        unkept.push(Unkept::Text(item.name.clone()));
    }
    let mut notes = elements(item).filter(|e| e.name == "notes");
    if let Some(first) = notes.next() {
        if !first.attributes.is_empty() {
            unkept.push(Unkept::Attributes(first.name.clone()));
        }
        if elements(first).next().is_some() {
            unkept.push(Unkept::Elements(first.name.clone()));
        }
    }
    if notes.next().is_some() {
        unkept.push(Unkept::Repeated("notes".to_owned()));
    }
}

/// Comments anywhere, attributes of a song's root (a set keeps its own) and text between the
/// root's elements.
fn common(bytes: &[u8], root: &Element) -> Result<Vec<Unkept>, CodecError> {
    let mut unkept = Vec::new();
    if xml::has_comments(bytes)? {
        unkept.push(Unkept::Comments);
    }
    if root.name == "song" && !root.attributes.is_empty() {
        unkept.push(Unkept::Attributes(root.name.clone()));
    }
    if has_text(root) {
        unkept.push(Unkept::Text(root.name.clone()));
    }
    Ok(unkept)
}

fn elements(e: &Element) -> impl Iterator<Item = &Element> {
    e.children.iter().filter_map(|n| match n {
        Node::Element(e) => Some(e),
        Node::Text(_) => None,
    })
}

/// Text other than layout whitespace directly inside `e`, between its elements.
fn has_text(e: &Element) -> bool {
    e.children.iter().any(|n| matches!(n, Node::Text(t) if !t.trim().is_empty()))
}

fn tidy(mut unkept: Vec<Unkept>) -> Vec<Unkept> {
    unkept.sort();
    unkept.dedup();
    unkept
}
