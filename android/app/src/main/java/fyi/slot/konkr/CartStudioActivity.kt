package fyi.slot.konkr

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.graphics.Color
import android.os.Bundle
import android.util.Log
import android.webkit.ValueCallback
import android.view.Gravity
import android.view.KeyEvent
import android.view.View
import android.view.WindowManager
import android.webkit.ConsoleMessage
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import android.widget.TextView
import android.widget.Toast
import java.io.ByteArrayInputStream
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Original Cart Studio, offline HTML/JS/WASM served under a private HTTPS origin.
 * Uses a restricted indexed-library SAF bridge, not a browser SD-card picker.
 */
class CartStudioActivity : Activity() {
    companion object {
        const val EXTRA_ROM = "cart_studio_rom_uri"
        const val RESULT_CHANGED = Activity.RESULT_OK
        private const val ORIGIN = "https://appassets.androidplatform.net"
        private const val PAGE = "$ORIGIN/studio/index.html"
        private const val TAG = "SlotCartStudio"
        private const val PICK_CUSTOM_PNG = 4709
    }

    private val worker = Executors.newSingleThreadExecutor()
    private val changed = AtomicBoolean(false)
    private var web: WebView? = null
    private var pendingPngPicker: ValueCallback<Array<Uri>>? = null

    private fun immersive() {
        // FLAG_FULLSCREEN removes the status-bar window (the grey strip seen
        // on KONKR), while immersive keeps the navigation area out of the UI.
        window.addFlags(WindowManager.LayoutParams.FLAG_FULLSCREEN)
        window.statusBarColor = Color.rgb(27, 27, 33)
        window.navigationBarColor = Color.BLACK
        window.decorView.systemUiVisibility = (
            View.SYSTEM_UI_FLAG_FULLSCREEN or
            View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or
            View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY or
            View.SYSTEM_UI_FLAG_LAYOUT_STABLE or
            View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or
            View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
        )
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) immersive()
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        immersive()
        val frame = FrameLayout(this)
        frame.setBackgroundColor(Color.rgb(27, 27, 33))
        setContentView(frame)
        val notice = TextView(this).apply {
            text = "Cart Studio\nLoading Slot library…"
            textSize = 18f
            gravity = Gravity.CENTER
            setTextColor(Color.WHITE)
        }
        frame.addView(notice, FrameLayout.LayoutParams(-1, -1))

        val selected = intent.getStringExtra(EXTRA_ROM)?.takeIf { it.startsWith("content://") }
        worker.execute {
            try {
                val carts = CartStudioCatalog.games(this)
                require(carts.isNotEmpty()) { "Choose GB, GBC or GBA ROM folders in Library" }
                require(selected == null || carts.any { it.uri == selected }) {
                    "Selected cartridge is no longer in the library"
                }
                // X is an editor for ONE ROM, not a full catalog scan.
                // Menu -> Cart Studio still sees all GB/GBC/GBA games.
                val studioCarts = if (selected != null) carts.filter { it.uri == selected } else carts
                val bridge = CartStudioBridge(this, studioCarts, selected, changed)
                runOnUiThread {
                    if (!isFinishing && !isDestroyed) openStudio(frame, notice, bridge)
                }
            } catch (error: Exception) {
                Log.e(TAG, "Cannot prepare Cart Studio", error)
                runOnUiThread {
                    if (!isFinishing && !isDestroyed) {
                        notice.text = "Unable to load library\n" +
                            (error.message ?: "Check your ROM folder permission")
                    }
                }
            }
        }
    }

    private fun openStudio(frame: FrameLayout, notice: View, bridge: CartStudioBridge) {
        val w = WebView(this)
        web = w
        w.setBackgroundColor(Color.rgb(27, 27, 33))
        w.settings.apply {
            javaScriptEnabled = true
            domStorageEnabled = true
            allowFileAccess = false
            allowContentAccess = false
            javaScriptCanOpenWindowsAutomatically = false
            setSupportMultipleWindows(false)
            mixedContentMode = android.webkit.WebSettings.MIXED_CONTENT_NEVER_ALLOW
        }
        w.isFocusableInTouchMode = true
        w.addJavascriptInterface(bridge, "AndroidStudio")
        val officialArt = CartStudioArtProxy(this)
        w.webViewClient = object : WebViewClient() {
            override fun shouldInterceptRequest(
                view: WebView?, request: WebResourceRequest?
            ): WebResourceResponse? {
                val url = request?.url ?: return null
                if (request.method != "GET" || url.scheme != "https" ||
                    url.host != "appassets.androidplatform.net" ||
                    !url.path.orEmpty().startsWith("/studio/")) return null
                val path = url.path.orEmpty().removePrefix("/studio/")
                // Native same-origin fetch avoids CORS restrictions from the
                // official art CDN when the Studio runs under appassets.
                if (path.startsWith("art/")) {
                    return officialArt.intercept(path.removePrefix("art/"))
                }
                val safe = Regex("[a-zA-Z0-9_.\\-/]+").matches(path) &&
                    !path.split('/').contains("..") && !path.startsWith("/")
                if (!safe) return missing()
                return try {
                    val mime = when {
                        path.endsWith(".html") -> "text/html"
                        path.endsWith(".js") -> "text/javascript"
                        path.endsWith(".css") -> "text/css"
                        path.endsWith(".wasm") -> "application/wasm"
                        path.endsWith(".json") -> "application/json"
                        path.endsWith(".png") -> "image/png"
                        path.endsWith(".svg") -> "image/svg+xml"
                        else -> return missing()
                    }
                    WebResourceResponse(mime, if (mime.startsWith("text/")) "UTF-8" else null,
                        assets.open("studio/$path"))
                } catch (_: Exception) { missing() }
            }
            override fun shouldOverrideUrlLoading(
                view: WebView?, request: WebResourceRequest?
            ): Boolean {
                // Never let a frame with the JavaScript bridge navigate to a remote page.
                return request?.url?.toString() != PAGE
            }
        }
        w.webChromeClient = object : WebChromeClient() {
            override fun onShowFileChooser(
                webView: WebView?,
                filePathCallback: ValueCallback<Array<Uri>>?,
                fileChooserParams: WebChromeClient.FileChooserParams?,
            ): Boolean {
                if (filePathCallback == null) return false
                // Original Studio's custom logo/label input needs Android's
                // document picker; no broad storage permission is requested.
                pendingPngPicker?.onReceiveValue(null)
                pendingPngPicker = filePathCallback
                val pick = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
                    addCategory(Intent.CATEGORY_OPENABLE)
                    type = "image/png"
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                }
                return try {
                    @Suppress("DEPRECATION")
                    startActivityForResult(pick, PICK_CUSTOM_PNG)
                    true
                } catch (error: Exception) {
                    Log.w(TAG, "Unable to open PNG picker", error)
                    pendingPngPicker?.onReceiveValue(null)
                    pendingPngPicker = null
                    false
                }
            }

            override fun onConsoleMessage(message: ConsoleMessage?): Boolean {
                if (message != null) Log.d(TAG, "Studio: " + message.message())
                return true
            }
        }
        frame.removeView(notice)
        // Keep native navigation OUTSIDE the WebView: it cannot cover the
        // original logo, label preview, or Cart Studio controls.
        val barHeight = (48 * resources.displayMetrics.density + 0.5f).toInt()
        val webLayout = FrameLayout.LayoutParams(-1, -1).apply {
            topMargin = barHeight
        }
        frame.addView(w, webLayout)
        val toolbar = FrameLayout(this).apply {
            setBackgroundColor(Color.rgb(27, 27, 33))
        }
        frame.addView(toolbar, FrameLayout.LayoutParams(-1, barHeight, Gravity.TOP))
        val title = TextView(this).apply {
            text = "Cart Studio"
            textSize = 17f
            setTextColor(Color.WHITE)
            gravity = Gravity.CENTER
        }
        toolbar.addView(title, FrameLayout.LayoutParams(-1, -1))
        val close = TextView(this).apply {
            text = "‹  slot."
            textSize = 15f
            setTextColor(Color.WHITE)
            gravity = Gravity.CENTER
            setOnClickListener { finish() }
            isFocusable = true
            contentDescription = "Close Cart Studio and return to Slot"
        }
        val backWidth = (116 * resources.displayMetrics.density + 0.5f).toInt()
        toolbar.addView(close, FrameLayout.LayoutParams(backWidth, -1, Gravity.START))
        val rule = View(this).apply { setBackgroundColor(Color.rgb(58, 58, 70)) }
        toolbar.addView(rule, FrameLayout.LayoutParams(-1,
            (resources.displayMetrics.density + 0.5f).toInt().coerceAtLeast(1),
            Gravity.BOTTOM))
        w.loadUrl(PAGE)
    }

    private fun missing(): WebResourceResponse =
        WebResourceResponse("text/plain", "UTF-8",
            ByteArrayInputStream("Not found".toByteArray(Charsets.UTF_8)))

    @Deprecated("Android 12 document picker result")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == PICK_CUSTOM_PNG) {
            val callback = pendingPngPicker
            pendingPngPicker = null
            val uri = if (resultCode == RESULT_OK) data?.data else null
            callback?.onReceiveValue(
                if (uri?.scheme == "content") arrayOf(uri) else null
            )
            immersive()
        }
    }

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        when (keyCode) {
            KeyEvent.KEYCODE_BUTTON_B, KeyEvent.KEYCODE_BACK -> { finish(); return true }
            KeyEvent.KEYCODE_BUTTON_A -> {
                return web?.dispatchKeyEvent(KeyEvent(KeyEvent.ACTION_DOWN,
                    KeyEvent.KEYCODE_DPAD_CENTER)) ?: super.onKeyDown(keyCode, event)
            }
        }
        return super.onKeyDown(keyCode, event)
    }

    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean {
        if (keyCode == KeyEvent.KEYCODE_BUTTON_B || keyCode == KeyEvent.KEYCODE_BACK)
            return true
        if (keyCode == KeyEvent.KEYCODE_BUTTON_A) {
            return web?.dispatchKeyEvent(KeyEvent(KeyEvent.ACTION_UP,
                KeyEvent.KEYCODE_DPAD_CENTER)) ?: true
        }
        return super.onKeyUp(keyCode, event)
    }

    override fun finish() {
        if (changed.get()) setResult(RESULT_CHANGED)
        super.finish()
    }

    override fun onDestroy() {
        pendingPngPicker?.onReceiveValue(null)
        pendingPngPicker = null
        web?.apply {
            stopLoading()
            removeJavascriptInterface("AndroidStudio")
            destroy()
        }
        web = null
        worker.shutdownNow()
        // Persist the final (<12 carts) CRC batch for fast subsequent launches.
        try { CartStudioCatalog.flush(this) }
        catch (error: Exception) { Log.w(TAG, "Could not flush Studio CRC cache", error) }
        super.onDestroy()
    }
}
