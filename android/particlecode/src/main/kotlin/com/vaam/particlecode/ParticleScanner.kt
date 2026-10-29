package com.vaam.particlecode

import android.graphics.ImageFormat
import androidx.camera.core.ImageProxy
import java.nio.ByteBuffer

/** A point in camera-image pixels (origin top-left, y down). */
data class Anchor(val x: Float, val y: Float)

/** Dot-field size of a frame's header. */
enum class CodeProfile { S, M, L }

/** Short advice for the user, derived from the last frame. Localize it in the app. */
enum class Hint(internal val code: Int) {
    None(NativeBridge.HINT_NONE),

    /** No code found: point the camera at it. */
    PointAtCode(NativeBridge.HINT_POINT_AT_CODE),

    /** Part of the code is outside the image: move back. */
    MoveBack(NativeBridge.HINT_MOVE_BACK),

    /** The code was found but could not be read: hold still, face it. */
    HoldSteady(NativeBridge.HINT_HOLD_STEADY),

    /** Too dark or too much glare: change the angle or the light. */
    Lighting(NativeBridge.HINT_LIGHTING),

    /** The code uses a newer format than this build understands. */
    UpdateApp(NativeBridge.HINT_UPDATE_APP),

    /** Something else; [ScanResult.hintText] has the English text. */
    Other(NativeBridge.HINT_OTHER),
    ;

    internal companion object {
        fun fromCode(code: Int): Hint = entries.firstOrNull { it.code == code } ?: Other
    }
}

/**
 * What the particles (the six dots in the code's particle band) showed.
 *
 * The value is a rolling HMAC of the wall-clock second, so a recording or screenshot of the
 * code goes stale. Check it with [verify] against the session key (a sender-side secret
 * that the verifier also holds).
 */
sealed class Liveness {
    /** The particles could not be read in this frame. */
    object Unknown : Liveness() {
        override fun toString(): String = "Liveness.Unknown"
    }

    /**
     * The particles were read.
     *
     * @property seedLow low byte of the packet seed the particles encode.
     * @property rolling the 16-bit rolling value.
     * @property consistent whether [seedLow] matches the decoded header (a mismatch means a
     *   misread or a spliced frame).
     */
    data class Read(val seedLow: Int, val rolling: Int, val consistent: Boolean) : Liveness() {
        /**
         * Whether [rolling] is the value for a second within [skewSeconds] of [nowUnix] for
         * the 32-byte session [key]. Always false when [consistent] is false.
         */
        fun verify(key: ByteArray, nowUnix: Long, skewSeconds: Long = DEFAULT_SKEW_SECONDS): Boolean =
            consistent && NativeBridge.nativeVerifyLiveness(key, rolling, nowUnix, skewSeconds)
    }

    companion object {
        /** Camera latency plus clock drift between the two phones. */
        const val DEFAULT_SKEW_SECONDS: Long = 2
    }
}

/** Where the transfer stands after one frame. */
sealed class Progress {
    /** The frame was not used. [reason] is a technical message for logs. */
    data class Rejected(val reason: String?) : Progress()

    /** The frame was accepted (possibly as a duplicate). */
    data class Accepted(
        val have: Int,
        val need: Int,
        val segmentsDone: Int,
        val segmentsTotal: Int,
    ) : Progress() {
        /** Share of the needed symbols held, 0..1 (can exceed 1 while repair symbols arrive). */
        val fraction: Float get() = if (need > 0) (have.toFloat() / need).coerceAtMost(1f) else 0f
    }

    /** The whole payload arrived and passed its checksum. */
    class Done(val payload: ByteArray) : Progress() {
        override fun equals(other: Any?): Boolean = other is Done && payload.contentEquals(other.payload)

        override fun hashCode(): Int = payload.contentHashCode()

        override fun toString(): String = "Progress.Done(${payload.size} bytes)"
    }
}

/**
 * The outcome of analyzing one camera frame. A frame without a code is a normal result
 * ([progress] is [Progress.Rejected]), not an error.
 *
 * @property locked whether the three anchors were found (draw an overlay).
 * @property profile dot-field size once the header was read.
 * @property shapeId shape id once the header was read.
 * @property anchors anchors in image pixels: top, lower right, lower left, then the center
 *   mark if it was found. Empty when not [locked]. Coordinates are in the *unrotated* image
 *   frame (`ImageProxy.imageInfo.rotationDegrees` tells how to rotate for display).
 * @property sharpness sharpness score (higher is sharper), 0 when not locked.
 * @property liveness what the particles showed.
 * @property progress transfer progress.
 * @property hint advice for the user.
 * @property hintText the English text behind [hint], or null.
 */
data class ScanResult(
    val locked: Boolean,
    val profile: CodeProfile?,
    val shapeId: Int?,
    val anchors: List<Anchor>,
    val sharpness: Float,
    val liveness: Liveness,
    val progress: Progress,
    val hint: Hint,
    val hintText: String?,
) {
    /** True when this frame completed the transfer. */
    val isDone: Boolean get() = progress is Progress.Done
}

/** A native call returned an error code. */
class ParticleCodeException(val code: Int, message: String) : RuntimeException(message)

/**
 * One scan session: feed it camera frames until [Progress.Done].
 *
 * The scanner owns a native decoder handle. [close] releases it; close it when the scan
 * screen goes away. All methods are safe to call from any thread: a lock serializes them,
 * as the native side requires (one thread per handle at a time). In the intended design
 * only the camera's analyzer thread feeds, and the UI thread reads nothing from here.
 *
 * Camera memory is read in place (no copy) by the direct-buffer overloads.
 *
 * ```
 * ParticleScanner().use { scanner ->
 *     val r = scanner.feed(imageProxy)          // on the analyzer thread
 *     (r.progress as? Progress.Done)?.let { handle(it.payload) }
 * }
 * ```
 */
class ParticleScanner : AutoCloseable {
    private val lock = Any()
    private var handle: Long = NativeBridge.nativeNew()
    private val out = IntArray(NativeBridge.OUT_LEN)
    private val geom = FloatArray(NativeBridge.GEOM_LEN)

    init {
        if (handle == 0L) throw ParticleCodeException(NativeBridge.ERR_JNI, "could not create a decoder")
    }

    /** True once [close] was called. */
    val isClosed: Boolean get() = synchronized(lock) { handle == 0L }

    /**
     * Feed one frame given as direct buffers, as `ImageProxy` planes: [uvPixelStride] is 2 for
     * NV21/NV12 (then [u] and [v] are views of one interleaved buffer) and 1 for I420.
     *
     * A buffer whose position is not 0 is sliced first (a view, no copy). A non-direct buffer
     * falls back to a copying path. Throws [IllegalArgumentException] for sizes that do not fit
     * the buffers, [IllegalStateException] after [close], and [ParticleCodeException] for
     * other native failures.
     */
    @Suppress("LongParameterList")
    fun feed(
        width: Int,
        height: Int,
        y: ByteBuffer,
        u: ByteBuffer,
        v: ByteBuffer,
        yRowStride: Int,
        uvRowStride: Int,
        uvPixelStride: Int,
    ): ScanResult {
        if (!y.isDirect || !u.isDirect || !v.isDirect) {
            return feed(
                width, height, y.toByteArray(), u.toByteArray(), v.toByteArray(),
                yRowStride, uvRowStride, uvPixelStride,
            )
        }
        val yb = y.atStart()
        val ub = u.atStart()
        val vb = v.atStart()
        return synchronized(lock) {
            val status = NativeBridge.nativeFeedYuv(
                handle, width, height, yb, ub, vb, yRowStride, uvRowStride, uvPixelStride, out, geom,
            )
            finish(status)
        }
    }

    /** Feed one frame given as byte arrays (copied by the native side). */
    @Suppress("LongParameterList")
    fun feed(
        width: Int,
        height: Int,
        y: ByteArray,
        u: ByteArray,
        v: ByteArray,
        yRowStride: Int,
        uvRowStride: Int,
        uvPixelStride: Int,
    ): ScanResult = synchronized(lock) {
        val status = NativeBridge.nativeFeedYuvBytes(
            handle, width, height, y, u, v, yRowStride, uvRowStride, uvPixelStride, out, geom,
        )
        finish(status)
    }

    /**
     * Feed a CameraX `YUV_420_888` frame: the three planes go to Rust as they are. The caller
     * keeps ownership of [image] and closes it (the analyzer does).
     */
    fun feed(image: ImageProxy): ScanResult {
        require(image.format == ImageFormat.YUV_420_888) {
            "expected YUV_420_888, got format ${image.format}; " +
                "use ParticleAnalyzer.newImageAnalysis() to request it"
        }
        val planes = image.planes
        require(planes.size == 3) { "expected 3 planes, got ${planes.size}" }
        val (py, pu, pv) = planes
        require(pu.rowStride == pv.rowStride && pu.pixelStride == pv.pixelStride) {
            "U and V planes differ in stride"
        }
        return feed(
            image.width, image.height, py.buffer, pu.buffer, pv.buffer,
            py.rowStride, pu.rowStride, pu.pixelStride,
        )
    }

    /** Forget the transfer and start over (the same session id is accepted again). */
    fun reset() {
        synchronized(lock) { throwIfError(NativeBridge.nativeReset(handle)) }
    }

    /**
     * Payload bytes delivered in order since the last call, segment by segment, so large
     * payloads can be consumed while the transfer runs. After [Progress.Done] the
     * concatenation of everything taken equals the payload. Null if there are none.
     */
    fun takeReady(): ByteArray? = synchronized(lock) { NativeBridge.nativeTakeReady(handle) }

    /** The whole payload once the transfer is done, else null. */
    fun payload(): ByteArray? = synchronized(lock) { NativeBridge.nativePayload(handle) }

    /** Session id of the transfer being received, once a frame was accepted. */
    val sessionId: Int?
        get() = synchronized(lock) { NativeBridge.nativeSessionId(handle).takeIf { it >= 0 } }

    /** Releases the native decoder. Idempotent. */
    override fun close() {
        synchronized(lock) {
            if (handle != 0L) {
                NativeBridge.nativeFree(handle)
                handle = 0L
            }
        }
    }

    // Called with `lock` held.
    private fun finish(status: Int): ScanResult {
        if (status < 0) throw errorFor(status)
        return toResult(status)
    }

    private fun throwIfError(status: Int) {
        if (status < 0) throw errorFor(status)
    }

    private fun errorFor(status: Int): RuntimeException = when (status) {
        NativeBridge.ERR_NULL ->
            if (handle == 0L) IllegalStateException("scanner is closed")
            else IllegalArgumentException("null buffer or result array")
        NativeBridge.ERR_INVALID ->
            IllegalArgumentException("buffers, strides or sizes do not describe a frame")
        NativeBridge.ERR_NOT_DIRECT -> IllegalArgumentException("buffer is not direct")
        NativeBridge.ERR_PANIC -> ParticleCodeException(status, "native panic; discard this scanner")
        else -> ParticleCodeException(status, "native call failed ($status)")
    }

    private fun toResult(status: Int): ScanResult {
        val o = out
        val anchors = ArrayList<Anchor>(o[NativeBridge.OUT_ANCHOR_COUNT])
        for (i in 0 until o[NativeBridge.OUT_ANCHOR_COUNT]) {
            val at = NativeBridge.GEOM_ANCHORS + 2 * i
            anchors += Anchor(geom[at], geom[at + 1])
        }
        val hint = Hint.fromCode(o[NativeBridge.OUT_HINT])
        val liveness = if (o[NativeBridge.OUT_LIVENESS_READ] == 1) {
            Liveness.Read(
                seedLow = o[NativeBridge.OUT_SEED_LOW],
                rolling = o[NativeBridge.OUT_ROLLING],
                consistent = o[NativeBridge.OUT_LIVENESS_CONSISTENT] == 1,
            )
        } else {
            Liveness.Unknown
        }
        val progress = when (status) {
            NativeBridge.STATUS_ACCEPTED -> Progress.Accepted(
                have = o[NativeBridge.OUT_HAVE],
                need = o[NativeBridge.OUT_NEED],
                segmentsDone = o[NativeBridge.OUT_SEGMENTS_DONE],
                segmentsTotal = o[NativeBridge.OUT_SEGMENTS_TOTAL],
            )
            NativeBridge.STATUS_DONE -> Progress.Done(NativeBridge.nativePayload(handle) ?: ByteArray(0))
            else -> Progress.Rejected(NativeBridge.nativeRejectReason(handle))
        }
        return ScanResult(
            locked = o[NativeBridge.OUT_LOCKED] == 1,
            profile = when (o[NativeBridge.OUT_PROFILE]) {
                0 -> CodeProfile.S
                1 -> CodeProfile.M
                2 -> CodeProfile.L
                else -> null
            },
            shapeId = o[NativeBridge.OUT_SHAPE].takeIf { it >= 0 },
            anchors = anchors,
            sharpness = geom[NativeBridge.GEOM_SHARPNESS],
            liveness = liveness,
            progress = progress,
            hint = hint,
            hintText = if (hint == Hint.None) null else NativeBridge.nativeHint(handle),
        )
    }

    private companion object {
        /** A view starting at the buffer's position (the native side ignores position). */
        fun ByteBuffer.atStart(): ByteBuffer = if (position() == 0) this else slice()

        /** The remaining bytes as an array, without moving the position. */
        fun ByteBuffer.toByteArray(): ByteArray {
            val copy = duplicate()
            return ByteArray(copy.remaining()).also { copy.get(it) }
        }
    }
}
