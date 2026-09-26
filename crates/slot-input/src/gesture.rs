use crate::{Btn, Millis, RawEvent};

/// How long a *held* SELECT may still arm a chord with a second key. Generous on purpose:
/// 120 ms was not enough to land the second key of a chord.
///
/// This no longer withholds anything. It used to be how long SELECT waited before conceding it
/// was a plain press, which meant a held SELECT arrived at the game 600 ms late whether or not
/// a chord ever followed — the whole of a hold-piece gesture on a Game Boy cart. The press goes
/// straight through now (see `select_down`), so this is the arming window and nothing else.
pub const SELECT_CHORD_MS: Millis = 600;

/// The least time SELECT stays down on the pad, measured from the press. The core reads the
/// mask once a frame, so a press and release drained in one batch would never be seen.
pub const SELECT_TAP_MS: Millis = 50;
pub const MENU_TAP_MS: Millis = 250;
pub const MENU_DOUBLE_TAP_MS: Millis = 350;
pub const MENU_HOLD_MS: Millis = 1000;
pub const FF_DOUBLE_TAP_MS: Millis = 250;
/// How far apart the two volume keys may go down and still read as the mute chord. Neither
/// press is deferred for it.
pub const MUTE_CHORD_MS: Millis = 200;
/// Well short of the PMIC's six second cutoff (`pmu_powkey_off_time` in the device tree), so
/// slot powers off gracefully before the hardware cuts the rails.
pub const POWER_HOLD_MS: Millis = 1000;

/// How long a volume key is held before it repeats, and the repeat interval after that.
pub const VOLUME_REPEAT_DELAY_MS: Millis = 400;
pub const VOLUME_REPEAT_MS: Millis = 120;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Action {
    GbaDown(Btn),
    GbaUp(Btn),
    ShelfLeft,
    ShelfRight,
    Insert,
    Eject,
    Polaroids,
    SaveState,
    LoadState,
    RewindStart,
    RewindStop,
    FfStart,
    FfStop,
    BrightnessUp,
    BrightnessDown,
    BlueLightUp,
    BlueLightDown,
    VolumeUp,
    VolumeDown,
    /// A tap of MENU.
    QuickMenu,
    /// The in-game menu, off SELECT+MENU. Emitted on every screen; the app decides where it
    /// lands.
    GameMenu,
    MuteToggle,
    /// TEMPORARY. SELECT+Y, for judging colour correction in a game. Remove with its `chord`
    /// entry and the branch in `App::adjust`.
    ColourCorrectionToggle,
    /// The press itself, so the save state is flushed before a hold can reach the PMIC's
    /// cutoff.
    PowerPress,
    /// A short press, delivered on release so a press becoming a hold does not lock first.
    PowerTap,
    /// The hold threshold, while still down. Arms the shutdown; `PowerOff` commits it.
    PowerHold,
    /// Released after a hold. The graceful shutdown starts here.
    PowerOff,
    LidClose,
    LidOpen,
}

#[derive(Copy, Clone, Default)]
enum Select {
    #[default]
    Idle,
    /// Down and handed to the core. `chorded` means a chord already fired under this hold,
    /// which keeps the window open for the rest of it.
    Held { since: Millis, chorded: bool },
    /// Up, with the release held back until `due` so a very short tap is still polled.
    ReleaseDue(Millis),
}

#[derive(Default)]
pub struct Gestures {
    select: Select,
    /// Buttons swallowed by a chord, so their release is swallowed too.
    chord_held: u8,
    menu_down_at: Option<Millis>,
    menu_last_tap: Option<Millis>,
    menu_eject_fired: bool,
    power_down_at: Option<Millis>,
    power_hold_fired: bool,
    vol_up_at: Option<Millis>,
    vol_down_at: Option<Millis>,
    /// When the ramp last emitted. Cleared by both edges, so every press starts its own ramp.
    vol_up_ramp: Option<Millis>,
    vol_down_ramp: Option<Millis>,
    /// The pair has already fired. Cleared only once both keys are up, so a key tapped again
    /// under a held one is not a second chord.
    mute_fired: bool,
    ff_on: bool,
    ff_latched: bool,
    /// The press that established the latch, whose release must not clear it.
    ff_latching_press: bool,
    /// A press `ff_down` refused because L2 was rewinding. Its release must not count as one.
    r2_refused: bool,
    r2_last_release: Option<Millis>,
    rewinding: bool,
}

/// Whether a held key owes a repeat step at `now`.
fn ramp_due(down: Option<Millis>, last: Option<Millis>, now: Millis) -> bool {
    let Some(down) = down else {
        return false;
    };
    if now.saturating_sub(down) < VOLUME_REPEAT_DELAY_MS {
        return false;
    }
    last.is_none_or(|l| now.saturating_sub(l) >= VOLUME_REPEAT_MS)
}

impl Gestures {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether fast forward is latched rather than held. The latch is set by a release that
    /// emits nothing, so this is the only way to see it.
    pub fn ff_latched(&self) -> bool {
        self.ff_latched
    }

    /// Drops a latched fast forward, for callers that know the slot is empty. The latch is the
    /// only hold with no finger on it. A held fast forward is left alone.
    pub fn drop_ff_latch(&mut self) -> Vec<Action> {
        if !self.ff_latched {
            return Vec::new();
        }
        self.ff_clear()
    }

    pub fn feed(&mut self, ev: RawEvent, now: Millis) -> Vec<Action> {
        match ev {
            RawEvent::Down(b) => self.down(b, now),
            RawEvent::Up(b) => self.up(b, now),
        }
    }

    pub fn tick(&mut self, now: Millis) -> Vec<Action> {
        let mut out = Vec::new();
        // The release of a SELECT tap too short to have been polled.
        if let Select::ReleaseDue(due) = self.select {
            if now >= due {
                self.select = Select::Idle;
                out.push(Action::GbaUp(Btn::Select));
            }
        }
        if let Some(d) = self.menu_down_at {
            if !self.menu_eject_fired && now.saturating_sub(d) >= MENU_HOLD_MS {
                self.menu_eject_fired = true;
                out.push(Action::Eject);
            }
        }
        if let Some(d) = self.power_down_at {
            if !self.power_hold_fired && now.saturating_sub(d) >= POWER_HOLD_MS {
                self.power_hold_fired = true;
                out.push(Action::PowerHold);
            }
        }
        // No ramp under a fired mute chord: it would move the level the mute remembered.
        if !self.mute_fired {
            if ramp_due(self.vol_up_at, self.vol_up_ramp, now) {
                self.vol_up_ramp = Some(now);
                out.push(Action::VolumeUp);
            }
            if ramp_due(self.vol_down_at, self.vol_down_ramp, now) {
                self.vol_down_ramp = Some(now);
                out.push(Action::VolumeDown);
            }
        }
        out
    }

    fn down(&mut self, b: Btn, now: Millis) -> Vec<Action> {
        match b {
            Btn::Select => self.select_down(now),
            Btn::Menu => self.menu_down(now),
            // Flush on the press: a held POWER may be cut by the PMIC before any release.
            Btn::Power => {
                self.power_down_at = Some(now);
                self.power_hold_fired = false;
                vec![Action::PowerPress]
            }
            Btn::Lid => vec![Action::LidClose],
            Btn::VolUp | Btn::VolDown => self.volume_press(b, now),
            Btn::L2 => self.rewind_start(),
            Btn::R2 => self.ff_down(now),
            _ => {
                if let (true, Some((bit, action))) = (self.chording(now), chord(b)) {
                    self.mark_chorded();
                    self.chord_held |= bit;
                    return vec![action];
                }
                vec![Action::GbaDown(b)]
            }
        }
    }

    fn up(&mut self, b: Btn, now: Millis) -> Vec<Action> {
        match b {
            Btn::Select => self.select_up(now),
            Btn::Menu => self.menu_up(now),
            Btn::Power => self.power_up(),
            Btn::Lid => vec![Action::LidOpen],
            Btn::VolUp | Btn::VolDown => self.volume_release(b),
            Btn::L2 => self.rewind_stop(),
            Btn::R2 => self.ff_up(now),
            _ => {
                if let Some((bit, _)) = chord(b) {
                    if self.chord_held & bit != 0 {
                        self.chord_held &= !bit;
                        return Vec::new();
                    }
                }
                vec![Action::GbaUp(b)]
            }
        }
    }

    /// Whether a key at `now` is the second half of a chord: SELECT is down and either the
    /// window is open or a chord already fired under this hold (so held SELECT can ramp).
    /// Read from the clock, not tick state, so a batch drained after a stall is judged right.
    fn chording(&self, now: Millis) -> bool {
        match self.select {
            Select::Held { since, chorded } => {
                chorded || now.saturating_sub(since) < SELECT_CHORD_MS
            }
            _ => false,
        }
    }

    fn mark_chorded(&mut self) {
        if let Select::Held { chorded, .. } = &mut self.select {
            *chorded = true;
        }
    }

    /// The press goes straight to the game, and the chord arms off the same hold behind it.
    ///
    /// SELECT used to be withheld for the whole of `SELECT_CHORD_MS` so that a chord could
    /// swallow it whole, which meant a *held* SELECT reached the game 600 ms late whether or
    /// not a chord ever followed. For a Game Boy game that holds a piece with SELECT that is
    /// the entire gesture, and it is why turning chords off for Game Boy carts would not have
    /// helped: the latency was never the chord's, it was the waiting to find out.
    ///
    /// What it costs is that a chord now hands the game a SELECT press it did not mean to send.
    /// There is no way around that while the two share the button — the press is already out by
    /// the time the second key says what it was for, and taking it back would be a release the
    /// player never made, which is the exact shape of failure this area has been bitten by three
    /// times. The second key is still the chord's alone, on both of its edges.
    ///
    /// Plus whatever the press before it still owed: `select_up` can leave a release held back
    /// for `SELECT_TAP_MS`, and a press arriving inside that window overwrote the state owing
    /// it. That is a switch bouncing rather than anything a player does — 50 ms is three frames
    /// — but the release it lost left the core holding SELECT with no up ever coming, so the
    /// interrupting press hands it back itself, ahead of its own.
    fn select_down(&mut self, now: Millis) -> Vec<Action> {
        let mut out = Vec::new();
        if matches!(self.select, Select::ReleaseDue(_)) {
            out.push(Action::GbaUp(Btn::Select));
        }
        // Never press a held button again, or presses and releases stop balancing.
        let held = matches!(self.select, Select::Held { .. });
        self.select = Select::Held {
            since: now,
            chorded: false,
        };
        if !held {
            out.push(Action::GbaDown(Btn::Select));
        }
        out
    }

    /// The release always reaches the game. Only a tap shorter than `SELECT_TAP_MS` from the
    /// press is deferred, so the core gets to poll it.
    fn select_up(&mut self, now: Millis) -> Vec<Action> {
        let Select::Held { since, .. } = std::mem::take(&mut self.select) else {
            return Vec::new();
        };
        if now.saturating_sub(since) >= SELECT_TAP_MS {
            return vec![Action::GbaUp(Btn::Select)];
        }
        self.select = Select::ReleaseDue(since + SELECT_TAP_MS);
        Vec::new()
    }

    /// MENU never reaches the `chord` table, so its chord lives here. The chord check must stay
    /// ahead of the double tap check, or SELECT+MENU after a recent tap opens the switcher.
    fn menu_down(&mut self, now: Millis) -> Vec<Action> {
        if self.chording(now) {
            self.mark_chorded();
            // Clearing these stops the press arming an eject, makes its release silent in
            // `menu_up`, and keeps it out of any double tap.
            self.menu_down_at = None;
            self.menu_last_tap = None;
            return vec![Action::GameMenu];
        }
        if let Some(tap) = self.menu_last_tap {
            if now.saturating_sub(tap) <= MENU_DOUBLE_TAP_MS {
                self.menu_last_tap = None;
                // A double tap acts on the second press, so that press cannot also arm an eject.
                self.menu_down_at = None;
                return vec![Action::Polaroids];
            }
        }
        self.menu_down_at = Some(now);
        self.menu_eject_fired = false;
        Vec::new()
    }

    fn menu_up(&mut self, now: Millis) -> Vec<Action> {
        // `menu_down` already spent this press on a double tap or the SELECT+MENU chord.
        let Some(d) = self.menu_down_at.take() else {
            return Vec::new();
        };
        let ejected = self.menu_eject_fired;
        self.menu_eject_fired = false;
        let tapped = !ejected && now.saturating_sub(d) < MENU_TAP_MS;
        self.menu_last_tap = tapped.then_some(now);
        // Fire on the release rather than waiting out the double tap window, to avoid the lag.
        match tapped {
            true => vec![Action::QuickMenu],
            false => Vec::new(),
        }
    }

    fn power_up(&mut self) -> Vec<Action> {
        let held = self.power_hold_fired;
        self.power_down_at = None;
        self.power_hold_fired = false;
        vec![if held {
            Action::PowerOff
        } else {
            Action::PowerTap
        }]
    }

    /// The press always lands; a chord window would make volume feel slow. The mute pair is
    /// recognised after its presses and the app undoes them.
    fn volume_press(&mut self, b: Btn, now: Millis) -> Vec<Action> {
        let (mine, other, action) = match b {
            Btn::VolUp => (&mut self.vol_up_at, self.vol_down_at, Action::VolumeUp),
            _ => (&mut self.vol_down_at, self.vol_up_at, Action::VolumeDown),
        };
        *mine = Some(now);
        match b {
            Btn::VolUp => self.vol_up_ramp = None,
            _ => self.vol_down_ramp = None,
        }
        let mut out = vec![action];
        let paired = other.is_some_and(|t| now.abs_diff(t) <= MUTE_CHORD_MS);
        if paired && !self.mute_fired {
            self.mute_fired = true;
            out.push(Action::MuteToggle);
        }
        out
    }

    fn volume_release(&mut self, b: Btn) -> Vec<Action> {
        match b {
            Btn::VolUp => (self.vol_up_at, self.vol_up_ramp) = (None, None),
            _ => (self.vol_down_at, self.vol_down_ramp) = (None, None),
        }
        if self.vol_up_at.is_none() && self.vol_down_at.is_none() {
            self.mute_fired = false;
        }
        Vec::new()
    }

    fn rewind_start(&mut self) -> Vec<Action> {
        if self.rewinding {
            return Vec::new();
        }
        let mut out = Vec::new();
        out.extend(self.ff_clear());
        self.rewinding = true;
        out.push(Action::RewindStart);
        out
    }

    fn rewind_stop(&mut self) -> Vec<Action> {
        if !self.rewinding {
            return Vec::new();
        }
        self.rewinding = false;
        vec![Action::RewindStop]
    }

    fn ff_down(&mut self, now: Millis) -> Vec<Action> {
        if self.rewinding {
            // Remembered as refused, so its release is not half of a double tap.
            self.r2_refused = true;
            return Vec::new();
        }
        if self.ff_latched {
            // Any further press is the one whose release clears the latch.
            self.ff_latching_press = false;
        } else {
            let double = self
                .r2_last_release
                .is_some_and(|rel| now.saturating_sub(rel) <= FF_DOUBLE_TAP_MS);
            self.ff_latched = double;
            self.ff_latching_press = double;
        }
        if self.ff_on {
            return Vec::new();
        }
        self.ff_on = true;
        vec![Action::FfStart]
    }

    fn ff_up(&mut self, now: Millis) -> Vec<Action> {
        // A refused press must not be recorded as a release, or the next single R2 press
        // latches fast forward as if it were a double tap.
        if std::mem::take(&mut self.r2_refused) {
            return Vec::new();
        }
        self.r2_last_release = Some(now);
        if self.ff_latching_press {
            self.ff_latching_press = false;
            return Vec::new();
        }
        self.ff_clear()
    }

    fn ff_clear(&mut self) -> Vec<Action> {
        self.ff_latched = false;
        self.ff_latching_press = false;
        if !self.ff_on {
            return Vec::new();
        }
        self.ff_on = false;
        vec![Action::FfStop]
    }
}

fn chord(b: Btn) -> Option<(u8, Action)> {
    Some(match b {
        Btn::Up => (1, Action::BrightnessUp),
        Btn::Down => (2, Action::BrightnessDown),
        Btn::Left => (4, Action::BlueLightDown),
        Btn::Right => (8, Action::BlueLightUp),
        Btn::L1 => (16, Action::LoadState),
        Btn::R1 => (32, Action::SaveState),
        // TEMPORARY. See `Action::ColourCorrectionToggle`.
        Btn::Y => (64, Action::ColourCorrectionToggle),
        _ => return None,
    })
}
