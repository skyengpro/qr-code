package com.vaam.particlecode

import android.util.Size
import androidx.camera.core.CameraControl
import androidx.camera.core.FocusMeteringAction
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.SurfaceOrientedMeteringPointFactory
import androidx.camera.core.resolutionselector.AspectRatioStrategy
import androidx.camera.core.resolutionselector.ResolutionSelector
import androidx.camera.core.resolutionselector.ResolutionStrategy
import java.util.concurrent.Executor
import java.util.concurrent.TimeUnit

/**
 * A CameraX analyzer that hands each `YUV_420_888` frame to a [ParticleScanner] without
 * copying it, and reports results through a plain [Listener].
 *
 * Why native: copying 1080p frames (3 MB each at 30 fps) through Dart or even the JVM heap
 * is too costly on low-end phones. The analyzer passes the `ImageProxy` planes' direct
 * buffers to Rust, which decodes in place; only a few ints cross back. With
 * `STRATEGY_KEEP_ONLY_LATEST` CameraX drops frames while the decoder is busy, so the
 * pipeline always works on the newest frame and never queues.
 *
 * ## Rules
 *
 * * The image is **always closed** (a `finally`), also on errors and after completion,
 *   otherwise CameraX stalls after a few frames.
 * * [Listener] callbacks run on the analyzer's executor thread. Post to the main thread
 *   before touching views, and from a Flutter plugin send only progress to Dart.
 * * Use one single-thread executor for the `ImageAnalysis`; the scanner's lock would
 *   serialize multi-threaded use anyway, but concurrency only adds latency.
 * * After [Progress.Done] the analyzer stops decoding (images are still closed) until
 *   [restart].
 *
 * ```
 * val scanner = ParticleScanner()
 * val analysis = ParticleAnalyzer.newImageAnalysis()
 * val executor = Executors.newSingleThreadExecutor()
 * analysis.setAnalyzer(executor, ParticleAnalyzer(scanner, listener))
 * cameraProvider.bindToLifecycle(owner, CameraSelector.DEFAULT_BACK_CAMERA, analysis)
 * ```
 *
 * ## Focus at close range
 *
 * The code is read from a phone screen 15 to 30 cm away. CameraX defaults to continuous
 * autofocus (`CONTROL_AF_MODE_CONTINUOUS_PICTURE`), which usually settles, but at close
 * range it can hunt, and a blurred frame is useless (blur washes out the dot colors and
 * sizes). To help:
 *
 * * Do not lock focus at infinity or disable AF; leave continuous AF on.
 * * Call [refocusCenter] when the camera starts and again every couple of seconds while no
 *   code is locked ([ScanResult.locked] false): it triggers a one-shot AF/AE on the image
 *   center and then hands control back to continuous AF (`autoCancel`).
 * * Meter exposure on the center too ([refocusCenter] does), so a bright room does not
 *   under-expose the screen.
 * * Some phones cannot focus closer than ~10 cm (see `LENS_INFO_MINIMUM_FOCUS_DISTANCE` via
 *   Camera2Interop); tell the user to move back instead of closer when the hint is
 *   [Hint.HoldSteady] with a low [ScanResult.sharpness].
 * * [ScanResult.sharpness] is comparable between frames of the same code: if you keep a
 *   frame for something else, keep the sharpest.
 *
 * @param reportIntervalMs minimum time between callbacks for frames that bring nothing new
 *   (still searching, same progress). Frames that change the lock state, the hint or the
 *   progress, and the final frame, are always reported. 0 reports every frame.
 */
class ParticleAnalyzer(
    private val scanner: ParticleScanner,
    private val listener: Listener,
    private val reportIntervalMs: Long = DEFAULT_REPORT_INTERVAL_MS,
) : ImageAnalysis.Analyzer {

    /** Receives results on the analyzer thread. */
    interface Listener {
        /** One frame was analyzed (see `reportIntervalMs` for which frames are reported). */
        fun onResult(result: ScanResult)

        /**
         * Like [onResult], with the frame the result came from. [image] is valid **only during
         * this call** (the analyzer closes it right after): read `image.imageInfo.rotationDegrees`
         * and `image.imageInfo.sensorToBufferTransformMatrix` (copy the matrix) to map
         * [ScanResult.anchors] onto a preview. The default forwards to [onResult], so existing
         * listeners are unaffected; the analyzer calls only this overload.
         */
        fun onResult(result: ScanResult, image: ImageProxy) {
            onResult(result)
        }

        /**
         * A frame could not be analyzed: unsupported image format, inconsistent buffers, a
         * closed scanner or a native failure. The analyzer keeps going unless the scanner is
         * closed.
         */
        fun onError(error: Throwable) {}
    }

    @Volatile
    private var finished = false
    private var lastReportedAt = 0L
    private var last: ScanResult? = null

    /** True after the transfer completed; further frames are only closed. */
    val isFinished: Boolean get() = finished

    /** Decode again after [Progress.Done]. Also resets the scanner. */
    fun restart() {
        scanner.reset()
        finished = false
        last = null
    }

    override fun analyze(image: ImageProxy) {
        try {
            if (finished) return
            val result = try {
                scanner.feed(image)
            } catch (e: IllegalStateException) {
                finished = true // closed
                listener.onError(e)
                return
            } catch (e: IllegalArgumentException) {
                listener.onError(e)
                return
            } catch (e: ParticleCodeException) {
                listener.onError(e)
                return
            }
            if (result.isDone) finished = true
            if (shouldReport(result)) listener.onResult(result, image)
        } finally {
            image.close()
        }
    }

    private fun shouldReport(r: ScanResult): Boolean {
        val now = System.nanoTime() / NANOS_PER_MS
        val prev = last
        last = r
        val changed = prev == null ||
            r.isDone ||
            prev.locked != r.locked ||
            prev.hint != r.hint ||
            progressKey(prev.progress) != progressKey(r.progress)
        if (changed || now - lastReportedAt >= reportIntervalMs) {
            lastReportedAt = now
            return true
        }
        return false
    }

    private fun progressKey(p: Progress): Any = when (p) {
        is Progress.Accepted -> Triple(p.have, p.segmentsDone, p.segmentsTotal)
        is Progress.Rejected -> "rejected"
        is Progress.Done -> "done"
    }

    companion object {
        /** Default for `reportIntervalMs`. */
        const val DEFAULT_REPORT_INTERVAL_MS: Long = 100

        private const val NANOS_PER_MS = 1_000_000L

        /** Target analysis size: 1920x1080 is enough for the dots at arm's length. */
        val TARGET_SIZE: Size = Size(1920, 1080)

        /**
         * The recommended `ImageAnalysis`: newest frame only, `YUV_420_888` output, about
         * 1920x1080 (the closest higher, else lower, resolution the camera offers) in 16:9.
         * Bind it with `bindToLifecycle` and attach a [ParticleAnalyzer] with
         * [ImageAnalysis.setAnalyzer].
         */
        @JvmStatic
        fun newImageAnalysis(): ImageAnalysis {
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

        /** Attach a [ParticleAnalyzer] for [scanner] to [analysis], running on [executor]. */
        @JvmStatic
        fun attach(
            analysis: ImageAnalysis,
            executor: Executor,
            scanner: ParticleScanner,
            listener: Listener,
        ): ParticleAnalyzer = ParticleAnalyzer(scanner, listener).also { analysis.setAnalyzer(executor, it) }

        /**
         * One-shot autofocus and auto-exposure on the image center, then back to continuous
         * AF after [autoCancelSeconds]. Call it when the camera starts and periodically while
         * no code is locked. Safe to call often; CameraX replaces a pending action.
         */
        @JvmStatic
        @JvmOverloads
        fun refocusCenter(control: CameraControl, autoCancelSeconds: Long = 3) {
            // A 1x1 surface-oriented factory takes normalized (0..1) coordinates of the sensor
            // area the use cases see, which is what we want: the middle of the image.
            val point = SurfaceOrientedMeteringPointFactory(1f, 1f).createPoint(0.5f, 0.5f)
            val action = FocusMeteringAction.Builder(
                point,
                FocusMeteringAction.FLAG_AF or FocusMeteringAction.FLAG_AE,
            )
                .setAutoCancelDuration(autoCancelSeconds, TimeUnit.SECONDS)
                .build()
            control.startFocusAndMetering(action)
        }
    }
}
