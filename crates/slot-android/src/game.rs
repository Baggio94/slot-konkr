//! Native libretro mGBA / gpSP game session. All core calls occur on GLSurfaceView's GL thread.
//! Android's SAF bridge materializes a **bounded private cache** copy of each selected ROM.
use slot_retro::{ButtonMask, LibretroCore, RetroCore, LinkChannel};
use crate::link_kind::LinkKind;
use crate::link_play::{Frame as LinkFrame, LinkError, LinkPlay};
use crate::multiplayer::Role;
use slot_store::{atomic_write, Core, Platform, StateEntry, StateRing, stamp_now, parse_stamp, format_stamp, RING_MAX};
use crate::thumb;
use crate::retroarch_state;
use crate::rewind::{RewindThread, REWIND_BYTES};
use std::path::{Path, PathBuf};
use std::io::Read;
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
    // Wrapped with the native softcore evaluator for opted-in GBA sessions.
    core: Box<dyn RetroCore>,
    // Optional: only used after the real network handshake, never in solo play.
    link: Option<LinkPlay>,
    // A linked mGBA state may contain TWO cores, incompatible with solo
    // RetroArch .state files. Never overwrite solo timeline after link use.
    link_touched: bool,
    save: PathBuf,
    rtc: PathBuf,
    manual_export_prefix: PathBuf,
    state: PathBuf,
    retroarch_export: PathBuf,
    retroarch_import: PathBuf,
    ring: StateRing,
    rewind: RewindThread,
    rewind_frames: u8,
    frame_count: u32,
    // Presentation-only pixels from speculative frames; save/RA keep the real state.
    runahead_video: Vec<u8>,
    // Preserve the libretro state allocation across speculative frames.
    runahead_state: Vec<u8>,
    runahead_unavailable: bool,
    palette_live: bool,
    core_kind: Core,
    pending_undo: Option<(Instant, PendingUndo)>,
    last_sram: Instant,
    pub sample_rate: i32,
    pub fps: f64,
}

/// Preserve original Slot turbo in TURBO mode; in A/B mode Y acts as A
/// and X as B, *in addition to* both original buttons. Applied equally to
/// real and speculative Run-Ahead frames so input prediction never diverges.
fn xy_buttons(buttons: ButtonMask, turbo: bool, frame: u32) -> ButtonMask {
    if turbo { return buttons.turbo(frame); }
    let mut mapped = buttons.without_turbo().0;
    if buttons.0 & ButtonMask::Y != 0 { mapped |= ButtonMask::A; }
    if buttons.0 & ButtonMask::X != 0 { mapped |= ButtonMask::B; }
    ButtonMask(mapped)
}

impl GameSession {
    pub fn open(rom: &Path, core_file: &Path, storage: &Path,
                which: Core, platform: Platform, resume_state: bool,
                rom_stem: &str, gb_palette: Option<&str>,
                colour_correction: bool,
                achievements: Option<&slot_achievements::Service>) -> Result<Self, String> {
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
        // Exact upstream Slot libretro options for colour and Game Boy palettes.
        // Read only the 336-byte header: never read the entire cached ROM.
        let dmg_only = platform != Platform::Gba && which == Core::Mgba
            && std::fs::File::open(rom).and_then(|mut file| {
                let mut header = [0u8; 0x150];
                file.read_exact(&mut header)?;
                Ok(header[0x143] & 0x80 == 0)
            }).unwrap_or(false);
        let palette_live = dmg_only && gb_palette.is_some();
        match which {
            Core::Mgba => {
                core.set_option("mgba_use_bios", "ON");
                core.set_option("mgba_skip_bios", "OFF");
                core.set_option("mgba_sgb_borders", "OFF");
                core.set_option("mgba_force_gbp", "ON");
                if let Some(name) = gb_palette.filter(|_| dmg_only) {
                    core.set_option("mgba_gb_model", "Game Boy");
                    core.set_option("mgba_gb_colors_preset", "0");
                    core.set_option("mgba_gb_colors", name);
                } else {
                    core.set_option("mgba_gb_model", "Autodetect");
                    core.set_option("mgba_gb_colors_preset", "1");
                    core.set_option("mgba_gb_colors", "GBC Dark Green →A");
                }
                core.set_option("mgba_color_correction",
                    if colour_correction { "Auto" } else { "OFF" });
            }
            Core::Gpsp => {
                core.set_option("gpsp_bios", "auto");
                core.set_option("gpsp_boot_mode", "bios");
                core.set_option("gpsp_color_correction",
                    if colour_correction { "enabled" } else { "disabled" });
            }
        }
        // Apply libretro core options BEFORE wrapping. The native rcheevos
        // worker observes Load/Frame/Unload but never holds emulator pointers.
        // GB/GBC need their own verified RA hash and RAM maps, so they are
        // explicitly excluded from this initial GBA-only milestone.
        let mut core: Box<dyn RetroCore> = match (platform, achievements) {
            (Platform::Gba, Some(service)) => service.wrap(Box::new(core)),
            _ => Box::new(core),
        };
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
            core, link: None, link_touched: false, save, rtc, manual_export_prefix, state, retroarch_export, retroarch_import, ring,
            rewind: RewindThread::spawn(REWIND_BYTES), rewind_frames: 0,
            frame_count: 0, palette_live, core_kind: which,
            runahead_video: Vec::new(), runahead_state: Vec::new(),
            runahead_unavailable: false,
            pending_undo: None, last_sram: Instant::now(),
            sample_rate: sample_rate.clamp(8_000, 96_000),
            fps,
        })
    }

    /// False while cable lockstep is awaiting the other player. The caller
    /// must not subtract an emulated-frame budget until this returns true.
    pub fn advance(&mut self, buttons: u16, turbo: bool, record_rewind: bool) -> bool {
        self.runahead_video.clear();
        let pressed = ButtonMask(buttons);
        let mapped = xy_buttons(pressed, turbo, self.frame_count);
        if let Some(link) = self.link.as_mut() {
            match link.frame(self.core.as_mut(), mapped) {
                LinkFrame::Advanced => {},
                LinkFrame::Waiting | LinkFrame::Failed(_) => return false,
            }
        } else {
            self.core.run_frame(mapped);
        }
        self.frame_count = self.frame_count.wrapping_add(1);
        if record_rewind && !self.link_touched {
            self.rewind_frames += 1;
            if self.rewind_frames >= REWIND_CAPTURE_FRAMES {
                self.rewind_frames = 0;
                // Original XOR/LZ4 history; never export it to RetroArch.
                if let Ok(snapshot) = self.core.serialize() {
                    self.rewind.push(snapshot);
                }
            }
        } else { self.rewind_frames = 0; }
        if self.last_sram.elapsed() >= SRAM_PERIOD {
            self.save_sram();
        }
        true
    }

    /// The pinned mGBA libretro core is SINGLE-PLAYER (it ignores controller
    /// port 1 and never registers a netpacket interface). gpSP is the ONLY
    /// installed core with genuine GBA serial/RFU netpacket callbacks. Both
    /// Cable and Wireless therefore use gpSP's network interface, never the
    /// simulated two-input single-core path used by original Slot tests.
    pub fn begin_multiplayer(&mut self, role: Role, mode: LinkKind,
                             transport: Box<dyn LinkChannel>) -> Result<(), String> {
        if self.link.is_some() { return Err("Already linked".into()); }
        if self.link_touched {
            return Err("Relaunch cartridge before another link".into());
        }
        if self.core_kind != Core::Gpsp {
            return Err("Multiplayer requires gpSP: this mGBA libretro build has no GBA Link interface".into());
        }
        if !self.core.supports_netpacket() {
            return Err("gpSP did not register GBA serial/RFU netpacket callbacks".into());
        }
        // The auto-detected serial mode must be decided when the cartridge is
        // loaded. Never attempt to switch RFU to Cable on a running gpSP core.
        let _ = mode;
        let link = LinkPlay::begin(self.core.as_mut(), role, mode, transport)?;
        self.link = Some(link);
        self.link_touched = true;
        self.runahead_video.clear();
        self.rewind_frames = 0;
        self.pending_undo = None;
        Ok(())
    }

    pub fn link_fault(&self) -> Option<LinkError> {
        self.link.as_ref().and_then(|link| link.fault())
    }

    pub fn linked(&self) -> bool { self.link.is_some() }

    /// An ended link must never become a local solo timeline. Eject and
    /// reopen the ROM to reinitialize both save and emulation state safely.
    pub fn requires_relaunch(&self) -> bool {
        self.link_touched && self.link.is_none()
    }

    pub fn core_kind(&self) -> Core { self.core_kind }

    pub fn wireless_capable(&self) -> bool {
        self.core_kind == Core::Gpsp && self.core.supports_netpacket()
    }

    pub fn end_multiplayer(&mut self) {
        if let Some(mut link) = self.link.take() {
            link.end(self.core.as_mut());
        }
        self.runahead_video.clear();
        self.rewind_frames = 0;
    }

    pub fn runahead_supported(&self) -> bool {
        !self.link_touched && self.link.is_none() && !self.runahead_unavailable && self.core.speculative_audio_supported()
    }

    /// Roll back both standard libretro state and core-specific transient
    /// audio: gpSP's sound ring, or mGBA's converter/filter/queue sidecar.
    pub fn prepare_runahead(&mut self, buttons: u16, turbo: bool, frames: u8) {
        self.runahead_video.clear();
        if frames == 0 || frames > 2 || self.runahead_unavailable || self.link_touched { return; }
        if !self.core.speculative_audio_supported() {
            self.runahead_unavailable = true;
            log_launch_timing("Run-Ahead disabled: core has no safe speculative audio support");
            return;
        }
        if let Err(error) = self.core.serialize_into(&mut self.runahead_state) {
            self.runahead_unavailable = true;
            log_launch_timing(&format!("Run-Ahead disabled: serialize failed: {error}"));
            return;
        }
        if let Err(error) = self.core.capture_speculative_audio() {
            self.runahead_unavailable = true;
            log_launch_timing(&format!("Run-Ahead disabled: audio snapshot failed: {error}"));
            return;
        }
        let pressed = ButtonMask(buttons);
        let mapped = xy_buttons(pressed, turbo, self.frame_count);
        for _ in 0..frames {
            self.core.run_speculative_frame(mapped);
            // Discard predicted audio without reallocating libretro's buffer.
            self.core.discard_audio();
        }
        self.runahead_video.extend_from_slice(self.core.video_xrgb8888());
        let restored = self.core.restore_speculative_state(&self.runahead_state)
            .and_then(|_| self.core.restore_speculative_audio());
        if let Err(error) = restored {
            // Never display a frame if the core could not restore its real state.
            self.runahead_video.clear();
            self.runahead_unavailable = true;
            log_launch_timing(&format!("Run-Ahead disabled: restore failed: {error}"));
            return;
        }

    }

    pub fn rewind_step(&mut self) -> bool {
        if self.link_touched { return false; }
        self.runahead_video.clear();
        let Some(previous) = self.rewind.pop() else { return false; };
        if self.core.unserialize(&previous).is_err() { return false; }
        // Most libretro cores only refresh video after retro_run. Run a single
        // input-free frame so the renderer shows the restored moment.
        self.core.run_frame(ButtonMask(0));
        let _ = self.core.take_audio();
        self.rewind_frames = 0;
        true
    }

    pub fn can_change_gb_palette(&self) -> bool {
        self.palette_live
    }

    pub fn set_gb_palette(&mut self, name: &str) {
        if self.palette_live {
            self.core.set_option("mgba_gb_colors_preset", "0");
            self.core.set_option("mgba_gb_colors", name);
        }
    }

    pub fn set_colour_correction(&mut self, enabled: bool) {
        let (key, value) = match self.core_kind {
            Core::Mgba => ("mgba_color_correction",
                if enabled { "Auto" } else { "OFF" }),
            Core::Gpsp => ("gpsp_color_correction",
                if enabled { "enabled" } else { "disabled" }),
        };
        self.core.set_option(key, value);
    }

    pub fn rumble_strength(&self) -> u16 {
        self.core.rumble().strength()
    }

    pub fn rewind_fill(&self) -> u8 {
        self.rewind.fill()
    }

    pub fn frame(&self) -> &[u8] {
        if !self.runahead_video.is_empty() { &self.runahead_video }
        else { self.core.video_xrgb8888() }
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
        // Exporting a single-core state during linked play would silently
        // corrupt the shared timeline. SRAM/RTC can still be saved safely.
        if with_state && !self.link_touched {
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
        if self.link_touched { return Err("Save States unavailable after linked play; relaunch cartridge".into()); }
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
        if self.link.is_some() { return Err("Load State unavailable during Multiplayer".into()); }
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
        if self.link.is_some() { return Err("Undo unavailable during Multiplayer".into()); }
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

impl Drop for GameSession {
    fn drop(&mut self) {
        // Ejection, Android lifecycle teardown and failed launches must tell
        // the remote console the link ended, not leave an orphaned TCP session.
        self.end_multiplayer();
    }
}

#[cfg(test)]
mod xy_mapping_tests {
    use super::*;
    #[test]
    fn classic_turbo_keeps_the_original_slot_x_y_behavior() {
        for frame in 0..12 {
            let buttons = ButtonMask(ButtonMask::X | ButtonMask::Y);
            assert_eq!(xy_buttons(buttons, true, frame), buttons.turbo(frame));
        }
    }
    #[test]
    fn ab_mode_maps_y_to_a_and_x_to_b_without_losing_original_buttons() {
        let y = xy_buttons(ButtonMask(ButtonMask::Y), false, 0);
        let x = xy_buttons(ButtonMask(ButtonMask::X), false, 3);
        assert_eq!(y.0, ButtonMask::A);
        assert_eq!(x.0, ButtonMask::B);
        let both = xy_buttons(ButtonMask(ButtonMask::A | ButtonMask::B |
            ButtonMask::X | ButtonMask::Y | ButtonMask::RIGHT), false, 5);
        assert_eq!(both.0, ButtonMask::A | ButtonMask::B | ButtonMask::RIGHT);
    }
}
