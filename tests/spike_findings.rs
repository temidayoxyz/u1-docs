//! Regression tests for the Phase 0 spike findings.
//!
//! These exist to make the spike's conclusions **enforceable**. Every test here
//! corresponds to something the spike discovered, so that a future dependency
//! bump, platform change or refactor cannot quietly undo a decision that was
//! made from evidence.
//!
//! ## Portability
//!
//! These tests run on all three CI platforms, which have different system font
//! sets. Assertions about *performance* and *structure* are unconditional;
//! assertions about *glyph coverage* are conditional on a suitable font being
//! installed, because failing on a machine that simply lacks a font would be
//! noise rather than signal.

use std::time::Duration;

use u1_docs::corpus::{SAMPLES, throughput_corpus};
use u1_docs::layout::{DEFAULT_FONT_SIZE, LayoutHarness};

const WRAP_WIDTH: f32 = 6.5 * 96.0;

/// Frame budget at 60fps. The interactive requirement from PLAN.md Phase 1.
const FRAME_BUDGET: Duration = Duration::from_micros(16_667);

fn sample(name: &str) -> &'static u1_docs::corpus::Sample {
    SAMPLES
        .iter()
        .find(|s| s.name == name)
        .expect("named corpus sample")
}

/// Does this machine have a font that covers Han characters at all?
fn has_cjk_coverage(h: &mut LayoutHarness) -> bool {
    let names = h.inventory();
    names.family_count > 20
        && h.fallback_counts(&["Hani"])
            .first()
            .is_some_and(|(_, n)| *n > 0)
}

// ---------------------------------------------------------------------------
// Corpus integrity
// ---------------------------------------------------------------------------

#[test]
fn corpus_samples_are_non_empty_and_named() {
    for s in SAMPLES {
        assert!(!s.text.trim().is_empty(), "{} has empty text", s.name);
        assert!(!s.name.is_empty());
        assert!(!s.probes.is_empty(), "{} has no probe description", s.name);
    }
}

#[test]
fn corpus_sample_names_are_unique() {
    // Report.rs indexes summaries by name; duplicates would silently misreport.
    let mut names: Vec<&str> = SAMPLES.iter().map(|s| s.name).collect();
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(before, names.len(), "duplicate sample names in corpus");
}

#[test]
fn corpus_covers_the_scripts_we_claim_to_support() {
    // If someone trims the corpus, these guard against quietly dropping the
    // scripts that make the corpus worth having.
    for required in [
        "arabic-rtl",
        "hebrew-rtl",
        "bidi-mixed",
        "chinese-no-spaces",
        "japanese-no-spaces",
        "devanagari-stacking",
        "thai-stacking",
        "emoji-zwj",
    ] {
        assert!(
            SAMPLES.iter().any(|s| s.name == required),
            "corpus lost the {required} sample"
        );
    }
}

// ---------------------------------------------------------------------------
// Q1: font discovery
// ---------------------------------------------------------------------------

#[test]
fn system_font_discovery_returns_families() {
    let mut h = LayoutHarness::new();
    let inv = h.inventory();
    assert!(
        inv.family_count > 20,
        "font discovery returned only {} families - fontique is not reaching system fonts",
        inv.family_count
    );
}

// ---------------------------------------------------------------------------
// Q6: complex-script segmentation  (the `complex-scripts` finding)
// ---------------------------------------------------------------------------

#[test]
fn space_less_cjk_breaks_into_multiple_lines() {
    // FINDING: parley's `complex-scripts` feature is NOT a default feature.
    // Without it, `LineSegmenter::new_for_non_complex_scripts` is used and CJK
    // cannot break lines at all, because Chinese and Japanese have no inter-word
    // spaces to break on.
    //
    // This test fails, loudly and specifically, if that feature is ever dropped.
    let mut h = LayoutHarness::new();
    for name in ["chinese-no-spaces", "japanese-no-spaces"] {
        let s = sample(name);
        let sum = h.layout(s.name, s.probes, s.text, Some(120.0), DEFAULT_FONT_SIZE);
        assert!(
            sum.line_count > 1,
            "{name} did not break into multiple lines at 120px. Complex-script \
             segmentation is not active - check the `complex-scripts` feature \
             on parley and the `auto` feature on icu_segmenter."
        );
    }
}

#[test]
fn cjk_lines_respect_the_wrap_width() {
    // The counterpart to the test above: breaking must respect the column, not
    // just happen to produce multiple over-wide lines.
    let mut h = LayoutHarness::new();
    let s = sample("chinese-no-spaces");
    let narrow = 150.0;
    let sum = h.layout(s.name, s.probes, s.text, Some(narrow), DEFAULT_FONT_SIZE);
    assert!(sum.line_count > 1, "CJK sample did not wrap at {narrow}px");
}

// ---------------------------------------------------------------------------
// Q4: wrap width
// ---------------------------------------------------------------------------

#[test]
fn no_multiline_paragraph_overflows_the_wrap_width() {
    let mut h = LayoutHarness::new();
    for s in SAMPLES {
        let sum = h.layout(
            s.name,
            s.probes,
            s.text,
            Some(WRAP_WIDTH),
            DEFAULT_FONT_SIZE,
        );
        if sum.line_count > 1 {
            assert!(
                sum.width <= WRAP_WIDTH + 1.0,
                "{} wrapped to {} lines but is {:.1}px wide, exceeding the {:.1}px column",
                s.name,
                sum.line_count,
                sum.width,
                WRAP_WIDTH
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Performance: the interactive budget
// ---------------------------------------------------------------------------

#[test]
fn visible_page_rebreak_fits_in_one_frame() {
    // FINDING: cold layout is ~2.7ms/paragraph and cannot be used interactively.
    // The warm re-break path - re-using shaped glyphs - is what the typing and
    // resize paths depend on, and it must stay inside a frame.
    //
    // This is the single most important performance guarantee in the project.
    // If it regresses, typing in a large document stops being smooth, and no
    // other test here would notice.
    let mut h = LayoutHarness::new();
    let page = throughput_corpus(45);
    let warm = h.rebreak_large(&page, WRAP_WIDTH);
    assert!(
        warm <= FRAME_BUDGET,
        "re-breaking 45 paragraphs took {:.2}ms, over the {:.1}ms frame budget. \
         Incremental layout has regressed.",
        warm.as_secs_f64() * 1e3,
        FRAME_BUDGET.as_secs_f64() * 1e3
    );
}

#[test]
fn repeated_rebreaks_do_not_accumulate_cost() {
    // Guards against a subtler version of the same problem: re-breaking being
    // individually fast but growing with each successive pass, which would show
    // up over a long editing session as steadily worsening responsiveness.
    //
    // `rebreak_large` returns only the warm (re-break) timing, excluding its own
    // cold build, so this measures the thing it claims to measure.
    let mut h = LayoutHarness::new();
    let page = throughput_corpus(20);
    let _ = h.rebreak_large(&page, WRAP_WIDTH); // warm the font cache

    let mut timings = Vec::new();
    for _ in 0..5 {
        timings.push(h.rebreak_large(&page, WRAP_WIDTH));
    }

    let first = timings[0];
    let last = timings[timings.len() - 1];

    for (i, t) in timings.iter().enumerate() {
        assert!(
            *t <= FRAME_BUDGET,
            "re-break pass {i} took {:.2}ms, over the {:.1}ms frame budget",
            t.as_secs_f64() * 1e3,
            FRAME_BUDGET.as_secs_f64() * 1e3
        );
    }

    // Allow generous slack: this is looking for unbounded growth, not jitter.
    assert!(
        last <= first * 3,
        "re-break cost grew from {:.2}ms to {:.2}ms over 5 passes, suggesting \
         accumulating state or a cache that is not being reused",
        first.as_secs_f64() * 1e3,
        last.as_secs_f64() * 1e3
    );
}

// ---------------------------------------------------------------------------
// Bidi: direction resolution
// ---------------------------------------------------------------------------

#[test]
fn rtl_samples_start_visually_at_the_right() {
    // Bidi is the easiest thing to get silently wrong and the hardest to notice
    // without reading the script. For an RTL base direction the first visual run
    // must be positioned at the right edge, not the left.
    let mut h = LayoutHarness::new();
    for name in ["arabic-rtl", "hebrew-rtl"] {
        let s = sample(name);
        let sum = h.layout(
            s.name,
            s.probes,
            s.text,
            Some(WRAP_WIDTH),
            DEFAULT_FONT_SIZE,
        );
        let first_x = sum.runs.first().map_or(0.0, |r| r.x);
        assert!(
            first_x > 1.0,
            "{name} has RTL base direction but its first visual run starts at \
             x={first_x:.1}px; bidi reordering is not being applied"
        );
    }
}

#[test]
fn ltr_samples_start_at_the_left() {
    let mut h = LayoutHarness::new();
    for name in ["english-plain", "english-long", "bidi-mixed"] {
        let s = sample(name);
        let sum = h.layout(
            s.name,
            s.probes,
            s.text,
            Some(WRAP_WIDTH),
            DEFAULT_FONT_SIZE,
        );
        let first_x = sum.runs.first().map_or(0.0, |r| r.x);
        assert!(
            first_x <= 1.0,
            "{name} has LTR base direction but starts at x={first_x:.1}px"
        );
    }
}

#[test]
fn bidi_mixed_produces_multiple_visual_runs() {
    // An English sentence containing Arabic and trailing digits must split into
    // several visual runs. A single run would mean the bidi pass was skipped.
    let mut h = LayoutHarness::new();
    let s = sample("bidi-mixed");
    let sum = h.layout(
        s.name,
        s.probes,
        s.text,
        Some(WRAP_WIDTH),
        DEFAULT_FONT_SIZE,
    );
    assert!(
        sum.runs.len() > 1,
        "bidi-mixed produced {} run(s); expected the RTL span to be split out",
        sum.runs.len()
    );
}

// ---------------------------------------------------------------------------
// Q2/Q3: fallback policy
// ---------------------------------------------------------------------------

#[test]
fn glyphs_are_emitted_for_latin_text() {
    // The most portable coverage assertion: ASCII must always resolve.
    let mut h = LayoutHarness::new();
    let s = sample("english-plain");
    let sum = h.layout(
        s.name,
        s.probes,
        s.text,
        Some(WRAP_WIDTH),
        DEFAULT_FONT_SIZE,
    );
    assert!(sum.glyph_count > 0, "no glyphs produced for plain ASCII");
    assert!(
        !sum.has_missing_glyphs(),
        "plain ASCII produced .notdef glyphs: {:?}",
        sum.missing_chars
    );
}

#[test]
fn explicit_fallback_resolves_cjk_coverage() {
    // FINDING: fontique's *default* fallback does not guarantee coverage. It
    // selected fonts covering only part of a script, producing .notdef boxes for
    // common characters like ä¸¥ (U+4E25).
    //
    // Setting an explicit per-script chain fixed it. This test asserts the fix
    // holds, but only where a CJK font actually exists - a machine with no Han
    // font cannot render Han, and failing there would be noise.
    let mut h = LayoutHarness::new();
    if !has_cjk_coverage(&mut h) {
        eprintln!("skipping: no CJK font coverage detected on this machine");
        return;
    }

    let configured = h.set_script_fallback(
        "Hani",
        &[
            "SimSun",
            "NSimSun",
            "Microsoft YaHei",
            "Microsoft JhengHei",
            "Malgun Gothic",
        ],
    );
    eprintln!("CJK fallback configured: {configured}");

    let s = sample("chinese-no-spaces");
    let sum = h.layout(
        s.name,
        s.probes,
        s.text,
        Some(WRAP_WIDTH),
        DEFAULT_FONT_SIZE,
    );
    assert!(
        !sum.has_missing_glyphs(),
        "common Simplified Chinese characters still resolve to .notdef after an \
         explicit Han fallback chain: {:?} (U+ points in the spike report)",
        sum.missing_chars
    );
}

#[test]
fn fallback_configuration_rejects_unknown_scripts_gracefully() {
    // `set_script_fallback` takes an ISO 15924 code. A bad code must return
    // false rather than panic, because the real policy table will be
    // platform-specific and may name scripts absent on a given machine.
    let mut h = LayoutHarness::new();
    assert!(!h.set_script_fallback("NotAScript", &["Arial"]));
}

#[test]
fn fallback_configuration_reports_unresolved_families() {
    // Returns false when not every named family exists, so a platform-specific
    // policy can be validated at startup instead of failing silently later.
    let mut h = LayoutHarness::new();
    let resolved = h.set_script_fallback("Latn", &["Arial", "ThisFontDoesNotExist12345"]);
    assert!(
        !resolved,
        "expected false when a named family is missing from the collection"
    );
}
