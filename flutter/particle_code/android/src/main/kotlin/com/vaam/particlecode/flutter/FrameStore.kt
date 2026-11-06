package com.vaam.particlecode.flutter

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

    /** Take [image] if no frame is in flight and return its id; otherwise close it and return -1. */
    fun offer(image: ImageProxy): Long {
        synchronized(lock) {
            if (held == null) {
                held = image
                heldId = NEXT_ID.incrementAndGet()
                return heldId
            }
        }
        image.close()
        return -1L
    }

    /** Whether the frame with [id] is still the one in flight. */
    fun isHeld(id: Long): Boolean = synchronized(lock) { held != null && heldId == id }

    /** Close the frame with [id]; other ids are ignored. */
    fun release(id: Long) {
        val image = synchronized(lock) {
            if (held != null && heldId == id) {
                val h = held
                held = null
                h
            } else {
                null
            }
        }
        image?.close()
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
