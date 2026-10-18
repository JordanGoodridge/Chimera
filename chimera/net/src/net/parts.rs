//! What a game (and Source's map logic) adds to the network, without the
//! network naming it (docs/plans/active/engine-split.md, "Remaining
//! couplings" (a)-(e)).
//!
//! - **Parts** (`NetParts`, `NetAppExt::net_part`): a game registers a
//!   function by name in its plugin's `build`; `NetPlugin::finish` calls
//!   them in name order, after the engine's own protocol, so client and
//!   server builds register the same protocol whichever order an app adds
//!   its plugins in. A part registers its replicated components, messages
//!   and events with replicon (with `ordered`, `interp::write_snapshot`
//!   and `interp::hold_heard` for buffered ones), its systems on both
//!   sides, and fills the hooks below. CS:S's is `cs_source::game::net`
//!   (the round, bomb and hostages, team and class requests, round ends
//!   and objective news); Source's logic's is `logic::net` (movers, HUD
//!   messages).
//! - **Hooks** the network calls where it used to name a game's rules:
//!   `JoinRules` (how a remote player comes in: the join menus or a team,
//!   out of the game while its client loads the map, in when it has it),
//!   `BotHooks` (kick a bot, the quota in step with the bots there are;
//!   bots are `core::Bot`), `Holds` (when the rules' freeze ends and what
//!   a frozen player's intent keeps, for the client's prediction) and
//!   `NetHooks` (what a part takes off when this process stops serving;
//!   what it places before movement in each replayed tick). Each has a
//!   default for a game without join rules of its own (Combat Arms so
//!   far): players join on the smaller team (the network's pick) and come
//!   in at a free spawn point of their team, else any (`enter_at_spawn`),
//!   bots are despawned, nothing is held.
//! - **Ordering**: a part orders its client-side holds before
//!   `predict::RecordCommand` (CS:S's `BombHold`), as the rules' freeze is
//!   applied in `SimSet::Rules`.

use std::collections::BTreeMap;

use bevy::prelude::*;

use avian3d::prelude::Position;

use crate::core::{Connecting, Dead, Health, Intent, SpawnPoint, Team, Velocity};

/// The parts registered so far, by name, waiting for `NetPlugin::finish`
/// (`NetAppExt::net_part`).
#[derive(Resource, Default)]
pub struct NetParts(BTreeMap<&'static str, fn(&mut App)>);

/// Registering a part of the network (`NetParts`).
pub trait NetAppExt {
    /// `register` runs when the network finishes building (`NetPlugin`'s
    /// `finish`), in name order with the other parts; nothing runs
    /// without a `NetPlugin`. A name registered again replaces the first.
    fn net_part(&mut self, name: &'static str, register: fn(&mut App)) -> &mut Self;
}

impl NetAppExt for App {
    fn net_part(&mut self, name: &'static str, register: fn(&mut App)) -> &mut Self {
        self.world_mut().get_resource_or_init::<NetParts>().0.insert(name, register);
        self
    }
}

/// Every registered part, in name order (`NetPlugin::finish`).
pub(super) fn apply(app: &mut App) {
    let Some(parts) = app.world_mut().remove_resource::<NetParts>() else {
        return;
    };
    for register in parts.0.into_values() {
        register(app);
    }
}

/// What the parts add to the network's own steps.
#[derive(Resource, Default)]
pub struct NetHooks {
    /// This process stopped serving: after the engine took its replicated
    /// components off what stays (the host, bots), each part takes its
    /// own off (and despawns what it spawned only to replicate).
    pub stopped_serving: Vec<fn(&mut World)>,
    /// A client's prediction replays a tick: called with the tick's clock
    /// and intent set, before movement, so a part places what the player
    /// moves against at that tick (Source's movers).
    pub replay: Vec<fn(&mut World)>,
}

/// How a remote player comes into the game (the game's join flow; CS:S's
/// `rules::JoinChoice`, `hold_out`, `enter_game`).
#[derive(Resource, Clone, Copy)]
pub struct JoinRules {
    /// A remote player's character was just spawned (on the smaller team,
    /// dead: `core::Dead`), with what its client asked for (its
    /// `server::JOIN_KEY` userinfo value): hold it out of the game while
    /// its client loads the map (`core::Connecting`) and put it where it
    /// asked (the join menus, a team, the spectators).
    pub joined: fn(&mut World, Entity, Option<&str>),
    /// Its client has the map: into the game, spawned as the rules allow.
    pub enter: fn(&mut World, Entity),
    /// A client: what it asks for (`server::JOIN_KEY`'s value in its
    /// userinfo).
    pub choice: fn(&World) -> String,
}

impl Default for JoinRules {
    fn default() -> Self {
        Self {
            joined: |w, e, _| {
                if let Ok(mut e) = w.get_entity_mut(e) {
                    e.insert(Connecting);
                }
            },
            enter: enter_at_spawn,
            choice: |_| "auto".into(),
        }
    }
}

/// How close (m, across) another living character may stand to a spawn
/// point for it to be free: about a player's width.
const SPAWN_CLEARANCE: f32 = 0.8;

/// The default way into the game (`JoinRules::enter`): out of
/// `Connecting` and alive with full health, still, at the first spawn
/// point of its team (`core::SpawnPoint`; none: any) that no living
/// character stands on (else the first of them), looking the way it
/// faces. Without spawn points it stays where it was spawned.
pub fn enter_at_spawn(world: &mut World, e: Entity) {
    if world.get_entity(e).is_err() {
        return;
    }
    let team = world.get::<Team>(e).copied();
    let spawns: Vec<(Transform, Option<Team>)> = world
        .query::<(&Transform, &SpawnPoint)>()
        .iter(world)
        .map(|(t, s)| (*t, s.team))
        .collect();
    let own: Vec<Transform> = spawns
        .iter()
        .filter(|(_, t)| t.is_some() && *t == team)
        .map(|(t, _)| *t)
        .collect();
    let pool: Vec<Transform> = if own.is_empty() {
        spawns.iter().map(|(t, _)| *t).collect()
    } else {
        own
    };
    let taken: Vec<Vec3> = world
        .query_filtered::<(Entity, &Transform, &Health), (With<Intent>, Without<Dead>, Without<Connecting>)>()
        .iter(world)
        .filter(|(o, _, h)| *o != e && h.current > 0.0)
        .map(|(_, t, _)| t.translation)
        .collect();
    let free = |t: &&Transform| {
        !taken.iter().any(|p| {
            let d = *p - t.translation;
            d.x.abs() < SPAWN_CLEARANCE && d.z.abs() < SPAWN_CLEARANCE && d.y.abs() < 2.0
        })
    };
    let at = pool.iter().find(free).or(pool.first()).copied();
    let mut ent = world.entity_mut(e);
    ent.remove::<(Connecting, Dead)>();
    if let Some(mut h) = ent.get_mut::<Health>() {
        h.current = h.max;
    }
    if let Some(mut v) = ent.get_mut::<Velocity>() {
        v.0 = Vec3::ZERO;
    }
    let Some(at) = at else { return };
    if let Some(mut t) = ent.get_mut::<Transform>() {
        t.translation = at.translation;
    }
    if let Some(mut p) = ent.get_mut::<Position>() {
        p.0 = at.translation;
    }
    if let Some(mut i) = ent.get_mut::<Intent>() {
        let (yaw, _, _) = at.rotation.to_euler(EulerRot::YXZ);
        i.yaw = yaw;
        i.pitch = 0.0;
    }
}

/// Bots (`core::Bot`) as the game keeps them (CS:S's `bot_quota`).
#[derive(Resource, Clone, Copy)]
pub struct BotHooks {
    /// Remove this bot (`kick <name>`), the quota one lower.
    pub kick: fn(&mut World, Entity),
    /// Serving starts: bring the quota in step with the bots there are.
    pub sync_quota: fn(&mut World),
}

impl Default for BotHooks {
    fn default() -> Self {
        Self {
            kick: |w, e| {
                if let Ok(e) = w.get_entity_mut(e) {
                    e.despawn();
                }
            },
            sync_quota: |_| {},
        }
    }
}

/// What the rules hold a player to, for a client predicting its own (the
/// server sends whether it is held: `OwnState::held`, `frozen_until`).
#[derive(Resource, Clone, Copy)]
pub struct Holds {
    /// The server: when the rules' freeze (`core::FreezeTime`) ends
    /// (`SimClock::now`, s), if they know; else it holds until it lifts.
    pub freeze_ends: fn(&World) -> Option<f64>,
    /// What a frozen player's intent keeps (CS:S's: the look, weapon picks
    /// and reload).
    pub frozen: fn(&Intent) -> Intent,
}

impl Default for Holds {
    fn default() -> Self {
        Self {
            freeze_ends: |_| None,
            frozen: |i| Intent {
                yaw: i.yaw,
                pitch: i.pitch,
                command: i.command,
                ..default()
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Order(Vec<&'static str>);

    #[test]
    fn parts_register_in_name_order_whatever_the_plugin_order() {
        let mut app = App::new();
        app.init_resource::<Order>();
        app.net_part("source_logic", |app| app.world_mut().resource_mut::<Order>().0.push("source_logic"))
            .net_part("cs_source", |app| app.world_mut().resource_mut::<Order>().0.push("cs_source"));
        apply(&mut app);
        assert_eq!(app.world().resource::<Order>().0, ["cs_source", "source_logic"]);
        // Applied once: the registrations are gone.
        assert!(!app.world().contains_resource::<NetParts>());
    }
}
