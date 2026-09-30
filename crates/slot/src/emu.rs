use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::cable::{self, Cable};
use slot_retro::{
    ButtonMask, Link, LinkChannel, RetroCore, Rumble, GBA_H, GBA_W, NETPACKET_RELIABLE,
};

use crate::audio::Ring;
use crate::drc::{drc_ratio, drc_target};
use crate::frames::{FrameRef, Frames};
use crate::persist::Snapshot;
use crate::resample::Resampler;
use crate::rewind::{RewindThread, REWIND_BYTES};

/// Present is locked to the 60 Hz panel and the core is stepped once per present, so the
/// 0.456% the GBA runs slow lands entirely on audio rate control.
const PRESENT: Duration = Duration::from_nanos(16_666_667);

/// Default fast forward ceiling, and the top of the quick menu's row. Must equal
/// `slot_store::FF_SPEED_DEFAULT` so a worker started before the card is read agrees with it.
pub const FAST_STEPS: u32 = 6;

/// The most core frames one fast forward present may run. On the SP eight was no faster than
/// six (mGBA 281 vs 280 fps, gpSP 5.4 vs 5.3 frames a present) and overran far more often.
/// Also stays under the 30 consecutive skips after which both cores force a render.
pub const FAST_STEPS_MAX: u32 = 6;

/// What one fast forward present aims to spend in total: core frames plus the publish,
/// snapshot, audio and link pump after them. 14 ms puts the measured tail near 15.5 ms on the
/// SP, inside the 16.67 ms present.
const FAST_TARGET: Duration = Duration::from_micros(14_000);

/// Share of the running per-frame estimate each measurement replaces: a quarter. Slow enough to
/// ride out one descheduled present, quick enough to follow a scene change.
const COST_BLEND: u32 = 4;

/// Snapshot every other frame, so rewinding at one pop per present runs back at 2x. On the H700
/// a snapshot costs 9.2 ms of the 16.67 ms frame, too much for every frame.
const SNAPSHOT_EVERY: u32 = 2;

/// How long a locked worker waits for the display before running a frame on its own: a frame
/// and a half, so a late tick is still followed and a stalled display does not stall the game.
const STALL: Duration = Duration::from_millis(25);

/// Ticks per window when measuring the display's present for the audio base rate.
const RATE_WINDOW: u32 = 300;

/// Frames between traced pacing lines, about five seconds.
const TRACE_EVERY: u64 = 300;

/// Per-present cap on packets moved from the transport into the core, so a flooding peer costs
/// bounded work. Real serial traffic never approaches it.
const MAX_LINK_PACKETS_PER_PRESENT: u32 = 256;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Speed {
    Paused,
    Normal,
    Fast,
}

impl Speed {
    /// The one place the atomic encoding is decoded, for both the worker and the handle.
    fn from_u8(v: u8) -> Speed {
        match v {
            0 => Speed::Paused,
            1 => Speed::Normal,
            _ => Speed::Fast,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum CoreState {
    Loading,
    Ready,
    Failed,
}

pub struct EmuHandle {
    frames: Arc<Frames>,
    shared: Arc<Shared>,
    cmds: Sender<Cmd>,
    join: Option<JoinHandle<()>>,
    rumble: Rumble,
    /// Taken from the core exactly once: `RetroCore::net`'s default returns a fresh, unrelated
    /// queue on each call.
    link: Link,
}

enum Cmd {
    Load(Vec<u8>),
    Save(Sender<Vec<u8>>),
    Sav(Sender<Option<Vec<u8>>>),
    Thumb(Sender<Option<Vec<u8>>>),
    /// Wires a transport to the core's serial traffic. `client_id` is libretro's own: 0 the
    /// host, 1 the joiner.
    BeginLink(u16, Box<dyn LinkChannel>),
    /// Drops the transport, which closes the wire, and ends the session.
    EndLink,
    /// The emulated link instead of netpacket. `player` is which port this device drives; the
    /// core runs both consoles and the transport carries button masks.
    BeginCable(u8, Box<dyn LinkChannel>),
    /// A core option for a running core, for settings the player can change mid-game. The rest
    /// are set before `load` in `core::apply_core_options`.
    SetOption(String, String),
}

struct Shared {
    input: AtomicU16,
    speed: AtomicU8,
    /// What the worker last read `speed` as; see `EmuHandle::observed_speed`.
    observed: AtomicU8,
    state: AtomicU8,
    rewind: AtomicBool,
    /// How much rewind history is left, 0 to 100, for the HUD bar to draw.
    rewind_fill: AtomicU8,
    stop: AtomicBool,
    /// 0 to 100. Read by the worker every batch, so a change lands within one frame.
    volume: AtomicU8,
    /// Most core frames a fast forward present may run, 1 to `FAST_STEPS_MAX`. A ceiling, not a
    /// count.
    fast_steps: AtomicU32,
    /// Whether fast forward is heard, sped up, rather than dropped.
    ff_sound: AtomicBool,
    /// Frames published. Counted because `Frames::latest` consumes, so peeking would steal a
    /// frame from the renderer.
    published: AtomicU64,
    /// Set at open if the core refused the resume state. Its `serialize()` is then not the
    /// player's progress and must not overwrite the resume file. See `EmuSnapshot::resume_trusted`.
    resume_refused: AtomicBool,
    /// Save-ram twin of `resume_refused`, separate because a core can accept one and refuse the
    /// other. See `EmuSnapshot::save_ram_trusted`.
    sav_refused: AtomicBool,
    /// The transport's far end went away during a session. Cleared when a session begins or
    /// ends. See `EmuHandle::link_lost`.
    link_lost: AtomicBool,
    /// Frames stepped through `run_frame_linked`: tells a running emulated link from one merely
    /// begun.
    linked: AtomicU64,
    /// The far end said it was ending the session, rather than merely going away. Separate from
    /// `link_lost` because a goodbye then a dropped wire sets both. Cleared like `link_lost`.
    peer_ended: AtomicBool,
    /// The display drives the frame clock, via `tick`. Set by the display loop that ticks.
    driven: AtomicBool,
    /// Following the display right now: driven, at normal speed, not rewinding, no link session.
    /// Everything else keeps the worker's own clock, so it can never stall the display.
    locked: AtomicBool,
    /// The display's present, in nanoseconds: the audio base rate's first guess.
    present_ns: AtomicU64,
    clock: Mutex<Clock>,
    clocked: Condvar,
}

/// Ticks sent by the display, and the last tick whose frame the worker has published.
#[derive(Default)]
struct Clock {
    tick: u64,
    done: u64,
}

impl EmuHandle {
    /// Takes the ring, not the device: the ring outlives the cart, so the slot can make sound
    /// with no core running.
    pub fn spawn(
        core: Box<dyn RetroCore>,
        rom: PathBuf,
        ring: Arc<Ring>,
        sav: Option<Vec<u8>>,
        resume: Option<Vec<u8>>,
    ) -> Self {
        // Taken before the core moves to its thread. `net()` only once: the default returns a
        // fresh, disconnected queue on every call.
        let rumble = core.rumble();
        let link = core.net();
        let frames = Frames::new((GBA_W * GBA_H * 4) as usize);
        let shared = Arc::new(Shared {
            input: AtomicU16::new(0),
            // Paused until `sync_speed` says otherwise, or the insert would run the start of the
            // BIOS boot animation.
            speed: AtomicU8::new(Speed::Paused as u8),
            observed: AtomicU8::new(Speed::Paused as u8),
            state: AtomicU8::new(CoreState::Loading as u8),
            rewind: AtomicBool::new(false),
            rewind_fill: AtomicU8::new(0),
            stop: AtomicBool::new(false),
            volume: AtomicU8::new(100),
            fast_steps: AtomicU32::new(FAST_STEPS),
            ff_sound: AtomicBool::new(false),
            published: AtomicU64::new(0),
            resume_refused: AtomicBool::new(false),
            sav_refused: AtomicBool::new(false),
            link_lost: AtomicBool::new(false),
            linked: AtomicU64::new(0),
            peer_ended: AtomicBool::new(false),
            driven: AtomicBool::new(false),
            locked: AtomicBool::new(false),
            present_ns: AtomicU64::new(PRESENT.as_nanos() as u64),
            clock: Mutex::new(Clock::default()),
            clocked: Condvar::new(),
        });
        let (tx, rx) = channel();
        let worker = Worker {
            frames: frames.clone(),
            shared: shared.clone(),
            cmds: rx,
        };
        // The worker pumps its own clone; this side keeps one for `EmuHandle::net`.
        let worker_link = link.clone();
        let join = std::thread::Builder::new()
            .name("slot-emu".into())
            .spawn(move || worker.run(core, rom, ring, sav, resume, worker_link))
            .ok();
        if join.is_none() {
            shared
                .state
                .store(CoreState::Failed as u8, Ordering::Release);
        }
        EmuHandle {
            frames,
            shared,
            cmds: tx,
            join,
            rumble,
            link,
        }
    }

    /// The core's end of the motor, written from the emulator thread and read on the render
    /// thread, which does the device write.
    pub fn rumble(&self) -> &Rumble {
        &self.rumble
    }

    /// The core's end of its own serial traffic. Tests push and read packets through it.
    pub fn net(&self) -> &Link {
        &self.link
    }

    /// Frames stepped through the emulated link.
    pub fn linked_frames(&self) -> u64 {
        self.shared.linked.load(Ordering::Relaxed)
    }

    /// The transport's far end went away during a session. Cleared when a session begins or
    /// ends.
    pub fn link_lost(&self) -> bool {
        self.shared.link_lost.load(Ordering::Relaxed)
    }

    /// The far end said it was ending the session, rather than merely vanishing. Check it before
    /// `link_lost`: the peer drops its wire right after, so both are soon up.
    pub fn peer_ended(&self) -> bool {
        self.shared.peer_ended.load(Ordering::Relaxed)
    }

    /// Wires a transport into the core's serial traffic on the emulator thread, the only thread
    /// allowed to call into a libretro core. `client_id`: 0 the host, 1 the joiner.
    pub fn begin_link(&self, client_id: u16, transport: Box<dyn LinkChannel>) {
        let _ = self.cmds.send(Cmd::BeginLink(client_id, transport));
    }

    /// Wires a transport to the emulated link instead of netpacket. `player` is which port this
    /// device drives; the frame clock moves to `cable.rs`.
    pub fn begin_cable(&self, player: u8, transport: Box<dyn LinkChannel>) {
        let _ = self.cmds.send(Cmd::BeginCable(player, transport));
    }

    /// Drops the transport and ends the session. Safe whether or not one was begun.
    pub fn end_link(&self) {
        let _ = self.cmds.send(Cmd::EndLink);
    }

    /// Hands a core option to the running core. Dropped if the worker has gone.
    pub fn set_option(&self, key: &str, value: &str) {
        let _ = self
            .cmds
            .send(Cmd::SetOption(key.to_owned(), value.to_owned()));
    }

    /// Hand the frame clock to the display, which then calls `tick` once per present. A worker
    /// nobody ticks keeps its own clock.
    pub fn set_driven(&self, driven: bool) {
        self.shared.driven.store(driven, Ordering::Relaxed);
        self.shared.clocked.notify_all();
    }

    /// Whether the worker is following the display right now. See `Shared::locked`.
    pub fn locked(&self) -> bool {
        self.shared.locked.load(Ordering::Acquire)
    }

    /// One present of the display, which lasts `present`: a locked worker runs one frame for it.
    pub fn tick(&self, present: Duration) {
        self.shared
            .present_ns
            .store(present.as_nanos() as u64, Ordering::Relaxed);
        let mut clock = self.shared.clock.lock().unwrap_or_else(|e| e.into_inner());
        clock.tick += 1;
        self.shared.clocked.notify_all();
    }

    /// Waits, at most `timeout`, for the frame of the last tick to be published. Returns at once,
    /// false, when the worker is not locked, so a paused or fast forwarding game never stalls the
    /// display.
    pub fn wait_frame(&self, timeout: Duration) -> bool {
        let until = Instant::now() + timeout;
        let mut clock = self.shared.clock.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if !self.locked() {
                return false;
            }
            if clock.done >= clock.tick {
                return true;
            }
            let Some(left) = until.checked_duration_since(Instant::now()) else {
                return false;
            };
            clock = match self.shared.clocked.wait_timeout(clock, left) {
                Ok((c, _)) => c,
                Err(e) => e.into_inner().0,
            };
        }
    }

    pub fn set_input(&self, mask: ButtonMask) {
        let before = self.shared.input.swap(mask.0, Ordering::Relaxed);
        crate::latency::applied(before, mask.0);
    }

    /// What the worker will read on its next pass. Lets tests check whether a press a menu used
    /// reached the core.
    pub fn input(&self) -> ButtonMask {
        ButtonMask(self.shared.input.load(Ordering::Relaxed))
    }

    pub fn latest_frame(&self) -> Option<FrameRef> {
        self.frames.latest()
    }

    pub fn set_speed(&self, speed: Speed) {
        self.shared.speed.store(speed as u8, Ordering::Relaxed);
    }

    /// Whether this core has produced anything yet. Never gate the game layer on the handle
    /// existing: it is built before its worker has run a frame.
    pub fn has_published(&self) -> bool {
        self.shared.published.load(Ordering::Relaxed) > 0
    }

    pub fn frame_ready(&self) -> bool {
        self.frames.is_ready()
    }

    pub fn frames_taken(&self) -> u64 {
        self.frames.taken()
    }

    pub fn published_count(&self) -> u64 {
        self.shared.published.load(Ordering::Relaxed)
    }

    /// What the worker last read `speed` as, not what it was told: the gap is the race an eject
    /// must close. `Acquire` pairs with the worker's `Release`, so seeing `Paused` also means
    /// seeing every frame published before it.
    pub fn observed_speed(&self) -> Speed {
        Speed::from_u8(self.shared.observed.load(Ordering::Acquire))
    }

    pub fn set_volume(&self, level: u8) {
        self.shared.volume.store(level.min(100), Ordering::Relaxed);
    }

    /// The most core frames a fast forward present may run, clamped to 1..=`FAST_STEPS_MAX`,
    /// the range the budget was measured for.
    pub fn set_fast_steps(&self, steps: u32) {
        self.shared
            .fast_steps
            .store(steps.clamp(1, FAST_STEPS_MAX), Ordering::Relaxed);
    }

    /// The ceiling the worker will hold its next fast forward present to.
    pub fn fast_steps(&self) -> u32 {
        self.shared.fast_steps.load(Ordering::Relaxed)
    }

    /// Whether fast forward is heard, squeezed into real time by the resampler, rather than
    /// dropped. Rewind is silent either way.
    pub fn set_ff_sound(&self, on: bool) {
        self.shared.ff_sound.store(on, Ordering::Relaxed);
    }

    pub fn ff_sound(&self) -> bool {
        self.shared.ff_sound.load(Ordering::Relaxed)
    }

    /// L2 is a separate axis from `Speed`: it overrides fast forward, and releasing it returns
    /// to whatever the speed was.
    pub fn set_rewinding(&self, on: bool) {
        self.shared.rewind.store(on, Ordering::Relaxed);
    }

    pub fn rewind_fill(&self) -> u8 {
        self.shared.rewind_fill.load(Ordering::Relaxed)
    }

    pub fn state(&self) -> CoreState {
        match self.shared.state.load(Ordering::Acquire) {
            0 => CoreState::Loading,
            1 => CoreState::Ready,
            _ => CoreState::Failed,
        }
    }

    /// The state arrives on the receiver once the worker reaches a frame boundary. A dead
    /// worker closes the channel rather than leaving the caller waiting forever.
    pub fn request_state(&self) -> Receiver<Vec<u8>> {
        let (tx, rx) = channel();
        let _ = self.cmds.send(Cmd::Save(tx));
        rx
    }

    pub fn request_load(&self, state: Vec<u8>) {
        let _ = self.cmds.send(Cmd::Load(state));
    }

    pub fn snapshot(&self) -> EmuSnapshot {
        EmuSnapshot {
            cmds: self.cmds.clone(),
            shared: self.shared.clone(),
        }
    }
}

/// What the flush paths need: the command sender, and the refusal flags behind
/// `resume_trusted`/`save_ram_trusted`.
#[derive(Clone)]
pub struct EmuSnapshot {
    cmds: Sender<Cmd>,
    shared: Arc<Shared>,
}

impl Snapshot for EmuSnapshot {
    fn state(&self) -> Option<Vec<u8>> {
        let (tx, rx) = channel();
        self.cmds.send(Cmd::Save(tx)).ok()?;
        rx.recv().ok()
    }

    fn save_ram(&self) -> Option<Vec<u8>> {
        let (tx, rx) = channel();
        self.cmds.send(Cmd::Sav(tx)).ok()?;
        rx.recv().ok().flatten()
    }

    fn thumb(&self) -> Option<Vec<u8>> {
        let (tx, rx) = channel();
        self.cmds.send(Cmd::Thumb(tx)).ok()?;
        rx.recv().ok().flatten()
    }

    fn load(&self, state: Vec<u8>) {
        let _ = self.cmds.send(Cmd::Load(state));
    }

    /// `false` when the core refused its resume. `state()` still returns bytes, so a flush must
    /// check this before writing them over the resume file.
    fn resume_trusted(&self) -> bool {
        !self.shared.resume_refused.load(Ordering::Acquire)
    }

    /// The save-ram twin of `resume_trusted`.
    fn save_ram_trusted(&self) -> bool {
        !self.shared.sav_refused.load(Ordering::Acquire)
    }
}

impl Drop for EmuHandle {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

struct Worker {
    frames: Arc<Frames>,
    shared: Arc<Shared>,
    cmds: Receiver<Cmd>,
}

impl Worker {
    fn run(
        self,
        mut core: Box<dyn RetroCore>,
        rom: PathBuf,
        ring: Arc<Ring>,
        sav: Option<Vec<u8>>,
        resume: Option<Vec<u8>>,
        link: Link,
    ) {
        if let Err(e) = core.load(&rom) {
            eprintln!("slot: {e}");
            self.shared
                .state
                .store(CoreState::Failed as u8, Ordering::Release);
            return;
        }
        // After the load, which sizes save ram. A game with none is not a failure to boot.
        if let Some(sav) = sav {
            if let Err(e) = core.load_save_ram(&sav) {
                eprintln!("slot: save ram: {e}");
                // The core now runs its own save ram, which must not overwrite the player's.
                self.shared.sav_refused.store(true, Ordering::Release);
            }
        }
        // Before Ready, so the reveal shows where the cart left off. A refused state still
        // leaves the save ram loaded: position lost, progress kept.
        if let Some(resume) = resume {
            if let Err(e) = core.unserialize(&resume) {
                eprintln!("slot: resume: {e}");
                // As with `sav_refused`: this core's `serialize()` is not the player's session.
                self.shared.resume_refused.store(true, Ordering::Release);
            }
        }
        let av = core.av_info();
        // A device that refused the GBA's rate reports its own, and a device that failed to
        // open reports zero, which the resampler reads as "no conversion to do".
        let device_hz = match ring.sample_rate() {
            0 => av.sample_rate,
            hz => hz as f64,
        };
        // Stepped once per 60 Hz present, each frame carries 59.7275 Hz worth of audio. Absorb
        // it in the base rate: left to DRC, occupancy parks at a measured 91% of the ring.
        let core_hz = match av.fps {
            fps if fps > 0.0 => av.sample_rate / (fps * PRESENT.as_secs_f64()),
            _ => av.sample_rate,
        };
        let mut resampler = Resampler::new(core_hz, device_hz);
        ring.clear_faults();
        self.shared
            .state
            .store(CoreState::Ready as u8, Ordering::Release);

        let mut out = Vec::new();
        // What the ring was last told: muted, and idle. Neither, to begin with.
        let mut gated = (false, false);
        let rewind = RewindThread::spawn(REWIND_BYTES);
        let mut since_snapshot = 0;
        // Worst recent core frame cost, to predict whether another fits in a fast present. Worst,
        // not mean: the mean overran 54% of mGBA presents. Seeded at a whole present, cautious.
        let mut frame_peak = PRESENT;
        // Measured cost of a present's work after its core frames, blended. Seeded
        // pessimistically at the full margin `FAST_TARGET` leaves.
        let mut post_cost = PRESENT - FAST_TARGET;
        // Set by a fast present: when it began and how long its core frames took.
        let mut fast_span: Option<(Instant, Duration)> = None;
        let mut deadline = Instant::now();
        let mut served = 0u64;
        let mut ticked = false;
        let mut paced = 0u64;
        // Locked to the display, the audio's base rate: the present the display actually keeps,
        // against the 60 Hz the resampler was built for. Measured over windows of real ticks,
        // since the panel is not 60 Hz (16.79 ms on the SP) and rate control only reaches 0.5%.
        let mut scale = 0.0f64;
        let mut window = (Instant::now(), 0u32, true);
        let mut stalled = false;
        // `None` until a session begins. Owned by this loop, which alone drains and feeds it.
        let mut transport: Option<Box<dyn LinkChannel>> = None;
        // `Some` only on the emulated link route; netpacket sessions leave it `None`.
        let mut cable: Option<Cable> = None;
        // Emulated link diagnostics: slow and stalling look alike except in the device log.
        let mut cable_presents = 0u32;
        let mut cable_stalls = 0u32;
        let mut cable_said = Instant::now();
        let mut cable_core = Duration::ZERO;
        let mut cable_wait = Duration::ZERO;
        while !self.shared.stop.load(Ordering::Relaxed) {
            for cmd in self.cmds.try_iter() {
                self.apply(cmd, core.as_mut(), &mut transport, &mut cable, &link);
            }

            // Pumped every present at any speed, so pausing locally never makes the link go
            // quiet. Never blocks.
            if let Some(t) = transport.as_mut() {
                // Only one route may read the wire: `drain_transport` would swallow the emulated
                // link's button masks.
                match cable.as_mut() {
                    Some(c) => {
                        while let Some(buf) = t.try_recv() {
                            c.accept(&buf);
                        }
                        // The host's machine, once whole. Restored here: `cable.rs` holds no core.
                        if let Some(state) = c.take_state() {
                            match core.unserialize(&state) {
                                Ok(()) => {
                                    eprintln!(
                                        "slot: cable: restored {} bytes, running the host's game",
                                        state.len()
                                    );
                                    c.prime();
                                    t.send(NETPACKET_RELIABLE, &cable::ready_packet());
                                }
                                // Refuse to start rather than run a different game.
                                Err(e) => eprintln!("slot: cable: the state was refused: {e}"),
                            }
                        }
                    }
                    None => drain_transport(t.as_mut(), &link, MAX_LINK_PACKETS_PER_PRESENT),
                }
                // After the drain, so a leaving peer's last packets still reach the core.
                if t.peer_ended() {
                    self.shared.peer_ended.store(true, Ordering::Relaxed);
                }
                if t.is_closed() {
                    self.shared.link_lost.store(true, Ordering::Relaxed);
                }
            }
            core.pump_link();
            // A `poll` can make the core send, so this catches anything it just queued. The
            // send that matters is the one after the frame runs, below.
            flush_outbound(&mut transport, &link);

            let speed = self.speed();
            // `Release` pairs with `observed_speed`'s `Acquire`, so a reader seeing `Paused`
            // also sees every earlier `publish`, whose counter is `Relaxed`.
            self.shared.observed.store(speed as u8, Ordering::Release);
            let ff_sound = self.shared.ff_sound.load(Ordering::Relaxed);
            // Fast forward without sound mutes the ring. Paused and rewinding mark it idle
            // instead: the insert click still mixes in, and that silence is not a starve.
            let rewinding = speed != Speed::Paused && self.shared.rewind.load(Ordering::Relaxed);
            let gate = (
                speed == Speed::Fast && !ff_sound,
                speed == Speed::Paused || rewinding,
            );
            if gate != gated {
                ring.set_muted(gate.0);
                ring.set_idle(gate.1);
                gated = gate;
            }
            let lock = self.shared.driven.load(Ordering::Relaxed)
                && speed == Speed::Normal
                && !rewinding
                && transport.is_none();
            if lock != self.shared.locked.load(Ordering::Relaxed) {
                self.shared.locked.store(lock, Ordering::Release);
                // Leaving the lock picks the worker's own clock up from now.
                deadline = Instant::now();
                self.shared.clocked.notify_all();
            }
            let input = ButtonMask(self.shared.input.load(Ordering::Relaxed));
            crate::latency::emu_frame(input.0);
            let ceiling = match speed {
                Speed::Paused => 0,
                // Locked, a frame runs only for a tick or in place of a missed one.
                Speed::Normal if lock && !ticked => 0,
                Speed::Normal => 1,
                Speed::Fast => self.shared.fast_steps.load(Ordering::Relaxed),
            };
            if rewinding {
                if let Some(state) = rewind.pop() {
                    if let Err(e) = core.unserialize(&state) {
                        eprintln!("slot: rewind: {e}");
                    }
                    // A core need not repaint from a load, so run one drawn frame. `pop` goes
                    // back two: 2x reverse. Empty input: the live mask holds L2.
                    core.set_frame_skip(false);
                    core.run_frame(ButtonMask(0));
                    self.publish(core.video_xrgb8888());
                }
                self.shared
                    .rewind_fill
                    .store(rewind.fill(), Ordering::Relaxed);
                // Reverse audio is noise, and the sink runs itself dry into silence.
                let _ = core.take_audio();
            } else if ceiling > 0 {
                // As many core frames as the present can afford, up to the ceiling; only the
                // last draws. "Last" is predicted before it runs, the only time a core can skip.
                // What is left of the present for core frames once what follows them is paid.
                let budget = FAST_TARGET.saturating_sub(post_cost);
                let began = Instant::now();
                let mut ran = 0u32;
                let mut worst = Duration::ZERO;
                loop {
                    ran += 1;
                    let last = ran >= ceiling || began.elapsed() + frame_peak * 2 > budget;
                    core.set_frame_skip(!last);
                    let frame_began = Instant::now();
                    match cable.as_mut() {
                        // One `run_frame_linked` steps both consoles, so wait for both masks.
                        // A stepped frame cannot be taken back, so a late peer stalls.
                        Some(c) => {
                            // Offer this frame's buttons before waiting. `sample` decides a
                            // frame once, so repeats are free.
                            if let Some(t) = transport.as_deref_mut() {
                                t.send(NETPACKET_RELIABLE, &c.sample(input));
                            }
                            // Wait within the present: the loops are out of phase, so giving
                            // up the present would halve the frame rate. Bounded by the budget.
                            let waited = Instant::now();
                            let ready = loop {
                                if let Some(pair) = c.ready() {
                                    break Some(pair);
                                }
                                if began.elapsed() + frame_peak > budget {
                                    break None;
                                }
                                if let Some(t) = transport.as_deref_mut() {
                                    while let Some(buf) = t.try_recv() {
                                        c.accept(&buf);
                                    }
                                }
                                std::thread::sleep(Duration::from_micros(250));
                            };
                            cable_wait += waited.elapsed();
                            match ready {
                                Some((p0, p1)) => {
                                    core.run_frame_linked(p0, p1);
                                    c.advance();
                                    self.shared.linked.fetch_add(1, Ordering::Relaxed);
                                }
                                None => {
                                    cable_stalls += 1;
                                    c.stall();
                                    self.shared.link_lost.store(
                                        c.stalled() >= cable::QUIET_FRAMES,
                                        Ordering::Relaxed,
                                    );
                                    break;
                                }
                            }
                        }
                        None => core.run_frame(input),
                    }
                    worst = worst.max(frame_began.elapsed());
                    if last {
                        break;
                    }
                }
                let core_time = began.elapsed();
                // Up at once, down slowly: the next present must survive a heavy frame.
                frame_peak = if worst > frame_peak {
                    worst
                } else {
                    blend(frame_peak, worst)
                };
                if cable.is_some() {
                    cable_presents += 1;
                    cable_core += core_time;
                    if cable_said.elapsed() >= Duration::from_secs(5) {
                        let secs = cable_said.elapsed().as_secs_f32();
                        eprintln!(
                            "slot: cable: {} presents, {} stalled, {:.1} fps, {:.1} ms core ({:.1} ms waiting) of {:.1} ms present",
                            cable_presents,
                            cable_stalls,
                            (cable_presents - cable_stalls) as f32 / secs,
                            cable_core.as_secs_f32() * 1000.0 / cable_presents as f32,
                            cable_wait.as_secs_f32() * 1000.0 / cable_presents as f32,
                            secs * 1000.0 / cable_presents as f32,
                        );
                        cable_presents = 0;
                        cable_stalls = 0;
                        cable_core = Duration::ZERO;
                        cable_wait = Duration::ZERO;
                        cable_said = Instant::now();
                    }
                }
                fast_span = Some((began, core_time));
                // Send now: serial only runs inside `run_frame`, and waiting a present adds
                // 16.7 ms to a ~2 ms wire. A GBA unanswered for four frames reports an error.
                flush_outbound(&mut transport, &link);
                self.publish(core.video_xrgb8888());
                // The display waits for this frame and nothing after it: the snapshot and the
                // audio below run while it draws.
                self.frame_done(served);

                // Per present, fast forward included, so rewind covers it. Skipped during a
                // link session: rewind is refused, and a two-console state is the costliest work.
                since_snapshot += 1;
                if cable.is_some() {
                    since_snapshot = 0;
                }
                if since_snapshot >= SNAPSHOT_EVERY {
                    since_snapshot = 0;
                    // A serialize failure was already reported by the save path.
                    if let Ok(state) = core.serialize() {
                        rewind.push(state);
                        self.shared
                            .rewind_fill
                            .store(rewind.fill(), Ordering::Relaxed);
                    }
                }

                let audio = core.take_audio();
                // Fast forward audio plays only with its sound on, squeezed into one present:
                // faster and higher.
                if speed == Speed::Normal || ff_sound {
                    let target = drc_target(ring.capacity_frames());
                    let queued = ring.queued_frames();
                    if lock && scale == 0.0 {
                        scale = self.shared.present_ns.load(Ordering::Relaxed) as f64
                            / PRESENT.as_nanos() as f64;
                    }
                    let base = if lock { scale } else { 1.0 };
                    resampler.set_ratio(drc_ratio(queued, target) * base / f64::from(ran));
                    resampler.process(&audio, &mut out);
                    crate::audio::volume::apply(
                        &mut out,
                        self.shared.volume.load(Ordering::Relaxed),
                    );
                    ring.push_blocking(&out);
                    // Occupancy against target is what a crackle report needs.
                    paced += 1;
                    if crate::session::trace() && paced.is_multiple_of(TRACE_EVERY) {
                        let (dropped, starved) = (ring.overruns(), ring.underruns());
                        eprintln!(
                            "slot: audio: {queued}/{target} queued, {dropped} dropped, {starved} starved, locked {lock} at {scale:.5}"
                        );
                    }
                }
            }

            // `push_blocking` is the backstop; this deadline paces everything else. Measure
            // trailing work before the sleep.
            if let Some((began, core_time)) = fast_span.take() {
                post_cost = blend(post_cost, began.elapsed().saturating_sub(core_time));
            }
            self.frame_done(served);
            if lock {
                // Once stalled, the worker keeps its own 60 Hz until a tick comes back.
                let patience = if stalled { PRESENT } else { STALL };
                match self.next_tick(served, patience) {
                    Some(tick) => {
                        served = tick;
                        stalled = false;
                        // A window of real ticks is the display's present; one with a stall in
                        // it is not, and is dropped.
                        window.1 += 1;
                        if window.1 == RATE_WINDOW {
                            if window.2 {
                                let present = window.0.elapsed() / RATE_WINDOW;
                                scale = present.as_secs_f64() / PRESENT.as_secs_f64();
                            }
                            window = (Instant::now(), 0, true);
                        }
                    }
                    // The display stalled (an autosave writing to the card, say): run this frame
                    // on the worker's own clock so the game and its audio carry on.
                    None => {
                        window.2 = false;
                        stalled = true;
                    }
                }
                ticked = true;
                continue;
            }
            ticked = false;
            deadline += PRESENT;
            let now = Instant::now();
            match deadline.checked_duration_since(now) {
                Some(wait) => std::thread::sleep(wait),
                // Falling behind by more than a frame means a stall, not a slow frame.
                // Catching up would sprint through frames nobody sees.
                None => deadline = now,
            }
        }
        // Drain once more after `stop`, while the core is alive. A lost `EndLink` reaches the
        // peer as a FIN rather than a goodbye; a lost `Save` loses the player's position.
        for cmd in self.cmds.try_iter() {
            self.apply(cmd, core.as_mut(), &mut transport, &mut cable, &link);
        }
        // The ring belongs to the session, so a cart that left while fast forwarding would
        // otherwise take every sound after it with it.
        ring.set_muted(false);
        ring.set_idle(false);
        let (dropped, starved) = (ring.overruns(), ring.underruns());
        if dropped > 0 || starved > 0 || crate::session::trace() {
            eprintln!("slot: audio: {dropped} samples dropped, {starved} starved");
        }
    }

    /// One command, against the core this worker is running. Also used by the shutdown drain.
    fn apply(
        &self,
        cmd: Cmd,
        core: &mut dyn RetroCore,
        transport: &mut Option<Box<dyn LinkChannel>>,
        cable: &mut Option<Cable>,
        link: &Link,
    ) {
        match cmd {
            Cmd::Save(reply) => match core.serialize() {
                Ok(state) => {
                    let _ = reply.send(state);
                }
                Err(e) => eprintln!("slot: {e}"),
            },
            Cmd::Load(state) => {
                if let Err(e) = core.unserialize(&state) {
                    eprintln!("slot: {e}");
                }
            }
            Cmd::Sav(reply) => {
                let _ = reply.send(core.save_ram());
            }
            Cmd::Thumb(reply) => {
                let _ = reply.send(crate::thumb::png(core.video_xrgb8888()));
            }
            Cmd::BeginLink(client_id, t) => {
                self.shared.link_lost.store(false, Ordering::Relaxed);
                self.shared.peer_ended.store(false, Ordering::Relaxed);
                // Clear before going live: a core with no `stop` (OPTIONAL in libretro) may keep
                // sending, and that would open the next session. Before `start_link` to keep a
                // handshake sent from `start`.
                link.clear();
                // Set here too: the mock starts no session of its own.
                core.start_link(client_id);
                link.set_active(true);
                *transport = Some(t);
            }
            Cmd::BeginCable(player, t) => {
                self.shared.link_lost.store(false, Ordering::Relaxed);
                self.shared.peer_ended.store(false, Ordering::Relaxed);
                // This route never uses libretro netpacket: no `start_link`, no `set_active`.
                let mut c = Cable::new(player);
                let mut wire = t;
                // Both devices simulate both consoles, so both start from the host's bytes,
                // sent before any frame runs.
                if player == 0 {
                    match core.serialize() {
                        Ok(state) => {
                            eprintln!("slot: cable: sending {} bytes of state", state.len());
                            for p in cable::state_packets(&state) {
                                wire.send(NETPACKET_RELIABLE, &p);
                            }
                        }
                        Err(e) => eprintln!("slot: cable: the core would not serialize: {e}"),
                    }
                    c.prime();
                }
                *cable = Some(c);
                *transport = Some(wire);
            }
            Cmd::SetOption(key, value) => {
                // The core re-reads options on its next `retro_run` via `GET_VARIABLE_UPDATE`.
                core.set_option(&key, &value);
            }
            Cmd::EndLink => {
                self.shared.link_lost.store(false, Ordering::Relaxed);
                self.shared.peer_ended.store(false, Ordering::Relaxed);
                *cable = None;
                // A no-op for a core with no `stop` callback (OPTIONAL in libretro).
                core.stop_link();
                // Goodbye before the wire goes, so the peer sees an ending, not silence. Every
                // ending passes here. Before the drop, which unblocks the transport's threads.
                if let Some(t) = transport.as_mut() {
                    t.send_end();
                }
                // Dropping closes the wire (see `TcpLink`'s `Drop`).
                *transport = None;
                // Clear before `set_active(false)`: its `Release` store makes the empty queues
                // visible to anyone who then reads `is_active() == false`.
                link.clear();
                link.set_active(false);
            }
        }
    }

    /// The frame for tick `served` is published, or there was none to run.
    fn frame_done(&self, served: u64) {
        let mut clock = self.shared.clock.lock().unwrap_or_else(|e| e.into_inner());
        if clock.done < served {
            clock.done = served;
            self.shared.clocked.notify_all();
        }
    }

    /// Waits for a display tick newer than `served`. `None` when none comes within `patience`, or
    /// the display lets go of the clock: the caller then runs the frame on its own. Two ticks that
    /// arrive before the worker gets to them are served by one frame.
    fn next_tick(&self, served: u64, patience: Duration) -> Option<u64> {
        let until = Instant::now() + patience;
        let mut clock = self.shared.clock.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if clock.tick > served {
                return Some(clock.tick);
            }
            let left = until.checked_duration_since(Instant::now())?;
            if self.shared.stop.load(Ordering::Relaxed)
                || !self.shared.driven.load(Ordering::Relaxed)
            {
                return None;
            }
            clock = match self.shared.clocked.wait_timeout(clock, left) {
                Ok((c, _)) => c,
                Err(e) => e.into_inner().0,
            };
        }
    }

    fn publish(&self, video: &[u8]) {
        let mut buf = self.frames.take_write();
        buf.clear();
        buf.extend_from_slice(video);
        self.frames.publish(buf);
        self.shared.published.fetch_add(1, Ordering::Relaxed);
        crate::latency::published();
    }

    fn speed(&self) -> Speed {
        Speed::from_u8(self.shared.speed.load(Ordering::Relaxed))
    }
}

/// Everything the core has queued for its peer, onto the wire. Always reliable: the
/// `netpacket_send` flag is not queued, and `LinkChannel::send` falls back to reliable anyway.
fn flush_outbound(transport: &mut Option<Box<dyn LinkChannel>>, link: &Link) {
    let Some(t) = transport.as_deref_mut() else {
        return;
    };
    while let Some(packet) = link.take_outbound() {
        t.send(NETPACKET_RELIABLE, &packet);
    }
}

/// Folds one present's measured per-frame cost into the running estimate, `COST_BLEND` being
/// how much of the old value the new measurement replaces.
fn blend(estimate: Duration, measured: Duration) -> Duration {
    (estimate * (COST_BLEND - 1) + measured) / COST_BLEND
}

/// Moves up to `cap` packets from `transport` into `link`'s inbound queue, leaving the rest
/// queued. Free so tests can drive the cap without a worker.
fn drain_transport(transport: &mut dyn LinkChannel, link: &Link, cap: u32) {
    for _ in 0..cap {
        let Some(packet) = transport.try_recv() else {
            break;
        };
        link.push_inbound(packet);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slot_retro::LoopbackLink;

    /// A flooding peer's packets stop at the cap and the rest stay queued.
    #[test]
    fn drain_transport_stops_at_the_cap_and_leaves_the_rest_queued() {
        let mut transport = LoopbackLink::default();
        for i in 0..10u8 {
            transport.send(0, &[i]);
        }
        let link = Link::default();

        drain_transport(&mut transport, &link, 4);

        let mut got = Vec::new();
        while let Some(p) = link.take_inbound() {
            got.push(p[0]);
        }
        assert_eq!(
            got,
            vec![0, 1, 2, 3],
            "the cap must stop the drain, not just slow it"
        );
        assert_eq!(
            transport.try_recv(),
            Some(vec![4]),
            "packets past the cap must stay queued in the transport, not be dropped"
        );
    }

    /// Under the cap, everything moves in one call.
    #[test]
    fn drain_transport_moves_everything_under_the_cap() {
        let mut transport = LoopbackLink::default();
        transport.send(0, b"one");
        transport.send(0, b"two");
        let link = Link::default();

        drain_transport(&mut transport, &link, MAX_LINK_PACKETS_PER_PRESENT);

        assert_eq!(link.take_inbound().as_deref(), Some(&b"one"[..]));
        assert_eq!(link.take_inbound().as_deref(), Some(&b"two"[..]));
        assert_eq!(link.take_inbound(), None);
    }
}
