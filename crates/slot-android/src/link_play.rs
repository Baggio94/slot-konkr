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
    // On linked mGBA, Host must import Join's own battery-backed GBA
    // before sending the authoritative paired state. Never seed a guest
    // with Host's blank peer cartridge.
    awaiting_join_seed: bool,
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
        let mut awaiting_join_seed = false;
        let cable = match mode {
            LinkKind::Cable if !core.supports_netpacket() => {
                let player = if role == Role::Host { 0 } else { 1 };
                let c = Cable::new(player);
                // Both devices start with their *own* cartridge progress.
                // mGBA's private SLK1 pair contains a blank remote GBA;
                // exchange Join's local half first, then Host combines
                // player 0 + player 1. Upstream single-state mocks retain
                // the old authoritative Host-only protocol.
                let state = core.serialize().map_err(|e| e.to_string())?;
                if state.len() > MAX_INITIAL_STATE {
                    return Err("Link state exceeds the safe transport limit".into());
                }
                if link_state::is_pair(&state) {
                    if role == Role::Host {
                        if link_state::local(&state, 0).is_none() {
                            return Err("Invalid mGBA Host SLK1 state".into());
                        }
                        awaiting_join_seed = true;
                    } else {
                        let own = link_state::local(&state, 1)
                            .ok_or("Invalid mGBA Join SLK1 state")?;
                        if own.len() < 0x400 || own.len() > MAX_INITIAL_STATE / 2 {
                            return Err("Unsafe mGBA Join state size".into());
                        }
                        for packet in cable::state_packets(&own) {
                            wire.send(NETPACKET_RELIABLE, &packet);
                        }
                    }
                } else if role == Role::Host {
                    for packet in cable::state_packets(&state) {
                        wire.send(NETPACKET_RELIABLE, &packet);
                    }
                }
                Some(c)
            }
            LinkKind::Cable | LinkKind::Wireless => {
                if !core.try_start_link(if role == Role::Host { 0 } else { 1 }) {
                    return Err("Emulation core refused the GBA Link peer in its current serial mode".into());
                }
                net.set_active(true);
                None
            }
        };
        Ok(Self {
            wire, cable, mode, net, error: None, state_bytes: 0,
            awaiting_join_seed, started: Instant::now(),
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
            if self.awaiting_join_seed {
                if let Some(join_state) = cable.take_state() {
                    self.state_bytes = 0;
                    let host_state = match core.serialize() {
                        Ok(state) => state,
                        Err(_) => return self.fail(LinkError::InvalidState),
                    };
                    if !link_state::same_bios(&host_state, &join_state) {
                        return self.fail(LinkError::BiosMismatch);
                    }
                    let combined = match link_state::combine_players(&host_state, &join_state) {
                        Some(state) => state,
                        None => return self.fail(LinkError::InvalidState),
                    };
                    // On a failed restore the patched core rolls BOTH
                    // GBAs back. Never send a potentially invalid pair.
                    if core.unserialize(&combined).is_err() {
                        return self.fail(LinkError::InvalidState);
                    }
                    for packet in cable::state_packets(&combined) {
                        self.wire.send(NETPACKET_RELIABLE, &packet);
                    }
                    self.awaiting_join_seed = false;
                }
                if self.awaiting_join_seed {
                    if self.started.elapsed() >= INITIAL_STATE_TIMEOUT {
                        return self.fail(LinkError::Timeout);
                    }
                    return Frame::Waiting;
                }
            }
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
            // Host is primed at construction but MUST NOT advance until
            // Join acknowledges the final combined SLK1 restore.
            if !cable.armed() {
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
    use std::sync::atomic::{AtomicBool, Ordering};
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

    // Simulated transport controls let us prove a lost peer NEVER advances
    // either core after failure, even when its socket worker lags a frame.
    struct FaultWire {
        disconnected: Arc<AtomicBool>,
        peer_ended: Arc<AtomicBool>,
        sent_end: Arc<AtomicBool>,
    }
    impl LinkChannel for FaultWire {
        fn send(&mut self, _flags: i32, _buf: &[u8]) {}
        fn try_recv(&mut self) -> Option<Vec<u8>> { None }
        fn is_closed(&self) -> bool { self.disconnected.load(Ordering::SeqCst) }
        fn peer_ended(&self) -> bool { self.peer_ended.load(Ordering::SeqCst) }
        fn send_end(&mut self) { self.sent_end.store(true, Ordering::SeqCst); }
    }
    fn fault_wire() -> (Box<dyn LinkChannel>, Arc<AtomicBool>, Arc<AtomicBool>, Arc<AtomicBool>) {
        let disconnected = Arc::new(AtomicBool::new(false));
        let peer_ended = Arc::new(AtomicBool::new(false));
        let sent_end = Arc::new(AtomicBool::new(false));
        (Box::new(FaultWire {
            disconnected: disconnected.clone(),
            peer_ended: peer_ended.clone(),
            sent_end: sent_end.clone(),
        }), disconnected, peer_ended, sent_end)
    }

    // Exercises the actual gpSP path, not the original mGBA single-core
    // lockstep simulator. Both visual Cable and Wireless share netpacket.
    struct NetworkCore {
        inner: MockCore,
        channel: Link,
        received: usize,
        began: bool,
        ended: bool,
    }
    impl NetworkCore {
        fn new() -> Self {
            Self { inner: MockCore::new(), channel: Link::default(),
                   received: 0, began: false, ended: false }
        }
    }
    impl RetroCore for NetworkCore {
        fn load(&mut self, path: &std::path::Path) -> Result<(), slot_retro::CoreError> {
            self.inner.load(path)
        }
        fn run_frame(&mut self, input: ButtonMask) {
            self.inner.run_frame(input);
            if self.channel.is_active() {
                self.channel.push_outbound(vec![0x47, input.0 as u8]);
            }
        }
        fn video_xrgb8888(&self) -> &[u8] { self.inner.video_xrgb8888() }
        fn take_audio(&mut self) -> Vec<i16> { self.inner.take_audio() }
        fn serialize(&mut self) -> Result<Vec<u8>, slot_retro::CoreError> {
            self.inner.serialize()
        }
        fn unserialize(&mut self, data: &[u8]) -> Result<(), slot_retro::CoreError> {
            self.inner.unserialize(data)
        }
        fn save_ram(&self) -> Option<Vec<u8>> { self.inner.save_ram() }
        fn load_save_ram(&mut self, data: &[u8]) -> Result<(), slot_retro::CoreError> {
            self.inner.load_save_ram(data)
        }
        fn av_info(&self) -> slot_retro::AvInfo { self.inner.av_info() }
        fn net(&self) -> Link { self.channel.clone() }
        fn supports_netpacket(&self) -> bool { true }
        fn start_link(&mut self, _client_id: u16) { self.began = true; }
        fn try_start_link(&mut self, _client_id: u16) -> bool {
            self.began = true;
            self.channel.set_active(true);
            true
        }
        fn pump_link(&mut self) {
            while self.channel.take_inbound().is_some() { self.received += 1; }
        }
        fn stop_link(&mut self) { self.ended = true; }
    }

    // Minimal dual-GBA adapter: snapshots are real SLK1 containers with
    // distinct player bytes, but no native mGBA is needed for protocol tests.
    struct PairedCore {
        state: Vec<u8>,
        steps: usize,
    }
    impl PairedCore {
        fn new(progress: u8, bios: u32) -> Self {
            let mut single = vec![progress; 0x800];
            single[4..8].copy_from_slice(&bios.to_le_bytes());
            Self { state: link_state::pair(&single), steps: 0 }
        }
    }
    impl RetroCore for PairedCore {
        fn load(&mut self, _: &std::path::Path) -> Result<(), slot_retro::CoreError> { Ok(()) }
        fn run_frame(&mut self, _: ButtonMask) { self.steps += 1; }
        fn run_frame_linked(&mut self, _: ButtonMask, _: ButtonMask) { self.steps += 1; }
        fn video_xrgb8888(&self) -> &[u8] { &[] }
        fn take_audio(&mut self) -> Vec<i16> { Vec::new() }
        fn serialize(&mut self) -> Result<Vec<u8>, slot_retro::CoreError> {
            Ok(self.state.clone())
        }
        fn unserialize(&mut self, state: &[u8]) -> Result<(), slot_retro::CoreError> {
            if link_state::local(state, 0).is_none()
                || link_state::local(state, 1).is_none() {
                return Err(slot_retro::CoreError::State("not a pair".into()));
            }
            self.state = state.to_vec();
            Ok(())
        }
        fn save_ram(&self) -> Option<Vec<u8>> { None }
        fn load_save_ram(&mut self, _: &[u8]) -> Result<(), slot_retro::CoreError> { Ok(()) }
        fn av_info(&self) -> slot_retro::AvInfo {
            slot_retro::AvInfo { fps: 60.0, sample_rate: 32768.0 }
        }
    }

    #[test]
    fn mga_link_merges_distinct_host_and_join_saves_before_first_frame() {
        let (host_wire, guest_wire) = pair();
        let (mut host, mut guest) = (PairedCore::new(0x11, 0xBAAE187F),
                                      PairedCore::new(0x22, 0xBAAE187F));
        let mut h = LinkPlay::begin(&mut host, Role::Host, LinkKind::Cable, host_wire).unwrap();
        // Never advance Host's blank peer before Join supplies its GBA.
        assert_eq!(h.frame(&mut host, ButtonMask(1)), Frame::Waiting);
        assert_eq!(host.steps, 0);
        let mut g = LinkPlay::begin(&mut guest, Role::Join, LinkKind::Cable, guest_wire).unwrap();
        assert_eq!(h.frame(&mut host, ButtonMask(2)), Frame::Waiting);
        assert_eq!(host.steps, 0);
        let host_local = link_state::local(&host.state, 0).unwrap();
        let guest_local = link_state::local(&host.state, 1).unwrap();
        assert_eq!(host_local[12], 0x11);
        assert_eq!(guest_local[12], 0x22);
        // Join must restore the combined pair before receiving any frame.
        assert_eq!(g.frame(&mut guest, ButtonMask(3)), Frame::Advanced);
        assert_eq!(g.state_bytes, 0);
        assert_eq!(h.frame(&mut host, ButtonMask(4)), Frame::Advanced);
        assert_eq!(host.state, guest.state);
        assert_eq!(host.steps, 1);
        assert_eq!(guest.steps, 1);
    }

    #[test]
    fn mga_link_rejects_guest_with_different_bios_without_replacing_host() {
        let (host_wire, guest_wire) = pair();
        let (mut host, mut guest) = (PairedCore::new(0x11, 0xBAAE187F),
                                      PairedCore::new(0x22, 0xA6473709));
        let before = host.state.clone();
        let mut h = LinkPlay::begin(&mut host, Role::Host, LinkKind::Cable, host_wire).unwrap();
        let _g = LinkPlay::begin(&mut guest, Role::Join, LinkKind::Cable, guest_wire).unwrap();
        assert_eq!(h.frame(&mut host, ButtonMask(0)), Frame::Failed(LinkError::BiosMismatch));
        assert_eq!(host.state, before);
        assert_eq!(host.steps, 0);
    }

    #[test]
    fn gpsp_netpacket_carries_both_cable_and_wireless_without_input_replay() {
        for mode in [LinkKind::Cable, LinkKind::Wireless] {
            let (hw, gw) = pair();
            let (mut host, mut guest) = (NetworkCore::new(), NetworkCore::new());
            let mut h = LinkPlay::begin(&mut host, Role::Host, mode, hw).unwrap();
            let mut g = LinkPlay::begin(&mut guest, Role::Join, mode, gw).unwrap();
            assert!(host.began && guest.began);
            assert_eq!(h.mode(), mode);
            assert_eq!(g.mode(), mode);
            assert_eq!(h.frame(&mut host, ButtonMask(10)), Frame::Advanced);
            assert_eq!(g.frame(&mut guest, ButtonMask(11)), Frame::Advanced);
            assert_eq!(guest.received, 1, "mode={mode:?} must deliver host netpacket");
            assert_eq!(h.frame(&mut host, ButtonMask(12)), Frame::Advanced);
            assert_eq!(host.received, 1, "mode={mode:?} must deliver guest netpacket");
            h.end(&mut host);
            g.end(&mut guest);
            assert!(host.ended && guest.ended);
        }
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
    fn lost_gpsp_peer_never_advances_after_disconnect_in_either_serial_mode() {
        for mode in [LinkKind::Cable, LinkKind::Wireless] {
            let (wire, closed, _peer_ended, sent_end) = fault_wire();
            let mut core = NetworkCore::new();
            let mut link = LinkPlay::begin(&mut core, Role::Join, mode, wire).unwrap();
            assert!(core.channel.is_active());
            assert_eq!(link.frame(&mut core, ButtonMask(1)), Frame::Advanced);
            let before = core.serialize().unwrap();
            closed.store(true, Ordering::SeqCst);
            for _ in 0..3 {
                assert_eq!(link.frame(&mut core, ButtonMask(2)),
                    Frame::Failed(LinkError::Disconnected));
                assert_eq!(core.serialize().unwrap(), before,
                    "a disconnected gpSP {mode:?} core must not emulate a solo frame");
            }
            link.end(&mut core);
            assert!(core.ended);
            assert!(sent_end.load(Ordering::SeqCst));
            assert!(!core.channel.is_active());
        }
    }

    #[test]
    fn ended_mgba_peer_keeps_both_cartridges_unchanged_until_safe_eject() {
        let (wire, _closed, peer_ended, sent_end) = fault_wire();
        let mut core = PairedCore::new(0x42, 0xBAAE187F);
        let mut link = LinkPlay::begin(&mut core, Role::Host, LinkKind::Cable, wire).unwrap();
        let before = core.serialize().unwrap();
        assert_eq!(link.frame(&mut core, ButtonMask(1)), Frame::Waiting);
        peer_ended.store(true, Ordering::SeqCst);
        assert_eq!(link.frame(&mut core, ButtonMask(1)),
            Frame::Failed(LinkError::PeerEnded));
        assert_eq!(link.frame(&mut core, ButtonMask(1)),
            Frame::Failed(LinkError::PeerEnded));
        assert_eq!(core.serialize().unwrap(), before);
        assert_eq!(core.steps, 0);
        link.end(&mut core);
        assert!(sent_end.load(Ordering::SeqCst));
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
