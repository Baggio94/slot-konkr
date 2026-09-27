//! Whether a picture is drawn at its own size or stretched over the whole panel, and which of
//! the two each cart was last left in.
//!
//! Game Boy and Game Boy Color carts only, since they have no shoulder buttons: L stretches,
//! R gives the largest whole multiple, centred. A GBA picture already fills the panel at 3x.
//! The stretch distorts (10:9 on a 3:2 panel) by request.

use std::path::Path;

use slot_gfx::{SRC_H, SRC_W, WHOLE_TEXTURE};
use slot_store::{ini, Platform};

/// Same `<stem> = <value>` shape as `selected_core.ini`. Stem-keyed, so same-named GB and GBC
/// carts share a line; acceptable for a cosmetic preference.
pub const VIDEO_MODE_FILE: &str = "System/video_mode.ini";

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum VideoMode {
    /// The largest whole multiple the panel holds, centred (a Game Boy's 160x144 at 480x432).
    #[default]
    Actual,
    /// The picture over the whole panel, aspect and all.
    Stretch,
}

impl VideoMode {
    /// The ini's spelling, meant to be typed by hand.
    pub fn as_str(self) -> &'static str {
        match self {
            VideoMode::Actual => "actual",
            VideoMode::Stretch => "stretch",
        }
    }

    pub fn parse(s: &str) -> Option<VideoMode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "actual" => Some(VideoMode::Actual),
            "stretch" => Some(VideoMode::Stretch),
            _ => None,
        }
    }
}

/// The mode one cart was last left in, `Actual` when missing or unparseable.
pub fn video_mode_for(root: &Path, stem: &str) -> VideoMode {
    ini::value(root, VIDEO_MODE_FILE, stem)
        .as_deref()
        .and_then(VideoMode::parse)
        .unwrap_or_default()
}

/// Set one cart's mode, leaving the rest of the file as it was.
pub fn write_video_mode(root: &Path, stem: &str, mode: VideoMode) -> std::io::Result<()> {
    ini::write(root, VIDEO_MODE_FILE, stem, mode.as_str())
}

/// The part of the frame buffer the panel shows, as origin then size in texture coordinates.
/// Uses `Platform::picture` and the same centring as `video_refresh` so the two cannot drift.
pub fn source_rect(platform: Platform, mode: VideoMode) -> [f32; 4] {
    let (w, h) = platform.picture();
    if mode == VideoMode::Actual || (w, h) == (SRC_W, SRC_H) {
        return WHOLE_TEXTURE;
    }
    let x = SRC_W.saturating_sub(w) / 2;
    let y = SRC_H.saturating_sub(h) / 2;
    [
        x as f32 / SRC_W as f32,
        y as f32 / SRC_H as f32,
        w as f32 / SRC_W as f32,
        h as f32 / SRC_H as f32,
    ]
}
