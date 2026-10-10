package fyi.slot.konkr

import org.junit.Assert.*
import org.junit.Test

/** Validates the direct SAF child IDs before they reach Android's
 * ExternalStorageProvider; no filesystem access or tree escape is allowed.
 */
class RetroArchStorageDirectUriTest {
    @Test fun primaryStorageCanResolveExactRetroArchSavesAndStates() {
        assertEquals(
            "primary:RetroArch/saves/mGBA/Pokemon - Version Rubis (France) (Rev 1).srm",
            RetroArchStorage.directDocumentId("primary:RetroArch/saves",
                "mGBA", "Pokemon - Version Rubis (France) (Rev 1).srm"))
        assertEquals(
            "primary:RetroArch/states/mGBA/Advance Wars 2 - Black Hole Rising (USA).state.auto",
            RetroArchStorage.directDocumentId("primary:RetroArch/states",
                "mGBA", "Advance Wars 2 - Black Hole Rising (USA).state.auto"))
    }

    @Test fun removableStorageAndAlternativeCoreWork() {
        assertEquals("ABCD-1234/".replace("/", ":") + "My Saves/gpSP/Game.rtc",
            RetroArchStorage.directDocumentId("ABCD-1234:My Saves", "gpSP", "Game.rtc"))
    }

    @Test fun invalidVolumesCoreAndTraversalsMustFallBack() {
        assertNull(RetroArchStorage.directDocumentId("cloud:roms", "mGBA", "Game.srm"))
        assertNull(RetroArchStorage.directDocumentId("primary:roms", "Other", "Game.srm"))
        assertNull(RetroArchStorage.directDocumentId("primary:roms", "mGBA", "../hidden"))
        assertNull(RetroArchStorage.directDocumentId("primary:roms", "mGBA", "a/b"))
        assertNull(RetroArchStorage.directDocumentId("primary:roms", "mGBA", "."))
        assertNull(RetroArchStorage.directDocumentId("primary:roms/../outside", "mGBA", "Game.srm"))
        assertNull(RetroArchStorage.directDocumentId("primary:roms", "mGBA", "bad\\name"))
        assertNull(RetroArchStorage.directDocumentId("primary:roms", "mGBA", "bad\nname"))
    }
}
