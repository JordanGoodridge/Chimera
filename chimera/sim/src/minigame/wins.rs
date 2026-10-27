//! Matches won, per player, for as long as the server (or the single
//! player game) runs: `MatchWins` counts them by the player's name (the
//! stable identity: it survives a disconnect and the character coming
//! back, a map rotation, `change_match` and a new automatic ladder, none of
//! which clear it), and `MatchesWon` carries each player's count on the
//! character, which `chimera_net::minigame_net` replicates for the
//! scoreboard (`chimera_client::client::minigame_scoreboard`). Counted
//! when a match is decided with a winner (`results::record`), on the
//! authoritative side only.
//!
//! Known shortcuts: keyed by name, so a player who renames starts from
//! zero and two players with one name share a count (the server does not
//! make names unique); not saved to disk.

use std::collections::BTreeMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Matches won so far, by player name.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct MatchWins(pub BTreeMap<String, u32>);

impl MatchWins {
    pub fn of(&self, name: &str) -> u32 {
        self.0.get(name).copied().unwrap_or(0)
    }

    pub fn add(&mut self, name: &str) {
        *self.0.entry(name.to_string()).or_insert(0) += 1;
    }
}

/// A player's matches won (replicated on the character).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchesWon(pub u32);

/// Puts each playing character's count on it.
pub(super) fn publish(world: &mut World, players: &[(Entity, String)]) {
    let Some(wins) = world.get_resource::<MatchWins>() else { return };
    let counts: Vec<(Entity, u32)> = players.iter().map(|(e, n)| (*e, wins.of(n))).collect();
    for (e, n) in counts {
        match world.get_mut::<MatchesWon>(e) {
            Some(mut m) => {
                m.set_if_neq(MatchesWon(n));
            }
            None => {
                world.entity_mut(e).insert(MatchesWon(n));
            }
        }
    }
}
