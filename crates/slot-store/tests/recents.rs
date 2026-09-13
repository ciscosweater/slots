mod common;

use common::tmp_root;
use slot_store::{read_recents, touch_recent, write_recents, RECENTS_FILE, RECENTS_MAX};

#[test]
fn missing_history_is_empty() {
    let d = tmp_root();
    assert!(read_recents(d.path()).is_empty());
}

#[test]
fn recents_round_trip_newest_first() {
    let d = tmp_root();
    let recents = vec!["Zelda".to_string(), "Advance Wars".to_string()];
    write_recents(d.path(), &recents).unwrap();
    assert_eq!(read_recents(d.path()), recents);
    assert_eq!(
        std::fs::read_to_string(d.path().join("System").join(RECENTS_FILE)).unwrap(),
        "Zelda\nAdvance Wars\n"
    );
}

#[test]
fn touching_promotes_without_duplicates_and_caps_the_history() {
    let mut recents = (0..RECENTS_MAX)
        .map(|i| format!("Game {i}"))
        .collect::<Vec<_>>();
    touch_recent(&mut recents, "Game 4");
    assert_eq!(recents[0], "Game 4");
    assert_eq!(recents.iter().filter(|stem| *stem == "Game 4").count(), 1);

    touch_recent(&mut recents, "New game");
    assert_eq!(recents[0], "New game");
    assert_eq!(recents.len(), RECENTS_MAX);
    assert!(!recents.contains(&"Game 9".to_string()));
}

#[test]
fn reading_repairs_duplicates_and_overlong_files_in_memory() {
    let d = tmp_root();
    let text = (0..RECENTS_MAX + 2)
        .map(|i| {
            if i == 2 {
                "Game 0".to_string()
            } else {
                format!("Game {i}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(d.path().join("System").join(RECENTS_FILE), text).unwrap();
    let recents = read_recents(d.path());
    assert_eq!(recents.len(), RECENTS_MAX);
    assert_eq!(recents.iter().filter(|stem| *stem == "Game 0").count(), 1);
}
