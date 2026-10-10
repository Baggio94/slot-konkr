package fyi.slot.konkr

import android.app.Activity
import android.graphics.Color
import android.os.Bundle
import android.util.Log
import android.view.Gravity
import android.view.KeyEvent
import android.view.View
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
 * Uses a restricted one-cartridge bridge, not the browser's SD-card picker.
 */
class CartStudioActivity : Activity() {
    companion object {
        const val EXTRA_ROM = "cart_studio_rom_uri"
        const val RESULT_CHANGED = Activity.RESULT_OK
        private const val ORIGIN = "https://appassets.androidplatform.net"
        private const val PAGE = "$ORIGIN/studio/index.html"
        private const val TAG = "SlotCartStudio"
    }

    private val worker = Executors.newSingleThreadExecutor()
    private val changed = AtomicBoolean(false)
    private var web: WebView? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.decorView.systemUiVisibility = (
            View.SYSTEM_UI_FLAG_FULLSCREEN or View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or
            View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY or View.SYSTEM_UI_FLAG_LAYOUT_STABLE)
        val frame = FrameLayout(this)
        frame.setBackgroundColor(Color.rgb(17, 20, 24))
        setContentView(frame)
        val notice = TextView(this).apply {
            text = "Cart Studio\nReading selected cartridge…"
            textSize = 18f
            gravity = Gravity.CENTER
            setTextColor(Color.WHITE)
        }
        frame.addView(notice, FrameLayout.LayoutParams(-1, -1))

        val selected = intent.getStringExtra(EXTRA_ROM).orEmpty()
        if (!selected.startsWith("content://")) {
            Toast.makeText(this, "No cartridge selected", Toast.LENGTH_LONG).show()
            finish()
            return
        }
        worker.execute {
            try {
                val rom = CartStudioCatalog.games(this).firstOrNull { it.uri == selected }
                    ?: error("Selected cartridge is no longer in the library")
                val identity = CartStudioCatalog.identify(this, rom)
                val bridge = CartStudioBridge(this, rom, identity, changed)
                runOnUiThread {
                    if (!isFinishing && !isDestroyed) openStudio(frame, notice, bridge)
                }
            } catch (error: Exception) {
                Log.e(TAG, "Cannot prepare Cart Studio", error)
                runOnUiThread {
                    if (!isFinishing && !isDestroyed) {
                        notice.text = "Unable to read cartridge\n" +
                            (error.message ?: "Check your ROM folder permission")
                    }
                }
            }
        }
    }

    private fun openStudio(frame: FrameLayout, notice: View, bridge: CartStudioBridge) {
        val w = WebView(this)
        web = w
        w.setBackgroundColor(Color.rgb(17, 20, 24))
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
        w.webViewClient = object : WebViewClient() {
            override fun shouldInterceptRequest(
                view: WebView?, request: WebResourceRequest?
            ): WebResourceResponse? {
                val url = request?.url ?: return null
                if (url.scheme != "https" || url.host != "appassets.androidplatform.net" ||
                    !url.path.orEmpty().startsWith("/studio/")) return null
                val path = url.path.orEmpty().removePrefix("/studio/")
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
            override fun onConsoleMessage(message: ConsoleMessage?): Boolean {
                if (message != null) Log.d(TAG, "Studio: " + message.message())
                return true
            }
        }
        frame.removeView(notice)
        frame.addView(w, FrameLayout.LayoutParams(-1, -1))
        val close = TextView(this).apply {
            text = "‹  SLOT."
            textSize = 14f
            setTextColor(Color.WHITE)
            setBackgroundColor(Color.rgb(22, 25, 30))
            gravity = Gravity.CENTER
            setPadding(18, 0, 18, 0)
            setOnClickListener { finish() }
            isFocusable = true
            contentDescription = "Close Cart Studio and return to Slot"
        }
        frame.addView(close, FrameLayout.LayoutParams(120, 38, Gravity.TOP or Gravity.START))
        w.loadUrl(PAGE)
    }

    private fun missing(): WebResourceResponse =
        WebResourceResponse("text/plain", "UTF-8",
            ByteArrayInputStream("Not found".toByteArray(Charsets.UTF_8)))

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
        web?.apply {
            stopLoading()
            removeJavascriptInterface("AndroidStudio")
            destroy()
        }
        web = null
        worker.shutdownNow()
        super.onDestroy()
    }
}
