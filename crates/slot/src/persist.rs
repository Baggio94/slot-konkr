use std::path::{Path, PathBuf};

use slot_store::{atomic_write, read_slot_state, write_slot_state, Core, StateRing, CART_DIR};

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
/// on the next boot.
///
/// `core` is the one `App` resolved at insert, so it cannot disagree with the seated cart.
/// `None` for `state` or `sav` skips a region the live core refused.
pub fn flush(
    root: &Path,
    core: Core,
    stem: &str,
    state: Option<&[u8]>,
    sav: Option<&[u8]>,
) -> std::io::Result<()> {
    if let Some(state) = state {
        StateRing::new(root, core, stem).write_resume(state)?;
    }
    if let Some(sav) = sav {
        write_sav(root, stem, sav)?;
    }
    Ok(())
}

/// Both writes land before the slot is recorded empty, so a power cut leaves a cart that still
/// resumes. The slot is cleared even when a refused region was withheld.
pub fn eject(
    root: &Path,
    core: Core,
    stem: &str,
    state: Option<&[u8]>,
    sav: Option<&[u8]>,
) -> std::io::Result<()> {
    flush(root, core, stem, state, sav)?;
    let mut slot = read_slot_state(root);
    slot.cart = None;
    write_slot_state(root, &slot)
}

/// Skips an unchanged save (up to 128 KB of card writes).
///
/// Refuses to shrink an existing save: a core that disagrees on `RETRO_MEMORY_SAVE_RAM`'s size
/// truncates silently while reporting success. Compared via `read_sav` so an `.srm`-only card is
/// protected too; otherwise the smaller new `.sav` would shadow it.
pub fn write_sav(root: &Path, stem: &str, sav: &[u8]) -> std::io::Result<bool> {
    let path = sav_path(root, stem);
    if let Some(old) = read_sav(root, stem) {
        if old == sav {
            return Ok(false);
        }
        if sav.len() < old.len() {
            eprintln!(
                "slot: save ram: refusing to shrink {} from {} to {} bytes",
                path.display(),
                old.len(),
                sav.len()
            );
            return Ok(false);
        }
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    atomic_write(&path, sav)?;
    Ok(true)
}

/// `.sav` (mGBA standalone) or `.srm` (RetroArch): the same battery bytes. Only `.sav` is
/// written, so it is the newer when both exist.
pub fn read_sav(root: &Path, stem: &str) -> Option<Vec<u8>> {
    std::fs::read(sav_path(root, stem))
        .or_else(|_| {
            std::fs::read(
                crate::root::saves_dir(root)
                    .join(CART_DIR)
                    .join(format!("{stem}.srm")),
            )
        })
        .ok()
}

/// The counterpart to the resume write in `flush`. `core` is resolved once per insert by the
/// caller, so the resume directory and the loaded core cannot disagree.
pub fn read_resume(root: &Path, core: Core, stem: &str) -> Option<Vec<u8>> {
    StateRing::new(root, core, stem)
        .read_resume()
        .ok()
        .flatten()
}

fn sav_path(root: &Path, stem: &str) -> PathBuf {
    crate::root::saves_dir(root)
        .join(CART_DIR)
        .join(format!("{stem}.sav"))
}
