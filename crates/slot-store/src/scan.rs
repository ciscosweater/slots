use std::fmt;
use std::path::{Path, PathBuf};

use crate::gba::{header_code, header_title};
use crate::read_favorites;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Platform {
    Gba,
    Gb,
    Gbc,
}

impl Platform {
    pub const ALL: [Platform; 3] = [Platform::Gba, Platform::Gb, Platform::Gbc];

    pub fn text(self) -> &'static str {
        match self {
            Platform::Gba => "GBA",
            Platform::Gb => "GB",
            Platform::Gbc => "GBC",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Cart {
    /// Filename stem, which is the key for labels, saves and states. Not a content hash.
    pub stem: String,
    pub rom: PathBuf,
    pub label: Option<PathBuf>,
    pub title: String,
    /// The four character header game code, empty when the rom has none.
    pub code: String,
    pub platform: Platform,
}

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "io: {e}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        StoreError::Io(e)
    }
}

/// An unmounted card or a card with no library is an empty shelf, not a boot failure.
pub fn scan(root: &Path) -> Result<Vec<Cart>, StoreError> {
    let dir = match std::fs::read_dir(root.join("Games")) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };

    let mut carts = Vec::new();
    for entry in dir {
        let rom = entry?.path();
        let Some(platform) = platform_for(&rom) else {
            continue;
        };
        let Some(stem) = rom.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let label = root.join("Labels").join(format!("{stem}.png"));
        carts.push(Cart {
            stem: stem.to_string(),
            title: match platform {
                Platform::Gba => header_title(&rom).unwrap_or_default(),
                Platform::Gb | Platform::Gbc => gb_title(&rom).unwrap_or_default(),
            },
            code: match platform {
                Platform::Gba => header_code(&rom).unwrap_or_default(),
                Platform::Gb | Platform::Gbc => String::new(),
            },
            platform,
            label: label.is_file().then_some(label),
            rom,
        });
    }
    let favorites = read_favorites(root);
    carts.sort_by(|a, b| {
        favorites
            .contains(&b.stem)
            .cmp(&favorites.contains(&a.stem))
            .then_with(|| a.stem.cmp(&b.stem))
    });
    Ok(carts)
}

fn platform_for(p: &Path) -> Option<Platform> {
    if is_hidden(p) || !p.is_file() {
        return None;
    }
    match p.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "gba" => Some(Platform::Gba),
        "gb" => Some(Platform::Gb),
        "gbc" => Some(Platform::Gbc),
        _ => None,
    }
}

fn gb_title(p: &Path) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(p).ok()?;
    file.seek(SeekFrom::Start(0x134)).ok()?;
    let mut raw = [0u8; 16];
    file.read_exact(&mut raw).ok()?;
    // In newer CGB headers 0x13f..=0x142 is the manufacturer code and 0x143 is the
    // CGB flag, leaving eleven bytes for the title. Older headers use all sixteen bytes.
    let title_bytes = match raw[15] {
        0x80 | 0xc0 => &raw[..11],
        _ => &raw[..],
    };
    let end = title_bytes
        .iter()
        .position(|b| *b == 0)
        .unwrap_or(title_bytes.len());
    let title = String::from_utf8_lossy(&title_bytes[..end])
        .trim()
        .to_string();
    (!title.is_empty()).then_some(title)
}

/// A leading dot is card metadata rather than content, and every folder on the card is read
/// through this. macOS writes `._<name>` beside each file it copies onto a FAT volume, which
/// carries the extension of the file it shadows, so the extension alone cannot tell them
/// apart. It also sorts first, which is why the sidecar rather than the file is what a picker
/// walking the folder in order tends to land on.
pub fn is_hidden(p: &Path) -> bool {
    p.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with('.'))
}
