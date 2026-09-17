use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use slot_retro::{LibretroCore, MockCore, RetroCore};
use slot_store::Core;

use crate::root;

/// Integration tests compile this crate without `cfg(test)`, so they opt in here. Production
/// never does: a missing dylib is a refused insert, not a rainbow cart.
static ALLOW_MOCK: AtomicBool = AtomicBool::new(false);

/// Lets a test `Session` run a machine when no dylib is planted. `SLOT_CORE` still wins: a
/// test that names a missing file is asking for the refuse path, not a mock.
pub fn allow_mock_for_tests() {
    ALLOW_MOCK.store(true, Ordering::Relaxed);
}

/// Names the dylib outright, for a build that keeps it somewhere the search does not look.
///
/// This wins over `System/selected_core.ini` for which dylib loads — it stays a developer
/// escape hatch, not a second way to pick a core. It is not silent about that: it does not
/// touch which `Core` a cart resolves to (still the ini, still what the state directory
/// under `States/<core>/` is named after), only which file `open_core` opens. Pointing this
/// at gpSP for a cart the ini never mentions runs gpSP with its states filed under
/// `States/mgba/` — correct for a developer who set the override on purpose, a trap for
/// anyone who forgot it was set.
const CORE_ENV: &str = "SLOT_CORE";

/// Which dylib backs a core. The device keeps all three in `System/`, so this is a filename
/// rather than a search: whichever the cart asked for is either there or it is not.
pub fn dylib_name(core: Core) -> String {
    format!(
        "{}_libretro.{}",
        core.as_str(),
        std::env::consts::DLL_EXTENSION
    )
}

/// Most specific first: the environment, then the content root's own `System/` — where the
/// device actually keeps a core, and, on the device, also where the binary itself lives, so
/// the next candidate coincides with this one there. Off the device — a host build, or a
/// test with its own tmp root — the binary's directory and `root` are different places, and
/// only searching the root's own `System/` gives a cart's own content root somewhere a test
/// can plant a dylib under. The `vendor` directory `scripts/fetch-core.sh` writes into is
/// last: a host development convenience, not anywhere a shipped device looks.
fn candidates(root: &Path, core: Core) -> Vec<PathBuf> {
    if let Some(named) = std::env::var_os(CORE_ENV) {
        return vec![PathBuf::from(named)];
    }
    // Spelled from the platform's own convention so the same search finds the device's `.so`.
    let name = dylib_name(core);
    let mut paths = vec![root.join("System").join(&name)];
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        paths.push(dir.join(&name));
        paths.push(dir.join("vendor").join(&name));
    }
    paths.push(Path::new("vendor").join(&name));
    paths
}

/// The one place a cart's `Core` becomes a dylib path. Callers that already know which core
/// they want — `session.rs` resolves it once per insert — pass it straight through, which is
/// what keeps this call from disagreeing with the caller's own choice. It says nothing about
/// what the caller does with that `Core` afterward: `App` is the one that has to keep using
/// the same value for every later read and write, and it does that by storing it rather than
/// asking again.
pub fn open_core(root: &Path, core: Core) -> Option<Box<dyn RetroCore>> {
    if let Some(opened) = open_core_with_options(root, core, "auto", true) {
        return Some(opened);
    }
    if ALLOW_MOCK.load(Ordering::Relaxed) && std::env::var_os(CORE_ENV).is_none() {
        eprintln!(
            "slot: no {} core found in tests, running the mock",
            core.as_str()
        );
        return Some(Box::new(MockCore::new()));
    }
    None
}

/// Open a core with the options that are selected for the next session. Options must be seeded
/// before the ROM is loaded because libretro cores read most of them during `retro_load_game`.
pub fn open_core_with_options(
    root: &Path,
    core: Core,
    serial: &str,
    colour: bool,
) -> Option<Box<dyn RetroCore>> {
    let paths = candidates(root, core);
    if let Some(opened) = open_core_for_with_options(root, core, serial, colour, &paths) {
        return Some(opened);
    }
    if ALLOW_MOCK.load(Ordering::Relaxed) && std::env::var_os(CORE_ENV).is_none() {
        eprintln!(
            "slot: no {} core found in tests, running the mock",
            core.as_str()
        );
        return Some(Box::new(MockCore::new()));
    }
    None
}

/// The named core if one of these opens. A missing or broken dylib is `None`, and the insert
/// that asked for it comes back out of the slot: seating a rainbow test pattern used to read
/// as a broken game rather than a missing core.
///
/// The core is told the content root's own folders, never the dylib's: on the device the
/// core lives in `System/` and the user's BIOS does not.
///
/// `core` is redundant with `paths` in production — `open_core` derived both from the same
/// `Core` — but this function stays the seam that takes `paths` explicitly, because tests
/// plant a dylib somewhere `candidates` would not otherwise look. `core_options` needs
/// `core` too, and only `open_core_for` ever holds a concrete `slot_retro::LibretroCore` to
/// seed them on: everything above here deals in `Box<dyn RetroCore>`, which has no
/// `set_option`. Options are handed to `open_with_options` rather than applied after
/// `open_with` returns because Gambatte reads `gambatte_gb_bootloader` during `retro_init`
/// and never again; mGBA already defaults to the official GBA intro when `gba_bios.bin` is
/// in this folder, and is left alone.
pub fn open_core_for(root: &Path, core: Core, paths: &[PathBuf]) -> Option<Box<dyn RetroCore>> {
    open_core_for_with_options(root, core, "auto", true, paths)
}

/// Test seam for opening a named core while supplying the same runtime options as production.
pub fn open_core_for_with_options(
    root: &Path,
    core: Core,
    serial: &str,
    colour: bool,
    paths: &[PathBuf],
) -> Option<Box<dyn RetroCore>> {
    let bios = root::bios_dir(root);
    let saves = root::saves_dir(root);
    // Gambatte reads its bootloader option during retro_init; the remaining options are applied
    // immediately after opening and before the ROM is loaded.
    let init_options = if core == Core::Gambatte {
        vec![("gambatte_gb_bootloader", "enabled")]
    } else {
        Vec::new()
    };
    for path in paths {
        if !path.exists() {
            continue;
        }
        match LibretroCore::open_with_options(path, &bios, &saves, &init_options) {
            Ok(mut opened) => {
                apply_core_options_with(
                    &mut opened,
                    core,
                    serial,
                    root::has_real_bios(root),
                    colour,
                );
                eprintln!("slot: core {}", path.display());
                return Some(Box::new(opened));
            }
            Err(e) => eprintln!("slot: {}: {e}", path.display()),
        }
    }
    eprintln!(
        "slot: no {} core found. Looked in: {}",
        core.as_str(),
        paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    None
}

/// Options a core needs before `load`, because libretro cores read them during
/// `retro_load_game` rather than continuously. Gambatte's bootloader flag is the
/// exception: it is read at `retro_init`, so `open_core_for` seeds this list there.
///
/// `auto` resolves the serial protocol from the ROM, so two devices running the same game
/// agree on a mode without either being told which. Anything more deliberate belongs to a
/// link session, which knows what the other end picked. Colour correction is enabled in each
/// core rather than duplicated in the frontend shader: each core owns the transform matching
/// its native pixel format and emulated GBA output.
///
/// Gambatte's own default for `gambatte_gb_bootloader` is enabled, but a failed
/// `GET_VARIABLE` turns it off. Seeding `enabled` is what makes `gb_bios.bin` /
/// `gbc_bios.bin` in `BIOS/` play the Nintendo logo the same way `gba_bios.bin` already
/// does for mGBA. mGBA is not named here: its C defaults already use a present BIOS and
/// do not skip the intro.
pub fn core_options(which: Core) -> Vec<(&'static str, &'static str)> {
    match which {
        Core::Mgba => vec![("mgba_color_correction", "GBA")],
        Core::Gpsp => vec![
            ("gpsp_serial", "auto"),
            ("gpsp_color_correction", "enabled"),
        ],
        Core::Gambatte => vec![
            ("gambatte_gb_bootloader", "enabled"),
            ("gambatte_gb_colorization", "internal"),
            ("gambatte_gb_internal_palette", "PixelShift - Pack 1"),
            (
                "gambatte_gb_palette_pixelshift_1",
                "PixelShift 03 - BGB 0.3 Emulator",
            ),
            ("gambatte_gbc_color_correction", "GBC only"),
            ("gambatte_gbc_color_correction_mode", "Accurate"),
        ],
    }
}

/// The libretro option that tracks the quick menu's colour-correction switch for a live core.
pub fn colour_correction_option(which: Core, on: bool) -> (&'static str, &'static str) {
    match which {
        Core::Mgba => ("mgba_color_correction", if on { "Auto" } else { "OFF" }),
        Core::Gpsp => (
            "gpsp_color_correction",
            if on { "enabled" } else { "disabled" },
        ),
        Core::Gambatte => (
            "gambatte_gbc_color_correction",
            if on { "GBC only" } else { "disabled" },
        ),
    }
}

pub fn apply_core_options(core: &mut LibretroCore, which: Core) {
    for (key, value) in core_options(which) {
        core.set_option(key, value);
    }
}

/// Apply the options that vary per session while retaining the Gambatte palette and boot setup.
pub fn apply_core_options_with(
    core: &mut LibretroCore,
    which: Core,
    serial: &str,
    bios: bool,
    colour: bool,
) {
    core.set_option(&format!("{}_frameskip", which.as_str()), "auto");
    match which {
        Core::Mgba => {
            core.set_option("mgba_sgb_borders", "OFF");
            core.set_option("mgba_gb_colors_preset", "1");
            core.set_option("mgba_gb_colors", "GBC Dark Green →A");
            core.set_option("mgba_color_correction", if colour { "Auto" } else { "OFF" });
        }
        Core::Gpsp => {
            core.set_option("gpsp_serial", serial);
            if bios {
                core.set_option("gpsp_boot_mode", "bios");
            }
            core.set_option(
                "gpsp_color_correction",
                if colour { "enabled" } else { "disabled" },
            );
        }
        Core::Gambatte => {
            for (key, value) in core_options(which) {
                if key == "gambatte_gbc_color_correction" {
                    core.set_option(key, if colour { "GBC only" } else { "disabled" });
                } else {
                    core.set_option(key, value);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Both tests below mutate `SLOT_CORE`, which is process-global; the test harness runs
    /// tests in this module on separate threads by default, so without this they can
    /// interleave and read back a value neither of them set.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The half of "the dylib chosen and the state directory used agree" that lives entirely
    /// in this module: `candidates`, and therefore `open_core`, never spells a filename that
    /// does not match the `Core` it was handed. `crates/slot/tests/gpsp.rs` pins the other
    /// half — that a cart resolved to the same `Core` reads its resume state from the
    /// matching `States/<core>/` directory — through the real `Session`.
    #[test]
    fn candidates_search_the_named_cores_own_filename_only() {
        let _g = lock();
        // A developer's shell leaking `SLOT_CORE` into this run would short-circuit the
        // search this test exists to check, so it cannot assume the var is unset.
        std::env::remove_var(CORE_ENV);

        let root = Path::new("/root");
        let mgba = candidates(root, Core::Mgba);
        let gpsp = candidates(root, Core::Gpsp);
        assert_ne!(mgba, gpsp, "the two cores searched the same paths");
        assert!(!mgba.is_empty());
        assert!(!gpsp.is_empty());
        for path in &mgba {
            let name = path.file_name().unwrap().to_str().unwrap();
            assert!(name.starts_with("mgba_libretro"), "{name} is not mGBA's");
        }
        for path in &gpsp {
            let name = path.file_name().unwrap().to_str().unwrap();
            assert!(name.starts_with("gpsp_libretro"), "{name} is not gpSP's");
        }
    }

    /// The property `crates/slot/tests/gpsp.rs`'s integration test relies on: a content root
    /// with its own tmp directory is not near `current_exe()` or `./vendor`, so without this
    /// candidate an integration test has nowhere to plant a fake dylib for `open_core` to
    /// find. Root cause of the gap F3 closed — before this candidate existed, a mutation
    /// that fed `open_core` the wrong `Core` had no candidate list a test could observe
    /// disagree.
    #[test]
    fn candidates_search_the_roots_own_system_directory() {
        let _g = lock();
        std::env::remove_var(CORE_ENV);
        let root = Path::new("/some/content/root");
        assert_eq!(
            candidates(root, Core::Gpsp)[0],
            root.join("System").join(dylib_name(Core::Gpsp)),
        );
    }

    /// The override is a filename, not a `Core`: it must win regardless of which core asked,
    /// which is what makes it a trap when the ini disagrees rather than a second selector.
    #[test]
    fn the_env_override_ignores_which_core_was_asked_for() {
        let _g = lock();
        std::env::set_var(CORE_ENV, "/dev/null/named-core");
        let root = Path::new("/root");
        let mgba = candidates(root, Core::Mgba);
        let gpsp = candidates(root, Core::Gpsp);
        std::env::remove_var(CORE_ENV);
        assert_eq!(mgba, vec![PathBuf::from("/dev/null/named-core")]);
        assert_eq!(
            mgba, gpsp,
            "the override stopped winning for one of the cores"
        );
    }

    /// Gambatte turns the official boot ROM off unless this is already set when `retro_init`
    /// runs. The GBA cores must not appear here: mGBA already plays `gba_bios.bin` from its
    /// own defaults, which is the behaviour this list is not allowed to disturb.
    #[test]
    fn gambatte_enables_the_boot_logo_and_gba_cores_do_not_touch_theirs() {
        assert!(core_options(Core::Gambatte)
            .iter()
            .any(|&(k, v)| k == "gambatte_gb_bootloader" && v == "enabled"));
        for which in [Core::Mgba, Core::Gpsp] {
            for (key, _) in core_options(which) {
                assert!(
                    !key.contains("bios") && !key.contains("boot"),
                    "{key} would change a GBA intro that already works"
                );
            }
        }
    }
}
