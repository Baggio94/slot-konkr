package fyi.slot.konkr

import android.app.Activity
import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioTrack
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import java.security.MessageDigest
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
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
            KeyEvent.KEYCODE_BUTTON_A, KeyEvent.KEYCODE_BUTTON_B,
            KeyEvent.KEYCODE_BUTTON_X, KeyEvent.KEYCODE_BUTTON_Y,
            KeyEvent.KEYCODE_BUTTON_START, KeyEvent.KEYCODE_BUTTON_SELECT
        )
    }

    external fun nativeSurfaceCreated(): Boolean
    external fun nativeSurfaceChanged(width: Int, height: Int)
    external fun nativeDrawFrame()
    external fun nativeKey(code: Int, pressed: Boolean)
    external fun nativeResetInput()
    external fun nativeSetLibrary(json: String): Int
    external fun nativeConfigure(dataDir: String, libraryDir: String)
    external fun nativePollLaunchUri(): String?
    external fun nativeGameReady(uri: String, path: String)
    external fun nativeGameError(uri: String, message: String)
    external fun nativeExitGame()
    external fun nativeSuspend()
    external fun nativeIsPlaying(): Boolean
    external fun nativeAudioSampleRate(): Int
    external fun nativeReadAudio(): ShortArray
    external fun nativePollMessage(): String?

    private val scanner = Executors.newSingleThreadExecutor()
    private val gameLoader = Executors.newSingleThreadExecutor()
    private val uiHandler = Handler(Looper.getMainLooper())
    private val audioRunning = AtomicBoolean(false)
    @Volatile private var resumed = false
    private var audioThread: Thread? = null
    private val pulse = object : Runnable {
        override fun run() {
            if (resumed && nativeIsPlaying()) {
                if (!audioRunning.get()) startAudio()
            } else if (audioRunning.get()) {
                stopAudio()
            }
            val request = nativePollLaunchUri()
            if (request != null) loadGameFromSaf(request)
            nativePollMessage()?.let { message ->
                Log.e(TAG, message)
                status.visibility = View.VISIBLE
                status.text = message + " — B returns to the shelf"
            }
            if (!isDestroyed) uiHandler.postDelayed(this, 100L)
        }
    }
    private val scanSerial = AtomicInteger()
    private lateinit var status: TextView

    private lateinit var view: GLSurfaceView
    private var startX = 0f
    private var startY = 0f

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        nativeConfigure(filesDir.absolutePath, applicationInfo.nativeLibraryDir)
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
                        // A single touch cannot be press+release in the same emulated frame.
                        if (nativeIsPlaying()) {
                            postDelayed({ nativeKey(code, false) }, 90L)
                        } else {
                            nativeKey(code, false)
                        }
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
        uiHandler.post(pulse)
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
        if (keyCode == KeyEvent.KEYCODE_BACK) {
            onBackPressed()
            return true
        }
        if (!nativeIsPlaying()) {
            if (keyCode == KeyEvent.KEYCODE_BUTTON_START && event.repeatCount == 0) {
                openRomFolderPicker()
                return true
            }
            if (keyCode == KeyEvent.KEYCODE_BUTTON_SELECT && event.repeatCount == 0) {
                val saved = getSharedPreferences(PREFS, MODE_PRIVATE).getString(ROM_ROOT, null)
                if (saved != null) scanFolder(Uri.parse(saved)) else openRomFolderPicker()
                return true
            }
            if (keyCode == KeyEvent.KEYCODE_BUTTON_START || keyCode == KeyEvent.KEYCODE_BUTTON_SELECT) return true
        }
        if (keyCode !in BUTTONS) return super.onKeyDown(keyCode, event)
        if (event.repeatCount == 0) nativeKey(keyCode, true)
        return true
    }

    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean {
        if (keyCode == KeyEvent.KEYCODE_BACK) return true
        if (!nativeIsPlaying() && keyCode in setOf(KeyEvent.KEYCODE_BUTTON_START, KeyEvent.KEYCODE_BUTTON_SELECT)) return true
        if (keyCode !in BUTTONS) return super.onKeyUp(keyCode, event)
        nativeKey(keyCode, false)
        return true
    }

    @Deprecated("Android 12 back key")
    override fun onBackPressed() {
        if (nativeIsPlaying()) {
            nativeExitGame()
        } else {
            nativeKey(KeyEvent.KEYCODE_BUTTON_B, true)
            nativeKey(KeyEvent.KEYCODE_BUTTON_B, false)
        }
    }

    private fun loadGameFromSaf(rawUri: String) {
        val uri = try { Uri.parse(rawUri) } catch (_: Exception) {
            nativeGameError(rawUri, "Invalid ROM URI")
            return
        }
        gameLoader.execute {
            try {
                val name = uri.lastPathSegment.orEmpty().lowercase()
                val ext = when {
                    name.endsWith(".gba") -> "gba"
                    name.endsWith(".gbc") -> "gbc"
                    name.endsWith(".gb") -> "gb"
                    else -> error("Unsupported ROM type")
                }
                val digest = MessageDigest.getInstance("SHA-256")
                    .digest(rawUri.toByteArray(Charsets.UTF_8))
                    .joinToString("") { byte -> "%02x".format(byte.toInt() and 0xff) }
                val dir = File(cacheDir, "rom-cache").apply { mkdirs() }
                val target = File(dir, "$digest.$ext")
                // Always open via Android SAF; no assumptions about physical filesystem paths.
                if (!target.isFile || target.length() < 0x150) {
                    val temp = File(dir, "$digest.tmp")
                    try {
                        contentResolver.openInputStream(uri)?.use { input ->
                            temp.outputStream().buffered().use { output ->
                                val buffer = ByteArray(64 * 1024)
                                var copied = 0L
                                while (true) {
                                    val n = input.read(buffer)
                                    if (n < 0) break
                                    copied += n
                                    require(copied <= 64L * 1024 * 1024) { "ROM exceeds 64 MiB" }
                                    output.write(buffer, 0, n)
                                }
                                require(copied >= 0x150) { "ROM too small" }
                            }
                        } ?: error("Cannot read this ROM")
                        check(temp.renameTo(target)) { "Cannot cache ROM" }
                    } finally { temp.delete() }
                }
                nativeGameReady(rawUri, target.absolutePath)
            } catch (error: Exception) {
                Log.e(TAG, "Could not prepare ROM", error)
                nativeGameError(rawUri, error.message ?: "Cannot open ROM")
            }
        }
    }

    private fun startAudio() {
        if (!audioRunning.compareAndSet(false, true)) return
        audioThread = Thread({
            var player: AudioTrack? = null
            var rate = 0
            try {
                while (audioRunning.get()) {
                    val wanted = nativeAudioSampleRate()
                    if (wanted <= 0) {
                        player?.pause()
                        SystemClock.sleep(35)
                        continue
                    }
                    if (player == null || rate != wanted) {
                        player?.release()
                        val minBytes = AudioTrack.getMinBufferSize(
                            wanted, AudioFormat.CHANNEL_OUT_STEREO, AudioFormat.ENCODING_PCM_16BIT
                        )
                        if (minBytes <= 0) {
                            SystemClock.sleep(100)
                            continue
                        }
                        player = AudioTrack.Builder()
                            .setAudioAttributes(AudioAttributes.Builder()
                                .setUsage(AudioAttributes.USAGE_GAME)
                                .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build())
                            .setAudioFormat(AudioFormat.Builder()
                                .setSampleRate(wanted)
                                .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                                .setChannelMask(AudioFormat.CHANNEL_OUT_STEREO).build())
                            .setBufferSizeInBytes(maxOf(16384, minBytes * 2))
                            .setTransferMode(AudioTrack.MODE_STREAM)
                            .build()
                        rate = wanted
                        player.play()
                    }
                    val samples = nativeReadAudio()
                    if (samples.isEmpty()) {
                        SystemClock.sleep(8)
                        continue
                    }
                    player.write(samples, 0, samples.size, AudioTrack.WRITE_BLOCKING)
                }
            } catch (error: Exception) {
                Log.e(TAG, "Android AudioTrack error", error)
            } finally {
                player?.stop()
                player?.release()
            }
        }, "SlotKONKR-Audio").apply { isDaemon = true; start() }
    }

    private fun stopAudio() {
        audioRunning.set(false)
        audioThread?.interrupt()
        audioThread = null
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
        gameLoader.shutdownNow()
        uiHandler.removeCallbacks(pulse)
        stopAudio()
        super.onDestroy()
    }

    override fun onPause() {
        resumed = false
        stopAudio()
        // Preserve SRAM and automatic resume state while the GL context is still current.
        val complete = CountDownLatch(1)
        view.queueEvent { try { nativeSuspend() } finally { complete.countDown() } }
        try {
            if (!complete.await(2, TimeUnit.SECONDS)) Log.w(TAG, "Suspend save timed out")
        } catch (_: InterruptedException) {
            Thread.currentThread().interrupt()
        }
        view.onPause()
        nativeResetInput()
        super.onPause()
    }

    override fun onResume() {
        super.onResume()
        resumed = true
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
