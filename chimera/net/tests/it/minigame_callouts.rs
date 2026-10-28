//! A minigame's callouts over the network (`minigame_net`'s events, the
//! sim's `minigame::hud`) on the engine's base and the test kit's guns:
//! a server playing a three-rung ladder and two clients that only
//! connect. When client 0's player reaches the last rung, client 0's HUD
//! data holds "Last level!" (cue `last_rung`) and client 1's "<that
//! player> is on the last level!" (cue `last_rung_other`), the player
//! being client 1's own copy of it.

use std::time::Duration;

use bevy::prelude::*;
use chimera_core::{
    core::{Damage, DamageKind, Dead, God, Hitgroup},
    slots::Loadout,
};
use chimera_net::{harness::NetSim, minigame_net::MinigameNetPlugin, net::memory::LinkConditions};
use chimera_sim::{
    SimBasePlugins,
    minigame::{self, MinigameAppExt, MinigamePlugin, hud::MinigameHud, ladder::Rung},
    movement::placeholder,
    testkit,
};
use chimera_world::greybox::GreyboxMapPlugin;

const DEF: &str = r#"
id = "kit:gun_game"
name = "Kit gun game"
players = { min = 2, max = 4 }
maps = ["chimera:greybox"]
teams = "free_for_all"
time_limit = "60s"
rotate = false

[spawn]
respawn = { delay = "1s", protection = "0.5s" }
points = "map"

[rules]
module = "chimera:ladder"
ladder = ["kit:pistol", "kit:rifle", "kit:shotgun"]
"#;

fn setup(app: &mut App) {
    app.add_plugins((GreyboxMapPlugin, MinigamePlugin, MinigameNetPlugin)).insert_resource(Loadout {
        movement: placeholder::ID,
    });
    testkit::add_weapons(app);
    app.add_minigames(&[("kit_gun_game.toml", DEF)]).expect("the definition reads");
}

/// Client `i`'s callout: its lines' texts, the first line's player, the cue.
fn callout(sim: &NetSim, i: usize) -> Option<(Vec<String>, Option<Entity>, Option<&'static str>)> {
    let hud = sim.clients[i].app.world().resource::<MinigameHud>();
    let c = &hud.callout.as_ref()?.callout;
    Some((c.lines.iter().map(|l| l.text.clone()).collect(), c.lines[0].player, c.cue))
}

/// The server's `b` killed by `a` (once both are alive and unprotected).
fn kill(sim: &mut NetSim, a: Entity, b: Entity) {
    let ready = |s: &mut NetSim| {
        let w = s.server.app.world();
        [a, b].iter().all(|e| w.get::<Dead>(*e).is_none() && w.get::<God>(*e).is_none())
    };
    assert!(sim.until(600, ready), "both in play");
    let at = sim.server.position(b);
    sim.server.app.world_mut().write_message(Damage {
        target: b,
        attacker: Some(a),
        amount: 500.0,
        point: at,
        dir: Vec3::NEG_Z,
        hitgroup: Hitgroup::Chest,
        kind: DamageKind::Bullet,
        weapon: None,
        force: Vec3::ZERO,
    });
}

#[test]
fn each_client_sees_its_own_and_anothers_last_rung_callout() {
    let mut sim = NetSim::new(
        SimBasePlugins,
        LinkConditions {
            latency: Duration::from_millis(20),
            jitter: Duration::ZERO,
            loss: 0.0,
        },
        11,
        2,
        setup,
    );
    minigame::start(sim.server.app.world_mut(), "kit:gun_game").expect("it starts");
    sim.until_joined(900);
    sim.ticks(200);
    let a = sim.character_of(0).expect("client 0's player");
    let b = sim.character_of(1).expect("client 1's player");
    for _ in 0..2 {
        kill(&mut sim, a, b);
        sim.ticks(5);
    }
    assert_eq!(sim.server.app.world().get::<Rung>(a).map(|r| r.0), Some(2), "the last rung");
    let seen = |s: &mut NetSim, i: usize, cue: &str| callout(s, i).is_some_and(|c| c.2 == Some(cue));
    assert!(sim.until(120, |s| seen(s, 0, "last_rung") && seen(s, 1, "last_rung_other")), "{:?} / {:?}", callout(&sim, 0), callout(&sim, 1));
    let (own, _, _) = callout(&sim, 0).unwrap();
    assert_eq!(own[0], "Last level!", "{own:?}");
    let (other, who, _) = callout(&sim, 1).unwrap();
    assert_eq!(other[0], "{player} is on the last level!");
    let a_on_1 = {
        let id = sim.client_id(0);
        NetSim::owned_by(&mut sim.clients[1], Some(id))
    };
    assert_eq!(who, a_on_1, "client 1's copy of client 0's player");
    assert!(who.is_some());
}
