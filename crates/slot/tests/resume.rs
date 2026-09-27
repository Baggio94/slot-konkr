mod common;

use slot::app::App;
use slot::audio::{AudioSink, StubSink};
use slot::emu::{CoreState, EmuHandle};
use slot::persist;
use slot::persist::Snapshot;
use slot_retro::MockCore;
use slot_store::{write_slot_state, Platform, SlotState};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn wait_ready(emu: &EmuHandle) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while emu.state() == CoreState::Loading {
        assert!(Instant::now() < deadline, "the core never settled");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// resume.state is read back when the core starts, before it reports ready.
#[test]
fn a_resume_state_is_restored_before_the_core_reports_ready() {
    let emu = EmuHandle::spawn(
        Box::new(MockCore::new()),
        PathBuf::from("unused.gba"),
        StubSink::new().ring(),
        None,
        Some(500_000u64.to_le_bytes().to_vec()),
    );
    wait_ready(&emu);
    let state = emu.request_state().recv().unwrap();
    let n = u64::from_le_bytes(state.try_into().expect("mock state is 8 bytes"));
    assert!(n >= 500_000, "the core started cold, counter is {n}");
}

#[test]
fn read_resume_finds_what_a_flush_wrote() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    persist::flush(
        d.path(),
        Platform::Gba,
        slot_store::Core::Mgba,
        "Emerald",
        Some(&[7u8; 64]),
        None,
    )
    .unwrap();
    assert_eq!(
        persist::read_resume(d.path(), Platform::Gba, slot_store::Core::Mgba, "Emerald"),
        Some(vec![7u8; 64])
    );
}

/// `flush` writes the resume under whichever `Core` it is handed, with no ini read of its own.
/// `gpsp.rs` covers `session.rs` resolving the ini once for every reader and writer.
#[test]
fn flush_routes_by_the_core_it_is_given() {
    let d = common::tmp_root_with_carts(&["Emerald"]);

    persist::flush(
        d.path(),
        Platform::Gba,
        slot_store::Core::Gpsp,
        "Emerald",
        Some(&[7u8; 64]),
        None,
    )
    .unwrap();

    assert!(d
        .path()
        .join("States/GBA/gpsp/Emerald/resume.state")
        .exists());
    assert!(!d
        .path()
        .join("States/GBA/mgba/Emerald/resume.state")
        .exists());
}

/// RetroArch's libretro cores write `.srm`; mGBA standalone writes `.sav`. A card carrying
/// only the RetroArch file has a real save on it and must not boot as a new game.
#[test]
fn a_retroarch_srm_is_read_when_there_is_no_sav() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    std::fs::create_dir_all(d.path().join("Saves/GBA")).unwrap();
    std::fs::write(d.path().join("Saves/GBA/Emerald.srm"), b"srm bytes").unwrap();
    assert_eq!(
        persist::read_sav(d.path(), Platform::Gba, "Emerald").as_deref(),
        Some(&b"srm bytes"[..])
    );
}

/// The battery bytes reach the core's save ram through the same call the session makes.
#[test]
fn srm_bytes_on_disk_reach_the_cores_save_ram() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    let srm: Vec<u8> = (0..8 * 1024).map(|i| (i % 251) as u8).collect();
    std::fs::create_dir_all(d.path().join("Saves/GBA")).unwrap();
    std::fs::write(d.path().join("Saves/GBA/Emerald.srm"), &srm).unwrap();

    let emu = EmuHandle::spawn(
        Box::new(MockCore::new()),
        d.path().join("Games/GBA/Emerald.gba"),
        StubSink::new().ring(),
        persist::read_sav(d.path(), Platform::Gba, "Emerald"),
        persist::read_resume(d.path(), Platform::Gba, slot_store::Core::Mgba, "Emerald"),
    );
    wait_ready(&emu);
    let got = emu.snapshot().save_ram().expect("the core has no save ram");
    assert_eq!(got, srm, "the srm never reached the core");
}

#[test]
fn a_sav_wins_over_an_srm_when_both_exist() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    std::fs::create_dir_all(d.path().join("Saves/GBA")).unwrap();
    std::fs::write(d.path().join("Saves/GBA/Emerald.srm"), b"srm bytes").unwrap();
    std::fs::write(d.path().join("Saves/GBA/Emerald.sav"), b"sav bytes").unwrap();
    assert_eq!(
        persist::read_sav(d.path(), Platform::Gba, "Emerald").as_deref(),
        Some(&b"sav bytes"[..])
    );
}

/// A mock core (no dylib) refuses a real 128 KB save and 256 KB resume at open, since it fixes
/// its own sizes. `EmuSnapshot` must record the refusal, or the next flush overwrites both files.
#[test]
fn a_mismatched_save_ram_and_resume_are_flagged_untrusted_rather_than_silently_swapped_in() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    let real_sav = vec![0x5Au8; 131_072];
    let real_resume = vec![0xA5u8; 262_144];

    let emu = EmuHandle::spawn(
        Box::new(MockCore::new()),
        d.path().join("Games/GBA/Emerald.gba"),
        StubSink::new().ring(),
        Some(real_sav),
        Some(real_resume),
    );
    wait_ready(&emu);
    let snapshot = emu.snapshot();
    assert!(
        !snapshot.resume_trusted(),
        "a mismatched resume must not read back as trusted"
    );
    assert!(
        !snapshot.save_ram_trusted(),
        "a mismatched save ram must not read back as trusted"
    );
}

/// A power tap, which flushes immediately, leaves the real files untouched after a refusal.
#[test]
fn a_power_press_does_not_let_a_refusing_mock_overwrite_a_real_save() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    let real_sav = vec![0x5Au8; 131_072];
    let real_resume = vec![0xA5u8; 262_144];
    std::fs::create_dir_all(d.path().join("Saves/GBA")).unwrap();
    std::fs::write(d.path().join("Saves/GBA/Emerald.sav"), &real_sav).unwrap();
    persist::flush(
        d.path(),
        Platform::Gba,
        slot_store::Core::Mgba,
        "Emerald",
        Some(&real_resume),
        None,
    )
    .unwrap();

    // Read back as `session.rs::spawn_core` does, and handed to the mock, which refuses both;
    // this exercises the downstream guard, not `open_core_for`'s fallback.
    let sav = persist::read_sav(d.path(), Platform::Gba, "Emerald");
    let resume = persist::read_resume(d.path(), Platform::Gba, slot_store::Core::Mgba, "Emerald");
    let emu = EmuHandle::spawn(
        Box::new(MockCore::new()),
        d.path().join("Games/GBA/Emerald.gba"),
        StubSink::new().ring(),
        sav,
        resume,
    );
    wait_ready(&emu);

    let mut a = common::app_playing_with(d.path(), "Emerald", Box::new(emu.snapshot()));
    a.apply(slot_input::Action::PowerPress);

    assert_eq!(
        std::fs::read(d.path().join("Saves/GBA/Emerald.sav")).unwrap(),
        real_sav,
        "the mock's own save ram overwrote the real one"
    );
    assert_eq!(
        persist::read_resume(d.path(), Platform::Gba, slot_store::Core::Mgba, "Emerald").unwrap(),
        real_resume,
        "the mock's own resume overwrote the real one"
    );
}

/// The same for `flush_eject`, a second independent call into `persist`.
#[test]
fn an_eject_does_not_let_a_refusing_mock_overwrite_a_real_save() {
    let d = common::tmp_root_with_carts(&["Emerald", "Fusion"]);
    let real_sav = vec![0x5Au8; 131_072];
    let real_resume = vec![0xA5u8; 262_144];
    std::fs::create_dir_all(d.path().join("Saves/GBA")).unwrap();
    std::fs::write(d.path().join("Saves/GBA/Emerald.sav"), &real_sav).unwrap();
    persist::flush(
        d.path(),
        Platform::Gba,
        slot_store::Core::Mgba,
        "Emerald",
        Some(&real_resume),
        None,
    )
    .unwrap();

    let sav = persist::read_sav(d.path(), Platform::Gba, "Emerald");
    let resume = persist::read_resume(d.path(), Platform::Gba, slot_store::Core::Mgba, "Emerald");
    let emu = EmuHandle::spawn(
        Box::new(MockCore::new()),
        d.path().join("Games/GBA/Emerald.gba"),
        StubSink::new().ring(),
        sav,
        resume,
    );
    wait_ready(&emu);

    let mut a = common::app_playing_with(d.path(), "Emerald", Box::new(emu.snapshot()));
    a.apply(slot_input::Action::Eject);

    assert_eq!(
        std::fs::read(d.path().join("Saves/GBA/Emerald.sav")).unwrap(),
        real_sav,
        "the mock's own save ram overwrote the real one on eject"
    );
    assert_eq!(
        persist::read_resume(d.path(), Platform::Gba, slot_store::Core::Mgba, "Emerald").unwrap(),
        real_resume,
        "the mock's own resume overwrote the real one on eject"
    );
}

/// `SELECT+R1` after a refusal must not push onto the ring: on a full ring the push would evict
/// the oldest genuine save to make room for the mock's placeholder.
#[test]
fn a_refusing_mock_does_not_evict_a_real_ring_entry_on_manual_save() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    let real_sav = vec![0x5Au8; 131_072];
    let real_resume = vec![0xA5u8; 262_144];

    // A full ring of genuine saves, ten deep, oldest to newest.
    let ring =
        slot_store::StateRing::new(d.path(), Platform::Gba, slot_store::Core::Mgba, "Emerald");
    for i in 0..slot_store::RING_MAX {
        let stamp = format!("2026-01-01_00-00-{i:02}");
        ring.push(&vec![i as u8; 200_000], b"png", &stamp)
            .expect("push");
    }
    let before = ring.list().expect("list");
    assert_eq!(before.len(), slot_store::RING_MAX, "the ring did not fill");
    let oldest = before.last().expect("an oldest entry").stamp.clone();
    assert_eq!(oldest, "2026-01-01_00-00-00", "wrong entry called oldest");

    // A mock handed a real resume it does not match.
    let emu = EmuHandle::spawn(
        Box::new(MockCore::new()),
        d.path().join("Games/GBA/Emerald.gba"),
        StubSink::new().ring(),
        Some(real_sav),
        Some(real_resume),
    );
    wait_ready(&emu);
    assert!(
        !emu.snapshot().resume_trusted(),
        "the mock must have refused the resume for this test to mean anything"
    );

    let mut a = common::app_playing_with(d.path(), "Emerald", Box::new(emu.snapshot()));
    a.apply(slot_input::Action::SaveState);

    let after = ring.list().expect("list");
    assert_eq!(
        after.len(),
        slot_store::RING_MAX,
        "the ring changed size: either nothing was declined or something else broke"
    );
    assert!(
        after.iter().any(|e| e.stamp == oldest),
        "the oldest genuine save was evicted to make room for the mock's placeholder"
    );
    assert!(
        a.refusal_active(a.now()),
        "nothing told the player the save was declined"
    );
}

/// `common::app_playing_with`, plus whether the opened emulator is the one the cart's states are
/// filed under (`App::set_named_core`). `App` will not retire a refused state until it is told.
fn seated_with_named_core(
    root: &Path,
    stem: &str,
    snapshot: Box<dyn Snapshot>,
    named: bool,
) -> App {
    write_slot_state(
        root,
        &SlotState {
            cart: Some(stem.to_string()),
            clock_set: true,
            utc_offset_min: 0,
            ..Default::default()
        },
    )
    .expect("write slot.state");
    let mut a = App::boot(root);
    a.set_snapshot(snapshot);
    a.set_named_core(named);
    // `on_core_ready` is where a refusal first becomes knowable. Then run past the insert floor
    // so the cart is playing, not mid-animation.
    a.on_core_ready();
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    a
}

/// A core opened on the cart's real resume, handed over as `session.rs::spawn_core` does.
/// `MockCore::unserialize` takes only eight bytes, so the refusal is genuine.
fn a_core_that_refuses(root: &Path, stem: &str) -> EmuHandle {
    let resume = persist::read_resume(root, Platform::Gba, slot_store::Core::Mgba, stem);
    assert!(
        resume.is_some(),
        "there is no resume on the card, so nothing can be refused"
    );
    let emu = EmuHandle::spawn(
        Box::new(MockCore::new()),
        root.join(format!("Games/GBA/{stem}.gba")),
        StubSink::new().ring(),
        None,
        resume,
    );
    wait_ready(&emu);
    assert!(
        !emu.snapshot().resume_trusted(),
        "the core took the resume, so this test proves nothing"
    );
    emu
}

/// Every retired state in one cart's directory, by full path.
fn retired_states(root: &Path, stem: &str) -> Vec<PathBuf> {
    let dir = root.join("States/GBA/mgba").join(stem);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .map(|e| e.expect("read the states directory").path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("resume-refused-"))
        })
        .collect();
    found.sort();
    found
}

/// A refused resume is moved aside so the next open does not hand it to the core again. Moved,
/// not deleted: it is still a real session to the core that wrote it.
#[test]
fn a_refused_resume_is_not_offered_to_the_core_a_second_time() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    let real_resume = vec![0xA5u8; 262_144];
    persist::flush(
        d.path(),
        Platform::Gba,
        slot_store::Core::Mgba,
        "Emerald",
        Some(&real_resume),
        None,
    )
    .unwrap();

    let emu = a_core_that_refuses(d.path(), "Emerald");
    let _a = seated_with_named_core(d.path(), "Emerald", Box::new(emu.snapshot()), true);

    assert_eq!(
        persist::read_resume(d.path(), Platform::Gba, slot_store::Core::Mgba, "Emerald"),
        None,
        "the refused state is still where the next open will find it and be refused again"
    );

    let retired = retired_states(d.path(), "Emerald");
    assert_eq!(
        retired.len(),
        1,
        "expected exactly one retired state, found {retired:?}"
    );
    assert_eq!(
        std::fs::read(&retired[0]).expect("read the retired state"),
        real_resume,
        "the retired file is not the bytes that were refused"
    );

    // The retired file must never be offered again: `list` feeds the switcher and
    // `load_newest`, and `evict` only deletes what `list` returns.
    let ring =
        slot_store::StateRing::new(d.path(), Platform::Gba, slot_store::Core::Mgba, "Emerald");
    assert!(
        ring.list().expect("list").is_empty(),
        "the retired state came back as a ring entry"
    );
}

/// A mock (missing dylib) refuses every state it did not write, so its refusal must not retire
/// the resume: the guard is which core refused, not how many times.
#[test]
fn a_stand_in_core_refusing_a_resume_leaves_it_alone() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    let real_resume = vec![0xA5u8; 262_144];
    persist::flush(
        d.path(),
        Platform::Gba,
        slot_store::Core::Mgba,
        "Emerald",
        Some(&real_resume),
        None,
    )
    .unwrap();

    let emu = a_core_that_refuses(d.path(), "Emerald");
    let _a = seated_with_named_core(d.path(), "Emerald", Box::new(emu.snapshot()), false);

    assert_eq!(
        persist::read_resume(d.path(), Platform::Gba, slot_store::Core::Mgba, "Emerald"),
        Some(real_resume),
        "a missing core cost the player the session it could not read"
    );
    assert!(
        retired_states(d.path(), "Emerald").is_empty(),
        "a stand-in's refusal filed the state away"
    );
}
