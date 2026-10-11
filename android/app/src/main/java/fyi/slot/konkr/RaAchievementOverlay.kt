package fyi.slot.konkr

import android.app.Activity
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Color
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.os.Handler
import android.os.Looper
import android.view.Gravity
import android.view.View
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import java.io.File
import java.util.ArrayDeque
import java.util.Locale
import java.util.concurrent.Executors

/**
 * RetroArch-inspired achievement card with Slot's REAL upstream label.ttf.
 * An in-Activity overlay, NOT an Android Toast: precise upper-left placement,
 * cached achievement badge art, no SYSTEM_ALERT_WINDOW permission and no
 * changes to the EGL surface, input, emulation or award synchronisation.
 */
internal class RaAchievementOverlay(
    private val activity: Activity,
    parent: FrameLayout,
) {
    data class Notice(
        val title: String,
        val detail: String,
        val earned: Boolean,
        val badge: String?,
    )

    private val handler = Handler(Looper.getMainLooper())
    private val loader = Executors.newSingleThreadExecutor { task ->
        Thread(task, "slot-ra-badge-ui").apply { priority = Thread.MIN_PRIORITY }
    }
    private val queue = ArrayDeque<Notice>()
    private var showing = false
    private var destroyed = false
    private var serial = 0
    private val ink = Color.rgb(246, 244, 239)
    private val secondary = Color.rgb(184, 180, 180)
    private val accent = Color.rgb(210, 188, 134)
    private val face: Typeface by lazy {
        runCatching { Typeface.createFromAsset(activity.assets, "label.ttf") }
            .getOrElse { Typeface.create("sans-serif-condensed", Typeface.BOLD) }
    }
    private fun dp(x: Int) = (activity.resources.displayMetrics.density * x + 0.5f).toInt()

    // Android 12 clips a one-pixel GradientDrawable stroke at some display
    // densities. Draw this inset outline in the foreground AFTER text/images:
    // all four edges remain visible, including the right-hand side.
    private val outline = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = Color.rgb(126, 122, 132)
        style = Paint.Style.STROKE
        strokeWidth = dp(2).toFloat()
    }
    private val card = object : LinearLayout(activity) {
        override fun onDrawForeground(canvas: Canvas) {
            super.onDrawForeground(canvas)
            val inset = outline.strokeWidth / 2 + dp(1)
            if (width > dp(4) && height > dp(4)) {
                canvas.drawRoundRect(
                    RectF(inset, inset, width - inset, height - inset),
                    dp(4).toFloat(), dp(4).toFloat(), outline
                )
            }
        }
    }.apply {
        orientation = LinearLayout.HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
        setPadding(dp(12), dp(9), dp(14), dp(9))
        background = GradientDrawable().apply {
            setColor(Color.rgb(23, 22, 25))
            cornerRadius = dp(4).toFloat()
        }
        elevation = dp(3).toFloat()
        isClickable = false
        isFocusable = false
        visibility = View.GONE
    }
    private val badge = ImageView(activity).apply {
        scaleType = ImageView.ScaleType.FIT_CENTER
        contentDescription = "RetroAchievements badge"
    }
    private val badgeFallback = TextView(activity).apply {
        text = "★"
        textSize = 24f
        gravity = Gravity.CENTER
        setTextColor(accent)
        background = GradientDrawable().apply {
            setColor(Color.rgb(47, 43, 40))
            cornerRadius = dp(3).toFloat()
        }
    }
    private val texts = LinearLayout(activity).apply { orientation = LinearLayout.VERTICAL }
    private val heading = label(10.5f, accent)
    private val title = label(15f, ink).apply { maxLines = 2 }
    private val subtitle = label(11f, secondary).apply { maxLines = 2 }

    init {
        card.addView(badge, LinearLayout.LayoutParams(dp(46), dp(46)).apply {
            rightMargin = dp(11)
        })
        card.addView(badgeFallback, LinearLayout.LayoutParams(dp(46), dp(46)).apply {
            rightMargin = dp(11)
        })
        texts.addView(heading)
        texts.addView(title)
        texts.addView(subtitle)
        card.addView(texts, LinearLayout.LayoutParams(0, -2, 1f))
        val maxWidth = minOf(dp(358), (activity.resources.displayMetrics.widthPixels * .68f).toInt())
        parent.addView(card, FrameLayout.LayoutParams(maxWidth, -2, Gravity.TOP or Gravity.START).apply {
            leftMargin = dp(15)
            topMargin = dp(14)
        })
        card.bringToFront()
    }

    private fun label(size: Float, colour: Int) = TextView(activity).apply {
        typeface = face
        textSize = size
        setTextColor(colour)
        letterSpacing = 0.09f
        includeFontPadding = false
        gravity = Gravity.CENTER_VERTICAL
        ellipsize = android.text.TextUtils.TruncateAt.END
    }

    /** Only called on Activity's main thread. Never blocks gameplay. */
    fun push(notice: Notice) {
        if (destroyed) return
        // Keep real unlocked rewards over ordinary status updates if busy.
        if (queue.size >= 12) {
            val iterator = queue.iterator()
            var removedStatus = false
            while (iterator.hasNext()) {
                if (!iterator.next().earned) {
                    iterator.remove()
                    removedStatus = true
                    break
                }
            }
            if (!removedStatus) queue.removeFirst()
        }
        queue.addLast(notice)
        if (!showing) showNext()
    }

    private fun showNext() {
        if (destroyed) return
        val notice = queue.pollFirst() ?: run {
            showing = false
            card.visibility = View.GONE
            return
        }
        showing = true
        val seq = ++serial
        val status = notice.title.trim()
        if (notice.earned) {
            heading.text = "ACHIEVEMENT EARNED  •  SYNC PENDING"
            title.text = status.uppercase(Locale.ROOT)
            subtitle.text = notice.detail.uppercase(Locale.ROOT)
            subtitle.visibility = if (notice.detail.isBlank()) View.GONE else View.VISIBLE
            badge.visibility = View.GONE
            badgeFallback.visibility = View.VISIBLE
            loadBadge(notice.badge, seq)
        } else {
            // Concise system notifications: server-sync state does not masquerade
            // as an achievement unlock and never displays an unrelated badge.
            heading.text = "RETROACHIEVEMENTS"
            title.text = status.uppercase(Locale.ROOT)
            subtitle.text = notice.detail.uppercase(Locale.ROOT)
            subtitle.visibility = if (notice.detail.isBlank()) View.GONE else View.VISIBLE
            badge.visibility = View.GONE
            badgeFallback.visibility = View.GONE
        }
        card.visibility = View.VISIBLE
        card.animate().cancel()
        card.alpha = 0f
        card.translationX = -dp(18).toFloat()
        card.animate().alpha(1f).translationX(0f).setDuration(220).start()
        handler.postDelayed({
            if (seq == serial && !destroyed) {
                card.animate().alpha(0f).translationX(-dp(12).toFloat())
                    .setDuration(180).withEndAction {
                        if (seq == serial && !destroyed) showNext()
                    }.start()
            }
        }, if (notice.earned) 3600L else 1750L)
    }

    private fun loadBadge(path: String?, seq: Int) {
        if (path.isNullOrBlank()) return
        loader.execute {
            // The native HTTP worker may still be downloading this exact badge.
            // Retry for a short bounded window, never downloading on UI/GL.
            for (attempt in 0 until 7) {
                if (Thread.currentThread().isInterrupted || destroyed || serial != seq) break
                val bitmap = loadValidatedBadge(path)
                if (bitmap != null) {
                    handler.post {
                        if (seq == serial && !destroyed) {
                            badge.setImageBitmap(bitmap)
                            badge.visibility = View.VISIBLE
                            badgeFallback.visibility = View.GONE
                        } else bitmap.recycle()
                    }
                    break
                }
                if (attempt < 6) {
                    try { Thread.sleep(350L) }
                    catch (_: InterruptedException) {
                        Thread.currentThread().interrupt()
                        break
                    }
                }
            }
        }
    }

    /** Only cached, bounded, app-private RA PNGs; never fetch art on UI thread. */
    private fun loadValidatedBadge(path: String): Bitmap? = runCatching {
        val safeRoot = File(activity.filesDir, "Saves/RetroAchievements").canonicalFile
        val file = File(path).canonicalFile
        if (!file.path.startsWith(safeRoot.path + File.separator) ||
            file.parentFile?.name != "badges" ||
            !file.name.matches(Regex("[A-Za-z0-9_]{1,64}\\.png")) ||
            !file.isFile || file.length() !in 1L..(128L * 1024)
        ) return@runCatching null
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(file.path, bounds)
        if (bounds.outWidth !in 1..256 || bounds.outHeight !in 1..256) return@runCatching null
        BitmapFactory.decodeFile(file.path)
    }.getOrNull()

    fun destroy() {
        destroyed = true
        ++serial
        queue.clear()
        handler.removeCallbacksAndMessages(null)
        card.animate().cancel()
        card.visibility = View.GONE
        loader.shutdownNow()
    }
}
