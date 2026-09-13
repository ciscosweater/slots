use std::time::Duration;

/// Nominal RG SP panel rate from the stock H700 DTB timings.  The panel, renderer and
/// emulator must share this clock; pretending it is 60 Hz periodically overwrites a frame
/// before the LCD can scan it out.
pub const PANEL_HZ: f64 = 59.155;

pub const PANEL_FRAME: Duration = Duration::from_nanos((1_000_000_000.0 / PANEL_HZ) as u64);
