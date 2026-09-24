use slot::build_info::Build;

fn sample() -> Build {
    Build {
        version: "0.1.0",
        hash: "9e11a10",
        dirty: false,
        date: "2026-08-12",
    }
}

/// Code 39 has no lower case, and a serial outside its alphabet would draw no bars at all.
#[test]
fn the_serial_is_upper_case_hex() {
    assert_eq!(sample().serial(), "9E11A10");
    assert!(sample().serial().chars().all(|c| c.is_ascii_hexdigit()));
}

/// The boxed digit beside the bars is the dirty-tree marker.
#[test]
fn the_boxed_digit_says_whether_the_tree_was_dirty() {
    let dirty = Build {
        dirty: true,
        ..sample()
    };
    assert_eq!(sample().dirty_digit(), '0');
    assert_eq!(dirty.dirty_digit(), '1');
}

/// Every probe fails soft: the device build runs as root on a bind mount owned by another uid,
/// which git refuses.
#[test]
fn a_build_with_no_git_still_has_a_serial() {
    let unknown = Build {
        hash: "unknown",
        date: "unknown",
        ..sample()
    };
    assert!(!unknown.serial().is_empty());
    // The fallback name is in the Code 39 alphabet, so it still scans.
    let payload = format!("*SLOT-{}-{}*", unknown.serial(), unknown.dirty_digit());
    assert!(slot_ui::code39(&payload).is_some());
}

#[test]
fn the_current_build_is_populated() {
    let b = Build::current();
    assert!(!b.version.is_empty());
    assert!(!b.hash.is_empty());
    assert!(!b.date.is_empty());
}
