package fyi.slot.konkr

import android.os.Build
import android.view.KeyEvent

/**
 * Keyboard adapter for Android's Mac-hosted virtual KONKR ONLY.
 * Never reinterpret a real KONKR gamepad or a hardware keyboard on a device.
 *
 * Called at Activity.dispatchKeyEvent, before GLSurfaceView / GridView
 * consume character keys (Activity.onKeyDown is too late for those views).
 */
internal object SlotEmulatorKeys {
    fun enabled(): Boolean {
        val fingerprint = Build.FINGERPRINT.lowercase()
        val hardware = Build.HARDWARE.lowercase()
        return fingerprint.startsWith("generic") ||
            fingerprint.contains("sdk_gphone") ||
            hardware.contains("ranchu") ||
            hardware.contains("goldfish")
    }

    fun translate(code: Int): Int? = if (!enabled()) null else mapping(code)

    /**
     * Dialogs and secondary activities do not pass through MainActivity's
     * keyboard adapter. Translate Mac keys only while the user is NOT editing
     * a text field, so Z/X remain typeable in RA passwords and manual IP forms.
     * Hardware KONKR input is untouched.
     */
    fun dialogKeyCode(code: Int, editingText: Boolean = false): Int =
        mappedDialogKeyCode(code, editingText, enabled())

    internal fun mappedDialogKeyCode(code: Int, editingText: Boolean, emulator: Boolean): Int =
        if (editingText || !emulator) code else mapping(code) ?: code

    /** Pure mapping so it can be covered by JVM unit tests. */
    internal fun mapping(code: Int): Int? = when (code) {
        KeyEvent.KEYCODE_Z -> KeyEvent.KEYCODE_BUTTON_A
        KeyEvent.KEYCODE_X -> KeyEvent.KEYCODE_BUTTON_B
        KeyEvent.KEYCODE_A -> KeyEvent.KEYCODE_BUTTON_X
        KeyEvent.KEYCODE_S -> KeyEvent.KEYCODE_BUTTON_Y
        KeyEvent.KEYCODE_Q -> KeyEvent.KEYCODE_BUTTON_L1
        KeyEvent.KEYCODE_E -> KeyEvent.KEYCODE_BUTTON_R1
        KeyEvent.KEYCODE_1 -> KeyEvent.KEYCODE_BUTTON_L2
        KeyEvent.KEYCODE_2 -> KeyEvent.KEYCODE_BUTTON_R2
        KeyEvent.KEYCODE_ENTER -> KeyEvent.KEYCODE_BUTTON_START
        KeyEvent.KEYCODE_SPACE -> KeyEvent.KEYCODE_BUTTON_SELECT
        KeyEvent.KEYCODE_TAB -> KeyEvent.KEYCODE_BUTTON_MODE
        else -> null
    }
}
