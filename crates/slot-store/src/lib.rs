mod atomic;
mod core;
mod display_prefs;
mod favorites;
mod font;
pub mod gb;
mod gba;
pub mod ini;
mod last_shelf;
mod lcd;
mod migrate;
mod platform;
mod recents;
mod ring;
mod scan;
mod slot_state;
mod stamp;
mod theme;

pub use atomic::atomic_write;
pub use core::{
    core_for, core_for_cart, read_selected_cores, write_selected_core, Core, SELECTED_CORE_FILE,
};
pub use display_prefs::{
    clear_target as clear_display_target, legacy_from_disk, legacy_prefs, reset_legacy,
    resolve_display, write_field as write_display_field, write_game as write_display_game,
    write_platform as write_display_platform, DisplayField, DisplayPrefs, DisplayTarget,
    DISPLAY_FILE,
};
pub use favorites::{read_favorites, write_favorites, FAVORITES_FILE};
pub use font::{read_pixelify, write_pixelify, FONT_FILE};
pub use gba::{header_clean, header_code, header_title};
pub use last_shelf::{
    read_last_shelf, write_last_shelf, write_last_shelf_stem, LastShelf, LAST_SHELF_FILE,
};
pub use lcd::{read_lcd, write_lcd, LCD_FILE};
pub use migrate::{migrate_platforms, migrate_states, MigrationReport};
pub use platform::Platform;
pub use recents::{read_recents, touch_recent, write_recents, RECENTS_FILE, RECENTS_MAX};
pub use ring::{StateEntry, StateRing, RING_MAX};
pub use scan::{is_hidden, scan, scan_cached, Cart, StoreError};
pub use slot_state::{
    read_slot_state, write_slot_state, FaceButtons, SlotState, BLUE_LIGHT_MAX, BRIGHTNESS_MAX,
    FF_SPEEDS, FF_SPEED_DEFAULT, FF_SPEED_MAX, FF_SPEED_MIN, UTC_OFFSET_MAX, UTC_OFFSET_MIN,
    VOLUME_MAX,
};
pub use stamp::{
    civil_from_days, days_from_civil, days_in_month, format_stamp, parse_stamp, stamp_now,
};
pub use theme::{Theme, THEME_FILE};
