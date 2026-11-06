//! A small C ABI over the encoder and decoder, for hosts that are not Dart: the Tauri
//! backend through a C-compatible loader, native apps, scripting languages.
//!
//! The declarations live in `include/particlecode.h`, written by hand to match this file
//! exactly. Conventions:
//!
//! * Handles ([`PcEncoder`], [`PcDecoder`]) are opaque heap objects created by a `*_new`
//!   function and released by the matching `*_free`. A handle must not be used from two
//!   threads at once; different handles are independent.
//! * Functions returning a pointer return NULL on failure; functions returning `i32`
//!   return a status: negative values are errors ([`PC_ERR_NULL`], [`PC_ERR_PANIC`],
//!   [`PC_ERR_INVALID`]), non-negative values are results.
//! * Strings returned by the library are NUL-terminated UTF-8 and must be released with
//!   [`pc_string_free`]; byte buffers with [`pc_bytes_free`].
//! * No panic crosses the boundary: every body runs under `catch_unwind`.
//! * Passing a pointer that is non-NULL but invalid, or a length larger than the buffer, is
//!   undefined behaviour; NULL is always checked.

#![allow(unsafe_code)] // The one module of the workspace that implements a C ABI.

use std::ffi::{CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

use crate::api::decoder::ParticleDecoder;
use crate::api::dto::{EncoderConfig, FrameResultDto, LivenessDto, ProgressDto};
use crate::api::encoder::ParticleEncoder;
use crate::api::error::FfiError;

/// Version of this C ABI. Bumped on any incompatible change to the functions below.
pub const PC_ABI_VERSION: u32 = 1;

/// Status: a pointer argument that must not be NULL was NULL.
pub const PC_ERR_NULL: i32 = -1;
/// Status: a panic was caught at the boundary (a bug); the handle should be discarded.
pub const PC_ERR_PANIC: i32 = -2;
/// Status: an argument had an invalid value (wrong key length, ...).
pub const PC_ERR_INVALID: i32 = -3;

/// `pc_decoder_feed_symbols` status: the frame was rejected.
pub const PC_REJECTED: i32 = 0;
/// `pc_decoder_feed_symbols` status: the frame was accepted.
pub const PC_ACCEPTED: i32 = 1;
/// `pc_decoder_feed_symbols` status: the whole payload has arrived.
pub const PC_DONE: i32 = 2;

/// Opaque encoder handle (wraps a [`ParticleEncoder`]).
#[derive(Debug)]
pub struct PcEncoder(ParticleEncoder);

/// Opaque decoder handle (wraps a [`ParticleDecoder`] plus the last progress).
#[derive(Debug)]
pub struct PcDecoder {
    inner: ParticleDecoder,
    progress: PcProgress,
    reject_reason: Option<String>,
    hint: Option<String>,
}

/// Transfer progress after the last accepted frame (`pc_decoder_progress`).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PcProgress {
    /// Status of the last `pc_decoder_feed_symbols` call.
    pub status: i32,
    /// Distinct symbols held, summed over segments.
    pub have: u32,
    /// Symbols needed if nothing is lost.
    pub need: u32,
    /// Segments decoded and verified.
    pub segments_done: u32,
    /// Segments in the transfer.
    pub segments_total: u32,
}

/// What a camera frame showed (`pc_decoder_feed_yuv`, `pc_decoder_feed_rgba`).
///
/// Fields that do not apply are -1 (`profile`, `shape_id`) or 0.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PcFrameResult {
    /// 1 if the three anchors were found.
    pub locked: i32,
    /// Profile of the frame's header: 0 S, 1 M, 2 L, -1 not read.
    pub profile: i32,
    /// Shape id of the frame's header, -1 if not read.
    pub shape_id: i32,
    /// 1 if the particles were read, else 0.
    pub liveness_read: i32,
    /// 1 if the particles' seed byte matches the header (valid when `liveness_read`).
    pub liveness_consistent: i32,
    /// Low byte of the packet seed shown by the particles (valid when `liveness_read`).
    pub liveness_seed_low: u32,
    /// Rolling HMAC value shown by the particles (valid when `liveness_read`); check it with
    /// `pc_liveness_verify`.
    pub liveness_rolling: u32,
    /// Number of valid anchor points: 0, 3 or 4.
    pub anchor_count: u32,
    /// Anchor positions in image pixels, `x, y` pairs: top, lower right, lower left, center.
    pub anchors: [f64; 8],
    /// Sharpness score (higher is sharper), 0 if not locked.
    pub sharpness: f64,
}

impl From<&FrameResultDto> for PcFrameResult {
    fn from(r: &FrameResultDto) -> Self {
        let mut out = PcFrameResult {
            locked: i32::from(r.locked),
            profile: r.profile.map_or(-1, |p| match p {
                crate::api::dto::ProfileDto::S => 0,
                crate::api::dto::ProfileDto::M => 1,
                crate::api::dto::ProfileDto::L => 2,
            }),
            shape_id: r.shape_id.map_or(-1, i32::from),
            sharpness: r.sharpness,
            ..PcFrameResult::default()
        };
        if let LivenessDto::Read {
            seed_low,
            rolling,
            consistent,
        } = r.liveness
        {
            out.liveness_read = 1;
            out.liveness_consistent = i32::from(consistent);
            out.liveness_seed_low = u32::from(seed_low);
            out.liveness_rolling = u32::from(rolling);
        }
        for (slot, p) in out.anchors.chunks_exact_mut(2).zip(&r.anchors) {
            slot[0] = p.x;
            slot[1] = p.y;
            out.anchor_count += 1;
        }
        out
    }
}

// ---------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------

/// Run `f`, turning a panic into an error value.
fn run<T>(f: impl FnOnce() -> Result<T, FfiError>) -> Result<T, FfiError> {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|payload| {
        let what = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic".to_owned());
        Err(FfiError::Internal(what))
    })
}

/// Run a body that yields a status code; a panic is [`PC_ERR_PANIC`].
fn guard(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(PC_ERR_PANIC)
}

/// Run a body that yields a pointer or `None`; a panic or `None` is NULL.
fn guard_ptr<T>(f: impl FnOnce() -> Option<*mut T>) -> *mut T {
    catch_unwind(AssertUnwindSafe(f))
        .ok()
        .flatten()
        .unwrap_or(ptr::null_mut())
}

/// View `(ptr, len)` as a slice; `None` if `ptr` is NULL.
///
/// # Safety
///
/// If `ptr` is non-NULL it must point to `len` readable, initialized `T`s that stay valid
/// and unmodified for `'a`.
unsafe fn slice<'a, T>(ptr: *const T, len: usize) -> Option<&'a [T]> {
    if ptr.is_null() {
        None
    } else {
        // SAFETY: non-NULL checked above; validity and length are the caller's contract.
        Some(unsafe { std::slice::from_raw_parts(ptr, len) })
    }
}

/// Hand a Rust string to C. Interior NULs cannot occur in the JSON and messages we emit;
/// if one does, the string is dropped and NULL returned.
fn to_c_string(s: String) -> Option<*mut c_char> {
    CString::new(s).ok().map(CString::into_raw)
}

/// Write `message` to `*out_err` if `out_err` is non-NULL.
///
/// # Safety
///
/// `out_err` must be NULL or point to a writable `*mut c_char`.
unsafe fn set_err(out_err: *mut *mut c_char, message: &str) {
    if out_err.is_null() {
        return;
    }
    let text = to_c_string(message.replace('\0', " ")).unwrap_or(ptr::null_mut());
    // SAFETY: non-NULL checked above; writability is the caller's contract.
    unsafe { *out_err = text };
}

/// Parse the JSON config; an empty config means "all defaults".
fn parse_config(json: &[u8]) -> Result<EncoderConfig, FfiError> {
    if json.iter().all(u8::is_ascii_whitespace) {
        return Ok(EncoderConfig::default());
    }
    serde_json::from_slice(json).map_err(|e| FfiError::InvalidConfig(e.to_string()))
}

impl PcDecoder {
    fn feed(&mut self, symbols: Vec<u8>, confidences: Vec<u8>) -> i32 {
        let progress = self.inner.feed_symbols(symbols, confidences);
        self.hint = None;
        self.apply(progress)
    }

    /// Record the outcome of a camera frame and fill `out` if given.
    fn apply_frame(&mut self, result: FrameResultDto, out: *mut PcFrameResult) -> i32 {
        if !out.is_null() {
            // SAFETY: non-NULL checked; writability is the caller's contract.
            unsafe { *out = PcFrameResult::from(&result) };
        }
        self.hint = result.hint;
        self.apply(result.progress)
    }

    /// Update the stored progress and reject reason; returns the status.
    fn apply(&mut self, progress: ProgressDto) -> i32 {
        self.reject_reason = None;
        match progress {
            ProgressDto::Rejected { reason } => {
                self.progress.status = PC_REJECTED;
                self.reject_reason = Some(reason);
            }
            ProgressDto::Accepted {
                have,
                need,
                segments_done,
                segments_total,
            } => {
                self.progress = PcProgress {
                    status: PC_ACCEPTED,
                    have,
                    need,
                    segments_done,
                    segments_total,
                };
            }
            ProgressDto::Done { .. } => {
                // The payload itself is fetched with `pc_decoder_take_ready`.
                self.progress = PcProgress {
                    status: PC_DONE,
                    have: self.progress.need,
                    segments_done: self.progress.segments_total,
                    ..self.progress
                };
            }
        }
        self.progress.status
    }
}

// ---------------------------------------------------------------------------------------
// Library
// ---------------------------------------------------------------------------------------

/// The ABI version of this build ([`PC_ABI_VERSION`]); compare with the header's
/// `PC_ABI_VERSION` at load time.
#[unsafe(no_mangle)]
pub extern "C" fn pc_abi_version() -> u32 {
    // Reading the flutter_rust_bridge keeper pulls it (and every symbol it names) into an
    // iOS static link: the Swift SymbolKeeper calls this function. See `frb_keep`.
    std::hint::black_box(&crate::frb_keep::FRB_SYMBOLS);
    PC_ABI_VERSION
}

/// Release a string returned by this library. NULL is a no-op.
///
/// # Safety
///
/// `s` must be NULL or a pointer returned by this library as a string, not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pc_string_free(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    // SAFETY: by contract `s` came from `CString::into_raw` here and is freed once.
    drop(unsafe { CString::from_raw(s) });
}

/// Release a byte buffer returned by `pc_decoder_take_ready`. NULL is a no-op.
///
/// # Safety
///
/// `ptr` and `len` must be exactly the pair returned by this library, not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pc_bytes_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: by contract this is the leaked `Box<[u8]>` of length `len`.
    drop(unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(ptr, len)) });
}

/// Whether `rolling` is the liveness value of some second within `skew` seconds of
/// `now_unix` for the 32-byte `key`. Returns 1 (valid), 0 (not valid), or a negative error
/// ([`PC_ERR_NULL`], [`PC_ERR_INVALID`] for a key length other than 32).
///
/// # Safety
///
/// `key` must be NULL or point to `key_len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pc_liveness_verify(
    key: *const u8,
    key_len: usize,
    rolling: u16,
    now_unix: u64,
    skew: u64,
) -> i32 {
    guard(|| {
        // SAFETY: forwarded from this function's contract.
        let Some(key) = (unsafe { slice(key, key_len) }) else {
            return PC_ERR_NULL;
        };
        if key.len() != 32 {
            return PC_ERR_INVALID;
        }
        i32::from(crate::api::liveness::liveness_verify(
            key.to_vec(),
            rolling,
            now_unix,
            skew,
        ))
    })
}

// ---------------------------------------------------------------------------------------
// Encoder
// ---------------------------------------------------------------------------------------

/// Create an encoder for `payload_len` bytes at `payload_ptr`.
///
/// `config_json_ptr`/`config_len` is a JSON `EncoderConfig` (every field optional; see the
/// README); NULL with length 0, or empty text, means all defaults. On failure returns NULL
/// and, if `out_err` is non-NULL, stores an error message there (release with
/// [`pc_string_free`]); on success `*out_err` is set to NULL.
///
/// # Safety
///
/// `payload_ptr` must be NULL or point to `payload_len` readable bytes; `config_json_ptr`
/// likewise for `config_len`; `out_err` must be NULL or point to a writable pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pc_encoder_new(
    payload_ptr: *const u8,
    payload_len: usize,
    config_json_ptr: *const c_char,
    config_len: usize,
    out_err: *mut *mut c_char,
) -> *mut PcEncoder {
    if !out_err.is_null() {
        // SAFETY: non-NULL checked; writability is this function's contract.
        unsafe { *out_err = ptr::null_mut() };
    }
    let built = run(|| {
        // SAFETY: forwarded from this function's contract.
        let payload = unsafe { slice(payload_ptr, payload_len) }
            .ok_or_else(|| FfiError::InvalidArgument("payload pointer is NULL".into()))?;
        let config = if config_json_ptr.is_null() {
            if config_len != 0 {
                return Err(FfiError::InvalidArgument(
                    "config pointer is NULL but its length is not 0".into(),
                ));
            }
            EncoderConfig::default()
        } else {
            // SAFETY: non-NULL checked; readability is this function's contract.
            let json = unsafe { slice(config_json_ptr.cast::<u8>(), config_len) }.unwrap_or(&[]);
            parse_config(json)?
        };
        ParticleEncoder::new(payload.to_vec(), config)
    });
    match built {
        Ok(enc) => Box::into_raw(Box::new(PcEncoder(enc))),
        Err(e) => {
            // SAFETY: forwarded from this function's contract.
            unsafe { set_err(out_err, &e.message()) };
            ptr::null_mut()
        }
    }
}

/// Release an encoder. NULL is a no-op.
///
/// # Safety
///
/// `enc` must be NULL or a handle from [`pc_encoder_new`], not yet freed, and not used
/// concurrently.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pc_encoder_free(enc: *mut PcEncoder) {
    if enc.is_null() {
        return;
    }
    // SAFETY: by contract `enc` is a live `Box<PcEncoder>` from `pc_encoder_new`.
    drop(unsafe { Box::from_raw(enc) });
}

/// Frame `i` as JSON (the serde form of the core `FrameSpec`), liveness clock derived from
/// the index. NULL on a NULL handle. Release with [`pc_string_free`].
///
/// # Safety
///
/// `enc` must be NULL or a live encoder handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pc_encoder_frame_json(enc: *const PcEncoder, i: u32) -> *mut c_char {
    guard_ptr(|| {
        // SAFETY: forwarded from this function's contract.
        let enc = unsafe { enc.as_ref() }?;
        to_c_string(enc.0.frame_json(i))
    })
}

/// Frame `i` as JSON with the particles showing the liveness value of `unix_seconds`.
/// NULL on a NULL handle. Release with [`pc_string_free`].
///
/// # Safety
///
/// `enc` must be NULL or a live encoder handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pc_encoder_frame_json_at(
    enc: *const PcEncoder,
    i: u32,
    unix_seconds: u64,
) -> *mut c_char {
    guard_ptr(|| {
        // SAFETY: forwarded from this function's contract.
        let enc = unsafe { enc.as_ref() }?;
        to_c_string(enc.0.frame_json_at(i, unix_seconds))
    })
}

/// Session facts (`EncoderInfo`) as JSON. NULL on a NULL handle. Release with
/// [`pc_string_free`].
///
/// # Safety
///
/// `enc` must be NULL or a live encoder handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pc_encoder_info_json(enc: *const PcEncoder) -> *mut c_char {
    guard_ptr(|| {
        // SAFETY: forwarded from this function's contract.
        let enc = unsafe { enc.as_ref() }?;
        to_c_string(serde_json::to_string(&enc.0.info()).ok()?)
    })
}

/// Frames in one pass over all segments; 0 on a NULL handle.
///
/// # Safety
///
/// `enc` must be NULL or a live encoder handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pc_encoder_frames_per_pass(enc: *const PcEncoder) -> u32 {
    // SAFETY: forwarded from this function's contract.
    let Some(enc) = (unsafe { enc.as_ref() }) else {
        return 0;
    };
    catch_unwind(AssertUnwindSafe(|| enc.0.frames_per_pass())).unwrap_or(0)
}

/// Copy the 32-byte session key to `out`. Returns 0, or [`PC_ERR_NULL`].
///
/// # Safety
///
/// `enc` must be NULL or a live encoder handle; `out` must be NULL or point to 32 writable
/// bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pc_encoder_key(enc: *const PcEncoder, out: *mut u8) -> i32 {
    guard(|| {
        // SAFETY: forwarded from this function's contract.
        let Some(enc) = (unsafe { enc.as_ref() }) else {
            return PC_ERR_NULL;
        };
        if out.is_null() {
            return PC_ERR_NULL;
        }
        let key = enc.0.key();
        // SAFETY: `out` is non-NULL and, by contract, has room for 32 bytes; `key` is a
        // separate allocation of exactly 32 bytes.
        unsafe { ptr::copy_nonoverlapping(key.as_ptr(), out, key.len()) };
        0
    })
}

// ---------------------------------------------------------------------------------------
// Decoder
// ---------------------------------------------------------------------------------------

/// Create a decoder. NULL only if allocation panics.
#[unsafe(no_mangle)]
pub extern "C" fn pc_decoder_new() -> *mut PcDecoder {
    guard_ptr(|| {
        Some(Box::into_raw(Box::new(PcDecoder {
            inner: ParticleDecoder::new(),
            progress: PcProgress::default(),
            reject_reason: None,
            hint: None,
        })))
    })
}

/// Release a decoder. NULL is a no-op.
///
/// # Safety
///
/// `dec` must be NULL or a handle from [`pc_decoder_new`], not yet freed, and not used
/// concurrently.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pc_decoder_free(dec: *mut PcDecoder) {
    if dec.is_null() {
        return;
    }
    // SAFETY: by contract `dec` is a live `Box<PcDecoder>` from `pc_decoder_new`.
    drop(unsafe { Box::from_raw(dec) });
}

/// Feed one frame: `len` symbols (48 header dots, then the body dots), each a 3-bit value
/// `color << 1 | size` in a byte. `confidences` is NULL ("certain") or `len` bytes of
/// classifier confidence. Returns [`PC_REJECTED`], [`PC_ACCEPTED`], [`PC_DONE`], or a
/// negative error ([`PC_ERR_NULL`] for a NULL handle or NULL `symbols`).
///
/// # Safety
///
/// `dec` must be NULL or a live decoder handle not used concurrently; `symbols` must be
/// NULL or point to `len` readable bytes; `confidences` must be NULL or point to `len`
/// readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pc_decoder_feed_symbols(
    dec: *mut PcDecoder,
    symbols: *const u8,
    confidences: *const u8,
    len: usize,
) -> i32 {
    guard(|| {
        // SAFETY: forwarded from this function's contract.
        let Some(dec) = (unsafe { dec.as_mut() }) else {
            return PC_ERR_NULL;
        };
        // SAFETY: forwarded from this function's contract.
        let Some(symbols) = (unsafe { slice(symbols, len) }) else {
            return PC_ERR_NULL;
        };
        let confidences = if confidences.is_null() {
            Vec::new()
        } else {
            // SAFETY: non-NULL checked; readability is this function's contract.
            unsafe { slice(confidences, len) }.unwrap_or(&[]).to_vec()
        };
        dec.feed(symbols.to_vec(), confidences)
    })
}

/// Feed one camera frame in YUV 4:2:0 (Android `YUV_420_888`, iOS NV12/NV21, I420).
///
/// `y` is `height` rows of `width` bytes, `y_stride` bytes apart. `u` and `v` are the Cb and
/// Cr buffers at half resolution (`ceil(width / 2)` x `ceil(height / 2)` samples); sample
/// `(col, row)` is the byte `row * uv_stride + col * uv_pixel_stride`, so planar I420 has
/// `uv_pixel_stride = 1` and interleaved chroma `2` (NV21: `v` is the interleaved buffer and
/// `u` the same buffer from byte 1; NV12 the other way round). Each `*_len` is the size of
/// its buffer in bytes.
///
/// Returns [`PC_REJECTED`] (no code found, unreadable, ...; see [`pc_decoder_reject_reason`]
/// and [`pc_decoder_hint`]), [`PC_ACCEPTED`], [`PC_DONE`], or a negative error:
/// [`PC_ERR_NULL`] for a NULL handle or plane, [`PC_ERR_INVALID`] if a size is zero or a
/// buffer is too short for the stated dimensions and strides. If `out` is non-NULL it is
/// filled for every non-negative result.
///
/// # Safety
///
/// `dec` must be NULL or a live decoder handle not used concurrently; `y`, `u`, `v` must be
/// NULL or point to `y_len`, `u_len`, `v_len` readable bytes; `out` must be NULL or point to
/// a writable `PcFrameResult`.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)] // a flat C signature, like the camera APIs it serves
pub unsafe extern "C" fn pc_decoder_feed_yuv(
    dec: *mut PcDecoder,
    width: u32,
    height: u32,
    y: *const u8,
    y_len: usize,
    u: *const u8,
    u_len: usize,
    v: *const u8,
    v_len: usize,
    y_stride: u32,
    uv_stride: u32,
    uv_pixel_stride: u32,
    out: *mut PcFrameResult,
) -> i32 {
    guard(|| {
        // SAFETY: forwarded from this function's contract.
        let Some(dec) = (unsafe { dec.as_mut() }) else {
            return PC_ERR_NULL;
        };
        // SAFETY: forwarded from this function's contract.
        let planes = unsafe { (slice(y, y_len), slice(u, u_len), slice(v, v_len)) };
        let (Some(y), Some(u), Some(v)) = planes else {
            return PC_ERR_NULL;
        };
        let strides = (y_stride, uv_stride, uv_pixel_stride);
        match dec.inner.try_feed_yuv(width, height, (y, u, v), strides) {
            Some(result) => dec.apply_frame(result, out),
            None => PC_ERR_INVALID,
        }
    })
}

/// Feed one RGBA8 camera frame (`rgba_len >= 4 * width * height`, tightly packed; alpha is
/// ignored), for desktop and webcam capture. Results as for [`pc_decoder_feed_yuv`];
/// [`PC_ERR_INVALID`] if a size is zero or `rgba_len` is too small.
///
/// # Safety
///
/// `dec` must be NULL or a live decoder handle not used concurrently; `rgba` must be NULL or
/// point to `rgba_len` readable bytes; `out` must be NULL or point to a writable
/// `PcFrameResult`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pc_decoder_feed_rgba(
    dec: *mut PcDecoder,
    width: u32,
    height: u32,
    rgba: *const u8,
    rgba_len: usize,
    out: *mut PcFrameResult,
) -> i32 {
    guard(|| {
        // SAFETY: forwarded from this function's contract.
        let Some(dec) = (unsafe { dec.as_mut() }) else {
            return PC_ERR_NULL;
        };
        // SAFETY: forwarded from this function's contract.
        let Some(rgba) = (unsafe { slice(rgba, rgba_len) }) else {
            return PC_ERR_NULL;
        };
        match dec.inner.try_feed_rgba(width, height, rgba) {
            Some(result) => dec.apply_frame(result, out),
            None => PC_ERR_INVALID,
        }
    })
}

/// User-facing advice after the last camera frame ("Too dark or too much glare ..."); NULL
/// if there is none (or on a NULL handle). Release with [`pc_string_free`].
///
/// # Safety
///
/// `dec` must be NULL or a live decoder handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pc_decoder_hint(dec: *const PcDecoder) -> *mut c_char {
    guard_ptr(|| {
        // SAFETY: forwarded from this function's contract.
        let dec = unsafe { dec.as_ref() }?;
        to_c_string(dec.hint.clone()?)
    })
}

/// Progress after the last `pc_decoder_feed_symbols` call, written to `*out`. Returns 0 or
/// [`PC_ERR_NULL`]. After [`PC_DONE`], `have == need` and `segments_done ==
/// segments_total`; before any accepted frame all counts are 0.
///
/// # Safety
///
/// `dec` must be NULL or a live decoder handle; `out` must be NULL or point to a writable
/// `PcProgress`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pc_decoder_progress(dec: *const PcDecoder, out: *mut PcProgress) -> i32 {
    guard(|| {
        // SAFETY: forwarded from this function's contract.
        let Some(dec) = (unsafe { dec.as_ref() }) else {
            return PC_ERR_NULL;
        };
        if out.is_null() {
            return PC_ERR_NULL;
        }
        // SAFETY: `out` is non-NULL and, by contract, writable.
        unsafe { *out = dec.progress };
        0
    })
}

/// Why the last feed (symbols or camera frame) returned [`PC_REJECTED`]; NULL if it did
/// not (or on a NULL handle). Release with [`pc_string_free`].
///
/// # Safety
///
/// `dec` must be NULL or a live decoder handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pc_decoder_reject_reason(dec: *const PcDecoder) -> *mut c_char {
    guard_ptr(|| {
        // SAFETY: forwarded from this function's contract.
        let dec = unsafe { dec.as_ref() }?;
        to_c_string(dec.reject_reason.clone()?)
    })
}

/// Take the payload bytes delivered in order since the last call. Returns 1 and sets
/// `*out_ptr`/`*out_len` (release with [`pc_bytes_free`]) if there are bytes, 0 and a NULL
/// buffer if there are none, or a negative error. After [`PC_DONE`], the concatenation of
/// everything taken equals the payload.
///
/// # Safety
///
/// `dec` must be NULL or a live decoder handle not used concurrently; `out_ptr` and
/// `out_len` must be NULL or point to writable storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pc_decoder_take_ready(
    dec: *mut PcDecoder,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    guard(|| {
        if out_ptr.is_null() || out_len.is_null() {
            return PC_ERR_NULL;
        }
        // SAFETY: both pointers are non-NULL and, by contract, writable.
        unsafe {
            *out_ptr = ptr::null_mut();
            *out_len = 0;
        }
        // SAFETY: forwarded from this function's contract.
        let Some(dec) = (unsafe { dec.as_mut() }) else {
            return PC_ERR_NULL;
        };
        match dec.inner.take_ready() {
            Some(bytes) if !bytes.is_empty() => {
                let boxed = bytes.into_boxed_slice();
                let len = boxed.len();
                let data = Box::into_raw(boxed).cast::<u8>();
                // SAFETY: both pointers are non-NULL and, by contract, writable.
                unsafe {
                    *out_ptr = data;
                    *out_len = len;
                }
                1
            }
            _ => 0,
        }
    })
}

/// Id of the session being received; returns 1 and sets `*out` once a frame was accepted,
/// 0 before that, or a negative error.
///
/// # Safety
///
/// `dec` must be NULL or a live decoder handle; `out` must be NULL or point to a writable
/// `u16`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pc_decoder_session_id(dec: *const PcDecoder, out: *mut u16) -> i32 {
    guard(|| {
        // SAFETY: forwarded from this function's contract.
        let Some(dec) = (unsafe { dec.as_ref() }) else {
            return PC_ERR_NULL;
        };
        if out.is_null() {
            return PC_ERR_NULL;
        }
        match dec.inner.session_id() {
            Some(id) => {
                // SAFETY: `out` is non-NULL and, by contract, writable.
                unsafe { *out = id };
                1
            }
            None => 0,
        }
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::CStr;

    use particlecode_core::FrameSpec;
    use particlecode_render::synth::RgbImage;

    use super::*;

    /// Owned copy of a library string, then freed.
    fn take_string(s: *mut c_char) -> Option<String> {
        if s.is_null() {
            return None;
        }
        // SAFETY: `s` is a live NUL-terminated string just returned by this library.
        let text = unsafe { CStr::from_ptr(s) }.to_str().unwrap().to_owned();
        // SAFETY: `s` came from this library and is freed exactly once, here.
        unsafe { pc_string_free(s) };
        Some(text)
    }

    fn new_encoder(payload: &[u8], config: &str) -> *mut PcEncoder {
        let mut err: *mut c_char = ptr::null_mut();
        // SAFETY: all pointers come from live slices/locals with the lengths passed.
        let enc = unsafe {
            pc_encoder_new(
                payload.as_ptr(),
                payload.len(),
                config.as_ptr().cast(),
                config.len(),
                &raw mut err,
            )
        };
        let message = take_string(err);
        assert!(!enc.is_null(), "encoder_new failed: {message:?}");
        assert!(message.is_none());
        enc
    }

    fn frame_symbols(enc: *const PcEncoder, i: u32) -> Vec<u8> {
        // SAFETY: `enc` is a live handle from `new_encoder`.
        let json = take_string(unsafe { pc_encoder_frame_json(enc, i) }).unwrap();
        let spec: FrameSpec = serde_json::from_str(&json).unwrap();
        spec.all_symbols().iter().map(|s| s.value).collect()
    }

    fn feed(dec: *mut PcDecoder, symbols: &[u8], conf: Option<&[u8]>) -> i32 {
        // SAFETY: `dec` is live; the slices outlive the call and have `symbols.len()` bytes.
        unsafe {
            pc_decoder_feed_symbols(
                dec,
                symbols.as_ptr(),
                conf.map_or(ptr::null(), <[u8]>::as_ptr),
                symbols.len(),
            )
        }
    }

    fn take_ready(dec: *mut PcDecoder) -> Option<Vec<u8>> {
        let mut data: *mut u8 = ptr::null_mut();
        let mut len = 0usize;
        // SAFETY: `dec` is live; the out pointers are valid locals.
        let status = unsafe { pc_decoder_take_ready(dec, &raw mut data, &raw mut len) };
        if status == 1 {
            // SAFETY: on status 1 the library returned `len` readable bytes at `data`.
            let bytes = unsafe { std::slice::from_raw_parts(data, len) }.to_vec();
            // SAFETY: the pair (data, len) is exactly what take_ready returned, freed once.
            unsafe { pc_bytes_free(data, len) };
            Some(bytes)
        } else {
            assert_eq!(status, 0);
            assert!(data.is_null() && len == 0);
            None
        }
    }

    fn payload(n: usize) -> Vec<u8> {
        let mut x = 0x0BAD_CAFEu32;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x >> 8) as u8
            })
            .collect()
    }

    const CONFIG: &str = r#"{"session_id":4660,"clock_origin_unix":1700000000,
        "key":[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32]}"#;

    #[test]
    fn c_abi_round_trip_with_dropped_frames() {
        let data = payload(4000);
        let enc = new_encoder(&data, CONFIG);
        // SAFETY: `enc` is a live handle.
        let per_pass = unsafe { pc_encoder_frames_per_pass(enc) };
        assert!(per_pass > 0);

        // SAFETY: `enc` is live; the info string is freed by take_string.
        let info = take_string(unsafe { pc_encoder_info_json(enc) }).unwrap();
        let info: serde_json::Value = serde_json::from_str(&info).unwrap();
        assert_eq!(info["session_id"], 4660);
        assert_eq!(info["frames_per_pass"], per_pass);

        let mut key = [0u8; 32];
        // SAFETY: `enc` is live and `key` has 32 writable bytes.
        assert_eq!(unsafe { pc_encoder_key(enc, key.as_mut_ptr()) }, 0);
        assert_eq!(key[0], 1);
        assert_eq!(key[31], 32);

        let dec = pc_decoder_new();
        assert!(!dec.is_null());
        let mut got = Vec::new();
        let mut done = false;
        for i in 0..per_pass * 4 {
            if i % 4 == 2 {
                continue; // dropped
            }
            let status = feed(dec, &frame_symbols(enc, i), None);
            assert!(
                status == PC_ACCEPTED || status == PC_DONE,
                "status {status}"
            );
            if let Some(bytes) = take_ready(dec) {
                got.extend(bytes);
            }
            if status == PC_DONE {
                done = true;
                break;
            }
        }
        assert!(done, "transfer did not finish");
        assert_eq!(got, data);

        let mut progress = PcProgress::default();
        // SAFETY: `dec` is live and `progress` is a valid local.
        assert_eq!(unsafe { pc_decoder_progress(dec, &raw mut progress) }, 0);
        assert_eq!(progress.status, PC_DONE);
        assert_eq!(progress.have, progress.need);
        assert_eq!(progress.segments_done, progress.segments_total);
        let mut id = 0u16;
        // SAFETY: `dec` is live and `id` is a valid local.
        assert_eq!(unsafe { pc_decoder_session_id(dec, &raw mut id) }, 1);
        assert_eq!(id, 4660);

        // SAFETY: both handles are live, freed once, not used afterwards.
        unsafe {
            pc_decoder_free(dec);
            pc_encoder_free(enc);
        }
    }

    #[test]
    fn frame_json_at_and_liveness_verify() {
        let enc = new_encoder(b"live", CONFIG);
        let now = 1_700_000_123u64;
        // SAFETY: `enc` is live.
        let json = take_string(unsafe { pc_encoder_frame_json_at(enc, 0, now) }).unwrap();
        let spec: FrameSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(spec.index, 0);
        let key: Vec<u8> = (1..=32).collect();
        let rolling =
            particlecode_core::liveness::rolling_value(&key.clone().try_into().unwrap(), now);
        // SAFETY: `key` is a live 32-byte buffer.
        unsafe {
            assert_eq!(pc_liveness_verify(key.as_ptr(), 32, rolling, now, 0), 1);
            assert_eq!(
                pc_liveness_verify(key.as_ptr(), 31, rolling, now, 0),
                PC_ERR_INVALID
            );
            assert_eq!(
                pc_liveness_verify(ptr::null(), 32, rolling, now, 0),
                PC_ERR_NULL
            );
        }
        // SAFETY: `enc` is live, freed once.
        unsafe { pc_encoder_free(enc) };
    }

    #[test]
    fn garbage_is_rejected_with_a_reason() {
        let dec = pc_decoder_new();
        let status = feed(dec, &[1, 2, 3], None);
        assert_eq!(status, PC_REJECTED);
        // SAFETY: `dec` is live.
        let reason = take_string(unsafe { pc_decoder_reject_reason(dec) });
        assert!(reason.is_some_and(|r| !r.is_empty()));
        assert_eq!(feed(dec, &[9; 48], Some(&[255; 48])), PC_REJECTED);
        let mut id = 0u16;
        // SAFETY: `dec` is live and `id` is a valid local.
        assert_eq!(unsafe { pc_decoder_session_id(dec, &raw mut id) }, 0);
        assert!(take_ready(dec).is_none());
        // SAFETY: `dec` is live, freed once.
        unsafe { pc_decoder_free(dec) };
    }

    #[test]
    fn null_pointers_return_errors_not_crashes() {
        let data = [1u8, 2, 3];
        let mut err: *mut c_char = ptr::null_mut();
        // SAFETY: every call below passes NULL or valid locals; NULL is checked by contract.
        unsafe {
            // NULL payload.
            let enc = pc_encoder_new(ptr::null(), 3, ptr::null(), 0, &raw mut err);
            assert!(enc.is_null());
            assert!(take_string(err).unwrap().contains("NULL"));

            // NULL out_err is allowed.
            assert!(pc_encoder_new(ptr::null(), 0, ptr::null(), 0, ptr::null_mut()).is_null());

            // NULL config pointer with a non-zero length.
            err = ptr::null_mut();
            assert!(pc_encoder_new(data.as_ptr(), 3, ptr::null(), 5, &raw mut err).is_null());
            assert!(take_string(err).is_some());

            // NULL config means defaults.
            err = ptr::null_mut();
            let ok = pc_encoder_new(data.as_ptr(), 3, ptr::null(), 0, &raw mut err);
            assert!(!ok.is_null() && err.is_null());
            pc_encoder_free(ok);

            assert!(pc_encoder_frame_json(ptr::null(), 0).is_null());
            assert!(pc_encoder_frame_json_at(ptr::null(), 0, 0).is_null());
            assert!(pc_encoder_info_json(ptr::null()).is_null());
            assert_eq!(pc_encoder_frames_per_pass(ptr::null()), 0);
            assert_eq!(pc_encoder_key(ptr::null(), ptr::null_mut()), PC_ERR_NULL);
            pc_encoder_free(ptr::null_mut());

            assert_eq!(
                pc_decoder_feed_symbols(ptr::null_mut(), data.as_ptr(), ptr::null(), 3),
                PC_ERR_NULL
            );
            assert_eq!(
                pc_decoder_progress(ptr::null(), ptr::null_mut()),
                PC_ERR_NULL
            );
            assert!(pc_decoder_reject_reason(ptr::null()).is_null());
            assert_eq!(
                pc_decoder_take_ready(ptr::null_mut(), ptr::null_mut(), ptr::null_mut()),
                PC_ERR_NULL
            );
            assert_eq!(
                pc_decoder_session_id(ptr::null(), ptr::null_mut()),
                PC_ERR_NULL
            );
            pc_decoder_free(ptr::null_mut());
            pc_string_free(ptr::null_mut());
            pc_bytes_free(ptr::null_mut(), 0);
        }

        // Live handle, NULL arguments.
        let dec = pc_decoder_new();
        let enc = new_encoder(&data, "");
        // SAFETY: handles are live; NULL arguments are checked by contract.
        unsafe {
            assert_eq!(
                pc_decoder_feed_symbols(dec, ptr::null(), ptr::null(), 3),
                PC_ERR_NULL
            );
            assert_eq!(pc_decoder_progress(dec, ptr::null_mut()), PC_ERR_NULL);
            assert_eq!(
                pc_decoder_take_ready(dec, ptr::null_mut(), ptr::null_mut()),
                PC_ERR_NULL
            );
            assert_eq!(pc_encoder_key(enc, ptr::null_mut()), PC_ERR_NULL);
            pc_decoder_free(dec);
            pc_encoder_free(enc);
        }
    }

    #[test]
    fn bad_config_reports_an_error() {
        let data = [7u8; 10];
        for (config, needle) in [
            ("{not json", "invalid configuration"),
            (r#"{"key":[1,2,3]}"#, "32 bytes"),
            (r#"{"shape":{"kind":"require","id":200}}"#, "shape"),
            (
                r#"{"unknown_field_is_fine_but_type_is_not":1,"fps":"fast"}"#,
                "invalid",
            ),
        ] {
            let mut err: *mut c_char = ptr::null_mut();
            // SAFETY: pointers come from live locals with matching lengths.
            let enc = unsafe {
                pc_encoder_new(
                    data.as_ptr(),
                    data.len(),
                    config.as_ptr().cast(),
                    config.len(),
                    &raw mut err,
                )
            };
            assert!(enc.is_null(), "{config}");
            let message = take_string(err).unwrap();
            assert!(message.contains(needle), "{config}: {message}");
        }
    }

    fn feed_planes(dec: *mut PcDecoder, p: &crate::testutil::Planes) -> (i32, PcFrameResult) {
        let mut out = PcFrameResult::default();
        // SAFETY: `dec` is live; each pointer/length pair comes from a live Vec; `out` is a
        // valid local.
        let status = unsafe {
            pc_decoder_feed_yuv(
                dec,
                p.width,
                p.height,
                p.y.as_ptr(),
                p.y.len(),
                p.u.as_ptr(),
                p.u.len(),
                p.v.as_ptr(),
                p.v.len(),
                p.y_stride,
                p.uv_stride,
                p.uv_pixel_stride,
                &raw mut out,
            )
        };
        (status, out)
    }

    /// Run a transfer through `feed` until `PC_DONE`; returns the payload taken.
    fn camera_transfer(
        data: &[u8],
        mut feed: impl FnMut(*mut PcDecoder, &RgbImage) -> (i32, PcFrameResult),
    ) {
        let enc = crate::testutil::encoder(data);
        let dec = pc_decoder_new();
        let mut got = Vec::new();
        let mut done = false;
        for i in 0..enc.frames_per_pass() * 3 {
            let (status, r) = feed(dec, &crate::testutil::camera_rgb(&enc, i, 760));
            assert!(
                status == PC_ACCEPTED || status == PC_DONE,
                "frame {i}: {status}"
            );
            assert_eq!((r.locked, r.profile, r.shape_id), (1, 1, 0));
            assert!(r.anchor_count >= 3 && r.sharpness > 0.0);
            assert_eq!((r.liveness_read, r.liveness_consistent), (1, 1));
            assert!(r.anchors[..6].iter().all(|v| v.is_finite() && *v > 0.0));
            if let Some(bytes) = take_ready(dec) {
                got.extend(bytes);
            }
            if status == PC_DONE {
                done = true;
                break;
            }
        }
        assert!(done, "transfer did not finish");
        assert_eq!(got, data);
        // SAFETY: `dec` is live, freed once.
        unsafe { pc_decoder_free(dec) };
    }

    #[test]
    fn c_abi_nv21_camera_round_trip() {
        let data = crate::testutil::payload(2600);
        camera_transfer(&data, |dec, img| {
            feed_planes(dec, &crate::testutil::nv21(img, 8))
        });
    }

    #[test]
    fn c_abi_i420_camera_round_trip() {
        let data = crate::testutil::payload(2600);
        camera_transfer(&data, |dec, img| {
            feed_planes(dec, &crate::testutil::i420(img, 0))
        });
    }

    #[test]
    fn c_abi_rgba_camera_round_trip() {
        let data = crate::testutil::payload(1200);
        camera_transfer(&data, |dec, img| {
            let rgba = crate::testutil::rgba(img);
            let mut out = PcFrameResult::default();
            // SAFETY: `dec` is live; `rgba` is a live Vec of the stated length; `out` is a
            // valid local.
            let status = unsafe {
                pc_decoder_feed_rgba(
                    dec,
                    img.width,
                    img.height,
                    rgba.as_ptr(),
                    rgba.len(),
                    &raw mut out,
                )
            };
            (status, out)
        });
    }

    #[test]
    fn camera_calls_check_pointers_and_sizes() {
        let dec = pc_decoder_new();
        let grey = vec![128u8; 64 * 64];
        let half = vec![128u8; 32 * 32];
        let g = grey.as_ptr();
        let h = half.as_ptr();
        // SAFETY: handles are live, non-NULL pointers cover the stated lengths, and NULL
        // arguments are checked by contract.
        unsafe {
            let yuv = |dec, w, hh, y, yl, u, ul, strides: (u32, u32, u32), out| {
                pc_decoder_feed_yuv(
                    dec,
                    w,
                    hh,
                    y,
                    yl,
                    u,
                    ul,
                    h,
                    half.len(),
                    strides.0,
                    strides.1,
                    strides.2,
                    out,
                )
            };
            // Valid but empty picture: rejected, with a hint and a reason.
            let mut out = PcFrameResult {
                locked: 9,
                ..PcFrameResult::default()
            };
            assert_eq!(
                yuv(
                    dec,
                    64,
                    64,
                    g,
                    grey.len(),
                    h,
                    half.len(),
                    (64, 32, 1),
                    &raw mut out
                ),
                PC_REJECTED
            );
            assert_eq!((out.locked, out.profile, out.anchor_count), (0, -1, 0));
            assert!(take_string(pc_decoder_hint(dec)).is_some());
            assert!(take_string(pc_decoder_reject_reason(dec)).is_some());
            // NULL out is allowed.
            assert_eq!(
                yuv(
                    dec,
                    64,
                    64,
                    g,
                    grey.len(),
                    h,
                    half.len(),
                    (64, 32, 1),
                    ptr::null_mut()
                ),
                PC_REJECTED
            );
            // NULL handle / planes.
            assert_eq!(
                yuv(
                    ptr::null_mut(),
                    64,
                    64,
                    g,
                    grey.len(),
                    h,
                    half.len(),
                    (64, 32, 1),
                    ptr::null_mut()
                ),
                PC_ERR_NULL
            );
            assert_eq!(
                yuv(
                    dec,
                    64,
                    64,
                    ptr::null(),
                    0,
                    h,
                    half.len(),
                    (64, 32, 1),
                    ptr::null_mut()
                ),
                PC_ERR_NULL
            );
            assert_eq!(
                yuv(
                    dec,
                    64,
                    64,
                    g,
                    grey.len(),
                    ptr::null(),
                    0,
                    (64, 32, 1),
                    ptr::null_mut()
                ),
                PC_ERR_NULL
            );
            // Sizes: zero dimension, short buffer, stride below width, bad pixel stride.
            assert_eq!(
                yuv(
                    dec,
                    0,
                    64,
                    g,
                    grey.len(),
                    h,
                    half.len(),
                    (64, 32, 1),
                    ptr::null_mut()
                ),
                PC_ERR_INVALID
            );
            assert_eq!(
                yuv(
                    dec,
                    64,
                    64,
                    g,
                    100,
                    h,
                    half.len(),
                    (64, 32, 1),
                    ptr::null_mut()
                ),
                PC_ERR_INVALID
            );
            assert_eq!(
                yuv(
                    dec,
                    64,
                    64,
                    g,
                    grey.len(),
                    h,
                    half.len(),
                    (63, 32, 1),
                    ptr::null_mut()
                ),
                PC_ERR_INVALID
            );
            assert_eq!(
                yuv(
                    dec,
                    64,
                    64,
                    g,
                    grey.len(),
                    h,
                    half.len(),
                    (64, 32, 0),
                    ptr::null_mut()
                ),
                PC_ERR_INVALID
            );
            assert_eq!(
                yuv(
                    dec,
                    64,
                    64,
                    g,
                    grey.len(),
                    h,
                    half.len(),
                    (64, 32, 5),
                    ptr::null_mut()
                ),
                PC_ERR_INVALID
            );
            assert_eq!(
                yuv(
                    dec,
                    u32::MAX,
                    u32::MAX,
                    g,
                    grey.len(),
                    h,
                    half.len(),
                    (u32::MAX, 1, 2),
                    ptr::null_mut()
                ),
                PC_ERR_INVALID
            );

            // RGBA.
            let rgba = vec![200u8; 32 * 32 * 4];
            assert_eq!(
                pc_decoder_feed_rgba(dec, 32, 32, rgba.as_ptr(), rgba.len(), ptr::null_mut()),
                PC_REJECTED
            );
            assert_eq!(
                pc_decoder_feed_rgba(dec, 32, 32, rgba.as_ptr(), rgba.len() - 1, ptr::null_mut()),
                PC_ERR_INVALID
            );
            assert_eq!(
                pc_decoder_feed_rgba(dec, 0, 32, rgba.as_ptr(), rgba.len(), ptr::null_mut()),
                PC_ERR_INVALID
            );
            assert_eq!(
                pc_decoder_feed_rgba(dec, 32, 32, ptr::null(), 0, ptr::null_mut()),
                PC_ERR_NULL
            );
            assert_eq!(
                pc_decoder_feed_rgba(
                    ptr::null_mut(),
                    32,
                    32,
                    rgba.as_ptr(),
                    rgba.len(),
                    ptr::null_mut()
                ),
                PC_ERR_NULL
            );
            assert!(pc_decoder_hint(ptr::null()).is_null());
            pc_decoder_free(dec);
        }
    }

    #[test]
    fn abi_version_matches_the_header() {
        assert_eq!(pc_abi_version(), 1);
        let header = include_str!("../include/particlecode.h");
        assert!(header.contains("#define PC_ABI_VERSION 1"));
    }

    #[test]
    fn every_exported_function_is_declared_in_the_header() {
        let header = include_str!("../include/particlecode.h");
        let source = include_str!("capi.rs");
        for line in source.lines() {
            if let Some(rest) = line
                .strip_prefix("pub unsafe extern \"C\" fn ")
                .or_else(|| line.strip_prefix("pub extern \"C\" fn "))
            {
                let name = rest.split('(').next().unwrap();
                assert!(
                    header.contains(&format!("{name}(")),
                    "{name} missing in header"
                );
            }
        }
    }
}
