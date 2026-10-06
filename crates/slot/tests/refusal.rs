mod common;

use common::{app_playing_in, boot, tmp_root_with_carts};
use slot::app::{App, Phase};
use slot_input::Action;
use slot_ui::{Draw, CART_H, CART_W};

#[test]
fn double_tap_menu_with_no_states_shakes_instead_of_doing_nothing() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::Polaroids);
    assert!(
        matches!(a.phase(), Phase::Playing { .. }),
        "it must not open"
    );
    assert!(a.refusal_active(a.now()), "nothing told the player why");
}

#[test]
fn loading_with_no_states_shakes_as_well() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::LoadState);
    assert!(a.refusal_active(a.now()), "nothing told the player why");
}

/// The whole device flinches: a new rectangle wobbling reads as a widget, not a refusal.
#[test]
fn no_plate_is_drawn_for_a_refusal() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::Polaroids);
    let mut out = Vec::new();
    a.draw(&mut out);
    assert!(a.refusal_active(a.now()), "nothing was refused");
    assert_eq!(
        out.len(),
        draws_without_refusal(&d),
        "a refusal added something to the screen"
    );
}

#[test]
fn the_screen_shake_decays_and_stops() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::Polaroids);
    let early = peak(&a, 0, 100);
    let late = peak(&a, 200, 300);
    assert!(early > 0.0, "nothing moved");
    assert!(
        late < early,
        "the shake is not decaying: {early} then {late}"
    );
    assert_eq!(a.shake_at(a.now() + 400), 0.0, "the shake never ends");
}

/// The pair, stated together, so the two halves of the rule cannot drift apart.
#[test]
fn an_empty_ring_shakes_the_screen_and_not_a_cart() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::Polaroids);
    assert!(
        matches!(a.phase(), Phase::Playing { .. }),
        "the switcher opened with nothing in it"
    );
    assert_ne!(a.screen_shake(), 0.0, "the screen did not flinch");
}

/// A small object jittering beside a slot reads as a rendering fault. The alert says it, so the
/// cart stands still.
#[test]
fn a_refused_cart_shows_an_alert_and_does_not_jitter() {
    // Three carts: one boots seated, two are centred as a pair so the cart slides sideways. On a
    // row of three the selection stays in the middle and only jitter could move its x.
    let d = tmp_root_with_carts(&["Broken", "Fusion", "Zzz"]);
    let mut a = boot(d.path());
    a.apply(Action::Insert);
    // Partway in: a core failing before the cart moved has nothing to hand back.
    for _ in 0..30 {
        a.update(1.0 / 60.0);
    }
    let straight = cart_x(&a);
    a.on_core_failed();
    a.update(1.0 / 60.0);
    assert_eq!(cart_x(&a), straight, "the cart is still jittering");
    assert!(
        a.alert_visible(),
        "nothing tells the player the cart was refused"
    );
    assert_eq!(
        a.screen_shake(),
        0.0,
        "the screen shook as well as the cart"
    );
}

/// It leaves with the cart. An alert still lit on the shelf reads as something to dismiss.
#[test]
fn the_alert_is_gone_before_the_cart_is() {
    let d = tmp_root_with_carts(&["Broken"]);
    let mut a = boot(d.path());
    a.on_core_failed();
    assert!(a.alert_visible(), "the alert never appeared");
    let mut last = 1.0;
    while matches!(a.phase(), Phase::Ejecting { .. }) {
        last = a.alert_alpha();
        a.update(1.0 / 60.0);
    }
    assert_eq!(last, 0.0, "the alert was still lit as the cart landed");
}

/// In game shaking the frame is right. On the shelf it is mostly backdrop, so shaking it just
/// shows the letterbox.
#[test]
fn a_shelf_refusal_shakes_the_carts_not_the_screen() {
    let d = tmp_root_with_carts(&["Emerald", "Zzz"]);
    let mut a = boot(d.path());
    a.refuse();
    assert_eq!(a.screen_shake(), 0.0, "the shelf shook the whole screen");
    assert_ne!(a.shelf_shake(), 0.0, "nothing moved at all");
}

/// Peak displacement over a window, so the result does not depend on the cycle phase.
fn peak(a: &App, from: u64, to: u64) -> f32 {
    (from..to)
        .map(|t| a.shake_at(a.now() + t).abs())
        .fold(0.0, f32::max)
}

/// The same app in the same phase with nothing refused, as the baseline draw list.
fn draws_without_refusal(d: &tempfile::TempDir) -> usize {
    let a = app_playing_in(d.path(), "Emerald");
    let mut out = Vec::new();
    a.draw(&mut out);
    out.len()
}

fn drawn(a: &App) -> Vec<Draw> {
    let mut out = Vec::new();
    a.draw(&mut out);
    out
}

fn cart_x(a: &App) -> f32 {
    let shaped = |w: f32, h: f32| w > 200.0 && (w / h - CART_W as f32 / CART_H as f32).abs() < 0.05;
    drawn(a)
        .iter()
        .find_map(|d| match *d {
            Draw::Rect { x, w, h, .. } | Draw::Tex { x, w, h, .. } if shaped(w, h) => {
                Some(x + w / 2.0)
            }
            _ => None,
        })
        .expect("no cart in the list")
}
