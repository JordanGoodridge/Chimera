//! The next map of a rotating minigame (`MinigameDef::rotate`), the host's
//! choice (a server or a standalone game: `chimera_net::minigame_net`
//! rotates; clients follow the level change):
//!
//! - In turn (`Rotation::InTurn`, the default): the pool's next map after
//!   this one that every player can play, this one last.
//! - Random (`Rotation::Random`): a map of the pool at random among those
//!   every player can play, not one of the last `avoid_recent` played
//!   (at most the candidates' count less one, so never the same map
//!   twice in a row while there is another).
//!
//! `MapHistory` is the maps played, most recent last, kept by the host for
//! as long as it runs (over every rotation and `change_match`), with the
//! generator random picks draw on (seeded: an app sets the seed, e.g.
//! Ultimate Gun Game its launch seed; the default is fixed, for tests).

use bevy::prelude::*;

use super::{
    auto::Rng,
    def::{MinigameDef, Rotation},
};

/// Most maps `MapHistory` keeps.
pub const HISTORY: usize = 64;

/// The maps played on this host, most recent last, and the generator its
/// random picks draw on.
#[derive(Resource, Clone, Debug)]
pub struct MapHistory {
    pub played: Vec<String>,
    pub rng: Rng,
}

impl Default for MapHistory {
    fn default() -> Self {
        Self::seeded(0)
    }
}

impl MapHistory {
    pub fn seeded(seed: u64) -> Self {
        Self {
            played: Vec::new(),
            rng: Rng::new(seed),
        }
    }

    /// `map` was played (not again when it was the last).
    pub fn record(&mut self, map: &str) {
        if self.played.last().is_some_and(|m| m == map) {
            return;
        }
        self.played.push(map.to_string());
        if self.played.len() > HISTORY {
            self.played.remove(0);
        }
    }

    /// The last `n` maps played.
    pub fn recent(&self, n: usize) -> &[String] {
        &self.played[self.played.len().saturating_sub(n)..]
    }

    /// The next map after `here` (played, recorded first) from `pool` by
    /// `def`'s rotation, among the maps `playable` allows; None: none is
    /// (stay).
    pub fn next(&mut self, def: &MinigameDef, pool: &[String], here: &str, playable: &dyn Fn(&str) -> bool) -> Option<String> {
        self.record(here);
        match def.rotation {
            Rotation::InTurn => {
                let start = pool.iter().position(|m| m == here).map_or(0, |i| i + 1);
                (0..pool.len()).map(|i| &pool[(start + i) % pool.len()]).find(|m| playable(m)).cloned()
            }
            Rotation::Random => {
                let mut candidates: Vec<&String> = Vec::new();
                for m in pool.iter().filter(|m| playable(m)) {
                    if !candidates.contains(&m) {
                        candidates.push(m);
                    }
                }
                if candidates.is_empty() {
                    return None;
                }
                let avoid = def.avoid_recent.min(candidates.len() - 1);
                let recent = self.recent(avoid).to_vec();
                let fresh: Vec<&String> = candidates.iter().copied().filter(|m| !recent.contains(m)).collect();
                let from = if fresh.is_empty() { candidates } else { fresh };
                Some(from[self.rng.below(from.len())].clone())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(rotation: &str, avoid: usize) -> MinigameDef {
        MinigameDef::parse(
            "t",
            &format!(
                "id = \"t:t\"\nname = \"T\"\nplayers = {{ min = 1, max = 4 }}\nmaps = [\"g:a\"]\n\
                 teams = \"free_for_all\"\ntime_limit = 60\nrotation = \"{rotation}\"\navoid_recent = {avoid}\n\
                 [rules]\nmodule = \"chimera:ladder\"\nladder = [\"x:y\"]\n"
            ),
        )
        .unwrap()
    }

    fn pool(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("g:m{i}")).collect()
    }

    #[test]
    fn random_rotation_skips_the_last_ten_played() {
        let d = def("random", 10);
        let pool = pool(25);
        let mut h = MapHistory::seeded(7);
        let mut here = pool[0].clone();
        let mut seen = vec![here.clone()];
        for _ in 0..300 {
            let next = h.next(&d, &pool, &here, &|_| true).unwrap();
            let last: Vec<String> = h.recent(10).to_vec();
            assert!(!last.contains(&next), "{next} is one of the last 10: {last:?}");
            here = next;
            seen.push(here.clone());
        }
        assert!(pool.iter().all(|m| seen.contains(m)), "every map comes up");
    }

    #[test]
    fn a_small_pool_never_repeats_a_map_and_cycles() {
        let d = def("random", 10);
        for n in [2, 3, 5] {
            let pool = pool(n);
            let mut h = MapHistory::seeded(n as u64);
            let mut here = pool[0].clone();
            let mut order = Vec::new();
            for _ in 0..4 * n {
                let next = h.next(&d, &pool, &here, &|_| true).unwrap();
                assert_ne!(next, here, "never the same map twice");
                // min(10, n - 1) back: each of the n maps once per n.
                assert!(!h.recent(n - 1).contains(&next), "{next}: {:?}", h.recent(n - 1));
                order.push(next.clone());
                here = next;
            }
            assert_eq!(order[..n], order[n..2 * n], "a pool smaller than the history cycles");
        }
        // One map: it again.
        let one = pool(1);
        assert_eq!(MapHistory::default().next(&d, &one, &one[0], &|_| true), Some(one[0].clone()));
    }

    #[test]
    fn unplayable_maps_are_skipped_and_in_turn_is_unchanged() {
        let d = def("random", 10);
        let pool = pool(6);
        let mut h = MapHistory::default();
        for _ in 0..20 {
            let next = h.next(&d, &pool, "g:m0", &|m| m != "g:m3").unwrap();
            assert_ne!(next, "g:m3");
        }
        assert_eq!(h.next(&d, &pool, "g:m0", &|_| false), None);
        let t = def("in_turn", 10);
        assert_eq!(h.next(&t, &pool, "g:m2", &|_| true).as_deref(), Some("g:m3"));
        assert_eq!(h.next(&t, &pool, "g:m5", &|m| m != "g:m0").as_deref(), Some("g:m1"));
    }
}
