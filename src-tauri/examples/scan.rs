//! Dev diagnostic: scans the local Steam install with saved settings and prints a summary.
//! Run with `cargo run --example scan` from src-tauri.

use idfkplay_lib::settings;
use idfkplay_lib::steam::SteamSource;

fn main() {
    let dir = settings::config_dir(std::env::temp_dir());
    let source = SteamSource::new(settings::load(&dir));
    match source.scan_detailed() {
        Ok(scan) => {
            println!("Steam: {} ({:?})", scan.info.steam_path, scan.info.steam_path_source);
            for lib in &scan.info.libraries {
                println!("  library {} reachable={} manifests={}", lib.path, lib.reachable, lib.game_count);
            }
            for a in &scan.info.accounts {
                let sel = if Some(&a.steam_id) == scan.info.selected_account.as_ref() { " [selected]" } else { "" };
                println!("  account {} ({}){sel}", a.persona_name, a.account_id);
            }
            let installed = scan.games.iter().filter(|g| g.installed).count();
            let played = scan.games.iter().filter(|g| g.playtime_minutes > 0).count();
            println!("Games: {} total, {installed} installed, {played} with playtime", scan.games.len());
            for g in scan.games.iter().take(10) {
                println!("  {:>8}  {:<40} installed={} playtime={}m", g.app_id, g.name, g.installed, g.playtime_minutes);
            }
            for w in &scan.warnings {
                println!("warning: {w}");
            }
        }
        Err(e) => println!("error: {e}"),
    }
}
