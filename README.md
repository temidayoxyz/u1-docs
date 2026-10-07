# U1 Docs

**A word processor. Open, edit and save the documents you already have.**

Part of the [UnsoftOne](https://github.com/temidayoxyz/unsoftone) suite.

```
Download. Install. Work.
```

No account. No subscription. No sign-in. No internet connection. No telemetry.
Your files stay on your disk, in standard formats, readable by any other
software.

---

## Status: pre-alpha

**There is no working application yet.** This repository is at the very
beginning — see [Current work](#current-work) below.

This is stated plainly rather than hidden behind a road map, because a project
that overstates its progress wastes people's time and, worse, their trust.

---

## What this will be

U1 Docs is intended to be a **serious** word processor — not a Markdown editor
and not a toy rich-text box. The goal is software that opens, edits, saves and
exports the files people already encounter in the Microsoft Office ecosystem,
without damaging them.

### The target

| Capability            | Meaning                                                        |
| --------------------- | -------------------------------------------------------------- |
| **Opens real files**  | The messy, hand-edited documents people actually have           |
| **Never loses content** | Open → save → open leaves the document semantically identical  |
| **Correct text**      | Arabic, Indic, CJK, emoji, bidirectional — all shaped properly  |
| **Fast**              | Typing stays responsive in a 500-page document                 |
| **Exports PDF**       | The universal "it just works" format                            |
| **Works offline**     | Verified with the network switched off                         |

### Feature depth, in priority order

The ordering is deliberate and is the definition of "done" for v1:

1. Paragraphs, runs, character formatting
2. Fonts, sizes, bold/italic/underline
3. Alignment, indentation, spacing
4. Lists and numbering
5. Named and linked styles
6. Tables
7. Page setup, margins, sections
8. Headers and footers
9. Images, inline and floating
10. Hyperlinks
11. Find and replace
12. PDF export

After that: footnotes, comments, track changes, text boxes, equations.

---

## Compatibility

Tracked honestly in the
[ecosystem compatibility matrix](https://github.com/temidayoxyz/unsoftone/blob/main/docs/compatibility.md).

| Format  | Status                                                      |
| ------- | ----------------------------------------------------------- |
| `.docx` | Primary target. Not implemented yet.                        |
| `.pdf`  | Import and export planned. Export is the realistic first goal. |
| `.txt`  | Planned.                                                     |
| `.md`   | Planned. Not a substitute for `.docx`.                      |

`.rtf`, `.odt` and the legacy binary `.doc` are deliberately **not** committed
to. See [`PLAN.md` "Not now"](https://github.com/temidayoxyz/unsoftone/blob/main/PLAN.md).

---

## Current work

The immediate phase is a **throwaway spike** validating the two assumptions that
gate the entire architecture:

1. Can we lay out a real Word document at interactive speed with correct
   shaping, bidirectional text and CJK?
2. Can we get a caret, IME and text selection working in the chosen UI stack?

The second question is the real one. Getting an input method editor working
correctly inside shaped, bidirectional text is the single largest technical risk
in this project, and finding out now is far cheaper than finding out in six
months. See
[ADR-0003](https://github.com/temidayoxyz/unsoftone/blob/main/docs/adr/0003-desktop-ui-stack.md).

**Spike exit criteria:** a document with RTL, CJK, tables and images scrolls and
edits smoothly, with a working caret and a working CJK input method.

---

## Building

Nothing to build yet. When there is:

```bash
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

Requires a Rust toolchain. Cross-platform: Windows, macOS, Linux.

---

## Architecture

Five layers, one-directional dependencies. Lower layers never know about upper
ones — that is what makes the core testable without a GUI.

```
5. Shell              windows, menus, chrome, IME anchor
4. Editing surface    caret, selection, commands, undo, clipboard
3. File I/O           DOCX, PDF          <- lossless (ADR-0005)
2. Model + layout     blocks, inlines, inline layout -> page layout
1. Foundations        fonts, shaping, geometry, undo engine
```

Text layout uses [`parley`](https://github.com/linebender/parley) — Web Platform
Tests validated, from the Linebender organisation. See
[ADR-0002](https://github.com/temidayoxyz/unsoftone/blob/main/docs/adr/0002-text-layout-parley.md).

The hardest part of this project is not the UI. It is the block/flow/page layout
layer above `parley` — paragraphs, tables, floats, page breaking, widows and
orphans. That layer does not exist yet.

Full overview:
[architecture/overview.md](https://github.com/temidayoxyz/unsoftone/blob/main/docs/architecture/overview.md).

---

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) first.

At this stage the most useful contributions are usually **not** feature code:

1. **Real-world `.docx` files** that break things — redacted appropriately
2. **Compatibility bug reports** with a sample file attached
3. Layout correctness tests for a specific script or direction

Do not send documents containing anything you do not have permission to share.

---

## Related

| Project                      | What it is                            |
| ---------------------------- | ------------------------------------- |
| [`unsoftone`](https://github.com/temidayoxyz/unsoftone) | Ecosystem direction, ADRs, roadmap |
| [`u1-sheets`](https://github.com/temidayoxyz/u1-sheets) | Spreadsheets                        |
| [`u1-slides`](https://github.com/temidayoxyz/u1-slides) | Presentations                       |
| [`u1-notes`](https://github.com/temidayoxyz/u1-notes)   | Notes                                |

## License

[GPL-3.0](LICENSE) ·
[ADR-0006](https://github.com/temidayoxyz/unsoftone/blob/main/docs/adr/0006-license-gpl-3.md)
