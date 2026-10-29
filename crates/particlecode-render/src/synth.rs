//! Synthetic camera degradations for vision tests, the shape validator and benchmarks.
//!
//! Everything works on [`RgbImage`] (packed RGB8) and is deterministic: the only source of
//! randomness is the seedable [`Rng`] in this module, so a test that adds noise with seed
//! 7 sees the same noise on every machine. The operations are meant to be chained the way
//! a phone camera would: warp the screen into the view ([`RgbImage::perspective`]), blur
//! (lens, motion), add glare and exposure error, tint, add sensor noise, downscale to the
//! camera resolution. [`Degradation`] and [`apply_all`] describe such a chain as data.
//!
//! Pixel coordinates are continuous: pixel `(i, j)` covers `[i, i + 1) × [j, j + 1)` and
//! its centre is `(i + 0.5, j + 0.5)`.

use particlecode_core::profile::Rgb;
use tiny_skia::Pixmap;

use crate::error::RenderError;
use crate::png_out::encode_rgb;

/// A small, fast, seedable generator (`SplitMix64`). Not cryptographic.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    /// Generator for `seed`. Equal seeds give equal streams.
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    /// Next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform integer in `0..n` (`n > 0`; 0 for `n == 0`).
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next_u64() % n }
    }

    /// Standard normal sample (Box-Muller).
    pub fn gaussian(&mut self) -> f64 {
        let u1 = 1.0 - self.next_f64(); // (0, 1]
        let u2 = self.next_f64();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

/// A packed RGB8 image, row-major, `data.len() == 3 * width * height`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `R, G, B` bytes, row-major.
    pub data: Vec<u8>,
}

fn clamp_u8(v: f32) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

impl RgbImage {
    /// A `width × height` image filled with one colour.
    pub fn new(width: u32, height: u32, fill: Rgb) -> RgbImage {
        let n = width as usize * height as usize;
        let mut data = Vec::with_capacity(n * 3);
        for _ in 0..n {
            data.extend_from_slice(&[fill.0, fill.1, fill.2]);
        }
        RgbImage {
            width,
            height,
            data,
        }
    }

    /// Wrap packed RGB8 data; `None` if the length does not match.
    pub fn from_rgb(width: u32, height: u32, data: Vec<u8>) -> Option<RgbImage> {
        (data.len() == width as usize * height as usize * 3).then_some(RgbImage {
            width,
            height,
            data,
        })
    }

    /// Convert a rendered pixmap (alpha is demultiplied and dropped).
    pub fn from_pixmap(pm: &Pixmap) -> RgbImage {
        let mut data = Vec::with_capacity(pm.pixels().len() * 3);
        for p in pm.pixels() {
            let c = p.demultiply();
            data.extend_from_slice(&[c.red(), c.green(), c.blue()]);
        }
        RgbImage {
            width: pm.width(),
            height: pm.height(),
            data,
        }
    }

    /// Pixel at `(x, y)`; coordinates are clamped to the image.
    pub fn pixel(&self, x: i64, y: i64) -> Rgb {
        let [r, g, b] = self.fetch(x, y);
        Rgb(r, g, b)
    }

    fn fetch(&self, x: i64, y: i64) -> [u8; 3] {
        if self.width == 0 || self.height == 0 {
            return [0; 3];
        }
        let x = x.clamp(0, i64::from(self.width) - 1) as usize;
        let y = y.clamp(0, i64::from(self.height) - 1) as usize;
        let i = (y * self.width as usize + x) * 3;
        [self.data[i], self.data[i + 1], self.data[i + 2]]
    }

    /// Bilinear sample at continuous coordinates (pixel centres at `i + 0.5`), edge-clamped.
    pub fn sample_bilinear(&self, x: f64, y: f64) -> [f32; 3] {
        let fx = x - 0.5;
        let fy = y - 0.5;
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = ((fx - x0) as f32, (fy - y0) as f32);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let (p00, p10) = (self.fetch(x0, y0), self.fetch(x0 + 1, y0));
        let (p01, p11) = (self.fetch(x0, y0 + 1), self.fetch(x0 + 1, y0 + 1));
        let mut out = [0.0; 3];
        for c in 0..3 {
            let top = f32::from(p00[c]) * (1.0 - tx) + f32::from(p10[c]) * tx;
            let bot = f32::from(p01[c]) * (1.0 - tx) + f32::from(p11[c]) * tx;
            out[c] = top * (1.0 - ty) + bot * ty;
        }
        out
    }

    /// Encode as an RGB PNG (handy for looking at a degraded image).
    pub fn to_png(&self) -> Result<Vec<u8>, RenderError> {
        encode_rgb(&self.data, self.width, self.height)
    }

    /// Gaussian blur with standard deviation `sigma` pixels (separable, edge-clamped).
    /// `sigma <= 0.05` returns a copy.
    #[must_use]
    pub fn gaussian_blur(&self, sigma: f64) -> RgbImage {
        if sigma <= 0.05 || self.width == 0 || self.height == 0 {
            return self.clone();
        }
        let radius = (3.0 * sigma).ceil() as i64;
        let mut kernel: Vec<f32> = (-radius..=radius)
            .map(|i| (-((i * i) as f64) / (2.0 * sigma * sigma)).exp() as f32)
            .collect();
        let sum: f32 = kernel.iter().sum();
        for k in &mut kernel {
            *k /= sum;
        }

        let (w, h) = (self.width as usize, self.height as usize);
        let src: Vec<f32> = self.data.iter().map(|&v| f32::from(v)).collect();
        // Horizontal pass.
        let mut tmp = vec![0.0f32; src.len()];
        for y in 0..h {
            let row = &src[y * w * 3..(y + 1) * w * 3];
            for x in 0..w {
                let mut acc = [0.0f32; 3];
                for (ki, &k) in kernel.iter().enumerate() {
                    let sx = (x as i64 + ki as i64 - radius).clamp(0, w as i64 - 1) as usize;
                    for c in 0..3 {
                        acc[c] += k * row[sx * 3 + c];
                    }
                }
                tmp[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&acc);
            }
        }
        // Vertical pass.
        let mut out = vec![0u8; src.len()];
        for y in 0..h {
            for x in 0..w {
                let mut acc = [0.0f32; 3];
                for (ki, &k) in kernel.iter().enumerate() {
                    let sy = (y as i64 + ki as i64 - radius).clamp(0, h as i64 - 1) as usize;
                    for c in 0..3 {
                        acc[c] += k * tmp[(sy * w + x) * 3 + c];
                    }
                }
                for c in 0..3 {
                    out[(y * w + x) * 3 + c] = clamp_u8(acc[c]);
                }
            }
        }
        RgbImage {
            width: self.width,
            height: self.height,
            data: out,
        }
    }

    /// Warp so that the source's corners (top-left, top-right, bottom-right, bottom-left)
    /// land on `quad` (output pixel coordinates), into an `out_w × out_h` image whose
    /// uncovered area is `background`. Bilinear sampling with a half-pixel anti-aliased
    /// edge. A degenerate quad returns an all-background image.
    #[must_use]
    pub fn perspective(
        &self,
        quad: [(f64, f64); 4],
        out_w: u32,
        out_h: u32,
        background: Rgb,
    ) -> RgbImage {
        let (w, h) = (f64::from(self.width), f64::from(self.height));
        let rect = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)];
        match Homography::from_points(rect, quad) {
            Some(hom) => self.warp(&hom, out_w, out_h, background),
            None => RgbImage::new(out_w, out_h, background),
        }
    }

    /// Warp with an explicit homography mapping source to output pixel coordinates.
    #[must_use]
    pub fn warp(&self, hom: &Homography, out_w: u32, out_h: u32, background: Rgb) -> RgbImage {
        let mut out = RgbImage::new(out_w, out_h, background);
        let Some(inv) = hom.inverse() else {
            return out;
        };
        let (w, h) = (f64::from(self.width), f64::from(self.height));
        let bg = [
            f32::from(background.0),
            f32::from(background.1),
            f32::from(background.2),
        ];
        for y in 0..out_h {
            for x in 0..out_w {
                let Some((sx, sy)) = inv.apply(f64::from(x) + 0.5, f64::from(y) + 0.5) else {
                    continue;
                };
                // Coverage of the output pixel by the source rectangle (soft edge).
                let inside = (sx.min(w - sx).min(sy).min(h - sy) + 0.5).clamp(0.0, 1.0) as f32;
                if inside <= 0.0 {
                    continue;
                }
                let s = self.sample_bilinear(sx, sy);
                let i = (y as usize * out_w as usize + x as usize) * 3;
                for c in 0..3 {
                    out.data[i + c] = clamp_u8(s[c] * inside + bg[c] * (1.0 - inside));
                }
            }
        }
        out
    }

    /// Add zero-mean Gaussian noise of standard deviation `sigma` (in 0..255 units) to every
    /// channel of every pixel, deterministically from `seed`.
    #[must_use]
    pub fn add_noise(&self, sigma: f64, seed: u64) -> RgbImage {
        let mut rng = Rng::new(seed);
        let data = self
            .data
            .iter()
            .map(|&v| clamp_u8(f32::from(v) + (rng.gaussian() * sigma) as f32))
            .collect();
        RgbImage {
            width: self.width,
            height: self.height,
            data,
        }
    }

    /// A soft specular highlight: pixels blend towards white by
    /// `strength * exp(-d² / 2 radius²)` where `d` is the distance to `center` (pixels).
    #[must_use]
    pub fn glare(&self, center: (f64, f64), radius: f64, strength: f64) -> RgbImage {
        let mut out = self.clone();
        let inv = 1.0 / (2.0 * radius.max(1e-6).powi(2));
        for y in 0..self.height {
            for x in 0..self.width {
                let dx = f64::from(x) + 0.5 - center.0;
                let dy = f64::from(y) + 0.5 - center.1;
                let wgt = ((strength * (-(dx * dx + dy * dy) * inv).exp()).clamp(0.0, 1.0)) as f32;
                if wgt < 1e-4 {
                    continue;
                }
                let i = (y as usize * self.width as usize + x as usize) * 3;
                for c in 0..3 {
                    let v = f32::from(out.data[i + c]);
                    out.data[i + c] = clamp_u8(v + (255.0 - v) * wgt);
                }
            }
        }
        out
    }

    /// Linear exposure change: `v * gain + offset`, clamped (a clipped highlight stays white).
    #[must_use]
    pub fn exposure(&self, gain: f64, offset: f64) -> RgbImage {
        let data = self
            .data
            .iter()
            .map(|&v| clamp_u8((f64::from(v) * gain + offset) as f32))
            .collect();
        RgbImage {
            width: self.width,
            height: self.height,
            data,
        }
    }

    /// Per-channel gains, e.g. a warm white balance `(1.08, 1.0, 0.88)`.
    #[must_use]
    pub fn color_cast(&self, r: f64, g: f64, b: f64) -> RgbImage {
        let gains = [r, g, b];
        let data = self
            .data
            .iter()
            .enumerate()
            .map(|(i, &v)| clamp_u8((f64::from(v) * gains[i % 3]) as f32))
            .collect();
        RgbImage {
            width: self.width,
            height: self.height,
            data,
        }
    }

    /// Resample to `out_w × out_h`: area averaging when shrinking an axis, linear
    /// interpolation when enlarging it (what a lower-resolution sensor sees).
    #[must_use]
    pub fn downscale(&self, out_w: u32, out_h: u32) -> RgbImage {
        let (out_w, out_h) = (out_w.max(1), out_h.max(1));
        if self.width == 0 || self.height == 0 {
            return RgbImage::new(out_w, out_h, Rgb(0, 0, 0));
        }
        let (w, h) = (self.width as usize, self.height as usize);
        let wx = axis_weights(w, out_w as usize);
        let wy = axis_weights(h, out_h as usize);
        let ow = out_w as usize;
        let mut tmp = vec![0.0f32; ow * h * 3];
        for y in 0..h {
            for (x, taps) in wx.iter().enumerate() {
                for &(sx, wt) in taps {
                    for c in 0..3 {
                        tmp[(y * ow + x) * 3 + c] +=
                            wt * f32::from(self.data[(y * w + sx) * 3 + c]);
                    }
                }
            }
        }
        let mut data = vec![0u8; ow * out_h as usize * 3];
        for (y, taps) in wy.iter().enumerate() {
            for x in 0..ow {
                let mut acc = [0.0f32; 3];
                for &(sy, wt) in taps {
                    for c in 0..3 {
                        acc[c] += wt * tmp[(sy * ow + x) * 3 + c];
                    }
                }
                for c in 0..3 {
                    data[(y * ow + x) * 3 + c] = clamp_u8(acc[c]);
                }
            }
        }
        RgbImage {
            width: out_w,
            height: out_h,
            data,
        }
    }
}

/// For each output sample along one axis, the `(source index, weight)` taps (weights sum
/// to 1): box filter when shrinking, linear interpolation when enlarging.
fn axis_weights(src: usize, dst: usize) -> Vec<Vec<(usize, f32)>> {
    let ratio = src as f64 / dst as f64;
    (0..dst)
        .map(|i| {
            if ratio >= 1.0 {
                let (a, b) = (i as f64 * ratio, (i + 1) as f64 * ratio);
                let first = a.floor() as usize;
                let last = ((b.ceil() as usize).min(src)).max(first + 1);
                let mut taps: Vec<(usize, f32)> = (first..last)
                    .map(|s| {
                        let ov = b.min((s + 1) as f64) - a.max(s as f64);
                        (s.min(src - 1), (ov.max(0.0) / ratio) as f32)
                    })
                    .collect();
                let total: f32 = taps.iter().map(|t| t.1).sum();
                for t in &mut taps {
                    t.1 /= total.max(1e-9);
                }
                taps
            } else {
                let c = (i as f64 + 0.5) * ratio - 0.5;
                let lo = c.floor();
                let t = (c - lo) as f32;
                let (a, b) = (
                    lo.max(0.0) as usize,
                    ((lo + 1.0).max(0.0) as usize).min(src - 1),
                );
                vec![(a.min(src - 1), 1.0 - t), (b, t)]
            }
        })
        .collect()
}

/// A 3×3 projective transform acting on continuous pixel coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Homography(pub [[f64; 3]; 3]);

impl Homography {
    /// The transform taking `src[i]` to `dst[i]` for four point pairs, or `None` if the
    /// configuration is degenerate (three collinear points).
    pub fn from_points(src: [(f64, f64); 4], dst: [(f64, f64); 4]) -> Option<Homography> {
        // Unknowns h0..h7 with h8 = 1; two equations per pair.
        let mut a = [[0.0f64; 9]; 8];
        for i in 0..4 {
            let (x, y) = src[i];
            let (u, v) = dst[i];
            a[2 * i] = [x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y, u];
            a[2 * i + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y, v];
        }
        for col in 0..8 {
            let piv = (col..8)
                .max_by(|&p, &q| a[p][col].abs().total_cmp(&a[q][col].abs()))
                .unwrap_or(col);
            if a[piv][col].abs() < 1e-12 {
                return None;
            }
            a.swap(col, piv);
            let d = a[col][col];
            for v in &mut a[col][col..] {
                *v /= d;
            }
            let pivot_row = a[col];
            for (r, row) in a.iter_mut().enumerate() {
                if r != col {
                    let f = row[col];
                    for (x, p) in row[col..].iter_mut().zip(&pivot_row[col..]) {
                        *x -= f * p;
                    }
                }
            }
        }
        let h = |i: usize| a[i][8];
        Some(Homography([
            [h(0), h(1), h(2)],
            [h(3), h(4), h(5)],
            [h(6), h(7), 1.0],
        ]))
    }

    /// Image of `(x, y)`, or `None` if it maps to infinity.
    pub fn apply(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        let m = &self.0;
        let w = m[2][0] * x + m[2][1] * y + m[2][2];
        (w.abs() > 1e-12).then(|| {
            (
                (m[0][0] * x + m[0][1] * y + m[0][2]) / w,
                (m[1][0] * x + m[1][1] * y + m[1][2]) / w,
            )
        })
    }

    /// The inverse transform, or `None` if singular.
    pub fn inverse(&self) -> Option<Homography> {
        let m = &self.0;
        let c = [
            [
                m[1][1] * m[2][2] - m[1][2] * m[2][1],
                m[0][2] * m[2][1] - m[0][1] * m[2][2],
                m[0][1] * m[1][2] - m[0][2] * m[1][1],
            ],
            [
                m[1][2] * m[2][0] - m[1][0] * m[2][2],
                m[0][0] * m[2][2] - m[0][2] * m[2][0],
                m[0][2] * m[1][0] - m[0][0] * m[1][2],
            ],
            [
                m[1][0] * m[2][1] - m[1][1] * m[2][0],
                m[0][1] * m[2][0] - m[0][0] * m[2][1],
                m[0][0] * m[1][1] - m[0][1] * m[1][0],
            ],
        ];
        let det = m[0][0] * c[0][0] + m[0][1] * c[1][0] + m[0][2] * c[2][0];
        (det.abs() > 1e-15).then(|| {
            let mut r = c;
            r.iter_mut().flatten().for_each(|v| *v /= det);
            Homography(r)
        })
    }

    /// A pinhole-camera view of a flat `src_w × src_h` image: rotate in the image plane,
    /// tilt the plane about the horizontal and vertical axes (see [`CameraPose`]) and
    /// project it (focal length 1.2 × `out_w`) so the untilted image would span
    /// `pose.scale × out_w` pixels, centred in the `out_w × out_h` output.
    pub fn camera_view(
        src_w: u32,
        src_h: u32,
        out_w: u32,
        out_h: u32,
        pose: CameraPose,
    ) -> Homography {
        let CameraPose {
            rotate_deg,
            tilt_x_deg,
            tilt_y_deg,
            scale,
        } = pose;
        let f = 1.2 * f64::from(out_w);
        let dist = f * f64::from(src_w) / (scale.max(1e-6) * f64::from(out_w));
        let (rz, rx, ry) = (
            rotate_deg.to_radians(),
            tilt_x_deg.to_radians(),
            tilt_y_deg.to_radians(),
        );
        let (cz, sz) = (rz.cos(), rz.sin());
        let (cx, sx) = (rx.cos(), rx.sin());
        let (cy, sy) = (ry.cos(), ry.sin());
        // R = Ry · Rx · Rz acting on (x, y, 0).
        let rzm = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
        let rxm = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
        let rym = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
        let r = mul3(&rym, &mul3(&rxm, &rzm));
        // Columns: image x axis, image y axis, translation (0, 0, dist).
        let cam = [
            [r[0][0], r[0][1], 0.0],
            [r[1][0], r[1][1], 0.0],
            [r[2][0], r[2][1], dist],
        ];
        let k = [
            [f, 0.0, f64::from(out_w) / 2.0],
            [0.0, f, f64::from(out_h) / 2.0],
            [0.0, 0.0, 1.0],
        ];
        let centre = [
            [1.0, 0.0, -f64::from(src_w) / 2.0],
            [0.0, 1.0, -f64::from(src_h) / 2.0],
            [0.0, 0.0, 1.0],
        ];
        Homography(mul3(&k, &mul3(&cam, &centre)))
    }
}

fn mul3(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut r = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            r[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    r
}

/// How a flat image is held in front of the camera, for [`Homography::camera_view`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraPose {
    /// In-plane rotation in degrees.
    pub rotate_deg: f64,
    /// Tilt about the horizontal axis in degrees.
    pub tilt_x_deg: f64,
    /// Tilt about the vertical axis in degrees.
    pub tilt_y_deg: f64,
    /// Width of the untilted image as a fraction of the output width.
    pub scale: f64,
}

/// One step of a degradation chain, applied by [`apply_all`].
#[derive(Debug, Clone, PartialEq)]
pub enum Degradation {
    /// [`RgbImage::gaussian_blur`].
    Blur {
        /// Standard deviation in pixels.
        sigma: f64,
    },
    /// [`RgbImage::perspective`].
    Perspective {
        /// Where the source's TL, TR, BR, BL corners land, in output pixels.
        quad: [(f64, f64); 4],
        /// Output width.
        out_w: u32,
        /// Output height.
        out_h: u32,
        /// Colour outside the warped image.
        background: Rgb,
    },
    /// [`Homography::camera_view`] followed by [`RgbImage::warp`].
    Tilt {
        /// How the image is held.
        pose: CameraPose,
        /// Output width.
        out_w: u32,
        /// Output height.
        out_h: u32,
        /// Colour outside the warped image.
        background: Rgb,
    },
    /// [`RgbImage::add_noise`].
    Noise {
        /// Standard deviation in 0..255 units.
        sigma: f64,
        /// Noise seed.
        seed: u64,
    },
    /// [`RgbImage::glare`].
    Glare {
        /// Centre in pixels.
        center: (f64, f64),
        /// Gaussian radius in pixels.
        radius: f64,
        /// Peak blend towards white, `0..=1`.
        strength: f64,
    },
    /// [`RgbImage::exposure`].
    Exposure {
        /// Multiplier.
        gain: f64,
        /// Additive offset in 0..255 units.
        offset: f64,
    },
    /// [`RgbImage::color_cast`].
    ColorCast {
        /// Red gain.
        r: f64,
        /// Green gain.
        g: f64,
        /// Blue gain.
        b: f64,
    },
    /// [`RgbImage::downscale`].
    Downscale {
        /// Output width.
        width: u32,
        /// Output height.
        height: u32,
    },
}

impl Degradation {
    /// Apply this step to `img`.
    pub fn apply(&self, img: &RgbImage) -> RgbImage {
        match *self {
            Degradation::Blur { sigma } => img.gaussian_blur(sigma),
            Degradation::Perspective {
                quad,
                out_w,
                out_h,
                background,
            } => img.perspective(quad, out_w, out_h, background),
            Degradation::Tilt {
                pose,
                out_w,
                out_h,
                background,
            } => {
                let h = Homography::camera_view(img.width, img.height, out_w, out_h, pose);
                img.warp(&h, out_w, out_h, background)
            }
            Degradation::Noise { sigma, seed } => img.add_noise(sigma, seed),
            Degradation::Glare {
                center,
                radius,
                strength,
            } => img.glare(center, radius, strength),
            Degradation::Exposure { gain, offset } => img.exposure(gain, offset),
            Degradation::ColorCast { r, g, b } => img.color_cast(r, g, b),
            Degradation::Downscale { width, height } => img.downscale(width, height),
        }
    }
}

/// Apply a chain of degradations in order.
pub fn apply_all(img: &RgbImage, chain: &[Degradation]) -> RgbImage {
    chain.iter().fold(img.clone(), |acc, d| d.apply(&acc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker(w: u32, h: u32, cell: u32) -> RgbImage {
        let mut img = RgbImage::new(w, h, Rgb(0, 0, 0));
        for y in 0..h {
            for x in 0..w {
                if ((x / cell) + (y / cell)).is_multiple_of(2) {
                    let i = (y as usize * w as usize + x as usize) * 3;
                    img.data[i..i + 3].copy_from_slice(&[255, 255, 255]);
                }
            }
        }
        img
    }

    fn mean(img: &RgbImage) -> f64 {
        img.data.iter().map(|&v| f64::from(v)).sum::<f64>() / img.data.len() as f64
    }

    #[test]
    fn rng_is_deterministic_and_roughly_normal() {
        let mut a = Rng::new(5);
        let mut b = Rng::new(5);
        assert_eq!(a.next_u64(), b.next_u64());
        let mut r = Rng::new(9);
        let n = 20_000;
        let xs: Vec<f64> = (0..n).map(|_| r.gaussian()).collect();
        let m = xs.iter().sum::<f64>() / f64::from(n);
        let v = xs.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / f64::from(n);
        assert!(m.abs() < 0.03 && (v - 1.0).abs() < 0.05, "{m} {v}");
        assert!((0..1000).all(|_| r.next_f64() < 1.0));
        assert_eq!(r.below(0), 0);
    }

    #[test]
    fn blur_keeps_flat_areas_and_softens_edges() {
        let flat = RgbImage::new(20, 20, Rgb(10, 200, 30));
        assert_eq!(flat.gaussian_blur(2.0), flat);
        let img = checker(40, 40, 8);
        let soft = img.gaussian_blur(2.5);
        assert!((mean(&img) - mean(&soft)).abs() < 3.0);
        // The pixel next to an edge is no longer pure black or white.
        let edge = soft.pixel(7, 4).0;
        assert!((20..235).contains(&edge), "{edge}");
        assert_eq!(img.gaussian_blur(0.0), img);
    }

    #[test]
    fn identity_perspective_reproduces_the_image() {
        let img = checker(32, 24, 4);
        let quad = [(0.0, 0.0), (32.0, 0.0), (32.0, 24.0), (0.0, 24.0)];
        let out = img.perspective(quad, 32, 24, Rgb(9, 9, 9));
        let max_diff = img
            .data
            .iter()
            .zip(&out.data)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        assert!(max_diff <= 1, "{max_diff}");
    }

    #[test]
    fn perspective_moves_the_corners_and_fills_the_rest() {
        let img = RgbImage::new(20, 20, Rgb(200, 0, 0));
        let quad = [(20.0, 10.0), (60.0, 5.0), (65.0, 45.0), (15.0, 50.0)];
        let out = img.perspective(quad, 80, 60, Rgb(1, 2, 3));
        assert_eq!(out.pixel(2, 2), Rgb(1, 2, 3));
        assert_eq!(out.pixel(40, 28), Rgb(200, 0, 0));
        assert_eq!(out.pixel(79, 59), Rgb(1, 2, 3));
    }

    #[test]
    fn homography_round_trips() {
        let src = [(0.0, 0.0), (100.0, 0.0), (100.0, 80.0), (0.0, 80.0)];
        let dst = [(12.0, 9.0), (90.0, 20.0), (80.0, 95.0), (5.0, 70.0)];
        let h = Homography::from_points(src, dst).expect("non-degenerate");
        for (s, d) in src.iter().zip(&dst) {
            let (u, v) = h.apply(s.0, s.1).expect("finite");
            assert!((u - d.0).abs() < 1e-6 && (v - d.1).abs() < 1e-6);
        }
        let inv = h.inverse().expect("invertible");
        let (x, y) = inv.apply(dst[2].0, dst[2].1).expect("finite");
        assert!((x - 100.0).abs() < 1e-6 && (y - 80.0).abs() < 1e-6);
        // Collinear points are rejected.
        let line = [(0.0, 0.0), (1.0, 0.0), (2.0, 0.0), (3.0, 0.0)];
        assert!(Homography::from_points(line, dst).is_none());
    }

    #[test]
    fn camera_view_without_tilt_is_a_centred_scaling() {
        let h = Homography::camera_view(
            100,
            100,
            200,
            200,
            CameraPose {
                rotate_deg: 0.0,
                tilt_x_deg: 0.0,
                tilt_y_deg: 0.0,
                scale: 0.5,
            },
        );
        let (x, y) = h.apply(50.0, 50.0).expect("finite");
        assert!((x - 100.0).abs() < 1e-6 && (y - 100.0).abs() < 1e-6);
        let (x, _) = h.apply(100.0, 50.0).expect("finite");
        assert!((x - 150.0).abs() < 1e-6, "{x}"); // half of 200 px wide
    }

    #[test]
    fn noise_is_seeded_and_zero_mean() {
        let img = RgbImage::new(64, 64, Rgb(128, 128, 128));
        let a = img.add_noise(10.0, 1);
        assert_eq!(a, img.add_noise(10.0, 1));
        assert_ne!(a, img.add_noise(10.0, 2));
        assert!((mean(&a) - 128.0).abs() < 0.5);
        assert_eq!(img.add_noise(0.0, 3), img);
    }

    #[test]
    fn glare_brightens_towards_the_centre_only() {
        let img = RgbImage::new(60, 60, Rgb(100, 100, 100));
        let g = img.glare((30.0, 30.0), 6.0, 0.8);
        assert!(g.pixel(30, 30).0 > 200);
        assert_eq!(g.pixel(0, 0), Rgb(100, 100, 100));
    }

    #[test]
    fn exposure_and_cast() {
        let img = RgbImage::new(2, 2, Rgb(100, 150, 200));
        assert_eq!(img.exposure(1.5, 0.0).pixel(0, 0), Rgb(150, 225, 255));
        assert_eq!(img.exposure(1.0, -50.0).pixel(0, 0), Rgb(50, 100, 150));
        assert_eq!(img.color_cast(1.0, 0.5, 0.0).pixel(1, 1), Rgb(100, 75, 0));
    }

    #[test]
    fn downscale_averages() {
        let img = checker(16, 16, 1);
        let half = img.downscale(8, 8);
        assert!(half.data.iter().all(|&v| (126..=129).contains(&v)));
        let flat = RgbImage::new(10, 10, Rgb(7, 8, 9)).downscale(3, 4);
        assert_eq!((flat.width, flat.height), (3, 4));
        assert_eq!(flat.pixel(1, 1), Rgb(7, 8, 9));
        let up = RgbImage::new(4, 4, Rgb(7, 8, 9)).downscale(9, 9);
        assert_eq!(up.pixel(4, 4), Rgb(7, 8, 9));
    }

    #[test]
    fn chain_applies_in_order_and_is_fast_enough() {
        let img = checker(600, 600, 20);
        let chain = [
            Degradation::Tilt {
                pose: CameraPose {
                    rotate_deg: 8.0,
                    tilt_x_deg: 20.0,
                    tilt_y_deg: -10.0,
                    scale: 0.8,
                },
                out_w: 600,
                out_h: 600,
                background: Rgb(60, 60, 60),
            },
            Degradation::Blur { sigma: 1.5 },
            Degradation::Glare {
                center: (400.0, 200.0),
                radius: 60.0,
                strength: 0.5,
            },
            Degradation::Exposure {
                gain: 0.9,
                offset: 5.0,
            },
            Degradation::ColorCast {
                r: 1.05,
                g: 1.0,
                b: 0.9,
            },
            Degradation::Noise {
                sigma: 4.0,
                seed: 3,
            },
            Degradation::Downscale {
                width: 400,
                height: 400,
            },
        ];
        let out = apply_all(&img, &chain);
        assert_eq!((out.width, out.height), (400, 400));
        assert_eq!(out, apply_all(&img, &chain));
        let swapped = apply_all(&img, &[chain[1].clone(), chain[0].clone()]);
        assert_ne!(swapped, apply_all(&img, &chain[..2]));
    }

    #[test]
    fn png_export_has_the_png_signature() {
        let bytes = RgbImage::new(3, 3, Rgb(1, 2, 3)).to_png().expect("encodes");
        assert_eq!(&bytes[..4], &[0x89, b'P', b'N', b'G']);
        assert!(RgbImage::from_rgb(2, 2, vec![0; 11]).is_none());
    }
}
