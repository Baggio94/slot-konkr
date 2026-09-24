use std::sync::OnceLock;

use crate::cart::{CART_H, CART_W};

const CART_SVG: &str = include_str!("../assets/cart.svg");
const DETAIL_SVG: &str = include_str!("../assets/cart_detail.svg");

/// Coverage of the cart outline, one byte per pixel, row major.
pub fn silhouette(w: u32, h: u32) -> Vec<u8> {
    rasterise(w, h).unwrap_or_else(|| vec![255; (w * h) as usize])
}

/// The mask is rasterised once and multiplied into every face.
pub(crate) fn cart_mask() -> &'static [u8] {
    static MASK: OnceLock<Vec<u8>> = OnceLock::new();
    MASK.get_or_init(|| silhouette(CART_W, CART_H))
}

/// How far inside the outline each pixel sits, in city block steps, saturating at 255. A
/// translucent shell fades from its edge inward.
pub(crate) fn cart_depth() -> &'static [u8] {
    static DEPTH: OnceLock<Vec<u8>> = OnceLock::new();
    DEPTH.get_or_init(|| depth_map(cart_mask(), CART_W as usize, CART_H as usize))
}

/// Two pass chamfer. Off-buffer counts as outside, so the top row is one step in.
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
        rasterise_detail(DETAIL_SVG, CART_W, CART_H)
            .unwrap_or_else(|| Detail::blank(CART_W, CART_H))
    })
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
