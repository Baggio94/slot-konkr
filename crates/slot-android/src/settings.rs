use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};
use slot_gfx::ScreenEffect;
use slot_store::GbPalette;

/// Matches Slot v1.7.0's canonical WallpaperPick semantics, while storing
/// the user's choice in the existing Android JSON settings file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallpaperPick {
    None,
    Random,
    Default,
    Blur,
    File(String),
}
impl Default for WallpaperPick {
    fn default() -> Self { Self::Default }
}
impl WallpaperPick {
    pub fn choices(root: &Path) -> Vec<Self> {
        let mut choices=vec![Self::None, Self::Random, Self::Default, Self::Blur];
        choices.extend(crate::wallpaper::list(root).into_iter().map(Self::File));
        choices
    }
    pub fn step(&self, root: &Path, right: bool) -> Self {
        let choices=Self::choices(root);
        let at=choices.iter().position(|v| v==self).unwrap_or(2);
        let n=choices.len();
        choices[if right {(at+1)%n}else{(at+n-1)%n}].clone()
    }
    pub fn is_personal(&self, has_wallpapers: bool) -> bool {
        matches!(self, Self::File(_)) || (matches!(self, Self::Random) && has_wallpapers)
    }
    pub fn label(&self) -> String {
        match self {
            Self::None=>"None".into(),
            Self::Random=>"Random".into(),
            Self::Default=>"Default".into(),
            Self::Blur=>"DEFAULT BLUR".into(),
            Self::File(name)=>name.rsplit_once('.').map(|(a,_)|a).unwrap_or(name).into()
        }
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallpaperTint { #[default] Off, Low, Medium, High }
impl WallpaperTint {
    pub const ALL: [Self; 4]=[Self::Off, Self::Low, Self::Medium, Self::High];
    pub fn step(self, right: bool) -> Self {
        let at=Self::ALL.iter().position(|v|*v==self).unwrap_or(0);
        let n=Self::ALL.len();
        Self::ALL[if right {(at+1)%n}else{(at+n-1)%n}]
    }
    pub fn alpha(self)->f32 {
        // Exact Slot v1.7.0 app.rs tint_alpha().
        match self { Self::Off=>0.0, Self::Low=>0.25, Self::Medium=>0.45, Self::High=>0.65 }
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Appearance { #[default] Dark, Light }
impl Appearance {
    pub fn toggle(self)->Self { if self==Self::Dark {Self::Light}else{Self::Dark} }
}

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub ff_speed: u8, pub ff_sound: bool, pub colour_correction: bool,
    pub rumble: bool, pub shader_gba: Shader, pub shader_gb: Shader,
    pub gb_palettes: bool, pub gb_palette: u8, pub rewind: bool,
    // X/Y Buttons: true = Slot's original Turbo, false = Y=A and X=B.
    // Existing persisted turbo=false now selects the alternative A/B mapping.
    pub turbo: bool, pub eject_save: bool,
    // Optional one-line FPS display and latency settings.
    pub show_fps: bool, pub responsive_pacing: bool,
    // Choreographer-driven display callbacks, with a single real core
    // frame maximum per callback and fractional frame time carryover.
    pub display_sync_pacing: bool,
    pub run_ahead_frames: u8,
    // Opt-in only. Old builds enabled these by default; the migration
    // below must not interpret those historical booleans as explicit consent.
    pub show_wifi: bool, pub show_bluetooth: bool,
    pub show_integrations: bool,
    #[serde(default)]
    pub status_opt_in_v2: bool,
    // Slot v1.7 visual settings; independent of saved emulation profiles.
    pub wallpaper: WallpaperPick,
    pub wallpaper_tint: WallpaperTint,
    pub appearance: Appearance,
    /// Slot v1.7 GENERAL: Recents is off by default; persist its list regardless.
    pub show_recents: bool,
    pub show_titles: bool,
}
impl Default for Settings {
    fn default() -> Self {
        // Dev13 settings survive upgrades. Do not change established defaults.
        Self { ff_speed: 2, ff_sound: true, colour_correction: false,
            rumble: true, shader_gba: Shader::Off, shader_gb: Shader::Off,
            gb_palettes: false, gb_palette: 1, rewind: true,
            turbo: true, eject_save: true,
            show_fps: false, responsive_pacing: false,
            display_sync_pacing: false,
            run_ahead_frames: 0,
            show_wifi: false, show_bluetooth: false, show_integrations: false,
            status_opt_in_v2: true, wallpaper: WallpaperPick::Default,
            wallpaper_tint: WallpaperTint::Off, appearance: Appearance::Dark,
            show_recents: false, show_titles: false }
    }
}
impl Settings {
    pub fn load(dir: &Path) -> Self {
        fs::read(dir.join(NAME)).ok()
            .and_then(|b| {
                let had_wallpaper_choice=serde_json::from_slice::<serde_json::Value>(&b)
                    .ok().is_some_and(|v|v.get("wallpaper").is_some());
                serde_json::from_slice::<Self>(&b).ok().map(|mut s| {
                    // Beta RC1 imported user.png but lacked an option to turn
                    // it off. Preserve that picture on update to U1 instead
                    // of silently replacing it with Slot's bundled default.
                    if !had_wallpaper_choice &&
                        dir.join("Wallpapers/user.png").is_file() {
                        s.wallpaper=WallpaperPick::File("user.png".into());
                    }
                    s
                })
            })
            .map(|mut s| {
                if !SPEEDS.contains(&s.ff_speed) { s.ff_speed = 2; }
                if !Shader::GBA.contains(&s.shader_gba) { s.shader_gba = Shader::Off; }
                if !Shader::GB.contains(&s.shader_gb) { s.shader_gb = Shader::Off; }
                if s.gb_palette >= 48 { s.gb_palette = 1; }
                if !s.responsive_pacing { s.display_sync_pacing = false; }
                if s.run_ahead_frames > 2 { s.run_ahead_frames = 0; }
                // Before this version all three toggles defaulted ON.
                // A previously written settings file therefore isn't proof
                // that the user chose to permit status probing. One-time
                // safe migration, without touching any other preference.
                if !s.status_opt_in_v2 {
                    s.show_wifi = false;
                    s.show_bluetooth = false;
                    s.show_integrations = false;
                    s.status_opt_in_v2 = true;
                }
                s
            }).unwrap_or_default()
    }
    pub fn save(&self, dir: &Path) -> io::Result<()> {
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
        let before = self.clone();
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
            11 => cycle_pacing(&mut self.responsive_pacing, &mut self.display_sync_pacing, right),
            12 => self.run_ahead_frames = cycle_u8(self.run_ahead_frames, 2, right),
            13 => self.show_wifi = !self.show_wifi,
            14 => self.show_bluetooth = !self.show_bluetooth,
            15 => self.show_integrations = !self.show_integrations,
            _ => return false,
        }
        *self != before
    }
    pub fn palette_name(&self) -> &'static str {
        GbPalette::all().nth(self.gb_palette as usize)
            .unwrap_or(GbPalette::DEFAULT).core_name()
    }
}
/// Three explicit states; persisted Beta 5 profiles with responsive=true
/// remain RESPONSIVE, not DISPLAY SYNC, after upgrade.
pub fn cycle_pacing(responsive: &mut bool, display_sync: &mut bool, right: bool) {
    let modes = [(false, false), (true, false), (true, true)];
    let current = modes.iter().position(|&(r, d)| r == *responsive && d == *display_sync)
        .unwrap_or(0);
    let next = if right { (current + 1) % modes.len() }
        else { (current + modes.len() - 1) % modes.len() };
    (*responsive, *display_sync) = modes[next];
}
pub fn cycle_u8(value: u8, max: u8, right: bool) -> u8 {
    if right { if value >= max { 0 } else { value + 1 } }
    else if value == 0 || value > max { max } else { value - 1 }
}
/// Model of the frame-debt clamp. STANDARD retains RC3 behavior.
/// RC4 Beta 5 restores the RC4 Beta 3 RESPONSIVE behavior requested after
/// on-device testing: never allow more than 1.5 frame periods of debt and
/// never execute more than one real emulation frame per display callback.
pub fn frame_debt(prev: f64, elapsed: f64, speed: f64, period: f64,
                  fast: bool, responsive: bool) -> f64 {
    let max = if fast { 0.40 }
        else if responsive { period * 1.5 }
        else { 0.10 };
    (prev + elapsed * speed).min(max)
}

/// A11: linked mGBA Cable can legitimately stall while awaiting a real
/// remote input frame. Unlike solo RESPONSIVE, its lost emulation time must
/// not be discarded, or the PCM producer permanently falls behind AudioTrack.
/// The 2.5-frame cap avoids runaway catch-up after a long AVD/Wi-Fi hitch.
/// The runtime may emulate at most TWO frames in one GL callback and each
/// still requires confirmed local AND peer inputs: never predict link traffic.
pub fn frame_debt_mgba_cable(prev: f64, elapsed: f64, period: f64) -> f64 {
    (prev + elapsed.max(0.0)).min(period * 2.5)
}

/// DISPLAY SYNC pacing: retain up to 0.95 frame of fractional carryover
/// after each simulation step, tolerating occasional 60Hz callback jitter.
/// The engine still executes a maximum of ONE real core frame per callback,
/// so this cannot reintroduce the Beta 4 catch-up/input-lag regression.
pub fn frame_debt_display_sync(prev: f64, elapsed: f64, speed: f64,
                               period: f64) -> f64 {
    (prev + elapsed * speed).min(period * 1.95)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn general_recents_and_titles_are_opt_in_and_persist_without_losing_settings() {
        let previous=r#"{"ff_speed":4,"wallpaper":"none","run_ahead_frames":1}"#;
        let mut s:Settings=serde_json::from_str(previous).unwrap();
        assert!(!s.show_recents && !s.show_titles);
        s.show_recents=true;s.show_titles=true;
        let copy:Settings=serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert!(copy.show_recents && copy.show_titles);
        assert_eq!(copy.ff_speed,4);
        assert_eq!(copy.run_ahead_frames,1);
        assert_eq!(copy.wallpaper,WallpaperPick::None);
    }

    #[test]
    fn upstream_v17_wallpaper_picker_follows_original_order() {
        use WallpaperPick::*;
        let root=std::env::temp_dir().join(format!(
            "slot-wallpaper-choice-test-{}", std::process::id()));
        let dir=root.join("Wallpapers");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("user.png"), b"placeholder").unwrap();
        std::fs::write(dir.join("another.png"), b"placeholder").unwrap();
        let choices=WallpaperPick::choices(&root);
        assert_eq!(choices, vec![None,Random,Default,Blur,
            File("another.png".into()),File("user.png".into())]);
        assert_eq!(None.step(&root,true), Random);
        assert_eq!(None.step(&root,false), File("user.png".into()));
        assert_eq!(Default.step(&root,true), Blur);
        assert_eq!(Blur.step(&root,true), File("another.png".into()));
        assert_eq!(File("missing.png".into()).step(&root,true), Blur);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wallpaper_none_persists_with_other_dev13_preferences_unchanged() {
        let mut settings: Settings=serde_json::from_str(
            r#"{"run_ahead_frames":2,"show_wifi":true,"status_opt_in_v2":true}"#).unwrap();
        assert_eq!(settings.wallpaper, WallpaperPick::Default);
        assert_eq!(settings.appearance, Appearance::Dark);
        assert_eq!(settings.wallpaper_tint, WallpaperTint::Off);
        settings.wallpaper=WallpaperPick::None;
        settings.wallpaper_tint=WallpaperTint::High;
        settings.appearance=Appearance::Light;
        let saved=serde_json::to_string(&settings).unwrap();
        let restored:Settings=serde_json::from_str(&saved).unwrap();
        assert_eq!(restored.wallpaper,WallpaperPick::None);
        assert_eq!(restored.wallpaper_tint.alpha(),0.65);
        assert_eq!(restored.appearance,Appearance::Light);
        assert_eq!(restored.run_ahead_frames,2);
        assert!(restored.show_wifi && restored.status_opt_in_v2);
    }

    #[test]
    fn old_user_png_migrates_to_file_without_overwriting_existing_settings() {
        let root=std::env::temp_dir().join(format!(
            "slot-wallpaper-migration-test-{}",std::process::id()));
        std::fs::create_dir_all(root.join("Wallpapers")).unwrap();
        std::fs::write(root.join("Wallpapers/user.png"),b"png").unwrap();
        std::fs::write(root.join("slot-konkr-settings.json"),
            br#"{"eject_save":false,"status_opt_in_v2":true}"#).unwrap();
        let s=Settings::load(&root);
        assert_eq!(s.wallpaper,WallpaperPick::File("user.png".into()));
        assert!(!s.eject_save);
        let mut settings=s.clone();
        settings.wallpaper=WallpaperPick::None;
        settings.save(&root).unwrap();
        assert_eq!(Settings::load(&root).wallpaper,WallpaperPick::None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn official_wallpaper_tint_alpha_and_theme_toggle_are_exact() {
        assert_eq!(WallpaperTint::ALL.map(WallpaperTint::alpha),
            [0.0,0.25,0.45,0.65]);
        assert_eq!(Appearance::Dark.toggle(),Appearance::Light);
        assert_eq!(Appearance::Light.toggle(),Appearance::Dark);
    }

    #[test]
    fn mgba_cable_preserves_bounded_debt_while_peer_frame_is_missing() {
        let frame = 1.0 / 59.7275;
        let mut debt = frame_debt_mgba_cable(0.0, frame, frame);
        assert!((debt - frame).abs() < 1e-9);
        // A missing peer input blocks the real frame: do not consume debt.
        debt = frame_debt_mgba_cable(debt, frame, frame);
        assert!((debt - 2.0 * frame).abs() < 1e-9);
        debt = frame_debt_mgba_cable(debt, 0.20, frame);
        assert!((debt - 2.5 * frame).abs() < 1e-9);
        // Represent TWO successful lockstep frames, only after peer data.
        for _ in 0..2 { debt -= frame; }
        assert!((debt - 0.5 * frame).abs() < 1e-9);
        assert_eq!(frame_debt_mgba_cable(0.0, -1.0, frame), 0.0);
    }

    #[test]
    fn mgba_cable_recovery_does_not_change_solo_responsive() {
        let frame = 1.0 / 59.7275;
        let solo = frame_debt(0.0, 0.1, 1.0, frame, false, true);
        let linked = frame_debt_mgba_cable(0.0, 0.1, frame);
        assert!((solo - 1.5 * frame).abs() < 1e-9);
        assert!((linked - 2.5 * frame).abs() < 1e-9);
    }

    #[test]
    fn responsive_pacing_rejects_backlog_without_touching_standard() {
        let frame = 1.0 / 59.7275;
        assert!((frame_debt(0.0, 0.05, 1.0, frame, false, false) - 0.05).abs() < 1e-9);
        assert!((frame_debt(0.0, 0.05, 1.0, frame, false, true) - frame * 1.5).abs() < 1e-9);
        assert!((frame_debt(0.0, 0.35, 2.0, frame, true, true) - 0.40).abs() < 1e-9);
    }

    #[test]
    fn responsive_discards_frame_debt_instead_of_catchup_bursts() {
        let period = 1.0 / 59.7275;
        let mut debt = 0.0;
        // Simulate an uneven 60Hz swap and verify each display callback
        // can emulate zero or one frames only, never two (Beta 4 regression).
        for elapsed in [period, 0.04, 0.011, 0.028, 0.008, period] {
            debt = frame_debt(debt, elapsed, 1.0, period, false, true);
            assert!(debt <= period * 1.5 + 1e-9);
            let mut frames = 0;
            if debt >= period {
                debt -= period;
                frames += 1;
            }
            assert!(frames <= 1);
        }
    }

    #[test]
    fn display_sync_preserves_more_fractional_time_without_multi_frame_catchup() {
        let period = 1.0 / 59.7275;
        let mut beta7_debt = 0.0;
        let mut beta8_debt = 0.0;
        let mut beta7_frames = 0u32;
        let mut beta8_frames = 0u32;
        // A reproducible 60Hz callback sequence with small scheduler jitter
        // and an occasional missed vsync, not a GPU throughput benchmark.
        for i in 0..6000 {
            let mut elapsed = 1.0 / 60.0;
            if i % 8 == 0 { elapsed += 0.0018; }
            if i % 8 == 1 || i % 8 == 2 { elapsed -= 0.001; }
            if i % 240 == 27 { elapsed += 1.0 / 60.0; }
            beta7_debt = frame_debt(beta7_debt, elapsed, 1.0, period, false, true);
            beta8_debt = frame_debt_display_sync(beta8_debt, elapsed, 1.0, period);
            // Both profiles have the same *single-frame* work limit.
            if beta7_debt >= period {
                beta7_debt -= period;
                beta7_frames += 1;
            }
            if beta8_debt >= period {
                beta8_debt -= period;
                beta8_frames += 1;
            }
            assert!(beta8_debt < period);
            assert!(beta8_debt >= 0.0);
        }
        assert!(beta8_frames > beta7_frames,
            "do not discard extra fractions of emulation time");
        assert!(beta8_frames - beta7_frames < 100,
            "this must never simulate large catch-up bursts");
    }

    #[test]
    fn display_sync_drops_large_hitches_without_overrunning_input() {
        let period = 1.0 / 59.7275;
        let debt = frame_debt_display_sync(
            period * 0.2, period * 10.0, 1.0, period);
        assert!((debt - period * 1.95).abs() < 1e-9);
        assert_eq!(frame_debt_display_sync(0.0, 0.0, 1.0, period), 0.0);
    }

    #[test]
    fn frame_pacing_cycles_three_modes_without_changing_legacy_defaults() {
        let mut s = Settings::default();
        assert!(!s.responsive_pacing && !s.display_sync_pacing);
        assert!(s.change(11, true));
        assert!(s.responsive_pacing && !s.display_sync_pacing);
        assert!(s.change(11, true));
        assert!(s.responsive_pacing && s.display_sync_pacing);
        assert!(s.change(11, true));
        assert!(!s.responsive_pacing && !s.display_sync_pacing);
        assert!(s.change(11, false));
        assert!(s.responsive_pacing && s.display_sync_pacing);
        let old: Settings = serde_json::from_str(r#"{"responsive_pacing":true}"#).unwrap();
        assert!(old.responsive_pacing && !old.display_sync_pacing);
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
    }
    #[test]
    fn beta8_obsolete_experiment_fields_are_ignored_when_migrating() {
        let old = r#"{
            "responsive_pacing": true,
            "display_sync_pacing": true,
            "run_ahead_frames": 1,
            "frame_delay_ms": 2,
            "gpu_sync": true,
            "audio_latency_ms": 96
        }"#;
        let parsed: Settings = serde_json::from_str(old).unwrap();
        assert!(parsed.responsive_pacing && parsed.display_sync_pacing);
        assert_eq!(parsed.run_ahead_frames, 1);
        let serialized = serde_json::to_string(&parsed).unwrap();
        for removed in ["frame_delay_ms", "gpu_sync", "audio_latency_ms"] {
            assert!(!serialized.contains(removed),
                "obsolete Beta 8 field must not survive next save: {removed}");
        }
    }
    #[test]
    fn interface_status_indicators_are_opt_in_only() {
        let mut s = Settings::default();
        assert!(!s.show_wifi && !s.show_bluetooth && !s.show_integrations);
        assert!(s.status_opt_in_v2);
        assert!(s.change(13, true) && s.show_wifi);
        assert!(s.change(14, true) && s.show_bluetooth);
        assert!(s.change(15, true) && s.show_integrations);
        let json = serde_json::to_string(&s).unwrap();
        let restored: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, s);
    }
    #[test]
    fn old_status_defaults_are_migrated_without_resurrecting_permission_prompts() {
        let legacy = r#"{"ff_speed":3,"show_wifi":true,"show_bluetooth":true,"show_integrations":true}"#;
        let dir = std::env::temp_dir().join(format!("kpa-status-opt-in-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(NAME), legacy).unwrap();
        let mut migrated = Settings::load(&dir);
        assert_eq!(migrated.ff_speed, 3);
        assert!(!migrated.show_wifi && !migrated.show_bluetooth && !migrated.show_integrations);
        assert!(migrated.status_opt_in_v2);
        migrated.change(14, true);
        migrated.save(&dir).unwrap();
        let restored = Settings::load(&dir);
        assert!(!restored.show_wifi && restored.show_bluetooth && !restored.show_integrations);
        let _ = std::fs::remove_dir_all(dir);
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
