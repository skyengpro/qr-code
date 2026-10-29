//! Test-only helpers: photograph encoder frames with the synthetic camera and lay the
//! result out the way Android and iOS camera APIs hand it over.

use particlecode_core::motion::{MotionConfig, seed_from_session};
use particlecode_core::profile::Rgb;
use particlecode_render::synth::{CameraPose, Degradation, RgbImage, apply_all};
use particlecode_render::{Anim, RenderConfig, render_anim};
use particlecode_vision::YuvFrame;

use crate::api::dto::EncoderConfig;
use crate::api::encoder::ParticleEncoder;

/// An encoder with a fixed identity, so tests are deterministic.
pub(crate) fn encoder(payload: &[u8]) -> ParticleEncoder {
    let config = EncoderConfig {
        session_id: Some(0xC0DE),
        key: Some(vec![3; 32]),
        clock_origin_unix: Some(1_700_000_000),
        ..EncoderConfig::default()
    };
    ParticleEncoder::new(payload.to_vec(), config).expect("encoder")
}

/// Deterministic incompressible-looking bytes.
pub(crate) fn payload(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i * 31 % 251) as u8 ^ 0x5A).collect()
}

/// Render frame `i` mid-way through its 100 ms slot and photograph it: tilt, rotation,
/// color cast, exposure, blur and noise.
pub(crate) fn camera_rgb(enc: &ParticleEncoder, i: u32, size_px: u32) -> RgbImage {
    let frame = enc.spec(i);
    let cfg = RenderConfig {
        size_px,
        ..RenderConfig::default()
    };
    let seed = seed_from_session(enc.session_id());
    let anim = Anim::at(MotionConfig::default(), seed, f64::from(i) * 0.1 + 0.06);
    let img = RgbImage::from_pixmap(&render_anim(&frame, &cfg, &anim));
    apply_all(
        &img,
        &[
            Degradation::Tilt {
                pose: CameraPose {
                    rotate_deg: 8.0 + f64::from(i % 5) * 7.0,
                    tilt_x_deg: 12.0,
                    tilt_y_deg: -8.0,
                    scale: 0.85,
                },
                out_w: size_px * 5 / 4,
                out_h: size_px,
                background: Rgb(40, 38, 36),
            },
            Degradation::ColorCast {
                r: 1.06,
                g: 1.0,
                b: 0.9,
            },
            Degradation::Exposure {
                gain: 0.85,
                offset: 4.0,
            },
            Degradation::Blur { sigma: 0.9 },
            Degradation::Noise {
                sigma: 4.0,
                seed: u64::from(i),
            },
        ],
    )
}

/// Camera bytes in the layout of Android `YUV_420_888` / iOS.
pub(crate) struct Planes {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) y: Vec<u8>,
    /// Cb buffer.
    pub(crate) u: Vec<u8>,
    /// Cr buffer.
    pub(crate) v: Vec<u8>,
    pub(crate) y_stride: u32,
    pub(crate) uv_stride: u32,
    pub(crate) uv_pixel_stride: u32,
}

/// Pad every row of `rows` (each `row_len` bytes) to `stride` with a recognizable filler.
fn pad_rows(data: &[u8], row_len: usize, stride: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / row_len * stride);
    for row in data.chunks(row_len) {
        out.extend_from_slice(row);
        out.resize(out.len() + (stride - row_len), 0xEE);
    }
    out
}

/// NV21 as Android hands it over: one luma plane and one interleaved `VU` buffer; the
/// plugin's `V` plane is that buffer and its `U` plane the same buffer from byte 1. Rows
/// are padded by `pad` bytes.
pub(crate) fn nv21(img: &RgbImage, pad: usize) -> Planes {
    let yuv =
        YuvFrame::from_rgb(img.width as usize, img.height as usize, &img.data).subsample_chroma();
    let (w, h) = (yuv.y.width, yuv.y.height);
    let (cw, ch) = (yuv.u.width, yuv.u.height);
    let mut vu = Vec::with_capacity(cw * ch * 2);
    for (u, v) in yuv.u.data.iter().zip(&yuv.v.data) {
        vu.extend_from_slice(&[*v, *u]);
    }
    let vu = pad_rows(&vu, cw * 2, cw * 2 + pad);
    Planes {
        width: w as u32,
        height: h as u32,
        y: pad_rows(&yuv.y.data, w, w + pad),
        u: vu[1..].to_vec(),
        v: vu,
        y_stride: (w + pad) as u32,
        uv_stride: (cw * 2 + pad) as u32,
        uv_pixel_stride: 2,
    }
}

/// Planar I420: three separate planes, chroma pixel stride 1.
pub(crate) fn i420(img: &RgbImage, pad: usize) -> Planes {
    let yuv =
        YuvFrame::from_rgb(img.width as usize, img.height as usize, &img.data).subsample_chroma();
    let (w, h) = (yuv.y.width, yuv.y.height);
    let cw = yuv.u.width;
    Planes {
        width: w as u32,
        height: h as u32,
        y: pad_rows(&yuv.y.data, w, w + pad),
        u: pad_rows(&yuv.u.data, cw, cw + pad),
        v: pad_rows(&yuv.v.data, cw, cw + pad),
        y_stride: (w + pad) as u32,
        uv_stride: (cw + pad) as u32,
        uv_pixel_stride: 1,
    }
}

/// Packed RGBA8 from an RGB image (alpha 255).
pub(crate) fn rgba(img: &RgbImage) -> Vec<u8> {
    img.data
        .chunks_exact(3)
        .flat_map(|p| [p[0], p[1], p[2], 255])
        .collect()
}
