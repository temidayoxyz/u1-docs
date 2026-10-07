//! Inline text layout harness for the Phase 0 spike.
//!
//! Wraps `parley` so the spike can ask concrete questions and get answers it can
//! assert on, rather than "it rendered something, probably fine".
//!
//! ## What this deliberately does NOT do
//!
//! This is inline layout only — shaping, line breaking, bidi reordering and
//! alignment of a single paragraph. The block, page and flow layer above it is
//! the largest engineering item in U1 Docs and does not exist yet. See
//! ADR-0002 and the architecture overview.
//!
//! What this *does* establish is that the inline foundation from ADR-0002
//! (parley + fontique) can carry the scripts and the performance budget a real
//! word processor needs. If that fails, we need to know before building a page
//! layout engine on top of it.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use parley::fontique;
use parley::{
    Alignment, AlignmentOptions, Cluster, FontContext, LayoutContext, PositionedLayoutItem,
    StyleProperty,
};

/// Display scale. 1.0 = CSS-like reference pixels.
const SCALE: f32 = 1.0;

/// Body text size in points, matching a typical word processor default.
pub const DEFAULT_FONT_SIZE: f32 = 11.0;

/// Glyph id 0 is `.notdef` — the "no glyph for this character" box.
///
/// Counting these is the cheapest way to detect a font-coverage failure, which
/// is the most common way a layout spike silently produces garbage.
const NOTDEF: u32 = 0;

/// A single positioned glyph run, reduced to what the spike needs to assert on.
#[derive(Debug, Clone)]
pub struct RunSummary {
    /// Byte range of the source text this run covers.
    pub start: usize,
    pub end: usize,
    /// Stable-per-process identity of the font backing this run. Two runs with
    /// different keys came from different font files, which is how we detect
    /// that fallback actually happened.
    pub font_key: usize,
    pub glyph_count: usize,
    /// Glyphs that resolved to `.notdef`.
    pub notdef: usize,
    /// Horizontal offset of the run's origin within the line.
    pub x: f32,
    /// Vertical offset within the line.
    pub y: f32,
    /// Advance width of the whole run.
    pub width: f32,
}

/// Everything the spike learned about one laid-out paragraph.
#[derive(Debug, Clone)]
pub struct ParagraphSummary {
    pub sample_name: &'static str,
    pub probes: &'static str,
    pub line_count: usize,
    /// Total laid-out width. Must not exceed the wrap width when one was given.
    pub width: f32,
    pub height: f32,
    pub runs: Vec<RunSummary>,
    /// Number of distinct font files used across all runs.
    pub distinct_fonts: usize,
    pub glyph_count: usize,
    pub notdef: usize,
    /// Exact characters that resolved to `.notdef`, in document order.
    ///
    /// Counting tofu tells you something is wrong. Naming the characters tells
    /// you what to fix, which is the only useful form of this information. A
    /// single missing ideographic full stop or Devanagari danda is invisible in
    /// a count and obvious in a character list.
    pub missing_chars: Vec<char>,
    /// Time to build the layout from scratch (shape + first line break).
    pub build_time: Duration,
    /// Time to re-break the *existing* layout at a new width.
    ///
    /// This is the number that matters most for a word processor. Typing,
    /// resizing and pagination changes re-use the existing shaped glyphs, so
    /// this path — not `build_time` — is what has to fit in a frame.
    pub rebreak_time: Duration,
}

impl ParagraphSummary {
    /// True if any glyph in the paragraph failed to resolve to a real glyph.
    pub fn has_missing_glyphs(&self) -> bool {
        self.notdef > 0
    }

    /// True if more than one font file was needed — i.e. fallback occurred.
    pub fn used_font_fallback(&self) -> bool {
        self.distinct_fonts > 1
    }
}

/// What the system font collection contains.
///
/// Whether fontique can actually enumerate and query Windows system fonts is
/// an assumption this spike exists to test, not a fact we should take on trust.
#[derive(Debug, Clone)]
pub struct FontInventory {
    pub family_count: usize,
    /// Family names containing any of the given substrings, for spot checks.
    pub matches: Vec<(String, String)>,
}

/// Look for families whose name contains `needle`, returning
/// `(needle, matched family names)`.
fn find_matching(names: &[String], needle: &str) -> (String, String) {
    let lower_needle = needle.to_lowercase();
    let hits: Vec<&str> = names
        .iter()
        .filter(|n| n.to_lowercase().contains(&lower_needle))
        .take(4)
        .map(String::as_str)
        .collect();
    (needle.to_string(), hits.join(", "))
}

/// Owns the two contexts parley needs. Construct once and reuse for the lifetime
/// of the process — building these is expensive and they are explicitly designed
/// to be long-lived scratch space.
pub struct LayoutHarness {
    font_cx: FontContext,
    layout_cx: LayoutContext,
    all_names: Vec<String>,
}

impl Default for LayoutHarness {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutHarness {
    pub fn new() -> Self {
        // `family_names` takes `&mut self` because it lazily populates the
        // collection on first call. This is also the point where system font
        // discovery actually happens.
        let mut font_cx = FontContext::new();
        let layout_cx = LayoutContext::new();

        let mut names: Vec<String> = font_cx
            .collection
            .family_names()
            .map(str::to_string)
            .collect();
        names.sort();
        names.dedup();

        Self {
            font_cx,
            layout_cx,
            all_names: names,
        }
    }

    /// Configure an explicit fallback chain for a script.
    ///
    /// `script` is an ISO 15924 code (`Hani`, `Deva`, `Arab`, ...). `families`
    /// are family names; those absent from the collection are skipped.
    ///
    /// Returns **how many families actually resolved**. Zero means either an
    /// unknown script code or none of the named families are installed — and
    /// that distinction matters, because a chain that resolves to nothing is
    /// worse than no chain at all: it replaces a partially-covering default
    /// with guaranteed tofu.
    ///
    /// Callers should therefore check the count, not assume success.
    pub fn set_script_fallback(&mut self, script: &str, families: &[&str]) -> usize {
        let Ok(script) = fontique::Script::parse(script) else {
            return 0;
        };
        let key = fontique::FallbackKey::new(script, None);
        let ids: Vec<fontique::FamilyId> = families
            .iter()
            .filter_map(|name| self.font_cx.collection.family_id(name))
            .collect();
        let resolved = ids.len();
        self.font_cx.collection.set_fallbacks(key, ids.into_iter());
        resolved
    }

    /// Apply the per-platform policy, returning `(script, resolved, requested)`
    /// for every entry.
    ///
    /// An entry with `resolved == 0` is a packaging gap on this machine — the
    /// user has no font for that script at all — and should be reported rather
    /// than silently ignored.
    pub fn apply_policy(&mut self) -> Vec<(&'static str, usize, usize)> {
        crate::policy::policy()
            .iter()
            .map(|e| {
                (
                    e.script,
                    self.set_script_fallback(e.script, e.families),
                    e.families.len(),
                )
            })
            .collect()
    }

    /// How many fallback families are configured for each given script.
    pub fn fallback_counts(&mut self, scripts: &[&str]) -> Vec<(String, usize)> {
        scripts
            .iter()
            .filter_map(|code| {
                fontique::Script::parse(code).ok().map(|s| {
                    let key = fontique::FallbackKey::new(s, None);
                    let n = self.font_cx.collection.fallback_families(key).count();
                    ((*code).to_string(), n)
                })
            })
            .collect()
    }

    /// Enumerate the system font collection and spot-check for the scripts our
    /// corpus needs.
    pub fn inventory(&mut self) -> FontInventory {
        let probe = [
            "segoe", "arial", "simsun", "malgun", "nirmala", "tahoma", "noto",
        ];
        FontInventory {
            family_count: self.all_names.len(),
            matches: probe
                .iter()
                .map(|n| find_matching(&self.all_names, n))
                .collect(),
        }
    }

    /// Lay out one paragraph, wrapping at `max_width` if given.
    pub fn layout(
        &mut self,
        sample_name: &'static str,
        probes: &'static str,
        text: &str,
        max_width: Option<f32>,
        font_size: f32,
    ) -> ParagraphSummary {
        let build_start = Instant::now();

        let mut builder = self
            .layout_cx
            .ranged_builder(&mut self.font_cx, text, SCALE, true);
        builder.push_default(StyleProperty::FontSize(font_size));

        let mut layout = builder.build(text);
        layout.break_all_lines(max_width);
        layout.align(Alignment::Start, AlignmentOptions::default());

        let build_time = build_start.elapsed();

        // Re-break at a slightly different width. This exercises the path a
        // word processor takes on window resize, zoom and pagination changes,
        // and is the reason to prefer a library that keeps shaped glyphs around
        // rather than re-shaping from scratch every frame.
        let rebreak_width = max_width.map(|w| w * 0.8);
        let rebreak_start = Instant::now();
        layout.break_all_lines(rebreak_width);
        layout.align(Alignment::Start, AlignmentOptions::default());
        let rebreak_time = rebreak_start.elapsed();

        // Collect runs at the original width by re-breaking once more.
        layout.break_all_lines(max_width);
        layout.align(Alignment::Start, AlignmentOptions::default());

        let mut runs = Vec::new();
        let mut fonts: BTreeSet<usize> = BTreeSet::new();
        let mut notdef = 0usize;
        let mut glyph_count = 0usize;

        for line in layout.lines() {
            for item in line.items() {
                if let PositionedLayoutItem::GlyphRun(run) = item {
                    // Font identity by the address of the interned `FontData`.
                    // `SourceCache` holds one instance per font file, so equal
                    // pointers mean the same file was used. This is a proxy for
                    // "did fallback occur", which is all the spike asks.
                    let font_key = std::ptr::from_ref(run.run().font()) as *const _ as usize;
                    fonts.insert(font_key);

                    let text_range = run.run().text_range();
                    let mut run_notdef = 0usize;
                    let mut run_glyphs = 0usize;
                    for glyph in run.glyphs() {
                        run_glyphs += 1;
                        if glyph.id == NOTDEF {
                            run_notdef += 1;
                        }
                    }

                    notdef += run_notdef;
                    glyph_count += run_glyphs;

                    runs.push(RunSummary {
                        start: text_range.start,
                        end: text_range.end,
                        font_key,
                        glyph_count: run_glyphs,
                        notdef: run_notdef,
                        x: run.offset(),
                        y: run.baseline(),
                        width: run.advance(),
                    });
                }
            }
        }

        // Identify the exact characters that produced `.notdef`, by asking the
        // layout what it put at each source character boundary.
        let mut missing_chars = Vec::new();
        for (byte_idx, ch) in text.char_indices() {
            if ch.is_whitespace() {
                continue;
            }
            if let Some(cluster) = Cluster::from_byte_index(&layout, byte_idx)
                && cluster.glyphs().any(|g| g.id == NOTDEF)
            {
                missing_chars.push(ch);
            }
        }

        ParagraphSummary {
            sample_name,
            probes,
            line_count: layout.lines().count(),
            width: layout.width(),
            height: layout.height(),
            runs,
            distinct_fonts: fonts.len(),
            glyph_count,
            notdef,
            missing_chars,
            build_time,
            rebreak_time,
        }
    }

    /// Measure the *warm* re-layout path: build every layout once, then re-break
    /// all of them at a new width.
    ///
    /// This is the path a word processor actually takes while the user types or
    /// resizes — shaped glyphs are reused and only line breaking is redone. It is
    /// deliberately distinct from [`Self::layout_large`], which measures cold
    /// layout and includes all one-time font loading costs.
    pub fn rebreak_large(&mut self, text: &str, max_width: f32) -> Duration {
        // Cold pass: build everything so the measurement below is warm.
        let mut layouts = Vec::new();
        for para in text.split('\n') {
            let mut builder = self
                .layout_cx
                .ranged_builder(&mut self.font_cx, para, SCALE, true);
            builder.push_default(StyleProperty::FontSize(DEFAULT_FONT_SIZE));
            let mut layout = builder.build(para);
            layout.break_all_lines(Some(max_width));
            layout.align(Alignment::Start, AlignmentOptions::default());
            layouts.push(layout);
        }

        // Warm pass: only line breaking and alignment.
        let start = Instant::now();
        for layout in &mut layouts {
            layout.break_all_lines(Some(max_width * 0.85));
            layout.align(Alignment::Start, AlignmentOptions::default());
        }
        start.elapsed()
    }

    /// Lay out a long multi-paragraph body and report total time.
    ///
    /// This is the closest approximation we can get to "re-layout a large
    /// document" before the page layout engine exists.
    pub fn layout_large(&mut self, text: &str, max_width: f32) -> (Duration, usize, usize) {
        let start = Instant::now();
        let mut lines = 0usize;
        let mut notdef = 0usize;

        for para in text.split('\n') {
            let mut builder = self
                .layout_cx
                .ranged_builder(&mut self.font_cx, para, SCALE, true);
            builder.push_default(StyleProperty::FontSize(DEFAULT_FONT_SIZE));
            let mut layout = builder.build(para);
            layout.break_all_lines(Some(max_width));
            layout.align(Alignment::Start, AlignmentOptions::default());

            lines += layout.lines().count();
            for line in layout.lines() {
                for item in line.items() {
                    if let PositionedLayoutItem::GlyphRun(run) = item {
                        notdef += run.glyphs().filter(|g| g.id == NOTDEF).count();
                    }
                }
            }
        }

        (start.elapsed(), lines, notdef)
    }
}
