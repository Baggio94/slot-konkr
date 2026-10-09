//! Native libretro mGBA / gpSP game session. All core calls occur on GLSurfaceView's GL thread.
//! Android's SAF bridge materializes a **bounded private cache** copy of each selected ROM.
use slot_retro::{ButtonMask, LibretroCore, RetroCore};
use slot_store::{atomic_write, Core};
use crate::retroarch_state;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const SRAM_PERIOD: Duration = Duration::from_secs(30);

pub struct GameSession {
    core: LibretroCore,
    save: PathBuf,
    state: PathBuf,
    retroarch_export: PathBuf,
    retroarch_import: PathBuf,
    last_sram: Instant,
    pub sample_rate: i32,
    pub fps: f64,
}

impl GameSession {
    pub fn open(rom: &Path, core_file: &Path, storage: &Path,
                which: Core, resume_state: bool) -> Result<Self, String> {
        if !rom.exists() || !core_file.exists() {
            return Err(format!("ROM cache or {} core missing", which.text()));
        }
        let root = storage.join("Saves");
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let id = rom.file_stem().and_then(|v| v.to_str())
            .filter(|v| !v.is_empty() && v.bytes().all(|c| c.is_ascii_hexdigit()))
            .ok_or_else(|| "Unsafe ROM cache file name".to_owned())?;
        let save = root.join(format!("{id}.{}.srm", which.as_str()));
        // Save states belong to their specific emulator core. Incompatible
        // mGBA/gpSP states must never be deserialized in the other core.
        // Legacy single-core .state files are still readable by mGBA only.
        let state = root.join(format!("{id}.{}.state", which.as_str()));
        let retroarch_export = root.join(format!("{id}.{}.retroarch-export.state.auto", which.as_str()));
        let retroarch_import = root.join(format!("{id}.{}.retroarch-import.state.auto", which.as_str()));
        // mGBA's libretro core looks for gba_bios.bin / gb_bios.bin /
        // gbc_bios.bin in RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY.
        // Keep the user-selected SAF folder external and read-only: Android
        // has already staged only allowed BIOS files into this private folder.
        let bios = storage.join("BIOS");
        std::fs::create_dir_all(&bios).map_err(|e| e.to_string())?;
        let mut core = LibretroCore::open_with(core_file, &bios, &root)
            .map_err(|e| e.to_string())?;
        match which {
            Core::Mgba => {
                core.set_option("mgba_use_bios", "ON");
                core.set_option("mgba_skip_bios", "OFF");
            }
            Core::Gpsp => {
                core.set_option("gpsp_bios", "auto");
                core.set_option("gpsp_boot_mode", "bios");
            }
        }
        core.load(rom).map_err(|e| e.to_string())?;
        let ram = std::fs::read(&save).or_else(|err| {
            if which == Core::Mgba && err.kind() == std::io::ErrorKind::NotFound {
                // Upgrade existing 0.0.6 single-core test data without loss.
                std::fs::read(root.join(format!("{id}.srm")))
            } else {
                Err(err)
            }
        });
        if let Ok(ram) = ram {
            if let Err(error) = core.load_save_ram(&ram) {
                eprintln!("slot-konkr: SRAM restore skipped: {error}");
            }
        }
        if resume_state {
            // Prefer a successfully decoded RetroArch auto state, then the
            // Slot private state. A compressed/unrecognized external file is
            // kept intact and is NEVER interpreted as raw emulator memory.
            let external = std::fs::read(&retroarch_import)
                .ok().and_then(|bytes| match retroarch_state::decode(&bytes) {
                    Ok(raw) => Some(raw),
                    Err(error) => {
                        eprintln!("slot-konkr: external {} savestate skipped: {error}", which.text());
                        None
                    }
                });
            let local = || std::fs::read(&state).or_else(|err| {
                if which == Core::Mgba && err.kind() == std::io::ErrorKind::NotFound {
                    std::fs::read(root.join(format!("{id}.state")))
                } else {
                    Err(err)
                }
            }).ok();
            if let Some(bytes) = external.or_else(local) {
                if let Err(error) = core.unserialize(&bytes) {
                    eprintln!("slot-konkr: {} state resume skipped: {error}", which.text());
                }
            }
        }
        let sample_rate = core.av_info().sample_rate.round() as i32;
        let fps = core.av_info().fps.clamp(30.0, 120.0);
        Ok(Self {
            core, save, state, retroarch_export, retroarch_import,
            last_sram: Instant::now(),
            sample_rate: sample_rate.clamp(8_000, 96_000),
            fps,
        })
    }

    pub fn advance(&mut self, buttons: u16) {
        self.core.run_frame(ButtonMask(buttons));
        if self.last_sram.elapsed() >= SRAM_PERIOD {
            self.save_sram();
        }
    }

    pub fn frame(&self) -> &[u8] {
        self.core.video_xrgb8888()
    }

    pub fn take_audio(&mut self) -> Vec<i16> {
        self.core.take_audio()
    }

    fn save_sram(&mut self) {
        if let Some(bytes) = self.core.save_ram() {
            if let Err(err) = atomic_write(&self.save, &bytes) {
                eprintln!("slot-konkr: SRAM save failed: {err}");
            }
        }
        self.last_sram = Instant::now();
    }

    pub fn save(&mut self, with_state: bool) {
        self.save_sram();
        if with_state {
            if let Ok(data) = self.core.serialize() {
                if let Err(err) = atomic_write(&self.state, &data) {
                    eprintln!("slot-konkr: state save failed: {err}");
                }
                match retroarch_state::encode(&data) {
                    Ok(container) => if let Err(error) = atomic_write(&self.retroarch_export, &container) {
                        eprintln!("slot-konkr: RetroArch state export failed: {error}");
                    },
                    Err(error) => eprintln!("slot-konkr: RetroArch container error: {error}"),
                }
            }
        }
    }
}
