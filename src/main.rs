//! U1 Docs entry point.
//!
//! The binary runs whichever spike is next. Both are disposable: the findings
//! are what matter, not the harnesses.
//!
//! ```text
//! cargo run            # Spike A - inline text layout (ADR-0002)
//! cargo run -- spike-b # Spike B - caret, hit testing, navigation (ADR-0003)
//! ```

fn main() {
    let spike_b = std::env::args().any(|a| a == "spike-b" || a == "--spike-b");

    // `report_b` returns (text, passed, total); only the counts matter here.
    let ok = if spike_b {
        let (_, passed, total) = u1_docs::report_b::run();
        passed == total
    } else {
        u1_docs::report::run()
    };

    if !ok {
        std::process::exit(1);
    }
}
