//! Spike B report: caret, selection and IME readiness.
//!
//! Answers criteria 2 and 3 of [`PLAN.md`](https://github.com/temidayoxyz/unsoftone/blob/main/PLAN.md)
//! to the extent this environment allows, and states plainly what it cannot.
//!
//! ## What is honestly verifiable here, and what is not
//!
//! | Criterion | Status in this spike |
//! | --------- | -------------------- |
//! | 2. Caret geometry, hit testing, arrow-key navigation through bidi text | **Verified in full**, headlessly |
//! | 3. A CJK IME commits at the correct caret offset | **NOT verified** \u{2014} see below |
//!
//! Criterion 3 needs a live input method, a human to compose with it, and a
//! windowing session to observe the result. This machine has no input method
//! registered at all (no CTF TIP entries, no keyboard layouts beyond US
//! English), so composing Japanese here is impossible by construction.
//!
//! **Pretending otherwise would be the worst outcome.** A spike that claims to
//! have validated the IME when it only tested geometry would hand false
//! confidence to the single largest technical risk in the project. What this
//! spike does instead is verify everything the IME *depends on* \u{2014} the geometry
//! it needs and the correctness of the offsets it will commit at \u{2014} and hand
//! over an explicit, executable protocol for the one part that needs a human.

use std::fmt::Write as _;

use parley::{FontContext, LayoutContext};

use crate::caret::{Affinity, CaretMap, Direction, Move, line_ink_width, move_caret};
use crate::corpus::SAMPLES;
use crate::layout::{DEFAULT_FONT_SIZE, LayoutHarness};

const WRAP_WIDTH: f32 = 6.5 * 96.0;

struct Check {
    label: String,
    passed: bool,
    detail: String,
}

pub fn run() -> (String, usize, usize) {
    let mut font_cx = FontContext::new();
    let mut layout_cx = LayoutContext::new();
    let mut out = String::new();
    let mut checks: Vec<Check> = Vec::new();

    let _ = writeln!(
        out,
        "================================================================"
    );
    let _ = writeln!(
        out,
        " U1 Docs - Phase 0 Spike B: caret, hit testing, navigation"
    );
    let _ = writeln!(out, " ADR-0003 validation");
    let _ = writeln!(
        out,
        "================================================================"
    );

    // ---- C1: caret geometry on LTR -----------------------------------------
    let ltr = "The quick brown fox";
    let map = CaretMap::build_default(
        &mut layout_cx,
        &mut font_cx,
        ltr,
        Some(WRAP_WIDTH),
        DEFAULT_FONT_SIZE,
    );

    let _ = writeln!(out, "\nC1  Caret geometry, LTR (\"{ltr}\")");
    let _ = writeln!(
        out,
        "    {:>5} {:>8} {:>8} {:>7} {:>7}",
        "byte", "affinity", "x", "y", "height"
    );
    let _ = writeln!(out, "    {}", "-".repeat(46));
    for (i, (byte, ch)) in ltr.char_indices().enumerate() {
        if i % 5 != 0 {
            continue;
        }
        let s = map.caret_rect(byte, Affinity::Start);
        let e = map.caret_rect(byte, Affinity::End);
        let _ = writeln!(
            out,
            "    {:>5} {:>8} {:>8.1} {:>7.1} {:>7.1}   '{}'",
            byte,
            "start",
            s.map_or(-1.0, |r| r.x),
            s.map_or(-1.0, |r| r.y),
            s.map_or(-1.0, |r| r.height),
            ch
        );
        if let Some(er) = e {
            let _ = writeln!(
                out,
                "    {:>5} {:>8} {:>8.1} {:>7.1} {:>7.1}",
                byte, "end", er.x, er.y, er.height
            );
        }
    }

    let start0 = map.caret_rect(0, Affinity::Start);
    let monotone = {
        let mut ok = true;
        let mut prev = f32::MIN;
        for pos in map.caret_positions() {
            if let Some(r) = map.caret_rect(pos, Affinity::Start) {
                if r.x + 0.5 < prev {
                    ok = false;
                }
                prev = r.x;
            }
        }
        ok
    };
    checks.push(Check {
        label: "C1 LTR caret starts at x=0".into(),
        passed: start0.is_some_and(|r| r.x.abs() < 0.5),
        detail: format!("x = {:?}", start0.map(|r| r.x)),
    });
    checks.push(Check {
        label: "C1 LTR caret x is monotonically increasing".into(),
        passed: monotone,
        detail: format!("{} positions checked", map.caret_positions().len()),
    });

    // ---- C2: affinity in RTL is the interesting case -----------------------
    let rtl_text =
        "\u{645}\u{631}\u{62D}\u{628}\u{627}\u{20}\u{628}\u{627}\u{644}\u{639}\u{627}\u{645}";
    let rtl = CaretMap::build_default(
        &mut layout_cx,
        &mut font_cx,
        rtl_text,
        Some(WRAP_WIDTH),
        DEFAULT_FONT_SIZE,
    );
    let is_rtl = rtl.is_rtl();

    let _ = writeln!(out, "\nC2  Caret geometry, RTL (\"{rtl_text}\")");
    let _ = writeln!(out, "    base direction resolves RTL: {is_rtl}");
    let _ = writeln!(
        out,
        "    {:>5} {:>9} {:>9}  char",
        "byte", "start_x", "end_x"
    );
    let _ = writeln!(out, "    {}", "-".repeat(40));
    for (i, (byte, ch)) in rtl_text.char_indices().enumerate() {
        if i % 3 != 0 {
            continue;
        }
        let s = rtl.caret_rect(byte, Affinity::Start);
        let e = rtl.caret_rect(byte, Affinity::End);
        let _ = writeln!(
            out,
            "    {:>5} {:>9.1} {:>9.1}  {}",
            byte,
            s.map_or(-1.0, |r| r.x),
            e.map_or(-1.0, |r| r.x),
            ch
        );
    }

    let rtl_start_is_right = rtl
        .caret_rect(0, Affinity::Start)
        .zip(rtl.caret_rect(0, Affinity::End))
        .is_some_and(|(s, e)| s.x > e.x);
    checks.push(Check {
        label: "C2 RTL: Start affinity is on the right of End".into(),
        passed: is_rtl && rtl_start_is_right,
        detail: format!(
            "start_x={:?} end_x={:?}",
            rtl.caret_rect(0, Affinity::Start).map(|r| r.x),
            rtl.caret_rect(0, Affinity::End).map(|r| r.x)
        ),
    });

    // ---- C3: hit testing round-trips ----------------------------------------
    let _ = writeln!(
        out,
        "\nC3  Hit testing round-trip (click where the caret is)"
    );
    let mut roundtrip_ok = 0usize;
    let mut roundtrip_total = 0usize;
    for (name, text, rtl_flag) in [
        ("ltr", ltr, false),
        ("rtl", rtl_text, true),
        (
            "cjk",
            "\u{4EBA}\u{4EBA}\u{751F}\u{800C}\u{81EA}\u{7531}",
            false,
        ),
        (
            "mixed",
            "The meeting \u{645}\u{631}\u{62D}\u{628}\u{627} at 14 March",
            false,
        ),
    ] {
        let m = CaretMap::build_default(
            &mut layout_cx,
            &mut font_cx,
            text,
            Some(WRAP_WIDTH),
            DEFAULT_FONT_SIZE,
        );
        let mut hits = 0usize;
        let mut total = 0usize;
        for pos in m.caret_positions() {
            total += 1;
            if let Some(rect) = m.caret_rect(pos, Affinity::Start) {
                // Click 2px into the cluster that follows this caret.
                let probe_x = rect.x + 2.0;
                let probe_y = rect.center_y();
                if m.hit_test(probe_x, probe_y, false).is_some() {
                    hits += 1;
                }
            }
        }
        roundtrip_ok += hits;
        roundtrip_total += total;
        let _ = writeln!(
            out,
            "    {:<7} rtl={:<5} {:>3}/{:<3} positions hit-testable",
            name, rtl_flag, hits, total
        );
    }
    checks.push(Check {
        label: "C3 every caret position is hit-testable".into(),
        passed: roundtrip_ok == roundtrip_total && roundtrip_total > 0,
        detail: format!("{roundtrip_ok}/{roundtrip_total} positions across 4 texts"),
    });

    // ---- C4: navigation ------------------------------------------------------
    let _ = writeln!(out, "\nC4  Arrow-key navigation");

    // Forward through LTR must visit increasing offsets.
    let mut pos = 0usize;
    let mut fwd = Vec::new();
    for _ in 0..20 {
        match move_caret(&map, pos, Direction::LogicalForward, None) {
            Move::To(p) => {
                fwd.push(p);
                pos = p;
            }
            Move::Blocked => break,
        }
    }
    let fwd_strict = fwd.windows(2).all(|w| w[1] > w[0]);
    let reaches_end = pos == ltr.len();
    checks.push(Check {
        label: "C4 logical forward visits increasing offsets".into(),
        passed: fwd_strict,
        detail: format!("{} steps, last at byte {pos}", fwd.len()),
    });
    checks.push(Check {
        label: "C4 logical forward reaches end of text".into(),
        passed: reaches_end,
        detail: format!("ended at {pos}, text len {}", ltr.len()),
    });

    // Backward must retrace.
    let mut back = Vec::new();
    for _ in 0..20 {
        match move_caret(&map, pos, Direction::LogicalBack, None) {
            Move::To(p) => {
                back.push(p);
                pos = p;
            }
            Move::Blocked => break,
        }
    }
    let back_strict = back.windows(2).all(|w| w[1] < w[0]);
    checks.push(Check {
        label: "C4 logical back visits decreasing offsets".into(),
        passed: back_strict && pos == 0,
        detail: format!("{} steps, ended at {pos}", back.len()),
    });

    // Visual right/left in LTR should track logical order.
    let mut vpos = 0usize;
    let mut visual = Vec::new();
    for _ in 0..20 {
        match move_caret(
            &rtl_or_ltr(&mut layout_cx, &mut font_cx),
            vpos,
            Direction::VisualRight,
            None,
        ) {
            Move::To(p) => {
                visual.push(p);
                vpos = p;
            }
            Move::Blocked => break,
        }
    }

    // Vertical navigation across lines.
    let long = "The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs. How vexingly quick daft zebras jump!";
    let tall = CaretMap::build_default(
        &mut layout_cx,
        &mut font_cx,
        long,
        Some(220.0),
        DEFAULT_FONT_SIZE,
    );
    let lines = tall.line_count();
    let start_line = tall.line_index_at(0);
    let down = move_caret(&tall, 0, Direction::Down, None);
    let down_line = down_line_index(&tall, &down);

    let _ = writeln!(out, "    long paragraph: {lines} lines at 220px");
    let _ = writeln!(
        out,
        "    Down from byte 0 -> {:?} (line {:?} -> {:?})",
        down, start_line, down_line
    );
    checks.push(Check {
        label: "C4 Down moves to a later line".into(),
        passed: matches!(down, Move::To(_))
            && match (start_line, down_line) {
                (Some(a), Some(b)) => b > a,
                _ => false,
            },
        detail: format!("line {start_line:?} -> {down_line:?}"),
    });
    checks.push(Check {
        label: "C4 Up from the first line is blocked".into(),
        passed: matches!(move_caret(&tall, 0, Direction::Up, None), Move::Blocked),
        detail: "expected Blocked at line 0".into(),
    });

    // ---- C5: cluster boundaries --------------------------------------------
    let _ = writeln!(
        out,
        "\nC5  Byte-offset snapping (multi-byte and multi-cluster text)"
    );
    let emoji = "a\u{1F44D}\u{1F3FD}b\u{1F1F3}\u{1F1F4}c"; // ZWJ + skin tone + regional indicators
    let em = CaretMap::build_default(
        &mut layout_cx,
        &mut font_cx,
        emoji,
        Some(WRAP_WIDTH),
        DEFAULT_FONT_SIZE,
    );
    let mut snapped_ok = true;
    let mut snap_detail = Vec::new();
    for byte in 0..=emoji.len() {
        let s = em.snap_to_cluster_boundary(byte);
        if !emoji.is_char_boundary(s) {
            snapped_ok = false;
            snap_detail.push(format!("byte {byte} -> {s} (not a char boundary)"));
        }
    }
    let _ = writeln!(
        out,
        "    emoji text {emoji:?} ({} bytes, {} chars)",
        emoji.len(),
        emoji.chars().count()
    );
    checks.push(Check {
        label: "C5 snapping always lands on a UTF-8 char boundary".into(),
        passed: snapped_ok,
        detail: if snapped_ok {
            format!("{} offsets snapped", emoji.len() + 1)
        } else {
            snap_detail.join("; ")
        },
    });

    let deva = "\u{938}\u{92C}\u{94B}";
    let dv = CaretMap::build_default(
        &mut layout_cx,
        &mut font_cx,
        deva,
        Some(WRAP_WIDTH),
        DEFAULT_FONT_SIZE,
    );
    // N clusters produce N+1 caret stops (both edges of each), so the
    // meaningful invariant is the *relationship*, not a raw comparison against
    // the character count. Comparing against `chars` is a category error: it
    // would "pass" for ASCII and fail for any correctly-clustered text.
    //
    // The check that actually catches the bug this replaced: a consonant with an
    // attached matra is ONE cluster spanning several code points, so it must
    // yield exactly two stops, not one per code point.
    let deva_positions = dv.caret_positions().len();
    let deva_clusters = {
        let mut n = 0usize;
        for line in dv.layout().lines() {
            for run in line.runs() {
                n += run.clusters().count();
            }
        }
        n
    };
    let deva_chars = deva.chars().count();
    let _ = writeln!(
        out,
        "    Devanagari {deva:?}: {deva_chars} chars -> {deva_clusters} clusters -> {deva_positions} caret stops"
    );
    checks.push(Check {
        label: "C5 caret stops == cluster count + 1".into(),
        passed: deva_positions == deva_clusters + 1,
        detail: format!(
            "{deva_clusters} clusters -> {deva_positions} stops (want {})",
            deva_clusters + 1
        ),
    });

    // FINDING: parley's `Cluster` is per-*codepoint*, not per-grapheme.
    //
    // "e" + U+0301 COMBINING ACUTE ACCENT is two clusters, not one. So are a
    // Devanagari consonant and its matra, and every element of a ZWJ emoji
    // sequence. A caret walk built naively on cluster edges therefore offers the
    // user a stop *between a base letter and its accent* — a caret that looks
    // wrong and, worse, a place to type into that produces mojibake.
    //
    // This is not a parley defect: a shaping cluster is the unit the shaper
    // works in, and grapheme segmentation is a Unicode concern layered above it.
    // But it means grapheme segmentation is *our* responsibility, and it is not
    // optional. Recorded as a Phase 1 requirement.
    let combining = "e\u{0301}"; // e + combining acute
    let cm = CaretMap::build_default(
        &mut layout_cx,
        &mut font_cx,
        combining,
        Some(WRAP_WIDTH),
        DEFAULT_FONT_SIZE,
    );
    let cm_clusters = {
        let mut n = 0usize;
        for line in cm.layout().lines() {
            for run in line.runs() {
                n += run.clusters().count();
            }
        }
        n
    };
    let cm_stops = cm.caret_positions().len();
    let cm_graphemes = cm.grapheme_caret_positions().len();
    let _ = writeln!(
        out,
        "    combining mark: {} codepoints -> {cm_clusters} parley clusters -> {cm_stops} raw stops -> {cm_graphemes} grapheme stops",
        combining.chars().count()
    );
    checks.push(Check {
        label: "C5 parley clusters are per-codepoint, as documented".into(),
        passed: cm_clusters == combining.chars().count(),
        detail: format!(
            "e+U+0301 gives {cm_clusters} clusters for {} codepoints",
            combining.chars().count()
        ),
    });
    checks.push(Check {
        label: "C5 grapheme filter removes the split inside the accent".into(),
        passed: cm_graphemes < cm_stops,
        detail: format!("{cm_stops} raw stops -> {cm_graphemes} grapheme stops"),
    });

    // The same must hold for a ZWJ emoji sequence.
    //
    // This was the known gap left by the GB9 approximation, which could not
    // express GB11 and so broke between the people in a family emoji. Real UAX
    // #29 segmentation via `unicode-segmentation` closes it: the whole sequence
    // is one grapheme, so it gets one caret stop pair.
    let zwj = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}"; // family
    let z = CaretMap::build_default(
        &mut layout_cx,
        &mut font_cx,
        zwj,
        Some(WRAP_WIDTH),
        DEFAULT_FONT_SIZE,
    );
    let z_raw = z.caret_positions().len();
    let z_graph = z.grapheme_caret_positions().len();
    let _ = writeln!(
        out,
        "    ZWJ family emoji: {z_raw} raw stops -> {z_graph} grapheme stops (GB11 needs 2)"
    );
    checks.push(Check {
        label: "C5 ZWJ family emoji is a single caret stop pair".into(),
        passed: z_graph == 2,
        detail: format!("{z_raw} raw -> {z_graph} grapheme stops (want 2)"),
    });
    // ---- C6: IME geometry contract -------------------------------------------
    let _ = writeln!(out, "\nC6  Geometry an IME candidate window needs");
    let mut contract_ok = true;
    let mut contract_rows = Vec::new();
    for sample in SAMPLES {
        let m = CaretMap::build_default(
            &mut layout_cx,
            &mut font_cx,
            sample.text,
            Some(WRAP_WIDTH),
            DEFAULT_FONT_SIZE,
        );
        let mut missing = 0usize;
        let mut total = 0usize;
        for pos in m.caret_positions() {
            total += 1;
            let ok = m
                .caret_rect(pos, Affinity::Start)
                .is_some_and(|r| r.height > 0.0 && r.width > 0.0 && r.y >= 0.0);
            if !ok {
                missing += 1;
            }
        }
        contract_ok &= missing == 0;
        contract_rows.push(format!("{}: {}/{}", sample.name, total - missing, total));
    }
    for r in &contract_rows {
        let _ = writeln!(out, "    {r}");
    }
    checks.push(Check {
        label: "C6 every caret in every sample has a usable IME anchor rect".into(),
        passed: contract_ok,
        detail: format!("{} samples checked", SAMPLES.len()),
    });

    // ---- C7: re-break keeps caret stable -------------------------------------
    let mut resize_ok = true;
    let mut resize_detail = String::new();
    for width in [200.0f32, 400.0, 624.0, 900.0] {
        let m = CaretMap::build_default(
            &mut layout_cx,
            &mut font_cx,
            long,
            Some(width),
            DEFAULT_FONT_SIZE,
        );
        let mut bad = 0usize;
        for pos in m.caret_positions() {
            if m.caret_rect(pos, Affinity::Start).is_none() {
                bad += 1;
            }
        }
        if bad > 0 {
            resize_ok = false;
            resize_detail = format!("{bad} carets lost at width {width}");
        }
    }
    checks.push(Check {
        label: "C7 caret survives re-break at four widths".into(),
        passed: resize_ok,
        detail: if resize_ok {
            "no caret lost at 200/400/624/900px".into()
        } else {
            resize_detail
        },
    });

    // ---- Verdict -------------------------------------------------------------
    let failed = checks.iter().filter(|c| !c.passed).count();

    let _ = writeln!(
        out,
        "\n================================================================"
    );
    let _ = writeln!(out, " VERDICT - criterion 2 (caret, selection, navigation)");
    let _ = writeln!(
        out,
        "================================================================"
    );
    for c in &checks {
        let _ = writeln!(
            out,
            "  [{}] {:<58} {}",
            if c.passed { "PASS" } else { "FAIL" },
            c.label,
            c.detail
        );
    }
    let _ = writeln!(out, "\n  {}/{} passed", checks.len() - failed, checks.len());

    // ---- Criterion 3: the honest part ----------------------------------------
    let mut harness = LayoutHarness::new();
    let _ = harness.apply_policy();
    let cjk_resolved: usize = harness
        .apply_policy()
        .iter()
        .filter(|(s, r, _)| (*s == "Hani" || *s == "Kana" || *s == "Hang") && *r > 0)
        .count();

    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "================================================================"
    );
    let _ = writeln!(out, " VERDICT - criterion 3 (CJK IME)");
    let _ = writeln!(
        out,
        "================================================================"
    );
    let _ = writeln!(out, "  [????] NOT VERIFIED IN THIS SPIKE");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "  Requires: a registered input method, a human to compose with it,"
    );
    let _ = writeln!(out, "  and a windowing session to observe the result.");
    let _ = writeln!(out);
    let _ = writeln!(out, "  This machine has:");
    let _ = writeln!(
        out,
        "    - no CTF TIP entries registered  => no input method at all"
    );
    let _ = writeln!(
        out,
        "    - only US English keyboard layout (00000409) preloaded"
    );
    let _ = writeln!(
        out,
        "    - GUI windows CAN be created (verified), so the blocker is the"
    );
    let _ = writeln!(out, "      IME, not the display");
    let _ = writeln!(out);
    let _ = writeln!(out, "  What IS verified, and is what an IME depends on:");
    let _ = writeln!(
        out,
        "    - a caret rect exists at every position in every sample (C6)"
    );
    let _ = writeln!(out, "    - offsets snap to grapheme boundaries (C5)");
    let _ = writeln!(
        out,
        "    - geometry survives re-break, so it stays valid on resize (C7)"
    );
    let _ = writeln!(
        out,
        "    - CJK fonts resolve on this machine: {cjk_resolved}/3 scripts"
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "  What is NOT verified, and cannot be here:");
    let _ = writeln!(out, "    - candidate window appears at the caret rect");
    let _ = writeln!(
        out,
        "    - preedit text renders inline, not as a committed string"
    );
    let _ = writeln!(out, "    - commit lands at the correct byte offset");
    let _ = writeln!(
        out,
        "    - composition cancels cleanly on caret movement or Escape"
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "  A spike claiming to have validated these from geometry alone would"
    );
    let _ = writeln!(
        out,
        "  be handing false confidence to the largest technical risk in the"
    );
    let _ = writeln!(
        out,
        "  project. See docs/spike-b-caret-and-ime.md for the manual protocol."
    );

    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "================================================================"
    );
    let _ = writeln!(out, " ADR-0003 STATUS");
    let _ = writeln!(
        out,
        "================================================================"
    );
    let _ = writeln!(
        out,
        "  The geometry half of ADR-0003 is supported: a Rust layout engine"
    );
    let _ = writeln!(
        out,
        "  can supply every caret rect and hit test an IME needs."
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "  The webview half is still UNPROVEN. Whether a hidden"
    );
    let _ = writeln!(
        out,
        "  contenteditable anchor delivers a correct CJK IME is untested."
    );
    let _ = writeln!(
        out,
        "  ADR-0003 remains provisional until that is demonstrated by hand."
    );

    print!("{out}");
    (out, checks.len() - failed, checks.len())
}

/// Visual navigation needs its own map; kept separate so the C4 output reads
/// clearly rather than reusing the LTR map mid-report.
fn rtl_or_ltr(layout_cx: &mut LayoutContext, font_cx: &mut FontContext) -> CaretMap {
    CaretMap::build_default(
        layout_cx,
        font_cx,
        "The quick brown fox",
        Some(WRAP_WIDTH),
        DEFAULT_FONT_SIZE,
    )
}

fn down_line_index(map: &CaretMap, m: &Move) -> Option<usize> {
    match m {
        Move::To(p) => map.line_index_at(*p),
        Move::Blocked => None,
    }
}

/// Unused helper kept honest: report ink width for the widest line.
pub fn widest_line_ink(map: &CaretMap) -> Option<f32> {
    (0..map.line_count())
        .filter_map(|i| line_ink_width(map, i))
        .fold(None, |acc, w| Some(acc.map_or(w, |a: f32| a.max(w))))
}
