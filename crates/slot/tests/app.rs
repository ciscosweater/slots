mod common;

use std::collections::BTreeMap;
use std::time::Duration;

use slot::app::{App, Phase, EJECT_S, INSERT_S, SEATED_AT};
use slot::audio::Sfx;
use slot::session::Session;
use slot_input::{Action, Btn, RawEvent};
use slot_retro::ButtonMask;
use slot_store::{
    read_favorites, read_last_shelf, read_lcd, read_pixelify, read_recents, write_slot_state, Cart,
    Core, FaceButtons, SlotState,
};
use slot_ui::{
    board_at, grown, lid_at, on_board, opening, shelf_cart, Draw, Placed, Printed, TexId, Toast,
    BOARD_W, CART_W, CHIP_H, CHIP_U, CHIP_V, CHIP_W, HINT_EDGE, HINT_H, LID_TURN, SLIDE_UP,
    SOCKET_H, SOCKET_U, SOCKET_V, SOCKET_W, TURN_PAD,
};

/// A tap of A, which is what plays a cart. The press alone is not enough: held, it means
/// start the cart clean, and the app cannot know which until the finger comes off.
fn play(a: &mut App) {
    a.apply(Action::GbaDown(Btn::A));
    a.apply(Action::GbaUp(Btn::A));
}

#[test]
fn shoulders_jump_between_letters_on_the_shelf() {
    let mut app = app_with_carts(&["Advance", "Astro", "Boktai", "Castlevania", "Crash"]);
    app.apply(Action::GbaDown(Btn::R1));
    assert_eq!(app.selected_stem(), Some("Boktai"));
    app.apply(Action::GbaDown(Btn::L1));
    assert_eq!(app.selected_stem(), Some("Advance"));
}

fn app_with_carts(stems: &[&str]) -> App {
    App::new(
        stems
            .iter()
            .map(|stem| Cart {
                stem: (*stem).to_string(),
                rom: format!("Games/{stem}.gba").into(),
                artwork: None,
                label: None,
                code: String::new(),
                title: stem.to_uppercase(),
                platform: slot_store::Platform::Gba,
            })
            .collect(),
    )
}

/// By colour. Sizing the detector to the bands meant that reshaping the slot made it match
/// nothing, which turned one test red and made its opposite pass for the wrong reason.
fn is_mouth(d: &Draw) -> bool {
    match *d {
        Draw::Rect { colour, .. } => (0..3).all(|i| (colour[i] - opening()[i]).abs() < 0.001),
        _ => false,
    }
}

/// Two carts, because a lone cart is a dedicated device and has nowhere to eject to.
fn playing(stem: &str) -> App {
    let mut a = app_with_carts(&[stem, "Zzz"]);
    a.apply(Action::Insert);
    a.on_core_ready();
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    a
}

#[test]
fn insert_waits_for_the_core_even_after_the_animation_floor() {
    let mut a = app_with_carts(&["Emerald"]);
    a.apply(Action::Insert);
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    } // 2s, well past the floor
    assert!(
        matches!(a.phase(), Phase::Inserting { .. }),
        "advanced without the core"
    );
    a.on_core_ready();
    a.update(1.0 / 60.0);
    assert!(matches!(a.phase(), Phase::Playing { .. }));
}

#[test]
fn insert_does_not_advance_before_the_animation_floor_even_if_the_core_is_instant() {
    let mut a = app_with_carts(&["Emerald"]);
    a.apply(Action::Insert);
    a.on_core_ready();
    a.update(1.0 / 60.0);
    assert!(matches!(a.phase(), Phase::Inserting { .. }));
}

/// Seconds of frames until the app gets where it is going, giving up rather than hanging.
fn seconds_until(a: &mut App, done: fn(&App) -> bool) -> f32 {
    let mut t = 0.0;
    while !done(a) && t < 5.0 {
        a.update(1.0 / 60.0);
        t += 1.0 / 60.0;
    }
    t
}

/// Long enough to read as a cart being pushed rather than a wipe, and no longer than the
/// recording of one: the travel is cut to fit the sound, not the other way round.
#[test]
fn the_insert_reads_as_a_push_and_the_eject_takes_the_same_time() {
    let mut a = app_with_carts(&["Emerald", "Zzz"]);
    a.apply(Action::Insert);
    a.on_core_ready();
    let insert = seconds_until(&mut a, |a| matches!(a.phase(), Phase::Playing { .. }));
    a.apply(Action::Eject);
    let eject = seconds_until(&mut a, |a| matches!(a.phase(), Phase::Shelf));
    assert!(insert >= 0.4, "insert is {insert}s, still a wipe");
    // Longer than the travel, because the picture has to go out and the cart waits a beat
    // after it. That the two travels match is `the_eject_is_the_insert_run_backwards`.
    assert!(
        eject > EJECT_S,
        "the eject is {eject}s, so the cart moved before the picture was out"
    );
}

#[test]
fn the_game_does_not_appear_the_instant_the_cart_seats() {
    let mut a = app_with_carts(&["Emerald"]);
    a.apply(Action::Insert);
    a.on_core_ready();
    while a.seat() < 1.0 {
        a.update(1.0 / 60.0);
    }
    assert!(
        matches!(a.phase(), Phase::Inserting { .. }),
        "revealed on the same frame it seated"
    );
    // Derived, not counted: the beat is set from the length of the sound of the cart
    // landing, so a different recording moves it.
    let beat = ((INSERT_S - SEATED_AT) * 60.0).ceil() as u32 + 1;
    for _ in 0..beat {
        a.update(1.0 / 60.0);
    }
    assert!(matches!(a.phase(), Phase::Playing { .. }));
}

/// The game must be invisible for the whole insert, not merely dimmed. Watching it play
/// behind the cart is what made the animation feel like it was covering nothing.
///
/// Two carts, so the cart actually travels: a lone cart resumes straight into the slot and
/// the only Inserting frames are the ones the core spends loading. The sleep is the worker
/// thread's, which is the other half of the race this is about.
#[test]
fn the_game_does_not_draw_during_the_insert() {
    let d = common::tmp_root_with_real_carts(&["Emerald", "Fusion"]);
    common::clocked(d.path());
    let mut s = Session::boot(d.path().to_path_buf());
    s.feed([RawEvent::Down(Btn::A), RawEvent::Up(Btn::A)], 16);
    for i in 0..120 {
        s.update(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(1));
        if matches!(s.app().phase(), Phase::Inserting { .. }) {
            assert!(
                !s.game_visible(),
                "frame {i}: the game is playing behind the cart"
            );
        }
    }
    assert!(
        s.game_visible(),
        "the core never published, so the insert proved nothing"
    );
}

#[test]
fn the_reveal_waits_for_the_power_on_to_finish() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    let mut a = App::boot(d.path());
    a.apply(Action::Insert);
    a.on_core_ready();
    while a.seat() < 1.0 {
        a.update(1.0 / 60.0);
    }
    assert!(
        a.screen_power() < 1.0,
        "the screen was already on when the cart landed"
    );
    for _ in 0..20 {
        a.update(1.0 / 60.0);
    }
    assert!((a.screen_power() - 1.0).abs() < 0.01);
}

/// The noise belongs to the contacts, not to the button. A click at the top of the travel
/// would be a cart that announced itself before it went anywhere.
#[test]
fn the_cart_sounds_when_it_reaches_the_slot_and_not_when_it_starts_moving() {
    let mut a = app_with_carts(&["Emerald", "Zzz"]);
    a.apply(Action::Insert);
    a.update(1.0 / 60.0);
    assert_eq!(a.take_sfx(), None, "it sounded before it touched anything");
    let mut heard = None;
    while a.seat() < 1.0 && heard.is_none() {
        a.update(1.0 / 60.0);
        heard = a.take_sfx();
    }
    assert_eq!(heard, Some(Sfx::Insert));
}

/// A cart already in the slot at boot never travelled, so it never touched the rails.
#[test]
fn a_resumed_cart_makes_no_sound() {
    let d = common::tmp_root_with_carts(&["Emerald", "Zzz"]);
    write_slot_state(
        d.path(),
        &SlotState {
            cart: Some("Emerald".into()),
            clock_set: true,
            utc_offset_min: 0,
            ..Default::default()
        },
    )
    .unwrap();
    let mut a = App::boot(d.path());
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    assert_eq!(a.take_sfx(), None);
}

/// Not when the button was held: the picture has to finish going out first, and the contacts
/// letting go is the sound of the cart starting to move rather than of the decision to move
/// it.
#[test]
fn the_cart_sounds_as_it_comes_free_and_not_before_the_screen_is_out() {
    let mut a = playing("Emerald");
    a.take_sfx();
    a.apply(Action::Eject);
    assert_eq!(a.take_sfx(), None, "it sounded over a live picture");
    let mut heard = None;
    for _ in 0..120 {
        a.update(1.0 / 60.0);
        if let Some(s) = a.take_sfx() {
            heard = Some(s);
            break;
        }
    }
    assert_eq!(heard, Some(Sfx::Eject));
    assert_eq!(a.screen_power(), 0.0, "the picture was still going out");
}

#[test]
fn a_cart_that_fails_to_load_returns_to_the_shelf() {
    let mut a = app_with_carts(&["Broken"]);
    a.apply(Action::Insert);
    a.on_core_failed();
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    assert!(matches!(a.phase(), Phase::Shelf));
}

#[test]
fn a_refused_cart_pushes_back_out_from_where_it_caught() {
    let mut a = app_with_carts(&["Broken"]);
    a.apply(Action::Insert);
    for _ in 0..6 {
        a.update(1.0 / 60.0);
    }
    let caught = a.seat();
    assert!(
        caught > 0.05 && caught < 0.95,
        "test needs a part seated cart, got {caught}"
    );
    a.on_core_failed();
    assert!(
        (a.seat() - caught).abs() < 1e-3,
        "cart jumped from {caught} to {}",
        a.seat()
    );
}

#[test]
fn an_empty_shelf_has_nothing_to_insert() {
    let mut a = app_with_carts(&[]);
    a.apply(Action::Insert);
    assert!(matches!(a.phase(), Phase::Shelf));
}

#[test]
fn an_empty_shelf_prints_on_the_case() {
    let mut a = app_with_carts(&[]);
    a.set_empty_caption(Printed::new(TexId::from_raw(3), 120));
    let mut out = Vec::new();
    a.draw(&mut out);
    assert!(
        out.iter().any(
            |d| matches!(d, Draw::Tex { y, w, .. } if (*y - 316.0).abs() < 0.1 && *w == 120.0)
        ),
        "the empty shelf did not print on the case: {out:?}"
    );
}

#[test]
fn face_buttons_drive_the_shelf_only_while_it_is_showing() {
    let mut a = app_with_carts(&["Emerald", "Wars"]);
    a.apply(Action::GbaDown(Btn::Right));
    play(&mut a);
    let Phase::Inserting { cart, .. } = a.phase() else {
        panic!("A on the shelf did not insert: {:?}", a.phase())
    };
    assert_eq!(cart, "Wars");

    a.on_core_ready();
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    // Left belongs to the game now and must not walk the shelf out from under it.
    a.apply(Action::GbaDown(Btn::Left));
    a.apply(Action::Eject);
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    a.apply(Action::Insert);
    let Phase::Inserting { cart, .. } = a.phase() else {
        panic!("insert after eject did nothing: {:?}", a.phase())
    };
    assert_eq!(cart, "Wars", "the game's d-pad moved the shelf behind it");
}

/// The repeat is the shelf's own, but only the app sees the up edge, so a direction let go of
/// has to reach it or the row walks on by itself.
#[test]
fn a_held_direction_walks_the_shelf_and_a_release_stops_it() {
    let mut a = app_with_carts(&["A", "B", "C", "D", "E", "F", "G"]);
    a.apply(Action::GbaDown(Btn::Right));
    for _ in 0..30 {
        a.update(1.0 / 60.0);
    }
    a.apply(Action::GbaUp(Btn::Right));
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    play(&mut a);
    let Phase::Inserting { cart, .. } = a.phase() else {
        panic!("A on the shelf did not insert: {:?}", a.phase())
    };
    assert_eq!(cart, "C", "one press and one repeat, then nothing");
}

#[test]
fn a_cart_in_flight_is_not_also_left_standing_on_the_shelf() {
    let mut a = app_with_carts(&["Emerald"]);
    a.apply(Action::Insert);
    a.update(0.2);
    let mut out = Vec::new();
    a.draw(&mut out);
    let carts = out
        .iter()
        .filter(|d| match **d {
            Draw::Rect { w, .. } | Draw::Tex { w, .. } | Draw::Turned { w, .. } => {
                (w - CART_W as f32).abs() < 0.01
            }
            Draw::Game | Draw::Shot { .. } => false,
        })
        .count();
    assert_eq!(
        carts, 1,
        "the shelf still holds the cart the slot is taking"
    );
}

/// The slot is part of the device, not part of the animation, so it is on screen before
/// anything is pushed into it. This test used to assert the opposite: the slot was a black
/// bar against a grey backdrop then, and hiding it was the wrong fix for the wrong problem.
#[test]
fn the_shelf_shows_the_empty_slot() {
    let a = app_with_carts(&["Emerald", "Zzz"]);
    let mut out = Vec::new();
    a.draw(&mut out);
    assert!(
        out.iter().any(is_mouth),
        "the shelf has no slot, so the cart has nowhere visible to go"
    );
}

#[test]
fn inserting_still_has_a_mouth_to_go_into() {
    let mut a = app_with_carts(&["Emerald"]);
    a.apply(Action::Insert);
    a.update(1.0 / 60.0);
    let mut out = Vec::new();
    a.draw(&mut out);
    assert!(
        out.iter().any(is_mouth),
        "the cart has nothing to slide into"
    );
}

#[test]
fn eject_returns_to_the_shelf_only_once_the_cart_is_out() {
    let mut a = playing("Emerald");
    a.apply(Action::Eject);
    dark(&mut a);
    // Stated as the thing itself rather than as a duration: the shelf is not allowed back
    // while any part of the cart is still in the slot, however long the travel and the beat
    // before it happen to be.
    while a.seat() > 0.0 {
        assert!(
            matches!(a.phase(), Phase::Ejecting { .. }),
            "the shelf came back with the cart {} of the way in",
            a.seat()
        );
        a.update(1.0 / 60.0);
    }
    a.update(1.0 / 60.0);
    assert!(matches!(a.phase(), Phase::Shelf));
}

/// Frames until the panel is out, giving up rather than hanging on a screen that never goes
/// dark. The eject is two movements now and the cart's is the second of them.
fn dark(a: &mut App) {
    for _ in 0..300 {
        if a.screen_power() == 0.0 {
            return;
        }
        a.update(1.0 / 60.0);
    }
    panic!("the screen never went dark");
}

fn lists_game(a: &App) -> bool {
    let mut out = Vec::new();
    a.draw(&mut out);
    out.iter().any(|d| matches!(d, Draw::Game))
}

/// The picture is an item in the draw list rather than a pass before it, which is what puts
/// it in front of the cart. The list is therefore also where a screen that never came up at
/// all would show, and nothing else in the tree renders one.
#[test]
fn the_game_layer_is_listed_only_once_the_screen_is_up() {
    let mut a = app_with_carts(&["Emerald", "Zzz"]);
    a.set_game_ready(true);
    a.apply(Action::Insert);
    a.on_core_ready();
    while a.seat() < 1.0 {
        a.update(1.0 / 60.0);
        assert!(
            !lists_game(&a),
            "the picture is drawn while the cart is still going in"
        );
    }
    for _ in 0..30 {
        a.update(1.0 / 60.0);
    }
    assert!(lists_game(&a), "the game never reached the draw list");
}

/// Quitting runs the insert backwards. A cart travelling out across a live picture is two
/// movements at once, and the picture is the one in front.
#[test]
fn the_cart_waits_for_the_screen_to_go_dark() {
    let d = common::tmp_root_with_carts(&["Emerald", "Zzz"]);
    let mut a = common::app_playing_in(d.path(), "Emerald");
    a.apply(Action::Eject);
    while a.screen_power() > 0.0 {
        assert_eq!(
            a.seat(),
            1.0,
            "the cart started leaving while the screen was still lit"
        );
        a.update(1.0 / 60.0);
        assert!(a.now() < 5_000, "the screen never went dark");
    }
    for _ in 0..40 {
        a.update(1.0 / 60.0);
    }
    assert!(
        a.seat() < 1.0,
        "the cart never left once the screen was dark"
    );
}

#[test]
fn eject_is_the_insert_backwards() {
    let d = common::tmp_root_with_carts(&["Emerald", "Zzz"]);
    let mut a = common::app_playing_in(d.path(), "Emerald");
    a.apply(Action::Eject);
    let first = a.screen_power();
    a.update(1.0 / 60.0);
    assert!(a.screen_power() < first, "the screen is not closing");
}

/// The shelf MENU opens the full quick menu; in-game MENU opens display settings over the cart.
#[test]
fn the_quick_menu_opens_from_the_shelf_and_over_a_game() {
    // Two carts, or `single_cart` makes this a dedicated device: one cart is seated at boot
    // whatever the state says, and the shelf is never on screen to press MENU from.
    let d = common::tmp_root_with_carts(&["Emerald", "Fusion"]);
    let (mut s, _motor) = common::session_with_platform(d.path());

    assert!(
        matches!(s.app().phase(), slot::app::Phase::Shelf),
        "not on the shelf: {:?}",
        s.app().phase()
    );
    s.app_mut().apply(slot_input::Action::QuickMenu);
    assert!(matches!(
        s.app().phase(),
        slot::app::Phase::QuickMenu { resume: None, .. }
    ));
    s.app_mut()
        .apply(slot_input::Action::GbaDown(slot_input::Btn::B));

    let mut a = common::app_playing_in(d.path(), "Emerald");
    a.apply(slot_input::Action::QuickMenu);
    assert!(a.play_settings_open());
    assert_eq!(a.quick_menu(), Some(slot_ui::QuickRow::LcdEffect));
    a.apply(slot_input::Action::GbaDown(slot_input::Btn::B));
    assert!(matches!(a.phase(), slot::app::Phase::Playing { .. }));
}

/// Both ways out of the label land back on the quick menu, on its About row. MENU works as well
/// as B, so the button that brought the user here gets them back.
#[test]
fn both_b_and_menu_take_the_about_screen_back_to_the_quick_menu() {
    let d = common::tmp_root_with_carts(&["Emerald", "Fusion"]);
    for out in [
        slot_input::Action::GbaDown(slot_input::Btn::B),
        slot_input::Action::QuickMenu,
    ] {
        let (mut s, _motor) = common::session_with_platform(d.path());
        s.app_mut().apply(slot_input::Action::QuickMenu);
        for _ in 0..slot_ui::QuickRow::About.index() {
            s.app_mut()
                .apply(slot_input::Action::GbaDown(slot_input::Btn::Down));
        }
        s.app_mut()
            .apply(slot_input::Action::GbaDown(slot_input::Btn::A));
        assert!(matches!(s.app().phase(), slot::app::Phase::About));
        s.app_mut().apply(out);
        assert_eq!(
            s.app().quick_menu(),
            Some(slot_ui::QuickRow::About),
            "{out:?} did not take the label back to the menu"
        );
    }
}

#[test]
fn left_and_right_turn_the_about_plate_over() {
    let (_d, mut app) = on_shelf(&["Emerald", "Fusion"]);
    app.apply(Action::QuickMenu);
    for _ in 0..slot_ui::QuickRow::About.index() {
        app.apply(Action::GbaDown(Btn::Down));
    }
    app.apply(Action::GbaDown(Btn::A));
    assert_eq!(app.about_page(), slot_ui::StickerPage::Credits);
    app.apply(Action::GbaDown(Btn::Right));
    assert_eq!(app.about_page(), slot_ui::StickerPage::Controls);
    app.apply(Action::GbaDown(Btn::Left));
    assert_eq!(app.about_page(), slot_ui::StickerPage::Credits);
}

#[test]
fn holding_a_on_about_opens_the_clock() {
    let (_d, mut app) = on_shelf(&["Emerald", "Fusion"]);
    app.apply(Action::QuickMenu);
    for _ in 0..slot_ui::QuickRow::About.index() {
        app.apply(Action::GbaDown(Btn::Down));
    }
    app.apply(Action::GbaDown(Btn::A));
    app.apply(Action::GbaDown(Btn::A));
    for _ in 0..40 {
        app.update(1.0 / 60.0);
    }
    assert!(
        matches!(app.phase(), Phase::SetClock { .. }),
        "hold A on the label did not open the clock: {:?}",
        app.phase()
    );
}

#[test]
fn tapping_a_on_about_leaves_a_live_clock_alone() {
    let (_d, mut app) = on_shelf(&["Emerald", "Fusion"]);
    app.apply(Action::QuickMenu);
    for _ in 0..slot_ui::QuickRow::About.index() {
        app.apply(Action::GbaDown(Btn::Down));
    }
    app.apply(Action::GbaDown(Btn::A));
    app.apply(Action::GbaDown(Btn::A));
    app.apply(Action::GbaUp(Btn::A));
    assert!(
        matches!(app.phase(), Phase::About),
        "tap A opened the clock while it was already set: {:?}",
        app.phase()
    );
}

#[test]
fn idle_hints_do_not_appear_on_shelf() {
    let (_d, mut app) = on_shelf(&["Emerald", "Fusion"]);
    app.set_shelf_idle_faces(vec![(TexId::from_raw(7), 40), (TexId::from_raw(8), 50)]);
    let mut out = Vec::new();
    app.draw(&mut out);
    assert!(
        !out.iter()
            .any(|d| matches!(d, Draw::Tex { tex, .. } if *tex == TexId::from_raw(7))),
        "the idle hint was on before anyone waited"
    );
    app.update(5.0);
    out.clear();
    app.draw(&mut out);
    assert!(
        !out.iter()
            .any(|d| matches!(d, Draw::Tex { tex, .. } if *tex == TexId::from_raw(7))),
        "idle hints should not appear on the minimalist shelf"
    );
}

/// A booted app sitting on the shelf, beside the card it reads and writes. Two carts at
/// least: one cart is a dedicated device, and `App::boot` seats it rather than leaving a
/// shelf to press anything on. `clock_set` because a card that has never been asked the
/// time opens on the clock screen, which owns every button.
fn on_shelf(stems: &[&str]) -> (tempfile::TempDir, App) {
    let d = common::tmp_root_with_carts(stems);
    write_slot_state(
        d.path(),
        &SlotState {
            clock_set: true,
            ..Default::default()
        },
    )
    .unwrap();
    let app = App::boot(d.path());
    (d, app)
}

#[test]
fn y_toggles_a_persistent_favorite_without_reordering_the_shelf() {
    let (d, mut app) = on_shelf(&["Advance", "Boktai", "Crash"]);
    app.apply(Action::GbaDown(Btn::Right));
    app.apply(Action::GbaDown(Btn::Right));
    assert_eq!(app.selected_stem(), Some("Crash"));

    app.apply(Action::GbaDown(Btn::Y));
    assert_eq!(app.selected_stem(), Some("Crash"));
    assert_eq!(
        app.shelf_index(),
        2,
        "favoriting Crash jumped it to the front of ALL"
    );
    assert_eq!(app.toast(), Some(Toast::Favorited));
    assert!(read_favorites(d.path()).contains("Crash"));

    let mut rebooted = App::boot(d.path());
    assert_eq!(rebooted.selected_stem(), Some("Crash"));
    rebooted.apply(Action::GbaDown(Btn::Y));
    assert_eq!(rebooted.toast(), Some(Toast::Unfavorited));
    assert!(read_favorites(d.path()).is_empty());
}

#[test]
fn the_favorites_tab_filters_the_shelf_and_removes_a_cart_immediately() {
    let (d, mut app) = on_shelf(&["Advance", "Boktai", "Crash"]);
    app.apply(Action::GbaDown(Btn::Y));
    assert!(read_favorites(d.path()).contains("Advance"));

    // ALL -> REC -> GBA -> FAVORITES. GB and GBC are skipped because this card has none.
    for _ in 0..3 {
        app.apply(Action::FfStart);
    }
    assert_eq!(app.shelf_category(), 5);
    assert_eq!(app.shelf_total(), 1);
    assert_eq!(app.selected_stem(), Some("Advance"));

    app.apply(Action::GbaDown(Btn::Y));
    assert_eq!(app.shelf_total(), 0);
    assert_eq!(app.toast(), Some(Toast::Unfavorited));
    assert!(read_favorites(d.path()).is_empty());
}

#[test]
fn switching_to_favorites_does_not_steal_the_all_selection() {
    let (_d, mut app) = on_shelf(&["Advance", "Boktai", "Crash"]);
    app.apply(Action::GbaDown(Btn::Y));
    app.apply(Action::GbaDown(Btn::Right));
    app.apply(Action::GbaDown(Btn::Right));
    assert_eq!(app.selected_stem(), Some("Crash"));

    for _ in 0..3 {
        app.apply(Action::FfStart);
    }
    assert_eq!(app.shelf_category(), 5);
    assert_eq!(app.selected_stem(), Some("Advance"));

    for _ in 0..3 {
        app.apply(Action::RewindStart);
    }
    assert_eq!(app.shelf_category(), 0);
    assert_eq!(
        app.selected_stem(),
        Some("Crash"),
        "coming back from Favorites landed on the starred cart"
    );
}

#[test]
fn the_shelf_selection_survives_a_reboot_and_a_render() {
    let (d, mut app) = on_shelf(&["Advance", "Boktai", "Crash"]);
    app.apply(Action::GbaDown(Btn::Right));
    app.apply(Action::GbaDown(Btn::Right));
    assert_eq!(app.selected_stem(), Some("Crash"));
    assert_eq!(read_last_shelf(d.path()).stem.as_deref(), Some("Crash"));

    let rebooted = App::boot(d.path());
    assert_eq!(rebooted.selected_stem(), Some("Crash"));
    let mut out = Vec::new();
    rebooted.draw(&mut out);
    assert_eq!(rebooted.selected_stem(), Some("Crash"));
}

#[test]
fn a_held_shelf_direction_persists_the_cart_reached_by_repeat() {
    let (d, mut app) = on_shelf(&["Advance", "Boktai", "Crash", "Emerald"]);
    app.apply(Action::GbaDown(Btn::Right));
    assert_eq!(app.selected_stem(), Some("Boktai"));

    // The first repeat is due after 400 ms. `update` also performs the persistence check used by
    // the frame loop, rather than relying only on the initial button press.
    app.update(0.4);
    assert_eq!(app.selected_stem(), Some("Crash"));
    assert_eq!(read_last_shelf(d.path()).stem.as_deref(), Some("Crash"));
    app.apply(Action::GbaUp(Btn::Right));
}

#[test]
fn a_game_becomes_recent_only_after_its_core_is_ready() {
    let (d, mut app) = on_shelf(&["Advance", "Boktai"]);
    play(&mut app);
    app.update(INSERT_S + 0.1);
    assert!(read_recents(d.path()).is_empty());

    app.on_core_ready();
    app.update(1.0 / 60.0);
    assert_eq!(read_recents(d.path()), ["Advance"]);
}

#[test]
fn a_failed_game_does_not_become_recent() {
    let (d, mut app) = on_shelf(&["Advance", "Boktai"]);
    play(&mut app);
    app.on_core_failed();
    for _ in 0..120 {
        app.update(1.0 / 60.0);
    }
    assert!(read_recents(d.path()).is_empty());
}

#[test]
fn the_recents_category_survives_a_reboot() {
    let (d, mut app) = on_shelf(&["Advance", "Boktai", "Crash"]);
    app.apply(Action::GbaDown(Btn::Right));
    play(&mut app);
    app.on_core_ready();
    app.update(INSERT_S + 0.1);
    assert_eq!(read_recents(d.path()), ["Boktai"]);

    // Clear the seated cart so this boot opens on the shelf, then enter REC from ALL.
    write_slot_state(
        d.path(),
        &SlotState {
            clock_set: true,
            ..Default::default()
        },
    )
    .unwrap();
    let mut rebooted = App::boot(d.path());
    rebooted.apply(Action::FfStart);
    assert_eq!(rebooted.shelf_category(), 1);
    assert_eq!(rebooted.selected_stem(), Some("Boktai"));

    // Category itself is durable: leave REC selected, reboot, land on REC without pressing R2.
    write_slot_state(
        d.path(),
        &SlotState {
            clock_set: true,
            ..Default::default()
        },
    )
    .unwrap();
    let again = App::boot(d.path());
    assert_eq!(again.shelf_category(), 1);
    assert_eq!(again.selected_stem(), Some("Boktai"));
}

#[test]
fn x_toggles_the_persistent_lcd_effect() {
    let (d, mut app) = on_shelf(&["Advance", "Boktai"]);
    // ALL does not own display writes; the GBA tab does.
    app.apply(Action::FfStart);
    app.apply(Action::FfStart);
    assert_eq!(app.shelf_category(), 2);
    assert!(app.lcd_enabled());

    app.apply(Action::GbaDown(Btn::X));
    assert!(!app.lcd_enabled());
    assert_eq!(app.toast(), Some(Toast::LcdOff));
    assert!(
        !slot_store::resolve_display(
            d.path(),
            slot_store::Platform::Gba,
            None,
            slot_store::DisplayPrefs::built_in(),
        )
        .lcd
    );
    assert!(read_lcd(d.path()), "legacy lcd.txt stays as the fallback");

    let mut rebooted = App::boot(d.path());
    // Last shelf was the GBA tab, so boot resolves that platform default.
    assert_eq!(rebooted.shelf_category(), 2);
    assert!(!rebooted.lcd_enabled());
    rebooted.apply(Action::GbaDown(Btn::X));
    assert!(rebooted.lcd_enabled());
    assert_eq!(rebooted.toast(), Some(Toast::LcdOn));
}

#[test]
fn y_toggles_colour_correction_with_a_toast_while_playing() {
    let (d, mut app) = on_shelf(&["Advance", "Boktai"]);
    play(&mut app);
    app.on_core_ready();
    for _ in 0..120 {
        app.update(1.0 / 60.0);
    }
    assert!(matches!(app.phase(), Phase::Playing { .. }));
    assert!(!app.colour_correction());

    app.apply(Action::GbaDown(Btn::Y));
    assert!(app.colour_correction());
    assert_eq!(app.toast(), Some(Toast::ColourOn));
    let stem = app.seated_cart().map(|c| c.stem.clone()).unwrap();
    assert!(
        slot_store::resolve_display(
            d.path(),
            slot_store::Platform::Gba,
            Some(&stem),
            slot_store::DisplayPrefs::built_in(),
        )
        .colour
    );

    app.apply(Action::GbaDown(Btn::Y));
    assert!(!app.colour_correction());
    assert_eq!(app.toast(), Some(Toast::ColourOff));
    assert!(
        !slot_store::resolve_display(
            d.path(),
            slot_store::Platform::Gba,
            Some(&stem),
            slot_store::DisplayPrefs::built_in(),
        )
        .colour
    );
}

#[test]
fn lcd_effect_stays_on_when_the_picture_is_stretched() {
    let mut app = App::new(vec![
        Cart {
            stem: "Tetris".into(),
            rom: "Games/Tetris.gb".into(),
            artwork: None,
            label: None,
            code: String::new(),
            title: "TETRIS".into(),
            platform: slot_store::Platform::Gb,
        },
        Cart {
            stem: "Zzz".into(),
            rom: "Games/Zzz.gba".into(),
            artwork: None,
            label: None,
            code: String::new(),
            title: "ZZZ".into(),
            platform: slot_store::Platform::Gba,
        },
    ]);
    play(&mut app);
    app.on_core_ready();
    for _ in 0..120 {
        app.update(1.0 / 60.0);
    }
    assert!(app.lcd_enabled());
    app.apply(Action::GbaDown(Btn::L1));
    assert_eq!(app.video_mode(), slot::video_mode::VideoMode::Stretch);
    assert!(app.lcd_enabled(), "stretch cleared the LCD effect");
    assert_eq!(app.toast(), Some(Toast::FillScreen));
    app.apply(Action::GbaDown(Btn::X));
    assert!(!app.lcd_enabled());
    assert_eq!(app.toast(), Some(Toast::LcdOff));
    app.apply(Action::GbaDown(Btn::X));
    assert!(app.lcd_enabled(), "LCD could not come back over stretch");
    assert_eq!(app.video_mode(), slot::video_mode::VideoMode::Stretch);
    assert_eq!(app.toast(), Some(Toast::LcdOn));
}

#[test]
fn gb_and_gbc_can_toggle_lcd_and_that_hides_the_overlay() {
    for platform in [slot_store::Platform::Gb, slot_store::Platform::Gbc] {
        let mut app = App::new(vec![
            Cart {
                stem: "Tetris".into(),
                rom: if platform == slot_store::Platform::Gb {
                    "Games/Tetris.gb".into()
                } else {
                    "Games/Tetris.gbc".into()
                },
                artwork: None,
                label: None,
                code: String::new(),
                title: "TETRIS".into(),
                platform,
            },
            Cart {
                stem: "Zzz".into(),
                rom: "Games/Zzz.gba".into(),
                artwork: None,
                label: None,
                code: String::new(),
                title: "ZZZ".into(),
                platform: slot_store::Platform::Gba,
            },
        ]);

        assert!(app.lcd_enabled(), "the shelf keeps the saved setting");
        app.apply(Action::Insert);
        app.on_core_ready();
        for _ in 0..120 {
            app.update(1.0 / 60.0);
        }
        assert!(
            matches!(app.phase(), Phase::Playing { .. }),
            "{platform:?} never reached Playing"
        );
        assert!(
            app.lcd_enabled(),
            "{platform:?} cleared the LCD preference on insert"
        );
        assert!(
            !app.gb_overlay_visible(),
            "{platform:?} drew the overlay over an LCD picture"
        );
        app.apply(Action::GbaDown(Btn::X));
        assert!(!app.lcd_enabled(), "{platform:?} ignored X");
        assert!(
            app.gb_overlay_visible(),
            "{platform:?} hid the overlay with LCD off"
        );
        app.apply(Action::GbaDown(Btn::X));
        assert!(app.lcd_enabled(), "{platform:?} could not turn LCD back on");
        assert!(!app.gb_overlay_visible());
    }
}

#[test]
fn select_toggles_the_persistent_font() {
    let d = common::tmp_root_with_carts(&["Advance", "Boktai"]);
    write_slot_state(
        d.path(),
        &SlotState {
            clock_set: true,
            ..Default::default()
        },
    )
    .unwrap();
    let (mut session, _motor) = common::session_with_platform(d.path());
    assert!(read_pixelify(d.path()));
    let revision = session.app().font_revision();

    session.feed([RawEvent::Down(Btn::Select), RawEvent::Up(Btn::Select)], 0);
    assert!(!read_pixelify(d.path()));
    assert_eq!(session.app().font_revision(), revision + 1);
    assert_eq!(session.app().toast(), Some(Toast::FontOriginal));

    let (mut rebooted, _motor) = common::session_with_platform(d.path());
    rebooted.feed([RawEvent::Down(Btn::Select), RawEvent::Up(Btn::Select)], 10);
    assert!(read_pixelify(d.path()));
    assert_eq!(rebooted.app().toast(), Some(Toast::FontPixelify));
}

#[test]
fn shelf_does_not_draw_redundant_letter_above_or_game_title_below() {
    let (_d, mut app) = on_shelf(&["Advance (USA) [Rev 1]", "Boktai"]);
    let letter = Printed::new(TexId::from_raw(101), 20);
    let title = Printed::new(TexId::from_raw(102), 360);
    let favorite = Printed::new(TexId::from_raw(103), 90);
    app.set_shelf_captions(
        BTreeMap::from([("Advance (USA) [Rev 1]".to_string(), (letter, title))]),
        favorite,
    );

    let mut out = Vec::new();
    app.draw(&mut out);
    assert!(!out.iter().any(|draw| matches!(draw,
        Draw::Tex { tex, .. } if *tex == TexId::from_raw(101) || *tex == TexId::from_raw(102) || *tex == TexId::from_raw(103)
    )));
}

/// Long enough for the close to put the lid back, with room to spare.
fn let_it_close(app: &mut App) {
    app.update(0.4);
}

/// Long enough for a hop to land.
fn let_it_hop(app: &mut App) {
    app.update(0.25);
}

/// Opening on mGBA whatever the cart runs would be a board that says every cart runs mGBA,
/// which is a lie the moment one of them does not.
#[test]
fn start_on_the_shelf_opens_the_core_picker_on_the_carts_current_core() {
    let (d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    assert_eq!(app.selected_stem(), Some("Emerald"));

    app.apply(Action::GbaDown(Btn::Start));
    assert_eq!(
        app.core_picker(),
        Some(Core::Mgba),
        "a cart with no line of its own runs the default core"
    );
    app.apply(Action::GbaDown(Btn::B));
    let_it_close(&mut app);

    slot_store::write_selected_core(d.path(), "Emerald", Core::Gpsp).unwrap();
    app.apply(Action::GbaDown(Btn::Start));
    assert_eq!(
        app.core_picker(),
        Some(Core::Gpsp),
        "the chip should start in the core the cart already uses"
    );
}

/// The write happens on the press, and the lid then takes its time going back on.
#[test]
fn choosing_a_core_writes_it_and_closes() {
    let (d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    app.apply(Action::GbaDown(Btn::Right));
    app.apply(Action::GbaDown(Btn::A));

    assert_eq!(slot_store::core_for(d.path(), "Emerald"), Core::Gpsp);
    assert_eq!(
        slot_store::core_for(d.path(), "Zzz"),
        Core::Mgba,
        "the choice landed on a cart the shelf was not on"
    );
    let_it_close(&mut app);
    assert_eq!(
        app.core_picker(),
        None,
        "the picker stayed open after a choice"
    );
}

#[test]
fn b_closes_the_picker_without_writing() {
    let (d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    app.apply(Action::GbaDown(Btn::Right));
    app.apply(Action::GbaDown(Btn::B));
    let_it_close(&mut app);

    assert_eq!(app.core_picker(), None);
    assert_eq!(
        slot_store::core_for(d.path(), "Emerald"),
        Core::Mgba,
        "backing out of the picker still changed the cart"
    );
}

/// The sockets sit left and right, so the arrows point at them: no wrapping. Toward the socket
/// the chip is already in, only the chip shakes.
#[test]
fn the_chip_goes_where_the_arrow_points_and_does_not_wrap() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));

    app.apply(Action::GbaDown(Btn::Left));
    assert_eq!(
        app.core_picker(),
        Some(Core::Mgba),
        "left from mGBA wrapped round"
    );
    assert_ne!(
        app.core_picker_chip().unwrap().shake,
        0.0,
        "a press toward the chip's own socket went unanswered"
    );
    assert_eq!(
        app.shelf_shake(),
        0.0,
        "the shelf shook as well as the chip"
    );

    app.apply(Action::GbaDown(Btn::Right));
    assert_eq!(app.core_picker(), Some(Core::Gpsp));
    let_it_hop(&mut app);
    app.apply(Action::GbaDown(Btn::Right));
    assert_eq!(
        app.core_picker(),
        Some(Core::Gpsp),
        "right from gpSP wrapped round"
    );
}

/// `the_chip_goes_where_the_arrow_points_and_does_not_wrap` asserts this too, but nothing in it
/// ever refuses the shelf, so that assertion passes whether or not `shelf_shake` still guards on
/// the picker being up. Arming a real refusal first is what makes the guard's absence show.
#[test]
fn a_shelf_refusal_does_not_shake_once_the_picker_takes_over() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    app.refuse();
    app.apply(Action::GbaDown(Btn::Start));
    assert_eq!(
        app.shelf_shake(),
        0.0,
        "the shelf shook under an open picker"
    );
}

#[test]
fn back_mid_hop_turns_round_and_onward_does_nothing() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    app.apply(Action::GbaDown(Btn::Right));
    app.update(0.05);

    app.apply(Action::GbaDown(Btn::Right));
    assert_eq!(app.core_picker(), Some(Core::Gpsp));
    assert_eq!(
        app.core_picker_chip().unwrap().shake,
        0.0,
        "onward mid-hop was refused"
    );

    app.apply(Action::GbaDown(Btn::Left));
    assert_eq!(
        app.core_picker(),
        Some(Core::Mgba),
        "back mid-hop did not turn the chip round"
    );
}

#[test]
fn a_mid_hop_writes_where_the_chip_is_heading() {
    let (d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    app.apply(Action::GbaDown(Btn::Right));
    app.update(0.05);
    app.apply(Action::GbaDown(Btn::A));
    assert_eq!(slot_store::core_for(d.path(), "Emerald"), Core::Gpsp);
}

/// A on the shelf inserts on the release of a press the shelf saw. The press that saved went
/// to the picker, so its release — however late — must not start the cart.
#[test]
fn the_a_that_saved_does_not_start_the_cart() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    app.apply(Action::GbaDown(Btn::Start));
    app.apply(Action::GbaDown(Btn::Right));
    app.apply(Action::GbaDown(Btn::A));
    let_it_close(&mut app);
    assert_eq!(app.core_picker(), None);

    app.apply(Action::GbaUp(Btn::A));
    assert!(
        matches!(app.phase(), Phase::Shelf),
        "releasing the A that saved inserted the cart: {:?}",
        app.phase()
    );
}

#[test]
fn presses_during_the_close_and_keys_the_picker_does_not_use_do_nothing() {
    let (d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    for key in [Btn::Up, Btn::Down, Btn::Start, Btn::Select] {
        app.apply(Action::GbaDown(key));
        assert_eq!(
            app.core_picker(),
            Some(Core::Mgba),
            "{key:?} moved the chip"
        );
    }

    app.apply(Action::GbaDown(Btn::B));
    app.apply(Action::GbaDown(Btn::Right));
    app.apply(Action::GbaDown(Btn::A));
    let_it_close(&mut app);
    assert_eq!(app.core_picker(), None);
    assert_eq!(
        slot_store::core_for(d.path(), "Emerald"),
        Core::Mgba,
        "a press during the close wrote a core"
    );
}

/// A shut lid is walking away. The picker goes at once and writes nothing, so waking lands on a
/// plain shelf.
#[test]
fn shutting_the_lid_closes_the_picker_without_writing() {
    let (d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    app.apply(Action::GbaDown(Btn::Right));
    app.apply(Action::LidClose);
    assert_eq!(app.core_picker(), None, "the picker survived the lid");
    assert_eq!(slot_store::core_for(d.path(), "Emerald"), Core::Mgba);
}

/// A menu that let the thing behind it move would act on a different cart than the one it
/// named when it opened.
#[test]
fn the_picker_swallows_the_shelf_arrows() {
    let (_d, mut app) = on_shelf(&["Emerald", "Metroid Fusion"]);
    app.apply(Action::GbaDown(Btn::Start));
    app.apply(Action::GbaDown(Btn::Right));
    app.apply(Action::GbaDown(Btn::B));
    assert_eq!(
        app.selected_stem(),
        Some("Emerald"),
        "the shelf moved underneath an open picker"
    );
}

/// An A pressed just before START belonged to the shelf that was showing when it went down.
/// Opening the picker has to let go of it, or its 500 ms hold still runs out underneath the
/// open cart and inserts it clean, skipping the resume the shelf would otherwise have offered.
#[test]
fn opening_the_picker_lets_go_of_a_play_hold_already_armed() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    app.apply(Action::GbaDown(Btn::A));
    app.apply(Action::GbaDown(Btn::Start));
    app.apply(Action::GbaUp(Btn::A));
    app.update(0.6);
    assert!(
        matches!(app.phase(), Phase::Shelf),
        "the held A inserted the cart under the open picker: {:?}",
        app.phase()
    );
}

/// A direction still repeating when START goes down is the shelf's, not the picker's: it must
/// stop, the same as when the shelf leaves the screen any other way, or the row keeps moving
/// underneath the cart that is supposedly open.
#[test]
fn opening_the_picker_lets_go_of_a_direction_still_held() {
    let (_d, mut app) = on_shelf(&["Emerald", "Metroid Fusion", "Zzz"]);
    app.apply(Action::GbaDown(Btn::Right));
    app.apply(Action::GbaDown(Btn::Start));
    app.update(0.6);
    assert_eq!(
        app.selected_stem(),
        Some("Metroid Fusion"),
        "the shelf moved underneath the open picker"
    );
}

/// Nothing to configure with no cart under the highlight, and a picker that wrote to an
/// empty stem would leave a line for a cart that is not there.
#[test]
fn the_picker_does_not_open_on_an_empty_shelf() {
    let (_d, mut app) = on_shelf(&[]);
    app.apply(Action::GbaDown(Btn::Start));
    assert_eq!(app.core_picker(), None);
}

/// The picker is on START because SELECT is the chord key. Held, SELECT turns Up/Down into
/// brightness and Left/Right into blue light, and `adjust` answers those on the shelf as
/// readily as in a game. A picker on SELECT would have to choose between eating the first
/// half of every one of those chords and putting the 600 ms chord window in front of the
/// menu; START is bound to nothing here and owes neither.
#[test]
fn select_on_the_shelf_leaves_the_picker_shut_so_it_can_still_chord() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);

    app.apply(Action::GbaDown(Btn::Select));
    assert_eq!(
        app.core_picker(),
        None,
        "SELECT must stay free for brightness and blue light on the shelf"
    );

    // That SELECT+Up actually yields BrightnessUp is the gesture layer's to prove, and it
    // does: see the chord table test in slot-input/tests/gesture.rs. What this layer owes is
    // only that the shelf does not intercept SELECT before the chord can form.
}

/// Stand-ins for everything the frontend uploads for the picker, so the draw can be read back
/// without a compositor. Every id distinct.
struct PickerFaces {
    board: TexId,
    lid: TexId,
    sockets: [TexId; 2],
    chips: [TexId; 2],
    blank: TexId,
    shadow: TexId,
    legend: [(TexId, u32); 3],
}

/// Only the faces the frontend uploads at boot: the sockets, the chips and the legend. The board
/// and the lid are the highlighted cart's, and are not set.
fn fake_boot_faces(app: &mut App) -> PickerFaces {
    let id = TexId::from_raw;
    let f = PickerFaces {
        board: id(900),
        lid: id(901),
        sockets: [id(902), id(903)],
        chips: [id(904), id(905)],
        blank: id(906),
        shadow: id(907),
        legend: [(id(908), 60), (id(909), 90), (id(910), 80)],
    };
    app.set_core_part_faces(f.sockets.to_vec(), f.chips.to_vec(), f.blank, f.shadow);
    app.set_core_legend_faces(f.legend.to_vec());
    f
}

/// Everything the picker draws, this cart's board and lid included, so START opens the cart at
/// once instead of leaving it waiting on the shelf, where the arrows do nothing.
fn fake_picker_faces(app: &mut App) -> PickerFaces {
    let f = fake_boot_faces(app);
    app.set_core_board_faces(f.board, f.lid);
    f
}

fn frame(app: &App) -> Vec<Draw> {
    let mut out = Vec::new();
    app.draw(&mut out);
    out
}

/// Where a plain face landed: its place in the frame, and its rect.
fn tex_at(out: &[Draw], want: TexId) -> Option<(usize, [f32; 4])> {
    out.iter().enumerate().find_map(|(i, d)| match *d {
        Draw::Tex {
            x, y, w, h, tex, ..
        } if tex == want => Some((i, [x, y, w, h])),
        _ => None,
    })
}

/// Every place one face landed, in frame order. The chip's shadow is drawn twice while the
/// chip is in the air: once under the lid, once under the chip.
fn tex_all(out: &[Draw], want: TexId) -> Vec<(usize, [f32; 4])> {
    out.iter()
        .enumerate()
        .filter_map(|(i, d)| match *d {
            Draw::Tex {
                x, y, w, h, tex, ..
            } if tex == want => Some((i, [x, y, w, h])),
            _ => None,
        })
        .collect()
}

/// Where a turned face landed, and its turn.
fn turned_at(out: &[Draw], want: TexId) -> Option<(usize, [f32; 4], f32)> {
    out.iter().enumerate().find_map(|(i, d)| match *d {
        Draw::Turned {
            x,
            y,
            w,
            h,
            tex,
            turn,
            ..
        } if tex == want => Some((i, [x, y, w, h], turn)),
        _ => None,
    })
}

fn near(a: [f32; 4], b: [f32; 4]) -> bool {
    a.iter().zip(b).all(|(p, q)| (p - q).abs() < 0.01)
}

/// How opaque a plain face was drawn.
fn alpha_of(out: &[Draw], want: TexId) -> Option<f32> {
    out.iter().find_map(|d| match *d {
        Draw::Tex { tex, alpha, .. } if tex == want => Some(alpha),
        _ => None,
    })
}

/// The sockets and the chip seated in mGBA ride `board` wherever it is and at whatever size it
/// is: a missing `* zoom` anywhere in that chain would separate them from it well before the
/// movement settles.
fn assert_parts_on_board(out: &[Draw], f: &PickerFaces, board: Placed, when: &str) {
    let zoom = board.w / BOARD_W as f32;
    for (i, socket) in f.sockets.iter().enumerate() {
        let (x, y) = on_board(board, SOCKET_U[i], SOCKET_V);
        let (_, at) = tex_at(out, *socket).unwrap_or_else(|| panic!("a socket is missing {when}"));
        assert!(
            near(
                at,
                [
                    x.round(),
                    y.round(),
                    SOCKET_W as f32 * zoom,
                    SOCKET_H as f32 * zoom
                ]
            ),
            "socket {i} at {at:?} {when}"
        );
    }
    let (cx, cy) = on_board(board, CHIP_U[0], CHIP_V);
    let want_chip = grown(
        Placed {
            x: cx,
            y: cy,
            w: CHIP_W as f32 * zoom,
            h: CHIP_H as f32 * zoom,
        },
        TURN_PAD as f32 * zoom,
    );
    let (_, chip, _) =
        turned_at(out, f.chips[0]).unwrap_or_else(|| panic!("no seated chip {when}"));
    assert!(
        near(
            chip,
            [
                want_chip.x.round(),
                want_chip.y.round(),
                want_chip.w,
                want_chip.h
            ]
        ),
        "the chip is not riding the board {when}: {chip:?}"
    );
}

/// Quads a shelf cart's width other than the open cart's board: the highlighted cart standing in
/// the row. Its neighbours are drawn smaller.
fn carts_standing(out: &[Draw], board: TexId) -> usize {
    out.iter()
        .filter(|d| match **d {
            Draw::Rect { w, .. } => (w - CART_W as f32).abs() < 0.01,
            Draw::Tex { w, tex, .. } => tex != board && (w - CART_W as f32).abs() < 0.01,
            _ => false,
        })
        .count()
}

/// Long enough for the lid to come off.
fn let_it_open(app: &mut App) {
    app.update(0.5);
}

/// At rest: the board where the mockup has it, both sockets on it, the chip seated in the cart's
/// own core, the lid lifted and turned, and the legend — all after the shelf's own slot, so over
/// the shelf rather than under it.
#[test]
fn the_open_cart_rests_over_the_shelf_with_its_lid_turned() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    let f = fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    let_it_open(&mut app);
    let out = frame(&app);

    let rest = board_at(1.0);
    let (board_i, board) = tex_at(&out, f.board).expect("no board");
    assert!(
        near(board, [rest.x, rest.y, rest.w, rest.h]),
        "board at {board:?}"
    );
    let mouth = out
        .iter()
        .rposition(is_mouth)
        .expect("no slot on the shelf");
    assert!(board_i > mouth, "the board is drawn under the shelf");

    for (i, socket) in f.sockets.iter().enumerate() {
        let (x, y) = on_board(rest, SOCKET_U[i], SOCKET_V);
        let (_, at) = tex_at(&out, *socket).expect("a socket is missing");
        assert!(
            near(at, [x.round(), y.round(), SOCKET_W as f32, SOCKET_H as f32]),
            "socket {i} at {at:?}"
        );
    }

    let (chip_i, chip, chip_turn) =
        turned_at(&out, f.chips[0]).expect("the chip is not seated in mGBA");
    assert_eq!(chip_turn, 0.0, "a seated chip is tipped");
    let (cx, cy) = on_board(rest, CHIP_U[0], CHIP_V);
    let want_chip = grown(
        Placed {
            x: cx,
            y: cy,
            w: CHIP_W as f32,
            h: CHIP_H as f32,
        },
        TURN_PAD as f32,
    );
    assert!(
        near(
            chip,
            [
                want_chip.x.round(),
                want_chip.y.round(),
                want_chip.w,
                want_chip.h
            ]
        ),
        "the seated chip is not in mGBA's socket: {chip:?}"
    );
    assert!(turned_at(&out, f.blank).is_none(), "a blank chip at rest");

    let (lid_rest, turn) = lid_at(1.0);
    let want = grown(lid_rest, TURN_PAD as f32 * lid_rest.w / CART_W as f32);
    let (lid_i, lid, lid_turn) = turned_at(&out, f.lid).expect("no lid");
    assert!(
        near(lid, [want.x, want.y, want.w, want.h]),
        "lid at {lid:?}"
    );
    assert_eq!((lid_turn, turn), (LID_TURN, LID_TURN));
    assert!(lid_i > chip_i, "the chip is drawn over the lid");

    // The soft oval on the ground under the lid, where the mockup has it: centred on
    // (360, 140), and under the lid rather than over it. A seated chip casts none.
    let shadows = tex_all(&out, f.shadow);
    assert_eq!(
        shadows.len(),
        1,
        "expected the lid's shadow alone, got {shadows:?}"
    );
    let (shadow_i, s) = shadows[0];
    assert!(
        (s[0] + s[2] / 2.0 - 360.0).abs() < 0.01 && (s[1] + s[3] / 2.0 - 140.0).abs() < 0.01,
        "the lid's shadow is not under it: {s:?}"
    );
    assert!(shadow_i < lid_i, "the lid's shadow is drawn over the lid");

    // Cancel under the cart's left edge, Swap centred by what shows, Choose's word ending under
    // the cart's right edge. A hint face's last HINT_EDGE pixels are transparent, so they do not
    // count toward where it sits.
    let [cancel, swap, choose] = f.legend;
    let at = |tex| tex_at(&out, tex).expect("a legend hint is missing").1;
    let seen = |w: u32| (w - HINT_EDGE) as f32;
    assert_eq!(at(cancel.0), [174.0, 386.0, cancel.1 as f32, HINT_H as f32]);
    assert_eq!(
        at(swap.0)[0] + seen(swap.1) / 2.0,
        360.0,
        "Swap is off the panel's centre"
    );
    assert_eq!(
        at(choose.0)[0] + seen(choose.1),
        546.0,
        "Choose does not end at the cart's edge"
    );
    assert_eq!((at(swap.0)[1], at(choose.0)[1]), (386.0, 386.0));
}

/// A face drawn at its own size is only sharp on whole pixels. At a fractional place the linear
/// filter splits each 1 px line of a socket's silkscreen across two pixels at half strength, and
/// the empty socket's outline goes faint.
#[test]
fn the_sockets_and_the_seated_chip_rest_on_whole_pixels() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    let f = fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    let_it_open(&mut app);
    let out = frame(&app);

    let whole = |r: [f32; 4]| r[0].fract() == 0.0 && r[1].fract() == 0.0;
    for (i, socket) in f.sockets.iter().enumerate() {
        let (_, at) = tex_at(&out, *socket).expect("a socket is missing");
        assert!(whole(at), "socket {i} is off the pixel grid at {at:?}");
    }
    let (_, chip, _) = turned_at(&out, f.chips[0]).expect("the chip is not seated in mGBA");
    assert!(
        whole(chip),
        "the seated chip is off the pixel grid at {chip:?}"
    );
}

/// The row parts to where the mockup stands the neighbours and dims them to a quarter, as it has
/// them. The recede alone, set by where they stand, left their faces at 0.41.
#[test]
fn the_neighbours_dim_to_a_quarter_while_a_cart_is_open() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    let faces = vec![TexId::from_raw(920), TexId::from_raw(921)];
    app.set_faces(faces.clone());
    fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    let_it_open(&mut app);
    let out = frame(&app);

    let alpha = out
        .iter()
        .find_map(|d| match *d {
            Draw::Tex { tex, alpha, .. } if tex == faces[1] => Some(alpha),
            _ => None,
        })
        .expect("the neighbour is not on screen while the cart is open");
    assert!(
        (alpha - 0.25).abs() <= 0.01,
        "the neighbour's face is at {alpha}, not a quarter"
    );
}

/// Which socket the chip lands in follows the arrow. A swapped `CHIP_U` index, or an `across`
/// inverted from what the picker reports, would still draw a chip named `chips[1]` somewhere on
/// the board and pass a test that only asked whether it was there.
#[test]
fn the_seated_chip_moves_to_the_socket_it_hopped_to() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    let f = fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    let_it_open(&mut app);
    app.apply(Action::GbaDown(Btn::Right));
    let_it_hop(&mut app);
    let out = frame(&app);

    let (x, y) = on_board(board_at(1.0), CHIP_U[1], CHIP_V);
    let want = grown(
        Placed {
            x,
            y,
            w: CHIP_W as f32,
            h: CHIP_H as f32,
        },
        TURN_PAD as f32,
    );
    let (_, chip, _) = turned_at(&out, f.chips[1]).expect("the chip did not land in gpSP");
    assert!(
        near(chip, [want.x.round(), want.y.round(), want.w, want.h]),
        "the gpSP chip is not in gpSP's socket: {chip:?}"
    );
    assert!(
        turned_at(&out, f.chips[0]).is_none(),
        "mGBA's chip is still drawn once the hop lands in gpSP"
    );
}

/// The lid is the highlighted cart. On the first frame it stands exactly where the shelf stood
/// it, and the row does not draw a second copy underneath.
#[test]
fn the_highlighted_cart_becomes_the_lid_rather_than_a_second_cart() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    let f = fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    let out = frame(&app);

    let standing = out
        .iter()
        .filter(|d| match **d {
            Draw::Rect { w, .. } => (w - CART_W as f32).abs() < 0.01,
            Draw::Tex { w, tex, .. } if tex != f.board => (w - CART_W as f32).abs() < 0.01,
            _ => false,
        })
        .count();
    assert_eq!(
        standing, 0,
        "the row still draws the cart whose lid is coming off"
    );

    let (_, lid, turn) = turned_at(&out, f.lid).expect("no lid");
    let want = grown(lid_at(0.0).0, TURN_PAD as f32);
    assert!(
        near(lid, [want.x, want.y, want.w, want.h]),
        "the lid starts at {lid:?}"
    );
    assert_eq!(turn, 0.0);

    // Halfway through the slide the front half has come up half its travel, level and still the
    // shelf cart's width, off a back half standing exactly where the shelf stood the cart, opaque,
    // with the sockets and the chip already on it. 80.5 ms rather than 80: the app's clock counts
    // whole milliseconds, and 0.08 s adds up to a hair under 80.
    app.update(0.0805);
    let slid = frame(&app);
    let (_, lid, turn) = turned_at(&slid, f.lid).expect("the lid vanished mid-slide");
    assert_eq!(turn, 0.0, "the lid turned while it slid");
    let shelf = shelf_cart();
    let want = grown(
        Placed {
            y: shelf.y - SLIDE_UP * 0.5,
            ..shelf
        },
        TURN_PAD as f32,
    );
    assert!(
        near(lid, [want.x, want.y, want.w, want.h]),
        "the lid is not half way up its slide: {lid:?}"
    );
    let (_, board) = tex_at(&slid, f.board).expect("no board mid-slide");
    assert_eq!(
        board,
        [shelf.x, shelf.y, shelf.w, shelf.h],
        "the back half moved while the front slid"
    );
    assert_eq!(
        alpha_of(&slid, f.board),
        Some(1.0),
        "the back half is not opaque under the front"
    );
    assert_parts_on_board(&slid, &f, shelf, "mid-slide");

    // Halfway through the lift, the lid is on its way up and turning, over a back half that is
    // part grown and still opaque.
    app.update(0.21);
    let lifting = frame(&app);
    let (_, lid, turn) = turned_at(&lifting, f.lid).expect("the lid vanished mid-lift");
    assert!(
        turn < 0.0 && turn > LID_TURN,
        "the lid is not turning: {turn}"
    );
    assert!(
        lid[1] < lid_at(0.0).0.y && lid[1] > lid_at(1.0).0.y,
        "the lid is not on its way up: {lid:?}"
    );
    let (_, [bx, by, bw, bh]) = tex_at(&lifting, f.board).expect("no board mid-lift");
    assert!(
        bw > board_at(0.0).w && bw < board_at(1.0).w,
        "the board is not growing: {bw}"
    );
    assert_eq!(
        alpha_of(&lifting, f.board),
        Some(1.0),
        "the back half faded mid-lift"
    );
    assert_parts_on_board(
        &lifting,
        &f,
        Placed {
            x: bx,
            y: by,
            w: bw,
            h: bh,
        },
        "mid-lift",
    );
}

#[test]
fn mid_hop_the_chip_is_blank_tipped_and_off_the_board() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    let f = fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    let_it_open(&mut app);
    let (_, seated, _) = turned_at(&frame(&app), f.chips[0]).expect("no seated chip");

    app.apply(Action::GbaDown(Btn::Right));
    app.update(0.09);
    let out = frame(&app);
    let (_, flying, tip) = turned_at(&out, f.blank).expect("no chip in flight");
    assert!(tip > 0.0, "a chip heading right does not lean right");
    assert!(flying[1] < seated[1], "the chip is not off the board");
    // Between the two sockets rather than merely "somewhere off the board": an inverted
    // `across` would send it the wrong way and still clear the two checks above.
    let board = board_at(1.0);
    let left = on_board(board, CHIP_U[0], CHIP_V).0 + CHIP_W as f32 / 2.0;
    let right = on_board(board, CHIP_U[1], CHIP_V).0 + CHIP_W as f32 / 2.0;
    let mid = flying[0] + flying[2] / 2.0;
    assert!(
        mid > left && mid < right,
        "the flying chip is not between the sockets: {mid} not in ({left}, {right})"
    );
    assert_eq!(
        tex_all(&out, f.shadow).len(),
        2,
        "nothing under the chip in flight, beside the lid's own shadow"
    );
    assert!(
        f.chips.iter().all(|c| turned_at(&out, *c).is_none()),
        "a named chip is drawn mid-hop"
    );
    for socket in f.sockets {
        assert!(tex_at(&out, socket).is_some(), "a socket is hidden mid-hop");
    }
}

/// A refusal moves the chip and nothing else on the panel.
#[test]
fn the_chip_alone_shakes_when_refused() {
    let (_d, mut app) = on_shelf(&["Emerald", "Metroid Fusion", "Zzz"]);
    let f = fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    let_it_open(&mut app);
    let before = frame(&app);
    app.apply(Action::GbaDown(Btn::Left));
    let after = frame(&app);

    let (_, still, _) = turned_at(&before, f.chips[0]).unwrap();
    let (_, shaken, _) = turned_at(&after, f.chips[0]).unwrap();
    assert_ne!(still[0], shaken[0], "the chip did not move");
    let rest = |out: &[Draw]| -> Vec<Draw> {
        out.iter()
            .filter(|d| !matches!(d, Draw::Turned { tex, .. } if *tex == f.chips[0]))
            .copied()
            .collect()
    };
    assert_eq!(
        rest(&before),
        rest(&after),
        "something besides the chip moved"
    );
}

/// Backing out puts the lid back on — partway through, the lid is on its way down and turning
/// level while the board shrinks under it — and then hands the cart back to the row.
#[test]
fn closing_puts_the_cart_back_on_the_shelf() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    let f = fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    let_it_open(&mut app);
    app.apply(Action::GbaDown(Btn::B));

    app.update(0.1);
    let partway = frame(&app);
    let (_, lid, turn) = turned_at(&partway, f.lid).expect("the lid vanished mid-close");
    assert!(
        turn < 0.0 && turn > LID_TURN,
        "the lid is not turning level: {turn}"
    );
    assert!(lid[1] > lid_at(1.0).0.y, "the lid is not coming back down");
    let (_, [bx, by, bw, bh]) = tex_at(&partway, f.board).expect("the board vanished mid-close");
    assert!(
        bw < board_at(1.0).w && bw > board_at(0.0).w,
        "the board is not shrinking: {bw}"
    );
    assert_eq!(
        alpha_of(&partway, f.board),
        Some(1.0),
        "the back half faded mid-close"
    );

    // The sockets and the seated chip shrink with the board rather than staying put, the same
    // check as mid-open run the other way.
    assert_parts_on_board(
        &partway,
        &f,
        Placed {
            x: bx,
            y: by,
            w: bw,
            h: bh,
        },
        "mid-close",
    );

    let_it_close(&mut app);
    let out = frame(&app);

    assert!(
        tex_at(&out, f.board).is_none(),
        "the board outlived the close"
    );
    assert!(
        turned_at(&out, f.lid).is_none(),
        "the lid outlived the close"
    );
    let standing = out
        .iter()
        .filter(|d| {
            matches!(**d, Draw::Rect { w, .. } | Draw::Tex { w, .. }
                if (w - CART_W as f32).abs() < 0.01)
        })
        .count();
    assert_eq!(standing, 1, "the cart did not go back on the shelf");
}

/// START responds immediately even before the detailed faces arrive. The shelf's own cart face
/// is a truthful fallback lid, and the worker's detailed board replaces it in place.
#[test]
fn the_open_starts_with_the_shelf_face_while_its_faces_are_built() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    let f = fake_boot_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    app.update(0.3);
    assert_eq!(
        carts_standing(&frame(&app), f.board),
        0,
        "START left the highlighted cart standing on the shelf"
    );

    app.set_core_board_faces(f.board, f.lid);
    app.update(0.016);
    let out = frame(&app);
    let (_, lid, _) = turned_at(&out, f.lid).expect("no lid once the faces arrived");
    assert!(
        lid[1] < shelf_cart().y,
        "the lid did not keep opening: {lid:?}"
    );
    assert_eq!(
        carts_standing(&out, f.board),
        0,
        "the row still draws the cart whose lid is coming off"
    );

    let_it_open(&mut app);
    let (rest, _) = lid_at(1.0);
    let rest = grown(rest, TURN_PAD as f32 * rest.w / CART_W as f32);
    let (_, lid, _) = turned_at(&frame(&app), f.lid).expect("no lid once open");
    assert!(
        near(lid, [rest.x, rest.y, rest.w, rest.h]),
        "the lid is not at rest: {lid:?}"
    );
}

#[test]
fn the_open_preserves_complete_artwork_aspect_ratio() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    app.set_face_with_size_and_artwork("Emerald", TexId::from_raw(950), (CART_W, 142), true);
    let f = fake_boot_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    app.set_core_board_faces_with_size(f.board, f.lid, (CART_W, 142));
    let_it_open(&mut app);

    let (rest, turn) = lid_at(1.0);
    let pad = TURN_PAD as f32 * rest.w / CART_W as f32;
    let h = rest.w * 142.0 / CART_W as f32;
    let want = [
        rest.x - pad,
        rest.y + rest.h - h - pad,
        rest.w + 2.0 * pad,
        h + 2.0 * pad,
    ];
    let (_, lid, lid_turn) = turned_at(&frame(&app), f.lid).expect("no artwork lid");
    assert!(
        near(lid, want),
        "artwork lid was resized incorrectly: {lid:?}"
    );
    assert_eq!(lid_turn, turn);
}

/// A face that never comes cannot freeze the picker. The only detailed faces on the GPU are the
/// other cart's, and the fallback open never borrows them.
#[test]
fn the_open_starts_anyway_when_the_faces_never_come() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    app.apply(Action::GbaDown(Btn::Right));
    app.apply(Action::GbaUp(Btn::Right));
    assert_eq!(app.selected_stem(), Some("Zzz"));
    let f = fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Left));
    app.apply(Action::GbaUp(Btn::Left));
    assert_eq!(app.selected_stem(), Some("Emerald"));

    app.apply(Action::GbaDown(Btn::Start));
    app.update(0.002);
    assert_eq!(
        carts_standing(&frame(&app), f.board),
        0,
        "the open did not start immediately"
    );

    let_it_open(&mut app);
    let out = frame(&app);
    assert!(
        tex_at(&out, f.board).is_none(),
        "the fallback open drew the other cart's board"
    );
    assert!(
        turned_at(&out, f.lid).is_none(),
        "the fallback open drew the other cart's lid"
    );
    for tex in [f.sockets[0], f.sockets[1], f.chips[0], f.chips[1], f.blank] {
        assert!(
            tex_at(&out, tex).is_none() && turned_at(&out, tex).is_none(),
            "the fallback open drew a socket or chip quad"
        );
    }
}

/// Past the cap, with nothing of this cart's own built yet, the fallback lifts the shelf's own
/// face for the cart — the only thing on the GPU that is honestly this cart's — rather than
/// leave the lid off altogether.
#[test]
fn the_fallback_open_lifts_the_shelfs_own_face_for_the_lid() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    let shelf_faces = [TexId::from_raw(950), TexId::from_raw(951)];
    app.set_faces(shelf_faces.to_vec());
    app.apply(Action::GbaDown(Btn::Start));
    app.update(1.501);
    let_it_open(&mut app);

    let (rest, _) = lid_at(1.0);
    let (_, lid, turn) =
        turned_at(&frame(&app), shelf_faces[0]).expect("no lid drawn from the shelf's own face");
    assert!(
        near(lid, [rest.x, rest.y, rest.w, rest.h]),
        "the fallback lid is not at rest: {lid:?}"
    );
    assert_eq!(turn, LID_TURN, "the fallback lid is not turned");
}

/// When this cart's own faces land mid-open, having missed the cap, the very next frame draws
/// them: the fallback was only ever a placeholder for a build the worker had not finished yet.
#[test]
fn the_real_faces_replace_the_fallback_once_they_arrive() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    let shelf_faces = [TexId::from_raw(950), TexId::from_raw(951)];
    app.set_faces(shelf_faces.to_vec());
    app.apply(Action::GbaDown(Btn::Start));
    app.update(1.501);
    let_it_open(&mut app);
    assert!(
        turned_at(&frame(&app), shelf_faces[0]).is_some(),
        "the fallback lid should be up before the real faces arrive"
    );

    let f = fake_picker_faces(&mut app);
    app.update(0.016);
    let out = frame(&app);
    assert!(tex_at(&out, f.board).is_some(), "the board never arrived");
    assert!(
        turned_at(&out, f.lid).is_some(),
        "the picker's own lid never arrived"
    );
}

/// A quick START then Right must not lose the choice while detailed faces are still building.
/// This was the device-visible failure: the picker looked inert and discarded the gpSP input.
#[test]
fn gpsp_can_be_selected_immediately_after_start() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    let f = fake_boot_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    app.apply(Action::GbaDown(Btn::Right));
    app.set_core_board_faces(f.board, f.lid);
    app.update(0.016);
    assert_eq!(
        app.core_picker(),
        Some(Core::Gpsp),
        "the gpSP choice was discarded while detailed faces were pending"
    );

    let_it_open(&mut app);
    let out = frame(&app);
    assert!(
        turned_at(&out, f.chips[0]).is_none(),
        "the chip stayed in mGBA"
    );
    assert!(
        turned_at(&out, f.chips[1]).is_some(),
        "the chip did not open in gpSP"
    );
}

#[test]
fn holding_a_progresses_to_clean_start_without_charging_bar() {
    let (_d, mut app) = on_shelf(&["Emerald", "Fusion"]);
    fake_boot_faces(&mut app);
    let mut out = Vec::new();
    app.draw(&mut out);
    assert_eq!(app.play_held_progress(), 0.0);

    app.apply(Action::GbaDown(Btn::A));
    app.update(0.04);
    assert_eq!(
        app.play_held_progress(),
        0.0,
        "short press should not progress yet"
    );

    app.update(0.20);
    assert!(
        app.play_held_progress() > 0.0,
        "held A should show progress"
    );
    out.clear();
    app.draw(&mut out);
    assert!(
        !out.iter()
            .any(|d| matches!(d, Draw::Rect { y, h, .. } if (*y - 310.0).abs() < 0.1 && *h == 3.0)),
        "charging bar should NOT be drawn while holding A"
    );

    app.update(0.30);
    assert!(
        matches!(app.phase(), Phase::Inserting { clean: true, .. }),
        "holding A past threshold should start clean"
    );
}

#[test]
fn shelf_count_face_is_not_drawn_when_set() {
    let (_d, mut app) = on_shelf(&["Emerald", "Fusion"]);
    fake_boot_faces(&mut app);
    let tex = TexId::from_raw(99);
    app.set_shelf_count_face(Some((tex, 40)));
    let mut out = Vec::new();
    app.draw(&mut out);
    assert!(
        !out.iter().any(
            |d| matches!(d, Draw::Tex { tex: t, y, .. } if *t == tex && (*y - 8.0).abs() < 0.1)
        ),
        "shelf count face should not be drawn for a minimalist shelf"
    );
}

#[test]
fn power_menu_draws_action_legend() {
    let (_d, mut app) = on_shelf(&["Emerald", "Fusion"]);
    fake_boot_faces(&mut app);
    let cancel = (TexId::from_raw(88), 30);
    let select = (TexId::from_raw(89), 35);
    app.set_power_legend_faces(vec![cancel, select]);
    app.set_power_menu(Some(0));
    let mut out = Vec::new();
    app.draw(&mut out);
    assert!(
        out.iter()
            .any(|d| matches!(d, Draw::Tex { tex, .. } if *tex == cancel.0)),
        "power menu should draw cancel legend"
    );
    assert!(
        out.iter()
            .any(|d| matches!(d, Draw::Tex { tex, .. } if *tex == select.0)),
        "power menu should draw select legend"
    );
}

#[test]
fn set_clock_can_be_cancelled_with_b_when_clock_valid() {
    let (_d, mut app) = on_shelf(&["Emerald", "Fusion"]);
    fake_boot_faces(&mut app);
    app.apply(Action::QuickMenu);
    for _ in 0..slot_ui::QuickRow::DateTime.index() {
        app.apply(Action::GbaDown(Btn::Down));
    }
    app.apply(Action::GbaDown(Btn::A));
    app.update(0.6);
    assert!(matches!(app.phase(), Phase::SetClock { .. }));
    app.apply(Action::GbaDown(Btn::B));
    assert_eq!(
        app.quick_menu(),
        Some(slot_ui::QuickRow::DateTime),
        "B should return to Date & Time"
    );
}

#[test]
fn empty_recents_shows_hint_to_browse_categories() {
    let (_d, mut app) = on_shelf(&["Emerald", "Fusion"]);
    fake_boot_faces(&mut app);
    let hint_tex = TexId::from_raw(77);
    app.set_empty_recents_hint((hint_tex, 60));
    app.apply(Action::FfStart);
    assert_eq!(app.shelf_total(), 0);
    let mut out = Vec::new();
    app.draw(&mut out);
    assert!(
        out.iter().any(|d| matches!(d, Draw::Tex { tex, y, .. } if *tex == hint_tex && (*y - 352.0).abs() < 0.1)),
        "empty recents should display hint"
    );
}

#[test]
fn r2_rapid_taps_on_shelf_advance_category_without_swallowing() {
    let d = common::tmp_root_with_real_carts(&["Emerald", "Fusion"]);
    common::clocked(d.path());
    let mut s = Session::boot(d.path().to_path_buf());
    assert_eq!(s.app().shelf_category(), 0);
    // Tap 1: ALL -> RECENTS
    s.feed([RawEvent::Down(Btn::R2), RawEvent::Up(Btn::R2)], 100);
    assert_eq!(s.app().shelf_category(), 1);
    // Tap 2: RECENTS -> GBA
    s.feed([RawEvent::Down(Btn::R2), RawEvent::Up(Btn::R2)], 200);
    assert_eq!(s.app().shelf_category(), 2);
    // Tap 3: GBA -> FAVORITES. The new tab is available even when it is empty.
    s.feed([RawEvent::Down(Btn::R2), RawEvent::Up(Btn::R2)], 300);
    assert_eq!(s.app().shelf_category(), 5);
    // Tap 4 still works after the new tab: L2 back, then R2 forward again
    s.feed([RawEvent::Down(Btn::L2), RawEvent::Up(Btn::L2)], 400);
    assert_eq!(s.app().shelf_category(), 2);
    s.feed([RawEvent::Down(Btn::R2), RawEvent::Up(Btn::R2)], 500);
    assert_eq!(s.app().shelf_category(), 5);
}

#[test]
fn xy_shoulders_map_y_to_l_and_x_to_r() {
    let d = common::tmp_root_with_carts(&["Emerald", "Fusion"]);
    common::clocked(d.path());
    let mut state = slot_store::read_slot_state(d.path());
    state.face_buttons = FaceButtons::Shoulders;
    write_slot_state(d.path(), &state).unwrap();
    let mut s = Session::boot(d.path().to_path_buf());
    s.feed([RawEvent::Down(Btn::A)], 16);
    s.feed([RawEvent::Up(Btn::A)], 32);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(
            std::time::Instant::now() < deadline,
            "the cart never reached the game"
        );
        s.update(1.0 / 60.0);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    s.feed([RawEvent::Down(Btn::Y)], 100);
    s.update(1.0 / 60.0);
    let mask = s.emu().unwrap().input().0;
    assert_ne!(mask & ButtonMask::L, 0, "Y did not send L");
    assert_eq!(mask & ButtonMask::R, 0, "Y also sent R");
    s.feed([RawEvent::Up(Btn::Y)], 110);

    s.feed([RawEvent::Down(Btn::X)], 120);
    s.update(1.0 / 60.0);
    let mask = s.emu().unwrap().input().0;
    assert_ne!(mask & ButtonMask::R, 0, "X did not send R");
    assert_eq!(mask & ButtonMask::L, 0, "X also sent L");
}
