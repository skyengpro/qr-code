//! Sharpness score for choosing the best of several captures of the same frame.

use crate::detect::Lock;
use crate::image::PlaneRef;

/// Variance of the 4-neighbour Laplacian of the luma plane over the field disc of `lock`.
///
/// Higher is sharper. Only meaningful for comparing frames of the same code at a similar
/// scale: the caller picks the best of the ~3 captures of each 100 ms data frame. Returns
/// `0.0` for unusable input.
pub fn sharpness<'a>(luma: impl Into<PlaneRef<'a>>, lock: &Lock) -> f64 {
    let luma: PlaneRef<'_> = luma.into();
    let Some(center) = lock.homography.apply((0.0, 0.0)) else {
        return 0.0;
    };
    let radius = 1.1 * lock.profile.field_half() * lock.pitch_px;
    if !(radius.is_finite() && radius > 4.0) {
        return 0.0;
    }
    let (w, h) = (luma.width() as i64, luma.height() as i64);
    let x0 = ((center.0 - radius).floor() as i64).max(1);
    let x1 = ((center.0 + radius).ceil() as i64).min(w - 2);
    let y0 = ((center.1 - radius).floor() as i64).max(1);
    let y1 = ((center.1 + radius).ceil() as i64).min(h - 2);
    if x1 <= x0 || y1 <= y0 {
        return 0.0;
    }
    // Large regions are subsampled; a Laplacian at every other pixel is as informative.
    let step = if (x1 - x0) * (y1 - y0) > 600_000 {
        2
    } else {
        1
    };
    let (mut n, mut sum, mut sum2) = (0.0f64, 0.0f64, 0.0f64);
    let r2 = radius * radius;
    let mut y = y0;
    while y <= y1 {
        let (up, mid, down) = (
            luma.row((y - 1) as usize),
            luma.row(y as usize),
            luma.row((y + 1) as usize),
        );
        let dy = y as f64 - center.1;
        let mut x = x0;
        while x <= x1 {
            let dx = x as f64 - center.0;
            if dx * dx + dy * dy <= r2 {
                let xi = x as usize;
                let lap = f64::from(up[xi])
                    + f64::from(down[xi])
                    + f64::from(mid[xi - 1])
                    + f64::from(mid[xi + 1])
                    - 4.0 * f64::from(mid[xi]);
                n += 1.0;
                sum += lap;
                sum2 += lap * lap;
            }
            x += step;
        }
        y += step;
    }
    if n < 16.0 {
        return 0.0;
    }
    let mean = sum / n;
    (sum2 / n - mean * mean).max(0.0)
}
