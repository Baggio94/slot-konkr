use std::path::PathBuf;
use std::time::Duration;

use slot_input::{Action, Gestures, Millis, RawEvent};
use slot_retro::Rumble;
use slot_store::Platform;
use slot_ui::FfState;

use crate::app::{App, Phase};
use crate::audio::{open_sink, AudioSink, Ring, Sfx, GBA_HZ};
use crate::core::open_core;
use crate::emu::{CoreState, EmuHandle, Speed};
use crate::frames::FrameRef;
use crate::input::Pad;
use crate::persist;

/// Everything the frontend is that is not a window: the app, the core behind it, and the
/// gesture layer between the two. The binary owns the GL and hands raw events in.
pub struct Session {
    root: PathBuf,
    app: App,
    emu: Option<EmuHandle>,
    /// Outlives every cart: the insert click plays before there is a core to own a sink.
    sink: Box<dyn AudioSink>,
    gestures: Gestures,
    pad: Pad,
    rewinding: bool,
    fast: bool,
    /// Last motor value set. On the device each set is a hardware write, and the core repeats
    /// the same value most frames.
    motor: u16,
    /// A reload for a link is underway and `App` is waiting to hear whether it loaded.
    reloading: bool,
    /// The display drives each emulator's frame clock; see `EmuHandle::set_driven`.
    driven: bool,
}

impl Session {
    pub fn boot(root: PathBuf) -> Self {
        let mut sink: Box<dyn AudioSink> = open_sink();
        // A device that refuses the rate still opens; the worker resamples to what it took.
        if let Err(e) = sink.open(GBA_HZ) {
            eprintln!("slot: audio: {e}");
        }
        Session {
            app: App::boot(&root),
            root,
            emu: None,
            sink,
            gestures: Gestures::new(),
            pad: Pad::default(),
            rewinding: false,
            fast: false,
            motor: 0,
            reloading: false,
            driven: false,
        }
    }

    /// Mixed in over whatever the game is already playing, so it lands with the thing on
    /// screen rather than a buffer behind it.
    pub fn play_sfx(&mut self, sfx: Sfx) {
        let ring = self.sink.ring();
        let rate = ring.sample_rate();
        if rate == 0 {
            return;
        }
        let mut samples = sfx.render(rate);
        // The worker levels core audio, but this path bypasses it, so apply the volume here.
        crate::audio::volume::apply(&mut samples, self.app.output_volume());
        ring.mix(&samples);
    }

    pub fn audio_queued(&self) -> usize {
        self.sink.ring().queued_frames()
    }

    /// The queued audio itself, for tests.
    pub fn audio_ring(&self) -> std::sync::Arc<Ring> {
        self.sink.ring()
    }

    /// Straight to the motor, skipping the phase. Only `sync_rumble` and a caller standing
    /// in for a cart that buzzes have any business here.
    pub fn rumble(&mut self, strength: u16) {
        if strength == self.motor {
            return;
        }
        self.motor = strength;
        self.app.set_rumble(strength);
    }

    /// The core's end of the motor, or nothing when the slot is empty.
    pub fn core_rumble(&self) -> Option<&Rumble> {
        self.emu.as_ref().map(EmuHandle::rumble)
    }

    pub fn app(&self) -> &App {
        &self.app
    }

    pub fn app_mut(&mut self) -> &mut App {
        &mut self.app
    }

    /// The emulator thread's handle, or `None` before a cart spawns one. `App` never touches it.
    pub fn emu(&self) -> Option<&EmuHandle> {
        self.emu.as_ref()
    }

    /// Hand every emulator's frame clock to the display, which then calls `step_emulator` once
    /// per present. The device loop does; the host's window can refresh at any rate.
    pub fn set_driven(&mut self, driven: bool) {
        self.driven = driven;
        if let Some(emu) = &self.emu {
            emu.set_driven(driven);
        }
    }

    /// One present of `present`: a locked emulator runs its frame, and this waits for it, at most
    /// `timeout`. False when no frame came, including when the emulator keeps its own clock.
    pub fn step_emulator(&self, present: Duration, timeout: Duration) -> bool {
        self.emu.as_ref().is_some_and(|emu| {
            emu.tick(present);
            emu.wait_frame(timeout)
        })
    }

    pub fn frame(&self) -> Option<FrameRef> {
        self.emu.as_ref().and_then(|e| e.latest_frame())
    }

    /// A cart in the slot is a core running, so this is also "is there a game layer".
    pub fn has_core(&self) -> bool {
        self.emu.is_some()
    }

    /// Whether the game layer may be drawn at all. Gate the draw on this, never on
    /// `has_core`: a handle exists before its worker has produced anything.
    pub fn frame_ready(&self) -> bool {
        self.emu.as_ref().is_some_and(EmuHandle::frame_ready)
    }

    /// Frames the current core has produced. Zero while it is loaded but paused, which is
    /// what the insert relies on.
    pub fn frames_published(&self) -> u64 {
        self.emu.as_ref().map_or(0, EmuHandle::published_count)
    }

    /// The speed the worker last observed, or `None` with no core. See
    /// `EmuHandle::observed_speed`.
    pub fn observed_speed(&self) -> Option<Speed> {
        self.emu.as_ref().map(EmuHandle::observed_speed)
    }

    /// Diagnostic: frames taken from the handoff buffer. Must equal what the renderer received.
    pub fn frames_taken(&self) -> u64 {
        self.emu.as_ref().map_or(0, EmuHandle::frames_taken)
    }

    /// Also gated on the screen being up: a core publishes long before the cart is home, and
    /// the game would otherwise play behind the cart.
    pub fn game_visible(&self) -> bool {
        self.app.game_visible()
    }

    /// Called every frame whether or not anything was pressed: the gesture windows expire on
    /// the tick, not on an event.
    pub fn feed(&mut self, events: impl IntoIterator<Item = RawEvent>, now: Millis) {
        let mut actions = Vec::new();
        for ev in events {
            actions.extend(self.gestures.feed(ev, now));
        }
        actions.extend(self.gestures.tick(now));
        for action in actions {
            self.act(action);
        }
        self.sync_pad();
    }

    /// Releases every button slot owns, then hands the pad to the core. Ownership moves with the
    /// phase, not on an edge, so a shoulder held across a cart seating would stay held in the core
    /// unless this re-asks every `feed` and `update`. Releases one button, never clears the pad.
    fn sync_pad(&mut self) {
        for btn in self.app.taken_buttons() {
            self.pad.apply(Action::GbaUp(*btn));
        }
        if let Some(emu) = &self.emu {
            emu.set_input(self.pad.mask());
        }
    }

    /// Runs `f` on `App` and, if it ended a link, tells the emulator thread, which owns the
    /// transport. Every call into `App` that can end a session must go through here.
    fn bridge_link(&mut self, f: impl FnOnce(&mut App)) {
        let had_link = self.app.link_active();
        f(&mut self.app);
        if had_link && !self.app.link_active() {
            if let Some(emu) = &self.emu {
                // Stops the core's link and drops the transport, closing the wire.
                emu.end_link();
            }
        }
    }

    fn act(&mut self, action: Action) {
        if trace() {
            eprintln!("slot: {action:?} in {:?}", self.app.phase());
        }
        match action {
            Action::RewindStart => self.rewinding = true,
            Action::RewindStop => self.rewinding = false,
            Action::FfStart => self.fast = true,
            Action::FfStop => self.fast = false,
            _ => {}
        }
        // The presses that open and dismiss a menu both belong to it, so check both sides of
        // `apply`.
        let menu = self.overlaid();
        self.bridge_link(|app| app.apply(action));
        if matches!(
            action,
            Action::VolumeUp | Action::VolumeDown | Action::MuteToggle
        ) {
            if let Some(emu) = &self.emu {
                emu.set_volume(self.app.output_volume());
            }
        }
        if menu || self.overlaid() {
            self.pad.clear();
        } else if self.app.takes_from_the_game(action) {
            // Released, not withheld: a shoulder pressed before slot took it would otherwise stay
            // held in the core. One button only, so a held direction stays down.
            if let Action::GbaDown(btn) | Action::GbaUp(btn) = action {
                self.pad.apply(Action::GbaUp(btn));
            }
        } else {
            self.pad.apply(action);
        }
        // Now, not next frame: an eject or doze may be the process's last act, and a running
        // motor outlives it.
        self.sync_rumble();
    }

    pub fn update(&mut self, dt: f32) {
        self.bridge_link(|app| app.update(dt));
        // Hand a new link's transport to the emulator thread. Before `sync_speed`, so the game
        // is running again on the frame the overlay closes.
        if let Some((client_id, transport)) = self.app.take_link_transport() {
            match &self.emu {
                // Only the emulated link loads the core in link mode, so `link_player` tells
                // the two routes apart.
                Some(emu) => match self.app.link_player() {
                    Some(player) => emu.begin_cable(player, transport),
                    None => emu.begin_link(client_id, transport),
                },
                // Dropping the transport closes the socket.
                None => eprintln!("slot: link: a transport arrived with no core to run it"),
            }
        }
        // Colour correction is the one option changeable with a game on screen. The key is
        // the seated core's, not the menu's.
        if let Some(on) = self.app.take_colour_correction() {
            if let Some((key, value)) = crate::core::colour_option(self.app.core(), on) {
                if let Some(emu) = &self.emu {
                    emu.set_option(key, value);
                }
            }
        }
        // A link picked in a mode the running core was not loaded with.
        if let Some((stem, serial)) = self.app.take_link_reload() {
            self.reload_for_link(&stem, serial);
        }
        // Check `peer_ended` first: a peer ending deliberately sends word and then drops the
        // wire, so both can be up on one frame. `peer_lost` only breaks the badge; `App::timers`
        // ends that session after `LINK_LOST_MS`.
        if self.app.link_active() {
            if self.emu.as_ref().is_some_and(EmuHandle::bios_mismatch) {
                self.bridge_link(|app| app.bios_mismatch());
            } else if self.emu.as_ref().is_some_and(EmuHandle::peer_ended) {
                self.bridge_link(|app| app.peer_ended());
            } else if self.emu.as_ref().is_some_and(EmuHandle::link_lost) {
                self.app.peer_lost();
            }
        }
        if let Some(sfx) = self.app.take_sfx() {
            self.play_sfx(sfx);
        }
        self.sync_core();
        self.sync_reload();
        // A latched fast forward must not outlive the cart, or the next game boots fast. Keyed
        // on "no core" so a refused load clears it too; through `act` so flag, speed and badge
        // end together.
        if !self.has_core() {
            for action in self.gestures.drop_ff_latch() {
                self.act(action);
            }
        }
        // After the core sync: a handle spawned or dropped this frame has published nothing
        // the renderer may show.
        self.app
            .set_game_ready(self.emu.as_ref().is_some_and(EmuHandle::has_published));
        self.sync_speed();
        self.sync_rewind_hud();
        self.sync_ff_hud();
        self.sync_rumble();
        // Last, after the phase has moved and after a core spawned this frame exists to be told:
        // this is the frame a cart seats on, and whoever owns a shoulder now owns it from here.
        self.sync_pad();
    }

    /// The one place the core's motor request reaches hardware. The phase and the quick menu's
    /// rumble setting override it with 0.
    fn sync_rumble(&mut self) {
        let want = match &self.emu {
            Some(emu) if self.playing() && self.app.rumble_enabled() => emu.rumble().strength(),
            _ => 0,
        };
        self.rumble(want);
    }

    /// The badge tracks the speed actually run, not the button, so it drops whenever fast
    /// forward is withheld.
    fn sync_ff_hud(&mut self) {
        let ff = match (self.actually_fast_forwarding(), self.gestures.ff_latched()) {
            (false, _) => FfState::Off,
            (true, false) => FfState::Held,
            (true, true) => FfState::Latched,
        };
        self.app.set_ff(ff);
    }

    /// The bar shows only while a rewind is really happening, not merely while L2 is held.
    fn sync_rewind_hud(&mut self) {
        let fill = self
            .actually_rewinding()
            .then(|| self.emu.as_ref().map(EmuHandle::rewind_fill))
            .flatten();
        match fill {
            Some(fill) => self.app.show_rewind(fill),
            None => self.app.hide_rewind(),
        }
    }

    /// Shared by `sync_speed` and `sync_rewind_hud` so they cannot disagree.
    fn actually_rewinding(&self) -> bool {
        self.rewinding && self.playing() && self.app.may_rewind()
    }

    /// Shared by `sync_speed` and `sync_ff_hud` so they cannot disagree.
    fn actually_fast_forwarding(&self) -> bool {
        self.fast && self.playing() && self.app.may_fast_forward()
    }

    fn inserting(&self) -> bool {
        matches!(self.app.phase(), Phase::Inserting { .. })
    }

    fn showing_polaroids(&self) -> bool {
        matches!(self.app.phase(), Phase::Polaroids { .. })
    }

    /// Screens whose buttons are theirs, not the game's. Pausing is not masking: a press taken
    /// while paused and released after would reach the game as a held button.
    fn overlaid(&self) -> bool {
        self.showing_polaroids() || self.held()
    }

    /// Whether the game is live and in charge of the device. The phase stays `Playing` under
    /// the power menu and shutdown screen, which are overlays, so the phase alone is not enough.
    fn playing(&self) -> bool {
        matches!(self.app.phase(), Phase::Playing { .. }) && !self.held()
    }

    /// The screens that have taken the panel away from a cart still seated. The switcher is
    /// not one of them: it has its own phase and `sync_speed` names it separately.
    fn held(&self) -> bool {
        self.app.power_menu().is_some() || self.app.game_menu_open() || self.app.shutting_down()
    }

    fn dozing(&self) -> bool {
        matches!(self.app.phase(), Phase::Doze { .. })
    }

    fn ejecting(&self) -> bool {
        matches!(self.app.phase(), Phase::Ejecting { .. })
    }

    /// The switcher pauses the game rather than dimming a live one. Paused publishes no
    /// frames, so the compositor keeps showing the last one behind the cards.
    fn sync_speed(&self) {
        if let Some(emu) = &self.emu {
            // Before the speed, so the first fast present uses them. A ceiling on core frames
            // per present, not a multiplier.
            emu.set_fast_steps(u32::from(self.app.ff_speed()));
            emu.set_ff_sound(self.app.ff_sound());
            // Paused while inserting so the BIOS intro starts as the screen comes on, and while
            // ejecting so the game stops being heard once the player ends it.
            //
            // `held()` does not pause a live link: gpSP drops a silent peer after 240 frames
            // (about four seconds), and real hardware cannot pause the other machine either.
            // `overlaid` keeps menu buttons out of the game.
            emu.set_speed(
                if self.inserting()
                    || self.ejecting()
                    || self.showing_polaroids()
                    || self.dozing()
                    || (self.held() && !self.app.link_active())
                {
                    Speed::Paused
                } else if self.actually_fast_forwarding() {
                    Speed::Fast
                } else {
                    Speed::Normal
                },
            );
            // Rewinding one linked device desyncs the other with no way back.
            emu.set_rewinding(self.actually_rewinding());
        }
    }

    /// `SLOT_NO_CORE=1` leaves the slot on screen with the cart in it and never starts a
    /// game, so the insert can be watched at full length. Eject and insert again to replay.
    fn no_core() -> bool {
        std::env::var_os("SLOT_NO_CORE").is_some_and(|v| v != "0")
    }

    /// The core exists exactly while a cart is in the slot. Loading it is what the insert
    /// animation is hiding, so the spawn happens on the way in, not on arrival.
    fn sync_core(&mut self) {
        if Self::no_core() {
            return;
        }
        let stem = match self.app.phase() {
            Phase::Shelf => {
                self.emu = None;
                return;
            }
            Phase::Inserting { cart, .. } => cart.clone(),
            _ => return,
        };
        if self.emu.is_none() {
            let (_, serial) = self.app.link_mode(&stem);
            self.spawn_core(&stem, serial);
        }
        match self.emu.as_ref().map(EmuHandle::state) {
            Some(CoreState::Loading) => {}
            Some(CoreState::Ready) => self.app.on_core_ready(),
            // Drop the dead worker to free the core: libretro allows only one.
            Some(CoreState::Failed) | None => {
                self.emu = None;
                self.app.on_core_failed();
            }
        }
    }

    /// `serial` is the `gpsp_serial` the core loads with.
    fn spawn_core(&mut self, stem: &str, serial: &'static str) {
        // The seated cart, not a stem lookup: a `.gb` and a `.gba` can share a stem.
        let Some((rom, platform)) = self
            .app
            .seated_cart()
            .filter(|c| c.stem == stem)
            .map(|c| (c.rom.clone(), c.platform))
        else {
            return;
        };
        // Resolved once, here, and stored in `App`: it picks the dylib, the resume directory and
        // every later flush, so deriving it twice could let the two drift apart.
        let core = slot_store::core_for_platform(&self.root, stem, platform);
        self.app.set_core(core);
        // Off the scanned `Cart`, not the stem, for the same shared-stem reason.
        self.app.set_platform(platform);
        // Read for every cart, so a GBA cart never inherits the last Game Boy cart's mode.
        self.app
            .set_video_mode(crate::video_mode::video_mode_for(&self.root, stem));
        // gpSP reads its link mode only while a game loads, so what this hands the core is what
        // the game links over from here on, and what `App` compares a picked link against.
        self.app.set_link_loaded(serial);
        // A clean start skips the state but leaves it on the card.
        let resume = (!self.app.starting_clean())
            .then(|| persist::read_resume(&self.root, platform, core, stem))
            .flatten();
        let player = self.app.link_player();
        let opened = open_core(
            &self.root,
            core,
            serial,
            self.app.colour_correction(),
            player,
        );
        // A refusal from the mock means a missing dylib, not a bad resume state.
        self.app.set_named_core(opened.named);
        let sav = persist::read_sav(&self.root, platform, stem);
        let ring = self.sink.ring();
        // Game Boy link mode keeps no link state.
        let emu = match player.filter(|_| platform == Platform::Gba) {
            Some(p) => EmuHandle::spawn_linked(opened.core, rom, ring, sav, resume, p),
            None => EmuHandle::spawn(opened.core, rom, ring, sav, resume),
        };
        // A cart seated after the level was lowered has to start there, not at full.
        emu.set_volume(self.app.output_volume());
        emu.set_driven(self.driven);
        self.app.set_snapshot(Box::new(emu.snapshot()));
        self.emu = Some(emu);
    }

    /// Loads the seated game again with `serial`, resuming from a fresh flush. The old core is
    /// dropped first: dropping joins its worker, and libretro allows only one core.
    fn reload_for_link(&mut self, stem: &str, serial: &'static str) {
        eprintln!("slot: link: loading {stem} again with gpsp_serial={serial}");
        if self.emu.is_some() {
            self.app.flush_resume();
        }
        self.emu = None;
        self.spawn_core(stem, serial);
        self.reloading = true;
    }

    /// Follows a link reload to its end. On failure `App` decides what follows: first a retry in
    /// the previous mode, started here so no frame has a seated cart without a core; then eject.
    fn sync_reload(&mut self) {
        if !self.reloading {
            return;
        }
        match self.emu.as_ref().map(EmuHandle::state) {
            Some(CoreState::Loading) => {}
            Some(CoreState::Ready) => {
                self.reloading = false;
                self.app.link_reload_done();
            }
            Some(CoreState::Failed) | None => {
                self.reloading = false;
                self.emu = None;
                self.app.link_reload_failed();
                if let Some((stem, serial)) = self.app.take_link_reload() {
                    self.reload_for_link(&stem, serial);
                }
            }
        }
    }
}

/// `SLOT_TRACE=1` prints every semantic action and the phase it landed in, to check whether a
/// key reaches the window at all.
pub(crate) fn trace() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("SLOT_TRACE").is_some())
}
