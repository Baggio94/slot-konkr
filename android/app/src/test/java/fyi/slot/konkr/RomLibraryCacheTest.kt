package fyi.slot.konkr

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RomLibraryCacheTest {
    @Test fun unchangedFileReusesHeaderSafely() {
        assertTrue(RomLibrary.canReuseHeader("Advance Wars", 8_388_608, 1_700_000_000_000,
            "Advance Wars", 8_388_608, 1_700_000_000_000))
    }

    @Test fun changedOrUnknownMetadataAlwaysReloadsHeader() {
        assertFalse(RomLibrary.canReuseHeader("Game", 1024, 1234, "Game", 1025, 1234))
        assertFalse(RomLibrary.canReuseHeader("Game", 1024, 1234, "Game", 1024, 1235))
        assertFalse(RomLibrary.canReuseHeader("Game", 1024, 1234, "Renamed", 1024, 1234))
        assertFalse(RomLibrary.canReuseHeader("Game", 1024, 1234, "Game", 1024, 0))
        assertFalse(RomLibrary.canReuseHeader("Game", 1024, 1234, "Game", -1, 1234))
        assertFalse(RomLibrary.canReuseHeader(null, 1024, 1234, "Game", 1024, 1234))
    }
}
