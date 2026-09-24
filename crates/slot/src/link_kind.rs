//! Which link hardware a cart uses, so the link screen draws what the game expects. Mirrors
//! gpSP's `gpsp_serial=auto`; `serial_option` names the mode once the player switches.

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Cable,
    Wireless,
}

impl LinkKind {
    /// The one SELECT switches to.
    pub fn other(self) -> LinkKind {
        match self {
            LinkKind::Cable => LinkKind::Wireless,
            LinkKind::Wireless => LinkKind::Cable,
        }
    }
}

/// gpSP's `FLAGS_RFU` entries, from `gba_over.h` in `vendor/gpsp-src.tar.gz`.
const WIRELESS: [&str; 43] = [
    "B2WE", "B3AE", "B4UE", "B4UP", "B85A", "B85P", "BDGE", "BDGP", "BG3E", "BKRJ", "BMGD", "BMGE",
    "BMGF", "BMGI", "BMGJ", "BMGP", "BMGS", "BMGU", "BPED", "BPEE", "BPEF", "BPEI", "BPEJ", "BPES",
    "BPGD", "BPGE", "BPGF", "BPGI", "BPGJ", "BPGS", "BPRD", "BPRE", "BPRF", "BPRI", "BPRJ", "BPRS",
    "BR5E", "BR6E", "BRBE", "BRKE", "BTME", "BTMJ", "BTMP",
];

/// `code` and `title` are the header's, as `Cart` has them; `clean` is
/// `slot_store::header_clean` for the same ROM.
pub fn link_kind(code: &str, title: &str, clean: bool) -> LinkKind {
    if pokemon(code, title) {
        // gpSP links a Pokémon ROM by cable (as a hack) unless the header is standard, it is at
        // most 16 MB, the code is known and the title is exactly retail. Of retail games only
        // FireRed, LeafGreen and Emerald get the adapter.
        let retail = clean
            && WIRELESS.contains(&code)
            && ["POKEMON FIRE", "POKEMON LEAF", "POKEMON EMER"].contains(&title);
        return if retail {
            LinkKind::Wireless
        } else {
            LinkKind::Cable
        };
    }
    if WIRELESS.contains(&code) {
        LinkKind::Wireless
    } else {
        LinkKind::Cable
    }
}

/// The `gpsp_serial` to load a cart with to link over `chosen`; `auto` is gpSP's own pick. The
/// adapter is one mode for all games, but gpSP only speaks per-family cable protocols, so any
/// other game stays on `auto`.
pub fn serial_option(chosen: LinkKind, auto: LinkKind, code: &str, title: &str) -> &'static str {
    if chosen == auto {
        return "auto";
    }
    match chosen {
        LinkKind::Wireless => "rfu",
        LinkKind::Cable if pokemon(code, title) => "mul_poke",
        LinkKind::Cable if code.starts_with("AWR") => "mul_aw1",
        LinkKind::Cable if code.starts_with("AW2") => "mul_aw2",
        LinkKind::Cable => "auto",
    }
}

/// Whether gpSP can carry this cart's link. It has no generic cable: only the adapter, Pokémon
/// Gen3 and Advance Wars 1 and 2. Otherwise `SERIAL_MODE_AUTO` underflows `maxpl - 1` in
/// `netpacket_connected`, so peers join and every packet is silently dropped.
///
/// A clean header only changes which mode carries the cart, so it is not asked for.
pub fn link_carried(code: &str, title: &str) -> bool {
    WIRELESS.contains(&code)
        || pokemon(code, title)
        || code.starts_with("AWR")
        || code.starts_with("AW2")
}

/// The Pokémon family, by title or code, as gpSP tests it.
fn pokemon(code: &str, title: &str) -> bool {
    title.starts_with("POKEMON")
        || ["AXV", "AXP", "BPE", "BPR", "BPG"]
            .iter()
            .any(|p| code.starts_with(p))
}
