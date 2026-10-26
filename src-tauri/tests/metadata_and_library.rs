use idfkplay_lib::library::{self, ApiStatus, MetaState, RawLibrary};
use idfkplay_lib::source::Game;
use idfkplay_lib::steam::locate::PathSource;
use idfkplay_lib::steam::store::{parse_appdetails, parse_steamspy_tags, AppMeta, MetaStatus, StoreResult};
use idfkplay_lib::steam::webapi::{parse_owned_games, WebApiError};
use idfkplay_lib::steam::SteamInfo;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

fn fixture(name: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join(name)).unwrap()
}

#[test]
fn appdetails_game() {
    let r = parse_appdetails(620, &fixture("appdetails_620.json")).unwrap();
    let StoreResult::Found { app_type, genres, categories } = r else { panic!("expected Found, got {r:?}") };
    assert_eq!(app_type.as_deref(), Some("game"));
    assert_eq!(genres, ["Action", "Adventure"]);
    assert!(categories.contains(&"Online Co-op".to_string()));
    assert!(categories.contains(&"Full controller support".to_string()));
    assert_eq!(categories.iter().filter(|c| *c == "Steam Workshop").count(), 1, "duplicates removed");
}

#[test]
fn appdetails_dlc_type_lowercased() {
    let r = parse_appdetails(323180, &fixture("appdetails_dlc.json")).unwrap();
    assert!(matches!(r, StoreResult::Found { app_type: Some(t), genres, .. } if t == "dlc" && genres.is_empty()));
}

#[test]
fn appdetails_failure_and_throttle() {
    assert_eq!(parse_appdetails(42, r#"{"42":{"success":false}}"#).unwrap(), StoreResult::NotFound);
    assert_eq!(parse_appdetails(42, r#"{}"#).unwrap(), StoreResult::NotFound);
    assert_eq!(parse_appdetails(42, "null").unwrap(), StoreResult::Throttled);
    assert!(parse_appdetails(42, "<html>").is_err());
}

#[test]
fn steamspy_tags_sorted_by_votes() {
    let tags = parse_steamspy_tags(&fixture("steamspy_620.json")).unwrap();
    assert_eq!(tags, ["Puzzle", "Co-op", "First-Person", "Sci-fi"]);
    // SteamSpy sends `[]` instead of `{}` when an app has no tags.
    assert!(parse_steamspy_tags(r#"{"appid":1,"tags":[]}"#).unwrap().is_empty());
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
        AppMeta { status: MetaStatus::Unknown, app_type: None, genres: vec![], categories: vec![], tags: vec![], tags_fetched: true, fetched_at: 1 },
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
fn ids_needing_meta_skips_cached_and_excluded() {
    let local = vec![
        local_game(620, "Portal 2", true, 0),
        local_game(228980, "Steamworks Common Redistributables", true, 0),
        local_game(888, "Not Fetched Yet", true, 0),
        local_game(889, "Tags Failed", true, 0),
    ];
    let mut cache = HashMap::new();
    cache.insert(620, meta("game"));
    cache.insert(889, AppMeta { tags_fetched: false, ..meta("game") });
    let mut ids = library::ids_needing_meta(&raw(local, None), &cache);
    ids.sort();
    assert_eq!(ids, [888, 889]);
}
