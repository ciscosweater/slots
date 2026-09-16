use slot_ui::{centred_hints, TexId, HINT_EDGE, LEGEND_GAP, OUT_W};

#[test]
fn a_row_of_hints_is_centred_by_what_shows_of_each() {
    let (a, b) = (TexId::from_raw(1), TexId::from_raw(2));
    let (aw, bw) = (71 + HINT_EDGE, 110 + HINT_EDGE);
    let placed = centred_hints(&[(a, aw), (b, bw)], LEGEND_GAP);
    let x = ((OUT_W as f32 - (71.0 + LEGEND_GAP + 110.0)) / 2.0).round();
    assert_eq!(
        placed,
        vec![(a, aw, x), (b, bw, (x + 71.0 + LEGEND_GAP).round())]
    );
}

#[test]
fn a_lone_hint_is_centred_by_what_shows_of_it() {
    let face = TexId::from_raw(1);
    let width = 150 + HINT_EDGE;
    assert_eq!(
        centred_hints(&[(face, width)], LEGEND_GAP),
        vec![(face, width, ((OUT_W as f32 - 150.0) / 2.0).round())]
    );
}
