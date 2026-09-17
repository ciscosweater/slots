//! The quick menu on the carousel: its rows, their faces, and where each lands on the panel.

use crate::draw::{Draw, TexId, OUT_H, OUT_W};
use crate::plate::{arrows_hint_face, centred_hints, hint_face, UndoFace, HINT_H, LEGEND_GAP};
use crate::power_menu::{MENU_H, MENU_INK, MENU_PAD, MENU_PX};
use crate::slot_chrome::{edge, opening};
use crate::text;

/// The menu's rows. Shelf and in-game show different subsets; label faces cover every variant.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum QuickRow {
    FastForward,
    FastForwardSound,
    ColourCorrection,
    Rumble,
    FaceButtons,
    Overlay,
    LcdEffect,
    Picture,
    DateTime,
    About,
}

impl QuickRow {
    /// Shelf menu, top to bottom. Picture stays out: it is per-cart and only live in-game.
    pub const ALL: [QuickRow; 9] = [
        QuickRow::FastForward,
        QuickRow::FastForwardSound,
        QuickRow::ColourCorrection,
        QuickRow::Rumble,
        QuickRow::FaceButtons,
        QuickRow::Overlay,
        QuickRow::LcdEffect,
        QuickRow::DateTime,
        QuickRow::About,
    ];

    /// Every row that has a rastered label, including Picture for the in-game menu.
    pub const LABELS: [QuickRow; 10] = [
        QuickRow::FastForward,
        QuickRow::FastForwardSound,
        QuickRow::ColourCorrection,
        QuickRow::Rumble,
        QuickRow::FaceButtons,
        QuickRow::Overlay,
        QuickRow::LcdEffect,
        QuickRow::Picture,
        QuickRow::DateTime,
        QuickRow::About,
    ];

    /// In-game display settings. Callers append `Picture` for Game Boy carts.
    pub const PLAYING: [QuickRow; 3] = [
        QuickRow::LcdEffect,
        QuickRow::ColourCorrection,
        QuickRow::Overlay,
    ];

    /// Position in `LABELS`, which is the order faces are uploaded in.
    pub fn label_index(self) -> usize {
        Self::LABELS
            .iter()
            .position(|row| *row == self)
            .expect("every QuickRow is in LABELS")
    }

    /// Position in `ALL` for walking the shelf menu with Down.
    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|row| *row == self)
            .expect("row is on the shelf menu")
    }

    pub fn label(self) -> &'static str {
        match self {
            QuickRow::FastForward => "Fast Forward",
            QuickRow::FastForwardSound => "Fast Forward Sound",
            QuickRow::ColourCorrection => "Colour Correction",
            QuickRow::Rumble => "Rumble",
            QuickRow::FaceButtons => "X / Y Buttons",
            QuickRow::Overlay => "GB Overlay",
            QuickRow::LcdEffect => "LCD Effect",
            QuickRow::Picture => "Picture",
            QuickRow::DateTime => "Date & Time",
            QuickRow::About => "About",
        }
    }

    /// A row A opens, rather than one the arrows change.
    pub fn opens(self) -> bool {
        matches!(self, QuickRow::DateTime | QuickRow::About)
    }

    /// The row above within `rows`, stopping at the top.
    pub fn up_in(self, rows: &[QuickRow]) -> QuickRow {
        match rows.iter().position(|row| *row == self) {
            Some(i) if i > 0 => rows[i - 1],
            _ => self,
        }
    }

    pub fn down_in(self, rows: &[QuickRow]) -> QuickRow {
        match rows.iter().position(|row| *row == self) {
            Some(i) if i + 1 < rows.len() => rows[i + 1],
            _ => self,
        }
    }

    /// Shelf navigation helper kept for call sites that only ever walk `ALL`.
    pub fn up(self) -> QuickRow {
        self.up_in(&Self::ALL)
    }

    pub fn down(self) -> QuickRow {
        self.down_in(&Self::ALL)
    }
}

/// Every value a row the arrows change can show. There are few enough that each is rastered
/// once at boot, in both inks, and never again.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum QuickValue {
    Speed2,
    Speed3,
    Speed4,
    Speed6,
    On,
    Off,
    Shortcuts,
    Shoulders,
    Turbo,
    FillScreen,
    ActualSize,
}

impl QuickValue {
    pub const ALL: [QuickValue; 11] = [
        QuickValue::Speed2,
        QuickValue::Speed3,
        QuickValue::Speed4,
        QuickValue::Speed6,
        QuickValue::On,
        QuickValue::Off,
        QuickValue::Shortcuts,
        QuickValue::Shoulders,
        QuickValue::Turbo,
        QuickValue::FillScreen,
        QuickValue::ActualSize,
    ];

    /// Position in `ALL`, which is the order in which faces are uploaded.
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
            QuickValue::Shortcuts => "Shortcuts",
            QuickValue::Shoulders => "L / R",
            QuickValue::Turbo => "A / B Turbo",
            QuickValue::FillScreen => "Fill Screen",
            QuickValue::ActualSize => "Actual Size",
        }
    }

    /// A fast-forward speed the menu offers, and `None` for any other.
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

    pub fn face_buttons(mode: slot_store::FaceButtons) -> QuickValue {
        match mode {
            slot_store::FaceButtons::Shortcuts => QuickValue::Shortcuts,
            slot_store::FaceButtons::Shoulders => QuickValue::Shoulders,
            slot_store::FaceButtons::Turbo => QuickValue::Turbo,
        }
    }
}

/// Nine shelf rows at 40 px leave room for the legend under the last bar.
pub const QUICK_PITCH: f32 = 40.0;
/// Labels start this far in from the left, and values end this far in from the right.
pub const QUICK_EDGE: f32 = 32.0;
const BAR_INSET: f32 = 4.0;
const TYPE_DROP: f32 = 4.0;
const CARET_GAP: f32 = 14.0;
const CARET_PX: f32 = 24.0;
const LEGEND_Y: f32 = 440.0;
const DIM_INK: [u8; 3] = [0x9a, 0x9a, 0xa4];

pub fn quick_top(row_count: usize) -> f32 {
    (OUT_H as f32 - QUICK_PITCH * row_count as f32) / 2.0
}

/// The first row's top when the shelf menu is showing every `ALL` row.
pub const QUICK_TOP: f32 = (OUT_H as f32 - QUICK_PITCH * QuickRow::ALL.len() as f32) / 2.0;

pub fn quick_label_face(row: QuickRow) -> UndoFace {
    quick_text_face(row.label(), MENU_INK)
}

pub fn quick_value_face(text: &str, lit: bool) -> UndoFace {
    quick_text_face(text, if lit { MENU_INK } else { DIM_INK })
}

fn quick_text_face(label: &str, colour: [u8; 3]) -> UndoFace {
    let Some(font) = text::label_font() else {
        return UndoFace {
            rgba: Vec::new(),
            w: 0,
            h: 0,
        };
    };
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

pub fn quick_legend_faces() -> [UndoFace; 3] {
    [
        hint_face("B", "Back"),
        arrows_hint_face("Change"),
        hint_face("A", "Open"),
    ]
}

pub struct QuickMenuFaces {
    pub labels: Vec<(TexId, u32, u32)>,
    pub values: Vec<[(TexId, u32, u32); 2]>,
    pub carets: [(TexId, u32, u32); 2],
    pub legend: [(TexId, u32); 3],
}

pub struct QuickMenu<'a> {
    pub row: QuickRow,
    /// Which rows this opening shows, in order. Shelf uses `ALL`; in-game uses the display set.
    pub rows: &'a [QuickRow],
    /// Value for each entry in `LABELS` order (`None` for rows that open or are not shown).
    pub values: [Option<QuickValue>; QuickRow::LABELS.len()],
    pub clock: Option<[(TexId, u32, u32); 2]>,
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
        let top = quick_top(self.rows.len());
        let Some(faces) = self.faces else {
            return;
        };
        let (right, pad) = (OUT_W as f32 - QUICK_EDGE, MENU_PAD as f32);
        for (i, &row) in self.rows.iter().enumerate() {
            let y0 = top + QUICK_PITCH * i as f32;
            let lit = row == self.row;
            if lit {
                out.push(Draw::Rect {
                    x: 0.0,
                    y: y0 + BAR_INSET,
                    w: OUT_W as f32,
                    h: QUICK_PITCH - 2.0 * BAR_INSET,
                    colour: edge(),
                });
            }
            let y = y0 + TYPE_DROP;
            if let Some(&(tex, w, h)) = faces.labels.get(row.label_index()) {
                push(out, tex, QUICK_EDGE - pad, y, w, h);
            }
            let value = match row {
                QuickRow::DateTime => self.clock.map(|c| c[lit as usize]),
                _ => self.values[row.label_index()]
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
