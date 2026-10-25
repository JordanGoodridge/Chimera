//! Map rotation and the end-of-match results (architecture review L2/L3
//! polish): a two-rung ladder on a pool of two maps, played to its end on
//! `NetSim`'s in-memory link. The results (winner first, each player's
//! rung, kills and deaths) reach every client through the intermission;
//! after the hold the server moves to the pool's next map by
//! `change_match` (the clients stay connected, the scores anew, the
//! results gone), or restarts on this map with `rotate = false`.

use bevy::prelude::*;
use chimera_core::{
    console::ConsoleAppExt,
    core::{Damage, DamageKind, Dead, Hitgroup, Score},
};
use chimera_net::{harness::NetSim, net::client::Joined};
use chimera_sim::{
    minigame::{MatchResults, hud::MinigameHud, ladder::Rung},
    mode::{Match, Phase},
};

use crate::net_party::{failure, party};

/// A two-rung ladder on a pool of two maps, short enough to finish in a
/// test.
const ROTATING: &str = r#"
id = "test:rotating"
name = "Rotating ladder"
players = { min = 2, max = 8 }
maps = ["chimera:greybox", "test:next"]
teams = "free_for_all"
time_limit = "5m"

[slots]
weapons = ["cs_source"]

[spawn]
respawn = { delay = "1s", protection = "0s" }
points = "map"

[rules]
module = "chimera:ladder"
ladder = ["cs_source:weapon_deagle", "cs_source:weapon_ak47"]
"#;

/// The levels the server was asked to change to (the test's
/// `changelevel`, which loads nothing).
#[derive(Resource, Default)]
struct Levels(Vec<String>);

/// `by` kills `victim` on the server with one hit.
fn hit(sim: &mut NetSim, victim: Entity, by: Entity) {
    let at = sim.server.position(victim);
    sim.server.app.world_mut().write_message(Damage {
        target: victim,
        attacker: Some(by),
        amount: 5.0,
        point: at,
        dir: Vec3::NEG_Z,
        hitgroup: Hitgroup::Chest,
        kind: DamageKind::Bullet,
        weapon: None,
        force: Vec3::ZERO,
    });
    sim.ticks(3);
}

/// Two clients joined to `text`'s ladder, and the first one's character
/// wins it (two kills on the second's).
fn a_won_match(text: &str) -> (NetSim, Entity) {
    let mut sim = party(2, Some(text));
    sim.server.app.init_resource::<Levels>();
    sim.server.app.console_command("changelevel", "changelevel <map>: a test's.", |w, a| {
        w.resource_mut::<Levels>().0.push(a.first().cloned().unwrap_or_default());
        Ok(None)
    });
    sim.until_joined(900);
    sim.ticks(120);
    let (a, b) = (sim.character_of(0).unwrap(), sim.character_of(1).unwrap());
    for _ in 0..2 {
        assert!(sim.until(600, |s| s.server.app.world().get::<Dead>(b).is_none()), "the victim is back");
        sim.ticks(30);
        hit(&mut sim, b, a);
    }
    (sim, a)
}

#[test]
fn the_results_reach_every_client_and_the_party_moves_to_the_next_map() {
    let (mut sim, a) = a_won_match(ROTATING);
    // The match is decided: the results (winner first) are on the server
    // and, replicated, on both clients, through the intermission.
    let ok = sim.until(1500, |s| s.clients.iter().all(|c| c.app.world().get_resource::<MatchResults>().is_some()));
    assert!(ok, "both clients get the results");
    let server = sim.server.app.world().resource::<MatchResults>().clone();
    assert_eq!(server.rungs, 2);
    assert_eq!(server.rows.len(), 2);
    assert!(server.rows[0].winner && server.rows[0].player == Some(a), "{server:?}");
    assert_eq!((server.rows[0].rung, server.rows[0].kills), (Some(2), 2), "{server:?}");
    assert_eq!((server.rows[1].rung, server.rows[1].deaths), (Some(1), 2), "{server:?}");
    for i in 0..2 {
        let got = sim.clients[i].app.world().resource::<MatchResults>().clone();
        assert_eq!(got.rows.len(), 2, "client {i}");
        assert_eq!(got.title, server.title);
        for (g, s) in got.rows.iter().zip(&server.rows) {
            assert_eq!((g.winner, g.rung, g.kills, g.deaths), (s.winner, s.rung, s.kills, s.deaths), "client {i}");
            assert!(!g.name.is_empty(), "client {i}: players are named");
        }
        assert!(got.rows[0].winner && !got.rows[1].winner, "client {i}: the winner first");
        let hud = sim.clients[i].app.world().resource::<MinigameHud>();
        assert_eq!(hud.results.as_ref().map(|r| r.rows.len()), Some(2), "client {i}'s HUD data");
    }
    assert!(sim.server.app.world().resource::<Levels>().0.is_empty(), "not yet: the results are up first");
    // After the hold the server rotates: the pool's next map, the minigame
    // started anew; the clients stay connected and the results go.
    let ok = sim.until(3000, |s| !s.server.app.world().resource::<Levels>().0.is_empty());
    assert!(ok, "the server moves on");
    assert_eq!(sim.server.app.world().resource::<Levels>().0, ["test:next"], "the pool's second map");
    sim.ticks(120);
    assert!(sim.server.app.world().get_resource::<MatchResults>().is_none(), "a new match, no results");
    for i in 0..2 {
        assert!(sim.clients[i].app.world().contains_resource::<Joined>(), "client {i} still in");
        assert!(failure(&sim, i).is_none(), "client {i} never left");
        assert!(sim.clients[i].app.world().get_resource::<MatchResults>().is_none(), "client {i}: the results are gone");
        let me = sim.local_player(i).unwrap();
        let w = sim.clients[i].app.world();
        assert_eq!(w.get::<Score>(me).map(|s| s.kills), Some(0), "client {i}: scores anew");
        assert_eq!(w.get::<Rung>(me).map(|r| r.0), Some(0), "client {i}: the first rung");
    }
}

#[test]
fn without_rotation_the_same_map_is_played_again() {
    let (mut sim, _a) = a_won_match(&ROTATING.replace("time_limit", "rotate = false\ntime_limit"));
    assert!(sim.until(1500, |s| s.server.app.world().get_resource::<MatchResults>().is_some()));
    assert!(sim.until(3000, |s| s.server.app.world().get_resource::<MatchResults>().is_none()), "the match restarts");
    sim.ticks(240);
    assert!(sim.server.app.world().resource::<Levels>().0.is_empty(), "no level change");
    assert_eq!(sim.server.app.world().resource::<Match>().phase, Phase::Live, "a new match on the same map");
    assert!((0..2).all(|i| sim.clients[i].app.world().contains_resource::<Joined>()));
}
