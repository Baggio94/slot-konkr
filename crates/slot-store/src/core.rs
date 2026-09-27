use std::collections::HashMap;
use std::path::Path;

use crate::Platform;

pub const SELECTED_CORE_FILE: &str = "System/selected_core.ini";

/// Which emulator runs a cart. mGBA is the default everywhere since it carries the emulated link
/// for all three platforms; gpSP covers serial hardware mGBA's libretro build lacks.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Core {
    #[default]
    Mgba,
    Gpsp,
}

impl Core {
    /// The core picker's sockets, in drawing order. The picker is a two-socket PCB traced from
    /// real hardware, so there is no room for a third.
    pub const ALL: [Core; 2] = [Core::Mgba, Core::Gpsp];

    pub fn as_str(&self) -> &'static str {
        match self {
            Core::Mgba => "mgba",
            Core::Gpsp => "gpsp",
        }
    }

    /// Position in `ALL`.
    pub fn index(self) -> usize {
        self as usize
    }

    /// The name shown to the player. `as_str` is the ini spelling and may differ.
    pub fn text(self) -> &'static str {
        match self {
            Core::Mgba => "mGBA",
            Core::Gpsp => "gpSP",
        }
    }

    pub fn parse(s: &str) -> Option<Core> {
        match s.trim().to_ascii_lowercase().as_str() {
            "mgba" => Some(Core::Mgba),
            "gpsp" => Some(Core::Gpsp),
            _ => None,
        }
    }

    /// Whether this core runs that platform's carts. gpSP is GBA only.
    pub fn runs(self, platform: Platform) -> bool {
        match self {
            Core::Mgba => true,
            Core::Gpsp => platform == Platform::Gba,
        }
    }

    /// The core a cart gets when the card names none. mGBA carries the emulated link on all three;
    /// linked pairs on an H700 cost 3.802 ms (Game Boy) and 2.661 ms (Colour) of a 16.743 ms frame.
    pub fn default_for(platform: Platform) -> Core {
        match platform {
            Platform::Gba | Platform::Gb | Platform::Gbc => Core::Mgba,
        }
    }
}

/// `<rom stem> = <core>` in `crate::ini`'s format. Unknown core names are dropped, not errors:
/// a newer build's card or a typo should fall back to the default.
pub fn read_selected_cores(root: &Path) -> HashMap<String, Core> {
    crate::ini::read(root, SELECTED_CORE_FILE)
        .into_iter()
        .filter_map(|(stem, name)| Core::parse(&name).map(|core| (stem, core)))
        .collect()
}

/// The core one cart wants, or the default if its line is missing or unparseable. Platform-blind,
/// so GBA-only in practice: prefer `core_for_platform`.
pub fn core_for(root: &Path, stem: &str) -> Core {
    crate::ini::value(root, SELECTED_CORE_FILE, stem)
        .as_deref()
        .and_then(Core::parse)
        .unwrap_or_default()
}

/// The cart's line in `selected_core.ini` if it names a core that runs this platform, else the
/// platform default. The file is hand-edited, so a line like `Tetris = gpsp` must not win.
pub fn core_for_platform(root: &Path, stem: &str, platform: Platform) -> Core {
    crate::ini::value(root, SELECTED_CORE_FILE, stem)
        .as_deref()
        .and_then(Core::parse)
        .filter(|core| core.runs(platform))
        .unwrap_or_else(|| Core::default_for(platform))
}

/// Set one cart's core, leaving the rest of the file exactly as it was.
pub fn write_selected_core(root: &Path, stem: &str, core: Core) -> std::io::Result<()> {
    crate::ini::write(root, SELECTED_CORE_FILE, stem, core.as_str())
}
