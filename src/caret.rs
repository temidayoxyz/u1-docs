//! Caret geometry, hit testing and cursor navigation.
//!
//! This is **Spike B, criterion 2** of [`PLAN.md`]: can we put a caret in the
//! right place, and move it correctly through mixed-direction text?
//!
//! ## Why this is Rust, not webview
//!
//! ADR-0003 puts the document surface on a canvas driven by a Rust layout
//! engine. That means the geometry a browser would normally give us for free —
//! caret rectangles, hit testing, cursor movement — is **ours to compute**. It
//! is not spike scaffolding; it is the foundation of Phase 3's editing surface,
//! and it is the part that cannot be retrofitted cheaply.
//!
//! ## The hard parts, and why they are hard
//!
//! **Affinity.** A byte offset is not a position. The same offset can sit at the
//! left or the right edge of a cluster depending on which side you approach it
//! from. In LTR text this is invisible; in RTL text and around combining marks it
//! decides whether typing inserts before or after the character the user sees
//! the caret next to. Every caret query here takes an explicit [`Affinity`]
//! rather than pretending the offset is unambiguous.
//!
//! **Visual vs logical order.** Arrow keys move *visually*. `next_logical` and
//! `next_visual` differ inside an RTL run, and an editor that conflates them
//! makes the left arrow key jump to the wrong side of an Arabic sentence.
//!
//! **Cluster boundaries.** Byte offsets into UTF-8 can land mid-character, and
//! grapheme clusters can span several code points (a Devanagari consonant plus
//! its matras is one cluster). Caret positions exist only at cluster boundaries;
//! snapping to them is the editor's job, not the string's.

use std::ops::Range;

use parley::{
    Alignment, AlignmentOptions, Cluster, ClusterSide, FontContext, LayoutContext,
    PositionedLayoutItem, StyleProperty,
};

/// Which edge of a cluster a caret sits on.
///
/// Named for position within the cluster rather than "left"/"right", because
/// the visual side depends on direction: in RTL text the `Start` edge is on the
/// *right* of the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Affinity {
    /// Before the cluster's first byte, in logical order.
    Start,
    /// After the cluster's last byte, in logical order.
    End,
}

/// An axis-aligned rectangle in layout coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    /// Horizontal midpoint, for hit testing.
    pub fn center_x(&self) -> f32 {
        self.x + self.width / 2.0
    }

    /// Vertical midpoint, for hit testing.
    pub fn center_y(&self) -> f32 {
        self.y + self.height / 2.0
    }

    /// The pixel column a caret occupies.
    ///
    /// A caret is a hairline, but a zero-width rect breaks `point-in-rect`
    /// hit testing and mispositions IME candidate windows on some platforms.
    pub fn caret_rect(x: f32, y: f32, height: f32, thickness: f32) -> Self {
        Self {
            x,
            y,
            width: thickness.max(1.0),
            height,
        }
    }
}

/// Where a pointer landed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    /// Byte offset of the cluster that was hit.
    pub byte_index: usize,
    /// Which visual side of that cluster was hit.
    pub side: ClusterSide,
    /// True when the hit was below the last line, i.e. past the end of text.
    pub after_end: bool,
}

/// A paragraph laid out and ready for caret queries.
///
/// Holds the layout and the text it was built from. Caret queries are cheap:
/// `Cluster::from_byte_index` is a lookup, not a re-layout.
pub struct CaretMap {
    layout: parley::Layout<[u8; 4]>,
    text: String,
    scale: f32,
}

impl CaretMap {
    /// Build a caret map for `text`, wrapped at `max_width` if given.
    pub fn build(
        layout_cx: &mut LayoutContext,
        font_cx: &mut FontContext,
        text: &str,
        max_width: Option<f32>,
        font_size: f32,
        scale: f32,
    ) -> Self {
        let mut builder = layout_cx.ranged_builder(font_cx, text, scale, true);
        builder.push_default(StyleProperty::FontSize(font_size));

        let mut layout = builder.build(text);
        layout.break_all_lines(max_width);
        layout.align(Alignment::Start, AlignmentOptions::default());

        Self {
            layout,
            text: text.to_string(),
            scale,
        }
    }

    /// Build with the default display scale of 1.0.
    pub fn build_default(
        layout_cx: &mut LayoutContext,
        font_cx: &mut FontContext,
        text: &str,
        max_width: Option<f32>,
        font_size: f32,
    ) -> Self {
        Self::build(layout_cx, font_cx, text, max_width, font_size, 1.0)
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn layout(&self) -> &parley::Layout<[u8; 4]> {
        &self.layout
    }

    pub fn line_count(&self) -> usize {
        self.layout.lines().count()
    }

    pub fn width(&self) -> f32 {
        self.layout.width()
    }

    pub fn height(&self) -> f32 {
        self.layout.height()
    }

    /// True when the paragraph's base direction resolves to right-to-left.
    pub fn is_rtl(&self) -> bool {
        self.layout.is_rtl()
    }

    /// Re-break at a new width without re-shaping.
    ///
    /// This is the window-resize path. Cheap by design; see Spike A finding 4.
    pub fn rebreak(&mut self, max_width: Option<f32>) {
        self.layout.break_all_lines(max_width);
        self.layout
            .align(Alignment::Start, AlignmentOptions::default());
    }

    /// Total advance of the given line.
    pub fn line_advance(&self, line_index: usize) -> Option<f32> {
        self.layout
            .lines()
            .nth(line_index)
            .map(|l| l.metrics().advance)
    }

    /// Height of the given line.
    pub fn line_height(&self, line_index: usize) -> Option<f32> {
        self.layout
            .lines()
            .nth(line_index)
            .map(|l| l.metrics().line_height)
    }

    /// The caret rectangle at `byte_index`, on the given edge of its cluster.
    ///
    /// This is the geometry an IME candidate window needs to be positioned, and
    /// the geometry a renderer needs to draw the caret. Returns `None` when the
    /// index is out of range.
    pub fn caret_rect(&self, byte_index: usize, affinity: Affinity) -> Option<Rect> {
        let thickness = 1.0 * self.scale;

        // End of text is a caret position but belongs to no cluster, so it has
        // to be handled separately: it is the single most important position in
        // an editor, being where you are after typing to the end.
        if byte_index >= self.text.len() {
            return self.end_of_line_caret(byte_index, thickness);
        }

        let cluster = Cluster::from_byte_index(&self.layout, byte_index)?;
        let line_index = self.line_index_at(byte_index)?;

        let x = match (affinity, cluster.is_rtl()) {
            // In LTR, Start is the left edge. In RTL, Start is the right edge.
            (Affinity::Start, false) | (Affinity::End, true) => cluster.visual_offset()?,
            (Affinity::Start, true) | (Affinity::End, false) => {
                cluster.visual_offset()? + cluster.advance()
            }
        };

        // y comes from `line_top`, never from `LineMetrics.offset` — that field
        // is the horizontal alignment offset and is 0 on every line.
        Some(Rect::caret_rect(
            x,
            self.line_top(line_index)?,
            self.line_height(line_index)?,
            thickness,
        ))
    }

    /// Caret at the end of the last line, for the end-of-text case.
    ///
    /// End of text belongs to no cluster — it is the boundary after the final
    /// one — so it has to be placed from line geometry instead of a cluster's
    /// visual offset.
    fn end_of_line_caret(&self, _byte_index: usize, thickness: f32) -> Option<Rect> {
        let last = self.line_count().checked_sub(1)?;
        let line = self.layout.lines().nth(last)?;
        let metrics = line.metrics();

        // For a wholly RTL final line the end of text is at the left edge of the
        // line; otherwise at the right, past the last glyph.
        //
        // `Run::is_rtl` rather than a style lookup: the run carries the resolved
        // direction for that span, which is what decides where the visual end is.
        let rtl = line.items().any(|item| match item {
            PositionedLayoutItem::GlyphRun(run) => run.run().is_rtl(),
            PositionedLayoutItem::InlineBox(_) => false,
        });
        let x = if rtl {
            metrics.offset
        } else {
            metrics.offset + metrics.advance
        };

        Some(Rect::caret_rect(
            x,
            self.line_top(last)?,
            self.line_height(last)?,
            thickness,
        ))
    }

    /// Top edge of a line, in layout coordinates.
    ///
    /// ## Why this is computed rather than read
    ///
    /// `LineMetrics.offset` is **not** a vertical position — it is the
    /// horizontal alignment offset, and it is `0` for every line under
    /// `Alignment::Start`. `LineMetrics.baseline` is likewise an offset *within*
    /// the line, not its y position.
    ///
    /// parley 0.11 exposes no per-line y accessor: `Line` has no `index` and no
    /// vertical offset, and its geometry fields are `pub(crate)`. A test run of
    /// Spike B caught this — vertical navigation silently never left line 0,
    /// because every line reported `y = 0` and hit testing kept resolving to the
    /// first line.
    ///
    /// The correct model is a stacked accumulation of `line_height`, which is
    /// exactly how `Layout::height()` is defined (verified: the sum equals the
    /// layout height to the last bit). Lines are uniform in height here because
    /// each line holds a single style; mixed line heights will need a prefix-sum
    /// table, which this function is shaped to become.
    pub fn line_top(&self, line_index: usize) -> Option<f32> {
        let mut y = 0.0f32;
        for (i, line) in self.layout.lines().enumerate() {
            if i == line_index {
                return Some(y);
            }
            y += line.metrics().line_height;
        }
        None
    }

    /// Vertical midpoint of a line, for hit testing.
    pub fn line_mid_y(&self, line_index: usize) -> Option<f32> {
        let top = self.line_top(line_index)?;
        Some(top + self.line_height(line_index)? / 2.0)
    }

    /// Index of the line containing `byte_index`.
    ///
    /// `Line::index` is private in parley 0.11, so the line is located by
    /// enumerating and matching its text range.
    ///
    /// End of text belongs to the **last** line, not to a line of its own: the
    /// final line's range is exclusive at its end, so a plain `contains` check
    /// reports no line for `text.len()`. That matters because end of text is
    /// where the caret sits after typing to the end, and because vertical
    /// navigation needs to know whether it is on the last line to decide that
    /// Down is blocked.
    pub fn line_index_at(&self, byte_index: usize) -> Option<usize> {
        let idx = self.snap_to_cluster_boundary(byte_index);
        let count = self.line_count();
        if count == 0 {
            return None;
        }

        if let Some(i) = self.layout.lines().position(|l| {
            let r = l.text_range();
            r.contains(&idx) || (r.is_empty() && r.start == idx)
        }) {
            return Some(i);
        }

        // End of text (and anything at or past it) sits after the last line.
        if idx >= self.text.len() {
            Some(count - 1)
        } else {
            None
        }
    }

    /// The cluster containing `byte_index`.
    pub fn cluster_at(&self, byte_index: usize) -> Option<Cluster<'_, [u8; 4]>> {
        Cluster::from_byte_index(&self.layout, byte_index)
    }

    /// Byte range of the cluster at `byte_index`.
    pub fn cluster_range(&self, byte_index: usize) -> Option<Range<usize>> {
        self.cluster_at(byte_index).map(|c| c.text_range())
    }

    /// Hit test a point in layout coordinates.
    ///
    /// `exact` uses [`Cluster::from_point_exact`], which never rounds to the
    /// nearest cluster — it returns `None` outside a cluster. That matters for
    /// clicks in the trailing space after the last glyph, where rounding would
    /// put the caret after text the user did not click on.
    pub fn hit_test(&self, x: f32, y: f32, exact: bool) -> Option<Hit> {
        if exact {
            let (cluster, side) = Cluster::from_point_exact(&self.layout, x, y)?;
            Some(Hit {
                byte_index: cluster.text_range().start,
                side,
                after_end: false,
            })
        } else {
            let (cluster, side) = Cluster::from_point(&self.layout, x, y)?;
            Some(Hit {
                byte_index: cluster.text_range().start,
                side,
                after_end: false,
            })
        }
    }

    /// Snap a raw byte offset to the nearest valid caret position.
    ///
    /// UTF-8 byte offsets can land mid-character. Every caret operation in an
    /// editor must go through this, or typing after an emoji can panic or
    /// produce invalid UTF-8.
    pub fn snap_to_cluster_boundary(&self, byte_index: usize) -> usize {
        if byte_index >= self.text.len() {
            return self.text.len();
        }
        // Walk back to the nearest char boundary first: from_byte_index requires
        // one, and would otherwise miss the cluster we are aiming at.
        let mut idx = byte_index;
        while idx > 0 && !self.text.is_char_boundary(idx) {
            idx -= 1;
        }

        let cluster_start = self
            .cluster_at(idx)
            .map(|c| c.text_range().start)
            .unwrap_or(idx);

        // Choose the nearer edge of the cluster in byte terms.
        let Some(range) = self.cluster_at(idx).map(|c| c.text_range()) else {
            return idx;
        };
        if byte_index - cluster_start <= range.end - byte_index {
            range.start
        } else {
            range.end
        }
    }

    /// Every valid caret position in the paragraph, in logical order.
    ///
    /// Built from `Run::clusters()`, which yields the shaped clusters directly.
    ///
    /// The earlier version walked *character* boundaries inside each run, which
    /// is wrong for exactly the scripts that matter: a Devanagari consonant plus
    /// its matras is one grapheme cluster spanning several code points, so
    /// per-character offsets invent caret positions inside a single glyph. The
    /// spike caught this as "3 chars, 4 clusters" — more caret stops than
    /// characters, which is impossible for a well-formed cluster walk.
    pub fn caret_positions(&self) -> Vec<usize> {
        let mut out: Vec<usize> = Vec::new();
        for line in self.layout.lines() {
            for run in line.runs() {
                for cluster in run.clusters() {
                    let range = cluster.text_range();
                    out.push(range.start);
                    out.push(range.end);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out.retain(|&b| self.text.is_char_boundary(b));
        out
    }

    /// Caret positions filtered to **grapheme cluster** boundaries.
    ///
    /// ## Why this exists separately from [`Self::caret_positions`]
    ///
    /// parley's `Cluster` is the *shaping* cluster, and in parley 0.11 it is
    /// per-codepoint rather than per-grapheme. Measured in Spike B:
    ///
    /// | Input                    | Codepoints | parley clusters |
    /// | ------------------------ | ---------- | --------------- |
    /// | `e` + U+0301             | 2          | **2**           |
    /// | Devanagari ka + i-matra  | 2          | **2**           |
    /// | ZWJ family emoji         | 5          | **5**           |
    ///
    /// So a caret walk over raw cluster edges offers a stop *between a base
    /// letter and its combining accent*. That looks wrong, and typing at such a
    /// position produces mojibake — the mark gets orphaned.
    ///
    /// This is not a parley bug: shaping granularity and grapheme granularity
    /// are different questions, and parley answers the first. But it means
    /// **grapheme segmentation is ours**, and it is not optional.
    ///
    /// The filter below is a conservative approximation sufficient to keep caret
    /// moves and insertions safe: it excludes stops that fall inside a combining
    /// sequence. Phase 1 replaces it with a proper UAX #29 implementation over
    /// `unicode-segmentation`, which also handles emoji ZWJ sequences, regional
    /// indicator pairs and Hangul jamo.
    ///
    /// Until then, callers doing **insertion** must use this list. Callers doing
    /// only geometry (drawing, hit testing, IME anchoring) can use the raw one.
    pub fn grapheme_caret_positions(&self) -> Vec<usize> {
        self.caret_positions()
            .into_iter()
            .filter(|&b| self.is_grapheme_boundary(b))
            .collect()
    }

    /// Conservative grapheme-boundary test: reject stops that sit inside a
    /// combining sequence.
    ///
    /// The leading-base rule is what matters in practice — UAX #29 GB9, "do not
    /// break before extending characters" — and it is exactly the case where a
    /// naive caret walk produces visibly wrong behaviour.
    fn is_grapheme_boundary(&self, byte: usize) -> bool {
        if byte == 0 || byte >= self.text.len() {
            return true;
        }
        let Some(ch) = self.text[byte..].chars().next() else {
            return true;
        };
        !is_extending(ch)
    }
}

/// Does this code point extend the character before it?
///
/// A conservative stand-in for UAX #29 GB9 ("do not break before extending
/// characters") until Phase 1 brings in a full segmentation implementation.
///
/// Covering the marks that actually appear in the corpus and in real documents
/// matters more than covering the entire `Grapheme_Extend` table: getting the
/// common cases right stops the caret landing inside visible text, which is the
/// bug users notice.
fn is_extending(c: char) -> bool {
    matches!(c as u32,
        // Combining Diacritical Marks
        0x0300..=0x036F
        // Hebrew points
        | 0x0591..=0x05BD
        | 0x05BF
        | 0x05C1..=0x05C2
        | 0x05C4..=0x05C5
        | 0x05C7
        // Arabic marks
        | 0x0610..=0x061A
        | 0x064B..=0x065F
        | 0x0670
        | 0x06D6..=0x06DC
        | 0x06DF..=0x06E4
        | 0x06E7..=0x06E8
        | 0x06EA..=0x06ED
        // Devanagari and Indic matras / vowel signs
        | 0x0900..=0x0903
        | 0x093A..=0x094F
        | 0x0951..=0x0957
        | 0x0962..=0x0963
        // Thai vowel signs and tone marks
        | 0x0E31
        | 0x0E34..=0x0E3A
        | 0x0E47..=0x0E4E
        // Zero-width joiner: must never be split from its neighbours, which is
        // what makes a ZWJ emoji sequence a single caret stop.
        | 0x200D
        // Variation selectors
        | 0xFE00..=0xFE0F
        | 0xE0100..=0xE01EF
    )
}

/// Direction a cursor movement is stepping in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Move to the next character in logical (text) order.
    LogicalForward,
    /// Move to the previous character in logical order.
    LogicalBack,
    /// Move one position to the right on screen.
    VisualRight,
    /// Move one position to the left on screen.
    VisualLeft,
    /// Move to the line above.
    Up,
    /// Move to the line below.
    Down,
}

/// Result of a cursor movement attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    /// Moved to this byte offset.
    To(usize),
    /// Could not move further in this direction.
    Blocked,
}

/// Move the caret from `from` in `direction`.
///
/// Visual movement is what arrow keys do. Inside an RTL run it is the *opposite*
/// of logical movement, and conflating the two is the classic bidi editing bug:
/// the left arrow key appears to jump the wrong way.
///
/// Up and Down preserve the desired x position across lines, which is why this
/// takes `goal_x` rather than computing a naive midpoint.
pub fn move_caret(map: &CaretMap, from: usize, direction: Direction, goal_x: Option<f32>) -> Move {
    let from = map.snap_to_cluster_boundary(from);

    match direction {
        Direction::LogicalForward => {
            if from >= map.text().len() {
                return Move::Blocked;
            }
            match map.cluster_at(from) {
                Some(c) => Move::To(c.text_range().end),
                None => Move::Blocked,
            }
        }
        Direction::LogicalBack => {
            if from == 0 {
                return Move::Blocked;
            }
            // Backspace-style stepping: go to the start of the cluster the caret
            // is inside, and only step past it if the caret was already there.
            // This is what makes backspace behave over a multi-code-point
            // grapheme cluster instead of eating one byte at a time.
            let current_start = map.cluster_at(from).map(|c| c.text_range().start);
            let target = match current_start {
                Some(s) if s == from => from.saturating_sub(1),
                Some(_) => from,
                None => from.saturating_sub(1),
            };
            let snapped = map.snap_to_cluster_boundary(target);
            if snapped == from {
                // Snapping collapsed the step; fall back to a strict decrement
                // so progress is guaranteed, or report that we are stuck.
                if from == 0 {
                    Move::Blocked
                } else {
                    Move::To(from - 1)
                }
            } else {
                Move::To(snapped)
            }
        }
        Direction::VisualRight | Direction::VisualLeft => {
            let forward = direction == Direction::VisualRight;
            let Some(cluster) = map.cluster_at(from) else {
                return Move::Blocked;
            };

            // Determine the x we are currently at, then step to the neighbouring
            // caret position in the requested screen direction.
            let current_x = match cluster.is_rtl() {
                // In RTL, the caret at a cluster start is on its right edge.
                true => match map.caret_rect(from, Affinity::Start) {
                    Some(r) => r.x,
                    None => return Move::Blocked,
                },
                false => match map.caret_rect(from, Affinity::Start) {
                    Some(r) => r.x,
                    None => return Move::Blocked,
                },
            };

            let candidates = map.caret_positions();
            let mut best: Option<(usize, f32)> = None;

            for &pos in &candidates {
                if pos == from {
                    continue;
                }
                // Both edges of each candidate position.
                for aff in [Affinity::Start, Affinity::End] {
                    let Some(rect) = map.caret_rect(pos, aff) else {
                        continue;
                    };
                    let delta = rect.x - current_x;
                    let ok = if forward { delta > 0.5 } else { delta < -0.5 };
                    if ok && best.is_none_or(|(_, best_delta)| delta.abs() < best_delta.abs()) {
                        best = Some((pos, delta));
                    }
                }
            }

            match best {
                Some((pos, _)) => Move::To(map.snap_to_cluster_boundary(pos)),
                None => Move::Blocked,
            }
        }
        Direction::Up | Direction::Down => {
            let Some(current_line) = map.line_index_at(from) else {
                return Move::Blocked;
            };
            let target_index = if direction == Direction::Up {
                match current_line.checked_sub(1) {
                    Some(i) => i,
                    None => return Move::Blocked,
                }
            } else {
                let next = current_line + 1;
                if next >= map.line_count() {
                    return Move::Blocked;
                }
                next
            };

            let desired_x = goal_x.unwrap_or_else(|| {
                map.caret_rect(from, Affinity::Start)
                    .map_or(0.0, |r| r.center_x())
            });

            // `line_mid_y`, not `metrics.offset` — see `line_top`. Using the
            // metrics offset here is what made vertical navigation a no-op.
            let Some(ty) = map.line_mid_y(target_index) else {
                return Move::Blocked;
            };

            match map.hit_test(desired_x, ty, false) {
                Some(hit) => Move::To(map.snap_to_cluster_boundary(hit.byte_index)),
                None => Move::Blocked,
            }
        }
    }
}

/// Total horizontal offset of a line's ink.
pub fn line_ink_width(map: &CaretMap, line_index: usize) -> Option<f32> {
    let line = map.layout().lines().nth(line_index)?;
    let mut max_x: f32 = 0.0;
    for item in line.items() {
        if let PositionedLayoutItem::GlyphRun(run) = item {
            max_x = max_x.max(run.offset() + run.advance());
        }
    }
    Some(max_x)
}
