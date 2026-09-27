use std::fmt;
use std::path::{Path, PathBuf};

use crate::gba::{header_code, header_title};
use crate::platform::Platform;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Cart {
    /// Decided by the folder `scan` found the rom in, never by reading the rom.
    pub platform: Platform,
    /// Filename stem, which is the key for labels, saves and states. Not a content hash.
    pub stem: String,
    pub rom: PathBuf,
    pub label: Option<PathBuf>,
    pub title: String,
    /// The four character header game code, empty when the rom has none. Always empty for
    /// Game Boy carts, which have no such field.
    pub code: String,
}

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "io: {e}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        StoreError::Io(e)
    }
}

/// A missing card, library or platform folder is an empty shelf, not a boot failure. An
/// unreadable folder or entry is skipped, since `App::boot` turns any `Err` into an empty shelf
/// and one bad folder must not hide the others.
pub fn scan(root: &Path) -> Result<Vec<Cart>, StoreError> {
    let mut carts = Vec::new();
    for platform in Platform::ALL {
        let dir = root.join("Games").join(platform.dir_name());
        let entries = match std::fs::read_dir(&dir) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                eprintln!("slot: scan: {}: {e}", dir.display());
                continue;
            }
        };
        for entry in entries {
            let Ok(entry) = entry else {
                continue;
            };
            let rom = entry.path();
            // The folder decides the platform; the extension decides whether this is a cart.
            if is_hidden(&rom) || !rom.is_file() || !platform.accepts(&rom) {
                continue;
            }
            let Some(stem) = rom.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let label = root
                .join("Labels")
                .join(platform.dir_name())
                .join(format!("{stem}.png"));
            let (title, code) = match platform {
                Platform::Gba => (
                    header_title(&rom).unwrap_or_default(),
                    header_code(&rom).unwrap_or_default(),
                ),
                // `gba.rs` offsets 0xA0 and 0xAC are in a Game Boy cart's RST vectors, so they
                // would read opcode bytes.
                _ => (crate::gb::title(&rom).unwrap_or_default(), String::new()),
            };
            carts.push(Cart {
                platform,
                stem: stem.to_string(),
                title,
                code,
                label: label.is_file().then_some(label),
                rom,
            });
        }
    }
    carts.sort_by(|a, b| {
        (a.platform as u8, sort_key(&a.stem)).cmp(&(b.platform as u8, sort_key(&b.stem)))
    });
    Ok(carts)
}

/// Where a title files on the shelf: digits first, then A to Z ignoring case, then anything led
/// by neither (a bracket, a quote).
pub fn sort_key(stem: &str) -> (u8, String) {
    (group_of(stem), stem.to_uppercase())
}

fn group_of(stem: &str) -> u8 {
    match stem.chars().find(|c| !c.is_whitespace()) {
        Some(c) if c.is_ascii_digit() => 0,
        Some(c) if c.is_alphabetic() => 1,
        _ => 2,
    }
}

/// The letter a title is filed under, for skipping a row a letter at a time. Anything not led
/// by a letter shares the `#` stop.
pub fn initial(stem: &str) -> char {
    match group_of(stem) {
        1 => stem
            .chars()
            .find(|c| !c.is_whitespace())
            .and_then(|c| c.to_uppercase().next())
            .unwrap_or('#'),
        _ => '#',
    }
}

/// A leading dot is metadata, not content. macOS writes `._<name>` sidecars onto FAT volumes
/// with the shadowed file's extension, so the extension alone cannot filter them.
pub fn is_hidden(p: &Path) -> bool {
    p.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with('.'))
}
