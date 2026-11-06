package com.vaam.particlecode.flutter

import android.Manifest
import android.app.Activity
import android.content.pm.PackageManager
import android.graphics.ImageFormat
import android.graphics.Matrix
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.util.Size
import androidx.camera.core.Camera
import androidx.camera.core.CameraSelector
import androidx.camera.core.CameraState
import androidx.camera.core.FocusMeteringAction
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.core.SurfaceOrientedMeteringPointFactory
import androidx.camera.core.UseCase
import androidx.camera.core.resolutionselector.AspectRatioStrategy
import androidx.camera.core.resolutionselector.ResolutionSelector
import androidx.camera.core.resolutionselector.ResolutionStrategy
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.core.content.ContextCompat
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LiveData
import androidx.lifecycle.Observer
import com.google.common.util.concurrent.ListenableFuture
import io.flutter.plugin.common.MethodChannel
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

/**
 * One camera run: CameraX preview and analysis bound to the activity. Each analysis frame that
 * finds the [FrameStore] empty is kept open and announced to Dart as a `frame` event with its
 * plane addresses; Dart decodes it (Rust, in place) and calls `releaseFrame`.
 *
 * All methods except the analyzer run on the main thread.
 *
 * Lifecycle: [start] binds, [stop] unbinds and releases the camera (the session is dead after
 * that; an in-flight frame stays in the [FrameStore] until released), [detachActivity] /
 * [attachActivity] bridge an activity change (rotation, theme).
 */
internal class ScannerSession(
    private var activity: Activity,
    private val events: CameraEvents,
    private val previews: PreviewRegistry,
    private val frames: FrameStore,
) {
    private enum class State { IDLE, STARTING, RUNNING, CLOSED }

    private val main = Handler(Looper.getMainLooper())

    // Runs the analyzer. Shut down in [stop].
    private val executor: ExecutorService = Executors.newSingleThreadExecutor { r ->
        Thread(r, "particlecode-analysis").apply { isDaemon = true }
    }

    private var state = State.IDLE
    private var suspended = false
    private var torch = false
    private val startWaiters = ArrayList<MethodChannel.Result>()

    private var provider: ProcessCameraProvider? = null
    private var camera: Camera? = null
    private var preview: Preview? = null
    private var analysis: ImageAnalysis? = null
    private var cameraStates: LiveData<CameraState>? = null

    /** Set by [stop]; analyzer callbacks that still arrive close their frames. */
    @Volatile
    private var closed = false

    private val stateObserver = Observer<CameraState> { cameraState ->
        val code = cameraState.error?.code ?: return@Observer
        // Recoverable errors (camera in use, ...) are retried by CameraX; report the final ones.
        if (code == CameraState.ERROR_CAMERA_DISABLED ||
            code == CameraState.ERROR_CAMERA_FATAL_ERROR ||
            code == CameraState.ERROR_DO_NOT_DISTURB_MODE_ENABLED
        ) {
            events.error("camera_error", "camera error $code")
        }
    }

    private val refocus = object : Runnable {
        override fun run() {
            val cam = camera ?: return
            refocusCenter(cam)
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
        if (!DirectBuffers.available) {
            result.error("camera_error", "libparticle_code_jni.so could not be loaded", null)
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

    /**
     * Unbind and release the camera. The session cannot be started again. A frame in flight stays
     * in the [FrameStore] until Dart releases it (or the next `start`).
     */
    fun stop() {
        if (state == State.CLOSED) return
        val wasStarting = state == State.STARTING
        state = State.CLOSED
        closed = true
        if (wasStarting) completeWaitersWithError("camera_error", "start was cancelled by stop")
        unbindUseCases()
        executor.shutdown()
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

    /** The activity is being replaced (configuration change): unbind. */
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
            events.error("camera_error", t.toString())
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

        // The preview shares the analysis' 16:9 so that both show the same part of the sensor.
        val newPreview = Preview.Builder()
            .setResolutionSelector(
                ResolutionSelector.Builder()
                    .setAspectRatioStrategy(AspectRatioStrategy.RATIO_16_9_FALLBACK_AUTO_STRATEGY)
                    .build(),
            )
            .build()
        newPreview.setSurfaceProvider(previews.current?.surfaceProvider)
        val newAnalysis = newImageAnalysis()
        newAnalysis.setAnalyzer(executor, analyzer)

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
        refocusCenter(cam)
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

    private fun newImageAnalysis(): ImageAnalysis {
        val selector = ResolutionSelector.Builder()
            .setAspectRatioStrategy(AspectRatioStrategy.RATIO_16_9_FALLBACK_AUTO_STRATEGY)
            .setResolutionStrategy(
                ResolutionStrategy(TARGET_SIZE, ResolutionStrategy.FALLBACK_RULE_CLOSEST_HIGHER_THEN_LOWER),
            )
            .build()
        return ImageAnalysis.Builder()
            .setResolutionSelector(selector)
            .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
            .setOutputImageFormat(ImageAnalysis.OUTPUT_IMAGE_FORMAT_YUV_420_888)
            .build()
    }

    /**
     * One-shot autofocus and auto-exposure on the image center, then back to continuous AF. The
     * code is read from a phone screen at close range, where continuous AF can hunt.
     */
    private fun refocusCenter(cam: Camera) {
        try {
            val point = SurfaceOrientedMeteringPointFactory(1f, 1f).createPoint(0.5f, 0.5f)
            val action = FocusMeteringAction.Builder(
                point,
                FocusMeteringAction.FLAG_AF or FocusMeteringAction.FLAG_AE,
            )
                .setAutoCancelDuration(AUTO_CANCEL_SECONDS, TimeUnit.SECONDS)
                .build()
            cam.cameraControl.startFocusAndMetering(action)
        } catch (e: RuntimeException) {
            Log.w(TAG, "refocus failed", e)
        }
    }

    // endregion

    // region frames

    /** Analyzer thread. Keeps the frame open and announces it, or closes it. */
    private val analyzer = ImageAnalysis.Analyzer { image -> onFrame(image) }

    private fun onFrame(image: ImageProxy) {
        if (closed) {
            image.close()
            return
        }
        val id = frames.offer(image) // closes the image when a frame is already in flight
        if (id < 0) return
        try {
            val planes = image.planes
            if (image.format != ImageFormat.YUV_420_888 || planes.size != 3) {
                throw IllegalStateException("unexpected image format ${image.format}")
            }
            val y = planes[0].buffer
            val u = planes[1].buffer
            val v = planes[2].buffer
            val info = image.imageInfo
            val event = HashMap<String, Any?>(16)
            event["type"] = "frame"
            event["id"] = id
            event["width"] = image.width
            event["height"] = image.height
            event["yAddress"] = DirectBuffers.readableAddress(y)
            event["yLen"] = y.remaining()
            event["uAddress"] = DirectBuffers.readableAddress(u)
            event["uLen"] = u.remaining()
            event["vAddress"] = DirectBuffers.readableAddress(v)
            event["vLen"] = v.remaining()
            event["yStride"] = planes[0].rowStride
            event["uvStride"] = planes[1].rowStride
            event["uvPixelStride"] = planes[1].pixelStride
            val geometry = FrameGeometry(
                width = image.width,
                height = image.height,
                rotationDegrees = info.rotationDegrees,
                sensorToBuffer = Matrix(info.sensorToBufferTransformMatrix),
            )
            events.post { emitFrame(id, event, geometry) }
        } catch (t: Throwable) {
            Log.w(TAG, "could not read a camera frame", t)
            events.post {
                frames.release(id)
                events.error("camera_error", "could not read a camera frame: $t")
            }
        }
    }

    /**
     * Main thread. Adds `toPreview` and sends the event. If the frame was released meanwhile
     * (a new `start`), nothing is sent: Dart must never see an id that is no longer valid. If
     * nobody listens, the frame is released at once.
     */
    private fun emitFrame(id: Long, event: HashMap<String, Any?>, geometry: FrameGeometry) {
        if (!frames.isHeld(id)) return
        try {
            event["toPreview"] = toPreview(geometry).map { it.toDouble() }
            if (events.emit(event)) return
        } catch (t: Throwable) {
            Log.w(TAG, "could not send a camera frame", t)
        }
        frames.release(id)
    }

    /** What the analyzer thread knows about one frame, copied out of the [ImageProxy]. */
    private class FrameGeometry(
        val width: Int,
        val height: Int,
        val rotationDegrees: Int,
        val sensorToBuffer: Matrix,
    )

    /**
     * The affine `[a, b, c, d, e, f]` mapping buffer pixels to 0..1 in the preview view:
     * `x' = a*x + b*y + c`, `y' = d*x + e*y + f`.
     *
     * With a laid-out [PreviewView] that has a stream, it is exact: `buffer -> sensor` (inverse of
     * the frame's `sensorToBufferTransformMatrix`), then `sensor -> view`
     * ([PreviewView.getSensorToViewTransform], which includes the display rotation and the
     * FILL_CENTER scale and crop), divided by the view size. Points the crop cut off land outside
     * 0..1. Without such a view it is the upright image: rotated clockwise by `rotationDegrees`
     * and divided by its size.
     */
    private fun toPreview(g: FrameGeometry): FloatArray {
        val view = previews.current
        if (view != null && view.width > 0 && view.height > 0) {
            val m = bufferToView(view, g)
            if (m != null) {
                m.postScale(1f / view.width, 1f / view.height)
                val v = FloatArray(9)
                m.getValues(v)
                return floatArrayOf(
                    v[Matrix.MSCALE_X], v[Matrix.MSKEW_X], v[Matrix.MTRANS_X],
                    v[Matrix.MSKEW_Y], v[Matrix.MSCALE_Y], v[Matrix.MTRANS_Y],
                )
            }
        }
        return uprightAffine(g)
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

    /** Buffer pixels to the upright image, 0..1; the image rotated clockwise by `rotationDegrees`. */
    private fun uprightAffine(g: FrameGeometry): FloatArray {
        val sx = 1f / g.width // x / width: 0..1 across the unrotated image
        val sy = 1f / g.height
        return when (((g.rotationDegrees % 360) + 360) % 360) {
            90 -> floatArrayOf(0f, -sy, 1f, sx, 0f, 0f) // (1 - y, x)
            180 -> floatArrayOf(-sx, 0f, 1f, 0f, -sy, 1f) // (1 - x, 1 - y)
            270 -> floatArrayOf(0f, sy, 0f, -sx, 0f, 1f) // (y, 1 - x)
            else -> floatArrayOf(sx, 0f, 0f, 0f, sy, 0f)
        }
    }

    // endregion

    private companion object {
        const val TAG = "ParticleCode"
        // Native no longer knows whether a code is locked, so refocus on a fixed, relaxed period
        // (longer than AUTO_CANCEL_SECONDS: continuous AF runs in between).
        const val REFOCUS_INTERVAL_MS = 4_000L
        const val AUTO_CANCEL_SECONDS = 3L
        val TARGET_SIZE = Size(1920, 1080)
    }
}
