//! Build-time-only export of EXACT Slot core-picker keycap faces for the
//! Android Apps drawer. Reuse the upstream drawing/font code, not an Android
//! imitation with different fonts or Unicode arrow glyphs.
use slot_ui::{arrows_hint_face, hint_face, UndoFace};
use std::{fs::{self, File}, io::BufWriter, path::PathBuf};

fn write_png(out: &PathBuf, face: &UndoFace) -> Result<(), Box<dyn std::error::Error>> {
    let file = BufWriter::new(File::create(out)?);
    let mut encoder = png::Encoder::new(file, face.w, face.h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&face.rgba)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let destination = std::env::args().nth(1)
        .ok_or("Usage: cargo run -p slot-ui --example export_apps_legend -- <output-dir>")?;
    let destination = PathBuf::from(destination);
    fs::create_dir_all(&destination)?;
    for (filename, face) in [
        ("move.png", arrows_hint_face("Move")),
        ("open.png", hint_face("A", "Open")),
        ("back.png", hint_face("B", "Back")),
        // Shared authentic keycaps for Android dialogs / Studio / RA panel.
        ("select.png", hint_face("A", "Select")),
        ("refresh.png", hint_face("START", "Refresh")),
        ("sign-out.png", hint_face("X", "Sign Out")),
    ] {
        assert!(face.w > 20 && face.h == 24, "Slot legend generation failed");
        assert_eq!(face.rgba.len(), (face.w * face.h * 4) as usize);
        write_png(&destination.join(filename), &face)?;
        println!("{}: {}×{} (upstream Slot pixels)", filename, face.w, face.h);
    }
    Ok(())
}
