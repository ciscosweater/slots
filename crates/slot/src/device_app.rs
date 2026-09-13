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
    let mut compositor = match Compositor::new(&surface) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("slot: {e}");
            return;
        }
    };
    let platform = DevicePlatform::new(root.clone());
    eprintln!("slot: {}", platform.report());
    platform.trace_boot();
    let mut frontend = Frontend::boot(Box::new(platform));
    frontend.upload_faces(&mut compositor);
    let mut input = DeviceInput::open(&root);
    eprintln!("slot: panel clock {PANEL_HZ:.3} Hz");
    let mut deadline = Instant::now();
    loop {
        frontend.render(&mut compositor, surface.window_size());
        if let Err(e) = surface.swap() {
            eprintln!("slot: {e}");
            return;
        }
        frontend.advance(&mut input);
        if frontend.restarting() {
            frontend.restart();
        }
        if frontend.powering_off() {
            frontend.poweroff();
            return;
        }
        // eglSwapBuffers is the primary clock.  The absolute deadline is the fallback for
        // Mali drivers that accept swap interval 1 but do not actually block on it; unlike a
        // per-frame elapsed sleep it does not accumulate scheduler error.
        deadline += PANEL_FRAME;
        let now = Instant::now();
        if let Some(left) = deadline.checked_duration_since(now) {
            std::thread::sleep(left);
        } else if now.duration_since(deadline) > PANEL_FRAME {
            deadline = now;
        }
    }
}
