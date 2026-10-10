use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

pub const SPEEDS: [u8; 4] = [2, 3, 4, 6];
const NAME: &str = "slot-konkr-settings.json";

/// Stored separately from Slot's internal save files and ROM cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub ff_speed: u8,
    pub ff_sound: bool,
    pub colour_correction: bool,
    pub rumble: bool,
}
impl Default for Settings {
    fn default() -> Self {
        // Preserve the KONKR dev12 behaviour during an in-place upgrade.
        Self { ff_speed: 2, ff_sound: true, colour_correction: false, rumble: true }
    }
}
impl Settings {
    pub fn load(dir: &Path) -> Self {
        fs::read(dir.join(NAME)).ok()
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .map(|mut settings| {
                if !SPEEDS.contains(&settings.ff_speed) { settings.ff_speed = 2; }
                settings
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
    pub fn change(&mut self, row: usize, right: bool) -> bool {
        let previous = *self;
        match row {
            0 => {
                let at = SPEEDS.iter().position(|x| *x == self.ff_speed).unwrap_or(0);
                let next = if right { (at + 1) % 4 } else { (at + 3) % 4 };
                self.ff_speed = SPEEDS[next];
            }
            1 => self.ff_sound = !self.ff_sound,
            2 => self.colour_correction = !self.colour_correction,
            3 => self.rumble = !self.rumble,
            _ => return false,
        }
        *self != previous
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_defaults_preserve_dev12() {
        let s = Settings::default();
        assert_eq!(s.ff_speed, 2);
        assert!(s.ff_sound && s.rumble);
        assert!(!s.colour_correction);
    }
    #[test]
    fn original_speed_values_wrap_and_flags_toggle() {
        let mut s = Settings::default();
        s.change(0, false);
        assert_eq!(s.ff_speed, 6);
        s.change(0, true);
        assert_eq!(s.ff_speed, 2);
        s.change(1, true);
        assert!(!s.ff_sound);
        s.change(2, false);
        assert!(s.colour_correction);
        s.change(3, true);
        assert!(!s.rumble);
        assert!(!s.change(4, true));
    }
    #[test]
    fn round_trip_and_invalid_speed_fallback() {
        let dir = std::env::temp_dir().join(format!("slot-konkr-config-test-{}",std::process::id()));
        let mut s = Settings::default();
        s.ff_speed=4;
        s.rumble=false;
        s.save(&dir).unwrap();
        assert_eq!(Settings::load(&dir),s);
        fs::write(dir.join(NAME),r#"{"ff_speed":9,"ff_sound":false}"#).unwrap();
        let loaded=Settings::load(&dir);
        assert_eq!(loaded.ff_speed,2);
        assert!(!loaded.ff_sound);
        assert!(loaded.rumble);
        let _=fs::remove_dir_all(dir);
    }
}
