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
//!   server is announced (`ChangeMatch`), as is the same id made again
//!   with another definition (an automatic gun game's new ladder), which a
//!   client then plays anew.
//! - **Changing match** (`change_match`, console `change_match <id>
//!   [map]`): the server starts another minigame, players kept and
//!   connected, scores anew, and changes level through the app's
//!   `changelevel` (a standalone game's `map`) when the map differs; only
//!   one every connected player can play there. A server rotates through
//!   it by itself once a match has been over for `RESULTS_HOLD` s
//!   (`serve_minigame`, `MinigameDef::rotate`) and puts the results up
//!   meanwhile (`NetMinigame::results`).
//! - **The playable set** (`playable`, `PartyOffer`, console
//!   `party_offer`): each registered minigame on its pool's first map
//!   against the players' mount doctor reports (their userinfo); the
//!   minigames every player can play go to the clients when the set or
//!   the players change (a lobby's list). `installed_by_all`: the games
//!   every player has (an automatic minigame's choice of games).
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
    minigame::{self, ActiveMinigame, MatchResults, EVENT_NAMES, MinigameDef, MinigameEvent, Minigames, ladder::Rung, runtime::InMinigame},
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
    /// The finished match's results (`minigame::MatchResults`), names as
    /// the players are known on the network; none while the match plays.
    pub results: Option<MatchResults>,
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
        .add_mapped_server_message::<MinigameNews>(Channel::Ordered)
        .add_server_message::<PartyOffer>(Channel::Ordered)
        .make_message_independent::<PartyOffer>();
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
    )
    .console_command(
        "party_offer",
        "The minigames every connected player can play (the server's mount doctor reports; a client: what \
         its server offers), and on the server who lacks what for the others.",
        |w, _| Ok(Some(offer_text(w))),
    );
    let server = || resource_equals(NetRole::Server);
    let client = || resource_equals(NetRole::Client);
    app.add_systems(FixedLast, serve_minigame.run_if(not(client())))
        .add_systems(
            PostUpdate,
            (send_news, send_offer).chain().before(ServerSystems::Send).run_if(server()),
        )
        .add_systems(
            PreUpdate,
            (apply_minigame, receive_news, receive_offer)
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

/// The definition `write_minigame` last put in `NetMinigame`.
#[derive(Resource)]
struct Announced(&'static MinigameDef);

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
        results: world.get_resource::<MatchResults>().cloned().map(|mut r| {
            for row in &mut r.rows {
                if let Some(c) = row.player.and_then(|p| world.get::<crate::net::NetCharacter>(p)) {
                    row.name = c.name.clone();
                }
            }
            r
        }),
    };
    // Another minigame, or its definition made again (same id, new text).
    let announced = world.get_resource::<Announced>().map(|a| a.0);
    let changed = have.as_ref().is_none_or(|(_, n)| n.id != now.id)
        || announced.is_some_and(|d| !std::ptr::eq(d, active.def));
    world.insert_resource(Announced(active.def));
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
    // Clients hear the match's new manifest.
    if changed {
        manifest::announce(world);
    }
}

/// Server -> clients: the minigames every connected player can play
/// (`playable`), by id in the registry's order; sent when it changes or
/// players come and go. On a client, the newest heard (a lobby's list).
#[derive(Resource, Message, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PartyOffer {
    pub minigames: Vec<String>,
}

/// Whether every connected player can play a minigame on a map
/// (`playable`).
#[derive(Clone, Debug, PartialEq)]
pub struct Playable {
    pub id: String,
    /// The map it would play on (`change_match`'s pick; the pool's first
    /// for the offer).
    pub map: String,
    /// Each player lacking a game it requires: the player's name and the
    /// game's id.
    pub lacking: Vec<(String, String)>,
}

impl Playable {
    pub fn ok(&self) -> bool {
        self.lacking.is_empty()
    }
}

/// The remote players' mount doctor reports (name, report), from their
/// userinfo (`manifest::MOUNTS_KEY`); a player that sent none isn't
/// listed (taken as having everything), nor are bots.
fn reports(world: &mut World) -> Vec<(String, manifest::MountReport)> {
    let mut q = world.query::<(&crate::net::NetCharacter, &chimera_core::core::UserInfo)>();
    q.iter(world)
        .filter(|(c, _)| c.owner.is_some())
        .filter_map(|(c, info)| {
            let r = manifest::from_userinfo(info.0.iter().map(|(k, v)| (k.as_str(), v.as_str())))?;
            Some((c.name.clone(), r))
        })
        .collect()
}

/// The games of `games` every player has installed: each remote player's
/// mount doctor report (one without a report has everything) and this
/// machine's own (`LocalMounts`, a listen host's or a standalone game's;
/// none: everything); a game not in `Games` (a stand-in) always. What
/// an automatic minigame is made of
/// (`chimera_sim::minigame::auto`).
pub fn installed_by_all(world: &mut World, games: &[&'static str]) -> Vec<&'static str> {
    let mut all = reports(world).into_iter().map(|(_, r)| r).collect::<Vec<_>>();
    if let Some(local) = world.get_resource::<manifest::LocalMounts>() {
        all.push(local.0.clone());
    }
    // A report lists the installed games only; a game that isn't a
    // registered one (no install to examine: a stand-in) counts as there.
    let registered = world.get_resource::<Games>().cloned();
    games
        .iter()
        .copied()
        .filter(|g| registered.as_ref().is_some_and(|r| r.get(g).is_none()) || all.iter().all(|r| r.has(g)))
        .collect()
}

/// Whether every connected player can play `def` on `map` (None: its
/// pool's first): each player's report against the games it requires
/// there (the map's game; `needs`).
pub fn playable_on(world: &mut World, def: &MinigameDef, map: Option<&str>) -> Playable {
    let map = map
        .map(crate::net::normalize_map)
        .or_else(|| def.maps.first().map(|m| crate::net::normalize_map(m)))
        .unwrap_or_else(|| GREYBOX.into());
    let mut needs = MatchManifest::default();
    if map != GREYBOX
        && let Some((game, _)) = map.split_once(':')
    {
        needs.need(game, true);
    }
    let mut lacking = Vec::new();
    for (name, report) in reports(world) {
        for n in needs.games.iter().filter(|n| n.required && !report.has(&n.game)) {
            lacking.push((name.clone(), n.game.clone()));
        }
    }
    Playable {
        id: def.id.clone(),
        map,
        lacking,
    }
}

/// Every registered minigame, on its pool's first map, against the
/// connected players (the lobby's playable set: `PartyOffer`).
pub fn playable(world: &mut World) -> Vec<Playable> {
    let defs: Vec<&'static MinigameDef> = world.get_resource::<Minigames>().map(|m| m.all().collect()).unwrap_or_default();
    defs.into_iter().map(|d| playable_on(world, d, None)).collect()
}

/// `party_offer`'s text.
fn offer_text(world: &mut World) -> String {
    if world.get_resource::<NetRole>() == Some(&NetRole::Client) {
        let offer = world.get_resource::<PartyOffer>().cloned().unwrap_or_default();
        return if offer.minigames.is_empty() {
            "the server offers no minigame".into()
        } else {
            offer.minigames.join("\n")
        };
    }
    let games = world.get_resource::<Games>().cloned();
    let lines: Vec<String> = playable(world)
        .iter()
        .map(|p| {
            if p.ok() {
                format!("{} on {}", p.id, p.map)
            } else {
                format!("({} on {}: {})", p.id, p.map, lacking_text(games.as_ref(), &p.lacking))
            }
        })
        .collect();
    if lines.is_empty() { "no minigames".into() } else { lines.join("\n") }
}

/// "Alice lacks CS:S (cs_source)", each player once.
fn lacking_text(games: Option<&Games>, lacking: &[(String, String)]) -> String {
    lacking
        .iter()
        .map(|(who, game)| format!("{who} lacks {}", manifest::game_name(games, game)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The server: the playable set to every client when it or the players
/// change.
fn send_offer(world: &mut World, mut last: Local<Option<(Vec<String>, usize)>>) {
    let players = world.query::<&crate::net::server::Player>().iter(world).count();
    let offer: Vec<String> = playable(world).into_iter().filter(|p| p.ok()).map(|p| p.id).collect();
    let now = (offer, players);
    if last.as_ref() == Some(&now) {
        return;
    }
    *last = Some(now.clone());
    if players == 0 {
        return;
    }
    world.write_message(ToClients {
        targets: SendTargets::CLIENTS_ONLY,
        message: PartyOffer { minigames: now.0 },
    });
}

/// A client: the server's offer, kept.
fn receive_offer(mut offers: MessageReader<PartyOffer>, mut commands: Commands) {
    if let Some(o) = offers.read().last() {
        commands.insert_resource(o.clone());
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
    // Only what every player can play (the playable set, M3).
    let p = playable_on(world, def, Some(&target));
    if !p.ok() {
        let games = world.get_resource::<Games>().cloned();
        return Err(format!(
            "not every player can play {id} on {target}: {}",
            lacking_text(games.as_ref(), &p.lacking)
        ));
    }
    minigame::start(world, id)?;
    if target != here {
        // A server moves its players with it; a standalone game (no
        // server) just loads the map.
        let command = if world.get_resource::<NetRole>() == Some(&NetRole::Server) { "changelevel" } else { "map" };
        world
            .resource_mut::<crate::console::Console>()
            .submit(format!("{command} {target}"));
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
    // A new minigame, or the same id with a new definition (the server
    // made it again: an automatic gun game's new ladder) that the
    // manifest put in place of the one playing.
    let current = world.get_resource::<Minigames>().and_then(|m| m.get(&n.id));
    if world
        .get_resource::<ActiveMinigame>()
        .is_none_or(|a| a.def.id != n.id || current.is_some_and(|c| !std::ptr::eq(c, a.def)))
    {
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
    if world.get_resource::<MatchResults>() != n.results.as_ref() {
        match n.results {
            Some(r) => world.insert_resource(r),
            None => {
                world.remove_resource::<MatchResults>();
            }
        }
    }
}

/// The minigame's end of the tick on a server or a standalone game: the
/// next map once the match has been over a while (`rotate`), and (a
/// server) the match into `NetMinigame`.
fn serve_minigame(world: &mut World, mut over_since: Local<Option<f64>>) {
    rotate(world, &mut over_since);
    if world.get_resource::<NetRole>() == Some(&NetRole::Server) {
        write_minigame(world);
    }
}

/// A server (or a standalone game): the match has been over for
/// `RESULTS_HOLD` s with its results up: on to the next map of the pool
/// that every player can play (`MinigameDef::rotate`; else the same map
/// anew), by `change_match`.
fn rotate(world: &mut World, over_since: &mut Option<f64>) {
    let over = world.get_resource::<Match>().is_some_and(|m| m.phase == Phase::MatchOver);
    let Some(active) = world.get_resource::<ActiveMinigame>().copied().filter(|_| over) else {
        *over_since = None;
        return;
    };
    let now = world.resource::<SimClock>().now;
    let since = *over_since.get_or_insert(now);
    if now < since + minigame::runtime::RESULTS_HOLD {
        return;
    }
    *over_since = None;
    let def = active.def;
    let here = crate::net::current_map(world);
    let pool: Vec<String> = def.maps.iter().map(|m| crate::net::normalize_map(m)).collect();
    // The next map in turn that every player can play, this one last.
    let mut target = here.clone();
    if def.rotate {
        let start = pool.iter().position(|m| *m == here).map_or(0, |i| i + 1);
        let candidates = (0..pool.len()).map(|i| pool[(start + i) % pool.len()].clone());
        let chosen = candidates.into_iter().find(|m| playable_on(world, def, Some(m)).ok());
        if let Some(m) = chosen {
            target = m;
        }
    }
    info!("{} is over: next, {} on {target}", def.id, def.name);
    if let Err(e) = change_match(world, &def.id, Some(&target)) {
        warn!("rotating {}: {e}", def.id);
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
