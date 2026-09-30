use std::time::{Duration, Instant};

use slot::frontend::Frontend;
use slot::input::HostInput;
use slot_gfx::{Compositor, HostSurface, Surface};
use slot_power::SimPlatform;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

/// Frame pacing, as in `device_app`. Vsync alone is not enough: macOS does not present an
/// occluded window, so the swap returns at once (0.22 ms vs 7.9 ms visible) and the loop would
/// free-run at ~1900 fps. Also caps faster panels at the device's 60 Hz.
const FRAME: Duration = Duration::from_micros(16_667);

struct Slot {
    gfx: Option<(HostSurface, Compositor)>,
    frontend: Frontend,
    input: HostInput,
}

impl Slot {
    fn new() -> Self {
        Slot {
            gfx: None,
            frontend: Frontend::boot(Box::new(SimPlatform::new())),
            input: HostInput::new(),
        }
    }
}

impl ApplicationHandler for Slot {
    fn resumed(&mut self, events: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let built = HostSurface::new(events).and_then(|s| Compositor::new(&s).map(|c| (s, c)));
        let (surface, mut compositor) = match built {
            Ok(gfx) => gfx,
            Err(e) => {
                eprintln!("slot: {e}");
                events.exit();
                return;
            }
        };
        self.frontend.upload_faces(&mut compositor);
        self.gfx = Some((surface, compositor));
    }

    fn window_event(&mut self, events: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        self.input.on_window_event(&event);
        let Some((surface, compositor)) = self.gfx.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => events.exit(),
            WindowEvent::Resized(size) => surface.resize(size),
            WindowEvent::RedrawRequested => {
                // Times the whole frame, not just the present, as `device_app` does.
                let began = Instant::now();
                self.frontend.render(compositor, surface.window_size());
                let swap = std::time::Instant::now();
                let swapped = surface.swap();
                slot::latency::swapped(swap.elapsed().as_secs_f64() * 1000.0);
                if let Err(e) = swapped {
                    eprintln!("slot: {e}");
                    events.exit();
                    return;
                }
                surface.request_redraw();
                self.frontend.advance(&mut self.input);
                // The simulated platform ends the process outright.
                if self.frontend.restarting() {
                    self.frontend.restart();
                }
                if self.frontend.powering_off() {
                    self.frontend.poweroff();
                    events.exit();
                    // Before the wait: a stopping machine owes the panel nothing.
                    return;
                }
                if let Some(left) = FRAME.checked_sub(began.elapsed()) {
                    std::thread::sleep(left);
                }
            }
            _ => {}
        }
    }
}

pub fn run() {
    let events = match EventLoop::new() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("slot: {e}");
            return;
        }
    };
    events.set_control_flow(ControlFlow::Poll);
    if let Err(e) = events.run_app(&mut Slot::new()) {
        eprintln!("slot: {e}");
    }
}
