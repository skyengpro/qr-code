//! Sub-pixel localisation of the dark ring-and-core anchors.

use crate::image::PlaneRef;

/// Result of [`refine_dark_blob`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct Blob {
    /// Centroid, image pixels.
    pub(crate) at: (f64, f64),
    /// Total weight (≈ dark area in pixels²).
    pub(crate) mass: f64,
}

/// Centroid of the *very dark* pixels within `radius` of `start`, iterated three times.
///
/// Anchors are far darker than every data hue, so pixels are weighted by how far below
/// `dark + 0.3 (bright − dark)` they fall, where `dark`/`bright` are the 3rd/97th luma
/// percentiles of the window. Data dots (luma ≥ ~0.4 of the background) get weight zero.
/// The window is a hard disc with a one-pixel antialiased rim; with the whole mark inside
/// it the centroid is unbiased by the mark's shape.
pub(crate) fn refine_dark_blob(
    luma: PlaneRef<'_>,
    start: (f64, f64),
    radius: f64,
    min_contrast: f64,
) -> Option<Blob> {
    if !(1.5..1000.0).contains(&radius) {
        return None;
    }
    let mut c = start;
    let mut last = None;
    for _ in 0..3 {
        let blob = dark_centroid(luma, c, radius, min_contrast)?;
        let moved = (blob.at.0 - c.0).hypot(blob.at.1 - c.1);
        c = blob.at;
        last = Some(blob);
        if moved < 0.02 {
            break;
        }
    }
    // The final centroid must not have wandered away from where the geometry put it.
    let blob = last?;
    ((blob.at.0 - start.0).hypot(blob.at.1 - start.1) <= radius * 0.9).then_some(blob)
}

fn dark_centroid(
    luma: PlaneRef<'_>,
    c: (f64, f64),
    radius: f64,
    min_contrast: f64,
) -> Option<Blob> {
    let half = radius.ceil() as i64 + 1;
    let (cx, cy) = (c.0.round() as i64, c.1.round() as i64);
    let (x0, x1) = ((cx - half).max(0), (cx + half).min(luma.width() as i64 - 1));
    let (y0, y1) = (
        (cy - half).max(0),
        (cy + half).min(luma.height() as i64 - 1),
    );
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    // Luma percentiles over the disc.
    let mut hist = [0u32; 256];
    let mut total = 0u32;
    for y in y0..=y1 {
        let row = luma.row(y as usize);
        let dy = y as f64 - c.1;
        for x in x0..=x1 {
            let dx = x as f64 - c.0;
            if dx * dx + dy * dy <= radius * radius {
                hist[usize::from(row[x as usize])] += 1;
                total += 1;
            }
        }
    }
    if total < 12 {
        return None;
    }
    let dark = percentile(&hist, total, 0.03);
    let bright = percentile(&hist, total, 0.97);
    if bright - dark < min_contrast {
        return None;
    }
    let level = dark + 0.3 * (bright - dark);
    let span = (level - dark).max(1.0);
    let (mut sw, mut sx, mut sy) = (0.0, 0.0, 0.0);
    for y in y0..=y1 {
        let row = luma.row(y as usize);
        let dy = y as f64 - c.1;
        for x in x0..=x1 {
            let dx = x as f64 - c.0;
            let d = (dx * dx + dy * dy).sqrt();
            let rim = (radius - d + 0.5).clamp(0.0, 1.0);
            if rim <= 0.0 {
                continue;
            }
            let w = ((level - f64::from(row[x as usize])) / span).clamp(0.0, 1.0) * rim;
            sw += w;
            sx += w * x as f64;
            sy += w * y as f64;
        }
    }
    (sw > 4.0).then(|| Blob {
        at: (sx / sw, sy / sw),
        mass: sw,
    })
}

fn percentile(hist: &[u32; 256], total: u32, q: f64) -> f64 {
    let target = (f64::from(total) * q).ceil() as u32;
    let mut acc = 0;
    for (v, &n) in hist.iter().enumerate() {
        acc += n;
        if acc >= target {
            return v as f64;
        }
    }
    255.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::Plane;

    fn ring_image(cx: f64, cy: f64) -> Plane {
        let (w, h) = (80, 80);
        let mut data = vec![240u8; w * h];
        for y in 0..h {
            for x in 0..w {
                let d = (x as f64 - cx).hypot(y as f64 - cy);
                if d < 4.5 || (8.0..13.0).contains(&d) {
                    data[y * w + x] = 30;
                }
            }
        }
        Plane::new(w, h, data)
    }

    #[test]
    fn centers_a_ring_and_core() {
        let p = ring_image(40.3, 39.6);
        let blob = refine_dark_blob(p.view().expect("valid"), (42.0, 38.0), 16.0, 60.0)
            .expect("blob found");
        assert!(
            (blob.at.0 - 40.3).abs() < 0.15 && (blob.at.1 - 39.6).abs() < 0.15,
            "{:?}",
            blob.at
        );
    }

    #[test]
    fn flat_image_has_no_blob() {
        let p = Plane::filled(64, 64, 128);
        assert!(refine_dark_blob(p.view().expect("valid"), (32.0, 32.0), 10.0, 60.0).is_none());
    }
}
