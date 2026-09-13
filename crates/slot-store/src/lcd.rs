use std::path::{Path, PathBuf};

use crate::atomic_write;

pub const LCD_FILE: &str = "lcd.txt";

fn path(root: &Path) -> PathBuf {
    root.join("System").join(LCD_FILE)
}

/// On by default, preserving the panel treatment used before the setting existed.
pub fn read_lcd(root: &Path) -> bool {
    std::fs::read_to_string(path(root))
        .ok()
        .and_then(|value| match value.trim() {
            "on" => Some(true),
            "off" => Some(false),
            _ => None,
        })
        .unwrap_or(true)
}

pub fn write_lcd(root: &Path, enabled: bool) -> std::io::Result<()> {
    atomic_write(&path(root), if enabled { b"on\n" } else { b"off\n" })
}
