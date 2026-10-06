use std::path::{Path, PathBuf};

use slot_store::{atomic_write, read_slot_state, write_slot_state, Core, Platform, StateRing};

/// What a save, a load or a flush needs from the emulator, which runs on a worker thread.
pub trait Snapshot {
    fn state(&self) -> Option<Vec<u8>>;
    fn save_ram(&self) -> Option<Vec<u8>>;
    /// The last frame, PNG encoded on the worker so a save costs the compositor no hitch.
    fn thumb(&self) -> Option<Vec<u8>>;
    fn load(&self, state: Vec<u8>);

    /// Whether `state()` comes from a core that accepted its resume. Only a live core whose
    /// `unserialize` failed answers `false` (see `EmuSnapshot::resume_trusted`). Flushing
    /// `state()` without checking this can overwrite the player's save with a default machine.
    fn resume_trusted(&self) -> bool {
        true
    }

    /// The `save_ram()` twin of `resume_trusted`, independent of it.
    fn save_ram_trusted(&self) -> bool {
        true
    }
}

/// What lid close, power and autosave write. The slot is untouched, so the cart is still in it
/// on the next boot. `platform` and `core` come from `App`'s one resolution at insert, so every
/// write agrees with the cart seated. A `None` region was refused by the core and is skipped.
pub fn flush(
    root: &Path,
    platform: Platform,
    core: Core,
    stem: &str,
    state: Option<&[u8]>,
    sav: Option<&[u8]>,
) -> std::io::Result<()> {
    if let Some(state) = state {
        StateRing::new(root, platform, core, stem).write_resume(state)?;
    }
    if let Some(sav) = sav {
        write_sav(root, platform, stem, sav)?;
    }
    Ok(())
}

/// Both writes land before the slot is recorded empty, so a power cut leaves a cart that still
/// resumes. The slot is cleared even when a refused region was withheld.
pub fn eject(
    root: &Path,
    platform: Platform,
    core: Core,
    stem: &str,
    state: Option<&[u8]>,
    sav: Option<&[u8]>,
) -> std::io::Result<()> {
    flush(root, platform, core, stem, state, sav)?;
    let mut slot = read_slot_state(root);
    slot.cart = None;
    // One fact, so cleared together: a leftover platform would describe no cart.
    slot.cart_platform = None;
    write_slot_state(root, &slot)
}

/// Skips an unchanged save (up to 128 KB of card writes).
pub fn write_sav(root: &Path, platform: Platform, stem: &str, sav: &[u8]) -> std::io::Result<bool> {
    let path = sav_path(root, platform, stem);
    if let Some(old) = read_sav(root, platform, stem) {
        if old == sav {
            return Ok(false);
        }
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    atomic_write(&path, sav)?;
    Ok(true)
}

/// mGBA standalone writes `.sav`, RetroArch's libretro cores write `.srm`. Both are the
/// same battery bytes, so a card carrying either has a real save on it. Only `.sav` is ever
/// written, which makes it the newer of the two whenever both exist.
pub fn read_sav(root: &Path, platform: Platform, stem: &str) -> Option<Vec<u8>> {
    std::fs::read(sav_path(root, platform, stem))
        .or_else(|_| {
            std::fs::read(
                crate::root::saves_dir(root)
                    .join(platform.dir_name())
                    .join(format!("{stem}.srm")),
            )
        })
        .ok()
}

/// The counterpart to the resume write in `flush`. `platform` and `core` are the caller's, the
/// same values handed to `open_core`, so the resume directory and the dylib agree.
pub fn read_resume(root: &Path, platform: Platform, core: Core, stem: &str) -> Option<Vec<u8>> {
    StateRing::new(root, platform, core, stem)
        .read_resume()
        .ok()
        .flatten()
}

fn sav_path(root: &Path, platform: Platform, stem: &str) -> PathBuf {
    crate::root::saves_dir(root)
        .join(platform.dir_name())
        .join(format!("{stem}.sav"))
}
