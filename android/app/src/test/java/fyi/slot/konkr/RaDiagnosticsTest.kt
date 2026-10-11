package fyi.slot.konkr

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test

class RaDiagnosticsTest {
    @Test fun accountIdentityMatchesNativeCaseFoldedMd5() {
        assertEquals("eb4c88f776c3f9b33d2285b891cf06c2",
            RaDiagnostics.accountKey("Player"))
        assertEquals(RaDiagnostics.accountKey("Baggio94"),
            RaDiagnostics.accountKey("baggio94"))
    }

    @Test fun eachAccountKeepsASeparateCacheAndUnlockLedger() {
        assertNotEquals(RaDiagnostics.accountKey("Baggio94"),
            RaDiagnostics.accountKey("AnotherPlayer"))
    }
}
