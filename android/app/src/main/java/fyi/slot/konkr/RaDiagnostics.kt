package fyi.slot.konkr

import android.graphics.BitmapFactory
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.security.MessageDigest
import java.util.Locale

/**
 * Read-only account diagnostics. Always run off the Android UI thread.
 * Never log ROM URIs, user tokens, or the contents of the pending-award ledger.
 */
internal object RaDiagnostics {
    data class Snapshot(
        val libraryGba: Int?,
        val indexedGba: Int?,
        val cachedGba: Int?,
        val pendingAwards: Int?,
        val recordedAwards: Int?,
        val warning: String?,
        val prefetchBudgetUsed: Int?,
        val nextBatchMinutes: Int?,
        val badgesDownloaded: Int?,
        val badgesTotal: Int?,
        val badgesPercent: Int?,
    )

    internal fun accountKey(username: String): String {
        val bytes = username.lowercase(Locale.ROOT).toByteArray(Charsets.UTF_8)
        return MessageDigest.getInstance("MD5").digest(bytes)
            .joinToString("") { "%02x".format(it.toInt() and 0xff) }
    }

    private fun readJson(file: File, limit: Long = 8L * 1024 * 1024): JSONObject? =
        if (!file.isFile || file.length() !in 1L..limit) null
        else runCatching { JSONObject(file.readText(Charsets.UTF_8)) }.getOrNull()

    fun inspect(root: File, username: String): Snapshot {
        val account = File(root, "Saves/RetroAchievements/" + accountKey(username))
        val ledger = File(account, "unlocks.json")
        var pending: Int? = 0
        var recorded: Int? = 0
        var warning: String? = null
        if (ledger.exists()) {
            val objectData = readJson(ledger)
            if (objectData == null) {
                pending = null
                recorded = null
                warning = "Local achievements ledger cannot be read. It will be preserved."
            } else {
                val keys = objectData.keys()
                var count = 0
                var unsynced = 0
                var invalid = false
                while (keys.hasNext()) {
                    val award = objectData.optJSONObject(keys.next())
                    if (award == null || !award.has("synced")) { invalid = true; break }
                    count++
                    if (!award.optBoolean("synced")) unsynced++
                }
                if (invalid) {
                    pending = null
                    recorded = null
                    warning = "Local achievements ledger may be damaged. Nothing was deleted."
                } else {
                    pending = unsynced
                    recorded = count
                }
            }
        }

        val shelf = readJson(File(root, "rom-library-cache-v1.json"))
        val index = readJson(File(root, "ra-smart-rom-index-v1.json"))
        val games = shelf?.optJSONArray("games")
        val shelfRoot = shelf?.optString("root")
        val indexed = if (index?.optString("root") == shelfRoot &&
            index?.optInt("schema") == 1
        ) index.optJSONArray("entries") else null

        val hashesByUri = HashMap<String, String>()
        if (indexed != null) {
            for (i in 0 until indexed.length().coerceAtMost(5000)) {
                val item = indexed.optJSONObject(i) ?: continue
                val hash = item.optString("md5")
                if (hash.length == 32 && hash.all { it in '0'..'9' || it in 'a'..'f' }) {
                    hashesByUri[item.optString("uri")] = hash
                }
            }
        }

        var totalGba = 0
        val discovered = HashSet<String>()
        if (games != null) {
            for (i in 0 until games.length().coerceAtMost(5000)) {
                val item = games.optJSONObject(i) ?: continue
                if (item.optString("platform") != "GBA") continue
                totalGba++
                val hash = hashesByUri[item.optString("uri")] ?: continue
                discovered.add(hash)
            }
        }
        val budget = readJson(File(account, "smart-prefetch-budget.json"))
        val attempts = budget?.optJSONArray("attempts")
        val now = System.currentTimeMillis() / 1000
        var budgetUsed: Int? = null
        var nextBatchMinutes: Int? = null
        if (attempts != null) {
            val valid = (0 until attempts.length()).map { attempts.optLong(it, -1) }
                .filter { it in (now - 1799)..now }
            budgetUsed = valid.size
            if (valid.size >= 100) {
                val availableAt = (valid.minOrNull() ?: now) + 1800
                nextBatchMinutes = ((availableAt - now + 59) / 60).toInt().coerceAtLeast(1)
            }
        }
        var cached = 0
        // Include only unique official achievement badge IDs referenced by
        // valid, account-scoped GBA patches. The total grows as new games are
        // prepared, so this is "known badges", NOT percent of game preparation.
        val badgeIds = HashSet<String>()
        for (hash in discovered) {
            val file = File(account, "$hash.json")
            if (!file.isFile || file.length() !in 4L..(8L * 1024 * 1024)) continue
            // A cached game is a [patch, unlockedIds] array, not an icon download.
            val prepared = runCatching {
                val data = JSONArray(file.readText(Charsets.UTF_8))
                val patch = data.optJSONObject(0)
                if (data.length() != 2 || patch?.optInt("ConsoleID") != 5) false
                else {
                    val achievements = patch.optJSONArray("Achievements")
                    if (achievements != null) {
                        for (i in 0 until achievements.length()) {
                            val a = achievements.optJSONObject(i) ?: continue
                            if (a.optInt("Flags", 0) != 3) continue
                            val badge = a.optString("BadgeName")
                            if (badge.matches(Regex("[A-Za-z0-9_]{1,64}")))
                                badgeIds.add(badge)
                        }
                    }
                    true
                }
            }.getOrDefault(false)
            if (prepared) cached++
        }
        // This is deliberately run on the diagnostics I/O executor, never the
        // UI or GL thread. Bounding file size/dimensions prevents malformed or
        // missing images being counted as successfully downloaded badges.
        val badgeRoot = File(account, "badges")
        var validBadgeCount = 0
        for (name in badgeIds) {
            val file = File(badgeRoot, "$name.png")
            if (!file.isFile || file.length() !in 1L..(128L * 1024)) continue
            val valid = runCatching {
                val opts = BitmapFactory.Options().apply { inJustDecodeBounds = true }
                BitmapFactory.decodeFile(file.absolutePath, opts)
                opts.outWidth in 1..256 && opts.outHeight in 1..256
            }.getOrDefault(false)
            if (valid) validBadgeCount++
        }
        val badgeTotal = badgeIds.size.takeIf { it > 0 }
        return Snapshot(
            libraryGba = if (games != null) totalGba else null,
            indexedGba = if (games != null) discovered.size else null,
            cachedGba = if (games != null) cached else null,
            pendingAwards = pending,
            recordedAwards = recorded,
            warning = warning,
            prefetchBudgetUsed = budgetUsed,
            nextBatchMinutes = nextBatchMinutes,
            badgesDownloaded = if (badgeTotal != null) validBadgeCount else null,
            badgesTotal = badgeTotal,
            badgesPercent = badgeTotal?.let { validBadgeCount * 100 / it },
        )
    }
}
