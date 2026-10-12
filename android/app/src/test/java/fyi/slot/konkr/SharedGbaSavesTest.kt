package fyi.slot.konkr

import org.junit.Assert.assertEquals
import org.junit.Test

/** No ROM content or Android storage provider required: migration must be
 * deterministic and preserve two disagreeing saves on first introduction.
 */
class SharedGbaSavesTest {
    private val a = ByteArray(32768) { 0x11 }
    private val b = ByteArray(32768) { 0x22 }

    @Test fun firstMigrationNeverGuessesBetweenDifferentProgress() {
        assertEquals(RetroArchStorage.SharedGbaSource.CONFLICT,
            RetroArchStorage.chooseGbaSource(a, b, null))
    }

    @Test fun equalSramCanBeMirroredAcrossBothEmulators() {
        assertEquals(RetroArchStorage.SharedGbaSource.OWN,
            RetroArchStorage.chooseGbaSource(a, a.copyOf(), null))
    }

    @Test fun oneExistingSaveCanSeedOtherCore() {
        assertEquals(RetroArchStorage.SharedGbaSource.OTHER,
            RetroArchStorage.chooseGbaSource(null, b, null))
        assertEquals(RetroArchStorage.SharedGbaSource.OWN,
            RetroArchStorage.chooseGbaSource(a, null, null))
    }

    @Test fun previouslyPublishedSramIdentifiesWhichCoreChanged() {
        assertEquals(RetroArchStorage.SharedGbaSource.OTHER,
            RetroArchStorage.chooseGbaSource(a, b, a))
        assertEquals(RetroArchStorage.SharedGbaSource.OWN,
            RetroArchStorage.chooseGbaSource(b, a, a))
    }

    @Test fun concurrentChangesConflictRatherThanDiscardProgress() {
        val c = ByteArray(32768) { 0x33 }
        assertEquals(RetroArchStorage.SharedGbaSource.CONFLICT,
            RetroArchStorage.chooseGbaSource(b, c, a))
    }

    @Test fun noPreviousSaveIsSafe() {
        assertEquals(RetroArchStorage.SharedGbaSource.NONE,
            RetroArchStorage.chooseGbaSource(null, null, null))
    }
}
