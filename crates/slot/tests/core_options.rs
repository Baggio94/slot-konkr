//! Every option `apply_core_options` sets, checked against what the core itself declares.
//!
//! `LibretroCore::set_option` round trips a misspelt key through the frontend's map, so this
//! reads back what was set and crosses it with `LibretroCore::declared_options` instead of
//! naming keys. Skipped when no core dylib has been fetched.

mod common;

use slot::link_kind::{serial_option, LinkKind};
use slot_retro::LibretroCore;
use slot_store::Core;

fn dylib_for(core: Core) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor")
        .join(slot::core::dylib_name(core))
}

/// Every `gpsp_serial` `serial_option` can produce, taken from the function itself. The codes
/// and titles reach each arm: a Pokémon header, Advance Wars 1 and 2, and none of them.
fn every_serial_mode() -> Vec<&'static str> {
    let carts = [
        ("BPEE", "POKEMON EMER"),
        ("AWRE", "ADVANCEWARS"),
        ("AW2E", "ADVANCE WARS2"),
        ("AMFE", "METROID4"),
    ];
    let mut modes = Vec::new();
    for (code, title) in carts {
        for auto in [LinkKind::Cable, LinkKind::Wireless] {
            for chosen in [LinkKind::Cable, LinkKind::Wireless] {
                let mode = serial_option(chosen, auto, code, title);
                if !modes.contains(&mode) {
                    modes.push(mode);
                }
            }
        }
    }
    modes
}

/// Asserts every option set on `core` is declared by it at a declared value. `what` names the
/// combination that produced a failure.
fn every_option_is_one_the_core_has(core: &LibretroCore, what: &str) {
    let declared = core.declared_options();
    assert!(
        !declared.is_empty(),
        "{what}: the core declared no options at all, so this test would pass on anything"
    );
    let set = core.options();
    assert!(!set.is_empty(), "{what}: no options were set");
    for (key, value) in set {
        let Some(values) = declared.get(&key) else {
            let mut known: Vec<_> = declared.keys().cloned().collect();
            known.sort();
            panic!("{what}: the core declares no option {key:?}. It declares: {known:?}");
        };
        assert!(
            values.contains(&value),
            "{what}: the core declares {key:?} as {values:?}, and slot set it to {value:?}"
        );
    }
}

/// Every combination of the three things `apply_core_options` branches on, against both cores.
/// One test because a libretro core keeps its machine in dylib globals, so `core_lock`
/// serialises them anyway.
#[test]
fn every_option_slot_sets_is_one_the_core_declares() {
    let _g = common::core_lock();
    let mut ran = 0;
    for which in Core::ALL {
        let path = dylib_for(which);
        if !path.exists() {
            eprintln!("no {} dylib on this host, skipping", which.as_str());
            continue;
        }
        for serial in every_serial_mode() {
            for bios in [false, true] {
                for colour in [false, true] {
                    let mut core = LibretroCore::open(&path).expect("open core");
                    slot::core::apply_core_options(&mut core, which, serial, bios, colour);
                    every_option_is_one_the_core_has(
                        &core,
                        &format!(
                            "{} serial={serial} bios={bios} colour={colour}",
                            which.as_str()
                        ),
                    );
                    ran += 1;
                }
            }
        }
    }
    if ran == 0 {
        eprintln!("no cores on this host, nothing was checked");
    }
}

/// Both cores declare a colour correction option and both are told about it. Named rather
/// than derived: the core declaring it is the core's own word, so a typo here fails.
#[test]
fn both_cores_declare_a_colour_correction_option() {
    let _g = common::core_lock();
    for (which, key) in [
        (Core::Mgba, "mgba_color_correction"),
        (Core::Gpsp, "gpsp_color_correction"),
    ] {
        let path = dylib_for(which);
        if !path.exists() {
            eprintln!("no {} dylib on this host, skipping", which.as_str());
            continue;
        }
        let core = LibretroCore::open(&path).expect("open core");
        assert!(
            core.declared_options().contains_key(key),
            "{} declares no {key}",
            which.as_str()
        );
    }
}

/// A cable session pins mGBA's BIOS option rather than deciding it from the card, since two
/// cards may disagree and each device runs both consoles. On hardware a mismatch made the
/// joiner refuse the state swap.
#[test]
fn a_cable_session_pins_the_bios_so_both_devices_agree() {
    let _g = common::core_lock();
    let path = dylib_for(Core::Mgba);
    if !path.exists() {
        eprintln!("no mgba dylib on this host, skipping");
        return;
    }
    let mut core = LibretroCore::open(&path).expect("open core");
    slot::core::apply_link_options(&mut core, Core::Mgba, 1);
    let set: std::collections::HashMap<String, String> = core.options().into_iter().collect();
    assert_eq!(
        set.get("mgba_use_bios").map(String::as_str),
        Some("OFF"),
        "a session left the BIOS to whatever each card happened to carry"
    );
    assert_eq!(set.get("mgba_link").map(String::as_str), Some("on"));
    assert_eq!(
        set.get("mgba_link_player").map(String::as_str),
        Some("1"),
        "the port this device drives did not reach the core"
    );
    every_option_is_one_the_core_has(&core, "mgba cable session");
}

/// And it is mGBA's alone: gpSP has no such mode, and a cable session never runs on it.
#[test]
fn a_cable_session_sets_nothing_on_gpsp() {
    let _g = common::core_lock();
    let path = dylib_for(Core::Gpsp);
    if !path.exists() {
        eprintln!("no gpsp dylib on this host, skipping");
        return;
    }
    let mut core = LibretroCore::open(&path).expect("open core");
    slot::core::apply_link_options(&mut core, Core::Gpsp, 0);
    assert!(
        core.options().is_empty(),
        "a cable session reached into gpSP, which has no cable to offer"
    );
}
