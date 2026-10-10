//! Native libretro mGBA / gpSP game session. All core calls occur on GLSurfaceView's GL thread.
//! Android's SAF bridge materializes a **bounded private cache** copy of each selected ROM.
use slot_retro::{ButtonMask, LibretroCore, RetroCore};
use slot_store::{atomic_write, Core, Platform, StateEntry, StateRing, stamp_now, parse_stamp, format_stamp, RING_MAX};
use crate::thumb;
use crate::retroarch_state;
use crate::rewind::{RewindThread, REWIND_BYTES};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[cfg(target_os = "android")]
#[link(name = "log")]
unsafe extern "C" {
    fn __android_log_write(priority: i32, tag: *const std::ffi::c_char,
        text: *const std::ffi::c_char) -> i32;
}

pub(crate) fn log_launch_timing(message: &str) {
    #[cfg(target_os = "android")]
    if let Ok(text) = std::ffi::CString::new(message) {
        unsafe {
            __android_log_write(4, c"SlotKONKR".as_ptr(), text.as_ptr());
        }
    }
    #[cfg(not(target_os = "android"))]
    eprintln!("SlotKONKR: {message}");
}

const SRAM_PERIOD: Duration = Duration::from_secs(30);
const REWIND_CAPTURE_FRAMES: u8 = 6; // 10 states/second at the original GBA FPS
const UNDO_WINDOW: Duration = Duration::from_secs(30);

enum PendingUndo {
    Saved { stamp: String, evicted: Option<(String, Vec<u8>, Vec<u8>)> },
    Loaded { prior: Vec<u8> },
}


pub struct GameSession {
    core: LibretroCore,
    save: PathBuf,
    rtc: PathBuf,
    manual_export_prefix: PathBuf,
    state: PathBuf,
    retroarch_export: PathBuf,
    retroarch_import: PathBuf,
    ring: StateRing,
    rewind: RewindThread,
    rewind_frames: u8,
    pending_undo: Option<(Instant, PendingUndo)>,
    last_sram: Instant,
    pub sample_rate: i32,
    pub fps: f64,
}

impl GameSession {
    pub fn open(rom: &Path, core_file: &Path, storage: &Path,
                which: Core, platform: Platform, resume_state: bool,
                rom_stem: &str) -> Result<Self, String> {
        let opening_at = Instant::now();
        if !rom.exists() || !core_file.exists() {
            return Err(format!("ROM cache or {} core missing", which.text()));
        }
        let root = storage.join("Saves");
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let id = rom.file_stem().and_then(|v| v.to_str())
            .filter(|v| !v.is_empty() && v.bytes().all(|c| c.is_ascii_hexdigit()))
            .ok_or_else(|| "Unsafe ROM cache file name".to_owned())?;
        // Canonical private layout matches RetroArch's per-core filenames.
        // The opaque ROM URI hash is retained for internal cache / state-ring IDs.
        if rom_stem.is_empty() || rom_stem.chars().count() > 200 ||
            rom_stem == "." || rom_stem == ".." ||
            rom_stem.chars().any(|c| c == '/' || c == '\\' || c.is_control()) {
            return Err("Unsafe ROM save filename".into());
        }
        let core_root = root.join(which.text());
        std::fs::create_dir_all(&core_root).map_err(|e| e.to_string())?;
        let save = core_root.join(format!("{rom_stem}.srm"));
        let rtc = core_root.join(format!("{rom_stem}.rtc"));
        // Save states belong to their specific emulator core. Incompatible
        // mGBA/gpSP states must never be deserialized in the other core.
        // Legacy single-core .state files are still readable by mGBA only.
        let state = root.join(format!("{id}.{}.state", which.as_str()));
        let retroarch_export = root.join(format!("{id}.{}.retroarch-export.state.auto", which.as_str()));
        let retroarch_import = root.join(format!("{id}.{}.retroarch-import.state.auto", which.as_str()));
        let manual_dir = root.join("ManualExports");
        std::fs::create_dir_all(&manual_dir).map_err(|e| e.to_string())?;
        let manual_export_prefix = manual_dir.join(format!("{id}.{}", which.as_str()));
        let ring = StateRing::new(storage, platform, which, id);
        // mGBA's libretro core looks for gba_bios.bin / gb_bios.bin /
        // gbc_bios.bin in RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY.
        // Keep the user-selected SAF folder external and read-only: Android
        // has already staged only allowed BIOS files into this private folder.
        let bios = storage.join("BIOS");
        std::fs::create_dir_all(&bios).map_err(|e| e.to_string())?;
        let mut core = LibretroCore::open_with(core_file, &bios, &core_root)
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
        let load_ms = opening_at.elapsed().as_millis();
        let restore_started = Instant::now();
        let ram = std::fs::read(&save).or_else(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                // Migrate the pre-0.0.7 hashed cache non-destructively.
                std::fs::read(root.join(format!("{id}.{}.srm", which.as_str())))
                    .or_else(|_| {
                        if which == Core::Mgba {
                            std::fs::read(root.join(format!("{id}.srm")))
                        } else { Err(err) }
                    })
            } else { Err(err) }
        });
        if let Ok(ram) = ram {
            if let Err(error) = core.load_save_ram(&ram) {
                eprintln!("slot-konkr: SRAM restore skipped: {error}");
            }
        }
        if let Ok(bytes) = std::fs::read(&rtc) {
            if let Err(error) = core.load_rtc(&bytes) {
                eprintln!("slot-konkr: RTC restore skipped: {error}");
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
        log_launch_timing(&format!(
            "Core init + ROM: {load_ms}ms, SRAM/RTC/state restore: {}ms, resume={resume_state}, total: {}ms",
            restore_started.elapsed().as_millis(), opening_at.elapsed().as_millis()
        ));
        let sample_rate = core.av_info().sample_rate.round() as i32;
        let fps = core.av_info().fps.clamp(30.0, 120.0);
        Ok(Self {
            core, save, rtc, manual_export_prefix, state, retroarch_export, retroarch_import, ring,
            rewind: RewindThread::spawn(REWIND_BYTES), rewind_frames: 0,
            pending_undo: None, last_sram: Instant::now(),
            sample_rate: sample_rate.clamp(8_000, 96_000),
            fps,
        })
    }

    pub fn advance(&mut self, buttons: u16) {
        self.core.run_frame(ButtonMask(buttons));
        self.rewind_frames += 1;
        if self.rewind_frames >= REWIND_CAPTURE_FRAMES {
            self.rewind_frames = 0;
            // Original Slot LZ4/XOR ring with 20 MiB bounded history.
            // Snapshots are private in memory and NEVER exported to RetroArch.
            if let Ok(snapshot) = self.core.serialize() {
                self.rewind.push(snapshot);
            }
        }
        if self.last_sram.elapsed() >= SRAM_PERIOD {
            self.save_sram();
        }
    }

    pub fn rewind_step(&mut self) -> bool {
        let Some(previous) = self.rewind.pop() else { return false; };
        if self.core.unserialize(&previous).is_err() { return false; }
        // Most libretro cores only refresh video after retro_run. Run a single
        // input-free frame so the renderer shows the restored moment.
        self.core.run_frame(ButtonMask(0));
        let _ = self.core.take_audio();
        self.rewind_frames = 0;
        true
    }

    pub fn rewind_fill(&self) -> u8 {
        self.rewind.fill()
    }

    pub fn frame(&self) -> &[u8] {
        self.core.video_xrgb8888()
    }

    pub fn take_audio(&mut self) -> Vec<i16> {
        self.core.take_audio()
    }

    fn save_sram(&mut self) -> bool {
        let success = match self.core.save_ram() {
            Some(bytes) => match atomic_write(&self.save, &bytes) {
                Ok(()) => true,
                Err(err) => {
                    eprintln!("slot-konkr: SRAM save failed: {err}");
                    false
                }
            },
            None => false,
        };
        // RTC may exist independently of SRAM on GB/GBC cartridges.
        let rtc_saved = self.core.save_rtc().is_some_and(|bytes|
            match atomic_write(&self.rtc, &bytes) {
                Ok(()) => true,
                Err(err) => {
                    eprintln!("slot-konkr: RTC save failed: {err}");
                    false
                }
            });
        self.last_sram = Instant::now();
        success || rtc_saved
    }

    pub fn save(&mut self, with_state: bool) -> bool {
        let saved_sram = self.save_sram();
        let mut state_written = false;
        if with_state {
            if let Ok(data) = self.core.serialize() {
                if let Err(err) = atomic_write(&self.state, &data) {
                    eprintln!("slot-konkr: state save failed: {err}");
                }
                match retroarch_state::encode(&data) {
                    Ok(container) => match atomic_write(&self.retroarch_export, &container) {
                        Ok(()) => state_written = true,
                        Err(error) => eprintln!("slot-konkr: RetroArch state export failed: {error}"),
                    },
                    Err(error) => eprintln!("slot-konkr: RetroArch container error: {error}"),
                }
            }
        }
        saved_sram || state_written
    }
    /// Original Slot ring: 10 timestamped states per ROM/core, each with
    /// a 240×160 PNG thumbnail. Independent from RetroArch's manual slots.
    pub fn save_manual(&mut self) -> Result<String, String> {
        let state = self.core.serialize().map_err(|e|e.to_string())?;
        let preview = thumb::png(self.core.video_xrgb8888()).unwrap_or_default();
        let entries = self.ring.list().map_err(|e|e.to_string())?;
        let mut seconds = slot_store::parse_stamp(&stamp_now()).ok_or("Invalid system clock")?;
        let mut stamp = format_stamp(seconds);
        while entries.iter().any(|e|e.stamp==stamp) {
            seconds+=1;
            stamp=format_stamp(seconds);
        }
        let evicted = if entries.len()>=RING_MAX {
            entries.last().and_then(|item|
                self.ring.read(&item.stamp).ok()
                    .map(|(state, thumb)|(item.stamp.clone(),state,thumb)))
        } else {None};
        self.ring.push(&state, &preview, &stamp).map_err(|e|e.to_string())?;
        self.pending_undo = Some((Instant::now(), PendingUndo::Saved {
            stamp: stamp.clone(), evicted
        }));
        // Keep Slot's Polaroid history private. The Android storage worker
        // publishes a distinct, verified RetroArch .stateN after this returns.
        let encoded = retroarch_state::encode(&state)?;
        let export = std::path::PathBuf::from(format!(
            "{}.{}.rastate", self.manual_export_prefix.display(), stamp
        ));
        atomic_write(&export, &encoded).map_err(|error|
            format!("Polaroid saved, manual RetroArch export failed: {error}"))?;
        Ok(stamp)
    }

    pub fn history(&self) -> Vec<StateEntry> {
        self.ring.list().unwrap_or_default()
    }

    pub fn load_manual(&mut self, stamp:&str) -> Result<(), String> {
        let (bytes, _) = self.ring.read(stamp).map_err(|e|e.to_string())?;
        let prior = self.core.serialize().map_err(|e|e.to_string())?;
        self.core.unserialize(&bytes).map_err(|e|e.to_string())?;
        self.pending_undo=Some((Instant::now(), PendingUndo::Loaded {prior}));
        Ok(())
    }

    pub fn load_latest(&mut self)->Result<(),String> {
        let newest=self.history().into_iter().next()
            .ok_or_else(||"No saved states yet".to_owned())?;
        self.load_manual(&newest.stamp)
    }

    pub fn delete_manual(&mut self, stamp:&str)->Result<(),String> {
        self.ring.remove(stamp).map_err(|e|e.to_string())
    }

    pub fn undo_manual(&mut self)->Result<(),String> {
        let Some((since,_))=&self.pending_undo else { return Err("Nothing to undo".into())};
        if since.elapsed()>UNDO_WINDOW {
            self.pending_undo=None;
            return Err("Undo expired (30 seconds)".into());
        }
        let (_, pending)=self.pending_undo.take().unwrap();
        match pending {
            PendingUndo::Loaded {prior} =>
                self.core.unserialize(&prior).map_err(|e|e.to_string()),
            PendingUndo::Saved {stamp,evicted} => {
                self.ring.remove(&stamp).map_err(|e|e.to_string())?;
                if let Some((name,state,thumb))=evicted {
                    self.ring.push(&state,&thumb,&name).map_err(|e|e.to_string())?;
                }
                Ok(())
            }
        }
    }

    pub fn undo_available(&self)->bool {
        self.pending_undo.as_ref().is_some_and(|(at,_)|at.elapsed()<=UNDO_WINDOW)
    }

}
