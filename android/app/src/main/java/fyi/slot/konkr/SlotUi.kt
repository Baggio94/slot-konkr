package fyi.slot.konkr

import android.content.Context
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.text.InputType
import android.view.Gravity
import android.view.View
import android.view.inputmethod.EditorInfo
import android.widget.EditText
import android.widget.TextView
import java.util.Locale

/**
 * One visual vocabulary for app-owned Android surfaces, taken from Slot's
 * upstream label.ttf (SIL OFL) and its charcoal / cream palette.
 * OS file/permission/IME dialogs and the original Cart Studio web editor
 * intentionally remain owned by Android / upstream Studio.
 */
internal object SlotUi {
    val background: Int = Color.rgb(23, 22, 25)
    val surface: Int = Color.rgb(37, 36, 43)
    val selected: Int = Color.rgb(68, 66, 77)
    val ink: Int = Color.rgb(246, 244, 239)
    val muted: Int = Color.rgb(174, 171, 180)
    val rule: Int = Color.rgb(91, 89, 96)
    val accent: Int = Color.rgb(210, 188, 134)
    val error: Int = Color.rgb(241, 160, 146)

    @Volatile private var cachedFont: Typeface? = null

    fun font(context: Context): Typeface {
        cachedFont?.let { return it }
        val typeface = runCatching {
            Typeface.createFromAsset(context.applicationContext.assets, "label.ttf")
        }.getOrElse { Typeface.create("sans-serif-condensed", Typeface.BOLD) }
        cachedFont = typeface
        return typeface
    }

    fun dp(context: Context, amount: Int): Int =
        (context.resources.displayMetrics.density * amount + 0.5f).toInt()

    fun plate(context: Context, fill: Int = surface, focused: Boolean = false): GradientDrawable =
        GradientDrawable().apply {
            setColor(fill)
            cornerRadius = dp(context, 4).toFloat()
            setStroke(dp(context, if (focused) 2 else 1), if (focused) ink else rule)
        }

    fun label(
        context: Context, value: String, sp: Float = 13f,
        color: Int = ink, upper: Boolean = true,
    ): TextView = TextView(context).apply {
        text = if (upper) value.uppercase(Locale.ROOT) else value
        typeface = font(context)
        textSize = sp
        letterSpacing = .09f
        includeFontPadding = false
        setTextColor(color)
        gravity = Gravity.CENTER_VERTICAL
    }

    fun button(context: Context, value: String, onClick: () -> Unit): TextView =
        label(context, value, 13f).apply {
            gravity = Gravity.CENTER
            isFocusable = true
            isClickable = true
            minHeight = dp(context, 41)
            setPadding(dp(context, 11), dp(context, 7), dp(context, 11), dp(context, 7))
            background = plate(context)
            setOnFocusChangeListener { view, focused ->
                view.background = plate(context, if (focused) selected else surface, focused)
            }
            setOnClickListener { onClick() }
        }

    fun field(context: Context, hint: String, password: Boolean = false): EditText =
        EditText(context).apply {
            this.hint = hint
            isSingleLine = true
            inputType = if (password)
                InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD
            else
                InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
            // Android changes a password field's font when inputType is set:
            // re-apply upstream label.ttf AFTER masking/IME flags.
            typeface = font(context)
            textSize = 15f
            setTextColor(ink)
            setHintTextColor(muted)
            letterSpacing = .04f
            imeOptions = if (password) EditorInfo.IME_ACTION_DONE
                else EditorInfo.IME_ACTION_NEXT
            setSelectAllOnFocus(false)
            setPadding(dp(context, 13), dp(context, 10),
                dp(context, 13), dp(context, 10))
            minHeight = dp(context, 48)
            background = plate(context)
            setOnFocusChangeListener { view, focused ->
                view.background = plate(context, surface, focused)
            }
        }
}
