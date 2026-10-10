package fyi.slot.konkr

import android.content.Context
import android.net.Uri
import android.os.SystemClock
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import android.util.Log
import java.io.File
import java.io.FileNotFoundException
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
        val manualExportPrefix: File,
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
            File(File(prefix, "ManualExports").apply { mkdirs() }, "$digest.$coreLower"),
            File(prefix, "$digest.$coreLower.retroarch-import.state.auto"),
            File(prefix, "$digest.$coreLower.retroarch-export.state.auto"),
            savesTree, statesTree)
    }

    fun importBeforeLaunch(context: Context, target: Target) {
        val started = SystemClock.elapsedRealtime()
        // The original implementation enumerated the entire save/state
        // directory SIX times per game launch. Large ROM collections with
        // Syncthing conflict files make SAF queries very expensive.
        // For Android's own ExternalStorageProvider, document IDs are
        // hierarchical. Use the user-granted tree to open the exact document
        // directly, without listing its siblings. Other providers retain the
        // existing safe directory-enumeration fallback.
        target.savesTree?.let { root ->
            readNamed(context, root, target.core, target.stem + ".srm", MAX_SAVE)
                ?.let { stage(target.saveLocal, it) }
            readNamed(context, root, target.core, target.stem + ".rtc", 4096)
                ?.let { bytes ->
                    require(bytes.isNotEmpty()) { "Empty RetroArch RTC" }
                    stage(target.rtcLocal, bytes)
                }
        }
        val savesAt = SystemClock.elapsedRealtime()
        val incoming = if (target.statesTree != null) {
            readNamed(context, target.statesTree, target.core,
                target.stem + ".state.auto", MAX_STATE)
        } else if (target.stateDefaultLocal.isFile) {
            target.stateDefaultLocal.readBytes()
        } else null
        val readAt = SystemClock.elapsedRealtime()
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
        val doneAt = SystemClock.elapsedRealtime()
        Log.i(TAG, "RetroArch SAF import breakdown: saves=${savesAt - started}ms, " +
            "state read=${readAt - savesAt}ms, state decode/stage=${doneAt - readAt}ms, " +
            "total=${doneAt - started}ms")
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
                Log.e(TAG, "RetroArch SRAM export failed for ${target.core}/${target.stem}", e)
                failures.add("Save sync failed (${target.core}): " + (e.message ?: "access denied"))
            }
        }
        if (target.savesTree != null && target.rtcLocal.isFile) {
            try {
                val data = target.rtcLocal.readBytes()
                require(data.size in 1..4096) { "Invalid RTC size" }
                writeSaf(context, target.savesTree, target.core,
                    target.stem + ".rtc", data, isState = false)
            } catch (e: Exception) {
                Log.e(TAG, "RetroArch RTC export failed for ${target.core}/${target.stem}", e)
                failures.add("RTC sync failed (${target.core}): " + (e.message ?: "access denied"))
            }
        }
        if (target.stateExportLocal.isFile) {
            try {
                val bytes = target.stateExportLocal.readBytes()
                require(bytes.size in 16..MAX_STATE && bytes.startsWithRASTATE()) {
                    "Invalid uncompressed RASTATE export"
                }
                // Keep the exact #RZIPv1# format used by this KONKR RetroArch.
                val compressed = RetroArchCompression.encode(bytes)
                if (target.statesTree != null) {
                    writeSaf(context, target.statesTree, target.core,
                        target.stem + ".state.auto", compressed, isState = true)
                } else {
                    // Private defaults use the same per-core RZIP layout as RetroArch.
                    stage(target.stateDefaultLocal, compressed)
                    check(target.stateDefaultLocal.readBytes().contentEquals(compressed)) {
                        "Default state verification failed"
                    }
                }
            } catch (e: Exception) {
                Log.e(TAG, "RetroArch state export failed for ${target.core}/${target.stem}", e)
                failures.add("State sync skipped (${target.core}): " + (e.message ?: "unsupported format"))
            }
        }
        return failures
    }

    /** Export each new Slot Polaroid to the first EMPTY RetroArch manual slot.
     * Never replace the user's existing .state/.state1..state9 files.
     * The private 10-entry Polaroid ring remains independent.
     */
    fun exportManualState(context: Context, target: Target, stamp: String): List<String> {
        val failures = mutableListOf<String>()
        try {
            require(stamp.matches(Regex("[0-9]{4}-[0-9]{2}-[0-9]{2}_[0-9]{2}-[0-9]{2}-[0-9]{2}"))) {
                "Invalid Slot state timestamp"
            }
            val staged = File(target.manualExportPrefix.absolutePath + "." + stamp + ".rastate")
            val bytes = staged.readBytes()
            RetroArchCompression.requireSupportedContainer(bytes)
            val compressed = RetroArchCompression.encode(bytes)
            for (slot in 0..9) {
                val name = target.stem + if (slot == 0) ".state" else ".state$slot"
                if (target.statesTree != null) {
                    if (find(context, target.statesTree, target.core, name) != null) continue
                    writeSaf(context, target.statesTree, target.core,
                        name, compressed, isState = true, createOnly = true)
                } else {
                    val local = File(target.stateDefaultLocal.parentFile, name)
                    if (local.exists()) continue
                    stage(local, compressed)
                    check(local.readBytes().contentEquals(compressed)) {
                        "Private numbered state verification failed"
                    }
                }
                if (!staged.delete()) Log.w(TAG, "Manual staging cleanup deferred: $staged")
                return failures
            }
            error("Manual slots 0-9 are occupied; Polaroid remains saved inside Slot")
        } catch (e: Exception) {
            Log.e(TAG, "RetroArch manual state export skipped", e)
            failures.add("Manual state sync skipped: " + (e.message ?: "unsupported format"))
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
        // ACTION_OPEN_DOCUMENT_TREE returns a tree URI without a /document/
        // segment. createDocument() requires a document URI, even when that
        // document is the selected tree root. Supplying `root` directly causes
        // ExternalStorageProvider to reject creation with "Invalid URI".
        val rootDocument = DocumentsContract.buildDocumentUriUsingTree(root, rootId)
        return DocumentsContract.createDocument(context.contentResolver, rootDocument,
            DocumentsContract.Document.MIME_TYPE_DIR, core)
            ?: error("Cannot create RetroArch core folder: $core")
    }

    /**
     * The AOSP external-storage document provider uses stable hierarchical IDs
     * such as "primary:RetroArch/states/mGBA/Game.state.auto". A tree grant
     * covers its descendants; we do not infer a file-system path or bypass SAF.
     * Never use this optimisation for downloads/cloud/third-party providers,
     * whose document identifiers need not be hierarchical.
     */
    internal fun directDocumentId(treeId: String, core: String, name: String): String? {
        if (core != "mGBA" && core != "gpSP") return null
        if (name.isEmpty() || name == "." || name == ".." ||
            name.any { it == '/' || it == '\\' || it.code < 32 }) return null
        val volume = treeId.substringBefore(':', "")
        if (volume != "primary" && !volume.matches(Regex("[0-9a-fA-F]{4}-[0-9a-fA-F]{4}"))) {
            return null
        }
        if (treeId.any { it == '\\' || it.code < 32 } ||
            treeId.split('/').any { it == "." || it == ".." }) return null
        return "$treeId/$core/$name"
    }

    private fun directDocument(root: Uri, core: String, name: String): Uri? {
        if (root.authority != "com.android.externalstorage.documents") return null
        val treeId = try {
            DocumentsContract.getTreeDocumentId(root)
        } catch (_: IllegalArgumentException) {
            return null
        }
        val childId = directDocumentId(treeId, core, name) ?: return null
        // Only the selected SAF tree, not a raw filesystem path, is accessed.
        return DocumentsContract.buildDocumentUriUsingTree(root, childId)
    }

    private fun readNamed(context: Context, root: Uri, core: String,
                          name: String, limit: Int): ByteArray? {
        val direct = directDocument(root, core, name)
        if (direct != null) {
            return try {
                read(context, direct, limit)
            } catch (_: FileNotFoundException) {
                // A missing RTC/auto-state is normal. AOSP document IDs map
                // names to paths; do not enumerate thousands of siblings.
                null
            }
        }
        return find(context, root, core, name)?.let { read(context, it, limit) }
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
        // Opening the same game must not rewrite and fsync an unchanged SRAM
        // or automatic state every time. Read+compare is cheap for these
        // bounded files and preserves the exact externally supplied bytes.
        if (local.isFile && local.length() == bytes.size.toLong()) {
            try {
                if (local.readBytes().contentEquals(bytes)) return
            } catch (_: java.io.IOException) {
                // Existing file cannot be read; replace it atomically below.
            }
        }
        val pending = File(local.parentFile, local.name + ".partial")
        try {
            pending.outputStream().use { it.write(bytes); it.fd.sync() }
            check(pending.renameTo(local)) { "Cannot import RetroArch file" }
        } finally {
            pending.delete()
        }
    }

    private fun writeSaf(context: Context, root: Uri, core: String,
                         name: String, bytes: ByteArray, isState: Boolean,
                         createOnly: Boolean = false) {
        val resolver = context.contentResolver
        val folder = coreFolder(context, root, core, create = true)
            ?: error("Cannot access RetroArch core folder")
        val dirId = DocumentsContract.getDocumentId(folder)
        val existing = children(context, root, dirId).firstOrNull { it.first == name }?.second
        require(!createOnly || existing == null) {
            "Manual RetroArch slot was occupied during sync; preserved"
        }
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
        if (createOnly) {
            // Some SAF providers silently rename a duplicate filename. Never
            // misreport that as an available RetroArch slot.
            val actual = resolver.query(target, arrayOf(OpenableColumns.DISPLAY_NAME),
                null, null, null)?.use { cursor ->
                if (cursor.moveToFirst()) cursor.getString(0) else null
            }
            if (actual != name) {
                DocumentsContract.deleteDocument(resolver, target)
                error("Manual state slot name collision; external file preserved")
            }
        }
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
