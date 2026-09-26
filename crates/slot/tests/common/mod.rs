#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use slot::app::App;
use slot::persist::Snapshot;
use slot::session::Session;
use slot_power::{Battery, Charge, LedState, Motor, Platform, Power, SimPlatform};
use slot_retro::{ButtonMask, MockCore, RetroCore};
// Aliased because `slot_power::Platform` — the device this runs on — is already in scope above
// under that name, and this one is the console a cart is for. Two different questions that
// happen to share a word.
use slot_store::{write_slot_state, Platform as CartPlatform, SlotState};
use tempfile::TempDir;

/// What the emulator was last told to load. `None` until something loads.
pub type Loaded = Arc<Mutex<Option<Vec<u8>>>>;

/// Libretro cores keep their machine in dylib globals, so only one may be live. Every test that
/// opens a real dylib takes `core_lock()` first, or a second open silently falls back to the mock.
static CORE_LOCK: Mutex<()> = Mutex::new(());

pub fn core_lock() -> MutexGuard<'static, ()> {
    CORE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// A free loopback TCP port from the OS, so concurrent test runs never collide on a fixed number.
pub fn free_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("loopback would not give out a port")
        .local_addr()
        .expect("a bound listener with no address")
        .port()
}

/// One live link session at a time per test binary. The product uses a single port, so a host
/// in one test and a joiner in another would connect. Hold it for the whole test: the worker
/// keeps the port until the `App` is dropped.
static LINK_PORT_LOCK: Mutex<()> = Mutex::new(());

pub fn link_port_lock() -> MutexGuard<'static, ()> {
    LINK_PORT_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn tmp_root_with_carts(stems: &[&str]) -> TempDir {
    let d = tmp_root();
    for stem in stems {
        let mut rom = vec![0u8; 0x100];
        let title = stem.to_uppercase();
        rom[0xa0..0xa0 + title.len()].copy_from_slice(title.as_bytes());
        std::fs::write(rom_path(&d, stem), rom).expect("write rom");
    }
    d
}

/// The same card, holding Game Boy carts instead. `Games/GB/` and the `.gb` extension are the
/// whole of what makes the scan read one as `Platform::Gb`: nothing in the bytes is consulted,
/// and the shelf falls back to the filename when the header title is empty — so what this
/// writes into the header is there to keep the fixture a plausible cart rather than to be read
/// back.
///
/// Kept apart from `tmp_root_with_carts` rather than folded into it with a platform argument,
/// because every existing caller is about a GBA card and naming the platform at fifty call
/// sites would say nothing any of them care about.
///
/// The stem is truncated to the header's eleven bytes rather than asserted against them: a
/// fixture built from a long filename is a normal thing for a caller to want, and the title is
/// not what any of these tests read back.
pub fn tmp_root_with_gb_carts(stems: &[&str]) -> TempDir {
    let d = tmp_root();
    for stem in stems {
        let title = stem.to_uppercase();
        write_gb_cart(&d, stem, &title[..title.len().min(11)]);
    }
    d
}

/// A Game Boy cart on the card, with a title of your choosing — which is what makes it worth
/// having beside `tmp_root_with_gb_carts`: the link refusal keys on the title, so a test needs
/// to be able to write `POKEMON RED` onto a cart whose filename says something else.
///
/// `scan` reads the platform off the folder and never off the ROM, so this writes into
/// `Games/GB/`. The title goes at 0x134 in the eleven byte field `slot_store::gb::title` reads,
/// a different place entirely from the 0xA0 a GBA header keeps its own in — and the rom runs
/// past 0x14F so the whole cartridge header, CGB flag and all, is inside the file rather than
/// running off the end of it.
pub fn write_gb_cart(d: &TempDir, stem: &str, title: &str) {
    assert!(
        title.len() <= 11,
        "a Game Boy header title is eleven bytes, and {title:?} is longer"
    );
    let mut rom = vec![0u8; 0x150];
    rom[0x134..0x134 + title.len()].copy_from_slice(title.as_bytes());
    std::fs::write(cart_path(d, CartPlatform::Gb, stem), rom).expect("write rom");
}

/// The headers `tmp_root_with_carts` writes are not roms, and a real core refuses them.
/// Anything that puts a cart in the slot for real needs these instead.
pub fn tmp_root_with_real_carts(stems: &[&str]) -> TempDir {
    let d = tmp_root();
    for stem in stems {
        std::fs::write(rom_path(&d, stem), gba_rom()).expect("write rom");
    }
    d
}

fn tmp_root() -> TempDir {
    let d = tempfile::tempdir().expect("tempdir");
    for sub in slot::root::DIRS {
        std::fs::create_dir(d.path().join(sub)).expect("create content dir");
    }
    d
}

fn rom_path(d: &TempDir, stem: &str) -> PathBuf {
    cart_path(d, CartPlatform::Gba, stem)
}

/// The one place a test builds a rom path, for either platform, and it builds it out of
/// `Platform` itself: the folder from `dir_name` and the extension from `extensions`, so a
/// fixture cannot be written to a folder whose scan would pass it over — which is exactly what
/// a `.gb` under `Games/GBA/` would be, a file on the card that never reaches the shelf.
fn cart_path(d: &TempDir, platform: CartPlatform, stem: &str) -> PathBuf {
    d.path()
        .join("Games")
        .join(platform.dir_name())
        .join(format!("{stem}.{}", platform.extensions()[0]))
}

/// A header gpSP takes at its word: title, code, the entry branch's 0xEA and the fixed 0x96.
pub fn write_retail_header(d: &TempDir, stem: &str, title: &str, code: &str) {
    let mut rom = vec![0u8; 0x100];
    rom[3] = 0xEA;
    rom[0xa0..0xa0 + title.len()].copy_from_slice(title.as_bytes());
    rom[0xac..0xac + code.len()].copy_from_slice(code.as_bytes());
    rom[0xb2] = 0x96;
    std::fs::write(rom_path(d, stem), rom).expect("write rom");
}

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The core `scripts/fetch-core.sh` pulls down, or `None` if it has not been run.
pub fn vendored_core() -> Option<PathBuf> {
    let p = repo_root().join(format!(
        "vendor/mgba_libretro.{}",
        std::env::consts::DLL_EXTENSION
    ));
    p.exists().then_some(p)
}

/// The user's own BIOS, if present. Never checked in (it is Nintendo's); tests needing it skip.
pub fn real_bios() -> Option<PathBuf> {
    let p = repo_root().join("sdcard/BIOS/gba_bios.bin");
    p.exists().then_some(p)
}

/// `gba_rom` with a real cart's Nintendo logo, lifted from a cart on the card (not checked in).
/// The BIOS will not play its splash without it. `None` when there is no cart to lift it from.
pub fn logo_rom() -> Option<Vec<u8>> {
    let logo = std::fs::read_dir(repo_root().join("sdcard/Games/GBA"))
        .ok()?
        .find_map(|e| {
            let p = e.ok()?.path();
            let rom = (p.extension()? == "gba").then(|| std::fs::read(&p).ok())??;
            (rom.get(4..8)? == [0x24, 0xff, 0xae, 0x51]).then(|| rom[4..0xa0].to_vec())
        })?;
    let mut rom = gba_rom();
    // Before the header checksum range (0xa0..0xbd), so `gba_rom`'s checksum still holds.
    rom[4..0xa0].copy_from_slice(&logo);
    Some(rom)
}

/// Whether a frame is the white BIOS boot screen rather than `gba_rom`'s black one.
pub fn mostly_lit(frame: &[u8]) -> bool {
    let lit = frame
        .chunks(4)
        .filter(|p| p[0] > 0x40 && p[1] > 0x40 && p[2] > 0x40)
        .count();
    lit * 2 > (slot_retro::GBA_W * slot_retro::GBA_H) as usize
}

/// Sets mode 3 and writes a frame counter into the first pixel once per vblank, so
/// consecutive frames differ and a savestate has both registers and VRAM worth restoring.
pub fn gba_rom() -> Vec<u8> {
    const CODE: [u32; 15] = [
        0xe3a00404, // mov  r0, #0x04000000
        0xe3a01c04, // mov  r1, #0x400
        0xe3811003, // orr  r1, r1, #3
        0xe5801000, // str  r1, [r0]          DISPCNT: mode 3, BG2 on
        0xe3a02406, // mov  r2, #0x06000000
        0xe3a03000, // mov  r3, #0
        0xe1d040b6, // vb:  ldrh r4, [r0, #6] VCOUNT
        0xe35400a0, //      cmp  r4, #160
        0x1afffffc, //      bne  vb
        0xe2833001, //      add  r3, r3, #1
        0xe1c230b0, //      strh r3, [r2]
        0xe1d040b6, // dr:  ldrh r4, [r0, #6]
        0xe35400a0, //      cmp  r4, #160
        0x0afffffc, //      beq  dr
        0xeafffff6, //      b    vb
    ];
    let mut rom = vec![0u8; 0x8000];
    rom[0..4].copy_from_slice(&0xea00002eu32.to_le_bytes()); // b 0xc0
    rom[0xa0..0xac].copy_from_slice(b"SLOT TEST\0\0\0");
    rom[0xac..0xb0].copy_from_slice(b"SLTE");
    rom[0xb0..0xb2].copy_from_slice(b"00");
    rom[0xb2] = 0x96; // fixed header byte, cores sniff it to identify a GBA rom
    let sum = rom[0xa0..0xbd].iter().fold(0u8, |a, b| a.wrapping_add(*b));
    rom[0xbd] = 0u8.wrapping_sub(sum).wrapping_sub(0x19);
    for (i, w) in CODE.iter().enumerate() {
        let o = 0xc0 + i * 4;
        rom[o..o + 4].copy_from_slice(&w.to_le_bytes());
    }
    rom
}

/// `gba_rom` with a different title and code and a fixed checksum: a loadable ROM with a chosen
/// identity, unlike `write_retail_header`'s bare header.
pub fn write_real_cart_as(d: &TempDir, stem: &str, title: &str, code: &str) {
    let mut rom = gba_rom();
    rom[0xa0..0xac].fill(0);
    rom[0xa0..0xa0 + title.len()].copy_from_slice(title.as_bytes());
    rom[0xac..0xac + code.len()].copy_from_slice(code.as_bytes());
    let sum = rom[0xa0..0xbd].iter().fold(0u8, |a, b| a.wrapping_add(*b));
    rom[0xbd] = 0u8.wrapping_sub(sum).wrapping_sub(0x19);
    std::fs::write(rom_path(d, stem), rom).expect("write rom");
}

/// Stands in for the emulator worker at a flush point.
pub struct StubSnapshot {
    pub state: Vec<u8>,
    pub sav: Option<Vec<u8>>,
    pub thumb: Option<Vec<u8>>,
    pub loaded: Loaded,
}

impl StubSnapshot {
    pub fn boxed() -> Box<dyn Snapshot> {
        StubSnapshot::pair().0
    }

    pub fn pair() -> (Box<dyn Snapshot>, Loaded) {
        let loaded = Loaded::default();
        let stub = StubSnapshot {
            state: vec![9u8; 1024],
            sav: None,
            thumb: Some(b"png".to_vec()),
            loaded: loaded.clone(),
        };
        (Box::new(stub), loaded)
    }
}

impl Snapshot for StubSnapshot {
    fn state(&self) -> Option<Vec<u8>> {
        Some(self.state.clone())
    }

    fn save_ram(&self) -> Option<Vec<u8>> {
        self.sav.clone()
    }

    fn thumb(&self) -> Option<Vec<u8>> {
        self.thumb.clone()
    }

    fn load(&self, state: Vec<u8>) {
        *self.loaded.lock().expect("loaded") = Some(state);
    }
}

/// A snapshot backed by a real core, so a load can be undone and read back.
#[derive(Clone, Default)]
pub struct CoreSnapshot(Arc<Mutex<MockCore>>);

impl CoreSnapshot {
    pub fn new() -> Self {
        let core = CoreSnapshot::default();
        core.with(|c| c.load(Path::new("unused")).expect("load"));
        core
    }

    pub fn boxed(&self) -> Box<dyn Snapshot> {
        Box::new(self.clone())
    }

    pub fn run_frame(&self) {
        self.with(|c| c.run_frame(ButtonMask::default()));
    }

    /// Where the core actually is, as opposed to what the app last asked it for.
    pub fn bytes(&self) -> Vec<u8> {
        self.with(|c| c.serialize().expect("serialize"))
    }

    fn with<T>(&self, f: impl FnOnce(&mut MockCore) -> T) -> T {
        f(&mut self.0.lock().expect("core"))
    }
}

impl Snapshot for CoreSnapshot {
    fn state(&self) -> Option<Vec<u8>> {
        Some(self.bytes())
    }

    fn save_ram(&self) -> Option<Vec<u8>> {
        None
    }

    fn thumb(&self) -> Option<Vec<u8>> {
        Some(b"png".to_vec())
    }

    fn load(&self, state: Vec<u8>) {
        self.with(|c| c.unserialize(&state).expect("unserialize"));
    }
}

/// The stub device's hardware clock. It only moves when a test advances it.
#[derive(Clone, Default)]
pub struct Clock(Arc<AtomicI64>);

impl Clock {
    pub fn at(secs: i64) -> Self {
        Clock(Arc::new(AtomicI64::new(secs)))
    }

    pub fn get(&self) -> i64 {
        self.0.load(Ordering::Relaxed)
    }

    pub fn advance(&self, secs: i64) {
        self.0.fetch_add(secs, Ordering::Relaxed);
    }
}

/// Stands in for the device the power path acts on.
pub struct StubPlatform {
    backlight: Arc<AtomicU8>,
    root: PathBuf,
    clock: Clock,
    /// 0 = Unknown, 1 = Discharging, 2 = Charging, 3 = Full. Shared so a test can move it mid-run.
    charge: Arc<AtomicU8>,
    /// The gauge reading `battery()` returns, movable independently of `charge`.
    percent: Arc<AtomicU8>,
    /// What `set_led` last wrote, coded by `led_code`, so a test sees what reached the platform.
    led: Arc<AtomicU8>,
    /// How many times `set_led` was called, to catch a write repeated every tick.
    led_writes: Arc<AtomicUsize>,
}

/// Integer coding of `LedState` so the stub can carry it through an `AtomicU8`.
pub fn led_code(state: LedState) -> u8 {
    match state {
        LedState::Off => 0,
        LedState::Running => 1,
        LedState::Low => 2,
        LedState::Charging => 3,
        LedState::Charged => 4,
    }
}

/// A clock that reads like a real date. At the epoch `set_power` would send the app to the
/// clock screen, since that means the RTC never came up.
pub const CLOCK_IS_SET: i64 = 1_786_568_000;

pub fn panel(root: &Path, timeout: Duration) -> (Power, Arc<AtomicU8>) {
    let (power, backlight, _, _, _) = rig_with_charge(root, timeout, CLOCK_IS_SET, 0, 50);
    (power, backlight)
}

/// `panel` with a chosen charge state and percent.
pub fn panel_with_battery(
    root: &Path,
    timeout: Duration,
    charge: u8,
    percent: u8,
) -> (Power, Arc<AtomicU8>) {
    let (power, backlight, _, _, _) = rig_with_charge(root, timeout, CLOCK_IS_SET, charge, percent);
    (power, backlight)
}

fn rig(root: &Path, timeout: Duration, secs: i64) -> (Power, Arc<AtomicU8>, Clock) {
    let (power, backlight, clock, _, _) = rig_with_charge(root, timeout, secs, 0, 50);
    (power, backlight, clock)
}

fn rig_with_charge(
    root: &Path,
    timeout: Duration,
    secs: i64,
    charge: u8,
    percent: u8,
) -> (Power, Arc<AtomicU8>, Clock, Arc<AtomicU8>, Arc<AtomicU8>) {
    let (power, backlight, clock, charge, percent, _led, _led_writes) =
        rig_with_led(root, timeout, secs, charge, percent);
    (power, backlight, clock, charge, percent)
}

#[allow(clippy::type_complexity)]
fn rig_with_led(
    root: &Path,
    timeout: Duration,
    secs: i64,
    charge: u8,
    percent: u8,
) -> (
    Power,
    Arc<AtomicU8>,
    Clock,
    Arc<AtomicU8>,
    Arc<AtomicU8>,
    Arc<AtomicU8>,
    Arc<AtomicUsize>,
) {
    let backlight = Arc::new(AtomicU8::new(0));
    let clock = Clock::at(secs);
    let charge = Arc::new(AtomicU8::new(charge));
    let percent = Arc::new(AtomicU8::new(percent));
    // u8::MAX is never produced by `led_code`, so "never written" differs from `LedState::Off`.
    let led = Arc::new(AtomicU8::new(u8::MAX));
    let led_writes = Arc::new(AtomicUsize::new(0));
    let platform = StubPlatform {
        backlight: backlight.clone(),
        root: root.to_path_buf(),
        clock: clock.clone(),
        charge: charge.clone(),
        percent: percent.clone(),
        led: led.clone(),
        led_writes: led_writes.clone(),
    };
    (
        Power::new(Box::new(platform), timeout),
        backlight,
        clock,
        charge,
        percent,
        led,
        led_writes,
    )
}

/// A whole session over `SimPlatform`, which records the motor. `StubPlatform` has none.
pub fn session_with_platform(root: &Path) -> (Session, Motor) {
    clocked(root);
    let platform = SimPlatform::at(root.to_path_buf());
    let motor = platform.motor();
    let mut session = Session::boot(root.to_path_buf());
    session
        .app_mut()
        .set_power(Power::new(Box::new(platform), Duration::from_secs(300)));
    (session, motor)
}

/// Booted onto the clock screen with a platform whose clock can be read back.
pub fn app_booting_with_clock(root: &Path) -> (App, Clock) {
    app_booting_at(root, 0)
}

pub fn app_booting_at(root: &Path, secs: i64) -> (App, Clock) {
    let mut a = App::boot(root);
    let (power, _, clock) = rig(root, Duration::from_secs(60), secs);
    a.set_power(power);
    (a, clock)
}

impl Platform for StubPlatform {
    fn set_backlight(&mut self, step: u8) {
        self.backlight.store(step, Ordering::Relaxed);
    }

    fn charge(&self) -> Charge {
        match self.charge.load(Ordering::Relaxed) {
            1 => Charge::Discharging,
            2 => Charge::Charging,
            3 => Charge::Full,
            _ => Charge::Unknown,
        }
    }

    fn battery(&self) -> Option<Battery> {
        Some(Battery {
            percent: self.percent.load(Ordering::Relaxed),
            charge: self.charge(),
        })
    }

    fn set_led(&mut self, state: LedState) {
        self.led.store(led_code(state), Ordering::Relaxed);
        self.led_writes.fetch_add(1, Ordering::Relaxed);
    }

    fn restart(&mut self) -> ! {
        panic!("the stub platform never powers off")
    }

    fn poweroff(&mut self) -> ! {
        panic!("the stub platform never powers off")
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn now(&self) -> i64 {
        self.clock.get()
    }

    fn set_clock(&mut self, secs: i64) {
        self.clock.0.store(secs, Ordering::Relaxed);
    }

    fn set_rumble(&mut self, _strength: u16) {}
}

/// Marks the clock as confirmed, so boot does not stop on the clock screen.
pub fn clocked(root: &Path) {
    let mut s = slot_store::read_slot_state(root);
    s.clock_set = true;
    write_slot_state(root, &s).expect("write slot.state");
}

pub fn boot(root: &Path) -> App {
    clocked(root);
    App::boot(root)
}

/// Booted onto a seated cart and run past the insert floor.
pub fn app_playing_in(root: &Path, stem: &str) -> App {
    app_playing_with(root, stem, StubSnapshot::boxed())
}

/// `app_playing_in` with charge (Discharging) and percent (50) a test can move independently.
pub fn app_playing_with_charge(root: &Path, stem: &str) -> (App, Arc<AtomicU8>, Arc<AtomicU8>) {
    let mut a = app_playing_with(root, stem, StubSnapshot::boxed());
    let (power, _backlight, _clock, charge, percent) =
        rig_with_charge(root, Duration::from_secs(60), 0, 1, 50);
    a.set_power(power);
    (a, charge, percent)
}

/// `app_playing_in` with the platform's LED record, to watch what reaches `Platform::set_led`.
pub fn app_playing_with_led(
    root: &Path,
    stem: &str,
) -> (
    App,
    Arc<AtomicU8>,
    Arc<AtomicU8>,
    Arc<AtomicU8>,
    Arc<AtomicUsize>,
) {
    let mut a = app_playing_with(root, stem, StubSnapshot::boxed());
    let (power, _backlight, _clock, charge, percent, led, led_writes) =
        rig_with_led(root, Duration::from_secs(60), 0, 1, 50);
    a.set_power(power);
    (a, charge, percent, led, led_writes)
}

/// The same, with the state switcher open. The ring must already hold an entry.
pub fn app_in_switcher(root: &Path, stem: &str) -> App {
    let mut a = app_playing_in(root, stem);
    a.apply(slot_input::Action::Polaroids);
    a
}

/// The same, with the mixer at a given level, written to the card.
pub fn app_playing_with_volume(root: &Path, volume: u8) -> App {
    seated(
        root,
        StubSnapshot::boxed(),
        SlotState {
            cart: Some("Emerald".to_string()),
            volume,
            clock_set: true,
            utc_offset_min: 0,
            ..Default::default()
        },
    )
}

pub fn app_playing_with(root: &Path, stem: &str, snapshot: Box<dyn Snapshot>) -> App {
    seated(
        root,
        snapshot,
        SlotState {
            cart: Some(stem.to_string()),
            clock_set: true,
            utc_offset_min: 0,
            ..Default::default()
        },
    )
}

fn seated(root: &Path, snapshot: Box<dyn Snapshot>, state: SlotState) -> App {
    write_slot_state(root, &state).expect("write slot.state");
    let mut a = App::boot(root);
    a.set_snapshot(snapshot);
    a.on_core_ready();
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    a
}
