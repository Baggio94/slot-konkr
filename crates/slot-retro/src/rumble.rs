use std::ffi::c_uint;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;

use crate::ffi::{RUMBLE_STRONG, RUMBLE_WEAK};

/// The core's end of the vibration motor. Written from the emulator thread, so setting it is
/// only an atomic store: never wait on the device there.
#[derive(Clone, Default)]
pub struct Rumble(Arc<Motors>);

#[derive(Default)]
struct Motors {
    strong: AtomicU16,
    weak: AtomicU16,
}

impl Rumble {
    /// The body of libretro's `set_rumble_state`, testable without a core.
    pub fn set(&self, port: c_uint, effect: c_uint, strength: u16) -> bool {
        // One pad, soldered in.
        if port != 0 {
            return false;
        }
        let motor = match effect {
            RUMBLE_STRONG => &self.0.strong,
            RUMBLE_WEAK => &self.0.weak,
            _ => return false,
        };
        motor.store(strength, Ordering::Relaxed);
        true
    }

    /// One motor for the two effects, so it takes the louder and stopping either one leaves
    /// the other running.
    pub fn strength(&self) -> u16 {
        self.0
            .strong
            .load(Ordering::Relaxed)
            .max(self.0.weak.load(Ordering::Relaxed))
    }
}
