//! `SLOT_TRACE_LATENCY=1`, or a `System/trace-latency.on` file on the card: follows one button
//! press at a time from the input reader to the swap that first shows its effect, and logs how
//! long each stage took to slot.log. Off by default.
//!
//! The stages run on three threads, so each stamp is set once, in order, and the render thread
//! logs and resets once the press has been swapped. A press arriving mid-flight is not tracked.
//! What it cannot see: the game's own frames of lag, and the 1.45 ms from the swap returning at
//! the SP's display latch to the vsync that puts the frame on the panel (measured with
//! slot-gfx's vsync_probe).

use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

pub const TRACE_VAR: &str = "SLOT_TRACE_LATENCY";
/// The device has no environment to set, so the card can ask instead.
pub const FLAG_FILE: &str = "System/trace-latency.on";

static READ: AtomicU64 = AtomicU64::new(0);
static APPLIED: AtomicU64 = AtomicU64::new(0);
static EMU: AtomicU64 = AtomicU64::new(0);
static PUBLISHED: AtomicU64 = AtomicU64::new(0);
static TAKEN: AtomicU64 = AtomicU64::new(0);
/// The bits the tracked press added to the pad.
static BITS: AtomicU16 = AtomicU16::new(0);

fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        let card = std::env::var_os("SLOT_ROOT").unwrap_or_else(|| "/mnt/sdcard".into());
        std::env::var_os(TRACE_VAR).is_some_and(|v| v != "0")
            || std::path::Path::new(&card).join(FLAG_FILE).is_file()
    })
}

/// Nanoseconds since the first stamp, plus one so zero can mean "not yet".
fn now() -> u64 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed().as_nanos() as u64 + 1
}

/// Set `slot` to now if it is unset and `after` is set: stages only ever fill in order.
fn stamp(slot: &AtomicU64, after: &AtomicU64) -> bool {
    if after.load(Ordering::Acquire) == 0 {
        return false;
    }
    slot.compare_exchange(0, now(), Ordering::AcqRel, Ordering::Relaxed)
        .is_ok()
}

/// The input reader saw a press.
pub fn read() {
    if on() {
        let _ = READ.compare_exchange(0, now(), Ordering::AcqRel, Ordering::Relaxed);
    }
}

/// The pad mask handed to the emulator, and the one before it.
pub fn applied(before: u16, after: u16) {
    let added = after & !before;
    if on() && added != 0 && READ.load(Ordering::Acquire) != 0 {
        BITS.store(added, Ordering::Release);
        stamp(&APPLIED, &READ);
    }
}

/// The emulator is about to run a frame with this mask.
pub fn emu_frame(mask: u16) {
    if on() && mask & BITS.load(Ordering::Acquire) != 0 {
        stamp(&EMU, &APPLIED);
    }
}

/// The emulator published the frame it just ran.
pub fn published() {
    if on() {
        stamp(&PUBLISHED, &EMU);
    }
}

/// The render loop took a frame to draw.
pub fn taken() {
    if on() {
        stamp(&TAKEN, &PUBLISHED);
    }
}

/// The swap returned, having taken `swap_ms` itself. Logs the press and starts over.
pub fn swapped(swap_ms: f64) {
    if !on() || TAKEN.load(Ordering::Acquire) == 0 {
        return;
    }
    let end = now();
    let [r, a, e, p, t] =
        [&READ, &APPLIED, &EMU, &PUBLISHED, &TAKEN].map(|s| s.swap(0, Ordering::AcqRel));
    BITS.store(0, Ordering::Release);
    let ms = |from: u64, to: u64| to.saturating_sub(from) as f64 / 1e6;
    eprintln!(
        "slot: latency: read>pad {:.1} pad>emu {:.1} emu>pub {:.1} pub>take {:.1} take>swapped {:.1} (swap {:.1}) total {:.1} ms",
        ms(r, a),
        ms(a, e),
        ms(e, p),
        ms(p, t),
        ms(t, end),
        swap_ms,
        ms(r, end)
    );
}

/// Whether the trace is on, for callers with counters of their own to report.
pub fn tracing() -> bool {
    on()
}
