//! Platform defaults and per-game overrides for the three display toggles: LCD effect, colour
//! correction, and the GB overlay.
//!
//! Written from the quick menu: a console tab on the shelf owns the platform line; the in-game
//! menu owns the game line. Resolve is game, then platform, then the older globals
//! (`lcd.txt` / `slot.state`) so a card that has never been edited this way keeps working.

use std::path::Path;

use crate::ini;
use crate::lcd::read_lcd;
use crate::platform::Platform;
use crate::slot_state::{read_slot_state, SlotState};

pub const DISPLAY_FILE: &str = "System/display.ini";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DisplayPrefs {
    pub lcd: bool,
    pub colour: bool,
    pub overlay: bool,
}

impl DisplayPrefs {
    /// What a card with no display.ini and no legacy files would run: LCD on, colour off,
    /// overlay on — the same defaults `SlotState` and `read_lcd` already used.
    pub fn built_in() -> Self {
        Self {
            lcd: true,
            colour: false,
            overlay: true,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DisplayField {
    Lcd,
    Colour,
    Overlay,
}

impl DisplayField {
    fn as_str(self) -> &'static str {
        match self {
            DisplayField::Lcd => "lcd",
            DisplayField::Colour => "colour",
            DisplayField::Overlay => "overlay",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DisplayTarget<'a> {
    Platform(Platform),
    Game { platform: Platform, stem: &'a str },
}

fn platform_prefix(platform: Platform) -> String {
    platform.dir_name().to_ascii_lowercase()
}

fn platform_key(platform: Platform, field: DisplayField) -> String {
    format!("{}.{}", platform_prefix(platform), field.as_str())
}

fn game_key(platform: Platform, stem: &str, field: DisplayField) -> String {
    format!("{}/{}.{}", platform_prefix(platform), stem, field.as_str())
}

fn key_for(target: DisplayTarget<'_>, field: DisplayField) -> String {
    match target {
        DisplayTarget::Platform(platform) => platform_key(platform, field),
        DisplayTarget::Game { platform, stem } => game_key(platform, stem, field),
    }
}

fn parse_flag(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "on" | "1" | "true" => Some(true),
        "off" | "0" | "false" => Some(false),
        _ => None,
    }
}

fn flag_str(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

fn read_field(root: &Path, key: &str) -> Option<bool> {
    ini::value(root, DISPLAY_FILE, key)
        .as_deref()
        .and_then(parse_flag)
}

/// Legacy globals: `lcd.txt` plus colour / overlay from `slot.state` (or the live copy handed in).
pub fn legacy_prefs(root: &Path, state: &SlotState) -> DisplayPrefs {
    DisplayPrefs {
        lcd: read_lcd(root),
        colour: state.colour_correction,
        overlay: state.gb_overlay,
    }
}

/// Legacy prefs read fresh from the card. Used when the live `SlotState` may already hold a
/// game override and must not be trusted as the fallback.
pub fn legacy_from_disk(root: &Path) -> DisplayPrefs {
    let state = read_slot_state(root);
    legacy_prefs(root, &state)
}

fn resolve_field(
    root: &Path,
    platform: Platform,
    stem: Option<&str>,
    field: DisplayField,
    legacy: bool,
) -> bool {
    if let Some(stem) = stem {
        if let Some(value) = read_field(root, &game_key(platform, stem, field)) {
            return value;
        }
    }
    if let Some(value) = read_field(root, &platform_key(platform, field)) {
        return value;
    }
    legacy
}

/// Effective display prefs for a platform, optionally narrowed to one cart.
pub fn resolve_display(
    root: &Path,
    platform: Platform,
    stem: Option<&str>,
    legacy: DisplayPrefs,
) -> DisplayPrefs {
    DisplayPrefs {
        lcd: resolve_field(root, platform, stem, DisplayField::Lcd, legacy.lcd),
        colour: resolve_field(root, platform, stem, DisplayField::Colour, legacy.colour),
        overlay: resolve_field(root, platform, stem, DisplayField::Overlay, legacy.overlay),
    }
}

/// Write one field for a platform default or a per-game override.
pub fn write_field(
    root: &Path,
    target: DisplayTarget<'_>,
    field: DisplayField,
    value: bool,
) -> std::io::Result<()> {
    ini::write(root, DISPLAY_FILE, &key_for(target, field), flag_str(value))
}

pub fn write_platform(
    root: &Path,
    platform: Platform,
    field: DisplayField,
    value: bool,
) -> std::io::Result<()> {
    write_field(root, DisplayTarget::Platform(platform), field, value)
}

pub fn write_game(
    root: &Path,
    platform: Platform,
    stem: &str,
    field: DisplayField,
    value: bool,
) -> std::io::Result<()> {
    write_field(root, DisplayTarget::Game { platform, stem }, field, value)
}

const FIELDS: [DisplayField; 3] = [
    DisplayField::Lcd,
    DisplayField::Colour,
    DisplayField::Overlay,
];

/// Drop every display.ini key for this target so resolve falls through to the next layer.
pub fn clear_target(root: &Path, target: DisplayTarget<'_>) -> std::io::Result<()> {
    for field in FIELDS {
        ini::remove(root, DISPLAY_FILE, &key_for(target, field))?;
    }
    Ok(())
}

/// Restore the pre-display.ini globals to the built-in defaults.
pub fn reset_legacy(root: &Path) -> std::io::Result<()> {
    use crate::lcd::write_lcd;
    use crate::slot_state::write_slot_state;

    write_lcd(root, DisplayPrefs::built_in().lcd)?;
    let mut state = read_slot_state(root);
    let defaults = DisplayPrefs::built_in();
    state.colour_correction = defaults.colour;
    state.gb_overlay = defaults.overlay;
    write_slot_state(root, &state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lcd::write_lcd;
    use crate::slot_state::write_slot_state;
    use tempfile::tempdir;

    fn root() -> tempfile::TempDir {
        let d = tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("System")).unwrap();
        d
    }

    #[test]
    fn resolve_falls_back_to_legacy_when_display_ini_is_empty() {
        let d = root();
        write_lcd(d.path(), false).unwrap();
        let state = SlotState {
            colour_correction: true,
            gb_overlay: false,
            ..Default::default()
        };
        write_slot_state(d.path(), &state).unwrap();

        let prefs = resolve_display(
            d.path(),
            Platform::Gba,
            Some("Emerald"),
            legacy_from_disk(d.path()),
        );
        assert_eq!(
            prefs,
            DisplayPrefs {
                lcd: false,
                colour: true,
                overlay: false
            }
        );
    }

    #[test]
    fn platform_beats_legacy_and_game_beats_platform() {
        let d = root();
        write_platform(d.path(), Platform::Gba, DisplayField::Lcd, false).unwrap();
        write_game(d.path(), Platform::Gba, "Emerald", DisplayField::Lcd, true).unwrap();

        let legacy = DisplayPrefs::built_in();
        assert!(!resolve_display(d.path(), Platform::Gba, None, legacy).lcd);
        assert!(resolve_display(d.path(), Platform::Gba, Some("Emerald"), legacy).lcd);
        assert!(!resolve_display(d.path(), Platform::Gba, Some("Fusion"), legacy).lcd);
    }

    #[test]
    fn fields_resolve_independently() {
        let d = root();
        write_platform(d.path(), Platform::Gb, DisplayField::Lcd, false).unwrap();
        write_game(d.path(), Platform::Gb, "Tetris", DisplayField::Colour, true).unwrap();

        let prefs = resolve_display(
            d.path(),
            Platform::Gb,
            Some("Tetris"),
            DisplayPrefs::built_in(),
        );
        assert!(!prefs.lcd);
        assert!(prefs.colour);
        assert!(prefs.overlay);
    }

    #[test]
    fn clear_target_drops_keys_so_resolve_falls_through() {
        let d = root();
        write_platform(d.path(), Platform::Gba, DisplayField::Lcd, false).unwrap();
        write_game(d.path(), Platform::Gba, "Emerald", DisplayField::Lcd, true).unwrap();
        clear_target(
            d.path(),
            DisplayTarget::Game {
                platform: Platform::Gba,
                stem: "Emerald",
            },
        )
        .unwrap();
        assert!(
            !resolve_display(
                d.path(),
                Platform::Gba,
                Some("Emerald"),
                DisplayPrefs::built_in(),
            )
            .lcd
        );
        clear_target(d.path(), DisplayTarget::Platform(Platform::Gba)).unwrap();
        assert!(
            resolve_display(
                d.path(),
                Platform::Gba,
                Some("Emerald"),
                DisplayPrefs::built_in(),
            )
            .lcd
        );
    }
}
