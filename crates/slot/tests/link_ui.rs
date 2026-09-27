//! The in-game menu and the link it starts.
//!
//! Nothing here touches `slotlink.sh` or a network interface: every starter is built through
//! `LinkStarter::spawn_with` with its slow parts injected. The one real `TcpLink` is over
//! loopback, because `LinkProgress::Ready` carries a transport.

mod common;

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::{Duration, Instant};

use slot::app::{
    App, GameMenu, LinkLegend, LinkRow, Phase, LINKED_HOLD_MS, LINK_LOST_MS, UNPLUG_HOLD_MS,
};
use slot::emu::{CoreState, EmuHandle, Speed};
use slot::link_kind::LinkKind;
use slot::link_net::{Cancel, TcpLink};
use slot::link_radio::{LinkRole, RadioJob, RadioJobs};
use slot::link_start::{LinkFail, LinkStarter, LinkStep};
use slot::persist::{self, Snapshot};
use slot::session::Session;
use slot_input::{Action, Btn, Millis, RawEvent};
use slot_retro::{ButtonMask, LinkChannel};
use slot_store::{write_slot_state, Core, Platform, SlotState};
use slot_ui::{arrows_hint_face, hint_face, opening, Draw, TexId, Toast, HINT_EDGE, OUT_H, OUT_W};
use tempfile::TempDir;

/// How long a test waits on a real worker thread before deciding it never will answer.
const BAIL: Duration = Duration::from_secs(5);

/// A game in the slot on a stated core, which decides whether the link screen exists. Two
/// carts, so `single_cart` does not apply; Ruby's code so gpSP carries it (`mul_poke`).
fn playing_on(core: Core) -> (App, TempDir) {
    let d = common::tmp_root_with_carts(&["Emerald", "Zzz"]);
    common::write_retail_header(&d, "Emerald", "POKEMON RUBY", "AXVE");
    let mut app = common::boot(d.path());
    app.apply(Action::Insert);
    app.set_core(core);
    app.on_core_ready();
    for _ in 0..120 {
        app.update(1.0 / 60.0);
    }
    assert!(matches!(app.phase(), Phase::Playing { .. }), "never seated");
    (app, d)
}

/// A worker whose radio always comes up and whose socket step is whatever the test says.
fn fake_starter(
    socket: impl FnMut(u16, &Cancel) -> io::Result<TcpLink> + Send + 'static,
) -> LinkStarter {
    LinkStarter::spawn_with(
        Box::new(|_, _| Ok(())),
        Box::new(|| {}),
        LinkRole::Host,
        0,
        Box::new(socket),
    )
}

fn io_err(kind: io::ErrorKind) -> io::Error {
    io::Error::new(kind, "from a test")
}

/// Frames until the overlay stops waiting on the real worker thread, bounded.
fn settle(app: &mut App) {
    let deadline = Instant::now() + BAIL;
    while matches!(app.game_menu(), Some(GameMenu::Working { .. })) {
        app.update(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(1));
        assert!(Instant::now() < deadline, "the overlay never left Working");
    }
}

/// HOST and JOIN, each a different width so a test can tell which one is drawn.
fn fake_roles(app: &mut App) -> Vec<(TexId, u32, u32)> {
    let faces: Vec<(TexId, u32, u32)> = (0..LinkRow::ALL.len())
        .map(|i| (TexId::from_raw(700 + i), 120 + 40 * i as u32, 40))
        .collect();
    app.set_link_menu_faces(faces.clone());
    faces
}

#[test]
fn select_and_menu_open_the_link_screen_on_host() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    assert_eq!(app.game_menu(), Some(GameMenu::Pick(LinkRow::Host)));
    assert!(matches!(app.phase(), Phase::Playing { .. }));
}

/// mGBA links by running both machines in step, so the screen opens on its own link rather
/// than sending the player to another core.
#[test]
fn the_link_screen_opens_under_mgba_on_its_own_cable() {
    let (mut app, _d) = playing_on(Core::Mgba);
    app.apply(Action::GameMenu);
    assert_eq!(
        app.game_menu(),
        Some(GameMenu::Pick(LinkRow::Host)),
        "mGBA did not offer the link it carries"
    );
    assert_eq!(
        app.toast(),
        None,
        "it opened the screen and banished it too"
    );
}

/// On gpSP the screen itself is the answer; the banner stays out of it.
#[test]
fn the_link_screen_on_gpsp_says_nothing_in_the_banner() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    assert!(app.game_menu_open());
    assert_eq!(app.toast(), None);
}

#[test]
fn left_and_right_swap_host_and_join_and_the_screen_remembers() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    app.apply(Action::GbaDown(Btn::Right));
    assert_eq!(app.game_menu(), Some(GameMenu::Pick(LinkRow::Join)));
    app.apply(Action::GbaDown(Btn::Left));
    assert_eq!(app.game_menu(), Some(GameMenu::Pick(LinkRow::Host)));
    app.apply(Action::GbaDown(Btn::Right));
    app.apply(Action::GbaDown(Btn::B));
    assert!(!app.game_menu_open());
    app.apply(Action::GameMenu);
    assert_eq!(
        app.game_menu(),
        Some(GameMenu::Pick(LinkRow::Join)),
        "the last role was forgotten"
    );
}

#[test]
fn b_on_pick_hands_the_game_back() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    app.apply(Action::GbaDown(Btn::B));
    assert!(!app.game_menu_open());
    assert!(matches!(app.phase(), Phase::Playing { .. }));
}

/// The shelf's quick menu is a different screen on a different button, and still works.
#[test]
fn the_game_menu_does_not_open_on_the_shelf() {
    let d = common::tmp_root_with_carts(&["Emerald", "Zzz"]);
    let mut app = common::boot(d.path());
    app.apply(Action::GameMenu);
    assert!(!app.game_menu_open(), "the shelf raised the in-game menu");
    assert!(matches!(app.phase(), Phase::Shelf));
    app.apply(Action::QuickMenu);
    assert!(
        matches!(app.phase(), Phase::QuickMenu { .. }),
        "the shelf lost its quick menu"
    );
}

/// Host is libretro's client 0 and the joiner is client 1, never the same.
#[test]
fn the_host_is_client_zero_and_the_joiner_client_one() {
    assert_eq!(LinkRow::Host.client_id(), 0);
    assert_eq!(LinkRow::Join.client_id(), 1);
    assert_eq!(LinkRow::Host.role(), LinkRole::Host);
    assert_eq!(LinkRow::Join.role(), LinkRole::Join);
    assert_eq!(LinkRow::from_client_id(0), LinkRow::Host);
    assert_eq!(LinkRow::from_client_id(1), LinkRow::Join);
    assert_eq!(LinkRow::Host.other(), LinkRow::Join);
}

#[test]
fn a_on_pick_starts_the_link_in_the_picked_role() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    app.apply(Action::GbaDown(Btn::Right));
    app.apply(Action::GbaDown(Btn::A));
    assert!(
        matches!(
            app.game_menu(),
            Some(GameMenu::Working {
                role: LinkRow::Join,
                step: LinkStep::Radio,
                ..
            })
        ),
        "A did not start a joiner: {:?}",
        app.game_menu()
    );
    app.apply(Action::GbaDown(Btn::B));
    settle(&mut app);
}

/// Each failure gets its own sentence, telling the player what to do.
#[test]
fn each_failure_says_which_one_it_was() {
    for (kind, want) in [
        (io::ErrorKind::TimedOut, LinkFail::NobodyCame),
        (io::ErrorKind::ConnectionRefused, LinkFail::PeerVanished),
    ] {
        let (mut app, _d) = playing_on(Core::Gpsp);
        app.apply(Action::GameMenu);
        app.start_link(fake_starter(move |_, _| Err(io_err(kind))), 0);
        settle(&mut app);
        assert!(matches!(app.game_menu(), Some(GameMenu::Failed { fail, .. }) if fail == want));
    }
    let lines: Vec<&str> = [
        LinkFail::Radio,
        LinkFail::NobodyCame,
        LinkFail::PeerVanished,
    ]
    .iter()
    .map(|f| f.line())
    .collect();
    assert_eq!(
        lines.len(),
        lines.iter().collect::<std::collections::HashSet<_>>().len(),
        "two failures share a sentence, which is a generic 'link failed' in disguise"
    );
}

/// B leaves a failure screen back into the game, which was never interrupted.
#[test]
fn b_on_a_failure_puts_the_player_back_in_the_game() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    app.start_link(fake_starter(|_, _| Err(io_err(io::ErrorKind::TimedOut))), 0);
    settle(&mut app);
    assert!(matches!(
        app.game_menu(),
        Some(GameMenu::Failed {
            fail: LinkFail::NobodyCame,
            ..
        })
    ));
    app.apply(Action::GbaDown(Btn::B));
    assert!(!app.game_menu_open());
    assert!(
        matches!(app.phase(), Phase::Playing { .. }),
        "a failed link ate the session"
    );
    assert!(!app.link_active(), "a failed link started a session anyway");
}

/// A player who backed out is not shown a screen about the thing they just did on purpose.
#[test]
fn a_cancelled_link_says_nothing_and_returns_to_the_game() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    app.start_link(
        fake_starter(|_, cancel: &Cancel| {
            let deadline = Instant::now() + BAIL;
            while !cancel.is_cancelled() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(io_err(io::ErrorKind::Interrupted))
        }),
        0,
    );
    app.update(1.0 / 60.0);
    app.apply(Action::GbaDown(Btn::B));
    settle(&mut app);
    assert!(!app.game_menu_open(), "the cancel left a screen behind");
    assert!(matches!(app.phase(), Phase::Playing { .. }));
}

/// `Ready` must start a session and hand its transport on; otherwise the overlay closes over
/// a game that looks linked and is not.
#[test]
fn a_link_that_comes_up_holds_linked_for_a_second_then_hands_the_game_back() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    let port = common::free_port();
    let far = std::thread::spawn(move || TcpLink::host("127.0.0.1", port).expect("host"));
    std::thread::sleep(Duration::from_millis(150));
    app.start_link(
        fake_starter(move |_, _| TcpLink::join("127.0.0.1", port)),
        0,
    );
    settle(&mut app);
    let _far = far.join().expect("host thread");

    assert!(matches!(
        app.game_menu(),
        Some(GameMenu::Linked {
            role: LinkRow::Host,
            ..
        })
    ));
    assert!(
        app.link_active(),
        "the session waited for the screen instead of starting"
    );
    let (client_id, _transport) = app.take_link_transport().expect("no transport handed on");
    assert_eq!(client_id, 0);

    for press in [Btn::A, Btn::B] {
        app.apply(Action::GbaDown(press));
    }
    app.apply(Action::GameMenu);
    assert!(
        matches!(app.game_menu(), Some(GameMenu::Linked { .. })),
        "the hold took a press"
    );

    let frames = (LINKED_HOLD_MS as f32 / (1000.0 / 60.0)) as usize;
    for _ in 0..frames - 2 {
        app.update(1.0 / 60.0);
    }
    assert!(app.game_menu_open(), "LINKED left before its second");
    for _ in 0..4 {
        app.update(1.0 / 60.0);
    }
    assert!(!app.game_menu_open(), "LINKED never handed the game back");
    assert!(app.link_active());
}

#[test]
fn a_peer_lost_during_the_hold_closes_the_screen_and_breaks_the_badge() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    let port = common::free_port();
    let far = std::thread::spawn(move || TcpLink::host("127.0.0.1", port).expect("host"));
    std::thread::sleep(Duration::from_millis(150));
    app.start_link(
        fake_starter(move |_, _| TcpLink::join("127.0.0.1", port)),
        1,
    );
    settle(&mut app);
    let _far = far.join().expect("host thread");
    assert!(matches!(app.game_menu(), Some(GameMenu::Linked { .. })));
    app.peer_lost();
    assert!(
        !app.game_menu_open(),
        "LINKED stayed up over a link that just died"
    );
    assert_eq!(app.link_badge(), slot_ui::LinkBadge::JoinedLost);
}

/// B asks the worker to stop, and the screen stays until it answers, or the access point could
/// still be coming up behind the game. This fake ignores the flag, the slowest case.
#[test]
fn b_during_the_radio_step_does_not_hand_the_game_back_early() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    let (release, held) = channel::<()>();
    app.start_link(
        LinkStarter::spawn_with(
            // Stands in for an `slotlink.sh link` that has not answered yet.
            Box::new(move |_, _| {
                held.recv().expect("released");
                Ok(())
            }),
            Box::new(|| {}),
            LinkRole::Host,
            0,
            Box::new(|_, cancel: &Cancel| {
                Err(io_err(if cancel.is_cancelled() {
                    io::ErrorKind::Interrupted
                } else {
                    io::ErrorKind::TimedOut
                }))
            }),
        ),
        0,
    );
    for _ in 0..10 {
        app.update(1.0 / 60.0);
    }
    app.apply(Action::GbaDown(Btn::B));
    for _ in 0..10 {
        app.update(1.0 / 60.0);
    }
    assert!(
        matches!(app.game_menu(), Some(GameMenu::Working { .. })),
        "B handed the game back while the radio was still coming up behind it"
    );
    release.send(()).expect("release the radio");
    settle(&mut app);
    assert!(!app.game_menu_open(), "the cancel never landed at all");
}

/// `LinkStarter` has no `Drop`, so a dropped host could leave its access point up for thirty
/// seconds. Every path that ends the overlay asks it to stop first.
#[test]
fn a_shut_lid_cancels_the_link_it_interrupted() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    let cancelled = Arc::new(AtomicBool::new(false));
    let seen = cancelled.clone();
    app.start_link(
        fake_starter(move |_, cancel: &Cancel| {
            let deadline = Instant::now() + BAIL;
            while !cancel.is_cancelled() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(2));
            }
            seen.store(cancel.is_cancelled(), Ordering::SeqCst);
            Err(io_err(io::ErrorKind::Interrupted))
        }),
        0,
    );
    app.update(1.0 / 60.0);
    app.apply(Action::LidClose);
    assert!(
        !app.game_menu_open(),
        "the overlay outlived the game it was drawn over"
    );
    let deadline = Instant::now() + BAIL;
    while !cancelled.load(Ordering::SeqCst) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        cancelled.load(Ordering::SeqCst),
        "a starter dropped mid-wait leaves a host's access point up for thirty seconds"
    );
}

/// The screen opens over a live session and shows it with the key that ends it. The core keeps
/// running (`Session::sync_speed`) and `Session::overlaid` keeps the buttons from the game.
#[test]
fn the_shortcut_opens_the_connected_screen_over_a_live_session() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    let log = watched(&mut app);
    app.begin_link(0);
    app.apply(Action::GameMenu);
    assert!(
        matches!(app.game_menu(), Some(GameMenu::Linked { opened: true, .. })),
        "the shortcut did not open the connected screen"
    );
    assert!(app.link_active(), "opening the screen ended the session");
    assert_eq!(app.toast(), None, "nothing has happened to announce yet");
    assert!(
        log.jobs().is_empty(),
        "the radio was touched by a screen that only opened"
    );
}

/// B changes nothing: the session and its radio carry on.
#[test]
fn b_leaves_the_session_running() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.begin_link(0);
    app.apply(Action::GameMenu);
    let log = watched(&mut app);
    app.apply(Action::GbaDown(Btn::B));
    assert!(!app.game_menu_open(), "B did not leave the screen");
    assert!(app.link_active(), "B ended the session it was opened over");
    assert!(
        !log.jobs().contains(&RadioJob::Cool),
        "leaving a live session cooled the radio it runs on"
    );
}

/// A ends the session immediately, and the banner says so.
#[test]
fn a_ends_the_session_and_says_so() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.begin_link(0);
    app.apply(Action::GameMenu);
    let log = watched(&mut app);
    app.apply(Action::GbaDown(Btn::A));
    assert!(!app.link_active(), "A left the session running");
    assert_eq!(app.toast(), Some(Toast::LinkEnded));
    // The session ends on the frame the key landed; the unplug animation only catches up.
    assert!(
        matches!(app.game_menu(), Some(GameMenu::Unplug { .. })),
        "A did not put the plug back out: {:?}",
        app.game_menu()
    );
    for _ in 0..((UNPLUG_HOLD_MS / 16 + 4) as usize) {
        app.update(1.0 / 60.0);
    }
    assert!(!app.game_menu_open(), "the screen stayed up over the game");
    // Down, then a cool for a BaseOS whose down leaves the driver loaded. Exactly those two:
    // closing through `close_game_menu` would cool a second time.
    assert_eq!(log.jobs(), vec![RadioJob::Down, RadioJob::Cool]);
}

/// The unplug also plays on the far device, which has no screen up, so both ends agree.
#[test]
fn a_peer_ending_the_link_unplugs_on_this_device_too() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.begin_link(0);
    assert!(!app.game_menu_open(), "nothing should be on screen yet");

    app.peer_ended();

    assert!(
        !app.link_active(),
        "the ending waited on the animation instead of the other way round"
    );
    assert!(
        matches!(app.game_menu(), Some(GameMenu::Unplug { .. })),
        "the far end's ending did not unplug on this device: {:?}",
        app.game_menu()
    );
    assert_eq!(app.toast(), Some(Toast::PeerEnded));

    for _ in 0..((UNPLUG_HOLD_MS / 16 + 4) as usize) {
        app.update(1.0 / 60.0);
    }
    assert!(
        !app.game_menu_open(),
        "the unplug screen never left by itself"
    );
}

/// The menu must draw something over the game, with a scrim between, or on a device it reads
/// as a chord that swallows buttons.
#[test]
fn the_link_screen_draws_its_role_over_the_game() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    let roles = fake_roles(&mut app);
    app.apply(Action::GameMenu);
    let mut out = Vec::new();
    app.draw(&mut out);
    let scrim = out
        .iter()
        .position(|d| {
            matches!(*d, Draw::Rect { w, h, colour, .. }
        if w == OUT_W as f32 && h == OUT_H as f32 && colour == opening())
        })
        .expect("the screen drew no ground over the game");
    let host = out
        .iter()
        .position(|d| matches!(*d, Draw::Tex { tex, .. } if tex == roles[0].0))
        .expect("HOST never reached the frame");
    assert!(host > scrim);
}

// --- the two wirings into the running game ------------------------------------------------
//
// `App` holds neither the core nor the transport, so these drive a real `Session` to prove the
// game pauses and the link's wire reaches the core's thread.

/// A real `Session` with a gpSP cart playing, on the mock core. `selected_core.ini` saying gpSP
/// is what makes the link screen exist.
fn session_playing_on_gpsp() -> (Session, TempDir, Millis) {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    // A cart gpSP can carry, as in `playing_on`: the link screen does not open otherwise.
    common::write_retail_header(&d, "Emerald", "POKEMON RUBY", "AXVE");
    slot_store::write_selected_core(d.path(), "Emerald", Core::Gpsp).expect("write core");
    write_slot_state(
        d.path(),
        &SlotState {
            cart: Some("Emerald".into()),
            clock_set: true,
            utc_offset_min: 0,
            ..Default::default()
        },
    )
    .expect("write slot.state");
    let mut s = Session::boot(d.path().to_path_buf());
    let mut now: Millis = 0;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(1));
    }
    (s, d, now)
}

fn step(s: &mut Session, now: &mut Millis, events: &[RawEvent]) {
    *now += 16;
    s.feed(events.iter().copied(), *now);
    s.update(1.0 / 60.0);
}

/// Frames until the worker has read the speed it was set to (`observed_speed`), not just been
/// told it.
fn runs_at(s: &mut Session, now: &mut Millis, want: Speed) -> bool {
    let deadline = Instant::now() + BAIL;
    while s.observed_speed() != Some(want) {
        if Instant::now() >= deadline {
            return false;
        }
        step(s, now, &[]);
        std::thread::sleep(Duration::from_millis(1));
    }
    true
}

/// The menu pauses the game through `Session::held`, or the core and motor run on behind it.
/// Driven from raw button edges so the opening chord goes through the real gesture layer.
#[test]
fn the_open_menu_pauses_the_game_underneath_it() {
    let (mut s, _d, mut now) = session_playing_on_gpsp();
    assert!(
        runs_at(&mut s, &mut now, Speed::Normal),
        "the game never started running, so pausing it proves nothing"
    );
    step(
        &mut s,
        &mut now,
        &[RawEvent::Down(Btn::Select), RawEvent::Down(Btn::Menu)],
    );
    assert!(
        s.app().game_menu_open(),
        "SELECT+MENU never reached the app through the gesture layer"
    );
    assert!(
        runs_at(&mut s, &mut now, Speed::Paused),
        "the game ran on behind the menu"
    );
}

/// A started link hands its transport to the emulator thread, not only marks itself live.
#[test]
fn a_started_link_reaches_the_emulator_thread_with_its_transport() {
    let (mut s, _d, mut now) = session_playing_on_gpsp();
    let port = common::free_port();
    let far = std::thread::spawn(move || TcpLink::host("127.0.0.1", port).expect("host"));
    std::thread::sleep(Duration::from_millis(150));
    s.app_mut().start_link(
        fake_starter(move |_, _| TcpLink::join("127.0.0.1", port)),
        1,
    );
    let deadline = Instant::now() + BAIL;
    while s.app().game_menu_open() {
        assert!(Instant::now() < deadline, "the link never came up");
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(1));
    }
    let _far = far.join().expect("host thread");
    assert!(s.app().link_active(), "no session started at all");
    assert_eq!(s.app().link_client_id(), Some(1), "the joiner is client 1");
    let deadline = Instant::now() + BAIL;
    while !s.emu().is_some_and(|e| e.net().is_active()) {
        assert!(
            Instant::now() < deadline,
            "the transport never reached the emulator thread"
        );
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// The menu's presses are masked from the game, or a link coming up before A is released
/// resumes the game with A already down.
#[test]
fn a_button_the_menu_is_using_never_reaches_the_game() {
    let (mut s, _d, mut now) = session_playing_on_gpsp();
    step(
        &mut s,
        &mut now,
        &[RawEvent::Down(Btn::Select), RawEvent::Down(Btn::Menu)],
    );
    assert!(s.app().game_menu_open(), "the chord never reached the app");
    step(&mut s, &mut now, &[RawEvent::Down(Btn::A)]);
    assert_eq!(
        s.emu().expect("a core is running").input(),
        ButtonMask(0),
        "the press that picked a row was handed to the game as well"
    );
}

/// A far end dropped over loopback breaks the badge and then ends the session on both `App`
/// and the emulator thread, through `Session::update`'s lost-peer hop.
#[test]
fn a_dropped_peer_breaks_the_badge_and_ends_the_session_end_to_end() {
    let (mut s, _d, mut now) = session_playing_on_gpsp();
    let port = common::free_port();
    let far = std::thread::spawn(move || TcpLink::host("127.0.0.1", port).expect("host"));
    std::thread::sleep(Duration::from_millis(150));
    s.app_mut().start_link(
        fake_starter(move |_, _| TcpLink::join("127.0.0.1", port)),
        1,
    );
    let deadline = Instant::now() + BAIL;
    while s.app().game_menu_open() {
        assert!(Instant::now() < deadline, "the link never came up");
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(1));
    }
    let far = far.join().expect("host thread");

    // Live on both sides before anything is dropped.
    let deadline = Instant::now() + BAIL;
    while !(s.app().link_active() && s.emu().is_some_and(|e| e.net().is_active())) {
        assert!(
            Instant::now() < deadline,
            "the link never went live on both sides"
        );
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(2));
    }

    // `TcpLink`'s `Drop` sends a real FIN.
    drop(far);

    let deadline = Instant::now() + BAIL;
    while s.app().link_badge() != slot_ui::LinkBadge::JoinedLost {
        assert!(Instant::now() < deadline, "the badge never broke");
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(2));
    }

    // The broken badge has to be seen for `LINK_LOST_MS` before the session ends on its own.
    let margin_steps = (LINK_LOST_MS / 16) as usize + 30;
    for _ in 0..margin_steps {
        step(&mut s, &mut now, &[]);
    }
    assert!(!s.app().link_active(), "the session never ended");

    // The ending reaches the emulator thread too, which only `Session::bridge_link` does.
    let deadline = Instant::now() + BAIL;
    while s.emu().is_some_and(|e| e.net().is_active()) {
        assert!(
            Instant::now() < deadline,
            "bridge_link never carried the ending to the emulator thread"
        );
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// A link ended on the far device ends here too, well inside `LINK_LOST_MS`, and the banner
/// says which end it was. The far `TcpLink` stays open after sending the control frame, so
/// only the message can explain the ending.
#[test]
fn a_peer_that_ends_the_link_ends_this_session_without_waiting_out_the_timeout() {
    let (mut s, _d, mut now) = session_playing_on_gpsp();
    let port = common::free_port();
    let far = std::thread::spawn(move || TcpLink::host("127.0.0.1", port).expect("host"));
    std::thread::sleep(Duration::from_millis(150));
    s.app_mut().start_link(
        fake_starter(move |_, _| TcpLink::join("127.0.0.1", port)),
        1,
    );
    let deadline = Instant::now() + BAIL;
    while s.app().game_menu_open() {
        assert!(Instant::now() < deadline, "the link never came up");
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(1));
    }
    let mut far = far.join().expect("host thread");

    let deadline = Instant::now() + BAIL;
    while !(s.app().link_active() && s.emu().is_some_and(|e| e.net().is_active())) {
        assert!(
            Instant::now() < deadline,
            "the link never went live on both sides"
        );
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(2));
    }

    // The far player ends it. This side has not stepped, so nothing of its own closed the wire.
    far.send_end();
    assert!(
        !far.is_closed(),
        "the far socket was already closed, so nothing below is about the message"
    );

    let began = now;
    let deadline = Instant::now() + BAIL;
    while s.app().link_active() {
        assert!(
            Instant::now() < deadline,
            "the far end's ending never reached this session"
        );
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(2));
    }

    assert!(
        now - began < LINK_LOST_MS,
        "the session took {}ms to end, which is the lost-peer timeout rather than the message",
        now - began
    );
    assert_eq!(
        s.app().toast(),
        Some(Toast::PeerEnded),
        "the banner did not say the link had been ended from the other end"
    );
    assert_eq!(
        s.app().link_badge(),
        slot_ui::LinkBadge::Off,
        "a deliberate ending broke the badge as if the peer had vanished"
    );

    // And it reached the emulator thread, which only `bridge_link` does.
    let deadline = Instant::now() + BAIL;
    while s.emu().is_some_and(|e| e.net().is_active()) {
        assert!(
            Instant::now() < deadline,
            "bridge_link never carried the ending to the emulator thread"
        );
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Sprites distinguishable only by their `TexId`, so a test can tell which one was drawn.
fn fake_link_sprites() -> slot::link_screen::LinkSprites {
    let s = |n: usize| slot::link_screen::Sprite {
        tex: TexId::from_raw(n),
        w: 10,
        h: 10,
    };
    slot::link_screen::LinkSprites {
        port: s(1),
        plug_host: s(2),
        plug_join: s(3),
        adapter: s(4),
        arcs_right: [s(7), s(8), s(9)],
        arcs_left: [s(10), s(11), s(12)],
        clicks: s(13),
        arrow_left: s(14),
        arrow_right: s(15),
    }
}

/// The first cart on the card in the slot and running on gpSP, with `fake_link_sprites`' faces
/// to tell the plug from the adapter.
fn seated_on_gpsp(d: &TempDir) -> App {
    seated_on(d, Core::Gpsp)
}

/// The same, on a named core, since the core decides whether the link screen opens.
fn seated_on(d: &TempDir, core: Core) -> App {
    seated_on_platform(d, core, Platform::Gba)
}

/// The same, for any platform. Core and platform are set together as `session::spawn_core`
/// does: a cart with no platform yet would be answered as a GBA cart.
fn seated_on_platform(d: &TempDir, core: Core, platform: Platform) -> App {
    let mut app = common::boot(d.path());
    app.apply(Action::Insert);
    app.set_core(core);
    app.set_platform(platform);
    app.on_core_ready();
    for _ in 0..120 {
        app.update(1.0 / 60.0);
    }
    app.set_link_sprites(fake_link_sprites());
    app
}

/// A cart in the slot, its link screen open and drawn, with `fake_link_sprites`' faces to
/// tell the plug from the adapter.
fn open_link_screen(d: &TempDir) -> Vec<Draw> {
    let mut app = seated_on_gpsp(d);
    app.apply(Action::GameMenu);
    let mut out = Vec::new();
    app.draw(&mut out);
    out
}

/// A retail Pokémon cart links over the Wireless Adapter, so that is what its screen shows.
#[test]
fn a_pokemon_cart_shows_the_adapter() {
    let d = common::tmp_root_with_carts(&["Zzz"]);
    // Written before `boot`, so the shelf scan reads it. It sorts before "Zzz", so it is seated.
    common::write_retail_header(&d, "Pokemon Emerald", "POKEMON EMER", "BPEE");
    let out = open_link_screen(&d);
    assert!(
        out.iter()
            .any(|d| matches!(*d, Draw::Tex { tex, .. } if tex == TexId::from_raw(4))),
        "no adapter"
    );
    assert!(
        !out.iter()
            .any(|d| matches!(*d, Draw::Tex { tex, .. } if tex == TexId::from_raw(2))),
        "a plug on a wireless cart"
    );
}

/// gpSP forces a Pokémon ROM with a non-standard header (a hack) to the cable, whatever its title.
#[test]
fn a_pokemon_hack_shows_the_cable() {
    let d = common::tmp_root_with_carts(&["Zzz"]);
    common::write_retail_header(&d, "Pokemon Emerald", "POKEMON EMER", "BPEE");
    // Overwrite the entry branch opcode byte gpSP checks, leaving title and code retail.
    let rom = d.path().join("Games/GBA").join("Pokemon Emerald.gba");
    let mut bytes = std::fs::read(&rom).expect("read rom");
    bytes[3] = 0;
    std::fs::write(&rom, bytes).expect("rewrite rom");
    let out = open_link_screen(&d);
    assert!(
        out.iter()
            .any(|d| matches!(*d, Draw::Tex { tex, .. } if tex == TexId::from_raw(2))),
        "no plug"
    );
    assert!(
        !out.iter()
            .any(|d| matches!(*d, Draw::Tex { tex, .. } if tex == TexId::from_raw(4))),
        "the adapter on a Pokémon hack"
    );
}

// --- the cable or the adapter -------------------------------------------------------------
//
// The screen opens on the hardware gpSP would pick and SELECT switches it. gpSP reads its link
// mode only while a game loads, so linking in the mode it was not loaded with reloads the game
// behind the screen first. `App` asks for that and `Session` carries it out.

/// A tap of SELECT the way the gesture layer delivers one: on the release, with no chord.
fn select(app: &mut App) {
    app.apply(Action::GbaDown(Btn::Select));
    app.apply(Action::GbaUp(Btn::Select));
}

/// Which hardware the open screen draws, told apart by `fake_link_sprites`' faces: 2 and 3
/// are the two plugs, 4 the adapter.
fn drawn_hardware(app: &App) -> LinkKind {
    let mut out = Vec::new();
    app.draw(&mut out);
    let drew = |n: usize| {
        out.iter().any(|d| {
            matches!(*d, Draw::Tex { tex, .. } | Draw::Turned { tex, .. }
                if tex == TexId::from_raw(n))
        })
    };
    match (drew(2) || drew(3), drew(4)) {
        (true, false) => LinkKind::Cable,
        (false, true) => LinkKind::Wireless,
        other => panic!("the screen drew (plug, adapter) = {other:?}"),
    }
}

/// Frames until the link worker reports its socket step, which tells a started link from a
/// screen still waiting on a reload.
fn reaches_waiting(app: &mut App) -> bool {
    let deadline = Instant::now() + BAIL;
    while !matches!(
        app.game_menu(),
        Some(GameMenu::Working {
            step: LinkStep::Waiting,
            ..
        })
    ) {
        if Instant::now() >= deadline {
            return false;
        }
        app.update(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(1));
    }
    true
}

/// Frames for a stretch of wall clock long enough for a worker, had one been started, to have
/// moved the screen on.
fn idle(app: &mut App, ms: u64) {
    let until = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < until {
        app.update(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// The link screen open as Join with the hardware switched, and A pressed. Join, so the
/// worker reaches out rather than binding the port every test shares.
fn switched_and_picked() -> (App, TempDir) {
    let (mut app, d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    app.apply(Action::GbaDown(Btn::Right));
    select(&mut app);
    app.apply(Action::GbaDown(Btn::A));
    (app, d)
}

/// SELECT swaps the cable for the adapter on the same frame, and the choice is kept per cart.
#[test]
fn select_on_pick_switches_the_hardware_and_the_cart_keeps_it() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.set_link_sprites(fake_link_sprites());
    app.apply(Action::GameMenu);
    assert_eq!(
        drawn_hardware(&app),
        LinkKind::Cable,
        "the test cart's own header links by cable"
    );
    select(&mut app);
    assert_eq!(
        drawn_hardware(&app),
        LinkKind::Wireless,
        "SELECT switched nothing"
    );
    assert_eq!(
        app.game_menu(),
        Some(GameMenu::Pick(LinkRow::Host)),
        "SELECT did more than switch the hardware"
    );
    app.apply(Action::GbaDown(Btn::B));
    app.apply(Action::GameMenu);
    assert_eq!(
        drawn_hardware(&app),
        LinkKind::Wireless,
        "the switch was forgotten when the screen closed"
    );
    select(&mut app);
    assert_eq!(
        drawn_hardware(&app),
        LinkKind::Cable,
        "SELECT only goes one way"
    );
}

/// Switched away and back is the mode the game runs, so A starts the link with no reload.
#[test]
fn a_in_the_mode_the_game_already_runs_starts_the_link_straight_away() {
    // One live link per process: every test shares the product's port.
    let _link = common::link_port_lock();
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    app.apply(Action::GbaDown(Btn::Right));
    select(&mut app);
    select(&mut app);
    app.apply(Action::GbaDown(Btn::A));
    assert_eq!(
        app.take_link_reload(),
        None,
        "a mode the game already runs asked for a reload"
    );
    assert!(reaches_waiting(&mut app), "A started no link");
    app.apply(Action::GbaDown(Btn::B));
    settle(&mut app);
}

/// A in the other mode asks for a reload (cart and `gpsp_serial`) and starts nothing yet, or a
/// link would come up over the old mode.
#[test]
fn a_in_a_switched_mode_asks_for_the_game_to_reload_first() {
    // One live link per process: every test shares the product's port.
    let _link = common::link_port_lock();
    let (mut app, _d) = switched_and_picked();
    assert_eq!(
        app.take_link_reload(),
        Some(("Emerald".to_string(), "rfu")),
        "no reload asked for, or the wrong one"
    );
    assert_eq!(
        app.take_link_reload(),
        None,
        "the request was handed on twice"
    );
    assert!(
        matches!(
            app.game_menu(),
            Some(GameMenu::Working {
                role: LinkRow::Join,
                step: LinkStep::Radio,
                ..
            })
        ),
        "the screen did not go to its first step: {:?}",
        app.game_menu()
    );
    idle(&mut app, 150);
    assert!(
        matches!(
            app.game_menu(),
            Some(GameMenu::Working {
                step: LinkStep::Radio,
                ..
            })
        ),
        "a link started before the game reloaded: {:?}",
        app.game_menu()
    );
}

/// After the reload the link starts in the picked role, and the screen carries on from its
/// current step.
#[test]
fn the_reload_finishing_starts_the_link() {
    // One live link per process: every test shares the product's port.
    let _link = common::link_port_lock();
    let (mut app, _d) = switched_and_picked();
    let Some(GameMenu::Working { since, .. }) = app.game_menu() else {
        panic!("A did not start working: {:?}", app.game_menu());
    };
    app.take_link_reload().expect("no reload asked for");
    idle(&mut app, 50);
    app.link_reload_done();
    assert!(
        matches!(
            app.game_menu(),
            Some(GameMenu::Working { role: LinkRow::Join, since: s, .. }) if s == since
        ),
        "the screen started over: {:?}",
        app.game_menu()
    );
    assert!(
        reaches_waiting(&mut app),
        "the reload finished and no link started"
    );
    app.apply(Action::GbaDown(Btn::B));
    settle(&mut app);
}

/// B during the reload is heard; once the reload finishes the screen closes and hands the game
/// back instead of linking.
#[test]
fn b_during_the_reload_hands_the_game_back_once_it_finishes() {
    // One live link per process: every test shares the product's port.
    let _link = common::link_port_lock();
    let (mut app, _d) = switched_and_picked();
    app.take_link_reload().expect("no reload asked for");
    app.apply(Action::GbaDown(Btn::B));
    idle(&mut app, 50);
    assert!(
        matches!(app.game_menu(), Some(GameMenu::Working { .. })),
        "B handed the game back while it was still loading"
    );
    app.link_reload_done();
    assert!(!app.game_menu_open(), "the cancel left a screen behind");
    assert!(matches!(app.phase(), Phase::Playing { .. }));
    idle(&mut app, 150);
    assert!(!app.game_menu_open(), "a link started anyway");
    assert!(!app.link_active());
}

/// A game that will not load in the switched mode goes back to the one it came from, the
/// switch is undone, and the refusal shakes.
#[test]
fn a_reload_that_fails_goes_back_to_the_mode_the_game_came_from() {
    // One live link per process: every test shares the product's port.
    let _link = common::link_port_lock();
    let (mut app, _d) = switched_and_picked();
    app.set_link_sprites(fake_link_sprites());
    assert_eq!(app.take_link_reload(), Some(("Emerald".to_string(), "rfu")));
    app.link_reload_failed();
    assert_eq!(
        app.take_link_reload(),
        Some(("Emerald".to_string(), "auto")),
        "the reload that failed did not go back to the mode the game came from"
    );
    assert!(
        matches!(app.game_menu(), Some(GameMenu::Working { .. })),
        "the screen closed with the game still loading behind it"
    );
    app.link_reload_done();
    assert!(
        !app.game_menu_open(),
        "the screen stayed up over a switch that never happened"
    );
    assert!(matches!(app.phase(), Phase::Playing { .. }));
    assert!(
        app.refusal_active(app.now()),
        "the game came back without saying the switch was refused"
    );
    idle(&mut app, 150);
    assert!(!app.game_menu_open(), "a link started anyway");
    assert!(!app.link_active());
    app.apply(Action::GameMenu);
    assert_eq!(
        drawn_hardware(&app),
        LinkKind::Cable,
        "the cart kept the switch that failed"
    );
}

/// Neither mode loads, so the cart comes back out of the slot with the alert rather than
/// sitting seated with no core.
#[test]
fn a_game_that_loads_in_neither_mode_comes_back_out_refused() {
    // One live link per process: every test shares the product's port.
    let _link = common::link_port_lock();
    let (mut app, _d) = switched_and_picked();
    app.take_link_reload().expect("no reload asked for");
    app.link_reload_failed();
    app.take_link_reload().expect("no way back asked for");
    app.link_reload_failed();
    assert_eq!(app.take_link_reload(), None, "a third load was asked for");
    assert!(!app.game_menu_open(), "the screen outlived the game");
    assert!(
        matches!(app.phase(), Phase::Ejecting { .. }),
        "the cart stayed seated with no game: {:?}",
        app.phase()
    );
    assert!(
        app.alert_visible(),
        "the cart came out without saying it was refused"
    );
    for _ in 0..120 {
        app.update(1.0 / 60.0);
    }
    assert!(matches!(app.phase(), Phase::Shelf), "{:?}", app.phase());
}

/// A shut lid closes the screen, but a reload underway still ends in a game or on the shelf,
/// never a seated cart with no core.
#[test]
fn a_lid_shut_over_a_game_that_loads_in_neither_mode_opens_onto_the_shelf() {
    // One live link per process: every test shares the product's port.
    let _link = common::link_port_lock();
    let (mut app, _d) = switched_and_picked();
    app.take_link_reload().expect("no reload asked for");
    app.apply(Action::LidClose);
    assert!(matches!(app.phase(), Phase::Doze { .. }));
    app.link_reload_failed();
    assert_eq!(
        app.take_link_reload(),
        Some(("Emerald".to_string(), "auto")),
        "the shut lid dropped the way back"
    );
    app.link_reload_failed();
    app.apply(Action::LidOpen);
    assert!(
        matches!(app.phase(), Phase::Shelf),
        "the lid opened onto a cart with no game: {:?}",
        app.phase()
    );
}

/// A game with no cable protocol loads on `auto` and gpSP links it over the adapter either
/// way, so SELECT is refused with the shake and A links straight away.
#[test]
fn select_is_refused_where_gpsp_would_link_the_same_either_way() {
    // One live link per process: every test shares the product's port.
    let _link = common::link_port_lock();
    let d = common::tmp_root_with_carts(&["Zzz"]);
    // "Mario Golf" sorts before "Zzz", so `Action::Insert` seats it.
    common::write_retail_header(&d, "Mario Golf", "MARIO GOLF", "BMGE");
    let mut app = seated_on_gpsp(&d);
    app.apply(Action::GameMenu);
    assert_eq!(drawn_hardware(&app), LinkKind::Wireless);
    app.apply(Action::GbaDown(Btn::Right));
    select(&mut app);
    assert!(app.refusal_active(app.now()), "SELECT was not refused");
    assert_eq!(
        drawn_hardware(&app),
        LinkKind::Wireless,
        "a plug drawn over a game gpSP links by adapter"
    );
    assert_eq!(app.game_menu(), Some(GameMenu::Pick(LinkRow::Join)));
    app.apply(Action::GbaDown(Btn::A));
    assert_eq!(
        app.take_link_reload(),
        None,
        "a reload for a mode gpSP would not change"
    );
    assert!(reaches_waiting(&mut app), "A started no link");
    app.apply(Action::GbaDown(Btn::B));
    settle(&mut app);
}

/// A compares the pick with the `gpsp_serial` the core was loaded with, not what the screen
/// opened on.
#[test]
fn a_reloads_only_for_a_serial_the_core_was_not_loaded_with() {
    // One live link per process: every test shares the product's port.
    let _link = common::link_port_lock();
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.set_link_loaded("rfu");
    app.apply(Action::GameMenu);
    app.apply(Action::GbaDown(Btn::Right));
    select(&mut app);
    app.apply(Action::GbaDown(Btn::A));
    assert_eq!(
        app.take_link_reload(),
        None,
        "the adapter, picked over a core loaded on rfu, asked for a reload"
    );
    assert!(reaches_waiting(&mut app), "A started no link");
    app.apply(Action::GbaDown(Btn::B));
    settle(&mut app);
    app.apply(Action::GameMenu);
    select(&mut app);
    app.apply(Action::GbaDown(Btn::A));
    assert_eq!(
        app.take_link_reload(),
        Some(("Emerald".to_string(), "auto")),
        "the cable, picked over a core loaded on rfu, linked without a reload"
    );
}

/// A core that refused its resume: running, but on its own default machine, which a flush
/// will not write back.
struct RefusedResume;

impl Snapshot for RefusedResume {
    fn state(&self) -> Option<Vec<u8>> {
        Some(vec![0; 8])
    }

    fn save_ram(&self) -> Option<Vec<u8>> {
        None
    }

    fn thumb(&self) -> Option<Vec<u8>> {
        None
    }

    fn load(&self, _state: Vec<u8>) {}

    fn resume_trusted(&self) -> bool {
        false
    }
}

/// Over a core that refused its resume, the flush will not overwrite the player's state, so a
/// reload would lose progress. A in a switched mode is refused; the current mode still links.
#[test]
fn a_switch_is_refused_over_a_resume_the_core_would_not_take() {
    // One live link per process: every test shares the product's port.
    let _link = common::link_port_lock();
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.set_snapshot(Box::new(RefusedResume));
    app.apply(Action::GameMenu);
    app.apply(Action::GbaDown(Btn::Right));
    select(&mut app);
    app.apply(Action::GbaDown(Btn::A));
    assert_eq!(
        app.take_link_reload(),
        None,
        "a reload over a state that cannot be saved"
    );
    assert!(app.refusal_active(app.now()), "A was not refused");
    assert_eq!(
        app.game_menu(),
        Some(GameMenu::Pick(LinkRow::Join)),
        "the refusal left Pick"
    );
    select(&mut app);
    app.apply(Action::GbaDown(Btn::A));
    assert!(
        reaches_waiting(&mut app),
        "the mode the game already runs did not link"
    );
    app.apply(Action::GbaDown(Btn::B));
    settle(&mut app);
}

/// Pick names every key it takes (B, SELECT, arrows, A), using the real faces so the row is
/// proven to fit the console strip.
#[test]
fn pick_names_cancel_mode_swap_and_link_across_the_strip() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    let width = |k: LinkLegend| match k {
        LinkLegend::Cancel => hint_face("B", "Cancel").w,
        LinkLegend::Mode => hint_face("SELECT", "Mode").w,
        LinkLegend::Swap => arrows_hint_face("Swap").w,
        LinkLegend::Link => hint_face("A", "Link").w,
        LinkLegend::Ok => hint_face("A", "OK").w,
        LinkLegend::Back => hint_face("B", "Back").w,
        LinkLegend::EndLink => hint_face("A", "End Link").w,
    };
    let faces: Vec<(TexId, u32)> = LinkLegend::ALL
        .iter()
        .map(|k| (TexId::from_raw(900 + k.index()), width(*k)))
        .collect();
    app.set_link_legend_faces(faces.clone());
    app.apply(Action::GameMenu);
    let mut out = Vec::new();
    app.draw(&mut out);
    let mut keys: Vec<(f32, f32, LinkLegend)> = out
        .iter()
        .filter_map(|d| match *d {
            Draw::Tex { x, w, tex, .. } => LinkLegend::ALL
                .iter()
                .find(|k| faces[k.index()].0 == tex)
                .map(|k| (x, w, *k)),
            _ => None,
        })
        .collect();
    keys.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert_eq!(
        keys.iter().map(|k| k.2).collect::<Vec<_>>(),
        [
            LinkLegend::Cancel,
            LinkLegend::Mode,
            LinkLegend::Swap,
            LinkLegend::Link
        ],
    );
    let left = keys[0].0;
    let (x, w, _) = keys[keys.len() - 1];
    let right = x + w - HINT_EDGE as f32;
    println!(
        "pick legend: faces {:?} wide, drawn from x {left} to {right} of {OUT_W}",
        keys.iter().map(|k| k.1).collect::<Vec<_>>()
    );
    assert!(
        left >= 0.0 && right <= OUT_W as f32,
        "the legend runs off the strip: {left}..{right}"
    );
}

/// The mock core's own frame counter, which is the whole of its save state.
fn counter(s: &Session) -> u64 {
    let state = s
        .emu()
        .expect("a core is running")
        .request_state()
        .recv_timeout(BAIL)
        .expect("the core gave up no state");
    u64::from_le_bytes(state.try_into().expect("mock state is 8 bytes"))
}

/// The reload end to end through a real `Session`: the new emulator carries on from exactly
/// where the old one stopped, and only then does the link start.
#[test]
fn a_link_in_a_switched_mode_reloads_the_game_and_then_starts_the_link() {
    let (mut s, _d, mut now) = session_playing_on_gpsp();
    assert!(
        runs_at(&mut s, &mut now, Speed::Normal),
        "the game never started running, so a fresh core would look the same"
    );
    step(
        &mut s,
        &mut now,
        &[RawEvent::Down(Btn::Select), RawEvent::Down(Btn::Menu)],
    );
    step(
        &mut s,
        &mut now,
        &[RawEvent::Up(Btn::Menu), RawEvent::Up(Btn::Select)],
    );
    assert!(s.app().game_menu_open(), "the chord never reached the app");
    assert!(
        runs_at(&mut s, &mut now, Speed::Paused),
        "the game ran on behind the menu"
    );
    step(&mut s, &mut now, &[RawEvent::Down(Btn::Right)]);
    step(&mut s, &mut now, &[RawEvent::Up(Btn::Right)]);
    step(&mut s, &mut now, &[RawEvent::Down(Btn::Select)]);
    step(&mut s, &mut now, &[RawEvent::Up(Btn::Select)]);

    let played = counter(&s);
    assert!(
        played > 0 && s.frames_published() > 0,
        "nothing ran before the reload"
    );

    step(&mut s, &mut now, &[RawEvent::Down(Btn::A)]);
    assert_eq!(
        s.frames_published(),
        0,
        "the emulator that was running is still the one in the slot"
    );
    let deadline = Instant::now() + BAIL;
    while s.emu().map(EmuHandle::state) != Some(CoreState::Ready) {
        assert!(Instant::now() < deadline, "the reloaded game never loaded");
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        counter(&s),
        played,
        "the reloaded game did not come back where it left off"
    );

    let deadline = Instant::now() + BAIL;
    while !matches!(
        s.app().game_menu(),
        Some(GameMenu::Working {
            step: LinkStep::Waiting,
            ..
        })
    ) {
        assert!(
            Instant::now() < deadline,
            "the game reloaded and no link started: {:?}",
            s.app().game_menu()
        );
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(1));
    }

    step(&mut s, &mut now, &[RawEvent::Down(Btn::B)]);
    let deadline = Instant::now() + BAIL;
    while s.app().game_menu_open() {
        assert!(Instant::now() < deadline, "the cancel never landed");
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// A `Session` over a real core planted under gpSP's name, and a real ROM, since the mock loads
/// anything. `None` with no core to plant; the caller holds `core_lock`.
fn session_on_a_real_core() -> Option<(Session, TempDir, Millis)> {
    let core = common::vendored_core()?;
    let d = common::tmp_root_with_real_carts(&["Emerald"]);
    // A real ROM, but carrying Ruby's identity, so the link screen this test drives will open.
    common::write_real_cart_as(&d, "Emerald", "POKEMON RUBY", "AXVE");
    slot_store::write_selected_core(d.path(), "Emerald", Core::Gpsp).expect("write core");
    std::fs::copy(
        &core,
        d.path()
            .join("System")
            .join(slot::core::dylib_name(Core::Gpsp)),
    )
    .expect("plant a core under gpSP's name");
    write_slot_state(
        d.path(),
        &SlotState {
            cart: Some("Emerald".into()),
            clock_set: true,
            utc_offset_min: 0,
            ..Default::default()
        },
    )
    .expect("write slot.state");
    let mut s = Session::boot(d.path().to_path_buf());
    let mut now: Millis = 0;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(1));
    }
    Some((s, d, now))
}

/// With the ROM removed under the running game, neither mode reloads: the session goes back
/// once, gives up, and ejects the cart refused, never leaving it seated with no core.
#[test]
fn a_game_that_will_not_load_again_comes_back_out_of_the_slot() {
    let _g = common::core_lock();
    let Some((mut s, d, mut now)) = session_on_a_real_core() else {
        eprintln!("no host-openable dylib on this machine, skipping");
        return;
    };
    step(
        &mut s,
        &mut now,
        &[RawEvent::Down(Btn::Select), RawEvent::Down(Btn::Menu)],
    );
    step(
        &mut s,
        &mut now,
        &[RawEvent::Up(Btn::Menu), RawEvent::Up(Btn::Select)],
    );
    assert!(s.app().game_menu_open(), "the chord never reached the app");
    step(&mut s, &mut now, &[RawEvent::Down(Btn::Right)]);
    step(&mut s, &mut now, &[RawEvent::Up(Btn::Right)]);
    step(&mut s, &mut now, &[RawEvent::Down(Btn::Select)]);
    step(&mut s, &mut now, &[RawEvent::Up(Btn::Select)]);
    std::fs::remove_file(d.path().join("Games/GBA").join("Emerald.gba"))
        .expect("take the rom away");

    step(&mut s, &mut now, &[RawEvent::Down(Btn::A)]);
    let deadline = Instant::now() + BAIL;
    while !matches!(s.app().phase(), Phase::Ejecting { .. }) {
        assert!(
            Instant::now() < deadline,
            "the cart never came back out: {:?}",
            s.app().phase()
        );
        assert!(
            s.has_core() || s.app().game_menu_open(),
            "a seated cart was left playing with no core behind it"
        );
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        s.app().alert_visible(),
        "the cart came out without the alert"
    );
    assert!(
        persist::read_resume(d.path(), Platform::Gba, Core::Gpsp, "Emerald").is_some(),
        "the state flushed before the reload is gone"
    );
    let deadline = Instant::now() + BAIL;
    while !matches!(s.app().phase(), Phase::Shelf) {
        assert!(
            Instant::now() < deadline,
            "the refused cart never reached the shelf"
        );
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(1));
    }
}

// --- the carts gpSP cannot link ----------------------------------------------------------
//
// gpSP speaks the Wireless Adapter and three named cable protocols. Any other cart stays on
// `SERIAL_MODE_AUTO`, which its netpacket hooks ignore, so a session would link and then drop
// every packet.

/// Apotris, a cable game absent from gpSP's `gba_over.h`: the screen stays shut and the banner
/// says so.
#[test]
fn a_cart_gpsp_cannot_carry_is_refused_the_link_screen_and_told_why() {
    let d = common::tmp_root_with_carts(&["Apotris", "Zzz"]);
    // "Apotris" sorts before "Zzz", so `Action::Insert` seats it.
    common::write_retail_header(&d, "Apotris", "APOTRIS", "2ATE");
    let mut app = seated_on_gpsp(&d);
    app.apply(Action::GameMenu);
    assert!(
        !app.game_menu_open(),
        "a cart gpSP has no protocol for was offered a link screen"
    );
    assert_eq!(
        app.toast(),
        Some(slot_ui::Toast::NoLink),
        "the press did nothing and said nothing"
    );
    assert!(
        matches!(app.phase(), Phase::Playing { .. }),
        "the refusal took the game away: {:?}",
        app.phase()
    );
    assert!(
        !app.link_active(),
        "a session started for a cart gpSP will not link"
    );
}

/// A cart gpSP does carry still opens the screen with no banner.
#[test]
fn a_cart_gpsp_carries_still_opens_the_link_screen() {
    for (stem, title, code) in [
        ("Mario Golf", "MARIO GOLF", "BMGE"),    // the adapter list
        ("Emerald", "POKEMON EMER", "BPEE"),     // the Pokémon family
        ("Advance Wars", "ADVANCEWARS", "AWRE"), // Advance Wars
    ] {
        let d = common::tmp_root_with_carts(&["Zzz"]);
        common::write_retail_header(&d, stem, title, code);
        let mut app = seated_on_gpsp(&d);
        app.apply(Action::GameMenu);
        assert!(app.game_menu_open(), "{code} was refused its link screen");
        assert_eq!(
            app.toast(),
            None,
            "{code} opened the screen and said so too"
        );
    }
}

/// When both refusals apply, the cart's own wins: Apotris on mGBA is not sent to gpSP, which
/// cannot carry it either.
#[test]
fn a_cart_gpsp_cannot_link_is_refused_on_gpsp_and_carried_by_mgbas_cable() {
    let refused = |core, open: bool| {
        let d = common::tmp_root_with_carts(&["Apotris", "Zzz"]);
        common::write_retail_header(&d, "Apotris", "APOTRIS", "2ATE");
        let mut app = seated_on(&d, core);
        app.apply(Action::GameMenu);
        assert_eq!(
            app.game_menu_open(),
            open,
            "{core:?} answered Apotris with the wrong screen"
        );
        app.toast()
    };
    // gpSP speaks named protocols and has none for this cart, so there is nothing to reach.
    assert_eq!(refused(Core::Gpsp, false), Some(Toast::NoLink));
    // mGBA runs both machines in step, which carries every cart, Apotris included.
    assert_eq!(refused(Core::Mgba, true), None);
}

/// A cart gpSP can link, sitting on mGBA, is still told to switch to gpSP.
#[test]
fn a_wireless_adapter_cart_on_mgba_still_says_to_switch_to_gpsp() {
    let d = common::tmp_root_with_carts(&["Emerald", "Zzz"]);
    common::write_retail_header(&d, "Emerald", "POKEMON EMER", "BPEE");
    let mut app = seated_on(&d, Core::Mgba);
    app.apply(Action::GameMenu);
    assert!(
        !app.game_menu_open(),
        "mGBA offered a cable to a cart that talks to the Wireless Adapter"
    );
    assert_eq!(
        app.toast(),
        Some(Toast::NeedsGpsp),
        "a cart gpSP can carry was told there is no link support for it"
    );
}

/// The platform is asked first: `link_carried` matches Pokémon by title, so `POKEMON RED` in a
/// `.gb` header would pass gpSP's test. On gpSP a Game Boy cart gets `NoLink`, never `NeedsGpsp`.
#[test]
fn a_game_boy_cart_links_on_mgba_and_is_refused_on_gpsp() {
    for (core, open) in [(Core::Mgba, true), (Core::Gpsp, false)] {
        let d = common::tmp_root_with_gb_carts(&["Pokemon Red", "Zzz"]);
        let mut app = seated_on_platform(&d, core, Platform::Gb);
        app.apply(Action::GameMenu);
        assert_eq!(
            app.game_menu_open(),
            open,
            "{core:?} answered a Game Boy cart with the wrong screen"
        );
        assert_ne!(
            app.toast(),
            Some(Toast::NeedsGpsp),
            "{core:?} told a Game Boy cart to switch to gpSP, which cannot run it at all"
        );
        if open {
            continue;
        }
        assert_eq!(
            app.toast(),
            Some(Toast::NoLink),
            "{core:?} answered a Game Boy cart with the wrong banner"
        );
        assert!(
            matches!(app.phase(), Phase::Playing { .. }),
            "{core:?}: the refusal took the game away: {:?}",
            app.phase()
        );
        assert!(
            !app.link_active(),
            "{core:?} started a link session for a Game Boy cart"
        );
    }
}

/// The legend names SELECT only where the press is not refused (see
/// `select_is_refused_where_gpsp_would_link_the_same_either_way`).
#[test]
fn the_pick_legend_names_mode_only_where_the_hardware_can_be_switched() {
    let faces: Vec<(TexId, u32)> = LinkLegend::ALL
        .iter()
        .map(|k| (TexId::from_raw(900 + k.index()), 40))
        .collect();
    let mode = faces[LinkLegend::Mode.index()].0;
    let drawn = |app: &App| {
        let mut out = Vec::new();
        app.draw(&mut out);
        out
    };

    // A Pokémon cart loads differently on cable (`mul_poke`) and adapter (`rfu`), so Mode shows.
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.set_link_legend_faces(faces.clone());
    app.apply(Action::GameMenu);
    assert!(
        drawn(&app)
            .iter()
            .any(|d| matches!(*d, Draw::Tex { tex, .. } if tex == mode)),
        "a cart whose hardware can be switched did not offer SELECT"
    );

    // An adapter-list game with no cable protocol links over the adapter either way.
    let d = common::tmp_root_with_carts(&["Zzz"]);
    common::write_retail_header(&d, "Mario Golf", "MARIO GOLF", "BMGE");
    let mut app = seated_on_gpsp(&d);
    app.set_link_legend_faces(faces.clone());
    app.apply(Action::GameMenu);
    let out = drawn(&app);
    assert!(
        !out.iter()
            .any(|d| matches!(*d, Draw::Tex { tex, .. } if tex == mode)),
        "the screen offered SELECT over a game gpSP links the same either way"
    );
    for k in [LinkLegend::Cancel, LinkLegend::Swap, LinkLegend::Link] {
        let want = faces[k.index()].0;
        assert!(
            out.iter()
                .any(|d| matches!(*d, Draw::Tex { tex, .. } if tex == want)),
            "{k:?} left the legend along with Mode"
        );
    }
}

/// What the radio was asked to do, in order. `App` never waits on it, so a test reads the list.
#[derive(Clone, Default)]
struct RadioLog(Arc<std::sync::Mutex<Vec<RadioJob>>>);

impl RadioLog {
    fn jobs(&self) -> Vec<RadioJob> {
        self.0.lock().expect("radio log").clone()
    }
}

impl RadioJobs for RadioLog {
    fn ask(&mut self, job: RadioJob) {
        self.0.lock().expect("radio log").push(job);
    }

    /// Asked for is not loaded: nothing here runs, so no warm ever finishes.
    fn warmed(&self) -> bool {
        false
    }
}

fn watched(app: &mut App) -> RadioLog {
    let log = RadioLog::default();
    app.set_radio_jobs(Box::new(log.clone()));
    log
}

/// Opening the screen starts warming the driver (about a second) while the player picks a
/// role. `link host` loads it itself if this has not finished.
#[test]
fn opening_the_link_screen_warms_the_radio() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    let log = watched(&mut app);
    app.apply(Action::GameMenu);
    assert_eq!(log.jobs(), vec![RadioJob::Warm]);
}

/// Leaving without starting a link cools the driver, or it drains the battery until power off.
#[test]
fn leaving_the_link_screen_without_a_session_cools_it() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    let log = watched(&mut app);
    app.apply(Action::GameMenu);
    app.apply(Action::GbaDown(Btn::B));
    assert_eq!(log.jobs(), vec![RadioJob::Warm, RadioJob::Cool]);
}

/// Closing because a session started must not cool: that takes the session's network down.
#[test]
fn a_screen_that_closes_over_a_live_session_leaves_the_radio_alone() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    let log = watched(&mut app);
    app.begin_link(0);
    app.apply(Action::GbaDown(Btn::B));
    assert!(
        !log.jobs().contains(&RadioJob::Cool),
        "cooled the radio a live session was running over"
    );
}

/// With no session, the same shortcut opens the screen.
#[test]
fn the_shortcut_still_opens_the_screen_when_nothing_is_linked() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    assert!(app.game_menu_open());
    assert_eq!(app.toast(), None);
}
