//! Combat Arms over the network (docs/plans/active/combat-arms.md, "Network
//! part"): what the game adds to Chimera's (`net`), so a listen server or
//! a client joining one plays Combat Arms as single player does.
//!
//! Most of it is Chimera's and needs nothing here: characters, health,
//! teams and deaths replicated; each client predicting its own player
//! (movement, the inventory and its weapons' parts) and corrected from the
//! server's state; others' shots, swings and held weapons sent to the
//! other clients; lag compensation's history; the default join flow
//! (`net::parts::enter_at_spawn`: on the smaller team, in at a free spawn
//! point once the map is loaded).
//!
//! What the game's parts register themselves, so prediction runs them on a
//! client (with or without this plugin):
//!
//! - `movement`: `CaMovement`, the whole movement state, as a networked
//!   predicted component;
//! - `weapons`: the spread meters (`CaSpread`), the kick (`CaRecoil`), the
//!   zoom (`CaZoom`; the owner's `Zoomed` and sway on its `ViewPunch` are
//!   Chimera's predicted parts) and a minigun's spin (`CaSpin`) as
//!   predicted weapon parts (`weapon::NetPart`), beside Chimera's
//!   (timers, burst, clip, fire mode: `WeaponState`, `Magazine`,
//!   `AltModes`); the held weapon's movement penalty in the predicted
//!   tick; the server's rounds lag compensated (`weapon::lagcomp`, the
//!   shooter's view tick); a client deals no damage;
//! - `inventory`: the selection (`CaSelection`: class cursors, last
//!   chosen, Q, the sprint pose; weapons as inventory indices) and the
//!   knife's attack (`Knife`'s `KnifeState`);
//! - `grenades`: the throw is Chimera's predicted `Throwable` (pin,
//!   release, post-fire, the count) with the sprint gate in the predicted
//!   tick; the grenade, its blast, flash timers and fire are the server's,
//!   and Chimera's network carries them: the grenade in flight as a
//!   `NetItem`, its detonation as `Detonation`, a flash as the victim's
//!   `Senses` (`Blinded`), a smoke cloud as `NetSmoke`, damage as health.
//!
//! What this part adds (`CaNetPlugin`, registered as `combat_arms`):
//!
//! - **Armour** (`weapons::CaArmor`): the server's, replicated in tick order
//!   to every client (the HUD's armour points; others' for spectating).
//! - **The pickup prompt** (`CaPickup`): the weapon id of the dropped item
//!   a player's view is on, the server's (`inventory::pickups` runs there),
//!   for its HUD.
//! - **The match** (`NetCaMatch`, one replicated entity): the mode, its
//!   goal, the phase, the team counters, round wins and number, the round
//!   timer's end (server ticks), the winner and result, the players per
//!   team; a client writes it into `rules::CaMatchView`, its times on its
//!   own clock (`net::game::ServerTime`), which the HUD reads as single
//!   player's. Each player's statistics (`rules::CaStats`: kills, deaths,
//!   assists, ...) replicated, for the scoreboard.
//! - **Respawning** (game_modes.md S5): a client's player, dead for more
//!   than `respawn_wait_client` s by its own clock in a mode that
//!   respawns, asks once (`CaRespawnRequest`); the server checks it
//!   (`rules::spawns::request_respawn`).
//! - **Hooks**: the join flow (`net::JoinRules::enter`:
//!   `rules::flow::enter_game`, S1: in at once, or held until the next
//!   round), the setup's freeze for a client's prediction (`net::Holds`:
//!   `rules::flow::freeze_ends`, `frozen_intent`).
//!
//! Known shortcuts: the knife's ray isn't lag compensated (it hits others
//! where the server has them); armour, the pickup prompt and the
//! statistics go to every client, not only the player's own; a refused
//! respawn request isn't told to the client (it stays dead, as S5 has
//! it); listen server only (no Combat Arms dedicated server binary).

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use super::{
    inventory::{CaSelection, Dropped},
    rules::{
        CaMatch, CaMatchView, CaRuleSettings, CaStats, CaTeamCounts, Phase, current_mode,
        flow::{self, enter_game},
        spawns,
    },
    weapons::CaArmor,
};
use crate::{
    core::{Dead, LocalPlayer, NetRole, SimClock},
    net::{
        Holds, JoinRules, NetAppExt, NetHooks,
        game::{ServerTime, tick_of},
        ordered::OrderedAppExt,
        server::Player,
    },
    weapon::Weapon,
};

/// Combat Arms' part of the network (`register`), named `combat_arms`.
pub struct CaNetPlugin;

impl Plugin for CaNetPlugin {
    fn build(&self, app: &mut App) {
        app.net_part("combat_arms", register);
    }
}

/// The weapon id of the dropped item a player's view is on (the server's
/// `CaSelection::pickup_target`), for its HUD's pickup prompt.
#[derive(Component, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CaPickup(pub Option<String>);

/// The protocol (in this order on every side) and the systems.
fn register(app: &mut App) {
    let server = || resource_equals(NetRole::Server);
    let client = || resource_equals(NetRole::Client);
    app.replicate::<CaArmor>()
        .ordered::<CaArmor>()
        .replicate::<CaPickup>()
        .ordered::<CaPickup>()
        .replicate::<CaStats>()
        .ordered::<CaStats>()
        .replicate::<NetCaMatch>()
        .ordered::<NetCaMatch>()
        .add_client_message::<CaRespawnRequest>(Channel::Ordered)
        .insert_resource(JoinRules {
            enter: enter_game,
            ..default()
        })
        .insert_resource(Holds {
            freeze_ends: flow::freeze_ends,
            frozen: flow::frozen_intent,
        })
        .add_systems(FixedLast, (write_pickups, write_match).run_if(server()))
        .add_systems(
            PreUpdate,
            receive_respawns
                .after(ServerSystems::Receive)
                .run_if(in_state(ServerState::Running)),
        )
        .add_systems(
            PreUpdate,
            apply_match
                .after(ClientSystems::Receive)
                .after(crate::net::predict::rebuild_own_states)
                .run_if(client()),
        )
        .add_systems(PostUpdate, ask_to_respawn.before(ClientSystems::Send).run_if(client()));
    app.world_mut()
        .resource_mut::<NetHooks>()
        .stopped_serving
        .push(stopped_serving);
}

/// The match, on one replicated entity the server keeps (`write_match`).
/// Times are server ticks (fractional).
#[derive(Component, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct NetCaMatch {
    /// The room key; empty: no mode.
    pub mode: String,
    pub goal: u32,
    /// `match_phase::*`.
    pub phase: u8,
    /// The phase's end (setup, round over, intermission), server ticks.
    pub until: f64,
    pub warmup: bool,
    pub round: u32,
    pub score: [i32; 2],
    pub round_wins: [u32; 2],
    /// Live: when the round timer reaches 0, server ticks.
    pub round_ends: Option<f64>,
    pub winner: Option<u8>,
    pub result: Option<Option<u8>>,
    pub team_counts: [u32; 2],
}

/// `NetCaMatch::phase`.
pub mod match_phase {
    pub const WAITING: u8 = 0;
    pub const STARTING: u8 = 1;
    pub const SETUP: u8 = 2;
    pub const LIVE: u8 = 3;
    pub const ROUND_OVER: u8 = 4;
    pub const INTERMISSION: u8 = 5;
    pub const MATCH_OVER: u8 = 6;
}

/// A client's player asks to respawn (game_modes.md S5).
#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct CaRespawnRequest;

/// The server's match into `NetCaMatch`.
fn write_match(world: &mut World) {
    let clock = *world.resource::<SimClock>();
    let t = |s: f64| tick_of(&clock, s);
    let m = world.resource::<CaMatch>().clone();
    let (phase, until) = match m.phase {
        Phase::Waiting => (match_phase::WAITING, 0.0),
        Phase::Starting => (match_phase::STARTING, 0.0),
        Phase::Setup { until } => (match_phase::SETUP, t(until)),
        Phase::Live => (match_phase::LIVE, 0.0),
        Phase::RoundOver { until } => (match_phase::ROUND_OVER, t(until)),
        Phase::Intermission { until } => (match_phase::INTERMISSION, t(until)),
        Phase::MatchOver => (match_phase::MATCH_OVER, 0.0),
    };
    let now = NetCaMatch {
        mode: current_mode(world).map(|m| m.key.to_string()).unwrap_or_default(),
        goal: world.resource::<CaRuleSettings>().goal(),
        phase,
        // Whole ticks, so they don't change with the clock's rounding.
        until: until.round(),
        warmup: m.warmup,
        round: m.round,
        score: m.score,
        round_wins: m.round_wins,
        round_ends: m.round_ends.map(|e| t(e).round()),
        winner: m.winner,
        result: m.result,
        team_counts: world.resource::<CaTeamCounts>().0,
    };
    let have = world.query::<(Entity, &NetCaMatch)>().iter(world).next().map(|(e, r)| (e, r.clone()));
    match have {
        Some((e, r)) => {
            if r != now {
                world.entity_mut(e).insert(now);
            }
        }
        None => {
            world.spawn((Name::new("Match"), Replicated, now));
        }
    }
}

/// A client asked to respawn its player.
fn receive_respawns(mut requests: MessageReader<FromClient<CaRespawnRequest>>, players: Query<&Player>, mut commands: Commands) {
    for r in requests.read() {
        let Some(client) = r.client_id.entity() else { continue };
        let Ok(p) = players.get(client) else { continue };
        let character = p.character;
        commands.queue(move |w: &mut World| {
            if w.get_entity(character).is_ok()
                && let Err(why) = spawns::request_respawn(w, character)
            {
                debug!("respawn request refused: {why}");
            }
        });
    }
}

/// The server's match into `CaMatchView`, on this client's clock.
fn apply_match(world: &mut World) {
    let Some(r) = world.query::<&NetCaMatch>().iter(world).next().cloned() else {
        return;
    };
    let Some(time) = ServerTime::of(world) else { return };
    let t = |tick: f64| time.local(tick);
    let phase = match r.phase {
        match_phase::STARTING => Phase::Starting,
        match_phase::SETUP => Phase::Setup { until: t(r.until) },
        match_phase::LIVE => Phase::Live,
        match_phase::ROUND_OVER => Phase::RoundOver { until: t(r.until) },
        match_phase::INTERMISSION => Phase::Intermission { until: t(r.until) },
        match_phase::MATCH_OVER => Phase::MatchOver,
        _ => Phase::Waiting,
    };
    let view = CaMatchView {
        mode: r.mode,
        goal: r.goal,
        phase,
        warmup: r.warmup,
        round: r.round,
        score: r.score,
        round_wins: r.round_wins,
        round_ends: r.round_ends.map(t),
        winner: r.winner,
        result: r.result,
        team_counts: r.team_counts,
    };
    if *world.resource::<CaMatchView>() != view {
        world.insert_resource(view);
    }
}

/// Our player, dead for more than the client's wait in a mode that
/// respawns, asks once per death (S5: a refused request isn't retried).
fn ask_to_respawn(
    me: Query<&Dead, With<LocalPlayer>>,
    view: Res<CaMatchView>,
    settings: Res<CaRuleSettings>,
    time: Res<Time<Fixed>>,
    mut asked: Local<Option<f64>>,
    mut out: MessageWriter<CaRespawnRequest>,
) {
    let Ok(dead) = me.single() else {
        *asked = None;
        return;
    };
    if !view.mode().is_some_and(|m| m.respawns) || *asked == Some(dead.since) {
        return;
    }
    if time.elapsed_secs_f64() - dead.since > settings.respawn_wait_client as f64 {
        *asked = Some(dead.since);
        out.write(CaRespawnRequest);
    }
}

/// Each player's pickup target as the weapon it is.
fn write_pickups(
    players: Query<(Entity, &CaSelection, Option<&CaPickup>)>,
    dropped: Query<&Dropped>,
    weapons: Query<&Weapon>,
    mut commands: Commands,
) {
    for (e, sel, have) in &players {
        let id = sel
            .pickup_target
            .and_then(|i| dropped.get(i).ok())
            .and_then(|d| weapons.get(d.weapon).ok())
            .map(|w| w.id.to_string());
        if have.is_none_or(|h| h.0 != id) {
            commands.entity(e).insert(CaPickup(id));
        }
    }
}

/// Serving stopped: the prompts and the match's entity go (armour and the
/// statistics stay: single player uses them).
fn stopped_serving(world: &mut World) {
    let matches: Vec<Entity> = world.query_filtered::<Entity, With<NetCaMatch>>().iter(world).collect();
    for e in matches {
        world.despawn(e);
    }
    let with: Vec<Entity> = world.query_filtered::<Entity, With<CaPickup>>().iter(world).collect();
    for e in with {
        world.entity_mut(e).remove::<CaPickup>();
    }
}
