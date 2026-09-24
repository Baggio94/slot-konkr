use std::path::Path;

/// `System/theme.txt`: one `name #rrggbb` per line. A leading `#` is a comment; there are no
/// trailing comments because `#` also starts a colour. A bad line leaves that colour at its
/// default and never stops the device booting.
pub const THEME_FILE: &str = "theme.txt";

/// Colours of the case around the slot, outside in. Field names are the theme file's keys.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Theme {
    /// The outer plastic.
    pub housing: [u8; 3],
    /// The floor of the bay, stepped down from the shell.
    pub recess: [u8; 3],
    /// The opening itself, and the inside of the thumb scoop.
    pub opening: [u8; 3],
    /// The lit edge of the plastic: the top of the slot and the rim of the scoop.
    pub edge: [u8; 3],
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            housing: [0x24, 0x24, 0x29],
            recess: [0x1a, 0x1a, 0x1d],
            opening: [0x05, 0x05, 0x08],
            edge: [0x4d, 0x4d, 0x57],
        }
    }
}

impl Theme {
    /// Best effort. A missing file is the default theme, not an error.
    pub fn read(root: &Path) -> Self {
        match std::fs::read_to_string(root.join("System").join(THEME_FILE)) {
            Ok(text) => Self::parse(&text),
            Err(_) => Theme::default(),
        }
    }

    pub fn parse(text: &str) -> Self {
        let mut theme = Theme::default();
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            let mut parts = line.split_whitespace();
            let (Some(name), Some(value)) = (parts.next(), parts.next()) else {
                continue;
            };
            // A third word means a malformed line; skip rather than guess.
            if parts.next().is_some() {
                continue;
            }
            let Some(rgb) = hex(value) else {
                continue;
            };
            match name.to_ascii_lowercase().as_str() {
                "housing" => theme.housing = rgb,
                "recess" => theme.recess = rgb,
                "opening" => theme.opening = rgb,
                "edge" => theme.edge = rgb,
                _ => {}
            }
        }
        theme
    }
}

/// `rrggbb`, with or without the leading hash.
fn hex(value: &str) -> Option<[u8; 3]> {
    let digits = value.strip_prefix('#').unwrap_or(value);
    if digits.len() != 6 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}
