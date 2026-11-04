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

    /// The value of an attribute.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attributes.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    /// `<name>text</name>` (`<name/>` when the text is empty).
    pub fn with_text(name: &str, text: &str) -> Element {
        let children = if text.is_empty() { Vec::new() } else { vec![Node::Text(text.to_owned())] };
        Element { name: name.to_owned(), attributes: Vec::new(), children }
    }

    /// Set an attribute, in place when it exists, otherwise at the end.
    pub fn set_attr(&mut self, name: &str, value: &str) {
        match self.attributes.iter_mut().find(|(k, _)| k == name) {
            Some((_, v)) => *v = value.to_owned(),
            None => self.attributes.push((name.to_owned(), value.to_owned())),
        }
    }

    /// Remove attribute `name`, if present.
    pub fn remove_attr(&mut self, name: &str) {
        self.attributes.retain(|(k, _)| k != name);
    }

    /// Replace the first child element named like `element`, in place, or add it at the end.
    pub fn set_child(&mut self, element: Element) {
        let slot = self.children.iter_mut().find(|n| matches!(n, Node::Element(e) if e.name == element.name));
        match slot {
            Some(slot) => *slot = Node::Element(element),
            None => self.children.push(Node::Element(element)),
        }
    }

    /// Remove every child element with this name.
    pub fn remove_child(&mut self, name: &str) {
        self.children.retain(|n| !matches!(n, Node::Element(e) if e.name == name));
    }

    /// The text of the first child element with this name, or `""` if there is none.
    pub fn child_text(&self, name: &str) -> String {
        self.child(name).map(Element::text).unwrap_or_default()
    }
}

fn err(e: impl std::fmt::Display) -> CodecError {
    CodecError(format!("invalid XML: {e}"))
}

impl Element {
    /// This element as XML text: start tag, children and end tag (`<name/>` when it has no
    /// children). Text and attribute values are escaped.
    pub fn to_xml(&self) -> String {
        let mut out = String::new();
        self.write_xml(&mut out);
        out
    }

    /// `<name attributes>text</name>`, escaped; empty text is still written as a start and an
    /// end tag, as OpenSong writes empty elements.
    pub fn text_element(name: &str, attributes: &[(String, String)], text: &str) -> String {
        let mut out = String::new();
        let element = Element { name: name.to_owned(), attributes: attributes.to_vec(), children: Vec::new() };
        element.write_start(&mut out);
        out.push('>');
        out.push_str(&quick_xml::escape::partial_escape(text));
        out.push_str("</");
        out.push_str(name);
        out.push('>');
        out
    }

    /// `<name attributes` without the closing `>`, escaped.
    pub fn start_tag(name: &str, attributes: &[(String, String)]) -> String {
        let mut out = String::new();
        Element { name: name.to_owned(), attributes: attributes.to_vec(), children: Vec::new() }.write_start(&mut out);
        out
    }

    fn write_start(&self, out: &mut String) {
        out.push('<');
        out.push_str(&self.name);
        for (key, value) in &self.attributes {
            out.push(' ');
            out.push_str(key);
            out.push_str("=\"");
            out.push_str(&quick_xml::escape::escape(value.as_str()));
            out.push('"');
        }
    }

    fn write_xml(&self, out: &mut String) {
        self.write_start(out);
        if self.children.is_empty() {
            out.push_str("/>");
            return;
        }
        out.push('>');
        for child in &self.children {
            match child {
                Node::Element(e) => e.write_xml(out),
                Node::Text(t) => out.push_str(&quick_xml::escape::partial_escape(t.as_str())),
            }
        }
        out.push_str("</");
        out.push_str(&self.name);
        out.push('>');
    }
}

/// The document as text (opensong.md §1). In order: a byte-order mark (which wins over the
/// declaration, as in OnSong's UTF-16 files that claim UTF-8); UTF-16 without a mark; the
/// `encoding` of the XML declaration; UTF-8; and Windows-1252 for undeclared bytes that are not
/// valid UTF-8.
fn decode(bytes: &[u8]) -> Result<String, CodecError> {
    use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1252};
    if let Some((encoding, bom)) = Encoding::for_bom(bytes) {
        return Ok(encoding.decode_without_bom_handling(&bytes[bom..]).0.into_owned());
    }
    match bytes {
        [b'<', 0, ..] => return Ok(UTF_16LE.decode_without_bom_handling(bytes).0.into_owned()),
        [0, b'<', ..] => return Ok(UTF_16BE.decode_without_bom_handling(bytes).0.into_owned()),
        _ => {}
    }
    let declared = declared_encoding(bytes)
        .map(|label| Encoding::for_label(label.as_bytes()).ok_or_else(|| CodecError(format!("unsupported encoding \"{label}\""))))
        .transpose()?;
    match declared {
        Some(encoding) if encoding != UTF_8 => Ok(encoding.decode_without_bom_handling(bytes).0.into_owned()),
        _ => match std::str::from_utf8(bytes) {
            Ok(text) => Ok(text.to_owned()),
            Err(_) => Ok(WINDOWS_1252.decode_without_bom_handling(bytes).0.into_owned()),
        },
    }
}

/// The `encoding` named in a leading `<?xml …?>` declaration.
fn declared_encoding(bytes: &[u8]) -> Option<String> {
    let start = bytes.iter().position(|b| !b.is_ascii_whitespace())?;
    let rest = bytes[start..].strip_prefix(b"<?xml")?;
    let end = rest.windows(2).position(|w| w == b"?>")?;
    let decl = std::str::from_utf8(&rest[..end]).ok()?;
    let after = &decl[decl.find("encoding")? + "encoding".len()..];
    let after = after.trim_start().strip_prefix('=')?.trim_start();
    let quote = after.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let value = &after[1..];
    Some(value[..value.find(quote)?].trim().to_owned())
}

/// Parse a document and return its root element. Any encoding [`decode`] knows is accepted;
/// line endings in text are normalised to `\n`.
pub(crate) fn parse(bytes: &[u8]) -> Result<Element, CodecError> {
    let text = decode(bytes)?;
    let mut reader = Reader::from_reader(text.as_bytes());
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
