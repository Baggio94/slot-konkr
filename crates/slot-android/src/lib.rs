//! Android host for the unmodified Slot cartridge graphics and shelf behavior.
//!
//! Stage M1: display-only preview carts, not executable games.
mod library;
mod settings;
// Directly compile the original Cart Studio No-Intro DAT matching code and tests.
// Pinned verbatim at third_party/slot-cart-studio, never reinterpret CRCs.
#[allow(dead_code)]
#[path = "../../../third_party/slot-cart-studio/src/dat.rs"]
mod studio_dat;
// Reuse the exact upstream Slot wallpaper ordering and filtering.
#[path = "../../slot/src/wallpaper.rs"]
mod wallpaper;
// Use Slot's original per-game Actual/Stretch geometry and video_mode.ini.
#[path = "../../slot/src/video_mode.rs"]
mod video_mode;
#[path = "../../slot/src/thumb.rs"]
mod thumb;
mod retroarch_state;
// Reuse the upstream 20 MiB XOR/LZ4 rewind ring without forking its algorithm.
#[path = "../../slot/src/rewind.rs"]
mod rewind;
mod core_selection;
mod core_legend;
#[cfg(target_os = "android")]
mod game;
#[cfg(target_os = "android")]
mod cart_render;
#[cfg(target_os = "android")]
mod runtime;

// Reuse upstream Slot's unmodified mechanical core picker: 420ms open,
// 180ms chip hop, 320ms close and refusal shake.
#[path = "../../slot/src/core_picker.rs"]
mod core_picker;

#[cfg(test)]
mod original_picker_integration_tests {
    use super::core_picker::{CorePicker, Outcome, Press, OPEN_MS, SLIDE_MS, LIFT_MS, HOP_MS, CLOSE_MS};
    use slot_store::Core;

    #[test]
    fn uses_upstream_open_slide_lift_and_close_timing() {
        assert_eq!(OPEN_MS, 420);
        assert_eq!(SLIDE_MS, 160);
        assert_eq!(LIFT_MS, 260);
        assert_eq!(HOP_MS, 180);
        assert_eq!(CLOSE_MS, 320);
        let mut picker = CorePicker::open(Core::Mgba, 0);
        picker.start(0);
        assert_eq!(picker.openness(210), 0.5);
        assert_eq!(picker.openness(420), 1.0);
        assert_eq!(picker.press(Press::Back, 420), Outcome::Nothing);
        assert!(!picker.finished(600));
        assert!(picker.finished(740));
    }

    #[test]
    fn hopping_chip_reaches_the_other_original_socket() {
        let mut picker = CorePicker::open(Core::Mgba, 0);
        picker.start(0);
        assert_eq!(picker.press(Press::Right, 500), Outcome::Nothing);
        assert_eq!(picker.seat(), Core::Gpsp);
        let chip = picker.chip(500 + HOP_MS / 2);
        assert!(chip.lift > 0.9);
        assert!(chip.seated.is_none());
        assert_eq!(picker.chip(500 + HOP_MS).seated, Some(Core::Gpsp));
    }
}

#[cfg(test)]
mod upstream_rewind_integration_tests {
    use super::rewind::{Rewind, REWIND_BYTES};
    #[test]
    fn original_ring_walks_back_through_snapshots_with_bounded_memory() {
        let mut ring = Rewind::new(REWIND_BYTES);
        for frame in 0..30u8 {
            let snapshot = vec![frame; 200_000];
            ring.push(&snapshot);
        }
        assert!(ring.bytes_used() <= REWIND_BYTES);
        assert_eq!(ring.depth(), 30);
        for frame in (1..30u8).rev() {
            assert_eq!(ring.pop(), Some(vec![frame; 200_000]));
        }
        assert_eq!(ring.pop(), Some(vec![0u8; 200_000]));
        assert_eq!(ring.pop(), None);
    }
}
