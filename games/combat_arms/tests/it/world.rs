//! Combat Arms' map conversion (`game::world::convert`) on a small world
//! built in code (no game data): a floor and a wall in the physics BSP, a
//! drawn triangle and a start point. Checks the axes (Z mirrored, units
//! to metres), the spawn on the floor facing its yaw, and a player that
//! walks to the wall and stops there, and that the wall stops shots. The
//! real maps: `heavy::world`.

use bevy::prelude::*;
use chimera_world::{harness::Sim, map::MapSimPlugin};
use combat_arms::{
    game::{movement, world},
    harness::CaSim,
};
use lithtech_formats::world::{
    BspNode, BspPolygon, Object, Plane, Property, RenderBlock, RenderVertex, Section, Surface, Triangle, Value, World,
    WorldModel, model_flags, shader, surface_flags,
};

/// A physics BSP of axis-aligned quads (corners in file axes, wound so
/// (v1 − v0) × (v2 − v0) runs along the normal).
fn physics(quads: &[([f32; 3], [[f32; 3]; 4])]) -> WorldModel {
    let mut m = WorldModel {
        flags: model_flags::MAIN_WORLD | model_flags::PHYSICS,
        name: "PhysicsBSP".into(),
        textures: vec!["Default".into()],
        surfaces: vec![Surface {
            flags: surface_flags::SOLID,
            ..default()
        }],
        root: -1,
        ..default()
    };
    for (n, corners) in quads {
        let d = n[0] * corners[0][0] + n[1] * corners[0][1] + n[2] * corners[0][2];
        m.planes.push(Plane { normal: *n, dist: d });
        let first = m.point_indices.len() as u32;
        for c in corners {
            m.point_indices.push(m.points.len() as u32);
            m.points.push(*c);
        }
        m.polygons.push(BspPolygon {
            surface: 0,
            plane: m.planes.len() as u32 - 1,
            first,
            count: 4,
        });
        m.nodes.push(BspNode {
            polygon: m.polygons.len() as u32 - 1,
            back: -2,
            front: -1,
        });
    }
    m
}

fn prop(name: &str, value: Value) -> Property {
    Property {
        name: name.into(),
        flags: 0,
        len: 0,
        value,
    }
}

/// A floor at y = 0 (±1000 u) and a wall across +Z at z = 500 facing the
/// origin, one drawn triangle, and a JSF start point 66 u up facing +Z.
fn small_world() -> World {
    let floor = (
        [0.0, 1.0, 0.0],
        [[-1000.0, 0.0, -1000.0], [-1000.0, 0.0, 1000.0], [1000.0, 0.0, 1000.0], [1000.0, 0.0, -1000.0]],
    );
    let wall = (
        [0.0, 0.0, -1.0],
        [[-1000.0, 0.0, 500.0], [-1000.0, 400.0, 500.0], [1000.0, 400.0, 500.0], [1000.0, 0.0, 500.0]],
    );
    let vertex = |pos: [f32; 3]| RenderVertex {
        pos,
        uv0: [pos[0] / 64.0, pos[2] / 64.0],
        color: 0xFF80_8080,
        normal: [0.0, 1.0, 0.0],
        ..default()
    };
    let mut w = World {
        models: vec![physics(&[floor, wall])],
        ..default()
    };
    w.render.blocks.push(RenderBlock {
        sections: vec![Section {
            textures: ["textures\\floor.dtx".into(), String::new()],
            shader: shader::GOURAUD,
            triangle_count: 1,
            ..default()
        }],
        vertices: vec![
            vertex([0.0, 0.0, 0.0]),
            vertex([0.0, 0.0, 100.0]),
            vertex([100.0, 0.0, 0.0]),
        ],
        triangles: vec![Triangle {
            indices: [0, 1, 2],
            polygon: 0,
        }],
        ..default()
    });
    w.objects.push(Object {
        class: "GameStartPoint".into(),
        properties: vec![
            prop("Name", Value::String("GameStartPoint00".into())),
            prop("Pos", Value::Vector([0.0, 66.0, 0.0])),
            prop("Rotation", Value::Rotation([0.0, 0.3, 0.0, 0.0])),
            prop("GameType", Value::String("Team Death Match".into())),
            prop("Teams", Value::String("JSF".into())),
        ],
    });
    w
}

#[test]
fn converts_axes_spawns_and_meshes() {
    let map = world::convert(&small_world(), "small", 0.01, &|_| None);
    assert_eq!(map.name, "combat_arms:small");
    // The drawn triangle: Z mirrored, metres, winding reversed so its
    // front (+Y) stays counter-clockwise from above.
    let m = &map.meshes[0];
    assert_eq!(m.positions, [[0.0, 0.0, 0.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]]);
    let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(m.positions[m.indices[k] as usize]));
    assert!((b - a).cross(c - a).y > 0.0);
    assert_eq!(m.uvs[1], [0.0, 100.0 / 64.0]);
    assert_eq!(m.lightmap_uvs.len(), 3);
    assert!(map.lightmap.is_some());
    assert_eq!((m.texture, map.warnings.len()), (None, 1), "no texture file: a flat colour and a warning");
    // Collision: two quads, two triangles each.
    assert_eq!(map.collision_indices.len(), 4);
    // The start point, dropped to the floor, team JSF, its yaw negated.
    assert_eq!(map.spawns.len(), 1);
    let (feet, team) = map.spawns[0];
    assert!(feet.distance(Vec3::ZERO) < 1e-4, "{feet}");
    assert_eq!(team.map(|t| t.0), Some(1));
    assert_eq!(map.spawn_yaws, [-0.3]);
}

/// Spawned at the start point (yaw 0: +Z in file axes, −Z here), a
/// player stands on the floor, walks forward and stops at the wall 5 m
/// ahead.
#[test]
fn a_player_walks_to_the_wall_and_stops() {
    let mut w = small_world();
    w.objects[0].properties[2] = prop("Rotation", Value::Rotation([0.0; 4]));
    let map = world::convert(&w, "small", movement::movement_data().meters_per_unit, &|_| None);
    let (feet, yaw) = (map.spawns[0].0, map.spawn_yaws[0]);
    let mut sim = Sim::new(CaSim, MapSimPlugin::new(map));
    sim.ticks(2);
    let p = sim.spawn_character(feet + Vec3::Y, movement::ID);
    sim.intent(p).yaw = yaw;
    sim.seconds(1.0);
    let rest = sim.position(p);
    assert!(rest.y > 0.0 && rest.y < 1.5 && sim.velocity(p).length() < 0.05, "standing at {rest}");
    sim.intent(p).move_axis = Vec2::new(0.0, 1.0);
    sim.seconds(5.0);
    let stopped = sim.position(p);
    assert!(stopped.z < -4.0 && stopped.z > -5.0, "stopped at the wall (z = -5 m): {stopped}");
    assert!(stopped.x.abs() < 0.01 && (stopped.y - rest.y).abs() < 0.05, "{stopped}");
}

/// Half a second of rifle fire from the start point (yaw 0: down −Z, at
/// the wall 5 m away) at a target `ahead` metres away; with `wall` false
/// the wall's two collision triangles are left out. Whether it was hurt.
fn shoot(ahead: f32, wall: bool) -> bool {
    let mut map = world::convert(&small_world(), "small", movement::movement_data().meters_per_unit, &|_| None);
    if !wall {
        map.collision_indices.truncate(2);
    }
    let feet = map.spawns[0].0;
    let mut sim = Sim::new(CaSim, MapSimPlugin::new(map));
    sim.ticks(2);
    let shooter = sim.spawn_character(feet + Vec3::Y, movement::ID);
    let target = sim.spawn_character(feet + Vec3::new(0.0, 1.0, -ahead), movement::ID);
    // Settled, the rifle drawn.
    sim.seconds(1.5);
    let health = |sim: &Sim| sim.app.world().get::<chimera_core::core::Health>(target).unwrap().current;
    let full = health(&sim);
    sim.intent(shooter).fire = true;
    sim.seconds(0.5);
    sim.intent(shooter).fire = false;
    sim.ticks(2);
    health(&sim) < full
}

/// Shots hit the map: the rifle hurts a target in front of the wall, not
/// one behind it (which it does hurt without the wall).
#[test]
fn the_wall_stops_shots() {
    assert!(shoot(3.0, true), "the target in front of the wall took no damage");
    assert!(!shoot(7.0, true), "a shot went through the wall");
    assert!(shoot(7.0, false), "without the wall the target behind it is hit");
}

