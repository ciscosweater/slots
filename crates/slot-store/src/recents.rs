use std::path::{Path, PathBuf};

use crate::atomic_write;

pub const RECENTS_FILE: &str = "recents.txt";
pub const RECENTS_MAX: usize = 10;

fn path(root: &Path) -> PathBuf {
    root.join("System").join(RECENTS_FILE)
}

/// Filename stems, newest first. Unknown games stay recorded so temporarily removing a ROM
/// from the card does not silently erase its place in the history.
pub fn read_recents(root: &Path) -> Vec<String> {
    std::fs::read_to_string(path(root))
        .ok()
        .map(|text| {
            let mut recents = Vec::new();
            for stem in text.lines().filter(|line| !line.is_empty()) {
                if !recents.iter().any(|known| known == stem) {
                    recents.push(stem.to_owned());
                }
                if recents.len() == RECENTS_MAX {
                    break;
                }
            }
            recents
        })
        .unwrap_or_default()
}

pub fn write_recents(root: &Path, recents: &[String]) -> std::io::Result<()> {
    let mut text = recents
        .iter()
        .take(RECENTS_MAX)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    atomic_write(&path(root), text.as_bytes())
}

/// Promotes `stem` to the front without duplicates and caps the durable history.
pub fn touch_recent(recents: &mut Vec<String>, stem: &str) {
    recents.retain(|known| known != stem);
    recents.insert(0, stem.to_owned());
    recents.truncate(RECENTS_MAX);
}
