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
    BspNode, BspPolygon, GeometryPolygon, Object, Plane, Property, RenderBlock, RenderVertex, Section, Surface,
    Triangle, Value, World, WorldModel, model_flags, shader, surface_flags,
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
/// player stands on the floor, walks forward and stops at the wall 500 u
/// ahead.
#[test]
fn a_player_walks_to_the_wall_and_stops() {
    let mut w = small_world();
    w.objects[0].properties[2] = prop("Rotation", Value::Rotation([0.0; 4]));
    let s = movement::movement_data().meters_per_unit;
    let map = world::convert(&w, "small", s, &|_| None);
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
    assert!(stopped.z < -400.0 * s && stopped.z > -500.0 * s, "stopped at the wall (z = -500 u): {stopped}");
    assert!(stopped.x.abs() < 0.01 && (stopped.y - rest.y).abs() < 0.05, "{stopped}");
}

/// Half a second of rifle fire from the start point (yaw 0: down −Z, at
/// the wall 500 u away) at a target `ahead` world units away; with `wall`
/// false the wall's two collision triangles are left out. Whether it was
/// hurt.
fn shoot(ahead: f32, wall: bool) -> bool {
    let s = movement::movement_data().meters_per_unit;
    let mut map = world::convert(&small_world(), "small", s, &|_| None);
    if !wall {
        map.collision_indices.truncate(2);
    }
    let feet = map.spawns[0].0;
    let mut sim = Sim::new(CaSim, MapSimPlugin::new(map));
    sim.ticks(2);
    let shooter = sim.spawn_character(feet + Vec3::Y, movement::ID);
    let target = sim.spawn_character(feet + Vec3::new(0.0, 1.0, -ahead * s), movement::ID);
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
    assert!(shoot(300.0, true), "the target in front of the wall took no damage");
    assert!(!shoot(700.0, true), "a shot went through the wall");
    assert!(shoot(700.0, false), "without the wall the target behind it is hit");
}

/// The small world with its wall's surface `flags` (non-solid, invisible)
/// or, with `blocker`, the wall as a collision blocker instead.
fn walled(flags: u32, blocker: bool) -> World {
    let mut w = small_world();
    let m = &mut w.models[0];
    m.surfaces.push(Surface { flags, ..default() });
    m.polygons[1].surface = 1;
    if blocker {
        let wall: Vec<[f32; 3]> = m.polygon_vertices(1).collect();
        m.polygons.truncate(1);
        m.nodes.truncate(1);
        w.blockers.polygons.push(GeometryPolygon {
            plane: Plane {
                normal: [0.0, 0.0, -1.0],
                dist: -500.0,
            },
            vertices: wall,
        });
    }
    w
}

/// world_dat.md cases 9 and 10 (Q5): which polygons stop what. A wall
/// without the solid bit stops shots (rays test every polygon) but not
/// players; a solid invisible one stops both; a collision blocker stops
/// players only.
#[test]
fn t09_t10_what_collides() {
    let s = movement::movement_data().meters_per_unit;
    for (what, w, shots, players) in [
        ("non-solid", walled(0, false), true, false),
        ("invisible", walled(surface_flags::SOLID | surface_flags::INVISIBLE, false), true, true),
        ("blocker", walled(0, true), false, true),
    ] {
        let map = world::convert(&w, "small", s, &|_| None);
        let sets = |players: bool, traces: bool| {
            map.collision_sets
                .iter()
                .filter(|c| c.characters == players && c.traces == traces)
                .map(|c| c.indices.len())
                .sum::<usize>()
        };
        let main = map.collision_indices.len();
        match (shots, players) {
            (true, false) => assert_eq!((main, sets(false, true)), (2, 2), "{what}"),
            (true, true) => assert_eq!(main, 4, "{what}"),
            _ => assert_eq!((main, sets(true, false)), (2, 2), "{what}"),
        }
    }
    // Shots: the non-solid wall stops them, the blocker doesn't.
    assert!(!shoot_world(walled(0, false), 700.0), "through non-solid detail");
    assert!(shoot_world(walled(0, true), 700.0), "a blocker stopped a shot");
    // Players: through the non-solid wall, stopped by the blocker.
    assert!(walk_world(walled(0, false)) < -500.0 * s, "stopped by non-solid detail");
    assert!(walk_world(walled(0, true)) > -500.0 * s, "through a blocker");
}

/// `shoot` on another world.
fn shoot_world(w: World, ahead: f32) -> bool {
    let s = movement::movement_data().meters_per_unit;
    let map = world::convert(&w, "small", s, &|_| None);
    let feet = map.spawns[0].0;
    let mut sim = Sim::new(CaSim, MapSimPlugin::new(map));
    sim.ticks(2);
    let shooter = sim.spawn_character(feet + Vec3::Y, movement::ID);
    let target = sim.spawn_character(feet + Vec3::new(0.0, 1.0, -ahead * s), movement::ID);
    sim.seconds(1.5);
    let health = |sim: &Sim| sim.app.world().get::<chimera_core::core::Health>(target).unwrap().current;
    let full = health(&sim);
    sim.intent(shooter).fire = true;
    sim.seconds(0.5);
    sim.intent(shooter).fire = false;
    sim.ticks(2);
    health(&sim) < full
}

/// A player walking 6 s from the start point toward the wall: where it
/// ends (z, m).
fn walk_world(mut w: World) -> f32 {
    w.objects[0].properties[2] = prop("Rotation", Value::Rotation([0.0; 4]));
    let s = movement::movement_data().meters_per_unit;
    let map = world::convert(&w, "small", s, &|_| None);
    let (feet, yaw) = (map.spawns[0].0, map.spawn_yaws[0]);
    let mut sim = Sim::new(CaSim, MapSimPlugin::new(map));
    sim.ticks(2);
    let p = sim.spawn_character(feet + Vec3::Y, movement::ID);
    sim.intent(p).yaw = yaw;
    sim.seconds(1.0);
    sim.intent(p).move_axis = Vec2::new(0.0, 1.0);
    sim.seconds(6.0);
    sim.position(p).z
}

/// A block with one lightmapped polygon: a shader-2 section (its
/// `lightmap`, `w` x `h` texels of RGB) and a shader-4 section over the
/// same three vertices, whose UV0 is the lightmap coordinate `lm`.
fn lit_block(lm: [[f32; 2]; 3], lightmap: Vec<u8>, w: u32, h: u32) -> RenderBlock {
    let pos = [[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.0, 0.0, 100.0]];
    let vertex = |i: usize| RenderVertex {
        pos: pos[i],
        uv0: lm[i],
        color: 0xFFFF_FFFF,
        normal: [0.0, 1.0, 0.0],
        ..default()
    };
    RenderBlock {
        sections: vec![
            Section {
                textures: ["".into(), String::new()],
                shader: shader::LIGHTMAP,
                triangle_count: 1,
                lightmap_width: w,
                lightmap_height: h,
                lightmap,
                ..default()
            },
            Section {
                textures: ["textures\\lit.dtx".into(), String::new()],
                shader: shader::LIGHTMAP_TEXTURE,
                triangle_count: 1,
                ..default()
            },
        ],
        vertices: (0..3).map(vertex).collect(),
        triangles: vec![
            Triangle {
                indices: [0, 1, 2],
                polygon: 0,
            },
            Triangle {
                indices: [0, 1, 2],
                polygon: 0,
            },
        ],
        ..default()
    }
}

/// world_dat.md case 8 (Q9): lightmap coordinates beyond 1 clamp per
/// pixel, not per vertex: the triangle is cut where u = 1 and the part
/// beyond reads the edge column; nothing is lost.
#[test]
fn t08_lightmap_clamps_per_pixel() {
    let mut w = small_world();
    w.render.blocks = vec![lit_block([[0.98, 0.5], [3.02, 0.5], [0.98, 0.6]], vec![128; 4 * 4 * 3], 4, 4)];
    let map = world::convert(&w, "small", 1.0, &|_| None);
    let m = map.meshes.iter().find(|m| m.material == "textures\\lit.dtx").expect("the lit mesh");
    // The cut: a vertex where u = 1 on the edge from u 0.98 to 3.02, at
    // x = 0.02 / 2.04 of the way (z mirrored).
    let cut = 100.0 * 0.02 / 2.04;
    assert!(
        m.positions.iter().any(|p| (p[0] - cut).abs() < 1e-3 && p[2].abs() < 1e-3),
        "{:?}",
        m.positions
    );
    // The area is all there.
    let area: f32 = m
        .indices
        .chunks(3)
        .map(|t| {
            let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(m.positions[t[k] as usize]));
            (b - a).cross(c - a).length() / 2.0
        })
        .sum();
    assert!((area - 5000.0).abs() < 1e-2, "{area}");
    // Beyond the cut every vertex reads the last column: the same atlas u.
    let edge = m
        .positions
        .iter()
        .zip(&m.lightmap_uvs)
        .filter(|(p, _)| p[0] >= cut - 1e-3)
        .map(|(_, uv)| uv[0])
        .collect::<Vec<f32>>();
    assert!(edge.len() >= 3 && edge.iter().all(|u| (u - edge[0]).abs() < 1e-6), "{edge:?}");
}

/// world_dat.md case 11 (Q12): a light group adds ⌊colour × x⌋ to each
/// channel, at most 255; case 15 (Q7): the drawn light is twice the
/// lightmap (`Saturate`) times the lightmap factor on both passes.
#[test]
fn t11_t15_light_group_and_lighting_multiply() {
    let on = [1.0, 0.968_627_5, 0.901_960_8];
    assert_eq!(on.map(|c| world::add_light(100, c, 100)), [200, 196, 190]);
    assert_eq!(world::add_light(100, 128.0 / 255.0, 100), 150);
    assert_eq!(world::add_light(200, 1.0, 100), 255);
    // Texture 0.8 under lightmap 0.5: 0.8 (Saturate 1), in gamma space.
    let shown = |tex: f32, lm: u8, t: f32| {
        let lin = Color::srgb(tex, tex, tex).to_linear().red * world::light_value(lm, t);
        Color::LinearRgba(LinearRgba::rgb(lin, lin, lin)).to_srgba().red
    };
    assert!((shown(0.8, 128, 1.0) - 0.8).abs() < 0.01, "{}", shown(0.8, 128, 1.0));
    assert!((shown(0.8, 64, 1.0) - 0.4).abs() < 0.015, "{}", shown(0.8, 64, 1.0));
    assert!((shown(0.8, 128, 128.0 / 255.0) - 0.2016).abs() < 0.015, "{}", shown(0.8, 128, 128.0 / 255.0));
}

/// world_dat.md case 12 (Q12) on a built world: a group whose
/// `LightGroup` object starts off adds nothing; one that starts on adds
/// its `StartColor`; without an object, the stored colour.
#[test]
fn t12_light_group_start_state() {
    let mut w = small_world();
    w.objects.push(Object {
        class: "LightGroup".into(),
        properties: vec![
            prop("Name", Value::String("Off".into())),
            prop("StartOn", Value::Bool(false)),
            prop("StartColor", Value::Color([255.0; 3])),
        ],
    });
    w.objects.push(Object {
        class: "LightGroup".into(),
        properties: vec![prop("Name", Value::String("On".into())), prop("StartColor", Value::Color([128.0; 3]))],
    });
    let colours = world::light_group_colours(&w);
    assert_eq!(colours("Off", [1.0; 3]), [0.0; 3]);
    assert_eq!(colours("On", [1.0; 3]), [128.0 / 255.0; 3]);
    assert_eq!(colours("Elsewhere", [0.5, 0.25, 1.0]), [0.5, 0.25, 1.0]);
}

/// world_dat.md case 17 (Q17): a section with a shader code other than
/// 1, 2, 4, 8 or 9 is kept and never drawn; case 20 (Q15): no file hash,
/// so the network handshake compares none.
#[test]
fn t17_t20_unknown_shader_and_no_hash() {
    let mut w = small_world();
    w.render.blocks[0].sections[0].shader = 3;
    let map = world::convert(&w, "small", 0.01, &|_| None);
    assert!(map.meshes.is_empty(), "not drawn");
    assert_eq!(map.file_hash, None);
}

/// world_dat.md case 22 (Q14): a `WorldModel` that is ray-hit but not
/// solid stops traces, not players; one that is neither is drawn only;
/// a solid one blocks both.
#[test]
fn t22_world_model_flags() {
    let s = 0.01;
    let with = |solid: bool, rayhit: bool| {
        let mut w = small_world();
        let mut door = physics(&[(
            [0.0, 0.0, -1.0],
            [[-50.0, 0.0, 300.0], [-50.0, 200.0, 300.0], [50.0, 200.0, 300.0], [50.0, 0.0, 300.0]],
        )]);
        door.flags = 0;
        door.name = "Door".into();
        w.models.push(door);
        w.objects.push(Object {
            class: "WorldModel".into(),
            properties: vec![
                prop("Name", Value::String("Door".into())),
                prop("Solid", Value::Bool(solid)),
                prop("Rayhit", Value::Bool(rayhit)),
            ],
        });
        let map = world::convert(&w, "small", s, &|_| None);
        let traces_only: usize = map
            .collision_sets
            .iter()
            .filter(|c| c.traces && !c.characters)
            .map(|c| c.indices.len())
            .sum();
        (map.collision_indices.len(), traces_only)
    };
    assert_eq!(with(true, true), (6, 0));
    assert_eq!(with(false, true), (4, 2));
    assert_eq!(with(false, false), (4, 0));
}

/// world_dat.md "Moving world models and volumes" (Q14) and grenades.md
/// section 7 (Q19): a `Water` volume reaches the game as a liquid
/// `CaVolume` (its `Viscosity`, `Current`); an `EndlessFall` volume kills
/// a player inside.
#[test]
fn volumes_from_the_map() {
    use combat_arms::game::volumes::CaVolume;
    let s = movement::movement_data().meters_per_unit;
    let volume = |class: &str, name: &str, props: Vec<Property>| {
        let mut m = physics(&[
            ([0.0, 1.0, 0.0], [[-100.0, 100.0, -100.0], [-100.0, 100.0, 100.0], [100.0, 100.0, 100.0], [100.0, 100.0, -100.0]]),
            ([0.0, -1.0, 0.0], [[-100.0, 0.0, -100.0], [100.0, 0.0, -100.0], [100.0, 0.0, 100.0], [-100.0, 0.0, 100.0]]),
            ([1.0, 0.0, 0.0], [[100.0, 0.0, -100.0], [100.0, 100.0, -100.0], [100.0, 100.0, 100.0], [100.0, 0.0, 100.0]]),
            ([-1.0, 0.0, 0.0], [[-100.0, 0.0, -100.0], [-100.0, 0.0, 100.0], [-100.0, 100.0, 100.0], [-100.0, 100.0, -100.0]]),
            ([0.0, 0.0, 1.0], [[-100.0, 0.0, 100.0], [100.0, 0.0, 100.0], [100.0, 100.0, 100.0], [-100.0, 100.0, 100.0]]),
            ([0.0, 0.0, -1.0], [[-100.0, 0.0, -100.0], [-100.0, 100.0, -100.0], [100.0, 100.0, -100.0], [100.0, 0.0, -100.0]]),
        ]);
        m.flags = 0;
        m.name = name.into();
        let mut properties = vec![prop("Name", Value::String(name.into()))];
        properties.extend(props);
        (m, Object { class: class.into(), properties })
    };
    let mut w = small_world();
    let (m, o) = volume(
        "Water",
        "Pond",
        vec![prop("Viscosity", Value::Real(0.1)), prop("Current", Value::Vector([0.0, 1500.0, 0.0]))],
    );
    w.models.push(m);
    w.objects.push(o);
    let map = world::convert(&w, "small", s, &|_| None);
    assert_eq!(map.collision_indices.len(), 4, "a volume doesn't collide");
    let mut sim = Sim::new(CaSim, MapSimPlugin::new(map));
    sim.ticks(3);
    let world = sim.app.world_mut();
    let vols: Vec<CaVolume> = world.query::<&CaVolume>().iter(world).cloned().collect();
    assert_eq!(vols.len(), 1);
    let v = &vols[0];
    assert!(v.liquid && (v.viscosity - 0.1).abs() < 1e-6 && v.current == Vec3::new(0.0, 1500.0, 0.0), "{v:?}");
    assert!(v.contains(Vec3::new(0.0, 50.0 * s, 0.0)) && !v.contains(Vec3::new(0.0, 150.0 * s, 0.0)));
    assert!(v.contains(Vec3::new(90.0 * s, 50.0 * s, -90.0 * s)) && !v.contains(Vec3::new(110.0 * s, 50.0 * s, 0.0)));

    // A pit that kills.
    let mut w = small_world();
    let (m, o) = volume("EndlessFall", "Pit", vec![]);
    w.models.push(m);
    w.objects.push(o);
    let map = world::convert(&w, "small", s, &|_| None);
    let feet = map.spawns[0].0;
    let mut sim = Sim::new(CaSim, MapSimPlugin::new(map));
    sim.ticks(2);
    let p = sim.spawn_character(feet + Vec3::Y, movement::ID);
    sim.seconds(1.0);
    let health = sim.app.world().get::<chimera_core::core::Health>(p).unwrap().current;
    assert!(health <= 0.0, "killed: {health}");
}


/// Glass (surface types 40 and 41, `world::GLASS_SURFACE_TYPES`) is
/// collision of its own marked Chimera's `SeeThrough`, solid to players as
/// the rest of the wall is (bots see through it, bots.md 5.2).
#[test]
fn glass_is_see_through() {
    let s = movement::movement_data().meters_per_unit;
    let mut w = small_world();
    let m = &mut w.models[0];
    m.surfaces.push(Surface {
        flags: surface_flags::SOLID,
        surface_type: 40,
        ..default()
    });
    m.polygons[1].surface = 1;
    let map = world::convert(&w, "small", s, &|_| None);
    assert_eq!(map.collision_indices.len(), 2, "the floor");
    let glass: Vec<_> = map.collision_sets.iter().filter(|c| c.see_through && !c.indices.is_empty()).collect();
    assert_eq!(glass.len(), 1);
    assert!(glass[0].characters && glass[0].traces && glass[0].indices.len() == 2, "{:?}", glass[0]);
    let mut sim = Sim::new(CaSim, MapSimPlugin::new(map));
    sim.ticks(2);
    let world = sim.app.world_mut();
    let marked = world
        .query_filtered::<&Name, With<chimera_world::map::SeeThrough>>()
        .iter(world)
        .map(|n| n.to_string())
        .collect::<Vec<_>>();
    assert_eq!(marked, ["Map collision (glass)"]);
    assert!(walk_world(w) > -500.0 * s, "players stop at the glass");
}

/// The AI objects (volumes, patrol nodes) pass through as map entities,
/// every property as text, vectors in entity axes; other objects don't.
#[test]
fn ai_objects_pass_through() {
    use combat_arms::game::bots::map_ai::MapAi;
    let s = movement::movement_data().meters_per_unit;
    let mut w = small_world();
    w.objects.push(Object {
        class: "AIVolume".into(),
        properties: vec![
            prop("Name", Value::String("AIVolume01".into())),
            prop("Pos", Value::Vector([0.0, 32.0, 300.0])),
            prop("Dims", Value::Vector([200.0, 32.0, 100.0])),
            prop("PreferredPath", Value::Bool(false)),
        ],
    });
    for (name, z, next) in [("AINodePatrol_01", 250.0, "AINodePatrol_02"), ("AINodePatrol_02", 350.0, "")] {
        w.objects.push(Object {
            class: "AINodePatrol".into(),
            properties: vec![
                prop("Name", Value::String(name.into())),
                prop("Pos", Value::Vector([0.0, 16.0, z])),
                prop("Next", Value::String(next.into())),
                prop("StartDisabled", Value::Bool(false)),
            ],
        });
    }
    let map = world::convert(&w, "small", s, &|_| None);
    let classes: Vec<&str> = map.entities.iter().map(|e| e.classname()).collect();
    assert_eq!(classes, ["AIVolume", "AINodePatrol", "AINodePatrol"]);
    let v = &map.entities[0];
    assert_eq!(v.get("targetname"), Some("AIVolume01"));
    assert_eq!(v.get("origin"), Some("0 300 32"));
    assert_eq!(v.get("PreferredPath"), Some("0"));
    assert_eq!(map.entity_scale, s);
    let ai = MapAi::read(&map.entities, map.entity_scale);
    assert_eq!(ai.volumes.len(), 1);
    // Z mirrored: the file's z 200..400 is engine z -400..-200 (u).
    assert!(ai.volumes[0].min.abs_diff_eq(Vec3::new(-200.0, 0.0, -400.0) * s, 1e-5), "{:?}", ai.volumes[0]);
    assert!(ai.volumes[0].max.abs_diff_eq(Vec3::new(200.0, 64.0, -200.0) * s, 1e-5), "{:?}", ai.volumes[0]);
    assert_eq!(ai.patrol.len(), 2);
    assert_eq!(ai.patrol[0].next, ["AINodePatrol_02"]);
    assert!(ai.patrol[1].pos.abs_diff_eq(Vec3::new(0.0, 16.0, -350.0) * s, 1e-5));
}
