package fyi.slot.konkr

import android.content.Context
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.wifi.WifiManager
import android.provider.Settings
import android.view.Gravity
import android.view.View
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView

/**
 * Small, non-interactive system row matching Slot's white printed HUD.
 * Uses app launcher icons for accurate brands. Status: green dot = confirmed
 * running, grey dot = confirmed stopped, amber ? = installed but unknown.
 * Never infer "running" from installed or recently used.
 */
internal class SlotStatusHud(private val context: Context, private val frame: FrameLayout) {
    private enum class State { RUNNING, STOPPED, UNKNOWN }
    private data class App(val title: String, val packages: List<String>, val state: State)
    private val left = LinearLayout(context).apply {
        gravity = Gravity.CENTER_VERTICAL
        orientation = LinearLayout.HORIZONTAL
        setPadding(dp(17), dp(8), 0, 0)
        isClickable = false
    }
    private val right = LinearLayout(context).apply {
        gravity = Gravity.CENTER_VERTICAL
        orientation = LinearLayout.HORIZONTAL
        setPadding(0, dp(8), dp(18), 0)
        isClickable = false
    }
    private var raState: Boolean? = null
    private var syncState: Boolean? = null
    private var syncObservedAt: Long = 0L
    private var lastLeft: String = ""
    private var lastRight: String = ""

    private fun dp(n: Int) = (context.resources.displayMetrics.density * n + .5f).toInt()
    init {
        frame.addView(left, FrameLayout.LayoutParams(-2, dp(35), Gravity.TOP or Gravity.START))
        frame.addView(right, FrameLayout.LayoutParams(-2, dp(35), Gravity.TOP or Gravity.END))
    }
    fun remove() { frame.removeView(left); frame.removeView(right) }
    fun setVisible(visible: Boolean) {
        val visibility = if (visible) View.VISIBLE else View.GONE
        left.visibility = visibility
        right.visibility = visibility
    }
    fun raRunning(value: Boolean?) { raState = value; updateApps() }
    fun basicSyncState(value: Boolean?) {
        syncState = value
        syncObservedAt = android.os.SystemClock.elapsedRealtime()
        updateApps()
    }

    private fun installed(packages: List<String>): String? =
        packages.firstOrNull { pkg ->
            try { context.packageManager.getPackageInfo(pkg, 0); true }
            catch (_: Exception) { false }
        }

    private fun t(text: String): TextView = TextView(context).apply {
        this.text = text
        textSize = 10.5f
        typeface = Typeface.create("sans-serif-condensed", Typeface.BOLD)
        letterSpacing = 0.12f
        setTextColor(Color.rgb(227, 225, 232))
        gravity = Gravity.CENTER_VERTICAL
    }

    private fun dot(color: Int) = View(context).apply {
        background = GradientDrawable().apply {
            shape = GradientDrawable.OVAL
            setColor(color)
        }
    }

    private fun apps(): List<App> = listOf(
        App("SleepManager", listOf("com.med.sleepmanager"), State.UNKNOWN),
        App("BasicSync", listOf("com.chiller3.basicsync"),
            when (if (android.os.SystemClock.elapsedRealtime() - syncObservedAt < 60000)
                syncState else null) {
                true -> State.RUNNING; false -> State.STOPPED; null -> State.UNKNOWN
            }),
        App("Syncthing", listOf("com.github.catfriend1.syncthingfork",
            "com.github.catfriend1.syncthingfork.debug",
            "com.github.catfriend1.syncthingandroid"), State.UNKNOWN),
        App("RAOfflineProxy", listOf("com.raofflineproxy"),
            when (raState) { true -> State.RUNNING; false -> State.STOPPED; null -> State.UNKNOWN })
    )
    private fun updateApps() {
        val entries = apps().mapNotNull { app ->
            installed(app.packages)?.let { app to it }
        }
        val stateKey = entries.joinToString("|") { (app, pkg) -> "$pkg:${app.state}" }
        if (lastLeft == stateKey) return
        lastLeft = stateKey
        left.removeAllViews()
        for ((app, pkg) in entries) {
            val icon = ImageView(context).apply {
                setImageDrawable(try { context.packageManager.getApplicationIcon(pkg) }
                                 catch (_: Exception) { null })
                scaleType = ImageView.ScaleType.FIT_CENTER
                contentDescription = app.title + ": " + when (app.state) {
                    State.RUNNING -> "active"; State.STOPPED -> "paused"
                    State.UNKNOWN -> "installed, state unknown"
                }
                alpha = if (app.state == State.STOPPED) .45f else 1f
                isClickable = false
            }
            val holder = FrameLayout(context)
            holder.addView(icon, FrameLayout.LayoutParams(dp(25), dp(25), Gravity.CENTER))
            val c = when (app.state) {
                State.RUNNING -> Color.rgb(137, 215, 153)
                State.STOPPED -> Color.rgb(123, 120, 128)
                State.UNKNOWN -> Color.rgb(211, 180, 119)
            }
            holder.addView(dot(c), FrameLayout.LayoutParams(dp(6), dp(6),
                Gravity.BOTTOM or Gravity.END))
            left.addView(holder, LinearLayout.LayoutParams(dp(29), dp(29)).apply {
                rightMargin = dp(6)
            })
        }
    }
    fun refresh() {
        updateApps()
        val wm = context.applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
        val cm = context.getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager
        val connected = try {
            val n = cm?.activeNetwork
            n != null && cm.getNetworkCapabilities(n)?.hasTransport(
                NetworkCapabilities.TRANSPORT_WIFI) == true
        } catch (_: Exception) { false }
        val wifiOn = try { wm?.isWifiEnabled == true } catch (_: Exception) { false }
        val rssi = try { wm?.connectionInfo?.rssi ?: -127 } catch (_: Exception) { -127 }
        val level = if (connected && rssi > -125) {
            WifiManager.calculateSignalLevel(rssi, 4).coerceIn(0, 3)
        } else -1
        // On Android 12, reading BluetoothAdapter.isEnabled requires a new
        // dangerous BLUETOOTH_CONNECT grant. Reading this platform setting is
        // permission-free; unavailable values produce no indicator.
        val bluetooth = try {
            Settings.Global.getInt(context.contentResolver, "bluetooth_on", 0) == 1
        } catch (_: Exception) { false }
        val key = "$wifiOn/$connected/$level/$bluetooth"
        if (key == lastRight) return
        lastRight = key
        right.removeAllViews()
        if (wifiOn && connected) {
            val bars = LinearLayout(context).apply {
                gravity = Gravity.BOTTOM
                orientation = LinearLayout.HORIZONTAL
            }
            for (i in 0..3) {
                val bar = View(context)
                bar.setBackgroundColor(if (level >= i) Color.rgb(235, 234, 240)
                    else Color.rgb(92, 91, 101))
                bars.addView(bar, LinearLayout.LayoutParams(dp(3), dp(5 + i * 3)).apply {
                    rightMargin = dp(2)
                })
            }
            right.addView(bars, LinearLayout.LayoutParams(-2, dp(22)).apply {
                rightMargin = dp(9)
            })
        }
        if (bluetooth) right.addView(t("ᛒ").apply {
            textSize = 18f
            contentDescription = "Bluetooth enabled"
        })
    }
}
