package fyi.slot.konkr

import android.util.AtomicFile
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
        var cached = 0
        for (hash in discovered) {
            val file = File(account, "$hash.json")
            if (!file.isFile || file.length() !in 4L..(8L * 1024 * 1024)) continue
            // A cached game is a [patch, unlockedIds] array, not an icon download.
            val prepared = runCatching {
                val data = JSONArray(file.readText(Charsets.UTF_8))
                data.length() == 2 && data.optJSONObject(0)?.optInt("ConsoleID") == 5
            }.getOrDefault(false)
            if (prepared) cached++
        }
        return Snapshot(
            libraryGba = if (games != null) totalGba else null,
            indexedGba = if (games != null) discovered.size else null,
            cachedGba = if (games != null) cached else null,
            pendingAwards = pending,
            recordedAwards = recorded,
            warning = warning,
        )
    }
}
