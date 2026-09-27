//! Which platform's folders a seated cart's files land in, through the real `Session`.
//!
//! `Platform::default()` is `Gba`, so only a Game Boy cart can tell a session that stores its
//! platform from one that assumes it. A wrong platform writes one game's battery save over
//! another's.

mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use slot::app::Phase;
use slot::session::Session;
use slot_input::{Btn, RawEvent};
use slot_store::{core_for_platform, Core, Platform, StateRing};

/// The core the session opens a Game Boy cart on, so these stay about the platform folder.
fn seated_core(root: &Path, stem: &str) -> Core {
    core_for_platform(root, stem, Platform::Gb)
}

/// Seats the cart under the shelf and runs past the autosave deadline.
fn seat_and_autosave(root: &Path) {
    common::clocked(root);
    let mut s = Session::boot(root.to_path_buf());
    s.feed([RawEvent::Down(Btn::A)], 16);
    s.feed([RawEvent::Up(Btn::A)], 32);

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut now = 32;
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        now += 16;
        s.feed([], now);
        s.update(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(1));
    }

    s.app_mut().tick_ms(60_000);
}

/// `session.rs` hands the cart's `Platform` to `App`; without it a Game Boy cart's state lands in
/// the GBA folder. Two carts, because a single cart boots straight past the shelf.
#[test]
fn a_game_boy_carts_autosave_lands_under_gb_and_never_under_gba() {
    let d = common::tmp_root_with_gb_carts(&["Tetris", "Zelda"]);

    seat_and_autosave(d.path());

    assert!(
        StateRing::new(
            d.path(),
            Platform::Gb,
            seated_core(d.path(), "Tetris"),
            "Tetris"
        )
        .read_resume()
        .unwrap()
        .is_some(),
        "the Game Boy cart's autosave did not land under its own platform's directory"
    );
    assert!(
        StateRing::new(
            d.path(),
            Platform::Gba,
            seated_core(d.path(), "Tetris"),
            "Tetris"
        )
        .read_resume()
        .unwrap()
        .is_none(),
        "the Game Boy cart's autosave was filed as a GBA cart's, which is where a GBA game \
         of the same name keeps its own"
    );

    // The battery save collides too: a GBA and a Game Boy cart of the same name would share a path.
    assert!(
        d.path().join("Saves/GB/Tetris.sav").is_file(),
        "the Game Boy cart's battery save did not land under its own platform's directory"
    );
    assert!(
        !d.path().join("Saves/GBA/Tetris.sav").exists(),
        "the Game Boy cart's battery save was written where a GBA game of the same name \
         keeps its own"
    );
}

/// A hand-organised Game Boy card is scanned, seated, saved under `GB/`, and resumed on the next
/// boot. `e2e.rs` covers GBA, which cannot tell a kept platform from the default.
#[test]
fn a_hand_organised_game_boy_card_scans_seats_saves_and_resumes() {
    let d = common::tmp_root_with_gb_carts(&["Tetris", "Zelda"]);

    seat_and_autosave(d.path());

    let resume = StateRing::new(
        d.path(),
        Platform::Gb,
        seated_core(d.path(), "Tetris"),
        "Tetris",
    )
    .read_resume()
    .unwrap();
    assert!(resume.is_some(), "nothing was written back to resume from");

    let again = slot::app::App::boot(d.path());
    let cart = again
        .seated_cart()
        .expect("the next boot came up with an empty slot");
    assert_eq!(
        cart.platform,
        Platform::Gb,
        "the card came back holding a cart for the wrong machine"
    );
    assert_eq!(cart.stem, "Tetris");
    assert!(
        cart.rom.ends_with("Games/GB/Tetris.gb"),
        "the rom the slot is holding is {:?}, which is not the one in the Game Boy folder",
        cart.rom
    );
}
