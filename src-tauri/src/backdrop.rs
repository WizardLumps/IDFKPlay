//! Chooses the cover art for the home-screen backdrop: a personal-feeling random mix
//! that loads fast (locally cached art first).

use crate::picker;

/// One candidate game for the backdrop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidate {
    pub app_id: u32,
    /// Cover art exists in Steam's local cache (instant, works offline).
    pub has_local_art: bool,
    pub favorite: bool,
    pub played: bool,
}

/// Small deterministic shuffle so tests can pin the result with a seed.
fn shuffle<T>(items: &mut [T], seed: &mut u64) {
    for i in (1..items.len()).rev() {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        let j = (*seed % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
}

/// Up to `n` app IDs: games with local art first (favorites, then played, then the rest),
/// then games that need the CDN, each group shuffled. The final order is shuffled too so
/// favorites don't bunch up in one row.
pub fn choose(candidates: &[Candidate], n: usize, seed: u64) -> Vec<u32> {
    let mut seed = seed | 1;
    let rank = |c: &Candidate| (!c.has_local_art, !c.favorite, !c.played);
    let mut groups: Vec<Vec<u32>> = vec![Vec::new(); 8];
    for c in candidates {
        let (a, b, d) = rank(c);
        groups[(a as usize) << 2 | (b as usize) << 1 | d as usize].push(c.app_id);
    }
    let mut out = Vec::with_capacity(n);
    for g in &mut groups {
        shuffle(g, &mut seed);
        out.extend(g.iter().copied().take(n.saturating_sub(out.len())));
    }
    shuffle(&mut out, &mut seed);
    out
}

/// Seed for a fresh mix each launch.
pub fn seed() -> u64 {
    picker::random_u64()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(app_id: u32, has_local_art: bool, favorite: bool, played: bool) -> Candidate {
        Candidate { app_id, has_local_art, favorite, played }
    }

    #[test]
    fn prefers_local_art_then_favorites_then_played() {
        let cands = [
            c(1, false, true, true), // favorite but CDN only
            c(2, true, false, false),
            c(3, true, true, false),
            c(4, true, false, true),
            c(5, true, false, false),
        ];
        let mut got = choose(&cands, 3, 7);
        got.sort();
        // Local favorite (3), local played (4), then one local other (2 or 5).
        assert!(got.contains(&3) && got.contains(&4));
        assert!(got.contains(&2) || got.contains(&5));
        assert!(!got.contains(&1), "CDN-only art comes last");
    }

    #[test]
    fn fills_from_cdn_when_local_runs_out_and_never_repeats() {
        let cands = [c(1, false, false, false), c(2, true, false, false), c(3, false, true, false)];
        let mut got = choose(&cands, 10, 3);
        got.sort();
        assert_eq!(got, [1, 2, 3]);
        assert!(choose(&[], 10, 3).is_empty());
    }

    #[test]
    fn same_seed_same_mix() {
        let cands: Vec<Candidate> = (0..50).map(|i| c(i, i % 2 == 0, i % 7 == 0, i % 3 == 0)).collect();
        assert_eq!(choose(&cands, 20, 99), choose(&cands, 20, 99));
        assert_eq!(choose(&cands, 20, 99).len(), 20);
    }
}
