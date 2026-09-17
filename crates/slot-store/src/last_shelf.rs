use std::path::{Path, PathBuf};

use crate::atomic_write;
use crate::platform::Platform;

/// The cart highlighted the last time the shelf was used. This is deliberately separate from
/// `slot.state`'s `cart`: that field describes the cart physically seated in the slot, while this
/// one describes where the shelf should open after an eject or a reboot.
pub const LAST_SHELF_FILE: &str = "last_shelf.txt";

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct LastShelf {
    pub stem: Option<String>,
    /// Shelf category tab index (ALL / REC / GBA / GB / GBC). `None` means the file did not
    /// say, which is every card written before categories were persisted.
    pub category: Option<usize>,
    /// Platform of the highlighted cart. `None` on files written before the highlight stored
    /// one, in which case the first matching stem is used.
    pub platform: Option<Platform>,
}

fn path(root: &Path) -> PathBuf {
    root.join("System").join(LAST_SHELF_FILE)
}

/// Read the last shelf highlight. A one-line file is the historical stem-only form; a
/// `version=1` file also carries the category tab, and may carry a platform.
pub fn read_last_shelf(root: &Path) -> LastShelf {
    let Ok(text) = std::fs::read_to_string(path(root)) else {
        return LastShelf::default();
    };
    parse(&text)
}

fn parse(text: &str) -> LastShelf {
    let mut version = None;
    let mut stem = None;
    let mut category = None;
    let mut platform = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            match key {
                "version" => version = value.parse::<u32>().ok(),
                "stem" => {
                    if !value.is_empty() {
                        stem = Some(value.to_owned());
                    }
                }
                "category" => category = value.parse::<usize>().ok(),
                "platform" => platform = Platform::from_dir_name(value),
                _ => {}
            }
            continue;
        }
        // Pre-version file: the first non-empty line is the stem alone.
        if stem.is_none() && version.is_none() {
            stem = Some(line.to_owned());
        }
    }
    if version.is_none() {
        category = None;
        platform = None;
    }
    LastShelf {
        stem,
        category,
        platform,
    }
}

pub fn write_last_shelf(root: &Path, shelf: &LastShelf) -> std::io::Result<()> {
    let Some(stem) = shelf.stem.as_deref() else {
        return Ok(());
    };
    let path = path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let category = shelf.category.unwrap_or(0);
    let mut text = format!("version=1\nstem={stem}\ncategory={category}\n");
    if let Some(platform) = shelf.platform {
        text.push_str(&format!("platform={}\n", platform.dir_name()));
    }
    atomic_write(&path, text.as_bytes())
}

/// Convenience for callers that only have a stem (legacy write sites / tests).
pub fn write_last_shelf_stem(root: &Path, stem: &str) -> std::io::Result<()> {
    write_last_shelf(
        root,
        &LastShelf {
            stem: Some(stem.to_owned()),
            category: None,
            platform: None,
        },
    )
}
