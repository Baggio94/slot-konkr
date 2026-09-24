use std::fmt;
use std::path::{Path, PathBuf};

use crate::gba::{header_code, header_title};

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Cart {
    /// Filename stem, which is the key for labels, saves and states. Not a content hash.
    pub stem: String,
    pub rom: PathBuf,
    pub label: Option<PathBuf>,
    pub title: String,
    /// The four character header game code, empty when the rom has none.
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

/// The carts in `Games/GBA/`, sorted. A missing or unreadable folder is an empty shelf, and an
/// entry that will not stat costs only that cart.
pub fn scan(root: &Path) -> Result<Vec<Cart>, StoreError> {
    let mut carts = Vec::new();
    let dir = root.join("Games").join(crate::CART_DIR);
    let entries = match std::fs::read_dir(&dir) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(carts),
        Err(e) => {
            eprintln!("slot: scan: {}: {e}", dir.display());
            return Ok(carts);
        }
    };
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let rom = entry.path();
        if is_hidden(&rom) || !rom.is_file() || !is_gba(&rom) {
            continue;
        }
        let Some(stem) = rom.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let label = root
            .join("Labels")
            .join(crate::CART_DIR)
            .join(format!("{stem}.png"));
        carts.push(Cart {
            stem: stem.to_string(),
            title: header_title(&rom).unwrap_or_default(),
            code: header_code(&rom).unwrap_or_default(),
            label: label.is_file().then_some(label),
            rom,
        });
    }
    carts.sort_by_key(|c| sort_key(&c.stem));
    Ok(carts)
}

fn is_gba(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gba"))
}

/// Shelf order: digit-led titles, then letters A to Z ignoring case, then everything else.
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
