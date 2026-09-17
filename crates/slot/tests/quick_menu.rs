mod common;

use common::{
    app_booting_at, app_booting_with_clock, app_playing_in, tmp_root_with_carts, Clock,
    CLOCK_IS_SET,
};
use slot::app::{App, Phase};
use slot_input::{Action, Btn};
use slot_store::{read_slot_state, write_slot_state, SlotState};
use slot_ui::{
    edge, Draw, Icon, QuickMenuFaces, QuickRow, QuickValue, TexId, MENU_PAD, OUT_H, OUT_W,
    QUICK_EDGE, QUICK_PITCH, QUICK_TOP,
};
use tempfile::TempDir;

/// On the carousel, beside the card it reads and writes, with a platform clock that reads like
/// a real date. Two carts, because one is a dedicated device that never shows the carousel.
fn on_carousel_with(state: SlotState) -> (TempDir, App, Clock) {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    write_slot_state(
        d.path(),
        &SlotState {
            clock_set: true,
            ..state
        },
    )
    .expect("write slot.state");
    let (a, clock) = app_booting_at(d.path(), CLOCK_IS_SET);
    assert!(matches!(a.phase(), Phase::Shelf));
    (d, a, clock)
}

fn on_carousel() -> (TempDir, App, Clock) {
    on_carousel_with(SlotState::default())
}

/// A press and its release, as the gesture layer delivers them.
fn press(a: &mut App, btn: Btn) {
    a.apply(Action::GbaDown(btn));
    a.apply(Action::GbaUp(btn));
}

/// MENU on the carousel, then the bar walked down to `row`.
fn open_at(a: &mut App, row: QuickRow) {
    a.apply(Action::QuickMenu);
    for _ in 0..row.index() {
        press(a, Btn::Down);
    }
    assert_eq!(a.quick_menu(), Some(row));
}

#[test]
fn menu_opens_the_quick_menu_with_its_top_row_selected_every_time() {
    let (_d, mut a, _) = on_carousel();
    a.apply(Action::QuickMenu);
    assert_eq!(a.quick_menu(), Some(QuickRow::FastForward));
    press(&mut a, Btn::Down);
    a.apply(Action::QuickMenu);
    a.apply(Action::QuickMenu);
    assert_eq!(a.quick_menu(), Some(QuickRow::FastForward));
}

#[test]
fn menu_or_b_closes_it_back_onto_the_carousel_where_you_were() {
    for close in [Action::QuickMenu, Action::GbaDown(Btn::B)] {
        let (_d, mut a, _) = on_carousel();
        press(&mut a, Btn::Right);
        assert_eq!(a.selected_stem(), Some("Fusion"));
        a.apply(Action::QuickMenu);
        a.apply(close);
        assert!(matches!(a.phase(), Phase::Shelf));
        assert_eq!(a.selected_stem(), Some("Fusion"));
    }
}

#[test]
fn menu_over_a_game_opens_display_settings_and_b_resumes_playing() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::QuickMenu);
    assert!(matches!(
        a.phase(),
        Phase::QuickMenu {
            resume: Some(_),
            ..
        }
    ));
    assert_eq!(a.quick_menu(), Some(QuickRow::LcdEffect));
    assert!(a.play_settings_open());
    for want in [
        QuickRow::ColourCorrection,
        QuickRow::Overlay,
        QuickRow::Overlay,
    ] {
        press(&mut a, Btn::Down);
        assert_eq!(a.quick_menu(), Some(want));
    }
    a.apply(Action::GbaDown(Btn::B));
    assert!(matches!(a.phase(), Phase::Playing { .. }));
    assert_eq!(a.seated_cart().map(|c| c.stem.as_str()), Some("Emerald"));
}

#[test]
fn menu_over_a_game_draws_the_paused_frame_under_a_scrim() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.set_game_ready(true);
    a.apply(Action::QuickMenu);
    let out = frame(&a);
    let game = out
        .iter()
        .position(|d| matches!(d, Draw::Game))
        .expect("the paused game was not under the menu");
    let scrim = out
        .iter()
        .position(|d| {
            matches!(
                *d,
                Draw::Rect {
                    w,
                    h,
                    colour,
                    ..
                } if w == OUT_W as f32 && h == OUT_H as f32 && colour[3] < 1.0 && colour[3] > 0.0
            )
        })
        .expect("the menu drew no translucent ground");
    assert!(scrim > game, "the scrim was not over the game");
}

#[test]
fn menu_on_the_shelf_keeps_an_opaque_ground() {
    let (_d, mut a, _) = on_carousel();
    a.apply(Action::QuickMenu);
    let out = frame(&a);
    assert!(
        out.iter().any(|d| matches!(
            *d,
            Draw::Rect {
                w,
                h,
                colour,
                ..
            } if w == OUT_W as f32 && h == OUT_H as f32 && (colour[3] - 1.0).abs() < 1e-4
        )),
        "the shelf menu lost its opaque ground"
    );
    assert!(
        !out.iter().any(|d| matches!(d, Draw::Game)),
        "the shelf menu drew a game layer"
    );
}

#[test]
fn menu_over_a_gb_game_includes_picture() {
    let d = common::tmp_root_with_gb_carts(&["Tetris", "Mario"]);
    let mut a = app_playing_in(d.path(), "Tetris");
    a.apply(Action::QuickMenu);
    assert_eq!(a.quick_menu(), Some(QuickRow::LcdEffect));
    for want in [
        QuickRow::ColourCorrection,
        QuickRow::Overlay,
        QuickRow::Picture,
        QuickRow::Picture,
    ] {
        press(&mut a, Btn::Down);
        assert_eq!(a.quick_menu(), Some(want));
    }
}

#[test]
fn up_and_down_move_the_bar_and_stop_at_the_ends() {
    let (_d, mut a, _) = on_carousel();
    a.apply(Action::QuickMenu);
    press(&mut a, Btn::Up);
    assert_eq!(a.quick_menu(), Some(QuickRow::FastForward));
    for want in [
        QuickRow::FastForwardSound,
        QuickRow::ColourCorrection,
        QuickRow::Rumble,
        QuickRow::FaceButtons,
        QuickRow::Overlay,
        QuickRow::LcdEffect,
        QuickRow::DateTime,
        QuickRow::About,
        QuickRow::About,
    ] {
        press(&mut a, Btn::Down);
        assert_eq!(a.quick_menu(), Some(want));
    }
}

#[test]
fn fast_forward_steps_through_its_speeds_and_saves_each_one() {
    let (d, mut a, _) = on_carousel();
    open_at(&mut a, QuickRow::FastForward);
    for (btn, want) in [
        (Btn::Left, 4),
        (Btn::Left, 3),
        (Btn::Left, 2),
        (Btn::Left, 2),
        (Btn::Right, 3),
        (Btn::Right, 4),
        (Btn::Right, 6),
        (Btn::Right, 6),
        (Btn::Left, 4),
    ] {
        press(&mut a, btn);
        assert_eq!(a.ff_speed(), want);
        assert_eq!(read_slot_state(d.path()).ff_speed, want);
        assert_eq!(
            a.quick_value(QuickRow::FastForward),
            QuickValue::speed(want)
        );
    }
}

#[test]
fn rumble_and_fast_forward_sound_flip_on_either_arrow_and_save() {
    let (d, mut a, _) = on_carousel();
    let card = |d: &TempDir| {
        let s = read_slot_state(d.path());
        (s.ff_sound, s.rumble)
    };
    open_at(&mut a, QuickRow::FastForwardSound);
    press(&mut a, Btn::Left);
    assert_eq!(card(&d), (true, true));
    assert_eq!(
        a.quick_value(QuickRow::FastForwardSound),
        Some(QuickValue::On)
    );
    press(&mut a, Btn::Right);
    assert_eq!(card(&d), (false, true));
    press(&mut a, Btn::Down);
    press(&mut a, Btn::Down);
    press(&mut a, Btn::Right);
    assert_eq!(card(&d), (false, false));
    assert!(!a.rumble_enabled());
    assert_eq!(a.quick_value(QuickRow::Rumble), Some(QuickValue::Off));
    press(&mut a, Btn::Left);
    assert_eq!(card(&d), (false, true));
}

#[test]
fn the_arrows_change_nothing_on_a_row_that_opens() {
    let (d, mut a, _) = on_carousel();
    let before = std::fs::read(d.path().join("System/slot.state")).expect("read slot.state");
    for row in [QuickRow::DateTime, QuickRow::About] {
        open_at(&mut a, row);
        press(&mut a, Btn::Left);
        press(&mut a, Btn::Right);
        assert_eq!(a.quick_menu(), Some(row));
        a.apply(Action::QuickMenu);
    }
    assert_eq!(
        std::fs::read(d.path().join("System/slot.state")).unwrap(),
        before
    );
}

#[test]
fn a_on_about_opens_the_label_and_b_comes_back_to_the_menu() {
    let (_d, mut a, _) = on_carousel();
    open_at(&mut a, QuickRow::About);
    press(&mut a, Btn::A);
    assert!(matches!(a.phase(), Phase::About));
    press(&mut a, Btn::B);
    assert_eq!(a.quick_menu(), Some(QuickRow::About));
}

#[test]
fn a_on_date_and_time_opens_the_clock_at_the_time_it_already_has() {
    let (_d, mut a, _) = on_carousel_with(SlotState {
        utc_offset_min: -300,
        ..SlotState::default()
    });
    open_at(&mut a, QuickRow::DateTime);
    press(&mut a, Btn::A);
    let picker = a.picker().expect("A on Date & Time did not open the clock");
    assert_eq!(picker.offset_min(), -300);
    assert_eq!(picker.secs(), CLOCK_IS_SET - CLOCK_IS_SET % 60);
}

#[test]
fn confirming_the_clock_from_the_menu_sets_it_and_comes_back_to_the_menu() {
    let (d, mut a, clock) = on_carousel_with(SlotState {
        utc_offset_min: -300,
        ..SlotState::default()
    });
    open_at(&mut a, QuickRow::DateTime);
    press(&mut a, Btn::A);
    press(&mut a, Btn::Up);
    for _ in 0..5 {
        press(&mut a, Btn::Right);
    }
    press(&mut a, Btn::Down);
    let changed = a.picker().unwrap().secs() - (CLOCK_IS_SET - CLOCK_IS_SET % 60);
    press(&mut a, Btn::A);
    assert_eq!(a.quick_menu(), Some(QuickRow::DateTime));
    assert_eq!(clock.get(), CLOCK_IS_SET + changed);
    let s = read_slot_state(d.path());
    assert_eq!(s.utc_offset_min, -330);
    assert!(s.clock_set);
}

#[test]
fn b_on_the_clock_from_the_menu_comes_back_without_changing_anything() {
    let (d, mut a, clock) = on_carousel_with(SlotState {
        utc_offset_min: -300,
        ..SlotState::default()
    });
    open_at(&mut a, QuickRow::DateTime);
    press(&mut a, Btn::A);
    press(&mut a, Btn::Up);
    for _ in 0..5 {
        press(&mut a, Btn::Right);
    }
    press(&mut a, Btn::Down);
    press(&mut a, Btn::B);
    assert_eq!(a.quick_menu(), Some(QuickRow::DateTime));
    assert_eq!(clock.get(), CLOCK_IS_SET);
    assert_eq!(read_slot_state(d.path()).utc_offset_min, -300);
}

#[test]
fn the_first_boot_clock_still_has_no_way_back() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    let (mut a, _) = app_booting_with_clock(d.path());
    press(&mut a, Btn::B);
    a.apply(Action::QuickMenu);
    assert!(matches!(a.phase(), Phase::SetClock { .. }));
}

#[test]
fn brightness_and_volume_still_answer_over_the_quick_menu() {
    let (d, mut a, _) = on_carousel();
    let icons: Vec<TexId> = (0..Icon::ALL.len())
        .map(|i| TexId::from_raw(700 + i))
        .collect();
    a.set_icon_faces(icons.clone());
    open_at(&mut a, QuickRow::FastForward);
    let before = read_slot_state(d.path());
    a.apply(Action::BrightnessUp);
    a.apply(Action::VolumeDown);
    let after = read_slot_state(d.path());
    assert_eq!(
        (after.brightness, after.volume),
        (before.brightness + 1, before.volume - 5)
    );
    assert_eq!(a.quick_menu(), Some(QuickRow::FastForward));
    let out = frame(&a);
    assert!(out
        .iter()
        .any(|d| matches!(*d, Draw::Tex { tex, .. } if icons.contains(&tex))));
}

#[test]
fn colour_correction_flips_on_either_arrow_and_saves() {
    let (d, mut a, _) = on_carousel();
    open_at(&mut a, QuickRow::ColourCorrection);
    assert!(!a.colour_correction());
    assert_eq!(
        a.quick_value(QuickRow::ColourCorrection),
        Some(QuickValue::Off)
    );
    for (btn, want) in [
        (Btn::Right, true),
        (Btn::Left, false),
        (Btn::Left, true),
        (Btn::Right, false),
    ] {
        press(&mut a, btn);
        assert_eq!(a.colour_correction(), want);
        assert_eq!(read_slot_state(d.path()).colour_correction, want);
        assert_eq!(
            a.quick_value(QuickRow::ColourCorrection),
            Some(QuickValue::flag(want))
        );
    }
}

#[test]
fn colour_correction_leaves_the_settings_around_it_alone() {
    let (d, mut a, _) = on_carousel();
    open_at(&mut a, QuickRow::ColourCorrection);
    press(&mut a, Btn::Right);
    let s = read_slot_state(d.path());
    assert!(s.colour_correction);
    assert_eq!(
        (s.ff_speed, s.ff_sound, s.rumble),
        (
            SlotState::default().ff_speed,
            SlotState::default().ff_sound,
            SlotState::default().rumble
        )
    );
}

fn fake_faces(a: &mut App) {
    let id = TexId::from_raw;
    a.set_quick_menu_faces(QuickMenuFaces {
        labels: (0..QuickRow::LABELS.len())
            .map(|i| (id(100 + i), 200, 40))
            .collect(),
        values: (0..QuickValue::ALL.len())
            .map(|i| [(id(200 + i), 60, 40), (id(210 + i), 60, 40)])
            .collect(),
        carets: [(id(300), 10, 40), (id(301), 10, 40)],
        legend: [(id(400), 70), (id(401), 110), (id(402), 80)],
    });
    a.set_quick_clock_faces((id(500), 150, 40), (id(501), 150, 40));
}

fn value(v: QuickValue, lit: bool) -> usize {
    if lit {
        210 + v.index()
    } else {
        200 + v.index()
    }
}

fn frame(a: &App) -> Vec<Draw> {
    let mut out = Vec::new();
    a.draw(&mut out);
    out
}

fn placed(out: &[Draw], id: usize) -> Option<[f32; 4]> {
    out.iter().find_map(|d| match *d {
        Draw::Tex {
            x, y, w, h, tex, ..
        } if tex == TexId::from_raw(id) => Some([x, y, w, h]),
        _ => None,
    })
}

fn drawn(out: &[Draw], id: usize) -> bool {
    placed(out, id).is_some()
}

#[test]
fn the_legend_says_change_on_a_value_row_and_open_on_a_row_that_opens() {
    let (_d, mut a, _) = on_carousel();
    fake_faces(&mut a);
    a.apply(Action::QuickMenu);
    for row in QuickRow::ALL {
        assert_eq!(a.quick_menu(), Some(row));
        let out = frame(&a);
        assert!(drawn(&out, 400));
        assert_eq!(drawn(&out, 401), !row.opens());
        assert_eq!(drawn(&out, 402), row.opens());
        press(&mut a, Btn::Down);
    }
}

#[test]
fn the_arrows_stand_only_around_the_selected_rows_value() {
    let (_d, mut a, _) = on_carousel();
    fake_faces(&mut a);
    a.apply(Action::QuickMenu);
    let out = frame(&a);
    assert!(drawn(&out, 300) && drawn(&out, 301));
    assert!(drawn(&out, value(QuickValue::Speed6, true)));
    assert!(drawn(&out, value(QuickValue::Off, false)));
    assert!(drawn(&out, value(QuickValue::On, false)));
    assert!(drawn(&out, 500));

    for _ in 0..QuickRow::DateTime.index() {
        press(&mut a, Btn::Down);
    }
    let out = frame(&a);
    assert!(!drawn(&out, 300) && !drawn(&out, 301));
    assert!(drawn(&out, 501));
    assert!(drawn(&out, value(QuickValue::Speed6, false)));
}

#[test]
fn the_bar_runs_edge_to_edge_behind_the_selected_row() {
    let (_d, mut a, _) = on_carousel();
    fake_faces(&mut a);
    a.apply(Action::QuickMenu);
    for row in QuickRow::ALL {
        let bars: Vec<_> = frame(&a)
            .into_iter()
            .filter_map(|d| match d {
                Draw::Rect { x, y, w, h, colour } if colour == edge() => Some([x, y, w, h]),
                _ => None,
            })
            .collect();
        let top = QUICK_TOP + QUICK_PITCH * row.index() as f32;
        assert_eq!(
            bars,
            vec![[0.0, top + 4.0, OUT_W as f32, QUICK_PITCH - 8.0]]
        );
        press(&mut a, Btn::Down);
    }
}

#[test]
fn labels_start_and_values_end_thirty_two_pixels_in() {
    let (_d, mut a, _) = on_carousel();
    fake_faces(&mut a);
    a.apply(Action::QuickMenu);
    let out = frame(&a);
    let right = OUT_W as f32 - QUICK_EDGE;
    for row in QuickRow::ALL {
        let [x, ..] = placed(&out, 100 + row.label_index()).expect("a label was not drawn");
        assert_eq!(x + MENU_PAD as f32, QUICK_EDGE);
    }
    let [x, _, w, _] = placed(&out, value(QuickValue::On, false)).expect("rumble's value");
    assert_eq!(x + w - MENU_PAD as f32, right);
    let [x, _, w, _] = placed(&out, 301).expect("the right arrow");
    assert_eq!(x + w, right);
}

#[test]
fn lcd_effect_flips_on_either_arrow_and_writes_lcd_txt() {
    let (d, mut a, _) = on_carousel();
    open_at(&mut a, QuickRow::LcdEffect);
    assert!(a.lcd_enabled());
    assert!(slot_store::read_lcd(d.path()));
    press(&mut a, Btn::Right);
    assert!(!a.lcd_enabled());
    assert!(!slot_store::read_lcd(d.path()));
    assert_eq!(a.quick_value(QuickRow::LcdEffect), Some(QuickValue::Off));
    press(&mut a, Btn::Left);
    assert!(a.lcd_enabled());
    assert!(slot_store::read_lcd(d.path()));
}

#[test]
fn picture_in_play_settings_cycles_and_persists() {
    let d = common::tmp_root_with_gb_carts(&["Tetris", "Mario"]);
    let mut a = app_playing_in(d.path(), "Tetris");
    a.apply(Action::QuickMenu);
    open_play_row(&mut a, QuickRow::Picture);
    assert_eq!(a.quick_value(QuickRow::Picture), Some(QuickValue::ActualSize));
    press(&mut a, Btn::Right);
    assert_eq!(a.video_mode(), slot::video_mode::VideoMode::Stretch);
    assert_eq!(
        slot::video_mode::video_mode_for(d.path(), "Tetris"),
        slot::video_mode::VideoMode::Stretch
    );
    press(&mut a, Btn::Left);
    assert_eq!(a.video_mode(), slot::video_mode::VideoMode::Actual);
}

/// Walk the in-game display menu to `row` after it is already open.
fn open_play_row(a: &mut App, row: QuickRow) {
    let rows = [
        QuickRow::LcdEffect,
        QuickRow::ColourCorrection,
        QuickRow::Overlay,
        QuickRow::Picture,
    ];
    let i = rows.iter().position(|r| *r == row).expect("playing row");
    for _ in 0..i {
        press(a, Btn::Down);
    }
    assert_eq!(a.quick_menu(), Some(row));
}

#[test]
fn only_the_clock_from_the_menu_offers_b_back() {
    let (_d, mut a, _) = on_carousel();
    fake_faces(&mut a);
    a.set_clock_faces(TexId::from_raw(600), TexId::from_raw(601));
    open_at(&mut a, QuickRow::DateTime);
    press(&mut a, Btn::A);
    let out = frame(&a);
    assert!(drawn(&out, 601));
    assert!(drawn(&out, 400));

    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    let (mut first, _) = app_booting_with_clock(d.path());
    fake_faces(&mut first);
    first.set_clock_faces(TexId::from_raw(600), TexId::from_raw(601));
    let out = frame(&first);
    assert!(drawn(&out, 601));
    assert!(!drawn(&out, 400));
}
