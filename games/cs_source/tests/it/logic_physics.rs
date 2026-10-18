//! Map logic meeting physics bodies in the running simulation, on a small
//! map in Source units. Breakables (specs/source/breakables.md, "Damage"
//! and "Breaking"): a physics prop falling on a glass func_breakable breaks
//! it by impact damage (unless the brush takes no physics damage), and a
//! breakable that explodes when it breaks breaks the one next to it. Model
//! doors (specs/source/doors_buttons.md, prop_door_rotating "Blocked"): a
//! crate pinned against a wall stops a closing door, unless it is
//! forceclosed. Props and players (specs/source/prop_damage.md 6): a
//! prop_physics crate or a 500 kg cupboard landing on a player hurts it as
//! two contacts and is hurt by it; a multiplayer one passes through; a
//! player running into a resting prop hurts neither.

use avian3d::prelude::{LinearVelocity, Position};
use bevy::{ecs::message::Messages, prelude::*};
use crate::stack::{
    core::{Damage, DamageKind},
    games::cs_source::{
        self,
        movement::{self, SourceMovementPlugin, to_engine, to_source},
        weapons::CsWeaponsPlugin,
    },
    harness::{CsSim, Sim},
    logic::{Logic, Value, Who, classes::Class},
    map::{
        MapBrush, MapCollision, MapConvex, MapData, MapEntity, MapHull, MapModel, MapPhysics, MapPlugin, MapProp,
        PropEntity, PropSolid, PushAway,
    },
};

const SCALE: f32 = 0.0254;

fn hull(lo: Vec3, hi: Vec3) -> MapHull {
    let b = MapBrush::from_box(lo, hi);
    let points = (0..8)
        .map(|i| {
            Vec3::new(
                if i & 1 == 0 { lo.x } else { hi.x },
                if i & 2 == 0 { lo.y } else { hi.y },
                if i & 4 == 0 { lo.z } else { hi.z },
            )
        })
        .collect();
    MapHull { planes: b.planes, points }
}

fn entity(pairs: &[(&str, &str)], hulls: Vec<MapHull>, mover: bool) -> MapEntity {
    MapEntity {
        keyvalues: pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        hulls,
        mover,
        physics: None,
    }
}

/// A floor at z = 0 plus `entities`.
fn map(entities: Vec<MapEntity>) -> MapData {
    let mut data = MapData {
        name: "test:logic_physics".into(),
        entities,
        entity_scale: SCALE,
        ..default()
    };
    solid(&mut data, Vec3::new(-4096.0, -4096.0, -64.0), Vec3::new(4096.0, 4096.0, 0.0));
    data
}

/// A solid box of the world (Source units): for players and the logic,
/// and for shots and physics bodies.
fn solid(data: &mut MapData, lo: Vec3, hi: Vec3) {
    let (a, b) = (to_engine(lo), to_engine(hi));
    let (a, b) = (a.min(b), a.max(b));
    data.collision_brushes.push(MapBrush::from_box(a, b));
    data.collision_hulls.push(
        (0..8)
            .map(|k| [[a.x, b.x][k & 1], [a.y, b.y][(k >> 1) & 1], [a.z, b.z][(k >> 2) & 1]])
            .collect(),
    );
}

/// A 20 kg, 32-unit physics crate (map entity `index`) at `at`.
fn add_crate(data: &mut MapData, index: usize, at: Vec3) {
    add_prop(data, index, at, PushAway::Collide, false, 20.0);
}

/// `add_crate` with how players meet it, whether it starts asleep and its
/// mass (kg).
fn add_prop(data: &mut MapData, index: usize, at: Vec3, push: PushAway, asleep: bool, mass: f32) {
    add_box_prop(data, index, at, push, asleep, mass, Vec3::splat(-16.0), Vec3::splat(16.0));
}

/// `add_prop` with its box (`lo`..`hi` around its origin, Source units).
#[allow(clippy::too_many_arguments)]
fn add_box_prop(data: &mut MapData, index: usize, at: Vec3, push: PushAway, asleep: bool, mass: f32, lo: Vec3, hi: Vec3) {
    let (lo, hi) = (to_engine(lo), to_engine(hi));
    let (lo, hi) = (lo.min(hi), lo.max(hi));
    let corners = (0..8)
        .map(|k| Vec3::new([lo.x, hi.x][k & 1], [lo.y, hi.y][(k >> 1) & 1], [lo.z, hi.z][(k >> 2) & 1]))
        .collect();
    data.models.push(MapModel {
        bounds: (lo, hi),
        collision: Some(MapCollision {
            pieces: vec![MapConvex { points: corners, planes: Vec::new() }],
            mass,
            ..default()
        }),
        ..default()
    });
    data.props.push(MapProp {
        pose: None,
        ragdoll: None,
        model: data.models.len() - 1,
        translation: to_engine(at),
        rotation: Quat::IDENTITY,
        solid: PropSolid::Mesh,
        skybox: false,
        lighting: None,
        vertex_light: None,
        casts_shadow: false,
        physics: Some(MapPhysics {
            mass,
            friction: 0.8,
            elasticity: 0.0,
            damping: 0.0,
            rotdamping: 0.0,
            push,
            frozen: false,
            asleep,
        }),
        fade: None,
        parent: None,
        entity: Some(index),
        skin: 0,
        body: 0,
        detail: false,
    });
}

fn sim(data: MapData) -> Sim {
    let mut sim = Sim::new(CsSim, (MapPlugin::new(data), SourceMovementPlugin, CsWeaponsPlugin));
    sim.set_tick_interval(cs_source::TICK_INTERVAL);
    sim
}

/// Whether map entity `index` is a broken breakable (or gone).
fn broken(sim: &Sim, index: usize) -> bool {
    let logic = sim.app.world().resource::<Logic>();
    let id = logic
        .world
        .ids()
        .into_iter()
        .find(|id| logic.world.get(*id).unwrap().map_index == Some(index));
    match id.map(|id| &logic.world.get(id).unwrap().class) {
        None => true,
        Some(Class::Breakable(b)) => b.broken,
        Some(_) => panic!("not a breakable"),
    }
}

/// Where the crate is (Source units).
fn crate_at(sim: &mut Sim) -> Vec3 {
    let world = sim.app.world_mut();
    let node = world.query::<(Entity, &PropEntity)>().iter(world).next().unwrap().0;
    to_source(world.get::<Position>(node).unwrap().0)
}

/// A glass pane lying flat 64 units up, a crate dropped from 160: the
/// fall breaks it (the glass table at 2 kg), and the crate lands on the
/// floor. With "Don't take physics damage" (1024) it holds the crate.
#[test]
fn a_falling_crate_breaks_a_glass_pane() {
    for (flags, breaks) in [("0", true), ("1024", false)] {
        let pane = entity(
            &[
                ("classname", "func_breakable"),
                ("origin", "0 0 64"),
                ("material", "0"),
                ("health", "1"),
                ("spawnflags", flags),
            ],
            vec![hull(Vec3::new(-48.0, -48.0, -1.0), Vec3::new(48.0, 48.0, 1.0))],
            true,
        );
        let box_ = entity(&[("classname", "prop_physics"), ("origin", "0 0 160")], Vec::new(), false);
        let mut data = map(vec![pane, box_]);
        add_crate(&mut data, 1, Vec3::new(0.0, 0.0, 160.0));
        let mut sim = sim(data);
        sim.seconds(2.0);
        assert_eq!(broken(&sim, 0), breaks, "flags {flags}");
        let z = crate_at(&mut sim).z;
        if breaks {
            assert!(z < 20.0, "fell through to the floor: {z}");
        } else {
            assert!((z - 81.0).abs() < 2.0, "rests on the pane: {z}");
        }
    }
}

/// "Break immediately on physics" (512): a slow touch breaks even a
/// sturdy wooden board.
#[test]
fn break_on_physics_breaks_at_the_first_impact() {
    let board = entity(
        &[
            ("classname", "func_breakable"),
            ("origin", "0 0 40"),
            ("material", "1"),
            ("health", "500"),
            ("spawnflags", "512"),
        ],
        vec![hull(Vec3::new(-48.0, -48.0, -2.0), Vec3::new(48.0, 48.0, 2.0))],
        true,
    );
    let box_ = entity(&[("classname", "prop_physics"), ("origin", "0 0 60")], Vec::new(), false);
    let mut data = map(vec![board, box_]);
    add_crate(&mut data, 1, Vec3::new(0.0, 0.0, 60.0));
    let mut sim = sim(data);
    sim.seconds(1.5);
    assert!(broken(&sim, 0));
}

/// A breakable with explodemagnitude explodes as it breaks (the HE
/// grenade's blast) and breaks the wooden one beside it.
#[test]
fn an_exploding_breakable_breaks_its_neighbour() {
    let barrel = entity(
        &[
            ("classname", "func_breakable"),
            ("targetname", "barrel"),
            ("origin", "0 0 32"),
            ("material", "2"),
            ("health", "10"),
            ("explodemagnitude", "100"),
        ],
        vec![hull(Vec3::splat(-16.0), Vec3::splat(16.0))],
        true,
    );
    let crate_ = entity(
        &[
            ("classname", "func_breakable"),
            ("origin", "80 0 32"),
            ("material", "1"),
            ("health", "40"),
        ],
        vec![hull(Vec3::splat(-16.0), Vec3::splat(16.0))],
        true,
    );
    let far = entity(
        &[
            ("classname", "func_breakable"),
            ("origin", "1000 0 32"),
            ("material", "1"),
            ("health", "40"),
        ],
        vec![hull(Vec3::splat(-16.0), Vec3::splat(16.0))],
        true,
    );
    let mut sim = sim(map(vec![barrel, crate_, far]));
    sim.ticks(5);
    sim.app
        .world_mut()
        .resource_mut::<Logic>()
        .world
        .queue_input("barrel", "Break", Value::Void, 0.0, None);
    sim.ticks(10);
    assert!(broken(&sim, 0));
    assert!(broken(&sim, 1), "the blast broke its neighbour");
    assert!(!broken(&sim, 2), "out of reach");
}

/// Map explosions fall off as CS:S's radius damage: 100 exp(-d^2 / 2σ^2)
/// with σ = R/3, reaching past the radius (measured 2026-10-17 on the probe
/// server: an env_explosion of magnitude 100 and radius 100 does 23.811 at
/// 56.5 units, 8.459 at 74.11, 1.998 at 93.27 and 0.555 at 107.44 from a
/// crate's centre, the bell within 0.01; specs/source/prop_damage.md Q3).
/// A brush is reached where the trace meets its face (16 units nearer than
/// its centre here), and takes blasts x1.25, truncated.
#[test]
fn an_explosion_falls_off_as_a_bell_past_its_radius() {
    let barrel = entity(
        &[
            ("classname", "func_breakable"),
            ("targetname", "barrel"),
            ("origin", "0 0 31"),
            ("material", "2"),
            ("health", "10"),
            ("ExplodeDamage", "100"),
            ("ExplodeRadius", "100"),
        ],
        vec![hull(Vec3::splat(-8.0), Vec3::splat(8.0))],
        true,
    );
    // One each way, so none shields another (the blast's src is 1 unit
    // above the barrel's centre: level with the targets' centres).
    let at = [(56.5, 0.0), (0.0, 74.11), (-93.27, 0.0), (0.0, -107.44)];
    let mut entities = vec![barrel];
    for (x, y) in at {
        entities.push(entity(
            &[
                ("classname", "func_breakable"),
                ("origin", &format!("{x} {y} 32")),
                ("material", "1"),
                ("health", "1000"),
            ],
            vec![hull(Vec3::splat(-16.0), Vec3::splat(16.0))],
            true,
        ));
    }
    let mut sim = sim(map(entities));
    sim.ticks(5);
    sim.app
        .world_mut()
        .resource_mut::<Logic>()
        .world
        .queue_input("barrel", "Break", Value::Void, 0.0, None);
    sim.ticks(10);
    let logic = sim.app.world().resource::<Logic>();
    let health = |index: usize| {
        let id = logic
            .world
            .ids()
            .into_iter()
            .find(|id| logic.world.get(*id).unwrap().map_index == Some(index))
            .unwrap();
        match &logic.world.get(id).unwrap().class {
            Class::Breakable(b) => b.health,
            _ => panic!(),
        }
    };
    // The bell at 40.5, 58.11, 77.27 and 91.44 units: 47.8, 21.9, 6.8 and
    // 2.3, x1.25 (linear would lose 74, 52, 28 and 10).
    let lost: Vec<i32> = (1..=4).map(|i| 1000 - health(i)).collect();
    assert_eq!(lost, vec![59, 27, 8, 2]);
}

/// A player walking into a "solid" multiplayer prop asleep since it
/// spawned wakes it, firing OnAwakened, and takes no damage from it (CS:S,
/// measured 2026-10-17: cs_office's trash can, walked into at 250 units/s;
/// specs/source/prop_damage.md Q14).
#[test]
fn walking_into_a_sleeping_prop_wakes_it() {
    let can = entity(
        &[
            ("classname", "prop_physics_multiplayer"),
            ("origin", "100 0 17"),
            ("spawnflags", "257"),
            ("OnAwakened", "c,Add,1,0,-1"),
        ],
        Vec::new(),
        false,
    );
    let counter = entity(&[("classname", "math_counter"), ("targetname", "c")], Vec::new(), false);
    let mut data = map(vec![can, counter]);
    add_prop(&mut data, 0, Vec3::new(100.0, 0.0, 17.0), PushAway::Solid, true, 20.0);
    let mut sim = sim(data);
    let p = sim.spawn_character(to_engine(Vec3::new(0.0, 0.0, 37.0)), movement::ID);
    // Source yaw 0: toward +X.
    sim.intent(p).yaw = -std::f32::consts::FRAC_PI_2;
    sim.ticks(10);
    let count = |sim: &Sim| {
        let logic = sim.app.world().resource::<Logic>();
        let id = logic.world.find("c").unwrap();
        match &logic.world.get(id).unwrap().class {
            Class::Counter(c) => Some(c.value),
            _ => None,
        }
    };
    assert_eq!(count(&sim), Some(0.0), "asleep");
    sim.intent(p).move_axis = Vec2::Y;
    sim.seconds(1.0);
    assert_eq!(count(&sim), Some(1.0), "woken once");
    assert!(crate_at(&mut sim).x > 100.5, "shoved away: {}", crate_at(&mut sim));
}

/// A player running (250 units/s) or walking (130) into a prop_physics
/// resting on the floor, a 30 kg crate or the 500 kg cupboard, hurts
/// neither (CS:S, measured 2026-10-17 on the probe server,
/// `tools/css_probe/impacts.py walk`, prop_damage.md 6: no damage either
/// way; the player stops at it and shoves it on in bursts).
#[test]
fn running_into_a_prop_hurts_nobody() {
    use crate::stack::map::entities::{PROP_HEALTH_KEY, PROP_MASS_KEY, PROP_PIECES_KEY};
    for (prop, speed) in [(&CRATE_30, 250.0), (&CRATE_30, 130.0), (&CUPBOARD_500, 250.0)] {
        let z = 0.05 - prop.lo.z;
        let (origin, health, mass) = (format!("100 0 {z}"), prop.health.to_string(), prop.mass.to_string());
        let e = entity(
            &[
                ("classname", "prop_physics"),
                ("origin", &origin),
                (PROP_HEALTH_KEY, &health),
                (PROP_PIECES_KEY, "1"),
                (PROP_MASS_KEY, &mass),
            ],
            Vec::new(),
            false,
        );
        let mut data = map(vec![e]);
        let at = Vec3::new(100.0 - prop.hi.x, 0.0, z);
        add_box_prop(&mut data, 0, at, PushAway::Collide, false, prop.mass, prop.lo, prop.hi);
        data.gravity = Some(800.0 * SCALE);
        let mut sim = sim(data);
        let p = sim.spawn_character(to_engine(Vec3::new(-60.0, 0.0, 37.0)), movement::ID);
        sim.intent(p).yaw = -std::f32::consts::FRAC_PI_2;
        sim.app.world_mut().entity_mut(p).insert(crate::stack::core::MaxSpeed(speed * SCALE));
        sim.ticks(30);
        let mut cursor = sim.app.world().resource::<Messages<Damage>>().get_cursor_current();
        sim.intent(p).move_axis = Vec2::Y;
        let mut hurt = Vec::new();
        for _ in 0..120 {
            sim.ticks(1);
            let world = sim.app.world();
            hurt.extend(cursor.read(world.resource::<Messages<Damage>>()).map(|d| (d.target, d.amount)));
        }
        assert_eq!(hurt, Vec::new(), "{} kg at {speed} units/s", prop.mass);
        // The crate is shoved on; the cupboard stays (CS:S: it moved 2.5
        // units, tipping).
        if prop.mass < 100.0 {
            assert!(crate_at(&mut sim).x > at.x + 10.0, "{} kg: shoved on", prop.mass);
        }
    }
}

/// A 30 kg crate (`class`, health 100) thrown straight down at `speed`
/// from 110 units above a standing player's feet, as the probe did: the
/// crush damage each hit dealt the player (with its attacker), the player,
/// the crate's node, the crate's health and last attacker after, and the
/// player's fastest horizontal speed meanwhile (units/s).
type Drop = (Vec<(i32, Option<Entity>)>, Entity, Entity, i32, Option<Who>, f32);

fn drop_on_player(class: &str, push: PushAway, speed: f32) -> Drop {
    drop_prop_on_player(class, push, speed, &CRATE_30)
}

/// A physics prop dropped on a player: its mass (kg), health, box
/// (Source units around its origin) and how high its origin starts above
/// the player's feet.
struct Dropped {
    mass: f32,
    health: i32,
    lo: Vec3,
    hi: Vec3,
    height: f32,
}

/// The probe's 30 kg wooden crate (a 32-unit box here), from 110 units.
const CRATE_30: Dropped = Dropped {
    mass: 30.0,
    health: 100,
    lo: Vec3::splat(-16.0),
    hi: Vec3::splat(16.0),
    height: 110.0,
};

/// The probe's 500 kg cupboard (`furniturecupboard001a`, health 50, its
/// collision box), from 120 units.
const CUPBOARD_500: Dropped = Dropped {
    mass: 500.0,
    health: 50,
    lo: Vec3::new(-11.02, -28.97, -5.72),
    hi: Vec3::new(6.97, 7.0, 25.23),
    height: 120.0,
};

fn drop_prop_on_player(class: &str, push: PushAway, speed: f32, prop: &Dropped) -> Drop {
    use crate::stack::map::entities::{PROP_HEALTH_KEY, PROP_MASS_KEY, PROP_PIECES_KEY};
    let at = Vec3::new(0.0, 0.0, prop.height);
    let (origin, health, mass) = (
        format!("0 0 {}", prop.height),
        prop.health.to_string(),
        prop.mass.to_string(),
    );
    let box_ = entity(
        &[
            ("classname", class),
            ("origin", &origin),
            (PROP_HEALTH_KEY, &health),
            (PROP_PIECES_KEY, "1"),
            (PROP_MASS_KEY, &mass),
        ],
        Vec::new(),
        false,
    );
    let mut data = map(vec![box_]);
    add_box_prop(&mut data, 0, at, push, false, prop.mass, prop.lo, prop.hi);
    // CS:S's gravity for physics, as its maps get it (800 units/s²): the
    // probe's props fell under it to the player.
    data.gravity = Some(800.0 * SCALE);
    let mut sim = sim(data);
    let p = sim.spawn_character(to_engine(Vec3::new(0.0, 0.0, 37.0)), movement::ID);
    // Health enough to take every contact (the probe's bot had 5000; the
    // squeeze tests crush it for longer).
    sim.app.world_mut().get_mut::<crate::stack::core::Health>(p).unwrap().current = 1000.0;
    sim.ticks(1);
    let node = {
        let world = sim.app.world_mut();
        world.query::<(Entity, &PropEntity)>().iter(world).next().unwrap().0
    };
    {
        let world = sim.app.world_mut();
        let mut e = world.entity_mut(node);
        e.get_mut::<Position>().unwrap().0 = to_engine(at);
        e.get_mut::<LinearVelocity>().unwrap().0 = to_engine(Vec3::new(0.0, 0.0, -speed));
    }
    let mut cursor = sim.app.world().resource::<Messages<Damage>>().get_cursor_current();
    let mut hits = Vec::new();
    let mut pushed: f32 = 0.0;
    for _ in 0..(1.5 / cs_source::TICK_INTERVAL) as usize {
        sim.ticks(1);
        pushed = pushed.max(sim.velocity(p).xz().length() / SCALE);
        let world = sim.app.world();
        for d in cursor.read(world.resource::<Messages<Damage>>()) {
            if d.target == p {
                assert_eq!(d.kind, DamageKind::Crush);
                hits.push(((d.amount * 100.0).round() as i32, d.attacker));
            }
        }
    }
    let logic = sim.app.world().resource::<Logic>();
    let id = logic
        .world
        .ids()
        .into_iter()
        .find(|id| logic.world.get(*id).unwrap().map_index == Some(0))
        .unwrap();
    let Class::Prop(prop) = &logic.world.get(id).unwrap().class else {
        panic!("not a prop")
    };
    (hits, p, node, prop.health, prop.last_attacker, pushed)
}

/// The 500 kg cupboard thrown down from 120 units at a few speeds onto a
/// player with 5000 health (CS:S, measured 2026-10-17 on the probe
/// server, `tools/css_probe/impacts.py drop`, prop_damage.md 6): two
/// contacts each landing, 5 + 50 at 0 in/s up to 500 + 500 at 1500; the
/// cupboard loses 5 to the player from 800 in/s, 10 at 1500; the player
/// isn't knocked away (13 units/s at most).
#[test]
fn a_cupboard_landing_on_a_player() {
    for (speed, want, lost) in [
        (0.0, [5, 50], 0),
        (100.0, [10, 50], 0),
        (200.0, [10, 100], 0),
        (300.0, [10, 100], 0),
        (400.0, [10, 500], 0),
        (600.0, [50, 500], 0),
        (800.0, [100, 500], 5),
        (1000.0, [100, 500], 5),
        (1500.0, [500, 500], 10),
    ] {
        let (hits, _, _, health, _, pushed) =
            drop_prop_on_player("prop_physics", PushAway::Collide, speed, &CUPBOARD_500);
        let amounts: Vec<i32> = hits.iter().map(|h| h.0).collect();
        eprintln!("cupboard {speed}: player hits {amounts:?}, cupboard health {health}, player pushed {pushed}");
        // The landing only: sliding off the head afterwards hurts nobody
        // (CS:S, measured at 0, 300 and 1000 in/s: the cupboard rose 2 to
        // 10 units, slid off and fell past the player with no more
        // damage), nor does the landing crush (no stress damage at any of
        // 13 speeds up to 2000 but 1450 to 1500, prop_damage.md 6).
        assert_eq!(amounts, want, "{speed} in/s");
        assert_eq!(CUPBOARD_500.health - health, lost, "the cupboard at {speed} in/s");
        assert!(pushed < 20.0, "the player knocked to {pushed} units/s");
    }
}

/// The cupboard made 4000 kg, set down on a standing player's head: its
/// weight past the stress limit (4000 × 0.125 / 85 = 5.9 > 5) crushes
/// the player by 200 from the world nearly every tick it rests there;
/// made 3000 kg (4.4), it doesn't once it rests (CS:S, measured
/// 2026-10-17 with `massScale` 8 and 6, prop_damage.md 6 "Stress": 200
/// on 26 of the first 30 ticks, force ≈ the weight; at 3000 kg only on
/// two bounces).
#[test]
fn a_heavy_prop_resting_on_a_player_crushes_it() {
    for (mass, crushed) in [(4000.0, true), (3000.0, false)] {
        let prop = Dropped {
            mass,
            // Centred over the head (the probe's was moved 11 units so).
            lo: Vec3::new(-11.02, -17.97, -5.72),
            hi: Vec3::new(6.97, 18.0, 25.23),
            height: 80.0,
            ..CUPBOARD_500
        };
        let (hits, ..) = drop_prop_on_player("prop_physics", PushAway::Collide, 0.0, &prop);
        let stress: Vec<&(i32, Option<Entity>)> = hits.iter().filter(|h| h.1.is_none()).collect();
        eprintln!("{mass} kg: {} stress hits of {} hits", stress.len(), hits.len());
        assert!(stress.iter().all(|h| h.0 == 200), "{mass} kg: {stress:?}");
        if crushed {
            // Ours rolls off the head after about 40 ticks.
            assert!(stress.len() >= 26, "{mass} kg crushed on {} ticks", stress.len());
        } else {
            assert!(stress.len() <= 3, "{mass} kg crushed on {} ticks", stress.len());
        }
    }
}

/// A prop_physics crate landing on a player hurts it as two contacts
/// (crush, attacker = the crate), and the player's contact hurts the
/// crate (attacker = the player): CS:S, measured 2026-10-17 with a 30 kg
/// crate at 544, 827 and 1219 in/s: 5 + 5, 5 + 10 and 10 + 20, and the
/// crate lost 5 at 1219 (specs/source/prop_damage.md 6, Q6 and Q13).
#[test]
fn a_crate_landing_on_a_player_hurts_both() {
    for (speed, want, crate_lost) in [(544.0, [5, 5], 0), (827.0, [5, 10], 0), (1219.0, [10, 20], 5)] {
        let (hits, p, node, health, last, _) = drop_on_player("prop_physics", PushAway::Collide, speed);
        eprintln!("{speed}: player hits {hits:?}, crate health {health}, last attacker {last:?}");
        let amounts: Vec<i32> = hits.iter().map(|h| h.0).collect();
        assert_eq!(amounts, want.to_vec(), "{speed} in/s");
        assert!(hits.iter().all(|h| h.1 == Some(node)), "the crate is the attacker");
        assert_eq!(100 - health, crate_lost, "the crate at {speed} in/s");
        if crate_lost > 0 {
            assert_eq!(last, Some(Who::Player(p)), "the player hurt the crate");
        }
    }
}

/// The same crate as a prop_physics_multiplayer passes through the player
/// and hurts nobody (CS:S, measured: multiplayer props never hit players).
#[test]
fn a_multiplayer_crate_never_hurts_a_player() {
    for speed in [544.0, 1219.0] {
        let (hits, ..) = drop_on_player("prop_physics_multiplayer", PushAway::Solid, speed);
        assert_eq!(hits, Vec::new(), "{speed} in/s");
    }
}

/// A model door open back (its leaf along -X) closing on a crate that
/// has a wall behind it: the door stops at the crate, while a forceclosed
/// one shoves it through and closes.
#[test]
fn a_pinned_crate_blocks_a_closing_model_door_unless_forceclosed() {
    use crate::stack::logic::movers::DoorState;
    for (forceclosed, closes) in [("0", false), ("1", true)] {
        let door = entity(
            &[
                ("classname", "prop_door_rotating"),
                ("targetname", "door"),
                ("origin", "0 0 1"),
                ("spawnpos", "2"),
                ("returndelay", "-1"),
                ("speed", "100"),
                ("distance", "90"),
                ("forceclosed", forceclosed),
            ],
            vec![hull(Vec3::new(-1.0, 0.0, 0.0), Vec3::new(1.0, 52.0, 100.0))],
            true,
        );
        let box_ = entity(&[("classname", "prop_physics"), ("origin", "-30 17.5 17")], Vec::new(), false);
        let mut data = map(vec![door, box_]);
        add_crate(&mut data, 1, Vec3::new(-30.0, 17.5, 17.0));
        // The wall above the crate, clear of the leaf's swing ends.
        solid(&mut data, Vec3::new(-60.0, 34.0, 0.0), Vec3::new(-5.0, 60.0, 100.0));
        let mut sim = sim(data);
        sim.ticks(10);
        sim.app
            .world_mut()
            .resource_mut::<Logic>()
            .world
            .queue_input("door", "Close", Value::Void, 0.0, None);
        sim.seconds(2.0);
        let logic = sim.app.world().resource::<Logic>();
        let id = logic
            .world
            .ids()
            .into_iter()
            .find(|id| logic.world.get(*id).unwrap().map_index == Some(0))
            .unwrap();
        let d = logic.world.prop_door(id).unwrap();
        eprintln!("forceclosed {forceclosed}: {:?} at {}", d.state, d.push.angles.y);
        if closes {
            assert_eq!((d.state, d.push.angles.y), (DoorState::Closed, 0.0));
        } else {
            assert_eq!(d.state, DoorState::Closing);
            assert!(d.push.angles.y > 10.0, "stopped at the crate: {}", d.push.angles.y);
        }
    }
}

/// Water (a 128-unit-deep pool): a light crate dropped in it comes up and
/// floats at the surface (map::buoyancy); out of the water the same crate
/// rests on the floor.
#[test]
fn a_crate_floats_in_water() {
    for (water, floats) in [(true, true), (false, false)] {
        let box_ = entity(
            &[("classname", "prop_physics"), ("origin", "0 0 40")],
            Vec::new(),
            false,
        );
        let mut data = map(vec![box_]);
        add_crate(&mut data, 0, Vec3::new(0.0, 0.0, 40.0));
        if water {
            let (a, b) = (
                to_engine(Vec3::new(-512.0, -512.0, 0.0)),
                to_engine(Vec3::new(512.0, 512.0, 128.0)),
            );
            data.water.push(crate::stack::core::MapWaterVolume {
                brush: MapBrush::from_box(a.min(b), a.max(b)),
                slime: false,
            });
        }
        let mut sim = sim(data);
        sim.seconds(5.0);
        let z = crate_at(&mut sim).z;
        if floats {
            // 20 kg in 32768 cubic inches: mostly above the surface.
            assert!(z > 110.0 && z < 150.0, "floats at the surface: {z}");
        } else {
            assert!(z < 20.0, "rests on the floor: {z}");
        }
    }
}

/// phys_ballsocket holding a crate to the world 32 units above it: it
/// hangs there instead of falling (physics_constraints.md); Break lets go.
#[test]
fn a_ball_socket_holds_a_crate_until_it_breaks() {
    let box_ = entity(
        &[
            ("classname", "prop_physics"),
            ("targetname", "lamp"),
            ("origin", "0 0 200"),
        ],
        Vec::new(),
        false,
    );
    let socket = entity(
        &[
            ("classname", "phys_ballsocket"),
            ("targetname", "socket"),
            ("origin", "0 0 232"),
            ("attach1", "lamp"),
        ],
        Vec::new(),
        false,
    );
    let mut data = map(vec![box_, socket]);
    add_crate(&mut data, 0, Vec3::new(0.0, 0.0, 200.0));
    let mut sim = sim(data);
    sim.seconds(2.0);
    let z = crate_at(&mut sim).z;
    assert!((z - 200.0).abs() < 8.0, "hangs from the socket: {z}");
    sim.app
        .world_mut()
        .resource_mut::<Logic>()
        .world
        .queue_input("socket", "Break", Value::Void, 0.0, None);
    sim.seconds(2.0);
    let z = crate_at(&mut sim).z;
    assert!(z < 20.0, "fell once broken: {z}");
}

/// A constraint naming an info_constraint_anchor holds the anchor's
/// parent (physics_constraints.md 1.2 step 1).
#[test]
fn a_constraint_through_an_anchor_holds_its_parent() {
    let box_ = entity(
        &[
            ("classname", "prop_physics"),
            ("targetname", "lamp"),
            ("origin", "0 0 200"),
        ],
        Vec::new(),
        false,
    );
    let anchor = entity(
        &[
            ("classname", "info_constraint_anchor"),
            ("targetname", "hook"),
            ("parentname", "lamp"),
            ("origin", "0 0 216"),
        ],
        Vec::new(),
        false,
    );
    let socket = entity(
        &[
            ("classname", "phys_ballsocket"),
            ("origin", "0 0 232"),
            ("attach1", "hook"),
        ],
        Vec::new(),
        false,
    );
    let mut data = map(vec![box_, anchor, socket]);
    add_crate(&mut data, 0, Vec3::new(0.0, 0.0, 200.0));
    let mut sim = sim(data);
    sim.seconds(2.0);
    let z = crate_at(&mut sim).z;
    assert!((z - 200.0).abs() < 8.0, "hangs through the anchor: {z}");
}

/// A force limit too weak for the hanging crate: the socket breaks by
/// itself (OnBreak, physics_constraints.md 1.5) and the crate falls; a
/// stronger one holds. CS:S's sockets need about 13.8 lb a kg hanging
/// (measured 2026-10-17, question 1): 276 for the 20 kg crate, which
/// breaks 200 and holds 400 (ours sees the spawn's jolt as a peak, so not
/// closer).
#[test]
fn a_ball_socket_breaks_under_a_weight_past_its_force_limit() {
    for (limit, breaks) in [("200", true), ("400", false)] {
        let box_ = entity(
            &[
                ("classname", "prop_physics"),
                ("targetname", "lamp"),
                ("origin", "0 0 200"),
            ],
            Vec::new(),
            false,
        );
        let socket = entity(
            &[
                ("classname", "phys_ballsocket"),
                ("targetname", "socket"),
                ("origin", "0 0 232"),
                ("attach1", "lamp"),
                ("forcelimit", limit),
            ],
            Vec::new(),
            false,
        );
        let mut data = map(vec![box_, socket]);
        add_crate(&mut data, 0, Vec3::new(0.0, 0.0, 200.0));
        let mut sim = sim(data);
        sim.app.world_mut().resource_mut::<Logic>().world.record = true;
        sim.seconds(2.0);
        let z = crate_at(&mut sim).z;
        let fired = sim
            .app
            .world()
            .resource::<Logic>()
            .world
            .fired
            .iter()
            .any(|(_, _, o)| o == "OnBreak");
        if breaks {
            assert!(fired && z < 20.0, "limit {limit}: broke and fell ({fired}, z {z})");
        } else {
            assert!(!fired && (z - 200.0).abs() < 8.0, "limit {limit}: holds ({fired}, z {z})");
        }
    }
}

/// phys_motor with "Hinge Object" (spawnflags 7, as surf_surreal's
/// flowers): the crate turns about the motor's vertical axis at its speed
/// and stays where it is.
#[test]
fn a_motor_spins_its_body_on_a_hinge() {
    use avian3d::prelude::AngularVelocity;
    let box_ = entity(
        &[
            ("classname", "prop_physics"),
            ("targetname", "flower"),
            ("origin", "0 0 100"),
        ],
        Vec::new(),
        false,
    );
    let motor = entity(
        &[
            ("classname", "phys_motor"),
            ("origin", "0 0 100"),
            ("axis", "0 0 140"),
            ("speed", "90"),
            ("spinup", "1"),
            ("spawnflags", "7"),
            ("attach1", "flower"),
        ],
        Vec::new(),
        false,
    );
    let mut data = map(vec![box_, motor]);
    add_crate(&mut data, 0, Vec3::new(0.0, 0.0, 100.0));
    let mut sim = sim(data);
    sim.seconds(3.0);
    let at = crate_at(&mut sim);
    assert!(
        (at.z - 100.0).abs() < 4.0 && at.x.abs() < 4.0,
        "held on its hinge: {at}"
    );
    let world = sim.app.world_mut();
    let node = world.query::<(Entity, &PropEntity)>().iter(world).next().unwrap().0;
    let w = world.get::<AngularVelocity>(node).unwrap().0;
    // 90 deg/s about Source +Z, engine +Y.
    assert!(
        (w.y.abs() - 90f32.to_radians()).abs() < 0.2,
        "spins at the motor's speed: {w}"
    );
}

/// Game_entities.md question 7 (measured 2026-10-17 on the probe: an
/// env_explosion of magnitude 100 at 100 units took 38-42 from a bot
/// without armour; with 100 armour, 19-21 health and 10-11 armour): armour
/// takes a map explosion's damage as a trigger_hurt's, health half and
/// armour half the rest (not the HE's ratio).
#[test]
fn an_env_explosion_takes_armour_at_ratio_one() {
    use crate::stack::{core::Health, weapon::Armor};
    let blast = |armour: f32| {
        let boom = entity(
            &[("classname", "env_explosion"), ("targetname", "boom"), ("origin", "60 0 36"), ("iMagnitude", "100")],
            Vec::new(),
            false,
        );
        let mut sim = sim(map(vec![boom]));
        let p = sim.spawn_character(to_engine(Vec3::new(0.0, 0.0, 37.0)), movement::ID);
        sim.app.world_mut().entity_mut(p).insert(Armor {
            amount: armour,
            helmet: false,
        });
        sim.ticks(10);
        sim.app
            .world_mut()
            .resource_mut::<Logic>()
            .world
            .queue_input("boom", "Explode", Value::Void, 0.0, None);
        sim.ticks(10);
        let w = sim.app.world();
        let health = (1.0 - w.get::<Health>(p).unwrap().current) * 100.0;
        let armour_lost = (armour - w.get::<Armor>(p).unwrap().amount) * 100.0;
        (health.round(), armour_lost.round())
    };
    let (bare, none) = blast(0.0);
    let (covered, taken) = blast(1.0);
    assert!(bare > 20.0 && none == 0.0, "no armour: {bare} {none}");
    assert!((covered - (bare / 2.0).floor()).abs() <= 1.0, "health half: {covered} of {bare}");
    assert!((taken - (bare - covered) / 2.0).abs() <= 1.0, "armour half the rest: {taken}");
}

/// Fire.md question 2 (measured 2026-10-17 on the probe: an env_fire of
/// size 256 took 7 a second from a bot without armour; with 100 armour 3
/// health and about 2 armour a second): armour takes its part of burns.
#[test]
fn fire_burns_through_armour_at_ratio_one() {
    use crate::stack::{core::Health, weapon::Armor};
    let burn = |armour: f32| {
        let fire = entity(
            &[
                ("classname", "env_fire"),
                ("targetname", "fire"),
                ("origin", "0 0 16"),
                ("firesize", "256"),
                ("fireattack", "1"),
                ("damagescale", "1.0"),
                ("spawnflags", "1"),
            ],
            Vec::new(),
            false,
        );
        let mut sim = sim(map(vec![fire]));
        let p = sim.spawn_character(to_engine(Vec3::new(0.0, 0.0, 37.0)), movement::ID);
        sim.app.world_mut().entity_mut(p).insert(Armor {
            amount: armour,
            helmet: false,
        });
        sim.ticks(10);
        sim.app
            .world_mut()
            .resource_mut::<Logic>()
            .world
            .queue_input("fire", "StartFire", Value::Void, 0.0, None);
        sim.seconds(6.0);
        let w = sim.app.world();
        let health = (1.0 - w.get::<Health>(p).unwrap().current) * 100.0;
        let armour_lost = (armour - w.get::<Armor>(p).unwrap().amount) * 100.0;
        (health.round(), armour_lost.round())
    };
    let (bare, none) = burn(0.0);
    let (covered, taken) = burn(1.0);
    assert!(bare >= 14.0 && none == 0.0, "no armour: {bare} {none}");
    assert!(covered <= bare / 2.0 + 1.0 && covered >= bare / 2.0 - 4.0, "health about half: {covered} of {bare}");
    assert!(taken > 0.0, "armour taken: {taken}");
}

/// A body broken while constrained breaks the constraint too: OnBreak
/// (CS:S, measured 2026-10-17: a crate hanging from a ball socket broken by
/// its Break input, the socket's OnBreak the tick after;
/// physics_constraints.md question 5).
#[test]
fn breaking_a_held_body_breaks_its_constraint() {
    let box_ = entity(
        &[("classname", "prop_physics"), ("targetname", "lamp"), ("origin", "0 0 200")],
        Vec::new(),
        false,
    );
    let socket = entity(
        &[("classname", "phys_ballsocket"), ("targetname", "socket"), ("origin", "0 0 232"), ("attach1", "lamp")],
        Vec::new(),
        false,
    );
    let mut data = map(vec![box_, socket]);
    add_crate(&mut data, 0, Vec3::new(0.0, 0.0, 200.0));
    let mut sim = sim(data);
    sim.app.world_mut().resource_mut::<Logic>().world.record = true;
    sim.seconds(1.0);
    let socket = sim.app.world().resource::<Logic>().world.find("socket").expect("socket");
    let broke = |sim: &Sim| {
        sim.app.world().resource::<Logic>().world.fired.iter().any(|(_, e, o)| o == "OnBreak" && *e == socket)
    };
    assert!(!broke(&sim), "holds");
    sim.app
        .world_mut()
        .resource_mut::<Logic>()
        .world
        .queue_input("lamp", "Break", Value::Void, 0.0, None);
    sim.ticks(5);
    assert!(broke(&sim), "the socket's OnBreak: {:?}", sim.app.world().resource::<Logic>().world.fired);
}
