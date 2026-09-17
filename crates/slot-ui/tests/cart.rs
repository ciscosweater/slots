use slot_store::scan;
use slot_ui::{
    cart_face, clean_label, label_colour, label_panel, label_text, silhouette, CART_H, CART_W,
    LABEL_H, LABEL_W, LABEL_X, LABEL_Y, OUT_W,
};
use tempfile::TempDir;

fn tmp_root() -> TempDir {
    let d = tempfile::tempdir().expect("tempdir");
    for sub in [
        "Games",
        "Games/GB",
        "Cartridges",
        "Labels",
        "Saves",
        "States",
        "System",
    ] {
        std::fs::create_dir_all(d.path().join(sub)).expect("create content dir");
    }
    d
}

fn write_rom(d: &TempDir, name: &str, title: &str) {
    let mut rom = vec![0u8; 0x100];
    rom[0xa0..0xa0 + title.len()].copy_from_slice(title.as_bytes());
    std::fs::write(d.path().join("Games").join(name), rom).expect("write rom");
}

fn write_label(d: &TempDir, name: &str, w: u32, h: u32, px: impl Fn(u32, u32) -> [u8; 3]) {
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            rgb.extend_from_slice(&px(x, y));
        }
    }
    let f = std::fs::File::create(d.path().join("Labels").join(name)).expect("create label");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .expect("png header")
        .write_image_data(&rgb)
        .expect("png data");
}

fn write_artwork(d: &TempDir, name: &str, w: u32, h: u32, px: impl Fn(u32, u32) -> [u8; 3]) {
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            rgb.extend_from_slice(&px(x, y));
        }
    }
    let f = std::fs::File::create(d.path().join("Cartridges").join(name)).expect("create artwork");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .expect("png header")
        .write_image_data(&rgb)
        .expect("png data");
}

fn pixel(face: &slot_ui::CartFace, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * face.w + x) * 4) as usize;
    [face.rgba[i], face.rgba[i + 1], face.rgba[i + 2]]
}

/// The label sits inset in the shell, so a face pixel is only the label's business inside
/// this rect. Coordinates are relative to the label's own top left.
fn label_pixel(face: &slot_ui::CartFace, x: u32, y: u32) -> [u8; 3] {
    pixel(face, LABEL_X + x, LABEL_Y + y)
}

#[test]
fn a_malformed_label_falls_back_to_a_generated_one() {
    let d = tmp_root();
    write_rom(&d, "Broken.gba", "BROKEN");
    std::fs::write(d.path().join("Labels/Broken.png"), b"not a png").unwrap();
    let cart = &scan(d.path()).unwrap()[0];
    let face = cart_face(cart);
    assert_eq!((face.w, face.h), (CART_W, CART_H));
    assert!(face.rgba.iter().any(|b| *b != 0), "face is blank");
}

#[test]
fn complete_artwork_uses_slot_width_and_keeps_its_natural_height() {
    let d = tmp_root();
    write_rom(&d, "Ruby.gba", "RUBY");
    write_artwork(&d, "Ruby.png", 600, 355, |_, _| [0xd0, 0x20, 0xa0]);
    let cart = &scan(d.path()).unwrap()[0];
    let face = cart_face(cart);
    assert_eq!((face.w, face.h), (CART_W, 142));
    assert!(face
        .rgba
        .chunks_exact(4)
        .all(|px| px == [0xd0, 0x20, 0xa0, 255]));
}

#[test]
fn malformed_complete_artwork_falls_back_to_the_label() {
    let d = tmp_root();
    write_rom(&d, "Fallback.gba", "FALLBACK");
    std::fs::write(d.path().join("Cartridges/Fallback.png"), b"not a png").unwrap();
    write_label(&d, "Fallback.png", 64, 64, |_, _| [0x12, 0x34, 0x56]);
    let face = cart_face(&scan(d.path()).unwrap()[0]);
    assert_eq!(
        label_pixel(&face, LABEL_W / 2, LABEL_H / 2),
        [0x12, 0x34, 0x56]
    );
}

#[test]
fn label_colour_is_stable_across_calls() {
    assert_eq!(label_colour("POKEMON EMER"), label_colour("POKEMON EMER"));
    assert_ne!(label_colour("POKEMON EMER"), label_colour("ADVANCEWARS"));
}

#[test]
fn a_label_that_decodes_is_what_the_face_shows() {
    let d = tmp_root();
    write_rom(&d, "Labelled.gba", "LABELLED");
    write_label(&d, "Labelled.png", 64, 64, |_, _| [0xd0, 0x20, 0xa0]);
    let face = cart_face(&scan(d.path()).unwrap()[0]);
    assert_eq!((face.w, face.h), (CART_W, CART_H));
    for y in [0, LABEL_H / 2, LABEL_H - 1] {
        for x in [0, LABEL_W / 2, LABEL_W - 1] {
            assert_eq!(label_pixel(&face, x, y), [0xd0, 0x20, 0xa0], "at {x},{y}");
        }
    }
}

#[test]
fn a_label_of_the_wrong_aspect_is_cropped_not_squashed() {
    let d = tmp_root();
    write_rom(&d, "Tall.gba", "TALL");
    // Thirds: a squashed fit would drag red and blue into the label, a centre crop cannot.
    write_label(&d, "Tall.png", 160, 480, |_, y| match y / 160 {
        0 => [0xff, 0x00, 0x00],
        1 => [0xff, 0xff, 0xff],
        _ => [0x00, 0x00, 0xff],
    });
    let face = cart_face(&scan(d.path()).unwrap()[0]);
    for y in 0..LABEL_H {
        for x in 0..LABEL_W {
            assert_eq!(label_pixel(&face, x, y), [0xff, 0xff, 0xff], "at {x},{y}");
        }
    }
}

/// The second title is the one the landscape label made dangerous: three short words that
/// each fit the width at the largest size, three lines of which are taller than the label.
#[test]
fn a_long_title_stays_inside_the_label() {
    for stem in [
        "Supercalifragilisticexpialidocious Anniversary Edition",
        "Super Mario Kart",
    ] {
        let d = tmp_root();
        std::fs::write(d.path().join(format!("Games/{stem}.gba")), [0u8; 8]).unwrap();
        let face = cart_face(&scan(d.path()).unwrap()[0]);
        let bg = label_colour(stem);
        let margin = 5;
        for y in 0..LABEL_H {
            for x in 0..LABEL_W {
                let edge =
                    x < margin || y < margin || x >= LABEL_W - margin || y >= LABEL_H - margin;
                if edge {
                    assert_eq!(
                        label_pixel(&face, x, y),
                        bg,
                        "{stem}: text spills into the border at {x},{y}"
                    );
                }
            }
        }
        assert!(
            (0..LABEL_H * LABEL_W).any(|i| label_pixel(&face, i % LABEL_W, i / LABEL_W) != bg),
            "{stem}: no text was drawn"
        );
    }
}

#[test]
fn the_cart_box_matches_the_traced_outline() {
    let ratio = CART_W as f32 / CART_H as f32;
    assert!(
        (ratio - 1.778).abs() < 0.02,
        "aspect {ratio:.3}, the svg is being stretched"
    );
    assert_eq!(
        CART_W * 3,
        OUT_W,
        "three carts no longer span the shelf exactly"
    );
}

#[test]
fn game_boy_cart_uses_the_real_front_and_label_proportions() {
    let d = tmp_root();
    std::fs::write(d.path().join("Games/GB/Tetris.gb"), vec![0u8; 0x150]).unwrap();
    let face = cart_face(&scan(d.path()).unwrap()[0]);
    assert_eq!((face.w, face.h), (slot_ui::GB_CART_W, slot_ui::GB_CART_H));
    let cart_ratio = face.w as f32 / face.h as f32;
    assert!((cart_ratio - 57.0 / 65.0).abs() < 0.01);
    let label_ratio = slot_ui::GB_LABEL_W as f32 / slot_ui::GB_LABEL_H as f32;
    assert!((label_ratio - 42.0 / 37.0).abs() < 0.01);
}

/// The label is wide and low, but keeps the roughly 43:22 aspect of real GBA paper.
#[test]
fn the_label_is_wide_and_sits_low() {
    let (x0, y0, x1, y1) = label_panel(CART_W, CART_H);
    let (w, h) = (CART_W as f32, CART_H as f32);
    let side = x0 as f32 / w;
    let top = y0 as f32 / h;
    let bottom = 1.0 - y1 as f32 / h;
    let width = (x1 - x0) as f32 / w;
    let ratio = (x1 - x0) as f32 / (y1 - y0) as f32;

    // Wide, not an exact number. It still occupies most of the shell while retaining the
    // physical label's less panoramic proportions.
    assert!(
        width > 0.72,
        "the label is only {:.0}% of the cart wide, that reads as a panel not a label",
        width * 100.0
    );
    assert!(
        top > 0.18,
        "only {:.0}% of grip band above the label",
        top * 100.0
    );
    assert!(
        top > side * 1.4,
        "top band {top:.2} against side margin {side:.2}: the grip has disappeared"
    );
    assert!(
        top > bottom * 1.4,
        "the label is not sitting low, top {top:.2} bottom {bottom:.2}"
    );
    assert!(
        (ratio - 43.0 / 22.0).abs() < 0.02,
        "label ratio {ratio:.3} is not the physical GBA label's 43:22"
    );
}

/// The feature that makes the outline read as a GBA cart is not corner rounding, it is the
/// grip ears: the body is narrower than its top. An earlier version of this test asserted
/// the top corners were rounder than the bottom, which described a rounded rectangle I had
/// invented rather than the cartridge that was traced.
#[test]
fn the_body_is_narrower_than_its_grip_ears() {
    let m = silhouette(CART_W, CART_H);
    let solid = |x: u32, y: u32| m[(y * CART_W + x) as usize] > 128;
    let width_at = |y: u32| (0..CART_W).filter(|&x| solid(x, y)).count();

    let ears = width_at(CART_H / 12);
    let body = width_at(CART_H / 2);
    assert!(
        ears > body,
        "top spans {ears}px and the body {body}px: the grip ears are missing"
    );
    assert!(
        ears - body >= 3,
        "the ears stick out by only {}px, which will not read at all",
        ears - body
    );
    assert!(solid(CART_W / 2, CART_H / 2), "the middle is not solid");
    assert!(
        solid(CART_W / 2, CART_H - 2),
        "the bottom edge is not straight"
    );
}

#[test]
fn cart_faces_are_clipped_to_the_silhouette() {
    let d = tmp_root();
    write_rom(&d, "Emerald.gba", "EMERALD");
    let cart = &scan(d.path()).unwrap()[0];
    let f = cart_face(cart);
    let px = |x: u32, y: u32| f.rgba[((y * f.w + x) * 4 + 3) as usize];
    assert_eq!(
        px(2, 2),
        0,
        "the rounded corner is opaque, the mask was not applied"
    );
    assert!(px(f.w / 2, f.h / 2) > 250);
}

#[test]
fn a_supplied_label_is_clipped_to_the_silhouette_too() {
    let d = tmp_root();
    write_rom(&d, "Labelled.gba", "LABELLED");
    write_label(&d, "Labelled.png", LABEL_W, LABEL_H, |_, _| {
        [0x40, 0x80, 0xc0]
    });
    let f = cart_face(&scan(d.path()).unwrap()[0]);
    let px = |x: u32, y: u32| f.rgba[((y * f.w + x) * 4 + 3) as usize];
    assert_eq!(px(2, 2), 0, "the label escaped the rounded corner");
    assert!(px(f.w / 2, f.h / 2) > 250);
}

/// Every case here is a real filename from the development card.
#[test]
fn the_label_is_the_filename_without_its_tags() {
    let cases = [
        (
            "Pokemon - Emerald Version (USA, Europe)",
            "Pokemon Emerald Version",
        ),
        (
            "Pokemon - LeafGreen Version (USA, Europe) (Rev 1)",
            "Pokemon LeafGreen Version",
        ),
        ("Shrek (USA) (Rev 6)", "Shrek"),
        ("Metroid Fusion", "Metroid Fusion"),
        ("Button Test", "Button Test"),
        ("Pokemon - Corrupt", "Pokemon Corrupt"),
        ("Some Game [!]", "Some Game"),
        ("Spaced   Out  (USA)", "Spaced Out"),
    ];
    for (stem, want) in cases {
        assert_eq!(clean_label(stem), want, "for {stem}");
    }
}

/// A hyphen inside a word is part of the word.
#[test]
fn an_unspaced_hyphen_survives() {
    assert_eq!(clean_label("Spider-Man 2 (USA)"), "Spider-Man 2");
    assert_eq!(
        clean_label("Wario Land 4 - Time Attack"),
        "Wario Land 4 Time Attack"
    );
}

#[test]
fn a_name_that_is_all_tags_falls_back_rather_than_going_blank() {
    assert_eq!(clean_label("(USA) (Rev 1)"), "(USA) (Rev 1)");
    assert_eq!(clean_label(""), "");
}

/// The twelve character header title is what this task exists to stop using.
#[test]
fn the_header_title_no_longer_reaches_the_label() {
    let d = tmp_root();
    write_rom(
        &d,
        "Pokemon - Emerald Version (USA, Europe).gba",
        "POKEMON EMER",
    );
    let cart = &scan(d.path()).unwrap()[0];
    assert_eq!(label_text(cart), "Pokemon Emerald Version");
}

#[test]
fn two_regions_of_one_game_get_the_same_generated_colour() {
    let a = label_colour(&clean_label("Pokemon - Ruby Version (USA, Europe) (Rev 2)"));
    let b = label_colour(&clean_label("Pokemon - Ruby Version (Japan)"));
    assert_eq!(a, b, "the same game came out two colours");
}

#[test]
fn a_rom_with_no_header_title_is_labelled_from_its_stem() {
    let d = tmp_root();
    std::fs::write(d.path().join("Games/Homebrew Demo.gba"), [0u8; 8]).unwrap();
    let face = cart_face(&scan(d.path()).unwrap()[0]);
    let bg = label_colour("Homebrew Demo");
    assert!(
        (0..LABEL_H * LABEL_W).any(|i| label_pixel(&face, i % LABEL_W, i / LABEL_W) != bg),
        "an untitled rom got a blank label"
    );
}

/// A side cart is dimmed by sitting a translucent face on this, not by letting the ground
/// show through it. Only the compositor can mint a `TexId`, so what reaches the screen is not
/// reachable here; the shape and the colour are.
#[test]
fn the_cart_shadow_is_the_cart_in_black() {
    let s = slot_ui::cart_shadow();
    assert_eq!((s.w, s.h), (CART_W, CART_H));
    let mask = silhouette(CART_W, CART_H);
    for (px, cover) in s.rgba.chunks_exact(4).zip(&mask) {
        assert_eq!(&px[..3], &[0, 0, 0], "the shadow is not black");
        assert_eq!(px[3], *cover, "the shadow is not the cart's shape");
    }
    assert!(
        s.rgba.chunks_exact(4).any(|p| p[3] > 250),
        "the shadow is transparent everywhere, so it backs nothing"
    );
}

#[test]
fn game_boy_dmg_cart_has_lock_notch() {
    let s = slot_ui::cart_shadow_for(slot_store::Platform::Gb);
    assert_eq!((s.w, s.h), (slot_ui::GB_CART_W, slot_ui::GB_CART_H));
    let pixel_alpha = |x: u32, y: u32| s.rgba[((y * s.w + x) * 4 + 3) as usize];
    // In the notch area on top-right, alpha should be 0 (cutout)
    assert_eq!(pixel_alpha(230, 8), 0, "DMG lock notch is not cut out");
    // While the main top body is solid
    assert!(pixel_alpha(200, 8) > 250, "DMG top body is not solid");
    // Bottom corners are rounded
    assert_eq!(
        pixel_alpha(0, slot_ui::GB_CART_H - 1),
        0,
        "bottom-left corner is not rounded"
    );
}

#[test]
fn game_boy_color_cart_has_no_lock_notch() {
    let s = slot_ui::cart_shadow_for(slot_store::Platform::Gbc);
    assert_eq!((s.w, s.h), (slot_ui::GB_CART_W, slot_ui::GB_CART_H));
    let pixel_alpha = |x: u32, y: u32| s.rgba[((y * s.w + x) * 4 + 3) as usize];
    // On GBC, the top right is solid (no lock notch)
    assert!(pixel_alpha(225, 14) > 250, "GBC top right should be solid");
    // Center top rises high
    assert!(
        pixel_alpha(120, 2) > 250,
        "GBC center crown should be solid"
    );
}
