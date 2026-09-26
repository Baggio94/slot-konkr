//! `<key> = <value>` files under `System/`, the untyped layer for per-cart preferences.
//!
//! Keyed on the rom stem, because that is already the key for `Labels/`, `Saves/` and
//! `States/`; a card stays consistent with itself. Nothing here requires that, though — the
//! key is whatever string the caller hands over.
//!
//! Two rules, and both exist because a person edits these files in a text editor on a card:
//!
//! - Every malformed line is skipped rather than raised. The cost of a typo must be that one
//!   entry falls back to its default, never that the shelf fails to load.
//! - A write replaces one line in place and never rebuilds the file from the map, so every
//!   comment, blank line and unparsed line survives — including the note somebody wrote to
//!   themselves above a cart.
//!
//! `selected_core.ini` had all of this to itself and `video_mode.ini` is the second file to
//! want it. The two were within a value type of being the same eighty lines, and two
//! hand-copied parsers is how two files meant to behave identically start to differ.

use std::collections::HashMap;
use std::path::Path;

/// Every key the file names, with its value trimmed, read fresh on every call. Empty values are
/// kept: what they mean is up to the caller's value type.
pub fn read(root: &Path, file: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Ok(text) = std::fs::read_to_string(root.join(file)) else {
        return out;
    };
    for line in text.lines() {
        let Some((key, value)) = entry(line) else {
            continue;
        };
        // A later line for the same key wins.
        out.insert(key.to_string(), value.to_string());
    }
    out
}

/// What one line names, or `None` for a blank, comment, section header, no `=`, or empty key.
/// Both `read` and `write` must use this, so they agree on which line belongs to which key.
fn entry(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') || line.starts_with(';') || line.starts_with('[') {
        return None;
    }
    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    (!key.is_empty()).then_some((key, value.trim()))
}

/// One key's value, or `None` when the file does not name it.
pub fn value(root: &Path, file: &str, key: &str) -> Option<String> {
    read(root, file).remove(key)
}

/// Set one key in place (or append it), leaving the rest of the file as it was.
///
/// Errors on a key or value that would not read back as itself (edge spaces, `=`, a leading
/// `#`, `;` or `[`, a newline): writing it would append an unfindable line on every call, or
/// overwrite another key's line.
pub fn write(root: &Path, file: &str, key: &str, value: &str) -> std::io::Result<()> {
    let line = format!("{key} = {value}");
    // Round-trip through `entry` so this check cannot drift from the parser. `entry` cannot see
    // newlines, hence `lines`.
    if line.lines().count() != 1 || entry(&line) != Some((key, value)) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{file} cannot hold the entry {line:?}"),
        ));
    }

    let path = root.join(file);
    // Missing is empty, but an unreadable file (e.g. non-UTF-8 from Notepad's ANSI default) must
    // error, or this write would replace the whole file with one line.
    let existing = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };

    let entry_line = line;
    let mut out = String::with_capacity(existing.len() + entry_line.len() + 1);
    let mut replaced = false;

    for line in existing.lines() {
        let is_this_key = entry(line).is_some_and(|(k, _)| k == key);
        if is_this_key && !replaced {
            out.push_str(&entry_line);
            replaced = true;
        } else if is_this_key {
            // Drop duplicates so the file says one thing per key.
            continue;
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    if !replaced {
        out.push_str(&entry_line);
        out.push('\n');
    }

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::atomic::atomic_write(&path, out.as_bytes())
}
