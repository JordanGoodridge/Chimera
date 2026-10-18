//! The end of the game on a map (CS:S's `mp_timelimit`, `mp_maxrounds`,
//! `mp_winlimit`): once a limit is reached (with rounds, as the round that
//! reaches it ends: CS:S lets a round past the time limit play out, then
//! starts the intermission in the same second as the round's end, measured
//! 2026-10-14, parity-gaps.md C9; in deathmatch, at once) everyone is held
//! still for the intermission (`mp_chattime`, CS:S: 10 s, then the map
//! cycle's next map) with the scoreboard up, then the game goes on to the
//! next map of the map cycle (`MapCycle`; the same map again without one).
//! CS:S's Create Server sets a 20-minute time limit
//! (`cfg/settings_default.scr`); the console's default is 0 (none), as
//! CS:S's. What CS:S shows during it is unmeasured. A time limit's
//! intermission logs `World triggered "Intermission_Time_Limit"`.
//!
//! Known shortcuts (docs/tech-debt.md): End of a map's game (`rules::game_end`;
//! timing measured, the look not): with rounds the limits are checked as a
//! round ends, in deathmatch the time limit at once; the intermission holds everyone (the freeze) with the scoreboard up
//! for `mp_chattime`; the next map is the map cycle's (`cfg/mapcycle.txt`, else
//! `mapcycle_default.txt`, maps not installed left out), held in the
//! intermission until it loads; `mp_maxrounds` counts draws; no `nextlevel`
//! cvar. Why it's OK for now: Captures wanted (C9) in
//! docs/plans/active/parity-gaps.md.

use bevy::prelude::*;

use super::rounds::RoundState;
use crate::console::{Console, ConsoleAppExt, resource_cvar};

/// The game's limits on a map.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct GameLimits {
    /// Minutes on a map (`mp_timelimit`; 0: none).
    pub time_limit: f32,
    /// Rounds played (`mp_maxrounds`; 0: none).
    pub max_rounds: u32,
    /// Rounds one side wins (`mp_winlimit`; 0: none).
    pub win_limit: u32,
    /// Seconds of intermission before the next map (`mp_chattime`).
    pub chat_time: f32,
}

impl Default for GameLimits {
    fn default() -> Self {
        Self {
            time_limit: 0.0,
            max_rounds: 0,
            win_limit: 0,
            chat_time: 10.0,
        }
    }
}

/// The maps played in turn: the engine's (`core`, the client follows it),
/// at its old path.
pub use crate::core::MapCycle;

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<GameLimits>().init_resource::<MapCycle>();
    resource_cvar::<GameLimits, f32>(
        app,
        "mp_timelimit",
        "Minutes on a map before the game moves to the next one (0: no limit).",
        |l| &mut l.time_limit,
    );
    resource_cvar::<GameLimits, u32>(
        app,
        "mp_maxrounds",
        "Rounds played on a map before the game moves to the next one (0: no limit).",
        |l| &mut l.max_rounds,
    );
    resource_cvar::<GameLimits, u32>(
        app,
        "mp_winlimit",
        "Rounds one side wins on a map before the game moves to the next one (0: no limit).",
        |l| &mut l.win_limit,
    );
    resource_cvar::<GameLimits, f32>(
        app,
        "mp_chattime",
        "Seconds of intermission (the scoreboard) before the next map.",
        |l| &mut l.chat_time,
    );
    app.console_command("timeleft", "Time left on this map (mp_timelimit).", |w, _| {
        let limit = w.resource::<GameLimits>().time_limit;
        let now = w.resource::<Time<Fixed>>().elapsed_secs_f64();
        Ok(Some(match time_left(limit, w.resource::<RoundState>().map_started, now) {
            Some(left) => format!("Time remaining on map: {}", clock(left)),
            None => "No timelimit for map".into(),
        }))
    });
}

/// Seconds left on the map with a limit of `minutes` (None: no limit).
pub fn time_left(minutes: f32, started: Option<f64>, now: f64) -> Option<f64> {
    (minutes > 0.0).then(|| (minutes as f64 * 60.0 - (now - started.unwrap_or(now))).max(0.0))
}

/// `m:ss`.
pub fn clock(secs: f64) -> String {
    let s = secs.ceil() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Whether a limit is reached: the time (`rounds` or not), the rounds
/// played and a side's wins (with rounds).
pub fn reached(limits: &GameLimits, state: &RoundState, now: f64, rounds: bool) -> bool {
    let time = time_left(limits.time_limit, state.map_started, now).is_some_and(|l| l <= 0.0);
    let played = state.wins[0] + state.wins[1] + state.draws;
    let max_rounds = rounds && limits.max_rounds > 0 && played >= limits.max_rounds;
    let wins = rounds && limits.win_limit > 0 && state.wins.iter().any(|w| *w >= limits.win_limit);
    time || max_rounds || wins
}

/// Start the intermission now.
pub(super) fn begin(world: &mut World, now: f64) {
    let started = world.resource::<RoundState>().map_started;
    if time_left(world.resource::<GameLimits>().time_limit, started, now).is_some_and(|l| l <= 0.0) {
        world.write_message(crate::game_log::LogLine::world("Intermission_Time_Limit"));
    }
    let chat = world.resource::<GameLimits>().chat_time.max(0.0) as f64;
    world.resource_mut::<RoundState>().game_over = Some(now + chat);
}

/// The intermission's end: on to the next map (`changelevel` on a
/// network server, `map` otherwise), or the same game over again without
/// a cycle.
pub(super) fn next_map(world: &mut World) {
    let next = world.resource::<MapCycle>().next();
    match next {
        Some(map) => {
            let server = world.get_resource::<crate::core::NetRole>() == Some(&crate::core::NetRole::Server);
            let verb = if server { "changelevel" } else { "map" };
            if let Some(mut console) = world.get_resource_mut::<Console>() {
                console.submit(format!("{verb} {map}"));
            }
            // Held in the intermission until the map loads (`new_game`).
            world.resource_mut::<RoundState>().game_over = Some(f64::INFINITY);
        }
        None => super::new_game(world),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cycle_goes_round() {
        let mut c = MapCycle {
            maps: ["cs_italy", "de_dust", "de_aztec"].map(String::from).to_vec(),
            current: "de_dust".into(),
        };
        assert_eq!(c.next().as_deref(), Some("de_aztec"));
        c.current = "DE_AZTEC".into();
        assert_eq!(c.next().as_deref(), Some("cs_italy"), "wraps");
        c.current = "mg_jacks".into();
        assert_eq!(c.next().as_deref(), Some("cs_italy"), "not in it: the first");
        assert_eq!(MapCycle::default().next(), None);
    }

    #[test]
    fn limits_and_time_left() {
        let limits = GameLimits {
            time_limit: 20.0,
            max_rounds: 3,
            win_limit: 2,
            ..default()
        };
        let mut state = RoundState::default();
        state.map_started = Some(100.0);
        assert_eq!(time_left(20.0, Some(100.0), 160.0), Some(1140.0));
        assert_eq!(clock(1139.2), "19:00");
        assert_eq!(time_left(0.0, Some(100.0), 160.0), None);
        assert!(!reached(&limits, &state, 160.0, true));
        assert!(reached(&limits, &state, 1300.0, false), "time, in deathmatch too");
        state.wins = [1, 1];
        assert!(!reached(&limits, &state, 160.0, true));
        state.draws = 1;
        assert!(reached(&limits, &state, 160.0, true), "three rounds played");
        assert!(!reached(&limits, &state, 160.0, false), "rounds count only with rounds");
        state.draws = 0;
        state.wins = [2, 0];
        assert!(reached(&limits, &state, 160.0, true), "a side's wins");
    }
}
