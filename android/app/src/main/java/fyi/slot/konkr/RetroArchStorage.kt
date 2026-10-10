package fyi.slot.konkr

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import android.util.Log
import java.io.File
import java.security.MessageDigest
import java.util.Locale

/**
 * RetroArch shared saves for "Sort Saves/States Into Folders By Core Name".
 * Operates only on user-approved READ+WRITE SAF tree roots, on a worker thread.
 * No direct /storage/emulated/0 or /Android/data paths are assumed.
 *
 * Layout: <chosen saves root>/mGBA/Game Name.srm or /gpSP/Game Name.srm
 *         <chosen states root>/mGBA/Game Name.state.auto (or gpSP).
 *
 * Never silently overwrite an external compressed state. Before replacing
 * existing saves, retain a separate untouched copy in SlotBackup/ as rollback.
 */
internal object RetroArchStorage {
    private const val TAG = "SlotKonkr"
    private const val MAX_SAVE = 2 * 1024 * 1024
    private const val MAX_STATE = 65 * 1024 * 1024

    data class Target(
        val romUri: String,
        val stem: String,
        val core: String,
        val saveLocal: File,
        val rtcLocal: File,
        val stateDefaultLocal: File,
        val stateImportLocal: File,
        val stateExportLocal: File,
        val savesTree: Uri?,
        val statesTree: Uri?
    )

    fun target(context: Context, rawUri: String, core: String, savesTree: Uri?, statesTree: Uri?): Target {
        require(core == "mGBA" || core == "gpSP")
        val name = queryRomName(context, Uri.parse(rawUri))
        val stem = name.substringBeforeLast('.', name)
            .trim().take(200)
        require(stem.isNotEmpty() && stem !in setOf(".", "..") &&
            stem.none { it == '/' || it == '\\' || it.code < 32 })
        val digest = MessageDigest.getInstance("SHA-256")
            .digest(rawUri.toByteArray(Charsets.UTF_8))
            .joinToString("") { "%02x".format(Locale.ROOT, it.toInt() and 255) }
        val prefix = File(context.filesDir, "Saves").apply { mkdirs() }
        val coreFolder = File(prefix, core).apply { mkdirs() }
        val defaultStates = File(File(context.filesDir, "States"), core).apply { mkdirs() }
        val coreLower = if (core == "mGBA") "mgba" else "gpsp"
        return Target(rawUri, stem, core,
            File(coreFolder, "$stem.srm"),
            File(coreFolder, "$stem.rtc"),
            File(defaultStates, "$stem.state.auto"),
            File(prefix, "$digest.$coreLower.retroarch-import.state.auto"),
            File(prefix, "$digest.$coreLower.retroarch-export.state.auto"),
            savesTree, statesTree)
    }

    fun importBeforeLaunch(context: Context, target: Target) {
        target.savesTree?.let { root ->
            find(context, root, target.core, target.stem + ".srm")?.let { doc ->
                val bytes = read(context, doc, MAX_SAVE)
                stage(target.saveLocal, bytes)
            }
            find(context, root, target.core, target.stem + ".rtc")?.let { doc ->
                val bytes = read(context, doc, 4096)
                require(bytes.isNotEmpty()) { "Empty RetroArch RTC" }
                stage(target.rtcLocal, bytes)
            }
        }
        val incoming = if (target.statesTree != null) {
            find(context, target.statesTree, target.core, target.stem + ".state.auto")
                ?.let { read(context, it, MAX_STATE) }
        } else if (target.stateDefaultLocal.isFile) {
            target.stateDefaultLocal.readBytes()
        } else null
        if (incoming != null) {
            try {
                // Rust validates RASTATE before passing the memory to libretro.
                stage(target.stateImportLocal, RetroArchCompression.decode(incoming))
            } catch (error: Exception) {
                Log.w(TAG, "RetroArch state not importable; private fallback preserved", error)
                target.stateImportLocal.delete()
            }
        } else {
            target.stateImportLocal.delete()
        }
    }

    /** Returns human-readable export errors; no silent data loss. */
    fun exportAfterSave(context: Context, target: Target): List<String> {
        val failures = mutableListOf<String>()
        if (target.savesTree != null && target.saveLocal.isFile) {
            try {
                val data = target.saveLocal.readBytes()
                if (data.isNotEmpty() && data.size <= MAX_SAVE) {
                    writeSaf(context, target.savesTree, target.core,
                        target.stem + ".srm", data, isState = false)
                }
            } catch (e: Exception) {
                Log.e(TAG, "RetroArch SRAM export failed", e)
                failures.add("Save sync failed: " + (e.message ?: "access denied"))
            }
        }
        if (target.statesTree != null && target.stateExportLocal.isFile) {
            try {
                val bytes = target.stateExportLocal.readBytes()
                require(bytes.size in 16..MAX_STATE && bytes.startsWithRASTATE()) {
                    "Invalid uncompressed RASTATE export"
                }
                // Keep the exact #RZIPv1# format used by this KONKR RetroArch.
                val compressed = RetroArchCompression.encode(bytes)
                writeSaf(context, target.statesTree, target.core,
                    target.stem + ".state.auto", compressed, isState = true)
            } catch (e: Exception) {
                Log.e(TAG, "RetroArch state export failed", e)
                failures.add("State sync skipped: " + (e.message ?: "unsupported format"))
            }
        }
        return failures
    }

    private fun queryRomName(context: Context, uri: Uri): String {
        context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME),
            null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) {
                val n = cursor.getString(0)
                if (!n.isNullOrBlank()) return n
            }
        }
        error("Cannot read ROM filename from Android document provider")
    }

    private fun ByteArray.startsWithRASTATE(): Boolean =
        size >= 8 && copyOfRange(0, 7).contentEquals("RASTATE".toByteArray()) && this[7] == 1.toByte()

    private fun children(context: Context, root: Uri, parent: String): List<Pair<String, Uri>> {
        val uri = DocumentsContract.buildChildDocumentsUriUsingTree(root, parent)
        val names = mutableListOf<Pair<String, Uri>>()
        val columns = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME
        )
        context.contentResolver.query(uri, columns, null, null, null)?.use { c ->
            val idCol = c.getColumnIndexOrThrow(columns[0])
            val nameCol = c.getColumnIndexOrThrow(columns[1])
            while (c.moveToNext()) {
                val id = c.getString(idCol) ?: continue
                val name = c.getString(nameCol) ?: continue
                names.add(name to DocumentsContract.buildDocumentUriUsingTree(root, id))
            }
        } ?: error("Cannot enumerate selected RetroArch folder")
        return names
    }

    private fun coreFolder(context: Context, root: Uri, core: String, create: Boolean): Uri? {
        val rootId = DocumentsContract.getTreeDocumentId(root)
        val existing = children(context, root, rootId).firstOrNull { it.first == core }?.second
        if (existing != null) return existing
        if (!create) return null
        return DocumentsContract.createDocument(context.contentResolver, root,
            DocumentsContract.Document.MIME_TYPE_DIR, core)
            ?: error("Cannot create RetroArch core folder: $core")
    }

    private fun find(context: Context, root: Uri, core: String, name: String): Uri? {
        val dir = coreFolder(context, root, core, create = false) ?: return null
        val id = DocumentsContract.getDocumentId(dir)
        return children(context, root, id).firstOrNull { it.first == name }?.second
    }

    private fun read(context: Context, uri: Uri, limit: Int): ByteArray =
        context.contentResolver.openInputStream(uri)?.use { input ->
            val out = java.io.ByteArrayOutputStream()
            val buffer = ByteArray(65536)
            while (true) {
                val len = input.read(buffer)
                if (len < 0) break
                out.write(buffer, 0, len)
                require(out.size() <= limit) { "RetroArch file exceeds safety limit" }
            }
            out.toByteArray()
        } ?: error("Cannot read RetroArch file")

    private fun stage(local: File, bytes: ByteArray) {
        val pending = File(local.parentFile, local.name + ".partial")
        try {
            pending.outputStream().use { it.write(bytes); it.fd.sync() }
            check(pending.renameTo(local)) { "Cannot import RetroArch file" }
        } finally {
            pending.delete()
        }
    }

    private fun writeSaf(context: Context, root: Uri, core: String,
                         name: String, bytes: ByteArray, isState: Boolean) {
        val resolver = context.contentResolver
        val folder = coreFolder(context, root, core, create = true)
            ?: error("Cannot access RetroArch core folder")
        val dirId = DocumentsContract.getDocumentId(folder)
        val existing = children(context, root, dirId).firstOrNull { it.first == name }?.second
        if (existing != null) {
            val old = read(context, existing, if (isState) MAX_STATE else MAX_SAVE)
            if (old.contentEquals(bytes)) return
            if (isState) {
                // Never overwrite unsupported or damaged RetroArch states,
                // including zstd v2 and legacy raw files we cannot validate.
                RetroArchCompression.requireSupportedContainer(old)
            }
            // Never destroy the pre-Slot save on first sync. A backup is
            // retained in a dedicated subdirectory of the same core folder.
            val backupName = name + ".before-slot"
            val backups = children(context, root, dirId)
            if (backups.none { it.first == backupName }) {
                val backup = DocumentsContract.createDocument(resolver, folder,
                    "application/octet-stream", backupName)
                    ?: error("Cannot back up existing RetroArch file")
                resolver.openOutputStream(backup, "wt")?.use { it.write(old) }
                    ?: error("Cannot write RetroArch backup")
                check(read(context, backup, if (isState) MAX_STATE else MAX_SAVE)
                    .contentEquals(old)) {
                    "RetroArch backup verification failed; external state preserved"
                }
            }
            // Best-effort conflict guard: abort if a sync service or RetroArch
            // modified the source while we were preparing the replacement.
            check(read(context, existing, if (isState) MAX_STATE else MAX_SAVE)
                .contentEquals(old)) {
                "RetroArch file changed during sync; refusing to overwrite"
            }
        }
        val target = existing ?: DocumentsContract.createDocument(resolver, folder,
            "application/octet-stream", name)
            ?: error("Cannot create RetroArch save")
        // Android SAF generally has no cross-provider atomic rename guarantee.
        // Write with truncate, and verify contents. The untouched .before-slot
        // copy remains recoverable if a provider fails mid-write.
        resolver.openOutputStream(target, "wt")?.use { it.write(bytes); it.flush() }
            ?: error("Cannot write RetroArch save")
        check(read(context, target, if (isState) MAX_STATE else MAX_SAVE).contentEquals(bytes)) {
            "RetroArch sync verification failed; backup is preserved"
        }
    }
}
