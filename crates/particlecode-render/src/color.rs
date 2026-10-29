//! Small colour helpers shared by the painter and the packagers.

use particlecode_core::profile::Rgb;

/// `#rrggbb` string.
pub(crate) fn hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
}

fn mix_u8(a: u8, b: u8, t: f64) -> u8 {
    let v = f64::from(a) + (f64::from(b) - f64::from(a)) * t;
    v.round().clamp(0.0, 255.0) as u8
}

/// Straight sRGB interpolation. Exact at `t <= 0` and `t >= 1`.
pub(crate) fn lerp_rgb(a: Rgb, b: Rgb, t: f64) -> Rgb {
    if t <= 0.0 {
        return a;
    }
    if t >= 1.0 {
        return b;
    }
    Rgb(
        mix_u8(a.0, b.0, t),
        mix_u8(a.1, b.1, t),
        mix_u8(a.2, b.2, t),
    )
}

/// Move `c` a fraction `amount` of the way to white (the ripple's "lighten").
pub(crate) fn lighten(c: Rgb, amount: f64) -> Rgb {
    lerp_rgb(c, Rgb(255, 255, 255), amount)
}

/// RGB (0..255) to HSV with hue in degrees.
fn to_hsv(c: Rgb) -> (f64, f64, f64) {
    let (r, g, b) = (
        f64::from(c.0) / 255.0,
        f64::from(c.1) / 255.0,
        f64::from(c.2) / 255.0,
    );
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d < 1e-12 {
        0.0
    } else if (max - r).abs() < 1e-12 {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if (max - g).abs() < 1e-12 {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, if max <= 0.0 { 0.0 } else { d / max }, max)
}

fn from_hsv(h: f64, s: f64, v: f64) -> Rgb {
    let c = v * s;
    let hp = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (hp.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let q = |u: f64| ((u + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Rgb(q(r), q(g), q(b))
}

/// Interpolate through HSV, rotating the hue along the shortest arc ("colour shimmer").
/// Exact at `t <= 0` and `t >= 1`.
pub(crate) fn lerp_hue(a: Rgb, b: Rgb, t: f64) -> Rgb {
    if t <= 0.0 {
        return a;
    }
    if t >= 1.0 {
        return b;
    }
    let (ha, sa, va) = to_hsv(a);
    let (hb, sb, vb) = to_hsv(b);
    let mut dh = hb - ha;
    if dh > 180.0 {
        dh -= 360.0;
    } else if dh < -180.0 {
        dh += 360.0;
    }
    from_hsv(ha + dh * t, sa + (sb - sa) * t, va + (vb - va) * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use particlecode_core::profile::colors;

    #[test]
    fn lerps_are_exact_at_the_ends() {
        let (a, b) = (colors::HUES4[0], colors::HUES4[2]);
        assert_eq!(lerp_rgb(a, b, 0.0), a);
        assert_eq!(lerp_rgb(a, b, 1.0), b);
        assert_eq!(lerp_hue(a, b, 0.0), a);
        assert_eq!(lerp_hue(a, b, 1.0), b);
    }

    #[test]
    fn hsv_round_trips_the_palette() {
        for c in colors::HUES4 {
            let (h, s, v) = to_hsv(c);
            assert_eq!(from_hsv(h, s, v), c);
        }
    }

    #[test]
    fn hue_lerp_keeps_saturation() {
        let m = lerp_hue(colors::HUES4[0], colors::HUES4[1], 0.5);
        let (_, s, v) = to_hsv(m);
        assert!(s > 0.5 && v > 0.5, "{m:?}");
    }

    #[test]
    fn hex_format() {
        assert_eq!(hex(Rgb(1, 171, 255)), "#01abff");
    }
}
