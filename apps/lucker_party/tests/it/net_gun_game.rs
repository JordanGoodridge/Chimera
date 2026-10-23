//! The gun game over the network (docs/plans/active/multiplayer.md,
//! `minigame_net`): a server playing `originals:gun_game_dust2` and two
//! clients that only `connect` (on `NetSim`'s in-memory link, the party's
//! simulation on the greybox). The clients learn the minigame from the
//! server, show its HUD (rung, next gun, leader, the match's state), and
//! follow a player up the ladder: each rung's gun, CS:S's and Combat
//! Arms', drawn on both clients with rounds in it, to the winner and the
//! match over.

use std::time::Duration;

use bevy::prelude::*;
use chimera_core::{
    console::Console,
    core::{Dead, God, Score},
    gameplay::Gameplay,
};
use chimera_net::{harness::NetSim, minigame_net::NetMinigame, net::memory::LinkConditions};
use chimera_sim::{
    minigame::{self, MINIGAME, hud::MinigameHud, ladder::Rung, runtime::InMinigame},
    mode::{Match, Phase},
    testkit,
    weapon::Magazine,
};
use chimera_world::greybox::GreyboxMapPlugin;
use combat_arms::{
    game::TICK_INTERVAL,
    harness::CaSim,
};
use lucker_party::{DEFAULT_MINIGAME, PartyPartsPlugin};

fn setup(app: &mut App) {
    app.add_plugins((GreyboxMapPlugin, PartyPartsPlugin));
}

/// A server playing the gun game and two clients that came in with no
/// minigame of their own.
fn joined() -> NetSim {
    let mut sim = NetSim::new(
        CaSim,
        LinkConditions {
            latency: Duration::from_millis(20),
            jitter: Duration::ZERO,
            loss: 0.0,
        },
        7,
        2,
        setup,
    );
    sim.set_frame(TICK_INTERVAL);
    minigame::start(sim.server.app.world_mut(), DEFAULT_MINIGAME).expect("the gun game starts");
    sim.until_joined(900);
    sim.ticks(300);
    sim
}

fn hud_lines(sim: &NetSim, i: usize) -> Vec<String> {
    let hud = sim.clients[i].app.world().resource::<MinigameHud>();
    hud.lines.iter().map(|l| l.text.clone()).collect()
}

fn held(sim: &mut NetSim, i: usize) -> Option<&'static str> {
    let me = sim.local_player(i)?;
    testkit::active_id(sim.clients[i].app.world(), me)
}

/// Client `i`'s own rounds in the clip of the gun in hand.
fn clip(sim: &mut NetSim, i: usize) -> Option<u32> {
    let me = sim.local_player(i)?;
    let w = sim.clients[i].app.world();
    w.get::<Magazine>(testkit::active(w, me)?).map(|m| m.clip)
}

fn server_alive(sim: &NetSim, e: Entity) -> bool {
    sim.server.app.world().get::<Dead>(e).is_none()
}

#[test]
fn a_client_that_only_connects_plays_the_servers_minigame() {
    let mut sim = joined();
    for i in 0..2 {
        let w = sim.clients[i].app.world();
        assert_eq!(w.resource::<Gameplay>().rules, MINIGAME, "client {i} plays the minigame's rules");
        assert_eq!(w.resource::<minigame::ActiveMinigame>().def.id, DEFAULT_MINIGAME);
        assert!(w.contains_resource::<minigame::ladder::Ladder>(), "client {i} has the ladder");
        let me = sim.local_player(i).unwrap();
        let w = sim.clients[i].app.world();
        assert!(w.get::<InMinigame>(me).is_some(), "client {i}'s player is in the minigame");
        assert_eq!(w.get::<Rung>(me).map(|r| r.0), Some(0));
        assert_eq!(held(&mut sim, i), Some(cs_source::game::weapons::GLOCK), "the first rung's Glock on client {i}");
        assert!(clip(&mut sim, i).is_some_and(|c| c > 0), "rounds in client {i}'s clip");
    }
    // The match's state: live (both are in), a minigame entity for each.
    let m = sim.clients[0].app.world().resource::<Match>();
    assert_eq!(m.phase, Phase::Live);
    assert!(m.round_ends.is_some());
    let lines = hud_lines(&sim, 0);
    assert!(lines.iter().any(|l| l.starts_with("Rung 1/10")), "{lines:?}");
    assert!(lines.iter().any(|l| l.starts_with("Next:")), "{lines:?}");
    assert!(lines.iter().any(|l| l.starts_with("Time ")), "the match's time: {lines:?}");
    assert!(!lines.iter().any(|l| l.contains("Waiting")), "{lines:?}");
    let w = sim.clients[1].app.world_mut();
    let n = w.query::<&NetMinigame>().iter(w).count();
    assert_eq!(n, 1, "one replicated match");
}

#[test]
fn a_client_cant_start_or_restart_the_minigame_itself() {
    let mut sim = joined();
    for line in ["minigame originals:gun_game_dust2", "minigame_restart"] {
        sim.clients[0].app.world_mut().resource_mut::<Console>().submit(line);
        sim.ticks(2);
    }
    let out: Vec<String> =
        sim.clients[0].app.world().resource::<Console>().output.iter().map(|l| l.text.clone()).collect();
    assert_eq!(out.iter().filter(|l| l.contains("the server picks the minigame")).count(), 2, "{out:?}");
    assert_eq!(
        sim.clients[0].app.world().resource::<Match>().phase,
        Phase::Live,
        "the match wasn't started over"
    );
}

/// Client 0 (character `a`) shoots client 1 (`b`) dead with rung `kill`'s
/// gun, which each client sees in its owner's hand with rounds in it.
fn kill_once(sim: &mut NetSim, kill: usize, a: Entity, b: Entity, ladder: &[&'static str]) {
    assert_eq!(held(sim, 0), Some(ladder[kill]), "rung {kill}'s gun in client 0's hand");
    assert!(clip(sim, 0).is_some_and(|c| c > 0), "rung {kill}: {} has rounds on its client", ladder[kill]);
    // The victim back in play and unprotected, 4 m in front of the shooter.
    assert!(
        sim.until(600, |s| server_alive(s, b) && s.server.app.world().get::<God>(b).is_none()),
        "the victim is back"
    );
    sim.ticks(30);
    assert_eq!(held(sim, 1), Some(ladder[0]), "the victim, back, has the first rung's gun on its client");
    let at = sim.server.position(a) + Vec3::new(0.0, 0.0, -4.0);
    {
        let w = sim.server.app.world_mut();
        w.get_mut::<Transform>(b).unwrap().translation = at;
        w.get_mut::<avian3d::prelude::Position>(b).unwrap().0 = at;
    }
    sim.ticks(90);
    let me = sim.local_player(0).unwrap();
    let mut dead = false;
    for t in 0..(10.0 / TICK_INTERVAL) as u64 {
        let target = sim.server.position(b);
        testkit::aim_at(&mut sim.clients[0], me, target);
        sim.clients[0].intent(me).fire = t % 2 == 0;
        sim.step();
        if !server_alive(sim, b) {
            dead = true;
            break;
        }
    }
    sim.clients[0].intent(me).fire = false;
    assert!(dead, "kill {kill}: {} killed in 10 s", ladder[kill]);
    sim.ticks(40);
}

/// Client `i`'s minigame panel: the rules' status lines, then the match's
/// own line (its time, waiting or over).
fn panel(sim: &NetSim, i: usize) -> (Vec<String>, String) {
    let mut lines = hud_lines(sim, i);
    let phase = lines.pop().unwrap_or_default();
    (lines, phase)
}

/// What the server's rules tell player `who` (a server entity), as HUD text
/// (`{weapon}` and `{player}` left in).
fn server_status(sim: &mut NetSim, who: Entity) -> Vec<String> {
    let w = sim.server.app.world_mut();
    let status = w.resource::<minigame::ActiveMinigame>().rules.status;
    status(w, who).into_iter().map(|l| l.text).collect()
}

/// The first line of the server's status for `who` without its `{...}`
/// placeholders' words: what a client's line must start with.
fn stem(line: &str) -> &str {
    line.split('{').next().unwrap_or(line).trim_end_matches([' ', ':'])
}

/// Client 0 kills client 1 with each of the first rungs' guns. Before each
/// kill and after the last, each client's minigame panel says what the
/// server's rules say to its own player (rung, next gun, leader), the time
/// on it is the server's, the match is live (never "waiting"), and the HUD
/// in use is the game of the gun in hand (CS:S's and Combat Arms'
/// alternate on this ladder).
#[test]
fn each_clients_minigame_panel_follows_the_servers_state_and_the_held_gun() {
    let mut sim = joined();
    let (a, b) = (sim.character_of(0).unwrap(), sim.character_of(1).unwrap());
    let ladder = sim.server.app.world().resource::<minigame::ladder::Ladder>().rungs.clone();
    for kill in 0..=4usize {
        for (i, who) in [(0usize, a), (1, b)] {
            let (lines, phase) = panel(&sim, i);
            let want = server_status(&mut sim, who);
            assert_eq!(lines.len(), want.len(), "kill {kill}, client {i}: {lines:?} against the server's {want:?}");
            for (got, want) in lines.iter().zip(&want) {
                assert!(got.starts_with(stem(want)), "kill {kill}, client {i}: {got:?} against {want:?}");
            }
            let rung = if i == 0 { kill } else { 0 };
            assert!(lines[0].starts_with(&format!("Rung {}/10", rung + 1)), "kill {kill}, client {i}: {lines:?}");
            assert_eq!(
                lines.last().is_some_and(|l| l.starts_with("Leader:")),
                i == 1 && kill > 0,
                "kill {kill}, client {i}: only the one behind is told who leads: {lines:?}"
            );
            // The match: live, with the server's time.
            let time = phase.strip_prefix("Time ").unwrap_or_else(|| panic!("kill {kill}, client {i}: {phase:?}"));
            let (m, s) = time.split_once(':').unwrap();
            let shown = m.parse::<f64>().unwrap() * 60.0 + s.parse::<f64>().unwrap();
            let w = sim.server.app.world();
            let left = w.resource::<Match>().round_ends.unwrap() - w.resource::<chimera_core::core::SimClock>().now;
            assert!((shown - left).abs() < 3.0, "kill {kill}, client {i}: {phase} against {left:.1} s left");
            // The HUD in use is the held gun's game's.
            // (The one just killed holds nothing until it respawns.)
            let Some(gun) = held(&mut sim, i) else {
                assert_eq!((i, kill > 0), (1, true), "kill {kill}: client {i} holds a gun");
                continue;
            };
            let w = sim.clients[i].app.world();
            let game = w.resource::<chimera_core::game_registry::Games>().get(gun.split(':').next().unwrap()).map(|g| g.id);
            assert_eq!(Some(w.resource::<Gameplay>().hud), game, "kill {kill}, client {i}: the HUD of {gun}");
        }
        if kill < 4 {
            kill_once(&mut sim, kill, a, b, &ladder);
        }
    }
}

/// Client 0 shoots client 1 with every rung's gun to the win. Each client
/// draws the rung's gun with rounds in it, the HUD follows (rung, next
/// gun, leader), and the match ends with the shooter the winner on both.
#[test]
fn a_ladder_climbed_by_a_client_shows_on_both_clients() {
    let mut sim = joined();
    let (a, b) = (sim.character_of(0).unwrap(), sim.character_of(1).unwrap());
    let ladder = sim.server.app.world().resource::<minigame::ladder::Ladder>().rungs.clone();
    assert_eq!(ladder.len(), 10);
    for kill in 0..10usize {
        kill_once(&mut sim, kill, a, b, &ladder);
        if kill < 9 {
            let rung = |sim: &mut NetSim, i: usize| {
                let me = sim.local_player(i).unwrap();
                sim.clients[i].app.world().get::<Rung>(me).map(|r| r.0)
            };
            assert_eq!(rung(&mut sim, 0), Some(kill + 1), "client 0 sees its rung up");
            assert_eq!(rung(&mut sim, 1), Some(0), "client 1 sees its own rung");
            let lines = hud_lines(&sim, 0);
            assert!(lines.iter().any(|l| l.starts_with(&format!("Rung {}/10", kill + 2))), "{lines:?}");
            let theirs = hud_lines(&sim, 1);
            assert!(theirs.iter().any(|l| l.starts_with("Leader:") || l.starts_with("Tied")), "{theirs:?}");
        }
    }
    let team_a = sim.server.app.world().get::<chimera_core::core::Team>(a).unwrap().0;
    assert_eq!(sim.server.app.world().resource::<Match>().winner, Some(team_a));
    assert_eq!(sim.server.app.world().get::<Score>(a).unwrap().kills, 10);
    // The match ends: over on both clients, the HUD says so.
    sim.until(1200, |s| s.clients.iter().all(|c| c.app.world().resource::<Match>().phase == Phase::MatchOver));
    for i in 0..2 {
        assert_eq!(sim.clients[i].app.world().resource::<Match>().phase, Phase::MatchOver, "client {i}");
        let lines = hud_lines(&sim, i);
        assert!(lines.iter().any(|l| l == "Match over"), "client {i}: {lines:?}");
    }
    let kills = {
        let me = sim.local_player(0).unwrap();
        sim.clients[0].app.world().get::<Score>(me).map(|s| s.kills)
    };
    assert_eq!(kills, Some(10), "the scoreboard's kills on the winner's own client");
}
