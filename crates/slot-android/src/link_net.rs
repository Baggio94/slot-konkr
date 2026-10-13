//! GPL-3.0-or-later: fork LAN greeting and original Slot TCP packet framing.
//! Source: https://github.com/pvaibhav/slot/blob/pvaibhav-improvements/crates/slot/src/link_net.rs
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use slot_retro::LinkChannel;

#[derive(Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Cancel {
        Cancel::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

const POLL_MS: u64 = 50;

pub const HOST_BOUND: Duration = Duration::from_secs(30);

/// How long either end gives the other to say or answer the greeting. Short: it is one small
/// packet on a network measured in milliseconds, and a host that cannot manage it in this time
/// is not the one this joiner wants.
const GREET_WAIT: Duration = Duration::from_millis(1500);

/// A greeting is `SLOT`, the greeting's own version, then the game's four-byte header code.
const GREETING_LEN: usize = 9;
const GREETING_VERSION: u8 = 1;
/// What a host answers a greeting it accepts with.
const GREETING_ACK: u8 = 0x06;

/// What opens a connection on a shared network. The game rides along so that two handhelds that
/// found each other but are not running the same cart never become a session; on the private
/// network nothing else could have connected, so nothing needed asking.
fn greeting(game: &str) -> [u8; GREETING_LEN] {
    let mut g = [b' '; GREETING_LEN];
    g[..4].copy_from_slice(b"SLOT");
    g[4] = GREETING_VERSION;
    for (slot, byte) in g[5..].iter_mut().zip(game.bytes()) {
        *slot = byte;
    }
    g
}

/// Read a stranger's greeting and answer it, or say it was not the right one. Never waits
/// longer than `GREET_WAIT`, so one connection that says nothing cannot hold the host up.
fn vet(stream: &mut TcpStream, want: &[u8]) -> bool {
    let mut got = vec![0u8; want.len()];
    stream.set_read_timeout(Some(GREET_WAIT)).is_ok()
        && stream.set_write_timeout(Some(GREET_WAIT)).is_ok()
        && stream.read_exact(&mut got).is_ok()
        && got == want
        && stream.write_all(&[GREETING_ACK]).is_ok()
        && stream.set_read_timeout(None).is_ok()
        && stream.set_write_timeout(None).is_ok()
}

/// Lab wire protocol is intentionally NOT compatible with legacy SLOT/1.
/// A legacy physical Beta5 must never reach Ready or change its emulation core.
const LAB_GREETING_LEN: usize = 4 + 1 + 4 + 1 + 1 + 32 + 32 + 32;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabGreeting {
    pub game: [u8; 4],
    pub backend: u8,
    pub mode: u8,
    pub rom_sha256: [u8; 32],
    pub core_sha256: [u8; 32],
    pub bios_sha256: [u8; 32],
}
impl LabGreeting {
    fn bytes(&self) -> [u8; LAB_GREETING_LEN] {
        let mut bytes = [0u8; LAB_GREETING_LEN];
        bytes[..4].copy_from_slice(b"KMLB");
        bytes[4] = 2; // Lab protocol revision; no implicit downgrade to SLOT/1.
        bytes[5..9].copy_from_slice(&self.game);
        bytes[9] = self.backend;
        bytes[10] = self.mode;
        bytes[11..43].copy_from_slice(&self.rom_sha256);
        bytes[43..75].copy_from_slice(&self.core_sha256);
        bytes[75..107].copy_from_slice(&self.bios_sha256);
        bytes
    }
}

const CONTROL_ENDED: u8 = 0x00;

const BYE_MS: u64 = 100;

enum Out {
    Packet(Vec<u8>),
    Control(u8, Sender<()>),
}

pub struct TcpLink {
    outbox: Sender<Out>,
    inbox: Receiver<Vec<u8>>,
    stream: TcpStream,
    closed: Arc<AtomicBool>,
    ended: Arc<AtomicBool>,
}

impl TcpLink {
    /// WiFi a link session actually runs over. On the private network that is the whole
    /// defence, since only the other handheld is there to connect; on the home network
    /// `host_greeted_until` is the one to use, which also turns away anything that does not
    /// open with the right greeting. See the module doc for why this transport does not
    /// hardcode which address that is.
    pub fn host_until(
        addr: &str,
        port: u16,
        bound: Duration,
        cancel: &Cancel,
    ) -> std::io::Result<TcpLink> {
        TcpLink::host_inner(addr, port, bound, cancel, None, || {})
    }

    /// `host_until` for a shared network, where anything on it can reach the listener. Only a
    /// peer that opens with this game's greeting is accepted; anything else is dropped without
    /// a word and the wait goes on, still bounded. `listening` runs once the port is bound and
    /// before the first wait, which is when a host on a shared network announces itself: a
    /// joiner that found it any sooner would be refused.
    pub fn host_greeted_until(
        addr: &str,
        port: u16,
        bound: Duration,
        cancel: &Cancel,
        game: &str,
        listening: impl FnOnce(),
    ) -> std::io::Result<TcpLink> {
        TcpLink::host_inner(addr, port, bound, cancel, Some(greeting(game).to_vec()), listening)
    }

    /// Lab-only strict negotiation: backend, Cable/Wireless mode, exact ROM,
    /// compiled libretro core and active GBA BIOS must all match BEFORE Ready.
    pub fn host_lab_greeted_until(
        addr: &str, port: u16, bound: Duration, cancel: &Cancel,
        identity: &LabGreeting, listening: impl FnOnce(),
    ) -> std::io::Result<TcpLink> {
        TcpLink::host_inner(addr, port, bound, cancel,
            Some(identity.bytes().to_vec()), listening)
    }

    fn host_inner(
        addr: &str,
        port: u16,
        bound: Duration,
        cancel: &Cancel,
        expect: Option<Vec<u8>>,
        listening: impl FnOnce(),
    ) -> std::io::Result<TcpLink> {
        let listener = TcpListener::bind((addr, port))?;
        listener.set_nonblocking(true)?;
        listening();
        let deadline = Instant::now() + bound;
        loop {
            if cancel.is_cancelled() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "cancelled while waiting for a peer",
                ));
            }
            match listener.accept() {
                Ok((mut stream, _peer)) => {
                    stream.set_nonblocking(false)?;
                    if let Some(ref want) = expect {
                        // Vetted before it is anyone's session: a stranger, a scanner or the
                        // wrong game gets dropped here and the host keeps waiting for the
                        // right one.
                        if !vet(&mut stream, want) {
                            continue;
                        }
                    }
                    return TcpLink::wrap(stream);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e),
            }
            if Instant::now() >= deadline {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "no peer arrived",
                ));
            }
            std::thread::sleep(Duration::from_millis(POLL_MS));
        }
    }

    pub fn host(addr: &str, port: u16) -> std::io::Result<TcpLink> {
        TcpLink::host_until(addr, port, HOST_BOUND, &Cancel::new())
    }

    pub fn join_until(
        addr: &str,
        port: u16,
        bound: Duration,
        cancel: &Cancel,
    ) -> std::io::Result<TcpLink> {
        let deadline = Instant::now() + bound;
        loop {
            if cancel.is_cancelled() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "cancelled while reaching for the host",
                ));
            }
            if let Ok(stream) = TcpStream::connect((addr, port)) {
                return TcpLink::wrap(stream);
            }
            if Instant::now() >= deadline {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "never reached the host",
                ));
            }
            std::thread::sleep(Duration::from_millis(POLL_MS));
        }
    }

    pub fn join(addr: &str, port: u16) -> std::io::Result<TcpLink> {
        TcpLink::wrap(TcpStream::connect((addr, port))?)
    }

    /// One attempt to reach a host on a shared network: connect, open with this game's
    /// greeting, and wait for the host to accept it. Both waits are short and neither retries,
    /// because the caller is walking a list of hosts that answered and the next one is the
    /// retry. A host that turns it away closes the socket, which reads here as an error.
    pub fn join_greeted(addr: std::net::SocketAddr, game: &str) -> std::io::Result<TcpLink> {
        TcpLink::join_with_greeting(addr, &greeting(game))
    }

    pub fn join_lab_greeted(addr: std::net::SocketAddr,
                            identity: &LabGreeting) -> std::io::Result<TcpLink> {
        TcpLink::join_with_greeting(addr, &identity.bytes())
    }

    fn join_with_greeting(addr: std::net::SocketAddr, bytes: &[u8]) -> std::io::Result<TcpLink> {
        let mut stream = TcpStream::connect_timeout(&addr, GREET_WAIT)?;
        stream.set_write_timeout(Some(GREET_WAIT))?;
        stream.write_all(bytes)?;
        stream.set_read_timeout(Some(GREET_WAIT))?;
        let mut ack = [0u8; 1];
        stream.read_exact(&mut ack)?;
        if ack[0] != GREETING_ACK {
            return Err(std::io::Error::other("host did not accept the greeting"));
        }
        stream.set_read_timeout(None)?;
        stream.set_write_timeout(None)?;
        TcpLink::wrap(stream)
    }

    fn wrap(stream: TcpStream) -> std::io::Result<TcpLink> {
        stream.set_nodelay(true)?;
        let mut reader = stream.try_clone()?;
        let mut writer = stream.try_clone()?;
        let (rtx, inbox) = channel();
        let (wtx, wrx) = channel::<Out>();
        let closed = Arc::new(AtomicBool::new(false));
        let reader_closed = closed.clone();
        let ended = Arc::new(AtomicBool::new(false));
        let reader_ended = ended.clone();

        std::thread::spawn(move || {
            let mut header = [0u8; 2];
            let mut control = false;
            loop {
                if reader.read_exact(&mut header).is_err() {
                    reader_closed.store(true, Ordering::Release);
                    return;
                }
                let len = u16::from_be_bytes(header) as usize;
                if len == 0 && !control {
                    control = true;
                    continue;
                }
                let mut buf = vec![0u8; len];
                if len > 0 && reader.read_exact(&mut buf).is_err() {
                    reader_closed.store(true, Ordering::Release);
                    return;
                }
                if std::mem::take(&mut control) {
                    if buf.first() == Some(&CONTROL_ENDED) {
                        reader_ended.store(true, Ordering::Release);
                    }
                    continue;
                }
                if rtx.send(buf).is_err() {
                    return;
                }
            }
        });

        let writer_closed = closed.clone();
        std::thread::spawn(move || {
            for out in wrx.iter() {
                let sent = match out {
                    Out::Packet(buf) => write_packet_frame(&mut writer, &buf),
                    Out::Control(op, ack) => {
                        let result = writer.write_all(&[0, 0, 0, 1, op]);
                        if result.is_ok() { let _ = ack.send(()); }
                        result
                    }
                };
                if sent.is_err() {
                    // A broken write side is a dead Link even when the reader
                    // has not yet observed EOF. Never keep advancing gpSP
                    // serial/RFU frames after a local TCP write failure.
                    writer_closed.store(true, Ordering::Release);
                    return;
                }
            }
        });

        Ok(TcpLink {
            outbox: wtx,
            inbox,
            stream,
            closed,
            ended,
        })
    }

    pub fn send_end(&mut self) {
        let (ack, wrote) = channel();
        if self.outbox.send(Out::Control(CONTROL_ENDED, ack)).is_err() {
            return;
        }
        let _ = wrote.recv_timeout(Duration::from_millis(BYE_MS));
    }

    pub fn nodelay(&self) -> std::io::Result<bool> {
        self.stream.nodelay()
    }
}

/// Every outbound frame must be lossless and fit the fork's u16 wire
/// length. Silently dropping one (or a partial write) would desynchronize
/// the linked emulators, so the writer treats either as a terminal error.
fn write_packet_frame(writer: &mut impl Write, buf: &[u8]) -> std::io::Result<()> {
    let len = u16::try_from(buf.len()).map_err(|_| std::io::Error::new(
        std::io::ErrorKind::InvalidData, "Multiplayer packet exceeds TCP frame limit"))?;
    writer.write_all(&len.to_be_bytes())?;
    writer.write_all(buf)
}

impl Drop for TcpLink {
    fn drop(&mut self) {
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}

impl LinkChannel for TcpLink {
    fn send(&mut self, _flags: i32, buf: &[u8]) {
        if buf.is_empty() {
            return;
        }
        let _ = self.outbox.send(Out::Packet(buf.to_vec()));
    }

    fn try_recv(&mut self) -> Option<Vec<u8>> {
        match self.inbox.try_recv() {
            Ok(p) => Some(p),
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => None,
        }
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    fn send_end(&mut self) {
        TcpLink::send_end(self);
    }

    fn peer_ended(&self) -> bool {
        self.ended.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod lab_negotiation_tests {
    use super::*;
    fn identity() -> LabGreeting {
        LabGreeting { game: *b"BPRE", backend: 1, mode: 1,
            rom_sha256: [3;32], core_sha256: [4;32], bios_sha256: [5;32] }
    }
    #[test]
    fn lab_greeting_is_not_legacy_slot_one() {
        assert_ne!(&identity().bytes()[..9], &greeting("BPRE"));
        assert_eq!(&identity().bytes()[..4], b"KMLB");
    }
    #[test]
    fn tcp_packet_framing_is_exact_and_never_silently_truncates_or_drops() {
        let mut out = Vec::new();
        write_packet_frame(&mut out, b"gba").unwrap();
        assert_eq!(out, vec![0, 3, b'g', b'b', b'a']);
        let oversized = vec![7u8; (u16::MAX as usize) + 1];
        assert_eq!(write_packet_frame(&mut out, &oversized).unwrap_err().kind(),
            std::io::ErrorKind::InvalidData);
        assert_eq!(out, vec![0, 3, b'g', b'b', b'a']);
    }

    #[test]
    fn broken_tcp_write_reports_error_before_link_can_assume_delivery() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "peer gone"))
            }
            fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
        }
        assert_eq!(write_packet_frame(&mut Broken, b"frame").unwrap_err().kind(),
            std::io::ErrorKind::BrokenPipe);
    }

    /// A real loopback listener exercises the actual TCP handshake and the
    /// reader/writer worker threads, not only serialised greeting bytes.
    /// No device/emulator, BIOS or ROM is needed.
    #[test]
    fn strict_kmlb2_host_rejects_wrong_peers_then_transfers_packets_and_end() {
        use slot_retro::NETPACKET_RELIABLE;
        use std::net::{SocketAddr, TcpListener};
        use std::thread;

        // Reserve an OS-assigned TCP port for this test instead of a fixed
        // 7211; tests can run simultaneously in GitHub Actions.
        let reserve = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr: SocketAddr = reserve.local_addr().unwrap();
        drop(reserve);
        let (started, listening) = channel();
        let expected = identity();
        let wanted = expected.clone();
        let host = thread::spawn(move || {
            TcpLink::host_lab_greeted_until(
                "127.0.0.1", addr.port(), Duration::from_secs(8),
                &Cancel::new(), &wanted,
                || { let _ = started.send(()); },
            ).expect("a compatible peer must still be accepted")
        });
        listening.recv_timeout(Duration::from_secs(3)).unwrap();

        // Every mismatch MUST be rejected without consuming the one live
        // Host session or implicitly falling back to legacy SLOT/1.
        let mut variants = Vec::new();
        let mut wrong = expected.clone();
        wrong.game = *b"AMAE";
        variants.push(wrong);
        let mut wrong = expected.clone();
        wrong.rom_sha256[0] ^= 1;
        variants.push(wrong);
        let mut wrong = expected.clone();
        wrong.core_sha256[0] ^= 1;
        variants.push(wrong);
        let mut wrong = expected.clone();
        wrong.bios_sha256[0] ^= 1;
        variants.push(wrong);
        let mut wrong = expected.clone();
        wrong.backend ^= 1;
        variants.push(wrong);
        let mut wrong = expected.clone();
        wrong.mode ^= 1;
        variants.push(wrong);
        for wrong in variants {
            assert!(TcpLink::join_lab_greeted(addr, &wrong).is_err(),
                "KMLB/2 must refuse a mismatched peer");
        }

        let mut guest = TcpLink::join_lab_greeted(addr, &expected).unwrap();
        let mut server = host.join().unwrap();
        assert!(guest.nodelay().unwrap());
        assert!(server.nodelay().unwrap());

        <TcpLink as LinkChannel>::send(&mut guest, NETPACKET_RELIABLE, b"guest-to-host");
        <TcpLink as LinkChannel>::send(&mut server, NETPACKET_RELIABLE, b"host-to-guest");
        let incoming = |side: &mut TcpLink, expected: &[u8]| {
            let deadline = Instant::now() + Duration::from_secs(3);
            while Instant::now() < deadline {
                if let Some(packet) = <TcpLink as LinkChannel>::try_recv(side) {
                    assert_eq!(packet, expected);
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            panic!("linked peer packet not delivered");
        };
        incoming(&mut server, b"guest-to-host");
        incoming(&mut guest, b"host-to-guest");

        guest.send_end();
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline && !server.peer_ended() {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(server.peer_ended(),
            "the original five-byte END control must reach the Host");
    }

    #[test]
    fn cancelled_listener_does_not_mark_a_peer_ready() {
        use std::net::TcpListener;
        let reserve = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = reserve.local_addr().unwrap().port();
        drop(reserve);
        let cancel = Cancel::new();
        let stop = cancel.clone();
        let (started, listening) = channel();
        let host = std::thread::spawn(move || TcpLink::host_lab_greeted_until(
            "127.0.0.1", port, Duration::from_secs(5), &stop,
            &identity(), || { let _ = started.send(()); },
        ));
        listening.recv_timeout(Duration::from_secs(3)).unwrap();
        cancel.cancel();
        let error = match host.join().unwrap() {
            Ok(_) => panic!("cancelled Host must not become Ready"),
            Err(e) => e,
        };
        assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
    }

    #[test]
    fn all_critical_parameters_are_in_negotiation() {
        let base = identity().bytes();
        let mut another = identity(); another.backend = 2;
        assert_ne!(base, another.bytes());
        let mut another = identity(); another.mode = 2;
        assert_ne!(base, another.bytes());
        let mut another = identity(); another.rom_sha256[2] ^= 1;
        assert_ne!(base, another.bytes());
        let mut another = identity(); another.core_sha256[2] ^= 1;
        assert_ne!(base, another.bytes());
        let mut another = identity(); another.bios_sha256[2] ^= 1;
        assert_ne!(base, another.bytes());
    }
}
