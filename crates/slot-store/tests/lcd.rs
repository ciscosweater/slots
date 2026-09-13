mod common;

use common::tmp_root;
use slot_store::{read_lcd, write_lcd, LCD_FILE};

#[test]
fn lcd_defaults_on_and_round_trips() {
    let d = tmp_root();
    assert!(read_lcd(d.path()));

    write_lcd(d.path(), false).unwrap();
    assert!(!read_lcd(d.path()));
    assert_eq!(
        std::fs::read_to_string(d.path().join("System").join(LCD_FILE)).unwrap(),
        "off\n"
    );

    write_lcd(d.path(), true).unwrap();
    assert!(read_lcd(d.path()));
}
