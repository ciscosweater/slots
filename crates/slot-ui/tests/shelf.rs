use slot_power::{Battery, Charge};
use slot_store::{Cart, Platform};
use slot_ui::{draw_footer, label_colour, Draw, Printed, Shelf, TexId, CART_H, CART_W, OUT_W};

fn shelf_with(n: usize) -> Shelf {
    Shelf::new(
        (0..n)
            .map(|i| Cart {
                stem: format!("Game {i}"),
                rom: format!("Games/Game {i}.gba").into(),
                artwork: None,
                label: None,
                code: String::new(),
                title: format!("GAME {i}"),
                platform: slot_store::Platform::Gba,
            })
            .collect(),
    )
}

#[test]
fn face_upload_order_prioritises_both_edges_of_the_ring() {
    let shelf = shelf_with(8);
    let order = shelf
        .face_upload_order()
        .into_iter()
        .map(|cart| cart.stem)
        .collect::<Vec<_>>();
    assert_eq!(
        &order[..5],
        ["Game 0", "Game 7", "Game 1", "Game 6", "Game 2"],
        "the wrapped neighbour must be hydrated before the far end of the queue"
    );
}

#[test]
fn a_missing_face_can_use_a_shape_placeholder_instead_of_a_square() {
    let mut shelf = shelf_with(1);
    shelf.set_placeholder(TexId::from_raw(77));
    let mut out = Vec::new();
    shelf.draw_row(None, 0.0, 0.0, 1.0, &mut out);
    assert!(
        matches!(out.first(), Some(Draw::Tex { tex, .. }) if *tex == TexId::from_raw(77)),
        "the shelf fell back to a rectangular placeholder: {out:?}"
    );
}

#[test]
fn a_complete_artwork_face_keeps_its_width_and_natural_height() {
    let mut shelf = shelf_with(1);
    shelf.set_face_with_size("Game 0", TexId::from_raw(88), (CART_W, 142));
    let mut out = Vec::new();
    shelf.draw_row(None, 0.0, 0.0, 1.0, &mut out);
    assert!(
        out.iter().any(|draw| matches!(draw,
            slot_ui::Draw::Tex { tex, w, h, .. }
                if *tex == TexId::from_raw(88)
                    && (*w - CART_W as f32).abs() < 0.01
                    && (*h - 142.0).abs() < 0.01
        )),
        "the uploaded artwork was stretched to the generated shell size: {out:?}"
    );
}

#[test]
fn uploading_a_face_does_not_restart_shelf_motion_or_repeat() {
    let mut shelf = shelf_with(5);
    shelf.hold_right(0);
    shelf.update(0.02);
    let moving_scroll = shelf.scroll;

    shelf.set_face("Game 4", TexId::from_raw(123));

    assert_eq!(
        shelf.scroll, moving_scroll,
        "a late face upload snapped the spring back to the selected cart"
    );
    shelf.tick(400);
    assert_eq!(
        shelf.index, 2,
        "a late face upload cancelled the held-direction repeat"
    );
}

#[test]
fn categories_cycle_and_filter_by_platform() {
    let carts = [Platform::Gba, Platform::Gb, Platform::Gbc]
        .into_iter()
        .enumerate()
        .map(|(i, platform)| Cart {
            stem: format!("Game {i}"),
            rom: format!("Games/Game {i}").into(),
            artwork: None,
            label: None,
            code: String::new(),
            title: format!("GAME {i}"),
            platform,
        })
        .collect();
    let mut shelf = Shelf::new(carts);
    shelf.set_recents(vec!["Game 2".to_string(), "Game 0".to_string()]);
    assert_eq!(shelf.carts.len(), 3);
    shelf.next_category();
    assert_eq!(
        shelf
            .carts
            .iter()
            .map(|cart| cart.stem.as_str())
            .collect::<Vec<_>>(),
        ["Game 2", "Game 0"]
    );
    shelf.next_category();
    assert_eq!(
        (shelf.category(), shelf.carts[0].platform),
        (2, Platform::Gba)
    );
    shelf.next_category();
    assert_eq!(
        (shelf.category(), shelf.carts[0].platform),
        (3, Platform::Gb)
    );
    shelf.next_category();
    assert_eq!(
        (shelf.category(), shelf.carts[0].platform),
        (4, Platform::Gbc)
    );
    shelf.next_category();
    assert_eq!((shelf.category(), shelf.carts.len()), (0, 3));
    shelf.previous_category();
    assert_eq!(shelf.category(), 4);
}

#[test]
fn favorites_do_not_override_recent_order() {
    use std::collections::BTreeSet;

    let mut shelf = shelf_with(3);
    shelf.set_recents(vec!["Game 2".to_string(), "Game 0".to_string()]);
    shelf.next_category();
    shelf.sort_by_favorites(&BTreeSet::from(["Game 0".to_string()]));
    assert_eq!(
        shelf
            .carts
            .iter()
            .map(|cart| cart.stem.as_str())
            .collect::<Vec<_>>(),
        ["Game 2", "Game 0"]
    );
}

#[test]
fn platform_categories_without_roms_are_skipped() {
    let mut shelf = shelf_with(3);
    shelf.next_category();
    assert_eq!(shelf.category(), 1, "REC is always available");
    shelf.next_category();
    assert_eq!(shelf.category(), 2, "the library contains GBA games");
    shelf.next_category();
    assert_eq!(
        shelf.category(),
        0,
        "empty GB and GBC categories were not skipped"
    );
    shelf.previous_category();
    assert_eq!(
        shelf.category(),
        2,
        "reverse navigation did not skip GB and GBC"
    );
}

fn placed(s: &Shelf) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    s.draw_row(None, 0.0, 0.0, 1.0, &mut out);
    out.iter()
        .map(|d| match *d {
            Draw::Rect { x, w, .. } => (x, w),
            Draw::Tex { x, w, .. } => (x, w),
            Draw::Turned { x, w, .. } => (x, w),
            Draw::Game | Draw::Shot { .. } => (0.0, OUT_W as f32),
        })
        .collect()
}

fn xw(d: &Draw) -> (f32, f32) {
    match *d {
        Draw::Rect { x, w, .. } | Draw::Tex { x, w, .. } | Draw::Turned { x, w, .. } => (x, w),
        Draw::Game | Draw::Shot { .. } => (0.0, OUT_W as f32),
    }
}

fn settle(s: &mut Shelf) {
    for _ in 0..600 {
        s.update(1.0 / 60.0);
    }
}

/// Which cart each quad in the row belongs to. No faces are uploaded, so every cart draws
/// as a rect in its own label colour, and that colour is the only identity on offer. The
/// colour comes from the cleaned stem, not the header title.
fn drawn_cart_indices(out: &[Draw]) -> Vec<usize> {
    let keys: Vec<[u8; 3]> = (0..16)
        .map(|i| label_colour(&format!("Game {i}")))
        .collect();
    out.iter()
        .map(|d| {
            let Draw::Rect { colour, .. } = d else {
                panic!("a cart with no face should draw as a rect");
            };
            let rgb = [0, 1, 2].map(|c| (colour[c] * 255.0).round() as u8);
            keys.iter()
                .position(|k| *k == rgb)
                .unwrap_or_else(|| panic!("quad {rgb:?} belongs to no cart"))
        })
        .collect()
}

#[test]
fn three_carts_fit_across_the_shelf() {
    let row = CART_W * 3;
    assert!(
        row <= OUT_W,
        "three carts are {row} px across a {OUT_W} px row, so it cannot show one either \
         side of the selection"
    );
}

#[test]
fn the_shelf_wraps_at_both_ends() {
    let mut s = shelf_with(4);
    s.left();
    assert_eq!(
        s.index, 3,
        "going left from the first cart should reach the last"
    );
    s.right();
    assert_eq!(
        s.index, 0,
        "going right from the last cart should reach the first"
    );
}

#[test]
fn shoulders_jump_between_initial_letters_and_wrap() {
    let carts = ["Advance", "Astro", "Boktai", "Castlevania", "Crash"]
        .into_iter()
        .map(|stem| Cart {
            stem: stem.into(),
            rom: format!("Games/{stem}.gba").into(),
            artwork: None,
            label: None,
            code: String::new(),
            title: stem.to_uppercase(),
            platform: slot_store::Platform::Gba,
        })
        .collect();
    let mut s = Shelf::new(carts);

    s.next_letter();
    assert_eq!(s.index, 2, "R1 did not skip the rest of A");
    s.next_letter();
    assert_eq!(s.index, 3);
    s.next_letter();
    assert_eq!(s.index, 0, "R1 did not wrap from C to A");
    s.previous_letter();
    assert_eq!(s.index, 3, "L1 did not wrap to the beginning of C");
    s.previous_letter();
    assert_eq!(s.index, 2);
}

#[test]
fn shoulders_are_inert_when_every_cart_has_the_same_initial() {
    let mut s = shelf_with(4);
    s.next_letter();
    s.previous_letter();
    assert_eq!(s.index, 0);
}

/// The spring chases `scroll`. If wrapping is a bare index change it unwinds the whole row.
#[test]
fn wrapping_animates_one_step_not_the_long_way_back() {
    let mut s = shelf_with(8);
    for _ in 0..7 {
        s.right();
    }
    settle(&mut s);
    let before = s.scroll;
    s.right();
    let travel = (s.scroll_target() - before).abs();
    assert!(
        travel < 1.5,
        "the spring is travelling {travel} slots to move one"
    );
}

#[test]
fn a_settled_wrap_still_lands_on_the_selected_cart() {
    let mut s = shelf_with(5);
    s.left();
    settle(&mut s);
    assert_eq!(s.index, 4);
    assert!(
        (s.scroll.rem_euclid(5.0) - 4.0).abs() < 0.01,
        "scroll {} did not settle",
        s.scroll
    );
}

#[test]
fn the_neighbour_of_the_last_cart_is_the_first() {
    let s = shelf_with(4);
    assert_eq!(
        s.cart_at_offset(-1),
        Some(3),
        "left of the first is the last"
    );
    assert_eq!(s.cart_at_offset(1), Some(1));
}

/// Two carts would otherwise appear on both sides of the selection at once.
#[test]
fn no_cart_is_drawn_twice_in_one_row() {
    for n in [2usize, 3, 4] {
        let s = shelf_with(n);
        let mut out = Vec::new();
        s.draw_row(None, 0.0, 0.0, 1.0, &mut out);
        let drawn = drawn_cart_indices(&out);
        let mut uniq = drawn.clone();
        uniq.sort();
        uniq.dedup();
        assert_eq!(drawn.len(), uniq.len(), "{n} carts: one is on screen twice");
    }
}

#[test]
fn shelf_scroll_settles_on_the_selected_index() {
    let mut s = shelf_with(5);
    s.right();
    s.right();
    for _ in 0..600 {
        s.update(1.0 / 60.0);
    }
    assert!(
        (s.scroll - 2.0).abs() < 0.01,
        "scroll {} did not settle",
        s.scroll
    );
}

#[test]
fn scroll_never_overshoots_the_cart_it_lands_on() {
    let mut s = shelf_with(5);
    s.right();
    for _ in 0..600 {
        s.update(1.0 / 60.0);
        assert!(s.scroll <= 1.0 + 1e-4, "overshot to {}", s.scroll);
    }
}

#[test]
fn an_empty_shelf_is_inert() {
    let mut s = shelf_with(0);
    s.right();
    s.left();
    assert_eq!(s.index, 0);
    s.update(1.0 / 60.0);
    assert!(placed(&s).is_empty());
}

#[test]
fn the_selected_cart_is_centred_and_full_size() {
    let mut s = shelf_with(5);
    s.right();
    s.right();
    settle(&mut s);
    let (x, w) = placed(&s)
        .into_iter()
        .fold((0.0, 0.0), |a, b| if b.1 > a.1 { b } else { a });
    assert!((w - CART_W as f32).abs() < 0.5, "selected cart is {w} wide");
    let centre = x + w / 2.0;
    assert!(
        (centre - 360.0).abs() < 0.5,
        "selected cart centre is {centre}"
    );
}

#[test]
fn the_selected_gb_cart_is_vertically_centred() {
    let gb_cart = Cart {
        stem: "Red".into(),
        rom: "Games/Red.gb".into(),
        artwork: None,
        label: None,
        code: String::new(),
        title: "POKEMON RED".into(),
        platform: slot_store::Platform::Gb,
    };
    let s = Shelf::new(vec![gb_cart]);
    let mut out = Vec::new();
    s.draw_row(None, 0.0, 0.0, 1.0, &mut out);
    let Draw::Rect { y, h, .. } = out[0] else {
        panic!()
    };
    let centre_y = y + h / 2.0;
    assert!(
        (centre_y - 240.0).abs() < 0.5,
        "GB cart centre is {centre_y}, expected 240.0"
    );
}

#[test]
fn holding_a_direction_repeats_after_a_delay() {
    let mut s = shelf_with(6);
    s.hold_right(0);
    assert_eq!(s.index, 1, "the first press did not move");
    s.tick(399);
    assert_eq!(s.index, 1, "it repeated before the delay");
    s.tick(400);
    assert_eq!(s.index, 2, "it never repeated");
    s.tick(510);
    assert_eq!(s.index, 3);
    s.release_right();
    s.tick(2_000);
    assert_eq!(s.index, 3, "it kept repeating after release");
}

/// Letting go of one direction while the other is held is a change of direction, not a stop.
#[test]
fn the_other_direction_letting_go_does_not_stop_the_repeat() {
    let mut s = shelf_with(6);
    s.hold_right(0);
    s.release_left();
    s.tick(400);
    assert_eq!(s.index, 2, "releasing left stopped a held right");
}

#[test]
fn holding_a_direction_accelerates_repeat_rate() {
    let mut s = shelf_with(20);
    s.hold_right(0);
    assert_eq!(s.index, 1);
    s.tick(400); // 1st repeat (+400ms delay): next due at 400 + 110 = 510
    assert_eq!(s.index, 2);
    s.tick(510); // 2nd repeat (+110ms): next due at 510 + 110 = 620
    assert_eq!(s.index, 3);
    s.tick(620); // 3rd repeat (+110ms): next due at 620 + 85 = 705
    assert_eq!(s.index, 4);
    s.tick(705); // 4th repeat (+85ms): next due at 705 + 85 = 790
    assert_eq!(s.index, 5);
    s.tick(790); // 5th repeat (+85ms): next due at 790 + 65 = 855
    assert_eq!(s.index, 6);
    s.tick(855); // 6th repeat (+65ms): next due at 855 + 65 = 920
    assert_eq!(s.index, 7);
}

/// The gauge takes the shelf the wordmark had, at the same margin, so what is printed on the
/// case still lines up with the row above it. Charging, with a bolt supplied: the bolt's own
/// slot is reserved ahead of the capsule, so it is only while charging that anything actually
/// reaches all the way to the margin — discharging leaves that slot empty and the capsule
/// inset from it, which is the whole point of reserving it unconditionally.
#[test]
fn the_gauge_sits_where_the_wordmark_did() {
    let mut out = Vec::new();
    draw_footer(
        Some(Battery {
            percent: 68,
            charge: Charge::Charging,
        }),
        Printed { face: None, w: 30 },
        Some(TexId::from_raw(1)),
        Printed { face: None, w: 40 },
        &mut out,
    );
    let leftmost = out
        .iter()
        .map(|d| match *d {
            Draw::Rect { x, .. } | Draw::Tex { x, .. } => x,
            _ => f32::MAX,
        })
        .fold(f32::MAX, f32::min);
    assert_eq!(leftmost, 24.0, "the case margin is the case margin");
}

/// `draw_gauge`'s own suite proves the capsule holds still in isolation; `the_gauge_sits_where_
/// the_wordmark_did` above only ever calls `draw_footer` while charging, so nothing here was
/// exercising the discharging path through the call the app actually makes. This is that path,
/// at both charge states, at the same percent: everything but the bolt itself has to come back
/// identical.
#[test]
fn the_footer_does_not_move_the_gauge_when_the_charge_state_changes() {
    let mut idle = Vec::new();
    draw_footer(
        Some(Battery {
            percent: 68,
            charge: Charge::Discharging,
        }),
        Printed { face: None, w: 30 },
        None,
        Printed { face: None, w: 40 },
        &mut idle,
    );
    let mut charging = Vec::new();
    draw_footer(
        Some(Battery {
            percent: 68,
            charge: Charge::Charging,
        }),
        Printed { face: None, w: 30 },
        Some(TexId::from_raw(2)),
        Printed { face: None, w: 40 },
        &mut charging,
    );
    for d in &idle {
        assert!(
            charging.contains(d),
            "{d:?} moved or vanished when charging started"
        );
    }
}

/// The clock is the one thing on this band that did not change.
#[test]
fn the_clock_stays_at_the_right_margin() {
    let mut out = Vec::new();
    draw_footer(
        None,
        Printed::default(),
        None,
        Printed { face: None, w: 40 },
        &mut out,
    );
    let rightmost = out
        .iter()
        .map(|d| match *d {
            Draw::Rect { x, w, .. } | Draw::Tex { x, w, .. } => x + w,
            _ => 0.0,
        })
        .fold(0.0, f32::max);
    assert_eq!(rightmost, OUT_W as f32 - 24.0);
}

/// A device with no gauge shows a band with a clock on it, not a band with a hole in it.
#[test]
fn a_band_with_no_gauge_still_draws_its_clock() {
    let mut out = Vec::new();
    draw_footer(
        None,
        Printed::default(),
        None,
        Printed { face: None, w: 40 },
        &mut out,
    );
    assert_eq!(out.len(), 1);
}

/// The carts are what was refused. Nothing else on the screen was: the slot is part of the
/// device and the legend is printed on it, and a screen that shook wholesale would read as a
/// rendering fault rather than as a cart being rejected.
#[test]
fn a_refusal_moves_the_carts_and_leaves_the_device_where_it_is() {
    let s = shelf_with(3);
    let (mut still, mut shaken) = (Vec::new(), Vec::new());
    s.draw(0.0, &mut still);
    s.draw(9.0, &mut shaken);
    assert_eq!(still.len(), shaken.len(), "the shake changed the row");
    // The row draws first, so its quads are the leading ones. Sizes cannot tell the two
    // apart: the carts either side of the selection are drawn scaled down.
    let mut row = Vec::new();
    s.draw_row(None, 0.0, 0.0, 1.0, &mut row);
    let carts = row.len();
    assert!(
        carts > 0 && carts < still.len(),
        "{carts} of {}",
        still.len()
    );
    for (i, (a, b)) in still.iter().zip(&shaken).enumerate() {
        let ((ax, _), (bx, _)) = (xw(a), xw(b));
        if i < carts {
            assert!((bx - ax - 9.0).abs() < 0.01, "a cart stood still");
        } else {
            assert_eq!(ax, bx, "the device moved with the carts");
        }
    }
}

#[test]
fn carts_past_the_edges_of_the_row_are_not_drawn() {
    let mut s = shelf_with(30);
    for _ in 0..8 {
        s.right();
    }
    settle(&mut s);
    let n = placed(&s).len();
    assert!(n > 1, "only {n} carts drawn, the neighbours should peek in");
    assert!(n <= 5, "{n} carts drawn into a 720 px row");
}

/// All three carts have to be wholly on screen. At the old pitch the outer two were clipped
/// 24px off each edge, so the row read as two and a bit rather than three.
#[test]
fn all_three_carts_fit_on_screen() {
    let s = shelf_with(5);
    let mut out = Vec::new();
    s.draw(0.0, &mut out);
    let spans = cart_spans(&out);
    assert_eq!(
        spans.len(),
        3,
        "expected three carts on screen, got {}",
        spans.len()
    );
    for (x0, x1) in &spans {
        assert!(*x0 >= 0.0, "a cart starts at {x0}, off the left edge");
        assert!(
            *x1 <= OUT_W as f32,
            "a cart ends at {x1}, off the right edge"
        );
    }
}

/// The edge margin and the gap beside the centre cart should match, or the row looks
/// crowded on one axis and loose on the other.
#[test]
fn the_row_is_evenly_spaced() {
    let s = shelf_with(5);
    let mut out = Vec::new();
    s.draw(0.0, &mut out);
    let mut spans = cart_spans(&out);
    spans.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let margin = spans[0].0;
    let gap = spans[1].0 - spans[0].1;
    assert!(
        (margin - gap).abs() < 4.0,
        "edge margin {margin:.1} but gap {gap:.1}: the row is lopsided"
    );
}

/// Carts only. The legend shares the draw list and its plates are short, so height is what
/// separates them.
fn cart_spans(out: &[Draw]) -> Vec<(f32, f32)> {
    out.iter()
        .filter_map(|d| match *d {
            Draw::Rect { x, w, h, .. }
            | Draw::Tex { x, w, h, .. }
            | Draw::Turned { x, w, h, .. } => (h > 60.0).then_some((x, x + w)),
            Draw::Game | Draw::Shot { .. } => None,
        })
        .filter(|(x0, x1)| *x1 > 0.0 && *x0 < OUT_W as f32)
        .collect()
}

/// The row makes way for the cart going into the slot: the others part outwards and are gone
/// by the time it is seated. Fading them where they stand reads as the screen dimming rather
/// than as the shelf clearing.
#[test]
fn the_row_parts_for_the_cart_going_in() {
    let s = shelf_with(5);
    let at = |recede: f32| {
        let mut out = Vec::new();
        s.draw_row(Some("Game 0"), 0.0, recede, 1.0, &mut out);
        out
    };
    let start = at(0.0);
    let part = at(0.5);
    assert_eq!(start.len(), part.len(), "a cart left the row early");

    let centre = OUT_W as f32 / 2.0;
    for (a, b) in start.iter().zip(&part) {
        let (ax, aw) = xw(a);
        let (bx, _) = xw(b);
        let side = (ax + aw / 2.0) - centre;
        assert!(
            (bx - ax).signum() == side.signum(),
            "a cart at {ax} moved to {bx}, which is towards the slot, not away from it"
        );
        assert!((bx - ax).abs() > 1.0, "the cart at {ax} did not move");
    }
    assert!(
        at(1.0).is_empty(),
        "the row is still on screen with the cart seated"
    );
}

/// Dimming darkens a side cart's face and leaves the black under it as the recede has it, so a
/// dimmed cart reads as a cart in shadow rather than a ghost over the wallpaper.
#[test]
fn dim_darkens_a_side_carts_face_and_not_the_black_under_it() {
    let mut s = shelf_with(3);
    let shadow = TexId::from_raw(99);
    s.set_shadow(shadow);
    let side = TexId::from_raw(11);
    s.set_faces(vec![TexId::from_raw(10), side, TexId::from_raw(12)]);
    let drawn = |dim: f32| {
        let mut out = Vec::new();
        s.draw_row(Some("Game 0"), 0.0, 0.3, dim, &mut out);
        let (x, face) = out
            .iter()
            .find_map(|d| match *d {
                Draw::Tex { x, tex, alpha, .. } if tex == side => Some((x, alpha)),
                _ => None,
            })
            .expect("the side cart is not drawn");
        let under = out
            .iter()
            .find_map(|d| match *d {
                Draw::Tex {
                    x: at, tex, alpha, ..
                } if tex == shadow && at == x => Some(alpha),
                _ => None,
            })
            .expect("nothing is drawn under the side cart");
        (face, under)
    };
    let (face, under) = drawn(1.0);
    let (dimmed, dimmed_under) = drawn(0.5);
    assert!(
        (dimmed - face * 0.5).abs() < 1e-6,
        "the face went from {face} to {dimmed} at half dim"
    );
    assert_eq!(
        dimmed_under, under,
        "the black under the side cart changed with the dim"
    );
}

#[test]
fn complete_artwork_does_not_draw_the_shell_shadow() {
    let carts = (0..3)
        .map(|i| Cart {
            stem: format!("Game {i}"),
            rom: format!("Games/Game {i}.gba").into(),
            artwork: Some(format!("Cartridges/Game {i}.png").into()),
            label: None,
            code: String::new(),
            title: format!("GAME {i}"),
            platform: slot_store::Platform::Gba,
        })
        .collect();
    let mut s = Shelf::new(carts);
    let shadow = TexId::from_raw(99);
    s.set_shadow(shadow);
    for (i, raw) in (0..3).zip(10..13) {
        s.set_face_with_size_and_artwork(
            &format!("Game {i}"),
            TexId::from_raw(raw),
            (CART_W, CART_H),
            true,
        );
    }
    let mut out = Vec::new();
    s.draw_row(Some("Game 0"), 0.0, 0.3, 1.0, &mut out);
    assert!(
        !out.iter()
            .any(|d| matches!(d, Draw::Tex { tex, .. } if *tex == shadow)),
        "complete artwork drew the shell shadow: {out:?}"
    );
}

#[test]
fn a_favorite_cart_wears_the_mark_on_its_label() {
    use std::collections::BTreeSet;
    let mut s = shelf_with(1);
    s.set_faces(vec![TexId::from_raw(10)]);
    s.set_favorite_mark(TexId::from_raw(99), 16, 16);
    let favorites = BTreeSet::from(["Game 0".to_string()]);
    s.sort_by_favorites(&favorites);
    let mut out = Vec::new();
    s.draw(0.0, &mut out);
    assert!(
        out.iter().any(
            |d| matches!(d, Draw::Tex { tex, w, .. } if *tex == TexId::from_raw(99) && *w == 16.0)
        ),
        "the favorite mark was not drawn: {out:?}"
    );
}

#[test]
fn recents_navigation_does_not_wrap_and_clamps_at_edges() {
    let mut s = shelf_with(5);
    s.set_recents((0..5).map(|i| format!("Game {i}")).collect());
    s.next_category(); // Switch to category 1 (Recents)
    assert_eq!(s.category(), 1);
    assert_eq!(s.index, 0);

    // Left at the beginning should NOT wrap to index 4
    s.left();
    assert_eq!(s.index, 0, "left at the first cart should not wrap in recents");

    // Right to the end
    for _ in 0..10 {
        s.right();
    }
    assert_eq!(s.index, 4, "right at the end should clamp to the last cart in recents");

    // Off edge slots should be None
    assert_eq!(s.cart_at_offset(1), None, "no cart should wrap to the right of the last cart");
    assert_eq!(s.cart_at_offset(-1), Some(3));

    // Jump letter in recents goes to extremes without wrapping
    s.previous_letter();
    assert_eq!(s.index, 0, "previous letter should jump to the start of recents");
    assert_eq!(s.cart_at_offset(-1), None, "no cart should wrap to the left of the first cart");
    s.next_letter();
    assert_eq!(s.index, 4, "next letter should jump to the end of recents");
}

#[test]
fn recents_is_capped_at_ten_items() {
    let mut s = shelf_with(20);
    let many_recents: Vec<String> = (0..20).map(|i| format!("Game {i}")).collect();
    s.set_recents(many_recents);
    s.next_category(); // Switch to category 1 (Recents)
    assert_eq!(s.category(), 1);
    assert_eq!(s.carts.len(), 10, "recents shelf should contain at most 10 items");
}
