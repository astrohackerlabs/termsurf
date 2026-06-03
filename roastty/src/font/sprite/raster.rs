//! Anti-aliased vector rasterization for the sprite font.
//!
//! Faithful port of the `z2d` vector-graphics library (vendored at
//! `vendor/z2d/`), which upstream `font/sprite/Canvas.zig` uses for its
//! anti-aliased path methods (`line`/`fill`/`stroke`). The fill pipeline is
//! `path → fill_plotter → Polygon → multisample rasterizer → surface`. This
//! module starts with the foundational [`Polygon`] tessellation core (a list of
//! oriented [`Edge`]s with bounding extents); the rasterizer and the
//! fill/stroke plotters are later slices.

/// A 2D point in floating-point device space. Faithful port of z2d's internal
/// `Point` (only `x`/`y` are needed here).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub(crate) fn new(x: f64, y: f64) -> Point {
        Point { x, y }
    }
}

/// A single non-horizontal polygon edge. Faithful port of z2d's
/// `tess.Polygon.Edge`: `y0`/`y1` keep the original vertex order (for the
/// winding [`dir`](Edge::dir)), while `x_start` is the x at the **top** (min-y)
/// vertex and `x_inc` is the downward slope `Δx/Δy`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Edge {
    pub y0: f64,
    pub y1: f64,
    pub x_start: f64,
    pub x_inc: f64,
}

impl Edge {
    /// The winding direction: `-1` for a down edge (`y0 < y1`), else `+1`.
    pub(crate) fn dir(&self) -> i8 {
        if self.y0 < self.y1 {
            -1
        } else {
            1
        }
    }

    /// The top (minimum) y of the edge.
    pub(crate) fn top(&self) -> f64 {
        if self.y0 < self.y1 {
            self.y0
        } else {
            self.y1
        }
    }

    /// The bottom (maximum) y of the edge.
    pub(crate) fn bottom(&self) -> f64 {
        if self.y0 < self.y1 {
            self.y1
        } else {
            self.y0
        }
    }
}

/// A tessellated polygon: a set of oriented edges with bounding extents (in
/// scaled device space). Faithful port of z2d's `tess.Polygon`.
#[derive(Debug, Clone)]
pub(crate) struct Polygon {
    pub edges: Vec<Edge>,
    /// Scale applied to points added via [`add_edge`](Polygon::add_edge). Only
    /// relevant when adding edges directly (not from contours).
    pub scale: f64,
    pub extent_top: f64,
    pub extent_bottom: f64,
    pub extent_left: f64,
    pub extent_right: f64,
}

impl Polygon {
    /// An empty polygon with the given `scale`.
    pub(crate) fn new(scale: f64) -> Polygon {
        Polygon {
            edges: Vec::new(),
            scale,
            extent_top: 0.0,
            extent_bottom: 0.0,
            extent_left: 0.0,
            extent_right: 0.0,
        }
    }

    /// Add the edge `p0 → p1` (scaled by [`scale`](Polygon::scale)). Horizontal
    /// edges are filtered out. Faithful port of z2d's `addEdge`.
    pub(crate) fn add_edge(&mut self, p0: Point, p1: Point) {
        assert!(p0.x.is_finite() && p0.y.is_finite());
        assert!(p1.x.is_finite() && p1.y.is_finite());
        let p0s = Point::new(p0.x * self.scale, p0.y * self.scale);
        let p1s = Point::new(p1.x * self.scale, p1.y * self.scale);

        let edge = if p0s.y < p1s.y {
            // Down edge.
            Edge {
                y0: p0s.y,
                y1: p1s.y,
                x_start: p0s.x,
                x_inc: (p1s.x - p0s.x) / (p1s.y - p0s.y),
            }
        } else if p0s.y > p1s.y {
            // Up edge.
            Edge {
                y0: p0s.y,
                y1: p1s.y,
                x_start: p1s.x,
                x_inc: (p0s.x - p1s.x) / (p0s.y - p1s.y),
            }
        } else {
            // Horizontal edge — filtered out.
            return;
        };

        let extent_top = edge.top();
        let extent_bottom = edge.bottom();
        let (extent_left, extent_right) = if p0s.x < p1s.x {
            (p0s.x, p1s.x)
        } else {
            (p1s.x, p0s.x)
        };
        if self.edges.is_empty() {
            self.extent_top = extent_top;
            self.extent_bottom = extent_bottom;
            self.extent_left = extent_left;
            self.extent_right = extent_right;
        } else {
            if extent_top < self.extent_top {
                self.extent_top = extent_top;
            }
            if extent_bottom > self.extent_bottom {
                self.extent_bottom = extent_bottom;
            }
            if extent_left < self.extent_left {
                self.extent_left = extent_left;
            }
            if extent_right > self.extent_right {
                self.extent_right = extent_right;
            }
        }

        self.edges.push(edge);
    }

    /// Whether the polygon intersects the box `(0,0)..(box_width, box_height)`
    /// (in device pixels, after dividing the scaled extents by `scale`). Used to
    /// decide whether to rasterize. Faithful port of z2d's `inBox`.
    pub(crate) fn in_box(&self, scale: f64, box_width: i32, box_height: i32) -> bool {
        assert!(
            self.extent_left.is_finite()
                && self.extent_top.is_finite()
                && self.extent_right.is_finite()
                && self.extent_bottom.is_finite(),
            "invalid polygon dimensions"
        );
        assert!(scale.is_finite() && scale >= 1.0, "invalid value for scale");
        assert!(
            box_width >= 1 && box_height >= 1,
            "invalid box width or height"
        );

        // Round the polygon out to whole device pixels.
        let poly_start_x = (self.extent_left / scale).floor() as i32;
        let poly_start_y = (self.extent_top / scale).floor() as i32;
        let poly_end_x = (self.extent_right / scale).ceil() as i32;
        let poly_end_y = (self.extent_bottom / scale).ceil() as i32;

        let poly_width = poly_end_x - poly_start_x;
        let poly_height = poly_end_y - poly_start_y;

        assert!(
            poly_width >= 0 && poly_height >= 0,
            "negative polygon width or height"
        );

        // A zero-area (degenerate) polygon draws nothing.
        if poly_width == 0 || poly_height == 0 {
            return false;
        }

        // With negative start offsets, make sure we still reach the surface.
        if poly_start_x + poly_width < 0 || poly_start_y + poly_height < 0 {
            return false;
        }

        // Outside the right/upper bounds of the surface.
        if poly_start_x >= box_width || poly_start_y >= box_height {
            return false;
        }

        true
    }
}

/// The polygon fill rule. Faithful port of z2d's `options.FillRule`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FillRule {
    NonZero,
    EvenOdd,
}

/// The active-edge-table the scanline rasterizer drives. For any sub-scanline it
/// yields the sorted, fill-rule-filtered x-crossings that bound the filled
/// spans. Faithful port of z2d's `Polygon.WorkingEdgeSet`.
///
/// Upstream reorders the source polygon's edge array as scratch; this port owns
/// a copy of the edges and permutes that — behaviorally identical, since
/// [`rescan`](WorkingEdgeSet::rescan) re-partitions the full set each call and
/// [`breakpoints`](WorkingEdgeSet::breakpoints) is computed once up front.
pub(crate) struct WorkingEdgeSet {
    /// Owned copy of the polygon's edges, permuted as scratch.
    edges: Vec<Edge>,
    /// The number of active edges (the working prefix length).
    active: usize,
    /// The x-crossing of each active edge at the current sub-scanline.
    x_values: Vec<i32>,
}

impl WorkingEdgeSet {
    /// Create a working edge set over a copy of `polygon`'s edges.
    pub(crate) fn new(polygon: &Polygon) -> WorkingEdgeSet {
        let n = polygon.edges.len();
        WorkingEdgeSet {
            edges: polygon.edges.clone(),
            active: 0,
            x_values: vec![0; n],
        }
    }

    /// The sorted, de-duplicated set of `round(top())`/`round(bottom())` for
    /// every edge — the scanlines at which the active set changes. Faithful port
    /// of `breakpoints` (binary-search insertion).
    pub(crate) fn breakpoints(&self) -> Vec<i32> {
        fn insert(list: &mut Vec<i32>, value: i32) {
            if list.is_empty() {
                list.push(value);
                return;
            }
            let mut low = 0usize;
            let mut high = list.len();
            while low < high {
                let mid = low + (high - low) / 2;
                if list[mid] < value {
                    low = mid + 1;
                } else {
                    high = mid;
                }
            }
            let idx = low;
            if idx == list.len() {
                list.push(value);
            } else if list[idx] != value {
                list.insert(idx, value);
            }
        }

        let mut result = Vec::with_capacity(self.edges.len() * 2);
        for e in &self.edges {
            insert(&mut result, e.top().round() as i32);
            insert(&mut result, e.bottom().round() as i32);
        }
        result
    }

    /// Partition the edges so the active ones — `top() < line_y + 0.5` and
    /// `bottom() >= line_y + 0.5` (measured at the line middle to break ties on
    /// point boundaries) — are at the front, setting [`active`]. Faithful port
    /// of `rescan`.
    pub(crate) fn rescan(&mut self, line_y: i32) {
        if self.edges.is_empty() {
            self.active = 0;
            return;
        }
        let line_y_middle = line_y as f64 + 0.5;
        let mut to = 0usize;
        for from in 0..self.edges.len() {
            if self.edges[from].top() < line_y_middle && self.edges[from].bottom() >= line_y_middle
            {
                if from != to {
                    self.edges.swap(to, from);
                }
                to += 1;
            }
        }
        self.active = to;
    }

    /// Compute each active edge's x-crossing at `y + 0.5`. Faithful port of
    /// `inc`.
    pub(crate) fn inc(&mut self, y: i32) {
        let y_mid = y as f64 + 0.5;
        for idx in 0..self.active {
            let edge = self.edges[idx];
            self.x_values[idx] = (edge.x_start + edge.x_inc * (y_mid - edge.top())).round() as i32;
        }
    }

    /// Sort the active edges by their x-crossing (ascending), co-permuting the
    /// edges. Faithful port of `sort` (unstable).
    pub(crate) fn sort(&mut self) {
        let mut order: Vec<usize> = (0..self.active).collect();
        order.sort_unstable_by_key(|&i| self.x_values[i]);
        let new_x: Vec<i32> = order.iter().map(|&i| self.x_values[i]).collect();
        let new_edges: Vec<Edge> = order.iter().map(|&i| self.edges[i]).collect();
        self.x_values[..self.active].copy_from_slice(&new_x);
        self.edges[..self.active].copy_from_slice(&new_edges);
    }

    /// The fill-rule-filtered active x-crossings (consecutive pairs are span
    /// `[start, end)` bounds). `EvenOdd` is pass-through; `NonZero` keeps only
    /// the points where the winding number enters/leaves `0`. Faithful port of
    /// `filter`.
    pub(crate) fn filter(&mut self, fill_rule: FillRule) -> &[i32] {
        match fill_rule {
            FillRule::EvenOdd => &self.x_values[..self.active],
            FillRule::NonZero => {
                let mut winding_number: i32 = 0;
                let mut to = 0usize;
                for from in 0..self.active {
                    self.x_values[to] = self.x_values[from];
                    if winding_number == 0 {
                        winding_number += self.edges[from].dir() as i32;
                        to += 1;
                    } else {
                        winding_number += self.edges[from].dir() as i32;
                        if winding_number == 0 {
                            to += 1;
                        }
                    }
                }
                &self.x_values[..to]
            }
        }
    }
}

/// A run-length-encoded coverage accumulator for a single scanline. Faithful
/// port of z2d's `SparseCoverageBuffer` (itself derived from tiny-skia's
/// `alpha_runs`): only run-start indices hold meaningful `values[x]`/
/// `lengths[x]`; callers walk runs by reading [`get`](SparseCoverageBuffer::get)
/// and advancing by the returned length.
///
/// z2d picks a `u8`/`u16`/`u32` length-storage type by capacity purely to save
/// memory; this port uses `u32` throughout, which is behaviorally identical.
/// Per upstream's contract, the caller guarantees x-bounds and that coverage
/// values do not overflow.
pub(crate) struct SparseCoverageBuffer {
    values: Vec<u8>,
    lengths: Vec<u32>,
    len: u32,
    capacity: u32,
}

impl SparseCoverageBuffer {
    /// A zeroed coverage buffer of the given pixel `capacity`.
    pub(crate) fn new(capacity: u32) -> SparseCoverageBuffer {
        SparseCoverageBuffer {
            values: vec![0; capacity as usize],
            lengths: vec![0; capacity as usize],
            len: 0,
            capacity,
        }
    }

    /// Reset to empty (without reallocating).
    pub(crate) fn reset(&mut self) {
        self.len = 0;
        self.lengths[0] = 0;
    }

    /// The total covered extent.
    pub(crate) fn covered_len(&self) -> u32 {
        self.len
    }

    /// The `(value, length)` run starting at `x`.
    pub(crate) fn get(&self, x: u32) -> (u8, u32) {
        (self.values[x as usize], self.lengths[x as usize])
    }

    /// Write a run start `(value, len)` at `x`.
    fn put(&mut self, x: u32, value: u8, len: u32) {
        assert!(x + len <= self.capacity);
        self.values[x as usize] = value;
        self.lengths[x as usize] = len;
    }

    /// Write just the coverage value at `x` (leaving the run length).
    fn put_value(&mut self, x: u32, value: u8) {
        self.values[x as usize] = value;
    }

    /// Ensure runs exist so `[x, x + len)` can be addressed, splitting existing
    /// runs at `x` and `x + len` as needed. Faithful port of `extend`.
    fn extend(&mut self, x: u32, len: u32) {
        if len == 0 {
            return;
        }

        // x is fully out of range (at or past the end).
        if x == self.len {
            self.put(x, 0, len);
            self.len = x + len;
            return;
        }
        if x > self.len {
            self.put(self.len, 0, x - self.len);
            self.put(x, 0, len);
            self.len = x + len;
            return;
        }

        // Split from the front so the run at `x` starts there.
        self.split_inner(0, x);

        // Extend past the existing length if needed, then split the remainder.
        let span_len = x + len;
        if span_len > self.len {
            self.put(self.len, 0, span_len - self.len);
            self.len = span_len;
        }

        self.split_inner(x, len);
    }

    /// Walk runs from `x`; when the remaining `len` falls inside a run, split it
    /// into `(value, rem)` and `(value, current_len - rem)`. Faithful port of
    /// `splitInner`.
    fn split_inner(&mut self, x: u32, len: u32) {
        let mut idx = x;
        let mut rem = len;
        loop {
            let (current_value, current_len) = self.get(idx);
            if rem < current_len {
                self.put(idx, current_value, rem);
                self.put(idx + rem, current_value, current_len - rem);
                break;
            } else if rem == current_len {
                break;
            }
            rem -= current_len;
            idx += current_len;
        }
    }

    /// Add `value` coverage to every run across `[x, x + len)`. Faithful port of
    /// `addSpan`.
    pub(crate) fn add_span(&mut self, x: u32, value: u8, len: u32) {
        self.extend(x, len);
        let mut x_cur = x;
        let x_end = x + len;
        while x_cur < x_end {
            let (coverage_value, coverage_len) = self.get(x_cur);
            self.put_value(x_cur, coverage_value + value);
            x_cur += coverage_len;
        }
    }

    /// Add `value` coverage to the single pixel at `x`. Faithful port of
    /// `addSingle`.
    pub(crate) fn add_single(&mut self, x: u32, value: u8) {
        self.extend(x, 1);
        let (coverage_value, _) = self.get(x);
        self.put_value(x, coverage_value + value);
    }
}

/// The multisample anti-aliasing scale: each device pixel spans `MSAA_SCALE`
/// horizontal (and vertical) subpixels, for `MSAA_SCALE * MSAA_SCALE` = 16
/// samples per pixel. z2d's multisample `scale`.
pub(crate) const MSAA_SCALE: u32 = 4;

/// Record a filled span (in `MSAA_SCALE`-supersampled x-coordinates) into the
/// coverage buffer, distributing it into per-device-pixel coverage counts: a
/// fully-covered pixel gets `MSAA_SCALE` coverage, a partially-covered edge
/// pixel gets the subpixel fraction. Faithful port of z2d's module-private
/// `multisample.addSpan`. `x`/`len` must be pre-clamped non-negative.
fn add_supersampled_span(cb: &mut SparseCoverageBuffer, x: u32, len: u32) {
    assert!(
        x + len <= cb.capacity * MSAA_SCALE,
        "attempt to add span beyond capacity"
    );
    if len == 0 {
        return;
    }

    let start_x = x / MSAA_SCALE;
    let start_offset = x - start_x * MSAA_SCALE;

    if start_offset == 0 && len >= MSAA_SCALE {
        // Start coverage is full: write the opaque middle, then the tail.
        let front_len = len / MSAA_SCALE;
        cb.add_span(start_x, MSAA_SCALE as u8, front_len);
        let end_coverage = MSAA_SCALE.min(len - front_len * MSAA_SCALE);
        if end_coverage > 0 {
            cb.add_single(start_x + front_len, end_coverage as u8);
        }
    } else {
        // Starts mid-pixel: leading partial, full middle, trailing partial.
        let start_coverage = MSAA_SCALE.min(len.min(MSAA_SCALE - start_offset));
        cb.add_single(start_x, start_coverage as u8);

        let after_start = len - start_coverage;
        let mid_len = after_start / MSAA_SCALE;
        if mid_len > 0 {
            cb.add_span(start_x + 1, MSAA_SCALE as u8, mid_len);
        }

        let end_coverage = MSAA_SCALE.min(after_start - mid_len * MSAA_SCALE);
        if end_coverage > 0 {
            cb.add_single(start_x + 1 + mid_len, end_coverage as u8);
        }
    }
}

/// `src_over` composite of an opaque-`.on` source (modulated to `alpha`) over the
/// destination alpha8 value `dst`. Faithful port of z2d's integer `srcOver` for
/// the single-channel case: `out = alpha + dst - trunc(alpha * dst / 255)`
/// (z2d's `mul` truncates).
fn src_over_alpha8(dst: u8, alpha: u8) -> u8 {
    (alpha as u32 + dst as u32 - (alpha as u32 * dst as u32) / 255) as u8
}

/// Rasterize `polygon` into the alpha8 `buf` (`width * height`, row-major) with
/// 4× multisample anti-aliasing, filling with the opaque `.on` source (255)
/// under `src_over`. Faithful port of z2d's `multisample.run`, specialized to
/// the sprite case (alpha8 surface, opaque source, bounded `src_over` operator —
/// so the unbounded-clear branches drop out).
pub(crate) fn fill_polygon(
    buf: &mut [u8],
    width: i32,
    height: i32,
    polygon: &Polygon,
    fill_rule: FillRule,
) {
    let scale = MSAA_SCALE as i32; // 4
    let coverage_full: u8 = (MSAA_SCALE * MSAA_SCALE) as u8; // 16
    let alpha_scale: i32 = 256 / coverage_full as i32; // 16

    if !polygon.in_box(MSAA_SCALE as f64, width, height) {
        return;
    }

    let scale_f = scale as f64;
    let start_scanline = ((polygon.extent_top / scale_f).floor() as i32).clamp(0, height - 1);
    let end_scanline =
        ((polygon.extent_bottom / scale_f).ceil() as i32).clamp(start_scanline, height - 1);
    let scanline_start_x = ((polygon.extent_left / scale_f).floor() as i32).clamp(0, width - 1);
    let scanline_end_x =
        ((polygon.extent_right / scale_f).ceil() as i32).clamp(scanline_start_x, width);
    let scanline_draw_width = scanline_end_x - scanline_start_x;
    if scanline_draw_width < 1 {
        return;
    }
    let scanline_start_x_scaled = scanline_start_x * scale;
    let scanline_draw_width_scaled = scanline_draw_width * scale;

    let mut coverage_buffer = SparseCoverageBuffer::new(scanline_draw_width as u32);
    let mut working = WorkingEdgeSet::new(polygon);
    let y_breakpoints = working.breakpoints();

    // The initial breakpoint index: the saturating predecessor of the first
    // breakpoint >= start_scanline. No-op if no breakpoint qualifies.
    let mut bp_idx: usize = {
        let mut found: Option<usize> = None;
        for (idx, &y) in y_breakpoints.iter().enumerate() {
            if y >= start_scanline {
                found = Some(idx.saturating_sub(1));
                break;
            }
        }
        match found {
            Some(i) => i,
            None => return,
        }
    };

    for y in start_scanline..=end_scanline {
        coverage_buffer.reset();
        let y_scaled = y * scale;
        for y_offset in 0..scale {
            let y_scanline_scaled = y_scaled + y_offset;
            if y_scanline_scaled >= y_breakpoints[bp_idx] {
                working.rescan(y_scanline_scaled);
                if bp_idx < y_breakpoints.len() - 1 {
                    bp_idx += 1;
                }
            }

            working.inc(y_scanline_scaled);
            working.sort();
            let filtered = working.filter(fill_rule);

            let mut x_min: i32 = 0;
            for pair in 0..filtered.len() / 2 {
                let start_x = x_min.max(filtered[pair * 2] - scanline_start_x_scaled);
                if start_x >= scanline_draw_width_scaled {
                    break;
                }
                let end_x = (filtered[pair * 2 + 1] - scanline_start_x_scaled)
                    .clamp(start_x, scanline_draw_width_scaled);
                let fill_len = end_x - start_x;
                if fill_len > 0 {
                    add_supersampled_span(
                        &mut coverage_buffer,
                        start_x.max(0) as u32,
                        fill_len.max(0) as u32,
                    );
                }
                x_min = end_x;
            }
        }

        // Write out the accumulated coverage runs for this pixel row.
        let coverage_x_max = coverage_buffer.len.min(coverage_buffer.capacity);
        let mut cov_x: u32 = 0;
        while cov_x < coverage_x_max {
            let x = cov_x as i32 + scanline_start_x;
            let (cov_raw, cov_len_raw) = coverage_buffer.get(cov_x);
            let coverage_val = cov_raw.min(coverage_full);
            let coverage_len = cov_len_raw.min((width - x).max(0) as u32);

            if coverage_val == 0 {
                // skip
            } else if coverage_val == coverage_full {
                // Fully opaque span: overwrite with the source (.on = 255).
                for k in 0..coverage_len as i32 {
                    buf[(y * width + x + k) as usize] = 255;
                }
            } else {
                let alpha = (coverage_val as i32 * alpha_scale - 1).clamp(0, 255) as u8;
                for k in 0..coverage_len as i32 {
                    let idx = (y * width + x + k) as usize;
                    buf[idx] = src_over_alpha8(buf[idx], alpha);
                }
            }

            cov_x += cov_len_raw;
        }
    }
}

/// A single path node. Faithful port of z2d's `PathNode` tagged union: the
/// drawing operations a path is built from. `CurveTo` is a cubic Bézier from the
/// current point through `p1`/`p2` to `p3`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PathNode {
    MoveTo(Point),
    LineTo(Point),
    CurveTo { p1: Point, p2: Point, p3: Point },
    ClosePath,
}

/// Whether every subpath in `nodes` is closed. Faithful port of z2d's
/// `PathNode.isClosedNodeSet`: empty is not closed; a `ClosePath` marks the
/// current subpath closed, any other drawing node reopens it, and a `MoveTo`
/// that is not the first node following an unclosed subpath ends the scan early.
pub(crate) fn is_closed_node_set(nodes: &[PathNode]) -> bool {
    if nodes.is_empty() {
        return false;
    }

    let mut closed = false;
    for (i, node) in nodes.iter().enumerate() {
        match node {
            PathNode::MoveTo(_) => {
                if !closed && i != 0 {
                    break;
                }
            }
            PathNode::ClosePath => closed = true,
            _ => closed = false,
        }
    }

    closed
}

/// Squared length `x² + y²`. Faithful port of z2d's `Spline.dotSq`.
fn dot_sq(x: f64, y: f64) -> f64 {
    x * x + y * y
}

/// The midpoint of `a` and `b` (`a + (b - a) / 2`). Faithful port of z2d's
/// `Spline.lerpHalf`.
fn lerp_half(a: Point, b: Point) -> Point {
    Point::new(a.x + (b.x - a.x) / 2.0, a.y + (b.y - a.y) / 2.0)
}

/// Four control points of a (sub)spline. Faithful port of z2d's `Spline.Knots`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Knots {
    a: Point,
    b: Point,
    c: Point,
    d: Point,
}

impl Knots {
    /// An upper bound on the squared error of approximating the spline by the
    /// chord `a → d`. Faithful port of z2d's `Knots.errorSq` (the Cairo metric):
    /// project the `b`/`c` control deltas onto the chord (clamped) and return the
    /// larger squared residual.
    fn error_sq(&self) -> f64 {
        let mut b_x_delta = self.b.x - self.a.x;
        let mut b_y_delta = self.b.y - self.a.y;
        let mut c_x_delta = self.c.x - self.a.x;
        let mut c_y_delta = self.c.y - self.a.y;

        if self.a.x != self.d.x || self.a.y != self.d.y {
            let d_x_delta = self.d.x - self.a.x;
            let d_y_delta = self.d.y - self.a.y;
            let d_dot_sq = dot_sq(d_x_delta, d_y_delta);

            let b_d_dot = b_x_delta * d_x_delta + b_y_delta * d_y_delta;
            if b_d_dot >= d_dot_sq {
                b_x_delta -= d_x_delta;
                b_y_delta -= d_y_delta;
            } else {
                b_x_delta -= b_d_dot / d_dot_sq * d_x_delta;
                b_y_delta -= b_d_dot / d_dot_sq * d_y_delta;
            }

            let c_d_dot = c_x_delta * d_x_delta + c_y_delta * d_y_delta;
            if c_d_dot >= d_dot_sq {
                c_x_delta -= d_x_delta;
                c_y_delta -= d_y_delta;
            } else {
                c_x_delta -= c_d_dot / d_dot_sq * d_x_delta;
                c_y_delta -= c_d_dot / d_dot_sq * d_y_delta;
            }
        }

        let b_err = dot_sq(b_x_delta, b_y_delta);
        let c_err = dot_sq(c_x_delta, c_y_delta);
        if b_err > c_err {
            b_err
        } else {
            c_err
        }
    }

    /// Midpoint (De Casteljau) subdivision: sets `self` to the first half
    /// (`a → middle`) and returns the second half (`middle → d`). Faithful port
    /// of z2d's `Knots.deCasteljau`.
    fn de_casteljau(&mut self) -> Knots {
        let ab = lerp_half(self.a, self.b);
        let bc = lerp_half(self.b, self.c);
        let cd = lerp_half(self.c, self.d);
        let abbc = lerp_half(ab, bc);
        let bccd = lerp_half(bc, cd);
        let final_pt = lerp_half(abbc, bccd);

        let result = Knots {
            a: final_pt,
            b: bccd,
            c: cd,
            d: self.d,
        };

        self.b = ab;
        self.c = abbc;
        self.d = final_pt;

        result
    }
}

/// Recursive flattening of `s1` into `out`, emitting `s1.a` (unless it equals the
/// original `start`) once the error drops below `tolerance`. Faithful port of
/// z2d's `Spline.decomposeInto`.
fn decompose_into(s1: &mut Knots, start: Point, tolerance: f64, out: &mut Vec<Point>) {
    if s1.error_sq() < tolerance {
        if s1.a != start {
            out.push(s1.a);
        }
        return;
    }

    let mut s2 = s1.de_casteljau();
    decompose_into(s1, start, tolerance, out);
    decompose_into(&mut s2, start, tolerance, out);
}

/// A cubic Bézier curve to be flattened into line segments. Faithful port of
/// z2d's `Spline` (derived from Cairo's `cairo-spline.c`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Spline {
    pub a: Point,
    pub b: Point,
    pub c: Point,
    pub d: Point,
    pub tolerance: f64,
}

impl Spline {
    /// Flatten the curve into a series of points appended to `out` (the analog
    /// of z2d's `line_to` plotting). Faithful port of `Spline.decompose`.
    pub(crate) fn decompose(&self, out: &mut Vec<Point>) {
        // Both tangents zero means this is just a straight line.
        if self.a == self.b && self.c == self.d {
            out.push(self.d);
            return;
        }

        let mut s1 = Knots {
            a: self.a,
            b: self.b,
            c: self.c,
            d: self.d,
        };
        decompose_into(&mut s1, self.a, self.tolerance * self.tolerance, out);
        out.push(self.d);
    }
}

/// A fixed buffer of points. Once full, the first `SPLIT` items are kept and the
/// rest FIFO-rotate (so with `SPLIT = 1`, `first` stays pinned to the subpath's
/// initial point while `last` follows the most recent). Faithful port of z2d's
/// `point_buffer.PointBuffer`.
struct PointBuffer<const SPLIT: usize, const LEN: usize> {
    items: [Point; LEN],
    len: usize,
}

impl<const SPLIT: usize, const LEN: usize> PointBuffer<SPLIT, LEN> {
    fn new() -> PointBuffer<SPLIT, LEN> {
        PointBuffer {
            items: [Point::new(0.0, 0.0); LEN],
            len: 0,
        }
    }

    fn add(&mut self, item: Point) {
        if self.len < LEN {
            self.items[self.len] = item;
            self.len += 1;
        } else {
            for idx in SPLIT..LEN - 1 {
                self.items[idx] = self.items[idx + 1];
            }
            self.items[LEN - 1] = item;
        }
    }

    fn reset(&mut self) {
        self.len = 0;
    }

    fn first(&self) -> Option<Point> {
        if self.len == 0 {
            None
        } else {
            Some(self.items[0])
        }
    }

    fn last(&self) -> Option<Point> {
        if self.len == 0 {
            None
        } else {
            Some(self.items[self.len - 1])
        }
    }

    /// `items[n]`, or `None` if out of range. Faithful port of `head`.
    fn head(&self, n: usize) -> Option<Point> {
        if n >= self.len {
            None
        } else {
            Some(self.items[n])
        }
    }

    /// `items[len - n]` (`n >= 1`), or `None` if out of range. Faithful port of
    /// `tail`.
    fn tail(&self, n: usize) -> Option<Point> {
        assert!(n != 0, "invalid tail index");
        if self.len < n {
            None
        } else {
            Some(self.items[self.len - n])
        }
    }
}

/// Plot a path's nodes into a fill `Polygon`. Faithful port of z2d's
/// `fill_plotter.plot`: walk the nodes, flattening each `CurveTo` via [`Spline`]
/// and adding each segment as a polygon edge. Malformed paths (a `LineTo`/
/// `CurveTo` with no current point) are `unreachable!` — the `Canvas` only emits
/// well-formed paths.
pub(crate) fn fill_plot(nodes: &[PathNode], scale: f64, tolerance: f64) -> Polygon {
    let mut result = Polygon::new(scale);
    let mut points: PointBuffer<1, 3> = PointBuffer::new();

    for (i, node) in nodes.iter().enumerate() {
        match *node {
            PathNode::MoveTo(p) => {
                // The auto-added move_to after a close_path is the last node.
                if i == nodes.len() - 1 {
                    break;
                }
                points.reset();
                points.add(p);
            }
            PathNode::LineTo(p) => match points.last() {
                Some(last) => {
                    if last != p {
                        result.add_edge(last, p);
                        points.add(p);
                    }
                }
                None => unreachable!("fill_plot: line_to with no current point"),
            },
            PathNode::CurveTo { p1, p2, p3 } => {
                let a = match points.last() {
                    Some(a) => a,
                    None => unreachable!("fill_plot: curve_to with no current point"),
                };
                let spline = Spline {
                    a,
                    b: p1,
                    c: p2,
                    d: p3,
                    tolerance,
                };
                let mut flat = Vec::new();
                spline.decompose(&mut flat);
                for pt in flat {
                    let last = points.last().expect("curve_to current point");
                    if last != pt {
                        result.add_edge(last, pt);
                        points.add(pt);
                    }
                }
            }
            PathNode::ClosePath => {
                // Only close a real subpath (>= 3 points); shorter ones are
                // degenerate and cleared on the next move_to.
                if points.len >= 3 {
                    let first = points.first().unwrap();
                    let last = points.last().unwrap();
                    if last != first {
                        result.add_edge(last, first);
                        points.add(first);
                    }
                }
            }
        }
    }

    result
}

/// The sign of `x`: `1.0`/`-1.0`/`0.0`. Faithful port of Zig's `math.sign`
/// (which is `0` at `0` — unlike `f64::signum`, which is `±1` at `±0`).
fn sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// A direction vector (a segment's slope deconstructed as deltas). Faithful port
/// of z2d's `Slope` (derived from Cairo).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Slope {
    pub dx: f64,
    pub dy: f64,
}

impl Slope {
    /// The slope vector `b - a`.
    pub(crate) fn init(a: Point, b: Point) -> Slope {
        Slope {
            dx: b.x - a.x,
            dy: b.y - a.y,
        }
    }

    /// Exact `dx`/`dy` equality.
    pub(crate) fn equal(self, other: Slope) -> bool {
        self.dx == other.dx && self.dy == other.dy
    }

    /// The slope as `dy / dx`.
    pub(crate) fn calculate(self) -> f64 {
        self.dy / self.dx
    }

    /// Angular comparison (`< 0` when `a < b`, `0` equal, `> 0` when `a > b`),
    /// done multiplicatively. Faithful port of `Slope.compare`.
    pub(crate) fn compare(a: Slope, b: Slope) -> i32 {
        // Snap b to a when within one f64 epsilon (avoids downstream NaNs from
        // near-parallel slopes).
        let bdy = if (b.dy - a.dy).abs() > f64::EPSILON {
            b.dy
        } else {
            a.dy
        };
        let bdx = if (b.dx - a.dx).abs() > f64::EPSILON {
            b.dx
        } else {
            a.dx
        };

        let cmp = sign(a.dy * bdx - bdy * a.dx);
        if cmp != 0.0 {
            return cmp as i32;
        }

        // Tie-breakers.
        if a.dx == 0.0 && a.dy == 0.0 && bdx == 0.0 && bdy == 0.0 {
            return 0;
        }
        if a.dx == 0.0 && a.dy == 0.0 {
            return 1;
        }
        if bdx == 0.0 && bdy == 0.0 {
            return -1;
        }

        // Opposite (pi-apart) directions.
        if sign(a.dx) != sign(bdx) || sign(a.dy) != sign(bdy) {
            return if a.dx > 0.0 || (a.dx == 0.0 && a.dy > 0.0) {
                -1
            } else {
                1
            };
        }

        0
    }

    /// Whether the miter join of `in_slope`→`out_slope` is within `miter_limit`:
    /// `2 <= miter_limit² · (1 + in·out)` (normalized). Faithful port of
    /// `Slope.compare_for_miter_limit`.
    pub(crate) fn compare_for_miter_limit(
        in_slope: Slope,
        out_slope: Slope,
        miter_limit: f64,
    ) -> bool {
        let mut in_n = in_slope;
        in_n.normalize();
        let mut out_n = out_slope;
        out_n.normalize();

        let in_dot_out = in_n.dx * out_n.dx + in_n.dy * out_n.dy;
        2.0 <= miter_limit * miter_limit * (1.0 + in_dot_out)
    }

    /// Set the slope to its unit vector and return the pre-normalization
    /// magnitude. Faithful port of `Slope.normalize` (with the axis fast paths).
    pub(crate) fn normalize(&mut self) -> f64 {
        assert!(self.dx != 0.0 || self.dy != 0.0);

        let (result_dx, result_dy, mag) = if self.dx == 0.0 {
            if self.dy > 0.0 {
                (0.0, 1.0, self.dy)
            } else {
                (0.0, -1.0, -self.dy)
            }
        } else if self.dy == 0.0 {
            if self.dx > 0.0 {
                (1.0, 0.0, self.dx)
            } else {
                (-1.0, 0.0, -self.dx)
            }
        } else {
            let mag = self.dx.hypot(self.dy);
            (self.dx / mag, self.dy / mag, mag)
        };

        self.dx = result_dx;
        self.dy = result_dy;
        mag
    }
}

/// A stroke edge of a line segment `p0 → p1`: the four offset corners
/// (`cw`/`ccw` at each end) a half-width perpendicular to the segment. Faithful
/// port of z2d's `Face` (Cairo-derived), **specialized to the sprite `Canvas`'s
/// translation-only CTM** (whose linear part is identity, so the perpendicular
/// offset is not warped).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Face {
    pub p0: Point,
    pub p1: Point,
    pub width: f64,
    pub dev_slope: Slope,
    pub half_width: f64,
    pub p0_cw: Point,
    pub p0_ccw: Point,
    pub p1_cw: Point,
    pub p1_ccw: Point,
}

impl Face {
    /// A face from `p0 → p1` with the given line `thickness`. Faithful port of
    /// `Face.init`.
    pub(crate) fn init(p0: Point, p1: Point, thickness: f64) -> Face {
        let mut dev_slope = Slope::init(p0, p1);
        dev_slope.normalize();
        Face::init_internal(p0, p1, dev_slope, thickness)
    }

    /// A face at a single `point` with a precomputed normalized `dev_slope`.
    /// Faithful port of `Face.initSingle`.
    pub(crate) fn init_single(point: Point, dev_slope: Slope, thickness: f64) -> Face {
        Face::init_internal(point, point, dev_slope, thickness)
    }

    fn init_internal(p0: Point, p1: Point, dev_slope: Slope, thickness: f64) -> Face {
        let half_width = thickness / 2.0;
        // Linear-identity CTM (the sprite Canvas is translation-only): the
        // perpendicular offset is unwarped.
        let offset_cw_x = -dev_slope.dy * half_width;
        let offset_cw_y = dev_slope.dx * half_width;
        let offset_ccw_x = -offset_cw_x;
        let offset_ccw_y = -offset_cw_y;

        Face {
            p0,
            p1,
            width: thickness,
            dev_slope,
            half_width,
            p0_cw: Point::new(p0.x + offset_cw_x, p0.y + offset_cw_y),
            p0_ccw: Point::new(p0.x + offset_ccw_x, p0.y + offset_ccw_y),
            p1_cw: Point::new(p1.x + offset_cw_x, p1.y + offset_cw_y),
            p1_ccw: Point::new(p1.x + offset_ccw_x, p1.y + offset_ccw_y),
        }
    }

    /// The miter-join intersection of two faces' inner edges. Faithful port of
    /// `Face.intersect`.
    pub(crate) fn intersect(in_face: &Face, out_face: &Face, clockwise: bool) -> Point {
        assert!(Slope::compare(in_face.dev_slope, out_face.dev_slope) != 0);

        let in_point = if clockwise {
            in_face.p1_ccw
        } else {
            in_face.p1_cw
        };
        let out_point = if clockwise {
            out_face.p0_ccw
        } else {
            out_face.p0_cw
        };

        let mut in_slope = in_face.dev_slope;
        in_slope.normalize();
        let mut out_slope = out_face.dev_slope;
        out_slope.normalize();

        let result_y = ((out_point.x - in_point.x) * in_slope.dy * out_slope.dy
            - out_point.y * out_slope.dx * in_slope.dy
            + in_point.y * in_slope.dx * out_slope.dy)
            / (in_slope.dx * out_slope.dy - out_slope.dx * in_slope.dy);

        let result_x = if in_slope.dy.abs() >= out_slope.dy.abs() {
            (result_y - in_point.y) * in_slope.dx / in_slope.dy + in_point.x
        } else {
            (result_y - out_point.y) * out_slope.dx / out_slope.dy + out_point.x
        };

        Point::new(result_x, result_y)
    }

    /// Emit a butt cap at `p1` (the two far corners). Faithful port of
    /// `Face.capButt`.
    pub(crate) fn cap_butt(&self, clockwise: bool, out: &mut Vec<Point>) {
        if clockwise {
            out.push(self.p1_ccw);
            out.push(self.p1_cw);
        } else {
            out.push(self.p1_cw);
            out.push(self.p1_ccw);
        }
    }
}

/// A polyline of corner points (scaled on insertion) that is later assembled
/// into `Polygon` edges. Faithful port of z2d's `Polygon.Contour`. Upstream
/// backs this with a doubly-linked list; this port uses a `Vec<Point>`, which is
/// behaviorally identical for the append/prepend/concat/iterate operations the
/// single-segment butt-cap stroke needs (the mid-list join insert is deferred).
pub(crate) struct Contour {
    pub corners: Vec<Point>,
    pub scale: f64,
}

impl Contour {
    /// An empty contour with the given `scale`.
    pub(crate) fn new(scale: f64) -> Contour {
        Contour {
            corners: Vec::new(),
            scale,
        }
    }

    /// The number of corners.
    pub(crate) fn len(&self) -> usize {
        self.corners.len()
    }

    /// Append `point` (scaled by `scale`). Faithful port of `plot` (append).
    pub(crate) fn plot(&mut self, point: Point) {
        assert!(point.x.is_finite() && point.y.is_finite());
        self.corners
            .push(Point::new(point.x * self.scale, point.y * self.scale));
    }

    /// Prepend `point` (scaled by `scale`). Faithful port of `plotReverse`.
    pub(crate) fn plot_reverse(&mut self, point: Point) {
        assert!(point.x.is_finite() && point.y.is_finite());
        self.corners
            .insert(0, Point::new(point.x * self.scale, point.y * self.scale));
    }

    /// Move `other`'s corners onto the end of `self`, emptying `other`. Faithful
    /// port of `concat`.
    pub(crate) fn concat(&mut self, other: &mut Contour) {
        self.corners.append(&mut other.corners);
    }
}

impl Polygon {
    /// Add edges from a [`Contour`]: an edge between each consecutive pair of
    /// corners, then a closing edge from the last corner back to the first.
    /// Faithful port of z2d's `Polygon.addEdgesFromContour`. (The corner points
    /// are already scaled by the contour; the receiving polygon should use
    /// `scale = 1.0` to avoid double-scaling via `add_edge`.)
    pub(crate) fn add_edges_from_contour(&mut self, contour: &Contour) {
        let mut initial: Option<Point> = None;
        let mut last: Option<Point> = None;
        for &current in &contour.corners {
            if initial.is_none() {
                initial = Some(current);
            }
            if let Some(l) = last {
                self.add_edge(l, current);
            }
            last = Some(current);
        }
        if let (Some(i), Some(l)) = (initial, last) {
            self.add_edge(l, i);
        }
    }
}

/// Stroke a single line segment `p0 → p1` with butt caps into a fill `Polygon`
/// (at `scale`). Faithful port of z2d's `stroke_plotter.plotSingle`: build the
/// segment's `Face`, emit the start cap (the reversed face's butt cap) then the
/// end cap into the outer contour, and assemble the rectangle's edges.
pub(crate) fn stroke_line(p0: Point, p1: Point, thickness: f64, scale: f64) -> Polygon {
    let face = Face::init(p0, p1, thickness);
    let reversed = Face::init(p1, p0, thickness);

    let mut pts = Vec::new();
    // cap_p0: the start cap is the reversed face's butt cap (clockwise).
    reversed.cap_butt(true, &mut pts);
    // cap_p1: the end cap is this face's butt cap (clockwise).
    face.cap_butt(true, &mut pts);

    let mut outer = Contour::new(scale);
    for p in pts {
        outer.plot(p);
    }

    // The contour scales the points, so the result polygon stays at scale 1.
    let mut result = Polygon::new(1.0);
    result.add_edges_from_contour(&outer);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_down() {
        let mut p = Polygon::new(1.0);
        p.add_edge(Point::new(1.0, 1.0), Point::new(3.0, 5.0));
        assert_eq!(p.edges.len(), 1);
        let e = p.edges[0];
        assert_eq!(e.y0, 1.0);
        assert_eq!(e.y1, 5.0);
        assert_eq!(e.x_start, 1.0);
        assert_eq!(e.x_inc, 0.5);
        assert_eq!(e.dir(), -1);
        assert_eq!(e.top(), 1.0);
        assert_eq!(e.bottom(), 5.0);
    }

    #[test]
    fn edge_up() {
        let mut p = Polygon::new(1.0);
        p.add_edge(Point::new(3.0, 5.0), Point::new(1.0, 1.0));
        let e = p.edges[0];
        assert_eq!(e.y0, 5.0);
        assert_eq!(e.y1, 1.0);
        // x_start is the lower-y (top) vertex's x.
        assert_eq!(e.x_start, 1.0);
        assert_eq!(e.x_inc, 0.5);
        assert_eq!(e.dir(), 1);
        assert_eq!(e.top(), 1.0);
        assert_eq!(e.bottom(), 5.0);
    }

    #[test]
    fn edge_horizontal_filtered() {
        let mut p = Polygon::new(1.0);
        p.add_edge(Point::new(1.0, 2.0), Point::new(5.0, 2.0));
        assert!(p.edges.is_empty());
    }

    #[test]
    fn extents_seed_and_grow() {
        let mut p = Polygon::new(1.0);
        p.add_edge(Point::new(2.0, 3.0), Point::new(4.0, 7.0));
        // Seeded by the first edge.
        assert_eq!(p.extent_top, 3.0);
        assert_eq!(p.extent_bottom, 7.0);
        assert_eq!(p.extent_left, 2.0);
        assert_eq!(p.extent_right, 4.0);
        // A second edge grows the extents.
        p.add_edge(Point::new(1.0, 1.0), Point::new(6.0, 9.0));
        assert_eq!(p.extent_top, 1.0);
        assert_eq!(p.extent_bottom, 9.0);
        assert_eq!(p.extent_left, 1.0);
        assert_eq!(p.extent_right, 6.0);
    }

    #[test]
    fn scale_applied() {
        let mut p = Polygon::new(4.0);
        p.add_edge(Point::new(1.0, 1.0), Point::new(3.0, 5.0));
        let e = p.edges[0];
        // Points scale to (4,4)-(12,20).
        assert_eq!(e.y0, 4.0);
        assert_eq!(e.y1, 20.0);
        assert_eq!(e.x_start, 4.0);
        assert_eq!(e.x_inc, 0.5);
        assert_eq!(p.extent_top, 4.0);
        assert_eq!(p.extent_bottom, 20.0);
        assert_eq!(p.extent_left, 4.0);
        assert_eq!(p.extent_right, 12.0);
    }

    #[test]
    fn in_box_inside() {
        let mut p = Polygon::new(1.0);
        p.add_edge(Point::new(2.0, 2.0), Point::new(8.0, 12.0));
        assert!(p.in_box(1.0, 20, 20));
    }

    #[test]
    fn in_box_degenerate() {
        // A purely vertical polygon (zero width) is degenerate.
        let mut p = Polygon::new(1.0);
        p.add_edge(Point::new(5.0, 2.0), Point::new(5.0, 10.0));
        // extent_left == extent_right -> poly_width 0 -> false.
        assert!(!p.in_box(1.0, 20, 20));
    }

    #[test]
    fn in_box_outside() {
        // Entirely to the right of a narrow box.
        let mut p = Polygon::new(1.0);
        p.add_edge(Point::new(30.0, 2.0), Point::new(36.0, 12.0));
        assert!(!p.in_box(1.0, 10, 20));
    }

    /// A closed axis-aligned square contour `(lo,lo)-(hi,hi)` added to `p`.
    fn add_square(p: &mut Polygon, lo: f64, hi: f64) {
        p.add_edge(Point::new(lo, lo), Point::new(hi, lo)); // top (horizontal, filtered)
        p.add_edge(Point::new(hi, lo), Point::new(hi, hi)); // right (down, dir -1)
        p.add_edge(Point::new(hi, hi), Point::new(lo, hi)); // bottom (horizontal, filtered)
        p.add_edge(Point::new(lo, hi), Point::new(lo, lo)); // left (up, dir +1)
    }

    /// The square `(2,2)-(10,10)` → edges right(x=10,dir-1), left(x=2,dir+1).
    fn square() -> Polygon {
        let mut p = Polygon::new(1.0);
        add_square(&mut p, 2.0, 10.0);
        p
    }

    #[test]
    fn breakpoints_sorted_unique() {
        let p = square();
        let w = WorkingEdgeSet::new(&p);
        assert_eq!(w.breakpoints(), vec![2, 10]);
    }

    #[test]
    fn rescan_active() {
        let p = square();
        let mut w = WorkingEdgeSet::new(&p);
        w.rescan(5);
        assert_eq!(w.active, 2);
        // A scanline below the square -> nothing active.
        w.rescan(20);
        assert_eq!(w.active, 0);
    }

    #[test]
    fn inc_x_crossings() {
        let p = square();
        let mut w = WorkingEdgeSet::new(&p);
        w.rescan(5);
        w.inc(5);
        // Vertical edges -> constant x; in edge order (right=10, left=2).
        let mut xs = w.x_values[..w.active].to_vec();
        xs.sort_unstable();
        assert_eq!(xs, vec![2, 10]);
    }

    #[test]
    fn sort_orders_by_x() {
        let p = square();
        let mut w = WorkingEdgeSet::new(&p);
        w.rescan(5);
        w.inc(5);
        w.sort();
        assert_eq!(&w.x_values[..w.active], &[2, 10]);
    }

    #[test]
    fn filter_non_zero_span() {
        let p = square();
        let mut w = WorkingEdgeSet::new(&p);
        w.rescan(5);
        w.inc(5);
        w.sort();
        // One span [2,10) -> the square interior at scanline 5.
        assert_eq!(w.filter(FillRule::NonZero), &[2, 10]);
    }

    #[test]
    fn filter_even_odd_passthru() {
        let p = square();
        let mut w = WorkingEdgeSet::new(&p);
        w.rescan(5);
        w.inc(5);
        w.sort();
        assert_eq!(w.filter(FillRule::EvenOdd), &[2, 10]);
    }

    #[test]
    fn nested_squares_fill_rules() {
        // Two same-wound nested squares. At a scanline crossing all four
        // vertical edges, non-zero fills solid (inner not carved) while even-odd
        // carves the inner (a frame).
        let mut p = Polygon::new(1.0);
        add_square(&mut p, 2.0, 14.0); // outer: left x=2(+1), right x=14(-1)
        add_square(&mut p, 5.0, 11.0); // inner: left x=5(+1), right x=11(-1)

        // even-odd: all four crossings -> two spans [2,5) and [11,14).
        let mut w = WorkingEdgeSet::new(&p);
        w.rescan(8);
        w.inc(8);
        w.sort();
        assert_eq!(w.filter(FillRule::EvenOdd), &[2, 5, 11, 14]);

        // non-zero: one span [2,14) (interior crossings filtered out).
        let mut w2 = WorkingEdgeSet::new(&p);
        w2.rescan(8);
        w2.inc(8);
        w2.sort();
        assert_eq!(w2.filter(FillRule::NonZero), &[2, 14]);
    }

    // SparseCoverageBuffer — the upstream `extend` suite, ported directly.

    #[test]
    fn extend_basic() {
        let mut c = SparseCoverageBuffer::new(10);
        c.put(0, 0, 4);
        c.put(4, 0, 4);
        c.len = 8;
        c.extend(2, 5);
        assert_eq!(c.len, 8);
        assert_eq!(c.get(0), (0, 2));
        assert_eq!(c.get(2), (0, 2));
        assert_eq!(c.get(4), (0, 3));
        assert_eq!(c.get(7), (0, 1));
    }

    #[test]
    fn extend_new_zero() {
        let mut c = SparseCoverageBuffer::new(10);
        c.extend(0, 5);
        assert_eq!(c.len, 5);
        assert_eq!(c.get(0), (0, 5));
    }

    #[test]
    fn extend_new_nonzero() {
        let mut c = SparseCoverageBuffer::new(10);
        c.extend(2, 5);
        assert_eq!(c.len, 7);
        assert_eq!(c.get(0), (0, 2));
        assert_eq!(c.get(2), (0, 5));
    }

    #[test]
    fn extend_split_end_no_extend() {
        let mut c = SparseCoverageBuffer::new(10);
        c.put(0, 0, 4);
        c.put(4, 0, 4);
        c.len = 8;
        c.extend(7, 1);
        assert_eq!(c.len, 8);
        assert_eq!(c.get(0), (0, 4));
        assert_eq!(c.get(4), (0, 3));
        assert_eq!(c.get(7), (0, 1));
    }

    #[test]
    fn extend_split_end_with_extend() {
        let mut c = SparseCoverageBuffer::new(10);
        c.put(0, 0, 4);
        c.put(4, 0, 4);
        c.len = 8;
        c.extend(7, 3);
        assert_eq!(c.len, 10);
        assert_eq!(c.get(0), (0, 4));
        assert_eq!(c.get(4), (0, 3));
        assert_eq!(c.get(7), (0, 1));
        assert_eq!(c.get(8), (0, 2));
    }

    #[test]
    fn extend_append_after_end() {
        let mut c = SparseCoverageBuffer::new(10);
        c.put(0, 0, 4);
        c.put(4, 0, 4);
        c.len = 8;
        c.extend(8, 2);
        assert_eq!(c.len, 10);
        assert_eq!(c.get(0), (0, 4));
        assert_eq!(c.get(4), (0, 4));
        assert_eq!(c.get(8), (0, 2));
    }

    #[test]
    fn extend_past_end() {
        let mut c = SparseCoverageBuffer::new(11);
        c.put(0, 0, 4);
        c.put(4, 0, 4);
        c.len = 8;
        c.extend(9, 2);
        assert_eq!(c.len, 11);
        assert_eq!(c.get(0), (0, 4));
        assert_eq!(c.get(4), (0, 4));
        assert_eq!(c.get(8), (0, 1));
        assert_eq!(c.get(9), (0, 2));
    }

    #[test]
    fn extend_zero_len() {
        let mut c = SparseCoverageBuffer::new(10);
        c.extend(0, 0);
        let (_, got_len) = c.get(0);
        assert_eq!(got_len, 0);
    }

    #[test]
    fn extend_split_to_capacity() {
        let mut c = SparseCoverageBuffer::new(255);
        c.extend(192, 63);
        assert_eq!(c.get(0), (0, 192));
        assert_eq!(c.get(192), (0, 63));
        // Walking the runs yields exactly 2 spans.
        let mut idx = 0u32;
        let mut spans = 0usize;
        while idx < c.len {
            let (_, inc) = c.get(idx);
            idx += inc;
            spans += 1;
        }
        assert_eq!(spans, 2);
    }

    #[test]
    fn add_span_accumulates() {
        let mut c = SparseCoverageBuffer::new(10);
        c.add_span(0, 1, 5);
        c.add_span(2, 1, 5);
        // Coverage 1 on [0,2), 2 on [2,5), 1 on [5,7).
        assert_eq!(c.get(0), (1, 2));
        assert_eq!(c.get(2), (2, 3));
        assert_eq!(c.get(5), (1, 2));
        assert_eq!(c.len, 7);
    }

    #[test]
    fn add_single_accumulates() {
        let mut c = SparseCoverageBuffer::new(10);
        c.add_span(0, 1, 4);
        c.add_single(2, 3);
        // [0,2) -> 1, [2,3) -> 4, [3,4) -> 1.
        assert_eq!(c.get(0), (1, 2));
        assert_eq!(c.get(2), (4, 1));
        assert_eq!(c.get(3), (1, 1));
    }

    // add_supersampled_span — the MSAA coverage distributor.

    #[test]
    fn supersample_span_triangle() {
        // The full upstream `addSpan` test: a triangle cross-section built from
        // four accumulating spans.
        let mut c = SparseCoverageBuffer::new(1024);
        add_supersampled_span(&mut c, 200, 400);
        assert_eq!(c.get(50), (4, 100));

        add_supersampled_span(&mut c, 201, 398);
        assert_eq!(c.get(50), (7, 1));
        assert_eq!(c.get(51), (8, 98));
        assert_eq!(c.get(149), (7, 1));

        add_supersampled_span(&mut c, 202, 396);
        assert_eq!(c.get(50), (9, 1));
        assert_eq!(c.get(51), (12, 98));
        assert_eq!(c.get(149), (9, 1));

        add_supersampled_span(&mut c, 203, 394);
        assert_eq!(c.get(50), (10, 1));
        assert_eq!(c.get(51), (16, 98));
        assert_eq!(c.get(149), (10, 1));

        // Walking the runs yields exactly 4 spans.
        let mut x = 0u32;
        let mut spans = 0usize;
        while x < c.len {
            let (_, inc) = c.get(x);
            x += inc;
            spans += 1;
        }
        assert_eq!(spans, 4);
    }

    #[test]
    fn supersample_span_partial_start() {
        // x=2..6: second half of pixel 0, first half of pixel 1 (each 2/4).
        let mut c = SparseCoverageBuffer::new(10);
        add_supersampled_span(&mut c, 2, 4);
        assert_eq!(c.get(0), (2, 1));
        assert_eq!(c.get(1), (2, 1));
    }

    #[test]
    fn supersample_span_full_plus_partial() {
        // x=0..6: pixel 0 full (4), pixel 1 half (2).
        let mut c = SparseCoverageBuffer::new(10);
        add_supersampled_span(&mut c, 0, 6);
        assert_eq!(c.get(0), (4, 1));
        assert_eq!(c.get(1), (2, 1));
    }

    #[test]
    fn supersample_span_zero() {
        let mut c = SparseCoverageBuffer::new(10);
        add_supersampled_span(&mut c, 0, 0);
        assert_eq!(c.len, 0);
    }

    // fill_polygon — the multisample rasterizer run.

    /// A closed-contour polygon (device coords) at the MSAA scale.
    fn closed_polygon(pts: &[(f64, f64)]) -> Polygon {
        let mut p = Polygon::new(MSAA_SCALE as f64);
        for i in 0..pts.len() {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            p.add_edge(Point::new(a.0, a.1), Point::new(b.0, b.1));
        }
        p
    }

    #[test]
    fn src_over_math() {
        assert_eq!(src_over_alpha8(0, 127), 127);
        assert_eq!(src_over_alpha8(255, 100), 255);
        // 127 + 127 - trunc(127*127/255) = 254 - 63 = 191.
        assert_eq!(src_over_alpha8(127, 127), 191);
    }

    #[test]
    fn fill_square_crisp() {
        // A 4x4 axis-aligned square at device (1,1)-(5,5) into a 6x6 surface.
        let p = closed_polygon(&[(1.0, 1.0), (5.0, 1.0), (5.0, 5.0), (1.0, 5.0)]);
        let mut buf = vec![0u8; 36];
        fill_polygon(&mut buf, 6, 6, &p, FillRule::NonZero);
        for y in 0..6 {
            for x in 0..6 {
                let want = if (1..5).contains(&x) && (1..5).contains(&y) {
                    255
                } else {
                    0
                };
                assert_eq!(buf[y * 6 + x], want, "pixel ({x},{y})");
            }
        }
    }

    #[test]
    fn fill_partial_row_aa() {
        // A rectangle (1,1)-(3,2.5): the bottom pixel row is half-covered ->
        // 2/4 sub-scanlines -> coverage 8 -> alpha 8*16-1 = 127.
        let p = closed_polygon(&[(1.0, 1.0), (3.0, 1.0), (3.0, 2.5), (1.0, 2.5)]);
        let mut buf = vec![0u8; 36];
        fill_polygon(&mut buf, 6, 6, &p, FillRule::NonZero);
        for y in 0..6 {
            for x in 0..6 {
                let want = match (x, y) {
                    (1, 1) | (2, 1) => 255,
                    (1, 2) | (2, 2) => 127,
                    _ => 0,
                };
                assert_eq!(buf[y * 6 + x], want, "pixel ({x},{y})");
            }
        }
    }

    #[test]
    fn fill_outside_noop() {
        // A square entirely outside the 6x6 surface draws nothing.
        let p = closed_polygon(&[(10.0, 10.0), (14.0, 10.0), (14.0, 14.0), (10.0, 14.0)]);
        let mut buf = vec![0u8; 36];
        fill_polygon(&mut buf, 6, 6, &p, FillRule::NonZero);
        assert!(buf.iter().all(|&v| v == 0));
    }

    // PathNode — the upstream isClosedNodeSet suite, ported directly.

    fn mv(x: f64, y: f64) -> PathNode {
        PathNode::MoveTo(Point::new(x, y))
    }
    fn ln(x: f64, y: f64) -> PathNode {
        PathNode::LineTo(Point::new(x, y))
    }

    #[test]
    fn closed_node_set_basic_closed() {
        let nodes = [
            mv(1.0, 1.0),
            ln(2.0, 2.0),
            ln(3.0, 3.0),
            PathNode::ClosePath,
            mv(1.0, 1.0),
        ];
        assert!(is_closed_node_set(&nodes));
    }

    #[test]
    fn closed_node_set_multiple_closed() {
        let nodes = [
            mv(1.0, 1.0),
            ln(2.0, 2.0),
            ln(3.0, 3.0),
            PathNode::ClosePath,
            mv(1.0, 1.0),
            mv(4.0, 4.0),
            ln(5.0, 5.0),
            ln(6.0, 6.0),
            PathNode::ClosePath,
            mv(4.0, 4.0),
        ];
        assert!(is_closed_node_set(&nodes));
    }

    #[test]
    fn closed_node_set_basic_not_closed() {
        let nodes = [
            mv(1.0, 1.0),
            ln(2.0, 2.0),
            mv(3.0, 3.0),
            ln(4.0, 4.0),
            ln(5.0, 5.0),
        ];
        assert!(!is_closed_node_set(&nodes));
    }

    #[test]
    fn closed_node_set_closed_in_middle() {
        let nodes = [
            mv(1.0, 1.0),
            ln(2.0, 2.0),
            PathNode::ClosePath,
            mv(1.0, 1.0),
            mv(3.0, 3.0),
            ln(4.0, 4.0),
            ln(5.0, 5.0),
        ];
        assert!(!is_closed_node_set(&nodes));
    }

    #[test]
    fn closed_node_set_closed_at_end_not_middle() {
        let nodes = [
            mv(1.0, 1.0),
            ln(2.0, 2.0),
            mv(3.0, 3.0),
            ln(4.0, 4.0),
            ln(5.0, 5.0),
            PathNode::ClosePath,
            mv(3.0, 3.0),
        ];
        assert!(!is_closed_node_set(&nodes));
    }

    #[test]
    fn closed_node_set_empty() {
        assert!(!is_closed_node_set(&[]));
    }

    // Spline — the cubic-Bézier flattener.

    fn pt(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn lerp_half_midpoint() {
        assert_eq!(lerp_half(pt(0.0, 0.0), pt(4.0, 6.0)), pt(2.0, 3.0));
    }

    #[test]
    fn dot_sq_value() {
        assert_eq!(dot_sq(3.0, 4.0), 25.0);
    }

    #[test]
    fn error_sq_offset() {
        // Control points 3 off the a->d chord -> squared residual 9.
        let k = Knots {
            a: pt(0.0, 0.0),
            b: pt(0.0, 3.0),
            c: pt(4.0, 3.0),
            d: pt(4.0, 0.0),
        };
        assert_eq!(k.error_sq(), 9.0);
    }

    #[test]
    fn de_casteljau_exact() {
        let mut k = Knots {
            a: pt(0.0, 0.0),
            b: pt(0.0, 12.0),
            c: pt(12.0, 12.0),
            d: pt(12.0, 0.0),
        };
        let second = k.de_casteljau();
        assert_eq!(
            k,
            Knots {
                a: pt(0.0, 0.0),
                b: pt(0.0, 6.0),
                c: pt(3.0, 9.0),
                d: pt(6.0, 9.0),
            }
        );
        assert_eq!(
            second,
            Knots {
                a: pt(6.0, 9.0),
                b: pt(9.0, 9.0),
                c: pt(12.0, 6.0),
                d: pt(12.0, 0.0),
            }
        );
    }

    #[test]
    fn decompose_straight() {
        // Both tangents zero -> a straight line -> just the endpoint.
        let s = Spline {
            a: pt(0.0, 0.0),
            b: pt(0.0, 0.0),
            c: pt(10.0, 10.0),
            d: pt(10.0, 10.0),
            tolerance: 0.1,
        };
        let mut out = Vec::new();
        s.decompose(&mut out);
        assert_eq!(out, vec![pt(10.0, 10.0)]);
    }

    #[test]
    fn decompose_collinear() {
        // All control points on the line y=x -> zero error -> just the endpoint.
        let s = Spline {
            a: pt(0.0, 0.0),
            b: pt(1.0, 1.0),
            c: pt(2.0, 2.0),
            d: pt(3.0, 3.0),
            tolerance: 0.1,
        };
        let mut out = Vec::new();
        s.decompose(&mut out);
        assert_eq!(out, vec![pt(3.0, 3.0)]);
    }

    #[test]
    fn decompose_curved() {
        // A real arch: flattens to several points, ending at d, rising in y.
        let s = Spline {
            a: pt(0.0, 0.0),
            b: pt(0.0, 10.0),
            c: pt(10.0, 10.0),
            d: pt(10.0, 0.0),
            tolerance: 0.1,
        };
        let mut out = Vec::new();
        s.decompose(&mut out);
        assert!(out.len() > 2, "the arch flattens to several segments");
        assert_eq!(*out.last().unwrap(), pt(10.0, 0.0));
        // Every point is within the control bounding box, and the arch rises.
        let mut max_y = f64::MIN;
        for p in &out {
            assert!((0.0..=10.0).contains(&p.x), "x in box");
            assert!((0.0..=10.0).contains(&p.y), "y in box");
            max_y = max_y.max(p.y);
        }
        assert!(max_y > 0.0, "the curve rises above the chord");
    }

    // PointBuffer + fill_plot.

    #[test]
    fn point_buffer_split_one() {
        let mut b: PointBuffer<1, 3> = PointBuffer::new();
        b.add(pt(0.0, 0.0));
        b.add(pt(1.0, 1.0));
        b.add(pt(2.0, 2.0));
        b.add(pt(3.0, 3.0)); // full -> keep items[0], FIFO the tail
        assert_eq!(b.len, 3);
        assert_eq!(b.first(), Some(pt(0.0, 0.0)));
        assert_eq!(b.last(), Some(pt(3.0, 3.0)));
        assert_eq!(b.head(1), Some(pt(2.0, 2.0)));
        assert_eq!(b.tail(1), Some(pt(3.0, 3.0)));
        b.reset();
        assert_eq!(b.len, 0);
        assert_eq!(b.first(), None);
        assert_eq!(b.last(), None);
    }

    fn edge(y0: f64, y1: f64, x_start: f64, x_inc: f64) -> Edge {
        Edge {
            y0,
            y1,
            x_start,
            x_inc,
        }
    }

    #[test]
    fn fill_degenerate_line_to() {
        let nodes = [
            mv(5.0, 0.0),
            ln(10.0, 10.0),
            ln(10.0, 10.0),
            ln(0.0, 10.0),
            PathNode::ClosePath,
            mv(5.0, 0.0),
        ];
        let result = fill_plot(&nodes, 1.0, 1.0);
        assert_eq!(
            result.edges,
            vec![edge(0.0, 10.0, 5.0, 0.5), edge(10.0, 0.0, 5.0, -0.5)]
        );
    }

    #[test]
    fn fill_degenerate_close_move_line() {
        let nodes = [
            mv(5.0, 0.0),
            ln(10.0, 10.0),
            PathNode::ClosePath,
            mv(5.0, 0.0),
        ];
        let result = fill_plot(&nodes, 1.0, 1.0);
        assert_eq!(result.edges, vec![edge(0.0, 10.0, 5.0, 0.5)]);
    }

    #[test]
    fn fill_degenerate_double_close() {
        let nodes = [
            mv(5.0, 0.0),
            ln(10.0, 10.0),
            ln(0.0, 10.0),
            PathNode::ClosePath,
            mv(5.0, 0.0),
            PathNode::ClosePath,
            mv(5.0, 0.0),
        ];
        let result = fill_plot(&nodes, 1.0, 1.0);
        assert_eq!(
            result.edges,
            vec![edge(0.0, 10.0, 5.0, 0.5), edge(10.0, 0.0, 5.0, -0.5)]
        );
    }

    #[test]
    fn fill_curve() {
        // A move + curve + close: the cubic flattens to several edges, all
        // within the control bounding box.
        let nodes = [
            mv(0.0, 0.0),
            PathNode::CurveTo {
                p1: pt(0.0, 10.0),
                p2: pt(10.0, 10.0),
                p3: pt(10.0, 0.0),
            },
            PathNode::ClosePath,
            mv(0.0, 0.0),
        ];
        let result = fill_plot(&nodes, 1.0, 0.1);
        assert!(result.edges.len() > 2, "the cubic flattens to many edges");
        assert!(result.extent_left >= 0.0 && result.extent_right <= 10.0);
        assert!(result.extent_top >= 0.0 && result.extent_bottom <= 10.0);
    }

    // Slope.

    #[test]
    fn slope_init() {
        let s = Slope::init(pt(1.0, 2.0), pt(4.0, 6.0));
        assert_eq!(s, Slope { dx: 3.0, dy: 4.0 });
    }

    #[test]
    fn slope_calculate() {
        assert_eq!(Slope { dx: 2.0, dy: 6.0 }.calculate(), 3.0);
        assert!(Slope { dx: 3.0, dy: 4.0 }.equal(Slope { dx: 3.0, dy: 4.0 }));
    }

    #[test]
    fn slope_normalize() {
        let mut s = Slope { dx: 3.0, dy: 4.0 };
        assert_eq!(s.normalize(), 5.0);
        assert_eq!(s, Slope { dx: 0.6, dy: 0.8 });

        let mut v = Slope { dx: 0.0, dy: 5.0 };
        assert_eq!(v.normalize(), 5.0);
        assert_eq!(v, Slope { dx: 0.0, dy: 1.0 });

        let mut h = Slope { dx: -4.0, dy: 0.0 };
        assert_eq!(h.normalize(), 4.0);
        assert_eq!(h, Slope { dx: -1.0, dy: 0.0 });
    }

    #[test]
    fn slope_compare() {
        // +x is angularly before +y.
        assert_eq!(
            Slope::compare(
                Slope::init(pt(0.0, 0.0), pt(1.0, 0.0)),
                Slope::init(pt(0.0, 0.0), pt(0.0, 1.0))
            ),
            -1
        );
        // Parallel, same direction -> 0.
        assert_eq!(
            Slope::compare(Slope { dx: 1.0, dy: 1.0 }, Slope { dx: 2.0, dy: 2.0 }),
            0
        );
        // Opposite (pi-apart) directions.
        assert_eq!(
            Slope::compare(Slope { dx: 1.0, dy: 0.0 }, Slope { dx: -1.0, dy: 0.0 }),
            -1
        );
    }

    #[test]
    fn slope_miter_limit() {
        // A right-angle turn: in.out = 0, so 2 <= ml^2.
        let in_s = Slope { dx: 1.0, dy: 0.0 };
        let out_s = Slope { dx: 0.0, dy: 1.0 };
        assert!(Slope::compare_for_miter_limit(in_s, out_s, 2.0));
        assert!(!Slope::compare_for_miter_limit(in_s, out_s, 1.0));
    }

    // Face.

    #[test]
    fn face_horizontal() {
        let f = Face::init(pt(0.0, 0.0), pt(10.0, 0.0), 2.0);
        assert_eq!(f.dev_slope, Slope { dx: 1.0, dy: 0.0 });
        assert_eq!(f.half_width, 1.0);
        assert_eq!(f.p0_cw, pt(0.0, 1.0));
        assert_eq!(f.p0_ccw, pt(0.0, -1.0));
        assert_eq!(f.p1_cw, pt(10.0, 1.0));
        assert_eq!(f.p1_ccw, pt(10.0, -1.0));
    }

    #[test]
    fn face_vertical() {
        let f = Face::init(pt(0.0, 0.0), pt(0.0, 10.0), 2.0);
        assert_eq!(f.dev_slope, Slope { dx: 0.0, dy: 1.0 });
        assert_eq!(f.p0_cw, pt(-1.0, 0.0));
        assert_eq!(f.p0_ccw, pt(1.0, 0.0));
        assert_eq!(f.p1_cw, pt(-1.0, 10.0));
        assert_eq!(f.p1_ccw, pt(1.0, 10.0));
    }

    #[test]
    fn cap_butt_emits() {
        let f = Face::init(pt(0.0, 0.0), pt(10.0, 0.0), 2.0);
        let mut out = Vec::new();
        f.cap_butt(false, &mut out);
        assert_eq!(out, vec![pt(10.0, 1.0), pt(10.0, -1.0)]);
        let mut out_cw = Vec::new();
        f.cap_butt(true, &mut out_cw);
        assert_eq!(out_cw, vec![pt(10.0, -1.0), pt(10.0, 1.0)]);
    }

    #[test]
    fn intersect_corner() {
        let in_face = Face::init(pt(0.0, 0.0), pt(10.0, 0.0), 2.0);
        let out_face = Face::init(pt(10.0, 0.0), pt(10.0, 10.0), 2.0);
        let p = Face::intersect(&in_face, &out_face, false);
        assert_eq!(p, pt(9.0, 1.0));
    }

    // Contour.

    #[test]
    fn contour_plot_scales() {
        let mut c = Contour::new(4.0);
        c.plot(pt(1.0, 2.0));
        assert_eq!(c.corners, vec![pt(4.0, 8.0)]);
        c.plot_reverse(pt(0.0, 0.0));
        assert_eq!(c.corners, vec![pt(0.0, 0.0), pt(4.0, 8.0)]);
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn contour_concat() {
        let mut a = Contour::new(1.0);
        a.plot(pt(1.0, 1.0));
        let mut b = Contour::new(1.0);
        b.plot(pt(2.0, 2.0));
        b.plot(pt(3.0, 3.0));
        a.concat(&mut b);
        assert_eq!(a.corners, vec![pt(1.0, 1.0), pt(2.0, 2.0), pt(3.0, 3.0)]);
        assert_eq!(b.len(), 0);
    }

    #[test]
    fn add_edges_from_contour_square() {
        let mut c = Contour::new(4.0);
        c.plot(pt(0.0, 0.0));
        c.plot(pt(4.0, 0.0));
        c.plot(pt(4.0, 4.0));
        c.plot(pt(0.0, 4.0));
        let mut poly = Polygon::new(1.0);
        poly.add_edges_from_contour(&c);
        assert_eq!(
            poly.edges,
            vec![edge(0.0, 16.0, 16.0, 0.0), edge(16.0, 0.0, 0.0, 0.0)]
        );
    }

    // stroke_line — the single-segment butt-cap stroke.

    #[test]
    fn stroke_horizontal() {
        let poly = stroke_line(pt(0.0, 0.0), pt(10.0, 0.0), 2.0, 1.0);
        assert_eq!(
            poly.edges,
            vec![edge(1.0, -1.0, 0.0, 0.0), edge(-1.0, 1.0, 10.0, 0.0)]
        );
    }

    #[test]
    fn stroke_vertical() {
        // A vertical segment strokes to a horizontal bar: two horizontal-running
        // edges of the rotated rectangle.
        let poly = stroke_line(pt(0.0, 0.0), pt(0.0, 10.0), 2.0, 1.0);
        // dev_slope (0,1) -> offset_cw (-1,0); corners are at x = ±1, y in [0,10].
        // The two non-horizontal edges run along the left/right of the bar.
        assert_eq!(poly.edges.len(), 2);
        assert_eq!(poly.extent_left, -1.0);
        assert_eq!(poly.extent_right, 1.0);
        assert_eq!(poly.extent_top, 0.0);
        assert_eq!(poly.extent_bottom, 10.0);
    }

    #[test]
    fn stroke_diagonal() {
        // A 45-degree segment strokes to a rotated rectangle: all four edges are
        // non-axis-aligned.
        let poly = stroke_line(pt(0.0, 0.0), pt(4.0, 4.0), 2.0, 1.0);
        assert_eq!(poly.edges.len(), 4);
        // The rectangle encloses the segment endpoints.
        assert!(poly.extent_left < 0.0);
        assert!(poly.extent_right > 4.0);
        assert!(poly.extent_top < 0.0);
        assert!(poly.extent_bottom > 4.0);
    }

    #[test]
    fn stroke_scaled() {
        // Same as the horizontal case but at scale 4: all coordinates ×4.
        let poly = stroke_line(pt(0.0, 0.0), pt(10.0, 0.0), 2.0, 4.0);
        assert_eq!(
            poly.edges,
            vec![edge(4.0, -4.0, 0.0, 0.0), edge(-4.0, 4.0, 40.0, 0.0)]
        );
    }
}
