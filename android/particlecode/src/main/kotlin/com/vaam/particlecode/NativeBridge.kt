package com.vaam.particlecode

import java.nio.ByteBuffer

/**
 * The raw JNI surface of `libparticlecode_jni` (Rust crate `particlecode-jni`).
 *
 * Most code should use [ParticleScanner], which owns a handle, enforces the threading
 * rule and turns the result arrays into typed values. This object is public only so that
 * tests and unusual hosts can reach the natives.
 *
 * ## Threading and ownership
 *
 * * A handle (from [nativeNew]) is used by **one thread at a time**. The native side
 *   serializes concurrent calls on one handle so a violation cannot corrupt memory, but
 *   it is not a design to lean on: [ParticleScanner] holds a lock around every call.
 * * Handles are table keys, not pointers: `0`, a negative number, a freed or a made-up
 *   handle yields an error code ([ERR_NULL]) or a null/-1 result, never a crash.
 * * [nativeFree] is idempotent. Free a handle exactly when its owner is closed.
 * * No native method throws for bad arguments: they return `ERR_*` codes. A caught Rust
 *   panic is [ERR_PANIC]; discard the handle after one.
 *
 * ## Result arrays
 *
 * [nativeFeedYuv] and [nativeFeedYuvBytes] fill two **preallocated** arrays, so a frame
 * costs no JVM allocation. The constants below are the indices, and the Rust test suite
 * checks that every `const val` in this file equals its Rust twin in
 * `crates/particlecode-jni/src/layout.rs`.
 *
 * `out: IntArray(OUT_LEN)`:
 *
 * | index | name | value |
 * | --- | --- | --- |
 * | 0 | [OUT_LOCKED] | 1 if the three anchors were found |
 * | 1 | [OUT_PROFILE] | 0 S, 1 M, 2 L; -1 if the header was not read |
 * | 2 | [OUT_SHAPE] | shape id of the header, -1 if not read |
 * | 3 | [OUT_HAVE] | distinct symbols held, summed over segments |
 * | 4 | [OUT_NEED] | symbols needed if nothing is lost |
 * | 5 | [OUT_SEGMENTS_DONE] | segments decoded and verified |
 * | 6 | [OUT_SEGMENTS_TOTAL] | segments in the transfer |
 * | 7 | [OUT_LIVENESS_READ] | 1 if the particles were read |
 * | 8 | [OUT_LIVENESS_CONSISTENT] | 1 if the particles' seed byte matches the header |
 * | 9 | [OUT_SEED_LOW] | low byte of the packet seed shown by the particles |
 * | 10 | [OUT_ROLLING] | rolling HMAC value shown by the particles (0..65535) |
 * | 11 | [OUT_ANCHOR_COUNT] | valid anchors in `outGeom`: 0, 3 or 4 |
 * | 12 | [OUT_HINT] | one of the `HINT_*` codes |
 *
 * The counts (3..6) are progress after the frame and keep their last value across
 * rejected frames.
 *
 * `outGeom: FloatArray(GEOM_LEN)`: image-pixel anchors as `x, y` pairs from
 * [GEOM_ANCHORS] (top, lower right, lower left, center; unused pairs are 0) and the
 * sharpness score at [GEOM_SHARPNESS] (higher is sharper, 0 if not locked).
 *
 * Both arrays may be longer than required; only the head is written. On a negative
 * status they are left untouched.
 */
object NativeBridge {
    init {
        System.loadLibrary("particlecode_jni")
    }

    // ---- Status codes returned by nativeFeedYuv / nativeFeedYuvBytes ----

    /** The frame was not used (no code in view, unreadable, ...). */
    const val STATUS_REJECTED = 0

    /** The frame was accepted. */
    const val STATUS_ACCEPTED = 1

    /** The whole payload has arrived; fetch it with [nativePayload]. */
    const val STATUS_DONE = 2

    /** Null or freed handle, or a null buffer/array argument. */
    const val ERR_NULL = -1

    /** A panic was caught at the boundary (a bug); discard the handle. */
    const val ERR_PANIC = -2

    /** A size, stride or array length does not fit the arguments. */
    const val ERR_INVALID = -3

    /** A [ByteBuffer] argument is not a direct buffer. */
    const val ERR_NOT_DIRECT = -4

    /** A JNI call failed (out of memory, pending exception, ...). */
    const val ERR_JNI = -5

    // ---- Layout of `out` ----

    /** Length of the `out` array. */
    const val OUT_LEN = 13
    const val OUT_LOCKED = 0
    const val OUT_PROFILE = 1
    const val OUT_SHAPE = 2
    const val OUT_HAVE = 3
    const val OUT_NEED = 4
    const val OUT_SEGMENTS_DONE = 5
    const val OUT_SEGMENTS_TOTAL = 6
    const val OUT_LIVENESS_READ = 7
    const val OUT_LIVENESS_CONSISTENT = 8
    const val OUT_SEED_LOW = 9
    const val OUT_ROLLING = 10
    const val OUT_ANCHOR_COUNT = 11
    const val OUT_HINT = 12

    // ---- Layout of `outGeom` ----

    /** Length of the `outGeom` array. */
    const val GEOM_LEN = 9

    /** Index of `x0`; `y0` follows, then `x1, y1, x2, y2, x3, y3`. */
    const val GEOM_ANCHORS = 0
    const val GEOM_SHARPNESS = 8

    // ---- Hint codes (`out[OUT_HINT]`) ----

    const val HINT_NONE = 0
    const val HINT_POINT_AT_CODE = 1
    const val HINT_MOVE_BACK = 2
    const val HINT_HOLD_STEADY = 3
    const val HINT_LIGHTING = 4
    const val HINT_UPDATE_APP = 5
    const val HINT_OTHER = 6

    // ---- Natives ----

    /** A new decoder session; the handle is positive, 0 means failure. */
    @JvmStatic
    external fun nativeNew(): Long

    /** Releases a session. Idempotent; unknown handles are ignored. */
    @JvmStatic
    external fun nativeFree(handle: Long)

    /** Forgets the transfer and starts over. 0, or an `ERR_*` code. */
    @JvmStatic
    external fun nativeReset(handle: Long): Int

    /**
     * Analyzes one `YUV_420_888` camera frame **in place**.
     *
     * `y`, `u`, `v` must be direct buffers; they are read from their base address for their
     * full capacity (position and limit are ignored, so pass `slice()` of a buffer whose
     * position is not 0). For NV21/NV12 `u` and `v` may be two views of one interleaved
     * buffer (pixel stride 2). Returns `STATUS_*` or `ERR_*`.
     */
    @JvmStatic
    @Suppress("LongParameterList")
    external fun nativeFeedYuv(
        handle: Long,
        width: Int,
        height: Int,
        y: ByteBuffer?,
        u: ByteBuffer?,
        v: ByteBuffer?,
        yRowStride: Int,
        uvRowStride: Int,
        uvPixelStride: Int,
        out: IntArray?,
        outGeom: FloatArray?,
    ): Int

    /** [nativeFeedYuv] over `ByteArray` planes (copied), for tests and non-direct buffers. */
    @JvmStatic
    @Suppress("LongParameterList")
    external fun nativeFeedYuvBytes(
        handle: Long,
        width: Int,
        height: Int,
        y: ByteArray?,
        u: ByteArray?,
        v: ByteArray?,
        yRowStride: Int,
        uvRowStride: Int,
        uvPixelStride: Int,
        out: IntArray?,
        outGeom: FloatArray?,
    ): Int

    /** Payload bytes delivered in order since the last call, or null if there are none. */
    @JvmStatic
    external fun nativeTakeReady(handle: Long): ByteArray?

    /** The whole payload once [STATUS_DONE] was returned, else null. */
    @JvmStatic
    external fun nativePayload(handle: Long): ByteArray?

    /** Technical reason the last frame was rejected (for logs), or null. */
    @JvmStatic
    external fun nativeRejectReason(handle: Long): String?

    /** User-facing advice after the last frame (English), or null. */
    @JvmStatic
    external fun nativeHint(handle: Long): String?

    /** Session id of the transfer (0..65535), or -1 if no frame was accepted yet. */
    @JvmStatic
    external fun nativeSessionId(handle: Long): Int

    /**
     * Whether [rolling] (0..65535, as read off the particles) is the value for a second
     * within [skew] seconds of [nowUnix] for the 32-byte session [key]. False for a null or
     * wrong-length key or out-of-range numbers.
     */
    @JvmStatic
    external fun nativeVerifyLiveness(key: ByteArray?, rolling: Int, nowUnix: Long, skew: Long): Boolean
}
