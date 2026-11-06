package com.vaam.particlecode.flutter

import android.os.Handler
import android.os.Looper
import io.flutter.plugin.common.EventChannel

/**
 * The `particle_code/camera_frames` stream. Events are plain maps (see `PROTOCOL.md`); values are
 * limited to what `StandardMessageCodec` supports: no `Float` (use `Double`).
 *
 * Producers run on the analyzer thread and call [post]; the sink itself is only touched on the
 * main thread.
 */
internal class CameraEvents : EventChannel.StreamHandler {
    private val main = Handler(Looper.getMainLooper())

    // Main thread only.
    private var sink: EventChannel.EventSink? = null

    override fun onListen(arguments: Any?, events: EventChannel.EventSink?) {
        sink = events
    }

    override fun onCancel(arguments: Any?) {
        sink = null
    }

    /** Run [block] on the main thread, after everything posted before it. */
    fun post(block: () -> Unit) {
        main.post(block)
    }

    /** Main thread. Returns whether a listener got the event. */
    fun emit(event: Map<String, Any?>): Boolean {
        val s = sink ?: return false
        s.success(event)
        return true
    }

    /** Main thread. */
    fun error(code: String, message: String) {
        emit(mapOf("type" to "error", "code" to code, "message" to message))
    }

    /** Main thread. */
    fun release() {
        sink = null
    }
}
