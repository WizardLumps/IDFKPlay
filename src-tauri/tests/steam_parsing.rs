use idfkplay_lib::settings::Settings;
use idfkplay_lib::steam::accounts::{parse_login_users, pick_account, STEAMID64_BASE};
use idfkplay_lib::steam::library::{parse_app_manifest, parse_library_folders, LibraryOrigin};
use idfkplay_lib::steam::playtime::parse_local_config;
use idfkplay_lib::steam::SteamSource;
use std::fs;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing fixture {}: {e}", path.display()))
}

#[test]
fn libraryfolders_lists_all_three_drives() {
    let libs = parse_library_folders(&fixture("libraryfolders.vdf")).unwrap();
    assert_eq!(
        libs,
        vec![
            PathBuf::from(r"C:\Program Files (x86)\Steam"),
            PathBuf::from(r"D:\SteamLibrary"),
            PathBuf::from(r"E:\Games\Steam"), // forward slashes normalized
        ]
    );
}

#[test]
fn libraryfolders_legacy_format() {
    let text = r#""LibraryFolders" { "TimeNextStatsReport" "1" "ContentStatsID" "-1" "1" "D:\\Games" "2" "F:\\Steam" }"#;
    let libs = parse_library_folders(text).unwrap();
    assert_eq!(libs, vec![PathBuf::from(r"D:\Games"), PathBuf::from(r"F:\Steam")]);
}

#[test]
fn appmanifest_installed() {
    let m = parse_app_manifest(&fixture("appmanifest_620.acf")).unwrap().unwrap();
    assert_eq!(m.app_id, 620);
    assert_eq!(m.name, "Portal 2");
    assert_eq!(m.install_dir.as_deref(), Some("Portal 2"));
    assert_eq!(m.size_on_disk, Some(12_793_004_032));
    assert!(m.is_installed());
}

#[test]
fn appmanifest_mixed_case_keys() {
    let m = parse_app_manifest(&fixture("appmanifest_1145360.acf")).unwrap().unwrap();
    assert_eq!(m.app_id, 1145360);
    assert_eq!(m.name, "Hades");
    assert_eq!(m.state_flags, 6);
    assert!(m.is_installed(), "flag 6 includes the fully-installed bit");
    assert_eq!(m.size_on_disk, Some(15_422_058_496));
}

#[test]
fn appmanifest_downloading_is_not_installed() {
    let m = parse_app_manifest(&fixture("appmanifest_570.acf")).unwrap().unwrap();
    assert_eq!(m.state_flags, 1026);
    assert!(!m.is_installed());
}

#[test]
fn appmanifest_without_appid_is_skipped() {
    assert_eq!(parse_app_manifest(r#""AppState" { "name" "Ghost" }"#).unwrap(), None);
}

#[test]
fn loginusers_two_accounts() {
    let accounts = parse_login_users(&fixture("loginusers.vdf")).unwrap();
    assert_eq!(accounts.len(), 2);

    let main = &accounts[0];
    assert_eq!(main.steam_id, "76561198000000001");
    assert_eq!(main.account_id as u64, 76561198000000001 - STEAMID64_BASE);
    assert_eq!(main.persona_name, "Main Player");
    assert!(main.most_recent);

    let alt = &accounts[1];
    assert_eq!(alt.persona_name, "Alt \"Smurf\" Account");
    assert_eq!(alt.account_id, 52_079_950);
    assert!(!alt.most_recent);
}

#[test]
fn account_selection() {
    let accounts = parse_login_users(&fixture("loginusers.vdf")).unwrap();
    // Default: MostRecent wins even though the alt has a newer timestamp.
    assert_eq!(pick_account(&accounts, None).unwrap().account_name, "mainaccount");
    assert_eq!(
        pick_account(&accounts, Some("76561198012345678")).unwrap().account_name,
        "altaccount"
    );
    // Unknown preference falls back to MostRecent.
    assert_eq!(pick_account(&accounts, Some("123")).unwrap().account_name, "mainaccount");
    assert!(pick_account(&[], None).is_none());
}

#[test]
fn localconfig_playtime_mixed_case() {
    let stats = parse_local_config(&fixture("localconfig.vdf")).unwrap();
    assert_eq!(stats[&620].playtime_minutes, 754);
    assert_eq!(stats[&620].last_played, Some(1_728_000_000));
    assert_eq!(stats[&1145360].playtime_minutes, 61);
    assert_eq!(stats[&1145360].last_played, Some(1_729_500_000));
    assert_eq!(stats[&570].playtime_minutes, 0, "missing Playtime = 0");
    assert_eq!(stats[&570].last_played, None, "LastPlayed 0 = never");
    assert_eq!(stats[&440].playtime_minutes, 9001);
}

/// Unique scratch dir under the OS temp folder; removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("idfkplay-test-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn full_scan_over_fake_steam_install() {
    let tmp = TempDir::new("scan");
    let steam = tmp.0.join("Steam");
    let lib2 = tmp.0.join("Library2");
    let missing = tmp.0.join("Unplugged");
    let esc = |p: &Path| p.display().to_string().replace('\\', "\\\\");

    write(
        &steam.join("steamapps").join("libraryfolders.vdf"),
        &format!(
            "\"libraryfolders\" {{ \"0\" {{ \"path\" \"{}\" }} \"1\" {{ \"path\" \"{}\" }} \"2\" {{ \"path\" \"{}\" }} }}",
            esc(&steam),
            esc(&lib2),
            esc(&missing)
        ),
    );
    write(&steam.join("steamapps").join("appmanifest_620.acf"), &fixture("appmanifest_620.acf"));
    write(&steam.join("steamapps").join("appmanifest_570.acf"), &fixture("appmanifest_570.acf"));
    write(&lib2.join("steamapps").join("appmanifest_1145360.acf"), &fixture("appmanifest_1145360.acf"));
    write(&lib2.join("steamapps").join("appmanifest_999.acf"), "\"AppState\" { broken");
    fs::write(lib2.join("steamapps").join("appmanifest_998.acf"), [0u8; 64]).unwrap();
    write(&steam.join("config").join("loginusers.vdf"), &fixture("loginusers.vdf"));
    let account_id = 76561198000000001u64 - STEAMID64_BASE;
    write(
        &steam.join("userdata").join(account_id.to_string()).join("config").join("localconfig.vdf"),
        &fixture("localconfig.vdf"),
    );

    let settings = Settings { steam_path: Some(steam.display().to_string()), ..Default::default() };
    let scan = SteamSource::new(settings).scan_detailed().unwrap();

    // Steam root listed once (it's both implicit and in libraryfolders), plus lib2 and the unplugged one.
    assert_eq!(scan.info.libraries.len(), 3);
    assert!(scan.info.libraries.iter().all(|l| l.origin == LibraryOrigin::Steam));
    assert_eq!(scan.info.libraries.iter().filter(|l| l.reachable).count(), 2);
    assert!(scan.warnings.iter().any(|w| w.contains("isn't reachable")));
    assert!(scan.warnings.iter().any(|w| w.contains("broken game manifest") && w.contains("line")));
    assert!(scan.warnings.iter().any(|w| w.contains("zero-filled")));

    assert_eq!(scan.info.selected_account.as_deref(), Some("76561198000000001"));
    let names: Vec<_> = scan.games.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(names, ["Dota 2", "Hades", "Portal 2"], "sorted by name");

    let portal = scan.games.iter().find(|g| g.app_id == 620).unwrap();
    assert!(portal.installed);
    assert_eq!(portal.playtime_minutes, 754);
    assert!(portal.install_path.as_deref().unwrap().ends_with(r"steamapps\common\Portal 2"));

    let dota = scan.games.iter().find(|g| g.app_id == 570).unwrap();
    assert!(!dota.installed);
    assert_eq!(dota.size_on_disk, None, "zero size is unknown");

    assert_eq!(scan.games.iter().find(|g| g.app_id == 1145360).unwrap().playtime_minutes, 61);
}

#[test]
fn bad_manual_path_falls_back_or_errors_calmly() {
    let tmp = TempDir::new("badpath");
    let settings = Settings {
        steam_path: Some(tmp.0.join("nope").display().to_string()),
        ..Default::default()
    };
    // Either auto-detection finds a real Steam (with a warning) or we get a specific message.
    match SteamSource::new(settings).scan_detailed() {
        Ok(scan) => assert!(scan.warnings.iter().any(|w| w.contains("doesn't look like a Steam install"))),
        Err(e) => assert!(e.to_string().contains("Steam folder in Settings")),
    }
}
