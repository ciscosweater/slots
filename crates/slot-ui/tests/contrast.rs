use slot_gfx::BACKDROP;
use slot_ui::{
    edge, favorite_mark_face, housing, opening, shell_for, title_face, DEFAULT_SHELL, EMPTY_SHELF,
    FAVORITE_INK,
};

fn distance(a: [u8; 3], b: [f32; 4]) -> u32 {
    (0..3)
        .map(|i| (a[i] as i32 - (b[i] * 255.0).round() as i32).unsigned_abs())
        .sum()
}

/// The first backdrop was #33353d and the default shell #35353a: a total channel difference
/// of 6 out of 765, so every ordinary cart on the shelf was invisible. Nothing in either
/// crate's own tests could catch that, because neither one is wrong on its own.
#[test]
fn every_shell_is_visible_against_the_backdrop() {
    let codes = ["", "AMTE", "AXVE", "AXPE", "BPEE", "BPRE", "BPGE", "MSKE"];
    for code in codes {
        let s = shell_for(code);
        let d = distance(s.colour, BACKDROP);
        assert!(
            d > 60,
            "{code} shell {:?} is only {d}/765 from the backdrop, it will not be seen",
            s.colour
        );
    }
    assert!(distance(DEFAULT_SHELL.colour, BACKDROP) > 60);
}

/// The new faces on the shelf have to clear the same bar the shells do, or a favorite star
/// and the empty-case type vanish into the photograph behind them.
#[test]
fn the_favorite_mark_and_empty_caption_clear_the_backdrop() {
    assert!(
        distance(FAVORITE_INK, BACKDROP) > 60,
        "the favorite mark is only {}/765 from the backdrop",
        distance(FAVORITE_INK, BACKDROP)
    );
    let star = favorite_mark_face();
    let lit = star.rgba.chunks(4).filter(|p| p[3] > 200).count();
    assert!(lit > 20, "the favorite mark has no opaque ink: {lit}");
    let caption = title_face(EMPTY_SHELF);
    let ink = caption
        .rgba
        .chunks(4)
        .find(|p| p[3] > 200)
        .expect("the empty shelf caption is blank");
    let colour = [ink[0], ink[1], ink[2]];
    assert!(
        distance(colour, BACKDROP) > 60,
        "empty shelf type {:?} is only {}/765 from the backdrop",
        colour,
        distance(colour, BACKDROP)
    );
}

/// The same failure one layer along: a near black mouth on a pure black backdrop is not an
/// opening, it is nothing. A slot only reads as a hole because of what it is cut into, so
/// each band has to clear the one behind it as well as the backdrop.
#[test]
fn every_chrome_band_is_visible_against_its_neighbour() {
    let d = |a: [f32; 4], b: [f32; 4]| -> f32 {
        (0..3).map(|i| (a[i] - b[i]).abs()).sum::<f32>() * 255.0
    };
    assert!(
        d(housing(), BACKDROP) > 60.0,
        "the housing vanishes into the backdrop"
    );
    assert!(
        d(opening(), housing()) > 40.0,
        "the opening vanishes into the housing"
    );
    assert!(
        d(edge(), opening()) > 60.0,
        "the lip vanishes into the opening"
    );
}
