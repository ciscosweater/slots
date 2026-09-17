use slot_store::parse_stamp;
use slot_ui::{
    date_time_text, quick_caret_face, quick_label_face, quick_value_face, ClockPicker, QuickRow,
    QuickValue, UndoFace, MENU_PAD,
};

/// Whether a face's pixel at a column and row carries much ink.
fn inked(face: &UndoFace, x: u32, y: u32) -> bool {
    face.rgba[((y * face.w + x) * 4 + 3) as usize] > 128
}

/// The first and last columns with ink in them.
fn ink_columns(face: &UndoFace) -> (u32, u32) {
    let columns: Vec<u32> = (0..face.w)
        .filter(|&x| (0..face.h).any(|y| inked(face, x, y)))
        .collect();
    (
        *columns.first().expect("an empty face"),
        *columns.last().expect("an empty face"),
    )
}

/// The menu places every face by its padding, so the type has to sit exactly `MENU_PAD` in from
/// both sides of its face, tracking and all.
#[test]
fn the_type_sits_exactly_menu_pad_in_from_both_sides_of_its_face() {
    for face in [
        quick_label_face(QuickRow::FastForwardSound),
        quick_label_face(QuickRow::Rumble),
        quick_value_face("Off", false),
        quick_value_face("SEP 15 16:35", true),
    ] {
        let (first, last) = ink_columns(&face);
        let right = face.w - 1 - MENU_PAD;
        assert!((MENU_PAD..MENU_PAD + 4).contains(&first));
        assert!((right - 4..=right).contains(&last));
    }
}

/// Set at the menu's size however long it is. A long label must not be shrunk to fit a face
/// whose width was measured without tracking.
#[test]
fn a_long_label_is_set_as_large_as_a_short_one() {
    let ink_height = |face: &UndoFace| {
        (0..face.h)
            .filter(|&y| (0..face.w).any(|x| inked(face, x, y)))
            .count()
    };
    let long = ink_height(&quick_label_face(QuickRow::FastForwardSound));
    let short = ink_height(&quick_label_face(QuickRow::Rumble));
    assert!(long + 1 >= short, "{long} rows of ink against {short}");
}

#[test]
fn the_rows_and_values_match_the_menu() {
    assert_eq!(
        QuickRow::ALL.map(QuickRow::label),
        [
            "Fast Forward",
            "Fast Forward Sound",
            "Colour Correction",
            "Rumble",
            "X / Y Buttons",
            "GB Overlay",
            "LCD Effect",
            "Reset Display",
            "Date & Time",
            "About"
        ]
    );
    assert_eq!(
        QuickValue::ALL.map(QuickValue::text),
        [
            "2×",
            "3×",
            "4×",
            "6×",
            "On",
            "Off",
            "Shortcuts",
            "L / R",
            "A / B Turbo",
            "Fill Screen",
            "Actual Size"
        ]
    );
    assert_eq!(
        [2, 3, 4, 6].map(QuickValue::speed),
        [
            Some(QuickValue::Speed2),
            Some(QuickValue::Speed3),
            Some(QuickValue::Speed4),
            Some(QuickValue::Speed6)
        ]
    );
    assert_eq!(QuickValue::speed(5), None);
    assert_eq!(QuickValue::flag(true), QuickValue::On);
    assert_eq!(QuickValue::flag(false), QuickValue::Off);
    assert_eq!(
        QuickRow::ALL
            .into_iter()
            .filter(|row| row.opens())
            .collect::<Vec<_>>(),
        vec![QuickRow::ResetDisplay, QuickRow::DateTime, QuickRow::About]
    );
}

#[test]
fn the_date_and_time_uses_the_carousels_24_hour_clock() {
    let at = |stamp: &str| date_time_text(parse_stamp(stamp).expect("a stamp"));
    assert_eq!(at("2026-09-15_16-35-00"), "SEP 15 16:35");
    assert_eq!(at("2027-01-05_04-07-59"), "JAN 5 04:07");
}

#[test]
fn a_set_clock_picker_starts_at_the_local_time_and_its_offset() {
    let utc = parse_stamp("2026-09-15_21-35-42").expect("a stamp");
    let picker = ClockPicker::local(utc, -300);
    assert_eq!(picker.offset_min(), -300);
    assert_eq!(picker.secs(), utc - 42);
    assert!(picker.text().starts_with("2026-09-15 16:35"));
}

#[test]
fn a_value_is_grey_until_its_row_is_in_hand() {
    let brightest = |lit| {
        quick_value_face("4×", lit)
            .rgba
            .chunks(4)
            .max_by_key(|pixel| pixel[3])
            .map(|pixel| [pixel[0], pixel[1], pixel[2]])
            .expect("an empty face")
    };
    assert_eq!(brightest(true), [0xf6, 0xf4, 0xef]);
    assert_eq!(brightest(false), [0x9a, 0x9a, 0xa4]);
}

#[test]
fn the_arrows_are_faces_the_height_of_a_value() {
    for right in [false, true] {
        let face = quick_caret_face(right);
        assert!(face.w > 0 && face.rgba.chunks(4).any(|pixel| pixel[3] > 0));
        assert_eq!(face.h, quick_value_face("On", true).h);
    }
}
