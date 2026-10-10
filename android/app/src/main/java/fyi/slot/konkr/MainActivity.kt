package fyi.slot.konkr

import android.app.Activity
import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioTrack
import android.media.PlaybackParams
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.os.BatteryManager
import android.os.VibrationEffect
import android.os.Vibrator
import android.content.IntentFilter
import android.util.AtomicFile
import java.security.MessageDigest
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import android.content.Intent
import android.content.ActivityNotFoundException
import android.graphics.Color
import android.graphics.BitmapFactory
import java.io.ByteArrayOutputStream
import android.view.Gravity
import android.widget.FrameLayout
import android.widget.TextView
import android.widget.Toast
import android.net.Uri
import org.json.JSONObject
import org.json.JSONArray
import java.util.concurrent.ConcurrentHashMap
import android.provider.DocumentsContract
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicInteger
import android.opengl.GLES20
import android.opengl.GLSurfaceView
import android.os.Bundle
import android.util.Log
import android.view.KeyEvent
import android.view.View
import javax.microedition.khronos.egl.EGLConfig
import javax.microedition.khronos.opengles.GL10

/** Android owns EGL, lifecycle and physical input; Rust owns Slot rendering. */
class MainActivity : Activity() {
    companion object {
        init { System.loadLibrary("slot_android") }
        private const val TAG = "SlotKonkr"
        private const val FOLDER_REQUEST = 4701
        private const val BIOS_REQUEST = 4702
        private const val SAVE_REQUEST = 4703
        private const val STATE_REQUEST = 4704
        private const val THEME_REQUEST = 4705
        private const val WALLPAPER_REQUEST = 4706
        private const val LABEL_REQUEST = 4707
        private const val PREFS = "slot_konkr_library"
        private const val ROM_ROOT = "rom_root_uri"
        private const val BIOS_ROOT = "bios_root_uri"
        private const val SAVE_ROOT = "saves_root_uri"
        private const val STATE_ROOT = "states_root_uri"
        private const val ROM_CACHE_NAME = "rom-library-cache-v1.json"
        private const val ROM_CACHE_SCHEMA = 1
        private const val ROM_CACHE_MAX_BYTES = 8 * 1024 * 1024
        private val BUTTONS = setOf(
            KeyEvent.KEYCODE_DPAD_LEFT, KeyEvent.KEYCODE_DPAD_RIGHT,
            KeyEvent.KEYCODE_DPAD_UP, KeyEvent.KEYCODE_DPAD_DOWN,
            KeyEvent.KEYCODE_BUTTON_L1, KeyEvent.KEYCODE_BUTTON_R1,
            KeyEvent.KEYCODE_BUTTON_L2, KeyEvent.KEYCODE_BUTTON_R2,
            KeyEvent.KEYCODE_BUTTON_A, KeyEvent.KEYCODE_BUTTON_B,
            KeyEvent.KEYCODE_BUTTON_X, KeyEvent.KEYCODE_BUTTON_Y,
            KeyEvent.KEYCODE_BUTTON_START, KeyEvent.KEYCODE_BUTTON_SELECT,
            KeyEvent.KEYCODE_BUTTON_MODE
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
    external fun nativeAudioSpeedPermille(): Int
    external fun nativeFastAudioSupported(supported: Boolean)
    external fun nativeReadAudio(): ShortArray
    external fun nativePollMessage(): String?
    external fun nativeSystemStatus(clock: String, batteryPercent: Int, charging: Boolean)
    external fun nativePollUiAction(): Int
    external fun nativeReloadVisualAssets()
    external fun nativePollCartLabelUri(): String?
    external fun nativeReloadCartLabel(uri: String)
    external fun nativePollCartSfx(): Int
    external fun nativePollRumbleStrength(): Int
    external fun nativeCoreForUri(uri: String): String
    external fun nativePollSaveFlush(): String?

    private var pendingCartLabelUri: String? = null
    private lateinit var cartSounds: CartSounds
    private val scanner = Executors.newSingleThreadExecutor()
    private val gameLoader = Executors.newSingleThreadExecutor()
    private val activeTargets = ConcurrentHashMap<String, RetroArchStorage.Target>()
    private val uiHandler = Handler(Looper.getMainLooper())
    private val audioRunning = AtomicBoolean(false)
    private val vibrator by lazy { getSystemService(VIBRATOR_SERVICE) as? Vibrator }
    private var lastRumbleAt = 0L
    @Volatile private var resumed = false
    private var audioThread: Thread? = null
    private val pulse = object : Runnable {
        override fun run() {
            pollSystemStatus()
            pollRumble()
            if (resumed && nativeIsPlaying()) {
                if (!audioRunning.get()) startAudio()
            } else if (audioRunning.get()) {
                stopAudio()
            }
            // Flush requests before accepting a new ROM launch to preserve
            // save-before-load ordering on the single storage worker.
            var completed = nativePollSaveFlush()
            while (completed != null) {
                try {
                    val payload = JSONObject(completed)
                    val uri = payload.getString("uri")
                    val core = when (payload.getString("core")) {
                        "gpsp" -> "gpSP"
                        else -> "mGBA"
                    }
                    val target = activeTargets[uri]
                    if (target != null && target.core == core) {
                        gameLoader.execute {
                            val stamp = payload.optString("manual_stamp", "")
                            val failures = if (stamp.isNotEmpty()) {
                                RetroArchStorage.exportManualState(this@MainActivity, target, stamp)
                            } else {
                                RetroArchStorage.exportAfterSave(this@MainActivity, target)
                            }
                            if (failures.isNotEmpty()) {
                                runOnUiThread {
                                    if (!isDestroyed) Toast.makeText(this@MainActivity,
                                        failures.joinToString("; "), Toast.LENGTH_LONG).show()
                                }
                            }
                        }
                    }
                } catch (error: Exception) {
                    Log.e(TAG, "Invalid save sync notification", error)
                }
                completed = nativePollSaveFlush()
            }
            val request = nativePollLaunchUri()
            if (request != null) loadGameFromSaf(request)
            when (nativePollUiAction()) {
                1 -> openRomFolderPicker()
                2 -> refreshLibrary()
                3 -> checkRaOfflineProxy()
                4 -> openBiosFolderPicker()
                5 -> openSharedFolderPicker(SAVE_REQUEST)
                6 -> openSharedFolderPicker(STATE_REQUEST)
                7 -> openVisualPicker(THEME_REQUEST)
                8 -> openVisualPicker(WALLPAPER_REQUEST)
                9 -> resetVisualFile(THEME_REQUEST)
                10 -> resetVisualFile(WALLPAPER_REQUEST)
                11 -> openCartLabelPicker()
                12 -> removeSelectedCartLabel()
            }
            nativePollMessage()?.let { message ->
                Log.w(TAG, message)
                Toast.makeText(this@MainActivity, message, Toast.LENGTH_SHORT).show()
            }
            if (!isDestroyed) uiHandler.postDelayed(this, 100L)
        }
    }
    private val scanSerial = AtomicInteger()
    private var lastStatusPoll: Long = -15000L
    private lateinit var status: TextView

    private lateinit var view: GLSurfaceView

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        nativeConfigure(filesDir.absolutePath, applicationInfo.nativeLibraryDir)
        cartSounds = CartSounds(this)
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
                            // Drain on the render thread for frame-accurate insert/eject clicks.
                            var kind = nativePollCartSfx()
                            while (kind != 0) {
                                cartSounds.play(kind)
                                kind = nativePollCartSfx()
                            }
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


        }
        val frame = FrameLayout(this)
        frame.addView(view, FrameLayout.LayoutParams(-1, -1))
        status = TextView(this).apply {
            setTextColor(Color.WHITE)
            setBackgroundColor(Color.TRANSPARENT)
            textSize = 15f
            gravity = Gravity.CENTER
            setPadding(12, 12, 12, 12)
        }
        frame.addView(
            status,
            FrameLayout.LayoutParams(-1, -2, Gravity.TOP)
        )
        setContentView(frame)
        immersive()
        uiHandler.post(pulse)
        // Stage BIOS files before the first queued ROM launch, using the same
        // worker that loads ROMs; no race between BIOS import and core startup.
        getSharedPreferences(PREFS, MODE_PRIVATE).getString(BIOS_ROOT, null)
            ?.let { savedBios ->
                val uri = Uri.parse(savedBios)
                if (contentResolver.persistedUriPermissions.any { perm ->
                        perm.uri == uri && perm.isReadPermission
                    }) {
                    importBiosFolder(uri, persistSelection = false)
                } else {
                    Log.w(TAG, "BIOS folder access expired; choose it again")
                }
            }
        val saved = getSharedPreferences(PREFS, MODE_PRIVATE).getString(ROM_ROOT, null)
        if (saved != null) {
            val uri = Uri.parse(saved)
            if (contentResolver.persistedUriPermissions.any {
                    it.uri == uri && it.isReadPermission
                }) {
                if (!restoreCachedLibrary(uri)) scanFolder(uri)
            } else {
                status.text = "ROM folder permission expired — START to choose again"
            }
        } else {
            status.text = "Press START to open the menu and add your ROMs."
        }
    }

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        if (keyCode == KeyEvent.KEYCODE_BACK) {
            onBackPressed()
            return true
        }
        if (keyCode == KeyEvent.KEYCODE_BUTTON_MODE && event.repeatCount == 0) {
            Log.i(TAG, "KONKR MENU BTN_MODE down: Android code=" + keyCode +
                " scan=" + event.scanCode + " device=" + event.deviceId)
        }
        // START / SELECT on the shelf are now rendered by Slot itself.
        // Log any otherwise unrecognized physical key so the KONKR's top-round
        // key can be mapped from real device evidence, not a guessed keycode.
        if (keyCode !in BUTTONS) {
            if (event.repeatCount == 0) {
                Log.i(TAG, "Unmapped hardware key down: code=" + keyCode +
                    " name=" + KeyEvent.keyCodeToString(keyCode) +
                    " device=" + event.deviceId + " scan=" + event.scanCode)
            }
            return super.onKeyDown(keyCode, event)
        }
        if (event.repeatCount == 0) nativeKey(keyCode, true)
        return true
    }

    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean {
        if (keyCode == KeyEvent.KEYCODE_BACK) return true
        if (keyCode == KeyEvent.KEYCODE_BUTTON_MODE) {
            Log.i(TAG, "KONKR MENU BTN_MODE up: Android code=" + keyCode)
        }
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
        val queuedAt = SystemClock.elapsedRealtime()
        gameLoader.execute {
            val launchStart = SystemClock.elapsedRealtime()
            val queueWait = launchStart - queuedAt
            if (queueWait > 20) {
                Log.i(TAG, "Slot launch queue delay: ${queueWait}ms (waiting for preceding save exports)")
            }
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
                val romReadyAt = SystemClock.elapsedRealtime()
                val savedCore = nativeCoreForUri(rawUri)
                val core = if (savedCore == "gpsp" && ext == "gba") "gpSP" else "mGBA"
                val prefs = getSharedPreferences(PREFS, MODE_PRIVATE)
                val saves = prefs.getString(SAVE_ROOT, null)?.let(Uri::parse)
                val states = prefs.getString(STATE_ROOT, null)?.let(Uri::parse)
                val shared = RetroArchStorage.target(this, rawUri, core, saves, states)
                try {
                    RetroArchStorage.importBeforeLaunch(this@MainActivity, shared)
                } catch (error: Exception) {
                    Log.w(TAG, "RetroArch SAF import skipped, keeping private saves", error)
                    runOnUiThread {
                        if (!isDestroyed) Toast.makeText(this@MainActivity,
                            "RetroArch import unavailable — check Save/State folders",
                            Toast.LENGTH_LONG).show()
                    }
                }
                activeTargets[rawUri] = shared
                val readyAt = SystemClock.elapsedRealtime()
                Log.i(TAG, "Slot launch preparation: ROM cache=${romReadyAt - launchStart}ms, " +
                    "RetroArch import=${readyAt - romReadyAt}ms, total=${readyAt - launchStart}ms")
                nativeGameReady(rawUri, target.absolutePath)
            } catch (error: Exception) {
                Log.e(TAG, "Could not prepare ROM", error)
                nativeGameError(rawUri, error.message ?: "Cannot open ROM")
            }
        }
    }

    private fun pollRumble() {
        if (!resumed || !nativeIsPlaying()) return
        val strength = nativePollRumbleStrength()
        if (strength <= 4096) return
        val now = SystemClock.elapsedRealtime()
        if (now - lastRumbleAt < 100L) return
        lastRumbleAt = now
        val device = vibrator ?: return
        if (!device.hasVibrator()) return
        try {
            val intensity = (strength / 257).coerceIn(10, 255)
            device.vibrate(VibrationEffect.createOneShot(45L, intensity))
        } catch (error: RuntimeException) {
            Log.w(TAG, "KONKR rumble unavailable", error)
        }
    }

    private fun startAudio() {
        if (!audioRunning.compareAndSet(false, true)) return
        audioThread = Thread({
            var player: AudioTrack? = null
            var rate = 0
            var requestedSpeed = 0
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
                        requestedSpeed = 0
                        player.play()
                    }
                    val speed = nativeAudioSpeedPermille().coerceIn(1000, 6000)
                    if (speed != requestedSpeed) {
                        requestedSpeed = speed
                        try {
                            // Android time-stretches at the selected speed while preserving
                            // the ORIGINAL sample pitch (not chipmunk audio).
                            // If the Audio HAL cannot stretch, mute rather
                            // than silently playing an incorrect-pitch stream.
                            val params = PlaybackParams()
                                .setSpeed(speed / 1000f)
                                .setPitch(1.0f)
                                .setAudioFallbackMode(PlaybackParams.AUDIO_FALLBACK_MODE_MUTE)
                            player.playbackParams = params
                            nativeFastAudioSupported(true)
                            Log.i(TAG, "FF audio: speed=${speed / 1000f}x, pitch=1.0x")
                        } catch (error: Exception) {
                            Log.w(TAG, "Pitch-preserving FF unsupported; silent fallback", error)
                            nativeFastAudioSupported(false)
                            try {
                                player.playbackParams = PlaybackParams()
                                    .setSpeed(1.0f).setPitch(1.0f)
                            } catch (_: Exception) { }
                        }
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

    private fun checkRaOfflineProxy() {
        scanner.execute {
            val proxy = RaEndpoint.discover(applicationContext)
            val message = when {
                proxy == null -> "RAOfflineProxy unavailable"
                !proxy.running -> "RAOfflineProxy stopped"
                proxy.base() == null -> "RAOfflineProxy address invalid"
                else -> {
                    val state = when (proxy.online) {
                        true -> "Online"
                        false -> "Offline"
                        null -> "Connectivity unknown"
                    }
                    val pending = proxy.pendingAwards?.toString() ?: "not reported"
                    "RAOfflineProxy: $state (port ${proxy.port})\nPending awards: $pending"
                }
            }
            runOnUiThread {
                if (!isDestroyed) {
                    Toast.makeText(this, message, Toast.LENGTH_LONG).show()
                }
            }
        }
    }

    private fun pollSystemStatus() {
        val now = SystemClock.elapsedRealtime()
        if (now - lastStatusPoll < 15000L) return
        lastStatusPoll = now
        val clock = java.text.SimpleDateFormat("HH:mm", java.util.Locale.getDefault())
            .format(java.util.Date())
        val battery = registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED))
        val level = battery?.getIntExtra(BatteryManager.EXTRA_LEVEL, -1) ?: -1
        val scale = battery?.getIntExtra(BatteryManager.EXTRA_SCALE, -1) ?: -1
        val percent = if (level >= 0 && scale > 0) (100 * level / scale).coerceIn(0, 100)
            else -1
        val state = battery?.getIntExtra(BatteryManager.EXTRA_STATUS, -1) ?: -1
        val charging = state == BatteryManager.BATTERY_STATUS_CHARGING ||
            state == BatteryManager.BATTERY_STATUS_FULL
        nativeSystemStatus(clock, percent, charging)
    }

    private fun refreshLibrary() {
        val saved = getSharedPreferences(PREFS, MODE_PRIVATE).getString(ROM_ROOT, null)
        if (saved == null) {
            Toast.makeText(this, "Choose a ROM folder first", Toast.LENGTH_SHORT).show()
            openRomFolderPicker()
            return
        }
        val uri = Uri.parse(saved)
        if (contentResolver.persistedUriPermissions.none { it.uri == uri && it.isReadPermission }) {
            Toast.makeText(this, "ROM folder permission expired", Toast.LENGTH_SHORT).show()
            openRomFolderPicker()
            return
        }
        scanFolder(uri)
    }

    private fun openSharedFolderPicker(requestCode: Int) {
        check(requestCode == SAVE_REQUEST || requestCode == STATE_REQUEST)
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or
                Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
                Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION)
        }
        try {
            @Suppress("DEPRECATION")
            startActivityForResult(intent, requestCode)
        } catch (error: ActivityNotFoundException) {
            Toast.makeText(this, "Android folder picker unavailable", Toast.LENGTH_LONG).show()
        }
    }

    private fun openBiosFolderPicker() {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
            )
        }
        try {
            @Suppress("DEPRECATION")
            startActivityForResult(intent, BIOS_REQUEST)
        } catch (error: ActivityNotFoundException) {
            Toast.makeText(this, "Android BIOS folder picker unavailable", Toast.LENGTH_LONG).show()
        }
    }

    /** Runs sequentially with ROM materialization, never on the UI/GL thread. */
    private fun importBiosFolder(uri: Uri, persistSelection: Boolean) {
        gameLoader.execute {
            try {
                val imported = BiosLibrary.importFrom(this, uri)
                if (persistSelection) {
                    getSharedPreferences(PREFS, MODE_PRIVATE).edit()
                        .putString(BIOS_ROOT, uri.toString()).apply()
                }
                runOnUiThread {
                    if (!isDestroyed) {
                        Toast.makeText(
                            this,
                            "BIOS ready: " + imported.joinToString(", ") +
                                " (next game launch)",
                            Toast.LENGTH_LONG
                        ).show()
                    }
                }
            } catch (error: Exception) {
                Log.e(TAG, "BIOS folder import failed", error)
                runOnUiThread {
                    if (!isDestroyed) {
                        Toast.makeText(this, "BIOS import failed: " +
                            (error.message ?: "check folder files"), Toast.LENGTH_LONG).show()
                    }
                }
            }
        }
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

    private fun openVisualPicker(kind: Int) {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = if (kind == THEME_REQUEST) "text/plain" else "image/png"
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        try {
            @Suppress("DEPRECATION")
            startActivityForResult(intent, kind)
        } catch (error: ActivityNotFoundException) {
            Toast.makeText(this, "Android file picker unavailable", Toast.LENGTH_LONG).show()
        }
    }

    private fun visualFile(kind: Int): File {
        return if (kind == THEME_REQUEST) File(filesDir, "Config/theme.txt")
               else File(filesDir, "Wallpapers/user.png")
    }

    private fun resetVisualFile(kind: Int) {
        val file = visualFile(kind)
        if (file.exists() && !file.delete()) {
            Toast.makeText(this, "Could not remove visual setting", Toast.LENGTH_LONG).show()
            return
        }
        nativeReloadVisualAssets()
    }

    private fun importVisualFile(kind: Int, uri: Uri) {
        try {
            val maxBytes = if (kind == THEME_REQUEST) 64 * 1024 else 6 * 1024 * 1024
            val bytes = contentResolver.openInputStream(uri)?.use { stream ->
                val output = ByteArrayOutputStream()
                val chunk = ByteArray(8192)
                while (true) {
                    val count = stream.read(chunk)
                    if (count < 0) break
                    if (output.size() + count > maxBytes) {
                        throw IllegalArgumentException("Selected file is too large")
                    }
                    output.write(chunk, 0, count)
                }
                output.toByteArray()
            } ?: throw IllegalArgumentException("Selected file could not be opened")
            if (kind == WALLPAPER_REQUEST) {
                val png = byteArrayOf(-119, 80, 78, 71, 13, 10, 26, 10)
                require(bytes.size >= 8 && bytes.copyOfRange(0, 8).contentEquals(png)) {
                    "Wallpaper must be a PNG"
                }
                val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
                BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
                require(bounds.outWidth in 1..4096 && bounds.outHeight in 1..4096) {
                    "Unsupported wallpaper dimensions"
                }
            }
            val target = visualFile(kind)
            target.parentFile?.mkdirs()
            val atomic = AtomicFile(target)
            val output = atomic.startWrite()
            try {
                output.write(bytes)
                atomic.finishWrite(output)
            } catch (error: Exception) {
                atomic.failWrite(output)
                throw error
            }
            nativeReloadVisualAssets()
            Toast.makeText(this, "Slot. personalization updated", Toast.LENGTH_SHORT).show()
        } catch (error: Exception) {
            Log.e(TAG, "Cannot import Slot visual file", error)
            Toast.makeText(this, "Import failed: " + error.message, Toast.LENGTH_LONG).show()
        }
    }

    // Match the Rust core_selection::key FNV-1a exactly. Cartridge
    // labels belong to opaque SAF URIs, not their non-unique filenames.
    private fun cartLabelFile(romUri: String): File {
        var hash = -3750763034362895579L // unsigned 0xcbf29ce484222325
        for (byte in romUri.toByteArray(Charsets.UTF_8)) {
            hash = (hash xor (byte.toInt() and 0xff).toLong()) * 0x100000001b3L
        }
        val name = java.lang.Long.toUnsignedString(hash, 16).padStart(16, '0')
        return File(File(filesDir, "Labels"), "$name.png")
    }

    private fun openCartLabelPicker() {
        val selected = nativePollCartLabelUri()
        if (selected.isNullOrBlank()) {
            Toast.makeText(this, "Select a cartridge first", Toast.LENGTH_SHORT).show()
            return
        }
        pendingCartLabelUri = selected
        openVisualPicker(LABEL_REQUEST)
    }

    private fun removeSelectedCartLabel() {
        val selected = nativePollCartLabelUri() ?: return
        val file = cartLabelFile(selected)
        if (file.exists() && !file.delete()) {
            Toast.makeText(this, "Could not remove label", Toast.LENGTH_LONG).show()
            return
        }
        nativeReloadCartLabel(selected)
        Toast.makeText(this, "Original Slot. label restored", Toast.LENGTH_SHORT).show()
    }

    private fun importCartLabel(source: Uri, romUri: String) {
        try {
            // Only standalone PNG artwork: not a ROM/BIOS folder or a
            // serialized game state. Decode bounds before storing it.
            val bytes = contentResolver.openInputStream(source)?.use { stream ->
                val output = ByteArrayOutputStream()
                val chunk = ByteArray(8192)
                while (true) {
                    val count = stream.read(chunk)
                    if (count < 0) break
                    if (output.size() + count > 2 * 1024 * 1024) {
                        throw IllegalArgumentException("Cartridge label exceeds 2 MiB")
                    }
                    output.write(chunk, 0, count)
                }
                output.toByteArray()
            } ?: throw IllegalArgumentException("Cannot open label image")
            val signature = byteArrayOf(-119, 80, 78, 71, 13, 10, 26, 10)
            require(bytes.size >= 8 && bytes.copyOfRange(0, 8).contentEquals(signature)) {
                "Cartridge artwork must be a PNG"
            }
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
            require(bounds.outWidth in 1..2048 && bounds.outHeight in 1..2048) {
                "Unsupported cartridge-label dimensions"
            }
            val target = cartLabelFile(romUri)
            target.parentFile?.mkdirs()
            val atomic = AtomicFile(target)
            val output = atomic.startWrite()
            try {
                output.write(bytes)
                atomic.finishWrite(output)
            } catch (error: Exception) {
                atomic.failWrite(output)
                throw error
            }
            nativeReloadCartLabel(romUri)
            Toast.makeText(this, "Cartridge label applied", Toast.LENGTH_SHORT).show()
        } catch (error: Exception) {
            Log.e(TAG, "Could not import cartridge label", error)
            Toast.makeText(this, "Label import failed: " + error.message, Toast.LENGTH_LONG).show()
        }
    }

    @Deprecated("Android 12 SAF activity result")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == LABEL_REQUEST) {
            val selected = pendingCartLabelUri
            pendingCartLabelUri = null
            if (resultCode == RESULT_OK && selected != null) {
                data?.data?.let { importCartLabel(it, selected) }
            }
            return
        }
        if (requestCode == THEME_REQUEST || requestCode == WALLPAPER_REQUEST) {
            if (resultCode == RESULT_OK) data?.data?.let { importVisualFile(requestCode, it) }
            return
        }
        if (requestCode !in setOf(FOLDER_REQUEST, BIOS_REQUEST,
                SAVE_REQUEST, STATE_REQUEST) || resultCode != RESULT_OK) return
        val uri = data?.data ?: return
        try {
            val writable = requestCode == SAVE_REQUEST || requestCode == STATE_REQUEST
            val flags = if (writable) {
                Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
            } else {
                Intent.FLAG_GRANT_READ_URI_PERMISSION
            }
            contentResolver.takePersistableUriPermission(uri, flags)
            when (requestCode) {
                FOLDER_REQUEST -> {
                    getSharedPreferences(PREFS, MODE_PRIVATE).edit()
                        .putString(ROM_ROOT, uri.toString()).apply()
                    scanFolder(uri)
                }
                BIOS_REQUEST -> importBiosFolder(uri, persistSelection = true)
                SAVE_REQUEST, STATE_REQUEST -> {
                    val key = if (requestCode == SAVE_REQUEST) SAVE_ROOT else STATE_ROOT
                    getSharedPreferences(PREFS, MODE_PRIVATE).edit()
                        .putString(key, uri.toString()).apply()
                    val kind = if (requestCode == SAVE_REQUEST) "Save" else "Save State"
                    Toast.makeText(this,
                        "$kind folder selected. RetroArch core subfolders will be used.",
                        Toast.LENGTH_LONG).show()
                }
            }
        } catch (error: SecurityException) {
            if (requestCode == FOLDER_REQUEST) {
                status.text = "Cannot keep ROM folder permission — choose another folder"
            } else {
                Toast.makeText(this, "Folder requires Android read/write access", Toast.LENGTH_LONG).show()
            }
            Log.e(TAG, "Android folder permission error", error)
        }
    }

    /** Load a previously indexed SAF ROM shelf without enumerating ROMs again.
     * The cache never grants access: the persisted SAF permission is checked by
     * the caller and opening an individual ROM still uses ContentResolver.
     */
    private fun restoreCachedLibrary(root: Uri): Boolean {
        val file = File(filesDir, ROM_CACHE_NAME)
        if (!file.isFile || file.length() !in 1L..ROM_CACHE_MAX_BYTES.toLong()) return false
        return try {
            val metadata = JSONObject(file.readText(Charsets.UTF_8))
            if (metadata.optInt("schema") != ROM_CACHE_SCHEMA ||
                metadata.optString("root") != root.toString()) return false
            val games = metadata.optJSONArray("games") ?: return false
            val count = nativeSetLibrary(games.toString())
            if (count < 0) return false
            if (count == 0) {
                status.visibility = View.VISIBLE
                status.text = "No .gb/.gbc/.gba games found — START choose another folder"
            } else {
                status.visibility = View.GONE
            }
            Log.i(TAG, "Restored $count cached ROMs without a SAF rescan")
            true
        } catch (error: Exception) {
            Log.w(TAG, "ROM library cache invalid; performing a fresh scan", error)
            false
        }
    }

    /** Commit the complete ROM index only after native import succeeds.
     * AtomicFile keeps the previous shelf intact on a failed/partial refresh.
     */
    private fun storeCachedLibrary(root: Uri, json: String) {
        val payload = JSONObject()
            .put("schema", ROM_CACHE_SCHEMA)
            .put("root", root.toString())
            .put("games", JSONArray(json))
            .toString().toByteArray(Charsets.UTF_8)
        if (payload.size > ROM_CACHE_MAX_BYTES) {
            Log.w(TAG, "ROM library cache too large; future launches will rescan")
            return
        }
        val atomic = AtomicFile(File(filesDir, ROM_CACHE_NAME))
        val stream = atomic.startWrite()
        try {
            stream.write(payload)
            atomic.finishWrite(stream)
        } catch (error: Exception) {
            atomic.failWrite(stream)
            throw error
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
                if (count >= 0) {
                    try {
                        storeCachedLibrary(uri, result.json)
                    } catch (error: Exception) {
                        Log.w(TAG, "Cannot cache ROM library; games remain available", error)
                    }
                }
                runOnUiThread {
                    if (isDestroyed || scanSerial.get() != serial) return@runOnUiThread
                    if (count < 0) {
                        status.text = "Library import failed — START to retry"
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
                        status.text = "Cannot read ROM folder — START to choose again"
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
        cartSounds.release()
        super.onDestroy()
    }

    override fun onPause() {
        resumed = false
        stopAudio()
        cartSounds.pause()
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
        cartSounds.resume()
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
