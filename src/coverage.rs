//! Font coverage gap reporting.
//!
//! ## The failure this prevents
//!
//! Spike A found that the *default* font fallback does not guarantee coverage: it
//! selected fonts covering part of a script and produced `.notdef` boxes —
//! invisible, silent, indistinguishable from correct rendering — for ordinary
//! characters like 严 (U+4E25).
//!
//! An application that renders those boxes without saying anything has
//! committed the worst kind of bug for a word processor: the user opens a
//! document, it looks *almost* right, and nothing reports the difference. It is
//! the same class as the data-loss rule in ADR-0005 — a missing feature is
//! visible, a silent substitution is not.
//!
//! ## What "reporting" means here
//!
//! Not an error. A document with unmapped characters still opens and still
//! edits. What changes is that the gap becomes **knowable**: the characters are
//! named, counted, and attributed, so the UI can offer to install a font, and a
//! bug report can carry the list.

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Severity of a coverage problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// A few characters in an otherwise covered script. Usually a rare symbol
    /// or an emoji the installed font set lacks.
    Sparse,
    /// An entire script has no font at all. Every character in it will be a
    /// box, and the user cannot read the document.
    ScriptMissing,
}

/// Anything that can supply a sequence of characters to record as unmapped.
///
/// Implemented for `&str`, `char`, and `Vec<char>` so callers do not have to
/// remember which of those is already an iterator — the mistake is easy to make
/// and the resulting type error is unhelpfully vague.
pub trait IntoMissingChars {
    fn into_chars(self) -> Vec<char>;
}

impl IntoMissingChars for &str {
    fn into_chars(self) -> Vec<char> {
        self.chars().collect()
    }
}

impl IntoMissingChars for char {
    fn into_chars(self) -> Vec<char> {
        vec![self]
    }
}

impl IntoMissingChars for Vec<char> {
    fn into_chars(self) -> Vec<char> {
        self
    }
}

impl IntoMissingChars for &[char] {
    fn into_chars(self) -> Vec<char> {
        self.to_vec()
    }
}

impl IntoMissingChars for String {
    fn into_chars(self) -> Vec<char> {
        self.chars().collect()
    }
}

/// Characters that could not be rendered, grouped and ready to display.
#[derive(Debug, Clone, Default)]
pub struct CoverageReport {
    /// Unmapped characters with their occurrence counts, ordered by count.
    ///
    /// A `BTreeMap` keyed by the character keeps the output stable, which
    /// matters for tests and for diffing two reports.
    missing: BTreeMap<char, usize>,
    /// Scripts for which **no** font resolved, if any.
    scripts_without_fonts: Vec<String>,
}

impl CoverageReport {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record characters that resolved to `.notdef`.
    ///
    /// Accepts `&str`, `char`, or any iterator of `char`, so callers can pass a
    /// freshly collected list or a single character without adapting.
    pub fn record_missing(&mut self, chars: impl IntoMissingChars) {
        for c in chars.into_chars() {
            *self.missing.entry(c).or_insert(0) += 1;
        }
    }

    /// Record a script for which no font could be found on this machine.
    ///
    /// This is a *packaging* problem rather than a document problem: no amount
    /// of fallback configuration fixes a user with no Thai font installed.
    pub fn record_missing_script(&mut self, script: impl Into<String>) {
        self.scripts_without_fonts.push(script.into());
    }

    /// Did anything fail to render?
    pub fn is_clean(&self) -> bool {
        self.missing.is_empty() && self.scripts_without_fonts.is_empty()
    }

    /// Total unmapped character occurrences.
    pub fn missing_count(&self) -> usize {
        self.missing.values().sum()
    }

    /// Number of distinct unmapped characters.
    pub fn distinct_missing(&self) -> usize {
        self.missing.len()
    }

    /// Scripts with no installed font.
    pub fn scripts_without_fonts(&self) -> &[String] {
        &self.scripts_without_fonts
    }

    pub fn severity(&self) -> Option<Severity> {
        if !self.scripts_without_fonts.is_empty() {
            Some(Severity::ScriptMissing)
        } else if self.is_clean() {
            None
        } else {
            Some(Severity::Sparse)
        }
    }

    /// Unmapped characters ordered by descending frequency, then by codepoint.
    ///
    /// Frequency order is what makes the report useful: the top entry is the one
    /// worth acting on, and a reader can see whether the gap is one odd symbol
    /// or a systematic failure.
    pub fn ranked_missing(&self) -> Vec<(char, usize)> {
        let mut v: Vec<(char, usize)> = self.missing.iter().map(|(c, n)| (*c, *n)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v
    }

    /// A single-line summary suitable for a status bar.
    pub fn summary(&self) -> String {
        if self.is_clean() {
            return "All characters rendered.".to_string();
        }
        match self.severity() {
            Some(Severity::ScriptMissing) => {
                let scripts = self.scripts_without_fonts.join(", ");
                format!(
                    "No font installed for: {scripts}. \
                     {} character(s) in this document cannot be displayed.",
                    self.missing_count()
                )
            }
            _ => {
                let top = self
                    .ranked_missing()
                    .first()
                    .map(|(c, n)| format!("{c} (x{n})"))
                    .unwrap_or_default();
                format!(
                    "{} occurrence(s) of {} distinct character(s) could not be \
                     displayed. Most frequent: {top}.",
                    self.missing_count(),
                    self.distinct_missing()
                )
            }
        }
    }

    /// A multi-line, human-readable detail block.
    ///
    /// Includes codepoints so a report can be acted on without the reporter
    /// needing to identify a glyph by eye.
    pub fn detail(&self) -> String {
        let mut out = String::new();
        if self.is_clean() {
            return out;
        }

        if !self.scripts_without_fonts.is_empty() {
            let _ = writeln!(
                out,
                "Scripts with no installed font: {}",
                self.scripts_without_fonts.join(", ")
            );
            let _ = writeln!(
                out,
                "  These are a machine or packaging problem, not a document one."
            );
        }

        let ranked = self.ranked_missing();
        if !ranked.is_empty() {
            let _ = writeln!(
                out,
                "Characters with no glyph ({} distinct, {} occurrence(s)):",
                ranked.len(),
                self.missing_count()
            );
            for (c, n) in ranked.iter().take(32) {
                let _ = writeln!(out, "  {c:?}  U+{:04X}  x{n}", *c as u32);
            }
            if ranked.len() > 32 {
                let _ = writeln!(out, "  ... and {} more", ranked.len() - 32);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_report_is_clean() {
        let r = CoverageReport::new();
        assert!(r.is_clean());
        assert_eq!(r.severity(), None);
        assert_eq!(r.summary(), "All characters rendered.");
    }

    #[test]
    fn ranks_by_frequency_then_codepoint() {
        let mut r = CoverageReport::new();
        r.record_missing("aaabb");
        let ranked = r.ranked_missing();
        assert_eq!(ranked, vec![('a', 3), ('b', 2)]);
    }

    #[test]
    fn missing_script_outranks_sparse() {
        let mut r = CoverageReport::new();
        r.record_missing('a');
        r.record_missing_script("Thai");
        assert_eq!(r.severity(), Some(Severity::ScriptMissing));
    }

    #[test]
    fn detail_includes_codepoints() {
        let mut r = CoverageReport::new();
        r.record_missing(vec!['\u{4E25}']);
        assert!(r.detail().contains("U+4E25"), "detail: {}", r.detail());
    }

    #[test]
    fn detail_truncates_long_lists() {
        let mut r = CoverageReport::new();
        let many: String = (0x4E00u32..0x4E40u32).filter_map(char::from_u32).collect();
        r.record_missing(many);
        let d = r.detail();
        assert!(d.contains("and "), "expected a truncation line: {d}");
    }
}
