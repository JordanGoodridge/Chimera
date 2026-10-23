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
//! - **The manifest** (`net::manifest`): the server's hook adds the
//!   minigame (its id, its definition's SHA-256 and text), the games it
//!   names (wanted; `needs`) and its weapon sets; a client's hook plays the
//!   server's definition (its own if the hash matches, else the text, read
//!   and checked: `minigame::add`) or leaves saying why. A client starts a
//!   minigame only once a manifest names it. Another minigame on the
//!   server is announced (`ChangeMatch`).
//! - **Changing match** (`change_match`, console `change_match <id>
//!   [map]`): the server starts another minigame, players kept and
//!   connected, scores anew, and changes level through the app's
//!   `changelevel` when the map differs.
//!
//! Nothing is sent while no minigame plays.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    core::{NetRole, SimClock},
    net::{
        GREYBOX, NetAppExt, NetHooks,
        game::{ServerTime, tick_of},
        manifest::{self, CANT_PLAY, GameNeed, MatchManifest, MinigameOffer, ServerManifest},
        maps::sha256,
        ordered::OrderedAppExt,
    },
    minigame::{self, ActiveMinigame, EVENT_NAMES, MinigameDef, MinigameEvent, Minigames, ladder::Rung, runtime::InMinigame},
    mode::{Match, Phase},
};
use chimera_core::game_registry::Games;

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
    {
        let mut hooks = app.world_mut().resource_mut::<NetHooks>();
        hooks.stopped_serving.push(stopped_serving);
        hooks.manifest.push(add_to_manifest);
        hooks.accept.push(accept_minigame);
    }
    use crate::console::ConsoleAppExt;
    app.console_command(
        "change_match",
        "change_match <minigame> [map]: play another minigame, the players kept and connected, its scores \
         anew; on [map], else on this map if it is in the minigame's pool, else on the pool's first.",
        |w, a| {
            let id = a.first().ok_or("change_match <minigame> [map]")?;
            change_match(w, id, a.get(1).map(String::as_str)).map(Some)
        },
    );
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
    let changed = have.as_ref().is_none_or(|(_, n)| n.id != now.id);
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
    // Another minigame: clients hear the match's new manifest.
    if changed {
        manifest::announce(world);
    }
}

/// `change_match <id> [map]` (M1): the server plays minigame `id` from
/// now on, its players kept and connected: a fresh match (`minigame::
/// start`: scores anew, everyone joins again), clients told by
/// `ChangeMatch` (`manifest::announce`, from `write_minigame`). The map is
/// `map`, else this one if it is in the minigame's pool, else the pool's
/// first; another map is the app's `changelevel` (which takes every client
/// along with the new match's manifest). A client can't change match.
pub fn change_match(world: &mut World, id: &str, map: Option<&str>) -> Result<String, String> {
    if world.get_resource::<NetRole>() == Some(&NetRole::Client) {
        return Err("the server picks the minigame; you are a client of it".into());
    }
    let def = world
        .get_resource::<Minigames>()
        .and_then(|m| m.get(id))
        .ok_or_else(|| format!("no minigame `{id}` (minigames lists them)"))?;
    let here = crate::net::current_map(world);
    let pool: Vec<String> = def.maps.iter().map(|m| crate::net::normalize_map(m)).collect();
    let target = match map {
        Some(m) => crate::net::normalize_map(m),
        None if pool.contains(&here) => here.clone(),
        None => pool.first().cloned().unwrap_or_else(|| here.clone()),
    };
    minigame::start(world, id)?;
    if target != here {
        world
            .resource_mut::<crate::console::Console>()
            .submit(format!("changelevel {target}"));
        return Ok(format!("playing {id}: {}; changing level to {target}", def.name));
    }
    Ok(format!("playing {id}: {} on {target}", def.name))
}

/// The games minigame `def` reads from a player's install: its first
/// map's game required (none for the greybox), and wanted, every other
/// registered game it names (its slots, movement, character, weapon sets
/// and kit). Ids that aren't registered games (`chimera`) are left out;
/// without `Games` every id counts.
pub fn needs(def: &MinigameDef, games: Option<&Games>) -> Vec<GameNeed> {
    let mut m = MatchManifest::default();
    let first = def.maps.first().map(|m| crate::net::normalize_map(m)).unwrap_or_default();
    if first != GREYBOX
        && let Some((game, _)) = first.split_once(':')
    {
        m.need(game, true);
    }
    for g in wanted(def) {
        m.need(&g, false);
    }
    m.games.retain(|n| games.is_none_or(|gs| gs.get(&n.game).is_some()));
    m.games
}

/// The game ids a definition names besides its maps.
fn wanted(def: &MinigameDef) -> Vec<String> {
    let s = &def.slots;
    let prefix = |id: &str| id.split_once(':').map(|(g, _)| g.to_string());
    let mut out: Vec<String> = [&s.hud, &s.binds, &s.tick, &s.bots].into_iter().flatten().cloned().collect();
    out.extend([&s.movement, &s.character].into_iter().flatten().filter_map(|id| prefix(id)));
    out.extend(s.weapons.iter().cloned());
    out.extend(def.spawn.kit.iter().filter_map(|id| prefix(id)));
    out
}

/// The server's manifest hook: the playing minigame (its id, its
/// definition's hash and text), the games it names (wanted), its weapon
/// sets and the slots it names over the gameplay's.
fn add_to_manifest(world: &mut World, m: &mut MatchManifest) {
    let Some(active) = world.get_resource::<ActiveMinigame>().copied() else {
        return;
    };
    let def = active.def;
    let games = world.get_resource::<Games>();
    // Its pool's first map's game too, wanted: the map played is the
    // served one (`manifest::current` requires its game).
    for n in needs(def, games) {
        m.need(&n.game, false);
    }
    m.weapons = def.slots.weapons.clone();
    for (slot, game) in [("hud", &def.slots.hud), ("binds", &def.slots.binds), ("bots", &def.slots.bots)] {
        if let Some(g) = game
            && let Some(s) = m.slots.iter_mut().find(|(s, _)| s == slot)
        {
            s.1 = g.clone();
        }
    }
    let text = world.get_resource::<Minigames>().and_then(|ms| ms.text(&def.id)).unwrap_or_default();
    m.minigame = Some(MinigameOffer {
        id: def.id.clone(),
        hash: sha256(text.as_bytes()),
        text: text.to_string(),
    });
}

/// A client's manifest hook: the server's minigame is one this build can
/// play: its own definition with the same hash, else the server's text,
/// read and checked against what this build registered (`minigame::add`).
fn accept_minigame(world: &mut World, m: &MatchManifest) -> Result<(), String> {
    let Some(offer) = &m.minigame else {
        return Ok(());
    };
    let have = world.get_resource::<Minigames>().and_then(|ms| ms.text(&offer.id));
    if have.is_some_and(|t| sha256(t.as_bytes()) == offer.hash) {
        return Ok(());
    }
    if sha256(offer.text.as_bytes()) != offer.hash {
        return Err(format!("{CANT_PLAY}{} came with a definition that doesn't match its hash.", offer.id));
    }
    let def = minigame::add(world, &offer.id, &offer.text)
        .map_err(|e| format!("{CANT_PLAY}{} can't be played by this build: {e}", offer.id))?;
    if def.id != offer.id {
        return Err(format!("{CANT_PLAY}{}'s definition names itself {}.", offer.id, def.id));
    }
    info!("playing the server's definition of {}", offer.id);
    Ok(())
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
    // The definition comes with the match's manifest (`accept_minigame`):
    // wait for the manifest that names it.
    let offered = world
        .get_resource::<ServerManifest>()
        .and_then(|m| m.0.minigame.as_ref())
        .is_some_and(|o| o.id == n.id);
    if !offered {
        return;
    }
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
