//! A minigame over the network (docs/plans/active/multiplayer.md), as a
//! part of the network (`net::NetAppExt::net_part`, registered by
//! `MinigameNetPlugin`, named `minigame`; an app that plays minigames over
//! the network adds it beside `NetPlugin`): the
//! server plays it (`chimera_sim::minigame`, its runtime and rules module
//! on the server only) and a client shows it, so the HUD it draws
//! (`minigame::hud`: the rung and next gun, the leader, the match's
//! state and time, notices) reads what it always reads.
//!
//! - **The match** (`NetMinigame`, one replicated entity): the minigame's
//!   id, the match's phase (`phase::*`) and when the round ends (a server
//!   tick). A client starts the same minigame as its gameplay when it
//!   learns the id (`minigame::join_remote`: the slots, the rules module's
//!   parameters, no restart) and keeps its `Match` in step, times on its
//!   own clock (`game::ServerTime`).
//! - **The players**: `InMinigame` and the rules module's own state
//!   (`ladder::Rung`) replicate on the characters, so the HUD finds who
//!   plays and who leads; kills and deaths are `NetScore`'s.
//! - **Events** (`MinigameNews`): rung ups, a match won, for the HUD's
//!   notices, as the index in `minigame::EVENT_NAMES`.
//!
//! Nothing is sent while no minigame plays.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    core::{NetRole, SimClock},
    net::{
        NetAppExt, NetHooks,
        game::{ServerTime, tick_of},
        ordered::OrderedAppExt,
    },
    minigame::{ActiveMinigame, EVENT_NAMES, MinigameEvent, ladder::Rung, runtime::InMinigame},
    mode::{Match, Phase},
};

/// `NetMinigame::phase`.
pub mod phase {
    /// Waiting for players, or about to start.
    pub const WAITING: u8 = 0;
    /// Live: kills count, the round's timer runs.
    pub const LIVE: u8 = 1;
    /// A round or the match is over.
    pub const OVER: u8 = 2;
}

/// The playing minigame and its match, on one replicated entity the server
/// keeps (`write_minigame`). Times are server ticks (fractional), which a
/// client turns into its own clock (`game::ServerTime`).
#[derive(Component, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct NetMinigame {
    /// The minigame's id (`originals:gun_game_dust2`).
    pub id: String,
    /// `phase::*`.
    pub phase: u8,
    /// Live: when the round's timer reaches 0.
    pub ends: Option<f64>,
}

/// Server -> clients: something happened in the match
/// (`minigame::MinigameEvent`): its name's place in `EVENT_NAMES`, who it
/// concerns (a placeholder for no one) and its number.
#[derive(Message, Serialize, Deserialize, Clone, Debug, PartialEq, bevy::ecs::entity::MapEntities)]
pub struct MinigameNews {
    pub name: u8,
    #[entities]
    pub player: Option<Entity>,
    pub value: i32,
}

/// The minigame's part of the network (`register`).
pub struct MinigameNetPlugin;

impl Plugin for MinigameNetPlugin {
    fn build(&self, app: &mut App) {
        app.net_part("minigame", register);
    }
}

/// The protocol (in this order on every side) and the systems.
fn register(app: &mut App) {
    // The events exist in an app without a minigame runtime too.
    app.add_message::<MinigameEvent>();
    app.replicate::<NetMinigame>()
        .ordered::<NetMinigame>()
        .replicate::<InMinigame>()
        .replicate::<Rung>()
        .ordered::<Rung>()
        .add_mapped_server_message::<MinigameNews>(Channel::Ordered);
    app.world_mut().resource_mut::<NetHooks>().stopped_serving.push(stopped_serving);
    let server = || resource_equals(NetRole::Server);
    let client = || resource_equals(NetRole::Client);
    app.add_systems(FixedLast, write_minigame.run_if(server()))
        .add_systems(PostUpdate, send_news.before(ServerSystems::Send).run_if(server()))
        .add_systems(
            PreUpdate,
            (apply_minigame, receive_news)
                .chain()
                .after(ClientSystems::Receive)
                .run_if(client()),
        );
}

/// Serving stopped: the match's entity goes, and the players stop
/// replicating the minigame's marks.
fn stopped_serving(world: &mut World) {
    let entities: Vec<Entity> = world.query_filtered::<Entity, With<NetMinigame>>().iter(world).collect();
    for e in entities {
        world.despawn(e);
    }
}

/// The minigame and the match into `NetMinigame` (gone with the minigame).
fn write_minigame(world: &mut World) {
    let have = world.query::<(Entity, &NetMinigame)>().iter(world).next().map(|(e, n)| (e, n.clone()));
    let Some(active) = world.get_resource::<ActiveMinigame>().copied() else {
        if let Some((e, _)) = have {
            world.despawn(e);
        }
        return;
    };
    let clock = *world.resource::<SimClock>();
    let m = world.get_resource::<Match>().cloned().unwrap_or_default();
    let now = NetMinigame {
        id: active.def.id.clone(),
        phase: match m.phase {
            Phase::Live => phase::LIVE,
            Phase::RoundOver { .. } | Phase::Intermission { .. } | Phase::MatchOver => phase::OVER,
            Phase::Waiting | Phase::Starting | Phase::Setup { .. } => phase::WAITING,
        },
        ends: m.round_ends.filter(|_| m.phase == Phase::Live).map(|s| tick_of(&clock, s)),
    };
    match have {
        Some((e, n)) => {
            if n != now {
                world.entity_mut(e).insert(now);
            }
        }
        None => {
            world.spawn((Name::new("Minigame"), Replicated, now));
        }
    }
}

/// The match's events, to every client.
fn send_news(
    mut events: MessageReader<MinigameEvent>,
    replicated: Query<(), With<Replicated>>,
    mut out: MessageWriter<ToClients<MinigameNews>>,
) {
    for e in events.read() {
        let Some(name) = EVENT_NAMES.iter().position(|n| *n == e.name) else {
            warn!("minigame event `{}` isn't in minigame::EVENT_NAMES: not sent", e.name);
            continue;
        };
        out.write(ToClients {
            targets: SendTargets::CLIENTS_ONLY,
            message: MinigameNews {
                name: name as u8,
                player: e.player.filter(|p| replicated.contains(*p)),
                value: e.value,
            },
        });
    }
}

/// The server's minigame and match into what the HUD reads: the minigame
/// started here when it is a new one, the match's phase and end.
fn apply_minigame(world: &mut World, mut refused: Local<Option<String>>) {
    let Some(n) = world.query::<&NetMinigame>().iter(world).next().cloned() else {
        return;
    };
    if world.get_resource::<ActiveMinigame>().is_none_or(|a| a.def.id != n.id) {
        match crate::minigame::join_remote(world, &n.id) {
            Ok(def) => {
                info!("the server plays {}: {}", def.id, def.name);
                *refused = None;
            }
            Err(e) => {
                if refused.as_deref() != Some(&n.id) {
                    error!("the server plays minigame {}, which this build can't start: {e}", n.id);
                    *refused = Some(n.id.clone());
                }
                return;
            }
        }
    }
    let local = ServerTime::of(world).map(|t| {
        let now = world.resource::<SimClock>().now;
        move |tick: f64| now + (tick - t.tick) * t.step
    });
    let (phase, ends) = match n.phase {
        phase::LIVE => (Phase::Live, n.ends.zip(local.as_ref()).map(|(e, f)| f(e))),
        phase::OVER => (Phase::MatchOver, None),
        _ => (Phase::Waiting, None),
    };
    let mut m = world.get_resource_or_init::<Match>();
    if m.phase != phase {
        m.phase = phase;
    }
    if m.round_ends != ends {
        m.round_ends = ends;
    }
}

/// The server's events back into `MinigameEvent`s (the HUD's notices).
fn receive_news(mut news: MessageReader<MinigameNews>, mut events: MessageWriter<MinigameEvent>) {
    for n in news.read() {
        let Some(name) = EVENT_NAMES.get(n.name as usize) else { continue };
        events.write(MinigameEvent {
            name,
            player: n.player.filter(|p| *p != Entity::PLACEHOLDER),
            value: n.value,
        });
    }
}
