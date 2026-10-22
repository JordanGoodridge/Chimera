//! The mode runtime's contract (`chimera_sim::mode`) on the test kit: a
//! made-up game (`kit`) registers two modes and its flow, and the runtime
//! steps the match through its phases at the game's frames, calling the
//! game's steps and the mode's hooks in the documented order; a counted
//! death's win ends the round at the next frame; the gameplay picks
//! whose modes play; free-for-all teams.

use bevy::prelude::*;
use chimera_core::{
    core::{Dead, FreezeTime, SimSet, Team},
    gameplay::Gameplay,
};
use chimera_sim::{
    mode::{
        self, ALPHA, BRAVO, Death, DeathKind, FFA_FIRST, Flow, Match, Mode, ModeAppExt, ModeGame,
        NO_HOOKS, Phase, Span, Teams, flow,
    },
    movement::placeholder,
    testkit::kit_sim,
};
use chimera_world::{greybox, harness::Sim};

/// What the kit's game and modes were asked to do, in order.
#[derive(Resource, Default)]
struct Log(Vec<String>);

fn log(world: &mut World, line: impl Into<String>) {
    world.get_resource_or_init::<Log>().0.push(line.into());
}

/// The mode the kit plays (`ca_mode`'s stand-in).
#[derive(Resource)]
struct Chosen(&'static str);

/// First side to 3 kills; respawning; no rounds' timer.
static DEATHMATCH: Mode = Mode {
    id: "kit:deathmatch",
    key: "dm",
    shown_as: "Deathmatch",
    description: "first side to 3 kills",
    respawns: true,
    warmup: false,
    warmup_revives: false,
    forced_setup: Some(0.0),
    teams: Teams::Two,
    on_death: |w, d| {
        let mut m = w.resource_mut::<Match>();
        let Some(i) = mode::side(mode::enemy(d.victim_team)) else { return };
        m.score[i] += 1;
        if m.score[i] >= 3 {
            m.win(i as u8 + 1);
        }
    },
    time_out: |_| {},
    match_end: |w| Some(w.resource::<Match>().winner),
    hooks: NO_HOOKS,
};

/// Rounds after a warm-up; Alpha wins a time-out; 2 round wins take the
/// match.
static ROUNDS: Mode = Mode {
    id: "kit:rounds",
    key: "rounds",
    shown_as: "Rounds",
    description: "rounds, 2 wins take the match",
    respawns: false,
    warmup: true,
    warmup_revives: true,
    forced_setup: None,
    teams: Teams::Two,
    on_death: |_, _| {},
    time_out: |w| w.resource_mut::<Match>().winner = Some(ALPHA),
    match_end: |w| {
        let m = w.resource::<Match>();
        m.round_wins.iter().any(|r| *r >= 2).then_some(Some(ALPHA))
    },
    hooks: mode::Hooks {
        setup: |w| log(w, "mode setup"),
        before_spawns: |w| log(w, "mode before_spawns"),
        round_over: |w| log(w, "mode round_over"),
        round_end: |w| log(w, "mode round_end"),
        after_round: |w| log(w, "mode after_round"),
        ..NO_HOOKS
    },
};

/// Every player a team of their own.
static FREE_FOR_ALL: Mode = Mode {
    id: "kit:ffa",
    key: "ffa",
    teams: Teams::FreeForAll,
    ..DEATHMATCH
};

const FRAME: f64 = 0.1;

const FLOW: Flow = Flow {
    frame: FRAME,
    players: |w| {
        let mut all: Vec<(Entity, u8)> = w
            .query::<(Entity, &Team)>()
            .iter(w)
            .filter(|(_, t)| mode::plays(t.0))
            .map(|(e, t)| (e, t.0))
            .collect();
        all.sort();
        all
    },
    respawn: |w, e| {
        chimera_sim::spawning::revive(w, e, None);
        log(w, "respawn");
    },
    length: |w, span| match span {
        Span::Setup => Some(1.0),
        Span::Round => (!mode::current_mode(w).is_some_and(|m| m.respawns)).then_some(5.0),
        Span::Warmup => Some(2.0),
        Span::RoundOver => Some(1.0),
        Span::Intermission => Some(1.0),
        Span::Revive => Some(0.5),
    },
    event: |w, e| log(w, format!("{e:?}")),
};

fn kit_game(game: &'static str) -> ModeGame {
    ModeGame {
        game,
        chosen: |w| w.get_resource::<Chosen>().map(|c| c.0),
        flow: FLOW,
    }
}

/// The kit's sim playing `key`, its runtime's systems in the rules' set.
fn sim(key: &'static str) -> Sim {
    let mut sim = kit_sim();
    sim.app
        .insert_resource(Chosen(key))
        .init_resource::<Log>()
        .init_resource::<Match>()
        .add_modes(kit_game("kit"), &[&DEATHMATCH, &ROUNDS, &FREE_FOR_ALL])
        .add_systems(
            FixedUpdate,
            (flow::update, flow::mission_update).chain().in_set(SimSet::Rules),
        );
    sim
}

fn player(sim: &mut Sim, team: u8) -> Entity {
    let p = sim.spawn_character(greybox::SPAWNS[0], placeholder::ID);
    sim.app.world_mut().entity_mut(p).insert(Team(team));
    p
}

fn phase(sim: &Sim) -> Phase {
    sim.app.world().resource::<Match>().phase
}

fn now(sim: &Sim) -> f64 {
    sim.app.world().resource::<chimera_core::core::SimClock>().now
}

/// Tick until `want` holds (at most `secs`), the time it took.
fn until(sim: &mut Sim, secs: f64, want: impl Fn(Phase) -> bool) -> f64 {
    let start = now(sim);
    while !want(phase(sim)) {
        assert!(now(sim) - start <= secs, "still {:?} after {secs} s", phase(sim));
        sim.ticks(1);
    }
    now(sim) - start
}

fn log_of(sim: &mut Sim) -> Vec<String> {
    std::mem::take(&mut sim.app.world_mut().resource_mut::<Log>().0)
}

#[test]
fn a_match_walks_its_phases_in_order() {
    let mut sim = sim("rounds");
    sim.seconds(0.5);
    assert_eq!(phase(&sim), Phase::Waiting, "nobody in");
    let a = player(&mut sim, ALPHA);
    let _b = player(&mut sim, BRAVO);
    until(&mut sim, 0.2, |p| p == Phase::Starting);
    log_of(&mut sim);
    until(&mut sim, 0.2, |p| matches!(p, Phase::Setup { .. }));
    // The first mission start: the game's steps and the mode's hooks in
    // order, everyone respawned.
    assert_eq!(
        log_of(&mut sim),
        [
            "Prepare { first: true }",
            "ResetStats",
            "MissionStart",
            "mode before_spawns",
            "respawn",
            "respawn",
            "mode setup"
        ]
    );
    assert!(sim.app.world().resource::<FreezeTime>().0, "frozen in the setup");
    let m = sim.app.world().resource::<Match>().clone();
    assert!(m.warmup, "the match starts with its warm-up");
    // Live at the frame after the setup's end, unfrozen, the warm-up's
    // timer.
    let t = until(&mut sim, 1.2, |p| p == Phase::Live);
    assert!((1.0..=1.0 + FRAME + 0.02).contains(&t), "setup {t}");
    assert!(!sim.app.world().resource::<FreezeTime>().0);
    let ends = sim.app.world().resource::<Match>().round_ends.expect("a timer");
    assert!((ends - now(&sim) - 2.0).abs() < 1e-6);
    // The warm-up revives the dead.
    let since = now(&sim);
    sim.app.world_mut().entity_mut(a).insert(Dead { since });
    sim.seconds(0.7);
    assert!(sim.app.world().get::<Dead>(a).is_none(), "revived in the warm-up");
    // The warm-up's time-out: no winner, nothing counted.
    until(&mut sim, 2.0, |p| matches!(p, Phase::RoundOver { .. }));
    let m = sim.app.world().resource::<Match>().clone();
    assert_eq!((m.winner, m.round_wins, m.results.len()), (None, [0, 0], 1));
    until(&mut sim, 1.2, |p| matches!(p, Phase::Intermission { .. }));
    assert_eq!(sim.app.world().resource::<Match>().round, 0, "the warm-up isn't a round");
    log_of(&mut sim);
    // The real rounds: the time-out's winner (Alpha) takes each.
    until(&mut sim, 1.2, |p| matches!(p, Phase::Setup { .. }));
    assert_eq!(log_of(&mut sim)[0], "Prepare { first: false }", "not the first mission");
    until(&mut sim, 1.2, |p| p == Phase::Live);
    until(&mut sim, 5.2, |p| matches!(p, Phase::RoundOver { .. }));
    assert_eq!(sim.app.world().resource::<Match>().round_wins, [1, 0]);
    until(&mut sim, 1.2, |p| matches!(p, Phase::Intermission { .. }));
    assert_eq!(
        log_of(&mut sim),
        ["mode round_over", "RoundOver", "mode round_end", "RoundEnd", "RoundEnded", "mode after_round"]
    );
    until(&mut sim, 10.0, |p| p == Phase::MatchOver);
    let m = sim.app.world().resource::<Match>().clone();
    assert_eq!((m.result, m.round, m.round_wins), (Some(Some(ALPHA)), 2, [2, 0]));
}

#[test]
fn a_counted_win_ends_the_round_at_the_next_frame() {
    let mut sim = sim("dm");
    let a = player(&mut sim, ALPHA);
    let b = player(&mut sim, BRAVO);
    until(&mut sim, 1.0, |p| p == Phase::Live);
    assert_eq!(sim.app.world().resource::<Match>().round_ends, None, "no timer");
    // A game counts deaths through the playing mode's rule.
    let count = |sim: &mut Sim| {
        let world = sim.app.world_mut();
        let mode = mode::current_mode(world).expect("a mode plays");
        let death = Death {
            victim: b,
            victim_team: BRAVO,
            killer: Some(a),
            kind: DeathKind::Enemy,
        };
        (mode.on_death)(world, &death);
    };
    for _ in 0..3 {
        count(&mut sim);
    }
    let m = sim.app.world().resource::<Match>().clone();
    assert!(m.decided() && m.phase == Phase::Live, "decided, still live until the frame");
    let t = until(&mut sim, 0.2, |p| matches!(p, Phase::RoundOver { .. }));
    assert!(t <= FRAME + 0.02);
    until(&mut sim, 3.0, |p| p == Phase::MatchOver);
    assert_eq!(sim.app.world().resource::<Match>().result, Some(Some(ALPHA)));
}

#[test]
fn free_for_all_players_get_teams_of_their_own() {
    let mut sim = sim("ffa");
    let a = player(&mut sim, ALPHA);
    let b = player(&mut sim, ALPHA);
    until(&mut sim, 1.0, |p| matches!(p, Phase::Setup { .. }));
    let team = |sim: &Sim, e| sim.app.world().get::<Team>(e).unwrap().0;
    let mut teams = [team(&sim, a), team(&sim, b)];
    teams.sort();
    assert_eq!(teams, [FFA_FIRST, FFA_FIRST + 1], "the lowest free ids");
}

#[test]
fn the_gameplay_picks_whose_modes_play() {
    let mut sim = sim("dm");
    sim.app.add_modes(kit_game("other"), &[]);
    assert_eq!(mode::current_mode(sim.app.world()).map(|m| m.id), Some("kit:deathmatch"), "the first game");
    let mut other = Gameplay {
        rules: "other",
        hud: "other",
        binds: "other",
        tick: std::time::Duration::from_millis(15),
    };
    sim.app.world_mut().insert_resource(other.clone());
    assert_eq!(mode::current_mode(sim.app.world()).map(|m| m.id), None, "the other game has no modes");
    other.rules = "kit";
    sim.app.world_mut().insert_resource(other);
    assert_eq!(mode::current_mode(sim.app.world()).map(|m| m.id), Some("kit:deathmatch"));
    // The registry lists them by id.
    let modes = sim.app.world().resource::<mode::Modes>();
    assert_eq!(modes.get("kit:ffa").map(|m| m.teams), Some(Teams::FreeForAll));
}
