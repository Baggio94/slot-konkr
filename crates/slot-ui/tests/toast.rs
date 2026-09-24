use slot_ui::{toast_face, toast_rect, Draw, Hud, HudKind, Toast, OUT_W, PLATE_H};

#[test]
fn saving_and_loading_say_which_one_happened() {
    assert_eq!(Toast::StateSaved.text(), "State Saved");
    assert_eq!(Toast::StateLoaded.text(), "State Loaded");
}

/// The link shortcut on a core that cannot link says which one can, in the same banner.
#[test]
fn the_link_shortcut_on_the_wrong_core_says_to_switch() {
    assert_eq!(Toast::NeedsGpsp.text(), "Please switch to gpSP");
    let f = toast_face(Toast::NeedsGpsp);
    assert!(f.rgba.chunks(4).any(|p| p[3] > 0), "the banner is blank");
}

/// gpSP only fakes named protocols, so on any other cart the shortcut says there is no link.
#[test]
fn a_cart_gpsp_cannot_link_says_there_is_no_link() {
    assert_eq!(Toast::NoLink.text(), "No link support");
    let f = toast_face(Toast::NoLink);
    assert!(f.rgba.chunks(4).any(|p| p[3] > 0), "the banner is blank");
}

/// Every banner answers something the user just did, never what is already on screen. Held
/// by name. The last two are TEMPORARY, with `Action::ColourCorrectionToggle`.
#[test]
fn the_banner_says_what_happened_and_never_what_is_on_screen() {
    assert_eq!(
        Toast::ALL,
        [
            Toast::StateSaved,
            Toast::StateLoaded,
            Toast::NeedsGpsp,
            Toast::NoLink,
            Toast::LinkEnded,
            Toast::PeerEnded,
            Toast::ColourOn,
            Toast::ColourOff,
        ],
        "a banner was added or dropped: every face is uploaded by its place in this list"
    );
    for (i, t) in Toast::ALL.iter().enumerate() {
        assert_eq!(t.index(), i, "{t:?} does not answer to its own place");
        let f = toast_face(*t);
        assert!(
            f.rgba.chunks(4).any(|p| p[3] > 0),
            "{t:?} rastered to a blank banner"
        );
    }
}

/// Every banner's ink covers the same rows, to within one (round capitals overshoot flat ones;
/// a shrunk line is four rows out).
#[test]
fn no_toast_is_shrunk_to_fit_its_box() {
    let rows = |t: Toast| {
        let f = toast_face(t);
        let inked: Vec<usize> = (0..f.h as usize)
            .filter(|y| (0..f.w as usize).any(|x| f.rgba[(y * f.w as usize + x) * 4 + 3] > 0))
            .collect();
        let first = *inked.first().expect("the banner is blank");
        let last = *inked.last().expect("the banner is blank");
        (first, last)
    };
    let (top, bottom) = rows(Toast::StateSaved);
    for t in Toast::ALL {
        let (a, b) = rows(t);
        assert!(
            a.abs_diff(top) <= 1 && b.abs_diff(bottom) <= 1,
            "{t:?} sits on rows {a}..{b} where the others sit on {top}..{bottom}, so it was shrunk to fit"
        );
    }
}

#[test]
fn a_toast_fades_on_the_same_curve_as_the_bar() {
    let mut h = Hud::new();
    h.toast(Toast::StateSaved, 1_000);
    assert!(h.toast_visible(2_499));
    assert!(!h.toast_visible(2_500));
}

/// A repeated toast re-stamps the one clock rather than queueing a second banner.
#[test]
fn saying_the_same_thing_twice_re_shows_it_rather_than_stacking() {
    let mut h = Hud::new();
    h.toast(Toast::StateSaved, 1_000);
    h.toast(Toast::StateSaved, 2_400);
    // Past the first stamp's fade and short of the second's.
    assert_eq!(h.said(3_400), Some(Toast::StateSaved), "it did not re-show");
    assert_eq!(h.said(3_900), None, "it never faded");
}

/// A different banner replaces the one showing rather than waiting behind it.
#[test]
fn a_second_banner_replaces_the_first() {
    let mut h = Hud::new();
    h.toast(Toast::StateSaved, 1_000);
    h.toast(Toast::LinkEnded, 1_100);
    assert_eq!(h.said(1_200), Some(Toast::LinkEnded));
}

#[test]
fn a_toast_is_centred() {
    let (x, _, w, _) = toast_rect();
    assert_eq!(x + w / 2.0, OUT_W as f32 / 2.0);
}

/// The type carries its own halo, like the badge, or it vanishes on a white frame.
#[test]
fn a_toast_carries_its_own_halo() {
    let f = toast_face(Toast::StateSaved);
    let dark = f
        .rgba
        .chunks(4)
        .any(|p| p[3] > 0 && p[0] < 0x40 && p[1] < 0x40 && p[2] < 0x40);
    assert!(dark, "there is nothing dark behind the type");
}

/// The toast reads against the same plate as the level bar, in the same place.
#[test]
fn a_toast_sits_in_the_plate_band_and_is_backed_by_it() {
    let mut h = Hud::new();
    h.toast(Toast::StateSaved, 0);
    let mut out = Vec::new();
    h.draw(0, &mut out);

    let plate = out
        .iter()
        .find(|d| matches!(d, Draw::Rect { w, .. } if *w == OUT_W as f32))
        .expect("the toast has nothing to be read against");
    let Draw::Rect { colour, h: ph, .. } = plate else {
        unreachable!()
    };
    assert!(colour[3] > 0.6, "the plate is too faint to give contrast");
    assert!((*ph - PLATE_H).abs() < 0.01, "the plate is not the band");

    let (_, y, _, th) = toast_rect();
    assert!(
        y >= 0.0 && y + th <= PLATE_H,
        "the toast at {y} is outside the band"
    );
}

/// They share one strip, and a toast outranks a level bar.
#[test]
fn a_toast_takes_the_band_from_the_bar() {
    let mut h = Hud::new();
    h.show(HudKind::Volume, 50, false, 0);
    let mut bar_only = Vec::new();
    h.draw(0, &mut bar_only);
    let bars = bar_only.len();

    h.toast(Toast::StateSaved, 0);
    let mut both = Vec::new();
    h.draw(0, &mut both);
    assert!(
        both.len() < bars,
        "the bar is still drawn underneath the toast"
    );
}
