//! Rasterises GBA, GB and GBC cart faces to PNGs so the silhouettes can be looked at.
//!
//! Does nothing unless `SCRATCH_DIR` names an output directory:
//!
//! `SCRATCH_DIR=/tmp/carts cargo test -p slot-ui --test render_cart -- --nocapture`

use std::path::Path;

use slot_store::Cart;
use slot_ui::cart_face;

fn save_png(path: &Path, rgba: &[u8], w: u32, h: u32) {
    let f = std::fs::File::create(path).unwrap();
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(rgba).unwrap();
}

fn cart(platform: slot_store::Platform, rom: &str, stem: &str, title: &str, code: &str) -> Cart {
    Cart {
        platform,
        rom: rom.into(),
        stem: stem.into(),
        title: title.into(),
        code: code.into(),
        artwork: None,
        label: None,
    }
}

#[test]
fn render_cartridges() {
    let Ok(out) = std::env::var("SCRATCH_DIR") else {
        return;
    };
    std::fs::create_dir_all(&out).unwrap();
    let out = Path::new(&out);

    let faces = [
        (
            "cart_gba.png",
            cart(
                slot_store::Platform::Gba,
                "Pokemon - Emerald Version (USA, Europe).gba",
                "Pokemon - Emerald Version",
                "POKEMON EMER",
                "BPEE",
            ),
        ),
        (
            "cart_gb.png",
            cart(
                slot_store::Platform::Gb,
                "Pokemon - Red Version (USA, Europe).gb",
                "Pokemon - Red Version",
                "POKEMON RED",
                "APAE",
            ),
        ),
        (
            "cart_gbc.png",
            cart(
                slot_store::Platform::Gbc,
                "Pokemon - Crystal Version (USA, Europe).gbc",
                "Pokemon - Crystal Version",
                "POKEMON CRYS",
                "BYTE",
            ),
        ),
    ];

    for (name, cart) in faces {
        let face = cart_face(&cart);
        let path = out.join(name);
        save_png(&path, &face.rgba, face.w, face.h);
        println!("wrote {}", path.display());
    }
}
