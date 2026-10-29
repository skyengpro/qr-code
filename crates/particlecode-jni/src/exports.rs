//! The `Java_com_vaam_particlecode_NativeBridge_*` symbols.
//!
//! Every export runs its body under `guarded`, which catches panics (the JVM would abort
//! on an unwind) and maps them, and JNI failures, to an error code. Bad arguments are error
//! codes, never undefined behaviour: handles are table keys (`crate::registry`), buffers and
//! arrays are validated before use.
//!
//! # Threading
//!
//! A handle is used by one thread at a time. The registry serializes concurrent calls on one
//! handle (so a violation costs latency, never memory safety), but the intended design is
//! that the Kotlin wrapper (`ParticleScanner`) holds a lock around every call and runs the
//! analyzer on a single executor thread. Different handles are independent.

#![allow(unsafe_code)] // `#[export_name]` symbols are `unsafe` attributes in edition 2024.

use jni::errors::Error as JniError;
use jni::objects::{JByteArray, JByteBuffer, JClass, JFloatArray, JIntArray, JString};
use jni::sys::{jint, jlong};
use jni::{Env, EnvUnowned, Outcome, jni_mangle};

use crate::direct::direct_slice;
use crate::layout::{ERR_INVALID, ERR_JNI, ERR_NULL, ERR_PANIC, FrameReport, GEOM_LEN, OUT_LEN};
use crate::registry;

/// Run `f` with an `Env`, turning a panic or a JNI error into `fail(code)`.
fn guarded<'local, T>(
    env: &mut EnvUnowned<'local>,
    fail: impl FnOnce(i32) -> T,
    f: impl FnOnce(&mut Env<'local>) -> Result<T, JniError>,
) -> T {
    match env.with_env(f).into_outcome() {
        Outcome::Ok(v) => v,
        Outcome::Err(_) => fail(ERR_JNI),
        Outcome::Panic(_) => fail(ERR_PANIC),
    }
}

/// `nativeNew(): Long`: a new decoder session. Never 0 on success; 0 on failure.
#[jni_mangle("com.vaam.particlecode.NativeBridge")]
pub fn native_new<'local>(mut env: EnvUnowned<'local>, _class: JClass<'local>) -> jlong {
    guarded(&mut env, |_| 0, |_| Ok(registry::create()))
}

/// `nativeFree(handle)`: release a session. Unknown, freed or zero handles are ignored.
#[jni_mangle("com.vaam.particlecode.NativeBridge")]
pub fn native_free<'local>(mut env: EnvUnowned<'local>, _class: JClass<'local>, handle: jlong) {
    guarded(
        &mut env,
        |_| (),
        |_| {
            registry::free(handle);
            Ok(())
        },
    );
}

/// `nativeReset(handle): Int`: forget the transfer and start over. 0, or `ERR_*`.
#[jni_mangle("com.vaam.particlecode.NativeBridge")]
pub fn native_reset<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    handle: jlong,
) -> jint {
    guarded(
        &mut env,
        |code| code,
        |_| Ok(registry::with(handle, crate::scanner::Scanner::reset).map_or_else(|e| e, |()| 0)),
    )
}

/// The arguments of a camera frame that are not buffers.
#[derive(Clone, Copy)]
struct Geometry {
    width: u32,
    height: u32,
    strides: (u32, u32, u32),
}

impl Geometry {
    /// `None` if any value is negative or the size is zero.
    fn new(
        width: jint,
        height: jint,
        y_stride: jint,
        uv_stride: jint,
        uv_pixel_stride: jint,
    ) -> Option<Geometry> {
        let u = |v: jint| u32::try_from(v).ok();
        let (width, height) = (u(width)?, u(height)?);
        (width > 0 && height > 0).then_some(())?;
        Some(Geometry {
            width,
            height,
            strides: (u(y_stride)?, u(uv_stride)?, u(uv_pixel_stride)?),
        })
    }
}

/// Validate the result arrays before a frame is consumed: `Ok(None)` if usable,
/// `Ok(Some(code))` for an argument error.
fn out_arrays_status(
    env: &Env<'_>,
    out: &JIntArray<'_>,
    geom: &JFloatArray<'_>,
) -> Result<Option<i32>, JniError> {
    if out.is_null() || geom.is_null() {
        return Ok(Some(ERR_NULL));
    }
    if out.len(env)? < OUT_LEN || geom.len(env)? < GEOM_LEN {
        return Ok(Some(ERR_INVALID));
    }
    Ok(None)
}

/// Feed `planes` to the session behind `handle` and write the arrays.
fn feed_and_report(
    env: &Env<'_>,
    handle: jlong,
    geometry: Geometry,
    planes: (&[u8], &[u8], &[u8]),
    out: &JIntArray<'_>,
    out_geom: &JFloatArray<'_>,
) -> Result<jint, JniError> {
    let fed = registry::with(handle, |s| {
        s.feed_yuv(geometry.width, geometry.height, planes, geometry.strides)
    });
    let report: FrameReport = match fed {
        Err(code) => return Ok(code),
        Ok(None) => return Ok(ERR_INVALID),
        Ok(Some(report)) => report,
    };
    out.set_region(env, 0, &report.ints)?;
    out_geom.set_region(env, 0, &report.geom)?;
    Ok(report.status)
}

/// `nativeFeedYuv(handle, width, height, y, u, v, yRowStride, uvRowStride, uvPixelStride,
/// out, outGeom): Int`: analyze one `YUV_420_888` camera frame in place.
///
/// The planes are direct `ByteBuffer`s read from their base address for their capacity
/// (position and limit are ignored; `u` and `v` may be two views of one interleaved buffer).
/// Returns `STATUS_*` or `ERR_*` (see [`crate::layout`]); the arrays are written for
/// non-negative results only.
#[jni_mangle("com.vaam.particlecode.NativeBridge")]
#[allow(clippy::too_many_arguments)] // the JNI signature is flat by nature
pub fn native_feed_yuv<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    handle: jlong,
    width: jint,
    height: jint,
    y: JByteBuffer<'local>,
    u: JByteBuffer<'local>,
    v: JByteBuffer<'local>,
    y_row_stride: jint,
    uv_row_stride: jint,
    uv_pixel_stride: jint,
    out: JIntArray<'local>,
    out_geom: JFloatArray<'local>,
) -> jint {
    guarded(
        &mut env,
        |code| code,
        |env| {
            if let Some(code) = out_arrays_status(env, &out, &out_geom)? {
                return Ok(code);
            }
            let Some(geometry) =
                Geometry::new(width, height, y_row_stride, uv_row_stride, uv_pixel_stride)
            else {
                return Ok(ERR_INVALID);
            };
            let planes = (|| {
                Ok::<_, i32>((
                    direct_slice(env, &y)?,
                    direct_slice(env, &u)?,
                    direct_slice(env, &v)?,
                ))
            })();
            match planes {
                Ok(planes) => feed_and_report(env, handle, geometry, planes, &out, &out_geom),
                Err(code) => Ok(code),
            }
        },
    )
}

/// Copy a `byte[]` into a `Vec`; `None` for null.
fn bytes_of(env: &Env<'_>, array: &JByteArray<'_>) -> Result<Option<Vec<u8>>, JniError> {
    if array.is_null() {
        return Ok(None);
    }
    env.convert_byte_array(array).map(Some)
}

/// `nativeFeedYuvBytes(...)`: [`native_feed_yuv`] over `byte[]` planes (copied), for tests and
/// callers without direct buffers.
#[jni_mangle("com.vaam.particlecode.NativeBridge")]
#[allow(clippy::too_many_arguments)] // the JNI signature is flat by nature
pub fn native_feed_yuv_bytes<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    handle: jlong,
    width: jint,
    height: jint,
    y: JByteArray<'local>,
    u: JByteArray<'local>,
    v: JByteArray<'local>,
    y_row_stride: jint,
    uv_row_stride: jint,
    uv_pixel_stride: jint,
    out: JIntArray<'local>,
    out_geom: JFloatArray<'local>,
) -> jint {
    guarded(
        &mut env,
        |code| code,
        |env| {
            if let Some(code) = out_arrays_status(env, &out, &out_geom)? {
                return Ok(code);
            }
            let Some(geometry) =
                Geometry::new(width, height, y_row_stride, uv_row_stride, uv_pixel_stride)
            else {
                return Ok(ERR_INVALID);
            };
            let (Some(y), Some(u), Some(v)) =
                (bytes_of(env, &y)?, bytes_of(env, &u)?, bytes_of(env, &v)?)
            else {
                return Ok(ERR_NULL);
            };
            feed_and_report(env, handle, geometry, (&y, &u, &v), &out, &out_geom)
        },
    )
}

/// `nativeTakeReady(handle): ByteArray?`: payload bytes delivered in order since the last
/// call (segment by segment); null if there are none or the handle is bad.
#[jni_mangle("com.vaam.particlecode.NativeBridge")]
pub fn native_take_ready<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    handle: jlong,
) -> JByteArray<'local> {
    guarded(
        &mut env,
        |_| JByteArray::default(),
        |env| match registry::with(handle, crate::scanner::Scanner::take_ready) {
            Ok(Some(bytes)) => env.byte_array_from_slice(&bytes),
            _ => Ok(JByteArray::default()),
        },
    )
}

/// `nativePayload(handle): ByteArray?`: the whole payload once the transfer is done, else null.
#[jni_mangle("com.vaam.particlecode.NativeBridge")]
pub fn native_payload<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    handle: jlong,
) -> JByteArray<'local> {
    guarded(
        &mut env,
        |_| JByteArray::default(),
        |env| match registry::with(handle, |s| s.payload().map(<[u8]>::to_vec)) {
            Ok(Some(bytes)) => env.byte_array_from_slice(&bytes),
            _ => Ok(JByteArray::default()),
        },
    )
}

/// Return `text` as a Java string, or null.
fn string_or_null<'local>(
    env: &mut Env<'local>,
    text: Option<String>,
) -> Result<JString<'local>, JniError> {
    match text {
        Some(text) => env.new_string(text),
        None => Ok(JString::default()),
    }
}

/// `nativeRejectReason(handle): String?`: why the last frame was rejected (for logs), or null.
#[jni_mangle("com.vaam.particlecode.NativeBridge")]
pub fn native_reject_reason<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    handle: jlong,
) -> JString<'local> {
    guarded(
        &mut env,
        |_| JString::default(),
        |env| {
            let text = registry::with(handle, |s| s.reject_reason().map(str::to_owned))
                .ok()
                .flatten();
            string_or_null(env, text)
        },
    )
}

/// `nativeHint(handle): String?`: advice for the user after the last frame, or null.
#[jni_mangle("com.vaam.particlecode.NativeBridge")]
pub fn native_hint<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    handle: jlong,
) -> JString<'local> {
    guarded(
        &mut env,
        |_| JString::default(),
        |env| {
            let text = registry::with(handle, |s| s.hint().map(str::to_owned))
                .ok()
                .flatten();
            string_or_null(env, text)
        },
    )
}

/// `nativeSessionId(handle): Int`: the transfer's session id (0..=65535), or -1 if none yet
/// or the handle is bad.
#[jni_mangle("com.vaam.particlecode.NativeBridge")]
pub fn native_session_id<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    handle: jlong,
) -> jint {
    guarded(
        &mut env,
        |_| -1,
        |_| {
            let id = registry::with(handle, |s| s.session_id()).ok().flatten();
            Ok(id.map_or(-1, jint::from))
        },
    )
}

/// `nativeVerifyLiveness(key, rolling, nowUnix, skew): Boolean`: whether `rolling` (0..=65535,
/// read off the particles) is the value for a second within `skew` seconds of `nowUnix`, for
/// the 32-byte session `key`. False for a null or wrong-length key or out-of-range numbers.
#[jni_mangle("com.vaam.particlecode.NativeBridge")]
pub fn native_verify_liveness<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    key: JByteArray<'local>,
    rolling: jint,
    now_unix: jlong,
    skew: jlong,
) -> bool {
    guarded(
        &mut env,
        |_| false,
        |env| {
            let (Ok(rolling), Ok(now), Ok(skew)) = (
                u16::try_from(rolling),
                u64::try_from(now_unix),
                u64::try_from(skew),
            ) else {
                return Ok(false);
            };
            let Some(key) = bytes_of(env, &key)? else {
                return Ok(false);
            };
            Ok(particlecode_ffi::api::liveness::liveness_verify(
                key, rolling, now, skew,
            ))
        },
    )
}
