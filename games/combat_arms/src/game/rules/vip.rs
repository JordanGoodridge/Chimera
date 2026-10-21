//! VIP Elimination (`combat_arms:vip_elimination`, room key `VTDM`;
//! specs/combat_arms/game_modes.md M12, missions.md F4.7): Team Death
//! Match (M1, the `tdm` rules) in which a death is worth the points of the
//! victim's VIP rank: the team counter is the sum, the first death that
//! brings a team to G points ends the round.
//!
//! - The VIP data (`missions::vip`: the install's `GMS\VTDM.GMS`, else our
//!   copy of its stock values): rank weights 1, 3, 5, 7 (none, bronze,
//!   silver, gold) and the VIP count of a team of n players from the first
//!   table whose `TargetPoint` ≥ G (every stock goal uses the 1000 one: 1
//!   player 0 VIPs, 2-4 one, 5-7 two, 8 or more three).
//! - Ranks (`rank_team`): each team's players in order of VIP points (the
//!   scoreboard's mission points, more first), then name; the top k (the
//!   VIP count) get ranks k, k - 1, ..., 1; everyone else 0. Recomputed
//!   for a team whenever its score rises and when a player joins it.
//! - A death adds the victim's weight (its rank before any re-ranking) to
//!   the victim's enemy team, whatever the kind of death (TDM's rule); an
//!   enemy kill adds it to the killer's VIP points too.
//!
//! Known shortcuts: the ranking's experience step is skipped (no player
//! levels); the VIP list isn't sent as its own message (`CaVipRank` is a
//! component on each player).

use bevy::prelude::*;

use super::{
    super::missions, CaMode, CaRuleSettings, CaStats, ScoreOrder, VIP_ELIMINATION,
    flow::players,
    scoring::{personal, stats},
    state::{Death, DeathKind, enemy},
    tdm,
};

/// A player's VIP rank (0 none, 1 bronze, 2 silver, 3 gold).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CaVipRank(pub u8);

/// The members each team was last ranked with (a joiner re-ranks).
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct CaVipTeams(pub [Vec<Entity>; 2]);

/// The mode.
pub const MODE: CaMode = CaMode {
    module: VIP_ELIMINATION,
    key: "VTDM",
    shown_as: "VIP Elimination",
    description: "VIP Elimination: Team Death Match where a death is worth the victim's VIP rank's points, first team \
                  to G points wins",
    order: ScoreOrder::MissionPoints,
    on_death,
    hooks: super::Hooks {
        update,
        ..super::NO_HOOKS
    },
    ..tdm::MODE
};

/// The points `e`'s death is worth.
pub fn worth(world: &World, e: Entity) -> i32 {
    let rank = world.get::<CaVipRank>(e).map_or(0, |r| r.0) as usize;
    missions::vip(world).weight(rank)
}

/// Rank `team`'s players (M12): by VIP points, then name; the top k of
/// the table's count get k .. 1.
pub fn rank_team(world: &mut World, team: u8) {
    let goal = world.resource::<CaRuleSettings>().goal() as i32;
    let data = missions::vip(world);
    let mut members: Vec<(Entity, i32, String)> = players(world)
        .into_iter()
        .filter(|(_, t)| *t == team)
        .map(|(e, _)| {
            let points = world.get::<CaStats>(e).map_or(0, |s| s.mission_points);
            let name = world.get::<Name>(e).map_or_else(String::new, |n| n.as_str().to_string());
            (e, points, name)
        })
        .collect();
    members.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.2.cmp(&b.2)));
    let k = data.vips(goal, members.len());
    for (i, (e, _, _)) in members.iter().enumerate() {
        let rank = if i < k { (k - i) as u8 } else { 0 };
        world.entity_mut(*e).insert(CaVipRank(rank));
    }
    if let Some(i) = super::state::side(team) {
        let list = members.iter().map(|m| m.0).collect();
        world.get_resource_or_init::<CaVipTeams>().0[i] = list;
    }
}

fn on_death(world: &mut World, d: &Death) {
    personal(world, d);
    let points = worth(world, d.victim);
    if d.kind == DeathKind::Enemy
        && let Some(k) = d.killer
    {
        stats(world, k).mission_points += points;
    }
    let before = world.resource::<super::CaMatch>().score;
    tdm::add_and_test(world, d.victim_team, points);
    if world.resource::<super::CaMatch>().score != before {
        rank_team(world, enemy(d.victim_team));
    }
}

/// A team that gained a player is ranked again.
fn update(world: &mut World) {
    let known = world.get_resource_or_init::<CaVipTeams>().clone();
    for (i, team) in [super::state::ALPHA, super::state::BRAVO].into_iter().enumerate() {
        let now: Vec<Entity> = players(world).into_iter().filter(|(_, t)| *t == team).map(|(e, _)| e).collect();
        if now.iter().any(|e| !known.0[i].contains(e)) {
            rank_team(world, team);
        }
    }
}
