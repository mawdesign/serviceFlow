//! Reading and writing sets (`Sets/<name>`).

use std::sync::Arc;

use serviceflow_core::set::{Advance, CustomItem, ImageItem, ImageSlide, ItemKind, ItemTransition, Set, SetItem, SongItem, StyleAction};
use serviceflow_core::song::RawElement;
use serviceflow_core::style::{ImageSource, Resize};

use crate::CodecError;
use crate::xml::{self, Element, Node};

/// Song item attributes the model describes; any other is kept in [`SongItem::attributes`].
const SONG_ITEM_ATTRIBUTES: [&str; 4] = ["name", "type", "path", "presentation"];

/// Read an OpenSong set. A bad item never stops the set: unknown types become placeholders.
pub fn read_set(bytes: &[u8]) -> Result<Set, CodecError> {
    let root = xml::parse(bytes)?;
    if root.name != "set" {
        return Err(CodecError(format!("not a set: root element is <{}>", root.name)));
    }
    let name = root.attr("name").unwrap_or_default().to_owned();
    let attributes = root.attributes.iter().filter(|(k, _)| k != "name").cloned().collect();
    let other_elements = root
        .children
        .iter()
        .filter_map(|n| match n {
            Node::Element(e) if e.name != "slide_groups" => Some(RawElement { name: e.name.clone(), xml: e.to_xml() }),
            _ => None,
        })
        .collect();
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
    Ok(Set { name, items, attributes, other_elements })
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
            attributes: group.attributes.iter().filter(|(k, _)| !SONG_ITEM_ATTRIBUTES.contains(&k.as_str())).cloned().collect(),
        })
    } else if item_type.eq_ignore_ascii_case("custom") {
        ItemKind::Custom(CustomItem {
            title: group.child_text("title").trim().to_owned(),
            subtitle: group.child_text("subtitle").trim().to_owned(),
            slides: slides(group).map(|slide| body_text(&slide.child_text("body"))).collect(),
            advance: advance(group),
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
            advance: advance(group),
            xml: group.to_xml(),
        })
    } else {
        ItemKind::Placeholder { item_type: item_type.to_ascii_lowercase(), xml: group.to_xml() }
    };
    SetItem { name, notes, style, kind }
}

/// `seconds`, `loop` and `transition` of a custom or image item. (`link` is not read: it is kept
/// in the item's XML and written back untouched.)
fn advance(group: &Element) -> Advance {
    let seconds =
        group.attr("seconds").and_then(|v| v.trim().parse::<f64>().ok()).filter(|s| s.is_finite() && *s >= 0.0).map_or(0, |s| s.round() as u32);
    let transition = match group.attr("transition").map(str::trim) {
        Some("1") => ItemTransition::On,
        Some("2") => ItemTransition::Off,
        _ => ItemTransition::Default,
    };
    Advance { seconds, looping: crate::settings::flag(group, "loop", false), transition }
}

/// Set the attributes of `group` that differ from what `new` says (so unreadable values and
/// attributes serviceFlow does not model stay as they were).
fn set_advance(group: &mut Element, new: Advance) {
    let old = advance(group);
    if old.seconds != new.seconds {
        if new.seconds == 0 { group.remove_attr("seconds") } else { group.set_attr("seconds", &new.seconds.to_string()) }
    }
    if old.looping != new.looping {
        group.set_attr("loop", if new.looping { "true" } else { "false" });
    }
    if old.transition != new.transition {
        group.set_attr(
            "transition",
            match new.transition {
                ItemTransition::Default => "0",
                ItemTransition::On => "1",
                ItemTransition::Off => "2",
            },
        );
    }
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

/// Write a set as OpenSong reads it: `<set name>` with its `slide_groups`, two-space indents,
/// UTF-8, CRLF line endings and a final newline (ADR-0002). Song items are written from the
/// model (`name`, `type`, `path` with OpenSong's trailing `/`, `presentation` when it has an
/// override, then any other attributes as read; `notes` when not empty, then the other children
/// as read). Every other item, and anything else the set held, is written back as read.
pub fn write_set(set: &Set) -> Vec<u8> {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let mut root_attributes = Vec::new();
    if !set.name.is_empty() {
        root_attributes.push(("name".to_owned(), set.name.clone()));
    }
    root_attributes.extend(set.attributes.iter().cloned());
    out.push_str(&Element::start_tag("set", &root_attributes));
    out.push_str(">\n");
    if set.items.is_empty() {
        out.push_str("  <slide_groups/>\n");
    } else {
        out.push_str("  <slide_groups>\n");
        for item in &set.items {
            out.push_str("    ");
            write_item(&mut out, item);
            out.push('\n');
        }
        out.push_str("  </slide_groups>\n");
    }
    for element in &set.other_elements {
        out.push_str("  ");
        out.push_str(&element.xml);
        out.push('\n');
    }
    out.push_str("</set>\n");
    // Text is read with LF line endings; every line ending is written as CRLF.
    out.replace("\r\n", "\n").replace('\r', "\n").replace('\n', "\r\n").into_bytes()
}

fn write_item(out: &mut String, item: &SetItem) {
    let song = match &item.kind {
        ItemKind::Song(song) => song,
        ItemKind::Custom(CustomItem { xml, .. })
        | ItemKind::Image(ImageItem { xml, .. })
        | ItemKind::Style { xml, .. }
        | ItemKind::Placeholder { xml, .. } => {
            // Unchanged since it was read: exactly as read. Edited or new: rebuilt from the model,
            // over what was read, so attributes and children the model does not describe stay.
            let base = (!xml.is_empty()).then(|| xml::parse(xml.as_bytes()).ok()).flatten().filter(|e| e.name == "slide_group");
            match base {
                Some(base) if read_item(&base) == *item => out.push_str(xml),
                base => out.push_str(&rebuilt(item, base.unwrap_or_else(|| Element::with_text("slide_group", ""))).to_xml()),
            }
            return;
        }
    };
    let mut attributes = vec![
        ("name".to_owned(), item.name.clone()),
        ("type".to_owned(), "song".to_owned()),
        ("path".to_owned(), if song.path.is_empty() { String::new() } else { format!("{}/", song.path) }),
    ];
    if !song.presentation.is_empty() {
        attributes.push(("presentation".to_owned(), song.presentation.join(" ")));
    }
    attributes.extend(song.attributes.iter().cloned());
    out.push_str(&Element::start_tag("slide_group", &attributes));
    if item.notes.is_empty() && song.other_elements.is_empty() {
        out.push_str("/>");
        return;
    }
    out.push_str(">\n");
    if !item.notes.is_empty() {
        out.push_str("      ");
        out.push_str(&Element::text_element("notes", &[], &item.notes));
        out.push('\n');
    }
    for element in &song.other_elements {
        out.push_str("      ");
        out.push_str(&element.xml);
        out.push('\n');
    }
    out.push_str("    </slide_group>");
}

/// An edited or new custom, image, style or kept item written from the model over `group` (the
/// item as read, or an empty `slide_group`): the modelled attributes and children are set in
/// place or added, everything else in `group` stays as it was.
fn rebuilt(item: &SetItem, mut group: Element) -> Element {
    group.set_attr("name", &item.name);
    let text = Element::with_text;
    match &item.kind {
        ItemKind::Custom(custom) => {
            group.set_attr("type", "custom");
            set_advance(&mut group, custom.advance);
            group.set_child(text("title", &custom.title));
            group.set_child(text("subtitle", &custom.subtitle));
            group.set_child(text("notes", &item.notes));
            let slides = custom.slides.iter().map(|body| Element { children: vec![Node::Element(text("body", body))], ..text("slide", "") });
            group.set_child(Element { children: slides.map(Node::Element).collect(), ..text("slides", "") });
        }
        ItemKind::Image(image) => {
            group.set_attr("type", "image");
            set_advance(&mut group, image.advance);
            group.set_attr(
                "resize",
                match image.resize {
                    Resize::Screen => "screen",
                    Resize::Body => "body",
                    Resize::None => "",
                },
            );
            group.set_attr("keep_aspect", if image.keep_aspect { "true" } else { "false" });
            group.set_child(text("title", &image.title));
            group.set_child(text("subtitle", &image.subtitle));
            group.set_child(text("notes", &item.notes));
            let slides = image.slides.iter().map(|slide| {
                let picture = match &slide.image {
                    Some(ImageSource::File(name)) => Some(text("filename", name)),
                    Some(ImageSource::Embedded(data)) => Some(text("image", data)),
                    None => None,
                };
                let children = picture.into_iter().chain([text("description", &slide.description)]).map(Node::Element).collect();
                Element { children, ..text("slide", "") }
            });
            group.set_child(Element { children: slides.map(Node::Element).collect(), ..text("slides", "") });
        }
        ItemKind::Style { action, .. } => {
            group.set_attr("type", "style");
            match action {
                StyleAction::New => group.set_attr("action", "new"),
                StyleAction::Revert => group.set_attr("action", "revert"),
                // Whatever other action it had stays.
                StyleAction::Other => {}
            }
            if !item.notes.is_empty() || group.child("notes").is_some() {
                group.set_child(text("notes", &item.notes));
            }
        }
        ItemKind::Placeholder { .. } => {
            if !item.notes.is_empty() || group.child("notes").is_some() {
                group.set_child(text("notes", &item.notes));
            }
        }
        ItemKind::Song(_) => {}
    }
    // The item's own style: as read when unchanged, otherwise written from the model.
    match &item.style {
        Some(style) if group.child("style").map(crate::settings::style).as_ref() != Some(&**style) => {
            group.set_child(crate::settings::style_element(style));
        }
        Some(_) => {}
        None => group.remove_child("style"),
    }
    group
}
