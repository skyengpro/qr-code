//! Geometric primitives the analytic shapes are unions of.
//!
//! All coordinates are in the 1000 × 1000 design box, y down. Angles are in degrees,
//! measured from +x towards +y (so 90° points down and arcs sweep clockwise on screen).

/// One filled primitive.
#[derive(Debug, Clone, Copy)]
pub(super) enum Prim {
    /// Axis-aligned rectangle, boundary included.
    Rect { x0: f64, y0: f64, x1: f64, y1: f64 },
    /// Filled polygon (even-odd rule; the letters only use simple polygons).
    Poly(&'static [(f64, f64)]),
    /// Arc stroke of width `w` around radius `r`, flat (radial) ends, starting at `a0` and
    /// sweeping clockwise by `sweep` degrees (360 or more is a full ring). `sx` stretches
    /// the arc horizontally into an ellipse (1 = circle); the stroke is then `sx` times
    /// wider at the sides than at the top and bottom, like a typeface with stress.
    Arc {
        cx: f64,
        cy: f64,
        r: f64,
        w: f64,
        sx: f64,
        a0: f64,
        sweep: f64,
    },
    /// Straight stroke of width `w` between two points with flat ends.
    Bar {
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        w: f64,
    },
}

/// Axis-aligned rectangle.
pub(super) const fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Prim {
    Prim::Rect { x0, y0, x1, y1 }
}

/// Circular arc stroke.
pub(super) const fn arc(cx: f64, cy: f64, r: f64, w: f64, a0: f64, sweep: f64) -> Prim {
    Prim::Arc {
        cx,
        cy,
        r,
        w,
        sx: 1.0,
        a0,
        sweep,
    }
}

/// Arc stroke on an ellipse stretched horizontally by `sx`.
pub(super) const fn earc(cx: f64, cy: f64, r: f64, w: f64, sx: f64, a0: f64, sweep: f64) -> Prim {
    Prim::Arc {
        cx,
        cy,
        r,
        w,
        sx,
        a0,
        sweep,
    }
}

/// Straight stroke with flat ends.
pub(super) const fn bar(x0: f64, y0: f64, x1: f64, y1: f64, w: f64) -> Prim {
    Prim::Bar { x0, y0, x1, y1, w }
}

impl Prim {
    /// Whether `(x, y)` is inside the primitive.
    pub(super) fn contains(&self, x: f64, y: f64) -> bool {
        match *self {
            Prim::Rect { x0, y0, x1, y1 } => x >= x0 && x <= x1 && y >= y0 && y <= y1,
            Prim::Poly(points) => poly_contains(points, x, y),
            Prim::Arc {
                cx,
                cy,
                r,
                w,
                sx,
                a0,
                sweep,
            } => arc_contains(
                &ArcSpec {
                    cx,
                    cy,
                    r,
                    w,
                    sx,
                    a0,
                    sweep,
                },
                x,
                y,
            ),
            Prim::Bar { x0, y0, x1, y1, w } => bar_contains(x0, y0, x1, y1, w, x, y),
        }
    }
}

/// Whether any primitive contains the point.
pub(super) fn any_contains(prims: &[Prim], x: f64, y: f64) -> bool {
    prims.iter().any(|p| p.contains(x, y))
}

fn poly_contains(points: &[(f64, f64)], x: f64, y: f64) -> bool {
    let mut inside = false;
    let mut j = points.len().wrapping_sub(1);
    for (i, &(xi, yi)) in points.iter().enumerate() {
        let (xj, yj) = points[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

struct ArcSpec {
    cx: f64,
    cy: f64,
    r: f64,
    w: f64,
    sx: f64,
    a0: f64,
    sweep: f64,
}

fn arc_contains(a: &ArcSpec, x: f64, y: f64) -> bool {
    let (dx, dy) = ((x - a.cx) / a.sx, y - a.cy);
    if (libm::hypot(dx, dy) - a.r).abs() > 0.5 * a.w {
        return false;
    }
    if a.sweep >= 360.0 {
        return true;
    }
    let ang = libm::atan2(dy, dx) * (180.0 / core::f64::consts::PI);
    let rel = ang - a.a0;
    let rel = rel - 360.0 * libm::floor(rel / 360.0);
    rel <= a.sweep
}

fn bar_contains(x0: f64, y0: f64, x1: f64, y1: f64, w: f64, x: f64, y: f64) -> bool {
    let (ux, uy) = (x1 - x0, y1 - y0);
    let len = libm::hypot(ux, uy);
    if len == 0.0 {
        return false;
    }
    let (ux, uy) = (ux / len, uy / len);
    let (px, py) = (x - x0, y - y0);
    let along = px * ux + py * uy;
    let across = -px * uy + py * ux;
    (0.0..=len).contains(&along) && across.abs() <= 0.5 * w
}
