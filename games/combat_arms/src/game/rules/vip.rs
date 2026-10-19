//! VIP Elimination (`combat_arms:vip_elimination`, room key `VTDM`;
//! specs/combat_arms/game_modes.md M12): Team Death Match (M1, the `tdm`
//! rules) in which a death is worth the points of the victim's VIP rank
//! (`vip_points`, by `CaVipRank`): the team counter is the sum, the first
//! death that brings a team to G points ends the round. The killer's
//! personal VIP points (mission points) are shown on the scoreboard.
//!
//! Known shortcuts: the VIP data isn't in the spec: every rank is worth
//! 1 point (PLACEHOLDER, data/modes.toml) and nothing gives a player a
//! rank (`CaVipRank` 0 unless set); the scoreboard ranks by mission
//! points (S13's "VIP rank, points" as one).

use bevy::prelude::*;

use super::{
    CaMode, CaRuleSettings, ScoreOrder, VIP_ELIMINATION,
    scoring::{personal, stats},
    state::{Death, DeathKind},
    tdm,
};

/// A player's VIP rank (an index into `vip_points`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CaVipRank(pub u8);

/// The mode.
pub const MODE: CaMode = CaMode {
    module: VIP_ELIMINATION,
    key: "VTDM",
    shown_as: "VIP Elimination",
    description: "VIP Elimination: Team Death Match where a death is worth the victim's VIP rank's points, first team \
                  to G points wins",
    order: ScoreOrder::MissionPoints,
    on_death,
    ..tdm::MODE
};

/// The points `e`'s death is worth.
pub fn worth(world: &World, e: Entity) -> i32 {
    let rank = world.get::<CaVipRank>(e).map_or(0, |r| r.0) as usize;
    let table = &world.resource::<CaRuleSettings>().objectives.vip_points;
    table.get(rank).or(table.last()).copied().unwrap_or(1)
}

fn on_death(world: &mut World, d: &Death) {
    personal(world, d);
    let points = worth(world, d.victim);
    if d.kind == DeathKind::Enemy
        && let Some(k) = d.killer
    {
        stats(world, k).mission_points += points;
    }
    tdm::add_and_test(world, d.victim_team, points);
}
