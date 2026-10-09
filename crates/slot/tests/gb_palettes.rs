mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use common::{repo_root, session_with_platform, tmp_root_with_carts, tmp_root_with_gb_carts};
use slot::app::Phase;
use slot::session::Session;
use slot_input::{Action, Btn, Millis, RawEvent};
use slot_store::{read_slot_state, write_slot_state, GbPalette, SlotState};
use slot_ui::Toast;
use tempfile::TempDir;

const FRAME_MS: Millis = 16;
const DT: f32 = 1.0 / 60.0;

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

fn play(s: &mut Session, now: &mut Millis) {
    event(s, RawEvent::Down(Btn::A), now);
    event(s, RawEvent::Up(Btn::A), now);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        step(s, now);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn catrap_with_colour_flag(cgb: u8, palettes: bool) -> Option<TempDir> {
    let real = repo_root().join("sdcard/Games/GB/Catrap (USA).gb");
    let mut rom = std::fs::read(&real).ok()?;
    rom[0x143] = cgb;
    let d = tmp_root_with_gb_carts(&["Catrap"]);
    std::fs::write(d.path().join("Games/GB/Catrap.gb"), rom).expect("write rom");
    palettes_on(d.path(), palettes);
    Some(d)
}

fn palettes_on(root: &Path, on: bool) {
    write_slot_state(
        root,
        &SlotState {
            gb_palettes: on,
            ..SlotState::default()
        },
    )
    .expect("write slot.state");
}

#[test]
fn select_x_steps_the_palette_and_names_it_in_a_game_boy_only_game() {
    let Some(d) = catrap_with_colour_flag(0x00, true) else {
        eprintln!("no Catrap on this machine's card, skipping");
        return;
    };
    let (mut s, _motor) = session_with_platform(d.path());
    let mut now = 0;
    play(&mut s, &mut now);
    assert!(
        s.app().palette_live(),
        "palettes are on and the cart is Game Boy only"
    );

    let before = s.app().gb_palette().expect("palettes are on");
    s.app_mut().apply(Action::PaletteNext);
    let after = s.app().gb_palette().expect("palettes are on");
    assert_eq!(after, before.next());
    assert_eq!(s.app().toast(), Some(Toast::Palette(after)));
    assert_eq!(
        read_slot_state(d.path()).gb_palette,
        after,
        "the pick was not saved"
    );
}

#[test]
fn select_x_does_nothing_with_palettes_off_or_in_a_colour_game() {
    for (cgb, on) in [(0x00, false), (0x80, true)] {
        let Some(d) = catrap_with_colour_flag(cgb, on) else {
            eprintln!("no Catrap on this machine's card, skipping");
            return;
        };
        let (mut s, _motor) = session_with_platform(d.path());
        let mut now = 0;
        play(&mut s, &mut now);
        assert!(!s.app().palette_live(), "cgb={cgb:#x} on={on}");
        let before = read_slot_state(d.path()).gb_palette;
        s.app_mut().apply(Action::PaletteNext);
        assert_eq!(
            s.app().toast(),
            None,
            "cgb={cgb:#x} on={on}: a palette toast showed"
        );
        assert_eq!(read_slot_state(d.path()).gb_palette, before);
        assert_eq!(GbPalette::DEFAULT, before);
    }
}

#[test]
fn select_x_on_the_carousel_does_nothing() {
    let d = tmp_root_with_carts(&["Emerald"]);
    palettes_on(d.path(), true);
    let (mut s, _motor) = session_with_platform(d.path());
    s.app_mut().apply(Action::PaletteNext);
    assert_eq!(s.app().toast(), None);
    assert_eq!(read_slot_state(d.path()).gb_palette, GbPalette::DEFAULT);
}

fn mean_saturation(frame: &[u8]) -> f64 {
    let sum: f64 = frame
        .chunks_exact(4)
        .map(|p| {
            let (hi, lo) = (p[..3].iter().max(), p[..3].iter().min());
            f64::from(hi.copied().unwrap_or(0) - lo.copied().unwrap_or(0))
        })
        .sum();
    sum / (frame.len() / 4) as f64
}

fn until(s: &mut Session, now: &mut Millis, what: &str, cond: impl Fn(&[u8]) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if s.frame().is_some_and(|f| cond(&f)) {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        step(s, now);
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn the_chord_recolours_the_running_game() {
    let Some(dylib) = common::vendored_core() else {
        eprintln!("no mgba dylib, skipping");
        return;
    };
    std::env::set_var("SLOT_CORE", dylib);
    let _g = common::core_lock();
    let Some(d) = catrap_with_colour_flag(0x00, true) else {
        eprintln!("no Catrap on this machine's card, skipping");
        return;
    };
    write_slot_state(
        d.path(),
        &SlotState {
            gb_palettes: true,
            gb_palette: GbPalette::parse("SGB 4-H").unwrap(),
            ..SlotState::default()
        },
    )
    .expect("write slot.state");
    let (mut s, _motor) = session_with_platform(d.path());
    let mut now = 0;
    play(&mut s, &mut now);
    until(
        &mut s,
        &mut now,
        "a title screen in SGB 4-H's colours",
        |f| mean_saturation(f) > 6.0,
    );

    for ev in [
        RawEvent::Down(Btn::Select),
        RawEvent::Down(Btn::X),
        RawEvent::Up(Btn::X),
        RawEvent::Up(Btn::Select),
    ] {
        event(&mut s, ev, &mut now);
    }
    assert_eq!(
        s.app().gb_palette().map(GbPalette::core_name),
        Some("Grayscale")
    );
    until(&mut s, &mut now, "the picture to turn to Grayscale", |f| {
        mean_saturation(f) < 4.0
    });
}
