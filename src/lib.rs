//! U1 Docs Ã¢â‚¬â€ a local-first word processor.
//!
//! **Status: pre-alpha.** There is no word processor here yet, and nothing in
//! this project should pretend otherwise.
//!
//! ## Layout
//!
//! This crate currently contains only the **Phase 0 spike** described in
//! [`PLAN.md`](https://github.com/temidayoxyz/unsoftone/blob/main/PLAN.md):
//! validating the two assumptions that gate the entire architecture.
//!
//! - [`layout`] Ã¢â‚¬â€ the parley harness (ADR-0002). Proves inline text layout can
//!   carry the scripts and performance budget a word processor needs.
//! - [`corpus`] Ã¢â‚¬â€ multilingual samples. A spike that only measures English
//!   proves nothing.
//! - [`report`] Ã¢â‚¬â€ the six questions and their verdicts.
//!
//! ## The one rule that governs everything here
//!
//! Never destroy content we do not understand. A feature we cannot *edit* is a
//! missing feature. A feature we cannot edit but *do destroy* is data loss, and
//! it is the worst class of bug this project can ship. See ADR-0005.
//!
//! ## The product philosophy
//!
//! No network, no accounts, no telemetry, no activation Ã¢â‚¬â€ ever. See ADR-0007.

pub mod caret;
pub mod corpus;
pub mod coverage;
pub mod layout;
pub mod policy;
pub mod report;
pub mod report_b;

/// Product identity. Fixed by ADR-0004; the repository is named for this.
pub const PRODUCT: &str = "U1 Docs";

/// Ecosystem this product belongs to.
pub const ECOSYSTEM: &str = "UnsoftOne";

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
