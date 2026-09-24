use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::drc::drc_target;

/// About eight video frames of audio. Four clicked: one late frame or callback from either end.
pub fn ring_capacity(sample_rate: u32) -> usize {
    sample_rate as usize * 8 / 60
}

/// Longest the emulator waits for room. Past this the device is not draining, and the audio
/// should pay for it, not the game.
const MAX_WAIT: Duration = Duration::from_millis(50);

struct Inner {
    buf: Vec<i16>,
    head: usize,
    len: usize,
    /// A mixed clip longer than the ring, held until the device makes room for the rest.
    pending: Vec<i16>,
    /// How much of `pending` has been laid into the buffer already.
    placed: usize,
    /// Where the next unplaced sample of `pending` belongs, in `drained`'s units: the head
    /// moves before the rest fits, so the clip is positioned against the stream.
    pending_at: u64,
    /// Samples handed to the device since the ring opened. Monotonic.
    drained: u64,
}

/// Interleaved stereo between the emulator thread and the device callback.
pub struct Ring {
    inner: Mutex<Inner>,
    /// Signalled by every device read; a producer with no room waits on it.
    room: Condvar,
    /// Mirror of `Inner::len`, readable without contending with the callback.
    queued: AtomicUsize,
    capacity: AtomicUsize,
    rate: AtomicU32,
    muted: AtomicBool,
    /// Whether anything should be feeding the ring. A dry ring is only a fault when so.
    idle: AtomicBool,
    overruns: AtomicU64,
    underruns: AtomicU64,
    /// Set by device reads, cleared when a wait for room expires, so the emulator is not held
    /// up by a device that is not draining.
    consuming: AtomicBool,
    /// Rebuilding the cushion after running dry. The device gets clean silence meanwhile;
    /// alternating fragments and padding sounds like a scratch.
    priming: AtomicBool,
}

impl Ring {
    pub fn new(capacity_frames: usize) -> Self {
        Ring {
            inner: Mutex::new(Inner {
                buf: vec![0; capacity_frames * 2],
                head: 0,
                len: 0,
                pending: Vec::new(),
                placed: 0,
                pending_at: 0,
                drained: 0,
            }),
            room: Condvar::new(),
            queued: AtomicUsize::new(0),
            idle: AtomicBool::new(false),
            capacity: AtomicUsize::new(capacity_frames),
            rate: AtomicU32::new(0),
            muted: AtomicBool::new(false),
            overruns: AtomicU64::new(0),
            underruns: AtomicU64::new(0),
            consuming: AtomicBool::new(true),
            priming: AtomicBool::new(false),
        }
    }

    /// Resize for the device's actual rate, discarding unplayed audio. Opens at the DRC target,
    /// not empty: the device pulls a block before the core runs a frame.
    pub fn reopen(&self, sample_rate: u32) {
        let frames = ring_capacity(sample_rate);
        let mut i = self.lock();
        i.buf = vec![0; frames * 2];
        i.head = 0;
        // A half-laid clip belonged to the discarded buffer.
        i.pending = Vec::new();
        i.placed = 0;
        i.pending_at = 0;
        i.drained = 0;
        // Must be even (a stereo frame is two samples), or every later sample lands one slot
        // out and the channels swap. At 32768 Hz `ring_capacity` is 4369, odd.
        i.len = drc_target(frames) * 2;
        self.queued.store(i.len, Ordering::Relaxed);
        self.capacity.store(frames, Ordering::Relaxed);
        self.rate.store(sample_rate, Ordering::Relaxed);
        self.overruns.store(0, Ordering::Relaxed);
        self.underruns.store(0, Ordering::Relaxed);
        self.consuming.store(true, Ordering::Relaxed);
        // Opened at the target already.
        self.priming.store(false, Ordering::Relaxed);
    }

    /// Overflow is dropped rather than evicting queued audio, so what plays stays contiguous.
    pub fn push(&self, samples: &[i16]) {
        let mut i = self.lock();
        let lost = write_into(&mut i, samples).len();
        self.queued.store(i.len, Ordering::Relaxed);
        drop(i);
        // Muted output and a bufferless ring are not overruns.
        if lost > 0 && !self.muted() && self.capacity_frames() > 0 {
            self.overruns.fetch_add(lost as u64, Ordering::Relaxed);
        }
    }

    /// Emulator side. Waits for room rather than discarding, so a worker running slightly fast
    /// stays in step with the device.
    pub fn push_blocking(&self, samples: &[i16]) {
        if self.muted() || self.capacity_frames() == 0 || !self.consuming.load(Ordering::Relaxed) {
            self.push(samples);
            return;
        }
        let deadline = Instant::now() + MAX_WAIT;
        let mut rest = samples;
        let mut i = self.lock();
        loop {
            rest = write_into(&mut i, rest);
            self.queued.store(i.len, Ordering::Relaxed);
            // Muted mid wait: the rest will never play.
            if rest.is_empty() || self.muted() {
                return;
            }
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                self.consuming.store(false, Ordering::Relaxed);
                self.overruns
                    .fetch_add(rest.len() as u64, Ordering::Relaxed);
                return;
            };
            i = self
                .room
                .wait_timeout(i, left)
                .map(|(guard, _)| guard)
                .unwrap_or_else(|e| e.into_inner().0);
        }
    }

    /// Adds on top of what is queued, so a UI sound is not a whole buffer late.
    ///
    /// The cart sounds (240 and 315 ms) outlast the ring (133 ms), so the overflow is held and
    /// laid down by later device reads rather than cut off.
    pub fn mix(&self, samples: &[i16]) {
        // On the shelf there is no core to rebuild a cushion, so priming would swallow the clip.
        self.priming.store(false, Ordering::Relaxed);
        let mut i = self.lock();
        // Anything still pending belongs to a replaced sound.
        i.pending.clear();
        i.placed = 0;
        if i.buf.is_empty() {
            return;
        }
        // The clip starts at the head, which in stream terms is `drained`.
        i.pending_at = i.drained;
        i.pending.extend_from_slice(samples);
        lay_pending(&mut i);
        self.queued.store(i.len, Ordering::Relaxed);
    }

    /// Device side. Pads an underrun with silence. Muted output still drains, so occupancy
    /// means the same to DRC.
    pub fn fill(&self, out: &mut [i16]) {
        let mut i = self.lock();
        // Before the copy, so an overlong clip's rest lands in this read, not a period later.
        lay_pending(&mut i);
        let cap = i.buf.len();
        // While priming, hand over silence until the ring reaches half (the DRC target).
        let holding = self.priming.load(Ordering::Relaxed) && cap > 0 && i.len < cap / 2;
        self.priming.store(holding, Ordering::Relaxed);
        let n = if holding { 0 } else { out.len().min(i.len) };
        if cap > 0 && !holding {
            let first = n.min(cap - i.head);
            out[..first].copy_from_slice(&i.buf[i.head..i.head + first]);
            out[first..n].copy_from_slice(&i.buf[..n - first]);
            i.head = (i.head + n) % cap;
            i.len -= n;
            i.drained += n as u64;
        }
        out[n..].fill(0);
        self.queued.store(i.len, Ordering::Relaxed);
        drop(i);
        // Even an empty read proves the device is alive.
        self.consuming.store(true, Ordering::Relaxed);
        self.room.notify_all();
        let short = out.len() - n;
        if short > 0 && !self.muted() && !self.idle.load(Ordering::Relaxed) && cap > 0 {
            self.underruns.fetch_add(short as u64, Ordering::Relaxed);
            // Ride it out on silence and return with a full cushion: one gap beats a scratch.
            self.priming.store(true, Ordering::Relaxed);
        }
        if self.muted() {
            out.fill(0);
        }
    }

    pub fn queued_frames(&self) -> usize {
        self.queued.load(Ordering::Relaxed) / 2
    }

    pub fn capacity_frames(&self) -> usize {
        self.capacity.load(Ordering::Relaxed)
    }

    /// Samples dropped for want of room. Any nonzero value is audible.
    pub fn overruns(&self) -> u64 {
        self.overruns.load(Ordering::Relaxed)
    }

    /// Samples the device asked for and got silence.
    pub fn underruns(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }

    /// Called by whoever takes the ring over, so a cart does not inherit the shelf's silence.
    pub fn clear_faults(&self) {
        self.overruns.store(0, Ordering::Relaxed);
        self.underruns.store(0, Ordering::Relaxed);
    }

    /// Zero until the device opens.
    pub fn sample_rate(&self) -> u32 {
        self.rate.load(Ordering::Relaxed)
    }

    /// A paused core feeds nothing, so a dry ring then is not an underrun.
    pub fn set_idle(&self, idle: bool) {
        self.idle.store(idle, Ordering::Relaxed);
    }

    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
        // Wake a producer parked on a sink that just went silent.
        self.room.notify_all();
    }

    pub fn muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }

    /// Ignores poisoning: one garbled buffer beats losing audio for the session.
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Lays as much of a held clip as fits, at its place in the stream rather than at the head.
/// Called by `mix` and by every device read.
fn lay_pending(i: &mut Inner) {
    let cap = i.buf.len();
    if cap == 0 || i.placed >= i.pending.len() {
        return;
    }
    // Offset past the head for the next unplaced sample. Only ever counts down.
    let Some(off) = i.pending_at.checked_sub(i.drained) else {
        // The device already played past where the rest belonged.
        i.pending.clear();
        i.placed = 0;
        return;
    };
    let off = off as usize;
    if off >= cap {
        return;
    }
    let n = (i.pending.len() - i.placed).min(cap - off);
    // Taken before the writes, so the whole run is judged against the same queue.
    let len = i.len;
    for k in 0..n {
        let s = i.pending[i.placed + k];
        let at = (i.head + off + k) % cap;
        // Past the queued run the buffer holds already-played samples: overwrite, do not add.
        i.buf[at] = if off + k < len {
            i.buf[at].saturating_add(s)
        } else {
            s
        };
    }
    i.len = i.len.max(off + n);
    i.placed += n;
    i.pending_at += n as u64;
    if i.placed >= i.pending.len() {
        i.pending = Vec::new();
        i.placed = 0;
    }
}

/// Writes what fits and returns the rest.
fn write_into<'a>(i: &mut Inner, samples: &'a [i16]) -> &'a [i16] {
    let cap = i.buf.len();
    if cap == 0 {
        return samples;
    }
    let n = samples.len().min(cap - i.len);
    let start = (i.head + i.len) % cap;
    let first = n.min(cap - start);
    i.buf[start..start + first].copy_from_slice(&samples[..first]);
    i.buf[..n - first].copy_from_slice(&samples[first..n]);
    i.len += n;
    &samples[n..]
}
