package fyi.slot.konkr

import android.app.Activity
import android.content.Intent
import android.content.ActivityNotFoundException
import android.graphics.Color
import android.view.Gravity
import android.widget.FrameLayout
import android.widget.TextView
import android.widget.Toast
import android.net.Uri
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicInteger
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
        private const val FOLDER_REQUEST = 4701
        private const val PREFS = "slot_konkr_library"
        private const val ROM_ROOT = "rom_root_uri"
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
    external fun nativeSetLibrary(json: String): Int

    private val scanner = Executors.newSingleThreadExecutor()
    private val scanSerial = AtomicInteger()
    private lateinit var status: TextView

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
        val frame = FrameLayout(this)
        frame.addView(view, FrameLayout.LayoutParams(-1, -1))
        status = TextView(this).apply {
            setTextColor(Color.WHITE)
            setBackgroundColor(Color.argb(212, 20, 20, 24))
            textSize = 15f
            gravity = Gravity.CENTER
            setPadding(12, 12, 12, 12)
            setOnClickListener { openRomFolderPicker() }
        }
        frame.addView(
            status,
            FrameLayout.LayoutParams(-1, -2, Gravity.BOTTOM)
        )
        setContentView(frame)
        immersive()
        val saved = getSharedPreferences(PREFS, MODE_PRIVATE).getString(ROM_ROOT, null)
        if (saved != null) {
            val uri = Uri.parse(saved)
            if (contentResolver.persistedUriPermissions.any {
                    it.uri == uri && it.isReadPermission
                }) {
                scanFolder(uri)
            } else {
                status.text = "ROM folder permission expired — press START to choose it again"
            }
        } else {
            status.text = "Press START to select your ROMs folder  •  Demo carts only"
        }
    }

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        if (keyCode == KeyEvent.KEYCODE_BUTTON_START && event.repeatCount == 0) {
            openRomFolderPicker()
            return true
        }
        if (keyCode == KeyEvent.KEYCODE_BUTTON_SELECT && event.repeatCount == 0) {
            val saved = getSharedPreferences(PREFS, MODE_PRIVATE).getString(ROM_ROOT, null)
            if (saved != null) scanFolder(Uri.parse(saved)) else openRomFolderPicker()
            return true
        }
        if (keyCode in setOf(KeyEvent.KEYCODE_BUTTON_START, KeyEvent.KEYCODE_BUTTON_SELECT)) return true
        if (keyCode !in BUTTONS) return super.onKeyDown(keyCode, event)
        if (event.repeatCount == 0) nativeKey(keyCode, true)
        return true
    }

    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean {
        if (keyCode in setOf(KeyEvent.KEYCODE_BUTTON_START, KeyEvent.KEYCODE_BUTTON_SELECT)) return true
        if (keyCode !in BUTTONS) return super.onKeyUp(keyCode, event)
        nativeKey(keyCode, false)
        return true
    }

    @Deprecated("Android 12 back key")
    override fun onBackPressed() {
        nativeKey(KeyEvent.KEYCODE_BUTTON_B, true)
        nativeKey(KeyEvent.KEYCODE_BUTTON_B, false)
    }

    private fun openRomFolderPicker() {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
            )
        }
        try {
            @Suppress("DEPRECATION")
            startActivityForResult(intent, FOLDER_REQUEST)
        } catch (error: ActivityNotFoundException) {
            status.text = "Android folder picker unavailable: " + error.message
        }
    }

    @Deprecated("Android 12 SAF activity result")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != FOLDER_REQUEST || resultCode != RESULT_OK) return
        val uri = data?.data ?: return
        try {
            contentResolver.takePersistableUriPermission(
                uri, Intent.FLAG_GRANT_READ_URI_PERMISSION
            )
            getSharedPreferences(PREFS, MODE_PRIVATE).edit()
                .putString(ROM_ROOT, uri.toString()).apply()
            scanFolder(uri)
        } catch (error: SecurityException) {
            status.text = "Cannot keep ROM folder permission — choose another folder"
            Log.e(TAG, "ROM folder permission error", error)
        }
    }

    private fun scanFolder(uri: Uri) {
        val serial = scanSerial.incrementAndGet()
        status.visibility = View.VISIBLE
        status.text = "Scanning GB / GBC / GBA cartridges…"
        scanner.execute {
            try {
                val result = RomLibrary.scan(this, uri)
                if (scanSerial.get() != serial) return@execute
                val count = nativeSetLibrary(result.json)
                runOnUiThread {
                    if (isDestroyed || scanSerial.get() != serial) return@runOnUiThread
                    if (count < 0) {
                        status.text = "Library import failed — press SELECT to retry"
                    } else if (count == 0) {
                        status.text = "No .gb/.gbc/.gba games found — START choose another folder"
                    } else {
                        status.visibility = View.GONE
                        val note = if (result.truncated) " (limited to 5000)" else ""
                        Toast.makeText(this, "Slot loaded " + count + " carts" + note, Toast.LENGTH_LONG).show()
                    }
                }
            } catch (error: Exception) {
                Log.e(TAG, "SAF cartridge scan failed", error)
                runOnUiThread {
                    if (!isDestroyed && scanSerial.get() == serial) {
                        status.visibility = View.VISIBLE
                        status.text = "Cannot read ROM folder — press START to choose again"
                    }
                }
            }
        }
    }

    override fun onDestroy() {
        scanSerial.incrementAndGet()
        scanner.shutdownNow()
        super.onDestroy()
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
