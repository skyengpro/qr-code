package com.vaam.particlecode.flutter

import android.os.Handler
import android.os.Looper
import io.flutter.plugin.common.EventChannel

/**
 * The `particle_code/scan_events` stream. Events are plain maps (see `PROTOCOL.md`); values are
 * limited to what `StandardMessageCodec` supports: no `Float` (use `Double`), `ByteArray` becomes
 * `Uint8List`.
 *
 * Producers run on the analyzer thread and call [post]; the sink itself is only touched on the
 * main thread.
 */
internal class ScanEvents : EventChannel.StreamHandler {
    private val main = Handler(Looper.getMainLooper())

    // Main thread only.
    private var sink: EventChannel.EventSink? = null
    private var pendingDone: Map<String, Any?>? = null

    override fun onListen(arguments: Any?, events: EventChannel.EventSink?) {
        sink = events
        // The transfer can finish before Dart listens (hot restart, a late subscription). The
        // payload must not get lost, so the `done` event waits for a listener.
        val waiting = pendingDone
        if (waiting != null && events != null) {
            pendingDone = null
            events.success(waiting)
        }
    }

    override fun onCancel(arguments: Any?) {
        sink = null
    }

    /** Run [block] on the main thread, after everything posted before it. */
    fun post(block: () -> Unit) {
        main.post(block)
    }

    /** Main thread. Dropped when nobody listens. */
    fun emit(event: Map<String, Any?>) {
        sink?.success(event)
    }

    /** Main thread. Kept until a listener arrives, because the payload is the whole point. */
    fun emitDone(event: Map<String, Any?>) {
        val s = sink
        if (s != null) s.success(event) else pendingDone = event
    }

    /** Main thread. Forget an undelivered `done` (stop, reset). */
    fun clearPending() {
        pendingDone = null
    }

    /** Main thread. */
    fun release() {
        sink = null
        pendingDone = null
    }
}
