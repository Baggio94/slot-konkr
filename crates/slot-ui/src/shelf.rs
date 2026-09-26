use slot_gfx::{Draw, TexId, OUT_H, OUT_W};
use slot_store::Cart;

use crate::cart::{cart_box, gb_shell_of, label_colour, label_text, CART_W};
use crate::hud::Millis;
use crate::silhouette::GbShell;
use crate::slot_chrome::draw_empty_slot;

/// Distance between cart centres. Puts the outer carts fully on screen with an edge margin
/// equal to the gap beside the centre cart.
const PITCH: f32 = 240.0;
const SIDE_SCALE: f32 = 0.78;
const SIDE_ALPHA: f32 = 0.55;
/// Where a cartridge of this height stands on the row: centred on the screen. Every shelf holds
/// one platform — the card is one folder per system — so a row never mixes heights, and what the
/// eye reads on the carousel is where the selected cartridge sits in the frame. A GBA cart has
/// always been centred; a Game Boy pak measured from a shared floor instead sat 59 px higher,
/// crowding the top of the screen and leaving a gap above the slot, which is what the user
/// objected to. The two now share a centre rather than a floor.
///
/// This is where the *full size* cartridge rests. A side cart is smaller, and it keeps its foot
/// on the line the selection's foot is on rather than shrinking about the middle, so the row
/// still reads as objects standing on a shelf: see `foot_y`.
pub fn rest_y(h: f32) -> f32 {
    (OUT_H as f32 - h) / 2.0
}

/// The line this platform's cartridges stand on, which is `rest_y` plus that cartridge's own
/// height. Asked with the cart's full height even for a shrunken neighbour: the foot stays put
/// as a cart shrinks away, which is what stops the row reading as carts floating.
pub fn foot_y(h: f32) -> f32 {
    rest_y(h) + h
}
/// Critically damped, so a flick lands on a cart instead of bouncing past and returning.
const OMEGA: f32 = 16.0;
/// How far the cart next to the selection is pushed aside as the chosen one goes in.
const PART: f32 = 130.0;

/// Slots considered either side of the selection. Two reach the edges of a 720 row, the
/// third covers the lag while the spring is still catching up with a flick.
const SLOTS: i32 = 3;

/// Before the first repeat. Long enough that one press cannot become two carts.
const REPEAT_DELAY_MS: Millis = 400;
/// Between repeats, shortening the longer a direction is held so a long hold crosses thirty
/// carts in about two seconds. The last entry is the floor. Each press restarts from the top,
/// so taps are never accelerated.
const REPEAT_MS: [Millis; 4] = [110, 85, 65, 50];

pub struct Shelf {
    pub carts: Vec<Cart>,
    pub index: usize,
    pub scroll: f32,
    faces: Vec<TexId>,
    /// The cart silhouette in black, drawn under a dimmed cart. One texture per *mould* rather
    /// than one for the row: a row can hold GBA carts or Game Boy paks, the two Game Pak shells
    /// differ at their top corners, and all three are different objects. Backing of the wrong
    /// outline either draws black over the wallpaper beside the cart or leaves part of the
    /// dimmed face with nothing behind it, and both of those are visible.
    shadow: Option<TexId>,
    gb_shadow: Option<TexId>,
    gbc_shadow: Option<TexId>,
    /// Which mould each cart in `carts` came out of, `None` for a GBA cart. Worked out once
    /// here because the answer is in the rom's header: asking it while drawing would open a
    /// file on every cart of every frame.
    shells: Vec<Option<GbShell>>,
    /// The presses added up, in the same continuous coordinate `scroll` lives in, so it counts
    /// laps rather than wrapping. This is what the spring aims at — see `scroll_target` — because
    /// it is the only thing that remembers which button was pressed once the row has wrapped.
    ride: f32,
    vel: f32,
    /// The held direction, when it next repeats, and how many repeats it has fired. Kept here
    /// rather than in the gesture layer so nothing in game auto fires.
    held: Option<(i32, Millis, usize)>,
}

impl Shelf {
    pub fn new(carts: Vec<Cart>) -> Self {
        Shelf {
            shells: carts.iter().map(gb_shell_of).collect(),
            carts,
            index: 0,
            scroll: 0.0,
            faces: Vec::new(),
            shadow: None,
            gb_shadow: None,
            gbc_shadow: None,
            ride: 0.0,
            vel: 0.0,
            held: None,
        }
    }

    /// Put the row on a cart without a ride. Assigning `index` alone leaves the spring aiming at
    /// the previous cart, so anything other than a press must move the shelf through this.
    pub fn select(&mut self, i: usize) {
        self.index = i;
        self.scroll = i as f32;
        self.ride = i as f32;
        self.vel = 0.0;
    }

    /// Face textures in `carts` order. The caller uploads them because only the compositor
    /// can mint a `TexId`.
    pub fn set_shadow(&mut self, face: TexId) {
        self.shadow = Some(face);
    }

    /// The Game Boy pak's outline in black, one per shell mould. A row whose carts are paks and
    /// whose only uploaded shadow is the GBA one draws no black at all rather than a tapered
    /// shape stretched under a straight sided cart.
    pub fn set_gb_shadow(&mut self, shell: GbShell, face: TexId) {
        match shell {
            GbShell::Notched => self.gb_shadow = Some(face),
            GbShell::Rounded => self.gbc_shadow = Some(face),
        }
    }

    pub fn set_faces(&mut self, faces: Vec<TexId>) {
        self.faces = faces;
    }

    /// In `hints` order.
    pub fn find(&self, stem: &str) -> Option<(&Cart, Option<TexId>)> {
        let i = self.carts.iter().position(|c| c.stem == stem)?;
        Some((&self.carts[i], self.faces.get(i).copied()))
    }

    pub fn left(&mut self) {
        self.step(-1);
    }

    pub fn right(&mut self) {
        self.step(1);
    }

    /// Up and Down: to the first cart of the next letter, or the previous one. The ring wraps
    /// in the pressed direction, never turning round. Up from mid-letter lands on that letter's
    /// start first.
    pub fn jump_next_letter(&mut self) {
        self.jump(1);
    }

    pub fn jump_prev_letter(&mut self) {
        self.jump(-1);
    }

    /// The first cart of the letter `from` is filed under.
    fn start_of_letter(&self, from: usize) -> usize {
        let n = self.carts.len();
        let letter = slot_store::initial(&self.carts[from].stem);
        let mut at = from;
        for _ in 0..n {
            let before = (at as i32 - 1).rem_euclid(n as i32) as usize;
            if slot_store::initial(&self.carts[before].stem) != letter {
                break;
            }
            at = before;
        }
        at
    }

    fn jump(&mut self, dir: i32) {
        let n = self.carts.len();
        if n < 2 {
            return;
        }
        let wrap = |i: i32| i.rem_euclid(n as i32) as usize;
        let here = slot_store::initial(&self.carts[self.index].stem);
        // One letter only: nowhere to go, and the walks below would find no other letter.
        if self
            .carts
            .iter()
            .all(|c| slot_store::initial(&c.stem) == here)
        {
            return;
        }
        let target = match dir > 0 {
            true => {
                let mut at = self.index;
                for _ in 0..n {
                    at = wrap(at as i32 + 1);
                    if slot_store::initial(&self.carts[at].stem) != here {
                        break;
                    }
                }
                at
            }
            false => {
                let start = self.start_of_letter(self.index);
                match start == self.index {
                    true => self.start_of_letter(wrap(start as i32 - 1)),
                    false => start,
                }
            }
        };
        // Signed the way the press asked, never the short way round.
        let ahead = (target as i32 - self.index as i32).rem_euclid(n as i32);
        let delta = match dir > 0 {
            true => ahead,
            false => ahead - n as i32,
        };
        self.index = target;
        self.ride += delta as f32;
    }

    pub fn hold_left(&mut self, now: Millis) {
        self.hold(-1, now);
    }

    pub fn hold_right(&mut self, now: Millis) {
        self.hold(1, now);
    }

    fn hold(&mut self, by: i32, now: Millis) {
        self.step(by);
        self.held = Some((by, now + REPEAT_DELAY_MS, 0));
    }

    pub fn release_left(&mut self) {
        self.release(-1);
    }

    pub fn release_right(&mut self) {
        self.release(1);
    }

    /// Only releasing the held direction stops it; the other was already a change of direction.
    fn release(&mut self, by: i32) {
        if matches!(self.held, Some((held, _, _)) if held == by) {
            self.held = None;
        }
    }

    pub fn release_hold(&mut self) {
        self.held = None;
    }

    /// Fires the repeat. Due from `now` rather than the passed deadline, so a late frame costs
    /// one cart instead of a burst of catching up.
    pub fn tick(&mut self, now: Millis) {
        let Some((by, due, fired)) = self.held else {
            return;
        };
        if now < due {
            return;
        }
        self.step(by);
        // `fired` saturates on the last entry, so a long hold settles at the floor.
        let rate = REPEAT_MS[fired.min(REPEAT_MS.len() - 1)];
        self.held = Some((by, now + rate, fired + 1));
    }

    /// A press; nothing on a row of fewer than two carts. A lone cart must return early: on a
    /// ring of one the target is `ride` itself, so bumping `ride` would throw the cart a pitch.
    fn step(&mut self, by: i32) {
        let n = self.carts.len();
        if n < 2 {
            return;
        }
        self.index = (self.index as i32 + by).rem_euclid(n as i32) as usize;
        self.ride += by as f32;
    }

    /// Where the spring is heading, in `scroll`'s coordinate: the selected cart's image nearest
    /// `ride`. Measuring from `scroll` instead reverses short rings while the spring lags behind
    /// a held press.
    pub fn scroll_target(&self) -> f32 {
        let n = self.carts.len();
        if n == 0 {
            return 0.0;
        }
        let from = self.ride;
        let n = n as f32;
        from + (self.index as f32 - from + n / 2.0).rem_euclid(n) - n / 2.0
    }

    /// The cart `off` slots right of the selection. Every slot is filled on any ring of two or
    /// more, so short rings repeat carts (the user asked for this on two) and a press slides
    /// rather than blinking a cart out; `draw_row` culls off-screen slots. One cart stays alone.
    pub fn cart_at_offset(&self, off: i32) -> Option<usize> {
        let n = self.carts.len() as i32;
        if n == 0 {
            return None;
        }
        if n == 1 {
            return (off == 0).then_some(self.index);
        }
        Some((self.index as i32 + off).rem_euclid(n) as usize)
    }

    /// Where the row is drawing its selected cart *this frame*: the left edge of its quad, and
    /// how big that quad is against the cartridge's own size. The cart going into the slot and
    /// the cart the core picker opens are both drawn by somebody else, taking over from this
    /// row mid-movement, so the row has to be able to say where it had the cart rather than
    /// have each of them assume.
    ///
    /// This frame and not once the spring has settled, which is the whole of what it is for. A
    /// settled row has its selection dead centre at full size and that is what this answers for
    /// it — but nothing makes the player wait for the spring before pressing A or START. Press
    /// either while the row is still travelling and the selection is somewhere between two
    /// slots, shrunken as much as `SIDE_SCALE`, with its foot on the row's floor; answering
    /// "dead centre, full size" then is a cartridge that jumps up to 266 px sideways and grows a
    /// third on the frame the button goes down. Composited and looked at, the chosen cart
    /// teleports into the middle of the screen while the cart it was passing is still sliding.
    ///
    /// The sum is `draw_row`'s own for the slot the selection is in, with `recede` at zero
    /// because nothing has begun to part yet — not a second copy of it. The width is asked of
    /// the selected cart rather than assumed to be `CART_W`: both cartridges are 240 wide today
    /// and the number is the same either way, and the point is that it stays the same as what is
    /// drawn if a later one is not. `CART_W` stands in for an empty row, which draws no cart to
    /// measure.
    pub fn selected_at(&self) -> (f32, f32) {
        let w = self
            .carts
            .get(self.index)
            .map_or(CART_W, |c| cart_box(c.platform).0) as f32;
        let offset = self.scroll_target() - self.scroll;
        let scale = shrink(offset);
        (OUT_W as f32 / 2.0 + offset * PITCH - w * scale / 2.0, scale)
    }

    pub fn update(&mut self, dt: f32) {
        let accel = -2.0 * OMEGA * self.vel - OMEGA * OMEGA * (self.scroll - self.scroll_target());
        self.vel += accel * dt;
        self.scroll += self.vel * dt;
    }

    /// The row of carts and the slot under it. The case text is drawn after, by the caller.
    pub fn draw(&self, shake: f32, out: &mut Vec<Draw>) {
        self.draw_row(None, shake, 0.0, 1.0, out);
        draw_empty_slot(out);
    }

    /// The row alone, minus `hidden` (the cart the chrome is drawing into the slot). `shake`
    /// moves only the carts, since shaking the backdrop shows the letterbox. `recede` 0..1 parts
    /// the other carts outwards. `dim` darkens faces but not the black shadow beneath them.
    pub fn draw_row(
        &self,
        hidden: Option<&str>,
        shake: f32,
        recede: f32,
        dim: f32,
        out: &mut Vec<Draw>,
    ) {
        let recede = recede.clamp(0.0, 1.0);
        let dim = dim.clamp(0.0, 1.0);
        let target = self.scroll_target();
        for slot in -SLOTS..=SLOTS {
            let Some(i) = self.cart_at_offset(slot) else {
                continue;
            };
            let cart = &self.carts[i];
            if hidden == Some(cart.stem.as_str()) {
                continue;
            }
            let offset = target + slot as f32 - self.scroll;
            let t = offset.abs().min(1.0);
            let scale = shrink(offset);
            let alpha = (1.0 + (SIDE_ALPHA - 1.0) * t) * (1.0 - recede);
            let (cw, ch) = cart_box(cart.platform);
            let (w, h) = (cw as f32 * scale, ch as f32 * scale);
            // Away from the middle, and further the further out it already was, so the row
            // opens rather than sliding sideways.
            let away = offset.signum() * (1.0 + offset.abs());
            let x = OUT_W as f32 / 2.0 + offset * PITCH - w / 2.0 + away * PART * recede;
            if x + w <= 0.0 || x >= OUT_W as f32 || alpha <= 0.0 {
                continue;
            }
            let x = x + shake;
            // The floor is this platform's, asked of the cartridge's own full height rather than
            // of the scaled one: a neighbour shrinks upward off a floor it shares with the
            // selection instead of shrinking about its own middle.
            let y = foot_y(ch as f32) - h;
            // Black in the cart's own shape, under the dimmed face. Without it the dimming is
            // transparency, and over a wallpaper the row reads as ghosts of carts.
            if alpha < 1.0 {
                // Three backings for three moulds: it is the cart's own outline, and a class C
                // pak's corners are not a class A/B pak's. See `cart::gb_cart_shadow`.
                //
                // Asked for rather than indexed, the way `faces` is asked for eighteen lines
                // below and for the same reason: `carts` is public, `shells` is not, and a push
                // through the public field would leave this one entry short. Indexed, that is a
                // panic inside the draw loop — on the device a black screen and a dead handset,
                // with no message anywhere — for a row that would otherwise have drawn. A cart
                // whose mould was never recorded gets the straight sided backing, which is the
                // same degrading a cart whose face was never uploaded already gets.
                let backing = match self.shells.get(i).copied().flatten() {
                    None => self.shadow,
                    Some(GbShell::Notched) => self.gb_shadow,
                    Some(GbShell::Rounded) => self.gbc_shadow,
                };
                if let Some(tex) = backing {
                    out.push(Draw::Tex {
                        x,
                        y,
                        w,
                        h,
                        tex,
                        alpha: recede_alpha(alpha),
                    });
                }
            }
            out.push(match self.faces.get(i) {
                Some(tex) => Draw::Tex {
                    x,
                    y,
                    w,
                    h,
                    tex: *tex,
                    alpha: alpha * dim,
                },
                // A face not yet uploaded still holds its place; a gap reads as a missing game.
                None => {
                    let c = label_colour(&label_text(cart));
                    Draw::Rect {
                        x,
                        y,
                        w,
                        h,
                        colour: [
                            c[0] as f32 / 255.0,
                            c[1] as f32 / 255.0,
                            c[2] as f32 / 255.0,
                            alpha * dim,
                        ],
                    }
                }
            });
        }
    }
}

/// Scale of a cart `offset` pitches from the selection. Shared so `draw_row` and `selected_at`
/// always agree.
fn shrink(offset: f32) -> f32 {
    1.0 + (SIDE_SCALE - 1.0) * offset.abs().min(1.0)
}

/// Opacity of the shadow under a dimmed cart: solid while the face is dimmed, fading as the
/// row parts.
fn recede_alpha(face_alpha: f32) -> f32 {
    (face_alpha / SIDE_ALPHA).clamp(0.0, 1.0)
}
