//! The primitives every minigame plays on: Chimera's side of the mode
//! runtime (`FLOW`) and the server's systems while a minigame is the
//! gameplay (`plugin`), lifted in shape from Combat Arms' flow and CS:S's
//! deathmatch, naming neither.
//!
//! - Players (`players`): characters on a team that plays, in the game
//!   (not connecting, spectating or an NPC), in the order they joined
//!   (`JoinOrder`). A new one (`join`) goes on the team the mode makes
//!   (one with no team first on the first side), comes in at once unless
//!   the match is over, and is marked `InMinigame`.
//! - The match waits (`Phase::Waiting`) until `players.min` are in; then
//!   the runtime starts it (everyone respawned, the module's
//!   `before_spawns` first), live at once (no setup), for `time_limit`;
//!   a round over of `ROUND_OVER` s, an intermission of `INTERMISSION` s,
//!   then the match over, its winner the round's (`Mode::match_end`).
//! - Spawning (`respawn`): at a spawn point the definition's `points`
//!   pick (`spawn_point`), revived (`spawning::revive`), the definition's
//!   movement set, the weapons taken and the life's given (the kit, then
//!   the rules module's `loadout`, the last drawn); with `identity =
//!   "weapon"` the player follows its weapon's game (`slots::
//!   FollowsWeapon`; `identity::follow` at once for the drawn one),
//!   protected from damage
//!   (`Protected`, through `core::God`) for `protection` s or until it
//!   fires. The dead respawn after `delay` s (`respawn_dead`) unless the
//!   match is over or the mode refuses.
//! - Deaths (`handle_deaths`, after the tick's damage): the dead marked
//!   (`Dead`, not solid), `core::Score`'s deaths and an enemy kill's kill
//!   counted while the match counts, the mode's `on_death`, then its
//!   `died` hook in any phase.
//! - `take_weapon`: a weapon out of a character's hands and inventory, for
//!   the rules' swaps.

use avian3d::prelude::ColliderDisabled;
use bevy::{ecs::message::MessageCursor, prelude::*};

use super::{
    ActiveMinigame, MINIGAME,
    def::{IdentityDef, Points, SpawnDef},
};
use crate::{
    core::{
        Connecting, Dead, Died, God, Health, Intent, Npc, Score, SimClock, SimSet, SpawnPoint, Spectating, Team,
    },
    mode::{
        self, ALPHA, Death, DeathKind, Flow, FlowEvent, Match, Phase, Span, classify, current_mode, plays,
    },
    weapon::{Inventory, give},
};
use chimera_core::slots::{FollowsWeapon, MovementSlot};

/// A decided round's results stay up this long before the round ends.
pub const ROUND_OVER: f32 = 5.0;
/// From the round's end to the match over.
pub const INTERMISSION: f32 = 3.0;
/// How close (m) another living character may be to a spawn point for it
/// to be free (about a player's width).
pub const SPAWN_CLEARANCE: f32 = 0.8;

/// Chimera's side of the mode runtime while a minigame plays.
pub const FLOW: Flow = Flow {
    frame: 0.0,
    players,
    respawn,
    length,
    event,
    // Combat Arms' missions' stepping: wait for players, the timer ends
    // the round, a round over and an intermission before the end.
    options: crate::mode::FlowOptions::MISSIONS,
};

/// The order a character joined (numbered as it gets a team).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct JoinOrder(pub u64);

/// A character the minigame took in (`join`).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct InMinigame;

/// Spawn protection: no damage (`core::God`, which this inserted) until
/// `until` or the first shot.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Protected {
    pub until: f64,
}

/// The `Points::Map` cursor.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct SpawnCursor(pub usize);

/// The players in the match, in join order.
pub fn players(world: &mut World) -> Vec<(Entity, u8)> {
    let mut all: Vec<(u64, Entity, u8)> = world
        .query_filtered::<(Entity, &Team, Option<&JoinOrder>), (
            With<Intent>,
            With<Health>,
            With<InMinigame>,
            Without<Connecting>,
            Without<Spectating>,
            Without<Npc>,
        )>()
        .iter(world)
        .filter(|(_, t, _)| plays(t.0))
        .map(|(e, t, o)| (o.map_or(u64::MAX, |o| o.0), e, t.0))
        .collect();
    all.sort();
    all.into_iter().map(|(_, e, t)| (e, t)).collect()
}

fn length(world: &mut World, span: Span) -> Option<f64> {
    let def = world.get_resource::<ActiveMinigame>()?.def;
    match span {
        Span::Setup => Some(0.0),
        Span::Round => Some(def.time_limit as f64),
        Span::RoundOver => Some(ROUND_OVER as f64),
        Span::Intermission => Some(INTERMISSION as f64),
        _ => None,
    }
}

fn event(world: &mut World, event: FlowEvent) {
    match event {
        FlowEvent::ResetStats => {
            for (e, _) in players(world) {
                world.entity_mut(e).insert(Score::default());
            }
        }
        FlowEvent::RoundOver => {
            if let Some(w) = world.resource::<Match>().winner {
                let player = players(world).into_iter().find(|(_, t)| *t == w).map(|(e, _)| e);
                world.write_message(super::MinigameEvent {
                    name: "match_won",
                    player,
                    value: w as i32,
                });
            }
        }
        _ => {}
    }
}

/// Bring `e` into play for a life: at a spawn point, revived, its movement
/// set, the life's weapons given, protected.
pub fn respawn(world: &mut World, e: Entity) {
    let Some(active) = world.get_resource::<ActiveMinigame>().copied() else { return };
    let def = active.def;
    let at = spawn_point_in(world, e, def.spawn.points, def.spawn.min_distance);
    if !crate::spawning::revive(world, e, at) {
        return;
    }
    if let Some(m) = def.slots.movement.as_deref()
        && world.get::<MovementSlot>(e).is_none_or(|s| s.0 != m)
        && let Some(id) = world
            .get_resource::<chimera_core::slots::MovementRegistry>()
            .and_then(|r| r.get(m))
            .map(|i| i.id)
    {
        chimera_core::slots::set_movement(e, id).apply(world);
    }
    let follows = def.slots.identity == Some(IdentityDef::Weapon);
    if follows {
        world.entity_mut(e).insert(FollowsWeapon);
    } else if world.get::<FollowsWeapon>(e).is_some() {
        crate::identity::unfollow(world, e);
    }
    crate::weapon::equip::strip(world, e);
    let catalog = world.get_resource::<crate::weapon::data::WeaponCatalog>().cloned().unwrap_or_default();
    let kit: Vec<&'static str> = def.spawn.kit.iter().filter_map(|id| super::weapon(def, &catalog, id).ok()).collect();
    for id in kit.into_iter().chain((active.rules.loadout)(world, e)) {
        give(world, e, id);
    }
    // Following the weapon: the life starts as the drawn weapon's game.
    if follows {
        crate::identity::follow(world, e);
    }
    let protection = def.spawn.respawn.protection;
    if protection > 0.0 && world.get::<God>(e).is_none() {
        let now = world.resource::<SimClock>().now;
        world.entity_mut(e).insert((God, Protected {
            until: now + protection as f64,
        }));
    }
}

/// Where `e` comes back, as `points` picks among the map's spawn points
/// (None: none, it stays where it is).
pub fn spawn_point(world: &mut World, e: Entity, points: Points) -> Option<Transform> {
    spawn_point_in(world, e, points, SpawnDef::default().min_distance)
}

/// `spawn_point` with `random_hidden`'s least distance from an enemy (m).
pub fn spawn_point_in(world: &mut World, e: Entity, points: Points, min_distance: f32) -> Option<Transform> {
    let mut all: Vec<(Entity, Transform, Option<Team>)> = world
        .query_filtered::<(Entity, &Transform, &SpawnPoint), Without<Intent>>()
        .iter(world)
        .map(|(p, t, s)| (p, *t, s.team))
        .collect();
    all.sort_by_key(|(p, ..)| *p);
    if all.is_empty() {
        return None;
    }
    let others: Vec<Vec3> = world
        .query_filtered::<(Entity, &Transform, &Health), (With<Intent>, Without<Dead>)>()
        .iter(world)
        .filter(|(o, _, h)| *o != e && h.current > 0.0)
        .map(|(_, t, _)| t.translation)
        .collect();
    let free = |t: &Transform| {
        others
            .iter()
            .all(|o| (o.xz() - t.translation.xz()).length() > SPAWN_CLEARANCE || (o.y - t.translation.y).abs() > 2.0)
    };
    let points = match points {
        Points::RandomHidden => match super::spawns::pick(world, e, min_distance) {
            Some(at) => return Some(at),
            None => Points::FarFromEnemies,
        },
        p => p,
    };
    match points {
        Points::RandomHidden => unreachable!("handled above"),
        Points::Map => {
            let n = all.len();
            let start = world.get_resource_or_init::<SpawnCursor>().0;
            let pick = (0..n).map(|i| (start + i) % n).find(|i| free(&all[*i].1)).unwrap_or(start % n);
            world.resource_mut::<SpawnCursor>().0 = pick + 1;
            Some(all[pick].1)
        }
        Points::Team => {
            let team = world.get::<Team>(e).copied();
            let own: Vec<_> = all.iter().filter(|(_, _, t)| t.is_some() && *t == team).cloned().collect();
            let from = if own.is_empty() { all } else { own };
            from.iter().find(|(_, t, _)| free(t)).or(from.first()).map(|(_, t, _)| *t)
        }
        Points::FarFromEnemies => {
            let nearest = |t: &Transform| {
                others
                    .iter()
                    .map(|o| o.distance(t.translation))
                    .fold(f32::INFINITY, f32::min)
            };
            // The farthest from its nearest other player (a free one if
            // any is); ties: the first.
            let mut best: Option<(f32, Transform)> = None;
            let any_free = all.iter().any(|(_, t, _)| free(t));
            for (_, t, _) in all.iter().filter(|(_, t, _)| !any_free || free(t)) {
                let d = nearest(t);
                if best.is_none_or(|(b, _)| d > b + 1e-3) {
                    best = Some((d, *t));
                }
            }
            best.map(|(_, t)| t)
        }
    }
}

/// Take `weapon` out of `owner`'s inventory (and hands) and despawn it.
pub fn take_weapon(world: &mut World, owner: Entity, weapon: Entity) {
    if let Some(mut inv) = world.get_mut::<Inventory>(owner) {
        let inv = &mut *inv;
        inv.weapons.retain(|w| *w != weapon);
        for slot in [&mut inv.active, &mut inv.last, &mut inv.wanted] {
            if *slot == Some(weapon) {
                *slot = None;
            }
        }
    }
    if let Ok(w) = world.get_entity_mut(weapon) {
        w.despawn();
    }
}

/// New characters into the minigame: a team as the mode makes them, at
/// once into play unless the match is over.
fn join(world: &mut World) {
    let new: Vec<(Entity, u8)> = world
        .query_filtered::<(Entity, &Team), (
            With<Intent>,
            With<Health>,
            Without<InMinigame>,
            Without<Connecting>,
            Without<Spectating>,
            Without<Npc>,
        )>()
        .iter(world)
        .map(|(e, t)| (e, t.0))
        .collect();
    let over = world.resource::<Match>().phase == Phase::MatchOver;
    for (e, team) in new {
        if team == 255 {
            continue;
        }
        if !plays(team) {
            world.entity_mut(e).insert(Team(ALPHA));
        }
        world.entity_mut(e).insert(InMinigame);
        mode::assign_team(world, e, false);
        if !over {
            respawn(world, e);
        }
    }
}

/// The runtime's phases, once `players.min` are in.
fn step_phases(world: &mut World) {
    let Some(min) = world.get_resource::<ActiveMinigame>().map(|a| a.def.players.min) else { return };
    if world.resource::<Match>().phase == Phase::Waiting && (players(world).len() as u32) < min {
        return;
    }
    mode::flow::mission_update(world);
}

/// The dead back after the definition's delay.
fn respawn_dead(world: &mut World) {
    let Some(def) = world.get_resource::<ActiveMinigame>().map(|a| a.def) else { return };
    let Some(mode) = current_mode(world) else { return };
    if !mode.respawns || world.resource::<Match>().phase == Phase::MatchOver {
        return;
    }
    let now = world.resource::<SimClock>().now;
    let due: Vec<Entity> = players(world)
        .into_iter()
        .filter(|(e, _)| world.get::<Dead>(*e).is_some_and(|d| now + 1e-9 >= d.since + def.spawn.respawn.delay as f64))
        .map(|(e, _)| e)
        .collect();
    for e in due {
        if (mode.hooks.may_respawn)(world, e) {
            respawn(world, e);
        }
    }
}

/// Protection ends at its time or the first shot.
fn end_protection(world: &mut World) {
    let now = world.resource::<SimClock>().now;
    let done: Vec<Entity> = world
        .query::<(Entity, &Protected, &Intent)>()
        .iter(world)
        .filter(|(_, p, i)| now + 1e-9 >= p.until || i.fire)
        .map(|(e, ..)| e)
        .collect();
    for e in done {
        world.entity_mut(e).remove::<(Protected, God)>();
    }
}

/// The tick's deaths: marked, counted, the mode told.
fn handle_deaths(world: &mut World, mut cursor: Local<MessageCursor<Died>>) {
    let deaths: Vec<Died> = cursor.read(world.resource::<Messages<Died>>()).cloned().collect();
    for d in deaths {
        on_died(world, &d);
    }
}

fn on_died(world: &mut World, d: &Died) {
    let Some(mode) = current_mode(world) else { return };
    let victim = d.entity;
    if world.get::<Intent>(victim).is_none() || world.get::<InMinigame>(victim).is_none() {
        return;
    }
    let now = world.resource::<SimClock>().now;
    world.entity_mut(victim).insert((Dead { since: now }, ColliderDisabled)).remove::<(Protected, God)>();
    let team_of = |w: &World, e: Entity| w.get::<Team>(e).map(|t| t.0);
    let Some(victim_team) = team_of(world, victim).filter(|t| plays(*t)) else { return };
    let fall = d.damage.kind == crate::core::DamageKind::Fall;
    let kind = classify(victim, d.attacker, d.attacker.and_then(|k| team_of(world, k)), victim_team, fall);
    if world.resource::<Match>().counting() {
        add_score(world, victim, 0, 1);
        if let Some(k) = d.attacker {
            match kind {
                DeathKind::Enemy => add_score(world, k, 1, 0),
                DeathKind::TeamKill | DeathKind::Suicide => add_score(world, k, -1, 0),
                DeathKind::NoKiller => {}
            }
        }
        let death = Death {
            victim,
            victim_team,
            killer: d.attacker,
            kind,
        };
        (mode.on_death)(world, &death);
    }
    (mode.hooks.died)(world, d);
}

fn add_score(world: &mut World, e: Entity, kills: i32, deaths: u32) {
    let Ok(mut ent) = world.get_entity_mut(e) else { return };
    let mut s = ent.get::<Score>().copied().unwrap_or_default();
    s.kills += kills;
    s.deaths += deaths;
    ent.insert(s);
}

/// The match as `minigame_status` prints it.
pub fn status(world: &mut World) -> Result<String, String> {
    let active = *world.get_resource::<ActiveMinigame>().ok_or("no minigame playing")?;
    let now = world.resource::<SimClock>().now;
    let m = world.resource::<Match>().clone();
    let left = m.round_ends.map(|e| format!(", {:.0} s left", (e - now).max(0.0))).unwrap_or_default();
    let mut lines = vec![format!(
        "{} ({}): {:?}{left}; winner {:?}; result {:?}",
        active.def.id, active.rules.mode.id, m.phase, m.winner, m.result
    )];
    for (e, team) in players(world) {
        let name = world.get::<Name>(e).map_or_else(|| e.to_string(), |n| n.as_str().to_string());
        let s = world.get::<Score>(e).copied().unwrap_or_default();
        let standing = super::ladder::standing(world, e).unwrap_or_default();
        lines.push(format!("  {name} team {team}: {} kills, {} deaths{standing}", s.kills, s.deaths));
    }
    Ok(lines.join("\n"))
}

/// The runtime's systems (the server's, while a minigame is the
/// gameplay's rules) and the join order.
pub(super) fn plugin(app: &mut App) {
    let on = |a: Option<Res<ActiveMinigame>>, g: Option<Res<chimera_core::gameplay::Gameplay>>| {
        a.is_some() && g.is_none_or(|g| g.rules == MINIGAME)
    };
    use crate::map::MapScopedApp;
    app.init_resource::<SpawnCursor>()
        .map_scoped::<super::spawns::SpawnSpots>()
        .add_observer(number_players)
        .add_systems(
            FixedUpdate,
            (
                (
                    join,
                    mode::flow::update,
                    step_phases,
                    respawn_dead,
                    crate::spawning::hold_the_dead,
                    end_protection,
                )
                    .chain()
                    .in_set(SimSet::Rules),
                (handle_deaths, mode::flow::touches).chain().after(crate::core::apply_damage),
            )
                .run_if(crate::core::authoritative)
                .run_if(on),
        );
}

fn number_players(add: On<Add, Team>, mut next: Local<u64>, mut commands: Commands) {
    *next += 1;
    if let Ok(mut e) = commands.get_entity(add.entity) {
        e.try_insert(JoinOrder(*next));
    }
}
