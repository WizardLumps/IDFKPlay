use idfkplay_lib::library::{LibraryGame, MetaState};
use idfkplay_lib::picker::{self, Mode, Mood, PickDecision, PickKind, PickRequest, PickRules, Session};
use idfkplay_lib::source::Game;

struct G {
    id: u32,
    installed: bool,
    minutes: u32,
    meta: MetaState,
    genres: &'static [&'static str],
    categories: &'static [&'static str],
    tags: &'static [&'static str],
}

impl Default for G {
    fn default() -> Self {
        G { id: 0, installed: true, minutes: 0, meta: MetaState::Known, genres: &[], categories: &[], tags: &[] }
    }
}

fn game(g: G) -> LibraryGame {
    let strings = |v: &[&str]| v.iter().map(|s| s.to_string()).collect();
    LibraryGame {
        game: Game {
            source: "steam",
            app_id: g.id,
            name: format!("Game {}", g.id),
            installed: g.installed,
            install_path: None,
            size_on_disk: None,
            playtime_minutes: g.minutes,
            playtime_unknown: false,
            last_played: None,
        },
        meta: g.meta,
        genres: strings(g.genres),
        categories: strings(g.categories),
        tags: strings(g.tags),
        favorite: false,
        edited: false,
    }
}

fn req(mode: Mode, moods: &[Mood], installed_only: bool) -> PickRequest {
    PickRequest { mode, moods: moods.to_vec(), installed_only, exclude: vec![], collections: vec![] }
}

const RULES: PickRules =
    PickRules { something_new_minutes: 120, blacklist: &[], collection_members: None, hidden: &[] };

fn ids(games: &[LibraryGame], r: &PickRequest, rules: &PickRules) -> Vec<u32> {
    picker::eligible(games, r, rules).iter().map(|g| g.game.app_id).collect()
}

fn library() -> Vec<LibraryGame> {
    vec![
        // Co-op action game with full controller support.
        game(G { id: 1, minutes: 900, genres: &["Action"], categories: &["Online Co-op", "Full controller support"], ..G::default() }),
        // Single-player puzzle (tag only), barely played.
        game(G { id: 2, minutes: 30, genres: &["Indie"], categories: &["Single-player"], tags: &["Puzzle"], ..G::default() }),
        // Uninstalled RPG with partial controller support, never played.
        game(G { id: 3, installed: false, genres: &["RPG"], categories: &["Partial Controller Support"], ..G::default() }),
        // Strategy via tag, multiplayer PvP, exactly at the threshold.
        game(G { id: 4, minutes: 120, genres: &["Simulation"], categories: &["Online PvP"], tags: &["Turn-Based Strategy"], ..G::default() }),
        // No metadata yet.
        game(G { id: 5, meta: MetaState::Pending, ..G::default() }),
        // Delisted: tags unknown.
        game(G { id: 6, meta: MetaState::Unknown, ..G::default() }),
    ]
}

#[test]
fn no_filters_includes_everything_even_unknown_meta() {
    assert_eq!(ids(&library(), &req(Mode::JustPlay, &[], false), &RULES), [1, 2, 3, 4, 5, 6]);
}

#[test]
fn installed_toggle() {
    assert_eq!(ids(&library(), &req(Mode::JustPlay, &[], true), &RULES), [1, 2, 4, 5, 6]);
}

#[test]
fn something_new_threshold_is_strictly_below() {
    // 120 min is not "under 2 hours".
    assert_eq!(ids(&library(), &req(Mode::SomethingNew, &[], false), &RULES), [2, 3, 5, 6]);
    let looser = PickRules { something_new_minutes: 121, ..RULES };
    assert_eq!(ids(&library(), &req(Mode::SomethingNew, &[], false), &looser), [2, 3, 4, 5, 6]);
}

#[test]
fn blacklist_excludes() {
    let rules = PickRules { blacklist: &[1, 3], ..RULES };
    assert_eq!(ids(&library(), &req(Mode::JustPlay, &[], false), &rules), [2, 4, 5, 6]);
}

#[test]
fn steam_hidden_games_excluded() {
    let rules = PickRules { hidden: &[2, 5], ..RULES };
    assert_eq!(ids(&library(), &req(Mode::JustPlay, &[], false), &rules), [1, 3, 4, 6]);
}

#[test]
fn collection_filter_limits_to_members() {
    let members: std::collections::HashSet<u32> = [1, 3, 5, 999].into_iter().collect();
    let rules = PickRules { collection_members: Some(&members), ..RULES };
    assert_eq!(ids(&library(), &req(Mode::JustPlay, &[], false), &rules), [1, 3, 5]);
    // Stacks with everything else: installed only drops 3, a mood drops 5 (no metadata).
    assert_eq!(ids(&library(), &req(Mode::JustPlay, &[Mood::Action], true), &rules), [1]);
    // An empty selection matches nothing (e.g. a collection whose games are all hidden).
    let none = std::collections::HashSet::new();
    let rules = PickRules { collection_members: Some(&none), ..RULES };
    assert!(ids(&library(), &req(Mode::JustPlay, &[], false), &rules).is_empty());
}

#[test]
fn genre_chips_match_any_and_use_tags() {
    let lib = library();
    assert_eq!(ids(&lib, &req(Mode::JustPlay, &[Mood::Puzzle], false), &RULES), [2]);
    assert_eq!(ids(&lib, &req(Mode::JustPlay, &[Mood::Strategy], false), &RULES), [4], "tag-only strategy");
    assert_eq!(ids(&lib, &req(Mode::JustPlay, &[Mood::Action, Mood::Rpg], false), &RULES), [1, 3]);
}

#[test]
fn feature_chips_must_all_match() {
    let lib = library();
    assert_eq!(ids(&lib, &req(Mode::JustPlay, &[Mood::Multiplayer], false), &RULES), [1, 4]);
    assert_eq!(ids(&lib, &req(Mode::JustPlay, &[Mood::Controller], false), &RULES), [1, 3]);
    assert_eq!(ids(&lib, &req(Mode::JustPlay, &[Mood::Multiplayer, Mood::Controller], false), &RULES), [1]);
}

#[test]
fn vr_chip_uses_store_vr_categories() {
    let lib = vec![
        game(G { id: 10, categories: &["VR Only"], ..G::default() }),
        game(G { id: 11, categories: &["vr supported", "Full controller support"], ..G::default() }),
        game(G { id: 12, categories: &["VR Support"], ..G::default() }),
        game(G { id: 13, categories: &["SteamVR Collectibles"], ..G::default() }),
        game(G { id: 14, categories: &["Single-player"], ..G::default() }),
    ];
    assert_eq!(ids(&lib, &req(Mode::JustPlay, &[Mood::Vr], false), &RULES), [10, 11, 12]);
    // Feature chips combine with AND.
    assert_eq!(ids(&lib, &req(Mode::JustPlay, &[Mood::Vr, Mood::Controller], false), &RULES), [11]);
    let r: PickRequest = serde_json::from_str(r#"{"mode":"justPlay","moods":["vr"],"installedOnly":false}"#).unwrap();
    assert_eq!(r.moods, [Mood::Vr]);
}

#[test]
fn genre_and_feature_combined() {
    let lib = library();
    // (Action OR RPG) AND Controller
    assert_eq!(ids(&lib, &req(Mode::JustPlay, &[Mood::Action, Mood::Rpg, Mood::Controller], false), &RULES), [1, 3]);
    // (Puzzle) AND Multiplayer: none
    assert!(ids(&lib, &req(Mode::JustPlay, &[Mood::Puzzle, Mood::Multiplayer], false), &RULES).is_empty());
}

#[test]
fn unknown_meta_excluded_when_filtering() {
    let lib = library();
    let r = ids(&lib, &req(Mode::JustPlay, &[Mood::Action, Mood::Adventure, Mood::Puzzle, Mood::Rpg, Mood::Strategy], false), &RULES);
    assert!(!r.contains(&5) && !r.contains(&6));
}

#[test]
fn filters_stack_with_mode_and_toggle() {
    // Installed, under 2h, controller: game 3 is uninstalled, game 1 is over threshold.
    let r = req(Mode::SomethingNew, &[Mood::Controller], true);
    assert!(ids(&library(), &r, &RULES).is_empty());
}

#[test]
fn choose_avoids_excluded_but_falls_back() {
    let lib = library();
    let pool: Vec<&LibraryGame> = lib.iter().take(2).collect();
    for r in 0..10 {
        assert_eq!(picker::choose(&pool, &[1], r).unwrap().game.app_id, 2);
    }
    // Everything excluded: repeat rather than show nothing.
    assert!(picker::choose(&pool, &[1, 2], 3).is_some());
    assert!(picker::choose(&[], &[], 3).is_none());
}

#[test]
fn choose_is_uniform_over_pool() {
    let lib = library();
    let pool: Vec<&LibraryGame> = lib.iter().collect();
    let mut seen = std::collections::HashSet::new();
    for r in 0..pool.len() as u64 {
        seen.insert(picker::choose(&pool, &[], r).unwrap().game.app_id);
    }
    assert_eq!(seen.len(), pool.len());
}

#[test]
fn reel_excludes_pick_and_fills_length() {
    let lib = library();
    let pool: Vec<&LibraryGame> = lib.iter().collect();
    let reel = picker::reel(&pool, 3, 14, 42);
    assert_eq!(reel.len(), 14, "small pools repeat to fill the reel");
    assert!(reel.iter().all(|g| g.game.app_id != 3));
    // Every other game appears.
    let distinct: std::collections::HashSet<u32> = reel.iter().map(|g| g.game.app_id).collect();
    assert_eq!(distinct.len(), 5);
    // Same seed, same order; only the pick -> empty reel.
    assert_eq!(
        picker::reel(&pool, 3, 14, 42).iter().map(|g| g.game.app_id).collect::<Vec<_>>(),
        reel.iter().map(|g| g.game.app_id).collect::<Vec<_>>()
    );
    assert!(picker::reel(&pool[..1], 1, 14, 7).is_empty());
}

#[test]
fn first_pick_is_free_then_rerolls_count_down_then_lock() {
    let limit = Some(2);
    let mut s = Session::default();
    assert_eq!(s.decide(PickKind::Fresh, limit), PickDecision::Free);
    s.shown(10);
    assert_eq!(s.decide(PickKind::Reroll, limit), PickDecision::Charged);
    s.shown(11);
    assert_eq!(s.left(limit), Some(1));
    assert_eq!(s.decide(PickKind::Reroll, limit), PickDecision::Charged);
    s.shown(12);
    assert_eq!(s.left(limit), Some(0));
    // Out of rerolls: the same game comes back, and nothing more is spent.
    assert_eq!(s.decide(PickKind::Reroll, limit), PickDecision::Locked(12));
    assert_eq!(s.used(), 2);
}

#[test]
fn cancel_then_pick_again_costs_a_reroll() {
    // Cancel isn't a session event: the next Fresh pick after a reveal is charged.
    let limit = Some(1);
    let mut s = Session::default();
    s.decide(PickKind::Fresh, limit);
    s.shown(10);
    assert_eq!(s.decide(PickKind::Fresh, limit), PickDecision::Charged);
    s.shown(11);
    assert_eq!(s.decide(PickKind::Fresh, limit), PickDecision::Locked(11), "no escaping via Cancel");
}

#[test]
fn zero_rerolls_locks_immediately_and_replacements_are_free() {
    let limit = Some(0);
    let mut s = Session::default();
    assert_eq!(s.decide(PickKind::Fresh, limit), PickDecision::Free);
    s.shown(10);
    assert_eq!(s.decide(PickKind::Reroll, limit), PickDecision::Locked(10));
    assert_eq!(s.decide(PickKind::Replace, limit), PickDecision::Free, "blacklisting swaps for free");
}

#[test]
fn unlimited_never_locks() {
    let mut s = Session::default();
    s.decide(PickKind::Fresh, None);
    for id in 0..50 {
        s.shown(id);
        assert_eq!(s.decide(PickKind::Reroll, None), PickDecision::Charged);
    }
    assert_eq!(s.left(None), None);
}

#[test]
fn launching_ends_the_cycle() {
    let limit = Some(0);
    let mut s = Session::default();
    s.decide(PickKind::Fresh, limit);
    s.shown(10);
    s.launched();
    assert_eq!(s.decide(PickKind::Fresh, limit), PickDecision::Free, "next pick after a launch is free");
}

#[test]
fn pick_kind_deserializes() {
    let k: PickKind = serde_json::from_str(r#""replace""#).unwrap();
    assert_eq!(k, PickKind::Replace);
}

#[test]
fn moods_deserialize_from_camel_case() {
    let r: PickRequest =
        serde_json::from_str(r#"{"mode":"somethingNew","moods":["rpg","multiplayer"],"installedOnly":true}"#).unwrap();
    assert_eq!(r.mode, Mode::SomethingNew);
    assert_eq!(r.moods, [Mood::Rpg, Mood::Multiplayer]);
    assert!(r.exclude.is_empty());
    assert!(r.collections.is_empty(), "collections default to none");
}
