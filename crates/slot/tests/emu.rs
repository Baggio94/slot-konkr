use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use slot::audio::{AudioSink, StubSink};
use slot::emu::{CoreState, EmuHandle, Speed, FAST_STEPS, FAST_STEPS_MAX};
use slot::persist::Snapshot;
use slot_retro::{
    AvInfo, ButtonMask, CoreError, LinkChannel, MockCore, RetroCore, NETPACKET_RELIABLE,
};

fn spawn() -> EmuHandle {
    spawn_with(None)
}

fn spawn_with(sav: Option<Vec<u8>>) -> EmuHandle {
    spawn_into(StubSink::new(), sav)
}

/// Stands in for the device, which always has room: the worker waits on a sink nothing drains.
fn drain(sink: StubSink) {
    std::thread::spawn(move || loop {
        sink.device_drain();
        std::thread::sleep(Duration::from_millis(2));
    });
}

fn spawn_into(mut sink: StubSink, sav: Option<Vec<u8>>) -> EmuHandle {
    sink.open(32_768).expect("the stub refused to open");
    drain(sink.clone());
    let emu = EmuHandle::spawn(
        Box::new(MockCore::new()),
        PathBuf::from("mock"),
        sink.ring(),
        sav,
        None,
    );
    // A worker starts paused, so boot frames never run behind the cart during an insert.
    emu.set_speed(Speed::Normal);
    assert!(
        wait_for(|| emu.state() != CoreState::Loading),
        "the core never finished loading"
    );
    assert_eq!(emu.state(), CoreState::Ready);
    emu
}

fn wait_for(cond: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if cond() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// MockCore serializes to its frame counter, so a state doubles as "how far has it run".
fn frame_count(emu: &EmuHandle) -> u64 {
    let bytes = emu
        .request_state()
        .recv_timeout(Duration::from_secs(2))
        .expect("the worker never answered a state request");
    u64::from_le_bytes(bytes.try_into().expect("mock state is a u64 counter"))
}

#[test]
fn the_worker_runs_until_it_is_paused_and_resumes_where_it_stopped() {
    let emu = spawn();
    let early = frame_count(&emu);
    std::thread::sleep(Duration::from_millis(200));
    let later = frame_count(&emu);
    assert!(
        later > early,
        "the core is not stepping: {early} then {later}"
    );

    emu.set_speed(Speed::Paused);
    std::thread::sleep(Duration::from_millis(100));
    let held = frame_count(&emu);
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(frame_count(&emu), held, "a paused core kept stepping");

    emu.set_speed(Speed::Normal);
    std::thread::sleep(Duration::from_millis(200));
    assert!(frame_count(&emu) > held, "the core did not resume");
}

#[test]
fn a_requested_load_rewinds_the_core() {
    let emu = spawn();
    let state = emu
        .request_state()
        .recv_timeout(Duration::from_secs(2))
        .expect("the worker never answered a state request");
    let at_save = u64::from_le_bytes(state.clone().try_into().expect("mock state is a counter"));

    std::thread::sleep(Duration::from_millis(200));
    assert!(frame_count(&emu) > at_save);

    emu.set_speed(Speed::Paused);
    emu.request_load(state);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(frame_count(&emu), at_save);
}

/// Letting go of a rewind plays on from where it landed, not from where it started.
#[test]
fn rewinding_walks_the_core_backwards_and_then_plays_on() {
    let emu = spawn();
    std::thread::sleep(Duration::from_millis(300));
    let played = frame_count(&emu);

    emu.set_rewinding(true);
    std::thread::sleep(Duration::from_millis(200));
    emu.set_rewinding(false);
    let rewound = frame_count(&emu);
    assert!(
        rewound < played,
        "the core did not rewind: {played} then {rewound}"
    );

    std::thread::sleep(Duration::from_millis(200));
    assert!(
        frame_count(&emu) > rewound,
        "the core did not resume after the rewind"
    );
}

/// Fast forward leaves up to four frames of stale audio in the ring, so it mutes.
#[test]
fn fast_forward_mutes_and_normal_speed_unmutes() {
    let sink = StubSink::new();
    let emu = spawn_into(sink.clone(), None);
    emu.set_speed(Speed::Fast);
    assert!(
        wait_for(|| sink.muted()),
        "fast forward did not mute the sink"
    );
    emu.set_speed(Speed::Normal);
    assert!(
        wait_for(|| !sink.muted()),
        "the sink stayed muted at normal speed"
    );
}

/// The device outlives the cart, so a worker stopped while muted must not leave it muted.
#[test]
fn a_worker_that_stops_while_muted_leaves_the_sink_audible() {
    let sink = StubSink::new();
    let emu = spawn_into(sink.clone(), None);
    emu.set_speed(Speed::Fast);
    assert!(wait_for(|| sink.muted()), "fast forward did not mute");
    drop(emu);
    assert!(!sink.muted(), "the sink stayed muted after the cart left");
}

/// `FAST_STEPS` core frames per present is the default speed.
#[test]
fn fast_forward_runs_fast_steps_core_frames_per_present() {
    let emu = spawn();
    let from = frame_count(&emu);
    std::thread::sleep(Duration::from_millis(300));
    let normal = frame_count(&emu) - from;

    emu.set_speed(Speed::Fast);
    let from = frame_count(&emu);
    std::thread::sleep(Duration::from_millis(300));
    let fast = frame_count(&emu) - from;

    let n = normal as u32;
    assert!(
        fast as u32 >= n * (FAST_STEPS - 1) && fast as u32 <= n * (FAST_STEPS + 1),
        "{fast} frames fast against {normal} normal is not {FAST_STEPS}x"
    );
}

/// Core frames run and presents published, read with the core held so neither moves between.
fn held_counts(emu: &EmuHandle) -> (u64, u64) {
    emu.set_speed(Speed::Paused);
    assert!(
        wait_for(|| emu.observed_speed() == Speed::Paused),
        "the worker never saw the pause"
    );
    (frame_count(emu), emu.published_count())
}

/// The quick menu's speed caps core frames per present, never above `FAST_STEPS_MAX`.
/// Counted against presents, not time, so a loaded machine cannot skew it.
#[test]
fn fast_forward_runs_the_chosen_number_of_core_frames_per_present() {
    let emu = spawn();
    // Normal play first: the per-frame estimate starts at a whole present and needs normal
    // presents to walk it down, or this would measure that climb rather than the ceiling.
    std::thread::sleep(Duration::from_millis(300));
    for (asked, runs) in [
        (2, 2),
        (3, 3),
        (4, 4),
        (6, 6),
        (FAST_STEPS_MAX, FAST_STEPS_MAX),
        (FAST_STEPS_MAX + 4, FAST_STEPS_MAX),
    ] {
        emu.set_fast_steps(asked);
        let (frames, presents) = held_counts(&emu);
        emu.set_speed(Speed::Fast);
        std::thread::sleep(Duration::from_millis(150));
        let (frames_after, presents_after) = held_counts(&emu);
        let (ran, shown) = (frames_after - frames, presents_after - presents);
        assert!(shown > 0, "nothing was presented asking for {asked}");
        let want = shown * u64::from(runs);
        // The ceiling is a hard cap, so going over it is always wrong.
        assert!(
            ran <= want,
            "{ran} frames over {shown} presents ran past the ceiling of {runs}, asking for {asked}"
        );
        // Under it is the suite descheduling the worker. Allow a present or two of that.
        assert!(
            ran * 5 >= want * 4,
            "{ran} frames over {shown} presents is well short of the ceiling of {runs}, asking \
             for {asked}: the ceiling is not what bound"
        );
    }
}

/// A `MockCore` that spends `cost` of wall clock per frame and records what it was told about
/// drawing before each one.
struct Probe {
    inner: MockCore,
    cost: Duration,
    skip: bool,
    log: Arc<Mutex<Vec<bool>>>,
}

impl Probe {
    fn new(cost: Duration) -> (Box<Probe>, Arc<Mutex<Vec<bool>>>) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let probe = Box::new(Probe {
            inner: MockCore::new(),
            cost,
            skip: false,
            log: log.clone(),
        });
        (probe, log)
    }
}

impl RetroCore for Probe {
    fn load(&mut self, rom: &Path) -> Result<(), CoreError> {
        self.inner.load(rom)
    }
    fn run_frame(&mut self, input: ButtonMask) {
        self.log.lock().expect("the skip log").push(self.skip);
        if !self.cost.is_zero() {
            std::thread::sleep(self.cost);
        }
        self.inner.run_frame(input);
    }
    fn set_frame_skip(&mut self, skip: bool) {
        self.skip = skip;
        self.inner.set_frame_skip(skip);
    }
    fn video_xrgb8888(&self) -> &[u8] {
        self.inner.video_xrgb8888()
    }
    fn take_audio(&mut self) -> Vec<i16> {
        self.inner.take_audio()
    }
    fn serialize(&mut self) -> Result<Vec<u8>, CoreError> {
        self.inner.serialize()
    }
    fn unserialize(&mut self, data: &[u8]) -> Result<(), CoreError> {
        self.inner.unserialize(data)
    }
    fn save_ram(&self) -> Option<Vec<u8>> {
        self.inner.save_ram()
    }
    fn load_save_ram(&mut self, data: &[u8]) -> Result<(), CoreError> {
        self.inner.load_save_ram(data)
    }
    fn av_info(&self) -> AvInfo {
        self.inner.av_info()
    }
}

fn spawn_probe(cost: Duration) -> (EmuHandle, Arc<Mutex<Vec<bool>>>) {
    let mut sink = StubSink::new();
    sink.open(32_768).expect("the stub refused to open");
    drain(sink.clone());
    let (core, log) = Probe::new(cost);
    let emu = EmuHandle::spawn(core, PathBuf::from("mock"), sink.ring(), None, None);
    assert!(
        wait_for(|| emu.state() == CoreState::Ready),
        "the core never finished loading"
    );
    (emu, log)
}

/// The picture on screen is the frame the core last ran, at every speed. `MockCore` paints a
/// pattern from its frame counter and leaves the buffer alone on a skipped frame, as real cores
/// do, so a reference core wound to the same count gives the exact expected bytes.
#[test]
fn every_speed_publishes_the_frame_the_core_last_ran() {
    let emu = spawn();
    for ceiling in [2, 3, 4, FAST_STEPS_MAX] {
        emu.set_fast_steps(ceiling);
        emu.set_speed(Speed::Fast);
        std::thread::sleep(Duration::from_millis(120));
        // Held, so the count and the picture cannot move between the two reads.
        let (frames, _) = held_counts(&emu);
        let shown = emu.latest_frame().expect("nothing was ever published");

        let mut reference = MockCore::new();
        reference
            .unserialize(&frames.to_le_bytes())
            .expect("the reference core refused the frame count");
        assert_eq!(
            &shown[..],
            reference.video_xrgb8888(),
            "at a ceiling of {ceiling} the picture shown is not frame {frames}"
        );
    }
}

/// Every frame of a present but its last is told to skip: the only change measured to move the
/// speed cap.
#[test]
fn a_fast_present_draws_only_its_last_frame() {
    let ceiling = 3;
    let (emu, log) = spawn_probe(Duration::ZERO);
    emu.set_fast_steps(ceiling);
    emu.set_speed(Speed::Fast);
    // Let the speed settle so the log holds whole fast presents.
    std::thread::sleep(Duration::from_millis(60));
    log.lock().expect("the skip log").clear();
    std::thread::sleep(Duration::from_millis(150));
    let (_, presents) = held_counts(&emu);
    assert!(presents > 0, "nothing was presented");

    let got = log.lock().expect("the skip log").clone();
    assert!(!got.is_empty(), "no core frames ran");
    let mut drawn = 0;
    let mut skipped_in_a_row = 0;
    for (i, skipped) in got.iter().enumerate() {
        if *skipped {
            skipped_in_a_row += 1;
            assert!(
                skipped_in_a_row < ceiling,
                "frame {i} is the {skipped_in_a_row}th skipped in a row, so a present ran more \
                 than its ceiling of {ceiling} or never drew"
            );
        } else {
            drawn += 1;
            skipped_in_a_row = 0;
        }
    }
    assert!(
        drawn > 1,
        "only {drawn} frames were drawn over {presents} presents"
    );
}

/// The speed is a ceiling: a present runs as many core frames as it can afford, so a heavy
/// game slows instead of dropping off 60 Hz.
#[test]
fn a_present_runs_what_it_can_afford_rather_than_the_whole_ceiling() {
    let (emu, _log) = spawn_probe(Duration::from_millis(5));
    emu.set_fast_steps(FAST_STEPS_MAX);
    let (frames, presents) = held_counts(&emu);
    emu.set_speed(Speed::Fast);
    // Long enough for the per-frame estimate to settle from its pessimistic seed.
    std::thread::sleep(Duration::from_millis(400));
    let (frames_after, presents_after) = held_counts(&emu);

    let (ran, shown) = (frames_after - frames, presents_after - presents);
    assert!(shown > 0, "nothing was presented");
    let per = ran as f64 / shown as f64;
    assert!(
        per >= 1.0,
        "{per:.1} frames a present is less than one, so a present ran nothing"
    );
    assert!(
        per <= 4.0,
        "{per:.1} frames a present at 5 ms each is {:.0} ms of a 16.67 ms present: the budget \
         never stopped it and the ceiling of {FAST_STEPS_MAX} did",
        per * 5.0
    );
}

/// A device that counts the frames it took, draining everything so the worker never waits.
fn counting(sink: StubSink) -> Arc<AtomicUsize> {
    let heard = Arc::new(AtomicUsize::new(0));
    let tally = heard.clone();
    std::thread::spawn(move || loop {
        tally.fetch_add(sink.device_drain(), Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(2));
    });
    heard
}

/// A worker left paused, as a real one starts, over a device that counts what it hears.
fn spawn_heard() -> (EmuHandle, StubSink, Arc<AtomicUsize>) {
    let mut sink = StubSink::new();
    sink.open(32_768).expect("the stub refused to open");
    let heard = counting(sink.clone());
    let emu = EmuHandle::spawn(
        Box::new(MockCore::new()),
        PathBuf::from("mock"),
        sink.ring(),
        None,
        None,
    );
    assert!(
        wait_for(|| emu.state() == CoreState::Ready),
        "the core never finished loading"
    );
    (emu, sink, heard)
}

/// Frames heard per present over fast forward. The core is held and the ring run dry either
/// side, so nothing queued outside the stretch counts.
fn heard_per_present(emu: &EmuHandle, sink: &StubSink, heard: &AtomicUsize) -> f64 {
    let settle = || {
        let (_, presents) = held_counts(emu);
        assert!(
            wait_for(|| sink.ring().queued_frames() == 0),
            "the device never drained the ring"
        );
        // The drain empties the ring a moment before it adds up what it took.
        std::thread::sleep(Duration::from_millis(20));
        (presents, heard.load(Ordering::Relaxed))
    };
    let (presents, before) = settle();
    emu.set_speed(Speed::Fast);
    std::thread::sleep(Duration::from_millis(300));
    let (presents_after, after) = settle();
    (after - before) as f64 / (presents_after - presents) as f64
}

/// With its sound off, fast forward audio never reaches the device and the ring is muted.
#[test]
fn fast_forward_is_silent_while_its_sound_is_off() {
    let (emu, sink, heard) = spawn_heard();
    emu.set_ff_sound(false);
    emu.set_speed(Speed::Fast);
    assert!(wait_for(|| sink.muted()), "fast forward did not mute");
    let per = heard_per_present(&emu, &sink, &heard);
    assert_eq!(per, 0.0, "{per:.0} frames a present reached the device");
}

/// With its sound on, fast forward is resampled into real time: the device takes normal-speed
/// amounts, not several times it.
#[test]
fn fast_forward_sound_plays_sped_up_in_real_time() {
    let (emu, sink, heard) = spawn_heard();
    emu.set_ff_sound(true);
    let real_time = 32_768.0 / 60.0;
    for steps in [2, 4] {
        emu.set_fast_steps(steps);
        emu.set_speed(Speed::Fast);
        std::thread::sleep(Duration::from_millis(50));
        assert!(!sink.muted(), "fast forward muted with its sound on");
        let per = heard_per_present(&emu, &sink, &heard);
        assert!(
            (per / real_time - 1.0).abs() < 0.05,
            "{per:.0} frames a present at {steps}x, against {real_time:.0} in real time"
        );
    }
}

/// The battery save must reach the core after the rom loads (no save ram before), unchanged.
#[test]
fn battery_save_ram_reaches_the_core_and_comes_back() {
    let mut sav = vec![0u8; 8 * 1024];
    sav[7] = 0xab;
    let emu = spawn_with(Some(sav.clone()));
    let got = emu
        .snapshot()
        .save_ram()
        .expect("the core reported no save ram");
    assert_eq!(got, sav);
}

/// The captured shot is the core's own frame: the LCD mask is applied when the switcher draws.
#[test]
fn a_captured_thumbnail_is_the_unfiltered_core_frame() {
    let emu = spawn();
    assert!(wait_for(|| emu.has_published()), "no frame was published");
    // Paused, so the frame waiting for the renderer is also the one the core is sitting on.
    emu.set_speed(Speed::Paused);
    std::thread::sleep(Duration::from_millis(100));
    let frame = emu.latest_frame().expect("the published frame went away");
    let png = emu
        .snapshot()
        .thumb()
        .expect("the snapshot carried no thumbnail");

    let mut reader = png::Decoder::new(png.as_slice())
        .read_info()
        .expect("read info");
    let mut buf = vec![0u8; reader.output_buffer_size()];
    reader.next_frame(&mut buf).expect("decode");

    // The core's frame is XRGB8888 little endian, so its bytes are B, G, R, X.
    for (i, px) in frame.chunks_exact(4).enumerate() {
        assert_eq!(
            &buf[i * 3..i * 3 + 3],
            &[px[2], px[1], px[0]],
            "thumbnail pixel {i} is not the frame's"
        );
    }
}

#[test]
fn the_renderer_is_handed_whole_gba_frames() {
    let emu = spawn();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(frame) = emu.latest_frame() {
            assert_eq!(frame.len(), 240 * 160 * 4);
            return;
        }
        assert!(Instant::now() < deadline, "no frame was published in 2s");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Fast forward snapshots too, so a rewind walks back through it instead of skipping it all.
#[test]
fn fast_forward_still_records_rewind_history() {
    let emu = spawn();
    std::thread::sleep(Duration::from_millis(200));
    let before_ff = frame_count(&emu);

    emu.set_speed(Speed::Fast);
    std::thread::sleep(Duration::from_millis(300));
    emu.set_speed(Speed::Normal);
    let after_ff = frame_count(&emu);
    assert!(
        after_ff > before_ff,
        "fast forward did not advance the core: {before_ff} then {after_ff}"
    );

    // A brief rewind should walk a little way into the fast stretch, not past it.
    emu.set_rewinding(true);
    std::thread::sleep(Duration::from_millis(60));
    emu.set_rewinding(false);
    let rewound = frame_count(&emu);

    assert!(
        rewound > before_ff,
        "a short rewind fell back past where the fast forward began, so nothing was \
         recorded while it ran: {before_ff} -> {after_ff} -> {rewound}"
    );
}

// --- the link pump --------------------------------------------------------------------
//
// `MockCore` never registers netpacket, so `pump_link` is a no-op here and any packet moved is
// the worker's own loop doing it.

/// Two `LinkChannel`s wired to each other, standing in for a peer. `LoopbackLink` cannot do
/// this: it echoes a send back to the same handle.
struct PairedLink {
    tx: mpsc::Sender<Vec<u8>>,
    rx: mpsc::Receiver<Vec<u8>>,
}

fn paired_links() -> (PairedLink, PairedLink) {
    let (tx_a, rx_b) = mpsc::channel();
    let (tx_b, rx_a) = mpsc::channel();
    (
        PairedLink { tx: tx_a, rx: rx_a },
        PairedLink { tx: tx_b, rx: rx_b },
    )
}

impl LinkChannel for PairedLink {
    fn send(&mut self, _flags: i32, buf: &[u8]) {
        let _ = self.tx.send(buf.to_vec());
    }

    fn try_recv(&mut self) -> Option<Vec<u8>> {
        self.rx.try_recv().ok()
    }
}

fn wait_for_packet(mut poll: impl FnMut() -> Option<Vec<u8>>) -> Option<Vec<u8>> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(p) = poll() {
            return Some(p);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Inbound: whatever the transport returns reaches `push_inbound` every present.
#[test]
fn a_transport_packet_reaches_the_cores_inbound_queue() {
    let emu = spawn();
    let (mut here, there) = paired_links();
    emu.begin_link(0, Box::new(there));
    assert!(
        wait_for(|| emu.net().is_active()),
        "begin_link must mark the session active"
    );

    here.send(NETPACKET_RELIABLE, b"from the peer");
    let got = wait_for_packet(|| emu.net().take_inbound());
    assert_eq!(got.as_deref(), Some(&b"from the peer"[..]));
}

/// Outbound: whatever lands in `Link`'s outbound queue reaches the transport.
#[test]
fn the_cores_outbound_queue_reaches_the_transport() {
    let emu = spawn();
    let (mut here, there) = paired_links();
    emu.begin_link(1, Box::new(there));
    // `Cmd::BeginLink` empties both queues, so a packet pushed before it is live would be
    // cleared rather than pumped.
    assert!(wait_for(|| emu.net().is_active()), "begin_link never took");

    emu.net().push_outbound(b"from the core".to_vec());
    let got = wait_for_packet(|| here.try_recv());
    assert_eq!(got.as_deref(), Some(&b"from the core"[..]));
}

/// `end_link` drops the transport and marks the session inactive, so nothing reaches the peer
/// after it.
#[test]
fn end_link_marks_the_session_inactive() {
    let emu = spawn();
    let (_here, there) = paired_links();
    emu.begin_link(0, Box::new(there));
    assert!(wait_for(|| emu.net().is_active()), "begin_link never took");

    emu.end_link();
    assert!(
        wait_for(|| !emu.net().is_active()),
        "end_link must mark the session no longer active"
    );
}

/// Counts the worker's calls to `start_link`, `pump_link` and `stop_link`, which are otherwise
/// invisible: `MockCore` overrides none of them.
struct SpyLinkCore {
    inner: MockCore,
    start_calls: Arc<Mutex<Vec<u16>>>,
    pump_calls: Arc<AtomicUsize>,
    stop_calls: Arc<AtomicUsize>,
}

impl Default for SpyLinkCore {
    fn default() -> Self {
        SpyLinkCore {
            inner: MockCore::new(),
            start_calls: Arc::new(Mutex::new(Vec::new())),
            pump_calls: Arc::new(AtomicUsize::new(0)),
            stop_calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl RetroCore for SpyLinkCore {
    fn load(&mut self, rom: &Path) -> Result<(), CoreError> {
        self.inner.load(rom)
    }
    fn run_frame(&mut self, input: ButtonMask) {
        self.inner.run_frame(input)
    }
    fn video_xrgb8888(&self) -> &[u8] {
        self.inner.video_xrgb8888()
    }
    fn take_audio(&mut self) -> Vec<i16> {
        self.inner.take_audio()
    }
    fn serialize(&mut self) -> Result<Vec<u8>, CoreError> {
        self.inner.serialize()
    }
    fn unserialize(&mut self, data: &[u8]) -> Result<(), CoreError> {
        self.inner.unserialize(data)
    }
    fn save_ram(&self) -> Option<Vec<u8>> {
        self.inner.save_ram()
    }
    fn load_save_ram(&mut self, data: &[u8]) -> Result<(), CoreError> {
        self.inner.load_save_ram(data)
    }
    fn av_info(&self) -> AvInfo {
        self.inner.av_info()
    }
    fn start_link(&mut self, client_id: u16) {
        self.start_calls.lock().unwrap().push(client_id);
    }
    fn pump_link(&mut self) {
        self.pump_calls.fetch_add(1, Ordering::Relaxed);
    }
    fn stop_link(&mut self) {
        self.stop_calls.fetch_add(1, Ordering::Relaxed);
    }
}

fn spawn_with_core(core: Box<dyn RetroCore>) -> EmuHandle {
    let mut sink = StubSink::new();
    sink.open(32_768).expect("the stub refused to open");
    drain(sink.clone());
    let emu = EmuHandle::spawn(core, PathBuf::from("mock"), sink.ring(), None, None);
    emu.set_speed(Speed::Normal);
    assert!(
        wait_for(|| emu.state() != CoreState::Loading),
        "the core never finished loading"
    );
    emu
}

/// `Cmd::EndLink` must reach the core's `stop_link`, or the core keeps producing packets.
#[test]
fn ending_a_link_tells_the_cores_own_stop_link() {
    let core = SpyLinkCore::default();
    let stop_calls = core.stop_calls.clone();
    let emu = spawn_with_core(Box::new(core));
    let (_here, there) = paired_links();
    emu.begin_link(0, Box::new(there));
    assert!(wait_for(|| emu.net().is_active()), "begin_link never took");

    emu.end_link();
    assert!(
        wait_for(|| stop_calls.load(Ordering::Relaxed) > 0),
        "end_link must call the core's own stop_link"
    );
}

/// `Cmd::BeginLink` must call the core's `start_link` with the `client_id` it was given.
#[test]
fn beginning_a_link_calls_the_cores_own_start_link() {
    let core = SpyLinkCore::default();
    let start_calls = core.start_calls.clone();
    let emu = spawn_with_core(Box::new(core));
    let (_here, there) = paired_links();

    emu.begin_link(1, Box::new(there));
    assert!(
        wait_for(|| !start_calls.lock().unwrap().is_empty()),
        "Cmd::BeginLink must call the core's own start_link"
    );
    assert_eq!(start_calls.lock().unwrap().as_slice(), &[1]);
}

/// The worker calls `core.pump_link()` every present, even with no session wired up.
#[test]
fn the_worker_pumps_the_core_every_present() {
    let core = SpyLinkCore::default();
    let pump_calls = core.pump_calls.clone();
    let emu = spawn_with_core(Box::new(core));

    assert!(
        wait_for(|| pump_calls.load(Ordering::Relaxed) > 3),
        "the worker must call core.pump_link() every present"
    );
    drop(emu);
}

/// `Cmd::EndLink` must drop the transport, not merely flip the active flag.
#[test]
fn end_link_drops_the_transport_not_just_marks_it_inactive() {
    struct DropSignal {
        inner: PairedLink,
        dropped: Arc<AtomicBool>,
    }
    impl LinkChannel for DropSignal {
        fn send(&mut self, flags: i32, buf: &[u8]) {
            self.inner.send(flags, buf)
        }
        fn try_recv(&mut self) -> Option<Vec<u8>> {
            self.inner.try_recv()
        }
    }
    impl Drop for DropSignal {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::Relaxed);
        }
    }

    let emu = spawn();
    let (_here, there) = paired_links();
    let dropped = Arc::new(AtomicBool::new(false));
    let transport = DropSignal {
        inner: there,
        dropped: dropped.clone(),
    };
    emu.begin_link(0, Box::new(transport));
    assert!(wait_for(|| emu.net().is_active()), "begin_link never took");

    emu.end_link();
    assert!(
        wait_for(|| dropped.load(Ordering::Relaxed)),
        "Cmd::EndLink must drop the transport, not merely mark the session inactive"
    );
}

/// A packet that arrived just before a session ended must not reach the next one.
#[test]
fn ending_a_link_clears_stale_packets_for_the_next_session() {
    let emu = spawn();
    let (_here, there) = paired_links();
    emu.begin_link(0, Box::new(there));
    assert!(wait_for(|| emu.net().is_active()), "begin_link never took");

    emu.net()
        .push_inbound(b"stale from the old session".to_vec());

    emu.end_link();
    // `Cmd::EndLink` clears the queues before flipping `is_active` (Release/Acquire), so
    // seeing the flag fall guarantees the clear happened.
    assert!(wait_for(|| !emu.net().is_active()), "end_link never took");

    assert!(
        emu.net().take_inbound().is_none(),
        "a packet queued before the session ended survived into the next one"
    );
}

/// A transport whose far end the test can close.
struct ClosingLink {
    closed: Arc<AtomicBool>,
}

impl LinkChannel for ClosingLink {
    fn send(&mut self, _flags: i32, _buf: &[u8]) {}
    fn try_recv(&mut self) -> Option<Vec<u8>> {
        None
    }
    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
}

#[test]
fn a_transport_that_closes_is_reported_as_a_lost_link() {
    let emu = spawn();
    let closed = Arc::new(AtomicBool::new(false));
    emu.begin_link(
        0,
        Box::new(ClosingLink {
            closed: closed.clone(),
        }),
    );
    assert!(wait_for(|| emu.net().is_active()));
    assert!(!emu.link_lost(), "lost before the far end went");
    closed.store(true, Ordering::SeqCst);
    assert!(
        wait_for(|| emu.link_lost()),
        "the closed transport was never noticed"
    );
}

#[test]
fn ending_or_beginning_a_link_clears_the_lost_flag() {
    let emu = spawn();
    let closed = Arc::new(AtomicBool::new(true));
    emu.begin_link(0, Box::new(ClosingLink { closed }));
    assert!(wait_for(|| emu.link_lost()));
    emu.end_link();
    assert!(
        wait_for(|| !emu.link_lost()),
        "the flag outlived the session"
    );
    emu.begin_link(
        0,
        Box::new(ClosingLink {
            closed: Arc::new(AtomicBool::new(false)),
        }),
    );
    assert!(wait_for(|| emu.net().is_active()));
    assert!(!emu.link_lost(), "a new session started already lost");
}

/// The goodbye goes out before the transport is dropped; dropping first loses it and leaves
/// the peer to infer the ending from a FIN. Every ending passes through `Cmd::EndLink`.
#[test]
fn ending_a_link_says_goodbye_before_it_drops_the_transport() {
    struct Bye {
        order: Arc<Mutex<Vec<&'static str>>>,
    }
    impl LinkChannel for Bye {
        fn send(&mut self, _flags: i32, _buf: &[u8]) {}
        fn try_recv(&mut self) -> Option<Vec<u8>> {
            None
        }
        fn send_end(&mut self) {
            self.order.lock().expect("order").push("bye");
        }
    }
    impl Drop for Bye {
        fn drop(&mut self) {
            self.order.lock().expect("order").push("drop");
        }
    }

    let emu = spawn();
    let order = Arc::new(Mutex::new(Vec::new()));
    emu.begin_link(
        0,
        Box::new(Bye {
            order: order.clone(),
        }),
    );
    assert!(wait_for(|| emu.net().is_active()), "begin_link never took");

    emu.end_link();
    assert!(
        wait_for(|| order.lock().expect("order").len() == 2),
        "the transport was never told, or never dropped"
    );
    assert_eq!(
        *order.lock().expect("order"),
        vec!["bye", "drop"],
        "the wire was dropped before the peer was told the link had ended"
    );
}

/// A deliberate ending from the peer is believed, and the flag clears when a session begins or
/// ends, like `link_lost`.
#[test]
fn a_transport_whose_peer_ended_is_reported_as_ended_not_merely_lost() {
    struct EndedLink {
        ended: Arc<AtomicBool>,
    }
    impl LinkChannel for EndedLink {
        fn send(&mut self, _flags: i32, _buf: &[u8]) {}
        fn try_recv(&mut self) -> Option<Vec<u8>> {
            None
        }
        fn peer_ended(&self) -> bool {
            self.ended.load(Ordering::SeqCst)
        }
    }

    let emu = spawn();
    let ended = Arc::new(AtomicBool::new(false));
    emu.begin_link(
        0,
        Box::new(EndedLink {
            ended: ended.clone(),
        }),
    );
    assert!(wait_for(|| emu.net().is_active()), "begin_link never took");
    assert!(!emu.peer_ended(), "ended before the peer said anything");

    ended.store(true, Ordering::SeqCst);
    assert!(
        wait_for(|| emu.peer_ended()),
        "the peer's ending was never noticed"
    );

    emu.end_link();
    assert!(
        wait_for(|| !emu.peer_ended()),
        "the flag outlived the session it belonged to"
    );
}

/// A packet the core produced between two sessions must not open the next one.
/// `netpacket_send` queues regardless of session and `stop` is optional in libretro, so
/// `Cmd::BeginLink` must clear the queues too.
#[test]
fn beginning_a_link_clears_what_the_last_session_left_behind() {
    let emu = spawn();
    let (_here, there) = paired_links();
    emu.begin_link(0, Box::new(there));
    assert!(wait_for(|| emu.net().is_active()), "begin_link never took");

    emu.end_link();
    assert!(wait_for(|| !emu.net().is_active()), "end_link never took");

    // Sent before the command below on the same thread, so it is queued when the session begins.
    emu.net()
        .push_outbound(b"the core did not hear the session end".to_vec());
    emu.net().push_inbound(b"and neither did this".to_vec());

    let (mut here, there) = paired_links();
    emu.begin_link(1, Box::new(there));
    assert!(
        wait_for(|| emu.net().is_active()),
        "the second begin_link never took"
    );
    // Several presents, so anything the worker was going to flush has had every chance to.
    std::thread::sleep(Duration::from_millis(100));

    assert_eq!(
        here.try_recv(),
        None,
        "the last session's traffic opened this one, to a peer that never asked for it"
    );
    assert_eq!(
        emu.net().take_inbound(),
        None,
        "a packet from before this session was waiting for its core"
    );
}

/// An ending asked for in the same present the cart leaves in still reaches the wire.
/// `stop` is only read at the top of the loop, so a dropped handle must not lose queued
/// commands. The core holds the worker in one 400 ms `run_frame` to force the interleaving.
#[test]
fn an_ending_asked_for_in_the_last_present_still_reaches_the_peer() {
    struct Bye {
        order: Arc<Mutex<Vec<&'static str>>>,
    }
    impl LinkChannel for Bye {
        fn send(&mut self, _flags: i32, _buf: &[u8]) {}
        fn try_recv(&mut self) -> Option<Vec<u8>> {
            None
        }
        fn send_end(&mut self) {
            self.order.lock().expect("order").push("bye");
        }
    }
    impl Drop for Bye {
        fn drop(&mut self) {
            self.order.lock().expect("order").push("drop");
        }
    }

    let (emu, _log) = spawn_probe(Duration::from_millis(400));
    emu.set_speed(Speed::Normal);
    let order = Arc::new(Mutex::new(Vec::new()));
    emu.begin_link(
        0,
        Box::new(Bye {
            order: order.clone(),
        }),
    );
    // Observed at the top of a present whose frame holds the worker for the next 400 ms.
    assert!(wait_for(|| emu.net().is_active()), "begin_link never took");

    emu.end_link();
    drop(emu);

    assert_eq!(
        *order.lock().expect("order"),
        vec!["bye", "drop"],
        "the cart leaving took the goodbye with it: the peer was never told the link ended"
    );
}

/// A flush queued in the present the cart leaves in is still answered, or the position is lost.
#[test]
fn a_state_asked_for_in_the_last_present_is_still_answered() {
    let (emu, _log) = spawn_probe(Duration::from_millis(400));
    emu.set_speed(Speed::Normal);
    // Far enough in that the worker is inside a frame rather than at the top of its loop.
    std::thread::sleep(Duration::from_millis(100));

    let state = emu.request_state();
    drop(emu);

    assert!(
        state.recv_timeout(Duration::from_secs(2)).is_ok(),
        "a flush racing the cart out of the slot was never answered"
    );
}
