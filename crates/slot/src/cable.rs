//! The emulated cable's frame clock: whose buttons go into which frame, and when a frame may run.
//!
//! In-core lockstep needs both players' buttons before `run_frame_linked` can step, and a frame
//! run on a guess cannot be undone. So a mask sampled now is stamped `DELAY` frames ahead and sent
//! at once; both ends seed the first `DELAY` frames as idle. No sockets here, so it is testable.

use std::collections::BTreeMap;

use slot_retro::ButtonMask;

/// Frames between sampling a mask and running it. Two devices' presents are phase-offset, so each
/// waits on the other: at two, measured waits of ~6 ms per 16.7 ms frame caused stall bursts and
/// choppy audio. Three gives 50 ms of slack against ~2 ms of wire, for one frame of input lag.
pub const DELAY: u64 = 3;

/// One player's buttons for one frame: kind, frame index, mask, little endian.
const PACKET: usize = 11;

const KIND_MASK: u8 = 0;
const KIND_STATE: u8 = 1;
const KIND_STATE_END: u8 = 2;
/// The joiner saying it is running the host's machine. Until then the host cannot tell a joiner
/// still restoring state from one that has left.
const KIND_READY: u8 = 3;

/// The transport frames with a `u16` length, so state crosses in chunks under 65535 with the
/// kind byte.
const CHUNK: usize = 60_000;

pub fn encode(frame: u64, mask: ButtonMask) -> [u8; PACKET] {
    let mut out = [0u8; PACKET];
    out[0] = KIND_MASK;
    out[1..9].copy_from_slice(&frame.to_le_bytes());
    out[9..].copy_from_slice(&mask.0.to_le_bytes());
    out
}

pub fn decode(buf: &[u8]) -> Option<(u64, ButtonMask)> {
    if buf.len() != PACKET || buf[0] != KIND_MASK {
        return None;
    }
    let frame = u64::from_le_bytes(buf[1..9].try_into().ok()?);
    let mask = u16::from_le_bytes(buf[9..].try_into().ok()?);
    Some((frame, ButtonMask(mask)))
}

pub fn ready_packet() -> [u8; 1] {
    [KIND_READY]
}

/// A serialized pair in wire-sized pieces. The last is `KIND_STATE_END`, so no length is sent.
pub fn state_packets(state: &[u8]) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = state
        .chunks(CHUNK)
        .map(|c| {
            let mut p = Vec::with_capacity(c.len() + 1);
            p.push(KIND_STATE);
            p.extend_from_slice(c);
            p
        })
        .collect();
    match out.last_mut() {
        Some(last) => last[0] = KIND_STATE_END,
        // An empty state still has to end, or the far end waits forever.
        None => out.push(vec![KIND_STATE_END]),
    }
    out
}

pub struct Cable {
    /// 0 or 1: libretro's client id and the port this device drives.
    player: u8,
    /// The frame `step` will run next.
    frame: u64,
    local: BTreeMap<u64, ButtonMask>,
    remote: BTreeMap<u64, ButtonMask>,
    /// Frames asked for and not run because the peer's mask had not arrived.
    stalled: u32,
    /// Whether this device runs the agreed starting machine: the host at once, the joiner once
    /// it has restored the host's state. Otherwise the two simulate different machines.
    primed: bool,
    /// The state arriving from the host, reassembled. Empty once taken.
    incoming: Vec<u8>,
    /// The reassembled state is whole and waiting to be restored.
    complete: bool,
    /// The far end is running the agreed machine. Until then a stall means "getting ready",
    /// not "gone".
    peer_ready: bool,
}

impl Cable {
    pub fn new(player: u8) -> Self {
        let idle = ButtonMask::default();
        let seed: BTreeMap<u64, ButtonMask> = (0..DELAY).map(|f| (f, idle)).collect();
        Cable {
            player,
            frame: 0,
            local: seed.clone(),
            remote: seed,
            stalled: 0,
            // The host's machine is the one copied, so it needs no priming.
            primed: player == 0,
            incoming: Vec::new(),
            complete: false,
            // The joiner's peer is the host.
            peer_ready: player != 0,
        }
    }

    pub fn primed(&self) -> bool {
        self.primed
    }

    /// Whether a stall means the peer is late. A restoring joiner stalls by design, and
    /// counting that as a lost peer would end every join.
    pub fn armed(&self) -> bool {
        self.primed && self.peer_ready
    }

    /// The host's machine, once whole. The caller restores it (only the worker holds the core)
    /// and then calls `prime`.
    pub fn take_state(&mut self) -> Option<Vec<u8>> {
        if !self.complete {
            return None;
        }
        self.complete = false;
        Some(std::mem::take(&mut self.incoming))
    }

    /// Marks this device as running the agreed machine. Frames can run from here.
    pub fn prime(&mut self) {
        self.primed = true;
    }

    pub fn player(&self) -> u8 {
        self.player
    }

    /// The frame about to run. Read by tests holding the delay to a constant.
    pub fn frame(&self) -> u64 {
        self.frame
    }

    pub fn stalled(&self) -> u32 {
        self.stalled
    }

    /// This device's buttons for the frame `DELAY` ahead. Decided once: a stalled present asking
    /// again gets the same mask. Revising it could desync the pair if the peer already ran it,
    /// and restamping per present would add unbounded input delay.
    pub fn sample(&mut self, mask: ButtonMask) -> [u8; PACKET] {
        let frame = self.frame + DELAY;
        let mask = *self.local.entry(frame).or_insert(mask);
        encode(frame, mask)
    }

    /// A packet off the wire. Unparseable or stale frames are dropped, not errors.
    pub fn accept(&mut self, buf: &[u8]) -> bool {
        match buf.first() {
            Some(&KIND_READY) => {
                self.peer_ready = true;
                return true;
            }
            Some(&KIND_STATE) => {
                self.incoming.extend_from_slice(&buf[1..]);
                return true;
            }
            Some(&KIND_STATE_END) => {
                self.incoming.extend_from_slice(&buf[1..]);
                self.complete = true;
                return true;
            }
            _ => {}
        }
        let Some((frame, mask)) = decode(buf) else {
            return false;
        };
        if frame < self.frame {
            return false;
        }
        self.remote.insert(frame, mask);
        true
    }

    /// The two masks for the frame about to run, in port order (swapping them swaps the
    /// consoles), or `None` until the peer's arrives.
    pub fn ready(&self) -> Option<(ButtonMask, ButtonMask)> {
        if !self.primed {
            return None;
        }
        let mine = *self.local.get(&self.frame)?;
        let theirs = *self.remote.get(&self.frame)?;
        match self.player {
            0 => Some((mine, theirs)),
            _ => Some((theirs, mine)),
        }
    }

    /// Consumes the frame `ready` answered for.
    pub fn advance(&mut self) {
        self.local.remove(&self.frame);
        self.remote.remove(&self.frame);
        self.frame += 1;
        self.stalled = 0;
    }

    /// The peer is late. Counted to tell a hiccup from a peer that stopped talking.
    pub fn stall(&mut self) {
        self.stalled = self.stalled.saturating_add(1);
    }
}

/// Stalled presents before a peer is lost: about a second at 60 Hz, enough to ride out a WiFi
/// hiccup.
pub const QUIET_FRAMES: u32 = 60;
