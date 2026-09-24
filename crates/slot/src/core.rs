use std::path::{Path, PathBuf};

use slot_retro::{LibretroCore, MockCore, RetroCore};
use slot_store::Core;

use crate::root;

/// Names the dylib outright, overriding the search. Developer escape hatch: it changes only which
/// file opens, not which `Core` a cart resolves to, so states still file under the ini's core.
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
/// mGBA declares `OFF|GBA|GBC|Auto`; gpSP declares `disabled|enabled`.
pub fn colour_option(which: Core, on: bool) -> Option<(&'static str, &'static str)> {
    match which {
        Core::Mgba => Some(("mgba_color_correction", if on { "GBA" } else { "OFF" })),
        Core::Gpsp => Some((
            "gpsp_color_correction",
            if on { "enabled" } else { "disabled" },
        )),
    }
}

/// mGBA's in-core link: `mgba_link_player` says which console this device drives. Other cores
/// are left alone.
///
/// `mgba_use_bios` is forced off so both devices boot the same machine. A state saved during a
/// real BIOS boot is refused by a peer without that BIOS (`GBADeserialize` checks the checksum),
/// and a card without Nintendo's image cannot be given one.
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
        // Values were read off the vendored dylib: a misspelt value is accepted silently and
        // never takes.
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
            Some(("mgba_color_correction", "GBA"))
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

    /// `candidates` only ever names the dylib of the `Core` it was given.
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
