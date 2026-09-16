use std::path::{Path, PathBuf};

/// The eight top level folders of a content root. A card that has never held slot. has none
/// of them, and every write path below assumes its own is already there.
pub const DIRS: [&str; 8] = [
    "BIOS",
    "Cartridges",
    "Games",
    "Labels",
    "Saves",
    "States",
    "System",
    "Wallpapers",
];
const MIGRATION_MARKER: &str = "states-v2.checked";

/// Best effort: an unmounted or read only card is an empty shelf, not a boot failure.
pub fn ensure(root: &Path) {
    for sub in DIRS {
        let _ = std::fs::create_dir_all(root.join(sub));
    }
}

/// Bring a card written before states were namespaced up to the current layout. Best
/// effort on purpose: a read only or half mounted card is an empty shelf, not a boot
/// failure, exactly as `ensure` treats it.
///
/// A per-entry failure does not stop the sweep — the rest of the shelf still gets a chance —
/// but silently eating every one of them would leave a cart stuck pre-migration forever with
/// nothing on the card to say so. Logged here, once per boot, rather than inside
/// `migrate_states` itself, which only counts and has no read on where "once per boot" ends.
pub fn migrate(root: &Path) {
    if migration_marker_current(root) {
        return;
    }
    if let Ok(report) = slot_store::migrate_states(root) {
        if report.failed > 0 {
            eprintln!(
                "slot: migrate: {} of {} state director{} did not move",
                report.failed,
                report.moved + report.failed,
                if report.moved + report.failed == 1 {
                    "y"
                } else {
                    "ies"
                }
            );
        } else {
            // This marker avoids re-walking a large, already namespaced States tree on
            // every boot. It is derived metadata; a failed write merely costs a future
            // cheap check and never affects correctness.
            let marker = root.join("System").join(MIGRATION_MARKER);
            let _ = std::fs::write(marker, b"1\n");
        }
    }
}

fn migration_marker_current(root: &Path) -> bool {
    let marker = root.join("System").join(MIGRATION_MARKER);
    let states = root.join("States");
    let (Ok(marker), Ok(states)) = (std::fs::metadata(marker), std::fs::metadata(states)) else {
        return false;
    };
    let (Ok(marker), Ok(states)) = (marker.modified(), states.modified()) else {
        return false;
    };
    marker >= states
}

/// Reported to the core as the libretro system directory. `gba_bios.bin`, `gb_bios.bin`
/// and `gbc_bios.bin` present means that system's real BIOS and its boot logo; absent
/// means the core's own high-level BIOS. Neither is an error.
pub fn bios_dir(root: &Path) -> PathBuf {
    root.join("BIOS")
}

pub fn saves_dir(root: &Path) -> PathBuf {
    root.join("Saves")
}
