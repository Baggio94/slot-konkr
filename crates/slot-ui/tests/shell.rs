use slot_store::scan;
use slot_ui::{
    cart_face, gba_shell_for, lookup_order_is_exact_then_family_then_default, table_keys, Finish,
    DEFAULT_SHELL,
};
use tempfile::TempDir;

fn tmp_root() -> TempDir {
    let d = tempfile::tempdir().expect("tempdir");
    for sub in ["Games", "Games/GBA", "Labels", "Saves", "States", "System"] {
        std::fs::create_dir(d.path().join(sub)).expect("create content dir");
    }
    d
}

fn write_rom_with_code(d: &TempDir, name: &str, title: &str, code: &str) {
    let mut rom = vec![0u8; 0x100];
    rom[0xa0..0xa0 + title.len()].copy_from_slice(title.as_bytes());
    rom[0xac..0xac + code.len()].copy_from_slice(code.as_bytes());
    std::fs::write(d.path().join("Games/GBA").join(name), rom).expect("write rom");
}

#[test]
fn an_unknown_game_gets_the_default_grey() {
    assert_eq!(gba_shell_for("ZZZZ").colour, DEFAULT_SHELL.colour);
    assert_eq!(gba_shell_for("").colour, DEFAULT_SHELL.colour);
    // Metroid Fusion, verified as AMTE: an ordinary cart gets the default.
    assert_eq!(gba_shell_for("AMTE").colour, DEFAULT_SHELL.colour);
}

#[test]
fn leafgreen_is_green_whatever_region_it_came_from() {
    for code in ["BPGE", "BPGJ", "BPGP", "BPGD"] {
        let s = gba_shell_for(code);
        assert_ne!(
            s.colour, DEFAULT_SHELL.colour,
            "{code} fell through to grey"
        );
        assert!(s.colour[1] > s.colour[0], "{code} is not green");
    }
}

/// Verified from a real header: Shrek GBA Video is MSKE.
#[test]
fn gba_video_carts_are_light_grey() {
    let v = gba_shell_for("MSKE");
    assert_ne!(
        v.colour, DEFAULT_SHELL.colour,
        "video fell through to the default grey"
    );
    assert!(
        v.colour.iter().all(|c| *c > 0xA0),
        "video shells are light grey, got {:?}",
        v.colour
    );
    assert_eq!(gba_shell_for("MPOE").colour, v.colour);
}

/// An explicit row has to beat the family letter.
#[test]
fn an_exact_entry_outranks_the_family_letter() {
    assert_eq!(gba_shell_for("MSKE").colour, gba_shell_for("MSKJ").colour);
    // The shipping table has no conflicting row, so check the order directly.
    assert!(lookup_order_is_exact_then_family_then_default());
}

/// Only the Pokemon rows are translucent; the GBA Video family stays solid, so finish is per
/// row, not per colour.
#[test]
fn the_pokemon_shells_are_clear_and_the_rest_are_solid() {
    for code in table_keys() {
        let want = if code.starts_with("AX") || code.starts_with("BP") {
            Finish::Translucent
        } else {
            Finish::Solid
        };
        assert_eq!(
            gba_shell_for(code).finish,
            want,
            "{code} has the wrong finish"
        );
    }
    assert_eq!(
        gba_shell_for("MSKE").finish,
        Finish::Solid,
        "the video family is not solid"
    );
    assert_eq!(
        gba_shell_for("ZZZZ").finish,
        Finish::Solid,
        "the default is not solid"
    );
}

#[test]
fn the_pokemon_shells_are_all_distinct() {
    let codes = ["AXVE", "AXPE", "BPEE", "BPRE", "BPGE"];
    let mut seen = Vec::new();
    for c in codes {
        let col = gba_shell_for(c).colour;
        assert!(
            !seen.contains(&col),
            "{c} shares a colour with another cart"
        );
        seen.push(col);
    }
}

#[test]
fn no_two_table_entries_share_a_prefix() {
    let mut keys: Vec<&str> = table_keys();
    keys.sort();
    let before = keys.len();
    keys.dedup();
    assert_eq!(keys.len(), before, "two entries claim the same prefix");
    assert!(
        keys.iter().all(|k| k.len() == 3),
        "keys must be the region free prefix"
    );
}

#[test]
fn the_label_does_not_cover_the_whole_shell() {
    let d = tmp_root();
    write_rom_with_code(&d, "Emerald.gba", "POKEMON EMER", "BPEE");
    let cart = &scan(d.path()).unwrap()[0];
    let f = cart_face(cart);
    let px = |x: u32, y: u32| {
        let i = ((y * f.w + x) * 4) as usize;
        [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2]]
    };
    let shell = gba_shell_for("BPEE").colour;
    assert_eq!(
        px(f.w / 2, 4),
        shell,
        "the label reaches the top edge, no shell shows"
    );
    assert_ne!(
        px(f.w / 2, f.h / 2),
        shell,
        "the label is missing from the middle"
    );
}
