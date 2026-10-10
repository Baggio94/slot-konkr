package fyi.slot.konkr

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import org.json.JSONArray
import org.json.JSONObject
import java.util.ArrayDeque
import java.util.HashSet
import java.util.Locale

/**
 * Read-only ROM discovery using the Storage Access Framework.
 * Only file names, small cartridge headers and opaque content URIs are read.
 * No commercial games, BIOS files, or writable access are requested or copied.
 */
internal object RomLibrary {
    private const val MAX_ROMS = 5000
    private const val MAX_DIRECTORIES = 1500
    private const val MAX_DEPTH = 12

    internal data class ScanResult(val json: String, val count: Int, val truncated: Boolean)
    private data class Directory(val id: String, val depth: Int, val folder: String)

    fun scan(context: Context, treeUri: Uri): ScanResult {
        val resolver = context.contentResolver
        val rootId = DocumentsContract.getTreeDocumentId(treeUri)
        val pending = ArrayDeque<Directory>()
        val seen = HashSet<String>()
        pending.add(Directory(rootId, 0, ""))
        val games = JSONArray()
        var directories = 0
        var truncated = false
        val columns = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE
        )

        while (pending.isNotEmpty()) {
            if (directories >= MAX_DIRECTORIES || games.length() >= MAX_ROMS) {
                truncated = true
                break
            }
            val dir = pending.removeFirst()
            if (!seen.add(dir.id)) continue
            directories++
            val children = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, dir.id)
            resolver.query(children, columns, null, null, null)?.use { cursor ->
                val idIndex = cursor.getColumnIndexOrThrow(columns[0])
                val nameIndex = cursor.getColumnIndexOrThrow(columns[1])
                val mimeIndex = cursor.getColumnIndexOrThrow(columns[2])
                while (cursor.moveToNext()) {
                    val id = cursor.getString(idIndex) ?: continue
                    val name = cursor.getString(nameIndex) ?: continue
                    val mime = cursor.getString(mimeIndex) ?: continue
                    if (name.startsWith(".")) continue
                    if (mime == DocumentsContract.Document.MIME_TYPE_DIR) {
                        if (dir.depth < MAX_DEPTH && !ignoredDirectory(name)) {
                            pending.add(Directory(id, dir.depth + 1, name))
                        }
                        continue
                    }

                    val extension = name.substringAfterLast('.', "").lowercase(Locale.ROOT)
                    if (extension != "gba" && extension != "gbc" && extension != "gb") continue
                    if (games.length() >= MAX_ROMS) {
                        truncated = true
                        break
                    }
                    val fileUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, id)
                    val header = cartridgeHeader(context, fileUri)
                    val gba = extension == "gba"
                    val colorOnly = !gba && header.size > 0x143 && (header[0x143].toInt() and 0xff) == 0xc0
                    val container = dir.folder.lowercase(Locale.ROOT)
                    val gbColor = !gba && (extension == "gbc" || colorOnly ||
                        container == "gbc" || container == "game boy color")
                    val platform = when {
                        gba -> "GBA"
                        gbColor -> "GBC"
                        else -> "GB"
                    }
                    val code = if (gba && header.size >= 0xb0) {
                        String(header, 0xac, 4, Charsets.US_ASCII)
                            .takeIf { it.all { c -> c in 'A'..'Z' || c in '0'..'9' } }
                            ?: ""
                    } else ""
                    // Keep the shelf title identical to the RetroArch save basename.
                    val title = name.substringBeforeLast('.').trim().take(200)
                    if (title.isEmpty()) continue
                    games.put(
                        JSONObject()
                            .put("title", title)
                            .put("platform", platform)
                            .put("uri", fileUri.toString())
                            .put("code", code)
                            .put("color_only", colorOnly || extension == "gbc")
                    )
                }
            }
        }
        return ScanResult(games.toString(), games.length(), truncated)
    }

    private fun ignoredDirectory(name: String): Boolean {
        return name.lowercase(Locale.ROOT) in setOf(
            "bios", "saves", "savestates", "states", "screenshots", "videos",
            "themes", "media", "images", "artwork", "covers", "system volume information"
        )
    }

    private fun cartridgeHeader(context: Context, fileUri: Uri): ByteArray {
        return try {
            context.contentResolver.openInputStream(fileUri)?.use { stream ->
                val header = ByteArray(0x150)
                var n = 0
                while (n < header.size) {
                    val bytes = stream.read(header, n, header.size - n)
                    if (bytes <= 0) break
                    n += bytes
                }
                header.copyOf(n)
            } ?: byteArrayOf()
        } catch (_: Exception) {
            byteArrayOf()
        }
    }
}
