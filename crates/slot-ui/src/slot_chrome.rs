use std::sync::OnceLock;

use slot_gfx::{Draw, TexId, OUT_H, OUT_W};
use slot_store::Cart;
use slot_store::Theme;

use crate::cart::{cart_box, label_colour, label_text, CART_W};
use crate::footer::Printed;
use crate::icon::icon_box;
use crate::plate::HINT_H;
use crate::shelf::{foot_y, rest_y};

/// Big enough to read as a symbol on a 240 px cart rather than a mark on its label.
pub const ALERT_PX: f32 = 44.0;

/// The bottom of the device. A 40px band under a 135px cart read as a detail, not a cart bay.
pub const MOUTH_H: f32 = 58.0;
const BAND_Y: f32 = OUT_H as f32 - MOUTH_H;

/// The opening: the one piece of the slot drawn *behind* the cart. Everything else is plastic
/// and draws in front, which is what cuts the cart off.
pub const MOUTH_W: f32 = CART_W as f32 + 14.0;
const SLIT_H: f32 = 9.0;
const MOUTH_X: f32 = (OUT_W as f32 - MOUTH_W) / 2.0;
const SLIT_Y: f32 = BAY_Y + 5.0;

/// The lit front edge of the slot: the line the cart is cut off at.
pub const LIP_H: f32 = 2.0;

/// The bay the slot sits in, stepped down from the outer shell and wider than the opening.
const BAY_W: f32 = MOUTH_W + 18.0;
const BAY_X: f32 = (OUT_W as f32 - BAY_W) / 2.0;
const BAY_Y: f32 = BAND_Y + LIP_H;
/// The thumb scoop: one broad arc across the middle of the bay's near wall, as on an SP.
const SCOOP_W: f32 = MOUTH_W * 0.88;
const SCOOP_D: f32 = RECESS_H - (SCOOP_Y - BAY_Y);
const SCOOP_Y: f32 = SLIT_Y + SLIT_H;

/// How deep you can see into the slot. Just past the cart's label inset, so a seated cart shows
/// its grip and the label's top edge through the scoop, and nothing readable.
const RECESS_H: f32 = 42.0;
/// The lit edge of the plastic where it is cut away for the scoop.
const RIM_W: f32 = 2.0;
const CX: f32 = OUT_W as f32 / 2.0;
/// Flatness of the arc's trough. An ellipse would bottom out in a curve; the real scoop runs
/// almost level and turns up hard at the ends.
const SCOOP_FLAT: f32 = 4.0;

/// The palette, from `System/theme.txt` if the card carries one. Set once at boot; it cannot
/// change while the device is on.
static THEME: OnceLock<Theme> = OnceLock::new();

/// Once, at boot. A second call is ignored.
pub fn set_theme(theme: Theme) {
    let _ = THEME.set(theme);
}

pub fn theme() -> &'static Theme {
    THEME.get_or_init(Theme::default)
}

fn rgb(c: [u8; 3]) -> [f32; 4] {
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
        1.0,
    ]
}

/// The case. Every band has to clear its neighbour, which `slot-ui/tests/contrast.rs` holds
/// for the default palette.
pub fn housing() -> [f32; 4] {
    rgb(theme().housing)
}

pub fn opening() -> [f32; 4] {
    rgb(theme().opening)
}

pub fn edge() -> [f32; 4] {
    rgb(theme().edge)
}

/// The floor of the bay: a step down from the shell, not a second opening.
pub fn recess() -> [f32; 4] {
    rgb(theme().recess)
}

const LIP_Y: f32 = BAND_Y;

/// Where the cart's top edge stops: four pixels into the recess, so the far wall shows above
/// it. Must not depend on the cartridge, or a taller pak would sink further under the lip.
const SEATED_Y: f32 = BAY_Y + 4.0;

/// The cart arrives centred on the mouth, which cannot move.
fn seated_x(w: f32) -> f32 {
    (OUT_W as f32 - w) / 2.0
}

/// The fraction of the travel at which the cart's foot meets the lip. Derived, not tuned: the
/// catch is a collision, so it must happen exactly there for either cartridge.
fn catch_at(h: f32) -> f32 {
    (LIP_Y - foot_y(h)) / (SEATED_Y - rest_y(h))
}

/// The seat either side of the catch. Fractions of the animation, so both cartridges catch on
/// the same frame and only their speeds differ.
const CATCH_IN: f32 = 0.42;
const CATCH_OUT: f32 = 0.62;
/// How far the cart creeps while caught. A dead stop reads as a dropped frame.
const CREEP: f32 = 0.03;

pub struct SlotChrome<'a> {
    pub cart: &'a Cart,
    pub face: Option<TexId>,
    /// The left edge of the cart on the frame the button went down, from `Shelf::selected_at`,
    /// so a cart still sliding to the middle does not jump to the mouth.
    pub rest: f32,
    /// The row's scale for the cart on that frame, from `Shelf::selected_at`. The cart grows to
    /// full size over the travel instead of jumping larger on the first frame.
    pub scale: f32,
    /// 0.0 standing where the shelf left it, 1.0 swallowed by the mouth.
    pub seat: f32,
    /// The refusal symbol and its fade. `None` when there is nothing to say or before the glyph
    /// is uploaded.
    pub alert: Option<(TexId, f32)>,
    /// Alpha of the black veil over the layer behind: the shelf on the way in, the live
    /// game on the way out.
    pub dim: f32,
    /// How far up the screen behind the slot is, 0.0 dark and 1.0 fully on. The housing fades
    /// out as it lights, so the picture never keeps a black bar across its bottom.
    pub screen: f32,
    /// Whether there is a picture to show. False while the core loads, when the game texture
    /// still holds the last cart's frame.
    pub game: bool,
}

impl SlotChrome<'_> {
    pub fn draw(&self, out: &mut Vec<Draw>) {
        let seat = self.seat.clamp(0.0, 1.0);
        let dim = self.dim.clamp(0.0, 1.0);
        if dim > 0.0 {
            out.push(Draw::Rect {
                x: 0.0,
                y: 0.0,
                w: OUT_W as f32,
                h: OUT_H as f32,
                colour: [0.0, 0.0, 0.0, dim],
            });
        }

        let chrome = 1.0 - self.screen.clamp(0.0, 1.0);

        // Behind the cart: the opening, so the cart fills it on the way through.
        draw_slot_back(chrome, out);

        // The cartridge's own box: a pak in a GBA cart's quad is squashed to 53% of its height.
        let (cw, ch) = cart_box(self.cart.platform);
        let (cw, ch) = (cw as f32, ch as f32);

        // Across, down and up to size on one progress, starting from where the row drew the
        // cart so nothing jumps on the press. `stands` is measured back from the foot, which
        // keeps `catch_at` exact from any starting scale.
        let scale = self.scale.clamp(0.0, 1.0);
        let (w0, h0) = (cw * scale, ch * scale);
        let stands = foot_y(ch) - h0;
        let travel = travel(seat, ch);
        let x = self.rest + (seated_x(cw) - self.rest) * travel;
        let y = stands + (SEATED_Y - stands) * travel;
        let (cw, ch) = (w0 + (cw - w0) * travel, h0 + (ch - h0) * travel);
        // A seated cart fades with the case, as one object.
        let cart_alpha = if seat >= 1.0 { chrome } else { 1.0 };
        out.push(match self.face {
            Some(tex) => Draw::Tex {
                x,
                y,
                w: cw,
                h: ch,
                tex,
                alpha: cart_alpha,
            },
            None => {
                let c = label_colour(&label_text(self.cart));
                Draw::Rect {
                    x,
                    y,
                    w: cw,
                    h: ch,
                    colour: [
                        c[0] as f32 / 255.0,
                        c[1] as f32 / 255.0,
                        c[2] as f32 / 255.0,
                        cart_alpha,
                    ],
                }
            }
        });

        // On the cart, so the alert goes behind the mouth with it.
        if let Some((tex, alpha)) = self.alert {
            let (w, h) = icon_box(ALERT_PX);
            let (w, h) = (w as f32, h as f32);
            out.push(Draw::Tex {
                x: x + (cw - w) / 2.0,
                y: y + (ch - h) / 2.0,
                w,
                h,
                tex,
                alpha,
            });
        }

        // After the cart and before the housing: the panel is the device's front surface.
        if self.game && self.screen > 0.0 {
            out.push(Draw::Game);
        }

        // In front of the cart: the plastic, which is what occludes it.
        draw_slot_front(chrome, out);
    }
}

fn band(x: f32, y: f32, w: f32, h: f32, c: [f32; 4], alpha: f32) -> Draw {
    Draw::Rect {
        x,
        y,
        w,
        h,
        colour: [c[0], c[1], c[2], c[3] * alpha],
    }
}

/// Everything you can see *into*: bay floor, opening and thumb scoop. All of it is a hole, so
/// it draws behind the cart. A scoop painted in front lies on top of the cart as a dark arc.
fn draw_slot_back(alpha: f32, out: &mut Vec<Draw>) {
    // The top bar goes back here too: in front, two pixels would rule a line across the label.
    out.push(band(BAY_X, BAND_Y, BAY_W, LIP_H, housing(), alpha));
    out.push(band(MOUTH_X, BAND_Y, MOUTH_W, LIP_H, edge(), alpha));
    out.push(band(BAY_X, BAY_Y, BAY_W, RECESS_H, recess(), alpha));
    out.push(band(MOUTH_X, SLIT_Y, MOUTH_W, SLIT_H, opening(), alpha));
    for_each_scoop_span(|x, w, depth| {
        out.push(band(x, SCOOP_Y, w, depth, opening(), alpha));
    });
}

/// The plastic, in pieces around the hole and never over it: the only thing that occludes the
/// cart.
fn draw_slot_front(alpha: f32, out: &mut Vec<Draw>) {
    let w = OUT_W as f32;
    let right = BAY_X + BAY_W;
    let floor = SCOOP_Y + SCOOP_D + RIM_W;
    out.push(band(0.0, BAND_Y, BAY_X, MOUTH_H, housing(), alpha));
    out.push(band(right, BAND_Y, w - right, MOUTH_H, housing(), alpha));

    // Beside the arc, where the bay is wider than the scoop.
    let near = CX - SCOOP_W / 2.0;
    out.push(band(
        BAY_X,
        SCOOP_Y,
        near - BAY_X,
        floor - SCOOP_Y,
        housing(),
        alpha,
    ));
    let far = CX + SCOOP_W / 2.0;
    out.push(band(
        far,
        SCOOP_Y,
        right - far,
        floor - SCOOP_Y,
        housing(),
        alpha,
    ));

    // The plastic under the cut, then its lit edge last so nothing is painted over it.
    for_each_scoop_span(|x, w, depth| {
        let top = SCOOP_Y + depth + RIM_W;
        out.push(band(x, top, w, floor - top, housing(), alpha));
    });
    out.push(band(0.0, floor, w, OUT_H as f32 - floor, housing(), alpha));
    for_each_scoop_span(|x, w, depth| {
        out.push(band(x, SCOOP_Y + depth, w, RIM_W, edge(), alpha));
    });
}

/// The arc, walked by column and merged into spans of equal depth, so the hole and the plastic
/// beside it cannot drift apart. Walking by depth makes the flat trough one wide jagged step.
fn for_each_scoop_span(mut span: impl FnMut(f32, f32, f32)) {
    let hw = SCOOP_W / 2.0;
    let depth = |x: f32| SCOOP_D * (1.0 - (x.abs() / hw).powf(SCOOP_FLAT)).max(0.0);
    let mut x = -hw;
    while x < hw {
        let d = depth(x + 0.5).round();
        let start = x;
        while x < hw && depth(x + 0.5).round() == d {
            x += 1.0;
        }
        span(CX + start, x - start, d);
    }
}

/// A line of type printed faintly in the empty slot's opening. Nothing is drawn without a face:
/// a placeholder would read as something stuck in the slot.
pub fn draw_slot_name(name: Printed, alpha: f32, out: &mut Vec<Draw>) {
    let (Some(tex), true) = (name.face, alpha > 0.0) else {
        return;
    };
    let (w, h) = (name.w as f32, HINT_H as f32);
    let hole = SCOOP_Y + SCOOP_D - SLIT_Y;
    out.push(Draw::Tex {
        x: CX - w / 2.0,
        y: SLIT_Y + (hole - h) / 2.0,
        w,
        h,
        tex,
        alpha,
    });
}

/// The slot with nothing going into it, so the bottom of the screen is the same object on
/// every screen.
pub fn draw_empty_slot(out: &mut Vec<Draw>) {
    // Both halves: the front pieces alone leave a hole onto the backdrop.
    draw_slot_back(1.0, out);
    draw_slot_front(1.0, out);
}

/// The travel in three parts: fall to the lip, rest on it, then push through and settle. A
/// single ease never meets anything and reads as a chute.
fn travel(seat: f32, h: f32) -> f32 {
    let catch = catch_at(h);
    if seat < CATCH_IN {
        catch * ease(seat / CATCH_IN)
    } else if seat < CATCH_OUT {
        catch + CREEP * (seat - CATCH_IN) / (CATCH_OUT - CATCH_IN)
    } else {
        let caught = catch + CREEP;
        caught + (1.0 - caught) * ease((seat - CATCH_OUT) / (1.0 - CATCH_OUT))
    }
}

/// Smootherstep: zero velocity at both ends, so the travel meets the catch without a step.
pub fn ease(u: f32) -> f32 {
    u * u * u * (u * (u * 6.0 - 15.0) + 10.0)
}
