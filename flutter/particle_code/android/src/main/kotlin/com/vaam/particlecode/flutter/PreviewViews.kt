package com.vaam.particlecode.flutter

import android.annotation.SuppressLint
import android.content.Context
import android.view.MotionEvent
import android.view.View
import androidx.camera.view.PreviewView
import io.flutter.plugin.common.StandardMessageCodec
import io.flutter.plugin.platform.PlatformView
import io.flutter.plugin.platform.PlatformViewFactory

/**
 * The `PreviewView`s Flutter created for the `particle_code/preview` platform view, newest last.
 * The last one wins: it is the one the camera's `Preview` renders into. Main thread only.
 */
internal class PreviewRegistry {
    private val views = ArrayList<PreviewView>()

    /** The view the camera preview should render into, or null. */
    val current: PreviewView? get() = views.lastOrNull()

    /** Called after [current] may have changed. */
    var onChanged: (() -> Unit)? = null

    /** Called for a tap on a preview: view, x and y in view pixels. */
    var onTap: ((PreviewView, Float, Float) -> Unit)? = null

    fun add(view: PreviewView) {
        views.add(view)
        onChanged?.invoke()
    }

    fun remove(view: PreviewView) {
        views.remove(view)
        onChanged?.invoke()
    }

    fun clear() {
        views.clear()
        onChanged?.invoke()
    }
}

/** Creates `PreviewView`s for the `particle_code/preview` platform view type. */
internal class PreviewViewFactory(private val registry: PreviewRegistry) :
    PlatformViewFactory(StandardMessageCodec.INSTANCE) {

    override fun create(context: Context, viewId: Int, args: Any?): PlatformView =
        PreviewPlatformView(context, registry)
}

private class PreviewPlatformView(context: Context, private val registry: PreviewRegistry) : PlatformView {
    private val previewView = PreviewView(context)

    init {
        // COMPATIBLE renders through a TextureView. When Flutter embeds the native view into its own
        // view hierarchy and composites it with the Flutter surface (Dart uses AndroidView, so the
        // exact mode is the engine's choice; see android/README.md), a
        // SurfaceView (PERFORMANCE mode) punches a hole in the window and misbehaves when it is
        // clipped, transformed or overlapped by Flutter widgets, which is exactly what an overlay
        // on the scanner does. COMPATIBLE is also the mode in which PreviewView's coordinate
        // transforms are specified to be accurate, and the anchors depend on them.
        previewView.implementationMode = PreviewView.ImplementationMode.COMPATIBLE
        // Fill the view and crop the overflow. The anchor mapping accounts for this crop.
        previewView.scaleType = PreviewView.ScaleType.FILL_CENTER
        setUpTapToFocus()
        registry.add(previewView)
    }

    // The view does not handle clicks itself; it only reports where the finger lifted.
    @SuppressLint("ClickableViewAccessibility")
    private fun setUpTapToFocus() {
        previewView.setOnTouchListener { v, event ->
            if (event.actionMasked == MotionEvent.ACTION_UP) {
                registry.onTap?.invoke(previewView, event.x, event.y)
                v.performClick()
            }
            true // consume DOWN so that UP arrives
        }
    }

    override fun getView(): View = previewView

    override fun dispose() {
        previewView.setOnTouchListener(null)
        registry.remove(previewView)
    }
}
