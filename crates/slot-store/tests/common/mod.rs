use tempfile::TempDir;

pub fn tmp_root() -> TempDir {
    let d = tempfile::tempdir().expect("tempdir");
    for sub in [
        "Games",
        "Games/GBA",
        "Games/GB",
        "Games/GBC",
        "Cartridges",
        "Cartridges/GBA",
        "Cartridges/GB",
        "Cartridges/GBC",
        "Labels",
        "Labels/GBA",
        "Labels/GB",
        "Labels/GBC",
        "Saves",
        "States",
        "System",
    ] {
        std::fs::create_dir_all(d.path().join(sub)).expect("create content dir");
    }
    d
}
