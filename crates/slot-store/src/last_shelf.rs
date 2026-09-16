use std::path::{Path, PathBuf};

use crate::atomic_write;

/// The cart highlighted the last time the shelf was used. This is deliberately separate from
/// `slot.state`'s `cart`: that field describes the cart physically seated in the slot, while this
/// one describes where the shelf should open after an eject or a reboot.
pub const LAST_SHELF_FILE: &str = "last_shelf.txt";

fn path(root: &Path) -> PathBuf {
    root.join("System").join(LAST_SHELF_FILE)
}

pub fn read_last_shelf(root: &Path) -> Option<String> {
    std::fs::read_to_string(path(root))
        .ok()?
        .lines()
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

pub fn write_last_shelf(root: &Path, stem: &str) -> std::io::Result<()> {
    let path = path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    atomic_write(&path, format!("{stem}\n").as_bytes())
}
