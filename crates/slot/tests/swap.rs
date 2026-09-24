mod common;

use std::time::{Duration, Instant};

use slot::app::Phase;
use slot::session::Session;
use slot_input::{Btn, RawEvent};

const FRAME_MS: u64 = 16;

struct Pass {
    session: Session,
    now: u64,
}

impl Pass {
    fn step(&mut self) {
        self.now += FRAME_MS;
        self.session.feed([], self.now);
        self.session.update(0.016);
    }

    fn tap(&mut self, b: Btn) {
        for ev in [RawEvent::Down(b), RawEvent::Up(b)] {
            self.now += FRAME_MS;
            self.session.feed([ev], self.now);
            self.session.update(0.016);
        }
    }

    fn until(&mut self, what: &str, cond: impl Fn(&Session) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !cond(&self.session) {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            self.step();
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

/// The compositor keeps the last game texture, so gating the game layer on "a core exists" shows
/// the previous cart's final frame through the insert.
#[test]
fn the_game_layer_stays_hidden_until_the_new_core_has_published() {
    let d = common::tmp_root_with_real_carts(&["Emerald", "Fusion"]);
    common::clocked(d.path());
    let mut p = Pass {
        session: Session::boot(d.path().to_path_buf()),
        now: 0,
    };
    assert!(!p.session.game_visible(), "the slot is empty at boot");

    p.tap(Btn::A);
    assert!(
        !p.session.game_visible(),
        "the game layer must not draw on the frame the core was spawned"
    );
    p.until("the first frame", |s| s.game_visible());
    // MENU held before the cart has seated is not an eject, so wait for the phase.
    p.until("the cart to seat", |s| {
        matches!(s.app().phase(), Phase::Playing { .. })
    });

    // Eject, then seat the other cart.
    p.session
        .feed([RawEvent::Down(Btn::Menu)], p.now + FRAME_MS);
    p.until("the eject", |s| !s.has_core());
    p.session.feed([RawEvent::Up(Btn::Menu)], p.now + FRAME_MS);
    assert!(
        !p.session.game_visible(),
        "an empty slot must not keep drawing the cart that just left"
    );

    p.until("the shelf", |s| !s.has_core());
    p.tap(Btn::Right);
    p.tap(Btn::A);
    assert!(
        !p.session.game_visible(),
        "the second cart must not show the first cart's last frame"
    );
    p.until("the second core", |s| s.game_visible());
}

/// `Frames::latest` consumes, so anything but the renderer calling it steals the newest frame
/// and the picture lags while audio stays perfect.
///
/// Paused, because a running core republishes within a frame and hides the theft.
#[test]
fn nothing_but_the_renderer_takes_a_frame() {
    let d = common::tmp_root_with_real_carts(&["Emerald"]);
    common::clocked(d.path());
    let mut p = Pass {
        session: Session::boot(d.path().to_path_buf()),
        now: 0,
    };
    p.tap(Btn::A);
    p.until("the first frame", |s| s.game_visible());

    // Closing the lid dozes and pauses the core, so a missing frame cannot be replaced.
    // Down only. A tap would close the lid and reopen it in the same breath.
    p.now += FRAME_MS;
    p.session.feed([RawEvent::Down(Btn::Lid)], p.now);
    p.session.update(0.016);
    p.until("a frame waiting on a paused core", |s| s.frame_ready());
    let before = p.session.frames_taken();

    for _ in 0..30 {
        p.now += FRAME_MS;
        p.session.feed([], p.now);
        p.session.update(0.016);
    }

    assert_eq!(
        p.session.frames_taken(),
        before,
        "update took {} frame(s) the renderer never saw",
        p.session.frames_taken() - before
    );
    assert!(
        p.session.frame_ready(),
        "the waiting frame was consumed by something other than the renderer"
    );
}

/// A core must not run behind the insert animation, or the GBA bios intro plays unseen and the
/// reveal catches only its tail.
#[test]
fn the_core_does_not_run_while_the_cart_is_going_in() {
    let d = common::tmp_root_with_real_carts(&["Emerald"]);
    let mut p = Pass {
        session: Session::boot(d.path().to_path_buf()),
        now: 0,
    };
    p.tap(Btn::A);
    p.until("the core to load", |s| s.has_core());

    // Well over a frame's worth of wall clock while the cart is still travelling.
    let mut ran = 0;
    for _ in 0..40 {
        if !matches!(p.session.app().phase(), Phase::Inserting { .. }) {
            break;
        }
        let before = p.session.frames_published();
        p.step();
        std::thread::sleep(Duration::from_millis(4));
        // The step itself can end the insert. Only frames produced while the cart travels at both
        // ends of the step count.
        let still_inserting = matches!(p.session.app().phase(), Phase::Inserting { .. });
        if still_inserting && p.session.frames_published() > before {
            ran += 1;
        }
    }
    assert_eq!(
        ran, 0,
        "the core produced {ran} frames while the cart was still going in"
    );
}
