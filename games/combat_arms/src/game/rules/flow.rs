//! The match's phases (specs/combat_arms/game_modes.md S3, S9, S10, S11):
//! the mission update the server runs once per server frame
//! (`grenades::SERVER_FRAME`, 20 Hz: the first tick at or after each
//! frame's time), joining, leaving, and a fresh match.
//!
//! - Waiting → Starting (the first player in a team) → mission start the
//!   next frame (the counters reset), which sets the round up at once:
//!   everyone on Alpha and Bravo respawned (S5, the living too), held
//!   players released, the living frozen (`core::FreezeTime`: movement
//!   keys held, `frozen_intent`) until `object_start` s (the mode's forced
//!   length: TDM 0, one frame).
//! - Live at the next frame after the setup's end: unfrozen, the winner
//!   cleared, the round timer started (the warm-up's `training_time_limit`,
//!   else the mode's `time_limit`; none: no timer).
//! - Round over at the first frame that sees a requested end or the timer
//!   at 0: the mode's time-out rule picks a winner if none was set, the
//!   winner's round wins +1 (not in the warm-up), the result recorded
//!   (`CaMatch::results`); `object_end` s on.
//! - Round end at the next frame after that: every body and every weapon
//!   item on the ground removed (S11; grenades stay), the round's counters
//!   cleared, the round counted (not the warm-up); the mode decides
//!   whether the match is over (`CaMode::match_end`, never after the
//!   warm-up); then the intermission (`mission_end` s), then the next
//!   mission start or the match's result.
//! - The warm-up (S9, `CaMode::warmup`): the match's first round, nothing
//!   counts, no winner; the dead revived every `warmup_revive` s in the
//!   modes that do (`CaMode::warmup_revives`).
//! - Joining (S1, `enter_game`, the network's `JoinRules::enter`): in a
//!   mode that respawns, at once (S4's points, protected); in a round mode,
//!   held until the next setup when the round is live and at least
//!   `late_join_wait` players are in, else at once; with join in progress
//!   off, held until the next setup.
//! - Leaving (S10): the mode's rule (`CaMode::leaving`) runs while the
//!   leaver still counts (`rules`' observer on the player's removal).
//! - A fresh match (`new_game`: a map loaded, `ca_restart`): back to
//!   waiting, the cursors at 0, everyone's statistics cleared.
//!
//! Known shortcuts: a round's end and the match's result are kept as data
//! (`CaMatch::results`, `result`) and replicated (`game::net`); the reward
//! roulette, the report to the lobby and the late-join closing (S1) aren't
//! there; the match clock (`MissionTimeLimit`) isn't kept (it ends
//! nothing, Quirk 11); between rounds the intermission (`MissionEnd`)
//! comes after every round's end (S3's 3 + 3 + 0 s between rounds; its
//! step 6 names it only before the match's result); map triggers and
//! objective weapons at the setup don't exist yet; a match over stays over
//! until `ca_restart` or a map load.

use bevy::prelude::*;

use super::{
    super::{grenades::SERVER_FRAME, inventory::Dropped},
    CaRuleSettings, RoundTime, Teams, current_mode, objectives,
    scoring::{CaDamageLog, sync_score},
    spawns::{self, CaHeld, CaSpawnCursors},
    state::{ALPHA, BRAVO, CaMatch, CaStats, FFA_FIRST, FFA_LAST, Phase, RoundResult, plays, side},
};
use crate::{
    core::{Connecting, Dead, FreezeTime, Health, Intent, MapLoads, Npc, SimClock, Spectating, Team},
    map::ragdoll::{Ragdoll, Ragdolled},
};

/// The mission update's frame counter (one update per server frame).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CaMissionFrames(pub u64);

/// The players in the match: on Alpha or Bravo, in the game (not
/// connecting, spectating or an NPC), in the order they came in
/// (`CaJoinOrder`).
pub fn players(world: &mut World) -> Vec<(Entity, u8)> {
    let mut all: Vec<(u64, Entity, u8)> = world
        .query_filtered::<(Entity, &Team, Option<&CaJoinOrder>), (
            With<Intent>,
            With<Health>,
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

/// The order a character came into the game (the server's player list's
/// order): given when it first gets a team.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CaJoinOrder(pub u64);

/// Characters with a team numbered as they come (`CaJoinOrder`).
pub(super) fn number_players(add: On<Add, Team>, mut next: Local<u64>, mut commands: Commands) {
    *next += 1;
    if let Ok(mut e) = commands.get_entity(add.entity) {
        e.try_insert(CaJoinOrder(*next));
    }
}

/// The living players of `team` in the match.
pub fn alive(world: &mut World, team: u8) -> usize {
    let living: Vec<Entity> = world
        .query_filtered::<(Entity, &Health), (Without<Dead>, Without<CaHeld>)>()
        .iter(world)
        .filter(|(_, h)| h.current > 0.0)
        .map(|(e, _)| e)
        .collect();
    players(world)
        .into_iter()
        .filter(|(e, t)| *t == team && living.contains(e))
        .count()
}

/// Every living player in the match (any team).
pub fn alive_players(world: &mut World) -> Vec<(Entity, u8)> {
    let living: Vec<Entity> = world
        .query_filtered::<(Entity, &Health), (Without<Dead>, Without<CaHeld>)>()
        .iter(world)
        .filter(|(_, h)| h.current > 0.0)
        .map(|(e, _)| e)
        .collect();
    players(world).into_iter().filter(|(e, _)| living.contains(e)).collect()
}

/// S1: `e`'s team as the mode makes teams: Alpha or Bravo (a player
/// without one goes on the smaller team, Alpha on a tie); every player a
/// team of their own in a free-for-all mode (the lowest free id from
/// `FFA_FIRST`); Alpha for everyone at a round's setup in Spy Hunt and
/// Quarantine (`reset`). Observers and players without a team are left
/// alone.
pub fn assign_team(world: &mut World, e: Entity, reset: bool) {
    let Some(mode) = current_mode(world) else { return };
    let Some(team) = world.get::<crate::core::Team>(e).map(|t| t.0) else { return };
    if !plays(team) {
        return;
    }
    let others: Vec<u8> = players(world).into_iter().filter(|(p, _)| *p != e).map(|(_, t)| t).collect();
    let want = match mode.teams {
        Teams::Two => {
            if side(team).is_some() {
                return;
            }
            let count = |t: u8| others.iter().filter(|o| **o == t).count();
            if count(BRAVO) < count(ALPHA) { BRAVO } else { ALPHA }
        }
        Teams::AllAlpha => {
            if !reset && side(team).is_some() {
                return;
            }
            ALPHA
        }
        Teams::FreeForAll => {
            if team >= FFA_FIRST && !others.contains(&team) {
                return;
            }
            let Some(free) = (FFA_FIRST..=FFA_LAST).find(|t| !others.contains(t)) else { return };
            free
        }
    };
    if want != team {
        world.entity_mut(e).insert(crate::core::Team(want));
    }
}

/// The mission update, once per server frame (the server's, with a mode).
pub(super) fn mission_update(world: &mut World) {
    if current_mode(world).is_none() {
        return;
    }
    let now = world.resource::<SimClock>().now;
    let due = ((now + 1e-9) / SERVER_FRAME).floor() as u64;
    {
        let mut frames = world.resource_mut::<CaMissionFrames>();
        if frames.0 >= due {
            return;
        }
        frames.0 = due;
    }
    step(world, now);
}

/// One mission update: at most one phase on, except where a phase starts
/// the next directly.
fn step(world: &mut World, now: f64) {
    let Some(mode) = current_mode(world) else { return };
    let phase = world.resource::<CaMatch>().phase;
    match phase {
        Phase::Waiting => {
            if !players(world).is_empty() {
                world.resource_mut::<CaMatch>().phase = Phase::Starting;
            }
        }
        Phase::Starting => mission_start(world, now, true),
        Phase::Setup { until } => {
            if now + 1e-9 >= until {
                go_live(world, now);
            }
        }
        Phase::Live => {
            let (requested, ends, warmup) = {
                let m = world.resource::<CaMatch>();
                (m.end_requested, m.round_ends, m.warmup)
            };
            let hold = world.resource::<CaMatch>().hold_until;
            if let Some(h) = hold {
                if now + 1e-9 >= h {
                    round_over(world, now);
                }
            } else if requested || ends.is_some_and(|e| now + 1e-9 >= e) {
                round_over(world, now);
            } else if warmup && mode.warmup_revives && now + 1e-9 >= world.resource::<CaMatch>().next_revive {
                let period = world.resource::<CaRuleSettings>().warmup_revive as f64;
                world.resource_mut::<CaMatch>().next_revive = now + period;
                revive_the_dead(world);
            }
        }
        Phase::RoundOver { until } => {
            if now + 1e-9 >= until {
                round_end(world, now);
            }
        }
        Phase::Intermission { until } => {
            if now + 1e-9 >= until {
                if world.resource::<CaMatch>().result.is_some() {
                    world.resource_mut::<CaMatch>().phase = Phase::MatchOver;
                } else {
                    mission_start(world, now, false);
                }
            }
        }
        Phase::MatchOver => {}
    }
}

/// Mission start (S3 steps 2 and 3): `first`, the match's counters reset;
/// then the round's setup.
fn mission_start(world: &mut World, now: f64, first: bool) {
    let Some(mode) = current_mode(world) else { return };
    let settings = world.resource::<CaRuleSettings>().clone();
    let training = settings.mode_data().training_time_limit.unwrap_or(0.0);
    {
        let mut m = world.resource_mut::<CaMatch>();
        if first {
            let warmup = mode.warmup && training > 0.0;
            *m = CaMatch {
                warmup,
                ..default()
            };
        } else if std::mem::take(&mut m.warmup_next) {
            m.warmup = training > 0.0;
        }
        m.hold_until = None;
        m.immune = false;
        m.round_kills = [0; 2];
        m.round_deaths = [0; 2];
        m.winner = None;
        m.end_requested = false;
        m.void = false;
        m.round_ends = None;
    }
    if first {
        reset_stats(world);
    }
    world.resource_mut::<super::super::inventory::CaDeathDrops>().0 = mode.death_drops;
    objectives::clear_players(world);
    // Everyone on a team respawned, held players with them, on the team
    // the mode gives.
    for (e, _) in players(world) {
        assign_team(world, e, true);
    }
    (mode.hooks.before_spawns)(world);
    for (e, _) in players(world) {
        spawns::respawn(world, e);
    }
    (mode.hooks.setup)(world);
    let setup = mode.forced_setup.unwrap_or(settings.object_start) as f64;
    world.resource_mut::<CaMatch>().phase = Phase::Setup { until: now + setup };
    world.resource_mut::<FreezeTime>().0 = true;
}

/// S3 step 4: live.
fn go_live(world: &mut World, now: f64) {
    let Some(mode) = current_mode(world) else { return };
    let settings = world.resource::<CaRuleSettings>().clone();
    let data = settings.mode_data();
    let goal = settings.goal() as f32;
    world.resource_mut::<FreezeTime>().0 = false;
    let revive = settings.warmup_revive as f64;
    let mut m = world.resource_mut::<CaMatch>();
    m.phase = Phase::Live;
    m.winner = None;
    m.end_requested = false;
    let length = if m.warmup {
        data.training_time_limit
    } else {
        match mode.round_time {
            RoundTime::Mission => data.time_limit,
            RoundTime::MissionTimesGoal => data.time_limit.map(|t| t * goal),
            RoundTime::GoalMinutes => Some(goal * 60.0),
        }
    };
    m.round_ends = length.map(|l| now + l as f64);
    m.next_revive = now + revive;
}

/// S3 step 5: round over.
fn round_over(world: &mut World, now: f64) {
    let Some(mode) = current_mode(world) else { return };
    let (warmup, void) = {
        let m = world.resource::<CaMatch>();
        (m.warmup, m.void)
    };
    if warmup || void {
        world.resource_mut::<CaMatch>().winner = None;
    } else if world.resource::<CaMatch>().winner.is_none() {
        (mode.time_out)(world);
    }
    // A free-for-all round's winner is a player (its own team id).
    if !warmup
        && let Some(w) = world.resource::<CaMatch>().winner.filter(|w| side(*w).is_none())
        && let Some((p, _)) = players(world).into_iter().find(|(_, t)| *t == w)
    {
        super::scoring::stats(world, p).round_wins += 1;
    }
    let object_end = world.resource::<CaRuleSettings>().object_end as f64;
    let mut m = world.resource_mut::<CaMatch>();
    if !warmup && let Some(i) = m.winner.and_then(side) {
        m.round_wins[i] += 1;
    }
    let result = RoundResult {
        round: m.round,
        warmup,
        winner: m.winner,
        score: m.score,
    };
    m.results.push(result);
    m.phase = Phase::RoundOver { until: now + object_end };
    m.round_ends = None;
    m.hold_until = None;
    (mode.hooks.round_over)(world);
}

/// S3 step 6: round end, then the intermission.
fn round_end(world: &mut World, now: f64) {
    let Some(mode) = current_mode(world) else { return };
    (mode.hooks.round_end)(world);
    clean_up(world);
    world.resource_mut::<FreezeTime>().0 = false;
    world.resource_mut::<CaMatch>().immune = false;
    let warmup = {
        let mut m = world.resource_mut::<CaMatch>();
        m.round_kills = [0; 2];
        m.round_deaths = [0; 2];
        let warmup = m.warmup;
        if !warmup {
            m.round += 1;
        }
        m.warmup = false;
        warmup
    };
    super::update_team_counts(world);
    if !warmup && let Some(result) = (mode.match_end)(world) {
        world.resource_mut::<CaMatch>().result = Some(result);
    }
    let mission_end = world.resource::<CaRuleSettings>().mission_end as f64;
    world.resource_mut::<CaMatch>().phase = Phase::Intermission { until: now + mission_end };
    (mode.hooks.after_round)(world);
}

/// S11: every body and every weapon item on the ground goes (live
/// grenades stay).
pub fn clean_up(world: &mut World) {
    let items: Vec<(Entity, Entity)> = world
        .query::<(Entity, &Dropped)>()
        .iter(world)
        .map(|(e, d)| (e, d.weapon))
        .collect();
    for (item, weapon) in items {
        if world
            .get::<crate::weapon::Weapon>(weapon)
            .is_some_and(|w| w.owner.is_none())
        {
            world.despawn(weapon);
        }
        if world.get_entity(item).is_ok() {
            world.despawn(item);
        }
    }
    let ragdolls: Vec<(Entity, Ragdoll)> = world
        .query::<(Entity, &Ragdoll)>()
        .iter(world)
        .filter(|(_, r)| !r.placed)
        .map(|(e, r)| (e, r.clone()))
        .collect();
    for (e, r) in ragdolls {
        for x in r.joints.iter().chain(&r.bodies).chain(&r.visual) {
            if world.get_entity(*x).is_ok() {
                world.despawn(*x);
            }
        }
        world.despawn(e);
        if let Ok(mut owner) = world.get_entity_mut(r.owner) {
            owner.remove::<Ragdolled>();
        }
    }
}

/// S9: every dead player in the match revived.
fn revive_the_dead(world: &mut World) {
    let dead: Vec<Entity> = world.query_filtered::<Entity, With<Dead>>().iter(world).collect();
    for (e, _) in players(world) {
        if dead.contains(&e) {
            spawns::respawn(world, e);
        }
    }
}

/// Everyone's statistics back to nothing.
fn reset_stats(world: &mut World) {
    let all: Vec<Entity> = world.query_filtered::<Entity, With<CaStats>>().iter(world).collect();
    for e in all {
        world.entity_mut(e).insert(CaStats::default());
        sync_score(world, e);
    }
    world.resource_mut::<CaDamageLog>().0.clear();
}

/// What a frozen player's intent keeps in the setup: everything but the
/// movement keys.
pub fn frozen_intent(i: &Intent) -> Intent {
    Intent {
        move_axis: Vec2::ZERO,
        jump: false,
        ..i.clone()
    }
}

/// The setup's freeze on every intent (the server's and, through
/// `net::Holds`, a client's prediction).
pub(super) fn hold_frozen(frozen: Res<FreezeTime>, m: Option<Res<CaMatch>>, mut intents: Query<&mut Intent>) {
    if !frozen.0 && !m.is_some_and(|m| m.immune) {
        return;
    }
    for mut i in &mut intents {
        let held = frozen_intent(&i);
        *i = held;
    }
}

/// When the setup's freeze ends (`net::Holds::freeze_ends`).
pub fn freeze_ends(world: &World) -> Option<f64> {
    match world.get_resource::<CaMatch>()?.phase {
        Phase::Setup { until } => Some(until),
        _ => None,
    }
}

/// A player has come into the game (S1; the network's
/// `JoinRules::enter`): spawned or held as the mode says.
pub fn enter_game(world: &mut World, e: Entity) {
    if world.get_entity(e).is_err() {
        return;
    }
    world.entity_mut(e).remove::<Connecting>();
    let Some(mode) = current_mode(world) else {
        crate::net::parts::enter_at_spawn(world, e);
        return;
    };
    let settings = world.resource::<CaRuleSettings>().clone();
    let phase = world.resource::<CaMatch>().phase;
    // A warm-up takes joiners at once (game_modes_2.md test case 34).
    let live = phase == Phase::Live && !world.resource::<CaMatch>().warmup;
    let started = !matches!(phase, Phase::Waiting | Phase::Starting);
    assign_team(world, e, false);
    let in_game = players(world).iter().filter(|(p, _)| *p != e).count() as u32;
    let no_joining = settings.join_in_progress == 0 || super::no_join_in_progress(mode.key);
    let hold = (started && no_joining)
        || (!mode.respawns && live && in_game >= settings.late_join_wait)
        || (!mode.respawns && live && (mode.hooks.holds_joiner)(world));
    if hold {
        spawns::hold(world, e);
    } else {
        spawns::respawn(world, e);
    }
}

/// A fresh match on the map just loaded (`core::NewGame`), or by
/// `ca_restart`: waiting for players, the cursors at 0, the statistics
/// cleared; the map loads counted.
pub fn new_game(world: &mut World) {
    restart(world);
    world.get_resource_or_init::<MapLoads>().0 += 1;
}

/// The match from the start (`ca_restart`): waiting for players (the
/// next frame starts it when there are some).
pub fn restart(world: &mut World) {
    *world.resource_mut::<CaMatch>() = CaMatch::default();
    *world.resource_mut::<CaSpawnCursors>() = CaSpawnCursors::default();
    world.resource_mut::<FreezeTime>().0 = false;
    objectives::reset(world);
    reset_stats(world);
    // The modes' own: Hired Guns' banks and mercenaries, VIP Escort's
    // roles and VIP, the clan modes' half.
    super::mercs::remove_all(world);
    world.insert_resource(super::hired_guns::CaHiredGuns::default());
    let banked: Vec<Entity> = world
        .query_filtered::<Entity, Or<(With<super::hired_guns::CaBank>, With<super::hired_guns::CaPaid>)>>()
        .iter(world)
        .collect();
    for e in banked {
        world
            .entity_mut(e)
            .remove::<(super::hired_guns::CaBank, super::hired_guns::CaPaid, super::super::movement::CustomMovePenalty)>();
    }
    super::escort::reset(world);
    super::clan::reset(world);
}
