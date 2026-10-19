//! Spy Hunt (`combat_arms:spy_hunt`, room key `BNT`;
//! specs/combat_arms/game_modes.md M7): an intel hunt in two stages, one
//! round of G minutes after a warm-up.
//!
//! - Stage 1 (respawning): everybody starts a hunter (Alpha). At the
//!   round's start `intel_count` (5) intel items lie on the first shuffled
//!   `BountyHunter` drop points (S12). A living player touching one picks
//!   it up (`objectives::CaCarry::intel`, mission points +1) and becomes a
//!   spy with a team of their own (S1's free-for-all ids), so everyone may
//!   shoot them; a spy's death drops their intel where they were (on the
//!   floor) and makes them a hunter again.
//! - Stage 2: the player holding all the intel becomes the final spy
//!   (Bravo), everyone else a hunter; every dead player is revived, every
//!   living one's health refilled; respawning stops. The hunters win by
//!   killing the final spy, the final spy when no hunter is left alive.
//! - Deaths: One Man Army's counts (`scoring::free_for_all`).
//! - Leaving: in stage 2 the last of a side leaving gives the round to the
//!   other; in stage 1 the round ends (no winner) when one player would be
//!   left.
//! - Spawns: the `BountyHunter` points, one list.
//!
//! Known shortcuts: the stage-2 time (`Stage2PlayTime`) and the time-out's
//! winner weren't traced (Open question 10): the round runs its G minutes
//! and a time-out has no winner; the refill at stage 2 is health only (no
//! ammunition); intel on the ground isn't drawn (no item model; the HUD
//! says how many lie about); its pick-up box is the flag's default.

use bevy::prelude::*;

use super::{
    CaMode, CaRuleSettings, Hooks, Leaver, NO_HOOKS, Panel, RoundTime, SPY_HUNT, ScoreOrder, Spawns, Teams,
    flow::{alive, players},
    objectives::{self, CaObjectives},
    objects::{CaMapObjects, FLAG_DIMS, ObjBox},
    scoring::free_for_all,
    spawns,
    state::{ALPHA, BRAVO, CaMatch, Death, FFA_FIRST, FFA_LAST, Phase},
};
use crate::core::{Dead, Died, Health, Team};

/// The intel drop points' and the start points' game type.
pub const FAMILY: &str = "BountyHunter";

/// The mode.
pub const MODE: CaMode = CaMode {
    module: SPY_HUNT,
    key: "BNT",
    shown_as: "Spy Hunt",
    description: "Spy Hunt: pick up intel to become a spy; whoever holds it all is the final spy, hunted by everyone \
                  else",
    respawns: true,
    warmup: true,
    warmup_revives: false,
    ff_scaled: false,
    assists: false,
    timer_shown: true,
    forced_setup: None,
    teams: Teams::AllAlpha,
    spawns: Spawns {
        family: Some(FAMILY),
        one_list: true,
        shuffled: false,
        locked_rounds: false,
    },
    round_time: RoundTime::GoalMinutes,
    fire_ends_protection: false,
    death_drops: true,
    panel: Panel::Kills,
    order: ScoreOrder::MissionPoints,
    on_death,
    time_out: |_| {},
    match_end: |w| Some(w.resource::<CaMatch>().winner),
    leaving,
    hooks: Hooks {
        setup,
        update,
        died,
        may_respawn: |w, _| w.resource::<CaObjectives>().stage != 2,
        ..NO_HOOKS
    },
};

/// The intel at the round's start (stage 1).
fn setup(world: &mut World) {
    let warmup = world.resource::<CaMatch>().warmup;
    let count = world.resource::<CaRuleSettings>().objectives.intel_count as usize;
    let mut points: Vec<Vec3> = world
        .resource::<CaMapObjects>()
        .drops
        .iter()
        .filter(|d| d.family == FAMILY)
        .map(|d| d.at)
        .collect();
    objectives::shuffle(world, &mut points);
    let mut o = world.resource_mut::<CaObjectives>();
    o.stage = if warmup { 0 } else { 1 };
    o.intel = if warmup { Vec::new() } else { points.into_iter().take(count).collect() };
}

/// A free-for-all id nobody has.
fn free_id(world: &mut World) -> Option<u8> {
    let used: Vec<u8> = players(world).into_iter().map(|(_, t)| t).collect();
    (FFA_FIRST..=FFA_LAST).find(|t| !used.contains(t))
}

/// A spy's death drops their intel and makes them a hunter.
fn died(world: &mut World, d: &Died) {
    let c = objectives::carry(world, d.entity);
    if c.intel == 0 || world.resource::<CaObjectives>().stage != 1 {
        return;
    }
    let at = world.get::<Transform>(d.entity).map_or(Vec3::ZERO, |t| t.translation);
    let floor = objectives::floor_below(world, at);
    {
        let mut o = world.resource_mut::<CaObjectives>();
        o.intel.extend(std::iter::repeat_n(floor, c.intel as usize));
    }
    let mut c = c;
    c.intel = 0;
    objectives::set_carry(world, d.entity, c);
    world.entity_mut(d.entity).insert(Team(ALPHA));
}

/// Pick-ups, then stage 2 when one player holds it all.
fn update(world: &mut World) {
    if !world.resource::<CaMatch>().counting() || world.resource::<CaObjectives>().stage != 1 {
        return;
    }
    let total = world.resource::<CaRuleSettings>().objectives.intel_count as u8;
    let half = objectives::player_half();
    for p in objectives::living(world) {
        let items = world.resource::<CaObjectives>().intel.clone();
        let Some(i) = items.iter().position(|at| {
            ObjBox {
                centre: *at,
                half: FLAG_DIMS * super::super::inventory::UNIT,
            }
            .touches(p.at, half)
        }) else {
            continue;
        };
        world.resource_mut::<CaObjectives>().intel.remove(i);
        let mut c = objectives::carry(world, p.e);
        c.intel += 1;
        objectives::set_carry(world, p.e, c);
        super::scoring::stats(world, p.e).mission_points += 1;
        if p.team == ALPHA
            && let Some(id) = free_id(world)
        {
            world.entity_mut(p.e).insert(Team(id));
        }
        if c.intel >= total {
            stage_two(world, p.e);
            return;
        }
    }
}

/// `spy` holds all the intel: the final spy against everyone else.
fn stage_two(world: &mut World, spy: Entity) {
    world.resource_mut::<CaObjectives>().stage = 2;
    world.resource_mut::<CaObjectives>().intel.clear();
    for (e, _) in players(world) {
        world.entity_mut(e).insert(Team(if e == spy { BRAVO } else { ALPHA }));
        if world.get::<Dead>(e).is_some() {
            spawns::respawn(world, e);
        } else if let Some(mut h) = world.get_mut::<Health>(e) {
            h.current = h.max;
        }
    }
    info!("Spy Hunt stage 2: the final spy is {spy:?}");
}

fn on_death(world: &mut World, d: &Death) {
    free_for_all(world, d);
    if world.resource::<CaObjectives>().stage != 2 || world.resource::<CaMatch>().winner.is_some() {
        return;
    }
    if alive(world, BRAVO) == 0 {
        world.resource_mut::<CaMatch>().win(ALPHA);
    } else if alive(world, ALPHA) == 0 {
        world.resource_mut::<CaMatch>().win(BRAVO);
    }
}

/// Stage 2: the last of a side leaving gives the round to the other;
/// stage 1: one player left ends the round with no winner.
fn leaving(m: &mut CaMatch, l: &Leaver) {
    if m.phase != Phase::Live || m.warmup || m.winner.is_some() {
        return;
    }
    let stage_two = l.team == BRAVO || l.others.contains(&BRAVO);
    if stage_two {
        if l.team_players <= 1 {
            m.win(if l.team == BRAVO { ALPHA } else { BRAVO });
        }
    } else if l.players <= 2 {
        m.end_without_winner();
    }
}
