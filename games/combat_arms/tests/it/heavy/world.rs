//! Combat Arms' maps from the user's install (`game::world`): every
//! plain map converts, and a player spawned on WAREHOUSE stands on its
//! floor, walks, and stops at a wall. Passes with a note without
//! `[games.combat_arms] path` in chimera.local.toml.

use avian3d::prelude::*;
use bevy::{ecs::system::SystemState, prelude::*};
use chimera_core::mount::config::LocalConfig;
use chimera_world::{
    harness::Sim,
    map::{MapData, MapSimPlugin},
};
use combat_arms::{
    game::{GAME, INFO, movement, world},
    harness::CaSim,
};

fn mount() -> Option<chimera_core::mount::Mount> {
    let Some(install) = LocalConfig::load().ok().and_then(|c| c.game_path(GAME)) else {
        eprintln!("skipping: no Combat Arms install configured");
        return None;
    };
    if !install.join("Game").is_dir() {
        eprintln!("skipping: no Combat Arms install at {}", install.display());
        return None;
    }
    Some((INFO.open)(&install).unwrap())
}

/// The 33 plain maps convert: drawn meshes with lightmap coordinates
/// inside the atlas, a collision mesh, and spawns on a floor.
#[test]
fn every_plain_map_converts() {
    let Some(mount) = mount() else { return };
    let names = world::map_names(&mount);
    let mut loaded = 0;
    for name in &names {
        let path = world::find(&mount, name).unwrap();
        if !(path.starts_with("worlds2/") || path.starts_with("worlds3/")) {
            continue;
        }
        let map = world::load_from(&mount, name).unwrap_or_else(|e| panic!("{name}: {e}"));
        let lightmap = map.lightmap.as_ref().unwrap();
        eprintln!(
            "{name}: {} meshes, {} triangles, {} collision triangles, {} spawns, lightmap {} x {}",
            map.meshes.len(),
            map.triangle_count(),
            map.collision_indices.len(),
            map.spawns.len(),
            lightmap.width,
            lightmap.height
        );
        assert!(map.triangle_count() > 1000, "{name}");
        assert!(map.collision_indices.len() > 1000, "{name}");
        assert!(lightmap.width <= 8192 && lightmap.height <= 8192, "{name}");
        for m in &map.meshes {
            assert_eq!(m.lightmap_uvs.len(), m.positions.len(), "{name}");
            assert!(m.lightmap_uvs.iter().flatten().all(|c| (0.0..=1.0).contains(c)), "{name}");
            assert!(m.indices.iter().all(|&i| (i as usize) < m.positions.len()), "{name}");
        }
        assert!(!map.spawns.is_empty() && map.spawn_yaws.len() == map.spawns.len(), "{name}");
        loaded += 1;
    }
    assert_eq!(loaded, 33);
}

fn warehouse() -> Option<MapData> {
    world::load_from(&mount()?, "warehouse").map(Some).unwrap()
}

/// WAREHOUSE's spawns: test case 2's 24 Team Death Match points, a team
/// each, and each over the floor.
#[test]
fn warehouse_spawns() {
    let Some(map) = warehouse() else { return };
    assert_eq!(map.name, "combat_arms:warehouse");
    assert_eq!(map.spawns.len(), 24);
    let teams = |t: u8| map.spawns.iter().filter(|(_, team)| team.map(|x| x.0) == Some(t)).count();
    assert_eq!((teams(1), teams(2)), (12, 12));
    // Object 92, GameStartPoint11 (JSF), at (618, −5546, −2271.0002) u in
    // file axes: Z mirrored, metres; its feet on the floor below.
    let s = movement::movement_data().meters_per_unit;
    let at = Vec3::new(618.0, -5546.0, 2271.0) * s;
    let (feet, team) = map
        .spawns
        .iter()
        .find(|(p, _)| p.xz().distance(at.xz()) < 0.01)
        .expect("GameStartPoint11");
    assert_eq!(team.map(|t| t.0), Some(1));
    assert!(feet.y < at.y && feet.y > at.y - 200.0 * s, "{feet} below {at}");
}

/// world_dat.md cases 9, 10, 13, 14 and 20 on WAREHOUSE: its non-solid
/// physics polygons stop traces only and its 50 collision-blocker
/// polygons players only; the sky camera follows the player through the
/// sky object's inner box; the fog is the world properties' (linear,
/// (23, 23, 19), 0 to 10000 u); no file hash.
#[test]
fn warehouse_collision_sky_and_fog() {
    let Some(map) = warehouse() else { return };
    let s = movement::movement_data().meters_per_unit;
    let tris = |players: bool, traces: bool| -> usize {
        map.collision_sets
            .iter()
            .filter(|c| c.characters == players && c.traces == traces)
            .map(|c| c.indices.len())
            .sum()
    };
    assert!(tris(false, true) >= 4_960, "non-solid polygons: {}", tris(false, true));
    assert!(tris(true, false) >= 50, "blocker polygons: {}", tris(true, false));
    // Sky: SkyPointerBox00 at (6623, 7009, -1471) u, SkyDims 256,
    // InnerPercent 0.001; extents +-(9952, 10080, 10112).
    let cam = map.sky_camera.as_ref().expect("a sky camera");
    let sky_at = |p: Vec3| cam.origin + p / cam.scale;
    let engine = |p: Vec3| Vec3::new(p.x, p.y, -p.z) * s;
    let close = |a: Vec3, b: Vec3| (a - b).abs().max_element() < 0.01 * s;
    assert!(close(sky_at(Vec3::ZERO), engine(Vec3::new(6623.0, 7009.0, -1471.0))), "{}", sky_at(Vec3::ZERO) / s);
    let corner = engine(Vec3::new(9952.0, 10080.0, 10112.0));
    let want = engine(Vec3::new(6623.256, 7009.256, -1470.744));
    assert!((sky_at(corner) - want).abs().max_element() < 0.02 * s, "{} vs {}", sky_at(corner) / s, want / s);
    assert!(map.meshes.iter().any(|m| m.skybox), "the sky's own world model is drawn by the sky camera");
    let fog = map.fog.as_ref().expect("fog");
    assert_eq!(fog.color.map(|c| (c * 255.0).round() as u8), [23, 23, 19]);
    assert!(fog.start.abs() < 1e-6 && (fog.end - 10_000.0 * s).abs() < 1e-3 && fog.linear, "{fog:?}");
    assert_eq!(map.file_hash, None);
}

/// world_dat.md case 12: SNOWSTORM's two light groups start off (their
/// objects' `StartOn` false); GRAVITYB's `Light_Grv_01` starts on and
/// `Light_Grv_02` off.
#[test]
fn light_groups_start_state() {
    let Some(mount) = mount() else { return };
    // Either may be in a keyed archive: then it is left out.
    let read = |name: &str| {
        let path = world::find(&mount, name)?;
        lithtech_formats::world::World::parse(&mount.read(&path).ok()?).ok()
    };
    let Some(snow) = read("snowstorm") else { return };
    let colours = world::light_group_colours(&snow);
    let names: Vec<String> = snow
        .render
        .all_blocks()
        .flat_map(|b| b.light_groups.iter().map(|g| g.name.clone()))
        .collect();
    assert!(!names.is_empty());
    for n in &names {
        assert_eq!(colours(n, [1.0, 0.968_627_5, 0.901_960_8]), [0.0; 3], "{n}");
    }
    let Some(grav) = read("gravityb") else {
        eprintln!("GRAVITYB is in a keyed archive: its part skipped");
        return;
    };
    let colours = world::light_group_colours(&grav);
    assert_ne!(colours("Light_Grv_01", [1.0; 3]), [0.0; 3]);
    assert_eq!(colours("Light_Grv_02", [1.0; 3]), [0.0; 3]);
}

/// A player spawned at a WAREHOUSE spawn stands on the floor, walks on
/// it, and a wall stops it.
#[test]
fn warehouse_walk_and_collide() {
    let Some(map) = warehouse() else { return };
    let (feet, _) = map.spawns[0];
    let yaw = map.spawn_yaws[0];
    let mut sim = Sim::new(CaSim, MapSimPlugin::new(map));
    sim.ticks(2);
    let p = sim.spawn_character(feet + Vec3::Y, movement::ID);
    sim.intent(p).yaw = yaw;
    sim.seconds(1.0);
    let rest = sim.position(p);
    assert!(sim.velocity(p).length() < 0.05, "at rest: {}", sim.velocity(p));
    assert!(rest.y > feet.y && rest.y < feet.y + 1.5, "standing at {rest}, feet {feet}");

    // Walk forward for a second: moves over the floor, stays on it.
    sim.intent(p).move_axis = Vec2::new(0.0, 1.0);
    sim.seconds(1.0);
    sim.intent(p).move_axis = Vec2::ZERO;
    sim.seconds(0.5);
    let walked = sim.position(p);
    assert!(walked.xz().distance(rest.xz()) > 1.0, "walked from {rest} to {walked}");
    assert!((walked.y - rest.y).abs() < 1.0, "still on the floor: {walked}");

    // The nearest wall around, by a ray at chest height; walk into it.
    let from = walked;
    let (dir, dist) = {
        let mut state = SystemState::<SpatialQuery>::new(sim.app.world_mut());
        let q = state.get(sim.app.world()).unwrap();
        // What players collide with (not the traces-only detail).
        let filter = SpatialQueryFilter::from_excluded_entities([p]).with_mask(chimera_core::core::CHARACTER_FILTER);
        (0..16)
            .filter_map(|k| {
                let a = k as f32 * std::f32::consts::TAU / 16.0;
                let d = Dir3::new(Vec3::new(a.sin(), 0.0, -a.cos())).unwrap();
                let hit = q.cast_ray(from, d, 30.0, true, &filter)?;
                Some((d, hit.distance))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .expect("a wall within 30 m")
    };
    // Face it (yaw 0 = −Z, positive turns toward −X) and run 3 s.
    sim.intent(p).yaw = (-dir.x).atan2(-dir.z);
    sim.intent(p).move_axis = Vec2::new(0.0, 1.0);
    sim.seconds(3.0 + f64::from(dist) / 3.0);
    let stopped = sim.position(p);
    let moved = (stopped - from).dot(*dir);
    assert!(moved < dist, "stopped at {moved} m of the wall's {dist} m");
    assert!(moved > dist - 1.5, "reached the wall: {moved} m of {dist} m");
}
