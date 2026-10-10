use std::path::PathBuf;
use slot_store::{Cart, Platform};
use slot_ui::{cart_face_with, cart_face_with_material, LABEL_X, LABEL_Y, LABEL_W, LABEL_H};

fn cart() -> Cart {
    Cart {
        platform: Platform::Gba,
        stem: "Advance Wars 2 - Black Hole Rising (USA)".into(),
        title: "Advance Wars 2".into(),
        code: "AW2E".into(),
        rom: PathBuf::new(),
        label: None,
        shell: None,
    }
}

#[test]
fn satin_finish_is_stable_and_preserves_original_slot_shape_and_label() {
    let cart = cart();
    let plain = cart_face_with(&cart, None);
    let material = cart_face_with_material(&cart, None);
    let material2 = cart_face_with_material(&cart, None);
    assert_eq!((plain.w, plain.h), (material.w, material.h));
    assert_eq!(material.rgba, material2.rgba, "material must never flicker");
    let mut changes = 0usize;
    let mut clearly_visible = 0usize;
    for (old, sat) in plain.rgba.chunks_exact(4).zip(material.rgba.chunks_exact(4)) {
        assert_eq!(old[3], sat[3], "upstream silhouette must remain identical");
        for c in 0..3 {
            let delta = (old[c] as i16 - sat[c] as i16).abs();
            assert!(delta <= 27, "material shade must not distort original colours");
            if delta >= 6 { clearly_visible += 1; }
            if delta != 0 { changes += 1; }
        }
    }
    assert!(changes > 100, "material finish should change the plastic");
    assert!(clearly_visible > 4000, "softbox reflections should be visible on screen");
    // The paper label must remain pixel-perfect and legible.
    for y in LABEL_Y..(LABEL_Y + LABEL_H) {
        for x in LABEL_X..(LABEL_X + LABEL_W) {
            let at = ((y * plain.w + x) * 4) as usize;
            assert_eq!(&plain.rgba[at..at + 4], &material.rgba[at..at + 4],
                "the label must never inherit plastic grain");
        }
    }
}
