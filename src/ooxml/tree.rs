//! A lossless XML tree.
//!
//! ## Why this is hand-written
//!
//! [ADR-0005](../../../../unsoftone/docs/adr/0005-ooxml-lossless-layer.md) makes
//! one requirement non-negotiable: **never destroy content we do not
//! understand.** A missing feature is visible; a silent substitution is not.
//!
//! That rules out the usual approach. Parse into a model of what you know, then
//! re-serialise the model — and every element you did not model is gone. It also
//! rules out most parsers, which normalise on the way in: attribute order is
//! sorted, quotes are unified, whitespace is reflowed, comments and CDATA
//! dropped, namespace prefixes rewritten. Each of those is individually
//! defensible and collectively a way of losing a user's document.
//!
//! So the tree keeps every byte it did not have a reason to change. It stores
//! each node's original text and reproduces it verbatim. Serialising an unedited
//! tree is byte-identical to the input, and that is a testable property rather
//! than an aspiration.
//!
//! ## What "editing" means here
//!
//! [`Node::replace_text`] changes exactly the matched text nodes and nothing
//! else. There is deliberately no "add element", "set attribute" or "normalise"
//! API yet: every one of those is a chance to drop something, and each should be
//! added alongside the test that proves it does not.

use std::fmt;

/// A parse failure.
///
/// Carries position because "your document is broken" is useless to a user
/// without a location, and carrying it costs nothing now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub offset: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "XML parse error at byte {}: {}",
            self.offset, self.message
        )
    }
}

impl std::error::Error for ParseError {}

/// One node in the document.
///
/// `Element` covers elements, comments, CDATA, processing instructions and the
/// prolog, because they all share one requirement: come back unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Element(Element),
    /// A comment, stored with its delimiters.
    Comment(String),
    /// A CDATA section, stored with its delimiters.
    CData(String),
    /// A processing instruction or XML declaration, without `<?`/`?>`.
    ProcessingInstruction(String),
    /// Character data.
    ///
    /// Stored verbatim, including whitespace. Whitespace between elements is
    /// significant to Word's indentation-sensitive behaviour in some parts.
    ///
    /// ## Undefined entities are an error, not a pass-through
    ///
    /// An unrecognised entity reference (`&nbsp;` with no declaration, or a bare
    /// `&foo;`) fails the parse.
    ///
    /// Passing it through looks more forgiving and is worse. In XML, `&` begins a
    /// reference and a conforming consumer resolves it; an undefined one means
    /// the document is malformed. Accepting it silently means U1 Docs echoes back
    /// a file it could not actually interpret — and the corruption surfaces later,
    /// in someone's word processor, where nobody will connect it to the save that
    /// caused it.
    ///
    /// Note the five predefined entities (`&amp;` and friends) and numeric
    /// character references *are* accepted, since those are always defined.
    Text(String),
}

impl Node {
    /// Parse a complete XML document.
    pub fn parse(input: &[u8]) -> Result<Node, ParseError> {
        let src = std::str::from_utf8(input).map_err(|e| ParseError {
            offset: e.valid_up_to(),
            message: format!("not valid UTF-8: {e}"),
        })?;
        Parser::new(src).parse_document()
    }

    /// Reproduce this node's bytes exactly.
    pub fn serialize(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.write_to(&mut out);
        out
    }

    fn write_to(&self, out: &mut Vec<u8>) {
        match self {
            Node::Element(e) => e.write_to(out),
            Node::Comment(body) => {
                out.extend_from_slice(b"<!--");
                out.extend_from_slice(body.as_bytes());
                out.extend_from_slice(b"-->");
            }
            Node::CData(body) => {
                out.extend_from_slice(b"<![CDATA[");
                out.extend_from_slice(body.as_bytes());
                out.extend_from_slice(b"]]>");
            }
            Node::ProcessingInstruction(body) => {
                out.extend_from_slice(b"<?");
                out.extend_from_slice(body.as_bytes());
                out.extend_from_slice(b"?>");
            }
            Node::Text(t) => out.extend_from_slice(t.as_bytes()),
        }
    }

    /// The element's local name, if this is an element.
    pub fn local_name(&self) -> Option<&str> {
        match self {
            Node::Element(e) => Some(e.local_name()),
            _ => None,
        }
    }

    /// Direct element children.
    pub fn children(&self) -> impl Iterator<Item = &Node> {
        const EMPTY: &[Node] = &[];
        match self {
            Node::Element(e) => e.children.iter(),
            _ => EMPTY.iter(),
        }
    }

    /// First descendant with the given local name, depth-first.
    pub fn find(&self, local_name: &str) -> Option<&Node> {
        if self.local_name() == Some(local_name) {
            return Some(self);
        }
        for child in self.children() {
            if let Some(found) = child.find(local_name) {
                return Some(found);
            }
        }
        None
    }

    /// All descendants with the given local name, in document order.
    pub fn find_all<'a>(&'a self, local_name: &str, out: &mut Vec<&'a Node>) {
        if self.local_name() == Some(local_name) {
            out.push(self);
        }
        for child in self.children() {
            child.find_all(local_name, out);
        }
    }

    /// All text content, concatenated, in document order.
    pub fn text_content(&self) -> String {
        let mut s = String::new();
        self.collect_text(&mut s);
        s
    }

    fn collect_text(&self, out: &mut String) {
        match self {
            Node::Text(t) => out.push_str(t),
            Node::CData(c) => out.push_str(c),
            Node::Element(e) => {
                for child in &e.children {
                    child.collect_text(out);
                }
            }
            _ => {}
        }
    }

    /// Replace every text node whose content equals `from` with `to`.
    ///
    /// Returns how many were replaced. Exact match only, on purpose: a
    /// "contains" or regex replace would silently modify text the user did not
    /// intend to change, which is data loss wearing a helpful hat.
    pub fn replace_text(&mut self, from: &str, to: &str) -> usize {
        match self {
            Node::Text(t) => {
                if t == from {
                    *t = to.to_string();
                    1
                } else {
                    0
                }
            }
            Node::Element(e) => e
                .children
                .iter_mut()
                .map(|c| c.replace_text(from, to))
                .sum(),
            _ => 0,
        }
    }

    /// Replace text matching a predicate, keeping the rest of the node intact.
    pub fn replace_text_where(&mut self, f: impl Fn(&str) -> bool, to: &str) -> usize {
        match self {
            Node::Text(t) => {
                if f(t) {
                    *t = to.to_string();
                    1
                } else {
                    0
                }
            }
            Node::Element(e) => e
                .children
                .iter_mut()
                .map(|c| c.replace_text_where(&f, to))
                .sum(),
            _ => 0,
        }
    }
}

/// An element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    /// The tag as written, including any namespace prefix: `w:body`.
    name: String,
    /// Attributes in source order, each with its original quoting.
    attributes: Vec<Attribute>,
    /// Child nodes in document order.
    pub children: Vec<Node>,
    /// The element's own start and end tags, verbatim.
    ///
    /// Stored because reconstructing them from `name` and `attributes` means
    /// re-deciding quoting, spacing and self-closing style — the exact
    /// normalisation this type exists to avoid.
    open_tag: String,
    close_tag: Option<String>,
}

/// One attribute, preserving its original quoting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    pub name: String,
    pub value: String,
    /// `'` or `"`, as written.
    pub quote: char,
}

impl Element {
    fn write_to(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self.open_tag.as_bytes());
        if self.children.is_empty() && self.close_tag.is_none() {
            return;
        }
        for child in &self.children {
            child.write_to(out);
        }
        if let Some(close) = &self.close_tag {
            out.extend_from_slice(close.as_bytes());
        }
    }

    /// Local name with any namespace prefix stripped.
    pub fn local_name(&self) -> &str {
        match self.name.split_once(':') {
            Some((_, local)) => local,
            None => &self.name,
        }
    }

    /// The tag as written, with any prefix.
    pub fn qualified_name(&self) -> &str {
        &self.name
    }

    pub fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }

    /// Attribute value by local name, ignoring any prefix.
    pub fn attr(&self, local_name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|a| {
                a.name
                    .rsplit_once(':')
                    .map_or(a.name.as_str(), |(_, local)| local)
                    == local_name
            })
            .map(|a| a.value.as_str())
    }
}

// ---------------------------------------------------------------------------
// Parser
//
// A hand-written scanner rather than a recursive-descent parser over a token
// stream, because the only goal is to reproduce bytes faithfully and the
// grammar that matters is "anything goes until the matching close tag".
// ---------------------------------------------------------------------------

struct Parser<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    fn err<T>(&self, message: impl Into<String>) -> Result<T, ParseError> {
        Err(ParseError {
            offset: self.pos,
            message: message.into(),
        })
    }

    fn peek(&self) -> Option<u8> {
        self.src.as_bytes().get(self.pos).copied()
    }

    fn starts_with(&self, s: &str) -> bool {
        self.src[self.pos..].starts_with(s)
    }

    fn find(&self, needle: &str) -> Option<usize> {
        self.src[self.pos..].find(needle).map(|i| i + self.pos)
    }

    /// Parse one root element plus any surrounding prolog, comments and
    /// whitespace.
    fn parse_document(mut self) -> Result<Node, ParseError> {
        let mut root: Option<Node> = None;
        let mut pending: Vec<Node> = Vec::new();

        while self.pos < self.src.len() {
            if self.starts_with("<?") {
                pending.push(Node::ProcessingInstruction(self.read_pi()?));
            } else if self.starts_with("<!--") {
                pending.push(Node::Comment(self.read_comment()?));
            } else if self.starts_with("<![CDATA[") {
                pending.push(Node::CData(self.read_cdata()?));
            } else if self.starts_with("<!") {
                // DOCTYPE and friends. Kept verbatim; not interpreted.
                pending.push(Node::ProcessingInstruction(self.read_declaration()?));
            } else if self.starts_with("</") {
                return self.err("closing tag with no matching opening tag");
            } else if self.peek() == Some(b'<') {
                if root.is_some() {
                    return self.err("more than one root element");
                }
                root = Some(Node::Element(self.read_element()?));
            } else {
                let text = self.read_text()?;
                if !text.trim().is_empty() && root.is_none() {
                    return self.err("character data before the root element");
                }
                pending.push(Node::Text(text));
            }
        }

        match root {
            Some(Node::Element(e)) => {
                // Attach everything that preceded or followed the root as
                // siblings of a synthetic holder, so serialization is exact.
                // A single-element document — which every OOXML part is — keeps
                // the root itself as the returned node, and `pending` is empty
                // in practice.
                if pending.is_empty() {
                    Ok(Node::Element(e))
                } else {
                    // Wrap so nothing is dropped. The wrapper never appears in
                    // output because serialize() writes children directly.
                    let mut wrapper = Element {
                        name: String::new(),
                        attributes: Vec::new(),
                        children: pending,
                        open_tag: String::new(),
                        close_tag: None,
                    };
                    wrapper.children.push(Node::Element(e));
                    Ok(Node::Element(wrapper))
                }
            }
            Some(other) => Ok(other),
            None => self.err("no root element"),
        }
    }

    fn read_pi(&mut self) -> Result<String, ParseError> {
        let start = self.pos + 2;
        match self.find("?>") {
            Some(end) => {
                let body = self.src[start..end].to_string();
                self.pos = end + 2;
                Ok(body)
            }
            None => self.err("unterminated processing instruction"),
        }
    }

    fn read_comment(&mut self) -> Result<String, ParseError> {
        let start = self.pos + 4;
        match self.find("-->") {
            Some(end) => {
                let body = self.src[start..end].to_string();
                self.pos = end + 3;
                Ok(body)
            }
            None => self.err("unterminated comment"),
        }
    }

    fn read_cdata(&mut self) -> Result<String, ParseError> {
        let start = self.pos + 9;
        match self.find("]]>") {
            Some(end) => {
                let body = self.src[start..end].to_string();
                self.pos = end + 3;
                Ok(body)
            }
            None => self.err("unterminated CDATA section"),
        }
    }

    fn read_declaration(&mut self) -> Result<String, ParseError> {
        // Scan for '>', honouring an internal subset in [ ... ] so a DOCTYPE
        // with an entity declaration is not cut short.
        let start = self.pos + 2;
        let mut i = start;
        let mut depth = 0usize;
        let bytes = self.src.as_bytes();
        while i < bytes.len() {
            match bytes[i] {
                b'[' => depth += 1,
                b']' => depth = depth.saturating_sub(1),
                b'>' if depth == 0 => {
                    let body = self.src[start..i].to_string();
                    self.pos = i + 1;
                    return Ok(body);
                }
                _ => {}
            }
            i += 1;
        }
        self.err("unterminated declaration")
    }

    /// Read text up to the next `<`, rejecting undefined entity references.
    fn read_text(&mut self) -> Result<String, ParseError> {
        let end = match self.src[self.pos..].find('<') {
            Some(i) => self.pos + i,
            None => self.src.len(),
        };
        let text = &self.src[self.pos..end];
        validate_entities(text, self.pos)?;
        self.pos = end;
        Ok(text.to_string())
    }

    fn read_element(&mut self) -> Result<Element, ParseError> {
        let tag_start = self.pos;

        // <name attrs>
        self.pos += 1; // '<'
        let name_start = self.pos;
        while let Some(b) = self.peek() {
            if b.is_ascii_whitespace() || b == b'>' || b == b'/' {
                break;
            }
            self.pos += 1;
        }
        let name = self.src[name_start..self.pos].to_string();
        if name.is_empty() {
            return self.err("element with no name");
        }

        let mut attributes = Vec::new();
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(b'>') => {
                    self.pos += 1;
                    break;
                }
                Some(b'/') => {
                    // Self-closing.
                    if self.src[self.pos..].starts_with("/>") {
                        let open = self.src[tag_start..self.pos + 2].to_string();
                        self.pos += 2;
                        return Ok(Element {
                            name,
                            attributes,
                            children: Vec::new(),
                            open_tag: open,
                            close_tag: None,
                        });
                    }
                    return self.err("stray '/' in element");
                }
                Some(_) => {
                    let attr = self.read_attribute()?;
                    attributes.push(attr);
                }
                None => return self.err("unterminated start tag"),
            }
        }

        let open_tag = self.src[tag_start..self.pos].to_string();
        let mut children = Vec::new();

        loop {
            if self.pos >= self.src.len() {
                return Err(ParseError {
                    offset: tag_start,
                    message: format!("element <{name}> is never closed"),
                });
            }

            if self.starts_with("</") {
                let close_start = self.pos;
                let close_end = match self.find(">") {
                    Some(i) => i,
                    None => return self.err("unterminated end tag"),
                };
                let close_tag = self.src[close_start..=close_end].to_string();
                let closing_name = self.src[close_start + 2..close_end].trim();
                if closing_name != name {
                    return Err(ParseError {
                        offset: close_start,
                        message: format!("<{name}> closed by </{closing_name}>"),
                    });
                }
                self.pos = close_end + 1;
                return Ok(Element {
                    name,
                    attributes,
                    children,
                    open_tag,
                    close_tag: Some(close_tag),
                });
            }

            if self.starts_with("<!--") {
                children.push(Node::Comment(self.read_comment()?));
            } else if self.starts_with("<![CDATA[") {
                children.push(Node::CData(self.read_cdata()?));
            } else if self.starts_with("<?") {
                children.push(Node::ProcessingInstruction(self.read_pi()?));
            } else if self.starts_with("<!") {
                children.push(Node::ProcessingInstruction(self.read_declaration()?));
            } else if self.starts_with("<") {
                children.push(Node::Element(self.read_element()?));
            } else {
                children.push(Node::Text(self.read_text()?));
            }
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(b) = self.peek() {
            if b.is_ascii_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn read_attribute(&mut self) -> Result<Attribute, ParseError> {
        let name_start = self.pos;
        while let Some(b) = self.peek() {
            if b.is_ascii_whitespace() || b == b'=' || b == b'>' || b == b'/' {
                break;
            }
            self.pos += 1;
        }
        let name = self.src[name_start..self.pos].to_string();
        if name.is_empty() {
            return self.err("attribute with no name");
        }

        self.skip_whitespace();
        if self.peek() != Some(b'=') {
            return self.err(format!("attribute {name} has no value"));
        }
        self.pos += 1;
        self.skip_whitespace();

        let quote = match self.peek() {
            Some(q @ (b'"' | b'\'')) => {
                self.pos += 1;
                q as char
            }
            _ => return self.err(format!("attribute {name} value is not quoted")),
        };

        let value_start = self.pos;
        // `str::find` takes a `char` pattern, so use the character rather than
        // the byte. Both quote characters are ASCII, so the offset is identical.
        let end = match self.src[self.pos..].find(quote) {
            Some(i) => self.pos + i,
            None => return self.err(format!("unterminated value for attribute {name}")),
        };
        let value = self.src[value_start..end].to_string();
        self.pos = end + 1;

        // Entities are decoded for *reading* but the raw text is what gets
        // serialized, because `open_tag` holds the original bytes.
        Ok(Attribute {
            name,
            value: decode_entities(&value),
            quote,
        })
    }
}

/// Reject undefined entity references in character data.
///
/// `base` is the document offset of `text`, so the reported position is absolute
/// rather than relative to the text run — otherwise an error in a long document
/// points somewhere no user can act on.
///
/// Only the five predefined entities and numeric character references are
/// accepted. Anything else needs a DTD declaration, and OOXML has no DTDs; a
/// reference that is not predefined is therefore undefined by definition.
fn validate_entities(text: &str, base: usize) -> Result<(), ParseError> {
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        let after = &rest[i + 1..];
        let Some(end) = after.find(';') else {
            // A bare '&' with no ';' anywhere after it. Some XML writers emit
            // this for a literal ampersand; it is still malformed, and guessing
            // is what this function exists to prevent.
            return Err(ParseError {
                offset: base + i,
                message: "unescaped '&' in character data".into(),
            });
        };
        let name = &after[..end];
        let ok = matches!(name, "amp" | "lt" | "gt" | "quot" | "apos")
            || (name.starts_with("#x") || name.starts_with("#X"))
                && u32::from_str_radix(&name[2..], 16).is_ok()
            || name.starts_with('#') && name[1..].parse::<u32>().is_ok();
        if !ok {
            return Err(ParseError {
                offset: base + i,
                message: format!("undefined entity reference &{name};"),
            });
        }
        rest = &after[end + 1..];
    }
    Ok(())
}

/// Decode the five predefined entities plus numeric character references.
///
/// Applied only to attribute values exposed through the API. Serialization uses
/// the untouched source text, so decoding here costs nothing in fidelity.
fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(end) = tail.find(';') else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let entity = &tail[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            e if e.starts_with("#x") || e.starts_with("#X") => u32::from_str_radix(&e[2..], 16)
                .ok()
                .and_then(char::from_u32),
            e if e.starts_with('#') => e[1..].parse::<u32>().ok().and_then(char::from_u32),
            _ => None,
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &tail[end + 1..];
            }
            None => {
                // Undefined entity. Preserved literally; not an error, because
                // OOXML in the wild contains these and dropping the reference
                // would change the document.
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}
