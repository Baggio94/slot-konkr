//! The Game Boy palette against the real core: a monochrome cart run through `open_core_for`,
//! compared against the same cart with the palette options off. A typo in the option value is
//! silently ignored and looks like mGBA's grayscale default, so only the pixels can tell.
//!
//! `SCRATCH_PNG_DIR=/tmp cargo test -p slot --test render_gb_palette -- --nocapture`

mod common;

use std::path::{Path, PathBuf};

use common::{core_lock, repo_root, vendored_core};
use slot_retro::{ButtonMask, LibretroCore, RetroCore, GBA_H, GBA_W};
use slot_store::Core;

/// The GBC boot ROM's default background palette (palette 29: `$7FFF`, `$1BEF`, `$6180`) as it
/// lands in slot's framebuffer. Compared with a tolerance, since the core's pixel format may vary.
const DEFAULT_BG: [[u8; 3]; 3] = [[255, 251, 255], [123, 251, 49], [0, 97, 198]];

/// Frames run before reading, the same for both arms so every difference is the option's.
const FRAMES: usize = 600;

fn to_rgba(xrgb: &[u8]) -> Vec<u8> {
    xrgb.chunks_exact(4)
        .flat_map(|p| [p[2], p[1], p[0], 0xff])
        .collect()
}

fn write_png(name: &str, rgba: &[u8]) {
    let Ok(dir) = std::env::var("SCRATCH_PNG_DIR") else {
        return;
    };
    let path = format!("{dir}/{name}.png");
    let file = std::fs::File::create(&path).expect("create png");
    let mut e = png::Encoder::new(std::io::BufWriter::new(file), GBA_W, GBA_H);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header()
        .expect("png header")
        .write_image_data(rgba)
        .expect("png data");
    println!("wrote {path}");
}

/// Mean per-pixel channel spread: near zero for a grey ramp, several times that when coloured.
fn mean_saturation(rgba: &[u8]) -> f64 {
    let sum: f64 = rgba
        .chunks_exact(4)
        .map(|p| {
            let (hi, lo) = (p[..3].iter().max(), p[..3].iter().min());
            f64::from(hi.copied().unwrap_or(0) - lo.copied().unwrap_or(0))
        })
        .sum();
    sum / (rgba.len() / 4) as f64
}

fn holds_colour(rgba: &[u8], want: [u8; 3]) -> bool {
    rgba.chunks_exact(4)
        .any(|p| (0..3).all(|c| p[c].abs_diff(want[c]) <= 6))
}

/// Distinct colours in a picture; a Game Boy picture has at most twelve.
fn distinct_colours(rgba: &[u8]) -> usize {
    let mut seen: Vec<[u8; 3]> = Vec::new();
    for p in rgba.chunks_exact(4) {
        let c = [p[0], p[1], p[2]];
        if !seen.contains(&c) {
            seen.push(c);
        }
    }
    seen.len()
}

/// One run through slot's own `open_core_for`, so the options come from production.
fn shipped(root: &Path, dylib: &Path, rom: &Path) -> Vec<u8> {
    let mut core = slot::core::open_core_for(
        root,
        Core::Mgba,
        "auto",
        false,
        std::slice::from_ref(&dylib.to_path_buf()),
    );
    core.load(rom).expect("the core would not take the rom");
    for _ in 0..FRAMES {
        core.run_frame(ButtonMask(0));
    }
    to_rgba(core.video_xrgb8888())
}

/// The control: the same options except the two palette ones, opened past `open_core_for` so the
/// change under test cannot affect it.
fn without_palette(root: &Path, dylib: &Path, rom: &Path) -> Vec<u8> {
    let mut core = LibretroCore::open_with(
        dylib,
        &slot::root::bios_dir(root),
        &slot::root::saves_dir(root),
    )
    .expect("open the core");
    core.set_option("mgba_frameskip", "auto");
    core.set_option("mgba_sgb_borders", "OFF");
    core.set_option("mgba_color_correction", "OFF");
    core.load(rom).expect("the core would not take the rom");
    for _ in 0..FRAMES {
        core.run_frame(ButtonMask(0));
    }
    to_rgba(core.video_xrgb8888())
}

/// A cart off the ignored `/sdcard`, read only. `None` without a card, which skips.
fn card_cart(name: &str) -> Option<PathBuf> {
    let p = repo_root().join(name);
    p.exists().then_some(p)
}

/// A monochrome cart comes up in the SP's default palette, not mGBA's grey ramp. Both carts are
/// third-party, so the boot ROM misses its table; a homebrew declaring Nintendo's licensee and
/// `TETRIS` gets Tetris's palette on hardware but not in mGBA, which keys on a header CRC32.
#[test]
fn a_monochrome_cart_comes_up_in_the_colours_the_sp_gave_it() {
    let Some(dylib) = vendored_core() else {
        eprintln!("no mgba dylib, skipping");
        return;
    };
    let _g = core_lock();
    let d = common::tmp_root_with_carts(&[]);

    for cart in ["Catrap (USA).gb", "A-mazing Tater (USA).gb"] {
        let Some(rom) = card_cart(&format!("sdcard/Games/GB/{cart}")) else {
            eprintln!("no {cart} on this machine's card, skipping");
            continue;
        };
        let stem = cart.split_whitespace().next().unwrap_or(cart);
        let off = without_palette(d.path(), &dylib, &rom);
        let on = shipped(d.path(), &dylib, &rom);
        write_png(&format!("gb-palette-{stem}-off"), &off);
        write_png(&format!("gb-palette-{stem}-on"), &on);

        // The control must be grey, or the comparison means nothing. Thresholds sit far from today's
        // measurements (Catrap about 11, A-mazing Tater about 15); the palette checks pin the colour.
        let (sat_off, sat_on) = (mean_saturation(&off), mean_saturation(&on));
        println!("{cart}: saturation {sat_off:.2} off, {sat_on:.2} on");
        assert!(
            sat_off < 4.0,
            "{cart}: the control should be grey, saturation was {sat_off:.1}"
        );
        assert!(
            sat_on > 6.0,
            "{cart}: the palette should have coloured this, saturation was {sat_on:.1}"
        );
        // Saturation only says some colour arrived; these say it is the default background palette.
        for want in DEFAULT_BG {
            assert!(
                holds_colour(&on, want),
                "{cart}: {want:?} is in the boot ROM's default background palette and is not on \
                 screen; some other palette was applied"
            );
        }
    }
}

/// A Game Boy Color cart is left pixel for pixel as it was.
#[test]
fn a_colour_cart_is_untouched() {
    let Some(dylib) = vendored_core() else {
        eprintln!("no mgba dylib, skipping");
        return;
    };
    let _g = core_lock();
    let d = common::tmp_root_with_carts(&[]);

    let Some(rom) = card_cart("sdcard/Games/GBC/Tetris Chromatic.gbc") else {
        eprintln!("no Game Boy Color cart on this machine's card, skipping");
        return;
    };
    let off = without_palette(d.path(), &dylib, &rom);
    let on = shipped(d.path(), &dylib, &rom);
    write_png("gb-palette-colour-off", &off);
    write_png("gb-palette-colour-on", &on);

    // A Colour cart that has drawn nothing would pass trivially. Counted in colours, not saturation,
    // because this title screen is a starfield on black.
    let colours = distinct_colours(&on);
    println!("the Colour cart drew {colours} distinct colours");
    assert!(
        colours > 12,
        "the Colour cart has not drawn a colour picture yet ({colours} colours), so comparing \
         the two proves nothing"
    );
    assert_eq!(
        off, on,
        "the Game Boy palette options changed what a Game Boy Color cart draws"
    );
}
