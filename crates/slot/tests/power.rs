mod common;

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use common::{
    app_playing_in, boot, clocked, panel, session_with_platform, tmp_root_with_carts,
    tmp_root_with_real_carts, StubSnapshot,
};
use slot::app::Phase;
use slot::emu::Speed;
use slot::session::Session;
use slot_gfx::Draw;
use slot_input::{Action, Btn, Millis, RawEvent, POWER_HOLD_MS};
use slot_store::{read_slot_state, write_slot_state, Core, Platform, SlotState, StateRing};
use slot_ui::PowerChoice;

use slot::link_radio::{RadioJob, RadioJobs};

const FRAME_MS: Millis = 16;
const DT: f32 = 1.0 / 60.0;

#[test]
fn lid_close_flushes_resume_before_dozing() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::LidClose);
    let r = StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald");
    assert!(
        r.read_resume().unwrap().is_some(),
        "state must be durable before doze"
    );
    assert!(matches!(a.phase(), Phase::Doze { .. }));
    assert!(
        r.list().unwrap().is_empty(),
        "lid close must not create a polaroid"
    );
}

#[test]
fn lid_open_returns_to_the_game_without_a_button() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::LidClose);
    a.apply(Action::LidOpen);
    assert!(matches!(a.phase(), Phase::Playing { .. }));
}

#[test]
fn doze_timeout_powers_off_with_the_cart_still_seated() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::LidClose);
    a.on_doze_timeout();
    assert_eq!(
        read_slot_state(d.path()).cart,
        Some("Emerald".into()),
        "power off is not an eject"
    );
}

/// Closing the lid on an empty slot is still a doze, with nothing to flush.
#[test]
fn lid_close_on_the_shelf_wakes_back_to_the_shelf() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = boot(d.path());
    a.set_snapshot(StubSnapshot::boxed());
    a.apply(Action::LidClose);
    assert!(matches!(a.phase(), Phase::Doze { cart: None }));
    assert!(
        StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald")
            .read_resume()
            .unwrap()
            .is_none()
    );
    a.apply(Action::LidOpen);
    assert!(matches!(a.phase(), Phase::Shelf));
}

/// The switcher is a pause over the game, so lid close and open land back in the game.
#[test]
fn lid_close_over_the_switcher_wakes_into_the_game() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::SaveState);
    a.apply(Action::Polaroids);
    a.apply(Action::LidClose);
    a.apply(Action::LidOpen);
    assert!(matches!(a.phase(), Phase::Playing { .. }));
    assert_eq!(
        StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald")
            .list()
            .unwrap()
            .len(),
        1,
        "only the deliberate save belongs in the ring"
    );
}

/// The doze timeout runs off the app's own ticks. The panel still draws 400-700 mA while dozing,
/// and this board cannot wake itself from sleep, so the timeout is a real power off.
#[test]
fn a_doze_that_outlasts_the_timeout_powers_off_by_itself() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.set_power(panel(d.path(), Duration::from_secs(2)).0);
    a.apply(Action::LidClose);
    for _ in 0..100 {
        a.update(1.0 / 60.0);
    }
    assert!(!a.powering_off(), "1.6 s is short of the 2 s timeout");
    for _ in 0..40 {
        a.update(1.0 / 60.0);
    }
    assert!(a.powering_off());
}

#[test]
fn a_stray_doze_timeout_does_not_power_off_a_running_game() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.on_doze_timeout();
    assert!(!a.powering_off());
    assert!(matches!(a.phase(), Phase::Playing { .. }));
}

/// The panel comes up at the level the card remembers, not at whatever the kernel left it.
#[test]
fn the_backlight_follows_brightness_from_boot() {
    let d = tmp_root_with_carts(&["Emerald"]);
    write_slot_state(
        d.path(),
        &SlotState {
            brightness: 3,
            clock_set: true,
            utc_offset_min: 0,
            ..Default::default()
        },
    )
    .unwrap();
    let mut a = boot(d.path());
    let (power, step) = panel(d.path(), Duration::from_secs(60));
    a.set_power(power);
    assert_eq!(step.load(Ordering::Relaxed), 3);
    a.apply(Action::BrightnessUp);
    assert_eq!(step.load(Ordering::Relaxed), 4);
}

/// The menu covers the screen in the case's own materials rather than tinting the game.
#[test]
fn the_menu_covers_the_screen_in_the_case_materials() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::PowerHold);

    let mut out = Vec::new();
    a.draw(&mut out);

    match out.first() {
        Some(Draw::Rect { w, h, colour, .. }) => {
            assert_eq!(
                *colour,
                slot_ui::opening(),
                "the ground is the case's opening"
            );
            assert!(*w > 0.0 && *h > 0.0, "and it covers the panel");
        }
        other => panic!("the menu drew {other:?} rather than a ground"),
    }
    // Only the ground is testable: the plate is sized from uploaded row faces, which need a
    // compositor.
    assert_eq!(
        out.len(),
        1,
        "nothing of the previous phase survives the menu"
    );
}

/// rcK takes about five seconds to stop the frontend and unload the GPU module, and a black
/// panel for that long reads as hung, so the shutdown draws a screen over everything.
#[test]
fn a_power_off_draws_a_shutdown_screen_over_everything() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::PowerHold);
    a.apply(Action::GbaDown(Btn::Down));
    a.apply(Action::GbaDown(Btn::A));

    let mut out = Vec::new();
    a.draw(&mut out);

    match out.first() {
        Some(Draw::Rect { w, h, colour, .. }) => {
            assert_eq!(*colour, [0.0, 0.0, 0.0, 1.0], "the shutdown is black");
            assert!(*w > 0.0 && *h > 0.0, "and covers the panel");
        }
        other => panic!("the shutdown drew {other:?} rather than a panel of black"),
    }
    // One draw: the line itself is a texture the binary uploads at boot.
    assert_eq!(
        out.len(),
        1,
        "nothing of the previous phase survives the shutdown screen"
    );
}

/// A held POWER opens a menu and commits to nothing until A.
#[test]
fn a_hold_opens_the_menu_and_commits_nothing() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::PowerHold);
    assert_eq!(a.power_menu(), Some(0), "the menu opens on Restart");
    assert!(!a.powering_off() && !a.restarting());
    assert!(
        StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald")
            .read_resume()
            .unwrap()
            .is_some(),
        "durable before the menu is even read: the user may hold on to the PMIC's own cutoff"
    );
}

#[test]
fn the_menu_moves_and_stops_at_both_ends() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::PowerHold);
    a.apply(Action::GbaDown(Btn::Up));
    assert_eq!(a.power_menu(), Some(0), "it does not wrap off the top");
    for _ in 0..PowerChoice::ALL.len() + 1 {
        a.apply(Action::GbaDown(Btn::Down));
    }
    assert_eq!(
        a.power_menu(),
        Some(PowerChoice::ALL.len() - 1),
        "nor off the bottom"
    );
}

#[test]
fn b_leaves_the_menu_without_doing_anything() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::PowerHold);
    a.apply(Action::GbaDown(Btn::B));
    assert_eq!(a.power_menu(), None);
    assert!(!a.powering_off() && !a.restarting());
    assert!(
        matches!(a.phase(), Phase::Playing { .. }),
        "back to the game"
    );
}

#[test]
fn each_row_commits_to_its_own_outcome() {
    for (down, want) in [(0, "restart"), (1, "off")] {
        let d = tmp_root_with_carts(&["Emerald"]);
        let mut a = app_playing_in(d.path(), "Emerald");
        a.apply(Action::PowerHold);
        for _ in 0..down {
            a.apply(Action::GbaDown(Btn::Down));
        }
        a.apply(Action::GbaDown(Btn::A));
        assert_eq!(
            a.power_menu(),
            None,
            "{want}: the menu closes on the choice"
        );
        match want {
            "restart" => assert!(a.restarting() && !a.powering_off()),
            _ => assert!(a.powering_off() && !a.restarting()),
        }
    }
}

/// The binary may not power off until the ordinary loop has drawn the shutdown screen. Drawing
/// it out of band hung the device on a GPU that was being torn down.
#[test]
fn the_shutdown_screen_is_up_before_the_machine_may_stop() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::PowerHold);
    a.apply(Action::GbaDown(Btn::Down));
    a.apply(Action::GbaDown(Btn::A));

    assert!(a.powering_off(), "the choice decides immediately");
    assert!(
        !a.ready_to_power_off(),
        "but the binary may not act until the screen has been presented"
    );

    let mut out = Vec::new();
    a.draw(&mut out);
    assert!(
        matches!(out.first(), Some(Draw::Rect { colour, .. }) if *colour == [0.0, 0.0, 0.0, 1.0]),
        "and the screen is what the loop is drawing in the meantime"
    );

    // Absolute, not a delta: `tick_ms` takes the later of the two clocks.
    a.tick_ms(600_000);
    assert!(a.ready_to_power_off(), "then it may stop");
}

/// A lid closed past the doze timeout powers off by itself. `doze_expired` is a level and the
/// phase stays `Doze`, so re-arming the shutdown every frame would push `act_at` out forever.
#[test]
fn a_dozing_device_powers_off_by_itself_rather_than_waiting_for_the_lid() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.set_power(panel(d.path(), Duration::from_secs(2)).0);
    a.apply(Action::LidClose);
    for _ in 0..180 {
        a.update(1.0 / 60.0);
    }
    assert!(
        a.powering_off(),
        "three seconds is past the two second timeout"
    );
    assert!(
        a.ready_to_power_off(),
        "the shutdown screen has had its 250 ms and the machine is still not allowed to stop"
    );
}

/// The menu is an overlay, so the phase stays `Playing`; the core must still pause behind it.
#[test]
fn the_power_menu_holds_the_core_still() {
    let d = tmp_root_with_real_carts(&["Advance Wars", "Emerald"]);
    let (mut s, _motor) = session_with_platform(d.path());
    let mut now = 0;
    play(&mut s, &mut now);
    assert_eq!(
        s.observed_speed(),
        Some(Speed::Normal),
        "the core should be running, or this test proves nothing"
    );

    hold_power(&mut s, &mut now);
    assert_eq!(s.app().power_menu(), Some(0), "the menu never opened");
    await_paused(&mut s);
}

/// The same for the shutdown screen after the choice.
#[test]
fn a_committed_shutdown_holds_the_core_still() {
    let d = tmp_root_with_real_carts(&["Advance Wars", "Emerald"]);
    let (mut s, _motor) = session_with_platform(d.path());
    let mut now = 0;
    play(&mut s, &mut now);

    hold_power(&mut s, &mut now);
    s.app_mut().apply(Action::GbaDown(Btn::Down));
    s.app_mut().apply(Action::GbaDown(Btn::A));
    assert!(s.app().powering_off(), "Power Off is the second row");
    s.update(DT);
    await_paused(&mut s);
}

/// POWER pressed while dozing lights the panel on the press, before the hold raises the menu.
/// Otherwise the menu is drawn on a dark panel and the user holds on to the PMIC's six second
/// cutoff, which cuts the rails with no sync. Checked at `Platform`, where the panel is real.
#[test]
fn power_pressed_while_dozing_lights_the_panel_before_the_menu_is_raised() {
    let d = tmp_root_with_carts(&["Emerald"]);
    clocked(d.path());
    let mut s = Session::boot(d.path().to_path_buf());
    let (power, backlight) = panel(d.path(), Duration::from_secs(180));
    s.app_mut().set_power(power);
    let lit = backlight.load(Ordering::Relaxed);
    assert!(
        lit > 0,
        "the panel never came on, so going dark proves nothing"
    );

    let mut now = 0;
    event(&mut s, RawEvent::Down(Btn::Power), &mut now);
    event(&mut s, RawEvent::Up(Btn::Power), &mut now);
    assert!(
        matches!(s.app().phase(), Phase::Doze { .. }),
        "the device never dozed"
    );
    assert_eq!(
        backlight.load(Ordering::Relaxed),
        0,
        "the doze left the panel lit"
    );

    // POWER again: the panel lights on the press, not the release.
    event(&mut s, RawEvent::Down(Btn::Power), &mut now);
    assert_eq!(
        backlight.load(Ordering::Relaxed),
        lit,
        "the panel is still dark under the thumb trying to wake it"
    );
    assert!(
        !matches!(s.app().phase(), Phase::Doze { .. }),
        "the panel came on over a device still dozing behind it"
    );

    // Held on, it still raises the power menu.
    let pressed = now;
    while now < pressed + POWER_HOLD_MS + FRAME_MS {
        step(&mut s, &mut now);
    }
    assert_eq!(
        s.app().power_menu(),
        Some(0),
        "the hold no longer raises the menu"
    );
    assert_eq!(
        backlight.load(Ordering::Relaxed),
        lit,
        "the menu is up on a panel nobody can see"
    );
}

/// The release of the press that woke the panel does not doze again; the next tap still does.
#[test]
fn the_press_that_woke_the_panel_does_not_doze_again_when_it_is_let_go() {
    let d = tmp_root_with_carts(&["Emerald"]);
    clocked(d.path());
    let mut s = Session::boot(d.path().to_path_buf());
    let (power, backlight) = panel(d.path(), Duration::from_secs(180));
    s.app_mut().set_power(power);
    let lit = backlight.load(Ordering::Relaxed);

    let mut now = 0;
    event(&mut s, RawEvent::Down(Btn::Power), &mut now);
    event(&mut s, RawEvent::Up(Btn::Power), &mut now);
    assert!(matches!(s.app().phase(), Phase::Doze { .. }));

    event(&mut s, RawEvent::Down(Btn::Power), &mut now);
    event(&mut s, RawEvent::Up(Btn::Power), &mut now);
    assert!(
        !matches!(s.app().phase(), Phase::Doze { .. }),
        "the tap that woke the device put it straight back to sleep"
    );
    assert_eq!(backlight.load(Ordering::Relaxed), lit);

    event(&mut s, RawEvent::Down(Btn::Power), &mut now);
    event(&mut s, RawEvent::Up(Btn::Power), &mut now);
    assert!(
        matches!(s.app().phase(), Phase::Doze { .. }),
        "POWER stopped being able to put the device out"
    );
    assert_eq!(backlight.load(Ordering::Relaxed), 0);
}

/// Waits for the worker to report `Paused`: a descheduled worker and a stopped one have the same
/// frame count.
fn await_paused(s: &mut Session) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while s.observed_speed() != Some(Speed::Paused) {
        assert!(
            Instant::now() < deadline,
            "the core was still running behind the shutdown"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// POWER down, then frames until the gesture layer's own tick raises the menu.
fn hold_power(s: &mut Session, now: &mut Millis) {
    let pressed = *now;
    event(s, RawEvent::Down(Btn::Power), now);
    while *now < pressed + POWER_HOLD_MS + FRAME_MS {
        step(s, now);
    }
}

fn step(s: &mut Session, now: &mut Millis) {
    *now += FRAME_MS;
    s.feed([], *now);
    s.update(DT);
}

fn event(s: &mut Session, ev: RawEvent, now: &mut Millis) {
    *now += FRAME_MS;
    s.feed([ev], *now);
    s.update(DT);
}

/// Puts the selected cart in and waits out the load, which happens on its own thread.
fn play(s: &mut Session, now: &mut Millis) {
    event(s, RawEvent::Down(Btn::A), now);
    event(s, RawEvent::Up(Btn::A), now);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        step(s, now);
        std::thread::sleep(Duration::from_millis(1));
    }
    // The worker must have run at Normal before a test may claim it stopped.
    let deadline = Instant::now() + Duration::from_secs(5);
    while s.observed_speed() != Some(Speed::Normal) {
        assert!(Instant::now() < deadline, "the core never started");
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// What the radio was asked to do, in order.
#[derive(Clone, Default)]
struct RadioLog(std::sync::Arc<std::sync::Mutex<Vec<RadioJob>>>);

impl RadioLog {
    fn jobs(&self) -> Vec<RadioJob> {
        self.0.lock().expect("radio log").clone()
    }
}

impl RadioJobs for RadioLog {
    fn ask(&mut self, job: RadioJob) {
        self.0.lock().expect("radio log").push(job);
    }

    fn warmed(&self) -> bool {
        false
    }
}

/// A doze ends at a power off, so a shut lid must not leave the radio driver loaded.
#[test]
fn a_shut_lid_cools_the_radio() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    let log = RadioLog::default();
    a.set_radio_jobs(Box::new(log.clone()));
    a.apply(Action::LidClose);
    assert!(
        log.jobs().contains(&RadioJob::Cool),
        "the lid closed with the radio left loaded behind it"
    );
}

/// A lid shut over a live session drops the session's network before cooling the driver.
#[test]
fn a_shut_lid_over_a_session_takes_its_network_down_too() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    let log = RadioLog::default();
    a.set_radio_jobs(Box::new(log.clone()));
    a.begin_link(0);
    a.apply(Action::LidClose);
    let jobs = log.jobs();
    assert_eq!(
        jobs.first(),
        Some(&RadioJob::Down),
        "the session's own network must come down before the driver does: {jobs:?}"
    );
    assert!(jobs.contains(&RadioJob::Cool));
}
