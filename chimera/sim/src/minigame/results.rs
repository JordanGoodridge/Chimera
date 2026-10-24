//! A match's results (the end-of-match panel): once the match has its
//! result (the intermission, then match over) `record` collects
//! `MatchResults` on the authoritative side: each player's name, kills,
//! deaths and, on a ladder, final rung, the winner first, then by rung,
//! kills and fewest deaths. It is data, not drawing: `hud::MinigameHud`
//! carries it to whichever game's HUD is up (`MatchResults::lines` words
//! it), and a network server replicates it (`chimera_net::minigame_net`'s
//! `NetMinigame`) so every client's HUD shows the same. Gone when the next
//! match starts (`minigame::restart`).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::{ActiveMinigame, ladder, runtime};
use crate::{core::Score, mode::Match};

/// One player's line of the results.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ResultRow {
    pub name: String,
    pub kills: i32,
    pub deaths: u32,
    /// On a ladder: the final rung, counted from 1.
    pub rung: Option<u32>,
    pub winner: bool,
    /// The character, on the side that played it (not replicated).
    #[serde(skip)]
    pub player: Option<Entity>,
}

/// The finished match's results, best first.
#[derive(Resource, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MatchResults {
    /// The minigame's name.
    pub title: String,
    /// On a ladder: how many rungs it has (0: not one).
    pub rungs: u32,
    pub rows: Vec<ResultRow>,
}

impl MatchResults {
    /// The match's winner, if it had one (a draw: none).
    pub fn winner(&self) -> Option<&ResultRow> {
        self.rows.iter().find(|r| r.winner)
    }

    /// The panel's text, top to bottom: who won, then a line per player
    /// (`name` is how the HUD words `row`).
    pub fn lines(&self, name: impl Fn(&ResultRow) -> String) -> Vec<String> {
        let mut out = vec![match self.winner() {
            Some(w) => format!("{} wins the match", name(w)),
            None => "Match over: a draw".to_string(),
        }];
        for (i, r) in self.rows.iter().enumerate() {
            let rung = match (r.rung, self.rungs) {
                (Some(n), total) if total > 0 => format!("  rung {n}/{total}"),
                _ => String::new(),
            };
            out.push(format!("{}. {}{rung}  {} kills  {} deaths", i + 1, name(r), r.kills, r.deaths));
        }
        out
    }
}

/// The results of the match as it stands.
pub fn collect(world: &mut World) -> Option<MatchResults> {
    let active = *world.get_resource::<ActiveMinigame>()?;
    let m = world.resource::<Match>().clone();
    let on_ladder = active.rules.mode.id == ladder::LADDER;
    let rungs = if on_ladder { world.get_resource::<ladder::Ladder>().map_or(0, |l| l.rungs.len() as u32) } else { 0 };
    let mut rows: Vec<(usize, ResultRow)> = runtime::players(world)
        .into_iter()
        .enumerate()
        .map(|(order, (e, team))| {
            let s = world.get::<Score>(e).copied().unwrap_or_default();
            let name = world.get::<Name>(e).map_or_else(|| format!("Player {}", order + 1), |n| n.as_str().to_string());
            let rung = on_ladder.then(|| world.get::<ladder::Rung>(e).map_or(0, |r| r.0 as u32) + 1);
            (order, ResultRow {
                name,
                kills: s.kills,
                deaths: s.deaths,
                rung,
                winner: m.result.flatten() == Some(team),
                player: Some(e),
            })
        })
        .collect();
    rows.sort_by(|(oa, a), (ob, b)| {
        b.winner
            .cmp(&a.winner)
            .then(b.rung.cmp(&a.rung))
            .then(b.kills.cmp(&a.kills))
            .then(a.deaths.cmp(&b.deaths))
            .then(oa.cmp(ob))
    });
    Some(MatchResults {
        title: active.def.name.clone(),
        rungs,
        rows: rows.into_iter().map(|(_, r)| r).collect(),
    })
}

/// Keeps `MatchResults` in step with the match: collected when it has its
/// result, gone when it has none (a new match).
pub(super) fn record(world: &mut World) {
    let decided = world.get_resource::<Match>().is_some_and(|m| m.result.is_some());
    let have = world.contains_resource::<MatchResults>();
    if decided && !have {
        if let Some(r) = collect(world) {
            world.insert_resource(r);
        }
    } else if !decided && have {
        world.remove_resource::<MatchResults>();
    }
}
