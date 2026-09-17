mod common;

use common::{boot, tmp_root_with_carts};
use slot::app::App;
use slot_gfx::TexId;
use slot_input::{Action, Btn};
use slot_store::Platform;

fn mark(app: &App, faces: &[TexId]) -> Option<TexId> {
    let mut out = Vec::new();
    app.draw(&mut out);
    faces.iter().copied().find(|face| {
        out.iter()
            .any(|draw| matches!(draw, slot_gfx::Draw::Tex { tex, .. } if tex == face))
    })
}

fn mixed_root() -> tempfile::TempDir {
    let root = tmp_root_with_carts(&["Emerald", "Zzz"]);
    let rom = vec![0u8; 0x150];
    std::fs::write(root.path().join("Games/GB/Tetris.gb"), rom).expect("write Game Boy rom");
    root
}

#[test]
fn the_shelf_mark_follows_the_active_platform() {
    let root = mixed_root();
    let mut app = boot(root.path());
    let faces: Vec<_> = (0..Platform::ALL.len())
        .map(|i| TexId::from_raw(700 + i))
        .collect();
    app.set_mark_faces(faces.clone());

    assert_eq!(mark(&app, &faces), Some(faces[0]));
    app.apply(Action::GbaDown(Btn::R1));
    assert_eq!(mark(&app, &faces), Some(faces[1]));
    assert_eq!(app.toast(), None);
}

#[test]
fn a_single_platform_card_has_no_platform_mark() {
    let root = tmp_root_with_carts(&["Emerald", "Zzz"]);
    let mut app = boot(root.path());
    let faces: Vec<_> = (0..Platform::ALL.len())
        .map(|i| TexId::from_raw(700 + i))
        .collect();
    app.set_mark_faces(faces.clone());
    assert_eq!(mark(&app, &faces), None);
}
