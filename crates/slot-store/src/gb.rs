use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Header title at 0x134, 11 bytes: later carts put a manufacturer code at 0x13F and the CGB flag
/// at 0x143, so reading 16 swallows both.
const TITLE_OFF: u64 = 0x134;
const TITLE_LEN: usize = 11;

/// CGB flag: 0x00 plain, 0x80 Colour-enhanced (runs on original hardware), 0xC0 Colour-only.
const CGB_OFF: u64 = 0x143;

/// The header title, or `None` when the field is empty, which valid ROMs do.
pub fn title(rom: &Path) -> Option<String> {
    let mut buf = [0u8; TITLE_LEN];
    read_at(rom, TITLE_OFF, &mut buf)?;
    let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
    let text = std::str::from_utf8(&buf[..end]).ok()?.trim();
    (!text.is_empty()).then(|| text.to_string())
}

pub fn cgb_flag(rom: &Path) -> Option<u8> {
    let mut buf = [0u8; 1];
    read_at(rom, CGB_OFF, &mut buf)?;
    Some(buf[0])
}

/// Which cartridge Nintendo manufactured for this rom, picked by the CGB flag. Not the file
/// extension: `.gb` files are often Colour-exclusive and `.gbc` files often DMG-compatible.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Class {
    /// Flag 0x00, "CGB Incompatible": a grey pak with the power-switch notch.
    Original,
    /// Flag 0x80, "CGB Compatible": a black pak in the notched shell. Never clear plastic.
    DualMode,
    /// Flag 0xc0, "CGB Exclusive": the clear pak, with no notch, so an original Game Boy will not
    /// power on with it.
    ColourOnly,
}

/// Any other flag, or a rom too short to have one, is an original pak: the commonest object is
/// the safe wrong answer.
pub fn class(rom: &Path) -> Class {
    match cgb_flag(rom) {
        Some(0xc0) => Class::ColourOnly,
        Some(0x80) => Class::DualMode,
        _ => Class::Original,
    }
}

fn read_at(rom: &Path, off: u64, buf: &mut [u8]) -> Option<()> {
    let mut f = File::open(rom).ok()?;
    f.seek(SeekFrom::Start(off)).ok()?;
    f.read_exact(buf).ok()
}
