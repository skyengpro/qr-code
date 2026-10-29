//! Test-only synthetic camera: rasterises a `FrameSpec` in the exact palette and degrades
//! it with blur, perspective, rotation, gain and noise.
#![allow(dead_code, unreachable_pub)]

use particlecode_core::Profile;
use particlecode_core::frame::FrameSpec;
use particlecode_core::header::Header;
use particlecode_core::layout::{self, DotOrder, Lattice, SwatchColor, dims};
use particlecode_core::profile::{Palette, Rgb, colors};
use particlecode_vision::{Homography, YuvFrame, locate, read_with_order};

/// Confidence below which the codec layer treats a symbol as an erasure
/// (`particlecode_core::rs::ERASURE_BELOW` once the codec package lands).
pub const ERASURE_BELOW: u8 = 96;

/// `SplitMix64`: tiny deterministic generator for tests.
pub struct Rng(pub u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    pub fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn below(&mut self, n: u32) -> u32 {
        (self.next_u64() % u64::from(n)) as u32
    }

    /// Standard normal (Box–Muller).
    pub fn gauss(&mut self) -> f64 {
        let u = self.f64().max(1e-12);
        let v = self.f64();
        (-2.0 * u.ln()).sqrt() * (2.0 * std::f64::consts::PI * v).cos()
    }
}

/// A frame plus the order it was assembled with.
pub struct Scene {
    pub profile: Profile,
    pub spec: FrameSpec,
    pub order: DotOrder,
}

pub fn disc_order(profile: Profile) -> DotOrder {
    let lattice = Lattice::new(profile);
    let f = profile.field_half();
    let mask: Vec<bool> = lattice.points().iter().map(|p| p.at.norm() <= f).collect();
    DotOrder::new(&lattice, &mask)
}

/// Masks other than the disc: a rounded square, a heavy T, a thin plus sign and a ring.
#[derive(Clone, Copy, Debug)]
pub enum Shape {
    RoundedSquare,
    T,
    Plus,
    Ring,
}

pub fn shape_order(profile: Profile, shape: Shape) -> DotOrder {
    let lattice = Lattice::new(profile);
    let f = profile.field_half();
    let mask: Vec<bool> = lattice
        .points()
        .iter()
        .map(|p| {
            let (x, y) = (p.at.x, p.at.y);
            match shape {
                Shape::RoundedSquare => {
                    // Square of half-side 0.97 F with corners rounded by 0.3 F.
                    let r = 0.3 * f;
                    let (qx, qy) = (
                        (x.abs() - (0.97 * f - r)).max(0.0),
                        (y.abs() - (0.97 * f - r)).max(0.0),
                    );
                    qx.hypot(qy) <= r
                }
                Shape::T => {
                    (y < -0.45 * f && y > -0.97 * f && x.abs() < 0.97 * f)
                        || (x.abs() < 0.2 * f && y >= -0.45 * f && y < 0.97 * f)
                }
                Shape::Plus => {
                    (x.abs() < 2.6 && y.abs() < 0.97 * f) || (y.abs() < 2.6 && x.abs() < 0.97 * f)
                }
                Shape::Ring => x.hypot(y) <= f && x.hypot(y) >= 0.55 * f,
            }
        })
        .collect();
    DotOrder::new(&lattice, &mask)
}

pub fn header_for(profile: Profile, rng: &mut Rng) -> Header {
    Header {
        shape_table_version: 0,
        profile,
        palette: Palette::Hues4,
        is_static: false,
        shape_id: 0,
        session_id: rng.below(65536) as u16,
        payload_len: 6000,
        segment_kb_log2: 3,
        segment_index: 0,
        packet_seed: rng.below(4096) as u16,
    }
}

/// Random symbols everywhere, random particle sectors.
pub fn scene(profile: Profile, seed: u64) -> Scene {
    scene_with_order(profile, disc_order(profile), seed)
}

pub fn scene_with_order(profile: Profile, order: DotOrder, seed: u64) -> Scene {
    let mut rng = Rng::new(seed);
    let header = header_for(profile, &mut rng);
    let header_syms: Vec<u8> = (0..48).map(|_| rng.below(8) as u8).collect();
    let body: Vec<u8> = (0..order.len()).map(|_| rng.below(8) as u8).collect();
    let mut sectors = [0u8; 6];
    for s in &mut sectors {
        *s = rng.below(16) as u8;
    }
    let spec = FrameSpec::assemble(0, header, &header_syms, &order, &body, sectors);
    Scene {
        profile,
        spec,
        order,
    }
}

/// How the synthetic camera sees the frame.
#[derive(Clone, Debug)]
pub struct Degrade {
    /// Pixels per dot pitch at the frame center.
    pub ppp: f64,
    /// Gaussian blur σ in output pixels.
    pub blur: f64,
    /// Gaussian noise σ per RGB channel, 8-bit units.
    pub noise: f64,
    /// Tilt about the horizontal axis, degrees (perspective).
    pub tilt_deg: f64,
    /// In-plane rotation, degrees.
    pub rot_deg: f64,
    /// Brightness gain.
    pub gain: f64,
    /// Amplitude (pitch units) of the coherent breathing wave: dots drift along a travelling
    /// sine and their radius pulses by +-5 % with it, as in the animation spec.
    pub drift: f64,
    /// Independent random per-dot displacement bound, pitch units (harsher than the wave).
    pub jitter: f64,
    /// Extra background border around the frame, pitch units.
    pub border: f64,
    /// Mirror the frame horizontally (front camera).
    pub mirror: bool,
    /// Linear motion blur: length in pixels along x and y.
    pub motion: (f64, f64),
    /// Blur at the bottom of the image when it differs from `blur` at the top (depth of
    /// field under tilt); 0 = uniform.
    pub blur_far: f64,
    /// Linear illumination gradient: gain `1 + shade * t` with `t` in -1..1 across the image.
    pub shade: f64,
    /// Vignetting: gain `1 - vignette * r^2` with `r` = 1 at the image corners.
    pub vignette: f64,
    /// Radial lens distortion: a point at normalised radius `r` (1 at the image edge) is
    /// displaced to `r (1 + barrel r^2)`; positive bows lines outwards.
    pub barrel: f64,
    /// Per-channel gain (a colour cast from warm or cold light, or a night-light filter).
    pub tint: [f64; 3],
    /// Specular glare: `(x, y, sigma, strength)` with the center in frame pitch units, the
    /// width in pitches and the peak as a fraction of full white added to every channel.
    pub glare: Option<(f64, f64, f64, f64)>,
    /// Seed for noise and drift.
    pub seed: u64,
}

impl Default for Degrade {
    fn default() -> Self {
        Degrade {
            ppp: 7.0,
            blur: 0.0,
            noise: 0.0,
            tilt_deg: 0.0,
            rot_deg: 0.0,
            gain: 1.0,
            drift: 0.0,
            jitter: 0.0,
            border: 0.0,
            mirror: false,
            motion: (0.0, 0.0),
            blur_far: 0.0,
            shade: 0.0,
            vignette: 0.0,
            barrel: 0.0,
            tint: [1.0; 3],
            glare: None,
            seed: 1,
        }
    }
}

pub struct Rendered {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
    /// Ground-truth homography: frame pitch units to image pixels (centers at integers).
    pub truth: Homography,
}

impl Rendered {
    pub fn yuv(&self) -> YuvFrame {
        YuvFrame::from_rgb(self.width, self.height, &self.rgb)
    }

    pub fn yuv420(&self) -> YuvFrame {
        self.yuv().subsample_chroma()
    }
}

fn rgb_f(c: Rgb) -> [f32; 3] {
    [f32::from(c.0), f32::from(c.1), f32::from(c.2)]
}

/// Antialiased coverage of a disc edge at distance `d` from the center.
#[inline]
fn cov(r: f64, d: f64) -> f32 {
    (r - d + 0.5).clamp(0.0, 1.0) as f32
}

struct Base {
    w: usize,
    h: usize,
    s: f64,
    data: Vec<f32>,
}

impl Base {
    fn new(half_extent: f64, s: f64) -> Base {
        let n = (2.0 * half_extent * s).ceil() as usize;
        let mut data = vec![0.0f32; n * n * 3];
        let bg = rgb_f(colors::BACKGROUND);
        for px in data.chunks_exact_mut(3) {
            px.copy_from_slice(&bg);
        }
        Base {
            w: n,
            h: n,
            s,
            data,
        }
    }

    /// Frame coordinate to base pixel coordinate.
    fn to_px(&self, x: f64, y: f64) -> (f64, f64) {
        (
            x * self.s + (self.w as f64 - 1.0) / 2.0,
            y * self.s + (self.h as f64 - 1.0) / 2.0,
        )
    }

    fn paint(&mut self, cx: f64, cy: f64, r_out: f64, r_in: f64, color: Rgb) {
        let (px, py) = self.to_px(cx, cy);
        let (ro, ri) = (r_out * self.s, r_in * self.s);
        let c = rgb_f(color);
        let x0 = ((px - ro - 1.0).floor().max(0.0)) as usize;
        let x1 = ((px + ro + 1.0).ceil().min(self.w as f64 - 1.0)) as usize;
        let y0 = ((py - ro - 1.0).floor().max(0.0)) as usize;
        let y1 = ((py + ro + 1.0).ceil().min(self.h as f64 - 1.0)) as usize;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let d = (x as f64 - px).hypot(y as f64 - py);
                let mut a = cov(ro, d);
                if ri > 0.0 {
                    a *= 1.0 - cov(ri, d);
                }
                if a > 0.0 {
                    let o = (y * self.w + x) * 3;
                    for (dst, &src) in self.data[o..o + 3].iter_mut().zip(&c) {
                        *dst += (src - *dst) * a;
                    }
                }
            }
        }
    }

    fn bilinear(&self, x: f64, y: f64) -> [f32; 3] {
        let bg = rgb_f(colors::BACKGROUND);
        if x < 0.0 || y < 0.0 || x > self.w as f64 - 1.0 || y > self.h as f64 - 1.0 {
            return bg;
        }
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (fx, fy) = ((x - x0 as f64) as f32, (y - y0 as f64) as f32);
        let x1 = (x0 + 1).min(self.w - 1);
        let y1 = (y0 + 1).min(self.h - 1);
        let g = |x: usize, y: usize, k: usize| self.data[(y * self.w + x) * 3 + k];
        let mut out = [0.0; 3];
        for (k, o) in out.iter_mut().enumerate() {
            let top = g(x0, y0, k) * (1.0 - fx) + g(x1, y0, k) * fx;
            let bot = g(x0, y1, k) * (1.0 - fx) + g(x1, y1, k) * fx;
            *o = top * (1.0 - fy) + bot * fy;
        }
        out
    }
}

fn swatch_rgb(c: SwatchColor) -> Rgb {
    match c {
        SwatchColor::Hue(k) => colors::HUES4[usize::from(k)],
        SwatchColor::White => colors::WHITE,
        SwatchColor::Black => colors::BLACK,
    }
}

/// Rasterise the flat frame at `s` pixels per pitch.
fn raster_flat(scene: &Scene, d: &Degrade, s: f64) -> Base {
    let spec = &scene.spec;
    let half = spec.frame_half_side + dims::QUIET_ZONE + 1.0 + d.border;
    let mut base = Base::new(half * 1.45, s); // room for rotation/perspective sampling
    let mut rng = Rng::new(d.seed ^ 0xA5A5);
    for a in &spec.anchors {
        base.paint(a.at.x, a.at.y, a.ring_out, a.ring_in, colors::ANCHOR);
        base.paint(a.at.x, a.at.y, a.core_r, 0.0, colors::ANCHOR);
        if a.pupil_r > 0.0 {
            base.paint(a.at.x, a.at.y, a.pupil_r, 0.0, colors::BACKGROUND);
        }
    }
    for sw in &spec.swatches {
        base.paint(
            sw.at.x,
            sw.at.y,
            dims::SWATCH_DOT_R,
            0.0,
            swatch_rgb(sw.color),
        );
    }
    let lambda = 0.6 * spec.frame_half_side;
    let wave_dir = rng.f64() * std::f64::consts::TAU;
    let drift_dir = rng.f64() * std::f64::consts::TAU;
    let phase = rng.f64() * std::f64::consts::TAU;
    let mut draw_dot = |base: &mut Base, dot: &particlecode_core::frame::Dot| {
        let (mut x, mut y) = (dot.at.x, dot.at.y);
        let mut radius = dot.radius();
        if d.drift > 0.0 {
            let w = (std::f64::consts::TAU * (x * wave_dir.cos() + y * wave_dir.sin()) / lambda
                + phase)
                .sin();
            x += d.drift * w * drift_dir.cos();
            y += d.drift * w * drift_dir.sin();
            radius *= 1.0 + 0.05 * w;
        }
        if d.jitter > 0.0 {
            let a = rng.f64() * std::f64::consts::TAU;
            let m = d.jitter * rng.f64().sqrt();
            x += m * a.cos();
            y += m * a.sin();
        }
        base.paint(x, y, radius, 0.0, colors::HUES4[usize::from(dot.color)]);
    };
    for dot in &spec.header_dots {
        draw_dot(&mut base, dot);
    }
    for dot in &spec.dots {
        draw_dot(&mut base, dot);
    }
    let mid = spec.particle_band.mid();
    for p in &spec.particles {
        base.paint(
            mid * p.angle.cos(),
            mid * p.angle.sin(),
            dims::PARTICLE_R,
            0.0,
            colors::PARTICLE,
        );
    }
    base
}

/// The ground-truth map from frame pitch units to output pixels.
fn truth_homography(d: &Degrade, w: usize, h: usize) -> Homography {
    let rot = d.rot_deg.to_radians();
    let tilt = d.tilt_deg.to_radians();
    let z0 = 1000.0 / d.ppp; // focal length 1000 px, expressed in pitches
    let mirror = if d.mirror { -1.0 } else { 1.0 };
    let r = Homography([
        [rot.cos() * mirror, -rot.sin(), 0.0],
        [rot.sin() * mirror, rot.cos(), 0.0],
        [0.0, 0.0, 1.0],
    ]);
    let t = Homography([
        [d.ppp, 0.0, 0.0],
        [0.0, d.ppp * tilt.cos(), 0.0],
        [0.0, tilt.sin() / z0, 1.0],
    ]);
    let c = Homography([
        [1.0, 0.0, (w as f64 - 1.0) / 2.0],
        [0.0, 1.0, (h as f64 - 1.0) / 2.0],
        [0.0, 0.0, 1.0],
    ]);
    c.then_after(&t).then_after(&r)
}

fn canvas_size(scene: &Scene, d: &Degrade) -> usize {
    let half = scene.spec.frame_half_side + dims::QUIET_ZONE + 1.0 + d.border;
    let tilt_pad = 1.0 + 0.35 * d.tilt_deg.to_radians().sin().abs();
    let rot_pad = if d.rot_deg.rem_euclid(90.0).abs() < 1e-9 {
        1.0
    } else {
        1.42
    };
    (2.0 * half * d.ppp * tilt_pad * rot_pad).ceil() as usize + 8
}

pub fn render(scene: &Scene, d: &Degrade) -> Rendered {
    let ss = if scene.profile == Profile::L {
        2.0
    } else {
        3.0
    };
    let base = raster_flat(scene, d, d.ppp * ss);
    let size = canvas_size(scene, d);
    let (w, h) = (size, size);
    let truth = truth_homography(d, w, h);
    let inv = truth.inverse().expect("invertible");
    let mut img = vec![0.0f32; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0.0f32; 3];
            for (ox, oy) in [(-0.25, -0.25), (0.25, -0.25), (-0.25, 0.25), (0.25, 0.25)] {
                let (ux, uy) = undistort(d.barrel, w, (x as f64 + ox, y as f64 + oy));
                let (fx, fy) = inv.apply((ux, uy)).expect("finite");
                let (bx, by) = base.to_px(fx, fy);
                let c = base.bilinear(bx, by);
                for k in 0..3 {
                    acc[k] += c[k] * 0.25;
                }
            }
            img[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&acc);
        }
    }
    if d.blur_far > 0.0 {
        let mut far = img.clone();
        gaussian_blur(&mut far, w, h, d.blur_far);
        if d.blur > 0.0 {
            gaussian_blur(&mut img, w, h, d.blur);
        }
        for y in 0..h {
            let t = y as f32 / h as f32;
            for i in y * w * 3..(y + 1) * w * 3 {
                img[i] = img[i] * (1.0 - t) + far[i] * t;
            }
        }
    } else if d.blur > 0.0 {
        gaussian_blur(&mut img, w, h, d.blur);
    }
    if d.motion != (0.0, 0.0) {
        motion_blur(&mut img, w, h, d.motion);
    }
    let mut rng = Rng::new(d.seed);
    let rgb = img
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let mut v = f64::from(v) * d.gain * d.tint[i % 3];
            if d.shade != 0.0 || d.vignette != 0.0 {
                let (px, py) = ((i / 3) % w, (i / 3) / w);
                let (u, vv) = (
                    (px as f64 / w as f64) * 2.0 - 1.0,
                    (py as f64 / h as f64) * 2.0 - 1.0,
                );
                let r2 = f64::midpoint(u * u, vv * vv);
                v *= 1.0 + d.shade * (0.8 * u + 0.6 * vv) - d.vignette * r2;
            }
            if let Some((gx, gy, sigma, strength)) = d.glare {
                let px = (i / 3) % w;
                let py = (i / 3) / w;
                let (fx, fy) = inv.apply((px as f64, py as f64)).expect("finite");
                let r2 = (fx - gx).powi(2) + (fy - gy).powi(2);
                v += 255.0 * strength * (-r2 / (2.0 * sigma * sigma)).exp();
            }
            if d.noise > 0.0 {
                v += d.noise * rng.gauss();
            }
            v.round().clamp(0.0, 255.0) as u8
        })
        .collect();
    Rendered {
        width: w,
        height: h,
        rgb,
        truth,
    }
}

/// Invert the radial distortion `p_d = c + (p_u - c)(1 + k r_u^2)` by fixed-point iteration.
fn undistort(k: f64, size: usize, p: (f64, f64)) -> (f64, f64) {
    if k == 0.0 {
        return p;
    }
    let c = (size as f64 - 1.0) / 2.0;
    let half = size as f64 / 2.0;
    let (dx, dy) = ((p.0 - c) / half, (p.1 - c) / half);
    let (mut ux, mut uy) = (dx, dy);
    for _ in 0..8 {
        let r2 = ux * ux + uy * uy;
        ux = dx / (1.0 + k * r2);
        uy = dy / (1.0 + k * r2);
    }
    (c + ux * half, c + uy * half)
}

/// Box blur along the vector `(dx, dy)` (pixels), centered.
fn motion_blur(img: &mut [f32], w: usize, h: usize, v: (f64, f64)) {
    let len = v.0.hypot(v.1);
    let steps = (len * 2.0).ceil().max(1.0) as usize + 1;
    let src = img.to_vec();
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0.0f32; 3];
            for k in 0..steps {
                let t = k as f64 / (steps - 1) as f64 - 0.5;
                let (sx, sy) = (x as f64 + t * v.0, y as f64 + t * v.1);
                let xi = sx.round().clamp(0.0, w as f64 - 1.0) as usize;
                let yi = sy.round().clamp(0.0, h as f64 - 1.0) as usize;
                for c in 0..3 {
                    acc[c] += src[(yi * w + xi) * 3 + c];
                }
            }
            for c in 0..3 {
                img[(y * w + x) * 3 + c] = acc[c] / steps as f32;
            }
        }
    }
}

pub fn gaussian_blur(img: &mut [f32], w: usize, h: usize, sigma: f64) {
    let rad = (3.0 * sigma).ceil() as i64;
    let mut k: Vec<f32> = (-rad..=rad)
        .map(|i| (-(i * i) as f64 / (2.0 * sigma * sigma)).exp() as f32)
        .collect();
    let sum: f32 = k.iter().sum();
    for v in &mut k {
        *v /= sum;
    }
    let mut tmp = vec![0.0f32; img.len()];
    for y in 0..h {
        for x in 0..w {
            for c in 0..3 {
                let mut a = 0.0;
                for (j, kv) in k.iter().enumerate() {
                    let xx = (x as i64 + j as i64 - rad).clamp(0, w as i64 - 1) as usize;
                    a += kv * img[(y * w + xx) * 3 + c];
                }
                tmp[(y * w + x) * 3 + c] = a;
            }
        }
    }
    for y in 0..h {
        for x in 0..w {
            for c in 0..3 {
                let mut a = 0.0;
                for (j, kv) in k.iter().enumerate() {
                    let yy = (y as i64 + j as i64 - rad).clamp(0, h as i64 - 1) as usize;
                    a += kv * tmp[(yy * w + x) * 3 + c];
                }
                img[(y * w + x) * 3 + c] = a;
            }
        }
    }
}

/// A busy image with no code: text-like bars, discs, rings, ring-and-core decoys and noise.
pub fn busy_image(w: usize, h: usize, seed: u64, with_decoy_triple: bool) -> Rendered {
    let mut rng = Rng::new(seed);
    let mut base = Base {
        w,
        h,
        s: 1.0,
        data: vec![0.0; w * h * 3],
    };
    for y in 0..h {
        for x in 0..w {
            let v = 200.0 + 40.0 * ((x as f64 / 37.0).sin() * (y as f64 / 53.0).cos()) as f32;
            base.data[(y * w + x) * 3..(y * w + x) * 3 + 3].fill(v);
        }
    }
    let palette = [
        colors::ANCHOR,
        colors::HUES4[0],
        colors::HUES4[1],
        colors::HUES4[2],
        colors::HUES4[3],
        colors::PARTICLE,
        colors::BLACK,
    ];
    // Mark-like decoys, to the pixel.
    let mark = |b: &mut Base, cx: f64, cy: f64, r: f64| {
        b.paint(cx, cy, r, r * 0.6, colors::ANCHOR);
        b.paint(cx, cy, r * 0.33, 0.0, colors::ANCHOR);
    };
    for _ in 0..120 {
        let (cx, cy) = (rng.f64() * w as f64, rng.f64() * h as f64);
        let r = 2.0 + rng.f64() * 14.0;
        let c = palette[rng.below(palette.len() as u32) as usize];
        match rng.below(4) {
            0 => base.paint(cx, cy, r, 0.0, c),
            1 => base.paint(cx, cy, r, r * 0.6, c),
            2 => mark(&mut base, cx, cy, r),
            _ => {
                for i in 0..8 {
                    base.paint(cx + f64::from(i) * 3.0, cy, 1.5, 0.0, c);
                }
            }
        }
    }
    if with_decoy_triple {
        let (cx, cy, side) = (w as f64 / 2.0, h as f64 / 2.0, h as f64 * 0.4);
        for i in 0..3 {
            let a = (-90.0 + 120.0 * f64::from(i)).to_radians();
            mark(
                &mut base,
                cx + side / 3f64.sqrt() * a.cos(),
                cy + side / 3f64.sqrt() * a.sin(),
                13.0,
            );
        }
        mark(&mut base, cx, cy, 5.0);
    }
    let rgb = base
        .data
        .iter()
        .map(|&v| (f64::from(v) + 3.0 * rng.gauss()).round().clamp(0.0, 255.0) as u8)
        .collect();
    Rendered {
        width: w,
        height: h,
        rgb,
        truth: Homography::IDENTITY,
    }
}

/// Fraction of mismatching symbols (value only), restricted to `confident >= min_conf`.
pub fn symbol_errors(truth: &[particlecode_core::Sym3], read: &[particlecode_core::Sym3]) -> usize {
    assert_eq!(truth.len(), read.len());
    truth
        .iter()
        .zip(read)
        .filter(|(a, b)| a.value != b.value)
        .count()
}

/// Paste a rendered frame into a larger canvas of a flat color with its top-left corner at
/// `(dx, dy)`, moving the ground truth along. Models a camera frame in which the code
/// fills only part of the picture.
pub fn paste(r: &Rendered, w: usize, h: usize, fill: [u8; 3], dx: usize, dy: usize) -> Rendered {
    let mut rgb = Vec::with_capacity(w * h * 3);
    for _ in 0..w * h {
        rgb.extend_from_slice(&fill);
    }
    for y in 0..r.height.min(h.saturating_sub(dy)) {
        for x in 0..r.width.min(w.saturating_sub(dx)) {
            let s = (y * r.width + x) * 3;
            let d = ((y + dy) * w + x + dx) * 3;
            rgb[d..d + 3].copy_from_slice(&r.rgb[s..s + 3]);
        }
    }
    let shift = Homography([
        [1.0, 0.0, dx as f64],
        [0.0, 1.0, dy as f64],
        [0.0, 0.0, 1.0],
    ]);
    Rendered {
        width: w,
        height: h,
        rgb,
        truth: shift.then_after(&r.truth),
    }
}

/// A lock built from the ground truth, to test the reader independently of `locate`.
pub fn lock_from_truth(profile: Profile, truth: &Homography) -> particlecode_vision::Lock {
    let a = layout::hero_anchors(profile);
    let px = |p: layout::Point| truth.apply((p.x, p.y)).expect("finite");
    particlecode_vision::Lock {
        profile,
        homography: *truth,
        anchors_px: [px(a[0]), px(a[1]), px(a[2])],
        center_px: Some(px(layout::Point::new(0.0, 0.0))),
        pitch_px: truth.scale_at((0.0, 0.0)).expect("scale"),
        score: 1.0,
    }
}

/// Largest distance (pitch units) between the lock's hero anchors and the true ones.
pub fn anchors_error_pitch(lock: &particlecode_vision::Lock, truth: &Homography) -> f64 {
    let a = layout::hero_anchors(lock.profile);
    (0..3)
        .map(|i| {
            let t = truth.apply((a[i].x, a[i].y)).expect("finite");
            let s = truth.scale_at((a[i].x, a[i].y)).expect("scale");
            (lock.anchors_px[i].0 - t.0).hypot(lock.anchors_px[i].1 - t.1) / s
        })
        .fold(0.0, f64::max)
}

/// Pitch-unit position error of a lock's homography against ground truth, evaluated at
/// the hero anchors and the centre.
pub fn homography_error_pitch(lock: &Homography, truth: &Homography, profile: Profile) -> f64 {
    let mut worst: f64 = 0.0;
    let mut pts: Vec<(f64, f64)> = layout::hero_anchors(profile)
        .iter()
        .map(|p| (p.x, p.y))
        .collect();
    pts.push((0.0, 0.0));
    let f = profile.field_half();
    pts.extend([
        (f * 0.8, 0.0),
        (-f * 0.8, 0.0),
        (0.0, f * 0.8),
        (0.0, -f * 0.8),
    ]);
    for p in pts {
        let (a, b) = (
            lock.apply(p).expect("finite"),
            truth.apply(p).expect("finite"),
        );
        let scale = truth.scale_at(p).expect("scale");
        let e = (a.0 - b.0).hypot(a.1 - b.1) / scale;
        worst = worst.max(e);
    }
    worst
}

/// Mean and standard deviation.
pub fn mean_sd(v: &[f64]) -> (f64, f64) {
    if v.is_empty() {
        return (0.0, 0.0);
    }
    let m = v.iter().sum::<f64>() / v.len() as f64;
    (
        m,
        (v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / v.len() as f64).sqrt(),
    )
}

/// Result of rendering a scene, locating it and reading it back.
pub struct Outcome {
    pub header_err: usize,
    pub body_err: usize,
    pub hue_err: usize,
    pub size_err: usize,
    pub body_n: usize,
    /// Wrong symbols the reader was confident about (confidence >= 128).
    pub confident_errors: usize,
    /// Symbols the reader marked as erasures (confidence below [`ERASURE_BELOW`]).
    pub erasures: usize,
    /// Wrong symbols that were *not* marked as erasures.
    pub unmarked_errors: usize,
    pub sectors_ok: Option<bool>,
    pub residual: f64,
    pub herr: f64,
    pub lerr: f64,
    pub ms: f64,
    pub small: (f64, f64),
    pub large: (f64, f64),
    pub thr: f64,
}

impl Outcome {
    pub fn ser(&self) -> f64 {
        self.body_err as f64 / self.body_n as f64
    }
}

/// Render, locate (or use the ground-truth lock with `PC_ORACLE=1`), read, and compare.
pub fn run(sc: &Scene, d: &Degrade, chroma420: bool) -> Result<Outcome, String> {
    let img = render(sc, d);
    let yuv = if chroma420 { img.yuv420() } else { img.yuv() };
    run_image(sc, &img, &yuv)
}

/// Like [`run`] on an already rendered image.
pub fn run_image(sc: &Scene, img: &Rendered, yuv: &YuvFrame) -> Result<Outcome, String> {
    let t = std::time::Instant::now();
    let lock = if std::env::var("PC_ORACLE").is_ok() {
        lock_from_truth(sc.profile, &img.truth)
    } else {
        locate(&yuv.y).ok_or("no lock")?
    };
    let raw = read_with_order(yuv, &lock, &sc.order).map_err(|e| e.to_string())?;
    let ms = t.elapsed().as_secs_f64() * 1e3;
    let truth_h = sc.spec.header_symbols();
    let truth_b = sc.spec.body_symbols();
    let want: Vec<u8> = sc.spec.particles.iter().map(|p| p.sector).collect();
    let (mut hue_err, mut size_err, mut confident, mut erasures, mut unmarked) = (0, 0, 0, 0, 0);
    let (mut small, mut large) = (Vec::new(), Vec::new());
    for ((t, r), ratio) in truth_b.iter().zip(&raw.body_symbols).zip(&raw.size_scores) {
        hue_err += usize::from(t.color() != r.color());
        size_err += usize::from(t.size() != r.size());
        let wrong = t.value != r.value;
        confident += usize::from(wrong && r.confidence >= 128);
        erasures += usize::from(r.confidence < ERASURE_BELOW);
        unmarked += usize::from(wrong && r.confidence >= ERASURE_BELOW);
        if t.size() == 0 {
            small.push(f64::from(*ratio));
        } else {
            large.push(f64::from(*ratio));
        }
    }
    Ok(Outcome {
        header_err: symbol_errors(&truth_h, &raw.header_symbols),
        body_err: symbol_errors(&truth_b, &raw.body_symbols),
        hue_err,
        size_err,
        body_n: truth_b.len(),
        confident_errors: confident,
        erasures,
        unmarked_errors: unmarked,
        sectors_ok: raw.liveness_sectors.map(|s| s.to_vec() == want),
        residual: raw.residual_pitch,
        herr: homography_error_pitch(&raw.homography, &img.truth, sc.profile),
        lerr: homography_error_pitch(&lock.homography, &img.truth, sc.profile),
        ms,
        small: mean_sd(&small),
        large: mean_sd(&large),
        thr: raw.size_separation,
    })
}

/// Aggregate over seeds: `(locks, header errors, body errors, body symbols, outcomes)`.
pub struct Batch {
    pub tried: usize,
    pub locks: usize,
    pub header_err: usize,
    pub body_err: usize,
    pub body_n: usize,
    pub outcomes: Vec<Outcome>,
}

impl Batch {
    pub fn ser(&self) -> f64 {
        if self.body_n == 0 {
            1.0
        } else {
            self.body_err as f64 / self.body_n as f64
        }
    }
}

/// Run `seeds` scenes of `profile` through the same degradation.
pub fn batch(profile: Profile, d: &Degrade, chroma420: bool, seeds: &[u64]) -> Batch {
    let mut b = Batch {
        tried: seeds.len(),
        locks: 0,
        header_err: 0,
        body_err: 0,
        body_n: 0,
        outcomes: Vec::new(),
    };
    for &seed in seeds {
        let sc = scene(profile, seed);
        if let Ok(o) = run(&sc, &Degrade { seed, ..d.clone() }, chroma420) {
            b.locks += 1;
            b.header_err += o.header_err;
            b.body_err += o.body_err;
            b.body_n += o.body_n;
            b.outcomes.push(o);
        }
    }
    b
}
