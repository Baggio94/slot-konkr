use std::sync::OnceLock;

use crate::cart::{CART_H, CART_W, GB_CART_H, GB_CART_W};

const CART_SVG: &str = include_str!("../assets/cart.svg");
const DETAIL_SVG: &str = include_str!("../assets/cart_detail.svg");
const GB_CART_SVG: &str = include_str!("../assets/gb_cart.svg");
const GBC_CART_SVG: &str = include_str!("../assets/gbc_cart.svg");
const GB_DETAIL_SVG: &str = include_str!("../assets/gb_cart_detail.svg");
const GBC_DETAIL_SVG: &str = include_str!("../assets/gbc_cart_detail.svg");

/// The Game Pak shell mould. A grey 0x00 pak and a black 0x80 pak share one mould; a clear
/// 0xc0 pak has its own.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum GbShell {
    /// Classes A and B: the power-switch notch cut out of the top right corner.
    Notched,
    /// Class C: no notch, and the top corners rounded rather than stepped.
    Rounded,
}

/// Coverage of the cart outline, one byte per pixel, row major.
pub fn silhouette(w: u32, h: u32) -> Vec<u8> {
    rasterise(w, h).unwrap_or_else(|| vec![255; (w * h) as usize])
}

/// The same, for the Game Boy Game Pak. Its own outline, because the pak's sides are parallel
/// where the GBA cart's taper into a grip ridge.
pub fn gb_silhouette(shell: GbShell, w: u32, h: u32) -> Vec<u8> {
    let svg = match shell {
        GbShell::Notched => GB_CART_SVG,
        GbShell::Rounded => GBC_CART_SVG,
    };
    rasterise_svg(svg, w, h).unwrap_or_else(|| vec![255; (w * h) as usize])
}

/// Every cart is the same shape, so the mask is rasterised once and multiplied into faces.
pub(crate) fn cart_mask() -> &'static [u8] {
    static MASK: OnceLock<Vec<u8>> = OnceLock::new();
    MASK.get_or_init(|| silhouette(CART_W, CART_H))
}

/// One cached mask per shell mould.
pub(crate) fn gb_cart_mask(shell: GbShell) -> &'static [u8] {
    static NOTCHED: OnceLock<Vec<u8>> = OnceLock::new();
    static ROUNDED: OnceLock<Vec<u8>> = OnceLock::new();
    let lock = match shell {
        GbShell::Notched => &NOTCHED,
        GbShell::Rounded => &ROUNDED,
    };
    lock.get_or_init(|| gb_silhouette(shell, GB_CART_W, GB_CART_H))
}

/// How far inside the outline each pixel sits, in city block steps, saturating at 255. A
/// translucent shell fades from its edge inward.
pub(crate) fn cart_depth() -> &'static [u8] {
    static DEPTH: OnceLock<Vec<u8>> = OnceLock::new();
    DEPTH.get_or_init(|| depth_map(cart_mask(), CART_W as usize, CART_H as usize))
}

pub(crate) fn gb_cart_depth(shell: GbShell) -> &'static [u8] {
    static NOTCHED: OnceLock<Vec<u8>> = OnceLock::new();
    static ROUNDED: OnceLock<Vec<u8>> = OnceLock::new();
    let lock = match shell {
        GbShell::Notched => &NOTCHED,
        GbShell::Rounded => &ROUNDED,
    };
    lock.get_or_init(|| depth_map(gb_cart_mask(shell), GB_CART_W as usize, GB_CART_H as usize))
}

/// Two pass chamfer. Everything off the edge of the buffer counts as outside, so a pixel on
/// the top row is one step in rather than unreachable.
fn depth_map(mask: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut d: Vec<u8> = mask
        .iter()
        .map(|c| if *c > 127 { 255 } else { 0 })
        .collect();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if d[i] == 0 {
                continue;
            }
            let up = if y == 0 { 0 } else { d[i - w] };
            let left = if x == 0 { 0 } else { d[i - 1] };
            d[i] = d[i].min(up.saturating_add(1)).min(left.saturating_add(1));
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let i = y * w + x;
            if d[i] == 0 {
                continue;
            }
            let down = if y + 1 == h { 0 } else { d[i + w] };
            let right = if x + 1 == w { 0 } else { d[i + 1] };
            d[i] = d[i]
                .min(down.saturating_add(1))
                .min(right.saturating_add(1));
        }
    }
    d
}

/// A moulded feature's shadow and lit side. Drawing only the shadow looks drawn on, not moulded.
/// Both are coverage masks that never overlap: their sum is the asset pixel's coverage.
pub(crate) struct Detail {
    pub shadow: Vec<u8>,
    pub highlight: Vec<u8>,
}

impl Detail {
    fn blank(w: u32, h: u32) -> Detail {
        Detail {
            shadow: vec![0; (w * h) as usize],
            highlight: vec![0; (w * h) as usize],
        }
    }
}

/// The grip ridge above the label and the thumb notch. Shaded into the shell, not a fixed colour.
pub(crate) fn detail_mask() -> &'static Detail {
    static MASK: OnceLock<Detail> = OnceLock::new();
    MASK.get_or_init(|| {
        let mut detail = rasterise_detail(DETAIL_SVG, CART_W, CART_H)
            .unwrap_or_else(|| Detail::blank(CART_W, CART_H));
        emboss(&mut detail, GBA_LETTERING, CART_W, CART_H);
        detail
    })
}

/// The Game Boy pak's moulding: shoulder ribs, lettering plate, side grooves and arrow. One per
/// shell, because the class C shoulder is smooth (see `gbc_cart_detail.svg`).
pub(crate) fn gb_detail_mask(shell: GbShell) -> &'static Detail {
    static NOTCHED: OnceLock<Detail> = OnceLock::new();
    static ROUNDED: OnceLock<Detail> = OnceLock::new();
    let (lock, svg) = match shell {
        GbShell::Notched => (&NOTCHED, GB_DETAIL_SVG),
        GbShell::Rounded => (&ROUNDED, GBC_DETAIL_SVG),
    };
    lock.get_or_init(|| {
        let mut detail = rasterise_detail(svg, GB_CART_W, GB_CART_H)
            .unwrap_or_else(|| Detail::blank(GB_CART_W, GB_CART_H));
        let lettering = match shell {
            GbShell::Notched => GB_LETTERING,
            GbShell::Rounded => GBC_LETTERING,
        };
        emboss(&mut detail, lettering, GB_CART_W, GB_CART_H);
        if shell == GbShell::Rounded {
            // The groove under GAME BOY COLOR: a cut, so shadow above and light below.
            let at = |v: u32, of: u32, to: u32| (v * to + of / 2) / of;
            let y = at(52, 253, GB_CART_H);
            for x in at(72, 240, GB_CART_W)..at(168, 240, GB_CART_W) {
                detail.shadow[(y * GB_CART_W + x) as usize] = 200;
                detail.highlight[((y + 1) * GB_CART_W + x) as usize] = 160;
            }
        }
        detail
    })
}

/// The platform name moulded into the shell, set by `examples/lettering.rs` in Nintendo's own
/// faces and kept as a coverage mask the size of the face.
const GB_LETTERING: &[u8] = include_bytes!("../assets/lettering_gb.png");
const GBC_LETTERING: &[u8] = include_bytes!("../assets/lettering_gbc.png");
const GBA_LETTERING: &[u8] = include_bytes!("../assets/lettering_gba.png");

/// Raises the lettering in `png` on the shell, lit from the upper left like the rest of the
/// moulding: an edge facing the light is highlight, the plastic just past a far edge is shadow.
fn emboss(detail: &mut Detail, png: &[u8], w: u32, h: u32) {
    let Some(mask) = decode_mask(png, w, h) else {
        return;
    };
    let at = |x: i32, y: i32| -> u8 {
        if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
            0
        } else {
            mask[(y as u32 * w + x as u32) as usize]
        }
    };
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let (here, before) = (at(x, y), at(x - 1, y - 1));
            let i = (y as u32 * w + x as u32) as usize;
            detail.highlight[i] = detail.highlight[i].saturating_add(here.saturating_sub(before));
            detail.shadow[i] = detail.shadow[i].saturating_add(before.saturating_sub(here));
        }
    }
}

/// One grey byte per pixel.
fn decode_mask(png: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(png))
        .read_info()
        .ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let grey =
        info.color_type == png::ColorType::Grayscale && info.bit_depth == png::BitDepth::Eight;
    if !grey {
        return None;
    }
    let (sw, sh) = (info.width, info.height);
    if (sw, sh) == (w, h) {
        return Some(buf[..(w * h) as usize].to_vec());
    }
    let src = |x: u32, y: u32| buf[(y.min(sh - 1) * sw + x.min(sw - 1)) as usize] as f32;
    let mut out = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        let fy = ((y as f32 + 0.5) * sh as f32 / h as f32 - 0.5).max(0.0);
        let (y0, ty) = (fy as u32, fy.fract());
        for x in 0..w {
            let fx = ((x as f32 + 0.5) * sw as f32 / w as f32 - 0.5).max(0.0);
            let (x0, tx) = (fx as u32, fx.fract());
            let top = src(x0, y0) * (1.0 - tx) + src(x0 + 1, y0) * tx;
            let bottom = src(x0, y0 + 1) * (1.0 - tx) + src(x0 + 1, y0 + 1) * tx;
            out.push((top * (1.0 - ty) + bottom * ty).round() as u8);
        }
    }
    Some(out)
}

fn rasterise(w: u32, h: u32) -> Option<Vec<u8>> {
    rasterise_svg(CART_SVG, w, h)
}

/// Splits one asset by luminance: black is shadow, white is light. One file keeps the two edges
/// from drifting apart.
fn rasterise_detail(svg: &str, w: u32, h: u32) -> Option<Detail> {
    let px = render(svg, w, h)?;
    let mut shadow = Vec::with_capacity((w * h) as usize);
    let mut highlight = Vec::with_capacity((w * h) as usize);
    for p in px.data().chunks_exact(4) {
        // Premultiplied, so a white pixel's luminance is its alpha. Weights are Rec. 709 over
        // 256; `min` guards against rounding pushing light past cover.
        let lit = ((p[0] as u32 * 54 + p[1] as u32 * 183 + p[2] as u32 * 19) / 256) as u8;
        let lit = lit.min(p[3]);
        highlight.push(lit);
        shadow.push(p[3] - lit);
    }
    Some(Detail { shadow, highlight })
}

fn rasterise_svg(svg: &str, w: u32, h: u32) -> Option<Vec<u8>> {
    let px = render(svg, w, h)?;
    Some(px.data().iter().skip(3).step_by(4).copied().collect())
}

fn render(svg: &str, w: u32, h: u32) -> Option<resvg::tiny_skia::Pixmap> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)?;
    let size = tree.size();
    let scale =
        resvg::tiny_skia::Transform::from_scale(w as f32 / size.width(), h as f32 / size.height());
    resvg::render(&tree, scale, &mut pixmap.as_mut());
    Some(pixmap)
}
