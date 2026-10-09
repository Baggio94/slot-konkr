//! Android host for the unmodified Slot cartridge graphics and shelf behavior.
//!
//! Stage M1: display-only preview carts, not executable games.
mod library;
#[cfg(target_os = "android")]
mod game;
#[cfg(target_os = "android")]
mod runtime;

// Reuse upstream Slot's unmodified mechanical core picker: 420ms open,
// 180ms chip hop, 320ms close and refusal shake.
#[path = "../../slot/src/core_picker.rs"]
mod core_picker;
