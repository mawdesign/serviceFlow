//! Reading sets (`Sets/<name>`).

use std::sync::Arc;

use serviceflow_core::set::{CustomItem, ImageItem, ImageSlide, ItemKind, Set, SetItem, SongItem, StyleAction};
use serviceflow_core::song::RawElement;
use serviceflow_core::style::{ImageSource, Resize};

use crate::CodecError;
use crate::xml::{self, Element, Node};

/// Read an OpenSong set. A bad item never stops the set: unknown types become placeholders.
pub fn read_set(bytes: &[u8]) -> Result<Set, CodecError> {
    let root = xml::parse(bytes)?;
    if root.name != "set" {
        return Err(CodecError(format!("not a set: root element is <{}>", root.name)));
    }
    let name = root.attr("name").unwrap_or_default().to_owned();
    let items = root
        .child("slide_groups")
        .map(|groups| {
            groups
                .children
                .iter()
                .filter_map(|n| match n {
                    Node::Element(e) if e.name == "slide_group" => Some(read_item(e)),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Set { name, items })
}

fn read_item(group: &Element) -> SetItem {
    let name = group.attr("name").unwrap_or_default().to_owned();
    let notes = group.child_text("notes");
    let item_type = group.attr("type").unwrap_or_default();
    let style = group.child("style").map(|e| Arc::new(crate::settings::style(e)));
    let kind = if item_type.eq_ignore_ascii_case("style") {
        let action = match group.attr("action").map(|a| a.trim().to_ascii_lowercase()).as_deref() {
            Some("new") => StyleAction::New,
            Some("revert") => StyleAction::Revert,
            _ => StyleAction::Other,
        };
        ItemKind::Style { action, xml: group.to_xml() }
    } else if item_type.eq_ignore_ascii_case("song") {
        let other_elements = group
            .children
            .iter()
            .filter_map(|n| match n {
                Node::Element(e) if e.name != "notes" => Some(RawElement { name: e.name.clone(), xml: e.to_xml() }),
                _ => None,
            })
            .collect();
        ItemKind::Song(SongItem {
            path: clean_path(group.attr("path").unwrap_or_default()),
            presentation: group.attr("presentation").unwrap_or_default().split_ascii_whitespace().map(str::to_owned).collect(),
            other_elements,
        })
    } else if item_type.eq_ignore_ascii_case("custom") {
        ItemKind::Custom(CustomItem {
            title: group.child_text("title").trim().to_owned(),
            subtitle: group.child_text("subtitle").trim().to_owned(),
            slides: slides(group).map(|slide| body_text(&slide.child_text("body"))).collect(),
            xml: group.to_xml(),
        })
    } else if item_type.eq_ignore_ascii_case("image") {
        let resize = match group.attr("resize").map(|r| r.trim().to_ascii_lowercase()).as_deref() {
            None | Some("screen") => Resize::Screen,
            Some("body") => Resize::Body,
            Some(_) => Resize::None,
        };
        ItemKind::Image(ImageItem {
            title: group.child_text("title").trim().to_owned(),
            subtitle: group.child_text("subtitle").trim().to_owned(),
            resize,
            keep_aspect: crate::settings::flag(group, "keep_aspect", false),
            slides: slides(group).map(image_slide).collect(),
            xml: group.to_xml(),
        })
    } else {
        ItemKind::Placeholder { item_type: item_type.to_ascii_lowercase(), xml: group.to_xml() }
    };
    SetItem { name, notes, style, kind }
}

/// OpenSong's `CleanPath`: `\` and `/` separators, no leading, trailing or doubled ones.
fn clean_path(path: &str) -> String {
    path.split(['/', '\\']).filter(|part| !part.is_empty()).collect::<Vec<_>>().join("/")
}

/// The `slides/slide` elements of an item.
fn slides(group: &Element) -> impl Iterator<Item = &Element> {
    group.child("slides").into_iter().flat_map(|s| s.children.iter()).filter_map(|n| match n {
        Node::Element(e) if e.name == "slide" => Some(e),
        _ => None,
    })
}

/// A slide's body: lines right-trimmed, no empty lines at the start or end.
fn body_text(body: &str) -> String {
    let lines: Vec<&str> = body.lines().map(str::trim_end).collect();
    let start = lines.iter().position(|l| !l.is_empty()).unwrap_or(lines.len());
    let end = lines.iter().rposition(|l| !l.is_empty()).map_or(start, |i| i + 1);
    lines[start..end].join("\n")
}

fn image_slide(slide: &Element) -> ImageSlide {
    let file = slide.child_text("filename");
    let data: String = slide.child_text("image").split_whitespace().collect();
    let image = match (file.trim(), data.is_empty()) {
        // OpenSong writes `\` on Windows; ids use `/`.
        (name, _) if !name.is_empty() => Some(ImageSource::File(name.replace('\\', "/"))),
        (_, false) => Some(ImageSource::Embedded(data)),
        _ => None,
    };
    ImageSlide { image, description: slide.child_text("description").trim().to_owned() }
}
