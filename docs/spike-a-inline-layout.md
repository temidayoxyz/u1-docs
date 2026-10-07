# Spike A — inline text layout

**Phase 0, criterion 1.** Validates the inline text layer chosen in
[ADR-0002](https://github.com/temidayoxyz/unsoftone/blob/main/docs/adr/0002-text-layout-parley.md)
before a page layout engine is built on top of it.

**Status: complete. ADR-0002 holds.**

Machine: Windows, x86_64-pc-windows-msvc, rustc 1.97.0, parley 0.11.1,
fontique 0.11.1, 212 system font families.

Reproduce with `cargo run`. All conclusions below are enforced by
`tests/spike_findings.rs`.

---

## What this did and did not test

**Tested:** inline layout — shaping, line breaking, bidi reordering, alignment,
font discovery and fallback, against a 12-sample multilingual corpus.

**Not tested:** the block/flow/page layer, tables, images, pagination, the caret,
selection, IME, or screen-reader output. Those are criteria 2–4 and remain open.

This is the layer `parley` provides. The layer above it — paragraphs, tables,
floats, page breaking, widows and orphans — **does not exist yet and is the
largest single engineering item in U1 Docs.** ADR-0002 explicitly stops at the
inline layer, so its risk is bounded by what is measured here.

---

## Results

| # | Question | Result |
| - | -------- | ------ |
| Q1 | Does fontique enumerate system fonts? | **PASS** — 212 families via DirectWrite |
| Q2 | Does *default* fallback guarantee coverage? | **FAIL** — 6 `.notdef`, all common characters |
| Q3 | Does *explicit* per-script fallback guarantee coverage? | **FAIL** — 1 residual `.notdef` |
| Q4 | Does line breaking respect the wrap width? | **PASS** |
| Q5 | Is per-paragraph re-layout within budget? | **PASS** — worst case 54µs vs 500µs |
| Q6 | Do CJK scripts break lines without word spaces? | **PASS** — 5 and 4 lines at 120px |
| Q7 | Does a visible page re-break within one frame? | **PASS** — 2.66ms vs 16.7ms |

### Bidi is correct

Both RTL samples start their first visual run at the right edge (Arabic 418.6px,
Hebrew 410.0px) while all LTR samples start at 0.0px. The mixed-direction sample
splits into 4 visual runs, so the bidi pass is genuinely running rather than
being skipped for text it cannot handle.

---

## Findings

### 1. `parley/complex-scripts` is mandatory and is not a default feature

Without it, parley constructs its segmenters as
`LineSegmenter::new_for_non_complex_scripts(...)` and every CJK and Thai layout
fails with:

```
ICU4X data error: No segmentation model for complex script: Chinese/Japanese
```

Chinese and Japanese have **no inter-word spaces**, so line breaking can only
come from a segmentation model. Without this feature, U1 Docs would silently
lose the ability to wrap CJK text at all — a whole class of documents rendered as
single over-wide lines.

### 2. `icu_segmenter` must be a direct dependency

parley declares `complex-scripts = []` — an *empty* feature — and depends on
`icu_segmenter` with `default-features = false`, which omits `auto`/`lstm`, the
models that actually segment complex scripts.

Enabling parley's feature alone is therefore **not sufficient**. Cargo unifies
features across the dependency graph, so `icu_segmenter` is declared directly:

```toml
parley = { version = "0.11.1", features = ["complex-scripts"] }
icu_segmenter = { version = "2.3", features = ["compiled_data", "auto"] }
```

This indirection is load-bearing and must not be "simplified" away.

### 3. Default font fallback does not guarantee coverage

This is the finding most likely to have shipped as a silent bug.

With the default policy, `chinese-no-spaces` produced 5 `.notdef` glyphs for
**严 U+4E25, 们 U+4EEC, 赋 U+8D4B, 应 U+5E94, 对 U+5BF9** — ordinary
Simplified Chinese characters, present in essentially any real document. The
fallback mechanism selected a font that covered *part* of the script and nothing
further, and nothing reported the gap.

Setting an explicit per-script chain (`Hani → SimSun, NSimSun, Microsoft YaHei,
Microsoft JhengHei, Malgun Gothic`) took that sample from **5 missing to 0**.

**Conclusion:** U1 Docs needs its own per-script, per-platform fallback policy,
the way browsers do, rather than trusting the system default. And it needs a
coverage check — a fallback chain that silently returns the wrong font is worse
than no fallback, because nothing tells the user.

Note this is also the shape of a real compatibility bug in a word processor: a
document opens, looks mostly fine, and has a few boxes in it.

### 4. Layout must be lazy and cached

| Path | Measured |
| ---- | -------- |
| Cold layout, 400 paragraphs / 248k chars | **1030ms** (2.58ms/paragraph) |
| Warm re-break, 45 paragraphs (≈ one visible page) | **2.66ms** |
| Warm re-break, single paragraph, worst case | **54µs** |

Cold layout is 65× a frame budget and includes one-time font loading. Warm
re-break — re-using shaped glyphs — is **6× under** a frame.

This is the most important performance result in the project. The design
implication is firm:

> Never lay out the whole document on the typing, resize or scroll path. Lay out
> the visible page only, cache it, and re-break rather than rebuild.

`Layout` is cheap to re-break and expensive to rebuild, so the cache must survive
edits to *other* paragraphs. This becomes the central design constraint on
`u1-layout` in Phase 1.

### 5. A single fallback policy does not work — it must be per-platform

This one was found by CI rather than by reasoning, and it is the most instructive
result in the spike.

The first version of the policy hardcoded **Windows** family names
(`SimSun`, `NSimSun`, `Microsoft YaHei`, …). It passed on the development
machine and on Windows CI. On **macOS CI every one of those families was
absent**, so the chain resolved to *nothing* and all 43 characters of the Chinese
sample rendered as `.notdef`.

That is strictly worse than the bug it was meant to fix: the failure is more
widespread, and it is invisible to whoever wrote it. A fallback chain that
resolves to nothing is not a safe default — it must be treated as an error.

Fixes applied:

- `src/policy.rs` holds separate tables for Windows, macOS and Linux.
- `set_script_fallback` now returns **how many families actually resolved**, not
  a boolean. Callers can distinguish "all resolved" from "some" from "none", and
  only the last is fatal — but the middle case still leaves a coverage gap worth
  reporting.
- `apply_policy` returns per-script resolution counts, reported in the spike
  output so a packaging gap is visible rather than silent.
- **All three tables are compiled on every platform** and asserted by tests, so a
  policy that is never compiled on your machine cannot rot unnoticed. A test also
  enforces that all three cover the same set of scripts.

`policy()` returning an empty slice for an unrecognised OS would silently
produce pure tofu with no error anywhere, so a test asserts it is non-empty on
every supported platform.

### 6. Linux requires system `libfontconfig` to build

Ubuntu CI failed to compile:

```
error: failed to run custom build command for `yeslogic-fontconfig-sys v6.0.1`
The system library `fontconfig` required by crate `yeslogic-fontconfig-sys` was
not found.
```

`fontique`'s fontconfig backend links against system fontconfig on Linux. This
is a genuine build dependency of U1 Docs on Linux, not a CI quirk —
**packaging in Phase 5 must depend on `libfontconfig`**, and the CI workflow now
installs `libfontconfig1-dev`.

CI also installs `fonts-noto-cjk`, `fonts-noto-devanagari`, `fonts-noto-thai`
and `fonts-noto-core`. The coverage tests skip themselves when a script has no
installed font, which keeps a minimal runner green — but also means they would
silently test nothing. Installing Noto guarantees the CJK and Indic assertions
actually execute on Linux.

---

## Open item

One character still resolves to `.notdef` after explicit fallback:
**। U+0964 DEVANAGARI DANDA**.

Most likely cause is this machine's font set — the only Devanagari families
installed are Nirmala UI and Nirmala Text, both from a `.ttc` collection, and
`Mangal` is absent. Not confirmed.

Carried to Phase 1. Cheap to confirm; trivial to miss. Worth a test asserting
coverage against a curated corpus rather than only spot-checking ASCII.

---

## What this means for Phase 1

1. `u1-font` must own an explicit, per-platform fallback policy
   (`src/policy.rs` is the prototype) — not the system default, and not a single
   hardcoded table.
2. `u1-font` must verify coverage and report gaps, rather than silently
   substituting a partial-coverage font. A chain that resolves to nothing is an
   error, not a fallback.
3. `u1-layout` must be incremental: re-break cached layouts, never rebuild the
   document on the interactive path.
4. Enabling two specific features is now enforced by a test that fails with an
   actionable message if either is dropped.
5. Linux packaging must depend on `libfontconfig`.

## Method note

Two of the six findings (5 and 6) came from CI, not from running the spike
locally — and finding 5 is the one that mattered most. The cross-platform matrix
was not ceremony: it is the only reason a policy that was silently broken on two
of three target platforms got caught in the same day it was written.

## Still open — criteria 2–4

| Criterion | Status |
| --------- | ------ |
| 2. Caret, selection, arrow-key navigation through mixed-direction text | Not started |
| 3. **CJK IME** — candidate window, inline preedit, commit at the correct caret offset | Not started |
| 4. Screen-reader output | Not started |

Criterion 3 remains the largest technical risk in U1 Docs and the reason
[ADR-0003](https://github.com/temidayoxyz/unsoftone/blob/main/docs/adr/0003-desktop-ui-stack.md)
is provisional. `Cluster::from_point()` exists in parley and gives the hit
testing the caret will need; that is the natural starting point.
