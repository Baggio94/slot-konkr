//! Strictly private, immutable-by-other-session mGBA Cable save namespaces.
//!
//! Historical MLab save directories used .../<game>/<role>/ directly.
//! Every new connection starts a fresh .../<game>/<role>/sessions/<nonce>/.
//! Do not migrate/delete any earlier state or import a previous Link session's
//! battery data when resuming a live solo cartridge. Solo SRAM/RTC are COPIED
//! into the new session, never moved.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_SOLO_SNAPSHOT: usize = 7 * 1024 * 1024;

pub fn validate_solo_snapshot(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() < 0x400 || bytes.len() > MAX_SOLO_SNAPSHOT || bytes.starts_with(b"SLK1") {
        return Err(format!(
            "Unsafe solo-to-Link mGBA snapshot ({} bytes or pair header)",
            bytes.len()
        ));
    }
    Ok(())
}

/// Atomic create_dir is our inter-session lock. Each call allocates an
/// independent namespace, including if a previous connection failed or
/// exited without saving. No rename/delete of historical session data.
pub fn fresh_session(root: &Path, game_id: &str, role: &str) -> io::Result<PathBuf> {
    if game_id.is_empty() || !game_id.bytes().all(|b| b.is_ascii_hexdigit()) ||
        !matches!(role, "host" | "join") {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid Link session identity"));
    }
    let role_root = root.join("Multiplayer").join("mGBA").join(game_id).join(role);
    let session_root = role_root.join("sessions");
    fs::create_dir_all(&session_root)?;
    let epoch = SystemTime::now().duration_since(UNIX_EPOCH)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?
        .as_micros();
    for attempt in 0..1024u16 {
        let dir = session_root.join(format!("session-{epoch}-{attempt:03}"));
        match fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists,
        "No unused Multiplayer session directory available"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "slot-a6-link-sessions-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos(),
        ));
        fs::create_dir(&base).unwrap();
        base
    }

    #[test]
    fn accepts_only_one_bounded_solo_state() {
        assert!(validate_solo_snapshot(&vec![0u8; 0x400]).is_ok());
        assert!(validate_solo_snapshot(&vec![0u8; MAX_SOLO_SNAPSHOT]).is_ok());
        assert!(validate_solo_snapshot(&[0u8; 0x3ff]).is_err());
        assert!(validate_solo_snapshot(&vec![0u8; MAX_SOLO_SNAPSHOT + 1]).is_err());
        let mut paired = b"SLK1".to_vec();
        paired.resize(0x404, 0);
        assert!(validate_solo_snapshot(&paired).is_err());
    }

    #[test]
    fn fresh_link_sessions_never_reuse_legacy_or_previous_battery_files() {
        let base = fixture();
        let role = base.join("Multiplayer/mGBA/abcdef0/host");
        fs::create_dir_all(&role).unwrap();
        let old = role.join("Pokemon.srm");
        fs::write(&old, b"historic multiplayer SRAM").unwrap();
        let one = fresh_session(&base, "abcdef0", "host").unwrap();
        fs::write(one.join("Pokemon.srm"), b"first session").unwrap();
        let two = fresh_session(&base, "abcdef0", "host").unwrap();
        assert_ne!(one, two);
        assert!(!two.join("Pokemon.srm").exists());
        assert_eq!(fs::read(old).unwrap(), b"historic multiplayer SRAM");
        assert_eq!(fs::read(one.join("Pokemon.srm")).unwrap(), b"first session");
        let guest = fresh_session(&base, "abcdef0", "join").unwrap();
        assert_ne!(two, guest);
        assert!(fresh_session(&base, "../bad", "host").is_err());
        assert!(fresh_session(&base, "abcdef0", "other").is_err());
        fs::remove_dir_all(base).unwrap();
    }
}
