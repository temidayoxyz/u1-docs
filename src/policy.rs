//! Per-platform font fallback policy.
//!
//! ## Why this module exists
//!
//! The Phase 0 spike found that fontique's *default* fallback does not guarantee
//! glyph coverage: it selected fonts covering part of a script and produced
//! `.notdef` boxes for ordinary Chinese characters such as ä¸¥ (U+4E25).
//!
//! It also found â€” the hard way, on macOS CI â€” that a **single hardcoded policy
//! does not work either**. Configuring `SimSun` resolves nothing on macOS, and
//! the fallback chain becomes empty, so *everything* renders as tofu. Worse than
//! the original bug, because it fails more visibly and only on machines that are
//! not the developer's.
//!
//! Browsers solve this with per-platform, per-script fallback tables. So do we.
//!
//! ## Design rules
//!
//! 1. **Always list several families per script.** No single family covers a
//!    whole script: Simplified and Traditional Han differ, and a document may
//!    contain both. Noto Sans CJK SC and Noto Sans CJK TC are separate families.
//! 2. **Never assume the developer's machine.** A policy is validated at runtime
//!    by [`crate::layout::LayoutHarness::set_script_fallback`], which reports
//!    unresolved families so a gap can be surfaced rather than discovered by a
//!    user opening a document.
//! 3. **Missing families are normal.** A minimal container may have none of
//!    these. That is a *packaging* problem to report, not a crash.

/// A fallback chain for one script.
pub struct FallbackEntry {
    /// ISO 15924 script code (`Hani`, `Kana`, `Deva`, ...).
    pub script: &'static str,
    /// Preferred-first family names available on this platform.
    pub families: &'static [&'static str],
}

/// Every fallback table this crate knows about, for all platforms.
///
/// Compiled unconditionally on purpose. Selecting with `cfg!` instead of `#[cfg]`
/// keeps the other platforms' tables live, so they cannot rot unnoticed and can
/// be asserted on from any host â€” a policy that is only ever compiled on the
/// machine that wrote it is exactly the bug this module exists to prevent.
pub const ALL_POLICIES: &[(&str, &[FallbackEntry])] =
    &[("windows", WINDOWS), ("macos", MACOS), ("linux", LINUX)];

/// The fallback policy for the platform this binary was built for.
///
/// Family names are the ones each platform actually ships. A name that is not
/// installed is skipped silently by [`crate::layout::LayoutHarness::set_script_fallback`],
/// so listing extras is cheap and listing a family from another platform is
/// harmless â€” it simply never matches.
pub fn policy() -> &'static [FallbackEntry] {
    if cfg!(target_os = "windows") {
        WINDOWS
    } else if cfg!(target_os = "macos") {
        MACOS
    } else if cfg!(target_os = "linux") {
        LINUX
    } else {
        &[]
    }
}

/// Scripts this policy attempts to cover.
pub fn scripts() -> Vec<&'static str> {
    policy().iter().map(|e| e.script).collect()
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

pub const WINDOWS: &[FallbackEntry] = &[
    FallbackEntry {
        script: "Latn",
        families: &[
            "Segoe UI",
            "Calibri",
            "Arial",
            "Times New Roman",
            "Verdana",
            "Georgia",
            "Tahoma",
        ],
    },
    FallbackEntry {
        script: "Hani",
        families: &[
            "SimSun",
            "NSimSun",
            "Microsoft YaHei",
            "Microsoft JhengHei",
            "Malgun Gothic",
            "MingLiU",
            "PMingLiU",
        ],
    },
    FallbackEntry {
        script: "Hana",
        families: &["Microsoft JhengHei", "PMingLiU", "Microsoft YaHei"],
    },
    FallbackEntry {
        script: "Kana",
        families: &[
            "Yu Gothic",
            "Meiryo",
            "MS Gothic",
            "MS Mincho",
            "Malgun Gothic",
        ],
    },
    FallbackEntry {
        script: "Hang",
        families: &["Malgun Gothic", "Gulim", "Dotum", "Batang", "SimSun"],
    },
    FallbackEntry {
        script: "Deva",
        families: &["Nirmala UI", "Nirmala Text", "Mangal", "Kokila", "Utsaah"],
    },
    FallbackEntry {
        script: "Arab",
        families: &[
            "Segoe UI",
            "Arial",
            "Tahoma",
            "Times New Roman",
            "Sakkal Majalla",
        ],
    },
    FallbackEntry {
        script: "Hebr",
        families: &["Segoe UI", "Arial", "Tahoma", "Times New Roman", "David"],
    },
    FallbackEntry {
        script: "Thai",
        families: &["Leelawadee UI", "Tahoma", "Angsana New", "Segoe UI"],
    },
];

// ---------------------------------------------------------------------------
// macOS
// ---------------------------------------------------------------------------

pub const MACOS: &[FallbackEntry] = &[
    FallbackEntry {
        script: "Latn",
        families: &[
            "SF Pro Text",
            "Helvetica Neue",
            "Times New Roman",
            "Georgia",
            "Palatino",
            "Geneva",
        ],
    },
    FallbackEntry {
        script: "Hani",
        families: &[
            "PingFang SC",
            "Songti SC",
            "Heiti SC",
            "Hiragino Sans GB",
            "STHeiti",
            "Apple LiGothic",
        ],
    },
    FallbackEntry {
        script: "Hana",
        families: &["Hiragino Sans", "Heiti TC", "Songti TC", "Apple LiGothic"],
    },
    FallbackEntry {
        script: "Kana",
        families: &[
            "Hiragino Sans",
            "Hiragino Kaku Gothic ProN",
            "Yu Gothic",
            "YuGothic",
            "Noto Sans CJK JP",
        ],
    },
    FallbackEntry {
        script: "Hang",
        families: &["Apple SD Gothic Neo", "AppleGothic", "Noto Sans CJK KR"],
    },
    FallbackEntry {
        script: "Deva",
        families: &[
            "Kohinoor Devanagari",
            "Devanagari Sangam MN",
            "Kohinoor Devanagari Light",
            "Noto Sans Devanagari",
        ],
    },
    FallbackEntry {
        script: "Arab",
        families: &["Geeza Pro", "SF Arabic", "Al Bayan", "Baghdad", "Arial"],
    },
    FallbackEntry {
        script: "Hebr",
        families: &["SF Hebrew", "Arial Hebrew", "Raanana", "Arial"],
    },
    FallbackEntry {
        script: "Thai",
        families: &[
            "Thonburi",
            "Sathu",
            "Ayuthaya",
            "Noto Sans Thai",
            "Krungthep",
        ],
    },
];

// ---------------------------------------------------------------------------
// Linux
// ---------------------------------------------------------------------------

pub const LINUX: &[FallbackEntry] = &[
    FallbackEntry {
        script: "Latn",
        families: &[
            "DejaVu Sans",
            "Liberation Sans",
            "Noto Sans",
            "FreeSans",
            "Ubuntu",
        ],
    },
    FallbackEntry {
        script: "Hani",
        families: &[
            "Noto Sans CJK SC",
            "Noto Sans CJK TC",
            "Noto Serif CJK SC",
            "WenQuanYi Zen Hei",
            "WenQuanYi Micro Hei",
            "Droid Sans Fallback",
            "Source Han Sans SC",
        ],
    },
    FallbackEntry {
        script: "Hana",
        families: &[
            "Noto Sans CJK TC",
            "Noto Sans CJK SC",
            "WenQuanYi Zen Hei",
            "Source Han Sans TC",
        ],
    },
    FallbackEntry {
        script: "Kana",
        families: &[
            "Noto Sans CJK JP",
            "Noto Serif CJK JP",
            "IPAGothic",
            "IPAPGothic",
            "Source Han Sans",
        ],
    },
    FallbackEntry {
        script: "Hang",
        families: &[
            "Noto Sans CJK KR",
            "Noto Serif CJK KR",
            "NanumGothic",
            "UnDotum",
        ],
    },
    FallbackEntry {
        script: "Deva",
        families: &[
            "Noto Sans Devanagari",
            "Noto Serif Devanagari",
            "Lohit Devanagari",
        ],
    },
    FallbackEntry {
        script: "Arab",
        families: &[
            "Noto Naskh Arabic",
            "Noto Sans Arabic",
            "Scheherazade New",
            "DejaVu Sans",
        ],
    },
    FallbackEntry {
        script: "Hebr",
        families: &["Noto Sans Hebrew", "Noto Serif Hebrew", "DejaVu Sans"],
    },
    FallbackEntry {
        script: "Thai",
        families: &[
            "Noto Sans Thai",
            "Noto Serif Thai",
            "Garuda",
            "Loma",
            "DejaVu Sans",
        ],
    },
];
