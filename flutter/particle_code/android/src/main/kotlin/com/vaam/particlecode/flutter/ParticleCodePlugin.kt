package com.vaam.particlecode.flutter

import android.Manifest
import android.app.Activity
import android.content.pm.PackageManager
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import io.flutter.embedding.engine.plugins.FlutterPlugin
import io.flutter.embedding.engine.plugins.activity.ActivityAware
import io.flutter.embedding.engine.plugins.activity.ActivityPluginBinding
import io.flutter.plugin.common.EventChannel
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import io.flutter.plugin.common.PluginRegistry

/**
 * Android side of the `particle_code` plugin (contract: `flutter/particle_code/PROTOCOL.md`).
 *
 * * `particle_code/scanner` (methods): camera permission, start, stop, torch, releaseFrame.
 * * `particle_code/camera_frames` (events): camera frames by plane address, for the Dart decoder.
 * * `particle_code/display` (methods): full brightness and screen-on while a code is shown.
 * * `particle_code/preview` (platform view): a CameraX `PreviewView`.
 *
 * No decoder and no Rust here; see [ScannerSession] and [FrameStore].
 */
class ParticleCodePlugin :
    FlutterPlugin,
    ActivityAware,
    PluginRegistry.RequestPermissionsResultListener {

    private var scannerChannel: MethodChannel? = null
    private var displayChannel: MethodChannel? = null
    private var eventChannel: EventChannel? = null

    private val events = CameraEvents()
    private val frames = FrameStore()
    private val previews = PreviewRegistry()
    private val display = DisplayController()

    private var activity: Activity? = null
    private var activityBinding: ActivityPluginBinding? = null
    private var session: ScannerSession? = null
    private val pendingPermission = ArrayList<MethodChannel.Result>()

    // region FlutterPlugin

    override fun onAttachedToEngine(binding: FlutterPlugin.FlutterPluginBinding) {
        scannerChannel = MethodChannel(binding.binaryMessenger, SCANNER_CHANNEL).also {
            it.setMethodCallHandler { call, result -> onScannerCall(call, result) }
        }
        displayChannel = MethodChannel(binding.binaryMessenger, DISPLAY_CHANNEL).also {
            it.setMethodCallHandler { call, result -> onDisplayCall(call, result) }
        }
        eventChannel = EventChannel(binding.binaryMessenger, EVENT_CHANNEL).also {
            it.setStreamHandler(events)
        }
        previews.onChanged = { session?.onPreviewChanged() }
        previews.onTap = { view, x, y -> session?.focusAt(view, x, y) }
        binding.platformViewRegistry.registerViewFactory(PREVIEW_VIEW_TYPE, PreviewViewFactory(previews))
    }

    override fun onDetachedFromEngine(binding: FlutterPlugin.FlutterPluginBinding) {
        scannerChannel?.setMethodCallHandler(null)
        displayChannel?.setMethodCallHandler(null)
        eventChannel?.setStreamHandler(null)
        scannerChannel = null
        displayChannel = null
        eventChannel = null
        releaseActivity()
        previews.onChanged = null
        previews.onTap = null
        previews.clear()
        events.release()
        frames.releaseAll()
    }

    // endregion

    // region ActivityAware

    override fun onAttachedToActivity(binding: ActivityPluginBinding) {
        attachActivity(binding)
    }

    override fun onReattachedToActivityForConfigChanges(binding: ActivityPluginBinding) {
        attachActivity(binding)
    }

    override fun onDetachedFromActivityForConfigChanges() {
        // The activity is being recreated: unbind the camera but keep the decoder and its
        // progress; onReattachedToActivityForConfigChanges rebinds.
        activityBinding?.removeRequestPermissionsResultListener(this)
        activityBinding = null
        activity = null
        session?.detachActivity()
        display.detach(restore = false)
    }

    override fun onDetachedFromActivity() {
        releaseActivity()
    }

    private fun attachActivity(binding: ActivityPluginBinding) {
        activityBinding?.removeRequestPermissionsResultListener(this)
        activityBinding = binding
        binding.addRequestPermissionsResultListener(this)
        val a = binding.activity
        activity = a
        display.attach(a)
        session?.attachActivity(a)
    }

    /** Final detach (activity gone for good, or the engine goes away): free everything. */
    private fun releaseActivity() {
        activityBinding?.removeRequestPermissionsResultListener(this)
        activityBinding = null
        activity = null
        stopSession()
        display.detach(restore = true)
        completePermission(false)
    }

    private fun stopSession() {
        session?.stop()
        session = null
    }

    // endregion

    // region permission

    private fun requestPermission(result: MethodChannel.Result) {
        val a = activity
        if (a == null) {
            result.success(false)
            return
        }
        if (ContextCompat.checkSelfPermission(a, Manifest.permission.CAMERA) ==
            PackageManager.PERMISSION_GRANTED
        ) {
            result.success(true)
            return
        }
        pendingPermission.add(result)
        if (pendingPermission.size == 1) {
            ActivityCompat.requestPermissions(a, arrayOf(Manifest.permission.CAMERA), PERMISSION_REQUEST_CODE)
        }
    }

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray,
    ): Boolean {
        if (requestCode != PERMISSION_REQUEST_CODE) return false
        completePermission(grantResults.isNotEmpty() && grantResults[0] == PackageManager.PERMISSION_GRANTED)
        return true
    }

    private fun completePermission(granted: Boolean) {
        if (pendingPermission.isEmpty()) return
        val waiting = ArrayList(pendingPermission)
        pendingPermission.clear()
        waiting.forEach { it.success(granted) }
    }

    // endregion

    // region channels

    private fun onScannerCall(call: MethodCall, result: MethodChannel.Result) {
        when (call.method) {
            "requestPermission" -> requestPermission(result)
            "start" -> {
                val a = activity
                if (a == null) {
                    result.error("camera_error", "the plugin is not attached to an activity", null)
                    return
                }
                val torch = call.argument<Boolean>("torch") ?: false
                val existing = session
                val s = if (existing != null) {
                    existing
                } else {
                    // A new session: a frame held from the previous one is stale.
                    frames.releaseAll()
                    ScannerSession(a, events, previews, frames).also { session = it }
                }
                s.start(torch, result)
            }
            "stop" -> {
                stopSession()
                result.success(null)
            }
            "releaseFrame" -> {
                val id = (call.argument<Any>("id") as? Number)?.toLong()
                if (id == null) {
                    result.error("bad_arguments", "releaseFrame needs {\"id\": int}", null)
                } else {
                    val heldNanos = frames.release(id)
                    result.success(if (heldNanos == null) null else mapOf("heldNanos" to heldNanos))
                }
            }
            "setTorch" -> {
                val on = call.argument<Boolean>("on")
                if (on == null) {
                    result.error("bad_arguments", "setTorch needs {\"on\": bool}", null)
                } else {
                    result.success(session?.setTorch(on) ?: false)
                }
            }
            else -> result.notImplemented()
        }
    }

    private fun onDisplayCall(call: MethodCall, result: MethodChannel.Result) {
        when (call.method) {
            "setShowingCode" -> {
                val on = call.argument<Boolean>("on")
                if (on == null) {
                    result.error("bad_arguments", "setShowingCode needs {\"on\": bool}", null)
                } else {
                    display.setShowingCode(on)
                    result.success(null)
                }
            }
            else -> result.notImplemented()
        }
    }

    // endregion

    private companion object {
        const val SCANNER_CHANNEL = "particle_code/scanner"
        const val DISPLAY_CHANNEL = "particle_code/display"
        const val EVENT_CHANNEL = "particle_code/camera_frames"
        const val PREVIEW_VIEW_TYPE = "particle_code/preview"

        /** Must fit in 16 bits (ActivityCompat). */
        const val PERMISSION_REQUEST_CODE = 0x5043
    }
}
