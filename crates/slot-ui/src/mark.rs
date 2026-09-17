use slot_gfx::OUT_W;
use slot_store::Platform;

use crate::art::render_svg;
use crate::hud::HUD_INK;
use crate::icon::{haloed, symbols_font, HALO_PX};
use crate::plate::UndoFace;
use crate::CartFace;

/// One per shelf, and the reason the carousel no longer has to say its shelf's name out loud:
/// the machine each shelf's cartridges were made for, drawn in the top plate's right corner
/// where a live session draws its link badge. Line art from the Noun Project under CC BY — each
/// file names its creator and `licenses/README.md` carries the attribution the stripped credit
/// line used to.
const GBA_SVG: &str = include_str!("../assets/platform_gba.svg");
const GB_SVG: &str = include_str!("../assets/platform_gb.svg");
const GBC_SVG: &str = include_str!("../assets/platform_gbc.svg");

/// How tall a mark is drawn, in offscreen pixels.
///
/// It was 24 on the case band and 32 in the corner, and both came back off the device as too
/// small — the second time with the corner itself called too tight as well. Rendered at 32, 36,
/// 40, 44 and 48 into the real shelf frame and looked at: 36 is not far enough from 32 to be
/// worth the move, 40 reads cleanly, 44 is confident, and 48 is the largest that is still an
/// annotation rather than a subject — beside a cart's own face it stays plainly the smaller
/// thing. 48 is what a third pass on "still too small" should be: 1.5 times the last size and
/// twice the first. It is also the size the physical panel argues for, since 480 px over a 53 mm
/// screen is about 9 px to the millimetre and 48 px is a 5.3 mm badge in the hand.
///
/// Multiples of 8 only, so `MARK_W` stays exact: `render_svg` scales x and y independently, and
/// a height that leaves `MARK_H * 5 / 8` with a remainder squeezes every drawing by the rounding.
pub const MARK_H: u32 = 48;

/// Category tab chips in the top-right. Source art is 21×16; drawn at 2× so they read beside
/// the ALL / REC glyphs instead of as dust in the corner.
const TAB_SRC_W: u32 = 21;
const TAB_SRC_H: u32 = 16;
const TAB_SCALE: u32 = 2;
const TAB_MARK_W: u32 = TAB_SRC_W * TAB_SCALE;
const TAB_MARK_H: u32 = TAB_SRC_H * TAB_SCALE;
const TAB_GLYPH_PX: f32 = 28.0;

const TAB_GBA_PNG: &[u8] = include_bytes!("../assets/tab_gba.png");
const TAB_GB_PNG: &[u8] = include_bytes!("../assets/tab_gb.png");
const TAB_GBC_PNG: &[u8] = include_bytes!("../assets/tab_gbc.png");

/// How far the mark's box is held off the right edge and the top of the screen.
///
/// Shared by the category tab row so the icons and the clock share one right edge.
///
/// The right margin is the case's own, the one `draw_footer` prints the gauge and the clock at,
/// rather than the 12 px `badge_at` holds the link badge off at. That is the whole of what made
/// this read as tucked into the corner: the mark sat half as far in as the clock directly below
/// it, and two things at the same end of the screen inset by different amounts read as one of
/// them being wrong. They now share an edge — the clock's type and the mark's ink both stop
/// 24 px short, give or take the halo's pixel.
///
/// The top margin has nothing above it to line up with, so it is the clock's own air read the
/// other way up: the band the footer prints its type in stops 17 px short of the bottom edge,
/// and 16 px of box plus the halo puts a mark's first lit pixel 17 px down from the top. Off the
/// rendered frame the two corners come out 17 above and 22 below, since the digits do not fill
/// their band — near enough that the corners read as a pair. Rendered at 8, 12, 16 and 20 and
/// looked at: 8 is still the complaint, 12 is comfortable, 16 is plainly deliberate, and by 20
/// the mark has come away from its corner and floats.
pub const MARK_MARGIN: f32 = 24.0;
const MARK_TOP: f32 = 16.0;

/// Where a mark of this size goes. Its own rule rather than `badge_at`, which it used to share.
///
/// The two are still the same idea — one corner, saying the single thing worth knowing about
/// where you are — but they are measured against different things, and at this size the shared
/// rule could no longer hold both. A badge is drawn inside the HUD's plate, only ever over a
/// running game, and 12 px in and centred in 40 px of depth is the plate's own margin. A mark is
/// drawn on the carousel, where there is no plate at all: the frame it is read against is the
/// screen, and everything else the screen prints — the gauge, the clock — is 24 px in.
///
/// That is also why the size is not bounded by the plate any more. Centred in 40 px the mark's
/// air is `(40 - box) / 2`, so every pixel of growth costs half a pixel of padding at each end,
/// and "bigger, with more room above it" cannot both be had: at today's 50 px box it is already
/// 5 px of overhang before any padding, and 16 px of air inside the plate would leave room for a
/// 6 px machine. So the mark keeps to the screen's margins and the plate, when the HUD raises
/// one, passes behind it. Over the shelf's black backdrop nothing shows; over a wallpaper the
/// plate's lower edge crosses the drawing, for the second and a half the bar is up.
///
/// Width alone, where `badge_at` takes both: a badge is centred in the plate and so has to know
/// how tall it is, and a mark is hung from the top of the screen and does not.
pub fn mark_at(w: f32) -> (f32, f32) {
    (OUT_W as f32 - MARK_MARGIN - w, MARK_TOP)
}

/// What the coverage is raised to before it is tinted, which is the whole of the fix for marks
/// that came off the device looking like a different, dimmer class of thing than the type beside
/// them. The ink was never the problem — it is `HUD_INK`, the same near-white the percent and the
/// clock are set in. The problem is that these are line drawings whose strokes are around 2 units
/// of a 100-unit box: even at 32 px that is two thirds of a pixel, so the rasteriser reports two
/// thirds coverage and two thirds of near-white on a dark ground is grey.
///
/// Raising the coverage is the same trick a font rasteriser's stem darkening is, and it is honest
/// for the same reason: the line is really there, the pixel grid is what cannot hold it. Solid
/// areas are untouched (1 to any power is 1) and so is empty space, so only the thin work moves.
///
/// 0.5 by eye against 1.0, 0.6, 0.45 and 0.4 at 32 px, magnified ten times: over the lit pixels
/// of a mark it takes the mean from 135 to 165 against a line of HUD type's own 189, which is
/// most of the gap closed while leaving a mark reading a shade under a glyph — which is what it
/// is. Below 0.45 the boost starts to be a defect rather than a correction: the DMG's screen
/// bezel begins to merge into its body and the Colour's buttons run together.
const INK_GAMMA: f32 = 0.5;

/// 5 to 8, which is what all three drawings' viewBoxes were re-fitted to. Their machines are
/// not the same shape — a DMG is 0.622 wide for its height, an SP 0.563, a Colour 0.598 — so
/// each viewBox was widened about the drawing's own centre to the widest of the three, rounded
/// to 5:8 so the box is whole pixels. That is what lets one box hold all three: every mark
/// draws at one height, in its own proportions, and ringing the shoulders through the shelves
/// moves nothing else in the corner.
pub const MARK_W: u32 = MARK_H * 5 / 8;

/// The box a mark is rastered into, halo included, for callers placing one in the corner before
/// they know which shelf is showing.
pub fn mark_box() -> (u32, u32) {
    (MARK_W + 2 * HALO_PX, MARK_H + 2 * HALO_PX)
}

/// The mark for a shelf, tinted and haloed exactly as the link badge that shares its corner is.
/// The drawings are black on nothing, so what is kept from the raster is its coverage alone —
/// boosted by `INK_GAMMA`, since sub-pixel line work under-reports itself — and the HUD's own ink
/// is put through it. The same two steps `icon_face` takes, for the same reason: what a mark is
/// drawn over is dark, and a mark in the artist's black would be a hole in it.
///
/// One ink for all three, deliberately. A per-platform tint was built and rendered beside this
/// one — the Advance's indigo, the DMG's pea green, the Colour's berry — and at real size it is
/// three saturated chips in a corner of a bar whose whole job is to be ignorable, with the indigo
/// coming out darker than the grey it was meant to replace. The plate is monochrome and this
/// stays monochrome with it.
pub fn mark_face(platform: Platform) -> CartFace {
    platform_face(platform, MARK_W, MARK_H)
}

/// Shelf category tabs: ALL / REC as symbols, GBA / GB / GBC as the coloured pixel chips.
pub fn category_tab_face(category: usize) -> UndoFace {
    let face = match category {
        0 => glyph_tab('\u{f00a}'), // th / grid — the whole library
        1 => glyph_tab('\u{f1da}'), // history — recents
        2 => tab_png(TAB_GBA_PNG),
        3 => tab_png(TAB_GB_PNG),
        4 => tab_png(TAB_GBC_PNG),
        _ => CartFace {
            rgba: Vec::new(),
            w: 0,
            h: 0,
        },
    };
    UndoFace {
        rgba: face.rgba,
        w: face.w,
        h: face.h,
    }
}

/// Decode a tab chip, scale it up by nearest neighbour, and keep the artist's colours. These
/// are not the monochrome shelf marks — they only live in the category row.
fn tab_png(bytes: &[u8]) -> CartFace {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::normalize_to_color8());
    let Ok(mut reader) = dec.read_info() else {
        return CartFace {
            rgba: Vec::new(),
            w: 0,
            h: 0,
        };
    };
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let Ok(info) = reader.next_frame(&mut buf) else {
        return CartFace {
            rgba: Vec::new(),
            w: 0,
            h: 0,
        };
    };
    if info.width != TAB_SRC_W || info.height != TAB_SRC_H {
        return CartFace {
            rgba: Vec::new(),
            w: 0,
            h: 0,
        };
    }
    let src = &buf[..info.buffer_size()];
    let n = (TAB_SRC_W * TAB_SRC_H) as usize;
    let mut rgba = vec![0u8; n * 4];
    match info.color_type {
        png::ColorType::Rgba => rgba.copy_from_slice(src),
        png::ColorType::Rgb => {
            for i in 0..n {
                rgba[i * 4..i * 4 + 3].copy_from_slice(&src[i * 3..i * 3 + 3]);
                rgba[i * 4 + 3] = 255;
            }
        }
        _ => {
            return CartFace {
                rgba: Vec::new(),
                w: 0,
                h: 0,
            }
        }
    }
    let (rgba, w, h) = scale_nearest(&rgba, TAB_SRC_W, TAB_SRC_H, TAB_SCALE);
    if w != TAB_MARK_W || h != TAB_MARK_H {
        return CartFace {
            rgba: Vec::new(),
            w: 0,
            h: 0,
        };
    }
    CartFace {
        rgba,
        w: TAB_MARK_W,
        h: TAB_MARK_H,
    }
}

/// Integer nearest-neighbour scale. Pixel chips stay sharp; bilinear would smear them.
fn scale_nearest(src: &[u8], sw: u32, sh: u32, scale: u32) -> (Vec<u8>, u32, u32) {
    let dw = sw * scale;
    let dh = sh * scale;
    let mut out = vec![0u8; (dw * dh * 4) as usize];
    for y in 0..dh {
        for x in 0..dw {
            let si = (((y / scale) * sw + (x / scale)) * 4) as usize;
            let di = ((y * dw + x) * 4) as usize;
            out[di..di + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
    (out, dw, dh)
}

fn platform_face(platform: Platform, w: u32, h: u32) -> CartFace {
    let svg = match platform {
        Platform::Gba => GBA_SVG,
        Platform::Gb => GB_SVG,
        Platform::Gbc => GBC_SVG,
    };
    let Some(rgba) = render_svg(svg, w, h) else {
        return CartFace {
            rgba: Vec::new(),
            w: 0,
            h: 0,
        };
    };
    let cov: Vec<u8> = rgba.chunks_exact(4).map(|px| boosted(px[3])).collect();
    haloed(&cov, w, h, HUD_INK)
}

fn glyph_tab(glyph: char) -> CartFace {
    let Some(font) = symbols_font() else {
        return CartFace {
            rgba: Vec::new(),
            w: 0,
            h: 0,
        };
    };
    let (m, cov) = font.rasterize(glyph, TAB_GLYPH_PX);
    if m.width == 0 || m.height == 0 {
        return CartFace {
            rgba: Vec::new(),
            w: 0,
            h: 0,
        };
    }
    haloed(&cov, m.width as u32, m.height as u32, HUD_INK)
}

/// One coverage byte through `INK_GAMMA`. Kept apart from the loop so a test can hold the curve
/// itself to its two fixed points rather than inferring them from a rastered machine.
fn boosted(cov: u8) -> u8 {
    if cov == 0 || cov == u8::MAX {
        return cov;
    }
    let a = f32::from(cov) / 255.0;
    (a.powf(INK_GAMMA) * 255.0).round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every shelf gets a mark, every mark lands in the one box, and every one of them has ink
    /// in it. A drawing that failed to parse comes back as an empty face rather than as a
    /// panic, which on the band would be a shelf that silently stopped saying what it was.
    #[test]
    fn every_shelf_has_a_mark_and_they_all_share_one_box() {
        let (w, h) = mark_box();
        for p in Platform::ALL {
            let face = mark_face(p);
            assert_eq!(
                (face.w, face.h),
                (w, h),
                "{p:?}'s mark is not the size the band reserves for it"
            );
            let inked = face.rgba.chunks_exact(4).filter(|px| px[3] > 0).count();
            assert!(inked > 0, "{p:?}'s mark rastered to nothing at all");
        }
    }

    #[test]
    fn every_category_tab_has_ink() {
        for category in 0..5 {
            let face = category_tab_face(category);
            assert!(face.w > 0 && face.h > 0, "category {category} has no box");
            let inked = face.rgba.chunks_exact(4).filter(|px| px[3] > 0).count();
            assert!(inked > 0, "category {category} rastered to nothing");
        }
    }

    /// The box is the 5:8 the three viewBoxes were re-fitted to, exactly, with no rounding left
    /// over. `render_svg` scales x and y independently, so a height that leaves `MARK_H * 5 / 8`
    /// with a remainder does not letterbox the drawing — it squeezes it, by up to a pixel, in
    /// every mark at once. Nothing about that would look broken on screen or fail a test; the
    /// machines would simply come out a little narrower than they were drawn.
    #[test]
    fn the_box_is_whole_pixels_of_the_ratio_the_drawings_were_fitted_to() {
        assert_eq!(
            MARK_W * 8,
            MARK_H * 5,
            "a {MARK_W}x{MARK_H} mark is not 5:8: pick a height that is a multiple of 8"
        );
        assert_eq!(
            (TAB_MARK_W, TAB_MARK_H),
            (42, 32),
            "console tab chips draw at 2× the 21×16 source"
        );
        for category in 2..=4 {
            let face = category_tab_face(category);
            assert_eq!(
                (face.w, face.h),
                (TAB_MARK_W, TAB_MARK_H),
                "category {category} is not the chip size"
            );
        }
        let chips: Vec<_> = (2..=4).map(category_tab_face).collect();
        assert_ne!(chips[0].rgba, chips[1].rgba);
        assert_ne!(chips[0].rgba, chips[2].rgba);
        assert_ne!(chips[1].rgba, chips[2].rgba);
    }

    /// The three are different pictures. Cheap to get wrong — the three files are one paste
    /// apart — and on the band it would look exactly like a mark that never changes, which is
    /// the one thing this feature has to not do.
    #[test]
    fn no_two_shelves_show_the_same_mark() {
        let faces: Vec<CartFace> = Platform::ALL.iter().map(|p| mark_face(*p)).collect();
        for (i, a) in faces.iter().enumerate() {
            for (j, b) in faces.iter().enumerate().skip(i + 1) {
                assert_ne!(
                    a.rgba,
                    b.rgba,
                    "{:?} and {:?} draw the same mark",
                    Platform::ALL[i],
                    Platform::ALL[j]
                );
            }
        }
    }

    /// The curve's two fixed points, which are the whole of what makes it a correction rather
    /// than a wash: nothing that the rasteriser said was empty gains ink, nothing it said was
    /// solid can gain any more, and every partial pixel in between comes up. A gamma above 1,
    /// or an off-by-one that inverted it, would darken exactly the line work this exists to
    /// lift and would still pass every other test in this file.
    #[test]
    fn the_boost_lifts_the_antialiased_middle_and_leaves_both_ends_alone() {
        assert_eq!(boosted(0), 0, "empty space gained ink");
        assert_eq!(boosted(255), 255, "solid ink was pushed past solid");
        for cov in 1..255u8 {
            assert!(
                boosted(cov) >= cov,
                "{cov} coverage came back as {}, darker than it went in",
                boosted(cov)
            );
        }
        // And actually moved, rather than merely not gone backwards. The last few steps are
        // excluded because they cannot move: the curve owes 254 half a level and rounding takes
        // it back, which is arithmetic rather than a mark that failed to brighten.
        for cov in 1..=250u8 {
            assert!(
                boosted(cov) > cov,
                "{cov} coverage came back as {}, no brighter",
                boosted(cov)
            );
        }
    }

    /// And what that does to a real drawing: every mark comes off the rasteriser brighter than
    /// it went in, without the thin work closing up into a block. The ceiling is the honest half
    /// of this — the failure a lower gamma would produce is not a dim mark but a solid one, and
    /// a test that only checked for "brighter" would wave that through.
    #[test]
    fn every_mark_comes_up_brighter_without_filling_its_box() {
        for p in Platform::ALL {
            let raw = render_svg(svg_of(p), MARK_W, MARK_H).expect("the drawing rasterises");
            let raw: Vec<u8> = raw.chunks_exact(4).map(|px| px[3]).collect();
            let lit: Vec<usize> = (0..raw.len()).filter(|i| raw[*i] > 0).collect();
            let mean = |c: &[u8]| -> f32 {
                lit.iter().map(|i| f32::from(c[*i])).sum::<f32>() / lit.len() as f32
            };
            let boost: Vec<u8> = raw.iter().map(|c| boosted(*c)).collect();
            let (before, after) = (mean(&raw), mean(&boost));
            assert!(
                after > before + 20.0,
                "{p:?} came up at {after:.0} against {before:.0}: the boost is not doing \
                 enough to be worth having"
            );
            let solid = boost.iter().filter(|c| **c == 255).count();
            assert!(
                solid < boost.len() / 2,
                "{p:?} is {solid} solid pixels of {}: the drawing has flooded rather than \
                 firmed up",
                boost.len()
            );
        }
    }

    fn svg_of(p: Platform) -> &'static str {
        match p {
            Platform::Gba => GBA_SVG,
            Platform::Gb => GB_SVG,
            Platform::Gbc => GBC_SVG,
        }
    }

    /// The credit the free download baked in sat below the drawing, at y 115 and y 120 of a
    /// 125-unit box that the drawing itself only reached y 100 of. Strip the type without
    /// re-fitting the box and every mark renders squashed into its top four fifths with a band
    /// of nothing under it — which is not a crash, not a test failure, and plainly wrong on
    /// screen. So the bottom row of each drawing is required to carry ink: it can only do that
    /// if the box ends where the machine does.
    #[test]
    fn the_box_was_re_fitted_to_the_drawing_and_not_left_holding_the_credit() {
        for p in Platform::ALL {
            let face = mark_face(p);
            let pad = HALO_PX;
            let row = |y: u32| {
                (pad..pad + MARK_W).any(|x| face.rgba[((y * face.w + x) * 4 + 3) as usize] > 0)
            };
            assert!(row(pad), "{p:?} is not drawn to the top of its box");
            assert!(
                row(pad + MARK_H - 1),
                "{p:?} stops short of the bottom of its box: the viewBox still holds the \
                 credit line the drawing was stripped of"
            );
        }
    }
}
