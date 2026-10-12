//! Per-frame emulation side of the original Slot link protocol.
//! The socket worker owns blocking I/O. This adapter only polls packets on
//! GLSurfaceView's GL thread, where libretro, RA evaluation and graphics live.
//! Inspired by Slot emu.rs BeginCable/BeginLink paths (GPL-3.0-or-later).

use slot_retro::{ButtonMask, Link, LinkChannel, RetroCore, NETPACKET_RELIABLE};
use std::time::{Duration, Instant};

use crate::cable::{self, Cable};
use crate::link_kind::LinkKind;
use crate::link_state;
use crate::multiplayer::Role;

const MAX_RECEIVE_PER_FRAME: usize = 256;
const MAX_INITIAL_STATE: usize = 16 * 1024 * 1024;
const MAX_FUTURE_FRAMES: u64 = 256;
const INITIAL_STATE_TIMEOUT: Duration = Duration::from_secs(12);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LinkError {
    Disconnected,
    PeerEnded,
    BiosMismatch,
    InvalidState,
    Timeout,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Frame {
    Advanced,
    Waiting,
    Failed(LinkError),
}

pub struct LinkPlay {
    wire: Box<dyn LinkChannel>,
    cable: Option<Cable>,
    mode: LinkKind,
    net: Link,
    error: Option<LinkError>,
    state_bytes: usize,
    started: Instant,
    // The cable protocol deliberately has a 3-frame input delay, as upstream.
    // Wireless passes bytes through the core's registered netpacket ABI.
}

impl LinkPlay {
    /// Host sends the authoritative core state before receiving frame inputs.
    /// The guest restores this state before running any linked frame.
    pub fn begin(
        core: &mut dyn RetroCore,
        role: Role,
        mode: LinkKind,
        mut wire: Box<dyn LinkChannel>,
    ) -> Result<Self, String> {
        let net = core.net();
        net.clear();
        net.set_active(false);
        // Original Slot's Cable fallback models two joypads in a single
        // libretro core. gpSP actually emulates a SECOND GBA through its
        // netpacket interface and needs its serial mode to exchange data;
        // its Cable and Wireless paths must BOTH use netpacket, not lockstep.
        // Keep the original Cable simulation only for upstream mock tests;
        // GameSession rejects any unsupported real core before reaching here.
        let cable = match mode {
            LinkKind::Cable if !core.supports_netpacket() => {
                let player = if role == Role::Host { 0 } else { 1 };
                let c = Cable::new(player);
                if role == Role::Host {
                    let state = core.serialize().map_err(|e| e.to_string())?;
                    if state.len() > MAX_INITIAL_STATE {
                        return Err("Link state exceeds the safe transport limit".into());
                    }
                    for packet in cable::state_packets(&state) {
                        wire.send(NETPACKET_RELIABLE, &packet);
                    }
                }
                Some(c)
            }
            LinkKind::Cable | LinkKind::Wireless => {
                core.start_link(if role == Role::Host { 0 } else { 1 });
                net.set_active(true);
                None
            }
        };
        Ok(Self {
            wire, cable, mode, net, error: None, state_bytes: 0,
            started: Instant::now(),
        })
    }

    pub fn mode(&self) -> LinkKind { self.mode }

    pub fn fault(&self) -> Option<LinkError> { self.error }

    /// A pending packet must never block a GPU frame. The emulation is held
    /// at the exact same frame until both input masks are available.
    pub fn frame(&mut self, core: &mut dyn RetroCore, buttons: ButtonMask) -> Frame {
        if let Some(error) = self.error { return Frame::Failed(error); }
        if self.wire.peer_ended() { return self.fail(LinkError::PeerEnded); }
        if self.wire.is_closed() { return self.fail(LinkError::Disconnected); }

        for _ in 0..MAX_RECEIVE_PER_FRAME {
            let Some(packet) = self.wire.try_recv() else { break };
            if let Some(cable) = self.cable.as_mut() {
                if matches!(packet.first(), Some(1 | 2)) {
                    self.state_bytes = self.state_bytes.saturating_add(packet.len().saturating_sub(1));
                    if self.state_bytes > MAX_INITIAL_STATE { return self.fail(LinkError::InvalidState); }
                }
                let completed = matches!(packet.first(), Some(2));
                if let Some((frame, _)) = cable::decode(&packet) {
                    if frame > cable.frame().saturating_add(MAX_FUTURE_FRAMES) {
                        return self.fail(LinkError::InvalidState);
                    }
                }
                cable.accept(&packet);
                if completed { self.state_bytes = 0; }
            } else {
                self.net.push_inbound(packet);
            }
        }
        if let Some(cable) = self.cable.as_mut() {
            if let Some(state) = cable.take_state() {
                let local = match core.serialize() {
                    Ok(local) => local,
                    Err(_) => return self.fail(LinkError::InvalidState),
                };
                if !link_state::same_bios(&state, &local) {
                    return self.fail(LinkError::BiosMismatch);
                }
                if core.unserialize(&state).is_err() {
                    return self.fail(LinkError::InvalidState);
                }
                cable.prime();
                self.wire.send(NETPACKET_RELIABLE, &cable::ready_packet());
            }
            if !cable.primed() {
                if self.started.elapsed() >= INITIAL_STATE_TIMEOUT {
                    return self.fail(LinkError::Timeout);
                }
                return Frame::Waiting;
            }
            // The original sampler is idempotent across a stalled frame.
            self.wire.send(NETPACKET_RELIABLE, &cable.sample(buttons));
            if let Some((p0, p1)) = cable.ready() {
                core.run_frame_linked(p0, p1);
                cable.advance();
                Frame::Advanced
            } else {
                cable.stall();
                if cable.stalled() >= cable::QUIET_FRAMES {
                    self.fail(LinkError::Timeout)
                } else {
                    Frame::Waiting
                }
            }
        } else {
            core.pump_link();
            self.flush();
            core.run_frame(buttons);
            core.pump_link();
            self.flush();
            Frame::Advanced
        }
    }

    fn flush(&mut self) {
        for _ in 0..MAX_RECEIVE_PER_FRAME {
            let Some(packet) = self.net.take_outbound() else { break };
            self.wire.send(NETPACKET_RELIABLE, &packet);
        }
    }

    fn fail(&mut self, error: LinkError) -> Frame {
        self.error = Some(error);
        Frame::Failed(error)
    }

    pub fn end(&mut self, core: &mut dyn RetroCore) {
        if self.cable.is_none() {
            core.stop_link();
        }
        self.wire.send_end();
        self.net.clear();
        self.net.set_active(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use slot_retro::MockCore;

    struct Wire {
        recv: Arc<Mutex<VecDeque<Vec<u8>>>>,
        send: Arc<Mutex<VecDeque<Vec<u8>>>>,
    }
    impl LinkChannel for Wire {
        fn send(&mut self, _flags: i32, buf: &[u8]) {
            self.send.lock().unwrap().push_back(buf.to_vec());
        }
        fn try_recv(&mut self) -> Option<Vec<u8>> {
            self.recv.lock().unwrap().pop_front()
        }
    }
    fn pair() -> (Box<dyn LinkChannel>, Box<dyn LinkChannel>) {
        let a = Arc::new(Mutex::new(VecDeque::new()));
        let b = Arc::new(Mutex::new(VecDeque::new()));
        (Box::new(Wire { recv: a.clone(), send: b.clone() }),
         Box::new(Wire { recv: b, send: a }))
    }

    #[test]
    fn host_and_guest_share_exact_snapshot_before_linked_frames() {
        let mut host = MockCore::new();
        let mut guest = MockCore::new();
        for _ in 0..9 { host.run_frame(ButtonMask(0)); }
        let (host_wire, guest_wire) = pair();
        let mut h = LinkPlay::begin(&mut host, Role::Host, LinkKind::Cable, host_wire).unwrap();
        let mut g = LinkPlay::begin(&mut guest, Role::Join, LinkKind::Cable, guest_wire).unwrap();
        assert_eq!(g.frame(&mut guest, ButtonMask(1)), Frame::Advanced);
        assert_eq!(h.frame(&mut host, ButtonMask(2)), Frame::Advanced);
        assert_eq!(host.serialize().unwrap(), guest.serialize().unwrap());
        for _ in 0..3 {
            let _ = g.frame(&mut guest, ButtonMask(1));
            let _ = h.frame(&mut host, ButtonMask(2));
        }
        assert_eq!(host.serialize().unwrap(), guest.serialize().unwrap());
        h.end(&mut host);
        g.end(&mut guest);
    }

    #[test]
    fn empty_network_does_not_advance_guest_or_serialize_bad_state() {
        let mut core = MockCore::new();
        let (wire, _other) = pair();
        let mut link = LinkPlay::begin(&mut core, Role::Join, LinkKind::Cable, wire).unwrap();
        let before = core.serialize().unwrap();
        assert_eq!(link.frame(&mut core, ButtonMask(1)), Frame::Waiting);
        assert_eq!(before, core.serialize().unwrap());
    }
}
