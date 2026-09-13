mod common;

use std::collections::BTreeSet;

use common::tmp_root;
use slot_store::{read_favorites, write_favorites, FAVORITES_FILE};

#[test]
fn favorites_round_trip_in_sorted_order() {
    let d = tmp_root();
    let favorites = BTreeSet::from(["Zelda".to_string(), "Advance Wars".to_string()]);
    write_favorites(d.path(), &favorites).unwrap();
    assert_eq!(read_favorites(d.path()), favorites);
    assert_eq!(
        std::fs::read_to_string(d.path().join("System").join(FAVORITES_FILE)).unwrap(),
        "Advance Wars\nZelda\n"
    );
}
