//! OOXML: the container, the XML it holds, and what that XML means.
//!
//! See ADR-0005. Three layers, one rule between them:
//!
//! > Never destroy content we do not understand.
//!
//! - [`opc`] — the zip container: open, inspect, save, with every part kept
//!   verbatim.
//! - [`tree`] — an XML tree that serialises back to the bytes it parsed.
//! - [`wml`] — WordprocessingML semantics: paragraphs, runs, and the text a
//!   paragraph actually contains.
//!
//! The first two are byte-exact and know nothing about meaning. The third reads
//! and never writes, because a reader that cannot corrupt the tree cannot lose
//! anything. Modelling was deferred until the container and the tree were
//! exactly right, since every later step assumes a round trip does not touch
//! what it did not edit.

pub mod opc;
pub mod tree;
pub mod wml;
