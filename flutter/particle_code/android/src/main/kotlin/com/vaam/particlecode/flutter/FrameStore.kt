package com.vaam.particlecode.flutter

import android.os.SystemClock
import androidx.camera.core.ImageProxy
import java.util.concurrent.atomic.AtomicLong

/**
 * The single camera frame in flight (PROTOCOL.md, "Frame ownership"). The [ImageProxy] stays open,
 * so its planes stay valid, until Dart calls `releaseFrame` with the frame's id. Newly delivered
 * frames are closed at once while one is held.
 *
 * Outlives sessions: `stop` does not close an in-flight frame. Thread-safe: [offer] runs on the
 * analyzer thread, [release] and [releaseAll] on the main thread.
 */
internal class FrameStore {
    private val lock = Any()
    private var held: ImageProxy? = null
    private var heldId = 0L
    private var heldReceivedNanos = 0L

    /**
     * Take [image] if no frame is in flight and return its id; otherwise close it and return -1.
     * [receivedNanos] (`SystemClock.elapsedRealtimeNanos()` at receipt) is kept for [release].
     */
    fun offer(image: ImageProxy, receivedNanos: Long): Long {
        synchronized(lock) {
            if (held == null) {
                held = image
                heldId = NEXT_ID.incrementAndGet()
                heldReceivedNanos = receivedNanos
                return heldId
            }
        }
        image.close()
        return -1L
    }

    /** Whether the frame with [id] is still the one in flight. */
    fun isHeld(id: Long): Boolean = synchronized(lock) { held != null && heldId == id }

    /**
     * Close the frame with [id] and return the nanoseconds it was held (receipt to now, both
     * `elapsedRealtimeNanos`); null (ignored) for other ids.
     */
    fun release(id: Long): Long? {
        var received = 0L
        val image = synchronized(lock) {
            if (held != null && heldId == id) {
                val h = held
                held = null
                received = heldReceivedNanos
                h
            } else {
                null
            }
        }
        if (image == null) return null
        val heldNanos = SystemClock.elapsedRealtimeNanos() - received
        image.close()
        return heldNanos
    }

    /** Close whatever is held (new session, engine gone). */
    fun releaseAll() {
        val image = synchronized(lock) {
            val h = held
            held = null
            h
        }
        image?.close()
    }

    private companion object {
        val NEXT_ID = AtomicLong(0)
    }
}
