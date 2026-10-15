//! Demo playback (`games::cs_source::demo`) on the greybox from a demo
//! built in code: puppets placed and turned as recorded and eased between
//! frames, a recorded shot traced again (`RemoteShot`), a death, a smoke
//! cloud whose age and fade follow the recorded props, the planted bomb on
//! its recorded clock, the round's clock and scores, seeking both ways,
//! and nothing simulated meanwhile (`core::Playback`). The sample demos
//! from a real server play on de_dust2 in `heavy::demo_playback`.

use std::{collections::HashMap, sync::Arc};

use bevy::{ecs::message::Messages, prelude::*};
use source_formats::dem::{
    Header,
    playback::{Frame, Timeline},
};

use crate::stack::{
    core::{Dead, Died, Intent, LocalPlayer, Playback, Spectating, Team, authoritative},
    games::cs_source::{
        TICK_INTERVAL,
        demo::{
            self, BombSnap, CsDemo, DemoPlayer, DemoPuppet, Happening, PlayerSnap, RulesSnap, SmokeSnap, TeamSnap,
            Thing,
        },
        weapons::CsWeaponsPlugin,
    },
    greybox::GreyboxMapPlugin,
    harness::{CsSim, Sim},
    movement::placeholder,
    objectives::bomb::{BombState, PlantedBomb},
    rules::rounds::{Phase, RoundState},
    weapon::{Inventory, Weapon, WeaponEvent, WeaponEventKind, grenade::SmokeCloud},
};

/// The server's tick at demo tick 0.
const SERVER_BASE: u32 = 10_000;
const AK: u16 = 1;

fn server_time(tick: i32) -> f32 {
    (SERVER_BASE as i32 + tick) as f32 * TICK_INTERVAL as f32
}

fn player(origin: [f32; 3], yaw: f32, alive: bool) -> Thing {
    Thing::Player(PlayerSnap {
        origin,
        pitch: 10.0,
        yaw,
        team: 3,
        class: 5,
        health: if alive { 100 } else { 0 },
        armor: 100,
        helmet: true,
        flags: 1,
        alive,
        weapon: Some(AK),
        money: 800,
        ..Default::default()
    })
}

/// A demo of one counter-terrorist walking along +X, shooting at tick 40,
/// a smoke cloud from tick 80 (fading from 15 s to 17 s after it began),
/// the bomb planted at tick 120 (a 45 s timer) and the player dying at
/// tick 200; a frame every 4 ticks to 400, the round live from tick 0.
fn built() -> CsDemo {
    let mut frames = Vec::new();
    for tick in (0..=400).step_by(4) {
        let alive = tick < 200;
        let mut entities = vec![(1, player([tick as f32, 0.0, 0.0], 0.0, alive))];
        if tick >= 80 {
            entities.push((
                40,
                Thing::Smoke(SmokeSnap {
                    origin: [0.0, 300.0, 0.0],
                    spawn_time: server_time(80),
                    fade_start: server_time(80) + 15.0,
                    fade_end: server_time(80) + 17.0,
                }),
            ));
        }
        if tick >= 120 {
            entities.push((
                41,
                Thing::Bomb(BombSnap {
                    origin: [200.0, -100.0, 0.0],
                    angles: [0.0, 90.0, 0.0],
                    ticking: true,
                    blow: server_time(120) + 45.0,
                    timer: 45.0,
                }),
            ));
        }
        entities.push((
            60,
            Thing::Rules(RulesSnap {
                freeze: false,
                round_time: 115,
                round_start: server_time(0),
                over: None,
                rounds: 1,
            }),
        ));
        entities.push((61, Thing::Team(TeamSnap { team: 2, score: 3 })));
        entities.push((62, Thing::Team(TeamSnap { team: 3, score: 5 })));
        let mut events = Vec::new();
        if tick == 40 {
            events.push(Happening::Shot {
                player: 1,
                origin: [40.0, 0.0, 64.0],
                pitch: 10.0,
                yaw: 0.0,
                mode: 0,
                seed: 7,
                inaccuracy: 0.01,
                spread: 0.0006,
            });
        }
        if tick == 200 {
            events.push(Happening::Death {
                victim: 1,
                attacker: None,
                weapon: "world".into(),
                headshot: false,
            });
        }
        frames.push(Frame {
            tick,
            server_tick: SERVER_BASE + tick as u32,
            entities,
            events,
        });
    }
    CsDemo {
        timeline: Timeline {
            header: Header {
                demo_protocol: 3,
                network_protocol: 24,
                server_name: "test".into(),
                client_name: "SourceTV Demo".into(),
                map: "greybox".into(),
                game_dir: "cstrike".into(),
                playback_time: 6.0,
                ticks: 400,
                frames: frames.len() as i32,
                signon_length: 0,
            },
            tick_interval: TICK_INTERVAL as f32,
            map: "greybox".into(),
            frames,
            errors: Vec::new(),
        },
        classes: vec!["CWorld".into(), "CAK47".into()],
        models: Vec::new(),
        names: HashMap::from([(1, "Mark".to_string())]),
        sounds: Vec::new(),
        recorder: None,
        cvars: Vec::new(),
    }
}

fn sim() -> Sim {
    let mut sim = Sim::new(CsSim, (GreyboxMapPlugin, CsWeaponsPlugin, demo::DemoPlugin));
    sim.set_tick_interval(TICK_INTERVAL);
    sim
}

fn puppet(sim: &mut Sim) -> Option<(Entity, Transform)> {
    let w = sim.app.world_mut();
    w.query::<(Entity, &DemoPuppet, &Transform)>()
        .iter(w)
        .find(|(_, p, _)| p.0 == 1)
        .map(|(e, _, t)| (e, *t))
}

fn seek(sim: &mut Sim, tick: f64) {
    demo::seek(sim.app.world_mut(), tick);
    sim.app.update();
}

#[test]
fn a_built_demo_plays_puppets_shots_smoke_bomb_and_round() {
    let mut sim = sim();
    // The watcher (the shell spawns one; headless, the test does).
    let me = sim.spawn_character(Vec3::ZERO, placeholder::ID);
    sim.app.world_mut().entity_mut(me).insert(LocalPlayer);
    demo::start(sim.app.world_mut(), Arc::new(built()));
    sim.app.update();
    let w = sim.app.world_mut();
    assert!(w.contains_resource::<Playback>());
    assert!(
        !w.run_system_cached(authoritative).unwrap(),
        "nothing simulates while a demo plays"
    );
    assert!(w.get::<Spectating>(me).is_some(), "the local player watches");

    // Placed as recorded at a frame's tick, eased between two.
    seek(&mut sim, 20.0);
    let (e, t) = puppet(&mut sim).expect("the player's puppet");
    let expect = demo::character_origin([20.0, 0.0, 0.0]);
    assert!(t.translation.distance(expect) < 1e-5, "{} vs {expect}", t.translation);
    let (yaw, pitch) = demo::look(10.0, 0.0);
    let w = sim.app.world();
    let i = w.get::<Intent>(e).unwrap();
    assert!((i.yaw - yaw).abs() < 1e-5 && (i.pitch - pitch).abs() < 1e-5);
    assert_eq!(w.get::<Team>(e), Some(&Team(2)));
    assert_eq!(w.get::<Name>(e).map(Name::as_str), Some("Mark"));
    let held = w
        .get::<Inventory>(e)
        .and_then(|i| i.active)
        .and_then(|a| w.get::<Weapon>(a));
    assert_eq!(held.map(|w| w.id), Some("cs_source:weapon_ak47"));
    seek(&mut sim, 22.0);
    let (_, t) = puppet(&mut sim).unwrap();
    assert!(t.translation.distance(demo::character_origin([22.0, 0.0, 0.0])) < 1e-4);

    // Played on: the shot at tick 40 is traced again from the puppet.
    seek(&mut sim, 30.0);
    let mut cursor = sim.app.world().resource::<Messages<WeaponEvent>>().get_cursor();
    let mut fired = 0;
    for _ in 0..40 {
        sim.app.update();
        let w = sim.app.world();
        fired += cursor
            .read(w.resource::<Messages<WeaponEvent>>())
            .filter(|ev| ev.owner == e && matches!(ev.kind, WeaponEventKind::Shot { .. }))
            .count();
    }
    let tick = sim.app.world().resource::<DemoPlayer>().tick;
    assert!(tick > 40.0, "played on to {tick}");
    assert_eq!(fired, 1, "the recorded shot");

    // The smoke cloud began at tick 80, 15 s fully there, then 2 s fading.
    seek(&mut sim, 100.0);
    let w = sim.app.world_mut();
    let now = w.resource::<Time>().elapsed_secs_f64();
    let cloud = w.query::<&SmokeCloud>().single(w).expect("the smoke cloud").clone();
    let age = (now - cloud.started) as f32;
    assert!((age - 20.0 * TICK_INTERVAL as f32).abs() < 0.02, "age {age}");
    assert_eq!((cloud.smoke.fade_start, cloud.smoke.fade_end), (15.0, 17.0));
    assert!((cloud.fade(16.0) - 0.5).abs() < 1e-5);

    // The bomb planted at tick 120 on its recorded clock.
    seek(&mut sim, 160.0);
    let w = sim.app.world_mut();
    let now = w.resource::<Time>().elapsed_secs_f64();
    let (b, bomb, at) = w
        .query::<(Entity, &PlantedBomb, &Transform)>()
        .single(w)
        .map(|(e, b, t)| (e, b.clone(), t.translation))
        .expect("the planted bomb");
    assert_eq!(w.resource::<BombState>().planted, Some(b));
    let left = bomb.explode_at - now;
    assert!((left - (45.0 - 40.0 * TICK_INTERVAL)).abs() < 0.02, "{left} s left");
    assert!(at.distance(demo::engine([200.0, -100.0, 0.0])) < 1e-5);
    // The round's clock and scores.
    let round = w.resource::<RoundState>().clone();
    assert_eq!(round.wins, [3, 5]);
    let Phase::Live { ends, .. } = round.phase else {
        panic!("{:?}", round.phase)
    };
    assert!((ends - now - (115.0 - 160.0 * TICK_INTERVAL)).abs() < 0.02);

    // Played through the death: dead, the death told (kill feed, ragdoll).
    seek(&mut sim, 190.0);
    let mut cursor = sim.app.world().resource::<Messages<Died>>().get_cursor();
    let mut died = 0;
    for _ in 0..30 {
        sim.app.update();
        died += cursor
            .read(sim.app.world().resource::<Messages<Died>>())
            .filter(|d| d.entity == e)
            .count();
    }
    assert_eq!(died, 1);
    assert!(sim.app.world().get::<Dead>(e).is_some());

    // Back before the smoke and bomb: gone; the player alive again.
    seek(&mut sim, 10.0);
    let w = sim.app.world_mut();
    assert_eq!(w.query::<&SmokeCloud>().iter(w).count(), 0);
    assert_eq!(w.query::<&PlantedBomb>().iter(w).count(), 0);
    assert!(w.get::<Dead>(e).is_none());

    demo::stop(sim.app.world_mut());
    let w = sim.app.world_mut();
    assert!(!w.contains_resource::<Playback>() && !w.contains_resource::<DemoPlayer>());
    assert_eq!(w.query::<&DemoPuppet>().iter(w).count(), 0);
}
