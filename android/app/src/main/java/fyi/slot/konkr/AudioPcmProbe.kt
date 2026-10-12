package fyi.slot.konkr

import android.app.Activity
import android.content.pm.ApplicationInfo
import android.os.SystemClock
import android.util.Log
import java.io.File
import java.io.RandomAccessFile
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Opt-in, debug-build-only PCM probe. Captures the *real* stereo 16-bit PCM
 * successfully submitted to AudioTrack, NEVER speculative/rolled-back audio.
 *
 * ADB protocol (via run-as fyi.slot.konkr, no exported receiver or permission):
 *   touch files/pcm-request-OFF
 *   touch files/pcm-request-RA1
 * Output: files/AudioPCMDiagnostic/{OFF,RA1}.wav and {OFF,RA1}.txt
 *
 * No I/O, blocking offer, resampling or AudioTrack modification on the
 * emulator/render thread. The audio worker only copies into a bounded queue.
 */
internal class AudioPcmProbe(private val activity: Activity) {
    private companion object {
        const val TAG = "SlotAudioPCM"
        const val SECONDS = 12
        const val QUEUE_CAPACITY = 128
    }
    private val enabled =
        activity.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0
    private val directory = File(activity.filesDir, "AudioPCMDiagnostic")
    private var lastPollMs = 0L
    private var current: Capture? = null

    private class Capture(val label: String, val rate: Int) {
        val queue = ArrayBlockingQueue<ShortArray>(QUEUE_CAPACITY)
        val recording = AtomicBoolean(true)
        var samples = 0 // audio thread exclusively; number of 16-bit samples
        var dropped = 0 // audio thread exclusively
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
            val capture = Capture(label, rate)
            current = capture
            Thread({ writeCapture(capture) }, "slot-pcm-$label").apply {
                priority = Thread.MIN_PRIORITY
                start()
            }
            Log.i(TAG, "CAPTURE START label=$label rate=$rate Hz seconds=$SECONDS")
            return
        }
    }

    /** Called AFTER AudioTrack.write; write count is shorts, not stereo frames. */
    fun capture(pcm: ShortArray, written: Int, rate: Int) {
        val c = current ?: return
        if (rate != c.rate) {
            Log.w(TAG, "CAPTURE RATE CHANGE label=${c.label} $rate != ${c.rate}")
            stop()
            return
        }
        val remaining = c.rate * SECONDS * 2 - c.samples
        val length = written.coerceAtLeast(0).coerceAtMost(pcm.size).coerceAtMost(remaining)
        // Never block the audio worker waiting for disk or a saturated queue.
        if (length > 0 && !c.queue.offer(pcm.copyOfRange(0, length))) {
            c.dropped += length
        }
        c.samples += length
        if (c.samples >= c.rate * SECONDS * 2) stop()
    }

    fun stop() {
        val c = current ?: return
        current = null
        c.recording.set(false)
        Log.i(TAG, "CAPTURE STOP label=${c.label} offered=${c.samples / 2} frames lost=${c.dropped / 2}")
    }

    private fun writeCapture(c: Capture) {
        val partial = File(directory, "${c.label}.wav.partial")
        val result = File(directory, "${c.label}.wav")
        val report = File(directory, "${c.label}.txt")
        var bytes = 0L
        var error = ""
        try {
            RandomAccessFile(partial, "rw").use { out ->
                out.setLength(0)
                wavHeader(out, c.rate, 0)
                while (c.recording.get() || c.queue.isNotEmpty()) {
                    val samples = c.queue.poll(300, TimeUnit.MILLISECONDS) ?: continue
                    val buffer = ByteArray(samples.size * 2)
                    for (i in samples.indices) {
                        val sample = samples[i].toInt()
                        buffer[i * 2] = sample.toByte()
                        buffer[i * 2 + 1] = (sample ushr 8).toByte()
                    }
                    out.write(buffer)
                    bytes += buffer.size
                }
                // Replace the placeholder header after writing actual PCM bytes.
                out.seek(0)
                wavHeader(out, c.rate, bytes.toInt())
                out.fd.sync()
            }
            if (!partial.renameTo(result)) throw IllegalStateException("Cannot publish WAV")
            Log.i(TAG, "CAPTURE COMPLETE label=${c.label} pcmBytes=$bytes dropShorts=${c.dropped}")
        } catch (e: Exception) {
            error = e.toString()
            Log.e(TAG, "CAPTURE FAILED label=${c.label}", e)
        } finally {
            try {
                report.writeText(
                    "label=${c.label}\nrate=${c.rate}\nchannels=2\nformat=pcm_s16le\n" +
                    "duration_requested_seconds=$SECONDS\nframes_written=${bytes / 4}\n" +
                    "lost_stereo_frames=${c.dropped / 2}\nerror=$error\n"
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
