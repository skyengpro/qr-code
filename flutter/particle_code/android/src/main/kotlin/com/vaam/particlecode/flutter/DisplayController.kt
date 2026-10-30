package com.vaam.particlecode.flutter

import android.app.Activity
import android.view.WindowManager

/**
 * `particle_code/display`: while a code is on screen, force full brightness and keep the screen
 * on; put both back afterwards. All window changes run on the UI thread. Idempotent.
 *
 * "Back" means the values the window had when the code appeared: the previous
 * `screenBrightness` (usually "follow the system", -1) and whether the app had already set
 * `FLAG_KEEP_SCREEN_ON`.
 */
internal class DisplayController {
    /** What the caller asked for. Survives an activity change. */
    private var wanted = false

    private var activity: Activity? = null

    /** The activity whose window currently carries our changes. */
    private var appliedTo: Activity? = null
    private var savedBrightness = WindowManager.LayoutParams.BRIGHTNESS_OVERRIDE_NONE
    private var savedKeepOn = false

    fun setShowingCode(on: Boolean) {
        wanted = on
        sync()
    }

    /** A (possibly new) activity is available. */
    fun attach(newActivity: Activity) {
        activity = newActivity
        sync()
    }

    /**
     * The activity went away. [restore] false is a configuration change: the same or a new
     * activity follows, so keep what was asked for and re-apply it on [attach]. True is final:
     * put the window back and forget the request.
     */
    fun detach(restore: Boolean) {
        if (restore) {
            wanted = false
            sync()
            appliedTo = null
        }
        activity = null
    }

    private fun sync() {
        val a = activity ?: return
        a.runOnUiThread {
            val applied = appliedTo
            if (applied != null && applied !== a) {
                // Our changes sit on a window that is gone; nothing to undo there.
                appliedTo = null
            }
            if (wanted && appliedTo == null) {
                apply(a)
                appliedTo = a
            } else if (!wanted && appliedTo != null) {
                restore(a)
                appliedTo = null
            }
        }
    }

    private fun apply(a: Activity) {
        val window = a.window
        val params = window.attributes
        savedBrightness = params.screenBrightness
        savedKeepOn = (params.flags and WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON) != 0
        params.screenBrightness = WindowManager.LayoutParams.BRIGHTNESS_OVERRIDE_FULL
        window.attributes = params
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
    }

    private fun restore(a: Activity) {
        val window = a.window
        val params = window.attributes
        params.screenBrightness = savedBrightness
        window.attributes = params
        if (!savedKeepOn) window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
    }
}
