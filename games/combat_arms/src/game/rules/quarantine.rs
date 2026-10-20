//! Quarantine Regen (`combat_arms:quarantine`, room key `QRT`;
//! specs/combat_arms/game_modes.md M8): infection rounds. Everyone starts
//! human (Alpha); `host_delay` (20 s) after a round goes live, H living
//! humans become infected hosts (Bravo); the infected spread it. A warm-up
//! first, then G rounds played of `TimeLimit` 3 minutes.
//!
//! - Hosts: H = 1 with N <= 3 players on teams, else max(round(N x
//!   `SelectionRate`), `MinHostNum`) (`hosts`), chosen at random among
//!   the living humans, only in real rounds (never in the warm-up); with H
//!   or fewer humans alive then, the round is void (no winner).
//! - Infected (`infect`): on Bravo, `movement::CaInfected` (S14's
//!   knock-back), the mission's infected weapons (a host's: the host's;
//!   stock 191 and 190, our documents for them with the V1 key), else our
//!   knife; after `WaitingTIme` (3 s) standing exactly still they heal
//!   `Heal` (0.1) of their maximum hit points every `TickTime` (1 s), the
//!   first at once when the wait ends; a move, a hit or firing restarts
//!   the wait (Open question 12, answered).
//! - Deaths: S7's personal counts (`scoring::personal`) and the mission
//!   points (`[ExpAndGPSetting]`): a human killing an infected +3, the
//!   infected killed -1; an infected killing a human +2, the human 0, and
//!   that human is back at once where it died (on the floor below), at
//!   full health, infected (M8).
//! - Round win: no human alive, the infected; after the selection, no
//!   infected alive, the humans. Time-out: the humans. The living members
//!   of the winning side get +10 (humans) or +5 (infected) at the round's
//!   end. Match end: after G rounds played, with no winner (players rank
//!   by mission points).
//! - Spawns: the `Quarantine` points, one list (all `JSF` in the stock
//!   maps). No respawn request.
//!
//! The numbers are the mission's (`missions::quarantine`) or, without the
//! install's, our data's.
//!
//! Known shortcuts: the infected keep their body (the model record's
//! infection override isn't loaded) and no change effect or sound plays;
//! the infected's live projectiles aren't removed (`[DestroyProjectiles]`
//! is read, not applied); any enemy kill by an infected infects (the
//! infected hold only their infection weapon).

use bevy::prelude::*;

use super::{
    CaMode, CaRuleSettings, Hooks, NO_HOOKS, Panel, QUARANTINE, RoundTime, ScoreOrder, Spawns, Teams,
    flow::{alive, players},
    objectives,
    scoring::{personal, stats},
    spawns,
    state::{ALPHA, BRAVO, CaMatch, Death, DeathKind},
};
use crate::core::{Health, SimClock, Team};

/// The mode.
pub const MODE: CaMode = CaMode {
    module: QUARANTINE,
    key: "QRT",
    shown_as: "Quarantine Regen",
    description: "Quarantine Regen: humans against the infected they become; hosts picked 20 s in, G rounds played",
    respawns: false,
    warmup: true,
    warmup_revives: false,
    ff_scaled: false,
    assists: false,
    timer_shown: true,
    forced_setup: None,
    teams: Teams::AllAlpha,
    spawns: Spawns {
        family: Some("Quarantine"),
        one_list: true,
        shuffled: false,
        locked_rounds: false,
    },
    round_time: RoundTime::Mission,
    fire_ends_protection: false,
    death_drops: true,
    panel: Panel::RoundWins,
    order: ScoreOrder::MissionOnly,
    on_death,
    time_out: |w| w.resource_mut::<CaMatch>().winner = Some(ALPHA),
    match_end: |w| (w.resource::<CaMatch>().round >= w.resource::<CaRuleSettings>().goal()).then_some(None),
    leaving: |_, _| {},
    hooks: Hooks {
        setup,
        update,
        round_over,
        ..NO_HOOKS
    },
};

/// The round's infection state (the server's).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct CaQuarantine {
    /// When the round went live.
    pub live_since: Option<f64>,
    /// The hosts were picked.
    pub picked: bool,
}

/// An infected player's stillness (the heal's clock): where it stood,
/// since when (its last move, hit or fire), the next heal, its health then.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct CaStill {
    pub at: Vec3,
    pub since: f64,
    pub next_heal: f64,
    pub health: f32,
}

/// An infected player waiting for its loadout to become the infected's
/// (a host's when `host`).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct CaInfectedKit {
    pub host: bool,
}

/// M8's host count for `n` players on teams.
pub fn hosts(n: usize, rate: f32, min: u32) -> usize {
    if n <= 3 {
        1
    } else {
        ((n as f32 * rate).round() as usize).max(min as usize)
    }
}

/// Everyone human again at the round's setup.
fn setup(world: &mut World) {
    *world.resource_mut::<CaQuarantine>() = CaQuarantine::default();
    for (e, _) in players(world) {
        world
            .entity_mut(e)
            .remove::<(super::super::movement::CaInfected, CaStill, CaInfectedKit)>();
    }
}

/// `e` becomes infected (a host when `host`): Bravo, the mission's
/// infected (or host) weapons, else our knife.
pub fn infect(world: &mut World, e: Entity, host: bool) {
    let now = world.resource::<SimClock>().now;
    let at = world.get::<Transform>(e).map_or(Vec3::ZERO, |t| t.translation);
    let health = world.get::<Health>(e).map_or(0.0, |h| h.current);
    world.entity_mut(e).insert((
        Team(BRAVO),
        super::super::movement::CaInfected,
        CaStill {
            at,
            since: now,
            next_heal: now,
            health,
        },
        CaInfectedKit { host },
    ));
}

/// M8 (Open question 12, answered): a human killed by an infected is
/// back at once where it died, on the floor below, at full health.
fn revive_in_place(world: &mut World, e: Entity) {
    let Some(t) = world.get::<Transform>(e).copied() else { return };
    let half = world
        .get::<crate::core::MovementState>(e)
        .map_or(0.0, |s| -s.hull_min.y)
        .max(0.0);
    let floor = objectives::floor_below(world, t.translation - Vec3::Y * half);
    let at = Transform::from_translation(floor + Vec3::Y * half).with_rotation(t.rotation);
    super::super::inventory::respawn_inventory(world, e);
    crate::spawning::revive(world, e, Some(at));
    world.entity_mut(e).remove::<(spawns::CaAsked, spawns::CaHeld)>();
}

/// Host selection, the infected's kit and heal, the win tests.
fn update(world: &mut World) {
    let now = world.resource::<SimClock>().now;
    let o = world.resource::<CaRuleSettings>().objectives.clone();
    let counting = world.resource::<CaMatch>().counting();
    if !counting {
        return;
    }
    let n = super::super::missions::quarantine(world);
    let state = *world.resource::<CaQuarantine>();
    let since = match state.live_since {
        Some(s) => s,
        None => {
            world.resource_mut::<CaQuarantine>().live_since = Some(now);
            now
        }
    };
    if !state.picked && now + 1e-9 >= since + o.host_delay as f64 {
        world.resource_mut::<CaQuarantine>().picked = true;
        let h = hosts(players(world).len(), n.selection_rate, n.min_hosts);
        let mut humans: Vec<Entity> = objectives::living(world)
            .into_iter()
            .filter(|p| p.team == ALPHA)
            .map(|p| p.e)
            .collect();
        if humans.len() <= h {
            world.resource_mut::<CaMatch>().end_without_winner();
            return;
        }
        objectives::shuffle(world, &mut humans);
        for e in humans.into_iter().take(h) {
            infect(world, e, true);
        }
    }
    // The infected's weapons only, once the loadout is there.
    let kits: Vec<(Entity, CaInfectedKit)> = world.query::<(Entity, &CaInfectedKit)>().iter(world).map(|(e, k)| (e, *k)).collect();
    for (e, kit) in kits {
        let mut keep = if kit.host { n.host_weapons.clone() } else { n.infected_weapons.clone() };
        if keep.is_empty() {
            keep.push(super::super::inventory::KNIFE);
        }
        let ids: Vec<&'static str> = world
            .get::<crate::weapon::Inventory>(e)
            .map(|i| {
                i.weapons
                    .iter()
                    .filter_map(|w| world.get::<crate::weapon::Weapon>(*w).map(|w| w.id))
                    .collect()
            })
            .unwrap_or_default();
        if ids.is_empty() {
            continue;
        }
        for id in &keep {
            objectives::give_weapon(world, e, id);
        }
        for id in ids.into_iter().filter(|id| !keep.contains(id)) {
            objectives::take_weapon(world, e, id);
        }
        world.entity_mut(e).remove::<CaInfectedKit>();
    }
    // The heal after standing still: `heal` of the maximum hit points
    // every period, the wait restarted by a move, a hit or firing.
    let still: Vec<(Entity, CaStill, Vec3, Health, bool)> = world
        .query::<(Entity, &CaStill, &Transform, &Health, &crate::core::Intent)>()
        .iter(world)
        .filter(|(_, _, _, h, _)| h.current > 0.0)
        .map(|(e, s, t, h, i)| (e, *s, t.translation, h.clone(), i.fire))
        .collect();
    for (e, mut s, at, health, firing) in still {
        if at.distance(s.at) > 1e-3 || health.current < s.health - 1e-6 || firing {
            s.at = at;
            s.since = now;
            s.next_heal = now + n.heal_wait as f64;
        } else if now + 1e-9 >= s.since + n.heal_wait as f64 && now + 1e-9 >= s.next_heal {
            if let Some(mut h) = world.get_mut::<Health>(e) {
                h.current = (h.current + n.heal * h.max).min(h.max);
            }
            s.next_heal = now + n.heal_period as f64;
        }
        s.health = world.get::<Health>(e).map_or(health.current, |h| h.current);
        world.entity_mut(e).insert(s);
    }
    test_win(world);
}

/// No human alive: the infected win; after the selection, no infected
/// alive: the humans.
fn test_win(world: &mut World) {
    if world.resource::<CaMatch>().winner.is_some() || !world.resource::<CaMatch>().counting() {
        return;
    }
    let (humans, infected) = (alive(world, ALPHA), alive(world, BRAVO));
    let picked = world.resource::<CaQuarantine>().picked;
    if humans == 0 {
        world.resource_mut::<CaMatch>().win(BRAVO);
    } else if picked && infected == 0 {
        world.resource_mut::<CaMatch>().win(ALPHA);
    }
}

fn on_death(world: &mut World, d: &Death) {
    personal(world, d);
    let n = super::super::missions::quarantine(world);
    if d.kind == DeathKind::Enemy
        && let Some(k) = d.killer
    {
        if d.victim_team == BRAVO {
            stats(world, k).mission_points += n.kill_point;
            stats(world, d.victim).mission_points += n.dead_point;
        } else {
            stats(world, k).mission_points += n.infect_point;
            stats(world, d.victim).mission_points += n.infected_point;
            revive_in_place(world, d.victim);
            infect(world, d.victim, false);
        }
    }
    test_win(world);
}

/// The living members of the winning side: +10 humans, +5 infected.
fn round_over(world: &mut World) {
    let m = world.resource::<CaMatch>();
    if m.warmup {
        return;
    }
    let Some(winner) = m.winner else { return };
    let n = super::super::missions::quarantine(world);
    let points = if winner == ALPHA { n.human_win_point } else { n.infect_win_point };
    for p in objectives::living(world) {
        if p.team == winner {
            stats(world, p.e).mission_points += points;
        }
    }
}
