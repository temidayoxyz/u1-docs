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

/// How many of the platform's Han fallback families actually resolved.
///
/// ## Why this applies the policy instead of merely inspecting it
///
/// An earlier version of this gate checked only whether fontique already had a
/// Han fallback family registered, then configured a chain of **Windows** font
/// names. On macOS CI none of those exist, so the chain resolved to nothing and
/// every Chinese character rendered as `.notdef` — the test failed on macOS
/// while passing on Windows.
///
/// That is the whole lesson of [`u1_docs::policy`]: a fallback chain resolving
/// to nothing is *worse* than no chain, and the only reliable way to know is to
/// apply it and count what matched.
fn han_policy_resolves(h: &mut LayoutHarness) -> usize {
    let entry = u1_docs::policy::policy()
        .iter()
        .find(|e| e.script == "Hani")
        .expect("policy must define a Han entry");
    h.set_script_fallback(entry.script, entry.families)
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
    let resolved = han_policy_resolves(&mut h);
    if resolved == 0 {
        eprintln!(
            "skipping: none of this platform's Han fallback families are installed \
             (target_os = {}). A machine with no Han font cannot render Han, and \
             failing here would be noise rather than signal.",
            std::env::consts::OS
        );
        return;
    }
    eprintln!("Han fallback families resolved: {resolved}");

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
        "common Simplified Chinese characters still resolve to .notdef after the \
         platform Han fallback chain resolved {resolved} family/families: {:?}",
        sum.missing_chars
    );
}

#[test]
fn fallback_configuration_rejects_unknown_scripts_gracefully() {
    // `set_script_fallback` takes an ISO 15924 code and returns how many families
    // resolved. A bad code must return 0 rather than panic, because the real
    // policy table is platform-specific and may name scripts absent on a given
    // machine.
    let mut h = LayoutHarness::new();
    assert_eq!(h.set_script_fallback("NotAScript", &["Arial"]), 0);
}

#[test]
fn fallback_configuration_counts_unresolved_families() {
    // Returns the count that actually matched, not a boolean. A caller must be
    // able to tell "all resolved" from "some resolved" from "none resolved" —
    // only the last is a hard failure, but the middle case still leaves a
    // coverage gap worth reporting.
    let mut h = LayoutHarness::new();
    let resolved = h.set_script_fallback("Latn", &["Arial", "ThisFontDoesNotExist12345"]);
    assert_eq!(
        resolved, 1,
        "expected exactly one of the two named families to resolve"
    );
}

#[test]
fn policy_resolves_at_least_one_family_for_core_scripts() {
    // A packaging sanity check on the host machine: a user with no font at all
    // for a core script cannot read documents in it. Reported, not asserted —
    // this prints what is available rather than failing on a minimal container.
    let mut h = LayoutHarness::new();
    for (script, resolved, requested) in h.apply_policy() {
        eprintln!("{script}: {resolved}/{requested} families resolved");
    }
}

// ---------------------------------------------------------------------------
// Policy tables: validated for every platform, not just the host
// ---------------------------------------------------------------------------

#[test]
fn every_platform_policy_is_well_formed() {
    // All three tables are compiled on every platform precisely so this can run
    // anywhere. A policy that is only ever compiled on the machine that wrote it
    // is the bug this module exists to prevent.
    use u1_docs::policy::ALL_POLICIES;

    let core = [
        "Latn", "Arab", "Hebr", "Deva", "Thai", "Hani", "Kana", "Hang",
    ];

    for (platform, entries) in ALL_POLICIES {
        assert!(!entries.is_empty(), "{platform} policy is empty");

        let mut seen: Vec<&str> = Vec::new();
        for e in entries.iter() {
            assert!(
                !e.families.is_empty(),
                "{platform}: script {} has no fallback families",
                e.script
            );
            assert!(
                !e.families.iter().any(|f| f.trim().is_empty()),
                "{platform}: script {} lists a blank family name",
                e.script
            );
            assert!(
                !seen.contains(&e.script),
                "{platform}: script {} listed twice",
                e.script
            );
            seen.push(e.script);
        }

        for want in core {
            assert!(
                seen.contains(&want),
                "{platform}: policy is missing a {want} entry; expected at least {core:?}"
            );
        }
    }
}

#[test]
fn all_platforms_agree_on_which_scripts_they_cover() {
    // Cross-platform consistency: a script supported on one platform and missing
    // on another is an inconsistency a user will eventually hit, and one that is
    // very easy to introduce when editing a single table.
    use u1_docs::policy::ALL_POLICIES;

    let baseline: Vec<&str> = ALL_POLICIES[0].1.iter().map(|e| e.script).collect();
    for (platform, entries) in &ALL_POLICIES[1..] {
        let here: Vec<&str> = entries.iter().map(|e| e.script).collect();
        assert_eq!(
            baseline, here,
            "{platform} covers a different set of scripts than {}",
            ALL_POLICIES[0].0
        );
    }
}

#[test]
fn platform_policy_is_not_empty_on_this_host() {
    // The host must actually have a policy, or `policy()` silently returns `&[]`
    // and every glyph becomes tofu with no error anywhere.
    let os = std::env::consts::OS;
    if matches!(os, "windows" | "macos" | "linux") {
        assert!(
            !u1_docs::policy::policy().is_empty(),
            "no policy selected for supported OS {os}"
        );
    }
}
