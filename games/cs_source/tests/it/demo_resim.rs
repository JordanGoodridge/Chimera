//! Recorded input played again (`games::cs_source::resim`, the `demosim`
//! tool) on a client demo built in code: its user commands written as a
//! client's `dem_usercmd` frames (`source_formats::dem::writer`), read
//! back through the demo reader and CS:S's mapper, with the recording
//! player's own-client states (tick base ahead of the command numbers by
//! a fixed amount, as a server's) and shots from a run of our own, driven
//! here without the harness under test. Played again it must match tick
//! for tick (the offset found, seeds and accuracy equal), a state changed
//! by a unit is found at its command, and the server's settings
//! (`net_SetConVar`) are applied. The probe's real CS:S runs:
//! `heavy::demo_resim`.

use bevy::{ecs::message::Messages, prelude::*};
use source_formats::dem::{
    Header, ServerInfo,
    usercmd::{UserCmd, buttons},
    writer::{DemoWriter, Message},
};

use crate::stack::{
    core::{Seed, SimTick},
    games::cs_source::{
        TICK_INTERVAL,
        demo::{self, CsDemo, FL_DUCKING, FL_ONGROUND, Happening, LocalSnap, PlayerSnap, Thing},
        movement::{self as source, SourceMovement, SourceMovementConfig, SourceMovementPlugin, to_source},
        resim::{self, Options, Recording},
        weapons::CsWeaponsPlugin,
    },
    greybox::{self, GreyboxMapPlugin},
    harness::{CsSim, Sim},
    weapon::{Inventory, Magazine, SpreadShape, WeaponEvent, WeaponEventKind, WeaponState, give},
};

/// Tick base minus command number (a server's count runs ahead of the
/// client's).
const OFFSET: i64 = 5000;
/// The first command's number.
const FIRST: u32 = 1000;
const AK: u16 = 1;

fn make() -> Sim {
    let mut sim = Sim::new(CsSim, (GreyboxMapPlugin, SourceMovementPlugin, CsWeaponsPlugin));
    sim.set_tick_interval(TICK_INTERVAL);
    sim
}

/// Walk, jump, duck sideways, tap and hold fire while turning, reload.
fn commands() -> Vec<UserCmd> {
    (0..140u32)
        .map(|i| {
            let mut c = UserCmd {
                command_number: FIRST + i,
                tick_count: 10_000 + i,
                view_angles: [5.0, 90.0, 0.0],
                ..Default::default()
            };
            let mut press = |b: u32| c.buttons |= b;
            match i {
                10..60 => {
                    press(buttons::FORWARD);
                    if i == 40 {
                        press(buttons::JUMP);
                    }
                }
                60..75 => press(buttons::DUCK | buttons::MOVERIGHT),
                80 | 85 => press(buttons::ATTACK),
                95..110 => press(buttons::ATTACK | buttons::FORWARD),
                120 => press(buttons::RELOAD),
                _ => {}
            }
            if c.buttons & buttons::FORWARD != 0 {
                c.forward_move = 400.0;
            }
            if c.buttons & buttons::MOVERIGHT != 0 {
                c.side_move = 400.0;
            }
            if (95..110).contains(&i) {
                c.view_angles[1] = 90.0 + (i - 95) as f32;
            }
            c
        })
        .collect()
}

/// A shot of ours: the command's place, seed, inaccuracy and spread.
type Shot = (usize, u32, f32, f32);

/// Our own run of `cmds` (with `maxspeed`), driven here: the player's
/// state before the first command, after each, and the shots.
fn reference(cmds: &[UserCmd], maxspeed: f32) -> (Vec<PlayerSnap>, Vec<Shot>) {
    let mut sim = make();
    let mut config = SourceMovementConfig::default();
    config.maxspeed = maxspeed;
    sim.app.insert_resource(config);
    let p = sim.spawn_character(greybox::SPAWNS[0], source::ID);
    sim.ticks(1);
    give(sim.app.world_mut(), p, "weapon_ak47").unwrap();
    sim.seconds(1.5);
    let snap = |sim: &Sim| -> PlayerSnap {
        let w = sim.app.world();
        let me = w.get::<SourceMovement>(p).unwrap();
        let a = w.get::<Inventory>(p).unwrap().active.unwrap();
        let m = w.get::<Magazine>(a).unwrap();
        let origin = (to_source(sim.position(p)) - Vec3::Z * 36.0).to_array();
        PlayerSnap {
            origin,
            pitch: 5.0,
            yaw: 90.0,
            team: 3,
            class: 5,
            health: 100,
            alive: true,
            flags: if me.on_ground { FL_ONGROUND } else { 0 } | if me.ducked { FL_DUCKING } else { 0 },
            weapon: Some(AK),
            local: Some(LocalSnap {
                origin,
                velocity: to_source(sim.velocity(p)).to_array(),
                ducked: me.ducked,
                ducking: false,
                tick_base: 0,
                shots_fired: w.get::<WeaponState>(a).unwrap().burst as i32,
                clip: Some(m.clip as i32),
                reserve: Some(m.reserve as i32),
                weapons: vec![AK],
            }),
            ..Default::default()
        }
    };
    let mut states = vec![snap(&sim)];
    let mut shots = Vec::new();
    let mut cursor = sim.app.world().resource::<Messages<WeaponEvent>>().get_cursor();
    for (i, c) in cmds.iter().enumerate() {
        let next = sim.app.world().resource::<SimTick>().0 + 1;
        sim.app
            .world_mut()
            .entity_mut(p)
            .insert(Seed(c.command_number.wrapping_sub(next as u32) as u64));
        {
            let mut it = sim.intent(p);
            it.move_axis = Vec2::new(c.side_move / 400.0, c.forward_move / 400.0);
            it.yaw = c.view_angles[1].to_radians() - std::f32::consts::FRAC_PI_2;
            it.pitch = -c.view_angles[0].to_radians();
            it.jump = c.buttons & buttons::JUMP != 0;
            it.crouch = c.buttons & buttons::DUCK != 0;
            it.fire = c.buttons & buttons::ATTACK != 0;
            it.reload = c.buttons & buttons::RELOAD != 0;
        }
        sim.ticks(1);
        assert_eq!(sim.intent(p).command, c.command_number, "the command's number stamped");
        for e in cursor.read(sim.app.world().resource::<Messages<WeaponEvent>>()) {
            if let WeaponEventKind::Fired {
                seed,
                spread: SpreadShape::Disc { inaccuracy, spread },
                ..
            } = e.kind
            {
                shots.push((i, seed, inaccuracy, spread));
            }
        }
        states.push(snap(&sim));
    }
    (states, shots)
}

/// The client demo: a signon packet (server info: the recorder in slot 0,
/// so entity 1; the server's `sv_maxspeed`), then per command a packet
/// and its `dem_usercmd` frame; read back, then the recorder's states
/// added to the frames (the state before command `i` in command `i`'s
/// frame, keyed by tick base) with its shots in the frame after them.
fn client_demo(cmds: &[UserCmd], states: &[PlayerSnap], shots: &[Shot], maxspeed: f32) -> CsDemo {
    let header = Header {
        demo_protocol: 3,
        network_protocol: 24,
        server_name: "loopback".into(),
        client_name: "Recorder".into(),
        map: "greybox".into(),
        game_dir: "cstrike".into(),
        playback_time: 0.0,
        ticks: 0,
        frames: 0,
        signon_length: 0,
    };
    let mut w = DemoWriter::new(&header);
    w.packet(
        0,
        true,
        &[
            Message::ServerInfo(ServerInfo {
                protocol: 24,
                player_slot: 0,
                max_clients: 2,
                tick_interval: TICK_INTERVAL as f32,
                os: 'L',
                game_dir: "cstrike".into(),
                map: "greybox".into(),
                host_name: "test".into(),
                ..Default::default()
            }),
            Message::SetConVar(vec![
                ("sv_maxspeed".into(), maxspeed.to_string()),
                ("mp_friendlyfire".into(), "0".into()),
            ]),
        ],
    );
    for (i, c) in cmds.iter().enumerate() {
        let tick = i as i32 + 1;
        w.packet(tick, false, &[Message::Tick(20_000 + tick as u32)]);
        w.usercmd(tick, 7 + i as i32, c);
    }
    let mut demo = demo::read(&w.finish()).expect("the written demo reads");
    assert!(demo.timeline.errors.is_empty(), "{:?}", demo.timeline.errors);
    demo.classes = vec!["CWorld".into(), "CAK47".into()];
    assert_eq!(demo.recorder, Some(1));
    assert_eq!(demo.timeline.frames.len(), cmds.len());
    for (i, f) in demo.timeline.frames.iter_mut().enumerate() {
        assert!(
            matches!(f.events.as_slice(), [Happening::Command { cmd, .. }] if cmd.command_number == FIRST + i as u32),
            "command {i} read back"
        );
        let mut s = states[i].clone();
        let tick_base = (FIRST as i64 + i as i64 - 1) + OFFSET;
        s.local.as_mut().unwrap().tick_base = tick_base;
        f.entities = vec![(1, Thing::Player(s.clone()))];
        for &(_, seed, inaccuracy, spread) in shots.iter().filter(|(at, ..)| *at + 1 == i) {
            f.events.push(Happening::Shot {
                player: 1,
                origin: s.origin,
                pitch: 5.0,
                yaw: 90.0,
                mode: 0,
                seed,
                inaccuracy,
                spread,
            });
        }
    }
    demo
}

fn run(rec: &Recording) -> resim::Report {
    let report = resim::resimulate(&make, rec, Options::default()).expect("re-simulated");
    println!("{report}");
    report
}

#[test]
fn a_client_demo_replays_tick_for_tick() {
    let cmds = commands();
    let (states, shots) = reference(&cmds, 320.0);
    assert!(shots.len() >= 4, "the run fired: {shots:?}");
    let demo = client_demo(&cmds, &states, &shots, 320.0);
    let rec = Recording::from_demo(&demo, None).unwrap();
    assert_eq!(rec.commands.len(), cmds.len());
    assert_eq!(rec.states.len(), cmds.len());
    assert_eq!(rec.commands[40].buttons, buttons::FORWARD | buttons::JUMP);
    let r = run(&rec);
    assert_eq!(r.offset, OFFSET, "the tick base's lead found");
    // The last state is in no frame (its frame would be the next command's).
    assert_eq!(r.compared, cmds.len() - 1);
    assert!(r.first.is_none(), "{:?}", r.first);
    assert!(r.max_position < 0.01, "{}", r.max_position);
    assert_eq!(
        r.shots.recorded,
        shots.len() - usize::from(shots.iter().any(|s| s.0 + 1 == cmds.len()))
    );
    assert_eq!(r.shots.same_tick, r.shots.recorded);
    assert_eq!(r.shots.seeds_equal, r.shots.recorded, "seeds from the command numbers");
    assert!(r.shots.max_inaccuracy_diff < 1e-5, "{}", r.shots.max_inaccuracy_diff);
    assert!(r.cvars.contains(&("sv_maxspeed".into(), "320".into())), "{:?}", r.cvars);
}

#[test]
fn a_difference_shows_at_its_command() {
    let cmds = commands();
    let (states, shots) = reference(&cmds, 320.0);
    let mut demo = client_demo(&cmds, &states, &shots, 320.0);
    // The state after command 50 (in command 51's frame) one unit off.
    let f = &mut demo.timeline.frames[51];
    let Thing::Player(p) = &mut f.entities[0].1 else {
        unreachable!()
    };
    p.local.as_mut().unwrap().origin[0] += 1.0;
    // Command 100's clip one round more.
    let f = &mut demo.timeline.frames[101];
    let Thing::Player(p) = &mut f.entities[0].1 else {
        unreachable!()
    };
    *p.local.as_mut().unwrap().clip.as_mut().unwrap() += 1;
    let r = run(&Recording::from_demo(&demo, None).unwrap());
    let d = r.first.expect("a divergence");
    assert_eq!((d.index, d.key), (50, FIRST as i64 + 50));
    assert!(d.what.starts_with("position 1.0"), "{}", d.what);
    assert_eq!(r.clip, 1, "the clip once");
    assert!((r.max_position - 1.0).abs() < 0.01);
}

#[test]
fn the_servers_settings_are_applied() {
    // Recorded on a server with sv_maxspeed 200 (the AK-47's own is 221).
    let cmds = commands();
    let (states, shots) = reference(&cmds, 200.0);
    let demo = client_demo(&cmds, &states, &shots, 200.0);
    let mut rec = Recording::from_demo(&demo, None).unwrap();
    let r = run(&rec);
    assert!(r.first.is_none(), "{:?}", r.first);
    // Without them, our default runs faster as soon as it walks.
    rec.cvars.clear();
    rec.offset = Some(OFFSET);
    let r = run(&rec);
    let d = r.first.expect("a divergence without the settings");
    assert!((10..20).contains(&d.index), "{}: {}", d.index, d.what);
}
