//! The two shelves through the real frontend, composited on the GPU and read back as pixels.
//!
//! `SCRATCH_PNG_DIR=/tmp cargo test -p slot --test render_shelves -- --nocapture`

#![cfg(target_os = "macos")]

mod common;

use std::collections::VecDeque;
use std::path::Path;

use common::{clocked, repo_root, tmp_root_with_carts};
use slot::app::{App, SEATED_AT};
use slot::frontend::Frontend;
use slot_gfx::{Compositor, HeadlessSurface, OUT_H, OUT_W};
use slot_input::{Action, Btn, InputSource, Millis, RawEvent};
use slot_power::SimPlatform;
// `slot_power::Platform` is the device; this one is the console a cart is for.
use slot_store::{Cart, Platform as CartPlatform};
use slot_ui::{
    cart_box, cart_face, clean_label, edge, housing, label_colour, label_text, opening, recess,
    rest_y, Draw, SlotChrome, CART_W, GB_CART_H, GB_LABEL_H, GB_LABEL_Y, LABEL_H, LABEL_Y, PLATE_H,
};

/// One batch of events per poll, and nothing once they run out.
struct Script(VecDeque<Vec<RawEvent>>);

impl InputSource for Script {
    fn poll(&mut self, _now: Millis) -> Vec<RawEvent> {
        self.0.pop_front().unwrap_or_default()
    }
}

/// A tap: down on one frame, up on the next.
fn tap(f: &mut Frontend, input: &mut Script, btn: Btn) {
    input.0.push_back(vec![RawEvent::Down(btn)]);
    f.advance(input);
    input.0.push_back(vec![RawEvent::Up(btn)]);
    f.advance(input);
}

fn at(px: &[u8], x: usize, y: usize) -> [u8; 3] {
    let o = (y * OUT_W as usize + x) * 4;
    [px[o], px[o + 1], px[o + 2]]
}

/// The average colour of a patch. Type across a label makes any single pixel unreliable.
fn patch(px: &[u8], x: usize, y: usize) -> [u32; 3] {
    let mut sum = [0u32; 3];
    let mut n = 0;
    for py in y - 6..y + 6 {
        for qx in x - 20..x + 20 {
            let c = at(px, qx, py);
            for (k, v) in c.iter().enumerate() {
                sum[k] += *v as u32;
            }
            n += 1;
        }
    }
    [sum[0] / n, sum[1] / n, sum[2] / n]
}

/// How far apart two readings are, summed over the channels.
fn apart(a: [u32; 3], b: [u32; 3]) -> u32 {
    (0..3).map(|k| a[k].abs_diff(b[k])).sum()
}

/// Lit pixels across the middle of the top plate, to catch a shelf-name banner coming back.
/// The span stops short of the corner the mark is in.
fn banner_ink(px: &[u8]) -> usize {
    (0..PLATE_H as usize)
        .flat_map(|y| (200..520).map(move |x| (x, y)))
        .filter(|(x, y)| at(px, *x, *y).iter().all(|c| *c > 0x80))
        .count()
}

/// Lit pixels across the bottom plate's HUD, which is drawn whatever the shelf holds, so it
/// says a frame was composed at all.
fn hud_ink(px: &[u8]) -> usize {
    ((OUT_H as usize - 40)..OUT_H as usize)
        .flat_map(|y| (0..OUT_W as usize).map(move |x| (x, y)))
        .filter(|(x, y)| at(px, *x, *y).iter().all(|c| *c > 0x80))
        .count()
}

/// Inside the empty slot's dark opening, where the shelf's machine is printed when the shelf
/// changes. Narrow enough to stay clear of the scoop's lit rim, which would read as ink.
fn name_window() -> (usize, usize, usize, usize) {
    (OUT_W as usize / 2 - 70, OUT_H as usize - 49, 140, 27)
}

/// Every pixel of the band's name. Two shelves' bands compare equal only if they print the same
/// word, which a count of lit pixels could not tell.
fn name_pixels(px: &[u8]) -> Vec<[u8; 3]> {
    let (x0, y0, w, h) = name_window();
    (y0..y0 + h)
        .flat_map(|y| (x0..x0 + w).map(move |x| (x, y)))
        .map(|(x, y)| at(px, x, y))
        .collect()
}

/// How much of the opening is lit above its own near-black, which is the name's type.
fn name_ink(px: &[u8]) -> usize {
    name_pixels(px)
        .iter()
        .filter(|c| u32::from(c[0]) > GROUND[0] + 0x10)
        .count()
}

/// One frame to rasterise the name's face, then past its fade-in.
fn let_the_name_in(f: &mut Frontend, c: &mut Compositor, input: &mut Script) {
    f.compose(c);
    f.advance(input);
    std::thread::sleep(std::time::Duration::from_millis(300));
    f.advance(input);
}

fn composed(f: &mut Frontend, c: &mut Compositor, name: &str) -> Vec<u8> {
    f.compose(c);
    let px = c.read_frame();
    if let Ok(dir) = std::env::var("SCRATCH_PNG_DIR") {
        let path = format!("{dir}/shelves-{name}.png");
        let file = std::fs::File::create(&path).expect("create png");
        let mut e = png::Encoder::new(std::io::BufWriter::new(file), OUT_W, OUT_H);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header()
            .expect("png header")
            .write_image_data(&px)
            .expect("png data");
        println!("wrote {path}");
    }
    px
}

/// The card's own Game Boy carts, copied read only into the test root. A fresh clone has no
/// `sdcard/`, so a stand-in cart is used.
fn put_game_boy_carts(root: &Path) {
    for (dir, stem, ext, cgb) in [
        ("GB", "Tetris Rosy Retrospection", "gb", 0x00u8),
        ("GBC", "Tetris Chromatic", "gbc", 0xc0),
    ] {
        let from = repo_root().join(format!("sdcard/Games/{dir}/{stem}.{ext}"));
        let to = root.join(format!("Games/{dir}/{stem}.{ext}"));
        match std::fs::read(&from) {
            Ok(rom) => std::fs::write(&to, rom).expect("copy the card's cart"),
            Err(_) => std::fs::write(&to, gb_rom(cgb)).expect("write a stand-in cart"),
        }
    }
}

/// 32 KiB with a Game Boy header. The CGB flag at 0x143 is the only byte the shelf reads; the
/// label name comes from the filename.
fn gb_rom(cgb: u8) -> Vec<u8> {
    let mut rom = vec![0u8; 0x8000];
    rom[0x143] = cgb;
    rom
}

/// A point `down` pixels down the face of the lone pak on a Game Boy shelf. Readings go through
/// here rather than a fixed screen row, which goes wrong the moment the cartridge moves.
fn on_the_lone_pak(down: u32) -> (usize, usize) {
    (
        (OUT_W / 2) as usize,
        (rest_y(GB_CART_H as f32) + down as f32) as usize,
    )
}

/// The pak's bare plastic, half way down the lettering plate above its label.
fn alone() -> (usize, usize) {
    on_the_lone_pak(GB_LABEL_Y / 2)
}

/// The middle of that pak's label well. Neither reading alone tells a Game Boy shelf from a
/// Colour one (plastic 55 apart, paper 59); together they are twice outside the tolerance.
fn alone_label() -> (usize, usize) {
    on_the_lone_pak(GB_LABEL_Y + GB_LABEL_H / 2)
}

/// The two side slots of the carousel, at the same screen row so both read the same part of
/// a side cart. A shelf of one leaves both bare.
const SIDE_LEFT: (usize, usize) = (120, 250);
const SIDE_RIGHT: (usize, usize) = (600, 250);
/// The middle slot, on the selection itself.
const MIDDLE: (usize, usize) = (360, 250);
/// The ground the carts stand on, which an empty place on the row leaves behind.
const GROUND: [u32; 3] = [0x05, 0x05, 0x08];

/// The shoulders ring over one shelf per platform, and the plate's corner shows a different
/// machine on each. On a two-cart shelf the other cart repeats on both sides of the selection.
#[test]
fn the_shoulders_ring_over_a_shelf_for_each_system() {
    let Ok(surface) = HeadlessSurface::new() else {
        return;
    };
    let Ok(mut c) = Compositor::new(&surface) else {
        return;
    };
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    put_game_boy_carts(d.path());
    clocked(d.path());
    let mut f = Frontend::boot(Box::new(SimPlatform::at(d.path().to_path_buf())));
    f.upload_faces(&mut c);
    let mut input = Script(VecDeque::new());
    f.advance(&mut input);
    let_the_name_in(&mut f, &mut c, &mut input);

    let gba = composed(&mut f, &mut c, "gba");
    assert_eq!(
        banner_ink(&gba),
        0,
        "the carousel named a system nobody had switched to"
    );
    assert!(
        name_ink(&gba) > 20,
        "the plate corner came up with no mark in it at all: {} lit pixels",
        name_ink(&gba)
    );
    // Every slot holds a cart, the two side slots hold the same one, and it is not the selection.
    let left = patch(&gba, SIDE_LEFT.0, SIDE_LEFT.1);
    let right = patch(&gba, SIDE_RIGHT.0, SIDE_RIGHT.1);
    let middle = patch(&gba, MIDDLE.0, MIDDLE.1);
    for (name, slot) in [("left", left), ("middle", middle), ("right", right)] {
        assert!(
            apart(slot, GROUND) > 60,
            "the {name} slot of a two-cart shelf is bare ground: {slot:?}"
        );
    }
    assert!(
        apart(left, right) < 30,
        "the two carts did not repeat around the ring: the side slots hold {left:?} and \
         {right:?}, which are different carts"
    );
    assert!(
        apart(left, middle) > 60,
        "the repeat put the selected cart beside itself: {left:?} either side of {middle:?}"
    );

    // One shelf per platform: each cart stands alone in the middle of its own.
    let mut seen = Vec::new();
    let mut marks = vec![name_pixels(&gba)];
    for (name, banner, _platform) in [
        ("game-boy", "Game Boy", CartPlatform::Gb),
        ("game-boy-color", "Game Boy Color", CartPlatform::Gbc),
    ] {
        tap(&mut f, &mut input, Btn::R1);
        let_the_name_in(&mut f, &mut c, &mut input);
        let px = composed(&mut f, &mut c, name);
        let (ax, ay) = alone();
        let cart = patch(&px, ax, ay);
        let (lx, ly) = alone_label();
        let label = patch(&px, lx, ly);
        assert!(
            apart(cart, GROUND) > 60,
            "no cart in the middle of the {banner} shelf: {cart:?}"
        );
        for (side, (x, y)) in [("left", SIDE_LEFT), ("right", SIDE_RIGHT)] {
            let beside = patch(&px, x, y);
            assert!(
                apart(beside, GROUND) < 30,
                "the {banner} shelf holds one cart but drew something on its {side}: {beside:?}"
            );
        }
        assert_eq!(
            banner_ink(&px),
            0,
            "the {banner} shelf banner'd its name over the carts"
        );
        let ink = name_ink(&px);
        assert!(
            ink > 20,
            "the {banner} shelf came up with nothing printed on the case: {ink} lit pixels"
        );
        let mark = name_pixels(&px);
        assert!(
            !marks.contains(&mark),
            "the {banner} shelf is printing a name another shelf already printed"
        );
        marks.push(mark);
        assert!(
            seen.iter()
                .all(|(c, l)| apart(cart, *c) + apart(label, *l) > 60),
            "{banner} is showing a cart another shelf already showed: {cart:?} in {label:?}"
        );
        seen.push((cart, label));
    }

    // Round the ring and back to where it started, on the cart the shelf was left on.
    tap(&mut f, &mut input, Btn::R1);
    let_the_name_in(&mut f, &mut c, &mut input);
    let back = composed(&mut f, &mut c, "gba-again");
    assert!(
        apart(patch(&back, SIDE_LEFT.0, SIDE_LEFT.1), left) < 30,
        "the ring did not come back to the cart the first shelf was left on"
    );
    assert_eq!(banner_ink(&back), 0, "the way back put a banner up");
    assert_eq!(
        name_pixels(&back),
        marks[0],
        "the ring came back to the Game Boy Advance shelf under another system's name"
    );

    // The name is a moment, not a label: it fades out of the slot on its own.
    std::thread::sleep(std::time::Duration::from_millis(2400));
    f.advance(&mut input);
    let later = composed(&mut f, &mut c, "gba-faded");
    assert_eq!(
        name_ink(&later),
        0,
        "the shelf's name was still in the slot after it should have faded"
    );
}

/// A card whose roms sit loose at the top of `Games/` comes up a clean empty shelf, still
/// composing a second later, not a crash, hang or half-drawn screen.
#[test]
fn a_card_nobody_has_organised_comes_up_an_empty_shelf() {
    let Ok(surface) = HeadlessSurface::new() else {
        return;
    };
    let Ok(mut c) = Compositor::new(&surface) else {
        return;
    };

    // A rom, its save and its label loose at the top of their folders, plus a pre-namespacing
    // state directory.
    let d = tmp_root_with_carts(&[]);
    std::fs::write(d.path().join("Games/Emerald.gba"), vec![0u8; 0x100]).expect("loose rom");
    std::fs::write(d.path().join("Saves/Emerald.sav"), vec![7u8; 0x10000]).expect("loose save");
    std::fs::write(d.path().join("Labels/Emerald.png"), b"png").expect("loose label");
    clocked(d.path());

    let mut f = Frontend::boot(Box::new(SimPlatform::at(d.path().to_path_buf())));
    f.upload_faces(&mut c);
    let mut input = Script(VecDeque::new());
    f.advance(&mut input);
    let empty = composed(&mut f, &mut c, "loose-card");

    // An empty library draws no carts or top plate, but the bottom plate's HUD is always there.
    let plate = patch(&empty, 150, 470);
    assert!(
        apart(plate, GROUND) > 20,
        "the bottom plate is not there, so this is a blank screen rather than an empty shelf: \
         {plate:?}"
    );
    assert!(
        hud_ink(&empty) > 100,
        "the plate came up with no battery and no clock printed on it: {} lit pixels",
        hud_ink(&empty)
    );
    for (name, (x, y)) in [
        ("left", SIDE_LEFT),
        ("middle", MIDDLE),
        ("right", SIDE_RIGHT),
    ] {
        let slot = patch(&empty, x, y);
        assert!(
            apart(slot, GROUND) < 30,
            "the {name} slot of an unorganised card is holding something: {slot:?}"
        );
    }

    // A second of frames later it is still the same screen, and still composing.
    for _ in 0..60 {
        f.advance(&mut input);
    }
    let later = composed(&mut f, &mut c, "loose-card-later");
    for (name, (x, y)) in [
        ("left", SIDE_LEFT),
        ("middle", MIDDLE),
        ("right", SIDE_RIGHT),
    ] {
        let slot = patch(&later, x, y);
        assert!(
            apart(slot, GROUND) < 30,
            "a cart appeared in the {name} slot a second after an unorganised card booted: \
             {slot:?}"
        );
    }

    // Contrast: the same readings find a cart when the rom is in `Games/GBA/`.
    let organised = tmp_root_with_carts(&["Emerald", "Fusion"]);
    clocked(organised.path());
    let mut f = Frontend::boot(Box::new(SimPlatform::at(organised.path().to_path_buf())));
    f.upload_faces(&mut c);
    f.advance(&mut input);
    let full = composed(&mut f, &mut c, "organised-card");
    let middle = patch(&full, MIDDLE.0, MIDDLE.1);
    assert!(
        apart(middle, GROUND) > 60,
        "the organised card's own shelf is bare too, so this test cannot see a cart at all: \
         {middle:?}"
    );
}

/// A card whose carts are all Game Boy Advance gets a bare corner: there is no shelf to switch
/// to. A two-platform card is composed beside it for contrast.
#[test]
fn a_card_on_one_shelf_leaves_the_corner_empty() {
    let Ok(surface) = HeadlessSurface::new() else {
        return;
    };
    let Ok(mut c) = Compositor::new(&surface) else {
        return;
    };

    let one = tmp_root_with_carts(&["Emerald", "Fusion"]);
    clocked(one.path());
    let mut f = Frontend::boot(Box::new(SimPlatform::at(one.path().to_path_buf())));
    f.upload_faces(&mut c);
    let mut input = Script(VecDeque::new());
    f.advance(&mut input);
    let bare = composed(&mut f, &mut c, "one-shelf");
    assert_eq!(
        name_ink(&bare),
        0,
        "a card with one shelf named it in the slot: {} lit pixels",
        name_ink(&bare)
    );
    // And the shoulders leave it that way.
    tap(&mut f, &mut input, Btn::R1);
    let pressed = composed(&mut f, &mut c, "one-shelf-after-r1");
    assert_eq!(
        name_ink(&pressed),
        0,
        "R1 named a shelf on a card that has only one"
    );

    // The same fixture with a Game Boy cart added.
    let two = tmp_root_with_carts(&["Emerald", "Fusion"]);
    put_game_boy_carts(two.path());
    clocked(two.path());
    let mut f = Frontend::boot(Box::new(SimPlatform::at(two.path().to_path_buf())));
    f.upload_faces(&mut c);
    f.advance(&mut input);
    let_the_name_in(&mut f, &mut c, &mut input);
    let marked = composed(&mut f, &mut c, "two-shelves");
    assert!(
        name_ink(&marked) > 20,
        "a card with two shelves did not say which one it was on: {} lit pixels",
        name_ink(&marked)
    );
}

/// The cart going into the slot from a shelf of two: it travels straight down, and both copies
/// of the other cart part and go. Driven off the draw list so the clock lands mid-travel exactly.
#[test]
fn a_cart_going_in_from_a_repeated_row_takes_both_copies_of_its_neighbour_with_it() {
    let Ok(surface) = HeadlessSurface::new() else {
        return;
    };
    let Ok(mut c) = Compositor::new(&surface) else {
        return;
    };
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    clocked(d.path());
    // No faces are uploaded, so each cart draws as a rect in its label colour.
    let mut app = App::boot(d.path());
    let ink = label_colour(&clean_label("Emerald"));

    let standing = shot(&app, &mut c, Some("insert-0-standing"));
    let (from, _) = span(&standing, ink);
    // Side carts are dimmed; what matters is that both sides match and the middle does not.
    let west = patch(&standing, SIDE_LEFT.0, SIDE_LEFT.1);
    let east = patch(&standing, SIDE_RIGHT.0, SIDE_RIGHT.1);
    app.apply(Action::Insert);
    app.update(SEATED_AT / 2.0);
    let halfway = shot(&app, &mut c, Some("insert-1-halfway"));
    let (mid, y) = span(&halfway, ink);
    for _ in 0..120 {
        app.update(1.0 / 60.0);
    }
    let seated = shot(&app, &mut c, Some("insert-2-seated"));
    let (home, home_y) = span(&seated, ink);

    for (side, slot) in [("left", west), ("right", east)] {
        assert!(
            apart(slot, GROUND) > 60,
            "the {side} of the selection is bare ground: {slot:?}"
        );
    }
    assert!(
        apart(west, east) < 30,
        "the other cart was not repeated on both sides of the selection: {west:?} and {east:?}"
    );
    assert!(
        (from - 360.0).abs() < 8.0,
        "the row did not stand its selection in the middle: {from}"
    );
    assert!(
        (home - 360.0).abs() < 8.0,
        "the cart did not seat in the middle of the slot: {home}"
    );
    assert!(
        (mid - from).abs() < 8.0,
        "the cart slid sideways on its way in: {from} then {mid} then {home}"
    );
    assert!(
        home_y > y,
        "the cart did not go down the slot: {y} then {home_y}"
    );
    for (side, (x, y)) in [("left", SIDE_LEFT), ("right", SIDE_RIGHT)] {
        let slot = patch(&seated, x, y);
        assert!(
            apart(slot, GROUND) < 30,
            "the {side} copy of the other cart is still on the row with the chosen one \
             seated: {slot:?}"
        );
    }
}

/// A whole scroll, frame by frame, on shelves of two and three: the carts move as a rigid row,
/// one way, no faster than the spring allows, and end home.
#[test]
fn a_scrolled_row_slides_by_a_pitch_rather_than_swapping_its_carts() {
    let Ok(surface) = HeadlessSurface::new() else {
        return;
    };
    let Ok(mut c) = Compositor::new(&surface) else {
        return;
    };
    for (carts, stems, least) in [
        (2, &["Emerald", "Fusion", ""][..2], 2),
        (3, &["Emerald", "Fusion", "Sapphire"][..], 2),
    ] {
        let d = tmp_root_with_carts(stems);
        clocked(d.path());
        // No faces: every cart is a rect in its own label colour.
        let mut app = App::boot(d.path());
        let mut frames = vec![shot(&app, &mut c, Some(&format!("scroll-{carts}-00")))];
        // Pressed and let go: the shoulder auto repeats while held.
        app.apply(Action::ShelfRight);
        app.apply(Action::GbaUp(Btn::Right));
        // Every frame is measured, every fourth written out.
        for f in 1..=30 {
            app.update(1.0 / 60.0);
            let name = (f % 4 == 0).then(|| format!("scroll-{carts}-{f:02}"));
            frames.push(shot(&app, &mut c, name.as_deref()));
        }

        // Row offset in pitches off the middle, unwrapped per frame; starts a pitch out.
        let mut stood = 1.0f32;
        for (f, px) in frames.iter().enumerate() {
            let runs = row_runs(px);
            assert!(
                (least..=4).contains(&runs.len()),
                "{carts} carts, frame {f}: {} carts on screen, not the {least} to four a \
                 720 px row of them holds",
                runs.len()
            );
            // Only carts wholly on screen: one hanging off an edge is measured short.
            let row: Vec<f32> = runs
                .iter()
                .filter(|(a, b)| *a > 0 && *b < OUT_W as usize - 1)
                .map(|(a, b)| ((a + b) as f32 / 2.0 - OUT_W as f32 / 2.0) / CART_W as f32)
                .collect();
            assert!(
                !row.is_empty(),
                "{carts} carts, frame {f}: only {} carts are wholly on screen, so there is \
                 nothing to compare the row against itself with",
                row.len()
            );
            let phase = row[0] - row[0].round();
            for q in &row {
                assert!(
                    (q - q.round() - phase).abs() < 0.03,
                    "{carts} carts, frame {f}: a cart stands at {q} pitches while another \
                     stands at {}, so this is not one row moving",
                    row[0]
                );
            }
            // The nearest reading to last frame's; a pitch is a whole cart, so nothing else fits.
            let now = [phase - 1.0, phase, phase + 1.0]
                .into_iter()
                .fold(f32::MAX, |a, b| {
                    if (b - stood).abs() < (a - stood).abs() {
                        b
                    } else {
                        a
                    }
                });
            assert!(
                now <= stood + 0.01,
                "{carts} carts, frame {f}: the row turned round, from {stood} to {now}"
            );
            // 23.5 px is the fastest a critically damped spring at this stiffness carries a
            // one-pitch move in a 60th of a second.
            assert!(
                stood - now < 0.12,
                "{carts} carts, frame {f}: the row jumped {} of a pitch, which is a cart \
                 teleporting rather than sliding",
                stood - now
            );
            stood = now;
        }
        assert!(
            stood.abs() < 0.01,
            "{carts} carts: the row came to rest {stood} of a pitch off the middle"
        );
    }
}

/// The first and last column of every cart on the row, in screen order. Carts are the only lit
/// thing in this band, separated by 26 px of backdrop.
fn row_runs(px: &[u8]) -> Vec<(usize, usize)> {
    let lit = |x: usize| {
        (210..300).any(|y| {
            let c = at(px, x, y);
            c.iter().map(|v| *v as u32).sum::<u32>() > 60
        })
    };
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for x in 0..OUT_W as usize {
        match runs.last_mut() {
            Some(run) if run.1 + 1 == x && lit(x) => run.1 = x,
            _ if lit(x) => runs.push((x, x)),
            _ => {}
        }
    }
    runs
}

/// The horizontal centre of everything drawn in `ink`, and the lowest row it reaches.
fn span(px: &[u8], ink: [u8; 3]) -> (f32, usize) {
    let close = |c: [u8; 3]| (0..3).all(|k| c[k].abs_diff(ink[k]) <= 24);
    let mut cols: Vec<usize> = Vec::new();
    let mut bottom = 0;
    for y in 0..OUT_H as usize {
        for x in 0..OUT_W as usize {
            if close(at(px, x, y)) {
                cols.push(x);
                bottom = y;
            }
        }
    }
    let first = *cols
        .first()
        .expect("nothing on screen in the cart's colour");
    let last = *cols.iter().max().expect("nothing in the cart's colour");
    ((first + last) as f32 / 2.0, bottom)
}

/// One frame of the app's draw list, composited and, if `SCRATCH_PNG_DIR` is set, written out
/// under `name`.
fn shot(app: &App, c: &mut Compositor, name: Option<&str>) -> Vec<u8> {
    let mut out = Vec::new();
    app.draw(&mut out);
    frame_named(c, &out, name)
}

/// The same for a hand-built draw list, so every frame of the insertion is reachable by seat and
/// platform.
fn frame(c: &mut Compositor, out: &[Draw], name: &str) -> Vec<u8> {
    frame_named(c, out, Some(name))
}

fn frame_named(c: &mut Compositor, out: &[Draw], name: Option<&str>) -> Vec<u8> {
    c.set_screen_power(1.0);
    c.begin_frame();
    c.draw_list(out);
    let px = c.read_frame();
    if let (Some(name), Ok(dir)) = (name, std::env::var("SCRATCH_PNG_DIR")) {
        let path = format!("{dir}/shelves-{name}.png");
        let file = std::fs::File::create(&path).expect("create png");
        let mut e = png::Encoder::new(std::io::BufWriter::new(file), OUT_W, OUT_H);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header()
            .expect("png header")
            .write_image_data(&px)
            .expect("png data");
        println!("wrote {path}");
    }
    px
}

/// A cartridge for each shape, with a title hashing to a label colour of its own.
fn cartridges() -> [(&'static str, Cart); 2] {
    [
        (
            "gba",
            Cart {
                platform: CartPlatform::Gba,
                stem: "Emerald".into(),
                rom: "Games/GBA/Emerald.gba".into(),
                label: None,
                code: String::new(),
                shell: None,
                title: "POKEMON EMER".into(),
            },
        ),
        (
            "pak",
            Cart {
                platform: CartPlatform::Gb,
                stem: "Tetris".into(),
                rom: "Games/GB/Tetris.gb".into(),
                label: None,
                code: String::new(),
                shell: None,
                title: "TETRIS".into(),
            },
        ),
    ]
}

/// Where this cartridge's paper starts down its face. A fixed row would land on paper for one
/// shape and plastic for the other: a pak is 253 px tall against a GBA cart's 135.
fn label_top(p: CartPlatform) -> usize {
    match p {
        CartPlatform::Gba => LABEL_Y as usize,
        CartPlatform::Gb | CartPlatform::Gbc => GB_LABEL_Y as usize,
    }
}

/// The first and last screen rows showing the cartridge's own paper. Only for a cartridge clear
/// of the machine: a seated pak shows none.
fn paper_rows(px: &[u8], ink: [u8; 3]) -> Option<(usize, usize)> {
    let close = |c: [u8; 3]| (0..3).all(|k| c[k].abs_diff(ink[k]) <= 24);
    let mut rows =
        (0..OUT_H as usize).filter(|y| (0..OUT_W as usize).any(|x| close(at(px, x, *y))));
    let first = rows.next()?;
    Some((first, rows.next_back().unwrap_or(first)))
}

/// Every colour in the frame that is not the cartridge: the clear black and the slot's four flat
/// theme colours.
fn backdrop() -> [[f32; 4]; 5] {
    [[0.0, 0.0, 0.0, 1.0], housing(), opening(), edge(), recess()]
}

/// The first and last screen rows the cartridge covers: whatever is neither backdrop nor theme
/// colour. Tolerance 8 a channel against a 17 gap (GBA 0x35 shell vs 0x24 housing).
fn shell_rows(px: &[u8]) -> Option<(usize, usize)> {
    let flat = backdrop();
    let cart = |c: [u8; 3]| {
        !flat.iter().any(|f| {
            (0..3).all(|k| {
                let want = (f[k] * 255.0).round() as u8;
                c[k].abs_diff(want) <= 8
            })
        })
    };
    let mut rows = (0..OUT_H as usize).filter(|y| (0..OUT_W as usize).any(|x| cart(at(px, x, *y))));
    let first = rows.next()?;
    Some((first, rows.next_back().unwrap_or(first)))
}

/// The insertion, rendered: a Game Boy pak stands centred, travels at its own size, catches on the
/// lip, and seats leaving as much cartridge out as a GBA cart.
#[test]
fn both_cartridges_go_into_the_slot_at_their_own_size() {
    let Ok(surface) = HeadlessSurface::new() else {
        return;
    };
    let Ok(mut c) = Compositor::new(&surface) else {
        return;
    };
    // Named for what the frame is of, so the sequence read in order is the animation.
    let beats = [
        ("0-standing", 0.0),
        ("1-falling", 0.25),
        ("2-at-the-catch", 0.42),
        ("3-caught", 0.55),
        ("4-pushed-through", 0.80),
        ("5-seated", 1.0),
    ];
    let mut seated = Vec::new();
    for (name, cart) in cartridges() {
        let face = cart_face(&cart);
        let (w, h) = cart_box(cart.platform);
        assert_eq!(
            (face.w, face.h),
            (w, h),
            "{name}: the face is not the size the layout thinks it is"
        );
        let tex = c.create_texture(face.w, face.h, &face.rgba);
        let ink = label_colour(&clean_label(&cart.stem));
        let rest = (OUT_W - w) as f32 / 2.0;

        for (beat, seat) in beats {
            let mut out = Vec::new();
            SlotChrome {
                cart: &cart,
                face: Some(tex),
                rest,
                // A settled row.
                scale: 1.0,
                seat,
                alert: None,
                dim: 0.0,
                screen: 0.0,
                game: false,
            }
            .draw(&mut out);
            let px = frame(&mut c, &out, &format!("insert-{name}-{beat}"));

            let Some((top, bottom)) = shell_rows(&px) else {
                panic!("{name} at {beat}: no cartridge on the screen at all");
            };
            if seat == 0.0 {
                assert!(
                    (top as f32 - rest_y(h as f32)).abs() < 1.5,
                    "{name} stands with its top edge at {top}, not at {} where the carousel \
                     centres a {h} px cartridge",
                    rest_y(h as f32)
                );
                let middle = (top + bottom) as f32 / 2.0;
                let want = rest_y(h as f32) + h as f32 / 2.0;
                assert!(
                    (middle - want).abs() < 1.5,
                    "{name} stands {top}..{bottom}, centred on {middle} rather than on {want}"
                );
                // The paper is the full label-well height at its own inset: a squashed cart
                // or the wrong platform's numbers would show here.
                let (paper_top, paper_bottom) =
                    paper_rows(&px, ink).expect("a standing cartridge shows its label");
                let inset = paper_top - top;
                assert!(
                    inset.abs_diff(label_top(cart.platform)) <= 2,
                    "{name}'s paper starts {inset} px down its face, not the {} its platform \
                     puts it at",
                    label_top(cart.platform)
                );
                let paper = paper_bottom - paper_top + 1;
                let want = match cart.platform {
                    CartPlatform::Gba => LABEL_H as usize,
                    _ => GB_LABEL_H as usize,
                };
                assert!(
                    paper.abs_diff(want) <= 2,
                    "{name}'s {want} px label came out {paper} px tall: it is being scaled"
                );
            }
            if seat == 1.0 {
                seated.push((name, top as f32, bottom as f32));
            }
        }
    }

    // Seated, the two are the same picture: same top edge, same run of cartridge left out.
    let (first, rest) = seated.split_first().expect("both cartridges seated");
    for (name, top, bottom) in rest {
        assert!(
            (top - first.1).abs() < 1.5,
            "{name} seats with its top edge at {top} and {} at {}: one is in deeper than the \
             other",
            first.0,
            first.1
        );
        assert!(
            ((bottom - top) - (first.2 - first.1)).abs() < 1.5,
            "{name} leaves {} px of itself out of the machine and {} leaves {}: the slot is \
             showing one cartridge more of itself than the other",
            bottom - top,
            first.0,
            first.2 - first.1
        );
    }
}

/// How far the cartridge moves each frame of the travel at device rate, printed for judging by
/// eye. Asserts no frame jumps more than half the cartridge, where it would read as two objects.
#[test]
fn no_frame_of_the_travel_jumps_further_than_the_cartridge_is_tall() {
    for (name, cart) in cartridges() {
        let (w, h) = cart_box(cart.platform);
        let ink = label_colour(&label_text(&cart));
        let ink = ink.map(|v| v as f32 / 255.0);
        let ys: Vec<f32> = (0..=27)
            .map(|f| {
                let mut out = Vec::new();
                SlotChrome {
                    cart: &cart,
                    face: None,
                    rest: (OUT_W - w) as f32 / 2.0,
                    scale: 1.0,
                    seat: (f as f32 / 27.0).min(1.0),
                    alert: None,
                    dim: 0.0,
                    screen: 0.0,
                    game: false,
                }
                .draw(&mut out);
                out.iter()
                    .find_map(|d| match d {
                        Draw::Rect { y, colour, .. }
                            if (0..3).all(|k| (colour[k] - ink[k]).abs() < 0.002) =>
                        {
                            Some(*y)
                        }
                        _ => None,
                    })
                    .expect("no cartridge in the list")
            })
            .collect();
        let steps: Vec<f32> = ys.windows(2).map(|w| (w[1] - w[0]).round()).collect();
        println!("{name}: {steps:?}");
        let worst = steps.iter().cloned().fold(0.0f32, f32::max);
        assert!(
            worst < h as f32 / 2.0,
            "{name} moves {worst} px in one frame, over half of its own {h} px: that is a \
             cut, not a movement"
        );
    }
}
