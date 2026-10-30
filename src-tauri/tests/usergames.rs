use idfkplay_lib::library::{self, ApiStatus, MetaState, RawLibrary};
use idfkplay_lib::picker::{self, Mode, Mood, PickRequest, PickRules};
use idfkplay_lib::steam::collections::{Collection, Collections, FAVORITES_ID};
use idfkplay_lib::steam::locate::PathSource;
use idfkplay_lib::steam::SteamInfo;
use idfkplay_lib::usergames::{
    is_custom, CustomGameInput, ImageChange, Launch, OverrideInput, UserGames, CUSTOM_ID_BASE,
};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("idfkplay-user-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn wow() -> CustomGameInput {
    CustomGameInput {
        id: None,
        name: "  World of Warcraft  ".into(),
        launch: Launch::Program {
            path: r#""C:\Program Files (x86)\Battle.net\Battle.net.exe""#.into(),
            args: r#"--exec="launch WoW""#.into(),
        },
        image: ImageChange::Keep,
        moods: vec![Mood::Rpg, Mood::Multiplayer, Mood::Rpg],
        favorite: true,
        playtime_hours: None,
    }
}

#[test]
fn add_custom_game_assigns_id_and_tidies_input() {
    let tmp = TempDir::new("add");
    let mut u = UserGames::default();
    let id = u.save_custom(&tmp.0, wow()).unwrap();
    assert_eq!(id, CUSTOM_ID_BASE);
    assert!(is_custom(id) && !is_custom(2483190));
    let g = u.custom(id).unwrap();
    assert_eq!(g.name, "World of Warcraft");
    assert_eq!(g.moods, [Mood::Rpg, Mood::Multiplayer], "duplicates dropped");
    assert_eq!(g.playtime_minutes, None);
    // Surrounding quotes from a pasted path are removed; arguments are kept as typed.
    assert_eq!(
        g.launch,
        Launch::Program {
            path: r"C:\Program Files (x86)\Battle.net\Battle.net.exe".into(),
            args: r#"--exec="launch WoW""#.into()
        }
    );
    assert_eq!(u.save_custom(&tmp.0, wow()).unwrap(), CUSTOM_ID_BASE + 1, "next id");
}

#[test]
fn custom_game_validation() {
    let tmp = TempDir::new("validate");
    let mut u = UserGames::default();
    let err = |input| UserGames::default().save_custom(&tmp.0, input).unwrap_err();
    assert!(err(CustomGameInput { name: " ".into(), ..wow() }).contains("name"));
    assert!(err(CustomGameInput { launch: Launch::Program { path: " ".into(), args: "".into() }, ..wow() }).contains("program"));
    assert!(err(CustomGameInput { launch: Launch::Link { url: "epic".into() }, ..wow() }).contains("://"));
    assert!(err(CustomGameInput { playtime_hours: Some(-1.0), ..wow() }).contains("negative"));
    assert!(err(CustomGameInput { image: ImageChange::Url { url: "http://x/a.jpg".into() }, ..wow() }).contains("https"));
    assert!(u.save_custom(&tmp.0, CustomGameInput { id: Some(42), ..wow() }).is_err(), "unknown id");
}

#[test]
fn images_are_copied_and_old_copies_cleaned_up() {
    let tmp = TempDir::new("img");
    let src = tmp.0.join("cover.PNG");
    fs::write(&src, b"png").unwrap();
    let mut u = UserGames::default();
    let id = u
        .save_custom(&tmp.0, CustomGameInput { image: ImageChange::File { path: src.display().to_string() }, ..wow() })
        .unwrap();
    let first = u.custom(id).unwrap().image.clone().unwrap();
    assert!(first.ends_with(".png"));
    assert!(tmp.0.join("art").join(&first).is_file(), "copied into the art folder");
    fs::remove_file(&src).unwrap();
    assert!(tmp.0.join("art").join(&first).is_file(), "survives the original being deleted");

    // Switching to a URL removes the copy.
    u.save_custom(
        &tmp.0,
        CustomGameInput { id: Some(id), image: ImageChange::Url { url: "https://example.com/a.jpg".into() }, ..wow() },
    )
    .unwrap();
    assert!(!tmp.0.join("art").join(&first).exists());
    let bad = tmp.0.join("notes.txt");
    fs::write(&bad, b"x").unwrap();
    assert!(u
        .save_custom(&tmp.0, CustomGameInput { image: ImageChange::File { path: bad.display().to_string() }, ..wow() })
        .is_err());
}

#[test]
fn overrides_and_reset() {
    let tmp = TempDir::new("override");
    let mut u = UserGames::default();
    u.save_override(&tmp.0, 620, OverrideInput { name: Some(" Portal Two ".into()), ..Default::default() }).unwrap();
    assert_eq!(u.overrides[&620].name.as_deref(), Some("Portal Two"));
    // Saving nothing removes the override entirely.
    u.save_override(&tmp.0, 620, OverrideInput::default()).unwrap();
    assert!(!u.overrides.contains_key(&620));
    u.save_override(&tmp.0, 620, OverrideInput { moods: Some(vec![Mood::Puzzle]), ..Default::default() }).unwrap();
    u.reset_override(&tmp.0, 620);
    assert!(u.overrides.is_empty());
}

#[test]
fn favorites_merge_steam_overrides_and_custom() {
    let tmp = TempDir::new("fav");
    let mut u = UserGames::default();
    let wow_id = u.save_custom(&tmp.0, wow()).unwrap(); // favorite: true
    u.set_favorite(620, false, true); // un-favorite a Steam favorite
    u.set_favorite(400, true, false); // favorite a Steam game
    u.set_favorite(70, true, true); // already a Steam favorite: no override needed
    assert!(!u.overrides.contains_key(&70));
    let favs = u.favorites(&[620, 70]);
    assert_eq!(favs, [70, 400, wow_id]);
    // Toggling back to Steam's state removes the override.
    u.set_favorite(620, true, true);
    assert!(!u.overrides.contains_key(&620));
    u.set_favorite(wow_id, false, false);
    assert!(!u.custom(wow_id).unwrap().favorite);
}

fn raw(user: UserGames, steam_favs: Vec<u32>) -> RawLibrary {
    let mut list = vec![];
    if !steam_favs.is_empty() {
        list.push(Collection { id: FAVORITES_ID.into(), name: "Favorites".into(), app_ids: steam_favs, dynamic: false });
    }
    RawLibrary {
        info: SteamInfo {
            steam_path: r"C:\Steam".into(),
            steam_path_source: PathSource::Default,
            libraries: vec![],
            accounts: vec![],
            selected_account: None,
        },
        local: vec![],
        owned: None,
        collections: Collections { list, hidden: vec![] },
        user,
        api: ApiStatus::NoKey,
        warnings: vec![],
    }
}

#[test]
fn custom_games_join_the_view_with_moods_and_favorites() {
    let tmp = TempDir::new("view");
    let mut u = UserGames::default();
    let id = u.save_custom(&tmp.0, wow()).unwrap();
    let raw = raw(u, vec![]);
    let view = library::build_view(&raw, &HashMap::new());
    let g = view.games.iter().find(|g| g.game.app_id == id).unwrap();
    assert_eq!(g.game.source, "custom");
    assert!(g.game.installed && g.game.playtime_unknown);
    assert_eq!(g.meta, MetaState::Known);
    assert!(picker::matches_moods(g, &[Mood::Rpg, Mood::Multiplayer]));
    assert!(!picker::matches_moods(g, &[Mood::Puzzle]));
    // A Favorites collection appears even though Steam has none.
    let fav = view.collections.iter().find(|c| c.id == FAVORITES_ID).unwrap();
    assert_eq!(fav.count, 1);
}

#[test]
fn unknown_playtime_never_counts_as_something_new() {
    let tmp = TempDir::new("new");
    let mut u = UserGames::default();
    let unknown = u.save_custom(&tmp.0, CustomGameInput { favorite: false, ..wow() }).unwrap();
    let barely = u
        .save_custom(&tmp.0, CustomGameInput { name: "Barely".into(), playtime_hours: Some(0.5), ..wow() })
        .unwrap();
    let view = library::build_view(&raw(u, vec![]), &HashMap::new());
    let req = |mode| PickRequest { mode, moods: vec![], installed_only: true, exclude: vec![], collections: vec![] };
    let rules = PickRules { something_new_minutes: 120, blacklist: &[], collection_members: None, hidden: &[] };
    let ids = |mode| picker::eligible(&view.games, &req(mode), &rules).iter().map(|g| g.game.app_id).collect::<Vec<_>>();
    assert_eq!(ids(Mode::SomethingNew), [barely]);
    let mut all = ids(Mode::JustPlay);
    all.sort();
    assert_eq!(all, [unknown, barely]);
}
