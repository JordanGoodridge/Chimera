//! Bombing Run (`combat_arms:bombing_run`, room key `SBT`;
//! specs/combat_arms/game_modes.md M9): one shared bomb, Alpha against
//! Bravo, respawning; a warm-up, then rounds of `TimeLimit` 300 s; G round
//! wins take the match.
//!
//! - The bomb: at each real round's setup it lies on the first of the
//!   shuffled `Sabotage` drop points (S12); either team's living player
//!   takes it by touch (the mission bomb, `objectives::BOMB`, given to the
//!   carrier: `CaCarry::bomb`) and plants it at the enemy's site (a
//!   `Sabotage` `BombZoneTrigger` whose `DefusableTeam` is the enemy;
//!   `bomb`); a carrier's death drops it where they were (on the floor),
//!   a carrier who leaves drops it where they last were.
//! - Round win: the detonation, the planting team; a defuse, the other.
//! - Time-out: both teams get a round win (Quirk 7), the round no winner.
//! - Match end: a team has G round wins or a team is empty; the winner
//!   more round wins, if equal the last round's winner (none after a
//!   time-out).
//! - Deaths: S7's personal counts (`scoring::personal`; the mode has no
//!   column of its own). Friendly fire on is full damage (S8).
//!
//! Known shortcuts: the spawn family isn't in the spec (the `Team Death
//! Match` points stand in); the bomb on the ground isn't drawn (no item
//! model; the HUD shows where it is); its pick-up box is the flag's
//! default (10, 25, 10); a map without `Sabotage` drop points has no bomb.

use bevy::prelude::*;

use super::{
    BOMBING_RUN, CaMode, CaRuleSettings, Hooks, NO_HOOKS, Panel, RoundTime, ScoreOrder, TEAM_POINTS, Teams, bomb,
    objectives::{self, BOMB, CaObjectives},
    objects::{CaMapObjects, FLAG_DIMS, ObjBox},
    scoring::personal,
    state::{ALPHA, BRAVO, CaMatch, side},
};
use crate::core::{Dead, Died};

/// The drop points' and sites' game type.
pub const FAMILY: &str = "Sabotage";

/// The mode.
pub const MODE: CaMode = CaMode {
    module: BOMBING_RUN,
    key: "SBT",
    shown_as: "Bombing Run",
    description: "Bombing Run: one bomb for both teams, respawning; plant it at the enemy's site, G round wins take the \
                  match",
    respawns: true,
    warmup: true,
    warmup_revives: true,
    ff_scaled: false,
    assists: false,
    timer_shown: true,
    forced_setup: None,
    teams: Teams::Two,
    spawns: TEAM_POINTS,
    round_time: RoundTime::Mission,
    fire_ends_protection: false,
    death_drops: true,
    panel: Panel::RoundWins,
    order: ScoreOrder::Kills,
    on_death: |w, d| personal(w, d),
    time_out,
    match_end,
    leaving: |_, _| {},
    hooks: Hooks {
        setup,
        update,
        died,
        round_end: |w| {
            bomb::remove_all(w);
            w.resource_mut::<CaCarrier>().0 = None;
        },
        ..NO_HOOKS
    },
};

/// The bomb's carrier and where they last were alive (the server's).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct CaCarrier(pub Option<(Entity, Vec3)>);

/// A real round's setup: the bomb on the first shuffled drop point.
fn setup(world: &mut World) {
    world.resource_mut::<CaCarrier>().0 = None;
    world.resource_mut::<bomb::CaPlanter>().0 = None;
    if world.resource::<CaMatch>().warmup {
        return;
    }
    let mut points: Vec<Vec3> = world
        .resource::<CaMapObjects>()
        .drops
        .iter()
        .filter(|d| d.family == FAMILY)
        .map(|d| d.at)
        .collect();
    objectives::shuffle(world, &mut points);
    world.resource_mut::<CaObjectives>().loose_bomb = points.first().copied();
}

/// Drop the bomb at `at` (on the floor).
fn drop_bomb(world: &mut World, carrier: Entity, at: Vec3) {
    objectives::take_weapon(world, carrier, BOMB);
    let mut c = objectives::carry(world, carrier);
    c.bomb = false;
    objectives::set_carry(world, carrier, c);
    world.resource_mut::<CaCarrier>().0 = None;
    let floor = objectives::floor_below(world, at);
    world.resource_mut::<CaObjectives>().loose_bomb = Some(floor);
}

/// The carrier's death drops the bomb.
fn died(world: &mut World, d: &Died) {
    if !objectives::carry(world, d.entity).bomb {
        return;
    }
    let at = world.get::<Transform>(d.entity).map_or(Vec3::ZERO, |t| t.translation);
    drop_bomb(world, d.entity, at);
}

/// The carrier kept (a leaver drops it), the pick-up, then the sites.
fn update(world: &mut World) {
    if let Some((e, last)) = world.resource::<CaCarrier>().0 {
        match world.get::<Transform>(e) {
            Some(t) if world.get::<Dead>(e).is_none() => {
                let at = t.translation;
                world.resource_mut::<CaCarrier>().0 = Some((e, at));
            }
            Some(_) => {}
            None => {
                world.resource_mut::<CaCarrier>().0 = None;
                let floor = objectives::floor_below(world, last);
                world.resource_mut::<CaObjectives>().loose_bomb = Some(floor);
            }
        }
    }
    if let Some(at) = world.resource::<CaObjectives>().loose_bomb
        && world.resource::<CaMatch>().counting()
    {
        let area = ObjBox {
            centre: at,
            half: FLAG_DIMS * super::super::inventory::UNIT,
        };
        let half = objectives::player_half();
        let taker = objectives::takers(world)
            .into_iter()
            .find(|p| side(p.team).is_some() && area.touches(p.at, half));
        if let Some(p) = taker
            && world.get::<crate::weapon::Inventory>(p.e).is_some_and(|i| !i.weapons.is_empty())
        {
            world.resource_mut::<CaObjectives>().loose_bomb = None;
            objectives::give_weapon(world, p.e, BOMB);
            let mut c = objectives::carry(world, p.e);
            c.bomb = true;
            objectives::set_carry(world, p.e, c);
            world.resource_mut::<CaCarrier>().0 = Some((p.e, p.at));
        }
    }
    bomb::update(world, FAMILY);
}

/// Both teams +1 round win, no winner.
fn time_out(world: &mut World) {
    let mut m = world.resource_mut::<CaMatch>();
    m.round_wins[0] += 1;
    m.round_wins[1] += 1;
    m.winner = None;
}

/// G round wins or an empty team: more wins, else the last round's winner.
fn match_end(world: &World) -> Option<Option<u8>> {
    let goal = world.resource::<CaRuleSettings>().goal();
    let m = world.resource::<CaMatch>();
    let counts = world.resource::<super::CaTeamCounts>().0;
    if !counts.contains(&0) && m.round_wins.iter().all(|w| *w < goal) {
        return None;
    }
    Some(match m.round_wins[0].cmp(&m.round_wins[1]) {
        std::cmp::Ordering::Greater => Some(ALPHA),
        std::cmp::Ordering::Less => Some(BRAVO),
        std::cmp::Ordering::Equal => m.results.last().and_then(|r| r.winner),
    })
}
