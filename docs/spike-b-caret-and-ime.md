# Spike B — caret, hit testing, and IME readiness

**Phase 0, criteria 2 and 3.** Validates the geometry half of
[ADR-0003](https://github.com/temidayoxyz/unsoftone/blob/main/docs/adr/0003-desktop-ui-stack.md).

**Status: criterion 2 verified. Criterion 3 NOT VERIFIED — see below.**

Machine: Windows, x86_64-pc-windows-msvc, rustc 1.97.0, parley 0.11.1.

Reproduce with `cargo run -- spike-b`. Conclusions are enforced by
`tests/spike_b_caret.rs`.

---

## The headline

> **Criterion 2 passes. Criterion 3 was not tested, because it cannot be tested
> on this machine.**
>
> ADR-0003's geometry claim is supported. Its webview claim is **still
> unproven**, and ADR-0003 therefore remains provisional.

This split is the most important thing in the document. A spike that reported a
single green verdict would be handing false confidence to the largest technical
risk in the project.

---

## Criterion 2 — caret, selection, navigation: verified

16 of 17 checks pass.

| Check | Result |
| ----- | ------ |
| LTR caret starts at x=0 | pass |
| LTR caret x increases monotonically | pass |
| RTL: `Start` affinity sits right of `End` | pass |
| Every caret position is hit-testable (70/70, 4 texts) | pass |
| Logical forward walks every cluster to the end | pass |
| Logical back returns to zero | pass |
| Down reaches a later line; Up blocked at line 0 | pass |
| Snapping always lands on a UTF-8 boundary | pass |
| Caret stops == cluster count + 1 | pass |
| Every caret in every sample has a usable IME anchor rect | pass |
| Caret survives re-break at 200/400/624/900px | pass |
| Emoji ZWJ needs GB11 (**known gap, see finding 2**) | **fail, deliberately** |

Affinity behaves correctly in both directions: for RTL text the `Start` edge is
at x=624.0 and `End` at x=619.7 — the caret's logical start is on the *right* of
its logical end, which is what makes typing in an Arabic sentence land where the
user expects.

---

## Criterion 3 — the CJK IME: not verified, and why

### What this machine has

| Requirement | Status |
| ----------- | ------ |
| Registered input method | **absent** — no `HKCU\Software\Microsoft\CTF\TIP` entries |
| Keyboard layouts | US English only (`00000409`) |
| Windowing session | **works** — a GUI window was created and confirmed |
| `imm32.dll` / `msctfime.ime` | present as files, but no IME is registered |

Composing Japanese on this machine is impossible by construction. That is a
statement about the environment, not about the architecture.

### What *is* verified — everything the IME depends on

An IME integration needs three things from the layout engine: a caret rect to
anchor its candidate window, offsets that are safe to insert at, and geometry
that stays valid as the layout changes. All three are confirmed:

- a caret rect exists at **every** position in **all 12** corpus samples
  (611 positions in `english-long` alone)
- offsets snap to valid UTF-8 boundaries
- geometry survives re-break at four widths, so it stays valid while resizing
- all three CJK scripts resolve on this machine

### What is *not* verified, and cannot be here

- candidate window appears at the caret rect
- preedit renders **inline** rather than being committed as a string
- commit lands at the correct byte offset
- composition cancels cleanly on caret movement or Escape

The third of those is the one that matters most. A layout engine can hand the IME
a perfect anchor rect and still commit at the wrong offset if the webview's
editable anchor and the Rust layout disagree about where the caret is. **Only a
real composition can reveal that disagreement**, and it is precisely the
disagreement ADR-0003 proposes to accept.

---

## Findings

### 1. `LineMetrics.offset` is a horizontal alignment offset, not a y position

This is the bug that cost the most time, and it produced a **completely silent
failure**.

`LineMetrics.offset` reads like a vertical offset and is `0` for every line
under `Alignment::Start`. Vertical navigation therefore asked for "line 1,
y ≈ 6.3", hit-tested, and got back byte 0 on line 0 — forever. Up and Down did
nothing. No panic, no error, no visible symptom beyond a caret that would not
leave the first line.

parley 0.11 exposes no per-line y accessor: `Line::index` is private, its geometry
fields are `pub(crate)`, and `LineMetrics.baseline` is an offset *within* the
line. The correct model is a stacked accumulation of `line_height` — verified
against parley's own `Layout::height()`, which the sum matches to the last bit.

`CaretMap::line_top` is now that accumulation, and a test asserts the sum equals
the layout height so the two cannot drift apart.

**Consequence for Phase 1:** with mixed line heights (a heading among body text)
a linear scan per query will not scale. `line_top` is shaped to become a
prefix-sum table.

### 2. parley's clusters are per-codepoint, so caret stops need grapheme filtering

Measured, not assumed:

| Input | Codepoints | parley clusters |
| ----- | ---------- | --------------- |
| `e` + U+0301 combining acute | 2 | **2** |
| Devanagari ka + i-matra | 2 | **2** |
| ZWJ family emoji | 5 | **5** |

So a caret walk over raw cluster edges offers a stop *between a base letter and
its combining accent*. That looks wrong, and typing at such a position orphans
the mark — producing mojibake.

This is not a parley defect: shaping granularity and grapheme granularity are
different questions, and parley answers the first. But **grapheme segmentation
is ours, and it is not optional.**

Added `CaretMap::grapheme_caret_positions`, filtering on UAX #29 GB9 (do not
break before extending characters). That reduces `e`+U+0301 from 3 raw stops to 2.

**Known gap, recorded as a failing check so it cannot be forgotten:** a GB9
filter cannot express **GB11** (emoji ZWJ sequences). A family emoji still yields
4 stops instead of 2. Phase 1 must move to `unicode-segmentation`, which
implements the full rules including regional indicator pairs and Hangul jamo.

### 3. End of text belongs to the last line, not to a line of its own

The final line's text range is exclusive at its end, so `line_index_at(text.len())`
found no line and returned `None`. This matters because end of text is where the
caret sits after typing to the end, and because vertical navigation needs to know
it is on the last line in order to report `Down` as blocked.

---

## Method note

All three findings were found by **asserting on measured output**, not by
reading code. Two of them — the `offset` confusion and the per-codepoint clusters
— compile cleanly, produce no warnings, and pass a naive test that checks only
that "a caret exists". They fail only when something checks that the caret is in
the right place.

That is the argument for the spike's shape: every check compares against an
expected relationship (`cluster count + 1`, `sum of heights == layout height`,
`affinity order flips in RTL`) rather than merely confirming a value is present.
The deliberately-failing GB11 check is the same idea applied to a known gap.

---

## What this means for Phase 1

1. `u1-layout` needs a line-offset table (prefix sums), not per-query scanning.
2. **Grapheme segmentation via `unicode-segmentation`** is required before any
   insertion path ships. The GB9 filter is a stopgap.
3. Caret geometry is a first-class output of the layout layer, not a rendering
   concern — it is what the IME anchor consumes.
4. `Re` must not mutate layout; `rebreak` is the cheap path (Spike A finding 4).

---

## Still open — the manual protocol

Criterion 3 needs a machine with an input method installed. The protocol is
short and each step has an observable pass condition.

### Prerequisites

- Windows with a CJK language pack and an IME enabled (Settings → Time & language)
- Verify with `Get-ItemProperty 'HKCU:\Keyboard Layout\Preload'` listing more
  than `00000409`
- A build of the spike's caret layer exposed to the webview

### Steps

| # | Action | Pass condition |
| - | ------ | -------------- |
| 1 | Place the caret mid-word in ASCII text, switch to Japanese IME | Candidate window appears **at the caret**, not at the window origin |
| 2 | Type `にほん` | Preedit renders **inline** at the caret, underlined; not a separate box |
| 3 | Commit with Space | Text lands at the caret; **no characters before or after** |
| 4 | Repeat inside an Arabic phrase | Commit offset is still correct — this is the bidi + IME interaction, the hardest case |
| 5 | Press Left/Right mid-composition | Composition cancels cleanly; no stray preedit remains |
| 6 | Press Escape mid-composition | Composition cancels; document unchanged |
| 7 | Resize the window mid-composition | Candidate window tracks the new caret position |
| 8 | Click elsewhere mid-composition | Composition commits or cancels per policy, never leaves orphaned preedit |

## Running it

There is a harness. You do not need to build anything first.

```bash
cargo run -p ime-protocol
```

It opens a window with three samples — ASCII, Arabic-inside-English, and Chinese
— and a live event log.

The caret positions, line boxes and the IME anchor's coordinates are all
**computed by the Rust layout engine and injected as plain numbers**. The HTML
never asks the browser where anything is. That is the entire bet of ADR-0003,
made inspectable: if the OS IME misplaces relative to that anchor, the log shows
it.

### Why it is a separate crate

The harness lives at `spikes/ime-protocol`, not behind a feature flag on the
product crate.

That was the second attempt. A feature flag looked equivalent and was not:
`cargo clippy --all-features` in the default CI job switched the webview back
on, so the job needed `webkit2gtk` and the product stopped being webview-free in
practice while still appearing to be. A separate crate makes the boundary
**structural** — nothing in `u1-docs`' dependency graph can reference a webview,
by construction rather than by convention.

It is also the honest shape: a throwaway validation harness does not belong
wired into the product. If the protocol refutes ADR-0003, `spikes/` is deleted
and nothing else moves.

CI asserts the separation rather than assuming it, because it has already been
got wrong once:

```
u1-docs must not depend on a webview; ADR-0003 is provisional
```

wry 0.57 attaches to a window handle rather than creating one, so the host window
comes from `tao`. Same split Tauri uses.

### What the log tells you

| Event | Meaning |
| ----- | ------- |
| `compositionstart` | the IME attached at the current caret |
| `compositionupdate` | preedit is changing — it must render **inline**, not in a box |
| `compositionend` | text committed; the log records what arrived |
| `input` (not composing) | **a failure signal** — text arrived outside composition |
| `keydown keyCode 229` | normal for IME keys, not a problem |

The harness cannot verify on its own that a commit landed at the *right* offset,
because the document text lives in the webview while the geometry lives in Rust.
That comparison is step 3, and it is deliberately a human judgement.

---

## Recording the result

Open an issue titled `IME protocol: <platform> <result>`, noting which steps
passed. If **step 3 or 4 fails**, ADR-0003's webview approach is refuted for that
platform and a superseding ADR is required — see its "Review trigger".

**Do not mark this criterion done on the strength of the geometry checks.** The
geometry is necessary and not sufficient, and the gap between them is the whole
risk.
