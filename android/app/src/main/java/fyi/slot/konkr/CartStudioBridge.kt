package fyi.slot.konkr

import android.content.Context
import android.graphics.BitmapFactory
import android.util.AtomicFile
import android.util.Base64
import android.webkit.JavascriptInterface
import org.json.JSONObject
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Only the one ROM selected in Slot is exposed to the trusted, packaged Studio page.
 * No JavaScript method accepts arbitrary Android paths, document URIs or ROM writes.
 */
internal class CartStudioBridge(
    private val context: Context,
    private val rom: CartStudioCatalog.Rom,
    private val identity: CartStudioCatalog.Fingerprint,
    private val dirty: AtomicBoolean,
) {
    companion object {
        fun key(uri: String): String {
            var hash = -3750763034362895579L
            for (byte in uri.toByteArray(Charsets.UTF_8)) {
                hash = (hash xor (byte.toInt() and 0xff).toLong()) * 0x100000001b3L
            }
            return java.lang.Long.toUnsignedString(hash, 16).padStart(16, '0')
        }

        fun label(context: Context, uri: String): File =
            File(File(context.filesDir, "Labels"), key(uri) + ".png")

        private fun profile(context: Context, uri: String): File =
            File(File(context.filesDir, "Studio"), key(uri) + ".shell")

        fun savedShell(context: Context, uri: String): String? {
            val file = profile(context, uri)
            if (!file.isFile || file.length() !in 1..100) return null
            val raw = file.readText(Charsets.UTF_8).trim()
            return raw.takeIf(::validShell)
        }

        private fun validShell(value: String): Boolean =
            Regex("(auto|notched|rounded) [0-9a-fA-F]{6} (solid|clear|glitter)")
                .matches(value)

        private fun atomicWrite(file: File, bytes: ByteArray) {
            file.parentFile?.mkdirs()
            val atomic = AtomicFile(file)
            val output = atomic.startWrite()
            try {
                output.write(bytes)
                atomic.finishWrite(output)
            } catch (error: Exception) {
                atomic.failWrite(output)
                throw error
            }
        }
    }

    @JavascriptInterface
    fun session(): String {
        val existing = label(context, rom.uri)
        val png = if (existing.isFile && existing.length() in 1..(2L * 1024 * 1024))
            Base64.encodeToString(existing.readBytes(), Base64.NO_WRAP) else ""
        val shell = savedShell(context, rom.uri)
        return JSONObject()
            .put("platform", rom.platform)
            .put("stem", rom.title)
            .put("crc", identity.crc)
            .put("head", Base64.encodeToString(identity.head, Base64.NO_WRAP))
            .put("label", png)
            .put("shellText", if (shell != null) rom.title + " = " + shell + "\n" else "")
            .toString()
    }

    @Synchronized
    @JavascriptInterface
    fun saveLabel(platform: String, stem: String, encoded: String, replace: Boolean): String {
        require(platform == rom.platform && stem == rom.title) { "Unknown cartridge" }
        require(encoded.length <= 3_000_000) { "Label too large" }
        val target = label(context, rom.uri)
        if (!replace && target.isFile) return "skipped"
        val bytes = Base64.decode(encoded, Base64.DEFAULT)
        require(bytes.size in 24..(2 * 1024 * 1024)) { "Invalid label size" }
        val signature = byteArrayOf(-119, 80, 78, 71, 13, 10, 26, 10)
        require(bytes.copyOfRange(0, 8).contentEquals(signature)) { "Expected PNG" }
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        require(bounds.outWidth in 1..2048 && bounds.outHeight in 1..2048) {
            "Invalid label dimensions"
        }
        atomicWrite(target, bytes)
        dirty.set(true)
        return "written"
    }

    @Synchronized
    @JavascriptInterface
    fun saveShells(text: String): Boolean {
        require(text.length < 1024) { "Shell data too large" }
        val lines = text.lineSequence().map(String::trim).filter(String::isNotEmpty)
            .filterNot { it.startsWith("#") || it.startsWith(";") }.toList()
        require(lines.size <= 1) { "One selected cartridge only" }
        val value = if (lines.isEmpty()) null else {
            val pair = lines.single().split('=', limit = 2)
            require(pair.size == 2 && pair[0].trim() == rom.title) { "Wrong cartridge key" }
            pair[1].trim().takeIf { it != "auto" }?.also {
                require(validShell(it)) { "Invalid shell selection" }
            }
        }
        val target = profile(context, rom.uri)
        if (value == null) {
            if (target.exists() && !target.delete()) error("Cannot reset shell")
        } else {
            atomicWrite(target, value.toByteArray(Charsets.UTF_8))
        }
        dirty.set(true)
        return true
    }
}
