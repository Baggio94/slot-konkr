use std::path::PathBuf;
use std::time::{Duration, Instant};

use slot::frontend::Frontend;
use slot::input::DeviceInput;
use slot_gfx::{Compositor, FbdevSurface, Surface};
use slot_power::{trace_first_frame, DevicePlatform};

/// Where BaseOS mounts the card slot has never been checked against a running device, so
/// `launch.sh` exports `SLOT_ROOT` and this is only what is left if it did not.
const CARD: &str = "/mnt/sdcard";

/// The loop never runs faster than this, for a driver whose swap returns at once.
const MIN_FRAME: Duration = Duration::from_millis(12);

/// The longest the display waits for the emulator's frame before drawing the last one again.
const STEP_TIMEOUT: Duration = Duration::from_millis(12);

/// Kept between the frame's work finishing and the latch, against the work running long.
const MARGIN: Duration = Duration::from_micros(1500);

/// The SP's Mali swap returns at the display engine's latch, 1.45 ms before each vsync, whenever it
/// is called: a frame submitted before the latch is on the panel at the next vsync, one submitted
/// after it waits a whole frame. So the loop paces on the swap, then sleeps through most of the
/// frame and does its work (input, drawing, the swap) just before the next latch. A fixed 16.667 ms
/// timer instead walked against the 16.76 ms panel and averaged half a frame of extra wait.
struct Pacer {
    last_return: Option<Instant>,
    /// The panel's frame, learned from the swap: 16.76 ms on the SP.
    period: Duration,
    /// The worst input, drawing and submitting of the last `WORK_WINDOW` frames. A window rather
    /// than a slow decay, so one long frame (a core loading) costs half a second, not fifteen.
    work: Duration,
    works: [Duration; WORK_WINDOW],
    next: usize,
}

const WORK_WINDOW: usize = 32;

impl Pacer {
    fn new() -> Self {
        Pacer {
            last_return: None,
            period: Duration::from_micros(16_760),
            work: Duration::from_millis(4),
            works: [Duration::from_millis(4); WORK_WINDOW],
            next: 0,
        }
    }

    /// Sleep until the work will just fit before the next latch.
    fn wait(&self) {
        let Some(last) = self.last_return else {
            return;
        };
        let delay = self.period.saturating_sub(self.work + MARGIN);
        if let Some(left) = (last + delay).checked_duration_since(Instant::now()) {
            std::thread::sleep(left);
        }
    }

    /// `work` is how long the frame took before its swap; `blocked` is whether the swap waited.
    /// Returns whether a latch went by with no new frame.
    fn swapped(&mut self, work: Duration, blocked: bool) -> bool {
        let now = Instant::now();
        let mut missed = false;
        if let Some(last) = self.last_return {
            let seen = now - last;
            missed = seen > self.period.mul_f64(1.5);
            // One frame apart, not a missed latch or a stall.
            if blocked && seen > Duration::from_millis(12) && seen < Duration::from_millis(22) {
                self.period = self.period.mul_f64(0.95) + seen.mul_f64(0.05);
            }
        }
        self.works[self.next] = work;
        self.next = (self.next + 1) % WORK_WINDOW;
        self.work = self.works.iter().copied().max().unwrap_or(work);
        self.last_return = Some(now);
        missed
    }
}

pub fn run() {
    let root = std::env::var_os("SLOT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(CARD));
    let mut surface = match FbdevSurface::new() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("slot: {e}");
            return;
        }
    };
    let mut compositor = match Compositor::new(&surface) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("slot: {e}");
            return;
        }
    };
    let platform = DevicePlatform::new(root.clone());
    eprintln!("slot: {}", platform.report());
    platform.trace_boot();
    let mut frontend = Frontend::boot(Box::new(platform));
    frontend.upload_faces(&mut compositor);
    let mut input = DeviceInput::open(&root);
    // The boot budget stopped at frontend-exec, which is the exec and not the picture on
    // the panel. Stamped after the swap rather than before it: the frame is only up once
    // EGL has taken it, and a number recorded earlier would flatter every measurement.
    let mut drawn = false;
    // The emulator runs its frame inside the display's, between reading input and drawing, so
    // the frame on the panel is the one run for the input just read.
    frontend.drive_emulator();
    let mut pacer = Pacer::new();
    // Trace only: latches that went by with no new frame, and frames drawn early enough to wait
    // more than half a frame for theirs.
    let (mut frames, mut missed, mut early) = (0u32, 0u32, 0u32);
    loop {
        pacer.wait();
        let began = Instant::now();
        frontend.advance(&mut input);
        if frontend.restarting() {
            frontend.restart();
        }
        if frontend.powering_off() {
            frontend.poweroff();
            return;
        }
        frontend.step_emulator(pacer.period, STEP_TIMEOUT);
        frontend.render(&mut compositor, surface.window_size());
        let swap = Instant::now();
        let work = swap - began;
        if let Err(e) = surface.swap() {
            eprintln!("slot: {e}");
            return;
        }
        let swap_took = swap.elapsed();
        slot::latency::swapped(swap_took.as_secs_f64() * 1000.0);
        let dropped = pacer.swapped(work, swap_took > Duration::from_millis(1));
        if slot::latency::tracing() {
            frames += 1;
            missed += u32::from(dropped);
            early += u32::from(swap_took > pacer.period / 2);
            if frames == 600 {
                eprintln!(
                    "slot: pace: 600 frames, {missed} missed a latch, {early} drawn early, work {:.1} ms, period {:.2} ms",
                    pacer.work.as_secs_f64() * 1e3,
                    pacer.period.as_secs_f64() * 1e3
                );
                (frames, missed, early) = (0, 0, 0);
            }
        }
        if !drawn {
            drawn = true;
            trace_first_frame();
        }
        if let Some(left) = MIN_FRAME.checked_sub(began.elapsed()) {
            std::thread::sleep(left);
        }
    }
}
