//! Static vector output for print.

use std::fmt::Write as _;

use particlecode_core::FrameSpec;
use particlecode_core::frame::{AnchorSpec, Dot};
use particlecode_core::layout::{Point, dims};
use particlecode_core::profile::{Rgb, colors};

use crate::color::hex;
use crate::config::RenderConfig;

/// Render one frame as a standalone SVG document (static, no motion).
///
/// The `viewBox` is in dot pitches, centred on the frame and covering
/// `[-(R + margin), R + margin]²`, so a printer can scale the whole document to a physical
/// size and know the pitch exactly. `width` and `height` are `cfg.size_px`. Anchor rings
/// are one even-odd path each; every dot, swatch and particle is a plain `<circle>` with a
/// flat fill and no stroke. Circles are grouped by colour to keep the file small.
pub fn render_svg(frame: &FrameSpec, cfg: &RenderConfig) -> String {
    let half = cfg.half_extent(frame);
    let size = cfg.size_px.max(1);
    let mut s = String::with_capacity(64 * 1024);
    // Writing to a String cannot fail, so the results of `write!` are ignored.
    let _ = writeln!(
        s,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{size}\" height=\"{size}\" \
         viewBox=\"{:.3} {:.3} {:.3} {:.3}\" shape-rendering=\"geometricPrecision\">",
        -half,
        -half,
        2.0 * half,
        2.0 * half
    );
    let _ = writeln!(
        s,
        "<rect x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" fill=\"{}\"/>",
        -half,
        -half,
        2.0 * half,
        2.0 * half,
        hex(cfg.background)
    );

    for a in &frame.anchors {
        anchor(&mut s, a, cfg.background);
    }

    // Swatches, then header dots, then payload dots, then particles; each group is one
    // colour so the file is a handful of `<g>` elements.
    let swatch_colors: Vec<(Rgb, Point)> = frame
        .swatches
        .iter()
        .map(|sw| {
            let rgb = match sw.color {
                particlecode_core::layout::SwatchColor::Hue(h) => colors::HUES4[usize::from(h & 3)],
                particlecode_core::layout::SwatchColor::White => colors::WHITE,
                particlecode_core::layout::SwatchColor::Black => colors::BLACK,
            };
            (rgb, sw.at)
        })
        .collect();
    for rgb in distinct(swatch_colors.iter().map(|(c, _)| *c)) {
        group(
            &mut s,
            rgb,
            swatch_colors
                .iter()
                .filter(|(c, _)| *c == rgb)
                .map(|(_, p)| (*p, dims::SWATCH_DOT_R)),
        );
    }
    dot_groups(&mut s, &frame.header_dots);
    dot_groups(&mut s, &frame.dots);

    let band = &frame.particle_band;
    group(
        &mut s,
        colors::PARTICLE,
        frame.particles.iter().map(|p| {
            (
                Point::default().polar(band.mid(), p.angle),
                dims::PARTICLE_R,
            )
        }),
    );
    s.push_str("</svg>\n");
    s
}

fn distinct(colors: impl Iterator<Item = Rgb>) -> Vec<Rgb> {
    let mut out: Vec<Rgb> = Vec::new();
    for c in colors {
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

fn group(s: &mut String, fill: Rgb, circles: impl Iterator<Item = (Point, f64)>) {
    let _ = writeln!(s, "<g fill=\"{}\">", hex(fill));
    for (p, r) in circles {
        let _ = writeln!(
            s,
            "<circle cx=\"{:.3}\" cy=\"{:.3}\" r=\"{r:.3}\"/>",
            p.x, p.y
        );
    }
    s.push_str("</g>\n");
}

fn dot_groups(s: &mut String, dots: &[Dot]) {
    for hue in 0..4u8 {
        let fill = colors::HUES4[usize::from(hue)];
        let mut it = dots.iter().filter(|d| d.color & 3 == hue).peekable();
        if it.peek().is_some() {
            group(s, fill, it.map(|d| (d.at, d.radius())));
        }
    }
}

fn anchor(s: &mut String, a: &AnchorSpec, pupil: Rgb) {
    let ring = |r: f64| {
        format!(
            "M{:.3} {:.3}A{r:.3} {r:.3} 0 1 0 {:.3} {:.3}A{r:.3} {r:.3} 0 1 0 {:.3} {:.3}Z",
            a.at.x - r,
            a.at.y,
            a.at.x + r,
            a.at.y,
            a.at.x - r,
            a.at.y,
        )
    };
    let _ = writeln!(
        s,
        "<path fill=\"{}\" fill-rule=\"evenodd\" d=\"{}{}\"/>",
        hex(colors::ANCHOR),
        ring(a.ring_out),
        ring(a.ring_in)
    );
    let _ = writeln!(
        s,
        "<circle cx=\"{:.3}\" cy=\"{:.3}\" r=\"{:.3}\" fill=\"{}\"/>",
        a.at.x,
        a.at.y,
        a.core_r,
        hex(colors::ANCHOR)
    );
    if a.pupil_r > 0.0 {
        let _ = writeln!(
            s,
            "<circle cx=\"{:.3}\" cy=\"{:.3}\" r=\"{:.3}\" fill=\"{}\"/>",
            a.at.x,
            a.at.y,
            a.pupil_r,
            hex(pupil)
        );
    }
}
