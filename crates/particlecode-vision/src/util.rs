//! Small shared helpers.

use crate::image::PlaneRef;

/// Mean luma over a disc: the center plus six points on a circle of `radius`, bilinear.
pub(crate) fn disc_mean(luma: PlaneRef<'_>, c: (f64, f64), radius: f64) -> f64 {
    let mut sum = f64::from(luma.bilinear(c.0, c.1));
    for k in 0..6 {
        let a = f64::from(k) * std::f64::consts::FRAC_PI_3;
        sum += f64::from(luma.bilinear(c.0 + radius * a.cos(), c.1 + radius * a.sin()));
    }
    sum / 7.0
}

/// Median of a slice (sorts in place). `0.0` for an empty slice.
pub(crate) fn median(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        0.5 * (v[n / 2 - 1] + v[n / 2])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_works() {
        assert!((median(&mut [3.0, 1.0, 2.0]) - 2.0).abs() < 1e-12);
        assert!((median(&mut [4.0, 1.0, 2.0, 3.0]) - 2.5).abs() < 1e-12);
        assert!(median(&mut []).abs() < 1e-12);
    }
}
