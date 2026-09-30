//! Records a site clip: slot on a card root, driven by a script of presses at exactly 60 fps,
//! the emulator stepped once per frame, frames piped to ffmpeg at 720x480.
//!
//! `SLOT_SILENT=1 cargo run --release --example record -- ROOT SCRIPT OUT`
//!
//! A script is one command per line, `#` for comments. Frames count from the last command:
//!   wait N         run N frames
//!   tap BTN [N]    press for N frames (6), then release
//!   hold BTN N     press for N frames, then release
//!   down BTN / up BTN
//!   rec [NAME]     start a clip: OUT itself, or OUT/NAME.mp4 when named
//!   cut            end it; nothing outside a clip is kept, and one session can cut many
//! BTN is up, down, left, right, a, b, x, y, l1, r1, l2, r2, start, select or menu.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

use slot::frontend::Frontend;
use slot_gfx::{blue_light_gain, Compositor, HeadlessSurface, OUT_H, OUT_W};
use slot_input::{Btn, InputSource, Millis, RawEvent};
use slot_power::SimPlatform;

const FPS: f64 = 60.0;

enum Step {
    Wait(u32),
    Down(Btn),
    Up(Btn),
    Rec(Option<String>),
    Cut,
}

/// One clip's ffmpeg, fed from its own thread.
struct Clip {
    path: String,
    frames: std::sync::mpsc::Sender<Vec<u8>>,
    writer: std::thread::JoinHandle<()>,
    ffmpeg: std::process::Child,
    written: u32,
}

impl Clip {
    fn start(path: String) -> Clip {
        let mut ffmpeg = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgba"])
            .args(["-s", &format!("{OUT_W}x{OUT_H}"), "-r", "60", "-i", "-"])
            .args(["-c:v", "libx264", "-preset", "slow", "-crf", "20"])
            .args(["-pix_fmt", "yuv420p", "-movflags", "+faststart", &path])
            .stdin(Stdio::piped())
            .spawn()
            .expect("ffmpeg");
        let mut pipe = ffmpeg.stdin.take().unwrap();
        // Off the frame loop: a frame that stalls more than 25 ms lets the emulator run one on
        // its own clock.
        let (frames, queued) = std::sync::mpsc::channel::<Vec<u8>>();
        let writer = std::thread::spawn(move || {
            for pixels in queued {
                pipe.write_all(&pixels).expect("ffmpeg went away");
            }
        });
        Clip {
            path,
            frames,
            writer,
            ffmpeg,
            written: 0,
        }
    }

    fn finish(self) {
        drop(self.frames);
        self.writer.join().unwrap();
        let mut ffmpeg = self.ffmpeg;
        let status = ffmpeg.wait().expect("ffmpeg");
        eprintln!(
            "record: {} frames ({:.2} s) to {}, {status}",
            self.written,
            f64::from(self.written) / FPS,
            self.path
        );
    }
}

fn btn(name: &str) -> Btn {
    match name {
        "up" => Btn::Up,
        "down" => Btn::Down,
        "left" => Btn::Left,
        "right" => Btn::Right,
        "a" => Btn::A,
        "b" => Btn::B,
        "x" => Btn::X,
        "y" => Btn::Y,
        "l1" => Btn::L1,
        "r1" => Btn::R1,
        "l2" => Btn::L2,
        "r2" => Btn::R2,
        "start" => Btn::Start,
        "select" => Btn::Select,
        "menu" => Btn::Menu,
        other => panic!("unknown button {other}"),
    }
}

fn parse(script: &str) -> Vec<Step> {
    let mut steps = Vec::new();
    for line in script.lines() {
        let words: Vec<&str> = line.split('#').next().unwrap().split_whitespace().collect();
        let n = |i: usize, default: u32| words.get(i).map_or(default, |w| w.parse().unwrap());
        match words.as_slice() {
            [] => {}
            ["wait", ..] => steps.push(Step::Wait(n(1, 1))),
            ["tap", b, ..] | ["hold", b, ..] => {
                steps.push(Step::Down(btn(b)));
                steps.push(Step::Wait(n(2, 6)));
                steps.push(Step::Up(btn(b)));
            }
            ["down", b] => steps.push(Step::Down(btn(b))),
            ["up", b] => steps.push(Step::Up(btn(b))),
            ["rec"] => steps.push(Step::Rec(None)),
            ["rec", name] => steps.push(Step::Rec(Some(name.to_string()))),
            ["cut"] => steps.push(Step::Cut),
            _ => panic!("cannot read {line:?}"),
        }
    }
    steps
}

/// What the device does after the frame the recorder reads: the blit's blue light gain, and the
/// backlight, which is linear in its level and shown against the level the take started at.
fn panel(pixels: &mut [u8], app: &slot::app::App, lit: u8) {
    let light = f32::from(app.brightness()) / f32::from(lit);
    let gain = blue_light_gain(app.blue_light()).map(|g| g * light);
    if gain == [1.0; 3] {
        return;
    }
    for px in pixels.chunks_exact_mut(4) {
        for (c, g) in px.iter_mut().zip(gain) {
            *c = (f32::from(*c) * g).round().min(255.0) as u8;
        }
    }
}

/// Hands the frontend whatever the script queued since the last poll.
#[derive(Default)]
struct Scripted(Vec<RawEvent>);

impl InputSource for Scripted {
    fn poll(&mut self, _now: Millis) -> Vec<RawEvent> {
        std::mem::take(&mut self.0)
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [root, script, out] = args.as_slice() else {
        eprintln!("usage: record ROOT SCRIPT OUT");
        std::process::exit(2);
    };
    let steps = parse(&std::fs::read_to_string(script).expect("read the script"));

    let surface = HeadlessSurface::new().expect("headless GL");
    let mut compositor = Compositor::new(&surface).expect("compositor");
    let mut frontend = Frontend::boot(Box::new(SimPlatform::at(root.into())));
    frontend.upload_faces(&mut compositor);
    frontend.drive_emulator();

    let mut input = Scripted::default();
    // The level the card starts at shows as recorded; a change from it dims or brightens.
    let lit = frontend.app().brightness().max(1);
    let mut frame = 0u64;
    let mut clip: Option<Clip> = None;
    let present = Duration::from_secs_f64(1.0 / FPS);
    // Real time: fast forward runs the emulator on its own clock, so a recorder racing ahead of
    // it caught a fraction of the frames the device shows.
    let mut due = std::time::Instant::now();
    let mut run = |input: &mut Scripted, frame: &mut u64, clip: &mut Option<Clip>| {
        // A core loads on its own thread in real time. Waiting here keeps that out of the
        // video, so a take never depends on how fast this machine loads it.
        let loading = std::time::Instant::now();
        while frontend.core_settling() && loading.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(5));
        }
        due += present;
        match due.checked_duration_since(std::time::Instant::now()) {
            Some(wait) => std::thread::sleep(wait),
            None => due = std::time::Instant::now(),
        }
        *frame += 1;
        let now = (*frame as f64 * 1000.0 / FPS) as Millis;
        frontend.advance_at(input, now, (1.0 / FPS) as f32);
        // Long enough that every frame is the emulator's own, whatever the host's speed.
        frontend.step_emulator(present, Duration::from_secs(1));
        frontend.compose(&mut compositor);
        // Read every frame, kept or not: frames composed and never read back came out black
        // once reading began.
        let mut pixels = compositor.read_frame();
        panel(&mut pixels, frontend.app(), lit);
        if let Some(clip) = clip {
            clip.frames.send(pixels).unwrap();
            clip.written += 1;
        }
    };
    for step in steps {
        match step {
            Step::Wait(n) => {
                for _ in 0..n {
                    run(&mut input, &mut frame, &mut clip);
                }
            }
            Step::Down(b) => input.0.push(RawEvent::Down(b)),
            Step::Up(b) => input.0.push(RawEvent::Up(b)),
            Step::Rec(name) => {
                if let Some(done) = clip.take() {
                    done.finish();
                }
                let path = match name {
                    Some(name) => format!("{out}/{name}.mp4"),
                    None => out.clone(),
                };
                clip = Some(Clip::start(path));
            }
            Step::Cut => {
                if let Some(done) = clip.take() {
                    done.finish();
                }
            }
        }
    }
    if let Some(done) = clip.take() {
        done.finish();
    }
}
