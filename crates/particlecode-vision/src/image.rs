//! Image containers: 8-bit planes and YCbCr frames, with bilinear sampling.
//!
//! Pixel coordinates put the *center* of pixel `(i, j)` at `(i, j)`. Chroma planes may be
//! subsampled (4:2:0) or full resolution; the ratio is inferred from the plane sizes and
//! chroma samples are taken at the centers of their cells.

/// An owned 8-bit image plane.
///
/// `stride` is the distance in bytes between the starts of consecutive rows
/// (`stride >= width`). Invalid planes (too little data, zero size) are tolerated by every
/// entry point of this crate: they simply fail to lock.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plane {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// Row stride in bytes.
    pub stride: usize,
    /// Pixel data, row-major.
    pub data: Vec<u8>,
}

impl Plane {
    /// A tightly packed plane (`stride == width`).
    pub fn new(width: usize, height: usize, data: Vec<u8>) -> Plane {
        Plane {
            width,
            height,
            stride: width,
            data,
        }
    }

    /// A plane with an explicit row stride.
    pub fn with_stride(width: usize, height: usize, stride: usize, data: Vec<u8>) -> Plane {
        Plane {
            width,
            height,
            stride,
            data,
        }
    }

    /// A plane filled with one value.
    pub fn filled(width: usize, height: usize, value: u8) -> Plane {
        Plane::new(width, height, vec![value; width * height])
    }

    /// Whether `data` really covers `height` rows of `width` pixels at `stride`.
    pub fn is_valid(&self) -> bool {
        self.view().is_some()
    }

    /// Borrow as a validated view, or `None` if the plane is inconsistent.
    pub fn view(&self) -> Option<PlaneRef<'_>> {
        PlaneRef::new(self.width, self.height, self.stride, &self.data)
    }

    /// Pixel at `(x, y)`, or `None` outside the image.
    pub fn get(&self, x: usize, y: usize) -> Option<u8> {
        self.view().and_then(|v| v.get(x, y))
    }
}

/// A borrowed, validated 8-bit plane. Indexing inside `width × height` cannot go out of
/// bounds because the constructor checked the buffer size.
#[derive(Debug, Clone, Copy)]
pub struct PlaneRef<'a> {
    width: usize,
    height: usize,
    stride: usize,
    data: &'a [u8],
}

impl<'a> PlaneRef<'a> {
    /// Validate and wrap a buffer. `None` if the buffer is too short or the size is zero.
    pub fn new(width: usize, height: usize, stride: usize, data: &'a [u8]) -> Option<Self> {
        if width == 0 || height == 0 || stride < width {
            return None;
        }
        let needed = (height - 1).checked_mul(stride)?.checked_add(width)?;
        (data.len() >= needed).then_some(PlaneRef {
            width,
            height,
            stride,
            data,
        })
    }

    /// Width in pixels.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> usize {
        self.height
    }

    /// Row stride in bytes.
    pub fn stride(&self) -> usize {
        self.stride
    }

    /// Pixel at `(x, y)`, or `None` outside the image.
    pub fn get(&self, x: usize, y: usize) -> Option<u8> {
        (x < self.width && y < self.height).then(|| self.data[y * self.stride + x])
    }

    /// Row `y` (`width` bytes). `y` must be below `height`.
    pub(crate) fn row(&self, y: usize) -> &'a [u8] {
        &self.data[y * self.stride..y * self.stride + self.width]
    }

    /// Pixel with clamped coordinates.
    #[inline]
    pub(crate) fn at(&self, x: i64, y: i64) -> f32 {
        let xi = x.clamp(0, self.width as i64 - 1) as usize;
        let yi = y.clamp(0, self.height as i64 - 1) as usize;
        f32::from(self.data[yi * self.stride + xi])
    }

    /// Whether the continuous point `(x, y)` lies inside the pixel centers' hull
    /// shrunk by `margin` pixels.
    #[inline]
    pub(crate) fn contains(&self, x: f64, y: f64, margin: f64) -> bool {
        x >= margin
            && y >= margin
            && x <= self.width as f64 - 1.0 - margin
            && y <= self.height as f64 - 1.0 - margin
    }

    /// Bilinear sample at continuous coordinates, borders clamped.
    #[inline]
    pub(crate) fn bilinear(&self, x: f64, y: f64) -> f32 {
        let x = x.clamp(0.0, self.width as f64 - 1.0);
        let y = y.clamp(0.0, self.height as f64 - 1.0);
        let x0 = x.floor();
        let y0 = y.floor();
        let fx = (x - x0) as f32;
        let fy = (y - y0) as f32;
        let (xi, yi) = (x0 as usize, y0 as usize);
        let x1 = (xi + 1).min(self.width - 1);
        let y1 = (yi + 1).min(self.height - 1);
        let p = |x: usize, y: usize| f32::from(self.data[y * self.stride + x]);
        let top = p(xi, yi) * (1.0 - fx) + p(x1, yi) * fx;
        let bottom = p(xi, y1) * (1.0 - fx) + p(x1, y1) * fx;
        top * (1.0 - fy) + bottom * fy
    }

    /// 2 × 2 box-filtered copy (odd trailing row/column dropped).
    pub(crate) fn downscale2(&self) -> Option<(usize, usize, Vec<u8>)> {
        let (w, h) = (self.width / 2, self.height / 2);
        if w == 0 || h == 0 {
            return None;
        }
        let mut out = Vec::with_capacity(w * h);
        for y in 0..h {
            let a = self.row(2 * y);
            let b = self.row(2 * y + 1);
            out.extend((0..w).map(|x| {
                let s = u32::from(a[2 * x])
                    + u32::from(a[2 * x + 1])
                    + u32::from(b[2 * x])
                    + u32::from(b[2 * x + 1]);
                ((s + 2) / 4) as u8
            }));
        }
        Some((w, h, out))
    }
}

impl<'a> From<&'a Plane> for PlaneRef<'a> {
    /// Infallible conversion: an inconsistent plane becomes an empty (1 × 1) view that no
    /// algorithm will lock onto.
    fn from(p: &'a Plane) -> Self {
        p.view().unwrap_or(PlaneRef {
            width: 1,
            height: 1,
            stride: 1,
            data: &[0],
        })
    }
}

/// An owned YCbCr frame (BT.601, full range). `u` is Cb, `v` is Cr; they may be half
/// resolution in each dimension (4:2:0) or full resolution.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct YuvFrame {
    /// Luma.
    pub y: Plane,
    /// Cb (blue difference).
    pub u: Plane,
    /// Cr (red difference).
    pub v: Plane,
}

impl YuvFrame {
    /// Convert tightly packed 8-bit RGB (`3 * width * height` bytes) with the BT.601
    /// full-range matrix. Chroma is kept at full resolution. A short buffer yields an
    /// empty frame.
    pub fn from_rgb(width: usize, height: usize, rgb: &[u8]) -> YuvFrame {
        let n = width * height;
        if n == 0 || rgb.len() < 3 * n {
            return YuvFrame::default();
        }
        let mut y = Vec::with_capacity(n);
        let mut u = Vec::with_capacity(n);
        let mut v = Vec::with_capacity(n);
        for px in rgb[..3 * n].chunks_exact(3) {
            let (r, g, b) = (f32::from(px[0]), f32::from(px[1]), f32::from(px[2]));
            y.push(to_u8(0.299 * r + 0.587 * g + 0.114 * b));
            u.push(to_u8(128.0 - 0.168_736 * r - 0.331_264 * g + 0.5 * b));
            v.push(to_u8(128.0 + 0.5 * r - 0.418_688 * g - 0.081_312 * b));
        }
        YuvFrame {
            y: Plane::new(width, height, y),
            u: Plane::new(width, height, u),
            v: Plane::new(width, height, v),
        }
    }

    /// Average the chroma planes down to 4:2:0 (half resolution, rounded up).
    #[must_use]
    pub fn subsample_chroma(&self) -> YuvFrame {
        YuvFrame {
            y: self.y.clone(),
            u: halve(&self.u),
            v: halve(&self.v),
        }
    }

    /// Build from an NV21-style buffer pair: a luma plane and one interleaved `VU` plane
    /// at half resolution. `uv_stride` is the row stride of the interleaved plane in bytes.
    pub fn from_nv21(
        width: usize,
        height: usize,
        luma: &[u8],
        luma_stride: usize,
        vu: &[u8],
        uv_stride: usize,
    ) -> YuvFrame {
        Self::from_semiplanar(width, height, luma, luma_stride, vu, uv_stride, true)
    }

    /// Like [`YuvFrame::from_nv21`] but for NV12 (`UV` interleaving).
    pub fn from_nv12(
        width: usize,
        height: usize,
        luma: &[u8],
        luma_stride: usize,
        uv: &[u8],
        uv_stride: usize,
    ) -> YuvFrame {
        Self::from_semiplanar(width, height, luma, luma_stride, uv, uv_stride, false)
    }

    fn from_semiplanar(
        width: usize,
        height: usize,
        luma: &[u8],
        luma_stride: usize,
        chroma: &[u8],
        chroma_stride: usize,
        vu_order: bool,
    ) -> YuvFrame {
        let (cw, ch) = (width.div_ceil(2), height.div_ceil(2));
        let y = Plane::with_stride(width, height, luma_stride, luma.to_vec());
        let mut a = Vec::with_capacity(cw * ch);
        let mut b = Vec::with_capacity(cw * ch);
        for row in 0..ch {
            for col in 0..cw {
                let i = row * chroma_stride + 2 * col;
                match (chroma.get(i), chroma.get(i + 1)) {
                    (Some(&p), Some(&q)) => {
                        a.push(p);
                        b.push(q);
                    }
                    _ => return YuvFrame::default(),
                }
            }
        }
        let (u, v) = if vu_order { (b, a) } else { (a, b) };
        YuvFrame {
            y,
            u: Plane::new(cw, ch, u),
            v: Plane::new(cw, ch, v),
        }
    }
}

fn halve(p: &Plane) -> Plane {
    let Some(v) = p.view() else {
        return Plane::default();
    };
    let (w, h) = (v.width().div_ceil(2), v.height().div_ceil(2));
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let (x0, y0) = (2 * x as i64, 2 * y as i64);
            let s = v.at(x0, y0) + v.at(x0 + 1, y0) + v.at(x0, y0 + 1) + v.at(x0 + 1, y0 + 1);
            out.push(to_u8(s / 4.0));
        }
    }
    Plane::new(w, h, out)
}

fn to_u8(v: f32) -> u8 {
    (v + 0.5).clamp(0.0, 255.0) as u8
}

/// A borrowed, validated YCbCr frame.
#[derive(Debug, Clone, Copy)]
pub struct YuvRef<'a> {
    /// Luma.
    pub(crate) y: PlaneRef<'a>,
    pub(crate) u: PlaneRef<'a>,
    pub(crate) v: PlaneRef<'a>,
    /// Luma pixels per chroma pixel, horizontally and vertically.
    ratio: (f64, f64),
}

impl<'a> YuvRef<'a> {
    /// Assemble from three planes. `None` if a plane is invalid or the chroma planes are
    /// not the same size as each other and no larger than luma.
    pub fn new(y: PlaneRef<'a>, u: PlaneRef<'a>, v: PlaneRef<'a>) -> Option<Self> {
        let same = u.width() == v.width() && u.height() == v.height();
        let fits = u.width() <= y.width() && u.height() <= y.height();
        // Chroma at least a quarter of luma in each dimension keeps the ratio sane.
        let sane = u.width() * 4 >= y.width() && u.height() * 4 >= y.height();
        (same && fits && sane).then(|| YuvRef {
            y,
            u,
            v,
            ratio: (
                y.width() as f64 / u.width() as f64,
                y.height() as f64 / u.height() as f64,
            ),
        })
    }

    /// Luma view.
    pub fn luma(&self) -> PlaneRef<'a> {
        self.y
    }

    /// The same frame with another luma plane of the same size (a flattened copy).
    pub(crate) fn with_luma<'b>(&self, y: PlaneRef<'b>) -> YuvRef<'b>
    where
        'a: 'b,
    {
        YuvRef {
            y,
            u: self.u,
            v: self.v,
            ratio: self.ratio,
        }
    }

    /// Bilinear Cb, Cr at luma pixel coordinates.
    #[inline]
    pub(crate) fn chroma(&self, x: f64, y: f64) -> (f32, f32) {
        let cx = (x + 0.5) / self.ratio.0 - 0.5;
        let cy = (y + 0.5) / self.ratio.1 - 0.5;
        (self.u.bilinear(cx, cy), self.v.bilinear(cx, cy))
    }
}

impl<'a> From<&'a YuvFrame> for YuvRef<'a> {
    /// Infallible conversion: an inconsistent frame becomes an empty (1 × 1) view that no
    /// algorithm will read from.
    fn from(f: &'a YuvFrame) -> Self {
        let empty = PlaneRef::from(&EMPTY);
        let (y, u, v) = (
            PlaneRef::from(&f.y),
            PlaneRef::from(&f.u),
            PlaneRef::from(&f.v),
        );
        YuvRef::new(y, u, v).unwrap_or(YuvRef {
            y: empty,
            u: empty,
            v: empty,
            ratio: (1.0, 1.0),
        })
    }
}

static EMPTY: Plane = Plane {
    width: 0,
    height: 0,
    stride: 0,
    data: Vec::new(),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_conversion_matches_bt601() {
        // Pure white, black, red.
        let f = YuvFrame::from_rgb(3, 1, &[255, 255, 255, 0, 0, 0, 255, 0, 0]);
        assert_eq!(f.y.data, vec![255, 0, 76]);
        assert_eq!(f.u.data, vec![128, 128, 85]);
        assert_eq!(f.v.data, vec![128, 128, 255]);
    }

    #[test]
    fn plane_validation() {
        assert!(Plane::new(4, 4, vec![0; 16]).is_valid());
        assert!(!Plane::new(4, 4, vec![0; 15]).is_valid());
        assert!(!Plane::new(0, 4, vec![]).is_valid());
        assert!(!Plane::with_stride(4, 2, 3, vec![0; 20]).is_valid());
        assert!(Plane::with_stride(4, 2, 6, vec![0; 10]).is_valid());
    }

    #[test]
    fn bilinear_interpolates() {
        let p = Plane::new(2, 2, vec![0, 100, 100, 200]);
        let v = p.view().expect("valid");
        assert!((v.bilinear(0.5, 0.5) - 100.0).abs() < 1e-4);
        assert!((v.bilinear(-3.0, -3.0)).abs() < 1e-4);
        assert!((v.bilinear(9.0, 9.0) - 200.0).abs() < 1e-4);
    }

    #[test]
    fn half_resolution_chroma_is_addressable() {
        let f = YuvFrame::from_rgb(4, 4, &[200; 48]).subsample_chroma();
        let r = YuvRef::from(&f);
        let (cb, cr) = r.chroma(3.0, 3.0);
        assert!((cb - 128.0).abs() < 1.0 && (cr - 128.0).abs() < 1.0);
    }
}
