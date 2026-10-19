//! Deaths, kills and friendly fire (specs/combat_arms/game_modes.md S7,
//! S8): what a death adds to whom, assists, and the teammate damage
//! filter.
//!
//! - A death (`core::Died`, after the damage that caused it) makes the
//!   player dead (`core::Dead`, its collider off); while the match counts
//!   (`CaMatch::counting`: the live phase, not a warm-up) the mode adds
//!   its row of S7's table (`CaMode::on_death`, which may also win the
//!   round), its kills and deaths copied into Chimera's `core::Score`
//!   (the scoreboard line the network replicates).
//! - The killer (`classify`): the attacker; the victim's own damage is a
//!   suicide, but a fall (the movement's crush damage from the player
//!   itself, no weapon) has no killer.
//! - Assists (the modes with `CaMode::assists`): damage a player dealt
//!   to another is summed while the victim lives (`CaDamageLog`; heals
//!   by the victim's enemies take it back, `heal`); an enemy kill gives
//!   every other enemy damager with at least half the victim's maximum
//!   health one assist; any death clears the victim's sums.
//! - Friendly fire (S8): off, Chimera's team rule refuses a teammate's
//!   damage (`core::FriendlyFire` 0; your own is never refused); on, in
//!   the modes that scale it (`CaMode::ff_scaled`), a teammate's bullets,
//!   melee and other direct damage land at `FF_SCALE` (explosions and
//!   fire at full).
//!
//! Known shortcuts: the scaling applies to what reached the target after
//! its armour (the weapon parts take armour before the damage message);
//! the "team id 0 or above 254" part of the filter isn't there (every
//! player in a mode is on Alpha or Bravo); there are no heals in the game
//! yet, so `heal` has no caller but the tests; the teammate-hit voice
//! isn't played.

use std::collections::HashMap;

use avian3d::prelude::ColliderDisabled;
use bevy::{
    ecs::message::{MessageCursor, MessageMutator},
    prelude::*,
};

use super::{
    CaRuleSettings, current_mode,
    state::{CaMatch, CaStats, Death, classify, side},
};
use crate::core::{Damage, DamageKind, DamageTaken, Dead, Died, FriendlyFire, Health, Intent, Score, SimClock, Team};

/// A teammate's direct damage with friendly fire on (S8).
pub const FF_SCALE: f32 = 0.25;
/// The share of the victim's maximum health that earns an assist (S7).
pub const ASSIST_SHARE: f32 = 0.5;

/// Damage summed per (victim, damager) while the victim lives, in health
/// units.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct CaDamageLog(pub HashMap<(Entity, Entity), f32>);

/// A heal of `victim` by `healer` for `amount` (health units; S7 as its
/// test case 15 has it: a heal by an enemy of the damager): every sum on
/// the victim of a damager on another team than the healer's goes down by
/// it, and is dropped below 0.
pub fn heal(world: &mut World, healer: Entity, victim: Entity, amount: f32) {
    let team = |w: &World, e| w.get::<Team>(e).map(|t| t.0);
    let healer_team = team(world, healer);
    let damagers: Vec<Entity> = world
        .resource::<CaDamageLog>()
        .0
        .keys()
        .filter(|(v, a)| *v == victim && team(world, *a) != healer_team)
        .map(|(_, a)| *a)
        .collect();
    let mut log = world.resource_mut::<CaDamageLog>();
    for a in damagers {
        let Some(sum) = log.0.get_mut(&(victim, a)) else { continue };
        *sum -= amount;
        if *sum < 0.0 {
            log.0.remove(&(victim, a));
        }
    }
}

/// Damage that reached a player, summed for assists.
pub(super) fn log_damage(mut taken: MessageReader<DamageTaken>, mut log: ResMut<CaDamageLog>) {
    for t in taken.read() {
        let d = &t.damage;
        if let Some(a) = d.attacker.filter(|a| *a != d.target) {
            *log.0.entry((d.target, a)).or_default() += d.amount;
        }
    }
}

/// S8: a teammate's direct damage at a quarter with friendly fire on (in
/// the modes that scale it); Chimera's team rule refuses it with it off.
pub(super) fn scale_friendly_fire(
    mut damage: MessageMutator<Damage>,
    teams: Query<&Team>,
    settings: Res<CaRuleSettings>,
    mut ff: ResMut<FriendlyFire>,
) {
    let on = settings.friendly_fire;
    if ff.0 != on {
        ff.0 = on;
    }
    let scaled = on != 0 && super::MODES.iter().any(|m| m.key == settings.mode && m.ff_scaled);
    for d in damage.read() {
        if !scaled || matches!(d.kind, DamageKind::Blast | DamageKind::Burn) {
            continue;
        }
        let Some(a) = d.attacker.filter(|a| *a != d.target) else { continue };
        let mates = matches!((teams.get(a), teams.get(d.target)), (Ok(x), Ok(y)) if x == y && side(x.0).is_some());
        if mates && d.kind != DamageKind::Crush {
            d.amount *= FF_SCALE;
        }
    }
}

/// The deaths of this tick (`Died`, after the damage), in order: the dead
/// marked, then counted by the mode while the match counts.
pub(super) fn handle_deaths(world: &mut World, mut cursor: Local<MessageCursor<Died>>) {
    let deaths: Vec<Died> = cursor.read(world.resource::<Messages<Died>>()).cloned().collect();
    for d in deaths {
        on_died(world, &d);
    }
}

/// One death (S7).
fn on_died(world: &mut World, d: &Died) {
    let Some(mode) = current_mode(world) else { return };
    let victim = d.entity;
    if world.get::<Intent>(victim).is_none() {
        return;
    }
    let now = world.resource::<SimClock>().now;
    world.entity_mut(victim).insert((Dead { since: now }, ColliderDisabled));
    let killer_team = |w: &World, e: Entity| w.get::<Team>(e).map(|t| t.0);
    let Some(victim_team) = killer_team(world, victim).filter(|t| side(*t).is_some()) else {
        clear_log(world, victim);
        return;
    };
    let fall = d.damage.kind == DamageKind::Crush && d.damage.weapon.is_none();
    let kind = classify(victim, d.attacker, d.attacker.and_then(|k| killer_team(world, k)), victim_team, fall);
    if world.resource::<CaMatch>().counting() {
        let death = Death {
            victim,
            victim_team,
            killer: d.attacker,
            kind,
        };
        if mode.assists && kind == super::state::DeathKind::Enemy {
            give_assists(world, &death);
        }
        (mode.on_death)(world, &death);
        sync_score(world, victim);
        if let Some(k) = d.attacker {
            sync_score(world, k);
        }
    }
    clear_log(world, victim);
}

/// S7's assists for an enemy kill.
fn give_assists(world: &mut World, death: &Death) {
    let max = world.get::<Health>(death.victim).map_or(1.0, |h| h.max);
    let sums: Vec<(Entity, f32)> = world
        .resource::<CaDamageLog>()
        .0
        .iter()
        .filter(|((v, a), _)| *v == death.victim && Some(*a) != death.killer)
        .map(|((_, a), s)| (*a, *s))
        .collect();
    for (a, sum) in sums {
        let enemy = world.get::<Team>(a).is_some_and(|t| t.0 != death.victim_team);
        if enemy && sum / max >= ASSIST_SHARE - 1e-6 {
            stats(world, a).assists += 1;
        }
    }
}

fn clear_log(world: &mut World, victim: Entity) {
    world.resource_mut::<CaDamageLog>().0.retain(|(v, _), _| *v != victim);
}

/// A player's statistics, made when missing.
pub fn stats(world: &mut World, e: Entity) -> Mut<'_, CaStats> {
    if world.get::<CaStats>(e).is_none() {
        world.entity_mut(e).insert(CaStats::default());
    }
    world.get_mut::<CaStats>(e).unwrap()
}

/// Chimera's scoreboard line (`core::Score`) from the statistics.
pub fn sync_score(world: &mut World, e: Entity) {
    let Some(s) = world.get::<CaStats>(e).copied() else { return };
    let score = Score {
        kills: s.kills as i32,
        deaths: s.deaths,
    };
    if let Ok(mut ent) = world.get_entity_mut(e) {
        ent.insert(score);
    }
}
