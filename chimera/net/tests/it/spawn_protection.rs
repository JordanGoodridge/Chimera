//! Spawn protection over the network (`minigame_net::NetProtected`): the
//! server's windows (`core::SpawnProtected`) reach every client on its
//! own clock, so a client's own shot at a protected player shows no hit
//! (no blood, impact or hit indicator: its trace has no hit) while the
//! server deals no damage, and once the windows are over both show and
//! deal the hit (no "blood but no damage" gap). On the engine's base and
//! the test kit's guns.

use std::time::Duration;

use bevy::prelude::*;
use chimera_core::{
    core::{Health, Intent, LocalPlayer, MovementState, SimClock, SpawnProtected},
    slots::Loadout,
};
use chimera_net::{harness::NetSim, minigame_net::MinigameNetPlugin, net::memory::LinkConditions};
use chimera_sim::{
    SimBasePlugins,
    minigame::{self, MinigameAppExt, MinigamePlugin},
    movement::placeholder,
    testkit,
    weapon::{WeaponEvent, WeaponEventKind},
};
use chimera_world::{greybox::GreyboxMapPlugin, harness::Sim};

const DEF: &str = r#"
id = "kit:gun_game"
name = "Kit gun game"
players = { min = 2, max = 4 }
maps = ["chimera:greybox"]
teams = "free_for_all"
time_limit = "60s"
rotate = false

[spawn]
respawn = { delay = "1s", protection = { damage = "5s", bullets = "4s" } }
points = "map"

[rules]
module = "chimera:ladder"
ladder = ["kit:rifle", "kit:pistol"]
"#;

fn setup(app: &mut App) {
    app.add_plugins((GreyboxMapPlugin, MinigamePlugin, MinigameNetPlugin)).insert_resource(Loadout {
        movement: placeholder::ID,
    });
    testkit::add_weapons(app);
    app.add_minigames(&[("kit_gun_game.toml", DEF)]).expect("the definition reads");
}

/// Seconds left of `p`'s damage window on `sim`'s clock.
fn left(sim: &Sim, p: Entity) -> Option<f64> {
    let w = sim.app.world();
    Some(w.get::<SpawnProtected>(p)?.damage_until - w.resource::<SimClock>().now)
}

/// Client 0 aims at its copy `target` and fires for a few ticks: the
/// traces its own shots made that hit `target`, of how many.
fn shoot(sim: &mut NetSim, target: Entity) -> (usize, usize) {
    let me = sim.local_player(0).unwrap();
    let mut cursor = sim.clients[0].app.world().resource::<Messages<WeaponEvent>>().get_cursor_current();
    let (mut shots, mut hits) = (0, 0);
    for k in 0..12 {
        let c = &mut sim.clients[0];
        let eye = c.position(me) + c.app.world().get::<MovementState>(me).unwrap().eye_offset;
        let d = c.position(target) + Vec3::Y * 0.2 - eye;
        let mut i = c.app.world_mut().get_mut::<Intent>(me).unwrap();
        i.yaw = (-d.x).atan2(-d.z);
        i.pitch = d.y.atan2(d.xz().length());
        i.fire = k < 8;
        sim.step();
        for e in cursor.read(sim.clients[0].app.world().resource::<Messages<WeaponEvent>>()) {
            if e.owner == me && e.shown() && let WeaponEventKind::Shot { hit, .. } = e.kind {
                shots += 1;
                hits += usize::from(hit == Some(target));
            }
        }
    }
    (shots, hits)
}

#[test]
fn clients_keep_the_servers_protection_windows_and_show_no_hit_inside_them() {
    let mut sim = NetSim::new(
        SimBasePlugins,
        LinkConditions {
            latency: Duration::from_millis(20),
            jitter: Duration::ZERO,
            loss: 0.0,
        },
        12,
        2,
        setup,
    );
    minigame::start(sim.server.app.world_mut(), "kit:gun_game").expect("it starts");
    sim.until_joined(900);
    let b = sim.character_of(1).expect("client 1's player");
    let b_on_0 = {
        let id = sim.client_id(1);
        NetSim::owned_by(&mut sim.clients[0], Some(id)).expect("client 0's copy of it")
    };
    assert!(
        sim.until(600, |s| left(&s.clients[0], b_on_0).is_some_and(|l| l > 4.0)),
        "client 0 learns b's protection"
    );
    // The same window on both clocks (to the clocks' jitter), on b's own
    // client too.
    let on_server = left(&sim.server, b).expect("protected on the server");
    let on_client = left(&sim.clients[0], b_on_0).unwrap();
    assert!((on_server - on_client).abs() < 0.25, "server {on_server:.3} s left, client {on_client:.3}");
    let own = sim.local_player(1).unwrap();
    assert!(sim.clients[1].app.world().get::<LocalPlayer>(own).is_some());
    assert!(left(&sim.clients[1], own).is_some_and(|l| (l - on_server).abs() < 0.25), "its own client");
    // Inside it (the gun drawn): client 0's shots show no hit on b, the server deals none.
    // b in front of a (the greybox's spawns have walls between).
    let a = sim.character_of(0).unwrap();
    let front = sim.server.position(a) - Vec3::Z * 5.0;
    sim.server.app.world_mut().get_mut::<Transform>(b).unwrap().translation = front;
    let hz = sim.server.tick_hz();
    sim.ticks((1.2 * hz) as u64);
    assert!(left(&sim.server, b).is_some_and(|l| l > 2.0), "still protected");
    let before = sim.server.app.world().get::<Health>(b).unwrap().current;
    let (shots, hits) = shoot(&mut sim, b_on_0);
    assert!(shots > 0, "client 0 fired");
    assert_eq!(hits, 0, "no hit shown inside the windows");
    assert_eq!(sim.server.app.world().get::<Health>(b).unwrap().current, before, "no damage");
    // Both over (on both sides): the hit shows and lands.
    assert!(sim.until(600, |s| left(&s.server, b).is_none() && left(&s.clients[0], b_on_0).is_none()), "over");
    let (shots, hits) = shoot(&mut sim, b_on_0);
    assert!(shots > 0 && hits > 0, "the hit shows after it ({hits} of {shots})");
    sim.ticks(20);
    assert!(sim.server.app.world().get::<Health>(b).unwrap().current < before, "and lands");
}
