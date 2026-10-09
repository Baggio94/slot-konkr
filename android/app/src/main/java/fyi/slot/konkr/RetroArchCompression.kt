package fyi.slot.konkr

import java.io.ByteArrayOutputStream
import java.util.zip.Inflater

/** RetroArch libretro-common rzip_stream.c chunk format, strict bounded v1
 * (zlib) decoding. RZIP v2 is zstd, not Android's built-in zlib; preserve it
 * untouched until a tested zstd decoder is available.
 */
internal object RetroArchCompression {
    private val marker = byteArrayOf(35,82,90,73,80,118) // #RZIPv
    private const val LIMIT = 65 * 1024 * 1024

    fun isRzip(data: ByteArray): Boolean =
        data.size >= 20 && marker.indices.all { data[it] == marker[it] } &&
        data[7] == 35.toByte()

    fun decode(data: ByteArray): ByteArray {
        if (!isRzip(data)) return data
        val version=data[6].toInt() and 255
        require(version==1) {
            if (version==2) "RetroArch RZIP zstd states are not supported yet"
            else "Unknown RetroArch RZIP version"
        }
        fun readLE32(offset:Int):Long =
            (0..3).fold(0L){total,k->total or ((data[offset+k].toLong() and 255L) shl (8*k))}
        val block=readLE32(8)
        val total=readLE32(12) or (readLE32(16) shl 32)
        require(block in 1..(64L*1024*1024)) {"Invalid RZIP block size"}
        require(total in 1..LIMIT.toLong()) {"Invalid RZIP uncompressed size"}
        val output=ByteArrayOutputStream(total.toInt())
        var offset=20
        while(output.size()<total) {
            require(offset+4<=data.size) {"Truncated RetroArch RZIP chunk"}
            val compressed=readLE32(offset)
            offset+=4
            require(compressed in 1..(block*2) && offset+compressed<=data.size) {
                "Invalid RetroArch RZIP compressed chunk size"
            }
            val inflater=Inflater(false)  // independent zlib stream per chunk
            try {
                inflater.setInput(data,offset,compressed.toInt())
                val expected=minOf(block.toInt(),total.toInt()-output.size())
                val buffer=ByteArray(expected)
                var written=0
                while(written<expected && !inflater.finished()) {
                    val n=inflater.inflate(buffer,written,expected-written)
                    if(n==0) {
                        require(!inflater.needsDictionary() && !inflater.needsInput()) {
                            "Malformed RZIP zlib chunk"
                        }
                        error("RetroArch inflater made no progress")
                    }
                    written+=n
                }
                require(written==expected && inflater.finished() && inflater.remaining==0) {
                    "Incomplete or oversized RetroArch RZIP chunk"
                }
                output.write(buffer)
            } finally {
                inflater.end()
            }
            offset+=compressed.toInt()
        }
        require(offset==data.size) {"Unexpected trailing RZIP data"}
        return output.toByteArray()
    }
}
