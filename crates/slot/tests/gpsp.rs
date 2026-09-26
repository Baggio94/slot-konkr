mod common;

use slot_store::{Core, Platform};

/// The device carries both cores in `System/`; a host build carries whichever were fetched.
/// Absent means this host cannot run the test, not that the test failed.
fn dylib_for(core: Core) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor")
        .join(slot::core::dylib_name(core))
}

/// Uses the real `dylib_name`, which `open_core` builds its search list from.
#[test]
fn each_core_resolves_to_its_own_dylib() {
    assert_ne!(
        slot::core::dylib_name(Core::Mgba),
        slot::core::dylib_name(Core::Gpsp)
    );
    assert!(slot::core::dylib_name(Core::Gpsp).starts_with("gpsp_libretro"));
    assert!(dylib_for(Core::Gpsp)
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("gpsp_libretro"));
}

#[test]
fn gpsp_loads_and_runs_a_frame() {
    let path = dylib_for(Core::Gpsp);
    if !path.exists() {
        eprintln!("no gpSP dylib on this host, skipping");
        return;
    }
    let _g = common::core_lock();
    let mut core = slot_retro::LibretroCore::open(&path).expect("open gpsp");
    // gpSP reads options during retro_load_game, so this must be set before load.
    core.set_option("gpsp_serial", "rfu");
    assert_eq!(core.option("gpsp_serial"), Some("rfu".to_string()));
}

/// `auto` resolves the serial protocol from the ROM, so two devices agree without being told.
#[test]
fn gpsp_is_told_its_serial_mode_before_load() {
    let path = dylib_for(Core::Gpsp);
    if !path.exists() {
        eprintln!("no gpSP dylib on this host, skipping");
        return;
    }
    let _g = common::core_lock();
    let mut core = slot_retro::LibretroCore::open(&path).expect("open gpsp");
    slot::core::apply_core_options(&mut core, Core::Gpsp, "auto", false, false);
    assert_eq!(
        core.option("gpsp_serial"),
        Some("auto".to_string()),
        "auto resolves per ROM, so both devices agree without being told"
    );
}

/// The mode the link screen asks for is the value the core is handed, never `auto`.
#[test]
fn gpsp_is_told_the_serial_mode_it_is_handed() {
    let path = dylib_for(Core::Gpsp);
    if !path.exists() {
        eprintln!("no gpSP dylib on this host, skipping");
        return;
    }
    let _g = common::core_lock();
    let mut core = slot_retro::LibretroCore::open(&path).expect("open gpsp");
    for serial in ["rfu", "mul_poke", "mul_aw1", "mul_aw2"] {
        slot::core::apply_core_options(&mut core, Core::Gpsp, serial, false, false);
        assert_eq!(
            core.option("gpsp_serial"),
            Some(serial.to_string()),
            "gpSP was not handed the mode the link screen asked for"
        );
    }
}

/// A real BIOS sets `gpsp_boot_mode` so the logo plays; gpSP's own default skips it.
/// `gpsp_bios` stays unset: its `auto` default already loads the card's BIOS, and `official`
/// would only add an OSD warning over slot's chrome on failure.
#[test]
fn gpsp_boots_through_the_bios_when_the_card_carries_one() {
    let path = dylib_for(Core::Gpsp);
    if !path.exists() {
        eprintln!("no gpSP dylib on this host, skipping");
        return;
    }
    let _g = common::core_lock();
    let mut core = slot_retro::LibretroCore::open(&path).expect("open gpsp");
    slot::core::apply_core_options(&mut core, Core::Gpsp, "auto", true, false);
    assert_eq!(
        core.option("gpsp_boot_mode"),
        Some("bios".to_string()),
        "a real BIOS on the card and gpSP still told to skip it"
    );
    assert_eq!(
        core.option("gpsp_bios"),
        None,
        "gpsp_bios was named: auto already picks the official image up, and official only \
         adds an on-screen warning when it cannot"
    );
    assert_eq!(
        core.option("gpsp_serial"),
        Some("auto".to_string()),
        "the link mode stopped getting through once the boot mode joined it"
    );
}

/// gpSP's built-in BIOS has no logo, so booting through it is a blank screen that reads as a
/// hang. Without the file the option stays unset.
#[test]
fn gpsp_is_left_on_its_own_boot_default_when_the_card_has_no_bios() {
    let path = dylib_for(Core::Gpsp);
    if !path.exists() {
        eprintln!("no gpSP dylib on this host, skipping");
        return;
    }
    let _g = common::core_lock();
    let mut core = slot_retro::LibretroCore::open(&path).expect("open gpsp");
    slot::core::apply_core_options(&mut core, Core::Gpsp, "auto", false, false);
    assert_eq!(
        core.option("gpsp_boot_mode"),
        None,
        "gpSP was sent through a BIOS the card does not have, which is its built-in one: \
         seconds of blank screen where the logo was promised"
    );
}

/// mGBA gets none of gpSP's options, including its BIOS switch, whatever the mode.
/// Its own frameskip key is pinned because a misspelling leaves the audio buffer status
/// callback unregistered and `set_frame_skip` a silent no-op.
#[test]
fn mgba_is_given_its_own_frameskip_and_none_of_gpsps() {
    let path = dylib_for(Core::Mgba);
    if !path.exists() {
        eprintln!("no mGBA dylib on this host, skipping");
        return;
    }
    let _g = common::core_lock();
    let mut core = slot_retro::LibretroCore::open(&path).expect("open mgba");
    slot::core::apply_core_options(&mut core, Core::Mgba, "rfu", true, false);
    assert_eq!(
        core.option("gpsp_serial"),
        None,
        "mGBA has no such option and must not be handed one"
    );
    assert_eq!(
        core.option("gpsp_boot_mode"),
        None,
        "gpSP's boot switch reached mGBA, which spells its own mgba_skip_bios"
    );
    assert_eq!(
        core.option("mgba_frameskip").as_deref(),
        Some("auto"),
        "mGBA was never put on auto frameskip, so nothing can tell it which frame to draw"
    );
    assert_eq!(
        core.option("gpsp_frameskip"),
        None,
        "gpSP's frameskip key reached mGBA, which reads only its own prefix"
    );
}

/// The quick menu's Colour Correction, on both cores, in each core's own spelling.
///
/// Pinned rather than trusted, because nothing in this tree can tell a correct option value
/// from a typo: `slot-retro` answers `SET_VARIABLES` with a bare `true` and throws the declared
/// list away, so `Autp` or `mgba_colour_correction` would be accepted in silence and simply
/// never take — a row that does nothing, with nothing failing anywhere. These four strings were
/// read off the vendored dylibs by dumping that discarded list; this is what keeps them true.
///
/// The two cores disagree about every part of it. mGBA declares `OFF|GBA|GBC|Auto` and gpSP
/// `disabled|enabled`, under different keys, and only mGBA has an `Auto` — it is the core that
/// runs Game Boy and Game Boy Color carts as well as GBA ones, so it is the only one with more
/// than one tint to choose between. Neither core may be handed the other's words.
#[test]
fn both_cores_are_told_about_colour_correction_in_their_own_words() {
    for (which, key, on, off) in [
        (Core::Mgba, "mgba_color_correction", "Auto", "OFF"),
        (Core::Gpsp, "gpsp_color_correction", "enabled", "disabled"),
    ] {
        let path = dylib_for(which);
        if !path.exists() {
            eprintln!("no {} dylib on this host, skipping", which.as_str());
            continue;
        }
        let _g = common::core_lock();
        let mut core = slot_retro::LibretroCore::open(&path).expect("open the core");
        for (colour, want) in [(true, on), (false, off)] {
            // Set when off too: unset would leave the core's own default, which is not "off".
            slot::core::apply_core_options(&mut core, which, "auto", false, colour);
            assert_eq!(
                core.option(key).as_deref(),
                Some(want),
                "{} was not told {want} for colour correction {colour}",
                which.as_str()
            );
        }
        let theirs = match which {
            Core::Mgba => "gpsp_color_correction",
            Core::Gpsp => "mgba_color_correction",
        };
        assert_eq!(
            core.option(theirs),
            None,
            "{} was handed the other core's key",
            which.as_str()
        );
    }
}

/// gpSP gets its own frameskip key, and none of mGBA's.
#[test]
fn gpsp_is_put_on_auto_frameskip() {
    let path = dylib_for(Core::Gpsp);
    if !path.exists() {
        eprintln!("no gpSP dylib on this host, skipping");
        return;
    }
    let _g = common::core_lock();
    let mut core = slot_retro::LibretroCore::open(&path).expect("open gpsp");
    slot::core::apply_core_options(&mut core, Core::Gpsp, "auto", false, false);
    assert_eq!(
        core.option("gpsp_frameskip").as_deref(),
        Some("auto"),
        "gpSP was never put on auto frameskip, so nothing can tell it which frame to draw"
    );
    assert_eq!(
        core.option("mgba_frameskip"),
        None,
        "mGBA's frameskip key reached gpSP, which reads only its own prefix"
    );
}

/// A content root holding the user's BIOS and a logo cart from the ignored `/sdcard`, or
/// `None` when absent, so these tests skip on a fresh clone and in CI. Never check them in.
fn root_with_bios_and_logo_cart() -> Option<(tempfile::TempDir, std::path::PathBuf)> {
    let (bios, rom_bytes) = (common::real_bios()?, common::logo_rom()?);
    let d = common::tmp_root_with_carts(&[]);
    std::fs::copy(bios, d.path().join("BIOS").join("gba_bios.bin")).expect("copy bios");
    let rom = d.path().join("Games").join("Logo.gba");
    std::fs::write(&rom, rom_bytes).expect("write logo rom");
    Some((d, rom))
}

/// Whether the BIOS boot animation shows anywhere in its ~2 s window, via `open_core_for`.
fn splash_plays(root: &std::path::Path, rom: &std::path::Path) -> bool {
    use slot_retro::ButtonMask;
    let mut core =
        slot::core::open_core_for(root, Core::Gpsp, "auto", false, &[dylib_for(Core::Gpsp)]);
    core.load(rom).expect("gpSP refused the logo rom");
    (0..150).any(|_| {
        core.run_frame(ButtonMask::default());
        common::mostly_lit(core.video_xrgb8888())
    })
}

/// A real BIOS on the card boots the cart through the logo, checked in pixels: gpSP silently
/// falls back to a splashless built-in BIOS if its own first-byte test rejects the image.
#[test]
fn a_cart_boots_through_a_real_bios_and_the_splash_reaches_the_screen() {
    if !dylib_for(Core::Gpsp).exists() {
        eprintln!("no gpSP dylib on this host, skipping");
        return;
    }
    let Some((d, rom)) = root_with_bios_and_logo_cart() else {
        eprintln!("no real BIOS or no cart to lift a logo from on this host, skipping");
        return;
    };
    let _g = common::core_lock();
    assert!(
        splash_plays(d.path(), &rom),
        "the cart went straight to the game with a real BIOS sitting in the content root"
    );
}

/// With no BIOS on the card the cart goes straight to the game.
#[test]
fn a_cart_goes_straight_to_the_game_when_the_card_has_no_bios() {
    if !dylib_for(Core::Gpsp).exists() {
        eprintln!("no gpSP dylib on this host, skipping");
        return;
    }
    let Some((d, rom)) = root_with_bios_and_logo_cart() else {
        eprintln!("no real BIOS or no cart to lift a logo from on this host, skipping");
        return;
    };
    let _g = common::core_lock();
    std::fs::remove_file(d.path().join("BIOS").join("gba_bios.bin")).unwrap();
    assert!(
        !splash_plays(d.path(), &rom),
        "a splash played with no BIOS on the card, so this test cannot tell the two apart"
    );
}

/// Whether any frame published through `EmuHandle::spawn` is the boot screen.
fn a_published_frame_is_the_splash(
    root: &std::path::Path,
    rom: &std::path::Path,
    resume: Option<Vec<u8>>,
) -> bool {
    use slot::audio::{AudioSink, StubSink};
    use slot::emu::{CoreState, EmuHandle, Speed};
    use std::time::{Duration, Instant};

    let mut sink = StubSink::new();
    sink.open(32_768).expect("the stub refused to open");
    // The worker waits for the device to make room, so the sink must be drained.
    let drain = sink.clone();
    std::thread::spawn(move || loop {
        drain.device_drain();
        std::thread::sleep(Duration::from_millis(2));
    });

    let emu = EmuHandle::spawn(
        slot::core::open_core_for(root, Core::Gpsp, "auto", false, &[dylib_for(Core::Gpsp)]),
        rom.to_path_buf(),
        sink.ring(),
        None,
        resume,
    );
    // A worker starts paused.
    emu.set_speed(Speed::Normal);
    let deadline = Instant::now() + Duration::from_secs(10);
    while emu.state() == CoreState::Loading {
        assert!(Instant::now() < deadline, "the core never finished loading");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(emu.state(), CoreState::Ready, "the core refused the cart");

    // Longer than the animation, so a splash that plays at all is a splash this sees.
    let watch = Instant::now() + Duration::from_secs(3);
    while Instant::now() < watch {
        if emu.latest_frame().is_some_and(|f| common::mostly_lit(&f)) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(4));
    }
    false
}

/// A cart resumed from a save state must not replay the splash; a fresh boot must.
/// This holds because `emu::Worker::run` restores the state after `load` and before it
/// publishes a frame.
#[test]
fn the_splash_plays_on_a_fresh_start_and_never_over_a_resume() {
    if !dylib_for(Core::Gpsp).exists() {
        eprintln!("no gpSP dylib on this host, skipping");
        return;
    }
    let Some((d, rom)) = root_with_bios_and_logo_cart() else {
        eprintln!("no real BIOS or no cart to lift a logo from on this host, skipping");
        return;
    };
    let _g = common::core_lock();

    // A gpSP state from past the boot: measured, the BIOS screen lasts to frame 269, and a
    // state taken earlier would replay the splash itself. libretro allows one live core.
    let state = {
        use slot_retro::ButtonMask;
        let mut core = slot::core::open_core_for(
            d.path(),
            Core::Gpsp,
            "auto",
            false,
            &[dylib_for(Core::Gpsp)],
        );
        core.load(&rom).expect("gpSP refused the logo rom");
        for _ in 0..480 {
            core.run_frame(ButtonMask::default());
        }
        assert!(
            !common::mostly_lit(core.video_xrgb8888()),
            "the state standing in for a resume is itself a frame of the boot splash, so the \
             half below would fail no matter what the resume did"
        );
        core.serialize().expect("gpSP gave up no state")
    };

    assert!(
        a_published_frame_is_the_splash(d.path(), &rom, None),
        "no splash on a fresh start, so this test cannot see one and proves nothing below"
    );
    assert!(
        !a_published_frame_is_the_splash(d.path(), &rom, Some(state)),
        "the BIOS splash played over a cart the player was resuming"
    );
}

/// A `gpsp` cart reads only the resume state under its own core's directory, through the
/// real `Session`. `each_core_resolves_to_its_own_dylib` pins the dylib half.
#[test]
fn a_gpsp_carts_resume_is_read_from_its_own_core_directory_through_the_session() {
    use slot::app::Phase;
    use slot::persist;
    use slot::session::Session;
    use slot_input::{Btn, RawEvent};
    use slot_store::{StateRing, SELECTED_CORE_FILE};
    use std::time::{Duration, Instant};

    let d = common::tmp_root_with_carts(&["Emerald", "Fusion"]);
    std::fs::write(d.path().join(SELECTED_CORE_FILE), "Emerald = gpsp\n").unwrap();

    // Distinguishable resume states in both directories. If `spawn_core` ever resolved the
    // core twice and the two calls disagreed, or fell back to the default, this is what
    // would catch it: the counter would come back from the wrong file.
    StateRing::new(d.path(), Platform::Gba, Core::Gpsp, "Emerald")
        .write_resume(&700_000u64.to_le_bytes())
        .unwrap();
    StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald")
        .write_resume(&1u64.to_le_bytes())
        .unwrap();

    common::clocked(d.path());
    let mut s = Session::boot(d.path().to_path_buf());
    s.feed([RawEvent::Down(Btn::A)], 16);
    s.feed([RawEvent::Up(Btn::A)], 32);

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut now = 32;
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        now += 16;
        s.feed([], now);
        s.update(1.0 / 60.0);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    // Past the autosave deadline, so the core's counter is written back.
    s.app_mut().tick_ms(60_000);

    let state = persist::read_resume(d.path(), Platform::Gba, Core::Gpsp, "Emerald")
        .expect("nothing resumed");
    let n = u64::from_le_bytes(state.try_into().expect("mock state is 8 bytes"));
    assert!(
        n >= 700_000,
        "the session resumed the wrong core's state (or none): counter is {n}"
    );
}

/// A `gpsp` cart opens gpSP's dylib through a full `Session`, not just its state directory.
/// mGBA's build is planted under gpSP's filename, since the mock would pass either way.
#[test]
fn a_gpsp_cart_runs_the_dylib_planted_under_its_own_name_through_the_session() {
    use slot::app::Phase;
    use slot::persist;
    use slot::session::Session;
    use slot_input::{Btn, RawEvent};
    use slot_store::SELECTED_CORE_FILE;
    use std::time::{Duration, Instant};

    let Some(mgba) = common::vendored_core() else {
        eprintln!("no host-openable dylib on this machine, skipping");
        return;
    };
    let _g = common::core_lock();

    let d = common::tmp_root_with_real_carts(&["Emerald"]);
    std::fs::write(d.path().join(SELECTED_CORE_FILE), "Emerald = gpsp\n").unwrap();
    let planted = d
        .path()
        .join("System")
        .join(slot::core::dylib_name(Core::Gpsp));
    std::fs::copy(&mgba, &planted).expect("plant a dylib under gpSP's name");

    common::clocked(d.path());
    let mut s = Session::boot(d.path().to_path_buf());
    s.feed([RawEvent::Down(Btn::A)], 16);
    s.feed([RawEvent::Up(Btn::A)], 32);

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut now = 32;
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        now += 16;
        s.feed([], now);
        s.update(1.0 / 60.0);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    // Past the autosave deadline, so the planted core's real state is written back.
    s.app_mut().tick_ms(60_000);

    let state = persist::read_resume(d.path(), Platform::Gba, Core::Gpsp, "Emerald")
        .expect("nothing resumed");
    assert!(
        state.len() > 100_000,
        "the session ran the mock, not the dylib the ini named: {} bytes",
        state.len()
    );
}

/// The ini vanishing mid-play (a removable card) must not move the autosave to mGBA's
/// directory: `App` keeps the core resolved at insert.
#[test]
fn changing_the_ini_mid_session_does_not_move_a_seated_carts_autosave() {
    use slot::app::Phase;
    use slot::session::Session;
    use slot_input::{Btn, RawEvent};
    use slot_store::{StateRing, SELECTED_CORE_FILE};
    use std::time::{Duration, Instant};

    let d = common::tmp_root_with_carts(&["Emerald"]);
    std::fs::write(d.path().join(SELECTED_CORE_FILE), "Emerald = gpsp\n").unwrap();

    common::clocked(d.path());
    let mut s = Session::boot(d.path().to_path_buf());
    s.feed([RawEvent::Down(Btn::A)], 16);
    s.feed([RawEvent::Up(Btn::A)], 32);

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut now = 32;
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        now += 16;
        s.feed([], now);
        s.update(1.0 / 60.0);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    // `read_selected_cores` treats this the same as a transient read failure: an empty map.
    std::fs::remove_file(d.path().join(SELECTED_CORE_FILE)).unwrap();

    s.app_mut().tick_ms(60_000);

    assert!(
        StateRing::new(d.path(), Platform::Gba, Core::Gpsp, "Emerald")
            .read_resume()
            .unwrap()
            .is_some(),
        "the autosave did not land under the seated core's own directory"
    );
    assert!(
        StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald")
            .read_resume()
            .unwrap()
            .is_none(),
        "the autosave followed the ini's new (absent) reading instead of the core the \
         session actually spawned"
    );
}

/// The manual save's `App::ring()` also uses the core resolved at insert, not the ini.
#[test]
fn changing_the_ini_mid_session_does_not_move_a_manual_save_state() {
    use slot::app::Phase;
    use slot::session::Session;
    use slot_input::{Action, Btn, RawEvent};
    use slot_store::{StateRing, SELECTED_CORE_FILE};
    use std::time::{Duration, Instant};

    let d = common::tmp_root_with_carts(&["Emerald"]);
    std::fs::write(d.path().join(SELECTED_CORE_FILE), "Emerald = gpsp\n").unwrap();

    common::clocked(d.path());
    let mut s = Session::boot(d.path().to_path_buf());
    s.feed([RawEvent::Down(Btn::A)], 16);
    s.feed([RawEvent::Up(Btn::A)], 32);

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut now = 32;
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        now += 16;
        s.feed([], now);
        s.update(1.0 / 60.0);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    std::fs::remove_file(d.path().join(SELECTED_CORE_FILE)).unwrap();

    s.app_mut().apply(Action::SaveState);

    assert!(
        !StateRing::new(d.path(), Platform::Gba, Core::Gpsp, "Emerald")
            .list()
            .unwrap()
            .is_empty(),
        "the manual save did not land under the seated core's own directory"
    );
    assert!(
        StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald")
            .list()
            .unwrap()
            .is_empty(),
        "the manual save followed the ini's new (absent) reading instead of the core the \
         session actually spawned"
    );
}

/// `flush_eject` also uses the core resolved at insert, not the ini.
#[test]
fn changing_the_ini_mid_session_does_not_move_an_ejected_carts_resume() {
    use slot::app::Phase;
    use slot::session::Session;
    use slot_input::{Action, Btn, RawEvent};
    use slot_store::{StateRing, SELECTED_CORE_FILE};
    use std::time::{Duration, Instant};

    // A lone cart has nowhere to eject to, so `eject()` refuses.
    let d = common::tmp_root_with_carts(&["Emerald", "Fusion"]);
    std::fs::write(d.path().join(SELECTED_CORE_FILE), "Emerald = gpsp\n").unwrap();

    common::clocked(d.path());
    let mut s = Session::boot(d.path().to_path_buf());
    s.feed([RawEvent::Down(Btn::A)], 16);
    s.feed([RawEvent::Up(Btn::A)], 32);

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut now = 32;
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        now += 16;
        s.feed([], now);
        s.update(1.0 / 60.0);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    std::fs::remove_file(d.path().join(SELECTED_CORE_FILE)).unwrap();

    s.app_mut().apply(Action::Eject);

    assert!(
        StateRing::new(d.path(), Platform::Gba, Core::Gpsp, "Emerald")
            .read_resume()
            .unwrap()
            .is_some(),
        "the ejected cart's resume did not land under the seated core's own directory"
    );
    assert!(
        StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald")
            .read_resume()
            .unwrap()
            .is_none(),
        "the ejected cart's resume followed the ini's new (absent) reading instead of the \
         core the session actually spawned"
    );
}

/// `open_core` searches `root/System` for the dylib `Core::Gpsp` names. mGBA's build is
/// planted under gpSP's filename so the mock would show if the wrong name was searched.
#[test]
fn open_core_reaches_a_gpsp_named_dylib_under_the_content_roots_system_directory() {
    use slot_retro::ButtonMask;

    let Some(mgba) = common::vendored_core() else {
        eprintln!("no host-openable dylib on this machine, skipping");
        return;
    };
    let _g = common::core_lock();
    let d = common::tmp_root_with_real_carts(&["Probe"]);
    let planted = d
        .path()
        .join("System")
        .join(slot::core::dylib_name(Core::Gpsp));
    std::fs::copy(&mgba, &planted).expect("plant a dylib under gpSP's name");

    let mut core = slot::core::open_core(d.path(), Core::Gpsp, "auto", false, None).core;
    core.load(&d.path().join("Games/GBA/Probe.gba"))
        .expect("the planted core refused the test rom");
    core.run_frame(ButtonMask::default());
    assert!(
        core.serialize().expect("core gave up no state").len() > 100_000,
        "open_core fell back to the mock instead of the dylib planted at root/System"
    );
}

/// `System/selected_core.ini` is a text file a person edits on a card, and nothing in it stops a
/// line naming gpSP for a Game Boy cart. gpSP does not run Game Boy games at all: it would refuse
/// the ROM outright or paint garbage, so a line naming it for a Game Boy cart is dropped and that
/// cart runs on its platform's default, which is asked for rather than named so this stays about
/// the dropped line when the default moves. The ini keeps every bit of its meaning
/// for a line naming a core that really does run the platform, which is what
/// `a_game_boy_cart_can_ask_for_mgba_by_hand` over in slot-store covers.
///
/// Driven through the real `Session`, because `spawn_core` is the one place a cart's core is
/// resolved, and read back through the directory that one resolution also names. The seeded
/// counter is 700_000, which is further than the mock could ever count to on its own, and it has
/// to come back **moved**: only a run that read `States/GB/<default>/` and then wrote back to it can
/// produce that, so one number pins both halves. A run that had honoured the ini would have left
/// that file exactly as seeded and filed its own state under `States/GB/gpsp/` instead, which is
/// what the second assertion refuses.
#[test]
fn a_game_boy_carts_gpsp_line_is_dropped_and_it_runs_on_the_platform_default() {
    let default = Core::default_for(Platform::Gb);
    use slot::app::Phase;
    use slot::persist;
    use slot::session::Session;
    use slot_input::{Btn, RawEvent};
    use slot_store::{StateRing, SELECTED_CORE_FILE};
    use std::time::{Duration, Instant};

    let d = common::tmp_root_with_gb_carts(&["Tetris", "Zzz"]);
    std::fs::write(d.path().join(SELECTED_CORE_FILE), "Tetris = gpsp\n").unwrap();
    StateRing::new(d.path(), Platform::Gb, default, "Tetris")
        .write_resume(&700_000u64.to_le_bytes())
        .unwrap();

    common::clocked(d.path());
    let mut s = Session::boot(d.path().to_path_buf());
    s.feed([RawEvent::Down(Btn::A)], 16);
    s.feed([RawEvent::Up(Btn::A)], 32);

    let mut now = 32;
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        now += 16;
        s.feed([], now);
        s.update(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(1));
    }

    // The counter each core directory holds, as the mock's eight byte state, or `None` where
    // nothing has ever been filed under that core at all.
    let counter = |core| {
        persist::read_resume(d.path(), Platform::Gb, core, "Tetris")
            .map(|b| u64::from_le_bytes(b.try_into().expect("the mock's state is 8 bytes")))
    };

    // Frames the seated core actually runs, flushed out through the path the binary uses, until
    // the resumed counter moves. A counter that merely still reads what it was seeded with says
    // nothing: that is equally what a run resuming from somewhere else leaves behind.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if counter(default).is_some_and(|n| n > 700_000) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the ini's `Tetris = gpsp` was honoured for a Game Boy cart: States/GB/{} still \
             reads {:?} and States/GB/gpsp reads {:?}",
            default.as_str(),
            counter(default),
            counter(Core::Gpsp)
        );
        now += 16;
        s.feed([], now);
        s.update(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(1));
        s.app_mut().flush_resume();
    }
    assert_eq!(
        counter(Core::Gpsp),
        None,
        "a Game Boy cart's state was filed under States/GB/gpsp"
    );
}
