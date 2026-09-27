use std::fs;
use std::path::Path;

use slot_power::{record_first_frame, uptime_seconds};
use tempfile::TempDir;

/// The boot budget is counted in seconds since kernel start, because that is what every
/// marker the OS writes to /run/boot-* means. slot's own clock is epoch seconds, so a first
/// frame stamped from it could not be compared with rcS-start, rcS-done or frontend-exec at
/// all — the numbers would not even be the same kind of thing.
#[test]
fn the_uptime_is_the_first_field_of_proc_uptime() {
    assert_eq!(
        uptime_seconds("1234.56 789.01\n").as_deref(),
        Some("1234.56")
    );
    assert_eq!(uptime_seconds("2.93 16.90").as_deref(), Some("2.93"));
}

/// /proc/uptime is an outside input, so anything that is not a number has to read as no
/// answer rather than as a marker file full of nonsense that later arithmetic trips over.
#[test]
fn nothing_usable_in_proc_uptime_is_no_answer() {
    assert_eq!(uptime_seconds(""), None);
    assert_eq!(uptime_seconds("\n"), None);
    assert_eq!(uptime_seconds("  "), None);
    assert_eq!(uptime_seconds("later 789.01"), None);
}

fn read(p: &Path) -> String {
    fs::read_to_string(p).unwrap_or_default()
}

/// The marker joins the same /run/boot-* family the OS already writes, so ags-boottrace and
/// validate-on-device.sh pick it up with no changes; the trace line puts the first frame on
/// the same timeline as every rcS step, which is the whole point of measuring it here.
#[test]
fn the_first_frame_lands_in_the_marker_and_on_the_trace() {
    let d = TempDir::new().unwrap();
    record_first_frame(d.path(), "2.93");
    assert_eq!(read(&d.path().join("boot-first-frame")).trim(), "2.93");
    let trace = read(&d.path().join("boot-trace"));
    assert!(trace.contains("2.93 first-frame"), "{trace}");
}

/// slot draws many frames and restarts without the machine rebooting, so only the first one
/// of a boot may claim the marker. Rewriting it later reports the newest frame as though it
/// were the boot's — exactly the fault that made a 2.9 s boot read as 7.66 s through
/// frontend-exec. The repeat still reaches the trace so a restart stays visible.
#[test]
fn a_later_frame_never_overwrites_the_boots_first_one() {
    let d = TempDir::new().unwrap();
    record_first_frame(d.path(), "2.93");
    record_first_frame(d.path(), "41.70");
    assert_eq!(read(&d.path().join("boot-first-frame")).trim(), "2.93");
    let trace = read(&d.path().join("boot-trace"));
    assert!(trace.contains("2.93 first-frame"), "{trace}");
    assert!(trace.contains("41.70 first-frame-again"), "{trace}");
}

/// A frontend that cannot write a diagnostic still has to draw. /run is tmpfs and always
/// present on the device, but the host build has no such directory, and a panic here would
/// trade a measurement for the thing being measured.
#[test]
fn a_run_directory_that_does_not_exist_is_not_a_crash() {
    let d = TempDir::new().unwrap();
    record_first_frame(&d.path().join("no/such/dir"), "2.93");
}
