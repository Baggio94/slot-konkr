use gilrs::{EventType, Gilrs};
use slot_input::{Btn, InputSource, Millis, RawEvent};
use winit::event::WindowEvent;
use winit::keyboard::{KeyCode, PhysicalKey};

use super::keys::key_to_btn;
use super::pad::pad_to_btn;

pub struct HostInput {
    pending: Vec<RawEvent>,
    pad: Option<Gilrs>,
    lid_closed: bool,
    /// Buttons the keyboard holds, so they can be released on focus loss. The gamepad is not
    /// tracked: gilrs reads the device, so its releases arrive regardless of focus.
    held_keys: Vec<Btn>,
}

impl HostInput {
    pub fn new() -> Self {
        HostInput {
            pending: Vec::new(),
            pad: Gilrs::new().ok(),
            lid_closed: false,
            held_keys: Vec::new(),
        }
    }

    pub fn on_window_event(&mut self, event: &WindowEvent) {
        match event {
            // An unfocused window never gets key-ups for keys held in it, so the game would keep
            // holding them. Release them here instead.
            WindowEvent::Focused(false) => self.release_keys(),
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                self.key(code, event.state.is_pressed(), event.repeat);
            }
            _ => {}
        }
    }

    /// Every key still down, released once each.
    fn release_keys(&mut self) {
        for btn in std::mem::take(&mut self.held_keys) {
            self.pending.push(RawEvent::Up(btn));
        }
    }

    pub fn key(&mut self, code: KeyCode, pressed: bool, repeat: bool) {
        // Autorepeat is presses with no release, which would re-arm hold and double-tap windows.
        if repeat {
            return;
        }
        let Some(btn) = key_to_btn(code) else {
            return;
        };
        if btn == Btn::Lid {
            // The host has no hinge, so the lid key latches: press to close, again to open.
            if pressed {
                self.lid_closed = !self.lid_closed;
                self.pending.push(if self.lid_closed {
                    RawEvent::Down(Btn::Lid)
                } else {
                    RawEvent::Up(Btn::Lid)
                });
            }
            return;
        }
        // Recorded before sending, so a focus loss knows what is down. The latching lid is not.
        match pressed {
            true if !self.held_keys.contains(&btn) => self.held_keys.push(btn),
            true => {}
            false => self.held_keys.retain(|b| *b != btn),
        }
        self.push(btn, pressed);
    }

    fn push(&mut self, btn: Btn, pressed: bool) {
        if pressed {
            crate::latency::read();
        }
        self.pending.push(if pressed {
            RawEvent::Down(btn)
        } else {
            RawEvent::Up(btn)
        });
    }

    fn drain_pad(&mut self) {
        let Some(pad) = self.pad.as_mut() else {
            return;
        };
        let mut edges = Vec::new();
        while let Some(ev) = pad.next_event() {
            match ev.event {
                EventType::ButtonPressed(b, _) => edges.push((b, true)),
                EventType::ButtonReleased(b, _) => edges.push((b, false)),
                _ => {}
            }
        }
        for (b, pressed) in edges {
            if let Some(btn) = pad_to_btn(b) {
                self.push(btn, pressed);
            }
        }
    }
}

impl Default for HostInput {
    fn default() -> Self {
        HostInput::new()
    }
}

impl InputSource for HostInput {
    fn poll(&mut self, _now: Millis) -> Vec<RawEvent> {
        self.drain_pad();
        std::mem::take(&mut self.pending)
    }
}
