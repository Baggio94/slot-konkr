package fyi.slot.konkr

import android.app.Activity
import android.opengl.GLES20
import android.opengl.GLSurfaceView
import android.os.Bundle
import android.util.Log
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.View
import javax.microedition.khronos.egl.EGLConfig
import javax.microedition.khronos.opengles.GL10
import kotlin.math.abs

/** Android owns EGL, lifecycle and physical input; Rust owns Slot rendering. */
class MainActivity : Activity() {
    companion object {
        init { System.loadLibrary("slot_android") }
        private const val TAG = "SlotKonkr"
        private val BUTTONS = setOf(
            KeyEvent.KEYCODE_DPAD_LEFT, KeyEvent.KEYCODE_DPAD_RIGHT,
            KeyEvent.KEYCODE_DPAD_UP, KeyEvent.KEYCODE_DPAD_DOWN,
            KeyEvent.KEYCODE_BUTTON_L1, KeyEvent.KEYCODE_BUTTON_R1,
            KeyEvent.KEYCODE_BUTTON_A, KeyEvent.KEYCODE_BUTTON_B
        )
    }

    external fun nativeSurfaceCreated(): Boolean
    external fun nativeSurfaceChanged(width: Int, height: Int)
    external fun nativeDrawFrame()
    external fun nativeKey(code: Int, pressed: Boolean)
    external fun nativeResetInput()

    private lateinit var view: GLSurfaceView
    private var startX = 0f
    private var startY = 0f

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        view = object : GLSurfaceView(this) {
            init {
                setEGLContextClientVersion(2)
                preserveEGLContextOnPause = true
                setRenderer(object : Renderer {
                    var ready = false
                    override fun onSurfaceCreated(gl: GL10?, config: EGLConfig?) {
                        ready = nativeSurfaceCreated()
                        if (!ready) Log.e(TAG, "Slot GLES initialization failed")
                    }
                    override fun onSurfaceChanged(gl: GL10?, width: Int, height: Int) {
                        if (ready) nativeSurfaceChanged(width, height)
                    }
                    override fun onDrawFrame(gl: GL10?) {
                        if (ready) {
                            nativeDrawFrame()
                        } else {
                            GLES20.glClearColor(0f, 0f, 0f, 1f)
                            GLES20.glClear(GLES20.GL_COLOR_BUFFER_BIT)
                        }
                    }
                })
                renderMode = RENDERMODE_CONTINUOUSLY
                isFocusableInTouchMode = true
                requestFocus()
            }

            override fun onTouchEvent(event: MotionEvent): Boolean {
                when (event.actionMasked) {
                    MotionEvent.ACTION_DOWN -> {
                        startX = event.x
                        startY = event.y
                        return true
                    }
                    MotionEvent.ACTION_UP -> {
                        val dx = event.x - startX
                        val dy = event.y - startY
                        val code = if (abs(dx) > 50 && abs(dx) > abs(dy)) {
                            if (dx < 0) KeyEvent.KEYCODE_DPAD_RIGHT else KeyEvent.KEYCODE_DPAD_LEFT
                        } else if (abs(dy) > 50) {
                            if (dy < 0) KeyEvent.KEYCODE_DPAD_DOWN else KeyEvent.KEYCODE_DPAD_UP
                        } else {
                            KeyEvent.KEYCODE_BUTTON_A
                        }
                        nativeKey(code, true)
                        nativeKey(code, false)
                        return true
                    }
                }
                return true
            }
        }
        setContentView(view)
        immersive()
    }

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        if (keyCode !in BUTTONS) return super.onKeyDown(keyCode, event)
        if (event.repeatCount == 0) nativeKey(keyCode, true)
        return true
    }

    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean {
        if (keyCode !in BUTTONS) return super.onKeyUp(keyCode, event)
        nativeKey(keyCode, false)
        return true
    }

    @Deprecated("Android 12 back key")
    override fun onBackPressed() {
        nativeKey(KeyEvent.KEYCODE_BUTTON_B, true)
        nativeKey(KeyEvent.KEYCODE_BUTTON_B, false)
    }

    override fun onPause() {
        view.onPause()
        nativeResetInput()
        super.onPause()
    }

    override fun onResume() {
        super.onResume()
        view.onResume()
        immersive()
    }

    private fun immersive() {
        @Suppress("DEPRECATION")
        window.decorView.systemUiVisibility = (
            View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY or
            View.SYSTEM_UI_FLAG_FULLSCREEN or
            View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or
            View.SYSTEM_UI_FLAG_LAYOUT_STABLE or
            View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or
            View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
        )
    }
}
