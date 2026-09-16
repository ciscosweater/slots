use std::path::PathBuf;
use std::time::Instant;

use slot::frontend::Frontend;
use slot::input::DeviceInput;
use slot::timing::{PANEL_FRAME, PANEL_HZ};
use slot_gfx::{Compositor, FbdevSurface, Surface};
use slot_power::DevicePlatform;

/// Where BaseOS mounts the card slot has never been checked against a running device, so
/// `launch.sh` exports `SLOT_ROOT` and this is only what is left if it did not.
const CARD: &str = "/mnt/sdcard";

pub fn run() {
    let boot_started = Instant::now();
    let root = std::env::var_os("SLOT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(CARD));
    let mut surface = match FbdevSurface::new() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("slot: {e}");
            return;
        }
    };
    eprintln!(
        "slot: boot stage=surface elapsed_ms={}",
        boot_started.elapsed().as_secs_f64() * 1000.0
    );
    let mut compositor = match Compositor::new(&surface) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("slot: {e}");
            return;
        }
    };
    eprintln!(
        "slot: boot stage=compositor elapsed_ms={}",
        boot_started.elapsed().as_secs_f64() * 1000.0
    );
    let platform = DevicePlatform::new(root.clone());
    eprintln!("slot: {}", platform.report());
    platform.trace_boot();
    let mut frontend = Frontend::boot(Box::new(platform));
    eprintln!(
        "slot: boot stage=state elapsed_ms={}",
        boot_started.elapsed().as_secs_f64() * 1000.0
    );
    let mut input = DeviceInput::open(&root);
    eprintln!("slot: panel clock {PANEL_HZ:.3} Hz");
    let vsync = surface.vsync_active();
    let mut deadline = Instant::now();
    let mut asset_stage = 0u8;
    loop {
        if !frontend.standby() {
            frontend.render(&mut compositor, surface.window_size());
            if let Err(e) = surface.swap() {
                eprintln!("slot: {e}");
                return;
            }
            match asset_stage {
                0 => {
                    eprintln!(
                        "slot: boot stage=first_frame elapsed_ms={}",
                        boot_started.elapsed().as_secs_f64() * 1000.0
                    );
                    asset_stage = 1;
                }
                1 if frontend.upload_next_static_faces(&mut compositor) => {
                    // Hydrate one static batch per frame. Once those are in place, the shelf-face
                    // worker feeds one completed cart at a time without stopping the spring clock.
                    if frontend.upload_next_cart_face(&mut compositor) {
                        asset_stage = 2;
                        eprintln!(
                            "slot: boot stage=assets elapsed_ms={}",
                            boot_started.elapsed().as_secs_f64() * 1000.0
                        );
                    }
                }
                1 => {}
                _ => {}
            }
        }
        frontend.advance(&mut input);
        // NextUI's core and swap live on one thread. This acknowledgement releases one core
        // frame and waits until it has been published, preserving that ordering without
        // giving up the worker isolation that keeps audio stable.
        if !frontend.standby() {
            frontend.presented();
        }
        if frontend.restarting() {
            frontend.restart();
        }
        if frontend.powering_off() {
            frontend.poweroff();
            return;
        }
        if frontend.standby() {
            std::thread::sleep(std::time::Duration::from_millis(200));
            deadline = Instant::now();
            continue;
        }
        // eglSwapBuffers is the primary clock.  The absolute deadline is the fallback for
        // Mali drivers that accept swap interval 1 but do not actually block on it; unlike a
        // per-frame elapsed sleep it does not accumulate scheduler error.
        if !vsync {
            deadline += PANEL_FRAME;
            let now = Instant::now();
            if let Some(left) = deadline.checked_duration_since(now) {
                std::thread::sleep(left);
            } else if now.duration_since(deadline) > PANEL_FRAME {
                deadline = now;
            }
        }
    }
}
