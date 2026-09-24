//! The TCP transport for a link session: two handhelds on the OS's private WiFi, ~2 ms round
//! trip. It carries a core's serial packets intact and promptly, nothing else. The bind address
//! is a parameter: that the device is `10.42.0.1` is a product fact for the caller.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use slot_retro::LinkChannel;

/// A shared "stop waiting" flag. Separate from a deadline because the screen says different
/// things: a deadline means nobody arrived, a cancel means the player changed their mind.
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

/// How often a waiting host checks for a cancel or the deadline.
const POLL_MS: u64 = 50;

/// What a host waits for a friend before deciding nobody is coming.
pub const HOST_BOUND: Duration = Duration::from_secs(30);

/// Control opcode for "session ended". A control message is a zero-length marker frame, then
/// a frame whose first byte is the opcode. Cores cannot emit an empty packet (`netpacket_send`
/// drops them and `send` refuses them), so the marker is unambiguous.
///
/// Unknown opcodes are dropped, never passed to the core, so later builds can add messages.
const CONTROL_ENDED: u8 = 0x00;

/// How long `send_end` waits for the goodbye to be written. Only a peer that stopped reading
/// ever costs this, and it must not block the emulator thread indefinitely.
const BYE_MS: u64 = 100;

/// What the writer thread is asked to put on the wire.
enum Out {
    /// A core's packet, framed with its own length.
    Packet(Vec<u8>),
    /// A control message, and an ack sent once it is written.
    Control(u8, Sender<()>),
}

/// One TCP connection carrying a core's serial traffic.
///
/// Reads and writes each run on their own thread, so `try_recv` is a queue poll and `send` only
/// queues: a peer that stops reading blocks the writer thread, never the emulator's frame.
pub struct TcpLink {
    /// The writer thread's queue. `send` never touches the socket itself.
    outbox: Sender<Out>,
    inbox: Receiver<Vec<u8>>,
    /// Kept so `Drop` can shut the socket down (reaching the threads' `try_clone` dups too),
    /// and so tests can read `nodelay`.
    stream: TcpStream,
    /// Set by the reader when a read fails: the peer is gone, not quiet. `try_recv` cannot
    /// tell the two apart.
    closed: Arc<AtomicBool>,
    /// Set when the peer sends "ended". Separate from `closed` (which follows moments later)
    /// so the session can end at once and say who ended it.
    ended: Arc<AtomicBool>,
}

impl TcpLink {
    /// Wait for the other handheld, bounded and cancellable.
    ///
    /// `accept()` cannot be cancelled, so the listener is non-blocking and polled. Errors map
    /// to the screen: `Interrupted` is a cancel, `TimedOut` is nobody coming, anything else is
    /// a socket fault.
    ///
    /// Binds to `addr`, not `0.0.0.0`, so the unauthenticated listener is not reachable from
    /// the user's home network.
    pub fn host_until(
        addr: &str,
        port: u16,
        bound: Duration,
        cancel: &Cancel,
    ) -> std::io::Result<TcpLink> {
        let listener = TcpListener::bind((addr, port))?;
        listener.set_nonblocking(true)?;
        let deadline = Instant::now() + bound;
        loop {
            if cancel.is_cancelled() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "cancelled while waiting for a peer",
                ));
            }
            match listener.accept() {
                Ok((stream, _peer)) => {
                    // Back to blocking before `wrap`: the mode is shared by `try_clone` dups,
                    // and a WouldBlock from `read_exact` reads as "peer gone" and kills the
                    // reader. Held by `a_bounded_host_still_accepts_a_peer_that_does_arrive`.
                    stream.set_nonblocking(false)?;
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

    /// `host_until` with the default bound, for tests that spawn their own peer.
    pub fn host(addr: &str, port: u16) -> std::io::Result<TcpLink> {
        TcpLink::host_until(addr, port, HOST_BOUND, &Cancel::new())
    }

    /// Reach a host, retrying until it is there, the player gives up, or the bound passes.
    ///
    /// Retrying makes press order irrelevant: the host needs 1 to 5 s to bring its radio up
    /// and bind. Same `bound` as the host, so neither side gives up while the other waits.
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
            // Every error is retried: refused, unreachable or unassigned address all mean "not
            // yet" as often as "never". The deadline decides.
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

    /// Connect once to a host already waiting. For tests.
    pub fn join(addr: &str, port: u16) -> std::io::Result<TcpLink> {
        TcpLink::wrap(TcpStream::connect((addr, port))?)
    }

    fn wrap(stream: TcpStream) -> std::io::Result<TcpLink> {
        // Small, latency-sensitive packets: Nagle would only delay them.
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
            // Whether the last frame was the control marker. Marker and message are written
            // together, so one frame of memory suffices.
            let mut control = false;
            loop {
                if reader.read_exact(&mut header).is_err() {
                    // The peer is gone. Flagged so the session ends instead of starving.
                    reader_closed.store(true, Ordering::Release);
                    return;
                }
                let len = u16::from_be_bytes(header) as usize;
                // A marker where a message was expected resets rather than reading as an empty
                // message.
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
                    // Unknown opcodes are dropped: not acted on, not passed to the core.
                    if buf.first() == Some(&CONTROL_ENDED) {
                        reader_ended.store(true, Ordering::Release);
                    }
                    continue;
                }
                if rtx.send(buf).is_err() {
                    return; // our own end hung up
                }
            }
        });

        std::thread::spawn(move || {
            // Ends when the `TcpLink` (and so `outbox`) drops. A `write_all` blocked on a
            // stalled peer is freed by that drop's `shutdown(Both)`.
            for out in wrx.iter() {
                match out {
                    // Framed here. Longer than u16 cannot come from GBA serial hardware, so
                    // refuse rather than truncate.
                    Out::Packet(buf) => {
                        let Ok(len) = u16::try_from(buf.len()) else {
                            continue;
                        };
                        if writer.write_all(&len.to_be_bytes()).is_err() {
                            return;
                        }
                        if writer.write_all(&buf).is_err() {
                            return;
                        }
                    }
                    // Marker and message in one `write_all`, so no packet can land between them.
                    Out::Control(op, ack) => {
                        if writer.write_all(&[0, 0, 0, 1, op]).is_err() {
                            return;
                        }
                        let _ = ack.send(());
                    }
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

    /// Tell the peer the session is ending, waiting up to `BYE_MS` for it to be written.
    /// Without the wait, `Drop`'s shutdown could discard the message still in the queue.
    pub fn send_end(&mut self) {
        let (ack, wrote) = channel();
        if self.outbox.send(Out::Control(CONTROL_ENDED, ack)).is_err() {
            return;
        }
        let _ = wrote.recv_timeout(Duration::from_millis(BYE_MS));
    }

    /// Whether Nagle's algorithm is disabled on the wrapped socket.
    pub fn nodelay(&self) -> std::io::Result<bool> {
        self.stream.nodelay()
    }
}

impl Drop for TcpLink {
    /// Shutting down unblocks the reader and writer threads so they exit, and sends the peer a
    /// FIN so it sees a close rather than silence. An error usually means the peer closed
    /// first, so it is ignored.
    fn drop(&mut self) {
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}

impl LinkChannel for TcpLink {
    fn send(&mut self, _flags: i32, buf: &[u8]) {
        // Only queued: this runs inside the worker's frame and a stalled peer must not block
        // it (`send_never_blocks_on_a_peer_that_stopped_reading`).
        //
        // An empty payload would forge the control marker (see `CONTROL_ENDED`), so refuse it.
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
