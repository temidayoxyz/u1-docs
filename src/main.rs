//! U1 Docs - a local-first word processor.
//!
//! **Status: pre-alpha.** This binary is a placeholder. There is no word
//! processor here yet, and nothing in this project should pretend otherwise.
//!
//! It exists so the repository builds, the test pipeline runs, and there is
//! somewhere honest for the Phase 0 spike to land.
//!
//! See the ecosystem `unsoftone` repository for direction, ADRs and the
//! roadmap. In particular:
//!
//! - `PLAN.md`            - phases, and the explicit "Not now" list
//! - ADR-0002             - text layout uses `parley`
//! - ADR-0003             - desktop UI stack, provisional pending this spike
//! - ADR-0005             - OOXML handling must be lossless
//! - ADR-0007             - no network, no accounts, ever
//!
//! ## The one rule that governs everything here
//!
//! Never destroy content we do not understand. A feature we cannot *edit* is a
//! missing feature. A feature we cannot edit but *do destroy* is data loss, and
//! it is the worst class of bug this project can ship.

/// Product identity. Fixed by ADR-0004; the repository is named for this.
const PRODUCT: &str = "U1 Docs";

/// Ecosystem this product belongs to.
const ECOSYSTEM: &str = "UnsoftOne";

fn main() {
    println!("{PRODUCT} ({ECOSYSTEM})");
    println!();
    println!("Status:  pre-alpha - no word processor yet");
    println!("Phase:   0 - spike");
    println!();
    println!("Validating two assumptions before building anything:");
    println!("  1. interactive layout with correct shaping, bidi and CJK");
    println!("  2. caret, IME and selection in the chosen UI stack");
    println!();
    println!("Roadmap:  https://github.com/temidayoxyz/unsoftone/blob/main/PLAN.md");
    println!("Contributing: https://github.com/temidayoxyz/unsoftone/blob/main/CONTRIBUTING.md");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_identity_is_stable() {
        // Renaming a product is an ecosystem-wide change, not a local one.
        // If this fails, PLAN.md and ADR-0004 need updating too.
        assert_eq!(PRODUCT, "U1 Docs");
        assert_eq!(ECOSYSTEM, "UnsoftOne");
    }
}
