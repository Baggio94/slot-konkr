package fyi.slot.konkr

import android.app.Activity
import android.app.role.RoleManager
import android.content.ActivityNotFoundException
import android.content.Intent
import android.graphics.Color
import android.graphics.drawable.GradientDrawable
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.util.Log
import android.view.Gravity
import android.view.KeyEvent
import android.view.View
import android.widget.BaseAdapter
import android.widget.FrameLayout
import android.widget.GridView
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView

/**
 * Opt-in home launcher and controller-friendly app drawer.
 * Lists only launchable apps exposed by the Android PackageManager; no
 * QUERY_ALL_PACKAGES, accessibility service, overlay or admin permission.
 */
class AppsActivity : Activity() {
    companion object { private const val TAG = "SlotApps" }
    private data class Entry(val label: String, val packageName: String, val launch: Intent)
    private var entries: List<Entry> = emptyList()

    private fun dp(n: Int) = (resources.displayMetrics.density * n + .5f).toInt()
    private fun label(s: String, sp: Float, white: Boolean = true) = TextView(this).apply {
        text = s
        textSize = sp
        setTextColor(if (white) Color.WHITE else Color.rgb(170, 169, 180))
        gravity = Gravity.CENTER_VERTICAL
        typeface = android.graphics.Typeface.create("sans-serif", android.graphics.Typeface.BOLD)
    }
    private fun background(colour: Int): GradientDrawable = GradientDrawable().apply {
        setColor(colour)
        cornerRadius = dp(8).toFloat()
    }
    private fun loadApps() {
        val query = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER)
        val resolve = packageManager.queryIntentActivities(query, 0)
        entries = resolve.mapNotNull { candidate ->
            val packageName = candidate.activityInfo?.packageName ?: return@mapNotNull null
            if (packageName == this.packageName) return@mapNotNull null
            val launch = Intent(Intent.ACTION_MAIN)
                .addCategory(Intent.CATEGORY_LAUNCHER)
                .setClassName(packageName, candidate.activityInfo.name)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_RESET_TASK_IF_NEEDED)
            Entry(candidate.loadLabel(packageManager).toString(), packageName, launch)
        }.distinctBy { it.launch.component?.flattenToString() }
         .sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.label })
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.decorView.systemUiVisibility = (View.SYSTEM_UI_FLAG_FULLSCREEN or
            View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY or
            View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION or View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN)
        loadApps()
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.rgb(19, 19, 23))
            setPadding(dp(22), dp(10), dp(22), dp(16))
        }
        setContentView(root)
        val titleRow = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }
        val back = label("‹  slot.", 18f).apply {
            isFocusable = true
            setPadding(dp(8), dp(8), dp(18), dp(8))
            setOnClickListener { finish() }
            contentDescription = "Back to slot. carousel"
        }
        titleRow.addView(back)
        titleRow.addView(label("APPS", 17f), LinearLayout.LayoutParams(0, dp(48), 1f))
        val home = label("Set as Home", 13f).apply {
            setPadding(dp(12), dp(8), dp(12), dp(8))
            background = background(Color.rgb(65, 64, 73))
            isFocusable = true
            setOnClickListener { chooseHome() }
            contentDescription = "Choose slot. or another default home app"
        }
        titleRow.addView(home)
        root.addView(titleRow)
        val divider = View(this).apply { setBackgroundColor(Color.rgb(68, 67, 75)) }
        root.addView(divider, LinearLayout.LayoutParams(-1, dp(1)))
        root.addView(label("A  OPEN      B  BACK      •  HOME IS OPTIONAL", 11f, false).apply {
            setPadding(0, dp(12), 0, dp(12))
        })
        val grid = GridView(this).apply {
            numColumns = 5
            horizontalSpacing = dp(12)
            verticalSpacing = dp(12)
            stretchMode = GridView.STRETCH_COLUMN_WIDTH
            isFocusable = true
            selector = background(Color.rgb(76, 74, 88))
            clipToPadding = false
            setPadding(0, dp(3), 0, dp(8))
        }
        grid.adapter = object : BaseAdapter() {
            override fun getCount() = entries.size
            override fun getItem(position: Int) = entries[position]
            override fun getItemId(position: Int) = position.toLong()
            override fun getView(position: Int, convertView: View?, parent: android.view.ViewGroup): View {
                val entry = entries[position]
                return LinearLayout(this@AppsActivity).apply {
                    orientation = LinearLayout.VERTICAL
                    gravity = Gravity.CENTER
                    background = background(Color.rgb(38, 37, 45))
                    minimumHeight = dp(110)
                    setPadding(dp(6), dp(9), dp(6), dp(7))
                    addView(ImageView(this@AppsActivity).apply {
                        setImageDrawable(try {
                            packageManager.getApplicationIcon(entry.packageName)
                        } catch (_: Exception) { null })
                        scaleType = ImageView.ScaleType.FIT_CENTER
                    }, LinearLayout.LayoutParams(dp(46), dp(46)))
                    addView(label(entry.label, 12f).apply {
                        gravity = Gravity.CENTER
                        maxLines = 2
                        ellipsize = android.text.TextUtils.TruncateAt.END
                    }, LinearLayout.LayoutParams(-1, dp(42)))
                    contentDescription = entry.label
                }
            }
        }
        grid.setOnItemClickListener { _, _, index, _ ->
            try { startActivity(entries[index].launch) }
            catch (e: Exception) {
                Log.w(TAG, "Can't open " + entries[index].packageName, e)
                SlotToast.makeText(this, "Couldn't open that app.", android.widget.Toast.LENGTH_SHORT).show()
            }
        }
        root.addView(grid, LinearLayout.LayoutParams(-1, 0, 1f))
        grid.requestFocus()
    }

    private fun chooseHome() {
        try {
            if (Build.VERSION.SDK_INT >= 29) {
                val manager = getSystemService(RoleManager::class.java)
                if (manager?.isRoleAvailable(RoleManager.ROLE_HOME) == true) {
                    startActivity(manager.createRequestRoleIntent(RoleManager.ROLE_HOME))
                    return
                }
            }
            startActivity(Intent(Settings.ACTION_HOME_SETTINGS))
        } catch (e: ActivityNotFoundException) {
            SlotToast.makeText(this, "Open Android Settings to choose your home app.",
                android.widget.Toast.LENGTH_LONG).show()
        }
    }

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        if (keyCode == KeyEvent.KEYCODE_BUTTON_B || keyCode == KeyEvent.KEYCODE_BACK) {
            finish()
            return true
        }
        if (keyCode == KeyEvent.KEYCODE_BUTTON_A || keyCode == KeyEvent.KEYCODE_DPAD_CENTER) {
            val focused = currentFocus
            if (focused is GridView && focused.selectedItemPosition >= 0) {
                val selected = focused.selectedItemPosition
                focused.performItemClick(focused.getChildAt(selected - focused.firstVisiblePosition),
                    selected, selected.toLong())
                return true
            }
            if (focused?.isClickable == true) return focused.performClick()
        }
        return super.onKeyDown(keyCode, event)
    }
}
