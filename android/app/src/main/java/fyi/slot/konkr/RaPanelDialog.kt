package fyi.slot.konkr

import android.app.Activity
import android.app.Dialog
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.view.Gravity
import android.view.KeyEvent
import android.view.View
import android.view.WindowManager
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView

/**
 * RetroAchievements status in Slot's actual OFL label font and flat charcoal
 * panels. The Android password prompt remains native for keyboard/security.
 * Never show account tokens, ROM URIs or contents of the pending ledger.
 */
internal class RaPanelDialog(
    private val activity: Activity,
    private val username: String,
    private val onRefresh: () -> Unit,
    private val onSignOut: () -> Unit,
) {
    private val ink = Color.rgb(246, 244, 239)
    private val muted = Color.rgb(164, 162, 170)
    private val border = Color.rgb(91, 89, 96)
    private val font: Typeface by lazy {
        runCatching { Typeface.createFromAsset(activity.assets, "label.ttf") }
            .getOrElse { Typeface.create("sans-serif-condensed", Typeface.BOLD) }
    }
    private fun dp(x: Int): Int = (x * activity.resources.displayMetrics.density + .5f).toInt()
    private val dialog: Dialog = object : Dialog(activity) {
        override fun dispatchKeyEvent(event: KeyEvent): Boolean {
            if (event.keyCode == KeyEvent.KEYCODE_BUTTON_B) {
                if (event.action == KeyEvent.ACTION_DOWN && event.repeatCount == 0) dismiss()
                return true
            }
            if (event.keyCode == KeyEvent.KEYCODE_BUTTON_A) {
                if (event.action == KeyEvent.ACTION_DOWN && event.repeatCount == 0) {
                    val focus = currentFocus
                    if (focus != null && focus.isClickable) focus.performClick()
                }
                return true
            }
            return super.dispatchKeyEvent(event)
        }
    }
    private lateinit var content: LinearLayout
    private lateinit var loading: TextView
    private lateinit var actions: LinearLayout

    private fun text(message: String, px: Float, color: Int = ink) = TextView(activity).apply {
        this.text = message.uppercase(java.util.Locale.ROOT)
        this.typeface = font
        this.textSize = px
        this.setTextColor(color)
        this.letterSpacing = 0.08f
        includeFontPadding = false
        gravity = Gravity.CENTER_VERTICAL
    }

    private fun separator(): View = View(activity).apply {
        setBackgroundColor(border)
    }

    private fun row(label: String, value: String) {
        val line = LinearLayout(activity).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(0, dp(8), 0, dp(8))
        }
        line.addView(text(label, 13f, muted), LinearLayout.LayoutParams(0, -2, 1f))
        val valueText = text(value, 13f).apply { gravity = Gravity.END or Gravity.CENTER_VERTICAL }
        line.addView(valueText, LinearLayout.LayoutParams(0, -2, 1.55f))
        content.addView(line)
    }

    fun show() {
        val root = LinearLayout(activity).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(22), dp(16), dp(22), dp(14))
            background = GradientDrawable().apply {
                setColor(Color.rgb(23, 22, 25))
                setStroke(dp(1), border)
                cornerRadius = dp(3).toFloat()
            }
        }
        root.addView(text("RETROACHIEVEMENTS", 19f), LinearLayout.LayoutParams(-1, dp(37)))
        root.addView(separator(), LinearLayout.LayoutParams(-1, dp(1)))
        val scroll = ScrollView(activity).apply {
            isFillViewport = false
            isVerticalScrollBarEnabled = false
            clipToPadding = false
        }
        content = LinearLayout(activity).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(0, dp(14), 0, dp(10))
        }
        loading = text("READING ACCOUNT CACHE...", 12.5f, muted)
        content.addView(loading)
        scroll.addView(content)
        root.addView(scroll, LinearLayout.LayoutParams(-1, 0, 1f))
        root.addView(separator(), LinearLayout.LayoutParams(-1, dp(1)))
        actions = LinearLayout(activity).apply {
            gravity = Gravity.CENTER_VERTICAL
            orientation = LinearLayout.HORIZONTAL
            setPadding(0, dp(12), 0, 0)
        }
        fun addAction(label: String, callback: () -> Unit) {
            val button = text(label, 12.5f).apply {
                gravity = Gravity.CENTER
                isFocusable = true
                isClickable = true
                minimumHeight = dp(36)
                setPadding(dp(7), dp(5), dp(7), dp(5))
                setOnClickListener { callback() }
                setOnFocusChangeListener { view, focused ->
                    view.setBackgroundColor(if (focused) Color.rgb(70, 68, 77) else Color.TRANSPARENT)
                }
            }
            actions.addView(button, LinearLayout.LayoutParams(0, dp(39), 1f))
        }
        addAction("REFRESH", onRefresh)
        addAction("SIGN OUT", onSignOut)
        addAction("CLOSE") { dialog.dismiss() }
        root.addView(actions)
        dialog.setContentView(root)
        dialog.window?.apply {
            setBackgroundDrawableResource(android.R.color.transparent)
            addFlags(WindowManager.LayoutParams.FLAG_DIM_BEHIND)
            setDimAmount(.68f)
        }
        dialog.show()
        val display = activity.resources.displayMetrics
        dialog.window?.setLayout((display.widthPixels * .70f).toInt(),
            (display.heightPixels * .77f).toInt())
        actions.getChildAt(0)?.requestFocus()
    }

    fun update(engine: String, snapshot: RaDiagnostics.Snapshot) {
        if (!dialog.isShowing) return
        content.removeAllViews()
        row("Account", username)
        row("Connection", "Direct to RA")
        row("Engine", engine)
        row("Mode", "Casual")
        row("GBA ROMs", snapshot.libraryGba?.toString() ?: "Not indexed")
        val cached = snapshot.cachedGba?.toString() ?: "Unknown"
        val indexed = snapshot.indexedGba?.toString() ?: "?"
        row("GBA prepared", "$cached / $indexed indexed")
        val budgetUsed = snapshot.prefetchBudgetUsed
        if (budgetUsed != null) {
            row("Cache requests", "$budgetUsed / 100 (30 min)")
            if (snapshot.nextBatchMinutes != null)
                row("Next batch", "In ${snapshot.nextBatchMinutes} min")
        }
        row("Awards to sync", snapshot.pendingAwards?.toString() ?: "Unknown")
        row("Awards saved", snapshot.recordedAwards?.toString() ?: "Unknown")
        row("GB / GBC", "Not supported yet")
        val note = snapshot.warning ?:
            "GBA definitions only. Account awards and cached games stay on this device after sign out."
        content.addView(text(note, 11f, muted).apply {
            setPadding(0, dp(15), 0, dp(2))
        })
    }

    fun dismiss() = dialog.dismiss()
}
