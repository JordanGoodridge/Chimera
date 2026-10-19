//! Combat Arms' game modes (specs/combat_arms/game_modes.md) as rule
//! modules (`rule_module`: registered by name, the one playing picked by
//! the `ca_mode` cvar): `combat_arms:tdm` (Team Death Match, shown as
//! Elimination, M1), `combat_arms:elimination_pro` (Elimination Pro, M2,
//! the brief's "Elimination") and `combat_arms:none` (no mode: the rules
//! leave players alone, as the skeleton had it).
//!
//! A mode is data and hooks (`CaMode`: its room key, whether it respawns,
//! has a warm-up, scales friendly fire, gives assists, shows a timer; what
//! a death adds, its time-out rule, when the match ends, what a leaver
//! does) on the shared rules, one submodule each:
//!
//! - `settings`: the room's and the server file's settings and the shared
//!   constants (S2), data/modes.toml under the `ca_*` cvars;
//! - `state`: the match (`CaMatch`: phase, clocks, team counters, round
//!   wins and results) and each player's statistics (`CaStats`, S7);
//! - `flow`: the phases the server's mission update runs once per server
//!   frame (S3), the warm-up (S9), round ends and their cleanup (S10,
//!   S11), joining and leaving (S1, S10), a fresh match (`ca_restart`, a
//!   map load: `core::NewGame`);
//! - `spawns`: start points and their cursors (S4), the respawn and its
//!   request (S5), telefrags, spawn protection (S6);
//! - `scoring`: deaths, kills and assists (S7), friendly fire (S8);
//! - `tdm`, `elimination`: the modes;
//! - `scoreboard`: the scoreboard's rows and order (S13).
//!
//! What the HUD and a network client read is `CaMatchView` (the server
//! writes it from `CaMatch` each tick; a client from the network,
//! `game::net`), with the team counts (`CaTeamCounts`).
//!
//! Teams are the lobby's (S1): a player keeps the team it came in with
//! (the network's join puts it on the smaller team; single player's local
//! player is Bravo, the shell's pick); no auto-balance or switching
//! (Quirk 10). Players on neither team (0, observers) are left alone.
//!
//! Lifted into Chimera for this (L15, docs/plans/active/combat-arms.md):
//! the dead held still and a character revived at a place
//! (`chimera_sim::spawning`), which CS:S's deathmatch uses too. CS:S's
//! round loop, team joining and game end stay its own: their phases,
//! money, objectives, `mp_*` settings and win table share nothing with
//! Combat Arms' beyond those.
//!
//! Known shortcuts (docs/tech-debt.md; the most literal reading where the
//! spec is unclear, each module's own list): see `flow`, `spawns`,
//! `scoring`, `settings`, `tdm`, `elimination`. Only the two team modes
//! are there; the server runs the mission update on its 20 Hz frames
//! (`grenades::SERVER_FRAME`) over the game's 15 ms tick.

pub mod elimination;
pub mod flow;
pub mod scoreboard;
pub mod scoring;
pub mod settings;
pub mod spawns;
pub mod state;
pub mod tdm;

use bevy::prelude::*;

pub use settings::CaRuleSettings;
pub use state::{CaMatch, CaStats, Phase};

use crate::{
    console::ConsoleAppExt,
    core::{Connecting, Dead, Health, Intent, Npc, SimClock, SimSet, Spectating, Team},
    rule_module::{self, RuleModule, RuleModules},
};

/// No mode: the rules leave players alone.
pub const NO_MODE: &str = "combat_arms:none";
/// Team Death Match (M1).
pub const TDM: &str = "combat_arms:tdm";
/// Elimination Pro (M2).
pub const ELIMINATION_PRO: &str = "combat_arms:elimination_pro";

/// A leaving player, as the leaving rule sees it (S10): its team, its
/// team's players and living players (the leaver counted), whether it was
/// alive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Leaver {
    pub team: u8,
    pub team_players: usize,
    pub team_alive: usize,
    pub alive: bool,
}

/// A game mode: its data and hooks.
#[derive(Clone, Copy)]
pub struct CaMode {
    /// The rule module's name.
    pub module: &'static str,
    /// The room key (`ca_mode`).
    pub key: &'static str,
    /// The name players see.
    pub shown_as: &'static str,
    pub description: &'static str,
    /// The dead respawn on request (S5).
    pub respawns: bool,
    /// The match starts with a warm-up round (S9) ...
    pub warmup: bool,
    /// ... that revives the dead every `warmup_revive` s.
    pub warmup_revives: bool,
    /// Friendly fire on scales a teammate's direct damage (S8).
    pub ff_scaled: bool,
    /// Enemy kills give assists (S7).
    pub assists: bool,
    /// The HUD shows the round timer (hud_crosshair.md D7).
    pub timer_shown: bool,
    /// The round setup's length, when the mode forces one (TDM: 0).
    pub forced_setup: Option<f32>,
    /// What a counted death adds (S7) and the win test.
    pub on_death: fn(&mut World, &state::Death),
    /// The winner of a round that ran out of time without one (may stay
    /// None: a draw).
    pub time_out: fn(&mut World),
    /// After a real round's end: the match's result if it is over
    /// (Some(None): a draw).
    pub match_end: fn(&World) -> Option<Option<u8>>,
    /// A player leaves (S10).
    pub leaving: fn(&mut CaMatch, &Leaver),
}

/// Every mode, by room key.
pub const MODES: [CaMode; 2] = [tdm::MODE, elimination::MODE];

/// The mode playing (`ca_mode`), if any.
pub fn current_mode(world: &World) -> Option<&'static CaMode> {
    let key = &world.get_resource::<CaRuleSettings>()?.mode;
    MODES.iter().find(|m| m.key == key)
}

fn mode_on(settings: Res<CaRuleSettings>) -> bool {
    MODES.iter().any(|m| m.key == settings.mode)
}

/// The cvars that set the modes up.
const SETTINGS: &[&str] = &[
    "ca_mode",
    "ca_goal",
    "ca_friendlyfire",
    "ca_join_in_progress",
    "ca_object_start",
    "ca_object_end",
    "ca_mission_end",
    "ca_respawn_wait",
    "ca_respawn_wait_client",
    "ca_respawn_invulnerability",
    "ca_respawn_bullet_protection",
];

/// The rule modules (`chimera_rules`): each mode's, picked by `ca_mode`,
/// then no mode.
pub fn modules() -> [RuleModule; 3] {
    [
        RuleModule {
            name: TDM,
            description: tdm::MODE.description,
            settings: SETTINGS,
            applies: |w| current_mode(w).is_some_and(|m| m.module == TDM),
        },
        RuleModule {
            name: ELIMINATION_PRO,
            description: elimination::MODE.description,
            settings: SETTINGS,
            applies: |w| current_mode(w).is_some_and(|m| m.module == ELIMINATION_PRO),
        },
        RuleModule {
            name: NO_MODE,
            description: "no game mode (ca_mode none): no teams' rules, rounds, score or respawning; the dead stay dead",
            settings: &["ca_mode"],
            applies: |w| current_mode(w).is_none(),
        },
    ]
}

/// The players on each team (Alpha, Bravo) in the match, kept each tick.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CaTeamCounts(pub [u32; 2]);

/// What the HUD and the scoreboard show of the match: the server's
/// `CaMatch` (or, on a network client, the network's copy), its times on
/// this machine's clock.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct CaMatchView {
    /// The room key; empty: no mode.
    pub mode: String,
    pub goal: u32,
    pub phase: Phase,
    pub warmup: bool,
    pub round: u32,
    pub score: [i32; 2],
    pub round_wins: [u32; 2],
    /// Live: when the round timer reaches 0 (`SimClock` seconds).
    pub round_ends: Option<f64>,
    pub winner: Option<u8>,
    pub result: Option<Option<u8>>,
    pub team_counts: [u32; 2],
}

impl CaMatchView {
    /// The mode it shows.
    pub fn mode(&self) -> Option<&'static CaMode> {
        MODES.iter().find(|m| m.key == self.mode)
    }
}

/// The server's match into `CaMatchView`.
fn publish(world: &mut World) {
    let settings = world.resource::<CaRuleSettings>();
    let mode = current_mode(world).map(|m| m.key.to_string()).unwrap_or_default();
    let goal = settings.goal();
    let m = world.resource::<CaMatch>();
    let view = CaMatchView {
        mode,
        goal,
        phase: m.phase,
        warmup: m.warmup,
        round: m.round,
        score: m.score,
        round_wins: m.round_wins,
        round_ends: m.round_ends,
        winner: m.winner,
        result: m.result,
        team_counts: world.resource::<CaTeamCounts>().0,
    };
    if *world.resource::<CaMatchView>() != view {
        world.insert_resource(view);
    }
}

/// The team counts now.
pub fn update_team_counts(world: &mut World) {
    let mut counts = [0u32; 2];
    for (_, t) in flow::players(world) {
        if let Some(i) = state::side(t) {
            counts[i] += 1;
        }
    }
    let counts = CaTeamCounts(counts);
    if *world.resource::<CaTeamCounts>() != counts {
        world.insert_resource(counts);
    }
}

/// S10: a player on a team leaves (its `Team` goes as it is despawned):
/// the mode's rule, with the leaver still counted.
#[allow(clippy::type_complexity)]
fn on_leave(
    leave: On<Remove, Team>,
    players: Query<(&Team, &Health, Has<Dead>), (With<Intent>, Without<Connecting>, Without<Spectating>, Without<Npc>)>,
    settings: Option<Res<CaRuleSettings>>,
    role: Option<Res<crate::core::NetRole>>,
    mut state: Option<ResMut<CaMatch>>,
) {
    if role.is_some_and(|r| *r == crate::core::NetRole::Client) {
        return;
    }
    let (Some(settings), Some(state)) = (settings, state.as_mut()) else { return };
    let Some(mode) = MODES.iter().find(|m| m.key == settings.mode) else { return };
    let Ok((team, health, dead)) = players.get(leave.entity) else { return };
    if state::side(team.0).is_none() {
        return;
    }
    let living = |h: &Health, d: bool| !d && h.current > 0.0;
    let mates = players.iter().filter(|(t, ..)| t.0 == team.0);
    let leaver = Leaver {
        team: team.0,
        team_players: mates.clone().count(),
        team_alive: mates.filter(|(_, h, d)| living(h, *d)).count(),
        alive: living(health, dead),
    };
    (mode.leaving)(state, &leaver);
}

/// Combat Arms' rule modules and the shared rules' systems.
pub struct CaRulesPlugin;

impl Plugin for CaRulesPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<RuleModules>() {
            rule_module::plugin(app);
        }
        for m in modules() {
            RuleModules::register(app, m);
        }
        settings::plugin(app);
        let server = crate::core::authoritative;
        app.init_resource::<CaMatch>()
            .init_resource::<CaMatchView>()
            .init_resource::<CaTeamCounts>()
            .init_resource::<flow::CaMissionFrames>()
            .init_resource::<spawns::CaSpawnCursors>()
            .init_resource::<scoring::CaDamageLog>()
            .insert_resource(crate::core::NewGame(flow::new_game))
            .add_observer(on_leave)
            .add_observer(flow::number_players)
            .add_systems(
                FixedUpdate,
                (
                    (
                        update_team_counts,
                        flow::mission_update,
                        spawns::ask_for_respawns,
                        crate::spawning::hold_the_dead,
                        flow::hold_frozen,
                        spawns::end_protection,
                        update_team_counts,
                        publish,
                    )
                        .chain()
                        .in_set(SimSet::Rules),
                    scoring::scale_friendly_fire
                        .after(SimSet::Weapons)
                        .before(crate::core::apply_damage),
                    (scoring::log_damage, scoring::handle_deaths, publish)
                        .chain()
                        .after(crate::core::apply_damage),
                )
                    .run_if(server.and_then(mode_on)),
            )
            .add_systems(FixedUpdate, publish.run_if(server.and_then(not(mode_on))))
            .console_command(
                "ca_restart",
                "Start the match over (ca_mode's mode, ca_goal's goal): everyone respawned at the next server frame.",
                |w, _| {
                    flow::restart(w);
                    Ok(Some("match restarted".into()))
                },
            )
            .console_command(
                "ca_match",
                "The match: mode, phase, team scores, round wins, players per team.",
                |w, _| {
                    let v = w.resource::<CaMatchView>().clone();
                    let now = w.resource::<SimClock>().now;
                    let left = v.round_ends.map(|e| format!(", {:.1} s left", (e - now).max(0.0)));
                    Ok(Some(format!(
                        "{} (G {}): {:?}{}, round {}{}; Alpha {} Bravo {}; round wins {}-{}; players {}-{}; result {:?}",
                        if v.mode.is_empty() { "none" } else { &v.mode },
                        v.goal,
                        v.phase,
                        if v.warmup { " (warm-up)" } else { "" },
                        v.round,
                        left.unwrap_or_default(),
                        v.score[0],
                        v.score[1],
                        v.round_wins[0],
                        v.round_wins[1],
                        v.team_counts[0],
                        v.team_counts[1],
                        v.result
                    )))
                },
            );
    }
}
