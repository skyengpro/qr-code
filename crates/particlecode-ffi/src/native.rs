//! Borrowed-buffer entry points for native hosts written in Rust (the Android JNI shim),
//! kept outside [`crate::api`] so the `flutter_rust_bridge` codegen never sees them.
//!
//! These are what [`crate::capi`] uses too: camera memory is read in place, never copied
//! except for de-interleaving chroma.

use crate::api::decoder::ParticleDecoder;
use crate::api::dto::FrameResultDto;

/// [`ParticleDecoder::feed_yuv`] over borrowed planes. `planes` is `(y, u, v)` and
/// `strides` is `(y_stride, uv_stride, uv_pixel_stride)`; for NV12/NV21 the `u` and `v`
/// slices may overlap the same interleaved buffer. `None` if the buffers are inconsistent
/// with the dimensions and strides.
pub fn feed_yuv(
    decoder: &mut ParticleDecoder,
    width: u32,
    height: u32,
    planes: (&[u8], &[u8], &[u8]),
    strides: (u32, u32, u32),
) -> Option<FrameResultDto> {
    decoder.try_feed_yuv(width, height, planes, strides)
}

/// [`ParticleDecoder::feed_rgba`] over a borrowed buffer. `None` if `rgba` is shorter than
/// `4 * width * height` or the size is zero.
pub fn feed_rgba(
    decoder: &mut ParticleDecoder,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Option<FrameResultDto> {
    decoder.try_feed_rgba(width, height, rgba)
}
