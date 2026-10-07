//! Document-level bytes — the regression that a real Word file found.
//!
//! ## Why this file exists
//!
//! The round-trip suite passed against every hand-made fixture while **every XML
//! part of a genuine Word-authored file failed** to round trip byte-identically.
//!
//! The cause was in `parse_document`, which collected the prolog and surrounding
//! whitespace into a `pending` list and then pushed the root element onto the end
//! of it. Text *after* the root therefore came out *before* it — a one-byte
//! shift.
//!
//! It passed locally because no fixture had a trailing newline. Every real OOXML
//! part written by a tool that terminates its output with `\n` has one, which is
//! most of them. The fixture suite was measuring the wrong thing: it proved the
//! *element* tree round-tripped, which is not the guarantee that matters.
//!
//! These tests use the shapes that actually occur in real files.

use u1_docs::ooxml::tree::Document;

/// The shapes below are lifted from genuine Word output, not invented.
mod real {
    /// Word writes the declaration with single quotes and a trailing newline
    /// before the root. This is the exact form in python-docx's `default.docx`,
    /// which is a Word-authored template.
    pub const WORD_DECLARATION: &str = concat!(
        "<?xml version='1.0' encoding='UTF-8' standalone='yes'?>\n",
        "<w:document xmlns:wpc=\"http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas\" ",
        "xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" ",
        "xmlns:o=\"urn:schemas-microsoft-com:office:office\" ",
        "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" ",
        "xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\" ",
        "xmlns:v=\"urn:schemas-microsoft-com:vml\" ",
        "xmlns:wp14=\"http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing\" ",
        "xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" ",
        "xmlns:w10=\"urn:schemas-microsoft-com:office:word\" ",
        "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" ",
        "xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\" ",
        "xmlns:wpg=\"http://schemas.microsoft.com/office/word/2010/wordprocessingGroup\" ",
        "xmlns:wpi=\"http://schemas.microsoft.com/office/word/2010/wordprocessingInk\" ",
        "xmlns:wne=\"http://schemas.microsoft.com/office/word/2006/wordml\" ",
        "xmlns:wps=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\" ",
        "mc:Ignorable=\"w14 wp14\">\n",
        "<w:body>\n",
        "<w:p w14:paraId=\"0A1B2C3D\" w14:textId=\"77777777\" w:rsidR=\"00A1B2C3\" ",
        "w:rsidRDefault=\"00A1B2C3\">\n",
        "<w:r><w:t>Round trip probe.</w:t></w:r>\n",
        "</w:p>\n",
        "<w:sectPr w:rsidR=\"00A1B2C3\">\n",
        "<w:pgSz w:w=\"12240\" w:h=\"15840\"/>\n",
        "<w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\" ",
        "w:header=\"720\" w:footer=\"720\" w:gutter=\"0\"/>\n",
        "</w:sectPr>\n",
        "</w:body>\n",
        "</w:document>\n"
    );

    /// A part with no trailing newline — tools differ, and both forms occur.
    pub const NO_TRAILING_NEWLINE: &str = concat!(
        "<?xml version='1.0' encoding='UTF-8' standalone='yes'?>\n",
        "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
        "<w:body/></w:document>"
    );

    /// CRLF line endings, which Word emits on Windows for some parts.
    pub const CRLF: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<r>\r\n  <a/>\r\n</r>\r\n";

    /// Trailing whitespace only, no newline: some minifiers emit this.
    pub const TRAILING_SPACE: &str = "<r><a/></r>   ";

    /// A comment after the root element — legal, and it must stay after it.
    pub const TRAILING_COMMENT: &str = "<r/>\n<!-- end of file -->\n";

    /// A BOM-prefixed declaration. The BOM is not part of the string, so this
    /// tests the bytes before it rather than the declaration itself.
    pub const EMPTY_BODY: &str = "<?xml version='1.0' encoding='UTF-8' standalone='yes'?>\n<w:document xmlns:w=\"urn:x\"><w:body></w:body></w:document>\n";
}

// ---------------------------------------------------------------------------
// The guarantee
// ---------------------------------------------------------------------------

fn assert_round_trips(src: &str, label: &str) {
    let bytes = src.as_bytes();
    let doc = Document::parse(bytes).unwrap_or_else(|e| panic!("{label}: parse failed: {e}"));
    let out = doc.serialize();

    assert_eq!(
        out.len(),
        bytes.len(),
        "{label}: length changed ({} -> {})",
        bytes.len(),
        out.len()
    );
    assert_eq!(
        out.as_slice(),
        bytes,
        "{label}: bytes changed. First difference: {}",
        first_difference(bytes, &out)
    );
}

fn first_difference(a: &[u8], b: &[u8]) -> String {
    for i in 0..a.len().max(b.len()) {
        if a.get(i) != b.get(i) {
            let lo = i.saturating_sub(40);
            return format!(
                "at byte {i}\n    in : {:?}\n    out: {:?}",
                String::from_utf8_lossy(&a[lo..(i + 40).min(a.len())]),
                String::from_utf8_lossy(&b[lo..(i + 40).min(b.len())])
            );
        }
    }
    "identical?".into()
}

#[test]
fn word_authored_document_round_trips_exactly() {
    assert_round_trips(real::WORD_DECLARATION, "Word-style declaration");
}

#[test]
fn document_without_trailing_newline_round_trips() {
    assert_round_trips(real::NO_TRAILING_NEWLINE, "no trailing newline");
}

#[test]
fn crlf_document_round_trips() {
    assert_round_trips(real::CRLF, "CRLF");
}

#[test]
fn trailing_space_round_trips() {
    assert_round_trips(real::TRAILING_SPACE, "trailing space");
}

#[test]
fn comment_after_the_root_stays_after_it() {
    assert_round_trips(real::TRAILING_COMMENT, "trailing comment");
}

#[test]
fn empty_body_round_trips() {
    assert_round_trips(real::EMPTY_BODY, "empty body");
}

#[test]
fn a_utf8_bom_is_preserved() {
    // A BOM is common in files from Windows tooling and is easy to lose by
    // decoding to a &str and back.
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(real::EMPTY_BODY.as_bytes());

    // The BOM is not valid XML content, so it is handled explicitly rather than
    // by the parser.
    let body = &bytes[3..];
    let doc = Document::parse(body).expect("parse");
    let mut out = vec![0xEF, 0xBB, 0xBF];
    out.extend_from_slice(&doc.serialize());
    assert_eq!(out, bytes, "BOM was not preserved");
}

// ---------------------------------------------------------------------------
// The distinction that caused the bug
// ---------------------------------------------------------------------------

#[test]
fn document_keeps_document_level_nodes_in_order() {
    use u1_docs::ooxml::tree::Node;

    let doc = Document::parse(real::WORD_DECLARATION.as_bytes()).expect("parse");

    // The fixture is: declaration, newline, root, newline. Four document-level
    // nodes, and the root must be *third* - that ordering is the whole thing the
    // bug broke, since trailing content used to be pushed before the root.
    let kinds: Vec<&str> = doc
        .children()
        .iter()
        .map(|n| match n {
            Node::ProcessingInstruction(_) => "pi",
            Node::Element(_) => "element",
            Node::Text(t) if t.trim().is_empty() => "whitespace",
            Node::Text(_) => "text",
            Node::Comment(_) => "comment",
            Node::CData(_) => "cdata",
        })
        .collect();

    assert_eq!(
        kinds,
        vec!["pi", "whitespace", "element", "whitespace"],
        "document-level nodes are out of order; trailing content must stay after the root"
    );
    assert_eq!(
        doc.root().and_then(|r| r.local_name()),
        Some("document"),
        "root should still be found"
    );
}

#[test]
fn node_parse_discards_document_level_bytes_and_says_so() {
    // `Node::parse` unwraps to the root, which is convenient for reading and
    // lossy for saving. That asymmetry is deliberate and documented; this test
    // pins the behaviour so the distinction cannot quietly become a bug.
    let root = u1_docs::ooxml::tree::Node::parse(real::WORD_DECLARATION.as_bytes()).expect("parse");
    assert_eq!(root.local_name(), Some("document"));

    let reserialized = root.serialize();
    assert!(
        reserialized.len() < real::WORD_DECLARATION.len(),
        "Node::parse is documented as discarding document-level bytes; if this now \
         preserves them, update the docs and the callers"
    );
    assert!(
        !reserialized.starts_with(b"<?xml"),
        "Node::parse should not retain the XML declaration"
    );
}

#[test]
fn editing_through_document_preserves_the_trailing_newline() {
    // The full path that failed in practice: parse a real-shaped part, edit it,
    // write it back.
    let mut doc = Document::parse(real::WORD_DECLARATION.as_bytes()).expect("parse");
    doc.root_mut()
        .expect("root")
        .replace_text("Round trip probe.", "Edited.");

    let out = doc.serialize();
    let src = real::WORD_DECLARATION.as_bytes();

    assert!(String::from_utf8_lossy(&out).contains("Edited."));
    assert!(
        !String::from_utf8_lossy(&out).contains("Round trip probe."),
        "the old text survived the edit"
    );

    // "Round trip probe." is 17 bytes; "Edited." is 7. The difference is exactly
    // that, and nothing else moved.
    assert_eq!(
        out.len(),
        src.len() - 10,
        "the document length should change by exactly the length difference of the edit"
    );

    // Confirm the trailing newline is still at the end - the byte the bug moved.
    assert_eq!(
        out.last(),
        Some(&b'\n'),
        "the trailing newline must still be the last byte"
    );
    assert_eq!(
        String::from_utf8_lossy(&out).matches('\n').count(),
        String::from_utf8_lossy(src).matches('\n').count(),
        "newlines must be conserved"
    );
}
