use std::collections::HashMap;
use std::path::{Path, PathBuf};

use slot_gfx::{OUT_H, OUT_W};
use slot_input::{Action, Btn, MUTE_CHORD_MS};
use slot_power::{Battery, Charge, LedState, LidPolicy, Power};
use slot_retro::LinkChannel;
use slot_store::{
    format_stamp, read_slot_state, scan, write_slot_state, Cart, Core, Platform, SlotState,
    StateEntry, StateRing, Theme, BLUE_LIGHT_MAX, BRIGHTNESS_MAX, FF_SPEEDS, RING_MAX, VOLUME_MAX,
};
use slot_ui::{
    board_from, board_zoom, draw_backdrop, draw_empty_slot, draw_footer, draw_slot_name,
    draw_sticker, ease, grown, lid_at, lid_from, lift_of, on_board, shelf_cart_at, ClockPicker,
    Draw, FfState, GbShell, Hud, HudKind, Icon, LinkBadge, Millis, Placed, Polaroids, PowerChoice,
    QuickMenu, QuickMenuFaces, QuickRow, QuickValue, Refusal, Shelf, SlotChrome, TexId, Toast,
    BOARD_W, BOARD_X, CART_W, CHIP_H, CHIP_U, CHIP_V, CHIP_W, HINT_EDGE, HINT_H, HOP_LIFT,
    SHADOW_H, SHADOW_W, SOCKET_H, SOCKET_U, SOCKET_V, SOCKET_W, TURN_PAD,
};

use crate::audio::Sfx;
use crate::core_picker::{Chip, CorePicker, Outcome, Press};
use crate::link_kind::{link_carried, link_kind, serial_option, LinkKind};
use crate::link_radio::{radio_jobs, LinkRole, RadioJob, RadioJobs};
use crate::link_screen::LinkSprites;
use crate::link_start::{link_port, LinkFail, LinkProgress, LinkStarter, LinkStep};
use crate::persist::{self, Snapshot};
use crate::video_mode::{self, VideoMode};

/// A floor, not a delay: a slow core load extends the insert, a fast one still waits it out.
pub const INSERT_S: f32 = 0.73;
/// The tail of the insert, after the cart has landed, so the game does not appear on the seating
/// frame. Covers the landing sound: see `the_game_waits_for_the_cart_to_finish_landing`.
const INSERT_HOLD_S: f32 = 0.28;

/// When the cart reaches the contacts, which is when it clicks.
pub const SEATED_AT: f32 = INSERT_S - INSERT_HOLD_S;

/// The insert played backwards. Everything on screen runs off one progress, so the lengths match.
pub const EJECT_S: f32 = SEATED_AT;

/// Pause before the cart moves, so it does not come out over the last ring of queued audio.
const EJECT_HOLD_S: f32 = 0.35;

/// The panel striking once the cart is home.
const POWER_ON_S: f32 = 0.22;

/// Quicker than power-on, as a panel dies faster than it strikes.
const POWER_OFF_S: f32 = 0.16;

/// Volume has ten times the range of the other two, so twenty presses end to end match their ten.
const VOLUME_STEP: u8 = 5;

/// Crash insurance, and the only durable write that happens with the game still running.
const AUTOSAVE_MS: Millis = 60_000;

/// At this charge the state is saved and the device stops: a dead battery is the likeliest cutoff.
const BATTERY_CRITICAL: u8 = 5;

/// The gauge moves over minutes and is a sysfs read.
const BATTERY_POLL_MS: Millis = 10_000;

/// Charge state flips the instant a cable goes in, and reading `status` is cheap.
const CHARGE_POLL_MS: Millis = 1_000;

/// The LED goes red below this. A warning, well clear of `BATTERY_CRITICAL`.
const BATTERY_LOW: u8 = 20;

/// Long enough to notice the wrong state loaded, short enough to be gone by the next opening.
pub const UNDO_GRACE_MS: Millis = 30_000;

/// How long a link whose other end went away shows its broken badge before the session ends.
pub const LINK_LOST_MS: Millis = 2000;

/// How long A is held on the shelf to start a cart clean.
const PLAY_HOLD_MS: Millis = 500;

/// How long the shutdown screen shows before the machine stops. A few frames, so the ordinary loop
/// presents it: rendering out of band can block on a GPU that is going away.
const SHUTDOWN_SHOW_MS: Millis = 250;

/// Row pitch: clears the 40 px face with a little air.
const POWER_MENU_PITCH: f32 = 44.0;
/// Bar inset top and bottom, so adjacent rows stay separate.
const POWER_MENU_BAR_INSET: f32 = 4.0;
/// How far the row recedes while a cart is open, as `Shelf::draw_row` counts it. Puts the
/// neighbours at -41 and 574, where the mockup frames the open cart.
const CORE_PICKER_RECEDE: f32 = 0.26;

/// The shelf's machine, printed in the slot when the shelf changes: faded in, held, faded out,
/// and never above `SLOT_NAME_ALPHA`, so it reads as something printed in the dark.
const SLOT_NAME_IN_MS: Millis = 200;
const SLOT_NAME_HOLD_MS: Millis = 1200;
const SLOT_NAME_OUT_MS: Millis = 800;
const SLOT_NAME_ALPHA: f32 = 0.4;
/// Extra dim on the neighbours while a cart is open: a side face at
/// `SIDE_ALPHA * (1 - CORE_PICKER_RECEDE)` = 0.407 goes to the mockup's 0.25 (0.25 / 0.407).
const CORE_PICKER_DIM: f32 = 0.614;
/// The legend's line, under the open cart and clear of the case band.
const CORE_LEGEND_Y: f32 = 386.0;
/// Top of the link screen's one line of text: its baseline lands near y 74.
const LINK_TEXT_Y: f32 = 44.0;
/// The legend, centred on the console strip (y 388–480).
const LINK_LEGEND_Y: f32 = 422.0;
const LINK_LEGEND_GAP: f32 = 40.0;
/// The soft oval under the resting lid, from the mockup: size, drop below the lid's bottom edge,
/// and darkness. Scaled with the lid as it lifts.
const LID_SHADOW_W: f32 = 168.0;
const LID_SHADOW_H: f32 = 18.0;
const LID_SHADOW_DROP: f32 = 29.0;
const LID_SHADOW_ALPHA: f32 = 0.8;
/// Longest the cart waits for its faces before opening anyway. Also covers the worker finishing a
/// previous cart's build after a fast scroll.
const FACES_WAIT_MS: Millis = 1500;

/// Fractions of a refused cart's exit: where the alert starts to fade and where it is gone. It must
/// be gone before the shelf returns, or it reads as something to dismiss.
const ALERT_HOLD: f32 = 0.45;
const ALERT_GONE: f32 = 0.9;

/// Any clock reading before this (2020-01-01) was never set: an RTC that lost power leaves the
/// kernel at the epoch.
const CLOCK_FLOOR: i64 = 1_577_836_800;

/// The most recent undoable action. A new save or load replaces it.
pub enum PendingUndo {
    Save {
        stamp: String,
        /// Read out of the ring before the push evicted it, so undo restores it.
        evicted: Option<(String, Vec<u8>, Vec<u8>)>,
    },
    Load {
        prior: Vec<u8>,
    },
}

/// One side of a live netpacket session: what the rest of `App` needs, and this device's libretro
/// client id.
struct LinkSession {
    client_id: u16,
    /// When the other end was found gone. The session lasts `LINK_LOST_MS` past it, so the
    /// broken badge is seen.
    lost_at: Option<Millis>,
}

/// A link being started. `client_id` comes from the picked row and `Ready` needs it frames later.
struct LinkStarting {
    starter: LinkStarter,
    client_id: u16,
}

/// A link picked in a mode the running game was not loaded for, and the reload that has to
/// happen before it can start.
struct Reload {
    /// The cart being loaded again.
    stem: String,
    /// The role A picked.
    role: LinkRow,
    /// No link starts when the reload finishes: B was pressed during it, or the screen closed.
    cancelled: bool,
    /// The mode before the switch, known to load, so a failed reload goes back to it.
    from: LinkKind,
    from_serial: &'static str,
    /// This load is already the way back to `from`.
    fallback: bool,
}

/// The in-game menu, an overlay on a paused game so cancelling returns to the same session.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GameMenu {
    /// Choosing a role. Left/Right swaps, SELECT switches the hardware, A links, B leaves.
    Pick(LinkRow),
    /// The worker is bringing a link up. `since` is when this state began.
    Working {
        role: LinkRow,
        step: LinkStep,
        since: Millis,
    },
    /// The link is up. `worked` is when Working began. `opened` is false for the flash that leaves
    /// after `LINKED_HOLD_MS`, true when opened over a live session, where it stays.
    Linked {
        role: LinkRow,
        worked: Millis,
        since: Millis,
        opened: bool,
    },
    /// A link that did not come up. `worked` is when Working began.
    Failed {
        role: LinkRow,
        fail: LinkFail,
        worked: Millis,
        since: Millis,
    },
    /// The plug coming back out after the session has already ended. Takes no input and leaves on
    /// its own after `UNPLUG_HOLD_MS`.
    Unplug { role: LinkRow, since: Millis },
}

/// Which end of a link this device offers to be. The player picks; there is no discovery.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LinkRow {
    Host,
    Join,
}

impl LinkRow {
    /// Host first: it is the end that has to exist before the other one can arrive.
    pub const ALL: [LinkRow; 2] = [LinkRow::Host, LinkRow::Join];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn text(self) -> &'static str {
        match self {
            LinkRow::Host => "Host",
            LinkRow::Join => "Join",
        }
    }

    /// What the radio is asked to bring up: an access point, or an association to one.
    pub fn role(self) -> LinkRole {
        match self {
            LinkRow::Host => LinkRole::Host,
            LinkRow::Join => LinkRole::Join,
        }
    }

    /// libretro's client id: 0 the host, 1 the joiner. The two devices must never share one.
    pub fn client_id(self) -> u16 {
        match self {
            LinkRow::Host => 0,
            LinkRow::Join => 1,
        }
    }

    pub fn other(self) -> LinkRow {
        match self {
            LinkRow::Host => LinkRow::Join,
            LinkRow::Join => LinkRow::Host,
        }
    }

    /// libretro's numbering: the host is client 0.
    pub fn from_client_id(id: u16) -> LinkRow {
        if id == 0 {
            LinkRow::Host
        } else {
            LinkRow::Join
        }
    }
}

/// The keys the link screen shows, in upload order.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LinkLegend {
    Cancel,
    Mode,
    Swap,
    Link,
    Ok,
    Back,
    EndLink,
}

impl LinkLegend {
    pub const ALL: [LinkLegend; 7] = [
        LinkLegend::Cancel,
        LinkLegend::Mode,
        LinkLegend::Swap,
        LinkLegend::Link,
        LinkLegend::Ok,
        LinkLegend::Back,
        LinkLegend::EndLink,
    ];

    pub fn index(self) -> usize {
        self as usize
    }
}

/// How long LINKED stays on screen once a link is up.
pub const LINKED_HOLD_MS: Millis = 1000;

/// How long the unplug stays on screen when a link ends. Past the plug's 260 ms travel so it is
/// seen at rest; the game is paused for exactly this long.
pub const UNPLUG_HOLD_MS: Millis = 420;

#[derive(Debug)]
pub enum Phase {
    /// Slot's first launch, before the shelf: three things run off the wall clock and a cartridge
    /// RTC breaks silently. Also the quick menu's Date & Time.
    SetClock {
        picker: ClockPicker,
        /// The UTC the picker opened on. Confirming sets now plus the picker's offset from this,
        /// since the clock keeps running under the screen.
        seed: i64,
        /// Opened from the quick menu, so B and confirming return to it. At first boot,
        /// confirming goes on to the shelf.
        from_menu: bool,
    },
    Shelf,
    /// The settings, from MENU on the carousel. The shelf keeps its place underneath.
    QuickMenu {
        row: QuickRow,
    },
    Inserting {
        cart: String,
        t: f32,
        core_ready: bool,
        /// A cart already seated at boot: drawn seated from the first frame, with no shelf behind.
        resumed: bool,
        /// Start clean, skipping (not deleting) `resume.state`.
        clean: bool,
    },
    Playing {
        cart: String,
    },
    Ejecting {
        cart: String,
        t: f32,
    },
    Polaroids {
        cart: String,
    },
    /// The label, opened from the quick menu and left back to it.
    About,
    Doze {
        cart: Option<String>,
    },
}

pub struct App {
    phase: Phase,
    /// One carousel per platform, in `Platform::ALL` order (the shoulders' ring order). Every
    /// platform is held even with no carts, and each keeps its own index, scroll and repeat.
    shelves: Vec<(Platform, Shelf)>,
    /// Which of `shelves` is on screen. Not persisted, like `Shelf::index`.
    shelf_at: usize,
    /// When A went down on the shelf. Held here because `Gestures` is blind to the screen.
    play_held: Option<Millis>,
    /// The last refused action: the only thing telling an eject from a cart that would not seat.
    refusal: Option<Refusal>,
    /// The `t` a refused cart's exit started from. `None` for a requested eject.
    refused_from: Option<f32>,
    alert_face: Option<TexId>,
    /// Shutdown text, one per `PowerChoice::ALL` in that order, at the menu's size.
    shutdown_faces: Vec<(TexId, u32, u32)>,
    /// The highlighted row while the POWER menu is open. An overlay, so cancelling needs no phase.
    power_menu: Option<usize>,
    /// One per `PowerChoice::ALL`, in that order, with the size each was rastered at.
    power_menu_faces: Vec<(TexId, u32, u32)>,
    /// The picker while the cart is open and while its lid goes back on. Acts on the shelf's cart,
    /// which cannot move while it is up.
    core_picker: Option<CorePicker>,
    /// The open cart's board and lid (the shelf face with a transparent border so it can turn).
    /// Rebuilt when the highlighted cart changes.
    core_board_face: Option<TexId>,
    core_lid_face: Option<TexId>,
    /// The cart the uploaded board and lid were built for.
    core_faces_stem: Option<String>,
    /// In `Core::ALL` order: empty sockets, and seated named chips. Uploaded once at boot.
    core_socket_faces: Vec<TexId>,
    core_chip_faces: Vec<TexId>,
    /// The chip in flight, blank, and the shadow under it.
    core_blank_chip_face: Option<TexId>,
    core_chip_shadow_face: Option<TexId>,
    /// Cancel, the Swap arrows and Choose, with raster widths. Laid out under the cart's left edge,
    /// the panel centre and the cart's right edge.
    core_legend_faces: Vec<(TexId, u32)>,
    /// Open when SELECT+MENU raised the in-game menu. An overlay because `Phase::Playing` holds the
    /// seated session.
    game_menu: Option<GameMenu>,
    link_sprites: Option<LinkSprites>,
    /// The hardware the link screen shows and a link runs over. Read when the screen opens,
    /// switched by SELECT.
    link_hardware: LinkKind,
    /// The last picked role, so the screen reopens on it.
    last_role: LinkRow,
    /// The hardware SELECT last chose, per cart stem. Not persisted.
    link_choices: HashMap<String, LinkKind>,
    /// The `gpsp_serial` the seated core was loaded with. gpSP reads link mode only at load, so
    /// this is what a link would run over. `None` when empty, or for a core loaded on `auto`.
    link_loaded: Option<&'static str>,
    /// A reload (cart, `gpsp_serial`) waiting for `Session::update` to collect.
    link_reload: Option<(String, &'static str)>,
    /// The reload in progress, from A until the game runs again or leaves the slot.
    reload: Option<Reload>,
    /// One per `LinkRow::ALL`, in that order.
    link_menu_faces: Vec<(TexId, u32, u32)>,
    /// What `Linked` says, at the menu's own size.
    link_linked_face: Option<(TexId, u32, u32)>,
    /// One per `LinkStep::ALL` and one per `LinkFail::SHOWN`, in those orders.
    link_step_faces: Vec<(TexId, u32, u32)>,
    link_fail_faces: Vec<(TexId, u32, u32)>,
    /// One per `LinkLegend::ALL`, in that order, with the width each was rastered at.
    link_legend_faces: Vec<(TexId, u32)>,
    /// The worker behind `GameMenu::Working`. It has no `Drop`, so `close_game_menu` stops it.
    starting: Option<LinkStarting>,
    /// The wire a finished starter handed over, drained by `Session::update` into
    /// `EmuHandle::begin_link`.
    link_transport: Option<(u16, Box<dyn LinkChannel>)>,
    /// Set when the menu's Restart is chosen. The binary acts on it, like `powering_off`.
    restarting: bool,
    /// When the binary may act on a shutdown. Waits a few frames so the ordinary loop presents
    /// the screen: an extra out-of-band draw and swap can block on a GPU being torn down.
    act_at: Millis,
    /// `None` outside the binary, where there is no content root and nothing persists.
    root: Option<PathBuf>,
    state: SlotState,
    /// Volume and mute before each of the last two volume presses, oldest first. The mute chord
    /// arrives after its two presses, so toggling must undo what they moved.
    vol_before: Vec<(u8, bool, Millis)>,
    /// `None` until a cart is in the slot. There is nothing to flush without a core.
    snapshot: Option<Box<dyn Snapshot>>,
    /// The seated cart's `Core`, set by whoever spawned `snapshot`. Used instead of `core_for` so
    /// nothing can disagree with the running dylib. Stale between carts, like `snapshot`; every
    /// reader is gated on a seated cart.
    core: Core,
    /// A colour correction change for `Session::update` to carry to the running core. The only
    /// option reachable in game; the rest are applied once before `load`.
    colour_pending: Option<bool>,
    /// The port this device drives in a cable session, read by `Session::spawn_core`.
    link_player: Option<u8>,
    /// The seated cart's `Platform`, set with `core`. Saves and states are filed under it, so a
    /// `.gb` and a `.gba` sharing a stem never share them.
    platform: Platform,
    /// Whether the running emulator is the one `core` names rather than the mock. Only
    /// `retire_refused_resume` reads it, and `false` acts on nothing.
    named_core: bool,
    /// How the seated cart's picture is drawn, off the card. Only a Game Boy cart can change it;
    /// `source_rect` decides.
    video_mode: VideoMode,
    /// `Some` while a netpacket session is live. Bookkeeping only: the transport and core live on
    /// the emulator thread.
    link: Option<LinkSession>,
    /// The slot's sound for this frame, drained by the device owner.
    sfx: Option<Sfx>,
    /// `Some` while the switcher shows, holding the ring as it opened so a save cannot renumber it.
    polaroids: Option<Polaroids>,
    /// The one undoable action and when it happened. Leaves with the cart.
    pending: Option<(PendingUndo, Millis)>,
    /// The switcher's key caps. The undo cap names its action, so it is rasterised on entry.
    legend_faces: Vec<TexId>,
    undo_face: Option<TexId>,
    /// The clock screen's line and instruction, rasterised when the line changes.
    clock_faces: Option<(TexId, TexId)>,
    /// The quick menu's rows, values, arrows and legend, uploaded once at boot.
    quick_menu_faces: Option<QuickMenuFaces>,
    /// Date & Time's value, grey then lit. Rebuilt when the minute turns while the menu is up.
    quick_clock_faces: Option<[(TexId, u32, u32); 2]>,
    /// The label, rasterised whole. Re-uploaded when the gauge moves.
    sticker_face: Option<TexId>,
    /// One picture from `Wallpapers`, behind the shelf. Usually `None`.
    wallpaper: Option<TexId>,
    /// What is printed on the case: the battery's percent, and the time as it stands.
    battery_percent: slot_ui::Printed,
    /// The charging glyph, uploaded once at boot.
    bolt: Option<TexId>,
    /// The machine whose shelf is showing. Empty on a card with one shelf, where naming it would
    /// label a thing that could not be anything else.
    shelf_platform: slot_ui::Printed,
    /// The shelf changed and its name has not been shown yet.
    name_pending: bool,
    slot_letter: Option<char>,
    /// When the name started showing in the slot. Starts on the first frame after its face
    /// exists, not at the switch: the face is rasterised a frame later, and at boot the clock
    /// jumps by however long every cart face took to rasterise.
    shelf_named: Option<Millis>,
    shelf_clock: slot_ui::Printed,
    hud: Hud,
    /// How far up the game layer's screen is. Not a phase: it outlives the insert.
    screen: f32,
    /// Whether the core has published a frame. Pushed in, since the compositor still holds the
    /// last cart's frame.
    game_ready: bool,
    /// Milliseconds accumulated from `update`, the app's only clock.
    clock: f64,
    /// `None` in unit tests, where there is no panel to darken and no battery to run out.
    power: Option<Power>,
    dozed_at: Millis,
    /// The held POWER press is the one that woke the panel. Assigned on every press so a lost
    /// release heals. See `power_press`.
    woke_on_press: bool,
    /// When the state next has to be on the card. Moved by every resume write.
    autosave_at: Millis,
    battery_at: Millis,
    charge_at: Millis,
    /// The last full reading, its charge half kept current by the fast tick. One snapshot, so
    /// percent and bolt always agree.
    battery: Option<Battery>,
    /// What the LED was last set to. The tick recomputes it every second and `set_led` is a real
    /// write, so only changes go through.
    last_led: Option<LedState>,
    powering_off: bool,
    /// The radio's slow work (driver load and unload), queued in order off the frame loop.
    radio: Box<dyn RadioJobs>,
}

/// The library split into one shelf per `Platform::ALL` entry, in ring order, empty ones
/// included.
fn shelves_of(carts: Vec<Cart>) -> Vec<(Platform, Shelf)> {
    let mut rows: Vec<(Platform, Vec<Cart>)> =
        Platform::ALL.iter().map(|p| (*p, Vec::new())).collect();
    for cart in carts {
        if let Some((_, row)) = rows.iter_mut().find(|(p, _)| *p == cart.platform) {
            row.push(cart);
        }
    }
    rows.into_iter()
        .map(|(platform, carts)| (platform, Shelf::new(carts)))
        .collect()
}

impl App {
    pub fn new(carts: Vec<Cart>) -> Self {
        let shelves = shelves_of(carts);
        // Never an empty shelf while another has carts on it: a device that opened on nothing
        // with a full shelf one button away would read as a card that failed to scan.
        let shelf_at = shelves
            .iter()
            .position(|(_, s)| !s.carts.is_empty())
            .unwrap_or(0);
        App {
            radio: radio_jobs(),
            phase: Phase::Shelf,
            shelves,
            shelf_at,
            play_held: None,
            refusal: None,
            refused_from: None,
            alert_face: None,
            shutdown_faces: Vec::new(),
            power_menu: None,
            power_menu_faces: Vec::new(),
            core_picker: None,
            core_board_face: None,
            core_lid_face: None,
            core_faces_stem: None,
            core_socket_faces: Vec::new(),
            core_chip_faces: Vec::new(),
            core_blank_chip_face: None,
            core_chip_shadow_face: None,
            core_legend_faces: Vec::new(),
            game_menu: None,
            link_sprites: None,
            link_hardware: LinkKind::Cable,
            last_role: LinkRow::Host,
            link_choices: HashMap::new(),
            link_loaded: None,
            link_reload: None,
            reload: None,
            link_menu_faces: Vec::new(),
            link_linked_face: None,
            link_step_faces: Vec::new(),
            link_fail_faces: Vec::new(),
            link_legend_faces: Vec::new(),
            starting: None,
            link_transport: None,
            restarting: false,
            act_at: 0,
            root: None,
            state: SlotState::default(),
            vol_before: Vec::new(),
            snapshot: None,
            core: Core::default(),
            colour_pending: None,
            link_player: None,
            platform: Platform::default(),
            named_core: false,
            video_mode: VideoMode::default(),
            link: None,
            sfx: None,
            polaroids: None,
            pending: None,
            legend_faces: Vec::new(),
            undo_face: None,
            clock_faces: None,
            quick_menu_faces: None,
            quick_clock_faces: None,
            sticker_face: None,
            wallpaper: None,
            battery_percent: slot_ui::Printed::default(),
            bolt: None,
            shelf_platform: slot_ui::Printed::default(),
            name_pending: false,
            slot_letter: None,
            shelf_named: None,
            shelf_clock: slot_ui::Printed::default(),
            hud: Hud::new(),
            screen: 0.0,
            game_ready: false,
            clock: 0.0,
            power: None,
            dozed_at: 0,
            woke_on_press: false,
            autosave_at: AUTOSAVE_MS,
            battery_at: BATTERY_POLL_MS,
            charge_at: CHARGE_POLL_MS,
            battery: None,
            last_led: None,
            powering_off: false,
        }
    }

    /// A seated cart goes back in through the insert animation, so boot and resume look the same.
    /// Only the platform folders under `Games/` are scanned; an empty scan is a shelf.
    pub fn boot(root: &Path) -> Self {
        crate::root::ensure(root);
        // Before anything is drawn. The card's palette is read once.
        slot_ui::set_theme(Theme::read(root));
        let mut app = App::new(scan(root).unwrap_or_default());
        app.root = Some(root.to_path_buf());
        app.state = read_slot_state(root);
        if app.state.clock_set {
            app.start();
        } else {
            // Re-seeded by `set_power`, the first moment the device's own clock can be read.
            app.phase = clock_screen(system_secs(), 0, false);
        }
        app
    }

    /// Into the slot or onto the shelf, once the clock is known.
    fn start(&mut self) {
        // One cart is a dedicated device: seat it whatever `slot.state` remembers.
        let seated = if self.single_cart() {
            // The one cart is on the one shelf that has anything, which is the shelf the
            // carousel already opened on.
            Some((self.shelf_at, 0))
        } else {
            let stem = self.state.cart.clone();
            let platform = self.state.cart_platform;
            stem.and_then(|stem| self.seat_of(&stem, platform))
        };
        self.phase = Phase::Shelf;
        match seated {
            Some((at, i)) => {
                // The carousel opens on the resumed cart's own shelf, sitting on the cart
                // itself, so ejecting it lands where it left.
                self.shelf_at = at;
                self.shelf_mut().select(i);
                // Never clean: a resume is the whole point of the cart still being in there.
                self.insert(false);
                if let Phase::Inserting { resumed, t, .. } = &mut self.phase {
                    *resumed = true;
                    // Already seated. The floor still runs so the core has the same time to load.
                    *t = INSERT_S;
                }
            }
            // A cart the library no longer has is an empty slot, stem and platform both. Left on
            // disk for the next seat to rewrite: boot is the worst moment for a write.
            None => {
                self.state.cart = None;
                self.state.cart_platform = None;
                // Booting onto the shelf names it once, the way a switch does.
                self.name_pending = true;
            }
        }
    }

    /// The carousel on screen. Not the library, which is `carts`.
    fn shelf(&self) -> &Shelf {
        &self.shelves[self.shelf_at].1
    }

    fn shelf_mut(&mut self) -> &mut Shelf {
        &mut self.shelves[self.shelf_at].1
    }

    /// L1/R1 on the carousel: `by` is -1 or 1. Passes over empty shelves, and does nothing at all
    /// (no refusal) when there is nowhere to go.
    fn switch_shelf(&mut self, by: i32) {
        let Some(to) = self.next_shelf(by) else {
            return;
        };
        // A held direction or A belongs to the row being left, and would fire on coming back to it.
        self.shelf_mut().release_hold();
        self.play_held = None;
        self.shelf_at = to;
        // The old name's face goes at once, so the new one is what the slot shows first.
        self.shelf_platform = slot_ui::Printed::default();
        self.slot_letter = None;
        self.shelf_named = None;
        self.name_pending = true;
    }

    fn jump_letter(&mut self, dir: i32) {
        let before = self.shelf().index;
        match dir > 0 {
            true => self.shelf_mut().jump_next_letter(),
            false => self.shelf_mut().jump_prev_letter(),
        }
        if self.shelf().index == before {
            return;
        }
        let Some(letter) = self.selected_stem().map(slot_store::initial) else {
            return;
        };
        if self.slot_letter != Some(letter) {
            self.shelf_platform = slot_ui::Printed::default();
        }
        self.slot_letter = Some(letter);
        self.shelf_named = None;
        self.name_pending = true;
    }

    /// How strongly the shelf's name shows in the slot right now: zero once it has faded, and
    /// always zero on a card with one shelf, which never gets a face to show.
    fn slot_name_alpha(&self) -> f32 {
        let Some(at) = self.shelf_named else {
            return 0.0;
        };
        let t = self.now().saturating_sub(at);
        let level = if t < SLOT_NAME_IN_MS {
            t as f32 / SLOT_NAME_IN_MS as f32
        } else if t < SLOT_NAME_IN_MS + SLOT_NAME_HOLD_MS {
            1.0
        } else {
            let out = t - SLOT_NAME_IN_MS - SLOT_NAME_HOLD_MS;
            1.0 - (out as f32 / SLOT_NAME_OUT_MS as f32).min(1.0)
        };
        SLOT_NAME_ALPHA * ease(level)
    }

    /// The shelf `by` steps round the ring, skipping empty ones. `None` when there is nowhere else.
    fn next_shelf(&self, by: i32) -> Option<usize> {
        let n = self.shelves.len() as i32;
        (1..n)
            .map(|step| (self.shelf_at as i32 + by * step).rem_euclid(n) as usize)
            .find(|at| !self.shelves[*at].1.carts.is_empty())
    }

    /// Where the cart named `stem` stands: shelf and index. With `platform`, only that shelf is
    /// asked, since `Tetris.gb` and `Tetris.gba` are different games. Without it (a card from
    /// before shelves) the first shelf in ring order wins, which puts GBA first.
    fn seat_of(&self, stem: &str, platform: Option<Platform>) -> Option<(usize, usize)> {
        self.shelves
            .iter()
            .enumerate()
            .filter(|(_, (p, _))| platform.is_none_or(|want| *p == want))
            .find_map(|(at, (_, shelf))| {
                shelf
                    .carts
                    .iter()
                    .position(|c| c.stem == stem)
                    .map(|i| (at, i))
            })
    }

    /// Confirms the clock screen: sets the clock and offset, then goes to the shelf at first
    /// boot or back to the quick menu.
    pub fn confirm_clock(&mut self) {
        let Phase::SetClock {
            picker,
            seed,
            from_menu,
        } = &self.phase
        else {
            return;
        };
        // The platform is given UTC, which the base system and its ntp assume; the offset turns
        // it back into wall time.
        let (moved, offset, from_menu) = (picker.secs() - *seed, picker.offset_min(), *from_menu);
        // Only the change, on top of the running clock. Setting the picker's own reading would
        // lose the seconds past its minute and however long the screen was open.
        let utc = self.utc_secs() + moved;
        if let Some(power) = &mut self.power {
            power.set_clock(utc);
        }
        self.state.utc_offset_min = offset as i16;
        self.state.clock_set = true;
        self.persist();
        if from_menu {
            self.phase = Phase::QuickMenu {
                row: QuickRow::DateTime,
            };
        } else {
            self.start();
        }
    }

    /// Where the card is mounted. `None` only in tests that never touch one.
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// Local, not UTC. Everything that shows or stamps time reads this, so the offset is applied
    /// in one place.
    pub fn wall_secs(&self) -> i64 {
        self.utc_secs() + i64::from(self.state.utc_offset_min) * 60
    }

    /// The clock the card keeps, in UTC: the platform's once there is one, the host's before.
    fn utc_secs(&self) -> i64 {
        self.power.as_ref().map_or_else(system_secs, |p| p.now())
    }

    /// What the clock screen shows, or `None` off it. The binary rasterises from its text.
    pub fn picker(&self) -> Option<&ClockPicker> {
        match &self.phase {
            Phase::SetClock { picker, .. } => Some(picker),
            _ => None,
        }
    }

    /// The highlighted row while the quick menu is up.
    pub fn quick_menu(&self) -> Option<QuickRow> {
        match self.phase {
            Phase::QuickMenu { row } => Some(row),
            _ => None,
        }
    }

    /// What a quick menu row shows. `None` for Date & Time (the binary rasterises the clock) and
    /// About.
    pub fn quick_value(&self, row: QuickRow) -> Option<QuickValue> {
        match row {
            QuickRow::FastForward => QuickValue::speed(self.state.ff_speed),
            QuickRow::FastForwardSound => Some(QuickValue::flag(self.state.ff_sound)),
            QuickRow::ColourCorrection => Some(QuickValue::flag(self.state.colour_correction)),
            QuickRow::Rumble => Some(QuickValue::flag(self.state.rumble)),
            QuickRow::DateTime | QuickRow::About => None,
        }
    }

    pub fn set_quick_menu_faces(&mut self, faces: QuickMenuFaces) {
        self.quick_menu_faces = Some(faces);
    }

    /// Date & Time's value, grey and lit, each with the size it was rastered at.
    pub fn set_quick_clock_faces(&mut self, dim: (TexId, u32, u32), lit: (TexId, u32, u32)) {
        self.quick_clock_faces = Some([dim, lit]);
    }

    pub fn set_sticker_face(&mut self, face: TexId) {
        self.sticker_face = Some(face);
    }

    pub fn set_clock_faces(&mut self, line: TexId, hint: TexId) {
        self.clock_faces = Some((line, hint));
    }

    /// The GBA cart's outline in black, handed to every shelf for dimming its side carts.
    pub fn set_cart_shadow(&mut self, face: TexId) {
        for (_, shelf) in &mut self.shelves {
            shelf.set_shadow(face);
        }
    }

    /// The Game Boy paks' outlines in black, one per mould since they differ at the top corners
    /// (see `slot_ui::gb_cart_shadow`). Every shelf gets both and picks per cart.
    pub fn set_gb_cart_shadows(&mut self, notched: TexId, rounded: TexId) {
        for (_, shelf) in &mut self.shelves {
            shelf.set_gb_shadow(GbShell::Notched, notched);
            shelf.set_gb_shadow(GbShell::Rounded, rounded);
        }
    }

    pub fn set_wallpaper(&mut self, face: TexId) {
        self.wallpaper = Some(face);
    }

    pub fn set_bolt_face(&mut self, bolt: TexId) {
        self.bolt = Some(bolt);
    }

    /// The shelves' marks, in `Platform::ALL` order. Uploaded once at boot.
    pub fn set_shelf_platform_face(&mut self, face: TexId, w: u32) {
        self.shelf_platform = slot_ui::Printed::new(face, w);
    }

    /// The band stops naming a shelf: a card that lost a platform while running, or one that
    /// only ever had the one.
    pub fn clear_shelf_platform(&mut self) {
        self.shelf_platform = slot_ui::Printed::default();
    }

    pub fn slot_text(&self) -> Option<String> {
        match self.slot_letter {
            Some(letter) => Some(letter.to_string()),
            None => self.shelf_platform_name().map(str::to_string),
        }
    }

    /// Which machine the band should name, or `None` on a card with one shelf.
    pub fn shelf_platform_name(&self) -> Option<&'static str> {
        self.next_shelf(1)?;
        Some(self.shelves[self.shelf_at].0.name())
    }

    pub fn set_battery_percent_face(&mut self, face: TexId, w: u32) {
        self.battery_percent = slot_ui::Printed::new(face, w);
    }

    pub fn set_shelf_clock_face(&mut self, face: TexId, w: u32) {
        self.shelf_clock = slot_ui::Printed::new(face, w);
    }

    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    /// The whole library, every shelf in ring order. Use this, not `shelf()`, to look a cart up by
    /// name.
    pub fn carts(&self) -> impl Iterator<Item = &Cart> {
        self.shelves
            .iter()
            .flat_map(|(_, shelf)| shelf.carts.iter())
    }

    /// The cartridge in the slot, on every screen that has one. Looked up on the showing shelf, not
    /// with `carts()`, which answers `Tetris.gba` for `Tetris.gb`. The shelf cannot change while a
    /// cart is in the slot.
    pub fn seated_cart(&self) -> Option<&Cart> {
        let stem = match &self.phase {
            Phase::Inserting { cart, .. }
            | Phase::Playing { cart }
            | Phase::Ejecting { cart, .. }
            | Phase::Polaroids { cart } => cart,
            Phase::Doze { cart: Some(cart) } => cart,
            _ => return None,
        };
        self.shelf().carts.iter().find(|c| c.stem == *stem)
    }

    /// Exactly one cart on the card, counting every shelf. The shelf is unreachable and eject is
    /// refused.
    pub fn single_cart(&self) -> bool {
        self.carts().count() == 1
    }

    /// Face textures in `carts` order, which is every shelf's carts end to end. Handed out again
    /// the same way, so each shelf gets its own and only its own. Only the compositor can mint a
    /// `TexId`.
    pub fn set_faces(&mut self, faces: Vec<TexId>) {
        let mut faces = faces.into_iter();
        for (_, shelf) in &mut self.shelves {
            let n = shelf.carts.len();
            shelf.set_faces(faces.by_ref().take(n).collect());
        }
    }

    /// Handed over when the core is spawned on the way into the slot.
    pub fn set_snapshot(&mut self, snapshot: Box<dyn Snapshot>) {
        self.snapshot = Some(snapshot);
    }

    /// The `Core` `session.rs` resolved for the cart it just spawned. Called with `set_snapshot`.
    pub fn set_core(&mut self, core: Core) {
        self.core = core;
    }

    /// The seated cart's `Platform`, from the `Cart` its core was spawned for. Called with
    /// `set_core`, so saves are filed under it rather than re-derived from the stem.
    pub fn set_platform(&mut self, platform: Platform) {
        self.platform = platform;
    }

    /// Whether the dylib `core` names actually opened (see `crate::core::Opened`). Stops a card
    /// missing a core file from retiring every cart's resume.
    pub fn set_named_core(&mut self, named: bool) {
        self.named_core = named;
    }

    /// The seated cart's picture mode, off the card, handed over with `core` and `platform`.
    pub fn set_video_mode(&mut self, mode: VideoMode) {
        self.video_mode = mode;
    }

    /// The part of the frame buffer the panel shows. `slot_gfx::WHOLE_TEXTURE` for GBA carts and
    /// for Game Boy carts at actual size.
    pub fn source_rect(&self) -> [f32; 4] {
        video_mode::source_rect(self.platform, self.video_mode)
    }

    /// Whether L and R are slot's, for the picture, rather than the game's. Only while playing a
    /// Game Boy or Colour cart, which had no shoulder buttons.
    fn slot_owns_the_shoulders(&self) -> bool {
        matches!(self.phase, Phase::Playing { .. }) && self.platform != Platform::Gba
    }

    /// The buttons slot holds right now, which the core must not be given. Changes with phase and
    /// platform under a held finger, so `Session::sync_pad` asks rather than waiting for a press.
    pub fn taken_buttons(&self) -> &'static [Btn] {
        if self.slot_owns_the_shoulders() {
            &[Btn::L1, Btn::R1]
        } else {
            &[]
        }
    }

    /// Whether slot takes this action, so the core must not also get it. The same predicate decides
    /// in `apply`, so a button cannot be both acted on and passed on.
    pub fn takes_from_the_game(&self, action: Action) -> bool {
        match action {
            Action::GbaDown(btn) | Action::GbaUp(btn) => self.taken_buttons().contains(&btn),
            _ => false,
        }
    }

    /// L or R, acted on and written down, with no toast. A press that changes nothing writes
    /// nothing: no line and `actual` mean the same.
    fn set_picture(&mut self, mode: VideoMode) {
        if self.video_mode == mode {
            return;
        }
        self.video_mode = mode;
        let (Some(root), Phase::Playing { cart }) = (self.root.clone(), &self.phase) else {
            return;
        };
        // Best effort: without the card the picture still changes, it just is not remembered.
        if let Err(e) = video_mode::write_video_mode(&root, cart, mode) {
            eprintln!("slot: video: could not write video_mode.ini: {e}");
        }
    }

    /// The `gpsp_serial` the just-spawned core was loaded with, so a picked link is compared
    /// against what the core actually got.
    pub fn set_link_loaded(&mut self, serial: &'static str) {
        self.link_loaded = Some(serial);
    }

    /// A cart's link hardware and the `gpsp_serial` to load it with: SELECT's last choice, else
    /// gpSP's pick. Core loads and picked links both read this so they cannot disagree. An unknown
    /// cart links by cable.
    pub fn link_mode(&self, stem: &str) -> (LinkKind, &'static str) {
        let Some((cart, auto)) = self.auto_link(stem) else {
            return (LinkKind::Cable, "auto");
        };
        let chosen = self.link_choices.get(stem).copied().unwrap_or(auto);
        (chosen, serial_option(chosen, auto, &cart.code, &cart.title))
    }

    /// Whether a netpacket session is live. libretro forbids rewind, state loads and fast forward
    /// during one, since they desync the peer with no way back.
    pub fn link_active(&self) -> bool {
        self.link.is_some()
    }

    /// Begins a session. `client_id` is libretro's: 0 host, 1 joiner. Bookkeeping only; the
    /// transport and core live on the emulator thread. A session starts from the battery save.
    pub fn begin_link(&mut self, client_id: u16) {
        self.link = Some(LinkSession {
            client_id,
            lost_at: None,
        });
        self.sync_link_badge();
    }

    /// This device's side of the session, or `None` with none live.
    pub fn link_client_id(&self) -> Option<u16> {
        self.link.as_ref().map(|s| s.client_id)
    }

    /// libretro's netpacket contract forbids it during a session: it desyncs the peer.
    pub fn may_rewind(&self) -> bool {
        !self.link_active()
    }

    /// Same hazard as rewinding: it moves this machine to a moment the peer cannot follow.
    pub fn may_load_state(&self) -> bool {
        !self.link_active()
    }

    /// Runs this machine ahead of the peer. libretro.h lists fast forward among what a session
    /// disables.
    pub fn may_fast_forward(&self) -> bool {
        !self.link_active()
    }

    /// Ends the session and leaves the cart playing single player. Bookkeeping only:
    /// `Session::act` watches `link_active()` around every `apply` and mirrors an ending onto
    /// `EmuHandle::end_link()`. Anything ending a session outside `apply` must do that itself.
    pub fn end_link(&mut self) {
        self.link = None;
        // The core stays in link mode until reopened, but the next cart must not load for one.
        self.link_player = None;
        self.sync_link_badge();
        // `down` ends the session's network and cools where BaseOS can; `cool` covers the one
        // that cannot. Queued: a teardown shells out for a second or two.
        self.radio.ask(RadioJob::Down);
        self.radio.ask(RadioJob::Cool);
    }

    /// The emulator thread found the transport closed. `timers` ends the session after
    /// `LINK_LOST_MS` of broken badge.
    pub fn peer_lost(&mut self) {
        if matches!(self.game_menu, Some(GameMenu::Linked { .. })) {
            self.game_menu = None;
        }
        let now = self.now();
        if let Some(session) = &mut self.link {
            session.lost_at.get_or_insert(now);
        }
        self.sync_link_badge();
    }

    /// The host runs another GBA BIOS. Ends like `peer_ended`, saying why.
    pub fn bios_mismatch(&mut self) {
        if !self.link_active() {
            return;
        }
        let role = self
            .link_client_id()
            .map_or(self.last_role, LinkRow::from_client_id);
        self.end_link();
        self.hud.toast(Toast::BiosMismatch, self.now());
        self.unplug(role);
    }

    /// The other player ended the link and said so. Ends now, with the same teardown as
    /// `end_link_from_menu`. `peer_lost` still covers a crash or going out of range.
    pub fn peer_ended(&mut self) {
        if !self.link_active() {
            return;
        }
        // Read before `end_link` clears it.
        let role = self
            .link_client_id()
            .map_or(self.last_role, LinkRow::from_client_id);
        self.end_link();
        self.hud.toast(Toast::PeerEnded, self.now());
        // The same unplug the other device plays: one cable cannot stay in at one end. Replaces
        // whatever screen was up, usually none.
        self.unplug(role);
    }

    pub fn link_badge(&self) -> LinkBadge {
        self.hud.link()
    }

    pub fn set_link_badge_faces(&mut self, faces: Vec<TexId>) {
        self.hud.set_link_faces(faces);
    }

    fn sync_link_badge(&mut self) {
        let badge = match &self.link {
            None => LinkBadge::Off,
            Some(s) => match (s.client_id == 0, s.lost_at.is_some()) {
                (true, false) => LinkBadge::Hosting,
                (false, false) => LinkBadge::Joined,
                (true, true) => LinkBadge::HostingLost,
                (false, true) => LinkBadge::JoinedLost,
            },
        };
        self.hud.set_link(badge);
    }

    /// The sound the app wants, for whoever owns the device.
    pub fn take_sfx(&mut self) -> Option<Sfx> {
        self.sfx.take()
    }

    /// The panel comes up at the card's remembered level, not the kernel's.
    pub fn set_power(&mut self, mut power: Power) {
        power.set_backlight(self.state.brightness);
        // The first moment the device's own clock can be read. Boot trusted `clock_set`, so a
        // dead RTC has to be caught here or it never reaches the clock screen.
        let secs = power.now();
        if matches!(self.phase, Phase::SetClock { .. }) || secs < CLOCK_FLOOR {
            self.phase = clock_screen(secs, 0, false);
        }
        self.power = Some(power);
        // Nothing was read or written before this, so the first tick must poll and set the LED
        // at once, or the band sits blank and the LED stale for a whole poll.
        self.battery_at = self.now();
        self.charge_at = self.now();
        self.last_led = None;
    }

    /// The motor. Never persisted: it belongs to the cart that asked for it.
    pub fn set_rumble(&mut self, strength: u16) {
        if let Some(power) = &mut self.power {
            power.set_rumble(strength);
        }
    }

    /// Whether the quick menu allows rumble. `Session::sync_rumble` enforces it.
    pub fn rumble_enabled(&self) -> bool {
        self.state.rumble
    }

    /// Core frames per present while fast forwarding, as the quick menu chose.
    pub fn ff_speed(&self) -> u8 {
        self.state.ff_speed
    }

    /// Whether fast forward is heard, sped up, rather than dropped.
    pub fn ff_sound(&self) -> bool {
        self.state.ff_sound
    }

    /// Whether cores loaded from here on tint for the console LCD. Read by `Session::spawn_core`,
    /// since a core reads options only at open.
    pub fn colour_correction(&self) -> bool {
        self.state.colour_correction
    }

    /// Set by the doze timeout and a graceful power off, once everything durable is written.
    pub fn powering_off(&self) -> bool {
        self.powering_off
    }

    /// Waits until the ordinary loop has presented the shutdown screen. An out-of-band draw and
    /// swap can block on a GPU about to be torn down and hang the device before `poweroff`.
    pub fn ready_to_power_off(&self) -> bool {
        self.powering_off && self.now() >= self.act_at
    }

    pub fn ready_to_restart(&self) -> bool {
        self.restarting && self.now() >= self.act_at
    }

    /// Whether the shutdown screen should show. True from the choice, before `powering_off`.
    pub fn shutting_down(&self) -> bool {
        self.powering_off || self.restarting
    }

    /// Set by the menu's Restart. Takes the same shutdown path (busybox init runs rcK for a
    /// reboot too), so the GPU module is unloaded and the hardware does not hang with rails up.
    pub fn restarting(&self) -> bool {
        self.restarting
    }

    pub fn restart(&mut self) {
        if let Some(power) = &mut self.power {
            power.restart();
        }
    }

    pub fn power_menu(&self) -> Option<usize> {
        self.power_menu
    }

    /// The core the chip is in or heading for. Still `Some` while the lid goes back on.
    pub fn core_picker(&self) -> Option<Core> {
        self.core_picker.map(|p| p.seat())
    }

    /// The chip's pose this frame, for whatever draws it.
    pub fn core_picker_chip(&self) -> Option<Chip> {
        self.core_picker.map(|p| p.chip(self.now()))
    }

    /// Whether the in-game menu is up. The game underneath is paused while it is.
    pub fn game_menu_open(&self) -> bool {
        self.game_menu.is_some()
    }

    /// Which in-game menu screen is up.
    pub fn game_menu(&self) -> Option<GameMenu> {
        self.game_menu
    }

    /// The wire a link that just came up runs over, taken once by the emulator thread's owner.
    pub fn take_link_transport(&mut self) -> Option<(u16, Box<dyn LinkChannel>)> {
        self.link_transport.take()
    }

    /// The reload a picked link waits on (cart, `gpsp_serial`), taken once. Answered with
    /// `link_reload_done` or `link_reload_failed`.
    pub fn take_link_reload(&mut self) -> Option<(String, &'static str)> {
        self.link_reload.take()
    }

    /// The port a cable session is loaded for. Not taken: a reload re-reads it.
    pub fn link_player(&self) -> Option<u8> {
        self.link_player
    }

    /// A colour correction change for the running core, taken once by `Session::update`.
    pub fn take_colour_correction(&mut self) -> Option<bool> {
        self.colour_pending.take()
    }

    /// The seated cart's core, so `Session` can name options for the core actually running.
    pub fn core(&self) -> Core {
        self.core
    }

    /// The game is loaded again. After a switch, the link starts in the picked role unless B
    /// cancelled it. After a failed switch that fell back, the choice is reverted and refused.
    pub fn link_reload_done(&mut self) {
        let Some(reload) = self.reload.take() else {
            return;
        };
        if reload.fallback {
            self.link_choices.insert(reload.stem, reload.from);
            self.close_game_menu();
            return self.refuse();
        }
        if reload.cancelled {
            return self.close_game_menu();
        }
        let since = match self.game_menu {
            Some(GameMenu::Working { since, .. }) => since,
            _ => self.now(),
        };
        self.start_link_from(
            LinkStarter::spawn(reload.role.role(), link_port()),
            reload.role.client_id(),
            since,
        );
    }

    /// The game would not load in the new mode. Asks for the previous mode, which just loaded;
    /// if that fails too, the cart is refused out of the slot rather than seated with no core.
    pub fn link_reload_failed(&mut self) {
        let Some(mut reload) = self.reload.take() else {
            return;
        };
        if !reload.fallback {
            self.link_reload = Some((reload.stem.clone(), reload.from_serial));
            reload.fallback = true;
            self.reload = Some(reload);
            return;
        }
        self.link_choices.insert(reload.stem, reload.from);
        self.refuse_seated();
    }

    /// The highlighted cart, or `None` on an empty shelf.
    pub fn selected_stem(&self) -> Option<&str> {
        self.shelf()
            .carts
            .get(self.shelf().index)
            .map(|c| c.stem.as_str())
    }

    /// The cached reading. `None` until the first slow tick, or with no gauge.
    pub fn battery(&self) -> Option<Battery> {
        self.battery
    }

    /// Does not return when there is a platform. In unit tests the flag is all there is.
    pub fn poweroff(&mut self) {
        if let Some(power) = &mut self.power {
            power.poweroff();
        }
    }

    /// The face buttons belong to whatever is on screen. While playing they are the game's and
    /// the app sees only gestures.
    pub fn apply(&mut self, action: Action) {
        // First, even before device keys: nothing else should happen while the user decides.
        if self.power_menu.is_some() {
            match action {
                Action::LidClose => return self.doze(),
                Action::LidOpen => return self.wake(),
                _ => return self.power_menu_input(action),
            }
        }
        // Lid, light and sound belong to the device, so they come before the phase.
        match action {
            Action::LidClose => return self.doze(),
            Action::LidOpen => return self.wake(),
            Action::PowerPress => {
                // A live session ends here rather than flushing: this is the one button a
                // trade partner mid-exchange can reach.
                if self.link_active() {
                    self.end_link();
                    return;
                }
                // Light a dark panel on the press, or a hold raises the menu unseen and the user
                // holds on into the PMIC's six second cut. A press may light, never darken.
                self.woke_on_press = matches!(self.phase, Phase::Doze { .. });
                if self.woke_on_press {
                    self.wake();
                }
                return self.flush_resume();
            }
            Action::PowerTap => return self.power_press(),
            Action::PowerHold => return self.open_power_menu(),
            // The hold raises the menu; the choice there is made with A.
            Action::PowerOff => return,
            _ => {}
        }
        // Before the levels: the clock owns all four directions, and at first boot there is no
        // way back from it.
        if let Phase::SetClock {
            picker, from_menu, ..
        } = &mut self.phase
        {
            match action {
                Action::GbaDown(Btn::Left) | Action::ShelfLeft => picker.left(),
                Action::GbaDown(Btn::Right) | Action::ShelfRight => picker.right(),
                Action::GbaDown(Btn::Up) => picker.up(),
                Action::GbaDown(Btn::Down) => picker.down(),
                Action::GbaDown(Btn::A) | Action::Insert => self.confirm_clock(),
                // Only the way in from the quick menu has a way back, and it changes nothing.
                Action::GbaDown(Btn::B) if *from_menu => {
                    self.phase = Phase::QuickMenu {
                        row: QuickRow::DateTime,
                    }
                }
                _ => {}
            }
            return;
        }
        if self.adjust(action) {
            return;
        }
        // Releases reach the shelf on any screen, or a direction let go during an insert stays
        // held.
        match action {
            Action::GbaUp(Btn::Left) => self.shelf_mut().release_left(),
            Action::GbaUp(Btn::Right) => self.shelf_mut().release_right(),
            _ => {}
        }
        // Before the phase match, so it and the power menu never both take a press. Below the
        // device keys, which keep working over a running game.
        if self.game_menu.is_some() {
            return self.game_menu_input(action);
        }
        let now = self.now();
        match self.phase {
            Phase::Shelf => match action {
                // START, not SELECT: SELECT is the chord key for brightness and blue light, so
                // opening on it would eat or delay those chords. START is unbound here.
                Action::GbaDown(Btn::Start) if self.core_picker.is_none() => {
                    self.open_core_picker()
                }
                // Before the shelf's own movement, so an open picker takes the arrows.
                _ if self.core_picker.is_some() => self.core_picker_input(action),
                // Up and Down jump a letter at a time. SELECT+Up is brightness, handled first.
                Action::GbaDown(Btn::Up) => self.jump_letter(-1),
                Action::GbaDown(Btn::Down) => self.jump_letter(1),
                Action::ShelfLeft | Action::GbaDown(Btn::Left) => self.shelf_mut().hold_left(now),
                Action::ShelfRight | Action::GbaDown(Btn::Right) => {
                    self.shelf_mut().hold_right(now)
                }
                Action::QuickMenu => self.open_quick_menu(),
                // A tap inserts on release; a hold that got there first has already taken it.
                Action::GbaDown(Btn::A) => self.play_held = Some(now),
                Action::GbaUp(Btn::A) => {
                    if self.play_held.take().is_some() {
                        self.insert(false);
                    }
                }
                Action::Insert => self.insert(false),
                // The shoulders move the carousel between shelves. Only here: in a game they
                // are the GBA's own L and R, and the row must never turn over under one.
                Action::GbaDown(Btn::L1) => self.switch_shelf(-1),
                Action::GbaDown(Btn::R1) => self.switch_shelf(1),
                _ => {}
            },
            // Eject reaches an insert too, so a cart whose core never arrived can be got out.
            Phase::Inserting { .. } if action == Action::Eject => self.eject(),
            Phase::Playing { .. } => match action {
                // Shoulders this cart's console never had. Same test as `takes_from_the_game`.
                Action::GbaDown(Btn::L1) if self.slot_owns_the_shoulders() => {
                    self.set_picture(VideoMode::Stretch)
                }
                Action::GbaDown(Btn::R1) if self.slot_owns_the_shoulders() => {
                    self.set_picture(VideoMode::Actual)
                }
                Action::Eject => self.eject(),
                Action::GameMenu => self.game_menu_shortcut(),
                Action::Polaroids => self.open_polaroids(),
                Action::SaveState => self.save_state(),
                Action::LoadState => self.load_newest(),
                // libretro forbids rewinding during a session. Refused so the press reads as
                // answered.
                Action::RewindStart if !self.may_rewind() => self.refuse(),
                // `Session::sync_speed` is what withholds `Speed::Fast`; this is only the shake.
                Action::FfStart if !self.may_fast_forward() => self.refuse(),
                _ => {}
            },
            Phase::Polaroids { .. } => match action {
                Action::ShelfLeft | Action::GbaDown(Btn::Left) => self.flick(Polaroids::left),
                Action::ShelfRight | Action::GbaDown(Btn::Right) => self.flick(Polaroids::right),
                Action::GbaDown(Btn::A) => self.load_selected(),
                Action::GbaDown(Btn::B) | Action::Polaroids => self.close_polaroids(),
                // X and Y are free everywhere: the GBA has neither.
                Action::GbaDown(Btn::X) => self.undo(self.now()),
                Action::GbaDown(Btn::Y) => self.delete_selected(),
                _ => {}
            },
            // Back to the quick menu on the row that opened it. MENU works as well as B.
            Phase::About if action == Action::GbaDown(Btn::B) || action == Action::QuickMenu => {
                self.phase = Phase::QuickMenu {
                    row: QuickRow::About,
                }
            }
            Phase::QuickMenu { row } => self.quick_menu_input(row, action),
            _ => {}
        }
    }

    /// MENU on the carousel. Always opens on the top row.
    fn open_quick_menu(&mut self) {
        self.phase = Phase::QuickMenu {
            row: QuickRow::ALL[0],
        };
    }

    /// Up/Down move the bar and stop at the ends, Left/Right change the row, A opens, MENU or B
    /// returns to the carousel.
    fn quick_menu_input(&mut self, row: QuickRow, action: Action) {
        let row = match action {
            Action::GbaDown(Btn::Up) => row.up(),
            Action::GbaDown(Btn::Down) => row.down(),
            Action::GbaDown(Btn::Left) => return self.change_setting(row, false),
            Action::GbaDown(Btn::Right) => return self.change_setting(row, true),
            Action::GbaDown(Btn::A) => return self.open_quick_row(row),
            Action::GbaDown(Btn::B) | Action::QuickMenu => {
                self.phase = Phase::Shelf;
                return;
            }
            _ => return,
        };
        self.phase = Phase::QuickMenu { row };
    }

    /// A on a row. Only Date & Time and About open anything.
    fn open_quick_row(&mut self, row: QuickRow) {
        match row {
            QuickRow::DateTime => {
                // Seeded from the current clock and offset: this is a correction.
                self.phase = clock_screen(self.utc_secs(), self.state.utc_offset_min, true);
            }
            QuickRow::About => self.phase = Phase::About,
            QuickRow::FastForward
            | QuickRow::FastForwardSound
            | QuickRow::ColourCorrection
            | QuickRow::Rumble => {}
        }
    }

    /// Left or Right on the highlighted row. Takes effect and persists at once; a press against an
    /// end writes nothing.
    fn change_setting(&mut self, row: QuickRow, right: bool) {
        let s = &mut self.state;
        match row {
            QuickRow::FastForward => {
                let to = ff_next(s.ff_speed, right);
                if to == s.ff_speed {
                    return;
                }
                s.ff_speed = to;
            }
            // Two values each, so either arrow toggles.
            QuickRow::FastForwardSound => s.ff_sound = !s.ff_sound,
            QuickRow::ColourCorrection => {
                s.colour_correction = !s.colour_correction;
                // Also sent to the running core, or it would only apply at the next insert.
                self.colour_pending = Some(s.colour_correction);
            }
            QuickRow::Rumble => s.rumble = !s.rumble,
            QuickRow::DateTime | QuickRow::About => return,
        }
        self.persist();
    }

    /// Applied at a stated moment. The clock is set, not advanced.
    pub fn apply_at(&mut self, action: Action, now: Millis) {
        self.clock = now as f64;
        self.apply(action);
    }

    /// `true` if the action was one of the three levels, moved or not. A press at an end still
    /// shows the bar.
    fn adjust(&mut self, action: Action) -> bool {
        if action == Action::MuteToggle {
            self.mute_toggle();
            return true;
        }
        // TEMPORARY. Through `change_setting` so the shortcut and the menu row cannot drift.
        if action == Action::ColourCorrectionToggle {
            self.change_setting(QuickRow::ColourCorrection, true);
            let said = match self.state.colour_correction {
                true => Toast::ColourOn,
                false => Toast::ColourOff,
            };
            self.hud.toast(said, self.now());
            return true;
        }
        let s = &self.state;
        let (kind, value) = match action {
            Action::BrightnessUp => (HudKind::Brightness, up(s.brightness, 1, BRIGHTNESS_MAX)),
            Action::BrightnessDown => (HudKind::Brightness, s.brightness.saturating_sub(1)),
            Action::BlueLightUp => (HudKind::BlueLight, up(s.blue_light, 1, BLUE_LIGHT_MAX)),
            Action::BlueLightDown => (HudKind::BlueLight, s.blue_light.saturating_sub(1)),
            Action::VolumeUp => (HudKind::Volume, up(s.volume, VOLUME_STEP, VOLUME_MAX)),
            Action::VolumeDown => (HudKind::Volume, s.volume.saturating_sub(VOLUME_STEP)),
            _ => return false,
        };
        if kind == HudKind::Volume {
            self.remember_volume();
        }
        let level = match kind {
            HudKind::Brightness => &mut self.state.brightness,
            HudKind::BlueLight => &mut self.state.blue_light,
            HudKind::Volume => &mut self.state.volume,
            // Rewind shares the bar but is never an action.
            HudKind::Rewind => return false,
        };
        let moved = *level != value;
        *level = value;
        // Changing the volume unmutes.
        let unmuted = kind == HudKind::Volume && std::mem::take(&mut self.state.muted);
        let (shown, now) = (self.hud_value(kind, value), self.now());
        self.hud.show(kind, shown, self.state.muted, now);
        if let (HudKind::Brightness, Some(power)) = (kind, &mut self.power) {
            power.set_backlight(value);
        }
        // A key held against an end would otherwise rewrite the file at the repeat rate.
        if moved || unmuted {
            self.persist();
        }
        true
    }

    /// Where the volume stood before this press. Two are kept: a chord is two presses.
    fn remember_volume(&mut self) {
        if self.vol_before.len() == 2 {
            self.vol_before.remove(0);
        }
        self.vol_before
            .push((self.state.volume, self.state.muted, self.now()));
    }

    /// Mute is a state, not a level. Both chord keys fire their own adjustment first, and from an
    /// end those do not cancel, so they are rolled back here.
    fn mute_toggle(&mut self) {
        let now = self.now();
        if let Some((volume, muted, _)) = self
            .vol_before
            .iter()
            .find(|(_, _, at)| now.saturating_sub(*at) <= MUTE_CHORD_MS)
            .copied()
        {
            self.state.volume = volume;
            self.state.muted = muted;
        }
        self.vol_before.clear();
        self.state.muted = !self.state.muted;
        self.hud
            .show(HudKind::Volume, self.output_volume(), self.state.muted, now);
        self.persist();
    }

    /// What the bar reads. Muted draws as an empty bar under the muted glyph.
    fn hud_value(&self, kind: HudKind, value: u8) -> u8 {
        match kind {
            HudKind::Volume => self.output_volume(),
            _ => value,
        }
    }

    /// Pushed in, since only the emulator knows how much history is left. Held until
    /// `hide_rewind`.
    pub fn show_rewind(&mut self, fill: u8) {
        let now = self.now();
        // Rewind cannot be muted, so never the muted glyph.
        self.hud.show(HudKind::Rewind, fill, false, now);
    }

    pub fn hide_rewind(&mut self) {
        self.hud.release_rewind();
    }

    /// Pushed in: held and latched are one action each to the app but look different.
    pub fn set_ff(&mut self, ff: FfState) {
        self.hud.set_ff(ff);
    }

    pub fn ff_badge(&self) -> Option<Icon> {
        self.hud.badge()
    }

    pub fn blue_light(&self) -> u8 {
        self.state.blue_light
    }

    pub fn brightness(&self) -> u8 {
        self.state.brightness
    }

    /// The chosen level, which a mute does not touch.
    pub fn volume(&self) -> u8 {
        self.state.volume
    }

    pub fn muted(&self) -> bool {
        self.state.muted
    }

    /// What the sink is set to. The only one the audio path may read: muted at 70 is silent.
    pub fn output_volume(&self) -> u8 {
        if self.state.muted {
            0
        } else {
            self.state.volume
        }
    }

    pub fn hud_icon(&self) -> Icon {
        self.hud.glyph()
    }

    pub fn now(&self) -> Millis {
        self.clock as Millis
    }

    fn flick(&mut self, step: fn(&mut Polaroids)) {
        if let Some(p) = &mut self.polaroids {
            step(p);
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.clock += dt as f64 * 1000.0;
        if self.name_pending && self.shelf_platform.face.is_some() {
            self.name_pending = false;
            self.shelf_named = Some(self.now());
        }
        self.timers();
        // A queue poll, not a syscall, so it is cheap every frame.
        self.poll_link();
        let now = self.now();
        // The cart opens once its board is on the GPU, so a slow build pauses on the shelf
        // rather than eating the animation.
        let ready = self.core_faces_ready();
        if let Some(picker) = &mut self.core_picker {
            if picker.waiting() && (ready || picker.waited(now) >= FACES_WAIT_MS) {
                picker.start(now);
            }
        }
        // The lid is back on.
        if self.core_picker.is_some_and(|p| p.finished(now)) {
            self.core_picker = None;
        }
        // A direction held as the shelf leaves the screen is not held when it returns.
        if !self.on_shelf() {
            self.shelf_mut().release_hold();
        }
        let mut touched = false;
        // Read out before the phase is borrowed, so the shelf on screen can still be reached
        // from inside the match below.
        let at = self.shelf_at;
        let next = match &mut self.phase {
            Phase::Shelf => {
                // Only the shelf being looked at. The others are exactly as they were left,
                // repeat and spring included, until the carousel comes back to them.
                let shelf = &mut self.shelves[at].1;
                shelf.tick(now);
                shelf.update(dt);
                None
            }
            Phase::Inserting {
                cart,
                t,
                core_ready,
                resumed,
                ..
            } => {
                let was = *t;
                *t += dt;
                // Started early so the clip's contact lands on the seating frame. A resumed cart
                // never travelled.
                let at = SEATED_AT - Sfx::Insert.lead();
                touched = !*resumed && was < at && *t >= at;
                (*t >= INSERT_S && *core_ready).then(|| Phase::Playing {
                    cart: std::mem::take(cart),
                })
            }
            // The panel goes out before the cart travels. `t` starts below zero for the pause,
            // and the contacts let go as the cart starts moving.
            Phase::Ejecting { t, .. } => {
                if self.screen <= 0.0 {
                    let was = *t;
                    *t += dt;
                    touched = was < 0.0 && *t >= 0.0;
                }
                (*t >= EJECT_S).then_some(Phase::Shelf)
            }
            _ => None,
        };
        // One clip for the whole movement, only its start is timed.
        if touched {
            self.sfx = Some(match self.phase {
                Phase::Ejecting { .. } => Sfx::Eject,
                _ => Sfx::Insert,
            });
        }
        if let Some(phase) = next {
            // Before the screen step, so the arrival frame is already the first power-on frame.
            self.phase = phase;
            // The cart is out; its refusal goes with it.
            self.refused_from = None;
            // `slot.state` mirrors the slot: seated on the way in, empty on the way back out.
            let seated = match &self.phase {
                Phase::Playing { cart } => Some(cart.clone()),
                _ => None,
            };
            // `Session` drops the core with the cart, and what it was loaded with goes too.
            if seated.is_none() {
                self.link_loaded = None;
            }
            self.record_cart(seated);
        }
        self.step_screen(dt);
    }

    /// The game layer's power, driven by the phase: insert, resume and wake all bring it up alike.
    fn step_screen(&mut self, dt: f32) {
        let lit = matches!(self.phase, Phase::Playing { .. } | Phase::Polaroids { .. });
        let step = if lit {
            dt / POWER_ON_S
        } else {
            -dt / POWER_OFF_S
        };
        self.screen = (self.screen + step).clamp(0.0, 1.0);
    }

    /// 0.0 dark, 1.0 fully on. Nothing may draw the game while it is zero.
    pub fn screen_power(&self) -> f32 {
        self.screen
    }

    pub fn set_game_ready(&mut self, ready: bool) {
        self.game_ready = ready;
    }

    /// Whether the draw list carries the game layer. A core that has published nothing would
    /// otherwise show the last cart's final frame.
    pub fn game_visible(&self) -> bool {
        self.game_ready && self.screen > 0.0
    }

    /// Jumps the clock without advancing animations, for tests spanning minutes.
    pub fn tick_ms(&mut self, now: Millis) {
        self.clock = self.clock.max(now as f64);
        self.timers();
    }

    /// Everything the clock alone drives.
    fn timers(&mut self) {
        self.play_hold();
        // The grace period can run out with the switcher open.
        let offer = self.undo_label();
        if let Some(p) = &mut self.polaroids {
            p.set_undo(offer);
        }
        if self.doze_expired() {
            self.on_doze_timeout();
        }
        if self.now() >= self.autosave_at {
            self.flush_resume();
        }
        if self.now() >= self.battery_at {
            self.battery_at = self.now() + BATTERY_POLL_MS;
            self.battery = self.power.as_ref().and_then(|p| p.battery());
            if let Some(b) = self.battery {
                self.on_battery(b);
            }
        }
        // Only the charge half. The percent beside it is at most one slow tick old.
        if self.now() >= self.charge_at {
            self.charge_at = self.now() + CHARGE_POLL_MS;
            if let (Some(power), Some(b)) = (self.power.as_ref(), self.battery.as_mut()) {
                b.charge = power.charge();
            }
            // On the fast tick: an amber LED lagging the cable by ten seconds is worse than none.
            let state = self.led_state();
            self.set_led(state);
        }
        // LINKED has been shown long enough.
        if let Some(GameMenu::Linked {
            since,
            opened: false,
            ..
        }) = self.game_menu
        {
            if self.now().saturating_sub(since) >= LINKED_HOLD_MS {
                self.game_menu = None;
            }
        }
        // Straight to `None`, not `close_game_menu`: that would queue a second radio cool behind
        // `end_link`'s `down` (checked by `a_ends_the_session_and_says_so`).
        if let Some(GameMenu::Unplug { since, .. }) = self.game_menu {
            if self.now().saturating_sub(since) >= UNPLUG_HOLD_MS {
                self.game_menu = None;
            }
        }
        // A lost peer's session ends once the broken badge has been seen.
        if let Some(at) = self.link.as_ref().and_then(|s| s.lost_at) {
            if self.now().saturating_sub(at) >= LINK_LOST_MS {
                self.end_link();
            }
        }
    }

    /// Modelled on the OG SP: green running, red low, amber charging, green when full. Charging
    /// outranks low.
    pub fn led_state(&self) -> LedState {
        let Some(b) = self.battery else {
            return LedState::Running;
        };
        match b.charge {
            Charge::Charging => LedState::Charging,
            Charge::Full => LedState::Charged,
            _ if b.percent <= BATTERY_LOW => LedState::Low,
            _ => LedState::Running,
        }
    }

    /// The only call to the platform's `set_led`, so the dedup in `last_led` cannot be bypassed.
    fn set_led(&mut self, state: LedState) {
        // A shutdown darkens the case for good. Without this a charge tick between the choice
        // and `poweroff` turns it green again for the five seconds rcK takes.
        if self.shutting_down() && state != LedState::Off {
            return;
        }
        if self.last_led == Some(state) {
            return;
        }
        self.last_led = Some(state);
        if let Some(power) = self.power.as_mut() {
            power.set_led(state);
        }
    }

    fn record_cart(&mut self, cart: Option<String>) {
        // Off the same cartridge as the stem, so the two lines cannot disagree. Not
        // `self.platform`: with `SLOT_NO_CORE=1` nothing writes it.
        let platform = self.seated_cart().map(|c| c.platform);
        if self.state.cart == cart && self.state.cart_platform == platform {
            return;
        }
        self.state.cart = cart;
        self.state.cart_platform = platform;
        self.persist();
    }

    fn persist(&self) {
        let Some(root) = &self.root else {
            return;
        };
        if let Err(e) = write_slot_state(root, &self.state) {
            eprintln!("slot: slot.state: {e}");
        }
    }

    pub fn on_core_ready(&mut self) {
        let Phase::Inserting {
            cart, core_ready, ..
        } = &mut self.phase
        else {
            return;
        };
        *core_ready = true;
        // The cart's name is only here during the insert, and this is when the core has
        // accepted or refused its resume.
        let cart = cart.clone();
        self.retire_refused_resume(&cart);
    }

    /// Moves a resume the core would not read aside, so it is not refused on every boot with no
    /// way to clear it on the device. Done as the cart settles, not at the next flush. Runs every
    /// frame of the insert; later calls find no `resume.state` and do nothing.
    fn retire_refused_resume(&mut self, stem: &str) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        // The common case, and one atomic load instead of a stat per frame.
        if snapshot.resume_trusted() {
            return;
        }
        // A missing dylib means the mock refused, which says nothing about the state.
        // `resume_trusted` stays false, so the mock still never overwrites it.
        if !self.named_core {
            return;
        }
        let Some(root) = &self.root else {
            return;
        };
        let ring = StateRing::new(root, self.platform, self.core, stem);
        match ring.retire_resume(&format_stamp(self.wall_secs())) {
            Ok(Some(to)) => eprintln!(
                "slot: resume: {} refused this state, moved it to {}",
                self.core.as_str(),
                to.display()
            ),
            Ok(None) => {}
            Err(e) => eprintln!("slot: resume: could not move the refused state aside: {e}"),
        }
    }

    pub fn on_core_failed(&mut self) {
        let caught = self.seat();
        let Phase::Inserting { cart, .. } = &mut self.phase else {
            return;
        };
        let cart = std::mem::take(cart);
        self.refuse_out(cart, caught);
    }

    /// Sends `cart` back out of the slot refused, from `caught` of the way in.
    fn refuse_out(&mut self, cart: String, caught: f32) {
        // Resumed from the depth it reached, so the refusal is one movement.
        let t = (1.0 - caught) * EJECT_S;
        self.phase = Phase::Ejecting { cart, t };
        // No shake: the cart carries the alert, and both would read as two failures.
        self.refused_from = Some(t);
    }

    /// Any action the app will not carry out. It decays on its own clock.
    pub fn refuse(&mut self) {
        self.refusal = Some(Refusal::started(self.now()));
    }

    pub fn refusal_active(&self, now: Millis) -> bool {
        self.refusal.is_some_and(|r| r.active(now))
    }

    /// How far the cart is into the slot: 0.0 on the shelf, 1.0 swallowed.
    pub fn seat(&self) -> f32 {
        match &self.phase {
            Phase::Shelf => 0.0,
            Phase::Inserting { t, resumed, .. } => {
                if *resumed {
                    1.0
                } else {
                    (t / SEATED_AT).clamp(0.0, 1.0)
                }
            }
            Phase::Ejecting { t, .. } => 1.0 - (t / EJECT_S).clamp(0.0, 1.0),
            _ => 1.0,
        }
    }

    pub fn draw(&self, out: &mut Vec<Draw>) {
        // Before every phase. rcK takes about five seconds (it unloads the GPU module), and that
        // long on a black panel looks like a hang.
        if let Some(index) = self.power_menu {
            self.draw_power_menu(index, out);
            return;
        }
        if self.shutting_down() {
            out.push(Draw::Rect {
                x: 0.0,
                y: 0.0,
                w: OUT_W as f32,
                h: OUT_H as f32,
                colour: [0.0, 0.0, 0.0, 1.0],
            });
            // Repeats back the row the user picked.
            let which = if self.restarting {
                PowerChoice::Restart
            } else {
                PowerChoice::PowerOff
            };
            if let Some((tex, w, h)) = self.shutdown_faces.get(which.index()).copied() {
                out.push(Draw::Tex {
                    x: ((OUT_W - w) / 2) as f32,
                    y: ((OUT_H - h) / 2) as f32,
                    w: w as f32,
                    h: h as f32,
                    tex,
                    alpha: 1.0,
                });
            }
            return;
        }
        match &self.phase {
            // Nothing goes over it, the HUD included.
            Phase::SetClock {
                picker, from_menu, ..
            } => {
                let (line, hint) = match self.clock_faces {
                    Some((line, hint)) => (Some(line), Some(hint)),
                    None => (None, None),
                };
                // The quick menu's B BACK, when there is a menu to go back to.
                let back = self
                    .quick_menu_faces
                    .as_ref()
                    .filter(|_| *from_menu)
                    .map(|f| f.legend[0]);
                picker.draw(line, hint, back, out);
                return;
            }
            // Not returned from: the level bars still draw over the menu.
            Phase::QuickMenu { row } => QuickMenu {
                row: *row,
                values: QuickRow::ALL.map(|r| self.quick_value(r)),
                clock: self.quick_clock_faces,
                faces: self.quick_menu_faces.as_ref(),
            }
            .draw(out),
            Phase::Shelf => {
                draw_backdrop(self.wallpaper, out);
                match (self.core_picker_shown(), self.selected_stem()) {
                    // The picker draws the highlighted cart while its lid is off, and the row
                    // makes way. Until its faces are up the cart stands.
                    (Some(picker), Some(stem)) => {
                        // Eased on the whole progress, so slide and lift are one movement.
                        let open = ease(picker.openness(self.now()));
                        // Dims in step with the lid coming off and going back on.
                        let dim = 1.0 + (CORE_PICKER_DIM - 1.0) * open;
                        self.shelf()
                            .draw_row(Some(stem), 0.0, CORE_PICKER_RECEDE * open, dim, out);
                        draw_empty_slot(out);
                    }
                    _ => {
                        self.shelf().draw(self.shelf_shake(), out);
                        draw_slot_name(self.shelf_platform, self.slot_name_alpha(), out);
                    }
                }
                draw_footer(
                    self.battery,
                    self.battery_percent,
                    self.bolt,
                    self.shelf_clock,
                    out,
                );
            }
            Phase::About => {
                // The shelf's backdrop and scrim, which the dark label needs over a photograph.
                draw_backdrop(self.wallpaper, out);
                draw_sticker(self.sticker_face, out);
                return;
            }
            // The shelf recedes behind the cart on the way in.
            Phase::Inserting { cart, resumed, .. } => {
                // Spec section 3: a resumed cart shows no shelf, not even one frame of it.
                if !resumed {
                    draw_backdrop(self.wallpaper, out);
                    self.shelf()
                        .draw_row(Some(cart), 0.0, self.seat(), 1.0, out);
                }
                self.chrome(cart, self.seat(), out);
            }
            // The insert run backwards, off the same progress.
            Phase::Ejecting { cart, .. } => {
                draw_backdrop(self.wallpaper, out);
                self.shelf()
                    .draw_row(Some(cart), 0.0, self.seat(), 1.0, out);
                self.chrome(cart, self.seat(), out);
            }
            // The slot stays until the picture has finished arriving, so the game blooms out of
            // a lit lip.
            Phase::Playing { cart } if self.screen < 1.0 => self.chrome(cart, 0.0, out),
            Phase::Playing { .. } => self.push_game(out),
            // The paused game stays underneath the switcher's full-screen screenshot.
            Phase::Polaroids { .. } => {
                self.push_game(out);
                if let Some(p) = &self.polaroids {
                    p.draw(
                        self.battery,
                        self.battery_percent,
                        self.bolt,
                        self.shelf_clock,
                        out,
                    );
                }
            }
            // The host has no backlight to cut, so the doze is drawn. Nothing goes over it.
            Phase::Doze { .. } => {
                out.push(Draw::Rect {
                    x: 0.0,
                    y: 0.0,
                    w: OUT_W as f32,
                    h: OUT_H as f32,
                    colour: [0.0, 0.0, 0.0, 1.0],
                });
                return;
            }
        }
        // After the shelf, or the row would paint over it.
        if let Some(picker) = self.core_picker_shown() {
            self.draw_core_picker(&picker, out);
        }
        // Over the game and under the HUD, so the level bars stay visible.
        if let Some(menu) = self.game_menu {
            self.draw_game_menu(menu, out);
        }
        // Over everything, in every phase.
        self.hud.draw(self.now(), out);
    }

    pub fn screen_shake(&self) -> f32 {
        self.shake_at(self.now())
    }

    /// Pixels the whole image is displaced by. Only while playing, when the game fills the frame.
    pub fn shake_at(&self, now: Millis) -> f32 {
        self.shake_when(matches!(self.phase, Phase::Playing { .. }), now)
    }

    /// Pixels the cart row is displaced by. On the shelf, shaking the whole image would just
    /// slide the letterbox.
    pub fn shelf_shake(&self) -> f32 {
        // The chip flinches while the picker is up; two shakes read as two refusals.
        self.shake_when(self.on_shelf() && self.core_picker.is_none(), self.now())
    }

    /// Shake only what represents the refused thing: two at once reads as two failures.
    fn shake_when(&self, mine: bool, now: Millis) -> f32 {
        if !mine {
            return 0.0;
        }
        self.refusal.map_or(0.0, |r| r.offset(now))
    }

    /// The game layer's place in the list: the whole picture, or in front of the cart in chrome.
    fn push_game(&self, out: &mut Vec<Draw>) {
        if self.game_visible() {
            out.push(Draw::Game);
        }
    }

    fn chrome(&self, stem: &str, dim: f32, out: &mut Vec<Draw>) {
        let Some((cart, face)) = self.shelf().find(stem) else {
            return;
        };
        let alpha = self.alert_alpha();
        // Where the row had this cart when the button went down. The shelf stops with the
        // phase change, so this is constant through the travel.
        let (rest, scale) = self.shelf().selected_at();
        SlotChrome {
            cart,
            face,
            rest,
            scale,
            seat: self.seat(),
            alert: self.alert_face.filter(|_| alpha > 0.0).map(|t| (t, alpha)),
            dim,
            screen: self.screen,
            game: self.game_ready,
        }
        .draw(out);
    }

    /// Whether the cart on its way back out is carrying the refusal symbol.
    pub fn alert_visible(&self) -> bool {
        self.alert_alpha() > 0.0
    }

    /// How lit that symbol is. It fades out before the exit ends, so the shelf does not cut it off.
    pub fn alert_alpha(&self) -> f32 {
        let (Some(from), Phase::Ejecting { t, .. }) = (self.refused_from, &self.phase) else {
            return 0.0;
        };
        let span = EJECT_S - from;
        if span <= 0.0 {
            return 0.0;
        }
        let u = ((t - from) / span).clamp(0.0, 1.0);
        ((ALERT_GONE - u) / (ALERT_GONE - ALERT_HOLD)).clamp(0.0, 1.0)
    }

    pub fn set_alert_face(&mut self, face: TexId) {
        self.alert_face = Some(face);
    }

    pub fn set_shutdown_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.shutdown_faces = faces;
    }

    pub fn set_power_menu_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.power_menu_faces = faces;
    }

    /// Recorded against the highlighted cart, the only one they are built for.
    pub fn set_core_board_faces(&mut self, board: TexId, lid: TexId) {
        self.core_board_face = Some(board);
        self.core_lid_face = Some(lid);
        self.core_faces_stem = self.selected_stem().map(str::to_string);
    }

    /// `sockets` and `chips` in `Core::ALL` order.
    pub fn set_core_part_faces(
        &mut self,
        sockets: Vec<TexId>,
        chips: Vec<TexId>,
        blank: TexId,
        shadow: TexId,
    ) {
        self.core_socket_faces = sockets;
        self.core_chip_faces = chips;
        self.core_blank_chip_face = Some(blank);
        self.core_chip_shadow_face = Some(shadow);
    }

    pub fn set_core_legend_faces(&mut self, faces: Vec<(TexId, u32)>) {
        self.core_legend_faces = faces;
    }

    /// One per `LinkRow::ALL`, in that order.
    pub fn set_link_menu_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.link_menu_faces = faces;
    }

    pub fn set_link_linked_face(&mut self, face: (TexId, u32, u32)) {
        self.link_linked_face = Some(face);
    }

    /// One per `LinkStep::ALL` and one per `LinkFail::SHOWN`, in those orders. Uploaded at boot so
    /// a failing link never waits on the font.
    pub fn set_link_step_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.link_step_faces = faces;
    }

    pub fn set_link_fail_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.link_fail_faces = faces;
    }

    /// In `LinkLegend::ALL` order: the face and its width.
    pub fn set_link_legend_faces(&mut self, faces: Vec<(TexId, u32)>) {
        self.link_legend_faces = faces;
    }

    /// The link art, once the worker has built it and the frontend has uploaded it.
    pub fn set_link_sprites(&mut self, sprites: LinkSprites) {
        self.link_sprites = Some(sprites);
    }

    pub fn link_sprites_ready(&self) -> bool {
        self.link_sprites.is_some()
    }

    /// The case's own ground and the rows on it. The ground is drawn here, not in
    /// `draw_menu_rows`, because the core picker draws over a shelf instead.
    fn draw_power_menu(&self, index: usize, out: &mut Vec<Draw>) {
        out.push(Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: OUT_W as f32,
            h: OUT_H as f32,
            colour: slot_ui::opening(),
        });
        draw_menu_rows(
            &self.power_menu_faces,
            Some(index),
            centred_top(self.power_menu_faces.len()),
            out,
        );
    }

    /// The picker once it has started opening. `None` while it waits for this cart's faces.
    fn core_picker_shown(&self) -> Option<CorePicker> {
        self.core_picker.filter(|p| !p.waiting())
    }

    /// The open cart over the receding shelf: board, sockets, chip, lifted lid and legend. Under
    /// the HUD, since the levels still answer. Until `ready` (this cart's own board and lid) only
    /// the lid is drawn, from the shelf's face, never a stale build of another cart.
    fn draw_core_picker(&self, picker: &CorePicker, out: &mut Vec<Draw>) {
        let now = self.now();
        let progress = picker.openness(now);
        // The shadows and the legend come in with the lift, not the slide.
        let lift = lift_of(progress);
        // Grows out of wherever the row has the cart this frame: the shelf is still running
        // underneath, so the spring may not have settled.
        let (rest, scale) = self.shelf().selected_at();
        let shelf = shelf_cart_at(rest, scale);
        let board = board_from(shelf, progress);
        let zoom = board_zoom(board);
        let ready = self.core_faces_ready();

        if ready {
            // Opaque from the first frame: the slide only uncovers what was under the front.
            if let Some(tex) = self.core_board_face {
                out.push(Draw::Tex {
                    x: board.x,
                    y: board.y,
                    w: board.w,
                    h: board.h,
                    tex,
                    alpha: 1.0,
                });
            }
            // A face drawn at its own size is only sharp on whole pixels.
            for (i, tex) in self.core_socket_faces.iter().copied().enumerate() {
                let (x, y) = on_board(board, SOCKET_U[i], SOCKET_V);
                out.push(Draw::Tex {
                    x: x.round(),
                    y: y.round(),
                    w: SOCKET_W as f32 * zoom,
                    h: SOCKET_H as f32 * zoom,
                    tex,
                    alpha: 1.0,
                });
            }

            let chip = picker.chip(now);
            let u = CHIP_U[0] + (CHIP_U[1] - CHIP_U[0]) * chip.across;
            if chip.lift > 0.0 {
                if let Some(tex) = self.core_chip_shadow_face {
                    // Under the body's middle and down the board where the mockup's oval falls,
                    // so it reads as cast on the board.
                    let (cx, cy) = on_board(board, u + 19.0, CHIP_V + 29.4);
                    let (w, h) = (SHADOW_W as f32 * zoom, SHADOW_H as f32 * zoom);
                    out.push(Draw::Tex {
                        x: cx - w / 2.0,
                        y: cy - h / 2.0,
                        w,
                        h,
                        tex,
                        alpha: 0.6 * chip.lift * lift,
                    });
                }
            }
            let face = match chip.seated {
                Some(core) => self.core_chip_faces.get(core.index()).copied(),
                None => self.core_blank_chip_face,
            };
            if let Some(tex) = face {
                let (x, y) = on_board(board, u, CHIP_V - HOP_LIFT * chip.lift);
                let body = Placed {
                    x: x + chip.shake,
                    y,
                    w: CHIP_W as f32 * zoom,
                    h: CHIP_H as f32 * zoom,
                };
                // Whole pixels, as the sockets.
                let at = grown(body, TURN_PAD as f32 * zoom);
                out.push(Draw::Turned {
                    x: at.x.round(),
                    y: at.y.round(),
                    w: at.w,
                    h: at.h,
                    tex,
                    alpha: 1.0,
                    turn: chip.tip,
                });
            }
        }

        // The soft oval under the lid, so it reads as held up rather than printed on. Drawn
        // whether or not the faces are ready, since some lid is always drawn.
        if let Some(tex) = self.core_chip_shadow_face {
            let (lid, _) = lid_from(shelf, progress);
            let k = lid.w / lid_at(1.0).0.w;
            let (w, h) = (LID_SHADOW_W * k, LID_SHADOW_H * k);
            out.push(Draw::Tex {
                x: lid.x + (lid.w - w) / 2.0,
                y: lid.y + lid.h + LID_SHADOW_DROP * k - h / 2.0,
                w,
                h,
                tex,
                alpha: LID_SHADOW_ALPHA * lift,
            });
        }

        if ready {
            // Always opaque: at either end of the movement the lid is the cart on the shelf.
            if let Some(tex) = self.core_lid_face {
                let (lid, turn) = lid_from(shelf, progress);
                let at = grown(lid, TURN_PAD as f32 * lid.w / CART_W as f32);
                out.push(Draw::Turned {
                    x: at.x,
                    y: at.y,
                    w: at.w,
                    h: at.h,
                    tex,
                    alpha: 1.0,
                    turn,
                });
            }
        } else if let Some((_, Some(tex))) = self
            .selected_stem()
            .and_then(|stem| self.shelf().find(stem))
        {
            // The wait ran out before this cart's lid arrived. Lift the shelf's face instead,
            // unpadded since it has no transparent border.
            let (lid, turn) = lid_from(shelf, progress);
            out.push(Draw::Turned {
                x: lid.x,
                y: lid.y,
                w: lid.w,
                h: lid.h,
                tex,
                alpha: 1.0,
                turn,
            });
        }

        // Placed by the visible part of each face (key caps and word), not its trailing
        // transparent strip.
        if let [cancel, swap, choose] = self.core_legend_faces.as_slice() {
            let right = BOARD_X + BOARD_W as f32;
            let seen = |w: u32| w.saturating_sub(HINT_EDGE) as f32;
            for (tex, w, x) in [
                (cancel.0, cancel.1, BOARD_X),
                (swap.0, swap.1, (OUT_W as f32 - seen(swap.1)) / 2.0),
                (choose.0, choose.1, right - seen(choose.1)),
            ] {
                out.push(Draw::Tex {
                    x: x.round(),
                    y: CORE_LEGEND_Y,
                    w: w as f32,
                    h: HINT_H as f32,
                    tex,
                    alpha: lift,
                });
            }
        }
    }

    /// The link screen: a scrim over the paused game, one line of text and its key legend.
    fn draw_game_menu(&self, menu: GameMenu, out: &mut Vec<Draw>) {
        out.push(Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: OUT_W as f32,
            h: OUT_H as f32,
            colour: slot_ui::opening(),
        });
        if let Some(sprites) = &self.link_sprites {
            crate::link_screen::draw_link_art(menu, self.link_hardware, self.now(), sprites, out);
        }
        let line = match menu {
            GameMenu::Pick(role) => self.link_menu_faces.get(role.index()).copied(),
            // The first step's sentence depends on whether the driver is warm. Asked every frame,
            // so a warm landing mid-step is picked up.
            GameMenu::Working { step, .. } => self
                .link_step_faces
                .get(step.shown(self.radio.warmed()).index())
                .copied(),
            GameMenu::Linked { .. } => self.link_linked_face,
            GameMenu::Failed { fail, .. } => fail
                .shown()
                .and_then(|i| self.link_fail_faces.get(i).copied()),
            // No line: the banner says what happened, and LINKED would contradict the art.
            GameMenu::Unplug { .. } => None,
        };
        if let Some((tex, w, h)) = line {
            out.push(Draw::Tex {
                x: ((OUT_W as f32 - w as f32) / 2.0).round(),
                y: LINK_TEXT_Y,
                w: w as f32,
                h: h as f32,
                tex,
                alpha: 1.0,
            });
        }
        // SELECT is shown only where it works: a game gpSP links the same way on either hardware
        // refuses the press.
        let switchable = self.seated().is_some_and(|stem| self.link_switchable(stem));
        let keys: &[LinkLegend] = match menu {
            GameMenu::Pick(_) if switchable => &[
                LinkLegend::Cancel,
                LinkLegend::Mode,
                LinkLegend::Swap,
                LinkLegend::Link,
            ],
            GameMenu::Pick(_) => &[LinkLegend::Cancel, LinkLegend::Swap, LinkLegend::Link],
            GameMenu::Working { .. } => &[LinkLegend::Cancel],
            // The flash leaves on its own. The screen opened over a live session offers two keys.
            GameMenu::Linked { opened: true, .. } => &[LinkLegend::Back, LinkLegend::EndLink],
            GameMenu::Linked { .. } => &[],
            GameMenu::Failed { .. } => &[LinkLegend::Ok],
            // Takes no presses and leaves on its own.
            GameMenu::Unplug { .. } => &[],
        };
        let faces: Vec<(TexId, u32)> = keys
            .iter()
            .filter_map(|k| self.link_legend_faces.get(k.index()).copied())
            .collect();
        let seen = |w: u32| w.saturating_sub(HINT_EDGE) as f32;
        let total: f32 = faces.iter().map(|(_, w)| seen(*w)).sum::<f32>()
            + LINK_LEGEND_GAP * faces.len().saturating_sub(1) as f32;
        let mut x = ((OUT_W as f32 - total) / 2.0).round();
        for (tex, w) in faces {
            out.push(Draw::Tex {
                x,
                y: LINK_LEGEND_Y,
                w: w as f32,
                h: HINT_H as f32,
                tex,
                alpha: 1.0,
            });
            x += (seen(w) + LINK_LEGEND_GAP).round();
        }
    }

    /// The cart named `stem` and the hardware gpSP picks for it. Reads the header on disk, so
    /// never call it per frame. `None` for an unknown cart.
    fn auto_link(&self, stem: &str) -> Option<(&Cart, LinkKind)> {
        let cart = self.shelf().carts.iter().find(|c| c.stem == stem)?;
        let auto = link_kind(&cart.code, &cart.title, slot_store::header_clean(&cart.rom));
        Some((cart, auto))
    }

    fn on_shelf(&self) -> bool {
        matches!(self.phase, Phase::Shelf)
    }

    fn insert(&mut self, clean: bool) {
        if !self.on_shelf() {
            return;
        }
        let Some(cart) = self
            .shelf()
            .carts
            .get(self.shelf().index)
            .map(|c| c.stem.clone())
        else {
            return;
        };
        self.play_held = None;
        self.refusal = None;
        self.refused_from = None;
        self.phase = Phase::Inserting {
            cart,
            t: 0.0,
            core_ready: false,
            resumed: false,
            clean,
        };
    }

    /// Whether the cart going in starts from the beginning. Read by whoever spawns the core.
    pub fn starting_clean(&self) -> bool {
        matches!(self.phase, Phase::Inserting { clean: true, .. })
    }

    /// The hold fires under the finger, not on release. Leaving the shelf with A down disarms it.
    fn play_hold(&mut self) {
        let Some(at) = self.play_held else {
            return;
        };
        if !self.on_shelf() {
            self.play_held = None;
            return;
        }
        if self.now().saturating_sub(at) >= PLAY_HOLD_MS {
            self.insert(true);
        }
    }

    fn eject(&mut self) {
        // Nowhere to eject to. Refused so the press is answered.
        if self.single_cart() {
            return self.refuse();
        }
        // Inserting too, so a slot whose core never arrived can be emptied.
        let cart = match &mut self.phase {
            Phase::Playing { cart } | Phase::Inserting { cart, .. } => std::mem::take(cart),
            _ => return,
        };
        // A session does not survive its cart, or `link_active()` stays true with no core and
        // blocks rewind, state loads and doze for good.
        self.end_link();
        // The overlay swallows eject, but guard here for any future route in.
        self.close_game_menu();
        self.flush_eject(&cart);
        // The offer names this cart's ring and core; carried across the slot it would act on the
        // wrong one.
        self.pending = None;
        // An eject asked for is not an eject refused.
        self.refusal = None;
        self.refused_from = None;
        self.phase = Phase::Ejecting {
            cart,
            t: -EJECT_HOLD_S,
        };
    }

    /// Everything durable happens here, before the animation, since the card can be pulled while
    /// the cart slides out. A failed write leaves the cart seated, so the next boot resumes it.
    fn flush_eject(&mut self, stem: &str) {
        let (Some(root), Some(snapshot)) = (&self.root, &self.snapshot) else {
            return;
        };
        let Some(state) = snapshot.state() else {
            eprintln!("slot: eject: the core gave up no state");
            return;
        };
        let (state, sav) = trusted_write(snapshot.as_ref(), state, "eject");
        match persist::eject(
            root,
            self.platform,
            self.core,
            stem,
            state.as_deref(),
            sav.as_deref(),
        ) {
            // Mirroring what `persist::eject` just wrote to the card: the slot is empty, and
            // which platform was in it is part of what emptying it forgets.
            Ok(()) => {
                self.state.cart = None;
                self.state.cart_platform = None;
            }
            Err(e) => eprintln!("slot: eject: {e}"),
        }
    }

    /// Flush, then dark, then idle. The cart stays seated and `slot.state` untouched, so the next
    /// boot resumes. A live session ends: pausing breaks the netpacket contract, and a held session
    /// draws 400-700 mA against a doze's sub-45 mA.
    fn doze(&mut self) {
        if self.link_active() {
            self.end_link();
        }
        // A starter left running would keep the radio up through the doze.
        self.close_game_menu();
        // A shut lid is walking away, not choosing: nothing is written.
        self.core_picker = None;
        if matches!(self.phase, Phase::Doze { .. }) {
            return;
        }
        self.flush_resume();
        // Only a running cart is worth waking into. Anything else wakes to the shelf rather than
        // a core that may not have finished loading.
        let cart = match &mut self.phase {
            Phase::Playing { cart } | Phase::Polaroids { cart } => Some(std::mem::take(cart)),
            _ => None,
        };
        self.polaroids = None;
        self.phase = Phase::Doze { cart };
        self.dozed_at = self.now();
        // A doze ends in power off, so drop the driver. After `end_link` this is a no-op.
        self.radio.ask(RadioJob::Cool);
        if let Some(power) = &mut self.power {
            power.on_close();
        }
    }

    fn wake(&mut self) {
        let Phase::Doze { cart } = &mut self.phase else {
            return;
        };
        self.phase = match cart.take() {
            Some(cart) => Phase::Playing { cart },
            None => Phase::Shelf,
        };
        if let Some(power) = &mut self.power {
            power.on_open();
        }
    }

    /// A dark panel still draws 400-700 mA, so the doze is a grace period before a real power
    /// off. Suspend (under 45 mA) is out: the RTC alarm arms but never fires, so nothing can wake
    /// it. `slot.state` still names the cart, so boot resumes the same frame.
    pub fn on_doze_timeout(&mut self) {
        if !matches!(self.phase, Phase::Doze { .. }) {
            return;
        }
        self.begin_power_off();
    }

    /// The lid's twin, and the one the device is certain to see. A tap dozes and a second wakes.
    /// A press that already woke the panel (see `apply`'s `PowerPress` arm) stops here, or its
    /// release would doze straight away.
    fn power_press(&mut self) {
        if std::mem::take(&mut self.woke_on_press) {
            return;
        }
        match self.phase {
            Phase::Doze { .. } => self.wake(),
            _ => self.doze(),
        }
    }

    /// The hold threshold raises the menu and nothing else, so the hold is safe to discover by
    /// accident.
    fn open_power_menu(&mut self) {
        if self.power_menu.is_some() {
            return;
        }
        // The menu pauses the core, which libretro's netpacket contract forbids in a session.
        // Declined, not ending the session.
        if self.link_active() {
            return self.refuse();
        }
        // Durable first: the user may hold on to the PMIC's six second cutoff.
        self.flush_resume();
        self.power_menu = Some(0);
    }

    /// Up and down move, A commits, B leaves. No timeout: it would fire when the user looked away.
    fn power_menu_input(&mut self, action: Action) {
        let Some(index) = self.power_menu else {
            return;
        };
        let last = PowerChoice::ALL.len() - 1;
        match action {
            Action::GbaDown(Btn::Up) => self.power_menu = Some(index.saturating_sub(1)),
            Action::GbaDown(Btn::Down) => self.power_menu = Some((index + 1).min(last)),
            Action::GbaDown(Btn::B) => self.power_menu = None,
            Action::GbaDown(Btn::A) => {
                self.power_menu = None;
                // Both choices end what was underneath. Restart sets its flag here rather than
                // going through `begin_power_off`.
                self.close_game_menu();
                match PowerChoice::ALL[index] {
                    PowerChoice::Restart => {
                        self.restarting = true;
                        self.act_at = self.now() + SHUTDOWN_SHOW_MS;
                        self.set_led(LedState::Off);
                    }
                    PowerChoice::PowerOff => self.begin_power_off(),
                }
            }
            _ => {}
        }
    }

    /// START on the shelf opens the highlighted cart's core picker on its current core. Inert
    /// with no card, since the choice could not be stored.
    fn open_core_picker(&mut self) {
        let Some(root) = self.root.clone() else {
            return;
        };
        // No cart under the highlight, nothing to configure.
        let Some(cart) = self.shelf().carts.get(self.shelf().index) else {
            return;
        };
        // The board is a traced GBA PCB, so a Game Boy cart does not open onto it; its core is set
        // by platform. No shake: there is no choice to decline.
        if cart.platform != Platform::Gba {
            return;
        }
        let seat = slot_store::core_for(&root, &cart.stem);
        let now = self.now();
        let mut picker = CorePicker::open(seat, now);
        if self.core_faces_ready() {
            picker.start(now);
        }
        self.core_picker = Some(picker);
        // Drop what the shelf had armed: a held direction would repeat under the lid, and a held
        // A would insert the cart after 500 ms.
        self.shelf_mut().release_hold();
        self.play_held = None;
    }

    /// Whether the board and lid on the GPU are the highlighted cart's, so its open can start.
    fn core_faces_ready(&self) -> bool {
        self.core_faces_stem
            .as_deref()
            .is_some_and(|stem| self.selected_stem() == Some(stem))
    }

    /// The picker owns every button while up, so the row cannot move to another cart. The arrows
    /// point at the sockets and do not wrap.
    fn core_picker_input(&mut self, action: Action) {
        let press = match action {
            Action::GbaDown(Btn::Left) | Action::ShelfLeft => Press::Left,
            Action::GbaDown(Btn::Right) | Action::ShelfRight => Press::Right,
            Action::GbaDown(Btn::A) => Press::Keep,
            Action::GbaDown(Btn::B) => Press::Back,
            _ => return,
        };
        let now = self.now();
        let Some(picker) = &mut self.core_picker else {
            return;
        };
        let outcome = picker.press(press, now);
        if let Outcome::Write(core) = outcome {
            self.write_core(core);
        }
    }

    /// SELECT+MENU over a running game. No phase change: cancelling gives the game straight back.
    fn open_game_menu(&mut self) {
        if self.game_menu.is_some() {
            return;
        }
        // The overlay pauses the core, which libretro's netpacket contract forbids in a
        // session.
        if self.link_active() {
            return self.refuse();
        }
        // mGBA links by running both machines in lockstep, so it carries every cart on every
        // platform and `link_carried` is not its question.
        if self.core == Core::Mgba {
            // mGBA emulates only the cable. A Wireless Adapter cart needs gpSP.
            let wireless = self
                .seated()
                .is_some_and(|stem| self.link_mode(stem).0 == LinkKind::Wireless);
            if wireless {
                self.hud.toast(Toast::NeedsGpsp, self.now());
                return;
            }
            self.link_hardware = LinkKind::Cable;
            self.radio.ask(RadioJob::Warm);
            self.game_menu = Some(GameMenu::Pick(self.last_role));
            return;
        }
        if self.platform != Platform::Gba {
            self.hud.toast(Toast::NoLink, self.now());
            return;
        }
        // gpSP fakes named protocols rather than emulating the cable, so a cart it has none for
        // would reach LINKED and drop every packet. Before the core check: nothing can link it.
        let carried = self
            .seated()
            .and_then(|stem| self.auto_link(stem))
            .is_some_and(|(cart, _)| link_carried(&cart.code, &cart.title));
        if !carried {
            self.hud.toast(Toast::NoLink, self.now());
            return;
        }
        // gpSP is the only other core with a netpacket interface. The banner says so.
        if self.core != Core::Gpsp {
            self.hud.toast(Toast::NeedsGpsp, self.now());
            return;
        }
        // Opens on the cart's last switched hardware, or gpSP's pick.
        let hardware = self
            .seated()
            .map_or(LinkKind::Cable, |stem| self.link_mode(stem).0);
        self.link_hardware = hardware;
        // The driver takes about a second to load. Nothing waits on it: `link host` and `link
        // join` load it themselves through the same queue.
        self.radio.ask(RadioJob::Warm);
        self.game_menu = Some(GameMenu::Pick(self.last_role));
    }

    /// SELECT+MENU: the link screen, or over a live session the same screen with the key that
    /// ends it. Ending is a choice there, not the press, since it cannot be undone.
    fn game_menu_shortcut(&mut self) {
        let Some(client_id) = self.link_client_id() else {
            return self.open_game_menu();
        };
        let now = self.now();
        self.game_menu = Some(GameMenu::Linked {
            role: LinkRow::from_client_id(client_id),
            worked: now,
            since: now,
            opened: true,
        });
    }

    /// A on that screen ends the link at once: the screen was the confirmation. The game carries
    /// on in its loaded mode.
    fn end_link_from_menu(&mut self) {
        // Read before `end_link` clears it: it decides which plug is drawn.
        let role = self
            .link_client_id()
            .map_or(self.last_role, LinkRow::from_client_id);
        self.end_link();
        self.hud.toast(Toast::LinkEnded, self.now());
        self.unplug(role);
    }

    /// The plug coming back out, after the teardown. Takes no presses and leaves on its own (see
    /// `timers`).
    fn unplug(&mut self, role: LinkRow) {
        self.game_menu = Some(GameMenu::Unplug {
            role,
            since: self.now(),
        });
    }

    /// Lets a test watch radio jobs. `App` never waits on them, so the queue is replaceable.
    pub fn set_radio_jobs(&mut self, jobs: Box<dyn RadioJobs>) {
        self.radio = jobs;
    }

    /// The menu owns every game-side button while it is up.
    fn game_menu_input(&mut self, action: Action) {
        let Some(menu) = self.game_menu else {
            return;
        };
        match menu {
            GameMenu::Pick(role) => match action {
                Action::GbaDown(Btn::Left) | Action::GbaDown(Btn::Right) => {
                    self.last_role = role.other();
                    self.game_menu = Some(GameMenu::Pick(role.other()));
                }
                // On the press, so a SELECT chord (e.g. SELECT+Up) also flips the mode. The
                // mode is a toggle, so pressing again undoes it.
                Action::GbaDown(Btn::Select) => self.switch_hardware(),
                Action::GbaDown(Btn::A) => self.pick_link(role),
                Action::GbaDown(Btn::B) | Action::GameMenu => self.close_game_menu(),
                _ => {}
            },
            // B asks the worker to stop and the screen waits for its answer. A link still
            // waiting on its reload has no worker, so the cancel is kept for the reload.
            GameMenu::Working { .. } => {
                if action == Action::GbaDown(Btn::B) {
                    if let Some(starting) = &mut self.starting {
                        starting.starter.cancel();
                    }
                    if let Some(reload) = &mut self.reload {
                        reload.cancelled = true;
                    }
                }
            }
            // The flash takes no presses. The opened screen takes two.
            GameMenu::Linked { opened: true, .. } => match action {
                Action::GbaDown(Btn::A) => self.end_link_from_menu(),
                Action::GbaDown(Btn::B) | Action::GameMenu => self.close_game_menu(),
                _ => {}
            },
            GameMenu::Linked { .. } => {}
            GameMenu::Failed { .. } => {
                if matches!(
                    action,
                    Action::GbaDown(Btn::A) | Action::GbaDown(Btn::B) | Action::GameMenu
                ) {
                    self.close_game_menu();
                }
            }
            // Takes no presses: the session is already over.
            GameMenu::Unplug { .. } => {}
        }
    }

    /// Hands the overlay an already-running worker and shows the first step. Split from the pick
    /// so tests can supply a starter with no network interface.
    pub fn start_link(&mut self, starter: LinkStarter, client_id: u16) {
        self.start_link_from(starter, client_id, self.now());
    }

    /// `start_link`, with the first step dated from `since`, so a link that waited on a reload
    /// does not restart its animation.
    fn start_link_from(&mut self, starter: LinkStarter, client_id: u16, since: Millis) {
        // `LinkStarter` has no `Drop`: a dropped one leaves its radio up, so cancel it.
        if let Some(mut old) = self.starting.replace(LinkStarting { starter, client_id }) {
            old.starter.cancel();
        }
        self.game_menu = Some(GameMenu::Working {
            role: LinkRow::from_client_id(client_id),
            step: LinkStep::Radio,
            since,
        });
    }

    /// SELECT on Pick. The other hardware becomes this cart's choice until slot restarts; the
    /// running game is untouched until A. Refused where gpSP would load the same either way.
    fn switch_hardware(&mut self) {
        let Some(stem) = self.seated().map(str::to_string) else {
            return;
        };
        if !self.link_switchable(&stem) {
            return self.refuse();
        }
        let other = self.link_hardware.other();
        self.link_hardware = other;
        self.link_choices.insert(stem, other);
    }

    /// Whether the other hardware would load this cart with a different `gpsp_serial`. Not for a
    /// game with no cable protocol, which loads on `auto` regardless. Read by the press and the
    /// legend.
    fn link_switchable(&self, stem: &str) -> bool {
        self.auto_link(stem).is_some_and(|(cart, auto)| {
            serial_option(self.link_hardware.other(), auto, &cart.code, &cart.title)
                != serial_option(self.link_hardware, auto, &cart.code, &cart.title)
        })
    }

    /// A on Pick. A link in the loaded mode starts now. Otherwise the game is reloaded first,
    /// since gpSP reads link mode only at load. Modes are compared by `gpsp_serial`.
    fn pick_link(&mut self, role: LinkRow) {
        let Some(stem) = self.seated().map(str::to_string) else {
            return;
        };
        // A core nobody reported was loaded on `auto`.
        let loaded = self.link_loaded.unwrap_or("auto");
        // mGBA reads `mgba_link` only at load too, so a core not in link mode for this port is
        // reloaded first. `from` is simply what is loaded.
        if self.core == Core::Mgba {
            let player = role.client_id() as u8;
            if self.link_player == Some(player) {
                return self.start_link(
                    LinkStarter::spawn(role.role(), link_port()),
                    role.client_id(),
                );
            }
            if self.snapshot.as_ref().is_some_and(|s| !s.resume_trusted()) {
                return self.refuse();
            }
            self.link_player = Some(player);
            self.link_reload = Some((stem.clone(), loaded));
            self.reload = Some(Reload {
                stem,
                role,
                cancelled: false,
                from: self.link_hardware,
                from_serial: loaded,
                fallback: false,
            });
            self.game_menu = Some(GameMenu::Working {
                role,
                step: LinkStep::Radio,
                since: self.now(),
            });
            return;
        }
        let (_, serial) = self.link_mode(&stem);
        if serial == loaded {
            return self.start_link(
                LinkStarter::spawn(role.role(), link_port()),
                role.client_id(),
            );
        }
        // A core that refused its resume is on its default machine. A reload would resume the
        // refused file again and lose everything since, so refuse; the loaded mode still links.
        if self.snapshot.as_ref().is_some_and(|s| !s.resume_trusted()) {
            return self.refuse();
        }
        self.link_reload = Some((stem.clone(), serial));
        self.reload = Some(Reload {
            stem,
            role,
            cancelled: false,
            // SELECT toggles between two modes and the picked one is not loaded, so this is the
            // other.
            from: self.link_hardware.other(),
            from_serial: loaded,
            fallback: false,
        });
        self.game_menu = Some(GameMenu::Working {
            role,
            step: LinkStep::Radio,
            since: self.now(),
        });
    }

    /// Sends the seated cart back out refused when there is no game left to hand back, like
    /// `on_core_failed` but from fully seated.
    fn refuse_seated(&mut self) {
        self.close_game_menu();
        // The offer names a state only this cart's core can read, as in `eject`.
        self.pending = None;
        let caught = self.seat();
        let cart = match &mut self.phase {
            Phase::Playing { cart } => Some(std::mem::take(cart)),
            // No cart on a dark panel to send back; opening the lid lands on the shelf.
            Phase::Doze { cart } => {
                *cart = None;
                None
            }
            _ => None,
        };
        if let Some(cart) = cart {
            self.refuse_out(cart, caught);
        }
    }

    /// Ends the overlay and anything it had running. `LinkStarter` has no `Drop`, so a dropped
    /// host would leave its access point up for up to thirty seconds. Every path that ends the
    /// overlay (lid, eject, power off) comes through here.
    fn close_game_menu(&mut self) {
        self.game_menu = None;
        if let Some(mut starting) = self.starting.take() {
            starting.starter.cancel();
        }
        // Cool the driver the screen warmed, unless a session just started: that would take
        // the link down.
        if !self.link_active() {
            self.radio.ask(RadioJob::Cool);
        }
        // An uncollected switch has not touched the game and is dropped. One underway must still
        // end in a game or on the shelf; only its link is cancelled.
        let uncollected =
            self.link_reload.is_some() && self.reload.as_ref().is_some_and(|r| !r.fallback);
        if uncollected {
            self.link_reload = None;
            self.reload = None;
        } else if let Some(reload) = &mut self.reload {
            reload.cancelled = true;
        }
    }

    /// The role and start time of the Working state a result arrived in.
    fn working_role(&self, client_id: u16) -> (LinkRow, Millis) {
        match self.game_menu {
            Some(GameMenu::Working { role, since, .. }) => (role, since),
            _ => (LinkRow::from_client_id(client_id), self.now()),
        }
    }

    /// One message a frame, which is all the worker ever has.
    fn poll_link(&mut self) {
        // A power menu over it pauses the core, and a session must not begin under one. The
        // message waits in the queue.
        if self.power_menu.is_some() {
            return;
        }
        let Some(mut starting) = self.starting.take() else {
            return;
        };
        match starting.starter.poll() {
            None => self.starting = Some(starting),
            Some(LinkProgress::At(step)) => {
                if let Some(GameMenu::Working { role, since, .. }) = self.game_menu {
                    self.game_menu = Some(GameMenu::Working { role, step, since });
                }
                self.starting = Some(starting);
            }
            // The session starts now; LINKED is only the screen saying so.
            Some(LinkProgress::Ready(link)) => {
                let (role, worked) = self.working_role(starting.client_id);
                self.game_menu = Some(GameMenu::Linked {
                    role,
                    worked,
                    since: self.now(),
                    opened: false,
                });
                self.begin_link(starting.client_id);
                self.link_transport = Some((starting.client_id, Box::new(link)));
            }
            // The player cancelled: straight back to the game.
            Some(LinkProgress::Failed(LinkFail::Cancelled)) => self.game_menu = None,
            Some(LinkProgress::Failed(fail)) => {
                let (role, worked) = self.working_role(starting.client_id);
                self.game_menu = Some(GameMenu::Failed {
                    role,
                    fail,
                    worked,
                    since: self.now(),
                });
            }
        }
    }

    /// Writes the choice to the card, best effort. `self.core` is untouched: it is the seated
    /// cart's, and on the shelf none is seated.
    fn write_core(&self, core: Core) {
        let (Some(root), Some(cart)) = (
            self.root.clone(),
            self.shelf().carts.get(self.shelf().index),
        ) else {
            return;
        };
        if let Err(e) = slot_store::write_selected_core(&root, &cart.stem, core) {
            eprintln!("slot: core: could not write selected_core.ini: {e}");
        }
    }

    /// Every shutdown route (held button, doze timeout, critical battery) funnels here. It goes
    /// through the OS rather than the PMIC's six second cut, so the GPU module is unloaded and the
    /// device does not hang with its rails up. The LED goes dark at once.
    fn begin_power_off(&mut self) {
        // Idempotent: `doze_expired` is a level and `timers` calls here every frame while the
        // lid is shut. Re-arming `act_at` would push the deadline ahead forever.
        if self.powering_off {
            return;
        }
        // A power off pauses the core, which libretro's netpacket contract forbids in a session.
        // Guarded here so every route, including a critical battery, is covered.
        if self.link_active() {
            self.end_link();
        }
        self.close_game_menu();
        self.powering_off = true;
        self.act_at = self.now() + SHUTDOWN_SHOW_MS;
        self.set_led(LedState::Off);
    }

    /// The gauge, polled from `timers` and injected by tests. Only a positively asserted charge
    /// state suppresses the cutoff; unknown powers off at the threshold.
    pub fn on_battery(&mut self, b: Battery) {
        if b.percent > BATTERY_CRITICAL || self.powering_off {
            return;
        }
        if matches!(b.charge, Charge::Charging | Charge::Full) {
            return;
        }
        // A real power off, not a sleep: suspending a cell this empty only drains it slower.
        self.flush_resume();
        self.begin_power_off();
    }

    fn doze_expired(&self) -> bool {
        // Suspended while a session is live, so this device's idle timer does not drop the peer.
        if self.link_active() {
            return false;
        }
        let (Phase::Doze { .. }, Some(power)) = (&self.phase, &self.power) else {
            return false;
        };
        self.now().saturating_sub(self.dozed_at) >= power.timeout().as_millis() as Millis
    }

    /// Writes resume.state and the battery save, leaving the slot alone. Public for `Session`'s
    /// link reload, which must flush before the core is dropped.
    pub fn flush_resume(&mut self) {
        // The invariant is 60 s since the state was last durable, so even an attempt with
        // nothing to write moves the deadline.
        self.autosave_at = self.now() + AUTOSAVE_MS;
        let (Some(root), Some(snapshot), Some(cart)) = (&self.root, &self.snapshot, self.seated())
        else {
            return;
        };
        let Some(state) = snapshot.state() else {
            eprintln!("slot: flush: the core gave up no state");
            return;
        };
        let (state, sav) = trusted_write(snapshot.as_ref(), state, "flush");
        if let Err(e) = persist::flush(
            root,
            self.platform,
            self.core,
            cart,
            state.as_deref(),
            sav.as_deref(),
        ) {
            eprintln!("slot: flush: {e}");
        }
    }

    /// The ring for the seated cart. `None` with no content root.
    fn ring(&self) -> Option<StateRing> {
        let (Some(root), Some(cart)) = (&self.root, self.seated()) else {
            return None;
        };
        Some(StateRing::new(root, self.platform, self.core, cart))
    }

    fn seated(&self) -> Option<&str> {
        match &self.phase {
            Phase::Playing { cart } | Phase::Polaroids { cart } => Some(cart),
            _ => None,
        }
    }

    fn entries(&self) -> Vec<StateEntry> {
        self.ring().and_then(|r| r.list().ok()).unwrap_or_default()
    }

    /// An empty ring shakes rather than opening an empty screen, per spec section 4.
    fn open_polaroids(&mut self) {
        // The switcher pauses the core, which libretro's netpacket contract forbids in a
        // session.
        if self.link_active() {
            return self.refuse();
        }
        let entries = self.entries();
        if entries.is_empty() {
            return self.refuse();
        }
        let Phase::Playing { cart } = &mut self.phase else {
            return;
        };
        let cart = std::mem::take(cart);
        let mut p = Polaroids::new(entries);
        p.set_undo(self.undo_label());
        self.polaroids = Some(p);
        self.phase = Phase::Polaroids { cart };
        self.push_hint_faces();
    }

    fn close_polaroids(&mut self) {
        let Phase::Polaroids { cart } = &mut self.phase else {
            return;
        };
        let cart = std::mem::take(cart);
        self.polaroids = None;
        self.phase = Phase::Playing { cart };
    }

    fn load_selected(&mut self) {
        let state = self
            .polaroids
            .as_ref()
            .and_then(|p| p.selected())
            .map(|e| e.state.clone());
        // `Some(false)`: `load_file` refused and already shook, so closing now would look like
        // the pick landed.
        let refused = state.map(|state| self.load_file(&state)) == Some(false);
        if !refused {
            self.close_polaroids();
        }
    }

    /// Not undoable. The undo slot holds one save or load, and a pick off a screen showing the
    /// state is a decision, not a slip.
    fn delete_selected(&mut self) {
        let stamp = self
            .polaroids
            .as_ref()
            .and_then(|p| p.selected())
            .map(|e| e.stamp.clone());
        let (Some(stamp), Some(ring)) = (stamp, self.ring()) else {
            return;
        };
        if let Err(e) = ring.remove(&stamp) {
            eprintln!("slot: delete: {e}");
            return;
        }
        // An offer pointing at a deleted file would remove nothing and restore the evicted entry.
        if self.undo_targets(&stamp) {
            self.pending = None;
        }
        let Some(p) = &mut self.polaroids else {
            return;
        };
        p.remove_selected();
        if p.is_empty() {
            self.close_polaroids();
        }
    }

    fn undo_targets(&self, stamp: &str) -> bool {
        match self.pending.as_ref() {
            Some((PendingUndo::Save { stamp: pending, .. }, _)) => pending == stamp,
            // A load's undo holds the prior state in memory, so no file can invalidate it.
            _ => false,
        }
    }

    fn load_newest(&mut self) {
        let Some(newest) = self.entries().first().map(|e| e.state.clone()) else {
            return self.refuse();
        };
        self.load_file(&newest);
    }

    /// Every load from disk goes through here, so the session guard lives here once. Returns
    /// whether the load happened, so `load_selected` does not close over a refusal.
    fn load_file(&mut self, state: &Path) -> bool {
        // A state load would desync the peer with no way back.
        if !self.may_load_state() {
            self.refuse();
            return false;
        }
        let Some(snapshot) = &self.snapshot else {
            return false;
        };
        let bytes = match std::fs::read(state) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("slot: load: {e}");
                return false;
            }
        };
        // Taken before the load, the last moment there is anything to go back to.
        let prior = snapshot.state();
        snapshot.load(bytes);
        self.hud.toast(Toast::StateLoaded, self.now());
        if let Some(prior) = prior {
            self.pending = Some((PendingUndo::Load { prior }, self.now()));
        }
        true
    }

    /// `SELECT+R1` only. A state with no picture still saves. Refused when the core rejected its
    /// resume, or a full ring would evict a real entry for a default-machine placeholder.
    fn save_state(&mut self) {
        let (Some(ring), Some(snapshot)) = (self.ring(), &self.snapshot) else {
            return;
        };
        if !snapshot.resume_trusted() {
            eprintln!("slot: save: the core refused the resume it was given, not pushing a state");
            return self.refuse();
        }
        let Some(state) = snapshot.state() else {
            eprintln!("slot: save: the core gave up no state");
            return;
        };
        let thumb = snapshot.thumb().unwrap_or_default();
        let stamp = free_stamp(&ring, self.wall_secs());
        let evicted = doomed(&ring);
        if let Err(e) = ring.push(&state, &thumb, &stamp) {
            eprintln!("slot: save: {e}");
            return;
        }
        self.hud.toast(Toast::StateSaved, self.now());
        self.pending = Some((PendingUndo::Save { stamp, evicted }, self.now()));
    }

    pub fn undo_available(&self, now: Millis) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|(_, at)| now.saturating_sub(*at) <= UNDO_GRACE_MS)
    }

    /// The offer's text, or `None`. The grace period is read off the app's clock so the two agree.
    pub fn undo_label(&self) -> Option<&'static str> {
        if !self.undo_available(self.now()) {
            return None;
        }
        match self.pending.as_ref()?.0 {
            PendingUndo::Save { .. } => Some("undo save"),
            PendingUndo::Load { .. } => Some("undo load"),
        }
    }

    /// In `LEGEND` order, uploaded once.
    pub fn set_legend_faces(&mut self, faces: Vec<TexId>) {
        self.legend_faces = faces;
        self.push_hint_faces();
    }

    pub fn set_undo_face(&mut self, face: Option<TexId>) {
        self.undo_face = face;
        self.push_hint_faces();
    }

    /// The undo goes last, matching `hints`, so faces and hints share indices.
    fn push_hint_faces(&mut self) {
        let mut faces = self.legend_faces.clone();
        faces.extend(self.undo_face);
        if let Some(p) = &mut self.polaroids {
            p.set_hint_faces(faces);
        }
    }

    /// The HUD glyphs, in `Icon::ALL` order. Uploaded once.
    pub fn set_icon_faces(&mut self, faces: Vec<TexId>) {
        self.hud.set_icons(faces);
    }

    /// The two lines the HUD can say, in `Toast::ALL` order.
    pub fn set_toast_faces(&mut self, faces: Vec<TexId>) {
        self.hud.set_toasts(faces);
    }

    /// What the HUD is saying, or `None` once faded. Refusals shake instead.
    pub fn toast(&self) -> Option<Toast> {
        self.hud.said(self.now())
    }

    /// One shot, and it returns to the game as loading does. No redo.
    pub fn undo(&mut self, now: Millis) {
        if !self.undo_available(now) {
            self.pending = None;
            return;
        }
        // Undoing a load bypasses `load_file`, so the session guard is repeated here. The offer
        // is kept for after the session. Undoing a save touches no core state.
        if matches!(&self.pending, Some((PendingUndo::Load { .. }, _))) && !self.may_load_state() {
            return self.refuse();
        }
        let Some((what, _)) = self.pending.take() else {
            return;
        };
        match what {
            PendingUndo::Save { stamp, evicted } => self.undo_save(&stamp, evicted),
            PendingUndo::Load { prior } => {
                if let Some(snapshot) = &self.snapshot {
                    snapshot.load(prior);
                }
            }
        }
        self.close_polaroids();
    }

    fn undo_save(&self, stamp: &str, evicted: Option<(String, Vec<u8>, Vec<u8>)>) {
        let Some(ring) = self.ring() else {
            return;
        };
        if let Err(e) = ring.remove(stamp) {
            eprintln!("slot: undo: {e}");
            return;
        }
        let Some((stamp, state, thumb)) = evicted else {
            return;
        };
        if let Err(e) = ring.push(&state, &thumb, &stamp) {
            eprintln!("slot: undo: {e}");
        }
    }

    /// Entries in the switcher's order, newest first, for the binary to build faces from.
    pub fn polaroid_entries(&self) -> &[StateEntry] {
        match &self.polaroids {
            Some(p) => &p.entries,
            None => &[],
        }
    }

    pub fn set_polaroid_faces(&mut self, faces: Vec<TexId>) {
        if let Some(p) = &mut self.polaroids {
            p.set_faces(faces);
        }
    }

    /// The selected entry's stamp, which the binary watches to re-rasterise the title. Not the
    /// index: a delete moves a different entry under the same index.
    pub fn polaroid_stamp(&self) -> Option<&str> {
        self.polaroids
            .as_ref()
            .and_then(|p| p.selected())
            .map(|e| e.stamp.as_str())
    }

    /// What the top plate says. `now` is a wall-clock stamp, since entries are named by stamp.
    pub fn polaroid_title(&self, now: &str) -> String {
        self.polaroids
            .as_ref()
            .map_or_else(String::new, |p| p.title(now))
    }

    pub fn set_polaroid_title_face(&mut self, face: TexId) {
        if let Some(p) = &mut self.polaroids {
            p.set_title_face(Some(face));
        }
    }
}

/// Withholds whatever the core refused on open (`resume_trusted`, `save_ram_trusted`) from
/// `persist::flush`/`eject`, so a core on its default state never overwrites the file it
/// rejected. `verb` is only for the log line.
fn trusted_write(
    snapshot: &dyn Snapshot,
    state: Vec<u8>,
    verb: &str,
) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let state = if snapshot.resume_trusted() {
        Some(state)
    } else {
        eprintln!(
            "slot: {verb}: the core refused the resume it was given, not overwriting the saved one"
        );
        None
    };
    let sav = snapshot.save_ram();
    let sav = if snapshot.save_ram_trusted() {
        sav
    } else {
        if sav.is_some() {
            eprintln!(
                "slot: {verb}: the core refused the save ram it was given, not overwriting the saved one"
            );
        }
        None
    };
    (state, sav)
}

fn up(level: u8, step: u8, max: u8) -> u8 {
    level.saturating_add(step).min(max)
}

/// One step along `FF_SPEEDS`. Does not wrap, so a press at either end returns the current value.
fn ff_next(from: u8, right: bool) -> u8 {
    let at = FF_SPEEDS.iter().position(|&v| v == from).unwrap_or(0);
    let to = if right { at + 1 } else { at.saturating_sub(1) };
    FF_SPEEDS[to.min(FF_SPEEDS.len() - 1)]
}

/// The clock screen, opened on `utc` with `offset_min` chosen. The seed is the minute the picker
/// shows, which `confirm_clock` measures the change from.
fn clock_screen(utc: i64, offset_min: i16, from_menu: bool) -> Phase {
    Phase::SetClock {
        picker: ClockPicker::local(utc, offset_min),
        seed: utc - utc.rem_euclid(60),
        from_menu,
    }
}

/// The host's clock, used until `set_power` hands over the device's.
fn system_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The entry the next push will evict, read while it is still there. `None` until the ring fills.
fn doomed(ring: &StateRing) -> Option<(String, Vec<u8>, Vec<u8>)> {
    let entries = ring.list().ok()?;
    let oldest = entries.get(RING_MAX - 1)?;
    let (state, thumb) = ring.read(&oldest.stamp).ok()?;
    Some((oldest.stamp.clone(), state, thumb))
}

/// Two saves in one second would share a filename, so the second moves on a second.
fn free_stamp(ring: &StateRing, now: i64) -> String {
    let taken: Vec<String> = ring
        .list()
        .map(|l| l.into_iter().map(|e| e.stamp).collect())
        .unwrap_or_default();
    // Local, from the wall clock the captions are read against.
    let mut secs = now;
    let mut stamp = format_stamp(secs);
    while taken.contains(&stamp) {
        secs += 1;
        stamp = format_stamp(secs);
    }
    stamp
}

/// Where a block of `rows` menu rows starts, centred on the panel.
fn centred_top(rows: usize) -> f32 {
    (OUT_H as f32 - POWER_MENU_PITCH * rows as f32) / 2.0
}

/// Menu rows at the menu pitch from `top`, with a bar behind the selected one. Only the power menu
/// uses this. The bar is `edge`, the lightest theme colour: `recess` was too close to `housing`.
/// A rect, not a face per row, so no textures are uploaded near shutdown.
fn draw_menu_rows(
    faces: &[(TexId, u32, u32)],
    index: Option<usize>,
    top: f32,
    out: &mut Vec<Draw>,
) {
    for (row, (tex, w, h)) in faces.iter().copied().enumerate() {
        let y = top + POWER_MENU_PITCH * row as f32;
        let x = ((OUT_W as f32 - w as f32) / 2.0).round();
        if index == Some(row) {
            out.push(Draw::Rect {
                x,
                y: y + POWER_MENU_BAR_INSET,
                w: w as f32,
                h: POWER_MENU_PITCH - 2.0 * POWER_MENU_BAR_INSET,
                colour: slot_ui::edge(),
            });
        }
        out.push(Draw::Tex {
            x,
            y: y + (POWER_MENU_PITCH - h as f32) / 2.0,
            w: w as f32,
            h: h as f32,
            tex,
            alpha: 1.0,
        });
    }
}
