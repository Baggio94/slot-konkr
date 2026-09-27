use std::path::Path;

/// Which console a cart is for: its card directory, and its shelf on the carousel. A file's
/// platform is where it is, so the scan never opens the file.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Platform {
    #[default]
    Gba,
    Gb,
    Gbc,
}

impl Platform {
    /// In shelf order.
    pub const ALL: [Platform; 3] = [Platform::Gba, Platform::Gb, Platform::Gbc];

    /// The machine's name as printed on the case band. `dir_name` is the folder's spelling.
    pub fn name(self) -> &'static str {
        match self {
            Platform::Gba => "Game Boy Advance",
            Platform::Gb => "Game Boy",
            Platform::Gbc => "Game Boy Color",
        }
    }

    /// The card directory under `Games/`, `Saves/`, `States/` and `Labels/`.
    pub fn dir_name(self) -> &'static str {
        match self {
            Platform::Gba => "GBA",
            Platform::Gb => "GB",
            Platform::Gbc => "GBC",
        }
    }

    /// The ROM extensions this folder holds. A `.gba` in `GB/` is not scanned.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Platform::Gba => &["gba"],
            Platform::Gb | Platform::Gbc => &["gb", "gbc"],
        }
    }

    /// The picture size in pixels. A Game Boy's is centred inside the GBA-sized frame buffer by
    /// `video_refresh`, so this also says how much of the buffer is margin.
    pub fn picture(self) -> (u32, u32) {
        match self {
            Platform::Gba => (240, 160),
            Platform::Gb | Platform::Gbc => (160, 144),
        }
    }

    pub fn accepts(self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| {
                self.extensions()
                    .iter()
                    .any(|k| ext.eq_ignore_ascii_case(k))
            })
    }
}
