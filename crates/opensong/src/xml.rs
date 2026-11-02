//! A small XML tree, read leniently with quick-xml. Every OpenSong document (song, set, settings)
//! is parsed into this first; the document readers then walk the tree.

use quick_xml::XmlVersion;
use quick_xml::encoding::Decoder;
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

use crate::CodecError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Node {
    Element(Element),
    Text(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Element {
    pub name: String,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Element {
    /// The concatenated text of this element's direct text children.
    pub fn text(&self) -> String {
        self.children
            .iter()
            .filter_map(|n| match n {
                Node::Text(t) => Some(t.as_str()),
                Node::Element(_) => None,
            })
            .collect()
    }

    /// The first child element with this name.
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find_map(|n| match n {
            Node::Element(e) if e.name == name => Some(e),
            _ => None,
        })
    }

    /// The text of the first child element with this name, or `""` if there is none.
    pub fn child_text(&self, name: &str) -> String {
        self.child(name).map(Element::text).unwrap_or_default()
    }
}

fn err(e: impl std::fmt::Display) -> CodecError {
    CodecError(format!("invalid XML: {e}"))
}

/// Parse a document and return its root element. Line endings in text are normalised to `\n`.
pub(crate) fn parse(bytes: &[u8]) -> Result<Element, CodecError> {
    let mut reader = Reader::from_reader(bytes);
    let mut buf = Vec::new();
    // The open elements; index 0 is a synthetic document node.
    let mut stack: Vec<Element> = vec![Element::default()];
    loop {
        match reader.read_event_into(&mut buf).map_err(err)? {
            Event::Start(start) => stack.push(element(&start, reader.decoder())?),
            Event::Empty(start) => push(&mut stack, Node::Element(element(&start, reader.decoder())?)),
            Event::End(_) => {
                let element = stack.pop().filter(|_| !stack.is_empty()).ok_or_else(|| err("unbalanced end tag"))?;
                push(&mut stack, Node::Element(element));
            }
            Event::Text(text) => push_text(&mut stack, &text.xml10_content().map_err(err)?),
            Event::CData(data) => push_text(&mut stack, &data.xml10_content().map_err(err)?),
            Event::GeneralRef(reference) => {
                let resolved = match reference.resolve_char_ref().map_err(err)? {
                    Some(c) => c.to_string(),
                    None => {
                        let name = reference.decode().map_err(err)?;
                        quick_xml::escape::resolve_predefined_entity(&name).ok_or_else(|| err(format!("unknown entity &{name};")))?.to_owned()
                    }
                };
                push_text(&mut stack, &resolved);
            }
            Event::Eof => break,
            Event::Comment(_) | Event::Decl(_) | Event::PI(_) | Event::DocType(_) => {}
        }
        buf.clear();
    }
    if stack.len() != 1 {
        return Err(err("unclosed element"));
    }
    let document = stack.pop().unwrap_or_default();
    document
        .children
        .into_iter()
        .find_map(|n| match n {
            Node::Element(e) => Some(e),
            Node::Text(_) => None,
        })
        .ok_or_else(|| err("no root element"))
}

fn element(start: &BytesStart<'_>, decoder: Decoder) -> Result<Element, CodecError> {
    let mut element = Element { name: String::from_utf8_lossy(start.name().as_ref()).into_owned(), ..Element::default() };
    for attr in start.attributes() {
        let attr = attr.map_err(err)?;
        let value = attr.decoded_and_normalized_value(XmlVersion::Implicit1_0, decoder).map_err(err)?;
        element.attributes.push((String::from_utf8_lossy(attr.key.as_ref()).into_owned(), value.into_owned()));
    }
    Ok(element)
}

fn push(stack: &mut [Element], node: Node) {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    }
}

fn push_text(stack: &mut [Element], text: &str) {
    let Some(parent) = stack.last_mut() else { return };
    match parent.children.last_mut() {
        Some(Node::Text(t)) => t.push_str(text),
        _ => parent.children.push(Node::Text(text.to_owned())),
    }
}
