//! Starting a link session off the UI thread. `link_radio::up` blocks 1 to 5 s and
//! `TcpLink::host_until` up to 30 s, which on the frame loop would freeze even the cancel
//! button. The worker reports each step, since 30 silent seconds reads as a crash.

use std::sync::mpsc::{channel, Receiver, TryRecvError};

use crate::link_net::{Cancel, TcpLink, HOST_BOUND};
use crate::link_radio::{self, LinkRole, RadioFail};

/// Where the host lives on the private WiFi. `link_net` leaves this to its caller.
#[cfg(feature = "device")]
pub const HOST_ADDR: &str = "10.42.0.1";

/// Loopback off device, so two copies of slot on one machine can drive the link screen.
#[cfg(not(feature = "device"))]
pub const HOST_ADDR: &str = "127.0.0.1";

/// The port a link session meets on. There is no discovery, so both ends use this fixed value,
/// below the ephemeral range so no outgoing connection can hold it.
pub const DEFAULT_LINK_PORT: u16 = 7211;

/// Overrides the port, so two copies of slot on one machine (loopback) can link.
const PORT_ENV: &str = "SLOT_LINK_PORT";

/// Both ends must agree, or the link fails as "nobody arrived", so an unusable value is
/// reported and the default used.
pub fn link_port() -> u16 {
    let Some(raw) = std::env::var_os(PORT_ENV) else {
        return DEFAULT_LINK_PORT;
    };
    // `export SLOT_LINK_PORT=` is how a shell clears one, so empty means unset.
    if raw.to_str().is_some_and(|s| s.trim().is_empty()) {
        return DEFAULT_LINK_PORT;
    }
    match raw.to_str().and_then(|s| s.trim().parse::<u16>().ok()) {
        // 0 means "any free port", which the joiner cannot learn.
        None | Some(0) => {
            eprintln!(
                "slot: {PORT_ENV}={:?} is not a port a link can meet on, using {DEFAULT_LINK_PORT}",
                raw.to_string_lossy()
            );
            DEFAULT_LINK_PORT
        }
        Some(port) => port,
    }
}

/// Which slow step the worker is on. The screen says a different sentence for each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkStep {
    /// Bringing the private network up. 1 to 5 s.
    Radio,
    /// The socket step: a host waiting for a friend, a joiner connecting out.
    Waiting,
}

/// Why a link did not start. Each has its own sentence, so the player knows whether to retry,
/// move closer or ask their friend to act.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkFail {
    /// The network never came up. Nothing to do with the other player.
    Radio,
    /// The host waited out its bound and nobody arrived.
    NobodyCame,
    /// A real fault on the wire, or a worker that died without reporting.
    PeerVanished,
    /// The player backed out. Not a failure, but it ends the same way.
    Cancelled,
}

impl LinkStep {
    /// Every step in report order, which is also face upload order.
    pub const ALL: [LinkStep; 2] = [LinkStep::Radio, LinkStep::Waiting];

    /// Position in `ALL`, so a step is a face without a lookup.
    pub fn index(self) -> usize {
        self as usize
    }

    /// What the screen says during this step. One sentence covers both host and joiner.
    pub fn line(self) -> &'static str {
        match self {
            LinkStep::Radio => "Bringing the radio up",
            LinkStep::Waiting => "Looking for the other player",
        }
    }

    /// Which step's sentence to show. When the radio is already warm (a finished `Warm`, see
    /// `RadioJobs::warmed`), `Radio` shows the `Waiting` sentence, since the ~1.1 s driver load
    /// is already paid. Mapped onto an existing step so `ALL` stays every face.
    pub fn shown(self, warm: bool) -> LinkStep {
        match self {
            LinkStep::Radio if warm => LinkStep::Waiting,
            step => step,
        }
    }
}

impl LinkFail {
    /// The failures that reach the screen, in face upload order. `Cancelled` returns straight
    /// to the game.
    pub const SHOWN: [LinkFail; 3] = [
        LinkFail::Radio,
        LinkFail::NobodyCame,
        LinkFail::PeerVanished,
    ];

    /// Index in `SHOWN`, or `None` for `Cancelled`.
    pub fn shown(self) -> Option<usize> {
        LinkFail::SHOWN.iter().position(|f| *f == self)
    }

    /// One sentence each. `Cancelled`'s is never drawn; it exists to keep this total.
    pub fn line(self) -> &'static str {
        match self {
            LinkFail::Radio => "The radio did not come up",
            LinkFail::NobodyCame => "Nobody arrived",
            LinkFail::PeerVanished => "The other player vanished",
            LinkFail::Cancelled => "Cancelled",
        }
    }
}

/// One message from the worker. `At` may repeat; exactly one `Ready` or `Failed` comes last.
pub enum LinkProgress {
    At(LinkStep),
    Ready(TcpLink),
    Failed(LinkFail),
}

/// Maps `host_until`'s errors: `Interrupted` is a cancel, `TimedOut` the deadline, anything
/// else a wire fault.
fn classify(e: &std::io::Error) -> LinkFail {
    match e.kind() {
        std::io::ErrorKind::Interrupted => LinkFail::Cancelled,
        std::io::ErrorKind::TimedOut => LinkFail::NobodyCame,
        _ => LinkFail::PeerVanished,
    }
}

/// Bring the private network up. Injectable so tests never shell out. Takes the cancel flag so
/// a joiner's 30 s search can be killed.
type RadioUp = Box<dyn FnMut(LinkRole, &Cancel) -> Result<(), RadioFail> + Send>;
/// Take it back down. Infallible, like the real one.
type RadioDown = Box<dyn FnMut() + Send>;
/// The socket step, given the port and the flag that ends it early.
type Socket = Box<dyn FnMut(u16, &Cancel) -> std::io::Result<TcpLink> + Send>;

/// A link session being started. Poll it once a frame; cancel it whenever.
pub struct LinkStarter {
    rx: Receiver<LinkProgress>,
    cancel: Cancel,
    /// Set once a terminal message is handed out, so the sender dropping afterwards is not
    /// reported as a second, contradictory outcome.
    done: bool,
}

impl LinkStarter {
    /// The real thing: `link_radio` for the network, `TcpLink` for the socket.
    pub fn spawn(role: LinkRole, port: u16) -> LinkStarter {
        LinkStarter::spawn_with(
            Box::new(link_radio::up),
            Box::new(link_radio::down),
            role,
            port,
            Box::new(move |port, cancel| match role {
                // Same bound both ways, so neither gives up while the other is still there.
                LinkRole::Host => TcpLink::host_until(HOST_ADDR, port, HOST_BOUND, cancel),
                LinkRole::Join => TcpLink::join_until(HOST_ADDR, port, HOST_BOUND, cancel),
            }),
        )
    }

    /// The same worker with injectable slow parts, for tests.
    pub fn spawn_with(
        mut radio_up: RadioUp,
        mut radio_down: RadioDown,
        role: LinkRole,
        port: u16,
        mut socket: Socket,
    ) -> LinkStarter {
        let (tx, rx) = channel();
        let cancel = Cancel::new();
        let flag = cancel.clone();
        std::thread::spawn(move || {
            // Sends ignore a dropped receiver (the screen left), but the teardown still runs.
            let _ = tx.send(LinkProgress::At(LinkStep::Radio));
            if let Err(e) = radio_up(role, &flag) {
                // A joiner that found no host reports "nobody came"; a kill is the player's
                // own cancel.
                let fail = match &e {
                    RadioFail::NoHost => LinkFail::NobodyCame,
                    RadioFail::Cancelled => LinkFail::Cancelled,
                    RadioFail::Radio(why) => {
                        eprintln!("slot: link: {role:?} could not bring the radio up: {why}");
                        LinkFail::Radio
                    }
                };
                // `slotlink.sh link` can configure an interface and still exit non-zero.
                radio_down();
                let _ = tx.send(LinkProgress::Failed(fail));
                return;
            }
            let _ = tx.send(LinkProgress::At(LinkStep::Waiting));
            // Logged before the attempt, so a hang shows its address. Nothing reconciles
            // mismatched ports, so this is the first thing to compare across two logs.
            eprintln!("slot: link: {role:?} using {HOST_ADDR}:{port}");
            match socket(port, &flag) {
                // No teardown: the session being handed over runs on this network.
                Ok(link) => {
                    // A failed send means the player left, so no one owns the network and this
                    // thread must take it down. A joiner can get here after a cancel, because
                    // `TcpLink::join` never checks the flag.
                    eprintln!("slot: link: {role:?} connected on {HOST_ADDR}:{port}");
                    if tx.send(LinkProgress::Ready(link)).is_err() {
                        eprintln!("slot: link: nobody left to hand it to, radio back down");
                        radio_down();
                    }
                }
                Err(e) => {
                    // The screen shows every kind as PeerVanished; the log keeps the detail.
                    eprintln!(
                        "slot: link: {role:?} failed on {HOST_ADDR}:{port}: {e} (kind {:?})",
                        e.kind()
                    );
                    // Down before the message: the message unblocks the watcher, and a failed
                    // link must not leave the access point running.
                    radio_down();
                    let _ = tx.send(LinkProgress::Failed(classify(&e)));
                }
            }
        });
        LinkStarter {
            rx,
            cancel,
            done: false,
        }
    }

    /// `None` means still working. A queue poll, cheap enough for every frame.
    pub fn poll(&mut self) -> Option<LinkProgress> {
        if self.done {
            return None;
        }
        match self.rx.try_recv() {
            Ok(progress) => {
                self.done = matches!(progress, LinkProgress::Ready(_) | LinkProgress::Failed(_));
                Some(progress)
            }
            Err(TryRecvError::Empty) => None,
            // The worker ended without reporting, so it panicked. Report a fault rather than
            // waiting forever.
            Err(TryRecvError::Disconnected) => {
                self.done = true;
                Some(LinkProgress::Failed(LinkFail::PeerVanished))
            }
        }
    }

    /// Ask the worker to give up. A waiting host notices within 50 ms and reports `Cancelled`,
    /// not a timeout.
    pub fn cancel(&mut self) {
        self.cancel.cancel();
    }
}

impl Drop for LinkStarter {
    /// Dropping a starter cancels it. Otherwise the worker would hold the access point and port
    /// for its whole 30 s bound, then take the radio down under whatever session came next.
    /// Idempotent with `App`'s explicit cancels; does not wait for the detached worker.
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[cfg(test)]
mod port_tests {
    use super::*;

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn unset_means_the_number_both_devices_already_agree_on() {
        let _g = lock();
        std::env::remove_var(PORT_ENV);
        assert_eq!(link_port(), DEFAULT_LINK_PORT);
    }

    #[test]
    fn a_port_in_the_environment_wins() {
        let _g = lock();
        std::env::set_var(PORT_ENV, "7300");
        assert_eq!(link_port(), 7300);
        std::env::remove_var(PORT_ENV);
    }

    /// Whitespace a shell export picked up is not a reason to split the two ends.
    #[test]
    fn a_padded_port_is_still_a_port() {
        let _g = lock();
        std::env::set_var(PORT_ENV, "  7300 ");
        assert_eq!(link_port(), 7300);
        std::env::remove_var(PORT_ENV);
    }

    /// Falls back rather than failing, so the pair still meets.
    #[test]
    fn a_value_that_is_not_a_port_falls_back() {
        let _g = lock();
        for bad in ["banana", "-1", "70000", "7300x"] {
            std::env::set_var(PORT_ENV, bad);
            assert_eq!(
                link_port(),
                DEFAULT_LINK_PORT,
                "{bad:?} should not be taken"
            );
        }
        std::env::remove_var(PORT_ENV);
    }

    #[test]
    fn an_empty_value_is_an_unset_value() {
        let _g = lock();
        std::env::set_var(PORT_ENV, "   ");
        assert_eq!(link_port(), DEFAULT_LINK_PORT);
        std::env::remove_var(PORT_ENV);
    }

    #[test]
    fn zero_is_refused_even_though_it_parses() {
        let _g = lock();
        std::env::set_var(PORT_ENV, "0");
        assert_eq!(link_port(), DEFAULT_LINK_PORT);
        std::env::remove_var(PORT_ENV);
    }
}

#[cfg(test)]
mod step_tests {
    use super::*;

    /// Warm: the step already shows it is looking for the other player.
    #[test]
    fn a_warm_radio_captions_the_first_step_as_the_search() {
        assert_eq!(LinkStep::Radio.shown(true), LinkStep::Waiting);
        assert_eq!(
            LinkStep::Radio.shown(true).line(),
            "Looking for the other player"
        );
    }

    /// Cold: the driver load is still ahead, so the screen still says so.
    #[test]
    fn a_cold_radio_still_says_it_is_bringing_the_radio_up() {
        assert_eq!(LinkStep::Radio.shown(false), LinkStep::Radio);
        assert_eq!(LinkStep::Radio.shown(false).line(), "Bringing the radio up");
    }

    /// The socket step never waited on the driver.
    #[test]
    fn the_socket_step_says_the_same_thing_in_both_states() {
        assert_eq!(LinkStep::Waiting.shown(true), LinkStep::Waiting);
        assert_eq!(LinkStep::Waiting.shown(false), LinkStep::Waiting);
    }

    /// Both sentences stay in `ALL`, which the faces are built from.
    #[test]
    fn every_sentence_a_step_can_show_is_still_one_of_the_faces() {
        for warm in [true, false] {
            for step in LinkStep::ALL {
                assert!(
                    LinkStep::ALL.contains(&step.shown(warm)),
                    "{step:?} at warm={warm} shows a sentence with no face uploaded for it"
                );
            }
        }
    }
}
