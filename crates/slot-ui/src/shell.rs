use std::path::Path;

use slot_store::gb::Class;
use slot_store::{Cart, Platform, ShellFinish};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Finish {
    Solid,
    /// Clear plastic: the colour lightens and desaturates toward the rim.
    Translucent,
    /// Clear plastic with flecks moulded into it.
    Glitter,
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

/// Keyed on the region-free game code prefix, or on one region's whole code where that region
/// shipped other plastic; the whole code wins. A wrong key paints another game in the wrong
/// shell, which is worse than the grey default, so a region nobody has photographed is left out.
const EXACT: &[(&str, Shell)] = &[
    ("AXV", shell([0xc2, 0x33, 0x2e], Finish::Translucent)), // Pokemon Ruby
    ("AXP", shell([0x2f, 0x5c, 0xc0], Finish::Translucent)), // Pokemon Sapphire
    ("BPE", shell([0x24, 0x9c, 0x60], Finish::Translucent)), // Pokemon Emerald
    ("BPR", shell([0xd8, 0x52, 0x24], Finish::Translucent)), // Pokemon FireRed
    ("BPG", shell([0x63, 0xb0, 0x44], Finish::Translucent)), // Pokemon LeafGreen
    ("U3I", CLEAR),                                          // Boktai
    ("U32", CLEAR),                                          // Boktai 2
    ("U33", CLEAR),                                          // Shin Bokura no Taiyou
    ("V49", shell([0xa9, 0x51, 0x3b], Finish::Solid)),       // Drill Dozer
    ("KYGE", YOSHI),                                         // Yoshi Topsy-Turvy
    ("KYGP", YOSHI),                                         // Yoshi's Universal Gravitation
    ("RZW", shell([0x5f, 0x62, 0x64], Finish::Translucent)), // WarioWare: Twisted!
    ("RZWJ", shell([0xec, 0xee, 0xe8], Finish::Solid)),      // Mawaru Made in Wario
];

/// Colourless clear, with the solar sensor showing through.
const CLEAR: Shell = shell([0xd9, 0xdb, 0xd8], Finish::Translucent);
const YOSHI: Shell = shell([0x2f, 0x8f, 0x4e], Finish::Solid);

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

/// Game Boy paks outside their flag's plastic, keyed on the code at 0x13F the way GBA carts are.
const GB_CODES: &[(&str, Shell)] = &[
    ("AAU", shell([0xb3, 0x8b, 0x3a], Finish::Solid)), // Pokemon Gold
    ("AAUJ", DUAL_MODE_SHELL),                         // Japan's Gold is black
    ("AAX", shell([0xa9, 0xaa, 0xa7], Finish::Solid)), // Pokemon Silver
    ("AAXJ", DUAL_MODE_SHELL),                         // Japan's Silver is black
    ("BYT", CRYSTAL),                                  // Pokemon Crystal
    ("BXT", CRYSTAL),                                  // Pocket Monsters Crystal
    ("KTN", KIRBY),                                    // Kirby Tilt 'n' Tumble
    ("KKK", KIRBY),                                    // Korokoro Kirby
    ("KCE", shell([0x1f, 0x9f, 0xb6], Finish::Translucent)), // Command Master
    ("VCA", shell([0xf0, 0xa9, 0x5e], Finish::Translucent)), // Chee-Chai Alien
    ("VPHJ", shell([0xe2, 0xb4, 0x13], Finish::Solid)), // Pokemon Pinball, Japan only
    ("BMG", DUAL_MODE_SHELL),                          // Metal Gear Solid, Colour-only in black
];

/// Paks from before the code field, keyed on the 11-byte title. Only outside Japan: Japan's
/// copies share the title and shipped in the flag's plastic.
const GB_OVERSEAS_TITLES: &[(&str, Shell)] = &[
    ("POKEMON RED", shell([0xc0, 0x28, 0x2c], Finish::Solid)),
    ("POKEMON BLU", shell([0x2b, 0x3a, 0x88], Finish::Solid)),
    ("POKEMON YEL", shell([0xe9, 0xa8, 0x26], Finish::Solid)),
];

const CRYSTAL: Shell = shell([0x86, 0xb9, 0xbf], Finish::Glitter);
const KIRBY: Shell = shell([0xec, 0x94, 0xb4], Finish::Translucent);

/// A shell chosen in `System/cart_shell.ini` wins; otherwise, what plastic this cart shipped in:
/// by game code for GBA; for a Game Boy pak by its code or title, then its CGB flag. The folder
/// is not asked, since `.gb` and `.gbc` extensions routinely disagree with the flag.
pub fn shell_for(cart: &Cart) -> Shell {
    if let Some(choice) = cart.shell {
        let finish = match choice.finish {
            ShellFinish::Solid => Finish::Solid,
            ShellFinish::Clear => Finish::Translucent,
            ShellFinish::Glitter => Finish::Glitter,
        };
        return shell(choice.colour, finish);
    }
    match cart.platform {
        Platform::Gba => gba_shell_for(&cart.code),
        Platform::Gb | Platform::Gbc => gb_shell_for(&cart.rom),
    }
}

/// `code` is the four character game code.
pub fn gba_shell_for(code: &str) -> Shell {
    lookup(code, EXACT, FAMILY)
}

fn gb_shell_for(rom: &Path) -> Shell {
    let Some(h) = slot_store::gb::header(rom) else {
        return flag_shell(slot_store::gb::class(rom));
    };
    let by_title = || {
        GB_OVERSEAS_TITLES
            .iter()
            .find(|(t, _)| !h.japan && *t == h.title)
            .map(|(_, s)| *s)
    };
    by_code(&h.code, GB_CODES)
        .or_else(by_title)
        .unwrap_or_else(|| flag_shell(h.class()))
}

/// Three flag values, three plastics: grey, black, clear. An unreadable rom falls out as a
/// plain pak so the shelf still has a cart to draw.
fn flag_shell(class: Class) -> Shell {
    match class {
        Class::Original => DMG_SHELL,
        Class::DualMode => DUAL_MODE_SHELL,
        Class::ColourOnly => GB_CLEAR_SHELL,
    }
}

pub fn table_keys() -> Vec<&'static str> {
    EXACT.iter().map(|(k, _)| *k).collect()
}

/// Every Game Boy row's plastic, since those are keyed on header fields rather than one code.
pub fn gb_table_shells() -> Vec<Shell> {
    GB_CODES
        .iter()
        .chain(GB_OVERSEAS_TITLES)
        .map(|(_, s)| *s)
        .collect()
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
    if let Some(s) = by_code(code, exact) {
        return s;
    }
    if let Some(first) = code.as_bytes().first() {
        if let Some((_, s)) = family.iter().find(|(k, _)| k == first) {
            return *s;
        }
    }
    DEFAULT_SHELL
}

/// The whole four character code first, then its region-free prefix.
fn by_code(code: &str, table: &[(&str, Shell)]) -> Option<Shell> {
    let whole: String = code.chars().take(4).collect();
    let prefix: String = code.chars().take(3).collect();
    [whole, prefix]
        .iter()
        .find_map(|key| table.iter().find(|(k, _)| k == key).map(|(_, s)| *s))
}

/// The real plastics, named, for someone choosing a shell by hand. Each is looked up in the
/// tables above so a colour is written down once.
pub fn shell_presets() -> Vec<(&'static str, Shell)> {
    const GBA: &[(&str, &str)] = &[
        ("Pokémon Ruby", "AXV"),
        ("Pokémon Sapphire", "AXP"),
        ("Pokémon Emerald", "BPE"),
        ("Pokémon FireRed", "BPR"),
        ("Pokémon LeafGreen", "BPG"),
        ("Boktai clear", "U3I"),
        ("Drill Dozer red", "V49"),
        ("Yoshi green", "KYGE"),
        ("WarioWare smoke", "RZW"),
        ("Made in Wario white", "RZWJ"),
    ];
    const GB: &[(&str, &str)] = &[
        ("Pokémon Gold", "AAU"),
        ("Pokémon Silver", "AAX"),
        ("Pokémon Crystal", "BYT"),
        ("Kirby pink", "KTN"),
        ("Command Master turquoise", "KCE"),
        ("Chee-Chai Alien orange", "VCA"),
        ("Pinball yellow", "VPHJ"),
    ];
    let mut out = vec![
        ("Game Boy grey", DMG_SHELL),
        ("Game Boy black", DUAL_MODE_SHELL),
        ("Game Boy Color clear", GB_CLEAR_SHELL),
        ("Advance charcoal", DEFAULT_SHELL),
        ("Advance Video grey", FAMILY[0].1),
    ];
    out.extend(
        GBA.iter()
            .filter_map(|(n, k)| Some((*n, by_code(k, EXACT)?))),
    );
    out.extend(
        GB.iter()
            .filter_map(|(n, k)| Some((*n, by_code(k, GB_CODES)?))),
    );
    out.extend(
        [
            ("Pokémon Red", "POKEMON RED"),
            ("Pokémon Blue", "POKEMON BLU"),
            ("Pokémon Yellow", "POKEMON YEL"),
        ]
        .iter()
        .filter_map(|(n, t)| {
            let s = GB_OVERSEAS_TITLES.iter().find(|(k, _)| k == t)?.1;
            Some((*n, s))
        }),
    );
    out
}
