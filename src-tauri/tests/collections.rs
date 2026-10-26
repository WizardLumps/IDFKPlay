use idfkplay_lib::steam::collections::{parse_collections, FAVORITES_ID};
use std::fs;
use std::path::Path;

fn fixture(name: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join(name)).unwrap()
}

#[test]
fn parses_favorites_user_and_dynamic_collections() {
    let c = parse_collections(&fixture("cloud-storage-namespace-1.json")).unwrap();
    let names: Vec<&str> = c.list.iter().map(|c| c.name.as_str()).collect();
    // Favorites first, then by name; Hidden, deleted and nameless collections are left out.
    assert_eq!(names, ["Favorites", "Driving", "Flight"]);

    assert_eq!(c.list[0].id, FAVORITES_ID);
    assert_eq!(c.list[0].app_ids, [620, 1145360], "deduplicated");

    let driving = &c.list[1];
    assert_eq!(driving.app_ids, [284160, 320140], "removed apps are excluded");
    assert!(!driving.dynamic);

    let flight = &c.list[2];
    assert!(flight.dynamic);
    assert_eq!(flight.app_ids, [223750, 502500, 667970]);
}

#[test]
fn hidden_collection_is_separate() {
    let c = parse_collections(&fixture("cloud-storage-namespace-1.json")).unwrap();
    assert_eq!(c.hidden, [570, 228980]);
}

#[test]
fn bad_input_is_an_error_not_a_panic() {
    assert!(parse_collections("not json").is_err());
    assert!(parse_collections("{}").is_err());
    assert_eq!(parse_collections("[]").unwrap().list.len(), 0);
}
