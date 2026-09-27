use std::path::{Path, PathBuf};

/// The folders of a content root, with a platform subdirectory under each per-platform folder.
/// The empty folders are the card's only guide to where hand-placed files go. Parents come before
/// children. `States/` is not scaffolded to the core level: slot creates that on first write.
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
