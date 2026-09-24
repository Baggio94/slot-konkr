use std::path::{Path, PathBuf};

use crate::atomic::atomic_write;

pub const BRIGHTNESS_MAX: u8 = 9;
pub const BLUE_LIGHT_MAX: u8 = 9;
pub const VOLUME_MAX: u8 = 100;

/// Real zone offsets, in minutes (some zones are off by 30 or 45). The card keeps UTC because
/// the base system's clock and ntp assume it.
pub const UTC_OFFSET_MIN: i16 = -720;
pub const UTC_OFFSET_MAX: i16 = 840;

/// The quick menu's fast forward ceilings, in game frames per refresh, left to right. The only
/// values `ff_speed` may hold, so 5 must be rejected. 8 was measured and bought nothing: 281 fps
/// against 6's 280 on mGBA, while 16.67 ms overruns rose from 1% to 7%.
pub const FF_SPEEDS: [u8; 4] = [2, 3, 4, 6];

/// Default fast forward ceiling, chosen on the device. Builds that only accept 2..=4 read it as
/// their default, 4x.
pub const FF_SPEED_DEFAULT: u8 = 6;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SlotState {
    /// Filename stem. `None` is an empty slot, which is the shelf.
    pub cart: Option<String>,
    pub brightness: u8,
    pub blue_light: u8,
    pub volume: u8,
    /// Separate from `volume`, so unmuting restores the last level.
    pub muted: bool,
    /// Whether the wall clock was ever confirmed. Marks first launch, so a fresh card must
    /// read false.
    pub clock_set: bool,
    /// Minutes to add to the card's UTC to get local time.
    pub utc_offset_min: i16,
    pub rumble: bool,
    /// The most game frames a screen refresh runs while fast forwarding: one of `FF_SPEEDS`.
    pub ff_speed: u8,
    /// Whether fast forward is heard, sped up, rather than dropped.
    pub ff_sound: bool,
    /// Whether the core simulates the original LCD's washed-out tint. Device-wide, since the
    /// quick menu only opens with no cart seated. See `slot::core::apply_core_options`.
    pub colour_correction: bool,
}

/// Not derived: all zeroes would boot with the backlight off and the mixer silent.
impl Default for SlotState {
    fn default() -> Self {
        SlotState {
            cart: None,
            brightness: 5,
            blue_light: 0,
            volume: 60,
            muted: false,
            clock_set: false,
            utc_offset_min: 0,
            rumble: true,
            ff_speed: FF_SPEED_DEFAULT,
            ff_sound: false,
            colour_correction: false,
        }
    }
}

fn state_path(root: &Path) -> PathBuf {
    root.join("System").join("slot.state")
}

pub fn read_slot_state(root: &Path) -> SlotState {
    std::fs::read(state_path(root))
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .and_then(|s| parse(&s))
        .unwrap_or_default()
}

pub fn write_slot_state(root: &Path, s: &SlotState) -> std::io::Result<()> {
    let text = format!(
        "cart={}\nbrightness={}\nblue_light={}\nvolume={}\nmuted={}\nclock_set={}\nutc_offset_min={}\nrumble={}\nff_speed={}\nff_sound={}\ncolour_correction={}\n",
        s.cart.as_deref().unwrap_or(""),
        s.brightness,
        s.blue_light,
        s.volume,
        s.muted as u8,
        s.clock_set as u8,
        s.utc_offset_min,
        s.rumble as u8,
        s.ff_speed,
        s.ff_sound as u8,
        s.colour_correction as u8
    );
    atomic_write(&state_path(root), text.as_bytes())
}

/// The original fields are all or nothing: a missing or out-of-range one means corruption, and
/// defaults would hide it. Unknown lines (a newer build) are skipped, and the later quick menu
/// fields fall back to their own defaults individually.
fn parse(text: &str) -> Option<SlotState> {
    let mut cart = None;
    let mut other_platform = false;
    let mut brightness = None;
    let mut blue_light = None;
    let mut volume = None;
    let mut muted = None;
    let mut clock_set = None;
    let mut utc_offset_min = None;
    let mut rumble = None;
    let mut ff_speed = None;
    let mut ff_sound = None;
    let mut colour_correction = None;
    for line in text.lines().filter(|l| !l.is_empty()) {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "cart" => cart = Some(value.to_string()),
            // Cards from builds that also ran Game Boy carts: a non-`gba` stem must not seat a
            // GBA cart of the same name.
            "cart_platform" => {
                other_platform = !value.is_empty() && !value.eq_ignore_ascii_case("gba")
            }
            "brightness" => brightness = Some(level(value, BRIGHTNESS_MAX)?),
            "blue_light" => blue_light = Some(level(value, BLUE_LIGHT_MAX)?),
            "volume" => volume = Some(level(value, VOLUME_MAX)?),
            "muted" => muted = Some(level(value, 1)? == 1),
            "clock_set" => clock_set = Some(level(value, 1)? == 1),
            "utc_offset_min" => utc_offset_min = Some(offset(value)?),
            "rumble" => rumble = flag(value),
            "ff_speed" => ff_speed = ff_speed_value(value),
            "ff_sound" => ff_sound = flag(value),
            "colour_correction" => colour_correction = flag(value),
            _ => {}
        }
    }
    let cart = cart?;
    let fallback = SlotState::default();
    Some(SlotState {
        cart: (!cart.is_empty() && !other_platform).then_some(cart),
        brightness: brightness?,
        blue_light: blue_light?,
        volume: volume?,
        muted: muted?,
        clock_set: clock_set?,
        utc_offset_min: utc_offset_min?,
        rumble: rumble.unwrap_or(fallback.rumble),
        ff_speed: ff_speed.unwrap_or(fallback.ff_speed),
        ff_sound: ff_sound.unwrap_or(fallback.ff_sound),
        colour_correction: colour_correction.unwrap_or(fallback.colour_correction),
    })
}

fn offset(value: &str) -> Option<i16> {
    value
        .parse()
        .ok()
        .filter(|n| (UTC_OFFSET_MIN..=UTC_OFFSET_MAX).contains(n))
}

/// One of `FF_SPEEDS`; anything else, including 5 and 7, reads as the default.
fn ff_speed_value(value: &str) -> Option<u8> {
    value.parse().ok().filter(|n| FF_SPEEDS.contains(n))
}

fn level(value: &str, max: u8) -> Option<u8> {
    value.parse().ok().filter(|n| *n <= max)
}

fn flag(value: &str) -> Option<bool> {
    level(value, 1).map(|n| n == 1)
}
