//! WordprocessingML semantics: paragraphs, runs, and the text they contain.
//!
//! ## Why this layer exists separately from the tree
//!
//! [`crate::ooxml::tree`] is a byte-exact representation. It deliberately knows
//! nothing about what a paragraph *means* — it will hand back a `w:p`, a `w:t`
//! and a `w:hyperlink` without comment. That is correct for the tree and useless
//! for anything a user sees.
//!
//! This module adds the meaning, and it is where most of the compatibility risk
//! in a word processor actually lives. The shape of the problem is that
//! WordprocessingML does not store text where a first reading suggests:
//!
//! - Runs are **not** direct children of a paragraph. A hyperlink, content
//!   control, smart tag, insertion mark or bookmark sits between the paragraph
//!   and its runs, so `w:p > w:r > w:t` covers a minority of real paragraphs.
//! - Some characters are **not** in a `w:t`. `w:tab`, `w:br`, `w:cr`,
//!   `w:noBreakHyphen` and `w:softHyphen` are empty elements that each stand for
//!   one character.
//! - Some `w:t`-shaped content is **not document text**. `w:instrText` is a
//!   field's instruction; `w:delText` is text inside a tracked deletion.
//! - Some text is **not reachable at all** from the paragraph.
//!   `mc:AlternateContent` holds a feature twice, under a `Requires` guard and as
//!   a legacy fallback, and both branches contain character data that is not
//!   document text.
//!
//! Each of these is a way to produce a document that opens fine and reads wrong.
//!
//! ## The rule that shapes the implementation
//!
//! This layer **reads** and never writes. Extraction produces owned `String`s;
//! edits go back through the tree so that ADR-0005's guarantee holds — a
//! parse/serialize cycle with no edits is byte-identical, and an edit changes
//! only the bytes it names.
//!
//! Where a value genuinely cannot be known, this module refuses to guess. `w:sym`
//! is the clearest case: `<w:sym w:font="Wingdings" w:char="F0E0"/>` names a
//! private-use codepoint whose real character depends on a font that may not be
//! installed. Emitting U+F0E0 would put a guaranteed-tofu character in the
//! user's document, so the text becomes a documented placeholder and the raw
//! symbol is reported separately for a font-aware renderer to resolve.

use crate::ooxml::tree::{Element, Node};

/// The WordprocessingML main namespace.
pub const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// Emitted in place of a `w:sym` whose character cannot be resolved.
///
/// U+FFFD REPLACEMENT CHARACTER is the honest answer: it is defined to stand for
/// content known to be present and not renderable, and it is visibly wrong to a
/// reader rather than silently wrong.
pub const SYMBOL_PLACEHOLDER: char = '\u{FFFD}';

/// How to render a paragraph's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextOptions {
    /// Include text inside `w:del` (tracked deletions).
    ///
    /// Off by default: a deletion is text the author has asked to remove, and
    /// showing it makes "accept all changes" ambiguous.
    pub include_deletions: bool,

    /// Collapse runs of ASCII whitespace and trim the edges, as Word does to
    /// `w:t` content that lacks `xml:space="preserve"`.
    ///
    /// U+00A0 is excluded from "whitespace" here: it is visible ink in French,
    /// German and others, and collapsing it corrupts the text.
    pub normalise_whitespace: bool,
}

impl Default for TextOptions {
    fn default() -> Self {
        Self {
            include_deletions: false,
            normalise_whitespace: true,
        }
    }
}

/// A `w:sym` as written.
///
/// The character is a hex code point in the *symbol font's* private use area, so
/// it means something different for each font and usually means nothing
/// renderable on a machine without it. Kept as data rather than guessed at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    /// The `w:font` attribute, if present.
    pub font: Option<String>,
    /// The `w:char` attribute, verbatim: hex digits, no `U+` prefix.
    pub char: String,
}

/// One piece of a run's text.
///
/// The distinction carried here is the whole reason whitespace handling is not a
/// one-line `split_whitespace().join(" ")`. A piece from `w:t` is text whose
/// leading and trailing whitespace may be trimmed or collapsed; a piece from
/// `w:tab` or `w:br` is a **character the user asked for** and is never
/// collapsed. Losing that distinction turns a tab between two runs into
/// nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Piece {
    text: String,
    /// `true` if `xml:space="preserve"` applies: whitespace is exact.
    preserve: bool,
    /// `true` if this came from a `w:t`, so its edge whitespace is eligible for
    /// trimming. `false` for the characters implied by empty elements.
    from_text: bool,
}

/// Text of one run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    /// The run's text, with interior whitespace normalised per
    /// [`TextOptions`].
    ///
    /// Edge whitespace is *not* resolved here, because whether a leading space is
    /// meaningful depends on what came before it — which may be in an earlier
    /// run. [`Paragraph::text`] resolves it across the whole paragraph.
    pub text: String,
    pieces: Vec<Piece>,
    /// `w:sym` elements in this run, kept raw because they cannot be resolved.
    pub symbols: Vec<Symbol>,
    /// `true` if this run came from inside a `w:del` or `w:moveFrom`.
    pub deleted: bool,
}

/// A `w:p` and the text it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paragraph {
    /// Concatenated text of the paragraph's live runs.
    pub text: String,
    /// Runs in document order. Includes deleted runs so that a caller can
    /// re-filter with different options; use [`Paragraph::runs`] for the live
    /// ones.
    runs: Vec<Run>,
}

/// Why a node could not be interpreted as a paragraph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotAParagraph {
    /// The node was not an element named `w:p`.
    NotAParagraph,
    /// It was a `w:p`-named element, but in another namespace.
    ForeignNamespace,
}

impl std::fmt::Display for NotAParagraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotAParagraph::NotAParagraph => write!(f, "element is not a w:p"),
            NotAParagraph::ForeignNamespace => {
                write!(f, "element is not in the WordprocessingML namespace")
            }
        }
    }
}

impl std::error::Error for NotAParagraph {}

/// Elements whose children are formatting, never content.
///
/// Descending into any of these would turn property values into document text.
/// `rPr` deserves particular suspicion: tracked-change markup nests arbitrary
/// elements inside it, so a walker that treats it as opaque by accident rather
/// than by decision will eventually pick up a `w:t` from there.
///
/// Deliberately absent: `del`, `ins`, `moveFrom` and `moveTo`. They hold runs,
/// and their meaning — kept or dropped — is decided in [`collect_runs`]. Listing
/// them here as well would make the property check win, and every tracked change
/// would silently vanish.
const PROPERTY_ELEMENTS: &[&str] = &[
    "pPr",
    "rPr",
    "tblPr",
    "trPr",
    "tcPr",
    "tblPrEx",
    "sectPr",
    "numPr",
    "framePr",
    "pBdr",
    "shd",
    "tabs",
    "spacing",
    "ind",
    "jc",
    "outlineLvl",
    "sdtPr",
    "smartTagPr",
    "tblGrid",
    "tblBorders",
    "tblCellMar",
    "tcBorders",
    "rPrChange",
    "pPrChange",
    "tblPrChange",
    "trPrChange",
    "tcPrChange",
    "sdtEndPr",
];

/// Subtrees that are never walked for text.
///
/// `AlternateContent` stores a feature twice — once guarded by `Requires`, once
/// as a legacy fallback — and both branches contain drawing trees whose character
/// data is not document text. `Fallback` is listed as well because it is the
/// element inside it that actually holds the duplicate content.
const FOREIGN_SUBTREES: &[&str] = &["AlternateContent", "Fallback"];

/// Elements that hold embedded or duplicate content rather than runs.
const NON_RUN_CONTENT: &[&str] = &[
    "object", "pict", "drawing", "comment", "footnote", "endnote", "p", "subDoc",
];

/// The character an empty element stands for, if it stands for one.
///
/// These are empty in the file. Ignoring them loses user-visible content, so each
/// maps to the character Word means.
fn implied_char(element: &Element) -> Option<char> {
    match element.local_name() {
        "tab" => Some('\t'),
        // A `w:br` with a type other than `textWrapping` is pagination, which
        // belongs to layout. Only the default break is a character.
        "br" => match element.attr("type") {
            None | Some("textWrapping") => Some('\n'),
            _ => None,
        },
        "cr" => Some('\n'),
        "noBreakHyphen" => Some('\u{2011}'),
        "softHyphen" => Some('\u{00AD}'),
        _ => None,
    }
}

/// Whether an element is one we know carries runs or inline content.
///
/// Used to decide whether to descend into an unrecognised element. Guessing
/// wrong here is cheap in one direction only: descending into something foreign
/// is prevented by the namespace check, while refusing to descend into a new
/// WordprocessingML wrapper would silently drop its text.
fn is_content_container(element: &Element) -> bool {
    if element.children.is_empty() {
        return false;
    }
    let local = element.local_name();
    is_transparent_wrapper(local) || element.children.iter().any(is_element)
}

/// Wrappers that exist only to hold runs or inline content.
fn is_transparent_wrapper(local: &str) -> bool {
    matches!(
        local,
        "hyperlink"
            | "sdt"
            | "sdtContent"
            | "smartTag"
            | "bookmarkStart"
            | "bookmarkEnd"
            | "proofErr"
            | "commentRangeStart"
            | "commentRangeEnd"
            | "permStart"
            | "permEnd"
            | "customXml"
            | "dir"
            | "bdo"
            | "fldSimple"
            | "bidi"
            | "moveFromRangeStart"
            | "moveFromRangeEnd"
            | "moveToRangeStart"
            | "moveToRangeEnd"
    )
}

fn is_element(node: &Node) -> bool {
    matches!(node, Node::Element(_))
}

impl Paragraph {
    /// Interpret a `w:p` element.
    ///
    /// Returns [`NotAParagraph`] rather than panicking on the wrong element:
    /// traversal code meets tables, comments and drawing nodes constantly, and a
    /// filter it has to write is one this function should be writing.
    pub fn from_element(node: &Node) -> Result<Paragraph, NotAParagraph> {
        let element = match node {
            Node::Element(e) => e,
            _ => return Err(NotAParagraph::NotAParagraph),
        };
        if element.local_name() != "p" {
            return Err(NotAParagraph::NotAParagraph);
        }
        if !is_wordprocessingml(element) {
            return Err(NotAParagraph::ForeignNamespace);
        }

        let opts = TextOptions::default();
        let mut runs = Vec::new();
        collect_runs(element, false, opts, &mut runs);
        let text = paragraph_text(&runs, opts);

        Ok(Paragraph { text, runs })
    }

    /// The paragraph's text under the given options.
    pub fn text(&self, opts: TextOptions) -> String {
        if opts == TextOptions::default() {
            return self.text.clone();
        }
        paragraph_text(&self.runs, opts)
    }

    /// Live runs, in document order.
    pub fn runs(&self) -> impl Iterator<Item = &Run> {
        self.runs.iter().filter(|r| !r.deleted)
    }

    /// Every run, including those inside a tracked deletion.
    pub fn runs_including_deletions(&self) -> impl Iterator<Item = &Run> {
        self.runs.iter()
    }

    /// Every `w:sym` in the paragraph, in document order.
    ///
    /// A font-aware renderer needs these to resolve to real glyphs; a plain text
    /// export gets [`SYMBOL_PLACEHOLDER`] and this list to explain why.
    pub fn symbols(&self) -> impl Iterator<Item = &Symbol> {
        self.runs.iter().flat_map(|r| r.symbols.iter())
    }
}

/// Join runs into paragraph text, resolving whitespace across run boundaries.
///
/// ## Why this is not a concatenation
///
/// Word splits text across runs wherever formatting changes, and the split is
/// invisible. `<w:t>Hello</w:t>` followed by `<w:t> world</w:t>` in two runs is
/// the same document as one run containing `Hello world`. Normalising each run
/// on its own turns the space at the start of the second run into a leading
/// space and then trims it, producing `Helloworld`.
///
/// So the runs are streamed as one character sequence, with edge decisions
/// deferred until the whole paragraph has been seen.
///
/// Two rules keep that from going wrong:
///
/// - A piece marked `xml:space="preserve"` is exact. Its whitespace is emitted
///   as written, including at the edges.
/// - Whitespace that came from an empty element (`w:tab`, `w:br`) is a
///   character, not layout. It is never collapsed and never trimmed, so a
///   paragraph ending in a tab keeps the tab.
fn join_normalised(pieces: &[Piece]) -> String {
    let mut out = String::new();
    let mut pending_space = false;

    for piece in pieces {
        if piece.preserve {
            // Exact text: settle any deferred separator, then take it as-is.
            if pending_space {
                if should_separate(&out) {
                    out.push(' ');
                }
                pending_space = false;
            }
            out.push_str(&piece.text);
            continue;
        }

        if !piece.from_text {
            // An implied character: a tab or a break. Emit it literally, and
            // let it count as content so a following space is not collapsed
            // into it.
            if pending_space {
                if should_separate(&out) {
                    out.push(' ');
                }
                pending_space = false;
            }
            out.push_str(&piece.text);
            continue;
        }

        for c in piece.text.chars() {
            if matches!(c, ' ' | '\t' | '\n' | '\r') {
                pending_space = true;
                continue;
            }
            if pending_space {
                if should_separate(&out) {
                    out.push(' ');
                }
                pending_space = false;
            }
            out.push(c);
        }
    }

    // A deferred separator at the very end is a trimmed edge.
    out
}

/// Whether a pending whitespace run should become a space in `out`.
///
/// Not at the start, not after a line break — a break already ends the line —
/// and not when something already provides the separation.
fn should_separate(out: &str) -> bool {
    !out.is_empty() && !out.ends_with('\n') && !out.ends_with(' ') && !out.ends_with('\t')
}

/// Join pieces verbatim, with no whitespace decisions at all.
fn join_verbatim(pieces: &[Piece]) -> String {
    pieces.iter().map(|p| p.text.as_str()).collect()
}

/// Join a paragraph's runs, streaming every live run's pieces as one sequence.
///
/// This is the paragraph-level view, and it is where run boundaries stop mattering
/// for whitespace. Deleted runs are skipped unless the caller asked for them —
/// and a skipped run's pieces do not participate in the whitespace state, so
/// deleting "a " from "a b" cannot leave a dangling space behind.
fn paragraph_text(runs: &[Run], opts: TextOptions) -> String {
    let mut out = String::new();
    let mut pending_space = false;

    for run in runs {
        if run.deleted && !opts.include_deletions {
            continue;
        }
        for piece in &run.pieces {
            if piece.preserve {
                if pending_space {
                    if should_separate(&out) {
                        out.push(' ');
                    }
                    pending_space = false;
                }
                out.push_str(&piece.text);
                continue;
            }
            if !piece.from_text {
                if pending_space {
                    if should_separate(&out) {
                        out.push(' ');
                    }
                    pending_space = false;
                }
                out.push_str(&piece.text);
                continue;
            }
            if !opts.normalise_whitespace {
                out.push_str(&piece.text);
                continue;
            }
            for c in piece.text.chars() {
                if matches!(c, ' ' | '\t' | '\n' | '\r') {
                    pending_space = true;
                    continue;
                }
                if pending_space {
                    if should_separate(&out) {
                        out.push(' ');
                    }
                    pending_space = false;
                }
                out.push(c);
            }
        }
    }

    out
}

/// Whether the element belongs to the WordprocessingML main namespace.
///
/// Prefers an explicit `xmlns` declaration. Failing that, falls back to the
/// prefix, because that is all the tree can see: declarations live on whichever
/// ancestor introduced them, and a subtree handed to us on its own has lost that
/// context. Word uses `w` for the main namespace in every file it writes, so the
/// fallback is right for real input and the explicit check is right whenever the
/// declaration is reachable.
fn is_wordprocessingml(element: &Element) -> bool {
    let qname = element.qualified_name();

    if let Some((prefix, _)) = qname.split_once(':') {
        return match element.attr(&format!("xmlns:{prefix}")) {
            Some(uri) => uri == W_NS,
            None => prefix == "w",
        };
    }

    match element.attr("xmlns") {
        Some(uri) => uri == W_NS,
        None => true,
    }
}

/// Whether an element's subtree may be searched for runs.
///
/// A `w:`-prefixed element always may. An element with another prefix is
/// foreign unless it declares the WordprocessingML namespace itself, which
/// happens when a document rebinds the default namespace.
fn is_in_walkable_namespace(element: &Element) -> bool {
    let qname = element.qualified_name();
    match qname.split_once(':') {
        Some(("w", _)) => true,
        Some((prefix, _)) => element.attr(&format!("xmlns:{prefix}")) == Some(W_NS),
        None => element.attr("xmlns").is_none_or(|uri| uri == W_NS),
    }
}

/// Collect runs from a paragraph element, descending through wrappers.
fn collect_runs(element: &Element, deleted: bool, opts: TextOptions, out: &mut Vec<Run>) {
    for child in &element.children {
        let Node::Element(child) = child else {
            continue;
        };
        let local = child.local_name();

        // Tracked-change containers first: they hold runs, and their meaning
        // depends on which way the change goes.
        match local {
            "del" | "moveFrom" => {
                collect_runs(child, true, opts, out);
                continue;
            }
            "ins" | "moveTo" => {
                // An insertion is text the author added and intends to keep.
                collect_runs(child, deleted, opts, out);
                continue;
            }
            _ => {}
        }

        if PROPERTY_ELEMENTS.contains(&local)
            || FOREIGN_SUBTREES.contains(&local)
            || NON_RUN_CONTENT.contains(&local)
            || !is_in_walkable_namespace(child)
        {
            continue;
        }

        if local == "r" {
            out.push(build_run(child, deleted, opts));
        } else if is_content_container(child) {
            collect_runs(child, deleted, opts, out);
        }
    }
}

/// Build one run's text.
fn build_run(element: &Element, deleted: bool, opts: TextOptions) -> Run {
    let mut pieces = Vec::new();
    let mut symbols = Vec::new();
    collect_run_text(element, deleted, &mut pieces, &mut symbols);

    // `Run::text` is the run on its own: interior whitespace collapsed, edges
    // left alone. Whether an edge is meaningful depends on the neighbouring
    // run, which only `Paragraph::text` can see.
    let text = if opts.normalise_whitespace {
        join_normalised(&pieces)
    } else {
        join_verbatim(&pieces)
    };

    Run {
        text,
        pieces,
        symbols,
        deleted,
    }
}

/// Collect the characters inside a single run.
fn collect_run_text(
    element: &Element,
    deleted: bool,
    pieces: &mut Vec<Piece>,
    symbols: &mut Vec<Symbol>,
) {
    for child in &element.children {
        let Node::Element(child) = child else {
            continue;
        };
        let local = child.local_name();

        match local {
            // Formatting.
            "rPr" | "rPrChange" => continue,

            // Deleted text. `w:delText` is the usual spelling; a plain `w:t`
            // inside a `w:del` is the same thing written by a different tool.
            "delText" => {
                if deleted {
                    push_text(pieces, &child.text_content(), preserves_space(child));
                }
                continue;
            }

            // A field's instruction, not its result. Showing it would put
            // `HYPERLINK "http://example.com"` in the user's document.
            "instrText" | "delInstrText" | "fldChar" => continue,

            // A symbol font codepoint we cannot resolve without the font.
            "sym" => {
                symbols.push(Symbol {
                    font: child.attr("font").map(str::to_string),
                    char: child.attr("char").unwrap_or_default().to_string(),
                });
                pieces.push(Piece {
                    text: SYMBOL_PLACEHOLDER.to_string(),
                    preserve: true,
                    from_text: false,
                });
                continue;
            }

            "t" => {
                push_text(pieces, &child.text_content(), preserves_space(child));
                continue;
            }

            _ => {}
        }

        if PROPERTY_ELEMENTS.contains(&local)
            || FOREIGN_SUBTREES.contains(&local)
            || NON_RUN_CONTENT.contains(&local)
            || !is_in_walkable_namespace(child)
        {
            continue;
        }

        if let Some(c) = implied_char(child) {
            pieces.push(Piece {
                text: c.to_string(),
                // Marked preserve so the normaliser treats it as a character
                // rather than layout.
                preserve: true,
                from_text: false,
            });
        } else if is_element_container(child) {
            collect_run_text(child, deleted, pieces, symbols);
        }
    }
}

/// Whether to descend into an unrecognised element inside a run.
fn is_element_container(element: &Element) -> bool {
    if element.children.is_empty() {
        return false;
    }
    is_transparent_wrapper(element.local_name()) || element.children.iter().any(is_element)
}

/// Whether the element asks for whitespace to be kept exactly.
fn preserves_space(element: &Element) -> bool {
    element.attr("space") == Some("preserve")
}

/// Append a text piece, recording how its whitespace should be treated.
///
/// Nothing is normalised here. `xml:space="preserve"` is recorded as exact, and
/// everything else is deferred to [`paragraph_text`], which is the only place
/// that can see what precedes and follows this piece.
fn push_text(pieces: &mut Vec<Piece>, raw: &str, preserve: bool) {
    if raw.is_empty() {
        return;
    }
    pieces.push(Piece {
        text: raw.to_string(),
        preserve,
        from_text: true,
    });
}
