use std::path::Path;

use slot_gfx::{Draw, TexId, OUT_H, OUT_W};

use crate::art;

/// Cover the whole panel, centre cropped. PNG only, as the labels are.
pub fn wallpaper_face(path: &Path) -> Option<Vec<u8>> {
    art::cover(path, OUT_W, OUT_H)
}

/// The picture as the first thing on the screen. Nothing at all when the card carries no
/// wallpaper: the clear colour is already the ground.
pub fn draw_backdrop(face: Option<TexId>, out: &mut Vec<Draw>) {
    let Some(tex) = face else {
        return;
    };
    out.push(Draw::Tex {
        x: 0.0,
        y: 0.0,
        w: OUT_W as f32,
        h: OUT_H as f32,
        tex,
        alpha: 1.0,
    });
}
