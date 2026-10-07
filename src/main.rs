//! U1 Docs entry point.
//!
//! For now this runs the Phase 0 inline-layout spike and prints its report.
//! It is not a word processor.
//!
//! Exits non-zero if any spike question fails, so CI can gate on it.

fn main() {
    if !u1_docs::report::run() {
        std::process::exit(1);
    }
}
