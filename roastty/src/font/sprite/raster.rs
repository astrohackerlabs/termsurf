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
}
