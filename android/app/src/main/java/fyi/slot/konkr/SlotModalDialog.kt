package fyi.slot.konkr

import android.app.Activity
import android.app.Dialog
import android.content.Context
import android.graphics.Color
import android.graphics.drawable.ColorDrawable
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
 * Single native-Slot-style confirmation / form surface for application-owned
 * dialogs. A vertical selected-row list, authentic font and controller legend
 * match Slot's menus. Android-owned permission and IME screens stay native.
 *
 * B cancels; D-pad navigates all fields/actions; A selects (or edits a field).
 * Sensitive confirmations default to the first, caller-provided safe action.
 */
internal class SlotModalDialog(
    private val activity: Activity,
    title: String,
    message: String,
    private val onDismiss: () -> Unit = {},
) {
    private var busy = false
    private val buttons = ArrayList<TextView>()
    private val secretFields = ArrayList<EditText>()
    private val fields = ArrayList<EditText>()
    private val dialog = object : Dialog(activity, android.R.style.Theme_Black_NoTitleBar_Fullscreen) {
        override fun dispatchKeyEvent(event: KeyEvent): Boolean {
            when (event.keyCode) {
                KeyEvent.KEYCODE_BUTTON_B, KeyEvent.KEYCODE_BACK -> {
                    if (event.action == KeyEvent.ACTION_DOWN &&
                        event.repeatCount == 0 && !busy) dismiss()
                    return true
                }
                KeyEvent.KEYCODE_DPAD_UP, KeyEvent.KEYCODE_DPAD_DOWN -> {
                    if (event.action == KeyEvent.ACTION_DOWN &&
                        event.repeatCount == 0 && !busy) {
                        moveFocus(if (event.keyCode == KeyEvent.KEYCODE_DPAD_DOWN) 1 else -1)
                    }
                    return true
                }
                KeyEvent.KEYCODE_DPAD_LEFT, KeyEvent.KEYCODE_DPAD_RIGHT -> {
                    // Preserve editing/cursor semantics when IME has focus.
                    if (currentFocus is EditText) return super.dispatchKeyEvent(event)
                    if (event.action == KeyEvent.ACTION_DOWN &&
                        event.repeatCount == 0 && !busy) {
                        moveFocus(if (event.keyCode == KeyEvent.KEYCODE_DPAD_RIGHT) 1 else -1)
                    }
                    return true
                }
                KeyEvent.KEYCODE_BUTTON_A, KeyEvent.KEYCODE_DPAD_CENTER -> {
                    if (event.action == KeyEvent.ACTION_DOWN &&
                        event.repeatCount == 0 && !busy) {
                        val focus = currentFocus
                        if (focus is EditText) {
                            focus.requestFocus()
                            val imm = activity.getSystemService(Context.INPUT_METHOD_SERVICE)
                                as? InputMethodManager
                            imm?.showSoftInput(focus, InputMethodManager.SHOW_IMPLICIT)
                        } else {
                            (focus as? TextView)?.takeIf { it in buttons && it.isEnabled }
                                ?.performClick()
                        }
                    }
                    return true
                }
            }
            return super.dispatchKeyEvent(event)
        }
    }
    private val root = LinearLayout(activity).apply {
        orientation = LinearLayout.VERTICAL
        setPadding(SlotUi.menuPixel(activity, 32), SlotUi.menuPixel(activity, 33),
            SlotUi.menuPixel(activity, 32), dp(9))
        setBackgroundColor(SlotUi.menuBackground)
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
        orientation = LinearLayout.VERTICAL
    }
    private val errorText = SlotUi.label(activity, "", 12f, SlotUi.error, upper = false).apply {
        visibility = View.GONE
        setPadding(0, dp(8), 0, 0)
    }

    private fun focusTargets(): List<View> =
        (fields + buttons).filter { it.isEnabled && it.visibility == View.VISIBLE }

    private fun moveFocus(delta: Int) {
        val items = focusTargets()
        if (items.isEmpty()) return
        val active = items.indexOf(dialog.currentFocus)
        val next = if (active < 0) 0
            else (active + delta + items.size) % items.size
        items[next].requestFocus()
    }

    init {
        root.addView(SlotUi.menuHeading(activity, title),
            LinearLayout.LayoutParams(-1, SlotUi.menuPixel(activity, 32)))
        root.addView(View(activity), LinearLayout.LayoutParams(-1,
            SlotUi.menuPixel(activity, 19)))
        if (message.isNotBlank()) {
            body.addView(SlotUi.label(activity, message, 15f, SlotUi.muted,
                upper = false).apply {
                setPadding(0, 0, 0, dp(14))
                maxLines = 5
            })
        }
        body.addView(errorText)
        scroll.addView(body, android.widget.FrameLayout.LayoutParams(-1, -2))
        root.addView(scroll, LinearLayout.LayoutParams(-1, 0, 1f))
        root.addView(actions, LinearLayout.LayoutParams(-1, -2))
        val legend = LinearLayout(activity).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER
        }
        legend.addView(SlotUi.nativeHint(activity, "A", "SELECT"))
        legend.addView(SlotUi.nativeHint(activity, "B", "BACK"),
            LinearLayout.LayoutParams(-2, -2).apply { leftMargin = dp(35) })
        root.addView(legend, LinearLayout.LayoutParams(-1,
            SlotUi.menuPixel(activity, 53)))
        dialog.requestWindowFeature(Window.FEATURE_NO_TITLE)
        dialog.setContentView(root)
        dialog.setCanceledOnTouchOutside(false)
        dialog.setOnDismissListener {
            secretFields.forEach { it.text.clear() }
            onDismiss()
        }
    }

    fun addInput(label: String, hint: String, password: Boolean = false): EditText {
        body.addView(SlotUi.label(activity, label, 13f, SlotUi.muted),
            LinearLayout.LayoutParams(-1, dp(27)).apply { topMargin = dp(4) })
        val field = SlotUi.field(activity, hint, password)
        field.contentDescription = label
        body.addView(field, LinearLayout.LayoutParams(-1, dp(48)).apply {
            bottomMargin = dp(10)
        })
        fields.add(field)
        if (password) secretFields.add(field)
        body.removeView(errorText)
        body.addView(errorText)
        return field
    }

    fun addButton(label: String, onClick: () -> Unit): TextView {
        val button = SlotUi.label(activity, label, 16f, SlotUi.muted).apply {
            gravity = Gravity.CENTER_VERTICAL
            setPadding(dp(12), 0, dp(12), 0)
            isFocusable = true
            isClickable = true
            contentDescription = label
            setOnFocusChangeListener { view, focused ->
                view.setBackgroundColor(if (focused) SlotUi.selected else Color.TRANSPARENT)
                setTextColor(if (focused) SlotUi.ink else SlotUi.muted)
            }
            setOnClickListener { if (!busy) onClick() }
        }
        actions.addView(button, LinearLayout.LayoutParams(-1,
            SlotUi.menuPixel(activity, 52)))
        buttons.add(button)
        return button
    }

    fun setError(message: String) {
        errorText.text = message
        errorText.visibility = if (message.isBlank()) View.GONE else View.VISIBLE
    }

    fun setBusy(value: Boolean) {
        busy = value
        buttons.forEach { it.isEnabled = !value; it.alpha = if (value) .52f else 1f }
        dialog.setCancelable(!value)
    }

    fun show() {
        dialog.show()
        dialog.window?.apply {
            setWindowAnimations(R.style.SlotWindowAnimation)
            setBackgroundDrawable(ColorDrawable(SlotUi.menuBackground))
            addFlags(WindowManager.LayoutParams.FLAG_FULLSCREEN)
            setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_STATE_ALWAYS_HIDDEN or
                WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE)
            setLayout(ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT)
            decorView.systemUiVisibility = View.SYSTEM_UI_FLAG_FULLSCREEN or
                View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
        }
        // Safe first choice on confirmations; first editable field on forms.
        (fields.firstOrNull() ?: buttons.firstOrNull())?.requestFocus()
    }

    fun dismiss() = dialog.dismiss()
    fun isShowing(): Boolean = dialog.isShowing
    private fun dp(amount: Int) = SlotUi.dp(activity, amount)
}
