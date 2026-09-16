mod common;

use std::time::{Duration, Instant};

use slot::app::Phase;
use slot::session::Session;
use slot_input::{Btn, Millis, RawEvent};
use slot_store::{write_slot_state, SlotState};
use slot_ui::Icon;

/// A latch is the one fast forward state that outlives the button, so it is the one the badge
/// has to be told about separately: a held R2 leaves with the finger, a latched one does not.
#[test]
fn the_badge_follows_the_latch_rather_than_the_button() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    write_slot_state(
        d.path(),
        &SlotState {
            cart: Some("Emerald".into()),
            clock_set: true,
            utc_offset_min: 0,
            ..Default::default()
        },
    )
    .expect("write slot.state");
    let mut s = Session::boot(d.path().to_path_buf());
    let mut now: Millis = 0;

    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        step(&mut s, &mut now, None);
        std::thread::sleep(Duration::from_millis(1));
    }

    step(&mut s, &mut now, Some(RawEvent::Down(Btn::R2)));
    assert!(badge(&s).is_some(), "no badge while R2 is held");
    step(&mut s, &mut now, Some(RawEvent::Up(Btn::R2)));
    assert!(badge(&s).is_none(), "the badge outlived the hold");

    // Well past the double tap window, so the next press is a first tap rather than a second.
    for _ in 0..20 {
        step(&mut s, &mut now, None);
    }
    step(&mut s, &mut now, Some(RawEvent::Down(Btn::R2)));
    step(&mut s, &mut now, Some(RawEvent::Up(Btn::R2)));
    step(&mut s, &mut now, Some(RawEvent::Down(Btn::R2)));
    step(&mut s, &mut now, Some(RawEvent::Up(Btn::R2)));
    assert!(badge(&s).is_some(), "the latched badge left with R2");

    step(&mut s, &mut now, Some(RawEvent::Down(Btn::R2)));
    step(&mut s, &mut now, Some(RawEvent::Up(Btn::R2)));
    assert!(badge(&s).is_none(), "the badge survived the latch it lost");
}

/// The quick menu's fast-forward settings are read from the card and handed to the emulator
/// thread, so the cart seated after they were chosen runs with them.
#[test]
fn the_fast_forward_settings_reach_the_emulator() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    write_slot_state(
        d.path(),
        &SlotState {
            cart: Some("Emerald".into()),
            clock_set: true,
            ff_speed: 2,
            ff_sound: true,
            ..Default::default()
        },
    )
    .expect("write slot.state");
    let mut s = Session::boot(d.path().to_path_buf());
    let mut now: Millis = 0;

    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        step(&mut s, &mut now, None);
        std::thread::sleep(Duration::from_millis(1));
    }

    let emu = s.emu().expect("a seated cart has a core");
    assert_eq!(
        emu.fast_steps(),
        2,
        "the chosen speed never reached the core"
    );
    assert!(emu.ff_sound(), "fast-forward sound never reached the core");
}

/// The quick menu owns R2 while it is open. A double tap there must not leave the next R2 press
/// on the carousel latched, because that press is the category control rather than fast-forward.
#[test]
fn r2_taps_in_the_quick_menu_do_not_consume_the_next_category_tap() {
    let d = common::tmp_root_with_carts(&["Emerald", "Fusion"]);
    let (mut s, _motor) = common::session_with_platform(d.path());
    let mut now = 0;

    step(&mut s, &mut now, Some(RawEvent::Down(Btn::Menu)));
    step(&mut s, &mut now, Some(RawEvent::Up(Btn::Menu)));
    assert!(matches!(s.app().phase(), Phase::QuickMenu { .. }));

    for event in [
        RawEvent::Down(Btn::R2),
        RawEvent::Up(Btn::R2),
        RawEvent::Down(Btn::R2),
        RawEvent::Up(Btn::R2),
    ] {
        step(&mut s, &mut now, Some(event));
    }
    step(&mut s, &mut now, Some(RawEvent::Down(Btn::B)));
    step(&mut s, &mut now, Some(RawEvent::Up(Btn::B)));
    assert!(matches!(s.app().phase(), Phase::Shelf));

    step(&mut s, &mut now, Some(RawEvent::Down(Btn::R2)));
    assert_eq!(s.app().shelf_category(), 1);
}

fn step(s: &mut Session, now: &mut Millis, ev: Option<RawEvent>) {
    *now += 16;
    s.feed(ev, *now);
    s.update(1.0 / 60.0);
}

/// The badge is read as state rather than as quads: the glyph needs an uploaded face and a
/// headless session has none, so a drawn list would be empty however R2 was pressed.
fn badge(s: &Session) -> Option<Icon> {
    s.app().ff_badge()
}
