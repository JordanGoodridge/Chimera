//! The match's phases as the runtime steps them (lifted from Combat Arms'
//! mission update, specs/combat_arms/game_modes.md S3, S9): once per the
//! game's frame (`Flow::frame`: the first tick at or after each frame's
//! time), at most one phase on, except where a phase starts the next
//! directly. The game's work at each step is its `Flow::event`
//! (`FlowEvent`), the mode's its `Hooks`.
//!
//! - Waiting → Starting (the first player in) → mission start the next
//!   frame (`Prepare`; the match's counters reset on the first,
//!   `ResetStats`), which sets the round up at once (`MissionStart`):
//!   everyone's team made (`assign_team`), the mode's `before_spawns`,
//!   everyone respawned (`Flow::respawn`, the living too), the mode's
//!   `setup`, everyone frozen (`core::FreezeTime`) until the setup's end
//!   (`Mode::forced_setup`, else `Span::Setup`).
//! - Live at the next frame after the setup's end: unfrozen, the winner
//!   cleared, the round timer started (`Span::Round`, the warm-up's
//!   `Span::Warmup`; none: no timer).
//! - Round over at the first frame that sees a requested end, the timer
//!   at 0 or a hold's end: the mode's time-out rule picks a winner if none
//!   was set (not in a warm-up or a void round), a free-for-all winner's
//!   round counted (`RoundWonBy`), the side's round wins +1 (not in the
//!   warm-up), the result recorded, the mode's `round_over`, `RoundOver`;
//!   `Span::RoundOver` on.
//! - Round end at the next frame after that: the mode's `round_end`,
//!   `RoundEnd`, unfrozen, the round's counters cleared, the round counted
//!   (not the warm-up), `RoundEnded`, the mode's `match_end` (never after
//!   the warm-up), the intermission (`Span::Intermission`), the mode's
//!   `after_round`; then the next mission start or the match over.
//! - The warm-up (`Mode::warmup`, when the game has a `Span::Warmup`):
//!   the match's first round, nothing counts, no winner; the dead revived
//!   every `Span::Revive` in the modes that do (`Mode::warmup_revives`).
//!
//! The systems a game chains into its rules: `mission_update`, `update`
//! (the mode's objects, before it) and `touches` (after the tick's
//! deaths).

use bevy::prelude::*;

use super::{Flow, FlowEvent, Mode, Span, assign_team, current, current_mode, state::*};
use crate::core::{Dead, FreezeTime, SimClock};

/// The phases' frame counter (one step per `Flow::frame`).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MissionFrames(pub u64);

/// The phases' step, once per the game's frame (the server's, with a
/// mode).
pub fn mission_update(world: &mut World) {
    let Some((mode, flow)) = current(world) else { return };
    let now = world.resource::<SimClock>().now;
    if flow.frame > 0.0 {
        let due = ((now + 1e-9) / flow.frame).floor() as u64;
        let mut frames = world.get_resource_or_init::<MissionFrames>();
        if frames.0 >= due {
            return;
        }
        frames.0 = due;
    }
    step(world, now, mode, flow);
}

/// The mode's own update (its objects), every server tick before the
/// phases step.
pub fn update(world: &mut World) {
    if let Some(mode) = current_mode(world) {
        (mode.hooks.update)(world);
    }
}

/// The mode's touches, after the tick's deaths.
pub fn touches(world: &mut World) {
    if let Some(mode) = current_mode(world) {
        (mode.hooks.touches)(world);
    }
}

/// One step: at most one phase on, except where a phase starts the next
/// directly.
fn step(world: &mut World, now: f64, mode: &'static Mode, flow: Flow) {
    let phase = world.resource::<Match>().phase;
    match phase {
        Phase::Waiting => {
            if !(flow.players)(world).is_empty() {
                world.resource_mut::<Match>().phase = Phase::Starting;
            }
        }
        Phase::Starting => mission_start(world, now, true, mode, flow),
        Phase::Setup { until } => {
            if now + 1e-9 >= until {
                go_live(world, now, flow);
            }
        }
        Phase::Live => {
            let (requested, ends, warmup, hold, next_revive) = {
                let m = world.resource::<Match>();
                (m.end_requested, m.round_ends, m.warmup, m.hold_until, m.next_revive)
            };
            if let Some(h) = hold {
                if now + 1e-9 >= h {
                    round_over(world, now, mode, flow);
                }
            } else if requested || ends.is_some_and(|e| now + 1e-9 >= e) {
                round_over(world, now, mode, flow);
            } else if warmup && mode.warmup_revives && now + 1e-9 >= next_revive {
                let period = (flow.length)(world, Span::Revive).unwrap_or(0.0) as f64;
                world.resource_mut::<Match>().next_revive = now + period;
                revive_the_dead(world, flow);
            }
        }
        Phase::RoundOver { until } => {
            if now + 1e-9 >= until {
                round_end(world, now, mode, flow);
            }
        }
        Phase::Intermission { until } => {
            if now + 1e-9 >= until {
                if world.resource::<Match>().result.is_some() {
                    world.resource_mut::<Match>().phase = Phase::MatchOver;
                } else {
                    mission_start(world, now, false, mode, flow);
                }
            }
        }
        Phase::MatchOver => {}
    }
}

/// A mission start: `first`, the match's counters reset; then the round's
/// setup.
fn mission_start(world: &mut World, now: f64, first: bool, mode: &'static Mode, flow: Flow) {
    (flow.event)(world, FlowEvent::Prepare { first });
    let has_warmup = (flow.length)(world, Span::Warmup).is_some();
    {
        let mut m = world.resource_mut::<Match>();
        if first {
            let warmup = mode.warmup && has_warmup;
            *m = Match {
                warmup,
                ..default()
            };
        } else if std::mem::take(&mut m.warmup_next) {
            m.warmup = has_warmup;
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
        (flow.event)(world, FlowEvent::ResetStats);
    }
    (flow.event)(world, FlowEvent::MissionStart);
    // Everyone in the match respawned, on the team the mode gives.
    for (e, _) in (flow.players)(world) {
        assign_team(world, e, true);
    }
    (mode.hooks.before_spawns)(world);
    for (e, _) in (flow.players)(world) {
        (flow.respawn)(world, e);
    }
    (mode.hooks.setup)(world);
    let setup = mode
        .forced_setup
        .unwrap_or_else(|| (flow.length)(world, Span::Setup).unwrap_or(0.0)) as f64;
    world.resource_mut::<Match>().phase = Phase::Setup { until: now + setup };
    world.resource_mut::<FreezeTime>().0 = true;
}

/// Live.
fn go_live(world: &mut World, now: f64, flow: Flow) {
    let warmup = world.resource::<Match>().warmup;
    let length = (flow.length)(world, if warmup { Span::Warmup } else { Span::Round });
    let revive = (flow.length)(world, Span::Revive).unwrap_or(0.0) as f64;
    world.resource_mut::<FreezeTime>().0 = false;
    let mut m = world.resource_mut::<Match>();
    m.phase = Phase::Live;
    m.winner = None;
    m.end_requested = false;
    m.round_ends = length.map(|l| now + l as f64);
    m.next_revive = now + revive;
}

/// Round over.
fn round_over(world: &mut World, now: f64, mode: &'static Mode, flow: Flow) {
    let (warmup, void) = {
        let m = world.resource::<Match>();
        (m.warmup, m.void)
    };
    if warmup || void {
        world.resource_mut::<Match>().winner = None;
    } else if world.resource::<Match>().winner.is_none() {
        (mode.time_out)(world);
    }
    // A free-for-all round's winner is a player (its own team id).
    if !warmup
        && let Some(w) = world.resource::<Match>().winner.filter(|w| side(*w).is_none())
        && let Some((p, _)) = (flow.players)(world).into_iter().find(|(_, t)| *t == w)
    {
        (flow.event)(world, FlowEvent::RoundWonBy(p));
    }
    let over = (flow.length)(world, Span::RoundOver).unwrap_or(0.0) as f64;
    let mut m = world.resource_mut::<Match>();
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
    m.phase = Phase::RoundOver { until: now + over };
    m.round_ends = None;
    m.hold_until = None;
    (mode.hooks.round_over)(world);
    (flow.event)(world, FlowEvent::RoundOver);
}

/// Round end, then the intermission.
fn round_end(world: &mut World, now: f64, mode: &'static Mode, flow: Flow) {
    (mode.hooks.round_end)(world);
    (flow.event)(world, FlowEvent::RoundEnd);
    world.resource_mut::<FreezeTime>().0 = false;
    world.resource_mut::<Match>().immune = false;
    let warmup = {
        let mut m = world.resource_mut::<Match>();
        m.round_kills = [0; 2];
        m.round_deaths = [0; 2];
        let warmup = m.warmup;
        if !warmup {
            m.round += 1;
        }
        m.warmup = false;
        warmup
    };
    (flow.event)(world, FlowEvent::RoundEnded);
    if !warmup && let Some(result) = (mode.match_end)(world) {
        world.resource_mut::<Match>().result = Some(result);
    }
    let intermission = (flow.length)(world, Span::Intermission).unwrap_or(0.0) as f64;
    world.resource_mut::<Match>().phase = Phase::Intermission { until: now + intermission };
    (mode.hooks.after_round)(world);
}

/// The warm-up's revive: every dead player in the match respawned.
fn revive_the_dead(world: &mut World, flow: Flow) {
    let dead: Vec<Entity> = world.query_filtered::<Entity, With<Dead>>().iter(world).collect();
    for (e, _) in (flow.players)(world) {
        if dead.contains(&e) {
            (flow.respawn)(world, e);
        }
    }
}
