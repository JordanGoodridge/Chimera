//! The scoreboard's data (specs/combat_arms/game_modes.md S13): one row
//! per player (name, kills, deaths, assists, the mode's column, ping),
//! Alpha's in the upper list and every other team's in the lower (a
//! free-for-all mode: everyone in one list), each list in the mode's
//! order (`ScoreOrder`): kills then fewer deaths (the default); captures,
//! returns, kills, fewer deaths (capture the flag); mission points, kills,
//! fewer deaths (Spy Hunt, Last Man Standing, Seize and Secure, VIP
//! Elimination, Hunted); mission points only (Quarantine). Per team the
//! score panel's number. Read from what every machine has (`CaStats`,
//! replicated, and the characters' names), so a network client builds the
//! same board; the front end draws it (`ui::scoreboard`).
//!
//! Known shortcuts: no level icon or clan emblem; the mode column is the
//! captures (capture the flag) or the mission points (the modes ordered
//! by them), none otherwise; ties keep the server's player order
//! (entities), as a stable sort of its list would; VIP Elimination's VIP
//! rank and Hunted's time hunted aren't columns (no VIP data, no time
//! kept); ping is the network's (`net::NetScore`) when there is one.

use bevy::prelude::*;

use super::{
    ScoreOrder,
    state::{ALPHA, CaStats},
};
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
    pub captures: u32,
    pub returns: u32,
    pub mission_points: i32,
    /// Milliseconds, when the network knows it.
    pub ping: Option<u32>,
}

impl ScoreRow {
    /// The mode's column in `order`, if it has one.
    pub fn mode_column(&self, order: ScoreOrder) -> Option<i64> {
        match order {
            ScoreOrder::Kills => None,
            ScoreOrder::Captures => Some(self.captures as i64),
            ScoreOrder::MissionPoints | ScoreOrder::MissionOnly => Some(self.mission_points as i64),
        }
    }
}

/// S13's order for `order` (stable).
pub fn sort_by(rows: &mut [ScoreRow], order: ScoreOrder) {
    let kills = |a: &ScoreRow, b: &ScoreRow| b.kills.cmp(&a.kills).then(a.deaths.cmp(&b.deaths));
    match order {
        ScoreOrder::Kills => rows.sort_by(kills),
        ScoreOrder::Captures => rows.sort_by(|a, b| {
            b.captures
                .cmp(&a.captures)
                .then(b.returns.cmp(&a.returns))
                .then(kills(a, b))
        }),
        ScoreOrder::MissionPoints => rows.sort_by(|a, b| b.mission_points.cmp(&a.mission_points).then(kills(a, b))),
        ScoreOrder::MissionOnly => rows.sort_by(|a, b| b.mission_points.cmp(&a.mission_points)),
    }
}

/// S13's default order: kills, then fewer deaths (stable).
pub fn sort(rows: &mut [ScoreRow]) {
    sort_by(rows, ScoreOrder::Kills);
}

/// The board: Alpha's list and the others', each sorted by kills.
pub fn board(rows: Vec<ScoreRow>) -> [Vec<ScoreRow>; 2] {
    board_for(rows, ScoreOrder::Kills, false)
}

/// The board in `order`: Alpha's list and the others' (`one_list`: every
/// row in the first).
pub fn board_for(mut rows: Vec<ScoreRow>, order: ScoreOrder, one_list: bool) -> [Vec<ScoreRow>; 2] {
    rows.sort_by_key(|r| r.player);
    let (mut alpha, mut rest): (Vec<_>, Vec<_>) = rows.into_iter().partition(|r| one_list || r.team == ALPHA);
    sort_by(&mut alpha, order);
    sort_by(&mut rest, order);
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
        .filter(|(_, t, ..)| super::state::plays(t.0))
        .map(|(e, t, s, n, net)| {
            let s = s.copied().unwrap_or_default();
            ScoreRow {
                player: e,
                name: n.map_or_else(|| format!("Player {}", e.index()), |n| n.to_string()),
                team: t.0,
                kills: s.kills,
                deaths: s.deaths,
                assists: s.assists,
                captures: s.captures,
                returns: s.returns,
                mission_points: s.mission_points,
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
            captures: 0,
            returns: 0,
            mission_points: 0,
            ping: None,
        }
    }

    fn names(l: &[ScoreRow]) -> Vec<String> {
        l.iter().map(|r| r.name.clone()).collect()
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
        assert_eq!(names(&alpha), ["p3", "p2", "p0"]);
        assert_eq!(names(&rest), ["p1"]);
    }

    /// S13: capture the flag by captures, returns, kills; the mission
    /// point modes by points, then kills.
    #[test]
    fn mode_orders() {
        let mut a = row(0, 1, 9, 0);
        let mut b = row(1, 1, 1, 0);
        b.captures = 2;
        let mut c = row(2, 1, 5, 0);
        c.captures = 2;
        c.returns = 1;
        let [l, _] = board_for(vec![a.clone(), b.clone(), c.clone()], ScoreOrder::Captures, false);
        assert_eq!(names(&l), ["p2", "p1", "p0"]);
        a.mission_points = 1;
        let [l, _] = board_for(vec![a, b, c], ScoreOrder::MissionPoints, true);
        assert_eq!(names(&l), ["p0", "p2", "p1"]);
    }
}
