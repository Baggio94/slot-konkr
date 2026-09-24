//! The game layer powering on as a line that blooms, and off as a line that closes to a dot.
//! `t` is 0.0 dark and 1.0 fully on; power off walks the same curve backwards.

use crate::surface::{OUT_H, OUT_W};

/// Height of the line. Two pixels, since an odd height centred in an even frame lands on a
/// half pixel and reads as grey.
const LINE_PX: f32 = 2.0;

/// The tail of the travel the horizontal collapse gets. More reads as an iris closing.
const DOT_T: f32 = 0.12;

/// How far past normal brightness the strike goes. It peaks while the picture is a line.
const OVERSHOOT: f32 = 0.6;

/// Height of the picture as a fraction of the frame, eased out.
pub fn screen_scale(t: f32) -> f32 {
    let left = 1.0 - t.clamp(0.0, 1.0);
    1.0 - (1.0 - LINE_PX / OUT_H as f32) * left * left
}

/// Width, which only shrinks over the last `DOT_T` of the travel.
pub fn screen_width(t: f32) -> f32 {
    let left = 1.0 - (t.clamp(0.0, 1.0) / DOT_T).min(1.0);
    1.0 - (1.0 - LINE_PX / OUT_W as f32) * left
}

/// Gain on the game layer: brightest as the line appears, exactly 1.0 when fully on.
pub fn screen_brightness(t: f32) -> f32 {
    let left = 1.0 - t.clamp(0.0, 1.0);
    1.0 + OVERSHOOT * left * left
}

/// The centred rect the game layer fills, in offscreen pixels.
pub fn screen_rect(t: f32) -> (f32, f32, f32, f32) {
    let w = OUT_W as f32 * screen_width(t);
    let h = OUT_H as f32 * screen_scale(t);
    ((OUT_W as f32 - w) / 2.0, (OUT_H as f32 - h) / 2.0, w, h)
}
