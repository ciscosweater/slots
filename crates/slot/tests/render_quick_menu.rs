//! The quick menu through the real frontend: faces uploaded at boot, MENU pressed through the
//! gesture layer, and the frame composited on the GPU and read back.
//!
//! `SCRATCH_PNG_DIR=/tmp cargo test -p slot --test render_quick_menu -- --nocapture`

#![cfg(target_os = "macos")]

mod common;

use std::collections::VecDeque;

use common::{clocked, tmp_root_with_carts};
use slot::frontend::Frontend;
use slot_gfx::{Compositor, HeadlessSurface, OUT_H, OUT_W};
use slot_input::{Btn, InputSource, Millis, RawEvent};
use slot_power::SimPlatform;
use slot_ui::{QuickRow, QUICK_PITCH, QUICK_TOP};

/// One batch of events per poll, and nothing once they run out.
struct Script(VecDeque<Vec<RawEvent>>);

impl InputSource for Script {
    fn poll(&mut self, _now: Millis) -> Vec<RawEvent> {
        self.0.pop_front().unwrap_or_default()
    }
}

/// A tap: down on one frame, up on the next.
fn tap(frontend: &mut Frontend, input: &mut Script, button: Btn) {
    input.0.push_back(vec![RawEvent::Down(button)]);
    frontend.advance(input);
    input.0.push_back(vec![RawEvent::Up(button)]);
    frontend.advance(input);
}

fn at(pixels: &[u8], x: usize, y: usize) -> [u8; 3] {
    let offset = (y * OUT_W as usize + x) * 4;
    [pixels[offset], pixels[offset + 1], pixels[offset + 2]]
}

/// The columns in `xs` with type in them anywhere across a row's bar: brighter than both the
/// ground and the bar, which grey values are as well.
fn inked(pixels: &[u8], xs: std::ops::Range<usize>, top: usize) -> Vec<usize> {
    xs.filter(|&x| (top + 8..top + 44).any(|y| at(pixels, x, y)[0] > 0x80))
        .collect()
}

fn composed(frontend: &mut Frontend, compositor: &mut Compositor, name: &str) -> Vec<u8> {
    frontend.compose(compositor);
    let pixels = compositor.read_frame();
    if let Ok(dir) = std::env::var("SCRATCH_PNG_DIR") {
        let path = format!("{dir}/quick-menu-{name}.png");
        let file = std::fs::File::create(&path).expect("create png");
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), OUT_W, OUT_H);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .expect("png header")
            .write_image_data(&pixels)
            .expect("png data");
        println!("wrote {path}");
    }
    pixels
}

#[test]
fn the_quick_menu_renders_full_screen() {
    let Ok(surface) = HeadlessSurface::new() else {
        return;
    };
    let Ok(mut compositor) = Compositor::new(&surface) else {
        return;
    };
    let root = tmp_root_with_carts(&["Emerald", "Fusion"]);
    clocked(root.path());
    let mut frontend = Frontend::boot(Box::new(SimPlatform::at(root.path().to_path_buf())));
    frontend.upload_faces(&mut compositor);
    let mut input = Script(VecDeque::new());
    tap(&mut frontend, &mut input, Btn::Menu);

    let bar = [0x4d, 0x4d, 0x57];
    let ground = [0x05, 0x05, 0x08];
    // Down presses from wherever the bar was before.
    for (name, downs, selected) in [
        ("fast-forward", 0, QuickRow::FastForward),
        ("date-time", 3, QuickRow::DateTime),
        ("about", 1, QuickRow::About),
    ] {
        for _ in 0..downs {
            tap(&mut frontend, &mut input, Btn::Down);
        }
        let pixels = composed(&mut frontend, &mut compositor, name);
        let top = (QUICK_TOP + QUICK_PITCH * selected.index() as f32) as usize;
        for x in [1, 360, OUT_W as usize - 2] {
            assert_eq!(at(&pixels, x, top + 26), bar, "{name}: no bar at x {x}");
        }
        assert_eq!(
            at(&pixels, 360, top + 1),
            ground,
            "{name}: the bar is not inset"
        );
        assert_eq!(
            at(&pixels, 2, OUT_H as usize - 2),
            ground,
            "{name}: not on the ground"
        );

        // Labels start 32 px in and values end 32 px from the right, on every row, measured the
        // way the mockup's type is placed: by where its line starts and ends, not by its ink.
        for row in QuickRow::ALL {
            let top = (QUICK_TOP + QUICK_PITCH * row.index() as f32) as usize;
            let label = inked(&pixels, 0..360, top);
            let first = *label.first().expect("a row with no label");
            assert!(
                (32..=36).contains(&first),
                "{name}: {row:?}'s label starts at x {first}"
            );
            if row == QuickRow::About {
                continue;
            }
            let value = inked(&pixels, 360..OUT_W as usize, top);
            let last = *value.last().expect("a row with no value");
            assert!(
                (679..=688).contains(&last),
                "{name}: {row:?}'s value ends at x {last}"
            );
        }
    }

    // Date & Time opens the clock with B BACK beside its own key. Centred as a pair, B's cap is
    // on the left; the first-boot clock's lone key is centred on its own and starts much farther
    // right.
    tap(&mut frontend, &mut input, Btn::Up);
    tap(&mut frontend, &mut input, Btn::A);
    let pixels = composed(&mut frontend, &mut compositor, "clock");
    assert!(
        (200..270).any(|x| at(&pixels, x, 298) == [0xf6, 0xf4, 0xef]),
        "the clock from the menu does not offer B BACK"
    );
}
