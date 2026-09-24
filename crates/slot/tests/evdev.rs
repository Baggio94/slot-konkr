use std::fs;
use std::path::Path;

use slot::input::evdev::{decode, has_bit, pick_devices, to_raw, EVENT_BYTES};
use slot_input::{Btn, RawEvent};

/// One `struct input_event` as the kernel writes it on 64 bit: two 64 bit words of timeval,
/// then type, code and value. Spelled out, not derived from the decoder under test.
fn packet(kind: u16, code: u16, value: i32) -> [u8; EVENT_BYTES] {
    let mut b = [0x11u8; EVENT_BYTES];
    b[16] = kind as u8;
    b[17] = (kind >> 8) as u8;
    b[18] = code as u8;
    b[19] = (code >> 8) as u8;
    b[20..24].copy_from_slice(&value.to_le_bytes());
    b
}

const EV_KEY: u16 = 0x01;
const EV_SW: u16 = 0x05;
const EV_SYN: u16 = 0x00;
/// R1 as the pad sends it, from a device trace. The board's codes are not the standard ones:
/// `0x137` is `BTN_TR` to the kernel headers and START to this device.
const CODE_R1: u16 = 0x135;
const KEY_VOLUMEUP: u16 = 115;
const SW_LID: u16 = 0x00;

#[test]
fn a_key_press_decodes_from_the_evdev_byte_layout() {
    let down = decode(&packet(EV_KEY, CODE_R1, 1)).and_then(to_raw);
    assert_eq!(down, Some(RawEvent::Down(Btn::R1)));
    let up = decode(&packet(EV_KEY, CODE_R1, 0)).and_then(to_raw);
    assert_eq!(up, Some(RawEvent::Up(Btn::R1)));
    let vol = decode(&packet(EV_KEY, KEY_VOLUMEUP, 1)).and_then(to_raw);
    assert_eq!(vol, Some(RawEvent::Down(Btn::VolUp)));
}

/// Autorepeat is presses with no release, which would re-arm every hold and double tap window.
#[test]
fn autorepeat_is_not_a_press() {
    assert_eq!(decode(&packet(EV_KEY, CODE_R1, 2)).and_then(to_raw), None);
}

#[test]
fn the_lid_arrives_as_a_switch_rather_than_a_key() {
    let shut = decode(&packet(EV_SW, SW_LID, 1)).and_then(to_raw);
    assert_eq!(shut, Some(RawEvent::Down(Btn::Lid)));
    let open = decode(&packet(EV_SW, SW_LID, 0)).and_then(to_raw);
    assert_eq!(open, Some(RawEvent::Up(Btn::Lid)));
    // A key with the same code is the escape key, unrelated to the hinge.
    assert_ne!(
        decode(&packet(EV_KEY, SW_LID, 1)).and_then(to_raw),
        Some(RawEvent::Down(Btn::Lid))
    );
}

#[test]
fn frame_markers_and_unmapped_codes_are_dropped() {
    assert_eq!(decode(&packet(EV_SYN, 0, 0)).and_then(to_raw), None);
    assert_eq!(decode(&packet(EV_KEY, 0x2ff, 1)).and_then(to_raw), None);
}

#[test]
fn a_short_read_decodes_to_nothing_rather_than_panicking() {
    assert!(decode(&[0u8; EVENT_BYTES - 1]).is_none());
    assert!(decode(&[]).is_none());
}

/// sysfs prints capability bitmasks as 64 bit words, most significant first, so offsets count
/// from the end. An off by one word picks the wrong devices, which reads as dead buttons.
#[test]
fn a_capability_bitmask_is_indexed_from_the_last_word() {
    assert!(has_bit("10 0", 68));
    assert!(!has_bit("10 0", 4));
    assert!(has_bit("8", 3));
    assert!(!has_bit("", 0));
    assert!(!has_bit("0 0 0", 300));
}

fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
}

/// An input tree shaped like the device's: a node with no useful buttons, the button pad, the
/// hall sensor, and a non event node.
fn input_tree() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let (dev, sys) = (d.path().join("dev/input"), d.path().join("sys/class/input"));
    for node in ["event0", "event1", "event2", "mice"] {
        write(&dev.join(node), "");
    }
    write(&sys.join("event0/device/name"), "axp2202-pek\n");
    write(&sys.join("event0/device/capabilities/key"), "0\n");
    write(&sys.join("event1/device/name"), "gpio-keys\n");
    // BTN_SOUTH, 0x130: word 4 counted from the end, bit 48 within it.
    write(
        &sys.join("event1/device/capabilities/key"),
        "1000000000000 0 0 0 0\n",
    );
    write(&sys.join("event2/device/name"), "gpio-keys-lid\n");
    write(&sys.join("event2/device/capabilities/key"), "0\n");
    write(&sys.join("event2/device/capabilities/sw"), "1\n");
    d
}

#[test]
fn devices_are_picked_by_capability_and_never_by_position() {
    let d = input_tree();
    let picked = pick_devices(
        &d.path().join("dev/input"),
        &d.path().join("sys/class/input"),
    );
    let names: Vec<_> = picked
        .iter()
        .filter_map(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["event1", "event2"]);
}

#[test]
fn an_input_node_with_no_sysfs_entry_is_skipped_rather_than_opened() {
    let d = tempfile::tempdir().unwrap();
    write(&d.path().join("dev/input/event0"), "");
    let picked = pick_devices(
        &d.path().join("dev/input"),
        &d.path().join("sys/class/input"),
    );
    assert!(picked.is_empty(), "{picked:?}");
}

/// A dead button is either an unknown code or a node never opened because sysfs advertised
/// nothing wanted. The trace has to tell them apart.
#[test]
fn the_survey_reports_the_nodes_that_were_not_opened_too() {
    let d = input_tree();
    let lines = slot::input::trace::survey(
        &d.path().join("dev/input"),
        &d.path().join("sys/class/input"),
    );
    let line = |node: &str| {
        lines
            .iter()
            .find(|l| l.contains(node))
            .unwrap_or_else(|| panic!("{node} was left out of the survey: {lines:?}"))
            .clone()
    };
    assert!(line("event0").contains("skipped"), "{}", line("event0"));
    assert!(
        line("event0").contains("axp2202-pek"),
        "the survey did not name the node: {}",
        line("event0")
    );
    assert!(line("event1").contains("opened"), "{}", line("event1"));
    assert!(line("event2").contains("opened"), "{}", line("event2"));
}

#[test]
fn an_unmapped_code_is_traced_by_number_rather_than_dropped() {
    let ev = |code| slot::input::evdev::Ev {
        kind: EV_KEY,
        code,
        value: 1,
    };
    let mapped = slot::input::trace::event_line("event1", 12, ev(CODE_R1));
    assert!(mapped.contains("R1"), "{mapped}");
    let unmapped = slot::input::trace::event_line("event1", 13, ev(0x2ff));
    assert!(unmapped.contains("0x2ff"), "{unmapped}");
    assert!(
        unmapped.contains("unmapped"),
        "an unknown code has to say so, or it looks like nothing arrived: {unmapped}"
    );
}

/// The trace is read by taking the card out, so it has to reach the card.
#[test]
fn a_trace_writes_the_survey_to_the_card() {
    let d = input_tree();
    let card = tempfile::tempdir().unwrap();
    let input = slot::input::DeviceInput::open_in(
        &d.path().join("dev/input"),
        &d.path().join("sys/class/input"),
        card.path(),
        true,
    );
    drop(input);
    let log = std::fs::read_to_string(card.path().join("input-trace.log"))
        .expect("no trace file reached the card");
    assert!(log.contains("event0 skipped"), "{log}");
    assert!(log.contains("event1 opened"), "{log}");
    assert!(log.contains("gpio-keys-lid"), "{log}");
}

#[test]
fn no_trace_file_is_written_unless_one_was_asked_for() {
    let d = input_tree();
    let card = tempfile::tempdir().unwrap();
    let input = slot::input::DeviceInput::open_in(
        &d.path().join("dev/input"),
        &d.path().join("sys/class/input"),
        card.path(),
        false,
    );
    drop(input);
    assert!(!card.path().join("input-trace.log").exists());
}

/// Every code here came off a device trace. The kernel's names are scrambled relative to the
/// legends on the case, so the standard spellings are wrong.
#[test]
fn the_pad_maps_by_the_codes_the_device_sends() {
    let expected = [
        (0x130, Btn::A),
        (0x131, Btn::B),
        (0x132, Btn::Y),
        (0x133, Btn::X),
        (0x134, Btn::L1),
        (0x135, Btn::R1),
        (0x136, Btn::Select),
        (0x137, Btn::Start),
        (0x138, Btn::Menu),
        (0x13a, Btn::L2),
        (0x13b, Btn::R2),
        (115, Btn::VolUp),
        (114, Btn::VolDown),
        (116, Btn::Power),
    ];
    for (code, want) in expected {
        assert_eq!(
            slot::input::evdev::code_to_btn(code),
            Some(want),
            "code {code:#x}"
        );
    }
}

/// The menu button sends a second code right behind the first. Mapping both would make one
/// press a double tap, which opens the switcher.
#[test]
fn the_menu_buttons_second_code_is_swallowed() {
    assert_eq!(slot::input::evdev::code_to_btn(0x162), None);
}

/// The d-pad is two axes on hat 0, and a zero releases whichever direction the axis was last
/// at, so the decode has to remember.
#[test]
fn the_d_pad_arrives_as_a_hat_rather_than_as_keys() {
    let mut hat = slot::input::evdev::Hat::default();
    let ev = |code, value| slot::input::evdev::Ev {
        kind: slot::input::evdev::EV_ABS,
        code,
        value,
    };
    assert_eq!(hat.feed(ev(0x11, -1)), vec![RawEvent::Down(Btn::Up)]);
    assert_eq!(hat.feed(ev(0x11, 0)), vec![RawEvent::Up(Btn::Up)]);
    assert_eq!(hat.feed(ev(0x11, 1)), vec![RawEvent::Down(Btn::Down)]);
    assert_eq!(hat.feed(ev(0x11, 0)), vec![RawEvent::Up(Btn::Down)]);
    assert_eq!(hat.feed(ev(0x10, -1)), vec![RawEvent::Down(Btn::Left)]);
    assert_eq!(hat.feed(ev(0x10, 0)), vec![RawEvent::Up(Btn::Left)]);
    assert_eq!(hat.feed(ev(0x10, 1)), vec![RawEvent::Down(Btn::Right)]);
    assert_eq!(hat.feed(ev(0x10, 0)), vec![RawEvent::Up(Btn::Right)]);
}

/// A rolled thumb moves the axis end to end without passing zero, which must release the
/// previous direction.
#[test]
fn a_hat_swung_end_to_end_releases_before_it_presses() {
    let mut hat = slot::input::evdev::Hat::default();
    let ev = |code, value| slot::input::evdev::Ev {
        kind: slot::input::evdev::EV_ABS,
        code,
        value,
    };
    assert_eq!(hat.feed(ev(0x10, -1)), vec![RawEvent::Down(Btn::Left)]);
    assert_eq!(
        hat.feed(ev(0x10, 1)),
        vec![RawEvent::Up(Btn::Left), RawEvent::Down(Btn::Right)]
    );
    // The same value twice is the kernel repeating itself, not a second press.
    assert!(hat.feed(ev(0x10, 1)).is_empty());
}

/// The power key is alone on its own node. Left out of the wanted list, that node is skipped.
#[test]
fn the_node_carrying_only_the_power_key_is_opened() {
    let d = tempfile::tempdir().unwrap();
    let (dev, sys) = (d.path().join("dev/input"), d.path().join("sys/class/input"));
    write(&dev.join("event0"), "");
    write(&sys.join("event0/device/name"), "axp2202-pek\n");
    // KEY_POWER, 116: bit 52 of the word holding bits 64 to 127.
    write(
        &sys.join("event0/device/capabilities/key"),
        "10000000000000 0\n",
    );
    assert_eq!(
        pick_devices(&dev, &sys).len(),
        1,
        "the power node was skipped"
    );
}

/// The d-pad is read, so the trace must not call it "ignored".
#[test]
fn a_hat_axis_is_traced_as_the_axis_it_is() {
    let line = slot::input::trace::event_line(
        "event1",
        9,
        slot::input::evdev::Ev {
            kind: slot::input::evdev::EV_ABS,
            code: 0x11,
            value: -1,
        },
    );
    assert!(line.contains("hat0y"), "{line}");
    assert!(!line.contains("ignored"), "{line}");
}
