package com.vaam.particlecode.flutter

import android.Manifest
import android.app.Activity
import android.content.pm.PackageManager
import android.graphics.Matrix
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.camera.core.Camera
import androidx.camera.core.CameraSelector
import androidx.camera.core.CameraState
import androidx.camera.core.FocusMeteringAction
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.core.UseCase
import androidx.camera.core.resolutionselector.AspectRatioStrategy
import androidx.camera.core.resolutionselector.ResolutionSelector
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.core.content.ContextCompat
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LiveData
import androidx.lifecycle.Observer
import com.google.common.util.concurrent.ListenableFuture
import com.vaam.particlecode.Hint
import com.vaam.particlecode.CodeProfile
import com.vaam.particlecode.Liveness
import com.vaam.particlecode.ParticleAnalyzer
import com.vaam.particlecode.ParticleCodeException
import com.vaam.particlecode.ParticleScanner
import com.vaam.particlecode.Progress
import com.vaam.particlecode.ScanResult
import io.flutter.plugin.common.MethodChannel
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

/**
 * One camera scan: CameraX preview and analysis bound to the activity, a native decoder
 * ([ParticleScanner]) fed on a single background thread, and the events sent to Dart.
 *
 * All methods except the analyzer callbacks run on the main thread. Camera frames never leave the
 * analyzer thread; only the small event maps cross to Dart.
 *
 * Lifecycle: [start] binds, [stop] unbinds and releases everything (the session is dead after
 * that), [detachActivity] / [attachActivity] bridge an activity change (rotation, theme) keeping the decoder and
 * its progress, [reset] swaps in a fresh decoder while the camera keeps running.
 */
internal class ScannerSession(
    private var activity: Activity,
    private val events: ScanEvents,
    private val previews: PreviewRegistry,
) {
    private enum class State { IDLE, STARTING, RUNNING, CLOSED }

    private val main = Handler(Looper.getMainLooper())

    // The only thread that touches the decoder. Shut down in [stop], after the decoder is closed.
    private val executor: ExecutorService = Executors.newSingleThreadExecutor { r ->
        Thread(r, "particlecode-analysis").apply { isDaemon = true }
    }

    private var state = State.IDLE
    private var suspended = false
    private var torch = false
    private val startWaiters = ArrayList<MethodChannel.Result>()

    private var pipeline: Pipeline? = null
    private var provider: ProcessCameraProvider? = null
    private var camera: Camera? = null
    private var preview: Preview? = null
    private var analysis: ImageAnalysis? = null
    private var cameraStates: LiveData<CameraState>? = null

    /** Written on the analyzer thread, read by the periodic refocus. */
    @Volatile
    private var locked = false

    private val stateObserver = Observer<CameraState> { cameraState ->
        val code = cameraState.error?.code ?: return@Observer
        // Recoverable errors (camera in use, ...) are retried by CameraX; report the final ones.
        if (code == CameraState.ERROR_CAMERA_DISABLED ||
            code == CameraState.ERROR_CAMERA_FATAL_ERROR ||
            code == CameraState.ERROR_DO_NOT_DISTURB_MODE_ENABLED
        ) {
            events.emit(
                mapOf(
                    "type" to "error",
                    "code" to "camera_error",
                    "message" to "camera error $code",
                ),
            )
        }
    }

    private val refocus = object : Runnable {
        override fun run() {
            val cam = camera ?: return
            if (!locked) {
                ParticleAnalyzer.refocusCenter(cam.cameraControl)
            }
            main.postDelayed(this, REFOCUS_INTERVAL_MS)
        }
    }

    // region commands

    /**
     * Bind the camera. [result] gets `null` once bound, or `permission_denied`, `no_camera` or
     * `camera_error`. Idempotent: starting a running session only applies [torch].
     */
    fun start(torch: Boolean, result: MethodChannel.Result) {
        this.torch = torch
        when (state) {
            State.CLOSED -> {
                result.error("camera_error", "the scanner was stopped", null)
                return
            }
            State.RUNNING -> {
                camera?.let { applyTorch(it) }
                result.success(null)
                return
            }
            State.STARTING -> {
                startWaiters.add(result)
                return
            }
            State.IDLE -> Unit
        }
        if (ContextCompat.checkSelfPermission(activity, Manifest.permission.CAMERA) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            result.error("permission_denied", "camera permission was not granted", null)
            return
        }
        if (activity !is LifecycleOwner) {
            result.error("camera_error", "the host activity is not a LifecycleOwner", null)
            return
        }
        try {
            if (pipeline == null) pipeline = Pipeline()
        } catch (t: Throwable) {
            // Typically UnsatisfiedLinkError: libparticlecode_jni.so is missing for this ABI.
            result.error("camera_error", "could not create the decoder: $t", null)
            return
        }
        state = State.STARTING
        startWaiters.add(result)
        val future: ListenableFuture<ProcessCameraProvider> = try {
            ProcessCameraProvider.getInstance(activity)
        } catch (t: Exception) {
            failStart("camera_error", "camera provider unavailable: $t")
            return
        }
        future.addListener({ onProvider(future) }, ContextCompat.getMainExecutor(activity))
    }

    /** Unbind and release everything. The session cannot be started again. */
    fun stop() {
        if (state == State.CLOSED) return
        val wasStarting = state == State.STARTING
        state = State.CLOSED
        if (wasStarting) completeWaitersWithError("camera_error", "start was cancelled by stop")
        unbindUseCases()
        pipeline?.let { retire(it) }
        pipeline = null
        events.clearPending()
        // Runs after the decoder-closing task queued by retire(); nothing is left to leak.
        executor.shutdown()
    }

    /** A fresh decoder, the camera keeps running. Drops all progress. */
    fun reset() {
        if (state == State.CLOSED) return
        val old = pipeline ?: return
        val fresh = try {
            Pipeline()
        } catch (t: Throwable) {
            Log.w(TAG, "reset: could not create a decoder", t)
            return
        }
        pipeline = fresh
        analysis?.setAnalyzer(executor, fresh.analyzer)
        retire(old)
        events.clearPending()
    }

    /** Switch the torch. Returns whether the camera has one (false when not running). */
    fun setTorch(on: Boolean): Boolean {
        torch = on
        val cam = camera ?: return false
        val has = cam.cameraInfo.hasFlashUnit()
        if (has) cam.cameraControl.enableTorch(on)
        return has
    }

    /** Tap on a preview: focus and meter there. */
    fun focusAt(view: PreviewView, x: Float, y: Float) {
        val cam = camera ?: return
        try {
            val point = view.meteringPointFactory.createPoint(x, y)
            val action = FocusMeteringAction.Builder(
                point,
                FocusMeteringAction.FLAG_AF or FocusMeteringAction.FLAG_AE,
            )
                .setAutoCancelDuration(AUTO_CANCEL_SECONDS, TimeUnit.SECONDS)
                .build()
            cam.cameraControl.startFocusAndMetering(action)
        } catch (e: RuntimeException) {
            Log.w(TAG, "tap to focus failed", e)
        }
    }

    /** The set of preview views changed: render into the newest one. */
    fun onPreviewChanged() {
        preview?.setSurfaceProvider(previews.current?.surfaceProvider)
    }

    /** The activity is being replaced (configuration change): unbind, keep the decoder. */
    fun detachActivity() {
        if (suspended) return
        suspended = true
        if (state == State.RUNNING) unbindUseCases()
    }

    /** The replacement activity arrived. */
    fun attachActivity(newActivity: Activity) {
        activity = newActivity
        if (!suspended) return
        suspended = false
        if (state != State.RUNNING) return
        val p = provider ?: return
        try {
            bind(p)
        } catch (t: Exception) {
            Log.w(TAG, "could not rebind the camera", t)
            events.emit(mapOf("type" to "error", "code" to "camera_error", "message" to t.toString()))
        }
    }

    // endregion

    // region camera

    private fun onProvider(future: ListenableFuture<ProcessCameraProvider>) {
        if (state != State.STARTING) return // stop() ran meanwhile and answered the waiters
        val p = try {
            future.get()
        } catch (e: Exception) {
            failStart("camera_error", "camera provider failed: $e")
            return
        }
        provider = p
        if (suspended) {
            // The activity is changing; attachActivity() binds. The caller only needs "started".
            state = State.RUNNING
            completeWaiters()
            return
        }
        try {
            if (!p.hasCamera(CameraSelector.DEFAULT_BACK_CAMERA)) {
                failStart("no_camera", "this device has no back camera")
                return
            }
            bind(p)
        } catch (e: androidx.camera.core.CameraInfoUnavailableException) {
            failStart("no_camera", "camera info unavailable: $e")
            return
        } catch (e: IllegalArgumentException) {
            // CameraX: no camera satisfies the selector.
            failStart("no_camera", "no usable camera: $e")
            return
        } catch (e: SecurityException) {
            failStart("permission_denied", "camera permission was revoked: $e")
            return
        } catch (e: Exception) {
            failStart("camera_error", "could not bind the camera: $e")
            return
        }
        state = State.RUNNING
        completeWaiters()
    }

    /** Bind preview and analysis to the activity. Throws on failure and leaves nothing bound. */
    private fun bind(p: ProcessCameraProvider) {
        val owner = activity as? LifecycleOwner
            ?: throw IllegalStateException("the host activity is not a LifecycleOwner")
        val current = pipeline ?: throw IllegalStateException("no decoder")

        // The preview shares the analysis' 16:9 so that both show the same part of the sensor.
        val newPreview = Preview.Builder()
            .setResolutionSelector(
                ResolutionSelector.Builder()
                    .setAspectRatioStrategy(AspectRatioStrategy.RATIO_16_9_FALLBACK_AUTO_STRATEGY)
                    .build(),
            )
            .build()
        newPreview.setSurfaceProvider(previews.current?.surfaceProvider)
        val newAnalysis = ParticleAnalyzer.newImageAnalysis()
        newAnalysis.setAnalyzer(executor, current.analyzer)

        val cam = try {
            p.bindToLifecycle(owner, CameraSelector.DEFAULT_BACK_CAMERA, newPreview, newAnalysis)
        } catch (e: Exception) {
            newAnalysis.clearAnalyzer()
            newPreview.setSurfaceProvider(null)
            throw e
        }
        preview = newPreview
        analysis = newAnalysis
        camera = cam

        applyTorch(cam)
        ParticleAnalyzer.refocusCenter(cam.cameraControl)
        main.removeCallbacks(refocus)
        main.postDelayed(refocus, REFOCUS_INTERVAL_MS)

        val liveData = cam.cameraInfo.cameraState
        cameraStates = liveData
        liveData.observeForever(stateObserver)
    }

    private fun unbindUseCases() {
        main.removeCallbacks(refocus)
        cameraStates?.removeObserver(stateObserver)
        cameraStates = null
        analysis?.clearAnalyzer()
        val useCases = listOfNotNull<UseCase>(preview, analysis)
        val p = provider
        if (p != null && useCases.isNotEmpty()) {
            try {
                p.unbind(*useCases.toTypedArray())
            } catch (e: Exception) {
                Log.w(TAG, "unbind failed", e)
            }
        }
        preview?.setSurfaceProvider(null)
        preview = null
        analysis = null
        camera = null
    }

    private fun applyTorch(cam: Camera) {
        if (torch && cam.cameraInfo.hasFlashUnit()) cam.cameraControl.enableTorch(true)
    }

    private fun failStart(code: String, message: String) {
        state = State.IDLE
        unbindUseCases()
        completeWaitersWithError(code, message)
    }

    private fun completeWaiters() {
        val waiting = ArrayList(startWaiters)
        startWaiters.clear()
        waiting.forEach { it.success(null) }
    }

    private fun completeWaitersWithError(code: String, message: String) {
        val waiting = ArrayList(startWaiters)
        startWaiters.clear()
        waiting.forEach { it.error(code, message, null) }
    }

    /** Stop using [p]; its decoder is closed on the analyzer thread after any frame in flight. */
    private fun retire(p: Pipeline) {
        p.closed = true
        try {
            executor.execute { p.scanner.close() }
        } catch (e: java.util.concurrent.RejectedExecutionException) {
            p.scanner.close()
        }
    }

    // endregion

    // region analysis

    /** A decoder plus its analyzer, and the state needed to turn its results into events. */
    private inner class Pipeline : ParticleAnalyzer.Listener {
        val scanner = ParticleScanner()
        val analyzer = ParticleAnalyzer(scanner, this)

        /** Set on the main thread by [retire]; stale results are then dropped. */
        @Volatile
        var closed = false

        // Analyzer thread only. Sticky across rejected frames, as the protocol says.
        private var have = 0
        private var need = 0
        private var segmentsDone = 0
        private var segmentsTotal = 0

        override fun onResult(result: ScanResult) = Unit

        override fun onResult(result: ScanResult, image: ImageProxy) {
            if (closed) return
            locked = result.locked

            var status = "rejected"
            var rejectReason: String? = null
            var payload: ByteArray? = null
            when (val progress = result.progress) {
                is Progress.Accepted -> {
                    status = "accepted"
                    have = progress.have
                    need = progress.need
                    segmentsDone = progress.segmentsDone
                    segmentsTotal = progress.segmentsTotal
                }
                is Progress.Done -> {
                    status = "done"
                    payload = progress.payload
                    if (need > 0) have = need
                    if (segmentsTotal > 0) segmentsDone = segmentsTotal
                }
                is Progress.Rejected -> rejectReason = progress.reason
            }

            val ready: ByteArray? = try {
                scanner.takeReady()
            } catch (e: IllegalStateException) {
                null // closed while we worked
            }
            val sessionId = if (payload != null) (scanner.sessionId ?: -1) else -1

            val frame = HashMap<String, Any?>(16)
            frame["type"] = "frame"
            frame["status"] = status
            frame["locked"] = result.locked
            frame["profile"] = when (result.profile) {
                CodeProfile.S -> 0
                CodeProfile.M -> 1
                CodeProfile.L -> 2
                null -> -1
            }
            frame["shapeId"] = result.shapeId ?: -1
            frame["have"] = have
            frame["need"] = need
            frame["segmentsDone"] = segmentsDone
            frame["segmentsTotal"] = segmentsTotal
            frame["liveness"] = when (val l = result.liveness) {
                is Liveness.Read -> mapOf(
                    "consistent" to l.consistent,
                    "seedLow" to l.seedLow,
                    "rolling" to l.rolling,
                )
                is Liveness.Unknown -> null
            }
            frame["sharpness"] = result.sharpness.toDouble()
            frame["hint"] = result.hintText
            frame["hintCode"] = hintCode(result.hint)
            frame["rejectReason"] = rejectReason

            // Anchor mapping needs the view (main thread) and this frame's geometry (valid only
            // inside this call), so capture the geometry now and map on the main thread.
            val raw = FloatArray(result.anchors.size * 2)
            result.anchors.forEachIndexed { i, a ->
                raw[2 * i] = a.x
                raw[2 * i + 1] = a.y
            }
            val geometry = if (raw.isNotEmpty()) {
                FrameGeometry(
                    width = image.width,
                    height = image.height,
                    rotationDegrees = image.imageInfo.rotationDegrees,
                    sensorToBuffer = Matrix(image.imageInfo.sensorToBufferTransformMatrix),
                )
            } else {
                null
            }

            val donePayload: ByteArray? = payload
            events.post {
                if (closed) return@post
                frame["anchors"] = mapAnchors(raw, geometry)
                events.emit(frame)
                if (ready != null && ready.isNotEmpty()) {
                    events.emit(mapOf("type" to "segment", "bytes" to ready))
                }
                if (donePayload != null) {
                    events.emitDone(
                        mapOf("type" to "done", "payload" to donePayload, "sessionId" to sessionId),
                    )
                }
            }
        }

        override fun onError(error: Throwable) {
            // Bad frames are normal in the first moments; a native panic is a bug worth a log.
            if (error is ParticleCodeException) Log.w(TAG, "decoder error ${error.code}: ${error.message}")
        }
    }

    /** What the analyzer thread knows about one frame, copied out of the [ImageProxy]. */
    private class FrameGeometry(
        val width: Int,
        val height: Int,
        val rotationDegrees: Int,
        val sensorToBuffer: Matrix,
    )

    /**
     * Anchors (x0, y0, x1, y1, ...) in image-buffer pixels to 0..1 in the preview view.
     *
     * With a laid-out [PreviewView] that has a stream, the mapping is exact:
     * `buffer -> sensor` (inverse of the analysis frame's `sensorToBufferTransformMatrix`), then
     * `sensor -> view` ([PreviewView.getSensorToViewTransform], which includes the display
     * rotation and the FILL_CENTER scale and crop), divided by the view size. Points the crop cut
     * off land outside 0..1 and are not clamped.
     *
     * Without such a view, the fallback is the upright analysis image itself: rotated by
     * `rotationDegrees` and divided by its size, as if the preview showed all of it.
     */
    private fun mapAnchors(raw: FloatArray, g: FrameGeometry?): List<Double> {
        if (g == null || raw.isEmpty()) return ArrayList()
        val view = previews.current
        if (view != null && view.width > 0 && view.height > 0) {
            val bufferToView = bufferToView(view, g)
            if (bufferToView != null) {
                val pts = raw.copyOf()
                bufferToView.mapPoints(pts)
                val w = view.width.toFloat()
                val h = view.height.toFloat()
                return List(pts.size) { i -> (pts[i] / (if (i % 2 == 0) w else h)).toDouble() }
            }
        }
        return uprightFallback(raw, g)
    }

    private fun bufferToView(view: PreviewView, g: FrameGeometry): Matrix? {
        return try {
            val sensorToView = view.sensorToViewTransform ?: return null
            val bufferToSensor = Matrix()
            if (!g.sensorToBuffer.invert(bufferToSensor)) return null
            bufferToSensor.postConcat(sensorToView)
            bufferToSensor
        } catch (e: RuntimeException) {
            Log.w(TAG, "preview transform unavailable", e)
            null
        }
    }

    private fun uprightFallback(raw: FloatArray, g: FrameGeometry): List<Double> {
        val w = g.width.toFloat()
        val h = g.height.toFloat()
        val out = ArrayList<Double>(raw.size)
        var i = 0
        while (i + 1 < raw.size) {
            val x = raw[i] / w // 0..1 across the unrotated image
            val y = raw[i + 1] / h
            // Rotating the image clockwise by r degrees (what ImageInfo.rotationDegrees means).
            val (rx, ry) = when (((g.rotationDegrees % 360) + 360) % 360) {
                90 -> (1f - y) to x
                180 -> (1f - x) to (1f - y)
                270 -> y to (1f - x)
                else -> x to y
            }
            out.add(rx.toDouble())
            out.add(ry.toDouble())
            i += 2
        }
        return out
    }

    // endregion

    private companion object {
        const val TAG = "ParticleCode"
        const val REFOCUS_INTERVAL_MS = 2_000L
        const val AUTO_CANCEL_SECONDS = 3L

        fun hintCode(hint: Hint): Int = when (hint) {
            Hint.None -> 0
            Hint.PointAtCode -> 1
            Hint.MoveBack -> 2
            Hint.HoldSteady -> 3
            Hint.Lighting -> 4
            Hint.UpdateApp -> 5
            Hint.Other -> 6
        }
    }
}
