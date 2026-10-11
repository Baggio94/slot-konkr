use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};
use slot_gfx::ScreenEffect;
use slot_store::GbPalette;

pub const SPEEDS: [u8; 4] = [2, 3, 4, 6];
const NAME: &str = "slot-konkr-settings.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shader { Off, Lcd3x, Grid, Dot, Simpletex }
impl Shader {
    pub const GBA: [Self; 4] = [Self::Off, Self::Lcd3x, Self::Grid, Self::Dot];
    pub const GB: [Self; 3] = [Self::Off, Self::Grid, Self::Simpletex];
    pub fn effect(self) -> ScreenEffect {
        match self { Self::Off => ScreenEffect::None,
            Self::Lcd3x => ScreenEffect::Lcd3x, Self::Grid => ScreenEffect::Grid,
            Self::Dot => ScreenEffect::Dot, Self::Simpletex => ScreenEffect::Simpletex }
    }
    fn step(self, values: &[Self], right: bool) -> Self {
        let at = values.iter().position(|v| *v == self).unwrap_or(0);
        values[if right { (at + 1) % values.len() }
               else { (at + values.len() - 1) % values.len() }]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub ff_speed: u8, pub ff_sound: bool, pub colour_correction: bool,
    pub rumble: bool, pub shader_gba: Shader, pub shader_gb: Shader,
    pub gb_palettes: bool, pub gb_palette: u8, pub rewind: bool,
    pub turbo: bool, pub eject_save: bool,
    // Android-only opt-in diagnostics; preserving original Slot defaults.
    pub show_fps: bool, pub responsive_pacing: bool,
    pub run_ahead_frames: u8, pub frame_delay_ms: u8,
    pub gpu_sync: bool, pub audio_latency_ms: u16,
}
impl Default for Settings {
    fn default() -> Self {
        // Dev13 settings survive upgrades. Do not change established defaults.
        Self { ff_speed: 2, ff_sound: true, colour_correction: false,
            rumble: true, shader_gba: Shader::Off, shader_gb: Shader::Off,
            gb_palettes: false, gb_palette: 1, rewind: true,
            turbo: true, eject_save: true,
            show_fps: false, responsive_pacing: false,
            run_ahead_frames: 0, frame_delay_ms: 0,
            gpu_sync: false, audio_latency_ms: 0 }
    }
}
impl Settings {
    pub fn load(dir: &Path) -> Self {
        fs::read(dir.join(NAME)).ok()
            .and_then(|b| serde_json::from_slice::<Self>(&b).ok())
            .map(|mut s| {
                if !SPEEDS.contains(&s.ff_speed) { s.ff_speed = 2; }
                if !Shader::GBA.contains(&s.shader_gba) { s.shader_gba = Shader::Off; }
                if !Shader::GB.contains(&s.shader_gb) { s.shader_gb = Shader::Off; }
                if s.gb_palette >= 48 { s.gb_palette = 1; }
                if s.run_ahead_frames > 2 { s.run_ahead_frames = 0; }
                if s.frame_delay_ms > 2 { s.frame_delay_ms = 0; }
                if ![0, 32, 48, 64, 96].contains(&s.audio_latency_ms) { s.audio_latency_ms = 0; }
                s
            }).unwrap_or_default()
    }
    pub fn save(self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        let target = dir.join(NAME);
        let temporary = dir.join(format!("{NAME}.partial"));
        let bytes = serde_json::to_vec_pretty(&self).map_err(io::Error::other)?;
        let write = (|| -> io::Result<()> {
            fs::write(&temporary, bytes)?;
            fs::rename(&temporary, target)
        })();
        if write.is_err() { let _ = fs::remove_file(temporary); }
        write
    }
    // Screen: keys 0..3; Gameplay: keys 4..9.
    // Faithful to Slot quick_menu.rs except Date & Time (owned by Android).
    pub fn change(&mut self, key: usize, right: bool) -> bool {
        let before = *self;
        match key {
            0 => self.shader_gba = self.shader_gba.step(&Shader::GBA, right),
            1 => self.shader_gb = self.shader_gb.step(&Shader::GB, right),
            2 => self.colour_correction = !self.colour_correction,
            3 => self.gb_palettes = !self.gb_palettes,
            4 => {
                let at = SPEEDS.iter().position(|x| *x == self.ff_speed).unwrap_or(0);
                self.ff_speed = SPEEDS[if right { (at + 1) % 4 } else { (at + 3) % 4 }];
            }
            5 => self.ff_sound = !self.ff_sound,
            6 => self.rewind = !self.rewind,
            7 => self.turbo = !self.turbo,
            8 => self.rumble = !self.rumble,
            9 => self.eject_save = !self.eject_save,
            10 => self.show_fps = !self.show_fps,
            11 => self.responsive_pacing = !self.responsive_pacing,
            12 => self.run_ahead_frames = cycle_u8(self.run_ahead_frames, 2, right),
            13 => self.frame_delay_ms = cycle_u8(self.frame_delay_ms, 2, right),
            14 => self.gpu_sync = !self.gpu_sync,
            15 => self.audio_latency_ms = cycle_audio(self.audio_latency_ms, right),
            _ => return false,
        }
        *self != before
    }
    pub fn palette_name(self) -> &'static str {
        GbPalette::all().nth(self.gb_palette as usize)
            .unwrap_or(GbPalette::DEFAULT).core_name()
    }
}
pub fn cycle_u8(value: u8, max: u8, right: bool) -> u8 {
    if right { if value >= max { 0 } else { value + 1 } }
    else if value == 0 || value > max { max } else { value - 1 }
}
pub fn cycle_audio(value: u16, right: bool) -> u16 {
    let modes = [0, 32, 48, 64, 96];
    let index = modes.iter().position(|x| *x == value).unwrap_or(0);
    modes[if right { (index + 1) % modes.len() }
          else { (index + modes.len() - 1) % modes.len() }]
}

/// Model of the frame-debt clamp. STANDARD retains RC3 behavior.
/// RESPONSIVE keeps no more than 2.5 frames of debt so a skipped vsync
/// can be recovered with at most one additional libretro frame. A strict
/// one-frame cap permanently under-ran on displays a little below 60 Hz.
pub fn frame_debt(prev: f64, elapsed: f64, speed: f64, period: f64,
                  fast: bool, responsive: bool) -> f64 {
    let max = if fast { 0.40 }
        else if responsive { period * 2.5 }
        else { 0.10 };
    (prev + elapsed * speed).min(max)
}

/// Normally render one simulation frame per display frame in RESPONSIVE.
/// Permit one *occasional* catch-up only when a whole second frame is due.
/// Unlike STANDARD this can never issue 3-4 delayed simulation frames.
pub fn responsive_frame_budget(debt: f64, period: f64) -> usize {
    if debt >= period * 2.0 { 2 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn responsive_pacing_rejects_backlog_without_touching_standard() {
        let frame = 1.0 / 59.7275;
        assert!((frame_debt(0.0, 0.05, 1.0, frame, false, false) - 0.05).abs() < 1e-9);
        assert!((frame_debt(0.0, 0.05, 1.0, frame, false, true) - frame * 2.5).abs() < 1e-9);
        assert!((frame_debt(0.0, 0.35, 2.0, frame, true, true) - 0.40).abs() < 1e-9);
    }

    #[test]
    fn responsive_recovers_single_missed_vsync_but_never_floods_core() {
        let period = 1.0 / 59.7275;
        assert_eq!(responsive_frame_budget(period * 1.99, period), 1);
        assert_eq!(responsive_frame_budget(period * 2.01, period), 2);
        assert_eq!(responsive_frame_budget(period * 9.0, period), 2);
        let mut debt = 0.0;
        let mut emulated = 0;
        // 59.1Hz panel pacing must not slow a ~59.73Hz GBA forever.
        for _ in 0..5910 {
            debt = frame_debt(debt, 1.0 / 59.1, 1.0, period, false, true);
            let frames = responsive_frame_budget(debt, period);
            for _ in 0..frames {
                if debt < period { break; }
                debt -= period;
                emulated += 1;
            }
        }
        assert!((emulated as i32 - 5973).abs() <= 2);
    }

    #[test]
    fn defaults_preserve_dev13() {
        let s = Settings::default();
        assert_eq!(s.ff_speed, 2);
        assert!(s.ff_sound && s.rumble && s.rewind && s.turbo && s.eject_save);
        assert!(!s.gb_palettes);
        assert_eq!(s.shader_gba, Shader::Off);
    }
    #[test]
    fn screen_gameplay_original_choices_cycle() {
        let mut s = Settings::default();
        s.change(0, false); assert_eq!(s.shader_gba, Shader::Dot);
        s.change(1, false); assert_eq!(s.shader_gb, Shader::Simpletex);
        s.change(4, false); assert_eq!(s.ff_speed, 6);
        s.change(4, true); assert_eq!(s.ff_speed, 2);
        for i in [2, 3, 5, 6, 7, 8, 9] { assert!(s.change(i, true)); }
        assert!(!s.change(16, true));
        assert!(!s.rewind && !s.turbo && !s.eject_save);
        assert!(s.change(10, true) && s.show_fps);
        assert!(s.change(11, true) && s.responsive_pacing);
        assert!(s.change(12, true) && s.run_ahead_frames == 1);
        assert!(s.change(13, true) && s.frame_delay_ms == 1);
        assert!(s.change(14, true) && s.gpu_sync);
        assert!(s.change(15, true) && s.audio_latency_ms == 32);
    }
    #[test]
    fn dev13_json_migrates_without_wiping_preferences() {
        let s: Settings = serde_json::from_str(
            r#"{"ff_speed":4,"ff_sound":false,"colour_correction":true,"rumble":false}"#
        ).unwrap();
        assert_eq!(s.ff_speed, 4);
        assert!(!s.ff_sound && !s.rumble);
        assert!(s.rewind && s.turbo && s.eject_save);
        assert!(!s.show_fps && !s.responsive_pacing);
        assert_eq!(s.run_ahead_frames, 0);
        assert_eq!(s.frame_delay_ms, 0);
        assert!(!s.gpu_sync);
        assert_eq!(s.audio_latency_ms, 0);
    }
    #[test]
    fn roundtrip_and_invalid_values() {
        let dir = std::env::temp_dir().join(format!("slot-konkr-dev14-{}", std::process::id()));
        let mut s = Settings::default();
        s.change(0, true); s.change(6, true); s.gb_palette = 47;
        s.save(&dir).unwrap();
        assert_eq!(Settings::load(&dir), s);
        fs::write(dir.join(NAME), r#"{"ff_speed":99,"shader_gba":"simpletex","gb_palette":89}"#).unwrap();
        let s = Settings::load(&dir);
        assert_eq!(s.ff_speed, 2);
        assert_eq!(s.shader_gba, Shader::Off);
        assert_eq!(s.gb_palette, 1);
        let _ = fs::remove_dir_all(&dir);
    }
}
