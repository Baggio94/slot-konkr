//! The quick menu on the carousel: its rows, their faces, and where each lands on the panel.

use crate::draw::{Draw, TexId, OUT_H, OUT_W};
use crate::plate::{arrows_hint_face, centred_hints, hint_face, UndoFace, HINT_H, LEGEND_GAP};
use crate::power_menu::{MENU_H, MENU_INK, MENU_PAD, MENU_PX};
use crate::slot_chrome::{edge, opening};
use crate::text;

/// The menu's rows, top to bottom in the order the user chose.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum QuickRow {
    FastForward,
    FastForwardSound,
    ColourCorrection,
    Rumble,
    DateTime,
    About,
}

impl QuickRow {
    /// The bar opens on `ALL[0]`, so Fast Forward stays first. Date & Time and About stay together
    /// so the OPEN/CHANGE legend flips once. Fast Forward and its Sound are a pair.
    pub const ALL: [QuickRow; 6] = [
        QuickRow::FastForward,
        QuickRow::FastForwardSound,
        QuickRow::ColourCorrection,
        QuickRow::Rumble,
        QuickRow::DateTime,
        QuickRow::About,
    ];

    /// Position in `ALL`, the order labels are uploaded and drawn in.
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn label(self) -> &'static str {
        match self {
            QuickRow::FastForward => "Fast Forward",
            QuickRow::FastForwardSound => "Fast Forward Sound",
            QuickRow::ColourCorrection => "Colour Correction",
            QuickRow::Rumble => "Rumble",
            QuickRow::DateTime => "Date & Time",
            QuickRow::About => "About",
        }
    }

    /// A row A opens, rather than one the arrows change.
    pub fn opens(self) -> bool {
        matches!(self, QuickRow::DateTime | QuickRow::About)
    }

    /// The row above, stopping at the top: no menu here wraps.
    pub fn up(self) -> QuickRow {
        QuickRow::ALL[self.index().saturating_sub(1)]
    }

    pub fn down(self) -> QuickRow {
        QuickRow::ALL[(self.index() + 1).min(QuickRow::ALL.len() - 1)]
    }
}

/// Every value a changeable row can show, each rastered once at boot in both inks.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum QuickValue {
    Speed2,
    Speed3,
    Speed4,
    Speed6,
    On,
    Off,
}

impl QuickValue {
    pub const ALL: [QuickValue; 6] = [
        QuickValue::Speed2,
        QuickValue::Speed3,
        QuickValue::Speed4,
        QuickValue::Speed6,
        QuickValue::On,
        QuickValue::Off,
    ];

    /// Position in `ALL`, which is the order the faces are uploaded in.
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn text(self) -> &'static str {
        match self {
            QuickValue::Speed2 => "2×",
            QuickValue::Speed3 => "3×",
            QuickValue::Speed4 => "4×",
            QuickValue::Speed6 => "6×",
            QuickValue::On => "On",
            QuickValue::Off => "Off",
        }
    }

    /// A fast forward ceiling the menu offers, `None` for any other. Must match `FF_SPEEDS`, or a
    /// card could hold a speed with no face; `tests/quick_menu.rs` holds them together.
    pub fn speed(frames: u8) -> Option<QuickValue> {
        match frames {
            2 => Some(QuickValue::Speed2),
            3 => Some(QuickValue::Speed3),
            4 => Some(QuickValue::Speed4),
            6 => Some(QuickValue::Speed6),
            _ => None,
        }
    }

    pub fn flag(on: bool) -> QuickValue {
        if on {
            QuickValue::On
        } else {
            QuickValue::Off
        }
    }
}

/// A size up from the power menu's rows: 30 px type on 52 px rows.
pub const QUICK_PITCH: f32 = 52.0;
/// The first row's top, with all rows centred on the panel.
pub const QUICK_TOP: f32 = (OUT_H as f32 - QUICK_PITCH * QuickRow::ALL.len() as f32) / 2.0;
/// Labels start this far in from the left, and values end this far in from the right.
pub const QUICK_EDGE: f32 = 32.0;
/// How much shorter the bar is than its row, top and bottom, as the power menu's is.
const BAR_INSET: f32 = 4.0;
/// Puts the baseline 36 px below the row's top, capitals centred in the bar, as the mockup has.
const TYPE_DROP: f32 = 4.0;
/// Between each arrow and the value it stands beside, about a space of the type.
const CARET_GAP: f32 = 14.0;
/// A little under the capitals they stand beside, so the arrows read as marks, not letters.
const CARET_PX: f32 = 24.0;
/// The legend's key caps are centred 41 px off the bottom of the panel, as in the mockup.
const LEGEND_Y: f32 = 427.0;
/// The value on every row but the one in hand.
const DIM_INK: [u8; 3] = [0x9a, 0x9a, 0xa4];

pub fn quick_label_face(row: QuickRow) -> UndoFace {
    quick_text_face(row.label(), MENU_INK)
}

/// A value in the menu's type: lit for the row in hand, grey for the rest.
pub fn quick_value_face(text: &str, lit: bool) -> UndoFace {
    quick_text_face(text, if lit { MENU_INK } else { DIM_INK })
}

/// A line of the menu's type, sized with tracking so it sits exactly `MENU_PAD` in from both
/// sides. Not `menu_face`, which ignores tracking and so misplaced labels off the 32 px edge.
fn quick_text_face(label: &str, colour: [u8; 3]) -> UndoFace {
    let Some(font) = text::label_font() else {
        return UndoFace {
            rgba: Vec::new(),
            w: 0,
            h: 0,
        };
    };
    // At the menu's size, as wide as the panel, so nothing is ever shrunk or broken.
    let layout = text::fit(font, label, OUT_W as f32, 1, MENU_PX, MENU_PX);
    let set = layout
        .lines
        .iter()
        .map(|l| text::line_width(font, l, layout.px, layout.tracking))
        .fold(0.0, f32::max);
    let w = set.ceil() as u32 + 2 * MENU_PAD;
    let mut rgba = vec![0u8; (w * MENU_H * 4) as usize];
    text::draw_centred(&mut rgba, w, MENU_H, &layout, colour);
    UndoFace { rgba, w, h: MENU_H }
}

/// One of the arrows beside the value in hand, from the symbols font (`label.ttf` has no
/// arrows). Centred on the capitals so it sits on the value's line.
pub fn quick_caret_face(right: bool) -> UndoFace {
    let glyph = if right { '\u{f0da}' } else { '\u{f0d9}' };
    let (Some(symbols), Some(label)) = (crate::icon::symbols_font(), text::label_font()) else {
        return UndoFace {
            rgba: Vec::new(),
            w: 0,
            h: 0,
        };
    };
    let (m, cov) = symbols.rasterize(glyph, CARET_PX);
    let (w, h) = (m.width as u32, MENU_H);
    // Where `menu_face` puts the capitals: the baseline less half a capital's height.
    let centre = match label.horizontal_line_metrics(MENU_PX) {
        Some(v) => {
            (h as f32 - v.new_line_size) / 2.0 + v.ascent
                - label.metrics('H', MENU_PX).height as f32 / 2.0
        }
        None => h as f32 / 2.0,
    };
    let top = (centre - m.height as f32 / 2.0).round() as i32;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for gy in 0..m.height {
        let y = top + gy as i32;
        if y < 0 || y >= h as i32 {
            continue;
        }
        for gx in 0..m.width {
            let at = ((y as u32 * w + gx as u32) * 4) as usize;
            let a = cov[gy * m.width + gx];
            rgba[at..at + 4].copy_from_slice(&[MENU_INK[0], MENU_INK[1], MENU_INK[2], a]);
        }
    }
    UndoFace { rgba, w, h }
}

/// B BACK, the arrows' CHANGE and A OPEN, in the order `QuickMenuFaces::legend` holds them.
pub fn quick_legend_faces() -> [UndoFace; 3] {
    [
        hint_face("B", "Back"),
        arrows_hint_face("Change"),
        hint_face("A", "Open"),
    ]
}

/// Everything the binary uploads for the menu at boot, each face with its raster size.
pub struct QuickMenuFaces {
    /// One per `QuickRow::ALL`, in that order.
    pub labels: Vec<(TexId, u32, u32)>,
    /// One pair per `QuickValue::ALL`, in that order: grey, then lit.
    pub values: Vec<[(TexId, u32, u32); 2]>,
    /// Left, then right.
    pub carets: [(TexId, u32, u32); 2],
    /// `quick_legend_faces`, in that order, with their widths.
    pub legend: [(TexId, u32); 3],
}

/// The menu as it stands this frame.
pub struct QuickMenu<'a> {
    pub row: QuickRow,
    /// What each row shows, in `QuickRow::ALL` order, and `None` for the two that open.
    pub values: [Option<QuickValue>; QuickRow::ALL.len()],
    /// Date & Time's value, grey then lit, once the binary has built it.
    pub clock: Option<[(TexId, u32, u32); 2]>,
    /// `None` until boot has uploaded them, when only the ground and the bar are drawn.
    pub faces: Option<&'a QuickMenuFaces>,
}

impl QuickMenu<'_> {
    pub fn draw(&self, out: &mut Vec<Draw>) {
        out.push(Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: OUT_W as f32,
            h: OUT_H as f32,
            colour: opening(),
        });
        // Full width like the power menu's: a bar sized to each label would jump as it moved.
        out.push(Draw::Rect {
            x: 0.0,
            y: row_top(self.row) + BAR_INSET,
            w: OUT_W as f32,
            h: QUICK_PITCH - 2.0 * BAR_INSET,
            colour: edge(),
        });
        let Some(faces) = self.faces else {
            return;
        };
        let (right, pad) = (OUT_W as f32 - QUICK_EDGE, MENU_PAD as f32);
        for row in QuickRow::ALL {
            let y = row_top(row) + TYPE_DROP;
            let lit = row == self.row;
            if let Some(&(tex, w, h)) = faces.labels.get(row.index()) {
                push(out, tex, QUICK_EDGE - pad, y, w, h);
            }
            let value = match row {
                QuickRow::DateTime => self.clock.map(|c| c[lit as usize]),
                _ => self.values[row.index()]
                    .and_then(|v| faces.values.get(v.index()))
                    .map(|v| v[lit as usize]),
            };
            let Some((tex, w, h)) = value else {
                continue;
            };
            if !lit || row.opens() {
                push(out, tex, right + pad - w as f32, y, w, h);
                continue;
            }
            // Arrows either side of the value in hand; the right one takes the value's edge.
            let [(left_tex, lw, lh), (right_tex, rw, rh)] = faces.carets;
            let rx = right - rw as f32;
            push(out, right_tex, rx, y, rw, rh);
            let vx = rx - CARET_GAP + pad - w as f32;
            push(out, tex, vx, y, w, h);
            push(out, left_tex, vx + pad - CARET_GAP - lw as f32, y, lw, lh);
        }
        let [back, change, open] = faces.legend;
        let other = if self.row.opens() { open } else { change };
        for (tex, w, x) in centred_hints(&[back, other], LEGEND_GAP) {
            push(out, tex, x, LEGEND_Y, w, HINT_H);
        }
    }
}

fn row_top(row: QuickRow) -> f32 {
    QUICK_TOP + QUICK_PITCH * row.index() as f32
}

/// A face at its own size, on whole pixels, the only place it is sharp.
fn push(out: &mut Vec<Draw>, tex: TexId, x: f32, y: f32, w: u32, h: u32) {
    out.push(Draw::Tex {
        x: x.round(),
        y: y.round(),
        w: w as f32,
        h: h as f32,
        tex,
        alpha: 1.0,
    });
}
