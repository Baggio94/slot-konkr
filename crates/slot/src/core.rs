use std::path::{Path, PathBuf};

use slot_retro::{LibretroCore, MockCore, RetroCore};
use slot_store::Core;

use crate::root;

/// Names the dylib outright, for a build that keeps it somewhere the search does not look.
///
/// This wins over `System/selected_core.ini` for which dylib loads — it stays a developer
/// escape hatch, not a second way to pick a core. It is not silent about that: it does not
/// touch which `Core` a cart resolves to (still the ini, still what the core half of
/// `States/<platform>/<core>/` is named after), only which file `open_core` opens. Pointing
/// this at gpSP for a cart the ini never mentions runs gpSP with its states filed under that
/// platform's `mgba` directory — correct for a developer who set the override on purpose, a
/// trap for anyone who forgot it was set.
const CORE_ENV: &str = "SLOT_CORE";

/// Which dylib backs a core. The device keeps both in `System/`.
pub fn dylib_name(core: Core) -> String {
    format!(
        "{}_libretro.{}",
        core.as_str(),
        std::env::consts::DLL_EXTENSION
    )
}

/// Most specific first: the environment, the content root's `System/` (where the device keeps
/// cores, and where tests plant one), the binary's directory, then the host-only `vendor`.
fn candidates(root: &Path, core: Core) -> Vec<PathBuf> {
    if let Some(named) = std::env::var_os(CORE_ENV) {
        return vec![PathBuf::from(named)];
    }
    let name = dylib_name(core);
    let mut paths = vec![root.join("System").join(&name)];
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        paths.push(dir.join(&name));
        paths.push(dir.join("vendor").join(&name));
    }
    paths.push(Path::new("vendor").join(&name));
    paths
}

/// What `open_core` opened, and whether it is the real core.
///
/// The mock refuses every state it did not write, so a refusal only means a bad state when
/// `named` is true. Retiring a resume on the mock's refusal would lose the save.
pub struct Opened {
    pub core: Box<dyn RetroCore>,
    /// `false` when no candidate dylib loaded and `MockCore` is standing in.
    pub named: bool,
}

/// The one place a cart's `Core` becomes a dylib path. `serial` is gpSP's `gpsp_serial` and
/// `colour` the quick menu's Colour Correction; see `apply_core_options`.
pub fn open_core(root: &Path, core: Core, serial: &str, colour: bool, link: Option<u8>) -> Opened {
    let paths = candidates(root, core);
    match open_named(root, core, serial, colour, link, &paths) {
        Some(core) => Opened { core, named: true },
        None => {
            report_missing(core, &paths);
            Opened {
                core: Box::new(MockCore::new()),
                named: false,
            }
        }
    }
}

/// The named core if one of these opens, the mock if none do. `paths` is explicit so tests can
/// plant a dylib where `candidates` would not look. The core is given the content root's
/// folders, never the dylib's.
pub fn open_core_for(
    root: &Path,
    core: Core,
    serial: &str,
    colour: bool,
    paths: &[PathBuf],
) -> Box<dyn RetroCore> {
    open_named(root, core, serial, colour, None, paths).unwrap_or_else(|| {
        report_missing(core, paths);
        Box::new(MockCore::new())
    })
}

/// The search with no fallback: `None` means every candidate was missing or would not load.
/// Options are applied here, the only place holding a concrete `LibretroCore`, before `load`.
fn open_named(
    root: &Path,
    core: Core,
    serial: &str,
    colour: bool,
    link: Option<u8>,
    paths: &[PathBuf],
) -> Option<Box<dyn RetroCore>> {
    let bios = root::bios_dir(root);
    let saves = root::saves_dir(root);
    for path in paths {
        if !path.exists() {
            continue;
        }
        match LibretroCore::open_with(path, &bios, &saves) {
            Ok(mut opened) => {
                apply_core_options(&mut opened, core, serial, root::has_real_bios(root), colour);
                // mGBA reads link options only in `retro_load_game`, so a running core cannot
                // enter link mode; `Session::reload_for_link` re-opens it.
                if let Some(player) = link {
                    apply_link_options(&mut opened, core, player);
                }
                eprintln!("slot: core {}", path.display());
                return Some(Box::new(opened));
            }
            Err(e) => eprintln!("slot: {}: {e}", path.display()),
        }
    }
    None
}

/// Log every path tried. The mock's test pattern looks like a broken core, not a missing one.
fn report_missing(core: Core, paths: &[PathBuf]) {
    eprintln!(
        "slot: no {} core found, running the mock test pattern instead. Looked in: {}",
        core.as_str(),
        paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
}

/// The colour correction option key and value for a core, shared by the load path and the quick
/// menu so they cannot drift: a core silently ignores an option it does not have.
///
/// One function rather than two call sites, because there are now two moments this is needed and
/// they must not drift: once before `load`, where every option is handed over, and once while a
/// game is on screen, when the quick menu row is toggled. A key that was right in one place and
/// stale in the other would be invisible from both, since a libretro core ignores an option it
/// does not have without saying so.
///
/// mGBA gets `Auto` rather than `GBA` or `GBC`: it is the core that runs all three consoles here
/// and `Auto` is the only value that picks the right tint for each. Naming one would give a Game
/// Boy cart the GBA's correction, a tint of the wrong console rather than a weaker version of the
/// right one. gpSP declares its own as `disabled|enabled`, a different key and a different pair of
/// words, and only ever runs GBA carts so it has no console to choose between.
///
/// Both shipped cores have the option, so nothing returns `None` today. The shape stays: a core
/// with nothing to set is a thing this has had to express once and may again.
pub fn colour_option(which: Core, on: bool) -> Option<(&'static str, &'static str)> {
    match which {
        Core::Mgba => Some(("mgba_color_correction", if on { "Auto" } else { "OFF" })),
        Core::Gpsp => Some((
            "gpsp_color_correction",
            if on { "enabled" } else { "disabled" },
        )),
    }
}

/// mGBA's in-core link: `mgba_link_player` says which console this device drives. Other cores
/// are left alone.
///
/// `serial` is gpSP's link mode, and loading a game is the only time gpSP reads it: a reset
/// does not, and a save state does not carry it. `auto` resolves the protocol from the ROM, so
/// two devices running the same game agree on a mode without either being told which, and it
/// is what every cart loads with until its link screen is switched to the other hardware (see
/// `link_kind::serial_option`). mGBA gets none of gpSP's options: it has no `gpsp_serial`, and
/// handing it one anyway is a landmine the day it grows one. It does get a frameskip option,
/// below, but under its own prefix — neither core reads the other's — and one of its own that
/// gpSP has no equivalent for, `mgba_sgb_borders`, because mGBA is the core that runs a Game Boy
/// cart and a Super Game Boy border is the one thing it can draw that will not fit the panel.
///
/// `bios` is whether the card carries a real BIOS (`root::has_real_bios`), and it buys the
/// player the boot logo and chime. gpSP defaults to `game`, which drops straight into the
/// cart; `bios` runs the BIOS first. It is only set when the file is really there, because
/// booting through gpSP's built-in replacement instead spends those seconds on a blank screen
/// — a pause that reads as a hang rather than as the hardware starting up.
///
/// `gpsp_bios` is deliberately left alone. Its default, `auto`, already loads
/// `<system>/gba_bios.bin` and uses it whenever the image passes gpSP's own first-byte test,
/// which is the same test `has_real_bios` applies — so naming `official` would select the
/// identical image. All it would change is the failure path, where `official` puts a warning
/// on screen through the core's OSD ("Could not load BIOS image file", "BIOS image seems
/// incorrect") before falling back to exactly the built-in BIOS `auto` falls back to silently.
/// That is a core-drawn message over slot's own chrome, bought for no change in behaviour.
///
/// Setting this on every load, rather than only on a fresh start, is safe because a resume
/// does not survive to be seen: `emu::Worker::run` unserializes the resume state after `load`
/// and before it publishes a single frame, so the restored machine replaces the BIOS's before
/// anything reaches the screen. That covers a reload for a link too — `session::reload_for_link`
/// flushes and resumes through the same path.
///
/// `colour` is the quick menu's Colour Correction, and unlike everything else here it is set on
/// both cores, because both of them have the option — which is worth stating outright, since it
/// was assumed for a while that only mGBA did. Each spells it its own way; see below.
/// The in-core cable, on a core that has one. mGBA runs both consoles itself and `mgba_link_player`
/// says which of them this device drives; every other core has no such mode and is left alone.
///
/// `mgba_use_bios` is forced off, and it is the one option here that is about the *other* device.
/// Nothing above sets it, so mGBA decides for itself: it uses `gba_bios.bin` when the card has
/// one and its own replacement when it does not. Two cards need not agree about that, and two
/// devices in a session are not running one game each, they are each running both. A pair booted
/// on different BIOSes is a pair of different machines, so they drift apart from the first frame
/// even if nothing complains.
///
/// It failed louder than drift, which is how it was found: the host serializes the moment a
/// session begins, while a real BIOS is still booting, so the saved PC is inside the BIOS. A
/// state whose BIOS checksum differs is refused outright in exactly that case, and only in that
/// case (`src/gba/serialize.c`, GBADeserialize). One card had `gba_bios.bin` and the other did
/// not, and the joiner answered the swap with "unserialize refused".
///
/// Off rather than on, because off is the only answer both devices can always give: the image is
/// Nintendo's and a card without one cannot be made to have one. The cost is the boot logo and
/// chime for a session, on a screen the players are about to leave anyway.
pub fn apply_link_options(core: &mut LibretroCore, which: Core, player: u8) {
    if which != Core::Mgba {
        return;
    }
    core.set_option("mgba_link", "on");
    core.set_option("mgba_link_player", &player.to_string());
    core.set_option("mgba_use_bios", "OFF");
    eprintln!("slot: core: link mode on, player {player}, built-in bios");
}

/// Options a core reads only during `retro_load_game`, so they must be set before `load`.
///
/// `serial` is gpSP's link mode; `auto` lets two devices on the same ROM agree without being told.
/// `bios` boots gpSP through the card's real BIOS, only when present: the built-in one shows a
/// blank pause that reads as a hang. A resume replaces the BIOS boot before any frame is shown.
/// `gpsp_bios` stays `auto`, which already uses a valid `gba_bios.bin`; `official` only adds an
/// on-screen warning on failure.
pub fn apply_core_options(
    core: &mut LibretroCore,
    which: Core,
    serial: &str,
    bios: bool,
    colour: bool,
) {
    // Auto frameskip only skips when the frontend reports audio running dry, which
    // `RetroCore::set_frame_skip` does only during fast forward. Set at load, since cores read
    // options in `retro_load_game`.
    core.set_option(&format!("{}_frameskip", which.as_str()), "auto");
    if which == Core::Mgba {
        // An SGB border makes the picture 256x224, and 256 is wider than the 240 this whole path
        // is built on — `video_refresh` would crop it. The core declares this one as `ON|OFF`
        // and its default is `ON`, so this is not merely belt and braces; and it is set
        // explicitly rather than counted on staying that way, so a default that changes under us
        // cannot turn a working screen into a cropped one either.
        //
        // `mgba_gb_model` is deliberately left alone. Its default is `Autodetect`, which is each
        // cart's own header read honestly, and naming a model here would override what the cart
        // says about itself. Nothing sets `mgba_use_bios` or `mgba_skip_bios`.
        core.set_option("mgba_sgb_borders", "OFF");
        // The next two put back what a Game Boy Advance SP showed when an original monochrome
        // Game Boy cartridge was pushed into it. The SP has no monochrome mode to fall back on:
        // a Game Boy cart runs through the Game Boy Color compatibility the AGB inherits, and
        // that boot ROM colourises the cart before handing it control. It colourises from a
        // table built into the ROM and keyed on the cartridge's own header — the licensee code
        // has to be Nintendo's `01`, and a checksum of the sixteen title bytes then selects one
        // of ninety-four palette assignments, with the fourth title letter breaking the ties.
        // A cart that misses, for either reason, is given the table's entry zero. So grayscale,
        // which is what mGBA does when left alone, is the one thing no hardware here ever put on
        // a screen: a Game Boy's own panel was green, and every machine that could still take
        // its cartridges afterwards coloured it. Two options rather than one because the
        // hardware behaviour has two halves, and setting either alone reproduces half of it.
        //
        // `mgba_gb_colors_preset` is the table. mGBA declares it as the bare digits `0|1|2|3`
        // with no labels in the list this frontend is old enough to be handed, so a digit is all
        // there is to set: 0 is off, 1 the Game Boy Color's presets, 2 the Super Game Boy's, 3
        // both. `1` is the SP exactly — for these cartridges an SP *is* a Game Boy Color, and it
        // is emphatically not a Super Game Boy, so `3` would hand a cart a palette no SP could
        // have shown. mGBA keys its own copy of the table on a CRC32 of the header instead of on
        // the licensee and title checksum, which reaches the same palette for a cart it has an
        // entry for by a different route, and can miss one the hardware would have matched.
        //
        // `mgba_gb_colors` is the other half: what a cart the table has no entry for is given.
        // The hardware's answer to that is not a neutral grey but one specific triple — the
        // background on the boot ROM's palette 29, white through green and blue to black, and
        // both object palettes on its 4, white through salmon and dark red to black — and it is
        // what every unlicensed, third-party and homebrew monochrome cart came up in. mGBA
        // exposes that triple under the name of a boot button combination, `GBC Dark Green →A`,
        // because the boot ROM reuses one entry for the default and for that combination. The
        // name is the coincidence; the colours are the hardware's default, which is why this is
        // that value rather than one of the grey or green ramps that sit above it in the list.
        //
        // Neither of these is the Colour Correction row below. That one is about how the SP's
        // screen answered a colour it was given; these decide which colours a cartridge with no
        // colours of its own is given in the first place. Both are Game Boy only: a Game Boy
        // Color or Game Boy Advance cart carries its own palettes and renders identically either
        // way, which `render_gb_palette.rs` holds to pixel for pixel.
        core.set_option("mgba_gb_colors_preset", "1");
        core.set_option("mgba_gb_colors", "GBC Dark Green →A");
        // mGBA declares this one as `OFF|GBA|GBC|Auto`, read off the vendored dylib rather than
        // guessed at, because nothing in this tree can tell a correct option value from a typo:
        // `SET_VARIABLES` is answered `true` and the declared list thrown away, so a misspelt
        // value is accepted in silence and simply never takes.
        //
        // `Auto` rather than `GBA` or `GBC`, because mGBA is the core that runs all three
        // consoles here and `Auto` is the only value that picks the right tint for each of them.
        // Naming one would give a Game Boy cart the GBA's correction, which is a tint of the
        // wrong console rather than a stronger or weaker version of the right one.
        // Through `colour_option`, which is also what the quick menu pushes at a running core.
        if let Some((key, value)) = colour_option(which, colour) {
            core.set_option(key, value);
        }
    }
    if which == Core::Gpsp {
        core.set_option("gpsp_serial", serial);
        if bios {
            core.set_option("gpsp_boot_mode", "bios");
        }
        // The quick menu cannot know the next cart's core, so gpSP must get colour too or the
        // row would silently do nothing on gpSP carts.
        if let Some((key, value)) = colour_option(which, colour) {
            core.set_option(key, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serialises tests that mutate the process-global `SLOT_CORE`.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// One key and value pair per core and state, for both load and live paths.
    #[test]
    fn each_core_spells_colour_correction_its_own_way() {
        assert_eq!(
            colour_option(Core::Mgba, true),
            Some(("mgba_color_correction", "Auto"))
        );
        assert_eq!(
            colour_option(Core::Mgba, false),
            Some(("mgba_color_correction", "OFF"))
        );
        assert_eq!(
            colour_option(Core::Gpsp, true),
            Some(("gpsp_color_correction", "enabled"))
        );
        assert_eq!(
            colour_option(Core::Gpsp, false),
            Some(("gpsp_color_correction", "disabled"))
        );
    }

    fn lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The half of "the dylib chosen and the state directory used agree" that lives entirely
    /// in this module: `candidates`, and therefore `open_core`, never spells a filename that
    /// does not match the `Core` it was handed. `crates/slot/tests/gpsp.rs` pins the other
    /// half — that a cart resolved to the same `Core` reads its resume state from the
    /// matching `States/<platform>/<core>/` directory — through the real `Session`.
    #[test]
    fn candidates_search_the_named_cores_own_filename_only() {
        let _g = lock();
        // A developer's shell may have `SLOT_CORE` set.
        std::env::remove_var(CORE_ENV);

        let root = Path::new("/root");
        let mgba = candidates(root, Core::Mgba);
        let gpsp = candidates(root, Core::Gpsp);
        assert_ne!(mgba, gpsp, "the two cores searched the same paths");
        assert!(!mgba.is_empty());
        assert!(!gpsp.is_empty());
        for path in &mgba {
            let name = path.file_name().unwrap().to_str().unwrap();
            assert!(name.starts_with("mgba_libretro"), "{name} is not mGBA's");
        }
        for path in &gpsp {
            let name = path.file_name().unwrap().to_str().unwrap();
            assert!(name.starts_with("gpsp_libretro"), "{name} is not gpSP's");
        }
    }

    /// The content root's `System/` is searched first, so integration tests can plant a dylib.
    #[test]
    fn candidates_search_the_roots_own_system_directory() {
        let _g = lock();
        std::env::remove_var(CORE_ENV);
        let root = Path::new("/some/content/root");
        assert_eq!(
            candidates(root, Core::Gpsp)[0],
            root.join("System").join(dylib_name(Core::Gpsp)),
        );
    }

    /// The override wins regardless of which core asked.
    #[test]
    fn the_env_override_ignores_which_core_was_asked_for() {
        let _g = lock();
        std::env::set_var(CORE_ENV, "/dev/null/named-core");
        let root = Path::new("/root");
        let mgba = candidates(root, Core::Mgba);
        let gpsp = candidates(root, Core::Gpsp);
        std::env::remove_var(CORE_ENV);
        assert_eq!(mgba, vec![PathBuf::from("/dev/null/named-core")]);
        assert_eq!(
            mgba, gpsp,
            "the override stopped winning for one of the cores"
        );
    }
}
