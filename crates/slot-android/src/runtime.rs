use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::{c_char, c_void, CString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use crate::game::GameSession;
use crate::settings::Settings;
use crate::video_mode::{self, VideoMode};
use crate::core_selection;
use crate::core_picker::{CorePicker, Press};
use slot_retro::ButtonMask;
use jni::{JNIEnv, objects::{JObject, JString}, sys::{jint, jboolean, jshortArray, jstring}};
use std::sync::Mutex;
use std::time::Instant;

use slot_gfx::{Compositor, GfxError, Surface, OUT_H, OUT_W};
use slot_store::{Cart, Platform, Core, stamp_now};
use slot_power::{Battery, Charge};
use crate::library::{carts_by_platform, RomEntry, LIBRARY};
use slot_ui::{
    board_face, chip_face, chip_shadow_face, socket_face, quick_value_face, cart_face_with_material,
    cart_shadow, gb_cart_shadow, board_from, board_zoom, lift_of, shelf_cart_at,
    on_board, lid_from, grown, draw_empty_slot, Draw, GbShell, Shelf, SlotChrome,
    BOARD_X, BOARD_W, SOCKET_U, SOCKET_V, SOCKET_W, SOCKET_H,
    CHIP_U, CHIP_V, CHIP_W, CHIP_H, HOP_LIFT, TURN_PAD, SHADOW_W, SHADOW_H,
    CART_W, hint_face, arrows_hint_face, title_face, photo_face, Polaroids, Printed, HINT_H, HINT_EDGE,
    draw_footer, draw_slot_name, word_face, icon_face, Icon, BOLT_PX, HUD_INK,
    Hud, HudKind, FfState, Toast, toast_face, QUICK_PITCH, opening, edge, centred_hints, LEGEND_GAP,
    sticker_face_konkr, StickerFields, STICKER_W, STICKER_H, quick_caret_face,
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
    ReloadVisuals,
    ReloadCartLabel { uri: String },
}

static INPUT: Mutex<VecDeque<Input>> = Mutex::new(VecDeque::new());
static REQUEST: Mutex<Option<String>> = Mutex::new(None);
static LABEL_PICK_REQUEST: Mutex<Option<String>> = Mutex::new(None);
static UI_ACTION: Mutex<VecDeque<i32>> = Mutex::new(VecDeque::new());
static SAVE_SYNC: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
static CART_SFX: Mutex<VecDeque<i32>> = Mutex::new(VecDeque::new());
const CART_INSERT_SFX: i32 = 1;
const CART_EJECT_SFX: i32 = 2;
// Exact upstream Slot timeline: 450ms mechanical seating + 280ms hold.
const INSERT_S: f32 = 0.73;
const SEATED_AT: f32 = 0.45;
const INSERT_SOUND_PROGRESS: f32 = (SEATED_AT - 0.24) / SEATED_AT;
const EJECT_SOUND_PROGRESS: f32 = 1.0 - (0.350 / 0.450);
// Upstream Slot screen power transition times (app.rs).
const SCREEN_POWER_ON_S: f32 = 0.22;
const SCREEN_POWER_OFF_S: f32 = 0.16;
const EJECT_S: f32 = 0.45;
const REWIND_STEP_S: f32 = 0.10;  // Original Slot's time-travel HUD + ~10 Hz restoration.

const FF_DOUBLE_TAP_MS: u128 = 320;
static MESSAGE: Mutex<Option<String>> = Mutex::new(None);
static PATHS: Mutex<Option<(PathBuf, PathBuf)>> = Mutex::new(None);
static AUDIO: Mutex<VecDeque<i16>> = Mutex::new(VecDeque::new());
static PLAYING: AtomicBool = AtomicBool::new(false);
static SAMPLE_RATE: AtomicI32 = AtomicI32::new(0);
// Android AudioTrack time-stretches 2x game audio while preserving pitch.
// Expose the current transport speed separately from libretro's sample rate.
static AUDIO_SPEED_PERMILLE: AtomicI32 = AtomicI32::new(1000);
static FF_AUDIO_SUPPORTED: AtomicBool = AtomicBool::new(true);
static FF_AUDIO_ENABLED: AtomicBool = AtomicBool::new(true);
static RUMBLE_STRENGTH: AtomicI32 = AtomicI32::new(0);

#[derive(Clone)]
struct SystemStatus {
    clock: String,
    battery: Option<Battery>,
}
static SYSTEM_STATUS: Mutex<SystemStatus> = Mutex::new(SystemStatus {
    clock: String::new(), battery: None,
});

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
    Settings { row: usize },
    ScreenSettings { row: usize },
    GameplaySettings { row: usize },
    Personalization { row: usize },
    About,
    Scraping,
    Achievements,
    Core,
    GameMenu { row: usize },
    States,
}

const MENU_TEXT: [&str; 48] = [
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
    "RetroArch core folders",           // 10
    "SCRAPING",                         // 11
    "Artwork scraping coming soon",    // 12
    "RETROACHIEVEMENTS",               // 13
    "A: Check RAOfflineProxy status",    // 14
    "SELECT CORE",                      // 15
    "mGBA",                             // 16
    "gpSP",                            // 17
    "A Confirm     B Back",             // 18
    "GAME MENU",                        // 19
    "Resume",                           // 20
    "Save and Eject",                   // 21
    "Save States",                      // 22
    "A Select     B Back",              // 23
    "Choose BIOS Folder",               // 24
    "Settings",                         // 25
    "SETTINGS",                         // 26
    "Fast Forward",                     // 27
    "Fast Forward Sound",               // 28
    "Color Correction",                 // 29
    "Rumble",                           // 30
    "About",                            // 31
    "SCREEN",                           // 32
    "GAMEPLAY",                         // 33
    "GBA Shader",                       // 34
    "GB / GBC Shader",                  // 35
    "GB Palettes",                      // 36
    "Rewind",                           // 37
    "Turbo Buttons",                    // 38
    "Auto Save on Eject",               // 39
    "Personalization",                  // 40
    "PERSONALIZATION",                  // 41
    "Import Theme",                     // 42
    "Import Wallpaper",                 // 43
    "Reset Theme",                      // 44
    "Remove Wallpaper",                 // 45
    "Import Selected Cart Label",       // 46
    "Remove Selected Cart Label",       // 47
];

struct Engine {
    gpu: Compositor,
    size: (u32, u32),
    shelves: Vec<Shelf>,
    active: usize,
    inserted: bool,
    progress: f32,
    insert_started: Option<Instant>,
    screen_power: f32,
    exiting_screen: bool,
    born: Instant,
    last: Instant,
    library_version: u64,
    texture_cache: VecDeque<(usize, usize, slot_gfx::TexId)>,
    game: Option<GameSession>,
    pending_game: Option<GameSession>,
    buttons: u16,
    ff_held: bool,
    ff_latched: bool,
    ff_last_release: Option<Instant>,
    rewind_held: bool,
    rewind_accum: f32,
    awaiting_game: bool,
    requested_uri: Option<String>,
    prepared_rom: Option<String>,
    game_accum: f64,
    overlay: ShelfOverlay,
    settings: Settings,
    video_mode: VideoMode,
    wallpaper_texture: Option<slot_gfx::TexId>,
    setting_values: Vec<(slot_gfx::TexId, u32, u32)>,
    setting_carets: [(slot_gfx::TexId, u32, u32); 2],
    settings_hints: [(slot_gfx::TexId, u32); 2],
    about_texture: Option<slot_gfx::TexId>,
    menu_seen: u8,
    menu_opened: Instant,
    menu_cursor_y: f32,
    menu_textures: Vec<(slot_gfx::TexId, u32, u32)>,
    menu_hints: [(slot_gfx::TexId, u32); 2],
    game_menu_hints: [(slot_gfx::TexId, u32); 3],
    board_texture: Option<slot_gfx::TexId>,
    chip_texture: slot_gfx::TexId,
    picker: Option<CorePicker>,
    socket_textures: [slot_gfx::TexId; 2],
    chip_textures: [slot_gfx::TexId; 2],
    chip_blank: slot_gfx::TexId,
    chip_shadow: slot_gfx::TexId,
    core_legend_faces: [(slot_gfx::TexId, u32); 3],
    a_down_at: Option<Instant>,
    fresh_launch: bool,
    mode_down_at: Option<Instant>,
    eject_sound_armed: bool,
    mode_last_tap: Option<Instant>,
    polaroids: Option<Polaroids>,
    photo_textures: Vec<slot_gfx::TexId>,
    polaroid_title_texture: Option<slot_gfx::TexId>,
    hud: Hud,
    clock_text: String,
    clock_texture: Option<slot_gfx::TexId>,
    clock_face: Printed,
    battery_percent: Option<u8>,
    battery_texture: Option<slot_gfx::TexId>,
    battery_face: Printed,
    battery: Option<Battery>,
    bolt_texture: slot_gfx::TexId,
    letter_texture: Option<slot_gfx::TexId>,
    letter_face: Printed,
    letter_shown_at: Option<Instant>,
}

impl Engine {
    fn new(library_version: u64, library: Option<&[RomEntry]>) -> Result<Self, String> {
        let size = (960, 640);
        let mut gpu = Compositor::new(&AndroidSurface { size }).map_err(|e| e.to_string())?;
        let settings = PATHS.lock().unwrap_or_else(|e| e.into_inner())
            .as_ref().map(|(root, _)| Settings::load(root)).unwrap_or_default();
        gpu.set_colour_correction(settings.colour_correction);
        FF_AUDIO_ENABLED.store(settings.ff_sound, Ordering::Release);
        let visuals_root = PATHS.lock().unwrap_or_else(|e| e.into_inner())
            .as_ref().map(|(root, _)| root.clone());
        if let Some(root) = visuals_root.as_deref() {
            slot_ui::set_theme(slot_store::Theme::read(root));
        }
        let wallpaper_texture = visuals_root.as_deref()
            .and_then(|root| crate::wallpaper::pick(root, 0))
            .as_deref().and_then(slot_ui::wallpaper_face)
            .map(|rgba| gpu.create_texture(OUT_W, OUT_H, &rgba));
        let mut shelves = Vec::new();

        // First-run empty shelf: never synthesize fake demo cartridges.
        let grouped = library.map(carts_by_platform)
            .unwrap_or_else(|| std::array::from_fn(|_| Vec::new()));
        for mut carts in grouped {
            if let Some(root) = visuals_root.as_deref() {
                for cart in &mut carts {
                    let uri = cart.rom.to_string_lossy();
                    let filename = format!("{}.png", core_selection::key(&uri));
                    // Only app-private labels are used; never rewrite ROMs.
                    cart.label = Some(root.join("Labels").join(filename));
                }
            }
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
        let menu_hints = [("A", "Select"), ("B", "Back")].map(|(key, label)| {
            let face = hint_face(key, label);
            let tex = gpu.create_texture(face.w, face.h, &face.rgba);
            (tex, face.w)
        });
        // Original Slot keycap typography also describes the actual in-game
        // shortcuts, so users need not guess physical controller buttons.
        let game_menu_hints = [
            ("B", "Back"), ("SEL+R1", "Save"), ("SEL+L1", "Load"),
        ].map(|(key, label)| {
            let face = hint_face(key, label);
            let tex = gpu.create_texture(face.w, face.h, &face.rgba);
            (tex, face.w)
        });
        let setting_values = ["2×", "3×", "4×", "6×", "ON", "OFF",
                              "LCD3x", "Grid", "Dot", "Simpletex"]
            .into_iter().map(|value| {
                let face = quick_value_face(value, true);
                let tex = gpu.create_texture(face.w, face.h, &face.rgba);
                (tex, face.w, face.h)
            }).collect();
        let setting_carets = [false, true].map(|right| {
            let face = quick_caret_face(right);
            let tex = gpu.create_texture(face.w, face.h, &face.rgba);
            (tex, face.w, face.h)
        });
        let settings_hints = [
            hint_face("B", "Back"), arrows_hint_face("Change")
        ].map(|face| {
            let tex = gpu.create_texture(face.w, face.h, &face.rgba);
            (tex, face.w)
        });
        // Rasterize the detailed barcode About label only when opened.
        // Creating SVG / fonts during every cold boot needlessly delays shelf.
        let about_texture = None;
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
        // The actual Slot legend is a set of three compact keycaps, not
        // oversized menu labels. These are the original Slot UI generators.
        let core_legend_faces = [
            hint_face("B", "Cancel"),
            arrows_hint_face("Swap"),
            hint_face("A", "Choose"),
        ].map(|face| {
            let texture = gpu.create_texture(face.w, face.h, &face.rgba);
            (texture, face.w)
        });

        // Use the original Slot toast faces and HUD, not Android text banners.
        let mut hud = Hud::new();
        let toasts = Toast::all().into_iter().map(|toast| {
            let face = toast_face(toast);
            gpu.create_texture(face.w, face.h, &face.rgba)
        }).collect();
        hud.set_toasts(toasts);
        // The upstream glyph subset is already packaged. Enable its exact
        // fast-forward badge and rewind progress icon in the Android HUD.
        let icons = Icon::ALL.into_iter().map(|icon| {
            let face = icon_face(icon, slot_ui::HUD_ICON_PX, HUD_INK);
            gpu.create_texture(face.w, face.h, &face.rgba)
        }).collect();
        hud.set_icons(icons);
        let bolt = icon_face(Icon::Charging, BOLT_PX, HUD_INK);
        let bolt_texture = gpu.create_texture(bolt.w, bolt.h, &bolt.rgba);

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
            insert_started: None,
            screen_power: 0.0,
            exiting_screen: false,
            born: now,
            last: now,
            library_version,
            texture_cache: VecDeque::new(),
            game: None,
            pending_game: None,
            buttons: 0,
            ff_held: false,
            ff_latched: false,
            ff_last_release: None,
            rewind_held: false,
            rewind_accum: 0.0,
            awaiting_game: false,
            requested_uri: None,
            prepared_rom: None,
            game_accum: 0.0,
            overlay: ShelfOverlay::None,
            settings,
            video_mode: VideoMode::Actual,
            wallpaper_texture,
            setting_values,
            setting_carets,
            settings_hints,
            about_texture,
            menu_seen: 0,
            menu_opened: now,
            menu_cursor_y: 0.0,
            menu_textures,
            menu_hints,
            game_menu_hints,
            board_texture: None,
            chip_texture,
            picker: None,
            socket_textures,
            chip_textures,
            chip_blank,
            chip_shadow,
            core_legend_faces,
            a_down_at: None,
            fresh_launch: false,
            mode_down_at: None,
            eject_sound_armed: false,
            mode_last_tap: None,
            polaroids: None,
            photo_textures: Vec::new(),
            polaroid_title_texture: None,
            hud,
            clock_text: String::new(),
            clock_texture: None,
            clock_face: Printed::default(),
            battery_percent: None,
            battery_texture: None,
            battery_face: Printed::default(),
            battery: None,
            bolt_texture,
            letter_texture: None,
            letter_face: Printed::default(),
            letter_shown_at: None,
        })
    }

    fn refresh_system_status(&mut self) {
        let status = SYSTEM_STATUS.lock().unwrap_or_else(|p| p.into_inner()).clone();
        if status.clock != self.clock_text {
            self.clock_text = status.clock;
            if !self.clock_text.is_empty() {
                let face = word_face(&self.clock_text);
                let tex = match self.clock_texture {
                    Some(tex) => { self.gpu.update_texture(tex, face.w, face.h, &face.rgba); tex }
                    None => { let tex = self.gpu.create_texture(face.w, face.h, &face.rgba);
                        self.clock_texture = Some(tex); tex }
                };
                self.clock_face = Printed::new(tex, face.w);
            }
        }
        self.battery = status.battery;
        let percent = self.battery.map(|b| b.percent);
        if percent != self.battery_percent {
            self.battery_percent = percent;
            // Just as with original Slot, the About label reflects the live
            // battery without rebuilding it on every animation frame.
            if let Some(tex) = self.about_texture {
                let sticker = sticker_face_konkr(&StickerFields {
                    battery: percent, serial: "0000130", dirty_digit: '0',
                });
                self.gpu.update_texture(tex, sticker.w, sticker.h, &sticker.rgba);
            }
            if let Some(percent) = percent {
                let face = word_face(&format!("{percent}%"));
                let tex = match self.battery_texture {
                    Some(tex) => { self.gpu.update_texture(tex, face.w, face.h, &face.rgba); tex }
                    None => { let tex = self.gpu.create_texture(face.w, face.h, &face.rgba);
                        self.battery_texture = Some(tex); tex }
                };
                self.battery_face = Printed::new(tex, face.w);
            } else { self.battery_face = Printed::default(); }
        }
    }

    fn reset_time_controls(&mut self) {
        self.ff_held = false;
        self.ff_latched = false;
        self.ff_last_release = None;
        self.rewind_held = false;
        self.rewind_accum = 0.0;
        self.game_accum = 0.0;
        self.hud.set_ff(FfState::Off);
        self.hud.release_rewind();
        AUDIO_SPEED_PERMILLE.store(1000, Ordering::Release);
        RUMBLE_STRENGTH.store(0, Ordering::Release);
    }

    fn handle(&mut self, input: Input) {
        match input {
            Input::ReloadVisuals => {
                // Triggered from Android SAF, applied on the GL thread only.
                let root = PATHS.lock().unwrap_or_else(|e| e.into_inner())
                    .as_ref().map(|(root, _)| root.clone());
                if let Some(root) = root {
                    slot_ui::set_theme(slot_store::Theme::read(&root));
                    let wallpaper = crate::wallpaper::pick(&root, 0)
                        .as_deref().and_then(slot_ui::wallpaper_face);
                    self.wallpaper_texture = wallpaper.map(|rgba| {
                        if let Some(tex) = self.wallpaper_texture {
                            self.gpu.update_texture(tex, OUT_W, OUT_H, &rgba);
                            tex
                        } else {
                            self.gpu.create_texture(OUT_W, OUT_H, &rgba)
                        }
                    });
                }
            }
            Input::ReloadCartLabel { uri } => {
                for shelf in &self.shelves {
                    for (index, cart) in shelf.carts.iter().enumerate() {
                        if cart.rom.to_string_lossy() == uri {
                            if let Some(tex) = shelf.face(index) {
                                let face = cart_face_with_material(cart, None);
                                self.gpu.update_texture(tex, face.w, face.h, &face.rgba);
                            }
                        }
                    }
                }
            }
            Input::Reset => {
                self.reset_time_controls();
                self.buttons = 0;
                self.mode_down_at = None;
                self.mode_last_tap = None;
                self.polaroids = None;
                self.overlay = ShelfOverlay::None;
                self.picker = None;
                for shelf in &mut self.shelves {
                    shelf.release_hold();
                }
            }
            Input::Suspend => {
                if let Some(session) = self.game.as_mut() {
                    if session.save(true) {
                        if let Some(uri) = self.requested_uri.as_deref() {
                            let paths = PATHS.lock().unwrap_or_else(|e| e.into_inner()).clone();
                            let core = paths.as_ref().map(|(storage, _)| {
                                core_selection::selected(storage, uri).as_str()
                            }).unwrap_or("mgba");
                            let event = serde_json::json!({"uri": uri, "core": core}).to_string();
                            SAVE_SYNC.lock().unwrap_or_else(|e| e.into_inner()).push_back(event);
                        }
                    }
                }
                self.buttons = 0;
                AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
            }
            Input::Exit => {
                self.reset_time_controls();
                self.pending_game = None;
                // Preserve the final video texture for the original 160ms CRT
                // power-off before the mechanical 450ms cartridge ejection.
                if self.game.is_some() { self.exiting_screen = true; }
                // The reversed cart animation will emit the original eject sound.
                if self.progress > 0.0 || self.inserted { self.eject_sound_armed = true; }
                if let Some(mut session) = self.game.take() {
                    let saved = session.save(self.settings.eject_save);
                    if saved {
                        if let Some(uri) = self.requested_uri.as_deref() {
                            let paths = PATHS.lock().unwrap_or_else(|e| e.into_inner()).clone();
                            let core = paths.as_ref().map(|(storage, _)| {
                                core_selection::selected(storage, uri).as_str()
                            }).unwrap_or("mgba");
                            let event = serde_json::json!({"uri": uri, "core": core}).to_string();
                            SAVE_SYNC.lock().unwrap_or_else(|e| e.into_inner()).push_back(event);
                        }
                    }
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
                self.mode_last_tap = None;
                self.polaroids = None;
                self.requested_uri = None;
                self.prepared_rom = None;
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
                            self.mode_last_tap = None;
                            if self.game.is_some() { self.handle(Input::Exit); }
                        } else if self.game.is_some() {
                            let is_double=self.mode_last_tap.take()
                                .is_some_and(|t|t.elapsed().as_millis()<360);
                            self.buttons=0;
                            self.game_accum=0.0;
                            AUDIO.lock().unwrap_or_else(|e|e.into_inner()).clear();
                            self.reset_time_controls();
                            RUMBLE_STRENGTH.store(0, Ordering::Release);
                            if is_double {
                                self.open_polaroids();
                            } else {
                                self.mode_last_tap=Some(Instant::now());
                                self.overlay=match self.overlay {
                                    ShelfOverlay::GameMenu { .. }=>ShelfOverlay::None,
                                    ShelfOverlay::States=>ShelfOverlay::None,
                                    _=>ShelfOverlay::GameMenu {row:0},
                                };
                            }
                        }
                        // START remains the ONLY menu key on the carousel.
                    }
                    return;
                }
                if self.game.is_some() && matches!(self.overlay, ShelfOverlay::States) {
                    if pressed {
                        match code {
                            21 => {
                                if let Some(p) = self.polaroids.as_mut() { p.left(); }
                                self.update_polaroid_title();
                            }
                            22 => {
                                if let Some(p) = self.polaroids.as_mut() { p.right(); }
                                self.update_polaroid_title();
                            }
                            96 => {
                                if let (Some(game),Some(p))=(self.game.as_mut(),self.polaroids.as_ref()) {
                                    if let Some(entry)=p.selected() {
                                        if let Err(err)=game.load_manual(&entry.stamp) {
                                            set_message(format!("State load failed: {err}"));
                                        }
                                    }
                                }
                                self.overlay=ShelfOverlay::None;
                            }
                            100 => { // Y deletes one Slot history entry only.
                                if let (Some(game),Some(p))=(self.game.as_mut(),self.polaroids.as_mut()) {
                                    if let Some(entry)=p.selected() {
                                        if let Err(err)=game.delete_manual(&entry.stamp) {
                                            set_message(format!("State delete failed: {err}"));
                                        } else {
                                            p.remove_selected();
                                        }
                                    }
                                }
                                if self.polaroids.as_ref().is_some_and(|p|p.is_empty()) {
                                    self.overlay=ShelfOverlay::None;
                                } else {
                                    self.update_polaroid_title();
                                }
                            }
                            99 => { // X: original Slot's 30-second undo.
                                if let Some(game)=self.game.as_mut() {
                                    if let Err(err)=game.undo_manual() { set_message(err); }
                                }
                                self.overlay=ShelfOverlay::None;
                            }
                            97=>self.overlay=ShelfOverlay::None,
                            _=>{}
                        }
                    }
                    return;
                }
        if self.game.is_some() && matches!(self.overlay, ShelfOverlay::GameMenu { .. }) {
                    if pressed {
                        match self.overlay {
                            ShelfOverlay::GameMenu { row } => match code {
                                19 | 20 => {
                                    self.overlay = ShelfOverlay::GameMenu {
                                        row: move_menu_row(row, code, 3),
                                    };
                                }
                                96 => {
                                    match row {
                                        1=>self.handle(Input::Exit),
                                        2=>self.open_polaroids(),
                                        _=>self.overlay=ShelfOverlay::None,
                                    }
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
                    // Upstream Slot gamepad mapping: R2 fast forward (hold or
                    // double tap to latch), L2 rewind. Handle these BEFORE
                    // libretro, so trigger presses never reach a game.
                    if code == 104 {
                        if !self.settings.rewind { return; }
                        if pressed {
                            self.rewind_held = true;
                            self.rewind_accum = 0.0;
                            self.ff_held = false;
                            self.ff_latched = false;
                            self.ff_last_release = None;
                            self.hud.set_ff(FfState::Off);
                            AUDIO_SPEED_PERMILLE.store(1000, Ordering::Release);
                            AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
                        } else {
                            self.rewind_held = false;
                            self.rewind_accum = 0.0;
                            self.game_accum = 0.0;
                            self.hud.release_rewind();
                            AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
                        }
                        return;
                    }
                    if code == 105 {
                        if pressed {
                            if self.ff_latched {
                                self.ff_latched = false;
                                self.ff_held = false;
                                self.ff_last_release = None;
                            } else {
                                let double = self.ff_last_release.take().is_some_and(
                                    |at| at.elapsed().as_millis() <= FF_DOUBLE_TAP_MS);
                                self.ff_latched = double;
                                self.ff_held = true;
                            }
                            self.rewind_held = false;
                            self.hud.release_rewind();
                            AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
                        } else if self.ff_held {
                            self.ff_held = false;
                            self.ff_last_release = Some(Instant::now());
                        }
                        // Flush old transport-speed PCM at every FF edge.
                        AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
                        AUDIO_SPEED_PERMILLE.store(
                            if self.ff_latched || self.ff_held {
                                i32::from(self.settings.ff_speed) * 1000
                            } else { 1000 },
                            Ordering::Release);
                        self.hud.set_ff(match (self.ff_latched, self.ff_held) {
                            (true, _) => FfState::Latched,
                            (false, true) => FfState::Held,
                            (false, false) => FfState::Off,
                        });
                        return;
                    }
                    // Use actual mGBA palette presets, and original Slot
                    // HUD toasts. Restrict to monochrome GB games.
                    if pressed && self.active == 1 && self.settings.gb_palettes
                        && self.buttons & ButtonMask::SELECT != 0
                        && (code == 21 || code == 22) {
                        let index = if code == 22 {
                            (self.settings.gb_palette + 1) % 48
                        } else { (self.settings.gb_palette + 47) % 48 };
                        self.settings.gb_palette = index;
                        if let Some(game) = self.game.as_mut() {
                            game.set_gb_palette(self.settings.palette_name());
                        }
                        let palette = slot_store::GbPalette::all().nth(index as usize)
                            .unwrap_or(slot_store::GbPalette::DEFAULT);
                        self.hud.toast(Toast::Palette(palette),
                            self.born.elapsed().as_millis() as u64);
                        self.persist_settings();
                        return;
                    }
                    // Slot original: GB/GBC L1 = Stretch, R1 = Actual.
                    // On GBA the shoulder buttons remain owned by the core.
                    // SELECT+shoulder save/load is handled below and takes priority.
                    if self.active != 0 && self.buttons & ButtonMask::SELECT == 0
                        && (code == 102 || code == 103) {
                        if pressed {
                            self.set_video_mode(if code == 102 {
                                VideoMode::Stretch
                            } else { VideoMode::Actual });
                        }
                        return;
                    }
                    // Restore upstream SELECT+R1/L1 save/load shortcuts.
                    if pressed && self.buttons & ButtonMask::SELECT != 0 {
                        if code == 103 {
                            if let Some(game)=self.game.as_mut() {
                                match game.save_manual() {
                                    Ok(stamp) => {
                                        self.hud.toast(Toast::StateSaved,
                                            self.born.elapsed().as_millis() as u64);
                                        if let Some(uri) = self.requested_uri.as_deref() {
                                            let paths = PATHS.lock().unwrap_or_else(|e| e.into_inner()).clone();
                                            let core = paths.as_ref().map(|(storage, _)| {
                                                core_selection::selected(storage, uri).as_str()
                                            }).unwrap_or("mgba");
                                            let event = serde_json::json!({
                                                "uri": uri, "core": core, "manual_stamp": stamp
                                            }).to_string();
                                            SAVE_SYNC.lock().unwrap_or_else(|e| e.into_inner())
                                                .push_back(event);
                                        }
                                    }
                                    Err(err) => set_message(format!("Cannot export state: {err}")),
                                }
                            }
                            return;
                        }
                        if code == 102 {
                            if let Some(game)=self.game.as_mut() {
                                match game.load_latest() {
                                    Ok(()) => self.hud.toast(Toast::StateLoaded,
                                        self.born.elapsed().as_millis() as u64),
                                    Err(err) => set_message(format!("Cannot load state: {err}")),
                                }
                            }
                            return;
                        }
                    }
                    if !pressed && (code==102 || code==103)
                        && self.buttons & ButtonMask::SELECT != 0 {return;}
                    let mask = controls(code);
                    if pressed { self.buttons |= mask; } else { self.buttons &= !mask; }
                    return;
                }
                if self.overlay != ShelfOverlay::None {
                    if pressed {
                        match self.overlay {
                            ShelfOverlay::Menu { row } => match code {
                                19 | 20 => {
                                    self.overlay = ShelfOverlay::Menu { row: move_menu_row(row, code, 5) };
                                }
                                96 => {
                                    self.overlay = match row {
                                        0 => ShelfOverlay::Library { row: 0 },
                                        1 => ShelfOverlay::Scraping,
                                        2 => ShelfOverlay::Achievements,
                                        3 => ShelfOverlay::Settings { row: 0 },
                                        _ => ShelfOverlay::Personalization { row: 0 },
                                    };
                                }
                                97 | 108 => self.overlay = ShelfOverlay::None,
                                _ => {}
                            },
                            ShelfOverlay::Library { row } => match code {
                                19 | 20 => {
                                    let next = if code == 19 { (row + 4) % 5 } else { (row + 1) % 5 };
                                    self.overlay = ShelfOverlay::Library { row: next };
                                }
                                96 => match row {
                                    0..=4 => {
                                        // ROM, BIOS, SRAM root, State root, Refresh.
                                        let action = match row {
                                            0 => 1,
                                            1 => 4,
                                            2 => 5,
                                            3 => 6,
                                            _ => 2,
                                        };
                                        UI_ACTION.lock().unwrap_or_else(|e| e.into_inner())
                                            .push_back(action);
                                        self.overlay = ShelfOverlay::None;
                                    }
                                    _ => {}
                                },
                                97 => self.overlay = ShelfOverlay::Menu { row: 0 },
                                108 => self.overlay = ShelfOverlay::None,
                                _ => {}
                            },
                            ShelfOverlay::Personalization { row } => match code {
                                19 | 20 => self.overlay = ShelfOverlay::Personalization {
                                    row: move_menu_row(row, code, 6),
                                },
                                96 => {
                                    let action = if row < 4 { 7 + row as i32 }
                                                 else { 11 + (row - 4) as i32 };
                                    if action >= 11 {
                                        if let Some(cart) = self.shelves[self.active].carts
                                            .get(self.shelves[self.active].index) {
                                            *LABEL_PICK_REQUEST.lock()
                                                .unwrap_or_else(|e| e.into_inner()) =
                                                Some(cart.rom.to_string_lossy().into_owned());
                                        } else {
                                            set_message("Choose a cartridge first".into());
                                            return;
                                        }
                                    }
                                    UI_ACTION.lock().unwrap_or_else(|e| e.into_inner())
                                        .push_back(action);
                                }
                                97 => self.overlay = ShelfOverlay::Menu { row: 4 },
                                108 => self.overlay = ShelfOverlay::None,
                                _ => {}
                            },
                            ShelfOverlay::Settings { row } => match code {
                                19 | 20 => self.overlay = ShelfOverlay::Settings {
                                    row: move_menu_row(row, code, 3),
                                },
                                96 => match row {
                                    0 => self.overlay = ShelfOverlay::ScreenSettings { row: 0 },
                                    1 => self.overlay = ShelfOverlay::GameplaySettings { row: 0 },
                                    _ => {
                                        self.prepare_about();
                                        self.overlay = ShelfOverlay::About;
                                    }
                                },
                                97 => self.overlay = ShelfOverlay::Menu { row: 3 },
                                108 => self.overlay = ShelfOverlay::None,
                                _ => {}
                            },
                            ShelfOverlay::ScreenSettings { row } => match code {
                                19 | 20 => self.overlay = ShelfOverlay::ScreenSettings {
                                    row: move_menu_row(row, code, 4),
                                },
                                21 | 22 => self.change_setting(row, code == 22),
                                97 => self.overlay = ShelfOverlay::Settings { row: 0 },
                                108 => self.overlay = ShelfOverlay::None,
                                _ => {}
                            },
                            ShelfOverlay::GameplaySettings { row } => match code {
                                19 | 20 => self.overlay = ShelfOverlay::GameplaySettings {
                                    row: move_menu_row(row, code, 6),
                                },
                                21 | 22 => self.change_setting(row + 4, code == 22),
                                97 => self.overlay = ShelfOverlay::Settings { row: 1 },
                                108 => self.overlay = ShelfOverlay::None,
                                _ => {}
                            },
                            ShelfOverlay::About => match code {
                                97 | 96 => self.overlay = ShelfOverlay::Settings { row: 2 },
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
                                            let core = picker.seat();
                                            let uri = self.shelves[self.active].carts
                                                [self.shelves[self.active].index]
                                                .rom.to_string_lossy().into_owned();
                                            let paths = PATHS.lock()
                                                .unwrap_or_else(|e| e.into_inner()).clone();
                                            if let Some((storage, library)) = paths {
                                                let core_file = library.join(
                                                    format!("lib{}_libretro.so", core.as_str()));
                                                if !core_file.is_file() {
                                                    set_message(format!(
                                                        "{} core is not installed", core.text()));
                                                } else {
                                                    match core_selection::set(&storage, &uri, core) {
                                                        Ok(()) => {
                                                            picker.press(Press::Keep, now);
                                                        }
                                                        Err(err) => set_message(format!(
                                                            "Cannot save core selection: {err}")),
                                                    }
                                                }
                                            }
                                        }
                                        97 | 109 => { picker.press(Press::Back, now); }
                                        _ => {}
                                    }
                                }
                            },
                            ShelfOverlay::GameMenu { .. } | ShelfOverlay::States | ShelfOverlay::None => {}
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
                            let current_core = PATHS.lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .as_ref()
                                .map(|(storage, _)| core_selection::selected(
                                    storage, &selected.rom.to_string_lossy()))
                                .unwrap_or(Core::Mgba);
                            let mut picker = CorePicker::open(current_core, now);
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
                                        self.prepared_rom = None;
                            self.pending_game = None;
                            self.insert_started = Some(Instant::now());
                            self.a_down_at = self.insert_started;
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
                            self.pending_game = None;
                            self.requested_uri = None;
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
                let before = shelf.index;
                match (code, pressed) {
                    (21, true) if !self.inserted => shelf.hold_left(ms),
                    (22, true) if !self.inserted => shelf.hold_right(ms),
                    (21, false) => shelf.release_left(),
                    (22, false) => shelf.release_right(),
                    (19, true) if !self.inserted => shelf.jump_prev_letter(),
                    (20, true) if !self.inserted => shelf.jump_next_letter(),
                    _ => {}
                }
                if !self.inserted && pressed && (code == 19 || code == 20)
                    && shelf.index != before {
                    let letter = slot_store::initial(&shelf.carts[shelf.index].stem);
                    let face = word_face(&letter.to_string());
                    let tex = match self.letter_texture {
                        Some(tex) => { self.gpu.update_texture(tex, face.w, face.h, &face.rgba); tex }
                        None => { let tex = self.gpu.create_texture(face.w, face.h, &face.rgba);
                            self.letter_texture = Some(tex); tex }
                    };
                    self.letter_face = Printed::new(tex, face.w);
                    self.letter_shown_at = Some(Instant::now());
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
            let face = cart_face_with_material(&self.shelves[shelf_id].carts[index], None);
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

    fn prepare_game_core(&mut self, local: &str) {
        let paths = PATHS.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some((storage, library)) = paths {
            let uri = self.requested_uri.as_deref().unwrap_or_default();
            let is_gba = self.shelves[self.active].carts
                .get(self.shelves[self.active].index)
                .is_some_and(|c| c.platform == Platform::Gba);
            let core = if is_gba {
                core_selection::selected(&storage, uri)
            } else {
                Core::Mgba
            };
            let core_file = library.join(format!("lib{}_libretro.so", core.as_str()));
            let platform = self.shelves[self.active].carts
                .get(self.shelves[self.active].index)
                .map_or(Platform::Gba, |cart|cart.platform);
            let rom_stem = self.shelves[self.active].carts
                .get(self.shelves[self.active].index)
                .map_or("", |cart| cart.stem.as_str());
            let started = Instant::now();
            match GameSession::open(Path::new(local), &core_file, &storage,
                core, platform, self.settings.eject_save && !self.fresh_launch,
                rom_stem, self.settings.gb_palettes.then(|| self.settings.palette_name())) {
                Ok(session) => {
                    // Create libretro on the same GL thread, but defer audio
                    // and CRT presentation until Slot's seated hold completes.
                    crate::game::log_launch_timing(&format!(
                        "Core prepared: {}ms loading, {}ms since A",
                        started.elapsed().as_millis(),
                        self.insert_started.map_or(0, |at| at.elapsed().as_millis())
                    ));
                    self.pending_game = Some(session);
                }
                Err(error) => {
                    set_message(format!("{}: {error}", core.text()));
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

    fn draw_menu_hints(&self, x: f32, y: f32, out: &mut Vec<Draw>) {
        let mut x = x;
        for (tex, w) in self.menu_hints {
            out.push(Draw::Tex { x, y, w: w as f32, h: HINT_H as f32,
                tex, alpha: 1.0 });
            x += w as f32 + 24.0;
        }
    }

    fn prepare_about(&mut self) {
        if self.about_texture.is_none() {
            let sticker = sticker_face_konkr(&StickerFields {
                battery: self.battery_percent, serial: "0000130", dirty_digit: '0',
            });
            self.about_texture = Some(self.gpu.create_texture(
                sticker.w, sticker.h, &sticker.rgba,
            ));
        }
    }

    fn current_cart_platform(&self) -> Platform {
        self.shelves[self.active].carts
            .get(self.shelves[self.active].index)
            .map_or(Platform::Gba, |cart| cart.platform)
    }

    fn apply_video_geometry(&mut self) {
        let platform = self.current_cart_platform();
        let (w, h) = platform.picture();
        let x = (slot_gfx::SRC_W - w) as f32 / (2 * slot_gfx::SRC_W) as f32;
        let y = (slot_gfx::SRC_H - h) as f32 / (2 * slot_gfx::SRC_H) as f32;
        let x2 = 1.0 - x;
        let y2 = 1.0 - y;
        self.gpu.set_picture([x, y, x2, y2]);
        self.gpu.set_game_source_rect(video_mode::source_rect(platform, self.video_mode));
    }

    fn set_video_mode(&mut self, mode: VideoMode) {
        if self.video_mode == mode || self.current_cart_platform() == Platform::Gba {
            return;
        }
        self.video_mode = mode;
        self.apply_video_geometry();
        let name = self.shelves[self.active].carts
            .get(self.shelves[self.active].index)
            .map_or("", |cart| cart.stem.as_str());
        if let Some((root, _)) = PATHS.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            if let Err(err) = video_mode::write_video_mode(root, name, mode) {
                set_message(format!("Cannot save display mode: {err}"));
            }
        }
    }

    fn persist_settings(&self) {
        if let Some((root, _)) = PATHS.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            if let Err(error) = self.settings.save(root) {
                set_message(format!("Cannot save Settings: {error}"));
            }
        }
    }

    fn change_setting(&mut self, key: usize, right: bool) {
        if !self.settings.change(key, right) { return; }
        if key == 5 {
            FF_AUDIO_ENABLED.store(self.settings.ff_sound, Ordering::Release);
            AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
        }
        if key == 2 {
            self.gpu.set_colour_correction(self.settings.colour_correction);
        }
        self.persist_settings();
    }

    fn setting_value(&self, key: usize) -> usize {
        use crate::settings::Shader;
        let shader = |shader| match shader {
            Shader::Off => 5, Shader::Lcd3x => 6, Shader::Grid => 7,
            Shader::Dot => 8, Shader::Simpletex => 9,
        };
        match key {
            0 => shader(self.settings.shader_gba),
            1 => shader(self.settings.shader_gb),
            2 => if self.settings.colour_correction { 4 } else { 5 },
            3 => if self.settings.gb_palettes { 4 } else { 5 },
            4 => match self.settings.ff_speed { 2 => 0, 3 => 1, 4 => 2, _ => 3 },
            5 => if self.settings.ff_sound { 4 } else { 5 },
            6 => if self.settings.rewind { 4 } else { 5 },
            7 => if self.settings.turbo { 4 } else { 5 },
            8 => if self.settings.rumble { 4 } else { 5 },
            _ => if self.settings.eject_save { 4 } else { 5 },
        }
    }

    fn draw_setting_values(&self, row: usize, keys: &[usize], out: &mut Vec<Draw>) {
        let top = settings_row_top(keys.len());
        for (index, key) in keys.iter().copied().enumerate() {
            let (tex, width, height) = self.setting_values[self.setting_value(key)];
            let y = top + index as f32 * QUICK_PITCH + 8.0;
            let scale = 0.79;
            let w = width as f32 * scale;
            let right = OUT_W as f32 - 34.0;
            let chosen = row == index;
            let x = right - w - if chosen { 22.0 } else { 0.0 };
            out.push(Draw::Tex {
                x, y, w, h: height as f32 * scale,
                tex, alpha: if chosen { 1.0 } else { 0.62 },
            });
            if chosen {
                let (left, lw, lh) = self.setting_carets[0];
                let (right_tex, rw, rh) = self.setting_carets[1];
                out.push(Draw::Tex {
                    x: x - lw as f32 - 8.0, y, w: lw as f32,
                    h: lh as f32, tex: left, alpha: 1.0,
                });
                out.push(Draw::Tex {
                    x: right - rw as f32, y, w: rw as f32,
                    h: rh as f32, tex: right_tex, alpha: 1.0,
                });
            }
        }
        // The original Back / Change physical keycap legend.
        out.push(Draw::Rect {
            x: 0.0, y: 420.0, w: OUT_W as f32, h: 60.0,
            colour: opening(),
        });
        for (tex, w, x) in centred_hints(&self.settings_hints, LEGEND_GAP) {
            out.push(Draw::Tex {
                x, y: 427.0, w: w as f32, h: HINT_H as f32,
                tex, alpha: 1.0,
            });
        }
    }

    fn draw_original_style_menu(&self, heading: usize, items: &[usize],
                                selected: usize, out: &mut Vec<Draw>) {
        // The original Slot full-screen quick menu uses the housing palette,
        // full-width selected bar, 52px pitch, and centred physical keycaps.
        // Reuse those exact primitives for Android's library settings.
        out.push(Draw::Rect {
            x: 0.0, y: 0.0, w: OUT_W as f32, h: OUT_H as f32,
            colour: opening(),
        });
        let top = settings_row_top(items.len());
        let cursor = if self.menu_cursor_y.is_finite() { self.menu_cursor_y }
            else { top + selected as f32 * QUICK_PITCH };
        let intro = (self.menu_opened.elapsed().as_secs_f32() / 0.16).clamp(0.0, 1.0);
        let fade = slot_ui::ease(intro);
        out.push(Draw::Rect {
            x: 0.0, y: cursor + 4.0, w: OUT_W as f32,
            h: QUICK_PITCH - 8.0,
            colour: { let mut c = edge(); c[3] *= fade; c },
        });
        self.text_fit(heading, 32.0, 33.0, 650.0, 0.68 * fade, out);
        for (i, index) in items.iter().copied().enumerate() {
            let x = 37.0;
            let y = top + i as f32 * QUICK_PITCH + 8.0
                + (1.0 - fade) * 8.0;
            self.text_fit(index, x, y, 650.0, fade, out);
        }
        // All menus use the original Slot keycap generator. The paused game
        // screen additionally advertises its active SAVE / LOAD shortcuts.
        if heading == 19 {
            for (tex, w, x) in centred_hints(&self.game_menu_hints, 15.0) {
                out.push(Draw::Tex {
                    x, y: 427.0, w: w as f32, h: HINT_H as f32,
                    tex, alpha: fade,
                });
            }
        } else {
            for (tex, w, x) in centred_hints(&self.menu_hints, LEGEND_GAP) {
                out.push(Draw::Tex {
                    x, y: 427.0, w: w as f32, h: HINT_H as f32,
                    tex, alpha: fade,
                });
            }
        }
    }

    fn draw_overlay(&self, out: &mut Vec<Draw>) {
        match self.overlay {
            ShelfOverlay::None => {}
            ShelfOverlay::Menu { row } => {
                self.draw_original_style_menu(0, &[1, 2, 3, 25, 40], row, out);
            }
            ShelfOverlay::Library { row } => {
                self.draw_original_style_menu(5, &[6, 24, 7, 8, 9], row, out);
            }
            ShelfOverlay::Settings { row } => {
                self.draw_original_style_menu(26, &[32, 33, 31], row, out);
            }
            ShelfOverlay::Personalization { row } => {
                self.draw_original_style_menu(41, &[42, 43, 44, 45, 46, 47], row, out);
            }
            ShelfOverlay::ScreenSettings { row } => {
                self.draw_original_style_menu(32, &[34, 35, 29, 36], row, out);
                self.draw_setting_values(row, &[0, 1, 2, 3], out);
            }
            ShelfOverlay::GameplaySettings { row } => {
                self.draw_original_style_menu(33, &[27, 28, 37, 38, 30, 39], row, out);
                self.draw_setting_values(row, &[4, 5, 6, 7, 8, 9], out);
            }
            ShelfOverlay::About => {
                out.push(Draw::Rect {
                    x: 0.0, y: 0.0, w: OUT_W as f32, h: OUT_H as f32,
                    colour: [0.055, 0.055, 0.065, 1.0],
                });
                if let Some(tex) = self.about_texture {
                    out.push(Draw::Tex {
                        x: (OUT_W - STICKER_W) as f32 / 2.0,
                        y: (OUT_H - STICKER_H) as f32 / 2.0,
                        w: STICKER_W as f32, h: STICKER_H as f32,
                        tex, alpha: 1.0,
                    });
                }
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
                self.draw_menu_hints(141.0, 311.0, out);
            }
            ShelfOverlay::GameMenu { row } => {
                self.draw_original_style_menu(19, &[20, 21, 22], row, out);
            }
            ShelfOverlay::Core => {
                self.draw_original_core_picker(out);
            }
            ShelfOverlay::States => {
                // Rendered separately as full-screen original Slot Polaroids.
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

        // Restore the original Slot keycap legends. The original positions
        // anchor Cancel to the board's left edge, Swap in the centre and
        // Choose to the board's right edge. Shrink uniformly only if needed
        // to preserve >=12px spacing on small screens / font changes.
        let widths = self.core_legend_faces.map(|(_, w)| w.saturating_sub(HINT_EDGE));
        let positions = crate::core_legend::positions(widths);
        for ((tex, w), (x, scale)) in self.core_legend_faces.into_iter().zip(positions) {
            let height = HINT_H as f32 * scale;
            out.push(Draw::Tex {
                x: x.round(),
                y: 386.0 + (HINT_H as f32 - height) / 2.0,
                w: w as f32 * scale,
                h: height,
                tex,
                alpha: lift,
            });
        }
    }

    fn update_polaroid_title(&mut self) {
        let Some(p)=self.polaroids.as_mut() else {return;};
        let face=title_face(&p.title(&stamp_now()));
        let tex=match self.polaroid_title_texture {
            Some(tex)=>{
                self.gpu.update_texture(tex,face.w,face.h,&face.rgba);
                tex
            }
            None=>{
                let tex=self.gpu.create_texture(face.w,face.h,&face.rgba);
                self.polaroid_title_texture=Some(tex);
                tex
            }
        };
        p.set_title_face(Some(tex));
    }

    fn open_polaroids(&mut self) {
        let Some(game)=self.game.as_ref() else {return;};
        let entries=game.history();
        if entries.is_empty() {
            set_message("No saved states yet — SELECT + R1 to save".into());
            return;
        }
        let mut polaroids=Polaroids::new(entries);
        let mut faces=Vec::new();
        for (index,entry) in polaroids.entries.iter().enumerate() {
            let face=photo_face(entry);
            let tex=if let Some(tex)=self.photo_textures.get(index).copied() {
                self.gpu.update_texture(tex,face.w,face.h,&face.rgba);
                tex
            } else {
                let tex=self.gpu.create_texture(face.w,face.h,&face.rgba);
                self.photo_textures.push(tex);
                tex
            };
            faces.push(tex);
        }
        polaroids.set_faces(faces);
        let hints=[("B","Back"),("Y","Delete"),("A","Load"),("X","Undo")];
        let hint_faces=hints.map(|(key,label)| {
            let face=hint_face(key,label);
            self.gpu.create_texture(face.w,face.h,&face.rgba)
        });
        polaroids.set_hint_faces(hint_faces.to_vec());
        if game.undo_available() {polaroids.set_undo(Some("undo"));}
        self.polaroids=Some(polaroids);
        self.update_polaroid_title();
        self.overlay=ShelfOverlay::States;
    }

    fn draw(&mut self) {
        self.refresh_system_status();
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
        // Update full-screen menus and spring-smooth the selection bar.
        // Keep animation entirely in rendering: never delay controller input.
        let (kind, selected, count) = match self.overlay {
            ShelfOverlay::Menu { row } => (1u8, row, 5usize),
            ShelfOverlay::Library { row } => (2u8, row, 5usize),
            ShelfOverlay::GameMenu { row } => (3u8, row, 3usize),
            ShelfOverlay::Settings { row } => (4u8, row, 3usize),
            ShelfOverlay::ScreenSettings { row } => (5u8, row, 4usize),
            ShelfOverlay::GameplaySettings { row } => (6u8, row, 6usize),
            ShelfOverlay::Personalization { row } => (7u8, row, 6usize),
            _ => (0, 0, 0),
        };
        if kind != self.menu_seen {
            self.menu_seen = kind;
            self.menu_opened = now;
            if count > 0 {
                self.menu_cursor_y = settings_row_top(count)
                    + selected as f32 * QUICK_PITCH;
            }
        } else if count > 0 {
            let goal = settings_row_top(count) + selected as f32 * QUICK_PITCH;
            let response = 1.0 - (-dt * 24.0).exp();
            self.menu_cursor_y += (goal - self.menu_cursor_y) * response;
        }

        // The CRT powers off WHILE the cartridge ejects, not before.
        // Upstream Slot combines those transitions via SlotChrome::screen.
        // Rendering the stored game frame also avoids advancing a stopped core.
        if self.exiting_screen {
            self.screen_power = (self.screen_power - dt / SCREEN_POWER_OFF_S).max(0.0);
            self.progress = advance_cart(self.progress, false, dt);
            self.gpu.set_screen_power(self.screen_power);
            let mut commands = vec![Draw::Rect {
                x: 0.0, y: 0.0, w: OUT_W as f32, h: OUT_H as f32,
                colour: [0.085, 0.085, 0.093, 1.0],
            }];
            if let Some(shelf) = self.shelves.get(self.active) {
                if let Some(cart) = shelf.carts.get(shelf.index) {
                    let (rest, scale) = shelf.selected_at();
                    shelf.draw_row(Some(cart.stem.as_str()), 0.0, 0.0, 1.0, &mut commands);
                    SlotChrome {
                        cart, face: shelf.face(shelf.index), rest, scale,
                        seat: self.progress, alert: None, dim: 0.0,
                        screen: self.screen_power, game: true,
                    }.draw(&mut commands);
                } else {
                    commands.push(Draw::Game);
                }
            }
            self.gpu.fit(self.size);
            self.gpu.begin_frame();
            self.gpu.draw_list(&commands);
            self.gpu.end_frame(self.size);
            if self.screen_power <= 0.0 { self.exiting_screen = false; }
            return;
        }
        if self.game.is_some() {
            self.screen_power = (self.screen_power + dt / SCREEN_POWER_ON_S).min(1.0);
            self.gpu.set_screen_power(self.screen_power);
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
        // Mirror original Slot: the cartridge seats at 450ms and remains
        // seated until 730ms (or until the core is ready, if later).
        // Use the hold to initialize mGBA/gpSP and restore SRAM/auto-state
        // instead of running this work only AFTER the insertion has ended.
        if self.inserted && self.game.is_none() {
            // Read wall time, not accumulated clipped frame dt: a slow core
            // load must be hidden INSIDE the hold, not added after it.
            let inserted_for = self.insert_started.map_or(0.0, |t| t.elapsed().as_secs_f32());
            if inserted_for >= SEATED_AT && self.a_down_at.is_none()
                && self.pending_game.is_none()
            {
                if let Some(local) = self.prepared_rom.take() {
                    self.prepare_game_core(&local);
                }
            }
            // Re-read wall time after core.load() and save-state import.
            let inserted_for = self.insert_started.map_or(0.0, |t| t.elapsed().as_secs_f32());
            if inserted_for >= INSERT_S {
                if let Some(session) = self.pending_game.take() {
                    crate::game::log_launch_timing(&format!(
                        "Start CRT: {}ms since A (upstream target 730ms)",
                        self.insert_started.map_or(0, |at| at.elapsed().as_millis())
                    ));
                    self.buttons = 0;
                    SAMPLE_RATE.store(session.sample_rate, Ordering::Release);
                    self.game = Some(session);
                    self.game_accum = 0.0;
                    let shader = if self.active == 0 {
                        self.settings.shader_gba
                    } else { self.settings.shader_gb };
                    self.gpu.set_screen_effect(shader.effect());
                    // The exact Slot upstream geometry: GB/GBC 160x144
                    // is centred within the 240x160 libretro canvas.
                    let name = self.shelves[self.active].carts
                        .get(self.shelves[self.active].index)
                        .map_or("", |cart| cart.stem.as_str());
                    self.video_mode = PATHS.lock().unwrap_or_else(|e| e.into_inner())
                        .as_ref().map_or(VideoMode::Actual,
                            |(root, _)| video_mode::video_mode_for(root, name));
                    self.apply_video_geometry();
                    self.screen_power = 0.0;
                    self.exiting_screen = false;
                    self.gpu.set_screen_power(0.0);
                    PLAYING.store(true, Ordering::Release);
                }
            }
        }

        if self.game.is_some() && matches!(self.overlay, ShelfOverlay::States) {
            // Polaroids are a full-screen, paused overlay. Render once per
            // display frame (not from a key event) and never advance libretro
            // or enqueue audio while browsing saved states.
            self.game_accum = 0.0;
            let mut commands = Vec::new();
            if let Some(p) = self.polaroids.as_mut() {
                p.set_undo(self.game.as_ref()
                    .is_some_and(|game| game.undo_available())
                    .then_some("undo"));
                p.draw(None, Printed::default(), None, Printed::default(), &mut commands);
            }
            self.gpu.fit(self.size);
            self.gpu.begin_frame();
            self.gpu.draw_list(&commands);
            self.gpu.end_frame(self.size);
            return;
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
            let mut stepped = 0;
            if self.rewind_held {
                // Rewind is mutually exclusive with forward libretro frames.
                // Restore up to 3 historical snapshots per render frame and
                // never pass rewound audio into the normal AudioTrack.
                self.game_accum = 0.0;
                self.rewind_accum = (self.rewind_accum + dt).min(0.30);
                while self.rewind_accum >= REWIND_STEP_S && stepped < 3 {
                    self.rewind_accum -= REWIND_STEP_S;
                    if !session.rewind_step() {
                        self.rewind_accum = 0.0;
                        break;
                    }
                    stepped += 1;
                }
                AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
                self.hud.show(HudKind::Rewind, session.rewind_fill(), false,
                    self.born.elapsed().as_millis() as u64);
            } else {
                // Original Slot offers 2/3/4/6x, independent of display Hz.
                let fast = self.ff_held || self.ff_latched;
                let multiplier = if fast { f64::from(self.settings.ff_speed) } else { 1.0 };
                self.game_accum = (self.game_accum + f64::from(dt) * multiplier)
                    .min(if fast { 0.40 } else { 0.10 });
                let period = 1.0 / session.fps;
                let frame_limit = if fast { usize::from(self.settings.ff_speed) * 4 }
                    else { 4 };
                while self.game_accum >= period && stepped < frame_limit {
                    session.advance(self.buttons, self.settings.turbo, self.settings.rewind);
                    self.game_accum -= period;
                    stepped += 1;
                    let samples = session.take_audio();
                    if (!fast || (self.settings.ff_sound &&
                        FF_AUDIO_SUPPORTED.load(Ordering::Acquire)))
                        && !samples.is_empty() {
                        let mut audio = AUDIO.lock().unwrap_or_else(|e| e.into_inner());
                        let overflow = audio.len().saturating_add(samples.len()).saturating_sub(96_000);
                        for _ in 0..overflow.min(audio.len()) { audio.pop_front(); }
                        audio.extend(samples);
                    }
                }
            }
            if stepped > 0 {
                self.gpu.upload_game(session.frame());
                RUMBLE_STRENGTH.store(
                    if self.settings.rumble && !self.rewind_held {
                        i32::from(session.rumble_strength())
                    } else { 0 },
                    Ordering::Release,
                );
            }
            self.gpu.fit(self.size);
            self.gpu.begin_frame();
            let mut commands = vec![Draw::Game];
            self.hud.draw(self.born.elapsed().as_millis() as u64, &mut commands);
            self.gpu.draw_list(&commands);
            self.gpu.end_frame(self.size);
            return;
        }

        let ms = self.born.elapsed().as_millis() as u64;
        self.shelves[self.active].tick(ms);
        self.shelves[self.active].update(dt);
        self.prepare_visible();
        let shelf = &mut self.shelves[self.active];

        // Original Slot 450ms mechanical insertion + 280ms hold. The click
        // arrives during the last 240ms of the seating motion. Ejection
        // continues to overlap CRT shutdown.
        let previous_progress = self.progress;
        self.progress = advance_cart(self.progress, self.inserted, dt);
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
        // Exact upstream Slot wallpaper compositor, behind its cartridge shelf.
        slot_ui::draw_backdrop(self.wallpaper_texture, &mut commands);
        if self.progress == 0.0 {
            if let Some(picker) = self.picker {
                let t = slot_ui::ease(picker.openness(self.born.elapsed().as_millis() as u64));
                let dim = 1.0 + (0.614 - 1.0) * t;
                let selected = shelf.carts.get(shelf.index).map(|c| c.stem.as_str());
                shelf.draw_row(selected, 0.0, 0.26 * t, dim, &mut commands);
                draw_empty_slot(&mut commands);
            } else {
                shelf.draw(0.0, &mut commands);
                if let Some(since) = self.letter_shown_at {
                    let ms = since.elapsed().as_millis();
                    if ms < 1500 {
                        let alpha = ((1500 - ms) as f32 / 250.0).min(1.0);
                        draw_slot_name(self.letter_face, alpha, &mut commands);
                    }
                }
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
        if self.progress == 0.0 && matches!(self.overlay, ShelfOverlay::None) {
            draw_footer(self.battery, self.battery_face,
                Some(self.bolt_texture), self.clock_face, &mut commands);
        }
        self.gpu.fit(self.size);
        self.gpu.begin_frame();
        self.gpu.draw_list(&commands);
        self.gpu.end_frame(self.size);
    }
}

/// Physical D-pad UP and DOWN must move the selection in opposite
/// directions, including wraparound; never leak navigation to libretro.
fn move_menu_row(row: usize, key: i32, count: usize) -> usize {
    if count == 0 { return 0; }
    match key {
        19 => (row + count - 1) % count,
        20 => (row + 1) % count,
        _ => row % count,
    }
}

/// Match Slot's centred menu row layout on the 720x480 design canvas.
fn settings_row_top(rows: usize) -> f32 {
    ((OUT_H as f32 - QUICK_PITCH * rows as f32) / 2.0).round()
}

/// Same easing timeline for the shelf and CRT-composited exit frames.
/// Separating progress from rendering makes the animation durations testable.
fn advance_cart(progress: f32, inserted: bool, dt: f32) -> f32 {
    if inserted {
        (progress + dt / SEATED_AT).min(1.0)
    } else {
        (progress - dt / EJECT_S).max(0.0)
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
    fn in_game_shortcut_keycaps_fit_720x480_without_clipping() {
        let widths = [
            slot_ui::hint_width("B", "Back"),
            slot_ui::hint_width("SEL+R1", "Save"),
            slot_ui::hint_width("SEL+L1", "Load"),
        ];
        let visible = widths.iter()
            .map(|w| w.saturating_sub(HINT_EDGE) as f32)
            .sum::<f32>() + 2.0 * 15.0;
        assert!(visible < OUT_W as f32 - 2.0 * 30.0,
            "original keycap hints must stay visible on the KONKR 3:2 screen");
    }

    #[test]
    fn paused_game_menu_dpad_up_down_wrap_correctly() {
        assert_eq!(move_menu_row(0, 19, 3), 2);
        assert_eq!(move_menu_row(2, 20, 3), 0);
        assert_eq!(move_menu_row(1, 19, 3), 0);
        assert_eq!(move_menu_row(1, 20, 3), 2);
        assert_eq!(move_menu_row(1, 96, 3), 1);
    }

    #[test]
    fn original_settings_menu_rows_are_on_screen_and_outside_footer() {
        for rows in [3, 5] {
            let top = settings_row_top(rows);
            assert!(top > 80.0);
            assert!(top + rows as f32 * QUICK_PITCH < 427.0,
                "settings text must not overlap Slot keycap legend");
            assert_eq!(top + (rows - 1) as f32 * QUICK_PITCH,
                settings_row_top(rows) + (rows - 1) as f32 * QUICK_PITCH);
        }
    }

    #[test]
    fn cartridge_timelines_match_upstream_seating_hold_and_ejection() {
        let from_seated = advance_cart(1.0, false, SCREEN_POWER_OFF_S);
        // After the 160ms CRT shutdown, the cartridge is already ejecting;
        // it must NOT wait to begin a fresh 450ms mechanical animation.
        assert!(from_seated < 0.70 && from_seated > 0.60);
        assert!(advance_cart(from_seated, false, EJECT_S - SCREEN_POWER_OFF_S) < 0.0001);
        assert_eq!(advance_cart(0.0, true, SEATED_AT), 1.0);
        assert!((INSERT_S - SEATED_AT - 0.28).abs() < 0.001);
        assert!((SEATED_AT * INSERT_SOUND_PROGRESS + 0.24 - SEATED_AT).abs() < 0.001);
    }

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
        let started = Instant::now();
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
                crate::game::log_launch_timing(&format!(
                    "Cold renderer ready in {}ms", started.elapsed().as_millis()));
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
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativePollCartLabelUri(
    env: JNIEnv<'_>, _this: JObject<'_>,
) -> jstring {
    let uri = LABEL_PICK_REQUEST.lock().unwrap_or_else(|e| e.into_inner()).take();
    uri.and_then(|s| env.new_string(s).ok())
        .map_or(std::ptr::null_mut(), |s| s.into_raw())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativeReloadCartLabel(
    mut env: JNIEnv<'_>, _this: JObject<'_>, uri: JString<'_>,
) {
    if let Ok(uri) = env.get_string(&uri) {
        INPUT.lock().unwrap_or_else(|e| e.into_inner())
            .push_back(Input::ReloadCartLabel {
                uri: uri.to_string_lossy().into_owned()
            });
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativeReloadVisualAssets(
    _env: *mut c_void,
    _activity: *mut c_void,
) {
    INPUT.lock().unwrap_or_else(|e| e.into_inner()).push_back(Input::ReloadVisuals);
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
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativeCoreForUri(
    mut env: JNIEnv<'_>, _this: JObject<'_>, uri: JString<'_>,
) -> jni::sys::jstring {
    let name = env.get_string(&uri).ok()
        .map(|uri| {
            let paths=PATHS.lock().unwrap_or_else(|e|e.into_inner()).clone();
            paths.map(|(storage,_)| core_selection::selected(&storage, &uri.to_string_lossy()))
                .unwrap_or(Core::Mgba).as_str()
        }).unwrap_or("mgba");
    env.new_string(name).map_or(std::ptr::null_mut(), |s| s.into_raw())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativePollSaveFlush(
    env: JNIEnv<'_>, _this: JObject<'_>,
) -> jni::sys::jstring {
    let item=SAVE_SYNC.lock().unwrap_or_else(|e|e.into_inner()).pop_front();
    match item.and_then(|s|env.new_string(s).ok()) {
        Some(s)=>s.into_raw(),
        None=>std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativePollUiAction(
    _env: *mut c_void, _this: *mut c_void,
) -> jint {
    UI_ACTION.lock().unwrap_or_else(|e| e.into_inner()).pop_front().unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativePollRumbleStrength(
    _env: *mut c_void, _this: *mut c_void,
) -> jint {
    RUMBLE_STRENGTH.load(Ordering::Acquire)
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

/// Android provides the local clock and device battery to the original Slot footer.
#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativeSystemStatus(
    mut env: JNIEnv<'_>, _this: JObject<'_>, clock: JString<'_>,
    percent: jint, charging: jboolean,
) {
    let Ok(clock) = env.get_string(&clock) else { return; };
    let clock = clock.to_string_lossy().into_owned();
    if clock.len() > 16 || !clock.chars().all(|c| c.is_ascii_digit() || c == ':') { return; }
    let battery = if (0..=100).contains(&percent) {
        Some(Battery { percent: percent as u8,
            charge: if charging != 0 { Charge::Charging } else { Charge::Discharging } })
    } else { None };
    *SYSTEM_STATUS.lock().unwrap_or_else(|p| p.into_inner()) = SystemStatus { clock, battery };
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
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativeAudioSpeedPermille(
    _env: *mut c_void, _this: *mut c_void,
) -> jint { AUDIO_SPEED_PERMILLE.load(Ordering::Acquire) }

#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativeFastAudioSupported(
    _env: *mut c_void, _this: *mut c_void, supported: jboolean,
) {
    FF_AUDIO_SUPPORTED.store(supported != 0, Ordering::Release);
    if supported == 0 { AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear(); }
}

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
