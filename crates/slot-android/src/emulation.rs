//! Per-cartridge opt-in latency profiles. Never touch the ROM or RetroArch saves.
use std::{fs, io, path::{Path, PathBuf}};
use serde::{Deserialize, Serialize};
use crate::{core_selection, settings::{self, Settings}};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EmulationProfile {
    pub show_fps: bool,
    pub responsive_pacing: bool,
    pub display_sync_pacing: bool,
    pub run_ahead_frames: u8,
    pub frame_delay_ms: u8,
    pub gpu_sync: bool,
    pub audio_latency_ms: u16,
}
impl Default for EmulationProfile {
    fn default() -> Self {
        Self { show_fps: false, responsive_pacing: false,
            display_sync_pacing: false,
            run_ahead_frames: 0, frame_delay_ms: 0,
            gpu_sync: false, audio_latency_ms: 0 }
    }
}
impl EmulationProfile {
    pub fn from_settings(settings: &Settings) -> Self {
        Self { show_fps: settings.show_fps,
            responsive_pacing: settings.responsive_pacing,
            display_sync_pacing: settings.display_sync_pacing,
            run_ahead_frames: settings.run_ahead_frames,
            frame_delay_ms: settings.frame_delay_ms,
            gpu_sync: settings.gpu_sync,
            audio_latency_ms: settings.audio_latency_ms }
    }
    pub fn sanitize(mut self) -> Self {
        if !self.responsive_pacing { self.display_sync_pacing = false; }
        if self.run_ahead_frames > 2 { self.run_ahead_frames = 0; }
        if self.frame_delay_ms > 2 { self.frame_delay_ms = 0; }
        if ![0, 32, 48, 64, 96].contains(&self.audio_latency_ms) {
            self.audio_latency_ms = 0;
        }
        self
    }
    pub fn change(&mut self, key: usize, right: bool) -> bool {
        let before = *self;
        match key {
            10 => self.show_fps = !self.show_fps,
            11 => settings::cycle_pacing(&mut self.responsive_pacing,
                &mut self.display_sync_pacing, right),
            12 => self.run_ahead_frames =
                settings::cycle_u8(self.run_ahead_frames, 2, right),
            13 => self.frame_delay_ms =
                settings::cycle_u8(self.frame_delay_ms, 2, right),
            14 => self.gpu_sync = !self.gpu_sync,
            15 => self.audio_latency_ms =
                settings::cycle_audio(self.audio_latency_ms, right),
            _ => return false,
        }
        *self != before
    }
    fn path(root: &Path, uri: &str) -> PathBuf {
        root.join("GameEmulation").join(format!("{}.json", core_selection::key(uri)))
    }
    /// Missing or invalid profile means inherit the current global defaults.
    pub fn load(root: &Path, uri: &str) -> Option<Self> {
        let bytes = fs::read(Self::path(root, uri)).ok()?;
        serde_json::from_slice::<Self>(&bytes).ok().map(Self::sanitize)
    }
    pub fn save(self, root: &Path, uri: &str) -> io::Result<()> {
        let path = Self::path(root, uri);
        fs::create_dir_all(path.parent().expect("app-owned profile path"))?;
        let tmp = path.with_extension("json.partial");
        let data = serde_json::to_vec_pretty(&self.sanitize()).map_err(io::Error::other)?;
        let result = fs::write(&tmp, data).and_then(|_| fs::rename(&tmp, path));
        if result.is_err() { let _ = fs::remove_file(&tmp); }
        result
    }
    pub fn clear(root: &Path, uri: &str) -> io::Result<()> {
        match fs::remove_file(Self::path(root, uri)) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_globals_inherited_until_game_is_customized() {
        let mut settings = Settings::default();
        settings.responsive_pacing = true;
        let mut profile = EmulationProfile::from_settings(&settings);
        assert!(profile.responsive_pacing);
        assert!(profile.change(12, true));
        assert_eq!(profile.run_ahead_frames, 1);
        assert!(profile.change(12, true));
        assert_eq!(profile.run_ahead_frames, 2);
        assert!(profile.change(12, true));
        assert_eq!(profile.run_ahead_frames, 0);
        assert_eq!(EmulationProfile::default().audio_latency_ms, 0);
    }

    #[test]
    fn pre_beta6_profile_inherits_original_responsive_mode() {
        let old: EmulationProfile =
            serde_json::from_str(r#"{"responsive_pacing":true,"run_ahead_frames":1}"#).unwrap();
        assert!(old.responsive_pacing);
        assert!(!old.display_sync_pacing);
        assert_eq!(old.run_ahead_frames, 1);
        let mut changed = old;
        assert!(changed.change(11, true));
        assert!(changed.display_sync_pacing);
        assert!(changed.change(11, true));
        assert!(!changed.responsive_pacing && !changed.display_sync_pacing);
    }

    #[test]
    fn profiles_are_isolated_by_cartridge_uri_and_reset_cleanly() {
        let root = std::env::temp_dir().join(format!(
            "slot-konkr-emulation-{}", std::process::id()));
        let a = "content://game/A";
        let b = "content://game/B";
        let mut profile = EmulationProfile::default();
        profile.run_ahead_frames = 1;
        profile.audio_latency_ms = 32;
        profile.save(&root, a).unwrap();
        assert_eq!(EmulationProfile::load(&root, a), Some(profile));
        assert_eq!(EmulationProfile::load(&root, b), None);
        EmulationProfile::clear(&root, a).unwrap();
        assert_eq!(EmulationProfile::load(&root, a), None);
        let _ = fs::remove_dir_all(root);
    }
}
