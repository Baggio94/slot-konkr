//! The untyped `<stem> = <value>` layer under `System/`.

use slot_store::ini;
use tempfile::tempdir;

const FILE: &str = "System/example.ini";

fn root_with(text: Option<&str>) -> tempfile::TempDir {
    let d = tempdir().unwrap();
    std::fs::create_dir(d.path().join("System")).unwrap();
    if let Some(text) = text {
        std::fs::write(d.path().join(FILE), text).unwrap();
    }
    d
}

#[test]
fn an_absent_file_is_an_empty_map_rather_than_an_error() {
    let d = root_with(None);
    assert!(ini::read(d.path(), FILE).is_empty());
    assert_eq!(ini::value(d.path(), FILE, "Emerald"), None);
}

/// Typos cost only their own line. Empty values are kept for the typed layer to interpret.
#[test]
fn malformed_lines_are_skipped_rather_than_fatal() {
    let d = root_with(Some(concat!(
        "\n",
        "# a comment\n",
        "; another comment\n",
        "[section]\n",
        "no equals sign here\n",
        "  Spaced Out   =   stretch  \n",
        "= orphaned\n",
        "Trailing =\n",
    )));
    let map = ini::read(d.path(), FILE);
    assert_eq!(map.get("Spaced Out").map(String::as_str), Some("stretch"));
    assert_eq!(map.get("Trailing").map(String::as_str), Some(""));
    assert_eq!(map.len(), 2, "a comment or an orphan was stored");
}

#[test]
fn a_later_duplicate_wins() {
    let d = root_with(Some("Emerald = one\nEmerald = two\n"));
    assert_eq!(
        ini::value(d.path(), FILE, "Emerald").as_deref(),
        Some("two")
    );
}

#[test]
fn writing_creates_the_file_when_it_is_absent() {
    let d = root_with(None);
    ini::write(d.path(), FILE, "Emerald", "stretch").unwrap();
    assert_eq!(
        ini::value(d.path(), FILE, "Emerald").as_deref(),
        Some("stretch")
    );
}

/// Comments, blank lines and unparsed lines survive a write.
#[test]
fn writing_replaces_one_line_and_leaves_the_rest_of_the_file_alone() {
    let d = root_with(Some(concat!(
        "# my notes\n",
        "\n",
        "Emerald = actual\n",
        "Metroid Fusion = stretch\n",
    )));
    ini::write(d.path(), FILE, "Emerald", "stretch").unwrap();

    let text = std::fs::read_to_string(d.path().join(FILE)).unwrap();
    assert!(
        text.contains("# my notes"),
        "a hand-written comment was destroyed"
    );
    assert!(text.contains("\n\n"), "a blank line was closed up");
    assert!(
        text.contains("Metroid Fusion = stretch"),
        "another key's entry was lost"
    );
    assert_eq!(text.matches("Emerald").count(), 1, "the old line was left");
}

#[test]
fn writing_appends_a_key_the_file_has_never_seen() {
    let d = root_with(Some("Emerald = actual\n"));
    ini::write(d.path(), FILE, "Drill Dozer", "stretch").unwrap();
    assert_eq!(
        ini::value(d.path(), FILE, "Emerald").as_deref(),
        Some("actual")
    );
    assert_eq!(
        ini::value(d.path(), FILE, "Drill Dozer").as_deref(),
        Some("stretch")
    );
}

/// Real filenames that would not read back as themselves (trimmed, cut at `=`, or parsed as a
/// comment or section) are refused and the file is left untouched.
const UNSAYABLE: [&str; 7] = [
    " Tetris",
    "Tetris ",
    "Cheats = On",
    "#1 Racer",
    ";Notes",
    "[Section]",
    "",
];

#[test]
fn a_key_the_file_cannot_say_is_refused_rather_than_written() {
    for key in UNSAYABLE {
        let d = root_with(Some("Emerald = actual\n"));
        let e = ini::write(d.path(), FILE, key, "stretch")
            .expect_err(&format!("{key:?} was written anyway"));
        assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput, "{key:?}");
        assert_eq!(
            std::fs::read_to_string(d.path().join(FILE)).unwrap(),
            "Emerald = actual\n",
            "{key:?} changed the file"
        );
    }
}

#[test]
fn a_refused_key_cannot_grow_the_file_a_line_at_a_time() {
    let d = root_with(None);
    for value in ["stretch", "actual", "stretch"] {
        let _ = ini::write(d.path(), FILE, " Tetris", value);
    }
    let text = std::fs::read_to_string(d.path().join(FILE)).unwrap_or_default();
    assert!(
        text.lines().count() <= 1,
        "the file grew a line per write: {text:?}"
    );
}

/// `Cheats = On = x` reads as key `Cheats`, so writing it would clobber another cart's line.
/// A hand-typed ambiguous line gets `read`'s interpretation.
#[test]
fn neither_cart_can_write_the_line_the_other_would_claim() {
    let d = root_with(None);
    ini::write(d.path(), FILE, "Cheats = On", "stretch")
        .expect_err("the ambiguous line was written");
    ini::write(d.path(), FILE, "Cheats", "actual").unwrap();
    assert_eq!(
        std::fs::read_to_string(d.path().join(FILE)).unwrap(),
        "Cheats = actual\n"
    );
}

/// A write touches only the line `read` attributes to this key.
#[test]
fn a_write_leaves_every_line_that_is_not_this_keys_alone() {
    let d = root_with(Some(concat!(
        "# Emerald = what I had before\n",
        "; Emerald = and before that\n",
        "[Emerald]\n",
        "= Emerald\n",
        "Emerald = actual\n",
        "Emeralds = actual\n",
    )));
    ini::write(d.path(), FILE, "Emerald", "stretch").unwrap();
    let text = std::fs::read_to_string(d.path().join(FILE)).unwrap();
    for kept in [
        "# Emerald = what I had before",
        "; Emerald = and before that",
        "[Emerald]",
        "= Emerald",
        "Emeralds = actual",
    ] {
        assert!(text.contains(kept), "{kept:?} was disturbed: {text:?}");
    }
    assert!(text.contains("Emerald = stretch"));
    assert!(!text.contains("Emerald = actual"));
}

/// Any key `write` accepts reads back, across punctuation real rom filenames carry.
#[test]
fn every_key_a_write_accepts_reads_back_as_itself() {
    for key in [
        "Emerald",
        "Pokemon - Emerald Version (USA, Europe)",
        "Mario & Luigi",
        "Rhythm Tengoku (J) [!]",
        "F-Zero: Maximum Velocity",
        "Yoshi's Island",
        "50%",
        "Dr. Mario",
    ] {
        let d = root_with(None);
        ini::write(d.path(), FILE, key, "stretch").expect(key);
        assert_eq!(
            ini::value(d.path(), FILE, key).as_deref(),
            Some("stretch"),
            "{key:?} did not come back"
        );
        // The second write must find the first one's line.
        ini::write(d.path(), FILE, key, "actual").expect(key);
        assert_eq!(ini::value(d.path(), FILE, key).as_deref(), Some("actual"));
        let text = std::fs::read_to_string(d.path().join(FILE)).unwrap();
        assert_eq!(text.lines().count(), 1, "{key:?} left {text:?}");
    }
}

/// A non-UTF-8 file is an error, not an empty file to overwrite. The bytes are `Pokémon` in
/// cp1252, Notepad's ANSI default.
#[test]
fn a_file_that_will_not_read_as_text_is_left_alone_rather_than_replaced() {
    let d = root_with(None);
    let mut latin1 = b"Pok\xe9mon = actual\nEmerald = actual\n".to_vec();
    latin1.extend_from_slice(b"Metroid Fusion = stretch\n");
    std::fs::write(d.path().join(FILE), &latin1).unwrap();

    let e = ini::write(d.path(), FILE, "Emerald", "stretch")
        .expect_err("the unreadable file was overwritten");
    assert_eq!(e.kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(
        std::fs::read(d.path().join(FILE)).unwrap(),
        latin1,
        "the rest of the card's choices were destroyed"
    );
}

#[test]
fn an_absent_file_is_still_created_by_a_write() {
    let d = root_with(None);
    ini::write(d.path(), FILE, "Emerald", "stretch").unwrap();
    assert_eq!(
        ini::value(d.path(), FILE, "Emerald").as_deref(),
        Some("stretch")
    );
}

#[test]
fn writing_collapses_a_duplicate_the_file_already_had() {
    let d = root_with(Some("Emerald = actual\nEmerald = stretch\n"));
    ini::write(d.path(), FILE, "Emerald", "actual").unwrap();
    let text = std::fs::read_to_string(d.path().join(FILE)).unwrap();
    assert_eq!(text.matches("Emerald").count(), 1);
    assert_eq!(
        ini::value(d.path(), FILE, "Emerald").as_deref(),
        Some("actual")
    );
}
