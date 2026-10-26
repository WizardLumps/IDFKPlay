//! Dev diagnostic: runs the picker offline against the local scan plus the saved
//! owned-games and metadata caches, and prints eligibility counts per filter.
//! Run with `cargo run --example pick` from src-tauri.

use idfkplay_lib::library::{self, ApiStatus, RawLibrary};
use idfkplay_lib::picker::{self, Mode, Mood, PickRequest, PickRules};
use idfkplay_lib::settings;
use idfkplay_lib::steam::store::AppMeta;
use idfkplay_lib::steam::webapi::OwnedGame;
use idfkplay_lib::steam::SteamSource;
use std::collections::HashMap;

fn read_json(path: &std::path::Path) -> Option<serde_json::Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn main() {
    let dir = settings::config_dir(std::env::temp_dir());
    let settings = settings::load(&dir);
    let scan = match SteamSource::new(settings.clone()).scan_detailed() {
        Ok(s) => s,
        Err(e) => return println!("error: {e}"),
    };
    let owned: Option<Vec<OwnedGame>> = read_json(&dir.join("owned-games.json"))
        .and_then(|v| serde_json::from_value(v.get("games")?.clone()).ok());
    let meta: HashMap<u32, AppMeta> = read_json(&dir.join("metadata-cache.json"))
        .and_then(|v| serde_json::from_value(v.get("apps")?.clone()).ok())
        .unwrap_or_default();
    let api = match &owned {
        Some(o) => ApiStatus::Ok { count: o.len() },
        None => ApiStatus::NoKey,
    };
    let raw = RawLibrary {
        info: scan.info,
        local: scan.games,
        owned,
        collections: scan.collections,
        api,
        warnings: scan.warnings,
    };
    let view = library::build_view(&raw, &meta);
    let known = view.games.iter().filter(|g| g.meta == library::MetaState::Known).count();
    println!("{} games ({} hidden), metadata known for {known}", view.games.len(), view.hidden.len());
    println!("Hidden in Steam: {}", raw.collections.hidden.len());
    for c in &view.collections {
        let tag = if c.dynamic { " (dynamic: stored list)" } else { "" };
        println!("Collection {:<20} {:>3} of {:>3} games in picker{tag}", c.name, c.count, raw.collections.list.iter().find(|x| x.id == c.id).map_or(0, |x| x.app_ids.len()));
    }

    let hidden: &[u32] = if settings.skip_steam_hidden { &raw.collections.hidden } else { &[] };
    let rules = PickRules {
        something_new_minutes: (settings.something_new_hours * 60.0).round() as u32,
        blacklist: &settings.blacklist,
        collection_members: None,
        hidden,
    };
    let cases: &[(&str, Mode, &[Mood], bool)] = &[
        ("Just Play", Mode::JustPlay, &[], false),
        ("Just Play, installed", Mode::JustPlay, &[], true),
        ("Something New", Mode::SomethingNew, &[], false),
        ("Something New, installed", Mode::SomethingNew, &[], true),
        ("Action", Mode::JustPlay, &[Mood::Action], false),
        ("Adventure", Mode::JustPlay, &[Mood::Adventure], false),
        ("Puzzle", Mode::JustPlay, &[Mood::Puzzle], false),
        ("RPG", Mode::JustPlay, &[Mood::Rpg], false),
        ("Strategy", Mode::JustPlay, &[Mood::Strategy], false),
        ("Multiplayer", Mode::JustPlay, &[Mood::Multiplayer], false),
        ("Controller", Mode::JustPlay, &[Mood::Controller], false),
        ("VR", Mode::JustPlay, &[Mood::Vr], false),
        ("VR, installed", Mode::JustPlay, &[Mood::Vr], true),
        ("RPG + Controller", Mode::JustPlay, &[Mood::Rpg, Mood::Controller], false),
        ("Puzzle + Multiplayer, installed", Mode::JustPlay, &[Mood::Puzzle, Mood::Multiplayer], true),
    ];
    for (label, mode, moods, installed_only) in cases {
        let req = PickRequest {
            mode: *mode,
            moods: moods.to_vec(),
            installed_only: *installed_only,
            exclude: vec![],
            collections: vec![],
        };
        let pool = picker::eligible(&view.games, &req, &rules);
        let sample = picker::choose(&pool, &[], picker::random_u64()).map_or("-".into(), |g| g.game.name.clone());
        println!("{label:<34} {:>4} eligible   e.g. {sample}", pool.len());
    }

    // Each collection on its own.
    for c in &view.collections {
        let members = library::collection_members(&raw.collections, std::slice::from_ref(&c.id));
        let rules = PickRules { collection_members: members.as_ref(), ..rules.clone() };
        let req = PickRequest {
            mode: Mode::JustPlay,
            moods: vec![],
            installed_only: false,
            exclude: vec![],
            collections: vec![c.id.clone()],
        };
        let pool = picker::eligible(&view.games, &req, &rules);
        let names: Vec<&str> = pool.iter().take(5).map(|g| g.game.name.as_str()).collect();
        println!("In {:<20} {:>4} eligible   e.g. {}", c.name, pool.len(), names.join(", "));
    }
}
