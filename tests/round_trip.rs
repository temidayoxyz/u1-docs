//! Round-trip tests — the most important suite in U1 Docs.
//!
//! See ADR-0005. The assertion is not "we read the text" or "we can write
//! valid XML". It is:
//!
//! > open(x) -> save -> open again, and the second document is semantically
//! > identical to the first, with no lost content.
//!
//! ## Written before the code, on purpose
//!
//! This file compiles and fails against an empty implementation. A round-trip
//! test written after the reader exists tends to assert whatever the reader
//! happens to do, which is how a lossy implementation gets certified as correct.
//! Written first, it states the requirement independently.
//!
//! ## What "lossless" means concretely
//!
//! Not "we preserve the parts we understand". Specifically:
//!
//! 1. Every part in the package is present after a save, with the same name.
//! 2. Parts we did not touch are **byte-identical**, not merely equivalent.
//! 3. XML elements we have no model for survive verbatim.
//! 4. Namespaces, attribute order and text content are not normalised away.
//!
//! Point 2 is the one that is easy to get wrong. Re-serialising an XML tree
//! "canonically" changes bytes without changing meaning — and the moment a
//! user's Word installation sees something it does not recognise, it silently
//! repairs the document on their next save. Byte-identity is the safe subset.

use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// The API these tests require. It does not exist yet; that is deliberate.
// ---------------------------------------------------------------------------

use u1_docs::ooxml::opc::Package;
use u1_docs::ooxml::tree::Node;

mod fixtures;

/// Every part in a package, keyed by name, with its raw bytes.
fn snapshot(pkg: &Package) -> BTreeMap<String, Vec<u8>> {
    pkg.parts()
        .iter()
        .map(|p| (p.name().to_string(), p.bytes().to_vec()))
        .collect()
}

/// Open, save to a temporary path, reopen.
fn round_trip(path: &std::path::Path, tmp: &std::path::Path) -> Package {
    let pkg = Package::open(path).expect("open should succeed");
    pkg.save(tmp).expect("save should succeed");
    Package::open(tmp).expect("reopen should succeed")
}

// ---------------------------------------------------------------------------
// 1. Open a real package
// ---------------------------------------------------------------------------

#[test]
fn opens_a_docx_and_finds_the_main_document() {
    let f = fixtures::minimal_docx();
    let pkg = Package::open(&f.path).expect("minimal.docx must open");

    // The main document is not at a fixed path; it is discovered through the
    // relationship graph. Getting this wrong is a classic bug: hard-coding
    // "word/document.xml" works for files Word wrote and nothing else.
    let main = pkg
        .main_document_part()
        .expect("package must resolve a main document");
    assert!(
        main.starts_with("word/"),
        "main document should live under word/, got {main}"
    );
    assert!(
        main.ends_with(".xml"),
        "main document must be XML, got {main}"
    );
}

#[test]
fn opens_a_package_with_no_main_document() {
    // A package can legitimately have none — a template, or a file that is not
    // really a document. This must be a clear error, not a panic and not a
    // silent empty document.
    let f = fixtures::docx_without_main_document();
    match Package::open(&f.path) {
        Ok(_) => panic!("expected an error for a package with no main document"),
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("document") || msg.contains("relationship"),
                "error should name the missing relationship, got: {msg}"
            );
        }
    }
}

#[test]
fn rejects_a_file_that_is_not_a_zip() {
    let f = fixtures::not_a_zip();
    match Package::open(&f.path) {
        Ok(_) => panic!("a non-zip must not open as an OOXML package"),
        Err(e) => {
            let msg = e.to_string().to_lowercase();
            assert!(
                msg.contains("zip") || msg.contains("package"),
                "error should mention the zip container, got: {msg}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 2. Round-trip preserves everything
// ---------------------------------------------------------------------------

#[test]
fn round_trip_preserves_every_part_byte_for_byte() {
    let f = fixtures::minimal_docx();
    let tmp = f.temp_sibling("out.docx");

    let before = snapshot(&Package::open(&f.path).expect("open"));
    let after = snapshot(&round_trip(&f.path, &tmp));

    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        after.keys().collect::<Vec<_>>(),
        "the set of parts changed across a round trip"
    );

    for (name, bytes) in &before {
        assert_eq!(
            after.get(name).map(|b| b.as_slice()),
            Some(bytes.as_slice()),
            "part {name} changed across a round trip with no edits"
        );
    }
}

#[test]
fn round_trip_is_idempotent() {
    // A second round trip must produce the same bytes as the first. If it does
    // not, something is being normalised on each pass — which means every save
    // a user makes drifts from the last.
    let f = fixtures::minimal_docx();
    let t1 = f.temp_sibling("rt1.docx");
    let t2 = f.temp_sibling("rt2.docx");
    let t3 = f.temp_sibling("rt3.docx");

    let pkg = Package::open(&f.path).expect("open");
    pkg.save(&t1).expect("save 1");
    let p1 = snapshot(&Package::open(&t1).expect("reopen 1"));
    Package::open(&t1)
        .expect("reopen 1b")
        .save(&t2)
        .expect("save 2");
    let p2 = snapshot(&Package::open(&t2).expect("reopen 2"));
    Package::open(&t2)
        .expect("reopen 2b")
        .save(&t3)
        .expect("save 3");
    let p3 = snapshot(&Package::open(&t3).expect("reopen 3"));

    assert_eq!(p1, p2, "second round trip changed the package");
    assert_eq!(p2, p3, "third round trip changed the package");
}

#[test]
fn round_trip_survives_unknown_elements() {
    // ADR-0005's core promise. A document using an element we have no model for
    // must come back with that element intact.
    //
    // `w:glitter` does not exist. That is the point: if the reader "helpfully"
    // dropped elements it does not recognise, this test fails.
    let f = fixtures::docx_with_unknown_elements();
    let tmp = f.temp_sibling("unknown-out.docx");

    let after = snapshot(&round_trip(&f.path, &tmp));
    let reopened = Package::open(&tmp).expect("reopen");
    let main = reopened
        .main_document_part()
        .expect("main part")
        .to_string();
    let xml = String::from_utf8(after[main.as_str()].clone()).expect("XML must be UTF-8");

    // Every element in the fixture must come back. Split into the ones we have
    // no model for (`glitter`, `smartTag`) and the real-but-unimplemented ones
    // (`commentRangeStart`, `commentReference`), because they fail differently:
    // an invented element proves we do not filter by schema, and a real one
    // proves we do not filter by "is this in the spec I implemented".
    for unknown in ["glitter", "smartTag"] {
        assert!(
            xml.contains(unknown),
            "an unrecognised element ({unknown}) was dropped: {xml}"
        );
    }
    for real_but_unimplemented in ["commentRangeStart", "commentRangeEnd", "commentReference"] {
        assert!(
            xml.contains(real_but_unimplemented),
            "a real but unimplemented element ({real_but_unimplemented}) was dropped: {xml}"
        );
    }
    // And the structural things a document cannot survive losing.
    for structural in ["tbl", "tblPr", "tr", "tc", "sectPr", "pgMar"] {
        assert!(
            xml.contains(structural),
            "structure ({structural}) was dropped: {xml}"
        );
    }
}

// ---------------------------------------------------------------------------
// 3. The XML tree preserves what it parsed
// ---------------------------------------------------------------------------

#[test]
fn xml_tree_round_trips_to_identical_bytes() {
    let src = fixtures::messy_document_xml();
    let tree = Node::parse(src.as_bytes()).expect("parse");
    assert_eq!(
        tree.serialize().as_slice(),
        src.as_bytes(),
        "a parse/serialize cycle changed the bytes"
    );
}

#[test]
fn xml_tree_preserves_attribute_order_and_quotes() {
    // `quick-xml` and most parsers normalise quotes and reorder attributes.
    // Both are lossy in a way that matters here.
    let src = "<w:p a='1'   b=\"2\"    c='3'/>";
    let tree = Node::parse(src.as_bytes()).expect("parse");
    assert_eq!(
        tree.serialize().as_slice(),
        src.as_bytes(),
        "attribute quoting or spacing was normalised away"
    );
}

#[test]
fn xml_tree_preserves_cdata_and_comments() {
    let src = "<r><!-- keep me --><![CDATA[<not markup>]]></r>";
    let tree = Node::parse(src.as_bytes()).expect("parse");
    let out = tree.serialize();
    assert!(
        String::from_utf8_lossy(&out).contains("keep me"),
        "comment was dropped: {}",
        String::from_utf8_lossy(&out)
    );
    assert!(
        String::from_utf8_lossy(&out).contains("<not markup>"),
        "CDATA was escaped or dropped: {}",
        String::from_utf8_lossy(&out)
    );
}

#[test]
fn xml_tree_handles_namespaces_without_rewriting_them() {
    let src = "<w:document xmlns:w=\"urn:w\"><w:body/></w:document>";
    let tree = Node::parse(src.as_bytes()).expect("parse");
    let out = String::from_utf8(tree.serialize()).expect("utf8");
    assert_eq!(out, src, "namespace prefixes were rewritten");
}

#[test]
fn xml_tree_rejects_malformed_input_rather_than_guessing() {
    // Silently recovering from malformed XML is how content gets lost. A clear
    // error lets the caller offer "open as plain text" instead.
    for bad in [
        "<w:p><w:r></w:p>", // mismatched close
        "<w:p",             // unclosed
        "not xml at all",   // no markup
        "</w:p>",           // close with no open
        "<w:p",             // unclosed element
        "",                 // empty
    ] {
        assert!(
            Node::parse(bad.as_bytes()).is_err(),
            "malformed input {bad:?} should not parse"
        );
    }
}

// ---------------------------------------------------------------------------
// 4. Editing an element we do understand
// ---------------------------------------------------------------------------

#[test]
fn editing_one_paragraph_leaves_everything_else_untouched() {
    // The realistic case: a user changes their name and nothing else moves.
    let f = fixtures::docx_with_unknown_elements();
    let tmp = f.temp_sibling("edited.docx");

    let pkg = Package::open(&f.path).expect("open");
    let before_other: BTreeMap<String, Vec<u8>> = snapshot(&pkg)
        .into_iter()
        .filter(|(k, _)| !k.starts_with("word/document.xml"))
        .collect();

    let mut edited = pkg.clone();
    let main = edited.main_document_part().expect("main part").to_string();
    let src = String::from_utf8(edited.part_bytes(&main).expect("bytes").to_vec()).expect("utf8");
    let replaced = src.replace("PLACEHOLDER", "Temitayo");
    assert_ne!(src, replaced, "fixture must contain the placeholder");

    let mut tree = Node::parse(src.as_bytes()).expect("parse");
    tree.replace_text("PLACEHOLDER", "Temitayo");
    edited.set_part_bytes(&main, tree.serialize()).expect("set");

    edited.save(&tmp).expect("save");

    let after = Package::open(&tmp).expect("reopen");
    let out_main = after.main_document_part().expect("main part");
    let out = String::from_utf8(after.part_bytes(out_main).expect("bytes").to_vec()).expect("utf8");

    assert!(out.contains("Temitayo"), "the edit was lost");
    assert!(!out.contains("PLACEHOLDER"), "the old text survived");
    assert!(
        out.contains("glitter"),
        "the edit cost us an unknown element"
    );

    let after_other: BTreeMap<String, Vec<u8>> = snapshot(&after)
        .into_iter()
        .filter(|(k, _)| !k.starts_with("word/document.xml"))
        .collect();
    assert_eq!(
        before_other, after_other,
        "editing the main document disturbed other parts"
    );
}
