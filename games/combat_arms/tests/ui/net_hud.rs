//! The HUD on network clients (docs/plans/active/combat-arms.md, "Network
//! part"): a server and two clients on chimera_net's `NetSim`, each
//! client with the HUD's state (`CaHudStatePlugin`). A client deals no
//! damage of its own, so its hit indicator comes from the server's
//! confirmations and its damage feedback from the server's report; the
//! armour shown is the server's.

use std::time::Duration;

use bevy::prelude::*;
use chimera_core::core::{Health, Intent};
use chimera_net::{harness::NetSim, net::memory::LinkConditions};
use chimera_sim::weapon::{Inventory, Magazine};
use chimera_world::greybox::GreyboxMapPlugin;
use combat_arms::{
    game::{TICK_INTERVAL, weapons::CaArmor},
    harness::CaSim,
    ui::hud::{CaHud, CaHudStatePlugin, HudMemory},
};

fn client_intent(sim: &mut NetSim, i: usize) -> Mut<'_, Intent> {
    let me = sim.local_player(i).unwrap();
    sim.clients[i].app.world_mut().get_mut::<Intent>(me).unwrap()
}

/// The server puts client `i` at `at`; its client looks along `yaw`.
fn place(sim: &mut NetSim, i: usize, at: Vec3, yaw: f32) {
    let e = sim.character_of(i).unwrap();
    let world = sim.server.app.world_mut();
    world.get_mut::<Transform>(e).unwrap().translation = at;
    if let Some(mut p) = world.get_mut::<avian3d::prelude::Position>(e) {
        p.0 = at;
    }
    *client_intent(sim, i) = Intent { yaw, ..default() };
}

#[test]
fn hits_damage_and_armour_come_from_the_server() {
    let link = LinkConditions {
        latency: Duration::from_millis(100),
        ..default()
    };
    let mut sim = NetSim::new(CaSim, link, 51, 2, |app| {
        app.add_plugins((GreyboxMapPlugin, CaHudStatePlugin));
    });
    sim.set_frame(TICK_INTERVAL);
    sim.until_joined(600);
    sim.ticks(240);
    place(&mut sim, 0, Vec3::new(0.0, 1.0, 12.0), 0.0);
    place(&mut sim, 1, Vec3::new(0.0, 1.0, 6.0), std::f32::consts::PI);
    sim.ticks(120);
    let me0 = sim.local_player(0).unwrap();
    let rifle = sim.clients[0].app.world().get::<Inventory>(me0).unwrap().active.unwrap();
    let clip = sim.clients[0].app.world().get::<Magazine>(rifle).unwrap().clip;
    client_intent(&mut sim, 0).pitch = -0.08;
    client_intent(&mut sim, 0).fire = true;
    let (mut indicated, mut felt) = (false, false);
    for _ in 0..120 {
        sim.step();
        if sim.clients[0].app.world().get::<Magazine>(rifle).unwrap().clip <= clip - 2 {
            client_intent(&mut sim, 0).fire = false;
        }
        indicated |= sim.clients[0].app.world().resource::<HudMemory>().hit.last.is_some();
        felt |= sim.clients[1].app.world().resource::<HudMemory>().damage.is_some();
    }
    let me1 = sim.local_player(1).unwrap();
    let w1 = sim.clients[1].app.world();
    assert!(w1.get::<Health>(me1).unwrap().current < 1.0, "client 1 was hurt");
    assert!(indicated, "client 0's hit indicator lit from the server's confirmation");
    assert!(felt, "client 1's damage feedback from the server's report");
    let points = w1.get::<CaArmor>(me1).unwrap().points;
    assert!(points < 100.0);
    let shown = w1.resource::<CaHud>().status.clone().expect("client 1's status block");
    assert_eq!(shown.ap, (points.max(0.0) as u32).to_string(), "the server's armour shown");
}
