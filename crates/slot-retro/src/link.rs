use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// Transport for a core's netpacket traffic (link cable and wireless adapter packets).
/// `Send` because `send` and `try_recv` are called from the emulator thread.
pub trait LinkChannel: Send {
    /// `flags` are libretro's `NETPACKET_*` flags. A transport that cannot honour one should
    /// fall back to reliable delivery rather than drop silently.
    fn send(&mut self, flags: i32, buf: &[u8]);

    /// Must never block: it is called once a frame. `None` means nothing has arrived yet,
    /// never an error.
    fn try_recv(&mut self) -> Option<Vec<u8>>;

    /// Whether the other end is known to have gone, as opposed to merely quiet.
    fn is_closed(&self) -> bool {
        false
    }

    /// Tell the far end this session is over, before the wire goes. Runs on the emulator
    /// thread, so it must be bounded: never wait for the peer to answer.
    fn send_end(&mut self) {}

    /// Whether the far end said it was ending the session, as opposed to vanishing. The quiet
    /// timeout still applies underneath, for transports with no control channel.
    fn peer_ended(&self) -> bool {
        false
    }
}

/// In-memory `LinkChannel` that hands back whatever was sent, in order.
#[derive(Default)]
pub struct LoopbackLink {
    queue: VecDeque<Vec<u8>>,
}

impl LinkChannel for LoopbackLink {
    fn send(&mut self, _flags: i32, buf: &[u8]) {
        self.queue.push_back(buf.to_vec());
    }

    fn try_recv(&mut self) -> Option<Vec<u8>> {
        self.queue.pop_front()
    }
}

/// The core's end of a link session, shared with whoever drives the transport: a packet queue
/// each way plus a live flag. Mutexes are fine here, it is touched about once a frame.
#[derive(Clone, Default)]
pub struct Link(Arc<LinkState>);

#[derive(Default)]
struct LinkState {
    /// Packets that arrived from the peer, waiting to reach the core.
    inbound: Mutex<VecDeque<Vec<u8>>>,
    /// Packets the core produced, waiting to reach the peer.
    outbound: Mutex<VecDeque<Vec<u8>>>,
    /// Whether a session is live, as opposed to a core merely having registered netpacket.
    active: AtomicBool,
}

/// Locks a queue, recovering from poison. No guard leaves this module and every critical
/// section is a single deque op, so a poisoned queue is still valid, and a link fault must not
/// take the running game down.
fn lock(queue: &Mutex<VecDeque<Vec<u8>>>) -> MutexGuard<'_, VecDeque<Vec<u8>>> {
    queue.lock().unwrap_or_else(|e| e.into_inner())
}

impl Link {
    /// A packet that arrived from the peer, queued for the core.
    pub fn push_inbound(&self, packet: Vec<u8>) {
        lock(&self.0.inbound).push_back(packet);
    }

    pub fn take_inbound(&self) -> Option<Vec<u8>> {
        lock(&self.0.inbound).pop_front()
    }

    /// A packet the core sent, queued for the transport.
    pub fn push_outbound(&self, packet: Vec<u8>) {
        lock(&self.0.outbound).push_back(packet);
    }

    pub fn take_outbound(&self) -> Option<Vec<u8>> {
        lock(&self.0.outbound).pop_front()
    }

    /// Whether a session is live. `Acquire` pairs with `set_active`'s `Release`, so a reader
    /// that sees `false` also sees the `clear` that `Cmd::EndLink` does before it.
    pub fn is_active(&self) -> bool {
        self.0.active.load(Ordering::Acquire)
    }

    /// Mark the session live or ended. Does not clear the queues; see `clear`.
    pub fn set_active(&self, active: bool) {
        self.0.active.store(active, Ordering::Release);
    }

    /// Empties both queues so a stale packet never reaches the next session. Must work through
    /// a poisoned lock for the same reason.
    pub fn clear(&self) {
        lock(&self.0.inbound).clear();
        lock(&self.0.outbound).clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Poison tests live here because only this module can reach the `Mutex` itself.

    /// Poisons one queue by panicking on another thread while holding it, and checks it took.
    fn poison(link: &Link, queue: fn(&LinkState) -> &Mutex<VecDeque<Vec<u8>>>) {
        let state = link.0.clone();
        std::thread::spawn(move || {
            let _held = queue(&state)
                .lock()
                .expect("the queue was poisoned already");
            panic!("a thread driving the transport died mid packet");
        })
        .join()
        .expect_err("the poisoning thread was supposed to panic");
        assert!(
            queue(&link.0).lock().is_err(),
            "the queue was not left poisoned, so this proves nothing"
        );
    }

    /// A poisoned queue must not kill the emulator thread that pumps it.
    #[test]
    fn a_dead_transport_does_not_take_the_emulator_thread_with_it() {
        let link = Link::default();
        link.set_active(true);
        link.push_inbound(b"arrived before the fault".to_vec());
        poison(&link, |s| &s.inbound);
        poison(&link, |s| &s.outbound);

        // One frame of `pump_link`/`flush_outbound`, on its own thread to see if it survives.
        let worker = link.clone();
        let frame = std::thread::spawn(move || {
            let got = worker.take_inbound();
            worker.push_outbound(b"this frame's traffic".to_vec());
            (got, worker.take_outbound())
        })
        .join();

        let (got, sent) = frame.expect("the emulator thread died with the transport");
        assert_eq!(
            got.as_deref(),
            Some(&b"arrived before the fault"[..]),
            "a packet queued before the fault is still the core's to read"
        );
        assert_eq!(
            sent.as_deref(),
            Some(&b"this frame's traffic"[..]),
            "the core must still be able to send after the fault"
        );
    }

    #[test]
    fn a_poisoned_inbound_queue_keeps_carrying_packets_to_the_core() {
        let link = Link::default();
        poison(&link, |s| &s.inbound);

        link.push_inbound(b"first".to_vec());
        link.push_inbound(b"second".to_vec());

        assert_eq!(link.take_inbound().as_deref(), Some(&b"first"[..]));
        assert_eq!(
            link.take_inbound().as_deref(),
            Some(&b"second"[..]),
            "order is not something a poisoned lock can disturb"
        );
        assert_eq!(link.take_inbound(), None);
    }

    #[test]
    fn a_poisoned_outbound_queue_keeps_carrying_packets_to_the_wire() {
        let link = Link::default();
        poison(&link, |s| &s.outbound);

        link.push_outbound(b"first".to_vec());
        link.push_outbound(b"second".to_vec());

        assert_eq!(link.take_outbound().as_deref(), Some(&b"first"[..]));
        assert_eq!(link.take_outbound().as_deref(), Some(&b"second"[..]));
        assert_eq!(link.take_outbound(), None);
    }

    /// `clear` empties both queues even when poisoned.
    #[test]
    fn ending_a_session_still_empties_both_poisoned_queues() {
        let link = Link::default();
        link.push_inbound(b"stale".to_vec());
        link.push_outbound(b"stale".to_vec());
        poison(&link, |s| &s.inbound);
        poison(&link, |s| &s.outbound);

        link.clear();

        assert_eq!(link.take_inbound(), None, "session one's inbound survived");
        assert_eq!(
            link.take_outbound(),
            None,
            "session one's outbound survived"
        );
    }
}
