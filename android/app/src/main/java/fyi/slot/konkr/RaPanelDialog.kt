package fyi.slot.konkr

import android.app.Activity
import android.app.Dialog
import android.graphics.drawable.ColorDrawable
import android.widget.FrameLayout
import android.view.ViewGroup
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
    private val ink = SlotUi.ink
    private val muted = SlotUi.muted
    private val border = SlotUi.rule
    private val font: Typeface by lazy { SlotUi.font(activity) }
    private fun dp(x: Int): Int = (x * activity.resources.displayMetrics.density + .5f).toInt()
    private val dialog: Dialog = object : Dialog(
        activity, android.R.style.Theme_Black_NoTitleBar_Fullscreen
    ) {
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

    private fun section(title: String) {
        content.addView(text(title, 14.5f, ink).apply {
            setPadding(0, dp(13), 0, dp(6))
        })
        content.addView(separator(), LinearLayout.LayoutParams(-1, dp(1)))
    }

    private fun badgeProgress(percent: Int?) {
        val pct = percent?.coerceIn(0, 100)
        val rail = FrameLayout(activity).apply {
            setBackgroundColor(Color.rgb(60, 57, 66))
            minimumHeight = dp(7)
        }
        val fill = View(activity).apply {
            setBackgroundColor(SlotUi.ink)
        }
        rail.addView(fill, FrameLayout.LayoutParams(0, dp(7), Gravity.START))
        content.addView(rail, LinearLayout.LayoutParams(-1, dp(7)).apply {
            topMargin = dp(4)
            bottomMargin = dp(8)
        })
        rail.post {
            fill.layoutParams = (fill.layoutParams as FrameLayout.LayoutParams).apply {
                width = if (pct == null) 0 else (rail.width * pct / 100)
            }
        }
    }

    fun show() {
        val root = LinearLayout(activity).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(37), dp(18), dp(37), dp(15))
            setBackgroundColor(SlotUi.background)
        }
        root.addView(text("RETROACHIEVEMENTS", 23f), LinearLayout.LayoutParams(-1, dp(43)))
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
                    view.setBackgroundColor(if (focused) SlotUi.selected else Color.TRANSPARENT)
                }
            }
            actions.addView(button, LinearLayout.LayoutParams(0, dp(39), 1f))
        }
        addAction("REFRESH", onRefresh)
        addAction("SIGN OUT", onSignOut)
        val hint = SlotUi.hint(activity, "B", "BACK")
        actions.addView(hint, LinearLayout.LayoutParams(0, dp(39), 1f))
        root.addView(actions)
        dialog.setContentView(root)
        dialog.show()
        dialog.window?.apply {
            setWindowAnimations(R.style.SlotWindowAnimation)
            setBackgroundDrawable(ColorDrawable(Color.rgb(23, 22, 25)))
            addFlags(WindowManager.LayoutParams.FLAG_FULLSCREEN)
            setLayout(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
            decorView.systemUiVisibility = (
                View.SYSTEM_UI_FLAG_FULLSCREEN or View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or
                View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY or
                View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or
                View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
            )
        }
        actions.getChildAt(0)?.requestFocus()
    }

    fun update(engine: String, snapshot: RaDiagnostics.Snapshot) {
        if (!dialog.isShowing) return
        content.removeAllViews()
        section("ACCOUNT")
        row("Player", username)
        row("Connection", "Direct to RA")
        row("Engine", engine)
        row("Mode", "Casual")
        section("GAME CACHE")
        row("GBA ROMs", snapshot.libraryGba?.toString() ?: "Not indexed")
        val cached = snapshot.cachedGba?.toString() ?: "Unknown"
        val indexed = snapshot.indexedGba?.toString() ?: "?"
        row("GBA prepared", "$cached / $indexed indexed")
        section("ACHIEVEMENT BADGES")
        val badgesDownloaded = snapshot.badgesDownloaded
        val badgesTotal = snapshot.badgesTotal
        val imageValue = if (badgesDownloaded != null && badgesTotal != null)
            "$badgesDownloaded / $badgesTotal    ${snapshot.badgesPercent}%"
        else "Not indexed"
        row("Images downloaded", imageValue)
        badgeProgress(snapshot.badgesPercent)
        section("SYNC & AWARDS")
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
            "Image percentage covers unique badges of GBA games already prepared. " +
                "New games can increase the total. Local awards remain after sign out."
        content.addView(text(note, 11f, muted).apply {
            setPadding(0, dp(15), 0, dp(2))
        })
    }

    fun isShowing(): Boolean = dialog.isShowing

    fun dismiss() = dialog.dismiss()
}
