//! Minimal Android mirror of upstream Slot's GameMenu transition model.
//! It exists ONLY to compile the ORIGINAL link_screen.rs renderer unmodified.
//! Network and emulation lifecycle are handled by multiplayer.rs/link_play.rs.

use slot_ui::Millis;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LinkRow { Host, Join }

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LinkStep { Radio, Waiting }

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LinkFail { Radio, NobodyCame, PeerVanished, Cancelled }

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GameMenu {
    Pick(LinkRow),
    Working { role: LinkRow, step: LinkStep, since: Millis },
    Linked { role: LinkRow, worked: Millis, since: Millis, opened: bool },
    Failed { role: LinkRow, fail: LinkFail, worked: Millis, since: Millis },
    Unplug { role: LinkRow, since: Millis },
}
