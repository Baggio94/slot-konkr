package fyi.slot.konkr

import android.app.Activity
import android.app.role.RoleManager
import android.content.ActivityNotFoundException
import android.content.Intent
import android.graphics.BitmapFactory
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.Drawable
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.util.Log
import android.view.Gravity
import android.view.KeyEvent
import android.view.View
import android.view.ViewGroup
import android.widget.BaseAdapter
import android.widget.GridView
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import android.widget.Toast

/**
 * Optional HOME launcher with a manually managed D-pad selection.
 *
 * GridView's own focus manager swallowed the KONKR gamepad arrows, and A
 * could not reach "Set as Home". Consume controller keys in dispatchKeyEvent
 * before child views and draw a visible selected tile on every interaction.
 * Touch taps and Android's standard B/Back behavior still work.
 */
class AppsActivity : Activity() {
    companion object {
        private const val TAG = "SlotApps"
        private const val COLUMNS = 5
    }

    private data class Entry(val label: String, val packageName: String, val launch: Intent)
    private var entries: List<Entry> = emptyList()
    private lateinit var grid: GridView
    private lateinit var homeButton: TextView
    private lateinit var appsAdapter: BaseAdapter
    private var selectedIndex = 0
    private var homeSelected = false
    // Cache icon decodes and reuse tile views rather than rebuilding the grid
    // on every D-pad event (which caused sluggish movement on the KONKR).
    private val iconCache = HashMap<String, Drawable>()
    private data class Tile(val icon: ImageView, val title: TextView)

    private fun dp(value: Int) = (value * resources.displayMetrics.density + .5f).toInt()

    private fun text(label: String, size: Float, bright: Boolean = true) = TextView(this).apply {
        this.text = label
        textSize = size
        typeface = SlotUi.font(this)
        setTextColor(if (bright) SlotUi.ink else SlotUi.muted)
        gravity = Gravity.CENTER_VERTICAL
    }

    private fun plate(color: Int, selected: Boolean = false) = GradientDrawable().apply {
        setColor(color)
        cornerRadius = dp(8).toFloat()
        if (selected) setStroke(dp(2), Color.rgb(228, 226, 236))
    }

    private fun loadApps() {
        val intent = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER)
        entries = packageManager.queryIntentActivities(intent, 0)
            .mapNotNull { info ->
                val component = info.activityInfo ?: return@mapNotNull null
                if (component.packageName == packageName) return@mapNotNull null
                val launch = Intent(Intent.ACTION_MAIN)
                    .addCategory(Intent.CATEGORY_LAUNCHER)
                    .setClassName(component.packageName, component.name)
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or
                        Intent.FLAG_ACTIVITY_RESET_TASK_IF_NEEDED)
                Entry(info.loadLabel(packageManager).toString(), component.packageName, launch)
            }
            .distinctBy { it.launch.component?.flattenToString() }
            .sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.label })
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        @Suppress("DEPRECATION")
        window.decorView.systemUiVisibility = (
            View.SYSTEM_UI_FLAG_FULLSCREEN or View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or
            View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY or
            View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or
            View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
        )
        loadApps()
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(SlotUi.background)
            setPadding(dp(22), dp(10), dp(22), dp(15))
            isFocusableInTouchMode = true
        }
        setContentView(root)

        val header = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }
        header.addView(text("APPS", 19f), LinearLayout.LayoutParams(0, dp(48), 1f))
        homeButton = text("Set as Home", 13f).apply {
            gravity = Gravity.CENTER
            setPadding(dp(15), dp(7), dp(15), dp(7))
            isClickable = true
            contentDescription = "Choose the default home launcher"
            setOnClickListener { chooseHome() }
        }
        header.addView(homeButton, LinearLayout.LayoutParams(-2, dp(38)))
        root.addView(header)
        root.addView(View(this).apply {
            setBackgroundColor(SlotUi.rule)
        }, LinearLayout.LayoutParams(-1, dp(1)))

        grid = GridView(this).apply {
            numColumns = COLUMNS
            horizontalSpacing = dp(12)
            verticalSpacing = dp(12)
            stretchMode = GridView.STRETCH_COLUMN_WIDTH
            selector = plate(Color.TRANSPARENT)
            clipToPadding = false
            setPadding(0, dp(3), 0, dp(8))
        }
        appsAdapter = object : BaseAdapter() {
            override fun getCount() = entries.size
            override fun getItem(position: Int) = entries[position]
            override fun getItemId(position: Int) = position.toLong()

            override fun getView(position: Int, convertView: View?, parent: ViewGroup): View {
                val entry = entries[position]
                val root: LinearLayout
                val tile: Tile
                if (convertView is LinearLayout && convertView.tag is Tile) {
                    root = convertView
                    tile = root.tag as Tile
                } else {
                    root = LinearLayout(this@AppsActivity).apply {
                        orientation = LinearLayout.VERTICAL
                        gravity = Gravity.CENTER
                        minimumHeight = dp(110)
                        setPadding(dp(5), dp(9), dp(5), dp(7))
                    }
                    val icon = ImageView(this@AppsActivity).apply {
                        scaleType = ImageView.ScaleType.FIT_CENTER
                    }
                    root.addView(icon, LinearLayout.LayoutParams(dp(46), dp(46)))
                    val title = text("", 12f).apply {
                        gravity = Gravity.CENTER
                        maxLines = 2
                        ellipsize = android.text.TextUtils.TruncateAt.END
                    }
                    root.addView(title, LinearLayout.LayoutParams(-1, dp(42)))
                    tile = Tile(icon, title)
                    root.tag = tile
                }
                val cached = iconCache.getOrPut(entry.packageName) {
                    runCatching { packageManager.getApplicationIcon(entry.packageName) }
                        .getOrElse { packageManager.defaultActivityIcon }
                }
                tile.icon.setImageDrawable(cached.constantState?.newDrawable(resources) ?: cached)
                tile.title.text = entry.label
                root.contentDescription = entry.label
                val selected = !homeSelected && position == selectedIndex
                root.background = plate(
                    if (selected) Color.rgb(68, 66, 77) else Color.rgb(37, 36, 43),
                    selected
                )
                return root
            }
        }
        grid.adapter = appsAdapter
        grid.setOnItemClickListener { _, _, position, _ ->
            val oldIndex = selectedIndex
            val wasHome = homeSelected
            selectedIndex = position
            homeSelected = false
            updateSelection(oldIndex, wasHome)
            launchSelected()
        }
        root.addView(grid, LinearLayout.LayoutParams(-1, 0, 1f))
        root.addView(controllerLegend())
        updateSelection(null, false)
        root.requestFocus()
    }

    /**
     * Pixel-for-pixel genuine Slot controller hints, rendered during the build
     * by slot_ui::plate::{arrows_hint_face,hint_face} from the same fonts and
     * actual Nerd Font arrow glyphs as the native core-selection screen.
     *
     * Slot's logical canvas is 720×480; scale each 24px face by the same
     * factor as that renderer on the KONKR display. Do not recreate keycaps
     * with Android TextView / Unicode substitutes.
     */
    private fun controllerLegend(): View {
        val scale = resources.displayMetrics.widthPixels.toFloat() / 720f
        fun hint(asset: String): ImageView {
            val bitmap = assets.open("apps_controls/$asset.png").use { stream ->
                requireNotNull(BitmapFactory.decodeStream(stream)) {
                    "Missing original Slot legend asset: $asset"
                }
            }
            return ImageView(this).apply {
                setImageBitmap(bitmap)
                scaleType = ImageView.ScaleType.FIT_XY
                contentDescription = when (asset) {
                    "move" -> "Left and right arrows: Move"
                    "open" -> "A: Open"
                    else -> "B: Back"
                }
                layoutParams = LinearLayout.LayoutParams(
                    (bitmap.width * scale).toInt().coerceAtLeast(1),
                    (bitmap.height * scale).toInt().coerceAtLeast(1)
                )
            }
        }
        val spacing = (36f * scale).toInt().coerceAtLeast(dp(16))
        return LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER
            setPadding(0, dp(8), 0, dp(7))
            addView(hint("move"))
            val open = hint("open")
            (open.layoutParams as LinearLayout.LayoutParams).leftMargin = spacing
            addView(open)
            val back = hint("back")
            (back.layoutParams as LinearLayout.LayoutParams).leftMargin = spacing
            addView(back)
        }
    }

    private fun redrawTile(position: Int) {
        if (position !in entries.indices) return
        val tile = grid.getChildAt(position - grid.firstVisiblePosition) ?: return
        val selected = !homeSelected && position == selectedIndex
        tile.background = plate(
            if (selected) Color.rgb(68, 66, 77) else Color.rgb(37, 36, 43),
            selected
        )
    }

    private fun updateSelection(oldIndex: Int?, oldHome: Boolean) {
        if (homeSelected != oldHome || oldIndex == null) {
            homeButton.background = plate(
                if (homeSelected) Color.rgb(89, 87, 102) else Color.rgb(56, 55, 64),
                homeSelected
            )
        }
        // Two lightweight drawables only; never notifyDataSetChanged()
        // or re-fetch launcher icons on controller navigation.
        oldIndex?.let(::redrawTile)
        redrawTile(selectedIndex)
        if (!homeSelected && entries.isNotEmpty()) {
            keepSelectedRowVisible()
        }
    }

    /**
     * A selected tile can be partially onscreen at the end of a GridView
     * page. Android treats that as visible so setSelection() alone doesn't
     * scroll; the highlighted third row ended up behind the footer.
     * Scroll only the minimum missing distance, not a whole page.
     */
    private fun keepSelectedRowVisible() {
        val requested = selectedIndex
        grid.post {
            if (homeSelected || requested != selectedIndex || requested !in entries.indices)
                return@post
            val item = grid.getChildAt(requested - grid.firstVisiblePosition)
            if (item == null) {
                val firstOfRow = requested / COLUMNS * COLUMNS
                grid.smoothScrollToPositionFromTop(firstOfRow, dp(9), 165)
                return@post
            }
            val top = grid.paddingTop + dp(8)
            val bottom = grid.height - grid.paddingBottom - dp(8)
            when {
                item.bottom > bottom -> grid.smoothScrollBy(item.bottom - bottom, 165)
                item.top < top -> grid.smoothScrollBy(item.top - top, 165)
            }
        }
    }

    private fun navigate(key: Int) {
        val oldIndex = selectedIndex
        val wasHome = homeSelected
        if (homeSelected) {
            if (key == KeyEvent.KEYCODE_DPAD_DOWN && entries.isNotEmpty()) {
                homeSelected = false
                selectedIndex = selectedIndex.coerceIn(0, entries.lastIndex)
            }
        } else if (entries.isNotEmpty()) {
            selectedIndex = when (key) {
                KeyEvent.KEYCODE_DPAD_LEFT -> (selectedIndex - 1).coerceAtLeast(0)
                KeyEvent.KEYCODE_DPAD_RIGHT -> (selectedIndex + 1).coerceAtMost(entries.lastIndex)
                KeyEvent.KEYCODE_DPAD_DOWN ->
                    (selectedIndex + COLUMNS).coerceAtMost(entries.lastIndex)
                KeyEvent.KEYCODE_DPAD_UP -> {
                    if (selectedIndex < COLUMNS) {
                        homeSelected = true
                        selectedIndex
                    } else selectedIndex - COLUMNS
                }
                else -> selectedIndex
            }
        }
        if (oldIndex != selectedIndex || wasHome != homeSelected) {
            updateSelection(oldIndex, wasHome)
        }
    }

    private fun launchSelected() {
        if (selectedIndex !in entries.indices) return
        try {
            startActivity(entries[selectedIndex].launch)
        } catch (error: Exception) {
            Log.w(TAG, "Unable to open " + entries[selectedIndex].packageName, error)
            SlotToast.makeText(this, "Couldn't open that app.", Toast.LENGTH_SHORT).show()
        }
    }

    private fun chooseHome() {
        // Prefer Android's actual Home selection settings on the KONKR.
        // The RoleManager dialog is the fallback, not a silent replacement.
        try {
            startActivity(Intent(Settings.ACTION_HOME_SETTINGS))
            Log.i(TAG, "Opened Android Home app settings")
            return
        } catch (error: Exception) {
            Log.w(TAG, "Android Home settings unavailable; trying role chooser", error)
        }
        try {
            if (Build.VERSION.SDK_INT >= 29) {
                val manager = getSystemService(RoleManager::class.java)
                if (manager?.isRoleAvailable(RoleManager.ROLE_HOME) == true) {
                    if (manager.isRoleHeld(RoleManager.ROLE_HOME)) {
                        SlotToast.makeText(this, "slot. is already your Home app.",
                            Toast.LENGTH_SHORT).show()
                    } else {
                        startActivity(manager.createRequestRoleIntent(RoleManager.ROLE_HOME))
                    }
                    return
                }
            }
        } catch (error: Exception) {
            Log.w(TAG, "Android Home role chooser unavailable", error)
        }
        SlotToast.makeText(this,
            "This device doesn't provide a Home app chooser.",
            Toast.LENGTH_LONG).show()
    }

    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        // Intercept *before* GridView gets first refusal. This applies to the
        // KONKR controls and standard keyboard D-pad events.
        when (event.keyCode) {
            KeyEvent.KEYCODE_DPAD_LEFT, KeyEvent.KEYCODE_DPAD_RIGHT,
            KeyEvent.KEYCODE_DPAD_UP, KeyEvent.KEYCODE_DPAD_DOWN -> {
                if (event.action == KeyEvent.ACTION_DOWN) navigate(event.keyCode)
                return true
            }
            KeyEvent.KEYCODE_BUTTON_A, KeyEvent.KEYCODE_DPAD_CENTER,
            KeyEvent.KEYCODE_ENTER -> {
                if (event.action == KeyEvent.ACTION_DOWN && event.repeatCount == 0) {
                    if (homeSelected) chooseHome() else launchSelected()
                }
                return true
            }
            KeyEvent.KEYCODE_BUTTON_B, KeyEvent.KEYCODE_BACK -> {
                if (event.action == KeyEvent.ACTION_DOWN && event.repeatCount == 0) finish()
                return true
            }
        }
        return super.dispatchKeyEvent(event)
    }
}
