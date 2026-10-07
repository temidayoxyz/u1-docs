//! OOXML: the container and the XML it holds.
//!
//! See ADR-0005. Two modules, one rule between them:
//!
//! > Never destroy content we do not understand.
//!
//! - [`opc`] — the zip container: open, inspect, save, with every part kept
//!   verbatim.
//! - [`tree`] — an XML tree that serialises back to the bytes it parsed.
//!
//! Neither models document content yet. That is deliberate: the lossy parts are
//! the WordprocessingML semantics, and modelling them properly is the next step.
//! The container and the tree have to be exactly right first, because every later
//! step assumes a round trip does not touch what it did not edit.

pub mod opc;
pub mod tree;
