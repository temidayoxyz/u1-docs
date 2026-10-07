# Contributing to U1 Docs

U1 Docs is one of four independent applications in the
[UnsoftOne](https://github.com/temidayoxyz/unsoftone) ecosystem. This repository
is self-contained: it has its own history, its own releases and its own
roadmap. Nothing in the `unsoftone` repository gates a build here.

## Read these first

| Document                                                                 | Why                                                        |
| ----------------------------------------------------------------------- | ---------------------------------------------------------- |
| [PLAN.md](https://github.com/temidayoxyz/unsoftone/blob/main/PLAN.md)     | Priorities, and the **"Not now"** table. Read it before proposing anything. |
| [ADR-0007 â€” local-first](https://github.com/temidayoxyz/unsoftone/blob/main/docs/adr/0007-local-first-is-not-negotiable.md) | The product's defining constraint. Non-negotiable. |
| [ADR-0005 â€” lossless OOXML](https://github.com/temidayoxyz/unsoftone/blob/main/docs/adr/0005-ooxml-lossless-layer.md) | Why "don't destroy what you don't understand" is a hard rule. |
| [CONTRIBUTING.md](https://github.com/temidayoxyz/unsoftone/blob/main/CONTRIBUTING.md) | Full contribution guide for the ecosystem. |

## Hard constraints

These are architectural requirements, not preferences. Changes violating them
will not be merged, however good they otherwise are:

1. **No network calls in core code paths.** No accounts, no telemetry, no
   activation, no phone-home, no analytics. The application must work fully
   offline â€” [ADR-0007](https://github.com/temidayoxyz/unsoftone/blob/main/docs/adr/0007-local-first-is-not-negotiable.md).
2. **Never destroy content we do not understand.** A feature we cannot *edit* is
   a missing feature. A feature we cannot edit but *do destroy* is data loss,
   which is the worst class of bug in this project.
3. **Respect the layering.** Foundations know nothing about the document model;
   the document model knows nothing about file formats or the UI.
4. **Accessibility is part of the model,** not a wrapper applied at the end.
5. **Rust core** â€” [ADR-0001](https://github.com/temidayoxyz/unsoftone/blob/main/docs/adr/0001-rust-first-architecture.md).

## Current status

**Pre-alpha. There is no working application yet.**

The current phase is a throwaway spike validating the two assumptions that gate
the entire architecture:

1. Can we lay out a real Word document at interactive speed with correct
   shaping, bidirectional text and CJK?
2. Can we get a caret, IME and text selection working in the chosen UI stack?

The second is the larger risk. Getting an input method editor working correctly
inside shaped, bidirectional text is the hardest technical problem in this
project. See
[ADR-0003](https://github.com/temidayoxyz/unsoftone/blob/main/docs/adr/0003-desktop-ui-stack.md).

Because of this, feature contributions are premature. Real-world test files and
compatibility bug reports are far more useful right now.

Because this project is at an early stage, the most valuable contributions right
now are usually **not** feature code. See the ecosystem
[CONTRIBUTING.md](https://github.com/temidayoxyz/unsoftone/blob/main/CONTRIBUTING.md)
for the current priority order â€” at this stage it is usually real-world test
files and compatibility bug reports.

## Before you open a pull request

```bash
cargo fmt --check                              # must be clean
cargo clippy --all-targets -- -D warnings      # must be clean
cargo test                                     # must pass
```

Do not add `#[allow(...)]`, `clippy::allow`, `todo!()` or `unimplemented!()` to
make a check pass. If a check is genuinely wrong for a good reason, say so in
the pull request and explain the reasoning â€” do not silence it silently.

## Commit messages

Explain **why**, not what. The diff already says what.

```
Good:
  preserve unknown w:sectPr children instead of dropping them

  Round-trip corpus caught a document losing its page borders after a
  save. The element is valid but we do not model it, so we now pass the
  original subtree through untouched.

Bad:
  fix section properties
  update code
  wip
```

## Changing an architecture decision

Do not edit an accepted ADR in the `unsoftone` repository. Write a new one that
supersedes it, and state what evidence changed your mind. Architecture PRs
should include the ADR **and** the implementation together.

## Reporting bugs

Use the **Bug report** issue template. The most valuable field is the sample
file â€” please redact anything you are not permitted to share. If a round trip
lost content, say so prominently; it is treated as the highest severity.

## License

[GPL-3.0](LICENSE). By contributing you confirm you have the right to license
the work under GPL-3.0. See
[ADR-0006](https://github.com/temidayoxyz/unsoftone/blob/main/docs/adr/0006-license-gpl-3.md).
Do not contribute code copied from an incompatible license, including
proprietary code. When in doubt, ask.
