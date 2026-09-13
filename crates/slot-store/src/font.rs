use std::path::{Path, PathBuf};

use crate::atomic_write;

pub const FONT_FILE: &str = "font.txt";

fn path(root: &Path) -> PathBuf {
    root.join("System").join(FONT_FILE)
}

/// Pixelify is the default now; `original` restores the face used before it was added.
pub fn read_pixelify(root: &Path) -> bool {
    std::fs::read_to_string(path(root))
        .ok()
        .and_then(|value| match value.trim() {
            "pixelify" => Some(true),
            "original" => Some(false),
            _ => None,
        })
        .unwrap_or(true)
}

pub fn write_pixelify(root: &Path, pixelify: bool) -> std::io::Result<()> {
    atomic_write(
        &path(root),
        if pixelify {
            b"pixelify\n"
        } else {
            b"original\n"
        },
    )
}
