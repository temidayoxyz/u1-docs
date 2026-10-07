//! Regression tests for the Phase 0 Spike B findings.
//!
//! Spike A's tests exist to stop a dependency bump silently undoing a decision
//! made from evidence. These do the same for the caret and navigation layer.
//!
//! Three of these tests encode bugs this spike actually found. Each would fail
//! if the underlying mistake were reintroduced, which is the point: the bugs
//! were silent, so nothing else would have caught them.
//!
//! ## Portability
//!
//! Everything here is font- and script-independent. The failures these tests
//! guard against were found on Windows, but the mistakes — a wrong coordinate
//! space, a per-codepoint cluster assumption — are the same everywhere.

use parley::{FontContext, LayoutContext};

use u1_docs::caret::{Affinity, CaretMap, Direction, Move, move_caret};

const WRAP: f32 = 624.0;
const SIZE: f32 = 11.0;

fn map_for(text: &str, width: f32) -> CaretMap {
    let mut fc = FontContext::new();
    let mut lc = LayoutContext::new();
    CaretMap::build_default(&mut lc, &mut fc, text, Some(width), SIZE)
}

/// Long enough to wrap at a narrow column.
const LONG: &str = "The quick brown fox jumps over the lazy dog. Pack my box with five dozen \
                    liquor jugs. How vexingly quick daft zebras jump!";

// ---------------------------------------------------------------------------
// Finding: LineMetrics.offset is a horizontal alignment offset, not a y
// position. Reading it as y made vertical navigation a silent no-op.
// ---------------------------------------------------------------------------

#[test]
fn line_top_is_strictly_increasing_down_the_paragraph() {
    // Every line reported y = 0 before this was fixed, so Down never left the
    // first line. `line_top` must accumulate.
    let m = map_for(LONG, 220.0);
    let n = m.line_count();
    assert!(n > 1, "test needs a multi-line paragraph, got {n} line(s)");

    let mut prev = f32::NEG_INFINITY;
    for i in 0..n {
        let top = m.line_top(i).expect("line_top for every line");
        assert!(
            top > prev,
            "line {i} top {top} is not below the previous line {prev}"
        );
        prev = top;
    }
}

#[test]
fn line_tops_sum_to_the_layout_height() {
    // Confirms the accumulation model matches parley's own definition of height.
    // If parley ever changes how height is derived, this catches the divergence
    // instead of letting caret geometry drift from layout.
    let m = map_for(LONG, 220.0);
    let last = m.line_count() - 1;
    let bottom =
        m.line_top(last).expect("last line top") + m.line_height(last).expect("last line height");
    assert!(
        (bottom - m.height()).abs() < 0.5,
        "stacked line heights {bottom} != layout height {}",
        m.height()
    );
}

#[test]
fn caret_rects_on_different_lines_have_different_y() {
    // The direct symptom of the offset bug: carets on different lines all
    // reported the same y, so the caret appeared to jump horizontally between
    // lines and an IME anchor could not be positioned.
    let m = map_for(LONG, 220.0);
    let first = m.caret_rect(0, Affinity::Start).expect("caret at 0");
    let last_byte = m.text().len();
    let last = m
        .caret_rect(last_byte, Affinity::Start)
        .expect("caret at end");

    assert!(
        last.y > first.y,
        "caret y did not increase between line 0 ({}) and the last line ({})",
        first.y,
        last.y
    );
}

#[test]
fn hit_testing_at_line_midpoints_resolves_to_that_line() {
    // Guards the hit-test half of vertical navigation.
    let m = map_for(LONG, 220.0);
    for i in 0..m.line_count() {
        let Some(mid) = m.line_mid_y(i) else { continue };
        let hit = m
            .hit_test(1.0, mid, false)
            .unwrap_or_else(|| panic!("hit_test returned nothing at y={mid} for line {i}"));
        let landed = m.line_index_at(hit.byte_index);
        assert_eq!(
            landed,
            Some(i),
            "hit at y={mid} landed on line {landed:?}, expected {i}"
        );
    }
}

// ---------------------------------------------------------------------------
// Finding: parley clusters are per-codepoint, so caret stops need grapheme
// filtering or the user gets a stop inside a combining sequence.
// ---------------------------------------------------------------------------

#[test]
fn grapheme_filter_removes_stops_inside_combining_sequences() {
    // e + COMBINING ACUTE ACCENT is one grapheme. parley gives 2 clusters and 3
    // raw stops, which puts a caret between the letter and its accent.
    let m = map_for("e\u{0301}", WRAP);
    let raw = m.caret_positions().len();
    let graphemes = m.grapheme_caret_positions().len();
    assert_eq!(raw, 3, "expected 3 raw stops for a 2-codepoint input");
    assert_eq!(graphemes, 2, "expected 2 grapheme stops: before and after");
}

#[test]
fn grapheme_filter_handles_devanagari_matra() {
    // A consonant plus its vowel sign is one grapheme. Without filtering, the
    // caret can land between them and typing there orphans the matra.
    let m = map_for("\u{0915}\u{093F}", WRAP); // ka + i-matra
    assert_eq!(
        m.grapheme_caret_positions().len(),
        2,
        "ka + i-matra must be a single caret stop pair"
    );
}

#[test]
fn raw_stops_always_sit_on_char_boundaries() {
    // Defensive: every geometry path assumes byte offsets are valid UTF-8
    // boundaries. A non-boundary offset would panic on slicing.
    for text in [
        "e\u{0301}",
        "\u{1F468}\u{200D}\u{1F469}",
        "\u{0E04}\u{0E38}",
        LONG,
    ] {
        let m = map_for(text, 220.0);
        for pos in m.caret_positions() {
            assert!(
                text.is_char_boundary(pos),
                "caret offset {pos} is not a char boundary in {text:?}"
            );
        }
    }
}

#[test]
fn snapping_never_produces_an_invalid_offset() {
    // Every byte index, including ones landing mid-codepoint, must snap to
    // something sliceable.
    let text = "a\u{1F44D}\u{1F3FD}b\u{1F1F3}\u{1F1F4}c";
    let m = map_for(text, WRAP);
    for byte in 0..=text.len() {
        let snapped = m.snap_to_cluster_boundary(byte);
        assert!(
            text.is_char_boundary(snapped),
            "snapping byte {byte} produced invalid offset {snapped}"
        );
    }
}

// ---------------------------------------------------------------------------
// Finding: affinity must be explicit, because Start and End swap sides in RTL.
// ---------------------------------------------------------------------------

#[test]
fn rtl_start_affinity_is_right_of_end_affinity() {
    let m = map_for("\u{0645}\u{0631}\u{062D}\u{0628}\u{0627}", WRAP);
    assert!(m.is_rtl(), "expected the paragraph to resolve RTL");
    let s = m.caret_rect(0, Affinity::Start).expect("start");
    let e = m.caret_rect(0, Affinity::End).expect("end");
    assert!(
        s.x > e.x,
        "in RTL the Start edge ({}) must be right of the End edge ({})",
        s.x,
        e.x
    );
}

#[test]
fn ltr_start_affinity_is_left_of_end_affinity() {
    let m = map_for("abc", WRAP);
    let s = m.caret_rect(0, Affinity::Start).expect("start");
    let e = m.caret_rect(0, Affinity::End).expect("end");
    assert!(
        s.x < e.x,
        "in LTR Start ({}) must be left of End ({})",
        s.x,
        e.x
    );
}

#[test]
fn caret_at_end_of_text_exists_and_has_height() {
    // Where the caret sits after typing to the end. It belongs to no cluster,
    // so it is placed from line geometry and is easy to break.
    let m = map_for("hello", WRAP);
    let r = m
        .caret_rect(m.text().len(), Affinity::End)
        .expect("caret at end of text");
    assert!(r.height > 0.0, "end-of-text caret has no height");
    assert!(r.width > 0.0, "end-of-text caret has no width");
}

// ---------------------------------------------------------------------------
// Navigation
// ---------------------------------------------------------------------------

#[test]
fn logical_forward_walks_every_cluster_to_the_end() {
    let m = map_for("The quick brown fox", WRAP);
    let mut pos = 0usize;
    let mut steps = 0usize;
    while let Move::To(next) = move_caret(&m, pos, Direction::LogicalForward, None) {
        assert!(next > pos, "forward move did not advance: {pos} -> {next}");
        pos = next;
        steps += 1;
        assert!(steps < 1000, "forward navigation is not terminating");
    }
    assert_eq!(pos, m.text().len(), "forward walk did not reach the end");
}

#[test]
fn logical_back_returns_to_zero() {
    let m = map_for("The quick brown fox", WRAP);
    let mut pos = m.text().len();
    let mut steps = 0usize;
    while let Move::To(next) = move_caret(&m, pos, Direction::LogicalBack, None) {
        assert!(next < pos, "backward move did not retreat: {pos} -> {next}");
        pos = next;
        steps += 1;
        assert!(steps < 1000, "backward navigation is not terminating");
    }
    assert_eq!(pos, 0, "backward walk did not reach the start");
}

#[test]
fn navigation_is_blocked_at_the_boundaries() {
    let m = map_for("abc", WRAP);
    assert_eq!(
        move_caret(&m, 0, Direction::LogicalBack, None),
        Move::Blocked,
        "backward at 0 must be blocked"
    );
    assert_eq!(
        move_caret(&m, m.text().len(), Direction::LogicalForward, None),
        Move::Blocked,
        "forward at the end must be blocked"
    );
}

#[test]
fn vertical_navigation_changes_line_and_is_bounded() {
    let m = map_for(LONG, 220.0);
    let start_line = m.line_index_at(0).expect("byte 0 is on a line");

    assert_eq!(
        move_caret(&m, 0, Direction::Up, None),
        Move::Blocked,
        "Up from the first line must be blocked"
    );

    let last_byte = m.text().len();
    let last_line = m.line_index_at(last_byte).expect("end is on a line");
    assert_eq!(
        move_caret(&m, last_byte, Direction::Down, None),
        Move::Blocked,
        "Down from the last line must be blocked"
    );

    // Walk down one line at a time and confirm we progress monotonically.
    let mut pos = 0usize;
    let mut lines = vec![start_line];
    while let Move::To(next) = move_caret(&m, pos, Direction::Down, None) {
        let Some(l) = m.line_index_at(next) else {
            break;
        };
        lines.push(l);
        pos = next;
        assert!(lines.len() < 200, "vertical navigation is not terminating");
    }
    assert!(
        lines.windows(2).all(|w| w[1] > w[0]),
        "line indices did not increase monotonically: {lines:?}"
    );
    assert_eq!(lines.last().copied(), Some(last_line));
}

// ---------------------------------------------------------------------------
// Re-break: caret geometry must stay valid across resizes (Spike A finding 4
// makes this the hot path, not an edge case).
// ---------------------------------------------------------------------------

#[test]
fn every_caret_survives_rebreak_at_every_width() {
    let text = LONG;
    let mut fc = FontContext::new();
    let mut lc = LayoutContext::new();
    let mut m = CaretMap::build_default(&mut lc, &mut fc, text, Some(624.0), SIZE);

    for width in [180.0f32, 260.0, 400.0, 624.0, 900.0] {
        m.rebreak(Some(width));
        for pos in m.caret_positions() {
            let r = m
                .caret_rect(pos, Affinity::Start)
                .unwrap_or_else(|| panic!("caret at {pos} lost after re-break at {width}"));
            assert!(r.height > 0.0, "caret height collapsed at width {width}");
        }
    }
}

#[test]
fn rebreak_does_not_require_rebuilding_the_document() {
    // The whole reason `rebreak` exists (Spike A finding 4): layout is cheap to
    // re-break and expensive to rebuild. A caret map must survive re-breaks
    // with the same content.
    let m = map_for(LONG, 400.0);
    let before = m.text().to_string();
    let mut m = m;
    for _ in 0..5 {
        m.rebreak(Some(300.0));
        assert_eq!(m.text(), before, "re-break mutated the text");
    }
}
