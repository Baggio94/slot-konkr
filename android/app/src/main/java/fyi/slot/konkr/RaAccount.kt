package fyi.slot.konkr

import android.system.Os
import android.util.AtomicFile
import java.io.File
import java.io.IOException
import java.io.ByteArrayOutputStream
import java.net.URLEncoder
import java.net.URL
import javax.net.ssl.HttpsURLConnection
import org.json.JSONObject

/**
 * One-time RetroAchievements login2 exchange on a background thread.
 * Never persist the password. The resulting session token is written ONLY
 * to Android's non-backed-up app-private filesDir, with owner-only permissions.
 *
 * This is NOT an offline login and does not prove any game/award is ready.
 */
internal object RaAccount {
    private const val ENDPOINT = "https://retroachievements.org/dorequest.php"
    private const val LIMIT = 64 * 1024

    sealed interface Result {
        data class Connected(val username: String) : Result
        data class Failed(val message: String) : Result
    }

    private fun configFile(root: File) = File(root, "Config/retroachievements.toml")

    fun configuredUser(root: File): String? {
        val config = configFile(root)
        if (!config.isFile || config.length() > 8192) return null
        return try {
            RaConfigCodec.displayedUsername(config.readText(Charsets.UTF_8))
        } catch (_: Exception) { null }
    }

    private fun writePrivateConfig(root: File, username: String, token: String) {
        val dir = File(root, "Config")
        if (!dir.isDirectory && !dir.mkdirs()) throw IOException("Cannot create private config")
        Os.chmod(dir.absolutePath, 0x1C0) // 0700
        val target = configFile(root)
        val bytes = RaConfigCodec.configuredAccount(username, token).toByteArray(Charsets.UTF_8)
        val atomic = AtomicFile(target)
        val output = atomic.startWrite()
        try {
            output.write(bytes)
            atomic.finishWrite(output)
        } catch (error: Exception) {
            atomic.failWrite(output)
            throw error
        }
        Os.chmod(target.absolutePath, 0x180) // 0600
    }

    fun authenticateAndSave(root: File, username: String, password: String): Result {
        val requested = username.trim()
        if (requested.isEmpty() || requested.length > 100 ||
            requested.any { it.isISOControl() } || password.isEmpty() || password.length > 1024
        ) return Result.Failed("Enter a valid RetroAchievements username and password.")
        // Phase 2 only supports initial onboarding. Changing accounts must safely
        // stop the existing native worker before replacing its configuration.
        if (configFile(root).exists()) {
            return Result.Failed("An account is already configured. Account switching is coming later.")
        }

        var connection: HttpsURLConnection? = null
        return try {
            connection = URL(ENDPOINT).openConnection() as HttpsURLConnection
            connection.connectTimeout = 8000
            connection.readTimeout = 8000
            connection.instanceFollowRedirects = false
            connection.requestMethod = "POST"
            connection.doOutput = true
            connection.useCaches = false
            connection.setRequestProperty("Content-Type", "application/x-www-form-urlencoded; charset=UTF-8")
            connection.setRequestProperty("Accept", "application/json")
            // Do NOT write this body, URL, password or response token to Logcat.
            val form = listOf("r" to "login2", "u" to requested, "p" to password)
                .joinToString("&") { (key, value) ->
                    key + "=" + URLEncoder.encode(value, "UTF-8")
                }
            connection.outputStream.use { it.write(form.toByteArray(Charsets.UTF_8)) }
            if (connection.responseCode != 200) {
                return Result.Failed("RetroAchievements is unavailable. Try again online.")
            }
            // API 31 does not guarantee Java 9 InputStream.readNBytes().
            // Read with a hard cap without depending on newer Android APIs.
            val payload = connection.inputStream.use { stream ->
                val bytes = ByteArrayOutputStream()
                val chunk = ByteArray(4096)
                while (true) {
                    val count = stream.read(chunk)
                    if (count < 0) break
                    if (bytes.size() + count > LIMIT) {
                        return Result.Failed("Unexpected RetroAchievements response.")
                    }
                    bytes.write(chunk, 0, count)
                }
                bytes.toByteArray()
            }
            if (payload.isEmpty()) {
                return Result.Failed("Unexpected RetroAchievements response.")
            }
            val reply = JSONObject(String(payload, Charsets.UTF_8))
            if (!reply.optBoolean("Success", false)) {
                return Result.Failed("RetroAchievements rejected the login. Check your credentials.")
            }
            val user = reply.optString("User", "")
            val token = reply.optString("Token", "")
            if (user.isBlank() || token.isBlank()) {
                return Result.Failed("RetroAchievements did not return a session token.")
            }
            writePrivateConfig(root, user, token)
            Result.Connected(user)
        } catch (_: Exception) {
            // Errors may contain URLs or server response details: keep them out
            // of user-visible toasts and logs. No credentials are printed.
            Result.Failed("Couldn't connect or save the account. Check internet and try again.")
        } finally {
            connection?.disconnect()
        }
    }
}
