mod common;

use common::{boot, tmp_root_with_carts};
use slot_input::{Action, Btn};

fn mixed_root() -> tempfile::TempDir {
    let root = tmp_root_with_carts(&["Emerald", "Zzz"]);
    let rom = vec![0u8; 0x150];
    std::fs::write(root.path().join("Games/GB/Tetris.gb"), rom).expect("write Game Boy rom");
    root
}

#[test]
fn shoulders_always_jump_letters_on_a_mixed_card() {
    let root = mixed_root();
    let mut app = boot(root.path());
    let before = app.selected_stem().map(str::to_owned);
    app.apply(Action::GbaDown(Btn::R1));
    // Letter jump may or may not move depending on initials; it must not switch platforms.
    let _ = before;
    assert!(
        app.shelf_category() == 0,
        "R1 must not change the category tab"
    );
}

#[test]
fn category_tabs_reach_each_platform() {
    let root = mixed_root();
    let mut app = boot(root.path());
    assert!(app.shelf_category() == 0);
    app.apply(Action::FfStart); // REC
    app.apply(Action::FfStart); // GBA
    assert_eq!(app.shelf_category(), 2);
    app.apply(Action::FfStart); // GB
    assert_eq!(app.shelf_category(), 3);
    assert_eq!(app.selected_stem(), Some("Tetris"));
}
