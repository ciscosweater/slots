use std::path::{Path, PathBuf};

/// The content-root folders. Platform subdirectories are created up front for hand-copied ROMs,
/// cartridge artwork and labels; saves and states are created lazily by their writers.
pub const DIRS: [&str; 17] = [
    "BIOS",
    "Cartridges",
    "Cartridges/GBA",
    "Cartridges/GB",
    "Cartridges/GBC",
    "Games",
    "Games/GBA",
    "Games/GB",
    "Games/GBC",
    "Labels",
    "Labels/GBA",
    "Labels/GB",
    "Labels/GBC",
    "Saves",
    "States",
    "System",
    "Wallpapers",
];

/// Best effort: an unmounted or read only card is an empty shelf, not a boot failure.
pub fn ensure(root: &Path) {
    for sub in DIRS {
        let _ = std::fs::create_dir_all(root.join(sub));
    }
}

/// Bring a card through both historical layouts. State-core migration must run first: otherwise a
/// pre-platform `States/<core>/` directory would be mistaken for a cart directory.
pub fn migrate(root: &Path) {
    report_migration("state", slot_store::migrate_states(root));
    report_migration("platform", slot_store::migrate_platforms(root));
}

fn report_migration(what: &str, result: std::io::Result<slot_store::MigrationReport>) {
    if let Ok(report) = result {
        if report.failed > 0 {
            eprintln!(
                "slot: migrate: {} of {} {what} director{} did not move",
                report.failed,
                report.moved + report.failed,
                if report.moved + report.failed == 1 {
                    "y"
                } else {
                    "ies"
                }
            );
        }
    }
}

/// Reported to the core as the libretro system directory. `gba_bios.bin`, `gb_bios.bin`
/// and `gbc_bios.bin` present means that system's real BIOS and its boot logo; absent
/// means the core's own high-level BIOS. Neither is an error.
pub fn bios_dir(root: &Path) -> PathBuf {
    root.join("BIOS")
}

/// Whether the card contains the complete, valid-sized GBA BIOS dump. This is deliberately a
/// cheap check because it runs once per core insertion; gpSP performs the same first-byte check
/// before accepting the image.
pub fn has_real_bios(root: &Path) -> bool {
    let path = bios_dir(root).join("gba_bios.bin");
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    if !file.metadata().is_ok_and(|meta| meta.len() == 16 * 1024) {
        return false;
    }
    let mut first = [0u8; 1];
    std::io::Read::read_exact(&mut file, &mut first).is_ok() && first[0] == 0x18
}

pub fn saves_dir(root: &Path) -> PathBuf {
    root.join("Saves")
}
