package fyi.slot.konkr

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test

class RaDiagnosticsTest {
    @Test fun accountIdentityMatchesNativeCaseFoldedMd5() {
        assertEquals("912af0dff974604f1321254ca8ff38b6",
            RaDiagnostics.accountKey("Player"))
        assertEquals(RaDiagnostics.accountKey("Baggio94"),
            RaDiagnostics.accountKey("baggio94"))
    }

    @Test fun eachAccountKeepsASeparateCacheAndUnlockLedger() {
        assertNotEquals(RaDiagnostics.accountKey("Baggio94"),
            RaDiagnostics.accountKey("AnotherPlayer"))
    }
}
