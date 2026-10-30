//! Projectiles over the network, as a client draws them (backlog, "Gun
//! game playtest, 2026-10-28": the grenade launcher's shell looked
//! choppy on a client). Every kind of Combat Arms object in flight the
//! server replicates as a `NetItem` (rockets, launcher shells, flame
//! puffs, crossbow bolts, thrown devices, thrown grenades) is made on the
//! server of a `NetSim` and its drawn `Transform` on a client sampled
//! every frame (frames shorter than ticks, as a game draws them): no
//! frame's step is longer than the server's speed over that frame (plus a
//! margin), and it never stands still for a frame while it flies.
//! CS:S's grenades are `cs_source`'s `net_weapons` tests.

use std::time::Duration;

use bevy::prelude::*;
use chimera_net::{
    harness::NetSim,
    net::{NetItem, memory::LinkConditions, server::RateLimits},
};
use chimera_core::core::Intent;
use chimera_net::net::own_items::PredictedItem;
use chimera_sim::weapon::{self, Inventory, Magazine, Weapon};
use chimera_world::map::loose::ShownItem;
use chimera_world::greybox::GreyboxMapPlugin;
use combat_arms::{
    game::{
        TICK_INTERVAL,
        grenades::{self, G7A, M18, M23F, M67, XM84},
        special::{self, CaBoltData, CaDeviceData, CaObject, CaProjectileData, Kind},
    },
    harness::CaSim,
};

/// Metres per world unit (the data's).
const U: f32 = combat_arms::game::inventory::UNIT;
/// A frame: 144 a second, about two between ticks.
const FRAME: f64 = 1.0 / 144.0;

fn net_sim() -> NetSim {
    net_sim_of(1)
}

fn net_sim_of(clients: usize) -> NetSim {
    let link = LinkConditions {
        latency: Duration::from_millis(30),
        jitter: Duration::ZERO,
        loss: 0.0,
    };
    let mut sim = NetSim::new(CaSim, link, 7, clients, |app| {
        app.add_plugins(GreyboxMapPlugin);
    });
    sim.set_frame(TICK_INTERVAL);
    sim.until_joined(600);
    sim.ticks(120);
    sim.set_frame(FRAME);
    sim
}

/// What a kind is made as on the server.
#[derive(Clone, Copy, Debug)]
enum Make {
    Impact,
    Bolt,
    Device,
    Grenade,
}

/// Make one object of weapon `id` on the server at `at` (m) going `dir`.
fn make(sim: &mut NetSim, make: Make, id: &str, at: Vec3, dir: Vec3) -> Entity {
    let w = sim.server.app.world_mut();
    let weapon = weapon::spawn_weapon(w, id, Entity::PLACEHOLDER).unwrap();
    let name = w.get::<Weapon>(weapon).unwrap().id;
    let o = match make {
        Make::Impact => {
            let d = w.get::<CaProjectileData>(weapon).unwrap().clone();
            let speed = d.params.speed;
            CaObject::new(name, None, d.unit, d.hp, at / U, dir * speed, Kind::Impact(d.params))
        }
        Make::Bolt => {
            let d = w.get::<CaBoltData>(weapon).unwrap().clone();
            let speed = d.params.speed;
            let mut o = CaObject::new(name, None, d.unit, d.hp, at / U, dir * speed, Kind::Bolt(d.params));
            o.s.origin = at / U;
            o
        }
        Make::Device => {
            let d = w.get::<CaDeviceData>(weapon).unwrap().clone();
            let speed = 600.0;
            let mut o = CaObject::new(name, None, d.unit, d.hp, at / U, dir * speed, Kind::Device(d.params));
            o.s.origin = at / U;
            o
        }
        Make::Grenade => {
            let speed = 600.0;
            let e = grenades::spawn_grenade(w, weapon, None, at, dir * speed * U).unwrap();
            return e;
        }
    };
    special::spawn_object(w, o)
}

/// The server's speed of `e` (m/s): its object's or grenade's velocity.
fn server_speed(sim: &NetSim, e: Entity) -> Option<f32> {
    let w = sim.server.app.world();
    if let Some(o) = w.get::<CaObject>(e) {
        return Some(o.velocity.length() * o.unit);
    }
    w.get::<grenades::CaGrenade>(e).map(|g| g.velocity.length() * g.unit)
}

/// The drawn positions of the one item in flight on client 0, a frame at
/// a time, while the server has it, with the server's speed then (m/s).
fn sample(sim: &mut NetSim, server: Entity, frames: usize) -> Vec<(Vec3, f32)> {
    let mut out = Vec::new();
    for _ in 0..frames {
        sim.step();
        let Some(speed) = server_speed(sim, server) else { break };
        let w = sim.clients[0].app.world_mut();
        let drawn = w.query_filtered::<&Transform, With<NetItem>>().iter(w).next().map(|t| t.translation);
        if let Some(d) = drawn {
            out.push((d, speed));
        }
    }
    out
}

/// Over the middle of the flight (from its first move, which waits for
/// the server's next frame, the first and last fifth left out: arriving
/// and going): the largest step a frame over the most the
/// server's speed allows (1: as fast as it flies), and the frames it
/// stood still.
fn smoothness(samples: &[(Vec3, f32)]) -> (f32, usize) {
    let first = samples.iter().position(|s| s.0 != samples[0].0).unwrap_or(0);
    let samples = &samples[first.saturating_sub(1)..];
    let n = samples.len();
    let (from, to) = ((n / 5).max(1), n - n / 5);
    let fastest = samples.iter().map(|s| s.1).fold(0.0f32, f32::max);
    let mut worst: f32 = 0.0;
    let mut still = 0;
    for k in from..to {
        let step = samples[k].0.distance(samples[k - 1].0);
        worst = worst.max(step / (fastest * FRAME as f32));
        if step < 1e-6 {
            still += 1;
        }
    }
    (worst, still)
}

/// Every kind of Combat Arms object in flight: (how it is made, weapon).
const KINDS: [(Make, &str); 17] = [
    (Make::Impact, "combat_arms:law"),
    (Make::Impact, "combat_arms:rpg-7"),
    (Make::Impact, "combat_arms:m136_at4"),
    (Make::Impact, "combat_arms:m32"),
    (Make::Impact, "combat_arms:mzp_1"),
    (Make::Impact, "combat_arms:xm_25"),
    (Make::Impact, "combat_arms:m32_incendiarybomb"),
    (Make::Impact, "combat_arms:flamethrower"),
    (Make::Bolt, "combat_arms:assassin_bowgun_nor"),
    (Make::Device, "combat_arms:mobile_grenade"),
    (Make::Device, "combat_arms:caltrops"),
    (Make::Device, "combat_arms:m16a1_mine"),
    (Make::Grenade, M67),
    (Make::Grenade, XM84),
    (Make::Grenade, M18),
    (Make::Grenade, M23F),
    (Make::Grenade, G7A),
];

/// Every kind of Combat Arms projectile the server flies is drawn
/// smoothly on a client (frames 1/144 s, ticks 15 ms, server frames 50
/// ms) at 20, 30 and 66 updates a second: no frame's step more than 25 %
/// over the server's speed over a frame, none standing still.
#[test]
fn net_projectiles_drawn_smoothly() {
    let mut report = Vec::new();
    let mut bad = Vec::new();
    for rate in [20.0, 30.0, 66.0] {
        let mut sim = net_sim();
        {
            let mut limits = sim.server.app.world_mut().resource_mut::<RateLimits>();
            limits.min_updaterate = rate;
            limits.max_updaterate = rate;
        }
        sim.ticks(30);
        for (how, id) in KINDS {
            let at = Vec3::new(0.0, 40.0, 0.0);
            let dir = Vec3::new(0.3, 1.0, 0.2).normalize();
            let e = make(&mut sim, how, id, at, dir);
            let samples = sample(&mut sim, e, 100);
            // Gone from the server and the client before the next.
            if let Ok(s) = sim.server.app.world_mut().get_entity_mut(e) {
                s.despawn();
            }
            sim.ticks(60);
            assert!(samples.len() > 40, "{id}: drawn for {} frames", samples.len());
            let (worst, still) = smoothness(&samples);
            let line = format!("{rate} Hz {id}: worst step x{worst:.2} of its speed, still {still} frames");
            if worst > 1.25 || still > 0 {
                bad.push(line.clone());
            }
            report.push(line);
        }
    }
    eprintln!("{}", report.join("\n"));
    assert!(bad.is_empty(), "choppy:\n{}", bad.join("\n"));
}

/// The shooter's own projectile (`net::own_items`): a client firing a LAW,
/// the M32 or a crossbow draws its round from the frame its predicted
/// weapon fires (the clip drops), flying smoothly; the server's copy, when
/// it comes, isn't drawn beside it on that client (one of the model at a
/// time), the other client draws the server's; when it goes off (or the
/// bolt sticks and the server's copy rests there) the prediction is gone.
#[test]
fn net_own_projectile_drawn_from_the_shot() {
    for id in ["combat_arms:law", "combat_arms:m32", "combat_arms:assassin_bowgun_nor"] {
        let mut sim = net_sim_of(2);
        let shooter = sim.character_of(0).unwrap();
        weapon::give(sim.server.app.world_mut(), shooter, id).unwrap();
        sim.ticks(400);
        let me = sim.local_player(0).unwrap();
        let w = sim.clients[0].app.world();
        let held = w.get::<Inventory>(me).unwrap().active.unwrap();
        assert_eq!(w.get::<Weapon>(held).unwrap().id, id, "drawn on the client");
        let clip = |sim: &NetSim| sim.clients[0].app.world().get::<Magazine>(held).unwrap().clip;
        let before = clip(&sim);
        assert!(before > 0);
        {
            let mut i = sim.clients[0].app.world_mut().get_mut::<Intent>(me).unwrap();
            i.fire = true;
            i.pitch = -0.2;
        }
        // Frame by frame: the prediction on the frame the clip drops.
        let mut fired_at = None;
        let mut drawn: Vec<Vec3> = Vec::new();
        let mut copy_seen = false;
        let mut other_saw = false;
        let mut speed: f32 = 0.0;
        for frame in 0..600 {
            sim.step();
            if frame == 2 {
                sim.clients[0].app.world_mut().get_mut::<Intent>(me).unwrap().fire = false;
            }
            if fired_at.is_none() && clip(&sim) < before {
                fired_at = Some(frame);
            }
            let w = sim.clients[0].app.world_mut();
            let predicted: Vec<(Vec3, f32)> = w
                .query::<(&Transform, &PredictedItem)>()
                .iter(w)
                .map(|(t, p)| (t.translation, p.velocity.length()))
                .collect();
            let model_shown = w.query_filtered::<(), (With<NetItem>, With<ShownItem>)>().iter(w).count();
            copy_seen |= w.query::<&NetItem>().iter(w).count() > 0;
            if fired_at == Some(frame) {
                assert_eq!(predicted.len(), 1, "{id}: drawn on the fire frame");
            }
            assert!(predicted.len() + model_shown <= 1, "{id}: one drawn at a time");
            if let Some((at, v)) = predicted.first() {
                drawn.push(*at);
                speed = speed.max(*v);
            }
            let w = sim.clients[1].app.world_mut();
            other_saw |= w.query_filtered::<(), (With<NetItem>, With<ShownItem>)>().iter(w).count() > 0;
            if fired_at.is_some() && frame > fired_at.unwrap() + 20 && predicted.is_empty() {
                break;
            }
        }
        assert!(fired_at.is_some(), "{id}: fired");
        assert!(copy_seen, "{id}: the server's copy came");
        assert!(other_saw, "{id}: the other client drew the server's");
        assert!(drawn.len() > 10, "{id}: predicted for {} frames", drawn.len());
        let worst = drawn.windows(2).map(|p| p[0].distance(p[1])).fold(0.0f32, f32::max);
        assert!(
            worst <= speed * FRAME as f32 * 1.25 + 1e-3,
            "{id}: smooth (worst step {worst} m, {} m a frame at most)",
            speed * FRAME as f32
        );
        let objs = {
            let w = sim.server.app.world_mut();
            w.query::<&CaObject>().iter(w).map(|o| o.at()).collect::<Vec<_>>()
        };
        let w = sim.clients[0].app.world_mut();
        let left: Vec<String> = w.query::<&PredictedItem>().iter(w).map(|p| format!("{p:?}")).collect();
        let copies: Vec<String> = w.query::<(&NetItem, &Transform)>().iter(w).map(|p| format!("{p:?}")).collect();
        assert!(left.is_empty(), "{id}: gone after the blast: {left:?}; copies {copies:?}; server {objs:?}");
    }
}
