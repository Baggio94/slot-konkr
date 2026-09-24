//! Bringing the private link network up and down, by running the card's `System/slotlink.sh`.
//!
//! Not on `Platform`: `App` owns its `Power` outright and a link session needs this from a
//! worker thread. `warm` readies the driver while the player is still choosing.

#[cfg(feature = "device")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "device")]
use std::sync::mpsc::{channel, Sender};
#[cfg(feature = "device")]
use std::sync::OnceLock;
#[cfg(any(feature = "device", test))]
use std::{path::Path, process::Command};

use crate::link_net::Cancel;

/// The host brings the access point up and waits; the joiner associates and connects out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkRole {
    Host,
    Join,
}

impl LinkRole {
    /// The `link` verb for this role.
    pub fn arg(self) -> &'static str {
        match self {
            LinkRole::Host => "host",
            LinkRole::Join => "join",
        }
    }
}

/// Why the network did not come up. Each variant gets its own sentence on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RadioFail {
    /// `link join` exited 3: no host answered during its search.
    NoHost,
    /// The player backed out and the child was killed.
    Cancelled,
    /// Anything else, including no link script at all.
    Radio(String),
}

/// Work for the radio that nothing waits on, run one at a time and in the order asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadioJob {
    /// Load the driver and wait for its interfaces, without associating or hosting.
    Warm,
    /// Let it go. BaseOS keeps the driver loaded, so the card script ignores this.
    Cool,
    /// End a session: drop the access point or the association, and cool on the way out.
    Down,
}

/// Where `App` sends that work. A trait so tests can watch the order of jobs.
pub trait RadioJobs: Send {
    fn ask(&mut self, job: RadioJob);

    /// Whether the driver is loaded, so the link screen does not caption a wait already paid.
    /// Set when a `Warm` finishes, not when asked: the two differ by ~1.1 s.
    fn warmed(&self) -> bool;
}

/// One queue on one thread, so a `Cool` can never overtake a `Warm` and leave the radio loaded.
pub struct RadioQueue;

/// The queue's thread spawns on the first job, so a host build that never links starts none.
pub fn radio_jobs() -> Box<dyn RadioJobs> {
    Box::new(RadioQueue)
}

#[cfg(feature = "device")]
impl RadioJobs for RadioQueue {
    fn ask(&mut self, job: RadioJob) {
        // Fails only if the worker panicked; nothing useful to do from a frame loop.
        let _ = queue().send(job);
    }

    fn warmed(&self) -> bool {
        WARM.load(Ordering::SeqCst)
    }
}

/// Whether the driver is loaded, written only by the queue's worker. One per process, like the
/// hardware.
#[cfg(feature = "device")]
static WARM: AtomicBool = AtomicBool::new(false);

/// One worker for the process, spawned on the first job.
#[cfg(feature = "device")]
fn queue() -> &'static Sender<RadioJob> {
    static Q: OnceLock<Sender<RadioJob>> = OnceLock::new();
    Q.get_or_init(|| {
        let (tx, rx) = channel::<RadioJob>();
        std::thread::spawn(move || {
            for job in rx {
                match job {
                    // Set from the exit status after the work: an old BaseOS without `warm`
                    // exits 2 having loaded nothing.
                    RadioJob::Warm => WARM.store(run("warm"), Ordering::SeqCst),
                    // Cleared before the work, so nothing reads "up" during the unload.
                    RadioJob::Cool => {
                        WARM.store(false, Ordering::SeqCst);
                        run("cool");
                    }
                    RadioJob::Down => {
                        WARM.store(false, Ordering::SeqCst);
                        down();
                    }
                }
            }
        });
        tx
    })
}

/// `link <verb>` against the card's script, through `sh` because exFAT carries no exec bit. A card
/// without the script fails the link with sh's error rather than doing anything else.
#[cfg(any(feature = "device", test))]
fn link_command(root: &Path, verb: &str) -> Command {
    let mut cmd = Command::new("/bin/sh");
    cmd.arg(root.join("System/slotlink.sh"))
        .arg("link")
        .arg(verb);
    cmd
}

/// The card slot runs from, as `device_app` resolves it.
#[cfg(feature = "device")]
fn link(verb: &str) -> Command {
    let root = std::env::var_os("SLOT_ROOT").unwrap_or_else(|| "/mnt/sdcard".into());
    link_command(Path::new(&root), verb)
}

/// A verb nothing waits on, and whether it succeeded.
#[cfg(feature = "device")]
fn run(sub: &str) -> bool {
    link(sub).status().is_ok_and(|status| status.success())
}

/// Blocking: about 2 s to host, up to 30 s for a joiner's search. Run off the UI thread; a
/// cancel kills the child process.
#[cfg(feature = "device")]
pub fn up(role: LinkRole, cancel: &Cancel) -> Result<(), RadioFail> {
    let mut child = link(role.arg())
        .spawn()
        .map_err(|e| RadioFail::Radio(format!("link {} would not start: {e}", role.arg())))?;
    loop {
        if cancel.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(RadioFail::Cancelled);
        }
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            // 3: joiner found no host. Reported as such, not as a radio fault.
            Ok(Some(status)) if status.code() == Some(3) => return Err(RadioFail::NoHost),
            Ok(Some(status)) => {
                return Err(RadioFail::Radio(format!(
                    "link {} failed: {status}",
                    role.arg()
                )))
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
            Err(e) => return Err(RadioFail::Radio(format!("link {}: {e}", role.arg()))),
        }
    }
}

/// Infallible on purpose: it runs on every failure path, and a fallible teardown gets skipped.
#[cfg(feature = "device")]
pub fn down() {
    let _ = link("down").status();
}

/// No radio off device. Succeeds, since two copies of slot over loopback can drive the screen.
#[cfg(not(feature = "device"))]
pub fn up(_role: LinkRole, _cancel: &Cancel) -> Result<(), RadioFail> {
    Ok(())
}

#[cfg(not(feature = "device"))]
pub fn down() {}

/// Off device every job is a no-op, keeping one code path in `App`.
#[cfg(not(feature = "device"))]
impl RadioJobs for RadioQueue {
    fn ask(&mut self, _job: RadioJob) {}

    /// No driver to load off device, so always warm.
    fn warmed(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A swapped role would read as "nobody arrived".
    #[test]
    fn each_role_asks_for_its_own_subcommand() {
        assert_eq!(LinkRole::Host.arg(), "host");
        assert_eq!(LinkRole::Join.arg(), "join");
    }

    /// Asking for a job is never an error.
    #[test]
    fn asking_for_a_job_is_never_an_error() {
        let mut jobs = radio_jobs();
        jobs.ask(RadioJob::Warm);
        jobs.ask(RadioJob::Cool);
        jobs.ask(RadioJob::Down);
    }

    fn argv(cmd: &Command) -> Vec<String> {
        std::iter::once(cmd.get_program())
            .chain(cmd.get_args())
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    /// The card's script, through `sh`, from the card.
    #[test]
    fn a_link_runs_the_cards_script_through_sh() {
        let d = tempfile::tempdir().unwrap();
        let script = d.path().join("System/slotlink.sh");
        assert_eq!(
            argv(&link_command(d.path(), "host")),
            ["/bin/sh", script.to_str().unwrap(), "link", "host"]
        );
    }

    /// A host build reports the driver warm, so no wait is captioned.
    #[cfg(not(feature = "device"))]
    #[test]
    fn a_host_build_has_no_driver_left_to_load() {
        assert!(radio_jobs().warmed());
    }
}
