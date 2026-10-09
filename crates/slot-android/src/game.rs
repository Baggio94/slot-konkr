//! Native libretro mGBA game session. All core calls occur on GLSurfaceView's GL thread.
//! Android's SAF bridge materializes a **bounded private cache** copy of each selected ROM.
use slot_retro::{ButtonMask, LibretroCore, RetroCore};
use slot_store::atomic_write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const SRAM_PERIOD: Duration = Duration::from_secs(30);

pub struct GameSession {
    core: LibretroCore,
    save: PathBuf,
    state: PathBuf,
    last_sram: Instant,
    pub sample_rate: i32,
}

impl GameSession {
    pub fn open(rom: &Path, core_file: &Path, storage: &Path) -> Result<Self, String> {
        if !rom.exists() || !core_file.exists() {
            return Err("ROM cache or mGBA core missing".into());
        }
        let root = storage.join("Saves");
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let id = rom.file_stem().and_then(|v| v.to_str())
            .filter(|v| !v.is_empty() && v.bytes().all(|c| c.is_ascii_hexdigit()))
            .ok_or_else(|| "Unsafe ROM cache file name".to_owned())?;
        let save = root.join(format!("{id}.srm"));
        let state = root.join(format!("{id}.state"));
        let mut core = LibretroCore::open_with(core_file, storage, &root)
            .map_err(|e| e.to_string())?;
        core.load(rom).map_err(|e| e.to_string())?;
        if let Ok(ram) = std::fs::read(&save) {
            if let Err(error) = core.load_save_ram(&ram) {
                eprintln!("slot-konkr: SRAM restore skipped: {error}");
            }
        }
        if let Ok(bytes) = std::fs::read(&state) {
            if let Err(error) = core.unserialize(&bytes) {
                eprintln!("slot-konkr: state resume skipped: {error}");
            }
        }
        let sample_rate = core.av_info().sample_rate.round() as i32;
        Ok(Self {
            core, save, state, last_sram: Instant::now(),
            sample_rate: sample_rate.clamp(8_000, 96_000),
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
            }
        }
    }
}
