use std::path::Path;

use slot_store::gb::Class;
use slot_store::{Cart, Platform};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Finish {
    Solid,
    /// Clear plastic: the colour lightens and desaturates toward the rim.
    Translucent,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Shell {
    pub colour: [u8; 3],
    pub finish: Finish,
}

pub const DEFAULT_SHELL: Shell = Shell {
    colour: [0x35, 0x35, 0x3a],
    finish: Finish::Solid,
};

const fn shell(colour: [u8; 3], finish: Finish) -> Shell {
    Shell { colour, finish }
}

/// Keyed on the region-free game code prefix. Every code was read off a real header: a wrong
/// one paints another game in the wrong shell, which is worse than the grey default.
const EXACT: &[(&str, Shell)] = &[
    ("AXV", shell([0xc2, 0x33, 0x2e], Finish::Translucent)), // Pokemon Ruby
    ("AXP", shell([0x2f, 0x5c, 0xc0], Finish::Translucent)), // Pokemon Sapphire
    ("BPE", shell([0x24, 0x9c, 0x60], Finish::Translucent)), // Pokemon Emerald
    ("BPR", shell([0xd8, 0x52, 0x24], Finish::Translucent)), // Pokemon FireRed
    ("BPG", shell([0x63, 0xb0, 0x44], Finish::Translucent)), // Pokemon LeafGreen
];

/// Keyed on the first letter alone. `M` is the Game Boy Advance Video family.
const FAMILY: &[(u8, Shell)] = &[(b'M', shell([0xc6, 0xc6, 0xc9], Finish::Solid))];

/// The plain Game Boy Game Pak, CGB flag 0x00: grey, as in the reference photograph.
pub const DMG_SHELL: Shell = shell([0x9a, 0x97, 0x8f], Finish::Solid);

/// A Colour-enhanced pak, CGB flag 0x80: the black cartridge in the notched shell. Charcoal
/// rather than black, because the moulding is drawn by darkening the shell.
pub const DUAL_MODE_SHELL: Shell = shell([0x33, 0x30, 0x31], Finish::Solid);

/// A Colour-only pak, CGB flag 0xc0: smoke clear plastic, cooler than the grey pak so the two
/// differ by more than the lit rim.
pub const GB_CLEAR_SHELL: Shell = shell([0x7c, 0x7a, 0x8a], Finish::Translucent);

/// What plastic this cart shipped in: by game code for GBA, by CGB flag for a Game Boy pak.
/// The folder is not asked, since `.gb` and `.gbc` extensions routinely disagree with the flag.
pub fn shell_for(cart: &Cart) -> Shell {
    match cart.platform {
        Platform::Gba => gba_shell_for(&cart.code),
        Platform::Gb | Platform::Gbc => gb_shell_for(&cart.rom),
    }
}

/// `code` is the four character game code; only the first three are matched.
pub fn gba_shell_for(code: &str) -> Shell {
    lookup(code, EXACT, FAMILY)
}

/// Three flag values, three plastics: grey, black, clear. An unreadable rom falls out as a
/// plain pak so the shelf still has a cart to draw.
fn gb_shell_for(rom: &Path) -> Shell {
    match slot_store::gb::class(rom) {
        Class::Original => DMG_SHELL,
        Class::DualMode => DUAL_MODE_SHELL,
        Class::ColourOnly => GB_CLEAR_SHELL,
    }
}

pub fn table_keys() -> Vec<&'static str> {
    EXACT.iter().map(|(k, _)| *k).collect()
}

/// Checks lookup order on a fixture, since the shipping table has no code two rules both
/// claim. The order matters: an exact row is how a wrongly coloured family member gets fixed.
pub fn lookup_order_is_exact_then_family_then_default() -> bool {
    const A: Shell = shell([1, 1, 1], Finish::Solid);
    const B: Shell = shell([2, 2, 2], Finish::Solid);
    let exact = [("MSK", A)];
    let family = [(b'M', B)];
    lookup("MSKE", &exact, &family) == A
        && lookup("MPOE", &exact, &family) == B
        && lookup("ZZZZ", &exact, &family) == DEFAULT_SHELL
}

fn lookup(code: &str, exact: &[(&str, Shell)], family: &[(u8, Shell)]) -> Shell {
    let prefix: String = code.chars().take(3).collect();
    if let Some((_, s)) = exact.iter().find(|(k, _)| *k == prefix) {
        return *s;
    }
    if let Some(first) = code.as_bytes().first() {
        if let Some((_, s)) = family.iter().find(|(k, _)| k == first) {
            return *s;
        }
    }
    DEFAULT_SHELL
}
