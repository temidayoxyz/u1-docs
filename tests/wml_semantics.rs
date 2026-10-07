//! WordprocessingML semantics: what text a paragraph actually contains.
//!
//! ## The problem this file exists for
//!
//! Reading a paragraph's text looks trivial and is not. `w:p` does not contain
//! runs directly — runs can be nested arbitrarily deep inside wrappers that have
//! no text of their own, and a few sibling elements contribute characters that
//! are not in a `w:t` at all. A naive `w:p > w:r > w:t` walk gets the common
//! case right and is wrong everywhere else.
//!
//! Three things make this genuinely hard, and all three occur in real files:
//!
//! 1. **Runs are nested, not direct children.** A hyperlink, a content control
//!    (`w:sdt`), a smart tag, an insertion or deletion under tracked changes, and
//!    a bookmark all sit between the paragraph and its runs. Walking only direct
//!    children silently drops their text.
//!
//! 2. **Not every character lives in a `w:t`.** `w:tab`, `w:br`, `w:cr`,
//!    `w:noBreakHyphen` and `w:softHyphen` are empty elements that each mean a
//!    specific character. Treating them as structural and dropping them loses
//!    user-visible content.
//!
//! 3. **Some `w:t`-shaped text is not document text.** `w:instrText` holds the
//!    *instruction* of a field (`HYPERLINK "http://..."`) and must never be shown
//!    to a reader. `w:delText` is text inside a tracked deletion. Including either
//!    puts machine-readable noise into the visible document.
//!
//! ## On the fixtures
//!
//! These are hand-built, and `tests/fixtures/mod.rs` explains why that is a floor
//! rather than a ceiling. The shapes here are taken from the spec and from files
//! Word emits, and `tests/real_documents.rs` exists to check the same properties
//! against genuine output.

use u1_docs::ooxml::tree::{Document, Node};
use u1_docs::ooxml::wml::{Paragraph, TextOptions};

/// Build a `<w:body>` document from fragment markup and return the body.
fn body(inner: &str) -> Node {
    let xml = format!(
        concat!(
            "<?xml version='1.0' encoding='UTF-8' standalone='yes'?>\n",
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#,
            "<w:body>{inner}</w:body></w:document>\n"
        ),
        inner = inner
    );
    let doc = Document::parse(xml.as_bytes()).expect("fixture parses");
    let root = doc.root().expect("root").clone();
    root.find("body").cloned().expect("body")
}

/// Every paragraph in the fragment, in order.
fn paragraphs(inner: &str) -> Vec<Paragraph> {
    let body = body(inner);
    let mut out = Vec::new();
    for node in body.children_elements() {
        if node.local_name() == Some("p") {
            out.push(Paragraph::from_element(node).expect("paragraph"));
        }
    }
    out
}

/// The visible text of the first paragraph.
fn text_of(inner: &str) -> String {
    paragraphs(inner)
        .into_iter()
        .next()
        .expect("at least one paragraph")
        .text(TextOptions::default())
}

// ---------------------------------------------------------------------------
// The easy case, pinned so it cannot regress
// ---------------------------------------------------------------------------

#[test]
fn plain_run_text() {
    assert_eq!(text_of("<w:p><w:r><w:t>Hello</w:t></w:r></w:p>"), "Hello");
}

#[test]
fn adjacent_runs_concatenate() {
    assert_eq!(
        text_of("<w:p><w:r><w:t>Hello</w:t></w:r><w:r><w:t> world</w:t></w:r></w:p>"),
        "Hello world"
    );
}

#[test]
fn empty_paragraph_is_empty() {
    assert_eq!(text_of("<w:p/>"), "");
    assert_eq!(text_of("<w:p></w:p>"), "");
}

#[test]
fn paragraph_with_properties_only_is_empty() {
    // w:pPr carries formatting, never text. Walking direct children and
    // concatenating text_content() naively would pick up nothing here, but a
    // walker that descends into everything must not pick up pPr either.
    assert_eq!(
        text_of(r#"<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>"#),
        "x"
    );
}

// ---------------------------------------------------------------------------
// Nesting: the case a direct-children walk gets wrong
// ---------------------------------------------------------------------------

#[test]
fn hyperlink_text_is_included() {
    // The single most common real-world shape. A direct-children walk drops this.
    assert_eq!(
        text_of(
            r#"<w:p><w:hyperlink r:id="rId5"><w:r><w:t>click me</w:t></w:r></w:hyperlink></w:p>"#
        ),
        "click me"
    );
}

#[test]
fn hyperlink_with_a_run_outside_it_keeps_document_order() {
    assert_eq!(
        text_of(
            r#"<w:p><w:r><w:t>see </w:t></w:r><w:hyperlink><w:r><w:t>here</w:t></w:r></w:hyperlink><w:r><w:t> now</w:t></w:r></w:p>"#
        ),
        "see here now"
    );
}

#[test]
fn content_control_text_is_included() {
    assert_eq!(
        text_of(concat!(
            "<w:p><w:sdt><w:sdtPr><w:alias w:val=\"Title\"/></w:sdtPr>",
            "<w:sdtContent><w:r><w:t>content</w:t></w:r></w:sdtContent></w:sdt></w:p>"
        )),
        "content"
    );
}

#[test]
fn smart_tag_text_is_included() {
    assert_eq!(
        text_of(concat!(
            "<w:p><w:smartTag w:uri=\"urn:x\"><w:smartTagPr/>",
            "<w:r><w:t>tagged</w:t></w:r></w:smartTag></w:p>"
        )),
        "tagged"
    );
}

#[test]
fn bookmarked_text_is_included_and_bookmarks_contribute_nothing() {
    assert_eq!(
        text_of(concat!(
            "<w:p><w:bookmarkStart w:id=\"0\" w:name=\"_GoBack\"/>",
            "<w:r><w:t>anchored</w:t></w:r>",
            "<w:bookmarkEnd w:id=\"0\"/></w:p>"
        )),
        "anchored"
    );
}

#[test]
fn deeply_nested_wrappers_are_still_reached() {
    // Every layer here occurs in real files, and three deep is not unusual.
    assert_eq!(
        text_of(concat!(
            "<w:p><w:ins w:id=\"1\"><w:smartTag w:uri=\"u\">",
            "<w:sdt><w:sdtContent><w:hyperlink><w:r><w:t>deep</w:t></w:r>",
            "</w:hyperlink></w:sdtContent></w:sdt></w:smartTag></w:ins></w:p>"
        )),
        "deep"
    );
}

// ---------------------------------------------------------------------------
// Characters that are not in a w:t
// ---------------------------------------------------------------------------

#[test]
fn tab_becomes_a_tab_character() {
    assert_eq!(
        text_of("<w:p><w:r><w:t>a</w:t><w:tab/><w:t>b</w:t></w:r></w:p>"),
        "a\tb"
    );
}

#[test]
fn line_break_and_carriage_return_become_newlines() {
    assert_eq!(
        text_of("<w:p><w:r><w:t>a</w:t><w:br/><w:t>b</w:t><w:cr/><w:t>c</w:t></w:r></w:p>"),
        "a\nb\nc"
    );
}

#[test]
fn page_break_is_a_page_break_not_a_newline() {
    // <w:br w:type="page"/> is pagination. Rendering it as a newline would be
    // wrong at the text level; it belongs to layout, not to the character
    // stream. The paragraph's text is unchanged by it.
    assert_eq!(
        text_of(r#"<w:p><w:r><w:t>a</w:t><w:br w:type="page"/><w:t>b</w:t></w:r></w:p>"#),
        "ab"
    );
}

#[test]
fn no_break_hyphen_becomes_a_non_breaking_hyphen() {
    assert_eq!(
        text_of("<w:p><w:r><w:t>anti</w:t><w:noBreakHyphen/><w:t>clockwise</w:t></w:r></w:p>"),
        "anti\u{2011}clockwise"
    );
}

#[test]
fn soft_hyphen_is_retained_as_its_character() {
    // U+00AD is invisible but is real content: copying the paragraph should
    // carry it. Whether it *renders* is a layout decision, not a text one.
    assert_eq!(
        text_of("<w:p><w:r><w:t>anti</w:t><w:softHyphen/><w:t>social</w:t></w:r></w:p>"),
        "anti\u{00AD}social"
    );
}

#[test]
fn a_symbol_is_reported_but_not_guessed() {
    // <w:sym w:font="Wingdings" w:char="F0E0"/> is a private-use codepoint whose
    // real character depends on a font we may not have. Emitting U+F0E0 as text
    // would be a guess that renders as tofu. The text is replaced by a
    // placeholder, and the caller can inspect runs for symbol content.
    let p = paragraphs(
        "<w:p><w:r><w:t>x</w:t><w:sym w:font=\"Wingdings\" w:char=\"F0E0\"/></w:r></w:p>",
    )
    .remove(0);
    let text = p.text(TextOptions::default());
    assert!(
        text.starts_with('x'),
        "text before the symbol must survive, got {text:?}"
    );
    assert!(
        !text.contains('\u{F0E0}'),
        "a private-use codepoint must not be emitted as text, got {text:?}"
    );
}

// ---------------------------------------------------------------------------
// Text that is not document text
// ---------------------------------------------------------------------------

#[test]
fn field_instruction_text_is_not_document_text() {
    // The visible result of a field is the run(s) after w:fldChar separate; the
    // instruction is machinery. Showing it would put "HYPERLINK http://..." in
    // the user's document.
    assert_eq!(
        text_of(concat!(
            "<w:p>",
            "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r>",
            "<w:r><w:instrText> HYPERLINK \"http://example.com\" </w:instrText></w:r>",
            "<w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>",
            "<w:r><w:t>Example</w:t></w:r>",
            "<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
            "</w:p>"
        )),
        "Example"
    );
}

#[test]
fn field_char_elements_contribute_no_text() {
    assert_eq!(
        text_of("<w:p><w:r><w:fldChar w:fldCharType=\"begin\"/><w:t>only this</w:t></w:r></w:p>"),
        "only this"
    );
}

#[test]
fn deleted_text_is_excluded_by_default() {
    assert_eq!(
        text_of(concat!(
            "<w:p>",
            "<w:r><w:t>keep</w:t></w:r>",
            "<w:del w:id=\"1\" w:author=\"a\" w:date=\"2020-01-01T00:00:00Z\">",
            "<w:r><w:delText>remove</w:delText></w:r></w:del>",
            "</w:p>"
        )),
        "keep"
    );
}

#[test]
fn deleted_text_can_be_included_on_request() {
    assert_eq!(
        text_of_opt(
            concat!(
                "<w:p><w:r><w:t>keep</w:t></w:r>",
                "<w:del w:id=\"1\"><w:r><w:delText>remove</w:delText></w:r></w:del></w:p>"
            ),
            TextOptions {
                include_deletions: true,
                ..TextOptions::default()
            }
        ),
        "keepremove"
    );
}

/// Text of the first paragraph with non-default options.
fn text_of_opt(inner: &str, opts: TextOptions) -> String {
    paragraphs(inner)
        .into_iter()
        .next()
        .expect("at least one paragraph")
        .text(opts)
}

#[test]
fn inserted_text_is_included() {
    // An insertion under tracked changes is text the author added and intends to
    // keep. Excluding it would be wrong.
    assert_eq!(
        text_of(concat!(
            "<w:p><w:r><w:t>a</w:t></w:r>",
            "<w:ins w:id=\"1\"><w:r><w:t>b</w:t></w:r></w:ins>",
            "<w:r><w:t>c</w:t></w:r></w:p>"
        )),
        "abc"
    );
}

#[test]
fn deleted_run_with_plain_w_t_is_still_excluded() {
    // w:delText is the usual spelling, but a w:del can contain a normal w:t too,
    // depending on which tool wrote the file. Both mean "deleted".
    assert_eq!(
        text_of("<w:p><w:del w:id=\"1\"><w:r><w:t>gone</w:t></w:r></w:del></w:p>"),
        ""
    );
}

// ---------------------------------------------------------------------------
// Whitespace
// ---------------------------------------------------------------------------

#[test]
fn xml_space_preserve_keeps_significant_whitespace() {
    assert_eq!(
        text_of(r#"<w:p><w:r><w:t xml:space="preserve">  spaced  </w:t></w:r></w:p>"#),
        "  spaced  "
    );
}

#[test]
fn without_preserve_whitespace_at_the_edges_is_normalised() {
    // Word collapses runs of whitespace and trims edges unless xml:space is set.
    // This is the reader's job, not the parser's: the bytes stay untouched.
    assert_eq!(
        text_of("<w:p><w:r><w:t>   padded   </w:t></w:r></w:p>"),
        "padded"
    );
}

#[test]
fn interior_whitespace_is_collapsed_to_single_spaces() {
    assert_eq!(text_of("<w:p><w:r><w:t>a \n\t  b</w:t></w:r></w:p>"), "a b");
}

#[test]
fn a_non_breaking_space_is_not_whitespace_for_this_purpose() {
    // U+00A0 is visible ink. Collapsing it would corrupt French and other text.
    assert_eq!(
        text_of("<w:p><w:r><w:t>a\u{00A0}b</w:t></w:r></w:p>"),
        "a\u{00A0}b"
    );
}

// ---------------------------------------------------------------------------
// Runs
// ---------------------------------------------------------------------------

#[test]
fn runs_are_reported_separately_in_document_order() {
    let p = paragraphs(
        r#"<w:p><w:r><w:t>one</w:t></w:r><w:hyperlink><w:r><w:t>two</w:t></w:r></w:hyperlink><w:r><w:t>three</w:t></w:r></w:p>"#,
    )
    .remove(0);

    let texts: Vec<String> = p.runs().map(|r| r.text.clone()).collect();
    assert_eq!(texts, vec!["one", "two", "three"]);
}

#[test]
fn run_properties_do_not_leak_into_text() {
    let runs: Vec<String> = paragraphs(concat!(
        "<w:p><w:r><w:rPr><w:b/><w:i/><w:sz w:val=\"24\"/></w:rPr>",
        "<w:t>bold</w:t></w:r></w:p>"
    ))
    .remove(0)
    .runs()
    .map(|r| r.text.clone())
    .collect();

    assert_eq!(runs, vec!["bold"]);
}

#[test]
fn a_run_with_no_text_element_yields_an_empty_run() {
    // Runs carrying only a break or a tab still exist and still matter for
    // formatting; they must not vanish from the run list.
    let runs: Vec<String> = paragraphs("<w:p><w:r><w:tab/></w:r><w:r><w:t>x</w:t></w:r></w:p>")
        .remove(0)
        .runs()
        .map(|r| r.text.clone())
        .collect();

    assert_eq!(runs, vec!["\t", "x"]);
}

#[test]
fn deleted_runs_are_excluded_from_the_run_list_but_includable() {
    let xml = concat!(
        "<w:p><w:r><w:t>live</w:t></w:r>",
        "<w:del w:id=\"1\"><w:r><w:delText>dead</w:delText></w:r></w:del></w:p>"
    );

    let live: Vec<String> = paragraphs(xml)
        .remove(0)
        .runs()
        .map(|r| r.text.clone())
        .collect();
    assert_eq!(live, vec!["live"]);

    let all: Vec<String> = paragraphs(xml)
        .remove(0)
        .runs_including_deletions()
        .map(|r| r.text.clone())
        .collect();
    assert_eq!(all, vec!["live", "dead"]);
}

// ---------------------------------------------------------------------------
// What a paragraph is not
// ---------------------------------------------------------------------------

#[test]
fn a_table_inside_a_body_is_not_a_paragraph() {
    // w:body contains w:tbl as well as w:p. Treating every body child as a
    // paragraph would invent empty paragraphs between table rows.
    let body = body(concat!(
        "<w:p><w:r><w:t>above</w:t></w:r></w:p>",
        "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
        "<w:p><w:r><w:t>below</w:t></w:r></w:p>"
    ));

    let top: Vec<String> = body
        .children_elements()
        .filter(|n| n.local_name() == Some("p"))
        .map(|n| {
            Paragraph::from_element(n)
                .unwrap()
                .text(TextOptions::default())
        })
        .collect();

    assert_eq!(top, vec!["above", "below"]);
}

#[test]
fn a_paragraph_is_never_its_own_nested_paragraph() {
    // Text boxes nest a whole w:p inside a run. A walker that descends
    // indiscriminately would merge the inner paragraph's text into the outer one.
    let inner = text_of(concat!(
        "<w:p><w:r><w:t>outer</w:t></w:r>",
        "<w:p><w:r><w:t>inner</w:t></w:r></w:p>",
        "</w:p>"
    ));
    assert_eq!(inner, "outer");
}

#[test]
fn text_inside_a_nested_paragraph_is_reachable_on_its_own() {
    // ...but it is not lost; it is a separate paragraph that document traversal
    // will find.
    let body = body(concat!(
        "<w:p><w:r><w:t>outer</w:t></w:r>",
        "<w:p><w:r><w:t>inner</w:t></w:r></w:p></w:p>"
    ));

    let all: Vec<String> = body
        .descendant_elements()
        .filter(|n| n.local_name() == Some("p"))
        .map(|n| {
            Paragraph::from_element(n)
                .unwrap()
                .text(TextOptions::default())
        })
        .collect();

    assert_eq!(all, vec!["outer", "inner"]);
}

// ---------------------------------------------------------------------------
// Non-WordprocessingML namespaces must not be read as text
// ---------------------------------------------------------------------------

#[test]
fn foreign_namespace_elements_are_not_walked() {
    // mc:AlternateContent is how Word stores a feature with a fallback. Its
    // Choice branch is often a DrawingML tree full of elements whose local names
    // collide with nothing useful but whose character data is not document text.
    // Walking it produces garbage.
    let inner = concat!(
        r#"<w:p><w:r><w:t>visible</w:t></w:r>"#,
        r#"<mc:AlternateContent>"#,
        r#"<mc:Choice Requires="wps"><w:drawing><wp:docPr descr="alt text"/></w:drawing></mc:Choice>"#,
        r#"<mc:Fallback><w:pict><v:shape/></w:pict></mc:Fallback>"#,
        r#"</mc:AlternateContent></w:p>"#
    );

    let p = Paragraph::from_element(body(inner).children_elements().next().unwrap())
        .expect("paragraph");
    assert_eq!(p.text(TextOptions::default()), "visible");
}

// ---------------------------------------------------------------------------
// Robustness
// ---------------------------------------------------------------------------

#[test]
fn an_unknown_element_does_not_break_text_extraction() {
    // Word files carry elements from extensions we have never heard of. The
    // correct behaviour is to ignore what we do not understand, not to fail.
    assert_eq!(
        text_of(concat!(
            "<w:p><w:r><w:t>a</w:t></w:r>",
            "<w14:someFutureThing w14:attr=\"1\"><w14:inner>x</w14:inner></w14:someFutureThing>",
            "<w:r><w:t>b</w:t></w:r></w:p>"
        )),
        "ab"
    );
}

#[test]
fn property_elements_never_contribute_text() {
    // The full set of property containers. Each of these can hold arbitrary
    // children in a schema extension, and none of them is content.
    let inner = concat!(
        "<w:p><w:pPr><w:rPr><w:t>in rPr</w:t></w:rPr></w:pPr>",
        "<w:r><w:rPr><w:del w:author=\"a\"><w:r><w:t>in del</w:t></w:r></w:del></w:rPr>",
        "<w:t>kept</w:t></w:r></w:p>"
    );
    assert_eq!(text_of(inner), "kept");
}

#[test]
fn text_of_a_paragraph_with_no_runs_at_all_is_empty_not_an_error() {
    assert_eq!(
        text_of("<w:p><w:pPr><w:jc w:val=\"both\"/></w:pPr></w:p>"),
        ""
    );
}
