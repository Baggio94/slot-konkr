package fyi.slot.konkr

import android.content.Context
import android.graphics.Color
import android.graphics.drawable.GradientDrawable
import android.view.Gravity
import android.view.View
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import android.widget.Toast

/**
 * Foreground-only, branded feedback for Slot's Android activities.
 *
 * Android 12's ordinary text Toast supplied a generic Android icon on KONKR,
 * despite the launcher having its own high-resolution adaptive icon. A small
 * custom foreground Toast consistently displays Slot's actual launcher art.
 * No overlay permission, no notification channel, and no background toasts.
 *
 * Exposes the same makeText(context, text, duration).show() interface so all
 * existing app feedback follows the same rendering and wording policy.
 */
internal object SlotToast {
    fun makeText(context: Context, message: CharSequence, duration: Int): Toast {
        val friendly = userMessage(message.toString())
        val density = context.resources.displayMetrics.density
        fun dp(value: Int) = (value * density + 0.5f).toInt()

        return try {
            val background = GradientDrawable().apply {
                shape = GradientDrawable.RECTANGLE
                setColor(Color.rgb(249, 246, 250))
                cornerRadius = dp(28).toFloat()
            }
            val row = LinearLayout(context).apply {
                orientation = LinearLayout.HORIZONTAL
                gravity = Gravity.CENTER_VERTICAL
                setPadding(dp(16), dp(12), dp(18), dp(12))
                this.background = background
                elevation = dp(5).toFloat()
            }
            val logo = ImageView(context).apply {
                setImageResource(R.mipmap.ic_launcher)
                contentDescription = "slot."
                scaleType = ImageView.ScaleType.FIT_CENTER
            }
            row.addView(logo, LinearLayout.LayoutParams(dp(34), dp(34)).apply {
                rightMargin = dp(12)
            })
            val text = TextView(context).apply {
                this.text = friendly
                textSize = 15f
                setTextColor(Color.rgb(30, 29, 34))
                maxLines = 3
                contentDescription = friendly
            }
            row.addView(text, LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.WRAP_CONTENT,
                LinearLayout.LayoutParams.WRAP_CONTENT
            ))
            @Suppress("DEPRECATION")
            Toast(context.applicationContext).apply {
                this.duration = duration
                setGravity(Gravity.BOTTOM or Gravity.CENTER_HORIZONTAL, 0, dp(56))
                view = row
            }
        } catch (_: Exception) {
            // A device-specific toast restriction must never break an action.
            Toast.makeText(context, friendly, duration)
        }
    }

    internal fun userMessage(raw: String): String {
        val msg = raw.trim()
        if (msg.isEmpty()) return "Done."
        return when {
            msg.equals("RAOfflineProxy unavailable", true) ->
                "RAOfflineProxy wasn't found. Install or open it to use achievements."
            msg.equals("RAOfflineProxy stopped", true) ->
                "RAOfflineProxy is stopped. Open it to check achievements."
            msg.equals("RAOfflineProxy address invalid", true) ->
                "RAOfflineProxy is running, but its connection settings need checking."
            msg.contains("ROM folder permission expired", true) ->
                "Access to your games folder expired. Please select the folder again."
            msg.contains("Choose a ROM folder first", true) ->
                "Choose the folder containing your games first."
            msg.contains("Choose a cartridge first", true) ->
                "Select a game in the carousel first."
            msg.contains("Reopen Slot to show studio changes", true) ->
                "Changes saved. Reopen slot. to refresh your cart labels."
            msg.startsWith("Original Slot. label restored") ->
                "Custom artwork removed. The default label is back."
            msg.startsWith("Slot loaded ") ->
                msg.replaceFirst("Slot loaded ", "Games ready: ")
                    .replace(" carts", " cartridges")
                    .replace(" (limited to 5000)", " (showing the first 5,000)")
            msg.contains("java.lang.", true) || msg.contains("Exception:", true) ||
                msg.contains("content://", true) ->
                "That didn't work. Check your files and try again."
            else -> msg
        }
    }
}
