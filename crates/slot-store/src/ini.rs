//! `<key> = <value>` files under `System/`, the untyped layer for per-cart preferences.
//!
//! People edit these by hand, so a malformed line is skipped rather than raised, and a write
//! replaces one line in place so comments, blank lines and unparsed lines survive.

use std::collections::HashMap;
use std::path::Path;

/// Every key the file names, with its value trimmed, read fresh on every call. Empty values are
/// kept: what they mean is up to the caller's value type.
pub fn read(root: &Path, file: &str) -> HashMap<String, String> {
    std::fs::read_to_string(root.join(file))
        .map(|text| parse(&text))
        .unwrap_or_default()
}

/// `read`, for text already in hand.
pub fn parse(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
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

/// `text` with one key set in place (or appended), or removed when `value` is `None`, leaving
/// every other line as it was.
///
/// Errors on a key or value that would not read back as itself (edge spaces, `=`, a leading
/// `#`, `;` or `[`, a newline): writing it would append an unfindable line on every call, or
/// overwrite another key's line.
pub fn set(text: &str, key: &str, value: Option<&str>) -> std::io::Result<String> {
    let probe = format!("{key} = {}", value.unwrap_or("x"));
    if probe.lines().count() != 1 || entry(&probe) != Some((key, value.unwrap_or("x"))) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("cannot hold the entry {probe:?}"),
        ));
    }
    let entry_line = value.map(|v| format!("{key} = {v}"));
    let mut out = String::with_capacity(text.len() + probe.len() + 1);
    let mut replaced = false;
    for line in text.lines() {
        let is_this_key = entry(line).is_some_and(|(k, _)| k == key);
        if !is_this_key {
            out.push_str(line);
            out.push('\n');
        } else if let (false, Some(new)) = (replaced, &entry_line) {
            out.push_str(new);
            out.push('\n');
            replaced = true;
        }
    }
    if let (false, Some(new)) = (replaced, &entry_line) {
        out.push_str(new);
        out.push('\n');
    }
    Ok(out)
}

/// Set one key in place (or append it), leaving the rest of the file as it was.
pub fn write(root: &Path, file: &str, key: &str, value: &str) -> std::io::Result<()> {
    let path = root.join(file);
    // Missing is empty, but an unreadable file (e.g. non-UTF-8 from Notepad's ANSI default) must
    // error, or this write would replace the whole file with one line.
    let existing = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let out = set(&existing, key, Some(value))
        .map_err(|e| std::io::Error::new(e.kind(), format!("{file} {e}")))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::atomic::atomic_write(&path, out.as_bytes())
}
