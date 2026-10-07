## What this changes

<!-- One paragraph. What is different, and why it is worth a reviewer's time. -->

Fixes #

## Why

<!-- The reasoning. What was the problem, what did you consider, what made you
     choose this? Future maintainers need the rejected options as much as the
     chosen one. -->

## How it was verified

<!-- How do we know this works? Tests run, corpus files round-tripped,
     screenshots, measurements. Be specific enough that a reviewer can repeat it. -->

## Checklist

- [ ] `cargo fmt --check` is clean
- [ ] `cargo clippy --all-targets -- -D warnings` is clean
- [ ] Tests pass, and new behaviour has tests
- [ ] No network calls, telemetry, accounts or activation added
      ([ADR-0007](https://github.com/temidayoxyz/unsoftone/blob/main/docs/adr/0007-local-first-is-not-negotiable.md))
- [ ] No content is destroyed that the code does not understand
      ([ADR-0005](https://github.com/temidayoxyz/unsoftone/blob/main/docs/adr/0005-ooxml-lossless-layer.md))
- [ ] Layering respected: foundations know nothing about the document model; the
      model knows nothing about file formats or the UI
- [ ] Accessibility considered — semantics in the model, not bolted on at the end
- [ ] Architecture changed? A superseding ADR is included, and the old ADR is
      **not** edited
- [ ] No proprietary or incompatibly-licensed code copied in
