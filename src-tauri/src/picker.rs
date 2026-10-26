//! Choosing a game: modes, mood filters, installed toggle, blacklist, rerolls.
//! Pure functions; randomness is passed in so tests are deterministic.

use crate::library::{LibraryGame, MetaState};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const REROLLS_PER_SESSION: u32 = 2;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    JustPlay,
    SomethingNew,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Mood {
    Action,
    Adventure,
    Puzzle,
    Rpg,
    Strategy,
    Multiplayer,
    Controller,
    Vr,
}

impl Mood {
    /// Feature moods must all match; genre moods need only one match.
    pub fn is_feature(self) -> bool {
        matches!(self, Mood::Multiplayer | Mood::Controller | Mood::Vr)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickRequest {
    pub mode: Mode,
    #[serde(default)]
    pub moods: Vec<Mood>,
    pub installed_only: bool,
    /// Games already shown in this reveal (avoided on reroll when possible).
    #[serde(default)]
    pub exclude: Vec<u32>,
    /// Steam collection IDs; a game must be in any of them. Empty = no collection filter.
    #[serde(default)]
    pub collections: Vec<String>,
}

/// Inputs from settings and the library that affect eligibility.
#[derive(Debug, Clone)]
pub struct PickRules<'a> {
    pub something_new_minutes: u32,
    pub blacklist: &'a [u32],
    /// Members of the selected collections; `None` = no collection filter.
    pub collection_members: Option<&'a HashSet<u32>>,
    /// Also never picked (games hidden in Steam, when that setting is on).
    pub hidden: &'a [u32],
}

fn has_any(list: &[String], wanted: &[&str]) -> bool {
    list.iter().any(|v| wanted.iter().any(|w| v.eq_ignore_ascii_case(w)))
}

pub fn matches_mood(g: &LibraryGame, mood: Mood) -> bool {
    let genre_or_tag = |names: &[&str]| has_any(&g.genres, names) || has_any(&g.tags, names);
    match mood {
        Mood::Action => genre_or_tag(&["Action"]),
        Mood::Adventure => genre_or_tag(&["Adventure"]),
        // Puzzle is a community tag, not a store genre.
        Mood::Puzzle => has_any(&g.tags, &["Puzzle", "Puzzle-Platformer", "Logic"]),
        Mood::Rpg => genre_or_tag(&["RPG", "Action RPG", "JRPG", "CRPG", "Party-Based RPG"]),
        Mood::Strategy => genre_or_tag(&["Strategy", "Turn-Based Strategy", "RTS", "Grand Strategy", "4X"]),
        Mood::Multiplayer => g.categories.iter().any(|c| {
            let c = c.to_lowercase();
            ["multi-player", "co-op", "pvp", "mmo", "split screen"].iter().any(|k| c.contains(k))
        }),
        Mood::Controller => has_any(&g.categories, &["Full controller support", "Partial Controller Support"]),
        // Store categories; "SteamVR Collectibles" is about trading cards, not VR play.
        Mood::Vr => has_any(&g.categories, &["VR Only", "VR Supported", "VR Support"]),
    }
}

/// Any genre mood AND every feature mood. Games without known metadata never match a filter.
pub fn matches_moods(g: &LibraryGame, moods: &[Mood]) -> bool {
    if moods.is_empty() {
        return true;
    }
    if g.meta != MetaState::Known {
        return false;
    }
    let mut genres = moods.iter().filter(|m| !m.is_feature()).peekable();
    let genre_ok = genres.peek().is_none() || genres.any(|&m| matches_mood(g, m));
    let features_ok = moods.iter().filter(|m| m.is_feature()).all(|&m| matches_mood(g, m));
    genre_ok && features_ok
}

pub fn is_eligible(g: &LibraryGame, req: &PickRequest, rules: &PickRules) -> bool {
    let id = g.game.app_id;
    !rules.blacklist.contains(&id)
        && !rules.hidden.contains(&id)
        && rules.collection_members.is_none_or(|m| m.contains(&id))
        && (!req.installed_only || g.game.installed)
        && (req.mode == Mode::JustPlay || g.game.playtime_minutes < rules.something_new_minutes)
        && matches_moods(g, &req.moods)
}

pub fn eligible<'a>(games: &'a [LibraryGame], req: &PickRequest, rules: &PickRules) -> Vec<&'a LibraryGame> {
    games.iter().filter(|g| is_eligible(g, req, rules)).collect()
}

/// Picks uniformly from `pool`, avoiding `exclude` unless that would leave nothing.
pub fn choose<'a>(pool: &[&'a LibraryGame], exclude: &[u32], random: u64) -> Option<&'a LibraryGame> {
    let fresh: Vec<&LibraryGame> = pool.iter().copied().filter(|g| !exclude.contains(&g.game.app_id)).collect();
    let from = if fresh.is_empty() { pool } else { &fresh[..] };
    if from.is_empty() {
        return None;
    }
    Some(from[(random % from.len() as u64) as usize])
}

/// Small deterministic PRNG (xorshift64*) for shuffling from one seed.
struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

/// Filler tiles for the spin animation: up to `n` games from `pool` other than `pick`,
/// shuffled, repeating when the pool is small. Empty if nothing else is eligible.
pub fn reel<'a>(pool: &[&'a LibraryGame], pick: u32, n: usize, seed: u64) -> Vec<&'a LibraryGame> {
    let mut others: Vec<&LibraryGame> = pool.iter().copied().filter(|g| g.game.app_id != pick).collect();
    if others.is_empty() {
        return Vec::new();
    }
    let mut rng = XorShift(seed | 1);
    for i in (1..others.len()).rev() {
        let j = (rng.next() % (i as u64 + 1)) as usize;
        others.swap(i, j);
    }
    others.iter().copied().cycle().take(n).collect()
}

/// Reroll budget for one app session.
#[derive(Debug, Clone)]
pub struct Rerolls {
    used: u32,
    limit: u32,
}

impl Default for Rerolls {
    fn default() -> Self {
        Rerolls { used: 0, limit: REROLLS_PER_SESSION }
    }
}

impl Rerolls {
    pub fn left(&self) -> u32 {
        self.limit.saturating_sub(self.used)
    }

    /// Uses one reroll. Returns false (and changes nothing) when none are left.
    pub fn try_use(&mut self) -> bool {
        if self.left() == 0 {
            return false;
        }
        self.used += 1;
        true
    }
}

/// A random u64 without an RNG dependency: std's per-process random hash keys mixed with the clock.
pub fn random_u64() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    );
    h.finish()
}
