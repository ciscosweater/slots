use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::core::Core;
use crate::gb::{self, Class};
use crate::platform::Platform;

/// What one `migrate_states` call did to a card. `failed` is what lets a caller decide
/// whether there is anything worth logging: an ordinary boot sees `moved == 0, failed == 0`
/// and has nothing to say, but a nonzero `failed` means some directory under `States/` needs
/// a person's attention — the card is read-only, or something under it is not what this
/// expects — and the boot call site is the only place that can put that on the record.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MigrationReport {
    pub moved: usize,
    pub failed: usize,
}

/// Move pre-namespacing state directories under `States/mgba/`.
///
/// States used to live at `States/<stem>/`, from before a card could hold more than one
/// core. Anything directly under `States/` that is neither a core directory nor a **platform**
/// directory is one of those, and belongs to mGBA because mGBA is what wrote it.
///
/// The platform half of that test is not decoration. `States/GB/` is a directory at exactly
/// this level whose name is not a core, so without it the sweep renames every Game Boy save
/// state into `States/mgba/GB/` on the first boot after Game Boy support ships — silently,
/// because this function is best-effort and reports only a count.
///
/// Safe to call on every boot: once a card is migrated there is nothing left that matches,
/// so the second call walks the same directory and moves nothing. Safe to call after an
/// interrupted run for the same reason — the carts that already moved no longer match.
pub fn migrate_states(root: &Path) -> std::io::Result<MigrationReport> {
    let states = root.join("States");
    let dir = match std::fs::read_dir(&states) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(MigrationReport::default()),
        Err(e) => return Err(e),
    };

    let known: Vec<&str> = Core::ALL
        .iter()
        .map(|c| c.as_str())
        .chain(Platform::ALL.iter().map(|p| p.dir_name()))
        .collect();
    let mut report = MigrationReport::default();

    // Collected before anything moves. Renaming entries out of a directory while iterating
    // that same directory is unspecified, and this one is on a user's card.
    let entries: Vec<_> = dir.collect::<Result<Vec<_>, _>>()?;

    for entry in entries {
        // Every fallible step from here on is isolated to this one entry rather than
        // propagated with `?`. `read_dir` above is the one place a hard `Err` is right,
        // because there is nothing left to iterate at all. Once inside the loop, `read_dir`
        // order is stable, so letting one entry's failure — a corrupt subdirectory, a
        // permission bit, `States/mgba` existing as a plain file — abort the whole call
        // would strand every cart that sorts after it on every future boot, which is
        // exactly the "safe to run on every boot" guarantee this function exists to keep.
        // A cart this loop cannot move this boot is still there, unharmed, to try again on
        // the next one. Each isolated failure still counts, though: unlike a deliberate skip
        // (not a directory, already a core directory, a name collision), it is a cart that
        // should have moved and did not.
        let Ok(file_type) = entry.file_type() else {
            report.failed += 1;
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            report.failed += 1;
            continue;
        };
        if known.contains(&name) {
            continue;
        }

        let dest = states.join(Core::Mgba.as_str()).join(name);
        // Never clobber. A collision means someone has already played this cart under the
        // new layout, and their newer states outrank the old ones; leaving the bare copy
        // in place loses nothing and keeps the situation visible on the card.
        if dest.exists() {
            continue;
        }
        let dest_parent = dest.parent().expect("dest has a parent");
        if std::fs::create_dir_all(dest_parent).is_err() {
            report.failed += 1;
            continue;
        }
        if std::fs::rename(entry.path(), &dest).is_ok() {
            report.moved += 1;
        } else {
            report.failed += 1;
        }
    }
    Ok(report)
}

impl MigrationReport {
    fn add(&mut self, other: MigrationReport) {
        self.moved += other.moved;
        self.failed += other.failed;
    }
}

/// Sweep loose content into platform folders, then repair companions that an earlier
/// "everything is GBA" pass left in the wrong place.
///
/// ROMs are classified by extension and, for the Game Boy family, by the CGB header flag.
/// Saves, labels and cartridges follow the ROM of the same stem when one exists; otherwise a
/// loose companion still falls through to `GBA/` so a pre-Game-Boy card keeps working.
///
/// **Must run after `migrate_states`.** Reversed, this would sweep a pre-namespacing
/// `States/<stem>/` into a platform folder where a core folder belongs.
///
/// Safe on every boot: once entries are where they belong, later calls move nothing.
pub fn migrate_platforms(root: &Path) -> std::io::Result<MigrationReport> {
    let mut report = MigrationReport::default();

    // ROMs first so companion matching and state repair can look them up.
    report.add(sweep_loose_roms(&root.join("Games")));
    report.add(repair_misfiled_roms(&root.join("Games")));

    let roms = index_roms(root);

    for dir in ["Saves", "Labels", "Cartridges"] {
        report.add(sweep_loose_companions(&root.join(dir), &roms));
        report.add(repair_misfiled_companions(&root.join(dir), &roms));
    }

    report.add(sweep_state_cores(&root.join("States"), &roms));
    report.add(repair_gambatte_states(root, &roms));

    Ok(report)
}

/// Stem → platform for every ROM under `Games/{GBA,GB,GBC}/`. A stem that appears on more
/// than one platform is omitted: companions for that name cannot be placed without guessing.
fn index_roms(root: &Path) -> HashMap<String, Platform> {
    let mut seen: HashMap<String, Platform> = HashMap::new();
    let mut ambiguous = Vec::new();
    for platform in Platform::ALL {
        let dir = root.join("Games").join(platform.dir_name());
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() || crate::is_hidden(&path) || !platform.accepts(&path) {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()).map(str::to_owned) else {
                continue;
            };
            if let Some(existing) = seen.get(&stem) {
                if *existing != platform {
                    ambiguous.push(stem);
                }
            } else {
                seen.insert(stem, platform);
            }
        }
    }
    for stem in ambiguous {
        seen.remove(&stem);
    }
    seen
}

fn platform_for_rom(path: &Path) -> Platform {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "gba" => Platform::Gba,
        "gb" | "gbc" => match gb::class(path) {
            Class::ColourOnly => Platform::Gbc,
            Class::Original | Class::DualMode => Platform::Gb,
        },
        _ => Platform::Gba,
    }
}

fn sweep_loose_roms(games: &Path) -> MigrationReport {
    let mut report = MigrationReport::default();
    let entries = match read_entries(games) {
        Some(e) => e,
        None => {
            report.failed += 1;
            return report;
        }
    };
    for entry in entries {
        let Ok(file_type) = entry.file_type() else {
            report.failed += 1;
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name();
        if crate::is_hidden(Path::new(&name)) {
            continue;
        }
        let path = entry.path();
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let ext = ext.to_ascii_lowercase();
        if !matches!(ext.as_str(), "gba" | "gb" | "gbc") {
            // Non-ROM loose files under Games/ are not classified here; they stay visible.
            continue;
        }
        let platform = platform_for_rom(&path);
        report.add(rename_into(games.join(platform.dir_name()), &path, &name));
    }
    report
}

fn repair_misfiled_roms(games: &Path) -> MigrationReport {
    let mut report = MigrationReport::default();
    let gba = games.join(Platform::Gba.dir_name());
    let entries = match read_entries(&gba) {
        Some(e) => e,
        None => return report,
    };
    for entry in entries {
        let Ok(file_type) = entry.file_type() else {
            report.failed += 1;
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name();
        if crate::is_hidden(Path::new(&name)) {
            continue;
        }
        let path = entry.path();
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if !ext.eq_ignore_ascii_case("gb") && !ext.eq_ignore_ascii_case("gbc") {
            continue;
        }
        let platform = platform_for_rom(&path);
        if platform == Platform::Gba {
            continue;
        }
        report.add(rename_into(games.join(platform.dir_name()), &path, &name));
    }
    report
}

fn sweep_loose_companions(dir: &Path, roms: &HashMap<String, Platform>) -> MigrationReport {
    let mut report = MigrationReport::default();
    let entries = match read_entries(dir) {
        Some(e) => e,
        None => {
            report.failed += 1;
            return report;
        }
    };
    for entry in entries {
        let Ok(file_type) = entry.file_type() else {
            report.failed += 1;
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name();
        if crate::is_hidden(Path::new(&name)) {
            continue;
        }
        let path = entry.path();
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let platform = roms.get(stem).copied().unwrap_or(Platform::Gba);
        report.add(rename_into(dir.join(platform.dir_name()), &path, &name));
    }
    report
}

fn repair_misfiled_companions(dir: &Path, roms: &HashMap<String, Platform>) -> MigrationReport {
    let mut report = MigrationReport::default();
    let gba = dir.join(Platform::Gba.dir_name());
    let entries = match read_entries(&gba) {
        Some(e) => e,
        None => return report,
    };
    for entry in entries {
        let Ok(file_type) = entry.file_type() else {
            report.failed += 1;
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name();
        if crate::is_hidden(Path::new(&name)) {
            continue;
        }
        let path = entry.path();
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(platform) = roms.get(stem).copied() else {
            continue;
        };
        if platform == Platform::Gba {
            continue;
        }
        report.add(rename_into(dir.join(platform.dir_name()), &path, &name));
    }
    report
}

/// Core directories still loose under `States/`. GBA cores go to `States/GBA/`. Gambatte is
/// split per stem into GB/GBC from the ROM index; unknown stems stay under GBA so nothing is
/// deleted, then `repair_gambatte_states` can place them once a ROM appears.
fn sweep_state_cores(states: &Path, roms: &HashMap<String, Platform>) -> MigrationReport {
    let mut report = MigrationReport::default();
    let entries = match read_entries(states) {
        Some(e) => e,
        None => {
            report.failed += 1;
            return report;
        }
    };
    for entry in entries {
        let Ok(file_type) = entry.file_type() else {
            report.failed += 1;
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            report.failed += 1;
            continue;
        };
        let Some(core) = Core::ALL.iter().find(|c| c.as_str() == name).copied() else {
            continue;
        };
        if core == Core::Gambatte {
            report.add(split_gambatte_core(states, &entry.path(), roms));
            continue;
        }
        let dest = states.join(Platform::Gba.dir_name()).join(name);
        if dest.exists() {
            continue;
        }
        if std::fs::create_dir_all(states.join(Platform::Gba.dir_name())).is_err() {
            report.failed += 1;
            continue;
        }
        match std::fs::rename(entry.path(), &dest) {
            Ok(()) => report.moved += 1,
            Err(_) => report.failed += 1,
        }
    }
    report
}

fn split_gambatte_core(
    states: &Path,
    gambatte: &Path,
    roms: &HashMap<String, Platform>,
) -> MigrationReport {
    let mut report = MigrationReport::default();
    let entries = match read_entries(gambatte) {
        Some(e) => e,
        None => {
            report.failed += 1;
            return report;
        }
    };
    for entry in entries {
        let Ok(file_type) = entry.file_type() else {
            report.failed += 1;
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let Some(stem) = name.to_str() else {
            report.failed += 1;
            continue;
        };
        let platform = roms.get(stem).copied().unwrap_or(Platform::Gba);
        let dest = states
            .join(platform.dir_name())
            .join(Core::Gambatte.as_str())
            .join(stem);
        report.add(rename_dir_into(&dest, &entry.path()));
    }
    // Drop an empty leftover gambatte directory when every stem moved.
    let _ = std::fs::remove_dir(gambatte);
    report
}

/// Move gambatte stems out of `States/GBA/` and dedupe identical copies under GB and GBC so
/// each stem lives only beside its ROM.
fn repair_gambatte_states(root: &Path, roms: &HashMap<String, Platform>) -> MigrationReport {
    let mut report = MigrationReport::default();
    let states = root.join("States");
    let gba_gambatte = states
        .join(Platform::Gba.dir_name())
        .join(Core::Gambatte.as_str());

    if let Some(entries) = read_entries(&gba_gambatte) {
        for entry in entries {
            let Ok(file_type) = entry.file_type() else {
                report.failed += 1;
                continue;
            };
            if !file_type.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let Some(stem) = name.to_str() else {
                report.failed += 1;
                continue;
            };
            let Some(platform) = roms.get(stem).copied() else {
                continue;
            };
            if platform == Platform::Gba {
                continue;
            }
            let dest = states
                .join(platform.dir_name())
                .join(Core::Gambatte.as_str())
                .join(stem);
            report.add(rename_dir_into(&dest, &entry.path()));
        }
        let _ = std::fs::remove_dir(&gba_gambatte);
    }

    report.add(dedupe_gambatte_pair(root, roms));
    report
}

fn dedupe_gambatte_pair(root: &Path, roms: &HashMap<String, Platform>) -> MigrationReport {
    let mut report = MigrationReport::default();
    let gb_dir = root
        .join("States")
        .join(Platform::Gb.dir_name())
        .join(Core::Gambatte.as_str());
    let gbc_dir = root
        .join("States")
        .join(Platform::Gbc.dir_name())
        .join(Core::Gambatte.as_str());

    let gb_stems = list_stem_dirs(&gb_dir);
    let gbc_stems = list_stem_dirs(&gbc_dir);
    let mut all: std::collections::BTreeSet<String> = gb_stems.iter().cloned().collect();
    all.extend(gbc_stems.iter().cloned());

    for stem in all {
        let in_gb = gb_stems.contains(&stem);
        let in_gbc = gbc_stems.contains(&stem);
        let Some(want) = roms.get(&stem).copied() else {
            // No ROM on the card: if duplicated, keep GB and drop the GBC copy.
            if in_gb && in_gbc {
                report.add(remove_dir_all_counted(&gbc_dir.join(&stem)));
            }
            continue;
        };
        match want {
            Platform::Gb => {
                if in_gbc {
                    let src = gbc_dir.join(&stem);
                    if in_gb {
                        report.add(remove_dir_all_counted(&src));
                    } else {
                        let dest = gb_dir.join(&stem);
                        report.add(rename_dir_into(&dest, &src));
                    }
                }
            }
            Platform::Gbc => {
                if in_gb {
                    let src = gb_dir.join(&stem);
                    if in_gbc {
                        report.add(remove_dir_all_counted(&src));
                    } else {
                        let dest = gbc_dir.join(&stem);
                        report.add(rename_dir_into(&dest, &src));
                    }
                }
            }
            Platform::Gba => {}
        }
    }
    report
}

fn list_stem_dirs(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| e.file_name().into_string().ok())
        .collect()
}

fn read_entries(dir: &Path) -> Option<Vec<std::fs::DirEntry>> {
    match std::fs::read_dir(dir) {
        Ok(d) => match d.collect::<Result<Vec<_>, _>>() {
            Ok(entries) => Some(entries),
            Err(_) => None,
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(Vec::new()),
        Err(_) => None,
    }
}

fn rename_into(dest_dir: PathBuf, src: &Path, name: &std::ffi::OsStr) -> MigrationReport {
    let mut report = MigrationReport::default();
    let dest = dest_dir.join(name);
    if dest.exists() {
        return report;
    }
    if std::fs::create_dir_all(&dest_dir).is_err() {
        report.failed += 1;
        return report;
    }
    match std::fs::rename(src, &dest) {
        Ok(()) => report.moved += 1,
        Err(_) => report.failed += 1,
    }
    report
}

fn rename_dir_into(dest: &Path, src: &Path) -> MigrationReport {
    let mut report = MigrationReport::default();
    if dest.exists() {
        // Destination already has states; drop the misfiled copy rather than merge blindly.
        return remove_dir_all_counted(src);
    }
    if let Some(parent) = dest.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            report.failed += 1;
            return report;
        }
    }
    match std::fs::rename(src, dest) {
        Ok(()) => report.moved += 1,
        Err(_) => report.failed += 1,
    }
    report
}

fn remove_dir_all_counted(path: &Path) -> MigrationReport {
    let mut report = MigrationReport::default();
    match std::fs::remove_dir_all(path) {
        Ok(()) => report.moved += 1,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => report.failed += 1,
    }
    report
}
