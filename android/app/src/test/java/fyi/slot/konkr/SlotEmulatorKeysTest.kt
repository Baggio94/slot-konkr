package fyi.slot.konkr

import android.view.KeyEvent
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** AVD keyboard translation; these tests do not alter the physical KONKR. */
class SlotEmulatorKeysTest {
    @Test fun macLetterKeysBecomeGamepadButtons() {
        assertEquals(KeyEvent.KEYCODE_BUTTON_A, SlotEmulatorKeys.mapping(KeyEvent.KEYCODE_Z))
        assertEquals(KeyEvent.KEYCODE_BUTTON_B, SlotEmulatorKeys.mapping(KeyEvent.KEYCODE_X))
        assertEquals(KeyEvent.KEYCODE_BUTTON_X, SlotEmulatorKeys.mapping(KeyEvent.KEYCODE_A))
        assertEquals(KeyEvent.KEYCODE_BUTTON_Y, SlotEmulatorKeys.mapping(KeyEvent.KEYCODE_S))
    }

    @Test fun modalKeyboardNeverSwallowsTypedCredentialsOrIpText() {
        assertEquals(KeyEvent.KEYCODE_BUTTON_A,
            SlotEmulatorKeys.mappedDialogKeyCode(KeyEvent.KEYCODE_Z, false, true))
        assertEquals(KeyEvent.KEYCODE_BUTTON_B,
            SlotEmulatorKeys.mappedDialogKeyCode(KeyEvent.KEYCODE_X, false, true))
        assertEquals(KeyEvent.KEYCODE_Z,
            SlotEmulatorKeys.mappedDialogKeyCode(KeyEvent.KEYCODE_Z, true, true))
        assertEquals(KeyEvent.KEYCODE_X,
            SlotEmulatorKeys.mappedDialogKeyCode(KeyEvent.KEYCODE_X, true, true))
        assertEquals(KeyEvent.KEYCODE_Z,
            SlotEmulatorKeys.mappedDialogKeyCode(KeyEvent.KEYCODE_Z, false, false))
        assertEquals(KeyEvent.KEYCODE_DPAD_UP,
            SlotEmulatorKeys.mappedDialogKeyCode(KeyEvent.KEYCODE_DPAD_UP, false, true))
    }

    @Test fun menuAndNavigationKeepTheirDocumentedKeys() {
        assertEquals(KeyEvent.KEYCODE_BUTTON_START, SlotEmulatorKeys.mapping(KeyEvent.KEYCODE_ENTER))
        assertEquals(KeyEvent.KEYCODE_BUTTON_SELECT, SlotEmulatorKeys.mapping(KeyEvent.KEYCODE_SPACE))
        assertEquals(KeyEvent.KEYCODE_BUTTON_MODE, SlotEmulatorKeys.mapping(KeyEvent.KEYCODE_TAB))
        assertNull(SlotEmulatorKeys.mapping(KeyEvent.KEYCODE_DPAD_LEFT))
        assertNull(SlotEmulatorKeys.mapping(KeyEvent.KEYCODE_BUTTON_A))
        assertNull(SlotEmulatorKeys.mapping(KeyEvent.KEYCODE_BACK))
    }
}
