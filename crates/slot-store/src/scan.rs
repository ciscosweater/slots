use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::gba::{header_code, header_title};
use crate::platform::Platform;
use crate::read_favorites;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Cart {
    /// The platform folder is part of the cart identity. Stems may be shared by different
    /// systems, so it must travel with the ROM rather than be reconstructed from its extension.
    pub platform: Platform,
    /// Filename stem, which is the key for labels, saves and states. Not a content hash.
    pub stem: String,
    pub rom: PathBuf,
    /// Optional complete cartridge artwork. When present and decodable it replaces the generated
    /// shell/label face; `label` remains available as the fallback.
    pub artwork: Option<PathBuf>,
    pub label: Option<PathBuf>,
    pub title: String,
    /// The four character header game code, empty when the rom has none.
    pub code: String,
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

/// Scan platform folders. Loose files are also accepted as legacy GBA content so a caller that
/// scans before the boot migration still sees the library instead of an empty shelf.
pub fn scan(root: &Path) -> Result<Vec<Cart>, StoreError> {
    let mut carts = Vec::new();
    for platform in Platform::ALL {
        let dir = root.join("Games").join(platform.dir_name());
        scan_dir(root, &dir, platform, false, &mut carts)?;
    }
    // Builds before platform namespacing only supported GBA and kept ROMs in Games/.
    scan_dir(root, &root.join("Games"), Platform::Gba, true, &mut carts)?;
    sort_carts(root, &mut carts);
    Ok(carts)
}

/// Scan with a derived header index. The index is keyed by platform and filename, so two systems
/// may safely contain ROMs with the same basename.
pub fn scan_cached(root: &Path) -> Result<Vec<Cart>, StoreError> {
    let cache = read_cache(&root.join("System").join("library.index"));
    let mut carts = Vec::new();
    let mut records = BTreeMap::new();
    let mut dirty = cache.is_none();

    for platform in Platform::ALL {
        let dir = root.join("Games").join(platform.dir_name());
        scan_dir_cached(
            root,
            &dir,
            platform,
            false,
            cache.as_ref(),
            &mut records,
            &mut dirty,
            &mut carts,
        )?;
    }
    scan_dir_cached(
        root,
        &root.join("Games"),
        Platform::Gba,
        true,
        cache.as_ref(),
        &mut records,
        &mut dirty,
        &mut carts,
    )?;

    if cache.as_ref().is_some_and(|old| old.len() != records.len()) {
        dirty = true;
    }
    sort_carts(root, &mut carts);
    if dirty {
        let path = root.join("System").join("library.index");
        let text = encode_cache(&records);
        std::thread::spawn(move || {
            let _ = crate::atomic_write(&path, text.as_bytes());
        });
    }
    Ok(carts)
}

fn scan_dir(
    root: &Path,
    dir: &Path,
    platform: Platform,
    legacy: bool,
    carts: &mut Vec<Cart>,
) -> Result<(), StoreError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let rom = entry?.path();
        if is_hidden(&rom) || !rom.is_file() || !accepts(platform, &rom) {
            continue;
        }
        let Some(stem) = rom.file_stem().and_then(|s| s.to_str()).map(str::to_owned) else {
            continue;
        };
        carts.push(Cart {
            platform,
            stem: stem.clone(),
            title: title_for(platform, &rom),
            code: code_for(platform, &rom),
            artwork: artwork_path(root, platform, legacy, &stem),
            label: label_path(root, platform, legacy, &stem),
            rom,
        });
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn scan_dir_cached(
    root: &Path,
    dir: &Path,
    platform: Platform,
    legacy: bool,
    cache: Option<&BTreeMap<String, CachedRecord>>,
    records: &mut BTreeMap<String, CachedRecord>,
    dirty: &mut bool,
    carts: &mut Vec<Cart>,
) -> Result<(), StoreError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let rom = entry?.path();
        if is_hidden(&rom) || !rom.is_file() || !accepts(platform, &rom) {
            continue;
        }
        let Some(name) = rom.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(stem) = rom.file_stem().and_then(|s| s.to_str()).map(str::to_owned) else {
            continue;
        };
        let fingerprint = Fingerprint::from_meta(&std::fs::metadata(&rom)?);
        let key = cache_key(platform, legacy, name);
        let (title, code) = match cache
            .and_then(|all| all.get(&key))
            .filter(|r| r.fingerprint == fingerprint)
        {
            Some(r) => (r.title.clone(), r.code.clone()),
            None => {
                *dirty = true;
                (title_for(platform, &rom), code_for(platform, &rom))
            }
        };
        records.insert(
            key,
            CachedRecord {
                fingerprint,
                title: title.clone(),
                code: code.clone(),
            },
        );
        carts.push(Cart {
            platform,
            stem: stem.clone(),
            rom,
            artwork: artwork_path(root, platform, legacy, &stem),
            label: label_path(root, platform, legacy, &stem),
            title,
            code,
        });
    }
    Ok(())
}

fn title_for(platform: Platform, rom: &Path) -> String {
    match platform {
        Platform::Gba => header_title(rom).unwrap_or_default(),
        Platform::Gb | Platform::Gbc => crate::gb::title(rom).unwrap_or_default(),
    }
}

fn code_for(platform: Platform, rom: &Path) -> String {
    match platform {
        Platform::Gba => header_code(rom).unwrap_or_default(),
        Platform::Gb | Platform::Gbc => String::new(),
    }
}

fn accepts(platform: Platform, path: &Path) -> bool {
    platform.accepts(path)
}

fn label_path(root: &Path, platform: Platform, legacy: bool, stem: &str) -> Option<PathBuf> {
    let nested = root
        .join("Labels")
        .join(platform.dir_name())
        .join(format!("{stem}.png"));
    if nested.is_file() {
        Some(nested)
    } else if legacy || platform == Platform::Gba {
        let old = root.join("Labels").join(format!("{stem}.png"));
        old.is_file().then_some(old)
    } else {
        None
    }
}

fn artwork_path(root: &Path, platform: Platform, legacy: bool, stem: &str) -> Option<PathBuf> {
    let nested = root
        .join("Cartridges")
        .join(platform.dir_name())
        .join(format!("{stem}.png"));
    if nested.is_file() {
        Some(nested)
    } else if legacy || platform == Platform::Gba {
        let old = root.join("Cartridges").join(format!("{stem}.png"));
        old.is_file().then_some(old)
    } else {
        None
    }
}

fn sort_carts(root: &Path, carts: &mut [Cart]) {
    let favorites = read_favorites(root);
    carts.sort_by(|a, b| {
        favorites
            .contains(&b.stem)
            .cmp(&favorites.contains(&a.stem))
            .then_with(|| a.stem.cmp(&b.stem))
            .then_with(|| (a.platform as u8).cmp(&(b.platform as u8)))
    });
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Fingerprint {
    len: u64,
    modified_ns: u128,
}

impl Fingerprint {
    fn from_meta(meta: &std::fs::Metadata) -> Self {
        let modified_ns = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_nanos());
        Fingerprint {
            len: meta.len(),
            modified_ns,
        }
    }
}

#[derive(Clone)]
struct CachedRecord {
    fingerprint: Fingerprint,
    title: String,
    code: String,
}

fn cache_key(platform: Platform, legacy: bool, name: &str) -> String {
    if legacy {
        format!("legacy/{name}")
    } else {
        format!("{}/{name}", platform.dir_name())
    }
}

fn read_cache(path: &Path) -> Option<BTreeMap<String, CachedRecord>> {
    let text = std::fs::read_to_string(path).ok()?;
    if text.lines().next()? != "slot-library-index=2" {
        return None;
    }
    let mut out = BTreeMap::new();
    for line in text.lines().skip(1) {
        let mut fields = line.split('\t');
        let (name, len, modified_ns, title, code) = (
            unhex(fields.next()?)?,
            fields.next()?.parse().ok()?,
            fields.next()?.parse().ok()?,
            unhex(fields.next()?)?,
            unhex(fields.next()?)?,
        );
        if fields.next().is_some() {
            return None;
        }
        out.insert(
            name,
            CachedRecord {
                fingerprint: Fingerprint { len, modified_ns },
                title,
                code,
            },
        );
    }
    Some(out)
}

fn encode_cache(records: &BTreeMap<String, CachedRecord>) -> String {
    let mut out = String::from("slot-library-index=2\n");
    for (name, record) in records {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            hex(name),
            record.fingerprint.len,
            record.fingerprint.modified_ns,
            hex(&record.title),
            hex(&record.code)
        ));
    }
    out
}

fn hex(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn unhex(value: &str) -> Option<String> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    let bytes = (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).ok())
        .collect::<Option<Vec<_>>>()?;
    String::from_utf8(bytes).ok()
}

/// A leading dot is card metadata rather than content.
pub fn is_hidden(p: &Path) -> bool {
    p.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with('.'))
}
