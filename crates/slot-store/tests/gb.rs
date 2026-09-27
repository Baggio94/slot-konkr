use slot_store::gb::Class;
use std::path::PathBuf;

fn card_rom(rel: &str) -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../sdcard/Games")
        .join(rel);
    p.is_file().then_some(p)
}

/// A plain Game Boy cart: CGB flag 0x00, and a title in the old 11-byte field.
#[test]
fn a_dmg_cart_reads_its_title_and_flag() {
    let Some(rom) = card_rom("GB/Tetris Rosy Retrospection.gb") else {
        eprintln!("no Game Boy ROM on the card: skipping");
        return;
    };
    assert_eq!(slot_store::gb::title(&rom).as_deref(), Some("TETRIS"));
    assert_eq!(slot_store::gb::cgb_flag(&rom), Some(0x00));
    assert_eq!(slot_store::gb::class(&rom), Class::Original);
}

/// A Colour-only cart: flag 0xc0, and an all-zero title, which is not a malformed ROM.
#[test]
fn a_colour_only_cart_has_no_title_and_that_is_fine() {
    let Some(rom) = card_rom("GBC/Tetris Chromatic.gbc") else {
        eprintln!("no Game Boy Color ROM on the card: skipping");
        return;
    };
    assert_eq!(
        slot_store::gb::title(&rom),
        None,
        "an empty title must read as none"
    );
    assert_eq!(slot_store::gb::cgb_flag(&rom), Some(0xc0));
    assert_eq!(slot_store::gb::class(&rom), Class::ColourOnly);
}

/// A synthesised 0x80 cart is `DualMode`, not collapsed into Colour-only.
#[test]
fn a_colour_enhanced_cart_is_its_own_class_and_not_a_colour_only_one() {
    let d = tempfile::tempdir().unwrap();
    let rom = d.path().join("Enhanced.gb");
    let mut bytes = vec![0u8; 0x150];
    bytes[0x134..0x134 + 6].copy_from_slice(b"ZELDA\0");
    bytes[0x143] = 0x80;
    std::fs::write(&rom, bytes).unwrap();

    assert_eq!(slot_store::gb::title(&rom).as_deref(), Some("ZELDA"));
    assert_eq!(slot_store::gb::cgb_flag(&rom), Some(0x80));
    assert_eq!(slot_store::gb::class(&rom), Class::DualMode);
    assert_ne!(slot_store::gb::class(&rom), Class::ColourOnly);
}

/// The title is 11 bytes; reading 16 from 0x134 would swallow the manufacturer code and flag.
#[test]
fn the_title_read_does_not_swallow_the_manufacturer_code_or_the_flag() {
    let d = tempfile::tempdir().unwrap();
    let rom = d.path().join("Long.gbc");
    let mut bytes = vec![0u8; 0x150];
    bytes[0x134..0x134 + 11].copy_from_slice(b"ABCDEFGHIJK");
    bytes[0x13f..0x143].copy_from_slice(b"WXYZ"); // manufacturer code
    bytes[0x143] = 0xc0;
    std::fs::write(&rom, bytes).unwrap();

    assert_eq!(slot_store::gb::title(&rom).as_deref(), Some("ABCDEFGHIJK"));
    assert_eq!(slot_store::gb::cgb_flag(&rom), Some(0xc0));
}

#[test]
fn a_truncated_rom_is_not_a_panic() {
    let d = tempfile::tempdir().unwrap();
    let rom = d.path().join("Short.gb");
    std::fs::write(&rom, [0u8; 0x50]).unwrap();
    assert_eq!(slot_store::gb::title(&rom), None);
    assert_eq!(slot_store::gb::cgb_flag(&rom), None);
}

/// Any flag besides 00H, 80H and C0H is an original pak.
#[test]
fn a_flag_the_manual_never_named_falls_back_to_the_original_pak() {
    let d = tempfile::tempdir().unwrap();
    for (name, flag) in [("Junk.gb", 0x42u8), ("Half.gb", 0x40)] {
        let rom = d.path().join(name);
        let mut bytes = vec![0u8; 0x150];
        bytes[0x143] = flag;
        std::fs::write(&rom, bytes).unwrap();
        assert_eq!(slot_store::gb::class(&rom), Class::Original, "{name}");
    }
    // And a rom with no header at all is still a cart the shelf can draw.
    let short = d.path().join("Short.gb");
    std::fs::write(&short, [0u8; 0x50]).unwrap();
    assert_eq!(slot_store::gb::class(&short), Class::Original);
}
