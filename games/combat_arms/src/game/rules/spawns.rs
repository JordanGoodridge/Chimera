//! Spawning (specs/combat_arms/game_modes.md S4, S5, S6): which point a
//! player respawns at, the respawn itself, the respawn request and its
//! waits, telefrags and spawn protection.
//!
//! - Points (S4): the map's start points (`core::SpawnPoint`, which
//!   `game::world` keeps to the running family's qualifying points:
//!   `Team Death Match`, unlocked, `Spawn` 1), one list per team in map
//!   order (`Name` "Spawn i", the map's index), each with a cursor from 0
//!   (`CaSpawnCursors`). A spawn advances the cursor (wrapping) and takes
//!   the first point no other player object stands near (`occupied`:
//!   within twice the fallback hull's half-extents on every axis, strict),
//!   at most once round the list; else the last tested. An empty team
//!   list: the map's own family's unlocked qualifying points as one list
//!   (`world::map_family`: a Hunted map's `HungerGame` points under TDM),
//!   else the map's first point.
//! - The respawn (`respawn`, S5): the loadout rebuilt (`inventory::
//!   respawn_inventory`), full health (`spawning::revive`, Chimera's) and
//!   armour (`CaArmor`'s default), at the point facing its yaw; then a
//!   telefrag of every unprotected player inside the doubled box of the
//!   new position (only when every point was occupied: a crush death
//!   credited to themselves, `TELEFRAG`); then protection.
//! - Protection (S6): every damage kind blocked for `protect_all` s
//!   (`core::DamageFilter`, the filter the player had put back after,
//!   `CaProtected`).
//! - Requests (S5): `request_respawn` is the server's check: refused when
//!   the mode doesn't respawn or the death is less than
//!   `respawn_wait_server` old; one request per death (`CaAsked`: the
//!   client doesn't retry, so a refused player stays dead until the next
//!   round setup). Players on this machine (single player, a listen
//!   server's host) ask as their client would, once dead for more than
//!   `respawn_wait_client` (`ask_for_respawns`); a network client asks
//!   through `game::net` (`CaRespawnRequest`).
//!
//! Known shortcuts: every player's box is the fallback hull (15, 50, 15)
//! (no per-figure dimensions); dead players count as player objects where
//! their entity lies (we have no separate body); instant-hit shots don't
//! pass through a protected player for `protect_bullets` s (they stop at
//! it and do nothing, as all damage is blocked for the longer
//! `protect_all`), and clients still draw blood on it; a point's
//! `Command` isn't run (no `SendCommandOnRespawn` on stock team points);
//! the waits are game time, not the OS millisecond clocks; the fast
//! respawn item isn't there.

use avian3d::prelude::ColliderDisabled;
use bevy::prelude::*;

use super::{
    super::{inventory, weapons::CaArmor},
    CaRuleSettings, current_mode,
    state::side,
};
use crate::{
    core::{
        Connecting, Damage, DamageFilter, DamageKind, Dead, Health, Hitgroup, Intent, MapLoads, Npc, RemotePlayer,
        SimClock, SpawnPoint, Spectating, Team,
    },
    spawning,
    weapon::{WeaponEvent, WeaponEventKind},
};

/// The fallback hull's half-extents, world units (S4).
pub const HULL: Vec3 = Vec3::new(15.0, 50.0, 15.0);
/// A point is occupied inside a player's box scaled by this (S4).
pub const OCCUPIED_SCALE: f32 = 2.0;
/// What a telefrag's crush damage counts as.
pub const TELEFRAG: &str = "combat_arms:telefrag";

/// Whether a player whose centre is `d` (world units) from a point
/// occupies it: strictly inside twice its half-extents on every axis.
pub fn occupied(d: Vec3) -> bool {
    let r = HULL * OCCUPIED_SCALE;
    d.x.abs() < r.x && d.y.abs() < r.y && d.z.abs() < r.z
}

/// Each team list's cursor (S4), Alpha and Bravo; the first point's
/// list (a team without points) has none.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CaSpawnCursors(pub [usize; 2]);

/// The client asked to respawn after this death (its one request).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CaAsked;

/// Held out until the next round setup (a late joiner in a round mode,
/// S1): dead, not solid, spawned with everyone at the next setup.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CaHeld;

/// Spawn protection (S6), from the respawn at `since` (game seconds).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct CaProtected {
    pub since: f64,
    /// Instant-hit shots pass through until then.
    pub bullets_until: f64,
    /// No damage at all until then.
    pub all_until: f64,
    /// The damage filter the player had before (put back after).
    pub kept: Option<DamageFilter>,
}

/// Every damage kind there is (the protection's filter).
const ALL_KINDS: [DamageKind; 8] = [
    DamageKind::Generic,
    DamageKind::Bullet,
    DamageKind::Melee,
    DamageKind::Blast,
    DamageKind::Crush,
    DamageKind::Fall,
    DamageKind::Burn,
    DamageKind::Drown,
];

/// A start point's place in the map's order (its `Name`, "Spawn i").
fn map_index(name: Option<&Name>) -> usize {
    name.and_then(|n| n.as_str().strip_prefix("Spawn "))
        .and_then(|i| i.trim().parse().ok())
        .unwrap_or(usize::MAX)
}

/// The start points of `team` (None: every point), in map order.
pub fn points(world: &mut World, team: Option<u8>) -> Vec<Transform> {
    let mut all: Vec<(usize, Entity, Transform)> = world
        .query::<(Entity, &Transform, &SpawnPoint, Option<&Name>)>()
        .iter(world)
        .filter(|(_, _, s, _)| team.is_none() || s.team.map(|t| t.0) == team)
        .map(|(e, t, _, n)| (map_index(n), e, *t))
        .collect();
    all.sort_by_key(|(i, e, _)| (*i, *e));
    all.into_iter().map(|(_, _, t)| t).collect()
}

/// Where every other player object is (players in the game, alive or
/// dead), and whether it is alive and unprotected.
fn others(world: &mut World, me: Entity) -> Vec<(Entity, Vec3, bool)> {
    world
        .query_filtered::<(Entity, &Transform, &Health, Has<Dead>, Has<CaProtected>), (
            With<Intent>,
            Without<Connecting>,
            Without<Spectating>,
            Without<CaHeld>,
            Without<Npc>,
        )>()
        .iter(world)
        .filter(|(e, ..)| *e != me)
        .map(|(e, t, h, dead, protected)| (e, t.translation, !dead && h.current > 0.0 && !protected))
        .collect()
}

/// The seed the shuffled lists are ordered by (a map load's).
fn shuffle_key(world: &World, index: usize) -> u64 {
    let loads = world.get_resource::<MapLoads>().map_or(0, |l| l.0 as u64);
    super::objectives::hash((loads << 32) ^ index as u64 ^ 0x5CA1_AB1E)
}

/// The list a player of `team` spawns from in the running mode (S4): its
/// family's qualifying points (the map's start points, `CaMapObjects`),
/// per team or one list, shuffled once a map load where the mode says;
/// when the mode's family has none for the team, the map's own family's
/// (`world::map_family`); None when that is empty too (the map's first
/// point is used). On a map
/// without start point objects (the greybox, tests), Chimera's spawn
/// points: per team, or all of them for a one-list mode. Also the cursor
/// it uses (one-list modes share Alpha's).
pub fn spawn_list(world: &mut World, team: u8) -> (Vec<Transform>, usize) {
    let mode = current_mode(world);
    let spawns = mode.map_or(super::TEAM_POINTS, |m| m.spawns);
    let warmup = world.resource::<super::CaMatch>().warmup;
    let cursor = if spawns.one_list { 0 } else { side(team).unwrap_or(0) };
    // VIP Escort's escort side spawns at the `JSF` points (its swap).
    let team = super::escort::point_team(world, team);
    let starts = world.resource::<super::objects::CaMapObjects>().starts.clone();
    let mut list: Vec<(usize, Transform)> = if starts.is_empty() {
        let all = points(world, if spawns.one_list { None } else { Some(team) });
        all.into_iter().enumerate().collect()
    } else {
        let family = spawns.family.unwrap_or(super::super::world::SPAWN_GAME_TYPE);
        let locked = spawns.locked_rounds && !warmup;
        let list: Vec<(usize, Transform)> = starts
            .iter()
            .enumerate()
            .filter(|(_, p)| p.qualifies(family, locked) && (spawns.one_list || p.team == Some(team)))
            .map(|(i, p)| (i, p.at))
            .collect();
        if list.is_empty() {
            // The map has none of the mode's: its own family's, as one
            // list (`world::map_family`).
            let points = starts.iter().map(|p| (p.family.as_str(), !p.locked, p.spawn == 1, p.team.is_some()));
            match super::super::world::map_family(points) {
                Some(own) => starts
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| p.qualifies(own, false))
                    .map(|(i, p)| (i, p.at))
                    .collect(),
                None => list,
            }
        } else {
            list
        }
    };
    if spawns.shuffled {
        list.sort_by_key(|(i, _)| shuffle_key(world, *i));
    }
    (list.into_iter().map(|(_, t)| t).collect(), cursor)
}

/// The map's first start point (S4: an empty list's).
fn first_point(world: &mut World) -> Option<Transform> {
    let starts = &world.resource::<super::objects::CaMapObjects>().starts;
    if let Some(p) = starts.first() {
        return Some(p.at);
    }
    points(world, None).first().copied()
}

/// S4: the point `me` (on `team`) spawns at, its cursor advanced; None
/// without any point. Also whether every point tested was occupied.
pub fn pick(world: &mut World, me: Entity, team: u8) -> Option<(Transform, bool)> {
    let unit = inventory::UNIT;
    let others = others(world, me);
    let taken = |p: &Transform| others.iter().any(|(_, at, _)| occupied((*at - p.translation) / unit));
    if side(team).is_none() && !super::state::plays(team) {
        return first_point(world).map(|p| (p, taken(&p)));
    }
    let (list, i) = spawn_list(world, team);
    if list.is_empty() {
        return first_point(world).map(|p| (p, taken(&p)));
    }
    let mut cursor = world.resource::<CaSpawnCursors>().0[i];
    let mut full = true;
    for _ in 0..list.len() {
        cursor = (cursor + 1) % list.len();
        if !taken(&list[cursor]) {
            full = false;
            break;
        }
    }
    world.resource_mut::<CaSpawnCursors>().0[i] = cursor;
    Some((list[cursor], full))
}

/// S5: respawn `e` at its team's next point with a fresh loadout, full
/// health and armour, protected; telefrag who stands there.
pub fn respawn(world: &mut World, e: Entity) {
    if world.get_entity(e).is_err() {
        return;
    }
    let team = world.get::<Team>(e).map_or(0, |t| t.0);
    inventory::respawn_inventory(world, e);
    let picked = pick(world, e, team);
    if !spawning::revive(world, e, picked.map(|(t, _)| t)) {
        return;
    }
    world
        .entity_mut(e)
        .remove::<(CaAsked, CaHeld)>()
        .insert(CaArmor::default());
    protect(world, e);
    if let Some((at, true)) = picked {
        telefrag(world, e, at.translation);
    }
}

/// S4 step 5: every other living, unprotected player inside the doubled
/// box of `at` dies by crush damage credited to itself.
fn telefrag(world: &mut World, me: Entity, at: Vec3) {
    let unit = inventory::UNIT;
    let victims: Vec<(Entity, Vec3)> = others(world, me)
        .into_iter()
        .filter(|(_, p, alive)| *alive && occupied((*p - at) / unit))
        .map(|(e, p, _)| (e, p))
        .collect();
    for (v, p) in victims {
        world.write_message(Damage {
            target: v,
            attacker: Some(v),
            amount: 1.0e6,
            point: p,
            dir: Vec3::Y,
            hitgroup: Hitgroup::Generic,
            kind: DamageKind::Crush,
            weapon: Some(TELEFRAG),
            force: Vec3::ZERO,
        });
    }
}

/// S6: protect `e` from now (nothing with `protect_all` 0).
pub fn protect(world: &mut World, e: Entity) {
    let s = world.resource::<CaRuleSettings>().clone();
    if s.protect_all <= 0.0 {
        return;
    }
    let now = world.resource::<SimClock>().now;
    let kept = match world.get::<CaProtected>(e) {
        Some(p) => p.kept.clone(),
        None => world.get::<DamageFilter>(e).cloned(),
    };
    world.entity_mut(e).insert((
        CaProtected {
            since: now,
            bullets_until: now + s.protect_bullets as f64,
            all_until: now + s.protect_all as f64,
            kept,
        },
        DamageFilter {
            blocked: ALL_KINDS.to_vec(),
        },
    ));
}

/// Protection runs out: the player's own damage filter back.
pub(super) fn end_protection(
    q: Query<(Entity, &CaProtected)>,
    clock: Res<SimClock>,
    mut commands: Commands,
) {
    for (e, p) in &q {
        if clock.now + 1e-9 < p.all_until {
            continue;
        }
        let mut ent = commands.entity(e);
        ent.remove::<CaProtected>();
        match &p.kept {
            Some(f) => {
                ent.insert(f.clone());
            }
            None => {
                ent.remove::<DamageFilter>();
            }
        }
    }
}

/// S6: in the modes where it does (`CaMode::fire_ends_protection`),
/// firing a weapon ends both parts of the shooter's protection at once.
pub(super) fn firing_ends_protection(
    mut events: MessageReader<WeaponEvent>,
    protected: Query<(), With<CaProtected>>,
    settings: Res<CaRuleSettings>,
    mut commands: Commands,
) {
    let ends = super::mode_by_key(&settings.mode).is_some_and(|m| m.fire_ends_protection);
    for ev in events.read() {
        let fired = matches!(
            ev.kind,
            WeaponEventKind::Fired { .. } | WeaponEventKind::Swing { .. } | WeaponEventKind::Thrown
        );
        if !ends || ev.replay || !fired || protected.get(ev.owner).is_err() {
            continue;
        }
        let owner = ev.owner;
        commands.queue(move |w: &mut World| unprotect(w, owner));
    }
}

/// End `e`'s protection now (a mode where firing ends it, a match reset).
pub fn unprotect(world: &mut World, e: Entity) {
    let Some(p) = world.get::<CaProtected>(e).cloned() else { return };
    let mut ent = world.entity_mut(e);
    ent.remove::<CaProtected>();
    match p.kept {
        Some(f) => {
            ent.insert(f);
        }
        None => {
            ent.remove::<DamageFilter>();
        }
    }
}

/// S5's server check of `e`'s respawn request; Ok when it respawned.
pub fn request_respawn(world: &mut World, e: Entity) -> Result<(), String> {
    let Some(mode) = current_mode(world) else {
        return Err("no game mode".into());
    };
    let Some(dead) = world.get::<Dead>(e).copied() else {
        return Err("not dead".into());
    };
    if world.get::<CaHeld>(e).is_some() || world.get::<Connecting>(e).is_some() {
        return Err("held until the next round".into());
    }
    world.entity_mut(e).insert(CaAsked);
    let now = world.resource::<SimClock>().now;
    let wait = world.resource::<CaRuleSettings>().respawn_wait_server as f64;
    if !mode.respawns || !(mode.hooks.may_respawn)(world, e) {
        warn!("illegal respawn request: {} doesn't respawn", mode.key);
        return Err("this mode doesn't respawn".into());
    }
    if now - dead.since < wait - 1e-9 {
        warn!("illegal respawn request: {:.3} s after the death", now - dead.since);
        return Err("too early".into());
    }
    respawn(world, e);
    Ok(())
}

/// Players on this machine ask as their client would: once, after
/// `respawn_wait_client` s dead (strictly more), in a mode that
/// respawns. Remote players' clients ask themselves.
pub(super) fn ask_for_respawns(world: &mut World) {
    let Some(mode) = current_mode(world) else { return };
    if !mode.respawns {
        return;
    }
    let now = world.resource::<SimClock>().now;
    let wait = world.resource::<CaRuleSettings>().respawn_wait_client as f64;
    let due: Vec<Entity> = world
        .query_filtered::<(Entity, &Dead, &Team), (
            With<Intent>,
            Without<RemotePlayer>,
            Without<CaAsked>,
            Without<CaHeld>,
            Without<Connecting>,
            Without<Spectating>,
            Without<Npc>,
        )>()
        .iter(world)
        .filter(|(_, d, t)| super::state::plays(t.0) && now - d.since > wait + 1e-9)
        .map(|(e, ..)| e)
        .collect();
    for e in due {
        let _ = request_respawn(world, e);
    }
}

/// Hold `e` out until the next round setup (S1): dead with no body.
pub fn hold(world: &mut World, e: Entity) {
    let Ok(mut ent) = world.get_entity_mut(e) else { return };
    ent.insert((CaHeld, Dead { since: f64::MIN }, ColliderDisabled));
    if let Some(mut h) = ent.get_mut::<Health>() {
        h.current = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test case 3: the occupied test's bounds.
    #[test]
    fn occupied_bounds() {
        assert!(occupied(Vec3::new(29.9, 0.0, 0.0)));
        assert!(!occupied(Vec3::new(30.0, 0.0, 0.0)));
        assert!(occupied(Vec3::new(0.0, 99.9, 0.0)));
        assert!(!occupied(Vec3::new(0.0, 100.0, 0.0)));
    }

    #[test]
    fn map_order_from_the_name() {
        assert_eq!(map_index(Some(&Name::new("Spawn 11"))), 11);
        assert_eq!(map_index(None), usize::MAX);
    }
}
