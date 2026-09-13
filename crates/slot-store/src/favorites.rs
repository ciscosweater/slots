use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::atomic_write;

pub const FAVORITES_FILE: &str = "favorites.txt";

fn path(root: &Path) -> PathBuf {
    root.join("System").join(FAVORITES_FILE)
}

/// Filename stems, one per line. Unknown games stay recorded so temporarily removing a ROM
/// from the card does not silently forget that it was a favourite.
pub fn read_favorites(root: &Path) -> BTreeSet<String> {
    std::fs::read_to_string(path(root))
        .ok()
        .map(|text| {
            text.lines()
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

pub fn write_favorites(root: &Path, favorites: &BTreeSet<String>) -> std::io::Result<()> {
    let mut text = favorites.iter().cloned().collect::<Vec<_>>().join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    atomic_write(&path(root), text.as_bytes())
}
