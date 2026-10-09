use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::{c_char, c_void, CString};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use slot_gfx::{Compositor, GfxError, Surface, OUT_H, OUT_W};
use slot_store::{Cart, Platform};
use slot_ui::{cart_face_with, cart_shadow, gb_cart_shadow, Draw, GbShell, Shelf, SlotChrome};

// Android GLSurfaceView owns EGL context creation, current context and buffer swaps.
#[link(name = "EGL")]
unsafe extern "C" {
    fn eglGetProcAddress(name: *const c_char) -> *const c_void;
}

struct AndroidSurface {
    size: (u32, u32),
}

impl Surface for AndroidSurface {
    fn make_current(&mut self) -> Result<(), GfxError> {
        Ok(())
    }

    fn window_size(&self) -> (u32, u32) {
        self.size
    }

    fn swap(&mut self) -> Result<(), GfxError> {
        Ok(())
    }

    fn proc_address(&self, name: &str) -> *const c_void {
        CString::new(name)
            .map(|c| unsafe { eglGetProcAddress(c.as_ptr()) })
            .unwrap_or(std::ptr::null())
    }
}

#[derive(Clone, Copy)]
enum Input {
    Key(i32, bool),
    Reset,
}

static INPUT: Mutex<VecDeque<Input>> = Mutex::new(VecDeque::new());

// All engine / OpenGL work occurs on GLSurfaceView's dedicated GL thread.
thread_local! {
    static ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) };
}

struct Engine {
    gpu: Compositor,
    size: (u32, u32),
    shelves: Vec<Shelf>,
    active: usize,
    inserted: bool,
    progress: f32,
    born: Instant,
    last: Instant,
}

impl Engine {
    fn new() -> Result<Self, String> {
        let size = (960, 640);
        let mut gpu = Compositor::new(&AndroidSurface { size }).map_err(|e| e.to_string())?;
        let mut shelves = Vec::new();

        // Synthetic labels are deliberate: no commercial ROM/BIOS is bundled.
        for (platform, titles) in [
            (Platform::Gba, ["ADVANCE ONE", "ADVANCE TWO", "ADVANCE THREE"]),
            (Platform::Gb, ["CLASSIC ONE", "CLASSIC TWO", "CLASSIC THREE"]),
            (Platform::Gbc, ["COLOR ONE", "COLOR TWO", "COLOR THREE"]),
        ] {
            let carts: Vec<Cart> = titles
                .iter()
                .map(|title| Cart {
                    platform,
                    stem: (*title).to_owned(),
                    rom: PathBuf::new(),
                    label: None,
                    title: (*title).to_owned(),
                    code: String::new(),
                    shell: None,
                })
                .collect();
            let mut shelf = Shelf::new(carts);
            let faces: Vec<_> = shelf
                .carts
                .iter()
                .map(|cart| {
                    let face = cart_face_with(cart, None);
                    gpu.create_texture(face.w, face.h, &face.rgba)
                })
                .collect();
            shelf.set_faces(faces);
            shelves.push(shelf);
        }

        let shadow = cart_shadow();
        let gba = gpu.create_texture(shadow.w, shadow.h, &shadow.rgba);
        let shadow = gb_cart_shadow(GbShell::Notched);
        let gb = gpu.create_texture(shadow.w, shadow.h, &shadow.rgba);
        let shadow = gb_cart_shadow(GbShell::Rounded);
        let gbc = gpu.create_texture(shadow.w, shadow.h, &shadow.rgba);
        for shelf in &mut shelves {
            shelf.set_shadow(gba);
            shelf.set_gb_shadow(GbShell::Notched, gb);
            shelf.set_gb_shadow(GbShell::Rounded, gbc);
        }

        let now = Instant::now();
        Ok(Self {
            gpu,
            size,
            shelves,
            active: 0,
            inserted: false,
            progress: 0.0,
            born: now,
            last: now,
        })
    }

    fn handle(&mut self, input: Input) {
        match input {
            Input::Reset => {
                for shelf in &mut self.shelves {
                    shelf.release_hold();
                }
            }
            Input::Key(code, pressed) => {
                if pressed {
                    match code {
                        102 | 103 => {
                            self.shelves[self.active].release_hold();
                            self.active = if code == 102 {
                                (self.active + self.shelves.len() - 1) % self.shelves.len()
                            } else {
                                (self.active + 1) % self.shelves.len()
                            };
                            self.inserted = false;
                            self.progress = 0.0;
                            return;
                        }
                        96 => {
                            self.inserted = true;
                            return;
                        }
                        97 => {
                            self.inserted = false;
                            return;
                        }
                        _ => {}
                    }
                }
                let ms = self.born.elapsed().as_millis() as u64;
                let shelf = &mut self.shelves[self.active];
                match (code, pressed) {
                    (21, true) if !self.inserted => shelf.hold_left(ms),
                    (22, true) if !self.inserted => shelf.hold_right(ms),
                    (21, false) => shelf.release_left(),
                    (22, false) => shelf.release_right(),
                    (19, true) if !self.inserted => shelf.jump_prev_letter(),
                    (20, true) if !self.inserted => shelf.jump_next_letter(),
                    _ => {}
                }
            }
        }
    }

    fn draw(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32().min(0.05);
        self.last = now;

        let events: Vec<Input> = INPUT
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .drain(..)
            .collect();
        for event in events {
            self.handle(event);
        }

        let ms = self.born.elapsed().as_millis() as u64;
        let shelf = &mut self.shelves[self.active];
        shelf.tick(ms);
        shelf.update(dt);

        // Match the upstream ~730 ms cartridge insertion timing.
        let change = dt / 0.73;
        self.progress = if self.inserted {
            (self.progress + change).min(1.0)
        } else {
            (self.progress - change).max(0.0)
        };

        let mut commands = vec![Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: OUT_W as f32,
            h: OUT_H as f32,
            colour: [0.085, 0.085, 0.093, 1.0],
        }];
        if self.progress == 0.0 {
            shelf.draw(0.0, &mut commands);
        } else {
            let cart = &shelf.carts[shelf.index];
            let (rest, scale) = shelf.selected_at();
            shelf.draw_row(Some(cart.stem.as_str()), 0.0, 0.0, 1.0, &mut commands);
            SlotChrome {
                cart,
                face: shelf.face(shelf.index),
                rest,
                scale,
                seat: self.progress,
                alert: None,
                dim: 0.0,
                screen: 0.0,
                game: false,
            }
            .draw(&mut commands);
        }
        self.gpu.fit(self.size);
        self.gpu.begin_frame();
        self.gpu.draw_list(&commands);
        self.gpu.end_frame(self.size);
    }
}

// The primitive JNI ABI is sufficient for this stage; no Java objects are touched.
#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativeSurfaceCreated(
    _env: *mut c_void,
    _activity: *mut c_void,
) -> u8 {
    ENGINE.with(|holder| {
        *holder.borrow_mut() = None;
        match Engine::new() {
            Ok(engine) => {
                *holder.borrow_mut() = Some(engine);
                1
            }
            Err(error) => {
                eprintln!("slot-konkr: OpenGL renderer initialization failed: {error}");
                0
            }
        }
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativeSurfaceChanged(
    _env: *mut c_void,
    _activity: *mut c_void,
    width: i32,
    height: i32,
) {
    ENGINE.with(|holder| {
        if let Some(engine) = holder.borrow_mut().as_mut() {
            engine.size = (width.max(1) as u32, height.max(1) as u32);
        }
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativeDrawFrame(
    _env: *mut c_void,
    _activity: *mut c_void,
) {
    ENGINE.with(|holder| {
        if let Some(engine) = holder.borrow_mut().as_mut() {
            engine.draw();
        }
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativeKey(
    _env: *mut c_void,
    _activity: *mut c_void,
    key: i32,
    pressed: u8,
) {
    INPUT
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .push_back(Input::Key(key, pressed != 0));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativeResetInput(
    _env: *mut c_void,
    _activity: *mut c_void,
) {
    let mut input = INPUT.lock().unwrap_or_else(|poison| poison.into_inner());
    input.clear();
    input.push_back(Input::Reset);
}
