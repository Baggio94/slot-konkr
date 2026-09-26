use std::path::{Path, PathBuf};

/// The folders of a content root, including a platform subdirectory under each of the four
/// folders whose contents are filed per platform, so `ensure` creates them and the card teaches
/// its own layout to someone dropping files in over USB.
///
/// The card teaching its own layout is now the whole of the guidance there is. slot used to
/// sweep loose files into these folders on every boot; it does not any more, so an empty folder
/// with the right name is the only thing on the card that says where a file belongs, and every
/// folder a person has to put something in has to be here.
///
/// `Saves/` and `States/` are on that list for the first time, and the reason they were kept off
/// it went out with the sweep. The old rule was who places a file: a person places a rom and a
/// piece of label art by hand and needs somewhere to put each that names a platform — a `.gb` and
/// a `.gba` cart may share a stem, so `Tetris.png` alone does not say which cart it is the face
/// of — whereas nobody hand-placed a battery save or a save state, because slot wrote both and
/// the sweep moved whatever was already there. Now that a card is organised by hand, a person
/// bringing an old card across carries their own `.sav` files and their own `States/<core>/`
/// trees over, and those two folders need to say where they go as much as `Games/` does.
///
/// `States/` goes one level deeper than this — `States/<platform>/<core>/<stem>/` — and the core
/// level is deliberately not scaffolded: slot creates it on first write, and someone moving an
/// old card's `States/mgba/` wholesale into `States/GBA/` lands on exactly the right shape
/// without having to be told the core's spelling.
///
/// A card that has never held slot. has none of them, and every write path below assumes its
/// own is already there.
///
/// Parents come before their children: `ensure` creates each in turn, and so does the test
/// harness's own root.
pub const DIRS: [&str; 19] = [
    "BIOS",
    "Games",
    "Games/GBA",
    "Games/GB",
    "Games/GBC",
    "Labels",
    "Labels/GBA",
    "Labels/GB",
    "Labels/GBC",
    "Saves",
    "Saves/GBA",
    "Saves/GB",
    "Saves/GBC",
    "States",
    "States/GBA",
    "States/GB",
    "States/GBC",
    "System",
    "Wallpapers",
];

/// Best effort: an unmounted or read only card is an empty shelf, not a boot failure.
pub fn ensure(root: &Path) {
    for sub in DIRS {
        let _ = std::fs::create_dir_all(root.join(sub));
    }
}

/// The libretro system directory. Without `gba_bios.bin`, mGBA uses its HLE BIOS.
pub fn bios_dir(root: &Path) -> PathBuf {
    root.join("BIOS")
}

/// The only BIOS name both cores look for.
const BIOS_FILE: &str = "gba_bios.bin";

/// A GBA BIOS is 16 KB and starts with entry branch `EA000018`, little endian, so 0x18 first.
const BIOS_BYTES: u64 = 16 * 1024;
const BIOS_FIRST_BYTE: u8 = 0x18;

/// Whether the card has a real GBA BIOS, which decides whether gpSP boots through it (see
/// `core::apply_core_options`). Asked on every core load, so kept to one byte read.
///
/// Mirrors gpSP's own checks: it reads 16 KB unchecked and falls back to its built-in BIOS when
/// the first byte is not 0x18. Disagreeing with it gives a blank pause instead of the logo. No
/// checksum, so regional dumps still work.
pub fn has_real_bios(root: &Path) -> bool {
    let Ok(mut f) = std::fs::File::open(bios_dir(root).join(BIOS_FILE)) else {
        return false;
    };
    if !f.metadata().is_ok_and(|m| m.len() == BIOS_BYTES) {
        return false;
    }
    let mut first = [0u8; 1];
    std::io::Read::read_exact(&mut f, &mut first).is_ok() && first[0] == BIOS_FIRST_BYTE
}

pub fn saves_dir(root: &Path) -> PathBuf {
    root.join("Saves")
}
