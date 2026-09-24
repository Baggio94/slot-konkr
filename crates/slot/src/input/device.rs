use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use slot_input::{Btn, InputSource, Millis, RawEvent};

use super::evdev::{
    decode, device_name, pick_devices, to_raw, Ev, Hat, EVENT_BYTES, EV_ABS, EV_SYN,
};
use super::trace;

const DEV: &str = "/dev/input";
const SYS: &str = "/sys/class/input";
/// Where this board keeps its lid sensor.
const PSY: &str = "/sys/class/power_supply";

/// The lid is not an input device here, so it is polled. Each read is an i2c transaction to the
/// PMIC, and 150 ms of hinge latency is imperceptible.
const LID_POLL_MS: Millis = 150;

/// Every node worth reading, each on a thread blocked in `read`, so there is no poll latency.
pub struct DeviceInput {
    pending: Arc<Mutex<Vec<RawEvent>>>,
    /// The PMIC hall sensor attribute. `None` means a board without a lid.
    hall: Option<PathBuf>,
    /// `None` until the first read, so a device that booted with the lid shut reports it.
    lid_shut: Option<bool>,
    next_lid_poll: Millis,
}

impl DeviceInput {
    /// `root` is the card, used only for the trace file.
    pub fn open(root: &Path) -> Self {
        DeviceInput::open_in(Path::new(DEV), Path::new(SYS), root, trace::enabled())
    }

    /// `trace` is a parameter so tests need not race on a shared environment variable.
    pub fn open_in(dev: &Path, sys: &Path, root: &Path, trace: bool) -> Self {
        let trace = trace.then(|| Trace::start(root, dev, sys)).flatten();
        DeviceInput::open_traced(dev, sys, trace)
    }

    fn open_traced(dev: &Path, sys: &Path, trace: Option<Arc<Trace>>) -> Self {
        let pending: Arc<Mutex<Vec<RawEvent>>> = Arc::new(Mutex::new(Vec::new()));
        for node in pick_devices(dev, sys) {
            eprintln!(
                "slot: input {} ({})",
                node.display(),
                device_name(sys, &node)
            );
            let queue = pending.clone();
            let name = node.display().to_string();
            let trace = trace.clone();
            let spawned = std::thread::Builder::new()
                .name(format!("slot-input-{name}"))
                .spawn(move || read_node(&node, &queue, trace.as_deref()));
            if let Err(e) = spawned {
                eprintln!("slot: input {name}: {e}");
            }
        }
        DeviceInput {
            pending,
            hall: find_hall(Path::new(PSY)),
            lid_shut: None,
            next_lid_poll: 0,
        }
    }
}

/// Runs until the node goes away. Never joined: the process ends by powering off.
fn read_node(node: &Path, queue: &Mutex<Vec<RawEvent>>, trace: Option<&Trace>) {
    let label = node
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| node.display().to_string());
    // One per node: the axes are that node's.
    let mut hat = Hat::default();
    let mut file = match File::open(node) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("slot: input {}: {e}", node.display());
            return;
        }
    };
    // Whole events only: the driver never splits them.
    let mut buf = [0u8; EVENT_BYTES * 16];
    loop {
        let read = match file.read(&mut buf) {
            Ok(0) => return,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                eprintln!("slot: input {}: {e}", node.display());
                return;
            }
        };
        // Traced before mapping: the interesting codes are the ones `to_raw` drops.
        let mut events = Vec::new();
        for ev in buf[..read].chunks_exact(EVENT_BYTES).filter_map(decode) {
            if let Some(trace) = trace {
                trace.event(&label, ev);
            }
            match ev.kind {
                // The d-pad needs its axis state to know what it released.
                EV_ABS => events.extend(hat.feed(ev)),
                _ => events.extend(to_raw(ev)),
            }
        }
        if events.is_empty() {
            continue;
        }
        queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .extend(events);
    }
}

/// Shared by every reader thread so lines interleave in kernel order.
struct Trace {
    out: Mutex<File>,
    began: Instant,
}

impl Trace {
    /// `None` unless asked for or if the card refuses the file; not a boot failure. The node
    /// survey goes first, since a skipped node explains a missing button.
    fn start(root: &Path, dev: &Path, sys: &Path) -> Option<Arc<Trace>> {
        let path = root.join(trace::TRACE_FILE);
        let mut file = match File::create(&path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("slot: {}: {e}", path.display());
                return None;
            }
        };
        for line in trace::survey(dev, sys) {
            let _ = writeln!(file, "{line}");
        }
        let _ = writeln!(file, "--");
        let _ = file.flush();
        Some(Arc::new(Trace {
            out: Mutex::new(file),
            began: Instant::now(),
        }))
    }

    /// Flushed per line: power-off does not unwind, so a buffered edge would be lost.
    fn event(&self, node: &str, ev: Ev) {
        // SYN follows every edge and would be most of the file.
        if ev.kind == EV_SYN {
            return;
        }
        let line = trace::event_line(node, self.began.elapsed().as_millis(), ev);
        let mut out = self.out.lock().unwrap_or_else(|e| e.into_inner());
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
    }
}

impl DeviceInput {
    /// `1` open, `0` shut, measured on an RG SP. `None` on a missing or unparseable attribute,
    /// which keeps the last lid state.
    fn read_lid(&self) -> Option<bool> {
        let raw = std::fs::read_to_string(self.hall.as_ref()?).ok()?;
        match raw.trim() {
            "0" => Some(true),
            "1" => Some(false),
            _ => None,
        }
    }
}

impl InputSource for DeviceInput {
    fn poll(&mut self, now: Millis) -> Vec<RawEvent> {
        let mut out = std::mem::take(&mut *self.pending.lock().unwrap_or_else(|e| e.into_inner()));
        if self.hall.is_some() && now >= self.next_lid_poll {
            self.next_lid_poll = now + LID_POLL_MS;
            if let Some(shut) = self.read_lid() {
                if self.lid_shut != Some(shut) {
                    self.lid_shut = Some(shut);
                    // The same events an `SW_LID` node would produce.
                    out.push(if shut {
                        RawEvent::Down(Btn::Lid)
                    } else {
                        RawEvent::Up(Btn::Lid)
                    });
                }
            }
        }
        out
    }
}

/// The hinge is reported as `hallkey` on the PMIC's battery node. Searched for rather than
/// hardcoded, so a board naming it differently reads as having no lid.
fn find_hall(psy: &Path) -> Option<PathBuf> {
    let mut supplies: Vec<PathBuf> = std::fs::read_dir(psy)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    supplies.sort();
    supplies
        .into_iter()
        .map(|d| d.join("hallkey"))
        .find(|p| p.is_file())
}
