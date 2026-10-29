//! SVG path data to a [`tiny_skia::Path`] in the 1000 × 1000 design box.

use svgtypes::{SimplePathSegment, SimplifyingPathParser};
use tiny_skia::{Path, PathBuilder};

use crate::error::Error;

/// Side of the design box, in design units.
pub const DESIGN_SIZE: u32 = 1000;

/// The SVG `viewBox` the path coordinates are expressed in. Coordinates are mapped so the
/// box covers the 1000 × 1000 design box exactly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewBox {
    /// Left edge in path units.
    pub min_x: f64,
    /// Top edge in path units.
    pub min_y: f64,
    /// Width in path units (> 0).
    pub width: f64,
    /// Height in path units (> 0).
    pub height: f64,
}

impl ViewBox {
    /// The identity box `0 0 1000 1000`.
    pub const DESIGN: ViewBox = ViewBox {
        min_x: 0.0,
        min_y: 0.0,
        width: 1000.0,
        height: 1000.0,
    };

    /// Parse the value of a `viewBox` attribute (`"minx miny w h"`, commas allowed). The box
    /// must be square: the frame's bounding square maps onto it.
    pub fn parse(text: &str) -> Result<ViewBox, Error> {
        let bad = || Error::BadSvg(format!("viewBox {text:?} is not four numbers"));
        let nums: Vec<f64> = text
            .split(|c: char| c.is_ascii_whitespace() || c == ',')
            .filter(|t| !t.is_empty())
            .map(str::parse::<f64>)
            .collect::<Result<_, _>>()
            .map_err(|_| bad())?;
        let [min_x, min_y, width, height] = nums[..] else {
            return Err(bad());
        };
        let finite = [min_x, min_y, width, height].iter().all(|v| v.is_finite());
        if !finite || width <= 0.0 || height <= 0.0 {
            return Err(Error::BadSvg(format!("viewBox {text:?} has no area")));
        }
        if (width - height).abs() > 1e-6 * width.max(height) {
            return Err(Error::BadSvg(format!(
                "viewBox {text:?} is not square; the frame's bounding square maps onto it"
            )));
        }
        Ok(ViewBox {
            min_x,
            min_y,
            width,
            height,
        })
    }

    fn map(&self, x: f64, y: f64) -> (f32, f32) {
        let s = f64::from(DESIGN_SIZE);
        (
            ((x - self.min_x) * s / self.width) as f32,
            ((y - self.min_y) * s / self.height) as f32,
        )
    }
}

/// Parse SVG path data (`d` attribute) into a path in design-box units.
///
/// Every command of the SVG grammar is accepted: relative and shorthand forms, `H`/`V`,
/// and elliptical arcs (which `svgtypes` converts to cubic curves). Open sub-paths are
/// closed implicitly when filled.
pub fn parse_path(d: &str, view_box: &ViewBox) -> Result<Path, Error> {
    let mut pb = PathBuilder::new();
    let mut segments = 0usize;
    for seg in SimplifyingPathParser::from(d) {
        let seg = seg.map_err(|e| Error::BadPath(e.to_string()))?;
        segments += 1;
        match seg {
            SimplePathSegment::MoveTo { x, y } => {
                let (x, y) = view_box.map(x, y);
                pb.move_to(x, y);
            }
            SimplePathSegment::LineTo { x, y } => {
                let (x, y) = view_box.map(x, y);
                pb.line_to(x, y);
            }
            SimplePathSegment::Quadratic { x1, y1, x, y } => {
                let (x1, y1) = view_box.map(x1, y1);
                let (x, y) = view_box.map(x, y);
                pb.quad_to(x1, y1, x, y);
            }
            SimplePathSegment::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let (x1, y1) = view_box.map(x1, y1);
                let (x2, y2) = view_box.map(x2, y2);
                let (x, y) = view_box.map(x, y);
                pb.cubic_to(x1, y1, x2, y2, x, y);
            }
            SimplePathSegment::ClosePath => pb.close(),
        }
    }
    if segments == 0 {
        return Err(Error::BadPath("no path commands".into()));
    }
    pb.finish()
        .ok_or_else(|| Error::BadPath("the path has no extent".into()))
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // reason: tests compare exact, hand-written coordinates
mod tests {
    use super::*;
    use tiny_skia::PathSegment;

    fn parse(d: &str) -> Path {
        parse_path(d, &ViewBox::DESIGN).expect("valid path")
    }

    fn segments(p: &Path) -> Vec<PathSegment> {
        p.segments().collect()
    }

    #[test]
    fn absolute_commands() {
        let p = parse("M 100 100 L 900 100 Q 900 900 500 900 C 100 900 100 500 100 100 Z");
        let segs = segments(&p);
        assert!(matches!(segs[0], PathSegment::MoveTo(pt) if pt.x == 100.0 && pt.y == 100.0));
        assert!(matches!(segs[1], PathSegment::LineTo(pt) if pt.x == 900.0));
        assert!(matches!(segs[2], PathSegment::QuadTo(c, e) if c.y == 900.0 && e.x == 500.0));
        assert!(matches!(segs[3], PathSegment::CubicTo(..)));
        assert!(matches!(segs.last(), Some(PathSegment::Close)));
        let b = p.bounds();
        assert!((b.left() - 100.0).abs() < 1e-3 && (b.right() - 900.0).abs() < 1e-3);
    }

    #[test]
    fn relative_and_shorthand_commands() {
        // m/h/v relative: the result must be the square 200..800.
        let p = parse("m 200 200 h 600 v 600 h -600 z");
        let b = p.bounds();
        assert_eq!(
            (b.left(), b.top(), b.right(), b.bottom()),
            (200.0, 200.0, 800.0, 800.0)
        );
        // Relative curves and smooth shorthands parse without error.
        let q = parse("m 100 500 c 0 -200 200 -200 200 0 s 200 200 200 0 q 100 -100 200 0 t 100 0");
        assert!(q.bounds().right() > 600.0, "{:?}", q.bounds());
    }

    #[test]
    fn arcs_become_curves() {
        let p = parse("M 100 500 A 400 400 0 0 1 900 500 A 400 400 0 0 1 100 500 Z");
        let b = p.bounds();
        // Bounds include the curves' control points, so they overshoot the half circles.
        assert!(b.top() <= 100.0 && b.top() > 50.0, "{b:?}");
        assert!(b.bottom() >= 900.0 && b.bottom() < 950.0, "{b:?}");
        assert!(
            segments(&p)
                .iter()
                .any(|s| matches!(s, PathSegment::CubicTo(..)))
        );
    }

    #[test]
    fn view_box_scales_to_design_box() {
        let vb = ViewBox::parse("0 0 100 100").unwrap();
        let p = parse_path("M 10 10 L 90 10 L 90 90 L 10 90 Z", &vb).unwrap();
        let b = p.bounds();
        assert_eq!(
            (b.left(), b.top(), b.right(), b.bottom()),
            (100.0, 100.0, 900.0, 900.0)
        );
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_path("", &ViewBox::DESIGN).is_err());
        assert!(parse_path("M 10 x", &ViewBox::DESIGN).is_err());
        assert!(ViewBox::parse("0 0 100").is_err());
        assert!(ViewBox::parse("0 0 100 50").is_err());
        assert!(ViewBox::parse("0 0 0 0").is_err());
    }
}
