use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::{c_char, c_void, CString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use crate::game::GameSession;
use crate::core_picker::{CorePicker, Press};
use slot_retro::ButtonMask;
use slot_gfx::ScreenEffect;
use jni::{JNIEnv, objects::{JObject, JString}, sys::{jint, jboolean, jshortArray, jstring}};
use std::sync::Mutex;
use std::time::Instant;

use slot_gfx::{Compositor, GfxError, Surface, OUT_H, OUT_W};
use slot_store::{Cart, Platform, Core};
use crate::library::{carts_by_platform, RomEntry, LIBRARY};
use slot_ui::{
    board_face, chip_face, chip_shadow_face, socket_face, quick_value_face, cart_face_with,
    cart_shadow, gb_cart_shadow, board_from, board_zoom, lift_of, shelf_cart_at,
    on_board, lid_from, grown, draw_empty_slot, Draw, GbShell, Shelf, SlotChrome,
    BOARD_X, BOARD_W, SOCKET_U, SOCKET_V, SOCKET_W, SOCKET_H,
    CHIP_U, CHIP_V, CHIP_W, CHIP_H, HOP_LIFT, TURN_PAD, SHADOW_W, SHADOW_H,
    CART_W,
};

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

#[derive(Clone)]
enum Input {
    Key(i32, bool),
    Reset,
    GameReady { uri: String, path: String },
    GameError { uri: String, message: String },
    Exit,
    Suspend,
}

static INPUT: Mutex<VecDeque<Input>> = Mutex::new(VecDeque::new());
static REQUEST: Mutex<Option<String>> = Mutex::new(None);
static UI_ACTION: Mutex<VecDeque<i32>> = Mutex::new(VecDeque::new());
static CART_SFX: Mutex<VecDeque<i32>> = Mutex::new(VecDeque::new());
const CART_INSERT_SFX: i32 = 1;
const CART_EJECT_SFX: i32 = 2;
const INSERT_SOUND_PROGRESS: f32 = 0.480 / 0.730;
const EJECT_SOUND_PROGRESS: f32 = 1.0 - (0.350 / 0.730);
static MESSAGE: Mutex<Option<String>> = Mutex::new(None);
static PATHS: Mutex<Option<(PathBuf, PathBuf)>> = Mutex::new(None);
static AUDIO: Mutex<VecDeque<i16>> = Mutex::new(VecDeque::new());
static PLAYING: AtomicBool = AtomicBool::new(false);
static SAMPLE_RATE: AtomicI32 = AtomicI32::new(0);

fn controls(code: i32) -> u16 {
    match code {
        97 => ButtonMask::B, 100 => ButtonMask::Y, 109 => ButtonMask::SELECT,
        108 => ButtonMask::START, 19 => ButtonMask::UP, 20 => ButtonMask::DOWN,
        21 => ButtonMask::LEFT, 22 => ButtonMask::RIGHT,
        96 => ButtonMask::A, 99 => ButtonMask::X,
        102 => ButtonMask::L, 103 => ButtonMask::R,
        _ => 0,
    }
}

fn set_message(message: String) {
    *MESSAGE.lock().unwrap_or_else(|e| e.into_inner()) = Some(message);
}


// All engine / OpenGL work occurs on GLSurfaceView's dedicated GL thread.
thread_local! {
    static ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) };
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum ShelfOverlay {
    None,
    Menu { row: usize },
    Library { row: usize },
    Scraping,
    Achievements,
    Core,
    GameMenu { row: usize },
}

const MENU_TEXT: [&str; 27] = [
    "MENU",                             // 0
    "Library",                          // 1
    "Scraping",                         // 2
    "RetroAchievements",               // 3
    "A Select     B Back",              // 4
    "LIBRARY",                          // 5
    "Choose ROM Folder",               // 6
    "Choose Save Folder",              // 7
    "Choose Save State Folder",        // 8
    "Refresh Library",                 // 9
    "Save locations coming soon",      // 10
    "SCRAPING",                         // 11
    "Artwork scraping coming soon",    // 12
    "RETROACHIEVEMENTS",               // 13
    "A: Check RAOfflineProxy status",    // 14
    "SELECT CORE",                      // 15
    "mGBA",                             // 16
    "gpSP (not installed)",            // 17
    "A Confirm     B Back",             // 18
    "GAME MENU",                        // 19
    "Resume",                           // 20
    "Save and Eject",                   // 21
    "States (coming soon)",            // 22
    "A Select     B Back",              // 23
    "B: Back",                          // 24
    "Left / Right: Swap",               // 25
    "A: Choose",                        // 26
];

struct Engine {
    gpu: Compositor,
    size: (u32, u32),
    shelves: Vec<Shelf>,
    active: usize,
    inserted: bool,
    progress: f32,
    born: Instant,
    last: Instant,
    library_version: u64,
    texture_cache: VecDeque<(usize, usize, slot_gfx::TexId)>,
    game: Option<GameSession>,
    buttons: u16,
    awaiting_game: bool,
    requested_uri: Option<String>,
    prepared_rom: Option<String>,
    seated_frame_seen: bool,
    game_accum: f64,
    overlay: ShelfOverlay,
    menu_textures: Vec<(slot_gfx::TexId, u32, u32)>,
    board_texture: Option<slot_gfx::TexId>,
    chip_texture: slot_gfx::TexId,
    picker: Option<CorePicker>,
    socket_textures: [slot_gfx::TexId; 2],
    chip_textures: [slot_gfx::TexId; 2],
    chip_blank: slot_gfx::TexId,
    chip_shadow: slot_gfx::TexId,
    a_down_at: Option<Instant>,
    fresh_launch: bool,
    mode_down_at: Option<Instant>,
    eject_sound_armed: bool,
}

impl Engine {
    fn new(library_version: u64, library: Option<&[RomEntry]>) -> Result<Self, String> {
        let size = (960, 640);
        let mut gpu = Compositor::new(&AndroidSurface { size }).map_err(|e| e.to_string())?;
        let mut shelves = Vec::new();

        // None means the user has not chosen a folder yet: retain M1 demo.
        // Some([]) is an explicitly empty library, not a reason to show fake games.
        let grouped = match library {
            Some(entries) => carts_by_platform(entries),
            None => {
                [
                    (Platform::Gba, ["ADVANCE ONE", "ADVANCE TWO", "ADVANCE THREE"]),
                    (Platform::Gb, ["CLASSIC ONE", "CLASSIC TWO", "CLASSIC THREE"]),
                    (Platform::Gbc, ["COLOR ONE", "COLOR TWO", "COLOR THREE"]),
                ].map(|(platform, names)| {
                    names.into_iter().map(|title| Cart {
                        platform,
                        stem: title.to_owned(),
                        rom: PathBuf::new(),
                        label: None,
                        title: title.to_owned(),
                        code: String::new(),
                        shell: None,
                    }).collect()
                })
            }
        };
        for carts in grouped {
            shelves.push(Shelf::new(carts));
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

        let menu_textures = MENU_TEXT.iter().map(|label| {
            let face = quick_value_face(label, true);
            let tex = gpu.create_texture(face.w, face.h, &face.rgba);
            (tex, face.w, face.h)
        }).collect();
        let chip = chip_face(Some(Core::Mgba));
        let chip_texture = gpu.create_texture(chip.w, chip.h, &chip.rgba);
        let socket_textures = Core::ALL.map(|core| {
            let face = socket_face(core);
            gpu.create_texture(face.w, face.h, &face.rgba)
        });
        let chip_textures = Core::ALL.map(|core| {
            if core == Core::Mgba { return chip_texture; }
            let face = chip_face(Some(core));
            gpu.create_texture(face.w, face.h, &face.rgba)
        });
        let blank = chip_face(None);
        let chip_blank = gpu.create_texture(blank.w, blank.h, &blank.rgba);
        let shadow = chip_shadow_face();
        let chip_shadow = gpu.create_texture(shadow.w, shadow.h, &shadow.rgba);

        // On a GBA-only library do not begin on an empty GB or GBC shelf.
        let active = shelves.iter().position(|s| !s.carts.is_empty()).unwrap_or(0);
        let now = Instant::now();
        Ok(Self {
            gpu,
            size,
            shelves,
            active,
            inserted: false,
            progress: 0.0,
            born: now,
            last: now,
            library_version,
            texture_cache: VecDeque::new(),
            game: None,
            buttons: 0,
            awaiting_game: false,
            requested_uri: None,
            prepared_rom: None,
            seated_frame_seen: false,
            game_accum: 0.0,
            overlay: ShelfOverlay::None,
            menu_textures,
            board_texture: None,
            chip_texture,
            picker: None,
            socket_textures,
            chip_textures,
            chip_blank,
            chip_shadow,
            a_down_at: None,
            fresh_launch: false,
            mode_down_at: None,
            eject_sound_armed: false,
        })
    }

    fn handle(&mut self, input: Input) {
        match input {
            Input::Reset => {
                self.buttons = 0;
                self.mode_down_at = None;
                self.overlay = ShelfOverlay::None;
                self.picker = None;
                for shelf in &mut self.shelves {
                    shelf.release_hold();
                }
            }
            Input::Suspend => {
                if let Some(session) = self.game.as_mut() {
                    session.save(true);
                }
                self.buttons = 0;
                AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
            }
            Input::Exit => {
                // The reversed cart animation will emit the original eject sound.
                if self.progress > 0.0 || self.inserted { self.eject_sound_armed = true; }
                if let Some(mut session) = self.game.take() {
                    session.save(true);
                }
                PLAYING.store(false, Ordering::Release);
                SAMPLE_RATE.store(0, Ordering::Release);
                AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
                self.buttons = 0;
                self.awaiting_game = false;
                self.inserted = false;
                self.overlay = ShelfOverlay::None;
                self.picker = None;
                self.a_down_at = None;
                self.fresh_launch = false;
                self.mode_down_at = None;
                self.requested_uri = None;
                self.prepared_rom = None;
                self.seated_frame_seen = false;
                *REQUEST.lock().unwrap_or_else(|e| e.into_inner()) = None;
            }
            Input::GameError { uri, message } => {
                if !self.inserted || !self.awaiting_game
                    || self.requested_uri.as_deref() != Some(uri.as_str())
                {
                    return;
                }
                set_message(format!("Cannot open game: {message}"));
                self.awaiting_game = false;
                self.inserted = false;
                self.requested_uri = None;
                self.prepared_rom = None;
                self.seated_frame_seen = false;
            }
            Input::GameReady { uri, path } => {
                // ROM preparation is asynchronous: a previously cancelled cart must
                // never launch after selecting another cartridge.
                if self.awaiting_game && self.inserted
                    && self.requested_uri.as_deref() == Some(uri.as_str())
                {
                    self.awaiting_game = false;
                    self.prepared_rom = Some(path);
                }
            }
            Input::Key(code, pressed) => {
                // KONKR's top-round controller key emits Linux BTN_MODE. Android
                // normally delivers that as KEYCODE_BUTTON_MODE (110); this is
                // separate from the system HOME launcher key.
                if code == 110 {
                    if pressed {
                        if self.mode_down_at.is_none() {
                            self.mode_down_at = Some(Instant::now());
                        }
                    } else if let Some(since) = self.mode_down_at.take() {
                        if since.elapsed().as_millis() >= 650 {
                            if self.game.is_some() {
                                self.handle(Input::Exit);
                            }
                        } else if self.game.is_some() {
                            // Short press: pause and show Slot's in-game menu.
                            self.buttons = 0;
                            self.game_accum = 0.0;
                            AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
                            self.overlay = match self.overlay {
                                ShelfOverlay::GameMenu { .. } => ShelfOverlay::None,
                                _ => ShelfOverlay::GameMenu { row: 0 },
                            };
                        }
                        // On the carousel, START is the sole menu shortcut;
                        // the physical MENU key is reserved for in-game actions.
                    }
                    return;
                }
                if self.game.is_some() && matches!(self.overlay, ShelfOverlay::GameMenu { .. }) {
                    if pressed {
                        match self.overlay {
                            ShelfOverlay::GameMenu { row } => match code {
                                19 | 20 => {
                                    let next = (row + 1) % 2;
                                    self.overlay = ShelfOverlay::GameMenu { row: next };
                                }
                                96 => {
                                    if row == 1 { self.handle(Input::Exit); }
                                    else { self.overlay = ShelfOverlay::None; }
                                }
                                97 => self.overlay = ShelfOverlay::None,
                                _ => {}
                            },
                            _ => {}
                        }
                    }
                    return;
                }
                if self.game.is_some() {
                    let mask = controls(code);
                    if pressed { self.buttons |= mask; } else { self.buttons &= !mask; }
                    let exit_combo = ButtonMask::START | ButtonMask::SELECT;
                    if pressed && (self.buttons & exit_combo) == exit_combo {
                        self.handle(Input::Exit);
                    }
                    return;
                }
                if self.overlay != ShelfOverlay::None {
                    if pressed {
                        match self.overlay {
                            ShelfOverlay::Menu { row } => match code {
                                19 | 20 => {
                                    let next = if code == 19 { (row + 2) % 3 } else { (row + 1) % 3 };
                                    self.overlay = ShelfOverlay::Menu { row: next };
                                }
                                96 => {
                                    self.overlay = match row {
                                        0 => ShelfOverlay::Library { row: 0 },
                                        1 => ShelfOverlay::Scraping,
                                        _ => ShelfOverlay::Achievements,
                                    };
                                }
                                97 | 108 => self.overlay = ShelfOverlay::None,
                                _ => {}
                            },
                            ShelfOverlay::Library { row } => match code {
                                19 | 20 => {
                                    let next = if code == 19 { (row + 3) % 4 } else { (row + 1) % 4 };
                                    self.overlay = ShelfOverlay::Library { row: next };
                                }
                                96 => match row {
                                    0 | 3 => {
                                        UI_ACTION.lock().unwrap_or_else(|e| e.into_inner())
                                            .push_back(if row == 0 { 1 } else { 2 });
                                        self.overlay = ShelfOverlay::None;
                                    }
                                    // Save and save-state folder pickers will be
                                    // enabled together with real SAF save routing.
                                    1 | 2 => {}
                                    _ => {}
                                },
                                97 => self.overlay = ShelfOverlay::Menu { row: 0 },
                                108 => self.overlay = ShelfOverlay::None,
                                _ => {}
                            },
                            ShelfOverlay::Scraping | ShelfOverlay::Achievements => match code {
                                96 if matches!(self.overlay, ShelfOverlay::Achievements) => {
                                    UI_ACTION.lock().unwrap_or_else(|e| e.into_inner()).push_back(3);
                                }
                                97 => self.overlay = ShelfOverlay::Menu {
                                    row: if matches!(self.overlay, ShelfOverlay::Scraping) { 1 } else { 2 },
                                },
                                108 => self.overlay = ShelfOverlay::None,
                                _ => {}
                            },
                            ShelfOverlay::Core => {
                                let now = self.born.elapsed().as_millis() as u64;
                                if let Some(picker) = self.picker.as_mut() {
                                    match code {
                                        21 => { picker.press(Press::Left, now); }
                                        22 => { picker.press(Press::Right, now); }
                                        96 => {
                                            if picker.seat() == Core::Gpsp {
                                                // Keep the authentic chip-hop animation but never
                                                // persist/choose a core that is not in the APK.
                                                set_message("gpSP core is not installed yet".into());
                                            } else {
                                                picker.press(Press::Keep, now);
                                            }
                                        }
                                        97 | 109 => { picker.press(Press::Back, now); }
                                        _ => {}
                                    }
                                }
                            },
                            ShelfOverlay::GameMenu { .. } | ShelfOverlay::None => {}
                        }
                    }
                    return;
                }
                if pressed {
                    match code {
                        108 if !self.inserted => {
                            self.shelves[self.active].release_hold();
                            self.overlay = ShelfOverlay::Menu { row: 0 };
                            return;
                        }
                        109 if !self.inserted && !self.shelves[self.active].carts.is_empty() => {
                            let selected = &self.shelves[self.active].carts[self.shelves[self.active].index];
                            if selected.platform != Platform::Gba {
                                set_message("mGBA is the only installed GB/GBC core".into());
                                return;
                            }
                            let face = board_face(selected);
                            if let Some(texture) = self.board_texture {
                                self.gpu.update_texture(texture, face.w, face.h, &face.rgba);
                            } else {
                                self.board_texture = Some(self.gpu.create_texture(face.w, face.h, &face.rgba));
                            }
                            let now = self.born.elapsed().as_millis() as u64;
                            let mut picker = CorePicker::open(Core::Mgba, now);
                            picker.start(now);
                            self.picker = Some(picker);
                            self.overlay = ShelfOverlay::Core;
                            return;
                        }
                        102 | 103 if !self.inserted => {
                            let delta = if code == 102 { -1 } else { 1 };
                            // An empty platform is never a visible stop.
                            let next = next_nonempty_shelf(&self.shelves, self.active, delta);
                            if next != self.active {
                                self.shelves[self.active].release_hold();
                                self.active = next;
                                self.progress = 0.0;
                            }
                            return;
                        }
                        96 if !self.inserted && !self.shelves[self.active].carts.is_empty() => {
                            self.inserted = true;
                            let selected = &self.shelves[self.active].carts[self.shelves[self.active].index];
                            let uri = selected.rom.to_string_lossy();
                            self.seated_frame_seen = false;
                            self.prepared_rom = None;
                            self.a_down_at = Some(Instant::now());
                            self.fresh_launch = false;
                            if uri.starts_with("content://") {
                                let uri = uri.into_owned();
                                self.requested_uri = Some(uri.clone());
                                self.awaiting_game = true;
                                *REQUEST.lock().unwrap_or_else(|e| e.into_inner()) = Some(uri);
                            }
                            return;
                        }
                        97 => {
                            if self.inserted && self.progress > 0.0 {
                                self.eject_sound_armed = true;
                            }
                            self.inserted = false;
                            self.a_down_at = None;
                            self.fresh_launch = false;
                            self.awaiting_game = false;
                            self.prepared_rom = None;
                            self.requested_uri = None;
                            self.seated_frame_seen = false;
                            *REQUEST.lock().unwrap_or_else(|e| e.into_inner()) = None;
                            return;
                        }
                        _ => {}
                    }
                }
                if code == 96 && !pressed {
                    if let Some(when) = self.a_down_at.take() {
                        self.fresh_launch = when.elapsed().as_millis() >= 400;
                    }
                    return;
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

    // Only rasterize visible cartridges. Recycling at most 42 GPU slots avoids
    // allocating one image per ROM when users select a large ROMM library.
    fn prepare_visible(&mut self) {
        let shelf_id = self.active;
        let needed = self.shelves[shelf_id].on_screen();
        for index in needed {
            if self.shelves[shelf_id].face(index).is_some() {
                continue;
            }
            let face = cart_face_with(&self.shelves[shelf_id].carts[index], None);
            let texture = if self.texture_cache.len() >= 42 {
                let (old_shelf, old_index, tex) = self.texture_cache
                    .pop_front().expect("nonempty texture pool");
                self.shelves[old_shelf].take_face(old_index);
                self.gpu.update_texture(tex, face.w, face.h, &face.rgba);
                tex
            } else {
                self.gpu.create_texture(face.w, face.h, &face.rgba)
            };
            self.shelves[shelf_id].set_face(index, texture);
            self.texture_cache.push_back((shelf_id, index, texture));
        }
    }

    fn start_prepared_game(&mut self, local: &str) {
        let paths = PATHS.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some((storage, library)) = paths {
            match GameSession::open(Path::new(local), &library.join("libmgba_libretro.so"), &storage, !self.fresh_launch) {
                Ok(session) => {
                    self.buttons = 0;
                    SAMPLE_RATE.store(session.sample_rate, Ordering::Release);
                    self.game = Some(session);
                    self.game_accum = 0.0;
                    self.gpu.set_screen_effect(ScreenEffect::None);
                    self.gpu.set_screen_power(1.0);
                    PLAYING.store(true, Ordering::Release);
                }
                Err(error) => {
                    set_message(format!("mGBA: {error}"));
                    self.inserted = false;
                    self.requested_uri = None;
                }
            }
        } else {
            set_message("Android game paths are not configured".into());
            self.inserted = false;
            self.requested_uri = None;
        }
    }

    fn add_text(&self, index: usize, x: f32, y: f32, out: &mut Vec<Draw>) {
        self.add_text_alpha(index, x, y, 1.0, out);
    }

    fn add_text_alpha(&self, index: usize, x: f32, y: f32, alpha: f32, out: &mut Vec<Draw>) {
        let (tex, w, h) = self.menu_textures[index];
        out.push(Draw::Tex { x, y, w: w as f32, h: h as f32, tex, alpha });
    }

    fn text_fit(&self, index: usize, x: f32, y: f32, max_w: f32,
                alpha: f32, out: &mut Vec<Draw>) {
        let (tex, raw_w, raw_h) = self.menu_textures[index];
        // Keep original Slot typography, but fit long settings labels without
        // horizontally stretching them or overflowing a 3:2 display.
        if raw_w == 0 || raw_h == 0 { return; }
        let scale = 0.79f32.min(max_w / raw_w as f32);
        out.push(Draw::Tex {
            x, y, w: raw_w as f32 * scale, h: raw_h as f32 * scale,
            tex, alpha,
        });
    }

    fn modal(&self, x: f32, y: f32, w: f32, h: f32,
             out: &mut Vec<Draw>, dim: f32) {
        out.push(Draw::Rect {
            x: 0.0, y: 0.0, w: OUT_W as f32, h: OUT_H as f32,
            colour: [0.0, 0.0, 0.0, dim],
        });
        out.push(Draw::Rect {
            x, y, w, h, colour: [0.085, 0.085, 0.093, 1.0],
        });
        out.push(Draw::Rect {
            x, y, w, h: 2.0, colour: [0.69, 0.69, 0.72, 1.0],
        });
    }

    fn draw_overlay(&self, out: &mut Vec<Draw>) {
        match self.overlay {
            ShelfOverlay::None => {}
            ShelfOverlay::Menu { row } => {
                self.modal(117.0, 75.0, 486.0, 330.0, out, 0.78);
                self.text_fit(0, 148.0, 98.0, 400.0, 1.0, out);
                for index in 0..3 {
                    let y = 156.0 + index as f32 * 59.0;
                    if index == row {
                        out.push(Draw::Rect {
                            x: 135.0, y: y - 6.0, w: 450.0, h: 44.0,
                            colour: [0.28, 0.28, 0.32, 1.0],
                        });
                    }
                    self.text_fit(index + 1, 152.0, y, 406.0, 1.0, out);
                }
                self.text_fit(4, 152.0, 360.0, 405.0, 0.85, out);
            }
            ShelfOverlay::Library { row } => {
                self.modal(105.0, 41.0, 510.0, 400.0, out, 0.78);
                self.text_fit(5, 136.0, 63.0, 445.0, 1.0, out);
                for index in 0..4 {
                    let y = 131.0 + index as f32 * 56.0;
                    if index == row {
                        out.push(Draw::Rect {
                            x: 125.0, y: y - 7.0, w: 470.0, h: 43.0,
                            colour: [0.28, 0.28, 0.32, 1.0],
                        });
                    }
                    self.text_fit(index + 6, 145.0, y, 430.0,
                        if index == 1 || index == 2 { 0.4 } else { 1.0 }, out);
                }
                self.text_fit(10, 145.0, 359.0, 440.0, 0.5, out);
                self.text_fit(4, 145.0, 409.0, 440.0, 0.85, out);
            }
            ShelfOverlay::Scraping | ShelfOverlay::Achievements => {
                self.modal(110.0, 127.0, 500.0, 227.0, out, 0.78);
                let (header, detail) = if matches!(self.overlay, ShelfOverlay::Scraping) {
                    (11, 12)
                } else {
                    (13, 14)
                };
                self.text_fit(header, 141.0, 152.0, 438.0, 1.0, out);
                self.text_fit(detail, 141.0, 230.0, 438.0, 0.65, out);
                self.text_fit(4, 141.0, 311.0, 438.0, 0.85, out);
            }
            ShelfOverlay::GameMenu { row } => {
                self.modal(117.0, 80.0, 486.0, 320.0, out, 0.75);
                self.text_fit(19, 148.0, 103.0, 400.0, 1.0, out);
                for index in 0..2 {
                    let y = 162.0 + index as f32 * 61.0;
                    if index == row {
                        out.push(Draw::Rect {
                            x: 135.0, y: y - 7.0, w: 450.0, h: 45.0,
                            colour: [0.28, 0.28, 0.32, 1.0],
                        });
                    }
                    self.text_fit(index + 20, 152.0, y, 406.0, 1.0, out);
                }
                self.text_fit(22, 152.0, 291.0, 420.0, 0.42, out);
                self.text_fit(23, 152.0, 355.0, 420.0, 0.85, out);
            }
            ShelfOverlay::Core => {
                self.draw_original_core_picker(out);
            }
        }
    }

    /// Slot's original board/lid/socket/chip geometry and timelines.
    /// The picker state machine is the unmodified upstream core_picker.rs.
    fn draw_original_core_picker(&self, out: &mut Vec<Draw>) {
        let Some(picker) = self.picker else { return };
        let now = self.born.elapsed().as_millis() as u64;
        let progress = picker.openness(now);
        let lift = lift_of(progress);
        let shelf = &self.shelves[self.active];
        let (rest, scale) = shelf.selected_at();
        let cart = shelf_cart_at(rest, scale);
        let board = board_from(cart, progress);
        let zoom = board_zoom(board);

        if let Some(tex) = self.board_texture {
            out.push(Draw::Tex {
                x: board.x, y: board.y, w: board.w, h: board.h,
                tex, alpha: 1.0,
            });
        }
        for (i, tex) in self.socket_textures.iter().copied().enumerate() {
            let (x, y) = on_board(board, SOCKET_U[i], SOCKET_V);
            out.push(Draw::Tex {
                x: x.round(), y: y.round(),
                w: SOCKET_W as f32 * zoom, h: SOCKET_H as f32 * zoom,
                tex, alpha: 1.0,
            });
        }

        let chip = picker.chip(now);
        let u = CHIP_U[0] + (CHIP_U[1] - CHIP_U[0]) * chip.across;
        if chip.lift > 0.0 {
            let (cx, cy) = on_board(board, u + 19.0, CHIP_V + 29.4);
            let (w, h) = (SHADOW_W as f32 * zoom, SHADOW_H as f32 * zoom);
            out.push(Draw::Tex {
                x: cx - w / 2.0, y: cy - h / 2.0, w, h,
                tex: self.chip_shadow, alpha: 0.6 * chip.lift * lift,
            });
        }
        let tex = chip.seated
            .map(|core| self.chip_textures[core.index()])
            .unwrap_or(self.chip_blank);
        let (x, y) = on_board(board, u, CHIP_V - HOP_LIFT * chip.lift);
        let body = grown(slot_ui::Placed {
            x: x + chip.shake, y,
            w: CHIP_W as f32 * zoom, h: CHIP_H as f32 * zoom,
        }, TURN_PAD as f32 * zoom);
        out.push(Draw::Turned {
            x: body.x.round(), y: body.y.round(),
            w: body.w, h: body.h, tex, alpha: 1.0, turn: chip.tip,
        });

        // The original lid slides up first (160ms), lifts and tilts (260ms)
        // and returns smoothly from any partial openness on B/A (320ms).
        let (lid, turn) = lid_from(cart, progress);
        let k = lid.w / slot_ui::lid_at(1.0).0.w;
        let (w, h) = (168.0 * k, 18.0 * k);
        out.push(Draw::Tex {
            x: lid.x + (lid.w - w) / 2.0,
            y: lid.y + lid.h + 29.0 * k - h / 2.0,
            w, h,
            tex: self.chip_shadow,
            alpha: 0.8 * lift,
        });
        if let Some(tex) = shelf.face(shelf.index) {
            let face = grown(lid, TURN_PAD as f32 * lid.w / CART_W as f32);
            out.push(Draw::Turned {
                x: face.x, y: face.y, w: face.w, h: face.h,
                tex, alpha: 1.0, turn,
            });
        }

        // Original Slot bottom three-column legend, fading in with the lift.
        let right = BOARD_X + BOARD_W as f32;
        self.text_fit(24, BOARD_X, 387.0, 150.0, lift, out);
        self.text_fit(25, 293.0, 387.0, 190.0, lift, out);
        self.text_fit(26, right - 118.0, 387.0, 118.0, lift, out);
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
        if self.picker.is_some_and(|p| p.finished(self.born.elapsed().as_millis() as u64)) {
            self.picker = None;
            self.overlay = ShelfOverlay::None;
        }
        // Holding physical MENU saves, ejects, and returns to shelf without
        // requiring a second keypress or sending MENU to libretro.
        if self.game.is_some()
            && self.mode_down_at.is_some_and(|t| t.elapsed().as_millis() >= 650)
        {
            self.mode_down_at = None;
            self.handle(Input::Exit);
        }

        // A long A press skips loading the auto-save state, but preserves SRAM.
        // At least 400ms must pass before treating a held A as a fresh launch.
        if let Some(when) = self.a_down_at {
            if when.elapsed().as_millis() >= 400 {
                self.fresh_launch = true;
                self.a_down_at = None;
            }
        }
        // ROM cache may finish at any time, but core.load() and automatic state
        // restore only occur AFTER the full 730ms insertion has been displayed.
        if self.inserted && self.progress >= 1.0 && self.seated_frame_seen
            && self.a_down_at.is_none()
        {
            if let Some(local) = self.prepared_rom.take() {
                self.start_prepared_game(&local);
            }
        }

        if self.game.is_some() && matches!(self.overlay, ShelfOverlay::GameMenu { .. }) {
            // Rendering the pause menu does not borrow an active libretro session
            // or advance the core, so input and audio stay frozen.
            self.game_accum = 0.0;
            let mut commands = vec![Draw::Game];
            self.draw_overlay(&mut commands);
            self.gpu.fit(self.size);
            self.gpu.begin_frame();
            self.gpu.draw_list(&commands);
            self.gpu.end_frame(self.size);
            return;
        }
        if let Some(session) = self.game.as_mut() {
            // Independent from display refresh: a 120 Hz panel must not run mGBA at 2x speed.
            self.game_accum = (self.game_accum + f64::from(dt)).min(0.10);
            let period = 1.0 / session.fps;
            let mut stepped = 0;
            while self.game_accum >= period && stepped < 4 {
                session.advance(self.buttons);
                self.game_accum -= period;
                stepped += 1;
                let samples = session.take_audio();
                if !samples.is_empty() {
                    let mut audio = AUDIO.lock().unwrap_or_else(|e| e.into_inner());
                    let overflow = audio.len().saturating_add(samples.len()).saturating_sub(96_000);
                    for _ in 0..overflow.min(audio.len()) { audio.pop_front(); }
                    audio.extend(samples);
                }
            }
            if stepped > 0 { self.gpu.upload_game(session.frame()); }
            self.gpu.fit(self.size);
            self.gpu.begin_frame();
            self.gpu.draw_list(&[Draw::Game]);
            self.gpu.end_frame(self.size);
            return;
        }

        let ms = self.born.elapsed().as_millis() as u64;
        self.shelves[self.active].tick(ms);
        self.shelves[self.active].update(dt);
        self.prepare_visible();
        let shelf = &mut self.shelves[self.active];

        // Original Slot's mechanical PCM is kept intact; move its onset to
        // 480ms so the 240ms clip finishes as the 730ms cart seats on KONKR.
        // The ejection effect follows the upstream 350ms hold. Keep this
        // independent of the game AudioTrack: no emulator needs to be running.
        let previous_progress = self.progress;
        let change = dt / 0.73;
        self.progress = if self.inserted {
            (self.progress + change).min(1.0)
        } else {
            (self.progress - change).max(0.0)
        };
        if self.inserted && previous_progress < INSERT_SOUND_PROGRESS
            && self.progress >= INSERT_SOUND_PROGRESS
        {
            CART_SFX.lock().unwrap_or_else(|e| e.into_inner()).push_back(CART_INSERT_SFX);
        }
        if !self.inserted && self.eject_sound_armed && self.progress <= EJECT_SOUND_PROGRESS {
            self.eject_sound_armed = false;
            CART_SFX.lock().unwrap_or_else(|e| e.into_inner()).push_back(CART_EJECT_SFX);
        }

        let mut commands = vec![Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: OUT_W as f32,
            h: OUT_H as f32,
            colour: [0.085, 0.085, 0.093, 1.0],
        }];
        if self.progress == 0.0 {
            if let Some(picker) = self.picker {
                let t = slot_ui::ease(picker.openness(self.born.elapsed().as_millis() as u64));
                let dim = 1.0 + (0.614 - 1.0) * t;
                let selected = shelf.carts.get(shelf.index).map(|c| c.stem.as_str());
                shelf.draw_row(selected, 0.0, 0.26 * t, dim, &mut commands);
                draw_empty_slot(&mut commands);
            } else {
                shelf.draw(0.0, &mut commands);
            }
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
        self.draw_overlay(&mut commands);
        self.gpu.fit(self.size);
        self.gpu.begin_frame();
        self.gpu.draw_list(&commands);
        self.gpu.end_frame(self.size);
        self.seated_frame_seen = self.inserted && self.progress >= 1.0;
    }
}

/// Return the current shelf if it is the only populated one, and never
/// select an empty GB/GBC/GBA shelf. Safe even if all libraries are empty.
fn next_nonempty_shelf(shelves: &[Shelf], current: usize, delta: i32) -> usize {
    let counts: Vec<usize> = shelves.iter().map(|s| s.carts.len()).collect();
    crate::library::next_populated(current, delta, &counts)
}

#[cfg(test)]
mod ui_feedback_tests {
    use super::*;
    #[test]
    fn empty_shelves_are_skipped_in_both_directions() {
        let mut shelves: Vec<Shelf> = (0..3).map(|_| Shelf::new(vec![])).collect();
        assert_eq!(next_nonempty_shelf(&shelves, 0, 1), 0);
        shelves[0].carts.push(Cart {
            platform: Platform::Gba, stem: "Test".into(), title: "Test".into(),
            code: String::new(), rom: PathBuf::new(), label: None, shell: None,
        });
        assert_eq!(next_nonempty_shelf(&shelves, 0, 1), 0);
        assert_eq!(next_nonempty_shelf(&shelves, 0, -1), 0);
        shelves[2].carts.push(Cart {
            platform: Platform::Gbc, stem: "Color".into(), title: "Color".into(),
            code: String::new(), rom: PathBuf::new(), label: None, shell: None,
        });
        assert_eq!(next_nonempty_shelf(&shelves, 0, 1), 2);
        assert_eq!(next_nonempty_shelf(&shelves, 0, -1), 2);
        assert_eq!(next_nonempty_shelf(&shelves, 2, 1), 0);
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
        PLAYING.store(false, Ordering::Release);
        SAMPLE_RATE.store(0, Ordering::Release);
        let (version, snapshot) = {
            let state = LIBRARY.lock().unwrap_or_else(|err| err.into_inner());
            (state.version, state.entries.clone())
        };
        match Engine::new(version, snapshot.as_deref()) {
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
        let current = holder.borrow().as_ref().map(|engine| engine.library_version);
        let updated = {
            let state = LIBRARY.lock().unwrap_or_else(|err| err.into_inner());
            if current.is_some_and(|version| version != state.version) {
                Some((state.version, state.entries.clone()))
            } else {
                None
            }
        };
        if let Some((version, snapshot)) = updated {
            // Saving an in-progress game on library replacement is mandatory.
            if let Some(engine) = holder.borrow_mut().as_mut() {
                if let Some(mut session) = engine.game.take() { session.save(true); }
            }
            PLAYING.store(false, Ordering::Release);
            SAMPLE_RATE.store(0, Ordering::Release);
            // Destruct old GL resources on the GL thread before recreating them.
            let size = holder.borrow().as_ref().map(|engine| engine.size).unwrap_or((960, 640));
            *holder.borrow_mut() = None;
            match Engine::new(version, snapshot.as_deref()) {
                Ok(mut engine) => {
                    engine.size = size;
                    *holder.borrow_mut() = Some(engine);
                }
                Err(error) => eprintln!("slot-konkr: could not reload library: {error}"),
            }
        }
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


#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativePollUiAction(
    _env: *mut c_void, _this: *mut c_void,
) -> jint {
    UI_ACTION.lock().unwrap_or_else(|e| e.into_inner()).pop_front().unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativePollCartSfx(
    _env: *mut c_void, _this: *mut c_void,
) -> jint {
    CART_SFX.lock().unwrap_or_else(|e| e.into_inner()).pop_front().unwrap_or(0)
}

fn push(input: Input) {
    INPUT.lock().unwrap_or_else(|e| e.into_inner()).push_back(input);
}

/// App-private files directory and nativeLibraryDir (where mGBA is packaged).
#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativeConfigure(
    mut env: JNIEnv<'_>, _this: JObject<'_>, root: JString<'_>, libraries: JString<'_>,
) {
    if let (Ok(root), Ok(libraries)) = (env.get_string(&root), env.get_string(&libraries)) {
        *PATHS.lock().unwrap_or_else(|e| e.into_inner()) =
            Some((PathBuf::from(root.to_string_lossy().as_ref()), PathBuf::from(libraries.to_string_lossy().as_ref())));
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativePollLaunchUri(
    env: JNIEnv<'_>, _this: JObject<'_>,
) -> jstring {
    let next = REQUEST.lock().unwrap_or_else(|e| e.into_inner()).take();
    match next.and_then(|v| env.new_string(v).ok()) {
        Some(v) => v.into_raw(), None => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativeGameReady(
    mut env: JNIEnv<'_>, _this: JObject<'_>, uri: JString<'_>, path: JString<'_>,
) {
    if let (Ok(uri), Ok(path)) = (env.get_string(&uri), env.get_string(&path)) {
        push(Input::GameReady {
            uri: uri.to_string_lossy().into_owned(),
            path: path.to_string_lossy().into_owned(),
        });
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativeGameError(
    mut env: JNIEnv<'_>, _this: JObject<'_>, uri: JString<'_>, message: JString<'_>,
) {
    if let (Ok(uri), Ok(message)) = (env.get_string(&uri), env.get_string(&message)) {
        push(Input::GameError {
            uri: uri.to_string_lossy().into_owned(),
            message: message.to_string_lossy().into_owned(),
        });
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativeExitGame(
    _env: *mut c_void, _this: *mut c_void,
) { push(Input::Exit); }

#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativeSuspend(
    _env: *mut c_void, _this: *mut c_void,
) {
    ENGINE.with(|holder| {
        if let Some(engine) = holder.borrow_mut().as_mut() { engine.handle(Input::Suspend); }
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativeIsPlaying(
    _env: *mut c_void, _this: *mut c_void,
) -> jboolean { PLAYING.load(Ordering::Acquire) as jboolean }

#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativeAudioSampleRate(
    _env: *mut c_void, _this: *mut c_void,
) -> jint { SAMPLE_RATE.load(Ordering::Acquire) }

#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativeReadAudio(
    mut env: JNIEnv<'_>, _this: JObject<'_>,
) -> jshortArray {
    let samples: Vec<i16> = {
        let mut audio = AUDIO.lock().unwrap_or_else(|e| e.into_inner());
        let n = audio.len().min(4096);
        audio.drain(..n).collect()
    };
    let Ok(out) = env.new_short_array(samples.len() as i32) else { return std::ptr::null_mut(); };
    let _ = env.set_short_array_region(&out, 0, &samples);
    out.into_raw()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativePollMessage(
    env: JNIEnv<'_>, _this: JObject<'_>,
) -> jstring {
    match MESSAGE.lock().unwrap_or_else(|e| e.into_inner()).take().and_then(|v| env.new_string(v).ok()) {
        Some(v) => v.into_raw(), None => std::ptr::null_mut(),
    }
}
