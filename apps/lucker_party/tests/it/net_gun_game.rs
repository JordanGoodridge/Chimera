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
        assert_eq!(held(&mut sim, 0), Some(ladder[kill]), "rung {kill}'s gun in client 0's hand");
        assert!(clip(&mut sim, 0).is_some_and(|c| c > 0), "rung {kill}: {} has rounds on its client", ladder[kill]);
        // The victim back in play and unprotected, 4 m in front of the shooter.
        assert!(
            sim.until(600, |s| server_alive(s, b) && s.server.app.world().get::<God>(b).is_none()),
            "the victim is back"
        );
        sim.ticks(30);
        assert_eq!(held(&mut sim, 1), Some(ladder[0]), "the victim, back, has the first rung's gun on its client");
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
            if !server_alive(&sim, b) {
                dead = true;
                break;
            }
        }
        sim.clients[0].intent(me).fire = false;
        assert!(dead, "kill {kill}: {} killed in 10 s", ladder[kill]);
        sim.ticks(40);
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
