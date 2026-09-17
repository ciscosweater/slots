mod common;

use std::collections::BTreeSet;

use common::tmp_root;
use slot_store::{scan, write_favorites, Platform};
use tempfile::TempDir;

/// `rel` is a path relative to `Games/`, e.g. `GBA/Pokemon Emerald.gba`.
fn write_rom(d: &TempDir, rel: &str, title: &str) {
    let mut rom = vec![0u8; 0x100];
    rom[0xa0..0xa0 + title.len()].copy_from_slice(title.as_bytes());
    let path = d.path().join("Games").join(rel);
    std::fs::create_dir_all(path.parent().expect("rom has a parent")).expect("create rom dir");
    std::fs::write(path, rom).expect("write rom");
}

/// `rel` is a path relative to `Labels/`, matching the ROM's platform folder.
fn write_png(d: &TempDir, rel: &str) {
    let path = d.path().join("Labels").join(rel);
    std::fs::create_dir_all(path.parent().expect("label has a parent")).expect("create label dir");
    std::fs::write(path, b"\x89PNG\r\n\x1a\n").expect("write png");
}

fn write_artwork(d: &TempDir, rel: &str) {
    let path = d.path().join("Cartridges").join(rel);
    std::fs::create_dir_all(path.parent().expect("artwork has a parent"))
        .expect("create artwork dir");
    std::fs::write(path, b"\x89PNG\r\n\x1a\n").expect("write artwork");
}

#[test]
fn a_png_in_labels_is_paired_to_its_rom_by_stem() {
    let d = tmp_root();
    write_rom(&d, "GBA/Pokemon Emerald.gba", "POKEMON EMER");
    write_png(&d, "GBA/Pokemon Emerald.png");
    write_rom(&d, "GBA/Advance Wars.gba", "ADVANCEWARS");
    let carts = scan(d.path()).unwrap();
    assert_eq!(carts.len(), 2);
    assert_eq!(carts[0].stem, "Advance Wars");
    assert!(carts[0].label.is_none());
    assert!(
        carts[1].label.is_some(),
        "a label in Labels/ was not picked up"
    );
    assert_eq!(carts[1].title, "POKEMON EMER");
}

#[test]
fn a_png_in_cartridges_is_paired_separately_from_the_label() {
    let d = tmp_root();
    write_rom(&d, "GBA/Pokemon Emerald.gba", "POKEMON EMER");
    write_png(&d, "GBA/Pokemon Emerald.png");
    write_artwork(&d, "GBA/Pokemon Emerald.png");
    let carts = scan(d.path()).unwrap();
    assert!(carts[0].label.is_some());
    assert!(carts[0].artwork.is_some());
}

#[test]
fn scan_accepts_gba_gb_and_gbc_and_ignores_other_files() {
    let d = tmp_root();
    write_rom(&d, "GBA/Real.gba", "REAL");
    let mut gb = vec![0u8; 0x150];
    gb[0x134..0x13a].copy_from_slice(b"TETRIS");
    std::fs::write(d.path().join("Games/GB/Tetris.gb"), &gb).unwrap();
    std::fs::write(d.path().join("Games/GBC/Zelda.GBC"), &gb).unwrap();
    std::fs::write(d.path().join("Games/GBA/notes.txt"), "hi").unwrap();
    let carts = scan(d.path()).unwrap();
    assert_eq!(carts.len(), 3);
    assert_eq!(
        carts.iter().map(|c| c.platform).collect::<Vec<_>>(),
        [Platform::Gba, Platform::Gb, Platform::Gbc,]
    );
    assert_eq!(carts[1].title, "TETRIS");
}

#[test]
fn a_modern_cgb_header_does_not_append_the_manufacturer_code_to_the_title() {
    let d = tmp_root();
    let mut gbc = vec![0u8; 0x150];
    gbc[0x134..0x13f].copy_from_slice(b"POKEMON RED");
    gbc[0x13f..0x143].copy_from_slice(b"ABCD");
    gbc[0x143] = 0x80;
    std::fs::write(d.path().join("Games/GBC/Pokemon.gbc"), gbc).unwrap();

    let carts = scan(d.path()).unwrap();
    assert_eq!(carts[0].title, "POKEMON RED");
}

#[test]
fn a_game_boy_header_uses_the_eleven_title_bytes_before_the_cgb_fields() {
    let d = tmp_root();
    let mut gb = vec![0u8; 0x150];
    gb[0x134..0x144].copy_from_slice(b"SIXTEEN BYTE NAM");
    std::fs::write(d.path().join("Games/GB/Old.gb"), gb).unwrap();

    let carts = scan(d.path()).unwrap();
    assert_eq!(carts[0].title, "SIXTEEN BYT");
}

#[test]
fn an_appledouble_sidecar_is_not_shelved_as_a_cart() {
    let d = tmp_root();
    write_rom(&d, "Metroid Fusion.gba", "METROID");
    // Copying a rom onto a FAT card from macOS leaves this beside it, carrying the same
    // extension and the same stem, so only the leading dot tells the two apart.
    write_rom(&d, "._Metroid Fusion.gba", "METROID");
    let carts = scan(d.path()).unwrap();
    assert_eq!(carts.len(), 1, "an AppleDouble sidecar reached the shelf");
    assert_eq!(carts[0].stem, "Metroid Fusion");
}

#[test]
fn header_title_of_a_truncated_rom_is_none_not_a_panic() {
    let d = tmp_root();
    std::fs::write(d.path().join("Games/GBA/Tiny.gba"), [0u8; 8]).unwrap();
    assert!(scan(d.path()).unwrap()[0].title.is_empty());
}

#[test]
fn a_header_title_that_is_not_text_is_dropped_rather_than_mangled() {
    let d = tmp_root();
    let mut rom = vec![0u8; 0x100];
    rom[0xa0..0xac].copy_from_slice(&[0xffu8; 12]);
    std::fs::write(d.path().join("Games/GBA/Garbage.gba"), rom).unwrap();
    assert!(scan(d.path()).unwrap()[0].title.is_empty());
}

#[test]
fn a_root_with_no_games_directory_scans_as_empty() {
    let d = tempfile::tempdir().unwrap();
    assert!(scan(d.path()).unwrap().is_empty());
}

#[test]
fn favorites_are_shelved_first_and_each_group_stays_alphabetical() {
    let d = tmp_root();
    for name in ["Advance.gba", "Boktai.gba", "Crash.gba", "Zelda.gba"] {
        write_rom(&d, &format!("GBA/{name}"), "GAME");
    }
    write_favorites(
        d.path(),
        &BTreeSet::from(["Boktai".to_string(), "Zelda".to_string()]),
    )
    .unwrap();
    let stems: Vec<_> = scan(d.path())
        .unwrap()
        .into_iter()
        .map(|cart| cart.stem)
        .collect();
    assert_eq!(stems, ["Boktai", "Zelda", "Advance", "Crash"]);
}
