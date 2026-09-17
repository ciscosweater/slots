mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use common::core_lock;
use slot::app::Phase;
use slot::audio::{AudioSink, StubSink};
use slot::core::open_core_for;
use slot::emu::{CoreState, EmuHandle};
use slot::session::Session;
use slot_input::{Btn, RawEvent};
use slot_retro::{ButtonMask, RetroCore};
use slot_store::Core;

fn rom(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&p, common::gba_rom()).expect("write rom");
    p
}

/// The two cores are indistinguishable through the trait except by what they serialize:
/// mGBA writes out a whole machine, the mock writes a frame counter.
fn is_mgba(core: &mut dyn RetroCore, rom: &Path) -> bool {
    core.load(rom).expect("core refused the test rom");
    core.run_frame(ButtonMask::default());
    core.serialize().expect("core gave up no state").len() > 100_000
}

#[test]
fn the_vendored_core_is_preferred_over_the_mock() {
    let _g = core_lock();
    let Some(dylib) = common::vendored_core() else {
        return;
    };
    let d = common::tmp_root_with_real_carts(&[]);
    let Some(mut core) = open_core_for(d.path(), Core::Mgba, &[dylib]) else {
        return;
    };
    assert!(
        is_mgba(core.as_mut(), &rom("preferred.gba")),
        "the mock ran with a vendored core sitting right there"
    );
}

#[test]
fn a_missing_core_does_not_open() {
    let d = common::tmp_root_with_real_carts(&[]);
    assert!(
        open_core_for(d.path(), Core::Mgba, &[PathBuf::from("no/such/core.dylib")]).is_none(),
        "a missing dylib must not seat a mock"
    );
}

/// A rom the core will not take has to reach the app as `Failed`. Anything else leaves the
/// insert animation waiting on a core that is never coming.
#[test]
fn a_rom_the_real_core_refuses_reports_failed() {
    let _g = core_lock();
    let Some(dylib) = common::vendored_core() else {
        return;
    };
    let d = common::tmp_root_with_carts(&["Broken"]);
    let Some(opened) = open_core_for(d.path(), Core::Mgba, &[dylib]) else {
        return;
    };
    let emu = EmuHandle::spawn(
        opened,
        d.path().join("Games/Broken.gba"),
        StubSink::new().ring(),
        None,
        None,
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while emu.state() == CoreState::Loading {
        assert!(Instant::now() < deadline, "the core never settled");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(emu.state(), CoreState::Failed);
}

/// And the refusal has to reach the slot. The mock takes anything, so this is the one path
/// that only exists once the real core is the default.
#[test]
fn a_cart_the_real_core_refuses_comes_back_out_of_the_slot() {
    let _g = core_lock();
    let Some(dylib) = common::vendored_core() else {
        return;
    };
    let d = common::tmp_root_with_carts(&["Broken"]);
    if open_core_for(d.path(), Core::Mgba, std::slice::from_ref(&dylib)).is_none() {
        return;
    }
    std::env::set_var("SLOT_CORE", &dylib);
    common::clocked(d.path());
    let mut s = Session::boot(d.path().to_path_buf());
    s.feed([RawEvent::Down(Btn::A)], 16);
    s.update(1.0 / 60.0);
    assert!(matches!(s.app().phase(), Phase::Inserting { .. }));

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut now = 16;
    while !matches!(s.app().phase(), Phase::Shelf) {
        assert!(
            Instant::now() < deadline,
            "the refused cart never came back out"
        );
        now += 16;
        s.feed([], now);
        s.update(1.0 / 60.0);
    }
}

/// A missing dylib is the same refusal a rom the core will not take is: the cart comes back
/// out rather than seating a test pattern that reads as a broken game.
#[test]
fn a_missing_core_comes_back_out_of_the_slot() {
    let _g = core_lock();
    std::env::set_var("SLOT_CORE", "no/such/core.dylib");
    let d = common::tmp_root_with_carts(&["Emerald", "Fusion"]);
    common::clocked(d.path());
    let mut s = Session::boot(d.path().to_path_buf());
    s.feed([RawEvent::Down(Btn::A)], 16);
    s.feed([RawEvent::Up(Btn::A)], 32);
    s.update(1.0 / 60.0);
    std::env::remove_var("SLOT_CORE");
    assert!(
        matches!(s.app().phase(), Phase::Ejecting { .. } | Phase::Shelf),
        "a missing core seated: {:?}",
        s.app().phase()
    );
}

/// A missing dylib must refuse on the first present and stay refused. Re-entering Inserting
/// (or reopening the same absent path every frame) thrashes the card and floods the log.
#[test]
fn a_missing_core_is_refused_once_not_every_frame() {
    let _g = core_lock();
    std::env::set_var("SLOT_CORE", "no/such/core.dylib");
    let d = common::tmp_root_with_carts(&["Emerald", "Fusion"]);
    common::clocked(d.path());
    let mut s = Session::boot(d.path().to_path_buf());
    s.feed([RawEvent::Down(Btn::A)], 16);
    s.feed([RawEvent::Up(Btn::A)], 32);
    let mut now = 32u64;
    for _ in 0..30 {
        now += 16;
        s.feed([], now);
        s.update(1.0 / 60.0);
        assert!(
            !matches!(s.app().phase(), Phase::Inserting { .. }),
            "missing core kept retrying the insert: {:?}",
            s.app().phase()
        );
    }
    std::env::remove_var("SLOT_CORE");
    assert!(
        matches!(s.app().phase(), Phase::Ejecting { .. } | Phase::Shelf),
        "a missing core seated: {:?}",
        s.app().phase()
    );
}
