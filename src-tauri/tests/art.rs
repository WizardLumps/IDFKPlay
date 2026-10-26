use idfkplay_lib::steam::art::{find_local, game_art, ArtKind, ArtSource};
use std::fs;
use std::path::{Path, PathBuf};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("idfkplay-art-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn touch(p: &Path) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, b"jpg").unwrap();
}

#[test]
fn per_app_layout_with_hashed_subfolders() {
    let tmp = TempDir::new("perapp");
    let cache = &tmp.0;
    touch(&cache.join("620").join("library_600x900.jpg"));
    touch(&cache.join("620").join("a58588d8").join("library_hero.jpg"));
    touch(&cache.join("620").join("faffc0f5").join("library_header.jpg"));

    assert_eq!(find_local(cache, 620, ArtKind::Portrait), Some(cache.join("620").join("library_600x900.jpg")));
    assert_eq!(find_local(cache, 620, ArtKind::Hero), Some(cache.join("620").join("a58588d8").join("library_hero.jpg")));
    assert_eq!(find_local(cache, 620, ArtKind::Header), Some(cache.join("620").join("faffc0f5").join("library_header.jpg")));
}

#[test]
fn prefers_2x_and_skips_empty_files() {
    let tmp = TempDir::new("2x");
    let cache = &tmp.0;
    touch(&cache.join("7").join("library_600x900_2x.jpg"));
    touch(&cache.join("7").join("library_600x900.jpg"));
    assert!(find_local(cache, 7, ArtKind::Portrait).unwrap().ends_with("library_600x900_2x.jpg"));

    // A zero-byte placeholder doesn't count.
    fs::create_dir_all(cache.join("8")).unwrap();
    fs::write(cache.join("8").join("header.jpg"), b"").unwrap();
    assert_eq!(find_local(cache, 8, ArtKind::Header), None);
}

#[test]
fn old_flat_layout() {
    let tmp = TempDir::new("flat");
    let cache = &tmp.0;
    touch(&cache.join("400_library_600x900.jpg"));
    touch(&cache.join("400_header.jpg"));
    touch(&cache.join("400_library_hero.jpg"));
    assert_eq!(find_local(cache, 400, ArtKind::Portrait), Some(cache.join("400_library_600x900.jpg")));
    assert_eq!(find_local(cache, 400, ArtKind::Header), Some(cache.join("400_header.jpg")));
    assert_eq!(find_local(cache, 400, ArtKind::Hero), Some(cache.join("400_library_hero.jpg")));
}

#[test]
fn falls_back_to_cdn() {
    let tmp = TempDir::new("cdn");
    let steam = &tmp.0;
    touch(&steam.join("appcache").join("librarycache").join("620").join("library_600x900.jpg"));

    let art = game_art(Some(steam), 620);
    assert!(matches!(&art.portrait[0], ArtSource::File(p) if p.ends_with("library_600x900.jpg")));
    assert_eq!(
        art.portrait[1],
        ArtSource::Url("https://cdn.cloudflare.steamstatic.com/steam/apps/620/library_600x900.jpg".into())
    );
    // Nothing cached for the hero: CDN only.
    assert_eq!(art.hero.len(), 1);
    assert!(matches!(&art.hero[0], ArtSource::Url(u) if u.ends_with("/620/library_hero.jpg")));

    let no_steam = game_art(None, 1145360);
    assert_eq!(no_steam.header, [ArtSource::Url("https://cdn.cloudflare.steamstatic.com/steam/apps/1145360/header.jpg".into())]);
}
