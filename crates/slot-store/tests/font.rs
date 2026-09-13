mod common;

use common::tmp_root;
use slot_store::{read_pixelify, write_pixelify, FONT_FILE};

#[test]
fn font_choice_defaults_to_pixelify_and_round_trips() {
    let d = tmp_root();
    assert!(read_pixelify(d.path()));
    write_pixelify(d.path(), false).unwrap();
    assert!(!read_pixelify(d.path()));
    assert_eq!(
        std::fs::read_to_string(d.path().join("System").join(FONT_FILE)).unwrap(),
        "original\n"
    );
    write_pixelify(d.path(), true).unwrap();
    assert!(read_pixelify(d.path()));
}
