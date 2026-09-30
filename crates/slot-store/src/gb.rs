use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Header title at 0x134, 11 bytes: later carts put a manufacturer code at 0x13F and the CGB flag
/// at 0x143, so reading 16 swallows both.
const TITLE_OFF: u64 = 0x134;
const TITLE_LEN: usize = 11;

/// CGB flag: 0x00 plain, 0x80 Colour-enhanced (runs on original hardware), 0xC0 Colour-only.
const CGB_OFF: u64 = 0x143;

/// The label's four character code, on carts late enough to have one.
const CODE_OFF: usize = 0x13f;
/// 0x00 for a cart sold in Japan, 0x01 anywhere else.
const DEST_OFF: usize = 0x14a;
/// Everything through the header checksum.
pub const HEADER_LEN: usize = 0x150;

/// The header fields that tell one pak's plastic from another's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    /// The 11-byte title field, trimmed; empty is valid.
    pub title: String,
    /// Empty unless 0x13F holds four capitals or digits: a 16-byte title runs through it.
    pub code: String,
    pub cgb: u8,
    pub japan: bool,
}

impl Header {
    /// `None` for fewer than `HEADER_LEN` bytes.
    pub fn parse(bytes: &[u8]) -> Option<Header> {
        let bytes = bytes.get(..HEADER_LEN)?;
        let title = &bytes[TITLE_OFF as usize..][..TITLE_LEN];
        let end = title.iter().position(|b| *b == 0).unwrap_or(TITLE_LEN);
        let code = &bytes[CODE_OFF..CODE_OFF + 4];
        let is_code = code
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit());
        Some(Header {
            title: String::from_utf8_lossy(&title[..end]).trim().to_string(),
            code: if is_code {
                String::from_utf8_lossy(code).into_owned()
            } else {
                String::new()
            },
            cgb: bytes[CGB_OFF as usize],
            japan: bytes[DEST_OFF] == 0,
        })
    }

    pub fn class(&self) -> Class {
        class_of(Some(self.cgb))
    }
}

pub fn header(rom: &Path) -> Option<Header> {
    let mut buf = [0u8; HEADER_LEN];
    read_at(rom, 0, &mut buf)?;
    Header::parse(&buf)
}

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
    class_of(cgb_flag(rom))
}

fn class_of(flag: Option<u8>) -> Class {
    match flag {
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
