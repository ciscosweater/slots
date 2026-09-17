mod common;

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use common::{
    app_playing_in, boot, panel, panel_with_charge, session_with_platform, tmp_root_with_carts,
    tmp_root_with_real_carts, StubSnapshot,
};
use slot::app::Phase;
use slot::emu::Speed;
use slot::session::Session;
use slot_gfx::Draw;
use slot_input::{Action, Btn, Millis, RawEvent, POWER_HOLD_MS};
use slot_store::{read_slot_state, write_slot_state, Core, Platform, SlotState, StateRing};

const FRAME_MS: Millis = 16;
const DT: f32 = 1.0 / 60.0;

#[test]
fn lid_close_flushes_resume_before_dozing() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::LidClose);
    let r = StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald");
    assert!(
        r.read_resume().unwrap().is_some(),
        "state must be durable before doze"
    );
    assert!(matches!(a.phase(), Phase::Doze { .. }));
    assert!(
        r.list().unwrap().is_empty(),
        "lid close must not create a polaroid"
    );
}

#[test]
fn lid_open_returns_to_the_game_without_a_button() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::LidClose);
    a.apply(Action::LidOpen);
    assert!(matches!(a.phase(), Phase::Playing { .. }));
}

#[test]
fn doze_timeout_powers_off_with_the_cart_still_seated() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::LidClose);
    a.on_doze_timeout();
    assert_eq!(
        read_slot_state(d.path()).cart,
        Some("Emerald".into()),
        "power off is not an eject"
    );
}

/// Closing the lid on an empty slot is still a doze. There is nothing to flush and nothing
/// to wake back into.
#[test]
fn lid_close_on_the_shelf_wakes_back_to_the_shelf() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = boot(d.path());
    a.set_snapshot(StubSnapshot::boxed());
    a.apply(Action::LidClose);
    assert!(matches!(a.phase(), Phase::Doze { cart: None }));
    assert!(
        StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald")
            .read_resume()
            .unwrap()
            .is_none()
    );
    a.apply(Action::LidOpen);
    assert!(matches!(a.phase(), Phase::Shelf));
}

/// The switcher is a pause over the game, so the lid closes on the game underneath it and
/// opens back onto it.
#[test]
fn lid_close_over_the_switcher_wakes_into_the_game() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::SaveState);
    a.apply(Action::Polaroids);
    a.apply(Action::LidClose);
    a.apply(Action::LidOpen);
    assert!(matches!(a.phase(), Phase::Playing { .. }));
    assert_eq!(
        StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald")
            .list()
            .unwrap()
            .len(),
        1,
        "only the deliberate save belongs in the ring"
    );
}

/// The first timeout enters the throttled standby stage; the second powers off.
#[test]
fn a_doze_that_outlasts_the_timeout_powers_off_by_itself() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.set_power(panel(d.path(), Duration::from_secs(2)).0);
    a.apply(Action::LidClose);
    for _ in 0..100 {
        a.update(1.0 / 60.0);
    }
    assert!(!a.powering_off(), "1.6 s is short of the 2 s timeout");
    for _ in 0..40 {
        a.update(1.0 / 60.0);
    }
    assert!(a.standby(), "the first timeout did not enter standby");
    assert!(!a.powering_off(), "standby skipped its five-minute window");
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    assert!(a.powering_off());
    assert!(
        !a.standby(),
        "the shutdown screen must leave standby so it can render"
    );
}

#[test]
fn standby_deadlines_use_absolute_time_not_frame_time() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.set_power(panel(d.path(), Duration::from_secs(2)).0);
    a.apply(Action::LidClose);

    a.tick_ms(10_000);
    assert!(a.standby());
    assert!(!a.powering_off());

    a.tick_ms(12_000);
    assert!(a.powering_off(), "the second absolute deadline was missed");
}

#[test]
fn plugging_in_during_standby_cancels_it_and_restarts_the_grace_period() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    let (power, charge) = panel_with_charge(d.path(), Duration::from_secs(2), 1);
    a.set_power(power);
    a.apply(Action::LidClose);

    a.tick_ms(4_000);
    assert!(a.standby());

    charge.store(2, Ordering::Relaxed);
    a.tick_ms(4_500);
    assert!(
        !a.standby(),
        "charger insertion should leave userspace standby"
    );

    charge.store(1, Ordering::Relaxed);
    a.tick_ms(6_000);
    assert!(
        !a.powering_off(),
        "unplugging should get a fresh doze grace period"
    );
}

#[test]
fn standby_keeps_the_led_off_while_charge_polling_continues() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let (mut a, _charge, _percent, led, _writes) =
        common::app_playing_with_led(d.path(), "Emerald");
    a.apply(Action::LidClose);

    a.tick_ms(100_000);
    assert!(a.standby());
    assert_eq!(
        led.load(Ordering::Relaxed),
        common::led_code(slot_power::LedState::Off)
    );

    // The fast LED tick runs again, but it must not light the case behind the closed lid.
    a.tick_ms(110_000);
    assert_eq!(
        led.load(Ordering::Relaxed),
        common::led_code(slot_power::LedState::Off)
    );
}

#[test]
fn a_stray_doze_timeout_does_not_power_off_a_running_game() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.on_doze_timeout();
    assert!(!a.powering_off());
    assert!(matches!(a.phase(), Phase::Playing { .. }));
}

/// Kernel suspend support is deliberately irrelevant: the reliable first stage is the
/// userspace standby whose POWER polling and second deadline keep running.
#[test]
fn a_platform_with_suspend_still_uses_the_reliable_standby_stage() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.set_power(common::panel_that_wakes(d.path(), Duration::from_secs(2)));
    a.apply(Action::LidClose);
    assert!(matches!(a.phase(), Phase::Doze { .. }));
    a.on_doze_timeout();
    assert!(a.standby());
    assert!(!a.powering_off());
    assert!(matches!(a.phase(), Phase::Doze { .. }));
    assert!(
        StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald")
            .read_resume()
            .unwrap()
            .is_some(),
        "resume.state from the doze is what the next boot would load"
    );
}

/// The panel comes up at the level the card remembers, not at whatever the kernel left it.
#[test]
fn the_backlight_follows_brightness_from_boot() {
    let d = tmp_root_with_carts(&["Emerald"]);
    write_slot_state(
        d.path(),
        &SlotState {
            brightness: 3,
            clock_set: true,
            utc_offset_min: 0,
            ..Default::default()
        },
    )
    .unwrap();
    let mut a = boot(d.path());
    let (power, step) = panel(d.path(), Duration::from_secs(60));
    a.set_power(power);
    assert_eq!(step.load(Ordering::Relaxed), 3);
    a.apply(Action::BrightnessUp);
    assert_eq!(step.load(Ordering::Relaxed), 4);
}

/// The hold replaces whatever was on screen with the shutdown screen immediately.
#[test]
fn the_hold_covers_the_screen_with_shutdown() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::PowerHold);

    let mut out = Vec::new();
    a.draw(&mut out);

    match out.first() {
        Some(Draw::Rect { w, h, colour, .. }) => {
            assert_eq!(*colour, [0.0, 0.0, 0.0, 1.0]);
            assert!(*w > 0.0 && *h > 0.0, "and it covers the panel");
        }
        other => panic!("the hold drew {other:?} rather than shutdown"),
    }
    // Only the ground is reachable here: the plate is sized from the uploaded row faces, and
    // a unit test has no compositor to upload them with. What this does prove is that the
    // menu replaces the screen rather than sitting over a game still being drawn.
    assert_eq!(
        out.len(),
        1,
        "nothing of the previous phase survives shutdown"
    );
}

/// rcK stops the frontend and unloads the GPU module before the kernel is allowed to halt,
/// which takes about five seconds on this hardware. A panel that simply goes black for five
/// seconds is one the user reads as hung — this device has already been opened once over
/// exactly that confusion — so the shutdown says so, over whatever was on screen.
#[test]
fn a_power_off_draws_a_shutdown_screen_over_everything() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::PowerHold);
    a.apply(Action::GbaDown(Btn::Down));
    a.apply(Action::GbaDown(Btn::A));

    let mut out = Vec::new();
    a.draw(&mut out);

    match out.first() {
        Some(Draw::Rect { w, h, colour, .. }) => {
            assert_eq!(*colour, [0.0, 0.0, 0.0, 1.0], "the shutdown is black");
            assert!(*w > 0.0 && *h > 0.0, "and covers the panel");
        }
        other => panic!("the shutdown drew {other:?} rather than a panel of black"),
    }
    // One draw, not two: the line itself is a texture uploaded by the binary at boot, and a
    // unit test has no compositor to upload it with. The screen is still correct without it.
    assert_eq!(
        out.len(),
        1,
        "nothing of the previous phase survives the shutdown screen"
    );
}

/// A held POWER commits directly to a graceful power off.
#[test]
fn a_hold_commits_to_power_off_without_a_menu() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::PowerHold);
    assert_eq!(a.power_menu(), None);
    assert!(a.powering_off() && !a.restarting());
    assert!(
        StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald")
            .read_resume()
            .unwrap()
            .is_some(),
        "durable before the OS powers off"
    );
}

#[test]
fn directions_after_a_power_hold_cannot_open_a_menu() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::PowerHold);
    a.apply(Action::GbaDown(Btn::Up));
    a.apply(Action::GbaDown(Btn::Down));
    assert_eq!(a.power_menu(), None);
    assert!(a.powering_off());
}

#[test]
fn b_cannot_cancel_a_committed_power_off() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::PowerHold);
    a.apply(Action::GbaDown(Btn::B));
    assert_eq!(a.power_menu(), None);
    assert!(a.powering_off() && !a.restarting());
}

#[test]
fn a_cannot_turn_a_power_hold_into_restart() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::PowerHold);
    a.apply(Action::GbaDown(Btn::A));
    assert!(a.powering_off());
    assert!(!a.restarting());
}

/// The decision and the moment the machine may stop are different things. Rendering the
/// shutdown screen out of band — an extra draw and swap between the choice and `poweroff` —
/// hung the device on a GPU that was about to be torn down: slot never reached `poweroff` at
/// all, init was never signalled, and it took the PMIC held down to recover. So the ordinary
/// loop draws the screen and the binary waits for it.
#[test]
fn the_shutdown_screen_is_up_before_the_machine_may_stop() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::PowerHold);
    a.apply(Action::GbaDown(Btn::Down));
    a.apply(Action::GbaDown(Btn::A));

    assert!(a.powering_off(), "the choice decides immediately");
    assert!(
        !a.ready_to_power_off(),
        "but the binary may not act until the screen has been presented"
    );

    let mut out = Vec::new();
    a.draw(&mut out);
    assert!(
        matches!(out.first(), Some(Draw::Rect { colour, .. }) if *colour == [0.0, 0.0, 0.0, 1.0]),
        "and the screen is what the loop is drawing in the meantime"
    );

    // Absolute, not a delta: `tick_ms` takes the later of the two, so a small number is a
    // no-op against whatever clock the harness already left behind.
    a.tick_ms(600_000);
    assert!(a.ready_to_power_off(), "then it may stop");
}

/// The reported hang: close the lid, wait out the doze, and the device sits there until the
/// lid is opened again — at which point it powers off, having thrown away the session the
/// user just came back for.
///
/// `doze_expired` is a level rather than an edge, and `begin_power_off` leaves the phase on
/// `Doze`, so `timers` re-armed the shutdown every frame and `act_at` walked ahead of the
/// clock forever. The three seconds the screen is meant to be up became a deadline that
/// could never arrive.
#[test]
fn a_dozing_device_powers_off_by_itself_rather_than_waiting_for_the_lid() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.set_power(panel(d.path(), Duration::from_secs(2)).0);
    a.apply(Action::LidClose);
    for _ in 0..180 {
        a.update(1.0 / 60.0);
    }
    assert!(a.standby(), "three seconds should reach the standby stage");
    assert!(!a.powering_off(), "the second timeout has not elapsed");
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    assert!(
        a.powering_off(),
        "two standby seconds should start poweroff"
    );
    assert!(
        !a.ready_to_power_off(),
        "Powering Down still has two of its three seconds left"
    );
    for _ in 0..180 {
        a.update(1.0 / 60.0);
    }
    assert!(
        a.ready_to_power_off(),
        "the shutdown screen has had its three seconds and the machine may stop"
    );
}

/// The shutdown screen pauses the core immediately. A device with five seconds of rcK ahead
/// of it is not one that should still be playing a game it has already said goodbye to.
#[test]
fn a_committed_shutdown_holds_the_core_still() {
    let d = tmp_root_with_real_carts(&["Advance Wars", "Emerald"]);
    let (mut s, _motor) = session_with_platform(d.path());
    let mut now = 0;
    play(&mut s, &mut now);

    hold_power(&mut s, &mut now);
    assert!(s.app().powering_off(), "the hold did not commit");
    s.update(DT);
    await_paused(&mut s);
}

/// The shutdown screen turns the LED off immediately. Leaving the codec open behind that is
/// a hiss with the case already dark, and if init then fails to cut the rails the machine
/// sits there "off" still making noise.
#[test]
fn a_committed_shutdown_closes_the_audio_device() {
    let d = tmp_root_with_real_carts(&["Advance Wars", "Emerald"]);
    let (mut s, _motor) = session_with_platform(d.path());
    let mut now = 0;
    play(&mut s, &mut now);
    hold_power(&mut s, &mut now);
    assert!(s.app().powering_off(), "the hold did not commit");
    s.update(DT);
    assert!(
        !s.audio_device_open(),
        "the codec was still open after the LED went off"
    );
}

/// Same hiss, earlier: a shut lid blanks the panel and the amp has to follow, not wait for
/// the standby timeout. Wake puts the device back only if it was running before.
#[test]
fn doze_closes_the_audio_device() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let (mut s, _motor) = session_with_platform(d.path());
    s.update(DT);
    let had = s.audio_device_open();
    s.feed([RawEvent::Down(Btn::Lid)], 0);
    s.update(DT);
    assert!(
        !s.audio_device_open(),
        "the codec stayed open behind a dark panel"
    );
    s.feed([RawEvent::Up(Btn::Lid)], FRAME_MS);
    s.update(DT);
    if had {
        assert!(
            s.audio_device_open(),
            "wake did not reopen a codec that was running before the doze"
        );
    }
}

/// Waits for the worker to report that it read `Paused`, rather than for the frame count to
/// sit still: a descheduled worker and a stopped one look identical from a count.
fn await_paused(s: &mut Session) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while s.observed_speed() != Some(Speed::Paused) {
        assert!(
            Instant::now() < deadline,
            "the core was still running behind the shutdown"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// POWER down, then frames until the gesture layer's own tick raises the menu.
fn hold_power(s: &mut Session, now: &mut Millis) {
    let pressed = *now;
    event(s, RawEvent::Down(Btn::Power), now);
    while *now < pressed + POWER_HOLD_MS + FRAME_MS {
        step(s, now);
    }
}

fn step(s: &mut Session, now: &mut Millis) {
    *now += FRAME_MS;
    s.feed([], *now);
    s.update(DT);
}

fn event(s: &mut Session, ev: RawEvent, now: &mut Millis) {
    *now += FRAME_MS;
    s.feed([ev], *now);
    s.update(DT);
}

/// Puts the selected cart in and waits out the load, which happens on its own thread.
fn play(s: &mut Session, now: &mut Millis) {
    event(s, RawEvent::Down(Btn::A), now);
    event(s, RawEvent::Up(Btn::A), now);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        step(s, now);
        std::thread::sleep(Duration::from_millis(1));
    }
    // The worker has to have taken a turn at Normal before a test may claim it stopped.
    let deadline = Instant::now() + Duration::from_secs(5);
    while s.observed_speed() != Some(Speed::Normal) {
        assert!(Instant::now() < deadline, "the core never started");
        std::thread::sleep(Duration::from_millis(1));
    }
}
