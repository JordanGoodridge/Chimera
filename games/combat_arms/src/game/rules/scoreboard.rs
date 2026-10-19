//! The scoreboard's data (specs/combat_arms/game_modes.md S13): one row
//! per player (name, kills, deaths, assists, ping), Alpha's in the upper
//! list and every other team's in the lower, each list sorted by kills,
//! then fewer deaths (the team modes' order); per team the score panel's
//! number. Read from what every machine has (`CaStats`, replicated, and
//! the characters' names), so a network client builds the same board; the
//! front end draws it (`ui::scoreboard`).
//!
//! Known shortcuts: no level icon, clan emblem or mode column (TDM and
//! Elimination Pro have none to fill: their mission points are 0); ties in
//! kills and deaths keep the server's player order (entities), as a
//! stable sort of its list would; ping is the network's (`net::NetScore`)
//! when there is one.

use bevy::prelude::*;

use super::state::{ALPHA, CaStats};
use crate::core::{Npc, Team};

/// One player's row.
#[derive(Clone, Debug, PartialEq)]
pub struct ScoreRow {
    pub player: Entity,
    pub name: String,
    pub team: u8,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    /// Milliseconds, when the network knows it.
    pub ping: Option<u32>,
}

/// S13's default order: kills, then fewer deaths (stable).
pub fn sort(rows: &mut [ScoreRow]) {
    rows.sort_by(|a, b| b.kills.cmp(&a.kills).then(a.deaths.cmp(&b.deaths)));
}

/// The board: Alpha's list and the others', each sorted.
pub fn board(mut rows: Vec<ScoreRow>) -> [Vec<ScoreRow>; 2] {
    rows.sort_by_key(|r| r.player);
    let (mut alpha, mut rest): (Vec<_>, Vec<_>) = rows.into_iter().partition(|r| r.team == ALPHA);
    sort(&mut alpha);
    sort(&mut rest);
    [alpha, rest]
}

/// Every player's row as this machine knows it.
pub fn rows(world: &mut World) -> Vec<ScoreRow> {
    world
        .query_filtered::<(Entity, &Team, Option<&CaStats>, Option<&Name>, Option<&crate::net::NetScore>), (
            With<crate::core::Health>,
            Without<Npc>,
        )>()
        .iter(world)
        .filter(|(_, t, ..)| super::state::side(t.0).is_some())
        .map(|(e, t, s, n, net)| {
            let s = s.copied().unwrap_or_default();
            ScoreRow {
                player: e,
                name: n.map_or_else(|| format!("Player {}", e.index()), |n| n.to_string()),
                team: t.0,
                kills: s.kills,
                deaths: s.deaths,
                assists: s.assists,
                ping: net.map(|n| n.ping as u32),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(i: u32, team: u8, kills: u32, deaths: u32) -> ScoreRow {
        ScoreRow {
            player: Entity::from_raw_u32(i + 1).unwrap(),
            name: format!("p{i}"),
            team,
            kills,
            deaths,
            assists: 0,
            ping: None,
        }
    }

    /// S13: Alpha's list on top; kills, then fewer deaths.
    #[test]
    fn sorted_by_kills_then_fewer_deaths() {
        let [alpha, rest] = board(vec![
            row(0, 1, 3, 5),
            row(1, 2, 7, 1),
            row(2, 1, 3, 2),
            row(3, 1, 9, 9),
        ]);
        let names = |l: &[ScoreRow]| l.iter().map(|r| r.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&alpha), ["p3", "p2", "p0"]);
        assert_eq!(names(&rest), ["p1"]);
    }
}
