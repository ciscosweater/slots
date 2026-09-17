mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use common::{clocked, tmp_root_with_carts, tmp_root_with_gb_carts};
use slot::app::Phase;
use slot::session::Session;
use slot::video_mode::{video_mode_for, VideoMode};
use slot_gfx::{SRC_H, SRC_W, WHOLE_TEXTURE};
use slot_input::{Btn, RawEvent};
use slot_retro::ButtonMask;

fn playing(root: &Path) -> Session {
    clocked(root);
    let mut session = Session::boot(root.to_path_buf());
    session.feed([RawEvent::Down(Btn::A)], 16);
    session.feed([RawEvent::Up(Btn::A)], 32);
    let deadline = Instant::now() + Duration::from_secs(2);
    while !matches!(session.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never reached the game");
        session.update(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(1));
    }
    session
}

#[test]
fn game_boy_shoulders_change_picture_mode_without_reaching_the_core() {
    let root = tmp_root_with_gb_carts(&["Tetris", "Zzz"]);
    let mut session = playing(root.path());
    let stretched = [
        40.0 / SRC_W as f32,
        4.0 / SRC_H as f32,
        160.0 / SRC_W as f32,
        144.0 / SRC_H as f32,
    ];

    assert_eq!(session.app().source_rect(), WHOLE_TEXTURE);
    session.feed([RawEvent::Down(Btn::L1)], 100);
    assert_eq!(session.app().video_mode(), VideoMode::Stretch);
    assert_eq!(session.app().source_rect(), stretched);
    assert_eq!(session.emu().unwrap().input().0 & ButtonMask::L, 0);

    session.feed([RawEvent::Down(Btn::R1)], 120);
    assert_eq!(session.app().video_mode(), VideoMode::Actual);
    assert_eq!(session.app().source_rect(), WHOLE_TEXTURE);
    assert_eq!(session.emu().unwrap().input().0 & ButtonMask::R, 0);
}

#[test]
fn picture_mode_is_loaded_and_saved_per_cart() {
    let root = tmp_root_with_gb_carts(&["Tetris", "Zzz"]);
    let mut session = playing(root.path());
    session.feed([RawEvent::Down(Btn::L1)], 100);
    assert_eq!(video_mode_for(root.path(), "Tetris"), VideoMode::Stretch);
    drop(session);

    let session = playing(root.path());
    assert_eq!(session.app().video_mode(), VideoMode::Stretch);
    assert_eq!(session.app().source_rect()[1], 4.0 / SRC_H as f32);
    drop(session);

    let gba = tmp_root_with_carts(&["Emerald", "Zzz"]);
    let mut session = playing(gba.path());
    session.feed([RawEvent::Down(Btn::L1)], 100);
    assert_eq!(session.app().video_mode(), VideoMode::Actual);
    assert_ne!(session.emu().unwrap().input().0 & ButtonMask::L, 0);
    assert_eq!(session.app().source_rect(), WHOLE_TEXTURE);
}
