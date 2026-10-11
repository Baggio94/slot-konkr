package fyi.slot.konkr

import android.app.Activity
import android.app.Dialog
import android.content.Context
import android.graphics.Color
import android.graphics.drawable.ColorDrawable
import android.os.Handler
import android.view.Gravity
import android.view.KeyEvent
import android.view.View
import android.view.ViewGroup
import android.view.Window
import android.view.WindowManager
import android.view.inputmethod.InputMethodManager
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView

/**
 * Small Slot-themed Android form/confirmation shell. Uses Android EditTexts
 * only for secure IME input while every owned visual pixel comes from SlotUi.
 * No token, username or password is ever printed or emitted to Logcat.
 */
internal class SlotModalDialog(
    private val activity: Activity,
    title: String,
    message: String,
) {
    private var busy = false
    private val buttons = ArrayList<TextView>()
    private val secretFields = ArrayList<EditText>()
    private val fields = ArrayList<EditText>()
    private val dialog = object : Dialog(activity) {
        override fun dispatchKeyEvent(event: KeyEvent): Boolean {
            when (event.keyCode) {
                KeyEvent.KEYCODE_BUTTON_B -> {
                    if (event.action == KeyEvent.ACTION_DOWN && event.repeatCount == 0 && !busy) dismiss()
                    return true
                }
                KeyEvent.KEYCODE_BUTTON_A -> {
                    if (event.action == KeyEvent.ACTION_DOWN && event.repeatCount == 0) {
                        val focus = currentFocus
                        if (focus is EditText) {
                            focus.requestFocus()
                            val imm = activity.getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager
                            imm?.showSoftInput(focus, InputMethodManager.SHOW_IMPLICIT)
                        } else if (focus?.isClickable == true && !busy) {
                            focus.performClick()
                        }
                    }
                    return true
                }
                KeyEvent.KEYCODE_BACK -> if (busy) return true
            }
            return super.dispatchKeyEvent(event)
        }
    }
    private val root = LinearLayout(activity).apply {
        orientation = LinearLayout.VERTICAL
        setPadding(dp(26), dp(16), dp(26), dp(13))
        background = SlotUi.plate(activity, SlotUi.background)
    }
    private val body = LinearLayout(activity).apply {
        orientation = LinearLayout.VERTICAL
    }
    private val scroll = ScrollView(activity).apply {
        isVerticalScrollBarEnabled = false
        clipToPadding = false
        isFillViewport = false
    }
    private val actions = LinearLayout(activity).apply {
        orientation = LinearLayout.HORIZONTAL
        gravity = Gravity.CENTER
    }
    private val errorText = SlotUi.label(activity, "", 11.5f, SlotUi.error, upper = false).apply {
        visibility = View.GONE
        setPadding(0, dp(8), 0, 0)
    }

    init {
        root.addView(SlotUi.label(activity, title, 21f), LinearLayout.LayoutParams(-1, dp(38)))
        root.addView(View(activity).apply { setBackgroundColor(SlotUi.rule) },
            LinearLayout.LayoutParams(-1, dp(1)))
        body.addView(SlotUi.label(activity, message, 12.5f, SlotUi.muted, upper = false).apply {
            setPadding(0, dp(16), 0, dp(10))
        })
        body.addView(errorText)
        scroll.addView(body, android.widget.FrameLayout.LayoutParams(-1, -2))
        root.addView(scroll, LinearLayout.LayoutParams(-1, 0, 1f))
        root.addView(View(activity).apply { setBackgroundColor(SlotUi.rule) },
            LinearLayout.LayoutParams(-1, dp(1)))
        root.addView(actions, LinearLayout.LayoutParams(-1, dp(56)).apply {
            topMargin = dp(9)
        })
        val legend = LinearLayout(activity).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER
        }
        legend.addView(SlotUi.hint(activity, "A", "SELECT"))
        legend.addView(SlotUi.hint(activity, "B", "BACK"),
            LinearLayout.LayoutParams(-2, -2).apply { leftMargin = dp(35) })
        root.addView(legend, LinearLayout.LayoutParams(-1, dp(27)))
        // Request no Android title/decorations BEFORE setContentView; the
        // custom Slot heading and four-sided plate are the only chrome.
        dialog.requestWindowFeature(Window.FEATURE_NO_TITLE)
        dialog.setContentView(root)
        dialog.setCanceledOnTouchOutside(false)
        dialog.setOnDismissListener {
            secretFields.forEach { it.text.clear() }
        }
    }

    fun addInput(label: String, hint: String, password: Boolean = false): EditText {
        body.addView(SlotUi.label(activity, label, 12.5f, SlotUi.ink),
            LinearLayout.LayoutParams(-1, dp(28)).apply {
                topMargin = dp(6)
            })
        val field = SlotUi.field(activity, hint, password)
        field.contentDescription = label
        body.addView(field, LinearLayout.LayoutParams(-1, dp(49)).apply {
            bottomMargin = dp(10)
        })
        fields.add(field)
        if (password) secretFields.add(field)
        // Validation / HTTPS errors belong AFTER the last field, near actions,
        // not hidden above the username when the keyboard scrolls the dialog.
        body.removeView(errorText)
        body.addView(errorText)
        return field
    }

    fun addButton(label: String, onClick: () -> Unit): TextView {
        val button = SlotUi.button(activity, label, onClick)
        val params = LinearLayout.LayoutParams(0, dp(43), 1f).apply {
            if (buttons.isNotEmpty()) leftMargin = dp(12)
        }
        actions.addView(button, params)
        buttons.add(button)
        return button
    }

    fun setError(message: String) {
        errorText.text = message
        errorText.visibility = if (message.isBlank()) View.GONE else View.VISIBLE
    }

    fun setBusy(busy: Boolean) {
        this.busy = busy
        buttons.forEach { it.isEnabled = !busy; it.alpha = if (busy) .52f else 1f }
        dialog.setCancelable(!busy)
    }

    fun show() {
        dialog.show()
        dialog.window?.apply {
            setWindowAnimations(R.style.SlotWindowAnimation)
            setBackgroundDrawable(ColorDrawable(Color.TRANSPARENT))
            addFlags(WindowManager.LayoutParams.FLAG_DIM_BEHIND)
            setDimAmount(.74f)
            setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_STATE_ALWAYS_HIDDEN or
                WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE)
            val metrics = activity.resources.displayMetrics
            setLayout((metrics.widthPixels * .77f).toInt().coerceAtMost(dp(720)),
                (metrics.heightPixels * .78f).toInt())
            decorView.systemUiVisibility = View.SYSTEM_UI_FLAG_FULLSCREEN or
                View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
        }
        // Start at the username field for first login; A summons the system
        // keyboard, B always returns. Don't pop the IME until explicitly asked.
        (fields.firstOrNull() ?: buttons.firstOrNull())?.requestFocus()
    }

    fun dismiss() = dialog.dismiss()
    fun isShowing(): Boolean = dialog.isShowing
    private fun dp(amount: Int) = SlotUi.dp(activity, amount)
}
