//! Quarantine Regen (`combat_arms:quarantine`, room key `QRT`;
//! specs/combat_arms/game_modes.md M8): infection rounds. Everyone starts
//! human (Alpha); `host_delay` (20 s) after a round goes live, H living
//! humans become infected hosts (Bravo); the infected spread it. A warm-up
//! first, then G rounds played of `TimeLimit` 3 minutes.
//!
//! - Hosts: H = 1 with N <= 3 players on teams, else max(round(N x
//!   `selection_rate`), `min_hosts`) (`hosts`), chosen at random among
//!   the living humans; with H or fewer humans alive then, the round is
//!   void (no winner).
//! - Infected (`infect`): on Bravo, `movement::CaInfected` (S14's
//!   knock-back), the knife only; after `infected_still` (3 s) without
//!   moving they heal `infected_heal` every `infected_heal_period` (1 s).
//! - Deaths: S7's personal counts (`scoring::personal`) and the mission
//!   points: a human killing an infected +3, the infected killed -1; an
//!   infected killing a human +2, and that human becomes infected.
//! - Round win: no human alive, the infected; after the selection, no
//!   infected alive, the humans. Time-out: the humans. The living members
//!   of the winning side get +10 (humans) or +5 (infected) at the round's
//!   end. Match end: after G rounds played.
//! - Spawns: the `Quarantine` points, one list (all `JSF` in the stock
//!   maps). No respawn request.
//!
//! Known shortcuts (the most literal readings): a human killed by an
//! infected comes back at once as infected at the next point (M8 says only
//! "become infected"; with no respawn request); the host selection runs
//! only in real rounds; the infected keep their body (no infected model
//! from the mission file) and get our knife; the heal is in hit points
//! (Open question 12); the match has no winner of its own (M8 names none:
//! the result is a draw, the scoreboard's mission points rank the
//! players).

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

/// An infected player's stillness (the heal's clock).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct CaStill {
    pub at: Vec3,
    pub since: f64,
    pub next_heal: f64,
}

/// An infected player waiting for its loadout to be cut to the knife.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct CaInfectedKit;

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

/// `e` becomes infected: Bravo, the knife only.
pub fn infect(world: &mut World, e: Entity) {
    let now = world.resource::<SimClock>().now;
    let at = world.get::<Transform>(e).map_or(Vec3::ZERO, |t| t.translation);
    world.entity_mut(e).insert((
        Team(BRAVO),
        super::super::movement::CaInfected,
        CaStill {
            at,
            since: now,
            next_heal: now,
        },
        CaInfectedKit,
    ));
}

/// Host selection, the infected's kit and heal, the win tests.
fn update(world: &mut World) {
    let now = world.resource::<SimClock>().now;
    let o = world.resource::<CaRuleSettings>().objectives.clone();
    let counting = world.resource::<CaMatch>().counting();
    if !counting {
        return;
    }
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
        let n = players(world).len();
        let h = hosts(n, o.selection_rate, o.min_hosts);
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
            infect(world, e);
        }
    }
    // The knife only, once the loadout is there.
    let kits: Vec<Entity> = world.query_filtered::<Entity, With<CaInfectedKit>>().iter(world).collect();
    for e in kits {
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
        for id in ids.into_iter().filter(|id| *id != super::super::inventory::KNIFE) {
            objectives::take_weapon(world, e, id);
        }
        world.entity_mut(e).remove::<CaInfectedKit>();
    }
    // The heal after standing still.
    let still: Vec<(Entity, CaStill, Vec3)> = world
        .query::<(Entity, &CaStill, &Transform, &Health)>()
        .iter(world)
        .filter(|(.., h)| h.current > 0.0)
        .map(|(e, s, t, _)| (e, *s, t.translation))
        .collect();
    for (e, mut s, at) in still {
        if at.distance(s.at) > 1e-3 {
            s.at = at;
            s.since = now;
            s.next_heal = now + o.infected_still as f64;
        } else if now + 1e-9 >= s.since + o.infected_still as f64 && now + 1e-9 >= s.next_heal {
            if let Some(mut h) = world.get_mut::<Health>(e) {
                h.current = (h.current + o.infected_heal * super::super::inventory::HP).min(h.max);
            }
            s.next_heal = now + o.infected_heal_period as f64;
        }
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
    let o = world.resource::<CaRuleSettings>().objectives.clone();
    if d.kind == DeathKind::Enemy
        && let Some(k) = d.killer
    {
        if d.victim_team == BRAVO {
            stats(world, k).mission_points += o.human_kills_infected;
            stats(world, d.victim).mission_points += o.infected_killed_by_human;
        } else {
            stats(world, k).mission_points += o.infected_infects;
            spawns::respawn(world, d.victim);
            infect(world, d.victim);
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
    let o = world.resource::<CaRuleSettings>().objectives.clone();
    let points = if winner == ALPHA { o.humans_win } else { o.infected_win };
    for p in objectives::living(world) {
        if p.team == winner {
            stats(world, p.e).mission_points += points;
        }
    }
}
