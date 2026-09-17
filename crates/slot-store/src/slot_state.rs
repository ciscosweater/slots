use std::path::{Path, PathBuf};

use crate::atomic::atomic_write;
use crate::platform::Platform;

pub const BRIGHTNESS_MAX: u8 = 16;
pub const BLUE_LIGHT_MAX: u8 = 9;
pub const VOLUME_MAX: u8 = 100;

/// What a real zone can be, in minutes. The card keeps UTC because the base system's clock
/// and its ntp both assume it; this is the only thing that turns it into the time on the
/// shelf. Minutes rather than hours: several zones are offset by thirty and forty five.
pub const UTC_OFFSET_MIN: i16 = -720;
pub const UTC_OFFSET_MAX: i16 = 840;

/// The fast-forward speeds the quick menu offers, in game frames per screen refresh. Four is
/// the most an H700 can serve (see `FAST_STEPS` in the emulator), and one is not fast at all.
pub const FF_SPEEDS: [u8; 4] = [2, 3, 4, 6];
pub const FF_SPEED_DEFAULT: u8 = 6;
pub const FF_SPEED_MIN: u8 = FF_SPEEDS[0];
pub const FF_SPEED_MAX: u8 = FF_SPEEDS[FF_SPEEDS.len() - 1];

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SlotState {
    /// Filename stem. `None` is an empty slot, which is the shelf.
    pub cart: Option<String>,
    /// Platform of the cart in the slot. `None` keeps compatibility with older cards whose
    /// state only stored the stem.
    pub cart_platform: Option<Platform>,
    pub brightness: u8,
    pub blue_light: u8,
    pub volume: u8,
    /// Silence on top of the level rather than instead of it, so unmuting gives back the
    /// number the user last chose.
    pub muted: bool,
    /// Whether anyone has ever confirmed the wall clock. The marker for slot's own first
    /// launch, and the one field a fresh card must read as false.
    pub clock_set: bool,
    /// Minutes to add to the card's UTC to get local time. Zero is a device that never left
    /// Greenwich, which is also what a card that has never been asked reads as.
    pub utc_offset_min: i16,
    /// Whether the motor may move. Off, a game still asks for it and is simply never obeyed.
    pub rumble: bool,
    /// Game frames per screen refresh while fast-forwarding, `FF_SPEED_MIN` to `FF_SPEED_MAX`.
    pub ff_speed: u8,
    /// Whether fast-forward is heard, sped up, rather than dropped.
    pub ff_sound: bool,
    /// Whether the selected core should apply its console LCD colour correction.
    pub colour_correction: bool,
}

/// Not derived. `read_slot_state` falls back here on a first boot, and all zeroes would
/// be a device with the backlight off and the mixer muted.
impl Default for SlotState {
    fn default() -> Self {
        SlotState {
            cart: None,
            cart_platform: None,
            brightness: 5,
            blue_light: 0,
            volume: 60,
            muted: false,
            clock_set: false,
            utc_offset_min: 0,
            rumble: true,
            ff_speed: FF_SPEED_DEFAULT,
            ff_sound: false,
            colour_correction: false,
        }
    }
}

fn state_path(root: &Path) -> PathBuf {
    root.join("System").join("slot.state")
}

pub fn read_slot_state(root: &Path) -> SlotState {
    std::fs::read(state_path(root))
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .and_then(|s| parse(&s))
        .unwrap_or_default()
}

pub fn write_slot_state(root: &Path, s: &SlotState) -> std::io::Result<()> {
    let text = format!(
        "version=2\ncart={}\ncart_platform={}\nbrightness={}\nblue_light={}\nvolume={}\nmuted={}\nclock_set={}\nutc_offset_min={}\nrumble={}\nff_speed={}\nff_sound={}\ncolour_correction={}\n",
         s.cart.as_deref().unwrap_or(""),
         s.cart_platform.map_or(String::new(), platform_key),
        s.brightness,
        s.blue_light,
        s.volume,
        s.muted as u8,
        s.clock_set as u8,
        s.utc_offset_min,
        s.rumble as u8,
        s.ff_speed,
         s.ff_sound as u8,
         s.colour_correction as u8
    );
    atomic_write(&state_path(root), text.as_bytes())
}

/// The lines every build has written are all or nothing. A file missing one of those, or
/// holding one out of range, is not one we wrote, and inheriting the missing fields from the
/// defaults would hide the corruption behind plausible values.
///
/// Everything else is forgiven. A line this build does not know was written by a later one,
/// and is skipped rather than costing the user their levels and their clock. The quick menu's
/// settings arrived after cards were already in use, so each of those that is missing or
/// unreadable reads as its own default and leaves the rest of the card alone.
fn parse(text: &str) -> Option<SlotState> {
    let mut version = None;
    let mut cart = None;
    let mut cart_platform = None;
    let mut brightness = None;
    let mut blue_light = None;
    let mut volume = None;
    let mut muted = None;
    let mut clock_set = None;
    let mut utc_offset_min = None;
    let mut rumble = None;
    let mut ff_speed = None;
    let mut ff_sound = None;
    let mut colour_correction = None;
    for line in text.lines().filter(|l| !l.is_empty()) {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "version" => version = Some(value.parse::<u8>().ok().filter(|v| *v == 2)?),
            "cart" => cart = Some(value.to_string()),
            "cart_platform" => cart_platform = platform_value(value),
            // Version 1 had ten positions. Keep their physical brightness on upgrade while
            // version 2 adds intermediate night-time levels between them.
            "brightness" => brightness = Some(value.parse::<u8>().ok()?),
            "blue_light" => blue_light = Some(level(value, BLUE_LIGHT_MAX)?),
            "volume" => volume = Some(level(value, VOLUME_MAX)?),
            "muted" => muted = Some(level(value, 1)? == 1),
            "clock_set" => clock_set = Some(level(value, 1)? == 1),
            "utc_offset_min" => utc_offset_min = Some(offset(value)?),
            "rumble" => rumble = flag(value),
            "ff_speed" => ff_speed = value.parse().ok().filter(|n| FF_SPEEDS.contains(n)),
            "ff_sound" => ff_sound = flag(value),
            "colour_correction" => colour_correction = flag(value),
            _ => {}
        }
    }
    let brightness = match version {
        Some(2) => level(&brightness?.to_string(), BRIGHTNESS_MAX)?,
        None => *[0, 1, 3, 5, 7, 9, 11, 13, 15, 16].get(brightness? as usize)?,
        _ => return None,
    };
    let cart = cart?;
    let fallback = SlotState::default();
    Some(SlotState {
        cart: (!cart.is_empty()).then_some(cart),
        cart_platform,
        brightness,
        blue_light: blue_light?,
        volume: volume?,
        muted: muted?,
        clock_set: clock_set?,
        utc_offset_min: utc_offset_min?,
        rumble: rumble.unwrap_or(fallback.rumble),
        ff_speed: ff_speed.unwrap_or(fallback.ff_speed),
        ff_sound: ff_sound.unwrap_or(fallback.ff_sound),
        colour_correction: colour_correction.unwrap_or(fallback.colour_correction),
    })
}

fn platform_key(platform: Platform) -> String {
    platform.dir_name().to_ascii_lowercase()
}

fn platform_value(value: &str) -> Option<Platform> {
    Platform::ALL
        .into_iter()
        .find(|p| value.eq_ignore_ascii_case(p.dir_name()))
}

fn offset(value: &str) -> Option<i16> {
    value
        .parse()
        .ok()
        .filter(|n| (UTC_OFFSET_MIN..=UTC_OFFSET_MAX).contains(n))
}

fn level(value: &str, max: u8) -> Option<u8> {
    value.parse().ok().filter(|n| *n <= max)
}

fn flag(value: &str) -> Option<bool> {
    level(value, 1).map(|n| n == 1)
}
