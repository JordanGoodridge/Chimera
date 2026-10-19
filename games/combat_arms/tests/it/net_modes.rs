//! Combat Arms' game modes over the network (specs/combat_arms/
//! game_modes.md; docs/plans/active/combat-arms.md, "Network part"): a
//! server and two clients on `NetSim`'s in-memory link playing Team Death
//! Match on the greybox with a team's row of start points each. The two
//! join on opposite teams, a kill scores on every machine (the team
//! counter and the killer's statistics), the dead player's client asks to
//! respawn after its 7 s and comes back at its team's point, and the
//! deciding kill ends the match on every machine.

use std::time::Duration;

use bevy::prelude::*;
use chimera_core::core::{Damage, DamageKind, Dead, Health, Hitgroup, SimClock, SpawnPoint, Team};
use chimera_net::{harness::NetSim, net::memory::LinkConditions};
use chimera_world::greybox::GreyboxMapPlugin;
use combat_arms::{
    game::{
        TICK_INTERVAL,
        rules::{CaMatch, CaMatchView, CaRuleSettings, CaStats, Phase},
        weapons::RIFLE,
    },
    harness::CaSim,
};

fn setup(app: &mut App) {
    app.add_plugins(GreyboxMapPlugin);
    let w = app.world_mut();
    {
        let mut s = w.resource_mut::<CaRuleSettings>();
        s.mode = "TDM".into();
        s.goal = 2;
    }
    for (k, team) in [1u8, 2].into_iter().enumerate() {
        for i in 0..4 {
            w.spawn((
                Name::new(format!("Spawn {}", k * 4 + i)),
                SpawnPoint { team: Some(Team(team)) },
                Transform::from_translation(Vec3::new(-6.0 + 3.0 * i as f32, 1.0, if team == 1 { 12.0 } else { -20.0 })),
            ));
        }
    }
}

fn view(sim: &NetSim, i: usize) -> CaMatchView {
    sim.clients[i].app.world().resource::<CaMatchView>().clone()
}

fn server_match(sim: &NetSim) -> CaMatch {
    sim.server.app.world().resource::<CaMatch>().clone()
}

/// `killer` kills `victim` on the server.
fn kill(sim: &mut NetSim, victim: Entity, killer: Entity) {
    sim.server.app.world_mut().write_message(Damage {
        target: victim,
        attacker: Some(killer),
        amount: 50.0,
        point: Vec3::ZERO,
        dir: Vec3::NEG_Z,
        hitgroup: Hitgroup::Generic,
        kind: DamageKind::Bullet,
        weapon: Some(RIFLE),
        force: Vec3::ZERO,
    });
}

#[test]
fn two_clients_play_team_death_match() {
    let link = LinkConditions {
        latency: Duration::from_millis(30),
        jitter: Duration::ZERO,
        loss: 0.0,
    };
    let mut sim = NetSim::new(CaSim, link, 7, 2, setup);
    sim.set_frame(TICK_INTERVAL);
    sim.until_joined(600);
    let ok = sim.until(400, |s| server_match(s).phase == Phase::Live && view(s, 0).phase == Phase::Live);
    assert!(ok, "live on the server and the client: {:?}", server_match(&sim).phase);
    let (a, b) = (sim.character_of(0).unwrap(), sim.character_of(1).unwrap());
    let team = |sim: &NetSim, e| sim.server.app.world().get::<Team>(e).unwrap().0;
    let (ta, tb) = (team(&sim, a), team(&sim, b));
    assert!(ta != tb && [ta, tb].iter().all(|t| *t == 1 || *t == 2), "opposite teams: {ta} {tb}");
    for i in 0..2 {
        assert_eq!(view(&sim, i).team_counts, [1, 1], "client {i} sees one player a team");
        assert_eq!(view(&sim, i).mode, "TDM");
        assert_eq!(view(&sim, i).goal, 2);
    }
    // Past the spawn protection: a kills b.
    sim.ticks((4.2 / TICK_INTERVAL) as u64);
    kill(&mut sim, b, a);
    let side = |t: u8| t as usize - 1;
    let scored = sim.until(60, |s| (0..2).all(|i| view(s, i).score[side(ta)] == 1));
    assert!(scored, "the kill scores on every client: {:?}", view(&sim, 0).score);
    let me = sim.local_player(0).unwrap();
    assert_eq!(sim.clients[0].app.world().get::<CaStats>(me).map(|s| s.kills), Some(1), "the killer's stats");
    let dead_b = sim.local_player(1).unwrap();
    assert!(sim.clients[1].app.world().get::<Dead>(dead_b).is_some(), "b's client sees itself dead");
    // b's client asks after its 7 s; the server respawns it.
    let died = sim.server.app.world().resource::<SimClock>().now;
    let back = sim.until(700, |s| s.server.app.world().get::<Dead>(b).is_none());
    assert!(back, "respawned");
    let after = sim.server.app.world().resource::<SimClock>().now - died;
    assert!((7.0..8.0).contains(&after), "respawned {after:.2} s after the death");
    let health = sim.server.app.world().get::<Health>(b).unwrap();
    assert_eq!(health.current, health.max);
    let alive = sim.until(60, |s| {
        let me = s.local_player(1).unwrap();
        s.clients[1].app.world().get::<Dead>(me).is_none()
    });
    assert!(alive, "b's client sees itself alive");
    // The deciding kill: the match ends everywhere.
    sim.ticks((4.2 / TICK_INTERVAL) as u64);
    kill(&mut sim, b, a);
    let over = sim.until(800, |s| (0..2).all(|i| view(s, i).phase == Phase::MatchOver));
    assert!(over, "match over on every client: {:?}", view(&sim, 0).phase);
    for i in 0..2 {
        let v = view(&sim, i);
        assert_eq!(v.result, Some(Some(ta)), "client {i}: {v:?}");
        assert_eq!(v.score[side(ta)], 2);
    }
}
