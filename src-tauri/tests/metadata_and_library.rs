use idfkplay_lib::library::{self, ApiStatus, MetaState, RawLibrary};
use idfkplay_lib::source::Game;
use idfkplay_lib::steam::locate::PathSource;
use idfkplay_lib::metadata::to_meta;
use idfkplay_lib::steam::store::{AppMeta, MetaStatus};
use idfkplay_lib::steam::storebrowse::{self, parse_items, parse_tag_list};
use idfkplay_lib::steam::webapi::{parse_owned_games, WebApiError};
use idfkplay_lib::steam::SteamInfo;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

fn fixture(name: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join(name)).unwrap()
}

fn items() -> Vec<storebrowse::Item> {
    parse_items(&fixture("storebrowse_items.json")).unwrap()
}

#[test]
fn batch_items_types_and_not_found() {
    let items = items();
    assert_eq!(items.len(), 6);
    let by_id: HashMap<u32, &storebrowse::Item> = items.iter().map(|i| (i.app_id, i)).collect();
    assert_eq!(by_id[&620].app_type, Some("game"));
    assert_eq!(by_id[&323180].app_type, Some("music"), "Portal 2 Soundtrack");
    // Not-found entries report appid 0; the requested id is kept from `id`.
    assert!(!by_id[&228980].found);
}

#[test]
fn batch_categories_map_to_store_names() {
    let items = items();
    let get = |id| items.iter().find(|i| i.app_id == id).unwrap();
    let portal = get(620);
    assert!(portal.categories.contains(&"Online Co-op".to_string()));
    assert!(portal.categories.contains(&"Full controller support".to_string()));
    // Categories the picker doesn't use (achievements, cloud…) are dropped.
    assert!(!portal.categories.iter().any(|c| c.contains("Achievements")));
    assert!(get(667970).categories.contains(&"PvP".to_string()), "VTOL VR");
    let mut hades = get(1145360).categories.clone();
    hades.sort();
    assert_eq!(hades, ["Full controller support", "Single-player"]);
}

#[test]
fn batch_tags_are_ordered_by_votes() {
    let portal = items().into_iter().find(|i| i.app_id == 620).unwrap();
    assert_eq!(portal.tag_ids.len(), 20);
    assert_eq!(portal.tag_ids[0], 4182, "highest weight first (Singleplayer)");
}

#[test]
fn tag_list_names_and_unknown_tags() {
    let names = parse_tag_list(&fixture("tag_list.json")).unwrap();
    let portal = items().into_iter().find(|i| i.app_id == 620).unwrap();
    let (tags, unknown) = storebrowse::tag_names(&portal.tag_ids, &names);
    assert_eq!(&tags[..3], ["Singleplayer", "Platformer", "Puzzle"]);
    // The fixture leaves out tag 19 ("Action") to cover tags Steam added after we cached the list.
    assert_eq!(unknown, 1);
    assert!(parse_tag_list("{}").is_err());
}

#[test]
fn to_meta_marks_incomplete_tags_for_retry() {
    let names = parse_tag_list(&fixture("tag_list.json")).unwrap();
    let items = items();
    let portal = to_meta(items.iter().find(|i| i.app_id == 620).unwrap(), &names, 5);
    assert_eq!(portal.status, MetaStatus::Known);
    assert!(portal.is_game());
    assert!(!portal.tags_fetched, "one tag couldn't be named yet");
    let hades = to_meta(items.iter().find(|i| i.app_id == 1145360).unwrap(), &names, 5);
    assert!(!hades.tags.is_empty());
    assert!(hades.categories.contains(&"Full controller support".to_string()));
    let soundtrack = to_meta(items.iter().find(|i| i.app_id == 323180).unwrap(), &names, 5);
    assert!(!soundtrack.is_game());
    let missing = to_meta(items.iter().find(|i| i.app_id == 228980).unwrap(), &names, 5);
    assert_eq!(missing.status, MetaStatus::Unknown);
    assert!(missing.tags_fetched, "nothing more to fetch");
}

#[test]
fn batch_parse_rejects_garbage() {
    assert!(parse_items("<html>").is_err());
    assert!(parse_items("{}").is_err());
    assert!(parse_items(r#"{"response":{"store_items":[]}}"#).unwrap().is_empty());
}

#[test]
fn items_url_encodes_ids_and_context() {
    let url = storebrowse::items_url(&[620, 400]).unwrap();
    let input = url.query_pairs().find(|(k, _)| k == "input_json").unwrap().1.to_string();
    let v: serde_json::Value = serde_json::from_str(&input).unwrap();
    assert_eq!(v["ids"][1]["appid"], 400);
    assert_eq!(v["context"]["language"], "english");
}

#[test]
fn owned_games_parse_and_private_profile() {
    let games = parse_owned_games(&fixture("owned_games.json")).unwrap();
    assert_eq!(games.len(), 3);
    assert_eq!(games[0].playtime_forever, 800);
    assert_eq!(games[2].rtime_last_played, 0, "missing field defaults");
    assert_eq!(parse_owned_games(r#"{"response":{}}"#).unwrap_err(), WebApiError::NoGames);
    assert!(WebApiError::NoGames.to_string().contains("Public"));
}

#[test]
fn excludes_tools_by_id_and_whole_word_name() {
    assert!(library::excluded_by_name(228980, "Steamworks Common Redistributables").is_some());
    assert!(library::excluded_by_name(1, "Proton 9.0").is_some());
    assert!(library::excluded_by_name(1, "Steam Linux Runtime 3.0 (sniper)").is_some());
    assert!(library::excluded_by_name(1, "Valheim Dedicated Server").is_some());
    assert!(library::excluded_by_name(1, "Source SDK Base 2013").is_some());
    assert!(library::excluded_by_name(1, "Hades Soundtrack").is_some());
    assert!(library::excluded_by_name(1, "Portal 2").is_none());
    // Whole words only: these merely contain the letters.
    assert!(library::excluded_by_name(1, "Protonwars").is_none());
    assert!(library::excluded_by_name(1, "SDKiller").is_none());
}

fn local_game(app_id: u32, name: &str, installed: bool, playtime: u32) -> Game {
    Game {
        source: "steam",
        app_id,
        name: name.into(),
        installed,
        install_path: installed.then(|| format!(r"C:\Steam\steamapps\common\{name}")),
        size_on_disk: installed.then_some(1_000_000),
        playtime_minutes: playtime,
        last_played: None,
    }
}

fn raw(local: Vec<Game>, owned: Option<&str>) -> RawLibrary {
    let owned = owned.map(|j| parse_owned_games(j).unwrap());
    RawLibrary {
        info: SteamInfo {
            steam_path: r"C:\Steam".into(),
            steam_path_source: PathSource::Default,
            libraries: vec![],
            accounts: vec![],
            selected_account: None,
        },
        api: if owned.is_some() { ApiStatus::Ok { count: 3 } } else { ApiStatus::NoKey },
        local,
        owned,
        collections: Default::default(),
        warnings: vec![],
    }
}

fn meta(app_type: &str) -> AppMeta {
    AppMeta {
        status: MetaStatus::Known,
        app_type: Some(app_type.into()),
        genres: vec!["Action".into()],
        categories: vec![],
        tags: vec!["Puzzle".into()],
        tags_fetched: true,
        fetched_at: 1,
        assets: None,
    }
}

#[test]
fn merge_uses_owned_list_and_local_install_state() {
    let local = vec![
        local_game(620, "Portal 2", true, 754),
        local_game(228980, "Steamworks Common Redistributables", true, 0),
        local_game(999, "Family Shared Game", true, 30),
    ];
    let owned = fixture("owned_games.json");
    let merged = library::merge(&local, raw(local.clone(), Some(&owned)).owned.as_deref());
    let by_id: HashMap<u32, &Game> = merged.iter().map(|g| (g.app_id, g)).collect();

    assert_eq!(merged.len(), 5, "3 owned + 2 local-only");
    assert!(by_id[&620].installed);
    assert_eq!(by_id[&620].playtime_minutes, 800, "API playtime wins when higher");
    assert_eq!(by_id[&620].last_played, Some(1_729_900_000));
    assert!(by_id[&620].install_path.is_some());
    assert!(!by_id[&400].installed, "owned but not installed");
    assert!(by_id[&999].installed, "local-only games are kept");
}

#[test]
fn view_hides_non_games_and_tracks_meta_state() {
    let local = vec![
        local_game(620, "Portal 2", true, 0),
        local_game(228980, "Steamworks Common Redistributables", true, 0),
        local_game(323180, "Portal 2 - Peer Review", true, 0),
        local_game(777, "Delisted Thing", true, 0),
        local_game(888, "Not Fetched Yet", true, 0),
    ];
    let mut cache = HashMap::new();
    cache.insert(620, meta("game"));
    cache.insert(323180, meta("dlc"));
    cache.insert(
        777,
        AppMeta { status: MetaStatus::Unknown, app_type: None, genres: vec![], categories: vec![], tags: vec![], tags_fetched: true, fetched_at: 1, assets: None },
    );

    let view = library::build_view(&raw(local, None), &cache);
    let state: HashMap<u32, MetaState> = view.games.iter().map(|g| (g.game.app_id, g.meta)).collect();
    assert_eq!(state.len(), 3);
    assert_eq!(state[&620], MetaState::Known);
    assert_eq!(state[&777], MetaState::Unknown, "delisted apps are kept with unknown tags");
    assert_eq!(state[&888], MetaState::Pending);

    let hidden: Vec<u32> = view.hidden.iter().map(|h| h.app_id).collect();
    assert_eq!(hidden.len(), 2);
    assert!(hidden.contains(&228980) && hidden.contains(&323180));
}

#[test]
fn ids_needing_meta_skips_cached_and_excluded_and_puts_new_games_first() {
    let local = vec![
        // "A…" sorts first by name, but it's a retry, so it goes after the new games.
        local_game(889, "A Game With Incomplete Tags", true, 0),
        local_game(620, "Portal 2", true, 0),
        local_game(228980, "Steamworks Common Redistributables", true, 0),
        local_game(888, "Brand New Game", true, 0),
        local_game(887, "Zebra New Game", true, 0),
    ];
    let mut cache = HashMap::new();
    cache.insert(620, meta("game"));
    cache.insert(889, AppMeta { tags_fetched: false, ..meta("game") });
    let ids = library::ids_needing_meta(&raw(local, None), &cache);
    assert_eq!(ids, [888, 887, 889]);
}

#[test]
fn batch_assets_become_full_urls() {
    let json = r#"{"response":{"store_items":[{"appid":2483190,"id":2483190,"success":1,"type":0,
        "assets":{"asset_url_format":"steam/apps/2483190/${FILENAME}?t=17",
                  "library_capsule":"711e/library_capsule.jpg",
                  "library_capsule_2x":"711e/library_capsule_2x.jpg",
                  "library_hero":"3eeb/library_hero.jpg",
                  "header":"bd1e/header_alt_assets_5.jpg"}}]}}"#;
    let item = &parse_items(json).unwrap()[0];
    let a = item.assets.as_ref().unwrap();
    let base = "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/2483190/";
    assert_eq!(a.portrait.as_deref(), Some(format!("{base}711e/library_capsule_2x.jpg?t=17").as_str()), "2x preferred");
    assert_eq!(a.hero.as_deref(), Some(format!("{base}3eeb/library_hero.jpg?t=17").as_str()));
    assert_eq!(a.header.as_deref(), Some(format!("{base}bd1e/header_alt_assets_5.jpg?t=17").as_str()));
    // Carried into the cache.
    assert_eq!(to_meta(item, &HashMap::new(), 1).assets.as_ref(), Some(a));
}
