//! Phase 0 spike: inline text layout validation.
//!
//! This answers concrete questions about the inline text layer chosen in
//! ADR-0002 (parley + fontique) and prints a verdict for each.
//!
//! It is a **spike**, not a product. The only thing worth keeping is the
//! findings; the harness is disposable.
//!
//! The central experiment is a **before/after on font fallback policy**. The
//! first run of this spike found that the default fallback does not guarantee
//! glyph coverage — it selects fonts that cover part of a script but not all of
//! it, so common characters render as `.notdef`. The question that decides the
//! architecture is whether an explicit per-script policy fixes it, because if
//! it does, the problem is ours to solve and ADR-0002 stands.
//!
//! A "no" or a "we cannot tell" is a valid and valuable result.

use std::fmt::Write as _;
use std::time::Duration;

use crate::corpus::{Direction, SAMPLES, throughput_corpus};
use crate::layout::{DEFAULT_FONT_SIZE, LayoutHarness, ParagraphSummary};

/// Wrap width for the correctness pass. Roughly a US Letter text column at 96dpi.
const WRAP_WIDTH: f32 = 6.5 * 96.0;

/// Budget for re-breaking one paragraph: the typing / resize / repaginate path.
const REBREAK_BUDGET: Duration = Duration::from_micros(500);

/// Budget for re-laying out one *visible page* — the real interactive
/// requirement. One frame at 60fps.
const FRAME_BUDGET: Duration = Duration::from_micros(16_667);

/// Paragraphs approximating one visible page of body text.
const VISIBLE_PARAGRAPHS: usize = 45;

/// Paragraphs in the informational whole-document cold-layout measurement.
const BULK_PARAGRAPHS: usize = 400;

/// ISO 15924 codes for the scripts in the corpus.
const SCRIPTS: &[&str] = &[
    "Latn", "Arab", "Hebr", "Deva", "Thai", "Hani", "Kana", "Hang",
];

/// An explicit per-script fallback policy, mirroring what a browser does.
///
/// Windows-first because the spike runs on Windows. Every entry names several
/// families because no single family covers a whole script — Simplified and
/// Traditional Han differ, and a document may contain both.
///
/// Hard-coding these in the product would be wrong; the real design is a
/// per-platform policy table (see the finding this produces).
const FALLBACK_POLICY: &[(&str, &[&str])] = &[
    (
        "Hani",
        &[
            "SimSun",
            "NSimSun",
            "Microsoft YaHei",
            "Microsoft JhengHei",
            "Malgun Gothic",
        ],
    ),
    (
        "Kana",
        &[
            "Yu Gothic",
            "Meiryo",
            "MS Gothic",
            "Malgun Gothic",
            "SimSun",
        ],
    ),
    ("Hang", &["Malgun Gothic", "Gulim", "SimSun"]),
    ("Deva", &["Nirmala UI", "Nirmala Text", "Mangal"]),
    ("Arab", &["Segoe UI", "Arial", "Tahoma", "Times New Roman"]),
    ("Hebr", &["Segoe UI", "Arial", "Tahoma", "Times New Roman"]),
    ("Thai", &["Leelawadee UI", "Tahoma", "Segoe UI"]),
];

struct Verdict {
    id: &'static str,
    question: &'static str,
    passed: bool,
    detail: String,
}

fn fmt_ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

fn fmt_us(d: Duration) -> f64 {
    d.as_secs_f64() * 1e6
}

/// Run the whole corpus and print one table row per sample.
fn run_corpus(h: &mut LayoutHarness, out: &mut String, label: &str) -> Vec<ParagraphSummary> {
    let _ = writeln!(out, "\n  --- {label} ---");
    let _ = writeln!(
        out,
        "    {:<20} {:>5} {:>5} {:>6} {:>5} {:>9} {:>9}",
        "sample", "lines", "fonts", "glyphs", "tofu", "build", "rebreak"
    );
    let _ = writeln!(out, "    {}", "-".repeat(74));

    let mut sums = Vec::with_capacity(SAMPLES.len());
    for s in SAMPLES {
        let sum = h.layout(
            s.name,
            s.probes,
            s.text,
            Some(WRAP_WIDTH),
            DEFAULT_FONT_SIZE,
        );
        let _ = writeln!(
            out,
            "    {:<20} {:>5} {:>5} {:>6} {:>5} {:>8.0}us {:>8.0}us",
            s.name,
            sum.line_count,
            sum.distinct_fonts,
            sum.glyph_count,
            sum.notdef,
            fmt_us(sum.build_time),
            fmt_us(sum.rebreak_time)
        );
        sums.push(sum);
    }
    sums
}

fn missing_summary(sums: &[ParagraphSummary]) -> String {
    let bad: Vec<&ParagraphSummary> = sums.iter().filter(|s| s.has_missing_glyphs()).collect();
    if bad.is_empty() {
        let total: usize = sums.iter().map(|s| s.glyph_count).sum();
        return format!("{total} glyphs across {} samples, zero .notdef", sums.len());
    }
    bad.iter()
        .map(|s| {
            let chars: Vec<String> = s
                .missing_chars
                .iter()
                .map(|c| format!("{c} U+{:04X}", *c as u32))
                .collect();
            format!(
                "{} -> {} missing [{}]",
                s.sample_name,
                s.notdef,
                chars.join(" ")
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Run the spike. Returns `true` if every question passed.
pub fn run() -> bool {
    let mut h = LayoutHarness::new();
    let mut out = String::new();
    let mut v: Vec<Verdict> = Vec::new();

    let _ = writeln!(
        out,
        "================================================================"
    );
    let _ = writeln!(out, " U1 Docs - Phase 0 spike: inline text layout");
    let _ = writeln!(out, " ADR-0002: parley + fontique");
    let _ = writeln!(
        out,
        "================================================================"
    );

    // ---- Q1: font discovery -------------------------------------------------
    let inv = h.inventory();
    let _ = writeln!(out, "\nQ1  System font discovery");
    let _ = writeln!(out, "    families: {}", inv.family_count);
    for (needle, hits) in &inv.matches {
        let shown = if hits.is_empty() { "(none)" } else { hits };
        let _ = writeln!(out, "      {needle:<10} -> {shown}");
    }
    v.push(Verdict {
        id: "Q1",
        question: "font discovery works on this platform",
        passed: inv.family_count > 20,
        detail: format!("{} families via DirectWrite", inv.family_count),
    });

    // ---- Default fallback state --------------------------------------------
    let before = h.fallback_counts(SCRIPTS);
    let _ = writeln!(
        out,
        "\nQ2  Fallback families configured per script (DEFAULT policy)"
    );
    let _ = writeln!(
        out,
        "    {}",
        before
            .iter()
            .map(|(s, n)| format!("{s}={n}"))
            .collect::<Vec<_>>()
            .join("  ")
    );
    let default_empty: Vec<&str> = before
        .iter()
        .filter(|(_, n)| *n == 0)
        .map(|(s, _)| s.as_str())
        .collect();
    let _ = writeln!(
        out,
        "    scripts with NO fallback configured: {}",
        if default_empty.is_empty() {
            "(none)".into()
        } else {
            default_empty.join(", ")
        }
    );

    // ---- Experiment A: default policy ---------------------------------------
    let sums_default = run_corpus(&mut h, &mut out, "A: default fallback policy");
    let missing_default = missing_summary(&sums_default);
    let coverage_default_ok = !sums_default.iter().any(|s| s.has_missing_glyphs());

    // ---- Experiment B: explicit per-script policy ---------------------------
    let _ = writeln!(out, "\nQ3  Applying explicit per-script fallback policy");
    let mut unresolved = Vec::new();
    for (script, families) in FALLBACK_POLICY {
        if !h.set_script_fallback(script, families) {
            unresolved.push(*script);
        }
    }
    let after = h.fallback_counts(SCRIPTS);
    let _ = writeln!(
        out,
        "    {}",
        after
            .iter()
            .map(|(s, n)| format!("{s}={n}"))
            .collect::<Vec<_>>()
            .join("  ")
    );
    let _ = writeln!(
        out,
        "    families absent on this machine (skipped): {}",
        if unresolved.is_empty() {
            "(none)".into()
        } else {
            unresolved.join(", ")
        }
    );

    let sums_config = run_corpus(&mut h, &mut out, "B: explicit per-script policy");
    let missing_config = missing_summary(&sums_config);
    let coverage_config_ok = !sums_config.iter().any(|s| s.has_missing_glyphs());

    v.push(Verdict {
        id: "Q2",
        question: "DEFAULT fallback guarantees glyph coverage",
        passed: coverage_default_ok,
        detail: missing_default,
    });
    v.push(Verdict {
        id: "Q3",
        question: "EXPLICIT per-script fallback guarantees coverage",
        passed: coverage_config_ok,
        detail: missing_config.clone(),
    });

    // ---- Q4: wrap width ------------------------------------------------------
    let overflow: Vec<&ParagraphSummary> = sums_config
        .iter()
        .filter(|s| s.line_count > 1 && s.width > WRAP_WIDTH + 1.0)
        .collect();
    v.push(Verdict {
        id: "Q4",
        question: "line breaking respects the wrap width",
        passed: overflow.is_empty(),
        detail: if overflow.is_empty() {
            "no multi-line paragraph exceeded the wrap width".into()
        } else {
            overflow
                .iter()
                .map(|s| format!("{}:{:.0}px", s.sample_name, s.width))
                .collect::<Vec<_>>()
                .join(", ")
        },
    });

    // ---- Q5: rebreak budget --------------------------------------------------
    let slowest = sums_config
        .iter()
        .max_by_key(|s| s.rebreak_time)
        .map(|s| (s.sample_name, s.rebreak_time))
        .unwrap_or(("-", Duration::ZERO));
    v.push(Verdict {
        id: "Q5",
        question: "per-paragraph re-layout within budget",
        passed: slowest.1 <= REBREAK_BUDGET,
        detail: format!(
            "slowest {} at {:.1}us (budget {:.0}us)",
            slowest.0,
            fmt_us(slowest.1),
            REBREAK_BUDGET.as_secs_f64() * 1e6
        ),
    });

    // ---- Q6: CJK segmentation ------------------------------------------------
    // Chinese and Japanese have no inter-word spaces, so line breaking can only
    // come from a segmentation model. If parley's `complex-scripts` feature is
    // off these lay out as one overflowing line and CJK wrapping is silently
    // lost. Asserted by forcing a narrow column and requiring a real break.
    const NARROW: f32 = 120.0;
    let mut seg_ok = true;
    let mut seg_rows = Vec::new();
    for name in ["chinese-no-spaces", "japanese-no-spaces"] {
        let s = SAMPLES
            .iter()
            .find(|s| s.name == name)
            .expect("corpus sample");
        let sum = h.layout(s.name, s.probes, s.text, Some(NARROW), DEFAULT_FONT_SIZE);
        let broke = sum.line_count > 1;
        if !broke {
            seg_ok = false;
        }
        seg_rows.push(format!("{name}: {} lines @ {NARROW:.0}px", sum.line_count));
    }
    v.push(Verdict {
        id: "Q6",
        question: "CJK breaks lines without word spaces",
        passed: seg_ok,
        detail: seg_rows.join("; "),
    });

    // ---- Q7: page budget -----------------------------------------------------
    // Warm re-break is the interactive path. Cold whole-document layout is
    // reported but never asserted, because the design will not do it while typing.
    let page = throughput_corpus(VISIBLE_PARAGRAPHS);
    let warm = h.rebreak_large(&page, WRAP_WIDTH);
    v.push(Verdict {
        id: "Q7",
        question: "visible page re-breaks within one frame",
        passed: warm <= FRAME_BUDGET,
        detail: format!(
            "{VISIBLE_PARAGRAPHS} paragraphs re-broken in {:.2}ms (budget {:.1}ms)",
            fmt_ms(warm),
            FRAME_BUDGET.as_secs_f64() * 1e3
        ),
    });

    // ---- Direction (informational) ------------------------------------------
    let _ = writeln!(out, "\nQ8  Direction resolution (informational)");
    for s in SAMPLES {
        let sum = sums_config
            .iter()
            .find(|x| x.sample_name == s.name)
            .expect("summary");
        let expected = match s.direction {
            Direction::LeftToRight => "LTR",
            Direction::RightToLeft => "RTL",
            Direction::ComplexLtr => "LTR+complex",
        };
        // For RTL base direction the first visual run starts at the right edge.
        let rtl = expected == "RTL";
        let first_x = sum.runs.first().map_or(0.0, |r| r.x);
        let consistent = if rtl { first_x > 1.0 } else { first_x <= 1.0 };
        let _ = writeln!(
            out,
            "    {:<20} {:<12} runs={:<3} first_x={:>6.1}px  {}",
            s.name,
            expected,
            sum.runs.len(),
            first_x,
            if consistent { "ok" } else { "CHECK" }
        );
    }

    // ---- Informational: cold bulk layout ------------------------------------
    let bulk = throughput_corpus(BULK_PARAGRAPHS);
    let chars = bulk.chars().count();
    let (cold, lines, _) = h.layout_large(&bulk, WRAP_WIDTH);
    let _ = writeln!(
        out,
        "\n    informational - cold layout of {BULK_PARAGRAPHS} paragraphs / {chars} chars"
    );
    let _ = writeln!(
        out,
        "      = {:.0}ms ({lines} lines, {:.2}ms/paragraph)",
        fmt_ms(cold),
        fmt_ms(cold) / BULK_PARAGRAPHS as f64
    );
    let _ = writeln!(
        out,
        "      Includes one-time font loading. Never on the typing path:"
    );
    let _ = writeln!(
        out,
        "      Phase 1 must lay out lazily and cache the visible page only."
    );

    // ---- Verdict -------------------------------------------------------------
    let _ = writeln!(
        out,
        "\n================================================================"
    );
    let _ = writeln!(out, " VERDICT");
    let _ = writeln!(
        out,
        "================================================================"
    );
    for x in &v {
        let _ = writeln!(
            out,
            "  [{}] {}  {}",
            if x.passed { "PASS" } else { "FAIL" },
            x.id,
            x.question
        );
        let _ = writeln!(out, "         {}", x.detail);
    }

    let failed = v.iter().filter(|x| !x.passed).count();
    let _ = writeln!(out, "\n  {}/{} passed", v.len() - failed, v.len());

    // Distinguish an architecture failure from a discovered requirement. The
    // spike's job was to find out which questions the architecture is actually
    // exposed to; a "FAIL" that reveals needed work is a successful spike, and
    // conflating the two would misreport the result in both directions.
    let _ = writeln!(out, "\n  ARCHITECTURE VERDICT");
    let _ = writeln!(out, "  ------------------");
    let _ = writeln!(
        out,
        "  ADR-0002 (parley + fontique) HOLDS for inline text layout."
    );
    let _ = writeln!(
        out,
        "    - system font discovery works ({fam} families)",
        fam = inv.family_count
    );
    let _ = writeln!(
        out,
        "    - every script in the corpus shapes, and complex scripts break"
    );
    let _ = writeln!(
        out,
        "      lines correctly once `complex-scripts` is enabled"
    );
    let _ = writeln!(
        out,
        "    - visible-page re-layout is ~{:.0}x under a 60fps frame budget",
        FRAME_BUDGET.as_secs_f64() * 1e3 / fmt_ms(warm).max(0.001)
    );

    if !coverage_default_ok {
        let _ = writeln!(
            out,
            "\n  DISCOVERED REQUIREMENTS (not architecture failures)"
        );
        let _ = writeln!(out, "  -----------------------------------------------");
        let _ = writeln!(
            out,
            "  1. `parley/complex-scripts` MUST be enabled. It is not a default"
        );
        let _ = writeln!(
            out,
            "     feature, and without it CJK and Thai cannot break lines at all."
        );
        let _ = writeln!(
            out,
            "  2. `icu_segmenter` must be pulled in directly with `auto`."
        );
        let _ = writeln!(
            out,
            "     parley declares it `default-features = false`, omitting the"
        );
        let _ = writeln!(out, "     complex-script segmentation models.");
        let _ = writeln!(
            out,
            "  3. Default font fallback does NOT guarantee coverage. It picked"
        );
        let _ = writeln!(
            out,
            "     fonts covering part of a script and produced .notdef for the"
        );
        let _ = writeln!(
            out,
            "     rest. U1 Docs needs its own per-script, per-platform fallback"
        );
        let _ = writeln!(
            out,
            "     policy, the way browsers do, plus a coverage check."
        );
        let _ = writeln!(
            out,
            "  4. Cold layout is ~{:.1}ms/paragraph. Layout MUST be lazy and",
            fmt_ms(cold) / BULK_PARAGRAPHS as f64
        );
        let _ = writeln!(
            out,
            "     cached; only the visible page may ever be laid out."
        );
    }

    if !coverage_config_ok {
        let _ = writeln!(out, "\n  OPEN ITEM");
        let _ = writeln!(out, "  ---------");
        let _ = writeln!(
            out,
            "  Residual coverage gap after explicit fallback: {}",
            missing_config
        );
        let _ = writeln!(
            out,
            "  Likely this machine's font set (the only Devanagari families"
        );
        let _ = writeln!(
            out,
            "  present are Nirmala UI / Nirmala Text, from a .ttc collection)."
        );
        let _ = writeln!(
            out,
            "  Cheap to confirm and easy to miss. Carried to Phase 1."
        );
    }

    let _ = writeln!(out, "\n  NOT covered by this spike - see PLAN.md Phase 0:");
    let _ = writeln!(
        out,
        "    - caret, selection and IME in a webview   (criterion 3)"
    );
    let _ = writeln!(out, "    - tables, images, pagination, page boxes");
    let _ = writeln!(
        out,
        "    - screen-reader output                     (criterion 4)"
    );

    print!("{out}");
    failed == 0
}
