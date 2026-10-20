//! CS:S's maps as the mashup plays them (the user's installs), headless on
//! Combat Arms' simulation with the mashup's parts
//! (`mashup_demo::MashupPartsPlugin`): Source's map logic runs for a
//! Combat Arms soldier (de_nuke: +use opens a door pair, the M4A1 breaks a
//! metal vent), CS:S's surfaces are Combat Arms' surface types (de_dust2's
//! sand, stone and wood; a round into the sand plays Combat Arms' sand
//! impact), and a slot of the loadout takes CS:S's AK-47, which a Combat
//! Arms soldier fires into another on de_dust2. Passes with a note
//! without both installs in chimera.local.toml.

use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::{ecs::system::SystemState, prelude::*};
use chimera_core::{
    core::{Health, SpawnPoint},
    mount::config::LocalConfig,
};
use chimera_sim::{
    movement::noclip,
    weapon::{Inventory, Magazine, StartingWeapons, Weapon},
};
use chimera_world::{
    harness::Sim,
    map::{
        MapBrushEntity, MapData, MapEntity, MapSimPlugin, SurfaceNames,
        entities::{entity_rotation, entity_to_engine},
        sound::SurfaceGrid,
    },
};
use combat_arms::{
    game::{
        effects::{data::CaFxTables, sounds::CaSound, world_surface_named},
        movement, weapons,
    },
    harness::CaSim,
};
use source_logic::logic::{Logic, classes::Class, movers::pusher};

/// CS:S map `name` as the mashup loads it; None (with a note) without
/// both installs.
fn load(name: &str) -> Option<MapData> {
    let config = LocalConfig::load().ok()?;
    let css = config.game_path(cs_source::game::GAME).filter(|p| p.join("cstrike").is_dir());
    let ca = config.game_path(combat_arms::game::GAME).filter(|p| p.join("Game").is_dir());
    if css.is_none() || ca.is_none() {
        eprintln!("skipping: needs both the CS:S and the Combat Arms installs");
        return None;
    }
    Some(mashup_demo::load_map(&format!("cs_source:{name}"), 0).unwrap())
}

fn sim(map: MapData) -> Sim {
    let mut sim = Sim::new(CaSim, (MapSimPlugin::new(map), mashup_demo::MashupPartsPlugin));
    sim.ticks(2);
    sim
}

/// A brush entity's world bounds, entity space.
fn bounds(e: &MapEntity) -> (Vec3, Vec3) {
    let rot = entity_rotation(e.angles());
    let pts = e.hulls.iter().flat_map(|h| &h.points).map(|p| rot * *p + e.origin());
    pts.fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| (lo.min(p), hi.max(p)))
}

/// Look from `p`'s eye at `target` (engine space).
fn aim(sim: &mut Sim, p: Entity, target: Vec3) {
    let eye = sim.position(p) + sim.state(p).eye_offset;
    let d = target - eye;
    let mut i = sim.intent(p);
    i.yaw = (-d.x).atan2(-d.z);
    i.pitch = d.y.atan2(d.xz().length());
}

/// The logic world's entity of map entity `index`.
fn logic_class(sim: &Sim, index: usize) -> Option<Class> {
    let logic = sim.app.world().resource::<Logic>();
    let id = logic
        .world
        .ids()
        .into_iter()
        .find(|id| logic.world.get(*id).unwrap().map_index == Some(index))?;
    Some(logic.world.get(id).unwrap().class.clone())
}

fn active(sim: &Sim, p: Entity) -> Entity {
    sim.app.world().get::<Inventory>(p).unwrap().active.unwrap()
}

fn clip(sim: &Sim, p: Entity) -> u32 {
    sim.app.world().get::<Magazine>(active(sim, p)).unwrap().clip
}

/// How far (m) a ray from `from` to `to` gets, `skip` left out.
fn ray(sim: &mut Sim, from: Vec3, to: Vec3, skip: Entity) -> f32 {
    let world = sim.app.world_mut();
    let mut state = SystemState::<SpatialQuery>::new(world);
    let q = state.get(world).unwrap();
    let filter = SpatialQueryFilter::from_excluded_entities([skip]);
    q.cast_ray(from, Dir3::new(to - from).unwrap(), 100.0, true, &filter)
        .map_or(f32::MAX, |h| h.distance)
}

fn fire(sim: &mut Sim, p: Entity) {
    sim.intent(p).fire = true;
    sim.ticks(1);
    sim.intent(p).fire = false;
    sim.ticks(3);
}

#[test]
fn a_combat_arms_soldier_opens_nukes_doors_and_breaks_a_vent() {
    let Some(map) = load("de_nuke") else { return };
    let scale = map.entity_scale;
    let doors: Vec<usize> = (0..map.entities.len())
        .filter(|&i| map.entities[i].classname() == "func_door_rotating" && map.entities[i].mover)
        .collect();
    let vents: Vec<usize> = (0..map.entities.len())
        .filter(|&i| map.entities[i].classname() == "func_breakable" && map.entities[i].get("material") == Some("2"))
        .collect();
    assert!(!doors.is_empty() && !vents.is_empty(), "de_nuke's doors and vents");
    let entities = map.entities.clone();
    let mut sim = sim(map);
    let yaw = |sim: &Sim, i: usize| logic_class(sim, i).and_then(|c| pusher(&c).map(|p| p.angles.y));

    // A door: stand 40 units in front of it on its floor, look at its
    // middle, press +use.
    let mut opened = false;
    for &door in &doors {
        let (lo, hi) = bounds(&entities[door]);
        let (size, centre) = (hi - lo, (lo + hi) / 2.0);
        let out = if size.x < size.y { Vec3::X } else { Vec3::Y } * (size.x.min(size.y) / 2.0 + 40.0);
        for side in [out, -out] {
            let feet = Vec3::new(centre.x, centre.y, lo.z) + side;
            let p = sim.spawn_character(entity_to_engine(feet, scale) + Vec3::Y * 1.0, movement::ID);
            sim.seconds(1.0);
            aim(&mut sim, p, entity_to_engine(centre, scale));
            sim.ticks(2);
            let before = yaw(&sim, door).unwrap();
            sim.intent(p).use_key = true;
            sim.ticks(1);
            sim.intent(p).use_key = false;
            sim.seconds(1.0);
            let turned = (yaw(&sim, door).unwrap() - before).abs();
            sim.app.world_mut().entity_mut(p).despawn();
            if turned > 45.0 {
                eprintln!("de_nuke: door {door} turned {turned} degrees on a Combat Arms soldier's +use");
                opened = true;
                break;
            }
        }
        if opened {
            break;
        }
    }
    assert!(opened, "a Combat Arms soldier's +use opens one of de_nuke's {} doors", doors.len());

    // A vent: a floating soldier 24 units off it shoots it with the M4A1.
    let mut broke = false;
    for &vent in &vents {
        let (lo, hi) = bounds(&entities[vent]);
        let (size, centre) = (hi - lo, (lo + hi) / 2.0);
        let thin = if size.x < size.y { Vec3::X } else { Vec3::Y };
        let out = thin * (size.dot(thin) / 2.0 + 24.0);
        for eye in [centre + out, centre - out] {
            let (eye, target) = (entity_to_engine(eye, scale), entity_to_engine(centre, scale));
            let p = sim.spawn_character(eye, noclip::ID);
            sim.ticks(1);
            let offset = sim.state(p).eye_offset;
            sim.app.world_mut().get_mut::<Transform>(p).unwrap().translation = eye - offset;
            sim.seconds(1.5);
            aim(&mut sim, p, target);
            let clear = ray(&mut sim, eye, target, p);
            if clear > eye.distance(target) + 0.2 {
                sim.app.world_mut().entity_mut(p).despawn();
                continue;
            }
            assert_eq!(sim.app.world().get::<Weapon>(active(&sim, p)).unwrap().id, weapons::RIFLE);
            fire(&mut sim, p);
            let gone = match logic_class(&sim, vent) {
                None => true,
                Some(Class::Breakable(b)) => b.broken,
                Some(_) => false,
            };
            sim.app.world_mut().entity_mut(p).despawn();
            if gone {
                eprintln!("de_nuke: vent {vent} broken by the M4A1");
                broke = true;
                break;
            }
        }
        if broke {
            break;
        }
    }
    assert!(broke, "the M4A1 breaks one of de_nuke's metal vents");
    let world = sim.app.world_mut();
    let nodes = world.query::<&MapBrushEntity>().iter(world).count();
    assert!(nodes > 0);
}

/// Every world triangle's centre with its surface name and normal, engine
/// space.
fn triangles(map: &MapData) -> Vec<(String, Vec3, Vec3)> {
    let mut out = Vec::new();
    for m in map.meshes.iter().filter(|m| !m.skybox) {
        let Some(s) = &m.surface else { continue };
        for t in m.indices.chunks_exact(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(m.positions[i as usize]));
            let n = (b - a).cross(c - a);
            if n.length() < 0.05 {
                continue;
            }
            out.push((s.clone(), (a + b + c) / 3.0, n.normalize()));
        }
    }
    out
}

#[derive(Resource, Default)]
struct Heard(Vec<String>);

fn listen(mut sounds: MessageReader<CaSound>, mut heard: ResMut<Heard>) {
    heard.0.extend(sounds.read().map(|s| s.file.clone()));
}

#[test]
fn dust2s_surfaces_are_combat_arms_types() {
    let Some(map) = load("de_dust2") else { return };
    let tris = triangles(&map);
    let mut names: Vec<&str> = tris.iter().map(|(s, ..)| s.as_str()).collect();
    names.sort();
    names.dedup();
    eprintln!("de_dust2's surfaces: {names:?}");
    let mut sim = sim(map);
    let tables = CaFxTables::built_in();
    let id = |name: &str| tables.surface_id(name).unwrap();
    let names = sim.app.world().resource::<SurfaceNames>().clone();
    let grid = sim.app.world().resource::<SurfaceGrid>();
    for (want, from) in [("Desert_Sand", "sand"), ("Wood", "wood"), ("Cement", "concrete")] {
        let hits: Vec<&(String, Vec3, Vec3)> = tris
            .iter()
            .filter(|(s, ..)| names.get(s).is_some_and(|n| n == want) && s.contains(from))
            .collect();
        assert!(!hits.is_empty(), "dust2 has {from} surfaces ({want})");
        let right = hits
            .iter()
            .filter(|(_, at, _)| world_surface_named(Some(grid), Some(&names), *at) == id(want))
            .count();
        eprintln!("de_dust2: {right} of {} {from} triangles answer {want}", hits.len());
        assert!(right * 2 > hits.len(), "{from} reads as {want}: {right} of {}", hits.len());
    }

    // A round into the sand from above: Combat Arms' sand impact sound.
    let (_, at, _) = tris
        .iter()
        .find(|(s, _, n)| names.get(s) == Some("Desert_Sand") && n.y > 0.9)
        .cloned()
        .expect("a sand floor");
    sim.app.init_resource::<Heard>().add_systems(Last, listen);
    let eye = at + Vec3::new(0.3, 2.0, 0.0);
    let p = sim.spawn_character(eye, noclip::ID);
    sim.ticks(1);
    let offset = sim.state(p).eye_offset;
    sim.app.world_mut().get_mut::<Transform>(p).unwrap().translation = eye - offset;
    sim.seconds(1.5);
    aim(&mut sim, p, at);
    sim.app.world_mut().resource_mut::<Heard>().0.clear();
    fire(&mut sim, p);
    let heard = sim.app.world().resource::<Heard>().0.clone();
    let sand = tables.surface(id("Desert_Sand"));
    eprintln!("de_dust2: a round into the sand: {heard:?}");
    assert!(
        heard.iter().any(|f| sand.bullet.iter().any(|b| !b.is_empty() && b == f)),
        "the sand's bullet impact ({:?}) among {heard:?}",
        sand.bullet
    );
}

#[test]
fn a_combat_arms_soldier_fires_the_ak47_on_dust2() {
    let Some(map) = load("de_dust2") else { return };
    assert!(
        map.view_models.iter().any(|v| v.key == cs_source::game::weapons::AK47),
        "CS:S's AK-47 view model kept"
    );
    let mut sim = sim(map);
    {
        let mut kit = sim.app.world_mut().resource_mut::<StartingWeapons>();
        mashup_demo::set_slot(&mut kit, 0, cs_source::game::weapons::AK47).unwrap();
    }
    let spawn = {
        let w = sim.app.world_mut();
        let mut q = w.query::<(&Transform, &SpawnPoint)>();
        q.iter(w).next().map(|(t, _)| t.translation).unwrap()
    };
    let p = sim.spawn_character(spawn, movement::ID);
    sim.seconds(1.5);
    let w = active(&sim, p);
    assert_eq!(sim.app.world().get::<Weapon>(w).unwrap().id, cs_source::game::weapons::AK47);

    // A second soldier 3 m off in a clear direction, on the same floor.
    let eye = sim.position(p) + sim.state(p).eye_offset;
    let dir = (0..16)
        .map(|i| Quat::from_rotation_y(i as f32 * std::f32::consts::TAU / 16.0) * Vec3::NEG_Z)
        .find(|d| ray(&mut sim, eye, eye + *d, p) > 4.0)
        .expect("a clear direction");
    let target = sim.spawn_character(sim.position(p) + dir * 3.0, movement::ID);
    sim.seconds(1.0);
    let health = sim.app.world().get::<Health>(target).unwrap().current;
    assert!(health > 0.0, "the target stands: {:?}", sim.position(target));
    let chest = sim.position(target) + Vec3::Y * 0.3;
    aim(&mut sim, p, chest);
    let full = clip(&sim, p);
    fire(&mut sim, p);
    let left = clip(&sim, p);
    let now = sim.app.world().get::<Health>(target).unwrap().current;
    eprintln!("de_dust2: the AK-47 fired ({left} of {full} left); the target {health} -> {now}");
    assert_eq!(left + 1, full, "one round");
    assert!(now < health, "the AK-47's round hurt a Combat Arms soldier");
}
