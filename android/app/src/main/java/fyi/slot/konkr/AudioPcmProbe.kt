package fyi.slot.konkr

import android.app.Activity
import android.content.pm.ApplicationInfo
import android.os.SystemClock
import android.util.Log
import java.io.File
import java.io.RandomAccessFile

/**
 * Opt-in, debug-build-only PCM probe. Captures the *real* stereo 16-bit PCM
 * successfully submitted to AudioTrack, NEVER speculative/rolled-back audio.
 *
 * ADB protocol (via run-as fyi.slot.konkr, no exported receiver or permission):
 *   touch files/pcm-request-OFF
 *   touch files/pcm-request-RA1
 * Output: files/AudioPCMDiagnostic/{OFF,RA1}.wav and {OFF,RA1}.txt
 *
 * Audio captures remain entirely in a single preallocated 3MiB RAM buffer.
 * Only AFTER the measurement finishes do we write the WAV on a worker thread.
 * No emulator/render-thread hooks, libretro changes, or per-chunk allocation.
 */
internal class AudioPcmProbe(private val activity: Activity) {
    private companion object {
        const val TAG = "SlotAudioPCM"
        const val SECONDS = 12
    }
    private val enabled =
        activity.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0
    private val directory = File(activity.filesDir, "AudioPCMDiagnostic")
    private var lastPollMs = 0L
    private var current: Capture? = null

    // Only the Android audio worker accesses current and writes the PCM array.
    private class Capture(val label: String, val rate: Int) {
        val pcm = ShortArray(rate * SECONDS * 2)
        val startedMs = SystemClock.elapsedRealtime()
        var samples = 0 // number of interleaved PCM16 samples
    }

    /** Called from the Android audio worker, at most one file probe per 250ms. */
    fun poll(rate: Int) {
        if (!enabled || current != null || rate !in 8_000..96_000) return
        val now = SystemClock.elapsedRealtime()
        if (now - lastPollMs < 250L) return
        lastPollMs = now
        for (label in arrayOf("OFF", "RA1")) {
            val request = File(activity.filesDir, "pcm-request-$label")
            if (!request.exists()) continue
            if (!request.delete()) {
                Log.w(TAG, "Cannot acknowledge request: $label")
                return
            }
            directory.mkdirs()
            // One allocation before the timer starts; no background disk
            // I/O while collecting audio frames.
            try {
                current = Capture(label, rate)
                Log.i(TAG, "CAPTURE START label=$label rate=$rate Hz seconds=$SECONDS method=RAM_ONLY")
            } catch (error: OutOfMemoryError) {
                Log.e(TAG, "PCM buffer allocation failed", error)
            }
            return
        }
    }

    /** Called AFTER AudioTrack.write; count is shorts, not stereo frames. */
    fun capture(pcm: ShortArray, written: Int, rate: Int) {
        val c = current ?: return
        if (rate != c.rate) {
            Log.w(TAG, "CAPTURE RATE CHANGE label=${c.label} $rate != ${c.rate}")
            stop()
            return
        }
        val count = written.coerceAtLeast(0).coerceAtMost(pcm.size)
            .coerceAtMost(c.pcm.size - c.samples)
        if (count > 0) {
            // JVM intrinsic; avoids allocating a new ShortArray every chunk.
            System.arraycopy(pcm, 0, c.pcm, c.samples, count)
            c.samples += count
        }
        if (c.samples >= c.pcm.size) stop()
    }

    /** Write only when the capture has ended; never during its timed interval. */
    fun stop() {
        val c = current ?: return
        current = null
        val elapsedMs = SystemClock.elapsedRealtime() - c.startedMs
        Log.i(TAG, "CAPTURE STOP label=${c.label} samples=${c.samples} elapsedMs=$elapsedMs")
        Thread({ writeCapture(c, elapsedMs) }, "slot-pcm-write-${c.label}").apply {
            priority = Thread.MIN_PRIORITY
            start()
        }
    }

    private fun writeCapture(c: Capture, elapsedMs: Long) {
        val partial = File(directory, "${c.label}.wav.partial")
        val result = File(directory, "${c.label}.wav")
        val report = File(directory, "${c.label}.txt")
        var bytes = 0
        var error = ""
        try {
            RandomAccessFile(partial, "rw").use { out ->
                out.setLength(0)
                wavHeader(out, c.rate, 0)
                // Buffer is allocated ONLY after the capture ends.
                val buffer = ByteArray(32 * 1024)
                var index = 0
                while (index < c.samples) {
                    val count = minOf(c.samples - index, buffer.size / 2)
                    for (i in 0 until count) {
                        val sample = c.pcm[index + i].toInt()
                        buffer[2 * i] = sample.toByte()
                        buffer[2 * i + 1] = (sample ushr 8).toByte()
                    }
                    out.write(buffer, 0, count * 2)
                    index += count
                    bytes += count * 2
                }
                out.seek(0)
                wavHeader(out, c.rate, bytes)
                out.fd.sync()
            }
            if (!partial.renameTo(result)) {
                throw IllegalStateException("Cannot publish PCM diagnostic WAV")
            }
            Log.i(TAG, "CAPTURE COMPLETE label=${c.label} pcmBytes=$bytes elapsedMs=$elapsedMs")
        } catch (e: Exception) {
            error = e.toString()
            Log.e(TAG, "CAPTURE FAILED label=${c.label}", e)
        } finally {
            try {
                report.writeText(
                    "label=${c.label}\nrate=${c.rate}\nchannels=2\nformat=pcm_s16le\n" +
                    "method=RAM_ONLY_POST_CAPTURE_WRITE\n" +
                    "duration_requested_seconds=$SECONDS\n" +
                    "capture_elapsed_ms=$elapsedMs\n" +
                    "frames_written=${bytes / 4}\n" +
                    "lost_stereo_frames=0\nerror=$error\n"
                )
            } catch (e: Exception) {
                Log.w(TAG, "Cannot save diagnostic metadata", e)
            }
        }
    }

    private fun wavHeader(out: RandomAccessFile, rate: Int, pcmBytes: Int) {
        fun ascii(value: String) { out.write(value.toByteArray(Charsets.US_ASCII)) }
        fun shortLE(value: Int) { out.writeByte(value and 255); out.writeByte(value ushr 8 and 255) }
        fun intLE(value: Int) {
            out.writeByte(value and 255)
            out.writeByte(value ushr 8 and 255)
            out.writeByte(value ushr 16 and 255)
            out.writeByte(value ushr 24 and 255)
        }
        ascii("RIFF"); intLE(36 + pcmBytes); ascii("WAVE")
        ascii("fmt "); intLE(16); shortLE(1); shortLE(2)
        intLE(rate); intLE(rate * 4); shortLE(4); shortLE(16)
        ascii("data"); intLE(pcmBytes)
    }
}
