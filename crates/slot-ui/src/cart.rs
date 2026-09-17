use slot_store::Cart;

use crate::art;
use crate::shell::{shell_for, Finish, Shell};
use crate::silhouette::{
    cart_depth, cart_mask, detail_mask, gb_cart_depth, gb_cart_mask, gb_detail_mask,
    gbc_cart_depth, gbc_cart_mask, gbc_detail_mask, DetailMask,
};
use crate::text;

/// The traced outline's own aspect, so `cart.svg` rasterises unstretched. Three across a
/// 720 wide row exactly, so the shelf can show a neighbour either side of the selection.
pub const CART_W: u32 = 240;
pub const CART_H: u32 = 135;
pub const GB_CART_W: u32 = 240;
pub const GB_CART_H: u32 = 274;
pub const GB_LABEL_W: u32 = 192;
pub const GB_LABEL_H: u32 = 169;
pub const GB_LABEL_X: u32 = (GB_CART_W - GB_LABEL_W) / 2;
pub const GB_LABEL_Y: u32 = 56;

/// The paper label, inset beneath the moulded grip at the physical GBA label's roughly
/// 43:22 aspect. At 176x90 it stays clear of both the ridge and lower thumb arrow while
/// avoiding the overly panoramic 196x86 window this cart used before.
pub const fn label_panel(w: u32, h: u32) -> (u32, u32, u32, u32) {
    (
        (w * 133 + 500) / 1000,
        (h * 200 + 500) / 1000,
        (w * 867 + 500) / 1000,
        (h * 867 + 500) / 1000,
    )
}

pub const LABEL_X: u32 = label_panel(CART_W, CART_H).0;
pub const LABEL_Y: u32 = label_panel(CART_W, CART_H).1;
pub const LABEL_W: u32 = label_panel(CART_W, CART_H).2 - LABEL_X;
pub const LABEL_H: u32 = label_panel(CART_W, CART_H).3 - LABEL_Y;

const PAD: u32 = 10;
const MAX_LINES: usize = 3;
/// Three lines have to clear the label's height, and Open Sans Bold sets at about 1.36x
/// the em. The label is landscape now, so it runs out of height long before width.
const MAX_PX: f32 = LABEL_H as f32 / (MAX_LINES as f32 * 1.36);
const MIN_PX: f32 = 10.0;

/// How far the translucent edge reaches in. Zero at this depth exactly, so a pixel any
/// further in is the plastic's own colour.
const RIM: u32 = 4;

pub struct CartFace {
    pub rgba: Vec<u8>,
    pub w: u32,
    pub h: u32,
}

/// The cart's own shape in black. Drawn under a side cart so the dimming is a cart in shadow
/// rather than a cart you can see through: over a wallpaper a translucent face is a ghost,
/// and the shelf's carts are solid objects.
pub fn cart_shadow() -> CartFace {
    let mut rgba = Vec::with_capacity((CART_W * CART_H * 4) as usize);
    for cover in cart_mask() {
        rgba.extend_from_slice(&[0, 0, 0, *cover]);
    }
    CartFace {
        rgba,
        w: CART_W,
        h: CART_H,
    }
}

pub fn cart_face(cart: &Cart) -> CartFace {
    cart_face_with_artwork(cart).0
}

/// Builds a shelf face and reports whether it came from a decodable complete-artwork image.
/// Keeping this result beside the face lets the shelf use the generated shadow and label badge
/// when a file exists but is malformed, rather than treating the path's mere presence as proof
/// that the fallback was not needed.
pub fn cart_face_with_artwork(cart: &Cart) -> (CartFace, bool) {
    let w = match cart.platform {
        slot_store::Platform::Gba => CART_W,
        slot_store::Platform::Gb | slot_store::Platform::Gbc => GB_CART_W,
    };
    if let Some((rgba, w, h)) = cart.artwork.as_deref().and_then(|p| art::fit_width(p, w)) {
        return (CartFace { rgba, w, h }, true);
    }
    if cart.platform != slot_store::Platform::Gba {
        return (gb_cart_face(cart), false);
    }
    let shell = shell_for(cart);
    let mut face = shell_face(&shell);
    let label = match cart
        .label
        .as_deref()
        .and_then(|p| art::cover(p, LABEL_W, LABEL_H))
    {
        Some(rgba) => rgba,
        None => generated_label(&label_text(cart)),
    };
    mould_detail(&mut face, &shell);
    recess_label(&mut face, &shell);
    paste_label(&mut face, &label);
    clip_to_silhouette(&mut face);
    (face, false)
}

pub fn cart_shadow_for(platform: slot_store::Platform) -> CartFace {
    match platform {
        slot_store::Platform::Gba => cart_shadow(),
        slot_store::Platform::Gb => {
            let mut rgba = Vec::with_capacity((GB_CART_W * GB_CART_H * 4) as usize);
            for cover in gb_cart_mask() {
                rgba.extend_from_slice(&[0, 0, 0, *cover]);
            }
            CartFace {
                rgba,
                w: GB_CART_W,
                h: GB_CART_H,
            }
        }
        slot_store::Platform::Gbc => {
            let mut rgba = Vec::with_capacity((GB_CART_W * GB_CART_H * 4) as usize);
            for cover in gbc_cart_mask() {
                rgba.extend_from_slice(&[0, 0, 0, *cover]);
            }
            CartFace {
                rgba,
                w: GB_CART_W,
                h: GB_CART_H,
            }
        }
    }
}

/// A cheap silhouette shown while a library face is being rasterised. It keeps the real cart
/// outline (including the GB/GBC proportions) without pretending a rectangular colour block is
/// a loaded label. One copy per platform is enough; the shelf uses it for every missing face.
pub fn cart_placeholder_for(platform: slot_store::Platform) -> CartFace {
    let mut face = cart_shadow_for(platform);
    let colour = match platform {
        slot_store::Platform::Gba => [0x4f, 0x54, 0x5f],
        slot_store::Platform::Gb => [0x9b, 0x9c, 0x96],
        slot_store::Platform::Gbc => [0x49, 0x70, 0x83],
    };
    for px in face.rgba.chunks_exact_mut(4) {
        px[..3].copy_from_slice(&colour);
    }
    face
}

fn gb_cart_face(cart: &Cart) -> CartFace {
    let (shell, mask, depth, detail) = match cart.platform {
        slot_store::Platform::Gb => (
            Shell {
                colour: [0x9b, 0x9c, 0x96],
                finish: Finish::Solid,
            },
            gb_cart_mask(),
            gb_cart_depth(),
            gb_detail_mask(),
        ),
        slot_store::Platform::Gbc => (
            Shell {
                colour: [0x49, 0x70, 0x83],
                finish: Finish::Translucent,
            },
            gbc_cart_mask(),
            gbc_cart_depth(),
            gbc_detail_mask(),
        ),
        slot_store::Platform::Gba => unreachable!(),
    };

    let mut face = shell_face_custom(&shell, GB_CART_W, GB_CART_H, depth);
    let label = match cart
        .label
        .as_deref()
        .and_then(|p| art::cover(p, GB_LABEL_W, GB_LABEL_H))
    {
        Some(rgba) => rgba,
        None => generated_label_sized(&label_text(cart), GB_LABEL_W, GB_LABEL_H),
    };

    mould_detail_custom(&mut face, &shell, detail);
    recess_label_custom(
        &mut face, &shell, GB_LABEL_X, GB_LABEL_Y, GB_LABEL_W, GB_LABEL_H,
    );
    paste_label_custom(
        &mut face, &label, GB_LABEL_X, GB_LABEL_Y, GB_LABEL_W, GB_LABEL_H,
    );
    clip_to_silhouette_custom(&mut face, mask);
    face
}

fn generated_label_sized(title: &str, w: u32, h: u32) -> Vec<u8> {
    let bg = label_colour(title);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..w * h {
        rgba.extend_from_slice(&[bg[0], bg[1], bg[2], 255]);
    }
    if let Some(font) = text::label_font() {
        let layout = text::fit(
            font,
            title,
            (w - 2 * PAD) as f32,
            MAX_LINES,
            h as f32 / 4.1,
            MIN_PX,
        );
        text::draw_centred(&mut rgba, w, h, &layout, ink(bg));
    }
    rgba
}

/// Colour is left alone and only alpha is cut, because the sprite pass blends straight
/// alpha rather than premultiplied.
fn clip_to_silhouette(face: &mut CartFace) {
    clip_to_silhouette_custom(face, cart_mask());
}

fn clip_to_silhouette_custom(face: &mut CartFace, mask: &[u8]) {
    for (px, cover) in face.rgba.chunks_exact_mut(4).zip(mask) {
        px[3] = ((px[3] as u32 * *cover as u32 + 127) / 255) as u8;
    }
}

/// Stable across runs, which the standard hasher is not: the same game must be the same
/// colour on every boot, or the shelf is unrecognisable from memory.
pub fn label_colour(title: &str) -> [u8; 3] {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in title.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hsv_to_rgb((h % 360) as f32, 0.52, 0.74)
}

/// The header title is capped at twelve characters, so it reads `POKEMON EMER`. The
/// filename holds the real name.
pub fn label_text(cart: &Cart) -> String {
    clean_label(&cart.stem)
}

/// A dumped filename carries region and revision tags and separates title from subtitle
/// with a spaced hyphen. A bare hyphen is part of a word, so `Spider-Man` keeps its own.
pub fn clean_label(stem: &str) -> String {
    let mut bare = String::with_capacity(stem.len());
    let mut depth = 0u32;
    for ch in stem.chars() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => bare.push(ch),
            _ => {}
        }
    }

    let mut out = String::with_capacity(bare.len());
    for word in bare.split_whitespace().filter(|w| *w != "-") {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    if out.is_empty() {
        stem.to_string()
    } else {
        out
    }
}

/// The bracketed groups `clean_label` throws away, in the order they appeared. A dump's
/// filename carries them as one run of parentheses — `(USA, Europe) (Rev 1)` — and each group
/// is one fact about this dump rather than about the game, which is why they are worth
/// keeping apart from the title instead of inside it.
///
/// One tag per group, not per comma: `(USA, Europe)` is a single release in two regions, and
/// splitting it would claim two.
pub fn label_tags(stem: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0u32;
    let mut cur = String::new();
    for ch in stem.chars() {
        match ch {
            '(' | '[' => {
                depth += 1;
                if depth == 1 {
                    cur.clear();
                    continue;
                }
            }
            ')' | ']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let t = cur.trim();
                    if !t.is_empty() {
                        out.push(t.to_string());
                    }
                    continue;
                }
            }
            _ => {}
        }
        if depth >= 1 {
            cur.push(ch);
        }
    }
    out
}

fn shell_face(shell: &Shell) -> CartFace {
    shell_face_custom(shell, CART_W, CART_H, cart_depth())
}

fn gbc_board() -> &'static [u8] {
    static BOARD: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    BOARD.get_or_init(|| generate_gbc_board(GB_CART_W, GB_CART_H))
}

fn generate_gbc_board(w: u32, h: u32) -> Vec<u8> {
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let mut put = |x: u32, y: u32, c: [u8; 3]| {
        if x < w && y < h {
            let i = ((y * w + x) * 4) as usize;
            rgba[i] = c[0];
            rgba[i + 1] = c[1];
            rgba[i + 2] = c[2];
            rgba[i + 3] = 255;
        }
    };

    // 1. Base PCB green
    for y in 16..262 {
        for x in 12..228 {
            if y < 28 && !(24..=216).contains(&x) {
                continue;
            }
            put(x, y, [0x1e, 0x54, 0x32]);
        }
    }

    // 2. Subtle PCB traces in top-left
    for y in 18..54 {
        for x in 28..115 {
            if (x + y) % 6 == 0 {
                put(x, y, [0x28, 0x6e, 0x40]);
            }
        }
    }

    // 3. Small SMT components on top-left
    for y in 24..28 {
        for x in 45..52 {
            put(x, y, [0x70, 0x65, 0x50]);
        }
    }
    for y in 34..38 {
        for x in 40..48 {
            put(x, y, [0x85, 0x78, 0x55]);
        }
    }
    for y in 44..50 {
        for x in 55..70 {
            put(x, y, [0x15, 0x15, 0x18]);
        }
    }
    for px in (56..69).step_by(3) {
        put(px, 43, [0xc0, 0xc0, 0xc8]);
        put(px, 50, [0xc0, 0xc0, 0xc8]);
    }

    // 4. Silver coin-cell battery (CR2025)
    let (cx, cy, r): (i32, i32, i32) = (158, 36, 19);
    for y in (cy - r - 2)..=(cy + r + 2) {
        for x in (cx - r - 2)..=(cx + r + 2) {
            let dx = x - cx;
            let dy = y - cy;
            let d_sq = dx * dx + dy * dy;
            if d_sq <= r * r {
                let grad = (210 - dx * 3 / 2 + dy).clamp(160, 235) as u8;
                put(
                    x as u32,
                    y as u32,
                    [grad, grad.saturating_add(2), grad.saturating_add(6)],
                );
            } else if d_sq <= (r + 1) * (r + 1) {
                put(x as u32, y as u32, [130, 132, 138]);
            }
        }
    }
    // Battery solder tab
    for y in 34..38 {
        for x in 115..158 {
            put(x, y, [190, 192, 198]);
        }
    }
    for y in 32..40 {
        for x in 112..118 {
            put(x, y, [210, 210, 218]);
        }
    }

    // 5. 32 Gold connector pins at bottom (y=224..258)
    for i in 0..32 {
        let px0 = (28.0 + i as f32 * 5.75).round() as u32;
        let px1 = px0 + 4;
        for y in 224..258 {
            let sh: u32 = if y > 226 && y < 255 { 210 } else { 170 };
            for x in px0..px1.min(228) {
                put(
                    x,
                    y,
                    [sh as u8, (sh * 82 / 100) as u8, (sh * 35 / 100) as u8],
                );
            }
            if y == 223 {
                for x in (px0 + 1)..px1.saturating_sub(1) {
                    put(x, y, [150, 120, 40]);
                }
            }
        }
    }

    rgba
}

fn shell_face_custom(shell: &Shell, w: u32, h: u32, depth: &[u8]) -> CartFace {
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    let edge = rim_colour(shell.colour);
    let board = match shell.finish {
        Finish::Translucent if w == GB_CART_W && h == GB_CART_H => Some(gbc_board()),
        _ => None,
    };

    for (i, d) in depth.iter().enumerate() {
        let plastic_base = match shell.finish {
            Finish::Solid => shell.colour,
            Finish::Translucent => lerp(edge, shell.colour, (*d as u32).min(RIM), RIM),
        };

        let c = if let Some(b) = board {
            let bi = i * 4;
            if b[bi + 3] > 0 {
                let p_rgb = [b[bi], b[bi + 1], b[bi + 2]];
                let rim_factor = (*d as u32).min(RIM);
                let opacity_num = 55 + (RIM - rim_factor) * 45 / RIM;
                lerp(p_rgb, plastic_base, opacity_num, 100)
            } else {
                plastic_base
            }
        } else {
            plastic_base
        };

        rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
    }
    CartFace { rgba, w, h }
}

/// Light through the plastic reads as a lighter, less saturated edge. Desaturating as well
/// as lightening is what keeps it from looking like a white outline drawn on the shell.
fn rim_colour(base: [u8; 3]) -> [u8; 3] {
    let mean = (base[0] as u16 + base[1] as u16 + base[2] as u16) / 3;
    base.map(|c| {
        let grey = (3 * c as u16 + mean) / 4;
        (grey + (255 - grey) * 2 / 5) as u8
    })
}

fn lerp(a: [u8; 3], b: [u8; 3], num: u32, den: u32) -> [u8; 3] {
    let mut out = [0u8; 3];
    for c in 0..3 {
        out[c] = ((a[c] as u32 * (den - num) + b[c] as u32 * num) / den) as u8;
    }
    out
}

/// The wall of the moulded recess the label sits in. Light comes from the upper left, so
/// the top and left walls are turned away from it and fall into shadow while the bottom and
/// right walls catch it. Painted before the label, so the label sits on the floor of the
/// recess with the wall showing around it.
const BEVEL: u32 = 3;

/// The grip ridge and the thumb notch, cut into the shell. Darkened rather than coloured:
/// moulded plastic is the same plastic, just turned away from the light.
fn mould_detail(face: &mut CartFace, shell: &Shell) {
    mould_detail_custom(face, shell, detail_mask());
}

fn mould_detail_custom(face: &mut CartFace, shell: &Shell, detail: &DetailMask) {
    let dark = [
        (shell.colour[0] as f32 * 0.60) as u8,
        (shell.colour[1] as f32 * 0.60) as u8,
        (shell.colour[2] as f32 * 0.60) as u8,
    ];
    let lit = [
        (shell.colour[0] as f32 * 1.35).min(255.0) as u8,
        (shell.colour[1] as f32 * 1.35).min(255.0) as u8,
        (shell.colour[2] as f32 * 1.35).min(255.0) as u8,
    ];
    for ((px, s), h) in face
        .rgba
        .chunks_exact_mut(4)
        .zip(&detail.shadow)
        .zip(&detail.highlight)
    {
        let sa = *s as u32;
        if sa > 0 {
            for c in 0..3 {
                px[c] = ((dark[c] as u32 * sa + px[c] as u32 * (255 - sa) + 127) / 255) as u8;
            }
        }
        let ha = *h as u32;
        if ha > 0 {
            for c in 0..3 {
                px[c] = ((lit[c] as u32 * ha + px[c] as u32 * (255 - ha) + 127) / 255) as u8;
            }
        }
    }
}

fn recess_label(face: &mut CartFace, shell: &Shell) {
    recess_label_custom(face, shell, LABEL_X, LABEL_Y, LABEL_W, LABEL_H);
}

fn recess_label_custom(face: &mut CartFace, shell: &Shell, lx: u32, ly: u32, lw: u32, lh: u32) {
    let shade = |c: [u8; 3], f: f32| -> [u8; 3] {
        [
            (c[0] as f32 * f).clamp(0.0, 255.0) as u8,
            (c[1] as f32 * f).clamp(0.0, 255.0) as u8,
            (c[2] as f32 * f).clamp(0.0, 255.0) as u8,
        ]
    };
    let dark = shade(shell.colour, 0.55);
    let lit = shade(shell.colour, 1.45);

    let (x0, y0) = (lx.saturating_sub(BEVEL), ly.saturating_sub(BEVEL));
    let (x1, y1) = (lx + lw + BEVEL, ly + lh + BEVEL);
    let mut put = |x: u32, y: u32, c: [u8; 3]| {
        if x >= face.w || y >= face.h {
            return;
        }
        let d = ((y * face.w + x) * 4) as usize;
        face.rgba[d] = c[0];
        face.rgba[d + 1] = c[1];
        face.rgba[d + 2] = c[2];
    };
    for y in y0..y1 {
        for x in x0..x1 {
            let inside = (lx..lx + lw).contains(&x) && (ly..ly + lh).contains(&y);
            if inside {
                continue;
            }
            // Which wall a pixel belongs to: the nearer of the two edges it sits between.
            let from_top = y.saturating_sub(y0);
            let from_left = x.saturating_sub(x0);
            let from_bottom = y1.saturating_sub(y + 1);
            let from_right = x1.saturating_sub(x + 1);
            let upper = from_top.min(from_left);
            let lower = from_bottom.min(from_right);
            put(x, y, if upper <= lower { dark } else { lit });
        }
    }
}

/// Source over, so a label with an alpha channel shows the shell through it rather than
/// punching a hole in the cart.
fn paste_label(face: &mut CartFace, label: &[u8]) {
    paste_label_custom(face, label, LABEL_X, LABEL_Y, LABEL_W, LABEL_H);
}

fn paste_label_custom(face: &mut CartFace, label: &[u8], lx: u32, ly: u32, lw: u32, lh: u32) {
    for y in 0..lh {
        for x in 0..lw {
            let s = ((y * lw + x) * 4) as usize;
            let a = label[s + 3] as u32;
            if a == 0 {
                continue;
            }
            let d = (((y + ly) * face.w + x + lx) * 4) as usize;
            for c in 0..3 {
                face.rgba[d + c] =
                    ((label[s + c] as u32 * a + face.rgba[d + c] as u32 * (255 - a) + 127) / 255)
                        as u8;
            }
        }
    }
}

fn generated_label(title: &str) -> Vec<u8> {
    let bg = label_colour(title);
    let mut rgba = Vec::with_capacity((LABEL_W * LABEL_H * 4) as usize);
    for _ in 0..LABEL_W * LABEL_H {
        rgba.extend_from_slice(&[bg[0], bg[1], bg[2], 255]);
    }

    if let Some(font) = text::label_font() {
        let layout = text::fit(
            font,
            title,
            (LABEL_W - 2 * PAD) as f32,
            MAX_LINES,
            MAX_PX,
            MIN_PX,
        );
        text::draw_centred(&mut rgba, LABEL_W, LABEL_H, &layout, ink(bg));
    }
    rgba
}

/// Hue rotation alone puts yellow and blue at very different luminance, so the ink flips
/// rather than sitting at one fixed value.
fn ink(bg: [u8; 3]) -> [u8; 3] {
    let luma = 0.2126 * bg[0] as f32 + 0.7152 * bg[1] as f32 + 0.0722 * bg[2] as f32;
    if luma > 140.0 {
        [0x1a, 0x18, 0x16]
    } else {
        [0xf4, 0xf1, 0xea]
    }
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    ]
}
