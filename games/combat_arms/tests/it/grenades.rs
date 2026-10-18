//! specs/combat_arms/grenades.md's test cases, one test each (`t01_` is
//! test case 1), on `CaSim` with the game's grenades
//! (data/weapons/m67.toml and the others): an open world of our own (a
//! flat floor at y = 0 of hardness 0.65, walls, sky and water where a
//! case asks), and Combat Arms players (100 HP, armour points 0, no
//! gear) where a case has victims. Server frames are the spec's 20 Hz
//! (`CaServerFrames`); "after step n" reads the grenade (`CaGrenade`, world
//! units) after its nth physics step.
//!
//! The spec's +Z forward is ours too (the flight doesn't care which way it
//! goes); world units are the data's (`inventory::UNIT`). Tolerances: the spec's ±0.05 u and u/s for
//! flight, 0.01 HP for damage, 1e-3 s for flash times and 1e-4 for the
//! screen's whiteness; "about" (over-time damage) ±1 HP. Times: a tick
//! (15 ms) beyond the server frame the spec names.
//!
//! Cases 6-8 are the bounce rule itself (`grenades::bounce`), the velocity
//! at contact being their premise. Case 25's bot and name tag: nothing in
//! the world blocks sight (no `SightBlocker`), which is what both read.
//! The last test is the network's: a thrown grenade seen and damaging on
//! both clients.

use std::time::Duration;

use avian3d::prelude::*;
use bevy::prelude::*;
use chimera_core::core::{
    Blinded, Damage, DamageKind, FriendlyFire, Health, Hitgroup, Intent, MovementState, PropSurface,
    SightBlocker, Team,
};
use chimera_net::{harness::NetSim, net::memory::LinkConditions, net::predict::NetGraph};
use chimera_sim::weapon::{
    self, Inventory, Weapon,
    data::WeaponDataAppExt,
    grenade::{Detonated, GrenadeKind, Throwable},
};
use chimera_world::{greybox::GreyboxMapPlugin, harness::Sim, map::loose::ShownItem};
use combat_arms::{
    game::{
        TICK_INTERVAL,
        grenades::{
            self, CaFlashProtection, CaFlashed, CaGrenade, CaLiquid, CaServerFrames, G7A, M18, M23F, M67, XM84,
            bounce, flash_white,
        },
        movement,
        weapons::{CaArmor, DEFAULT_BODY_GEAR, MALE, body_geometry},
    },
    harness::CaSim,
};

/// Metres per world unit (the data's).
const U: f32 = combat_arms::game::inventory::UNIT;
/// Health per hit point.
const HP: f32 = 0.01;

/// The M67 with no spread (case 26: the throw's direction exactly the
/// view's).
const M67_EXACT: &str = r#"
id = "test:m67_exact"
base = "combat_arms:m67"
[hooks."combat_arms:grenade"]
perturb = [0, 0]
"#;

/// Detonations seen: (server frame, weapon, kind, where in u).
#[derive(Resource, Default)]
struct Booms(Vec<(u64, &'static str, GrenadeKind, Vec3)>);

fn log_booms(mut d: MessageReader<Detonated>, frames: Res<CaServerFrames>, mut log: ResMut<Booms>) {
    for d in d.read() {
        log.0.push((frames.frame, d.weapon, d.kind, d.at / U));
    }
}

/// An open world: a floor at y = 0 (the grenade surfaces' default
/// hardness), nothing else.
fn sim() -> Sim {
    let mut sim = Sim::with(CaSim, |app| {
        app.init_resource::<Booms>().add_systems(FixedPostUpdate, log_booms);
    });
    sim.set_tick_interval(TICK_INTERVAL);
    sim.app.add_weapon_data(&[("m67_exact", M67_EXACT)]).unwrap();
    solid(&mut sim, Vec3::new(-200.0, -1.0, -200.0), Vec3::new(200.0, 0.0, 200.0), None);
    sim.ticks(1);
    sim
}

/// A static box from `min` to `max` (m), with a surface name.
fn solid(sim: &mut Sim, min: Vec3, max: Vec3, surface: Option<&str>) -> Entity {
    let size = max - min;
    let mut e = sim.app.world_mut().spawn((
        RigidBody::Static,
        Collider::cuboid(size.x, size.y, size.z),
        Transform::from_translation((min + max) / 2.0),
    ));
    if let Some(s) = surface {
        e.insert(PropSurface(s.into()));
    }
    let e = e.id();
    sim.ticks(1);
    e
}

/// A weapon of `id` to make grenades from, owned by `owner` (a placeholder
/// when none).
fn weapon_of(sim: &mut Sim, id: &str, owner: Option<Entity>) -> Entity {
    weapon::spawn_weapon(sim.app.world_mut(), id, owner.unwrap_or(Entity::PLACEHOLDER)).unwrap()
}

/// A live grenade of `id` at `at` (u) moving at `v` (u/s), thrown by
/// `thrower`.
fn grenade(sim: &mut Sim, id: &str, thrower: Option<Entity>, at: Vec3, v: Vec3) -> Entity {
    let w = weapon_of(sim, id, thrower);
    grenades::spawn_grenade(sim.app.world_mut(), w, thrower, at * U, v * U).unwrap()
}

fn state(sim: &Sim, g: Entity) -> CaGrenade {
    sim.app.world().get::<CaGrenade>(g).expect("grenade gone").clone()
}

/// Run until `g` has run `n` physics steps.
fn to_step(sim: &mut Sim, g: Entity, n: u32) -> CaGrenade {
    for _ in 0..2000 {
        let s = state(sim, g);
        if s.steps >= n {
            assert_eq!(s.steps, n, "stepped past {n}");
            return s;
        }
        sim.ticks(1);
    }
    panic!("no step {n}");
}

fn near(a: Vec3, b: Vec3, eps: f32) -> bool {
    (a - b).abs().max_element() <= eps
}

#[track_caller]
fn assert_near(a: Vec3, b: Vec3, eps: f32, what: &str) {
    assert!(near(a, b, eps), "{what}: {a} vs {b}");
}

fn booms(sim: &Sim) -> Vec<(u64, &'static str, GrenadeKind, Vec3)> {
    sim.app.world().resource::<Booms>().0.clone()
}

/// Run until something detonates (or `limit` seconds).
fn until_boom(sim: &mut Sim, limit: f64) -> (u64, &'static str, GrenadeKind, Vec3) {
    let start = booms(sim).len();
    for _ in 0..((limit / TICK_INTERVAL) as u64 + 2) {
        sim.ticks(1);
        if let Some(b) = booms(sim).get(start) {
            return *b;
        }
    }
    panic!("no detonation in {limit} s");
}

// --- Players

/// A Combat Arms player standing at `x`, `z` (u) facing `yaw`, 100 HP,
/// no armour, on `team`.
fn player(sim: &mut Sim, x: f32, z: f32, yaw: f32, team: u8) -> Entity {
    let p = sim.spawn_character(Vec3::new(x * U, 1.0, z * U), movement::ID);
    sim.intent(p).yaw = yaw;
    sim.app.world_mut().entity_mut(p).insert(Team(team));
    sim.seconds(0.5);
    sim.app.world_mut().entity_mut(p).insert((
        Health { current: 1.0, max: 1.0 },
        CaArmor {
            points: 0.0,
            gear: Vec::new(),
        },
    ));
    p
}

/// A player's centre and head top (m), as blasts see them.
fn body(sim: &mut Sim, p: Entity) -> (Vec3, f32) {
    let world = sim.app.world_mut();
    let mut q = world.query::<(&Transform, &Intent, &MovementState, Option<&ColliderAabb>)>();
    let (t, i, s, a) = q.get(world, p).unwrap();
    let (min, max, spheres) = body_geometry(t, i, s, a, MALE).unwrap();
    let top = MALE
        .iter()
        .zip(&spheres)
        .filter(|(s, _)| s.location == combat_arms::game::weapons::Location::Head)
        .map(|(_, (c, r))| c.y + r)
        .fold(f32::MIN, f32::max);
    ((min + max) / 2.0, top)
}

fn health(sim: &Sim, p: Entity) -> f32 {
    sim.app.world().get::<Health>(p).unwrap().current / HP
}

/// A grenade of `id` that goes off at `at` (m) on the next server frame
/// but one, without moving (its fuse set to 0).
fn explode_at(sim: &mut Sim, id: &str, thrower: Option<Entity>, at: Vec3) {
    let g = grenade(sim, id, thrower, at / U, Vec3::ZERO);
    sim.app.world_mut().get_mut::<CaGrenade>(g).unwrap().params.fuse = 0.0;
    until_boom(sim, 1.0);
    // Damage lands on the tick it is dealt; flash timers run on.
    sim.ticks(1);
}

/// The point `d` u from the victim's centre in the direction it faces
/// (`ahead`) or behind it, at its centre's height.
fn in_front(sim: &mut Sim, p: Entity, d: f32, ahead: bool) -> Vec3 {
    let (c, _) = body(sim, p);
    let yaw = sim.app.world().get::<Intent>(p).unwrap().yaw;
    let f = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
    c + f * d * U * if ahead { 1.0 } else { -1.0 }
}

/// A thrower out of every blast's reach, on `team`.
fn far_thrower(sim: &mut Sim, team: u8) -> Entity {
    player(sim, 5000.0, 5000.0, 0.0, team)
}

// ---------------------------------------------------------------------------
// Flight (1-8)

#[test]
fn t01_level_throw() {
    let mut sim = sim();
    let g = grenade(&mut sim, M67, None, Vec3::new(0.0, 60.0, 0.0), Vec3::new(0.0, 0.0, 1550.0));
    let s = to_step(&mut sim, g, 1);
    assert_near(s.pos, Vec3::new(0.0, 59.0, 77.5), 0.05, "step 1");
    assert_near(s.velocity, Vec3::new(0.0, -80.0, 1550.0), 0.05, "step 1 v");
    let s = to_step(&mut sim, g, 5);
    assert_near(s.pos, Vec3::new(0.0, 15.0, 387.5), 0.05, "step 5");
    let s = to_step(&mut sim, g, 6);
    let c = s.contacts[0];
    assert_eq!(c.step, 6);
    assert_near(c.at, Vec3::new(0.0, 0.0, 442.86), 0.05, "contact");
    assert_near(c.velocity, Vec3::new(0.0, 311.94, 1007.5), 0.05, "bounce");
}

#[test]
fn t02_same_throw_later() {
    let mut sim = sim();
    let g = grenade(&mut sim, M67, None, Vec3::new(0.0, 60.0, 0.0), Vec3::new(0.0, 0.0, 1550.0));
    let s = to_step(&mut sim, g, 49);
    let floors: Vec<(u32, f32)> = s.contacts.iter().map(|c| (c.step, c.at.z)).collect();
    let want = [
        (6, 442.86),
        (15, 859.68),
        (23, 1092.65),
        (30, 1232.09),
        (36, 1309.53),
        (41, 1354.01),
        (46, 1377.87),
    ];
    assert_eq!(floors.len(), want.len(), "{floors:?}");
    for ((step, z), (ws, wz)) in floors.iter().zip(want) {
        assert_eq!(*step, ws, "{floors:?}");
        assert!((z - wz).abs() <= 0.05, "step {step}: z {z} vs {wz}");
    }
    assert!(s.contacts.iter().all(|c| !c.rest) && !s.resting, "never at rest");
    let created = s.created.unwrap();
    let (at_frame, weapon, kind, at) = until_boom(&mut sim, 1.0);
    assert_eq!((weapon, kind), (M67, GrenadeKind::Blast));
    assert_eq!(at_frame - created, 50, "the start of step 50");
    assert_near(at, Vec3::new(0.0, 9.95, 1389.27), 0.05, "where");
}

#[test]
fn t03_dropped_straight_down() {
    let mut sim = sim();
    let g = grenade(&mut sim, M67, None, Vec3::new(0.0, 60.0, 0.0), Vec3::ZERO);
    for (n, y) in [(1, 59.0), (2, 54.0), (3, 45.0), (4, 32.0), (5, 15.0)] {
        let s = to_step(&mut sim, g, n);
        assert_near(s.pos, Vec3::new(0.0, y, 0.0), 0.05, &format!("step {n}"));
    }
    let s = to_step(&mut sim, g, 6);
    let c = s.contacts[0];
    assert!(c.rest && s.resting && s.velocity == Vec3::ZERO);
    assert_near(c.at, Vec3::ZERO, 0.05, "contact");
    // Then only the one-time nudge (half-height x 0.65, up).
    let created = s.created.unwrap();
    sim.seconds(1.0);
    let s = state(&sim, g);
    assert_eq!(s.steps, 6, "no more steps at rest");
    assert_near(s.pos, Vec3::new(0.0, 2.0 * 0.65, 0.0), 0.05, "nudged");
    let (f, ..) = until_boom(&mut sim, 2.0);
    assert_eq!(f - created, 50, "at 2.5 s");
}

#[test]
fn t04_point_blank_wall() {
    let mut sim = sim();
    solid(&mut sim, Vec3::new(-10.0, -1.0, 50.0 * U), Vec3::new(10.0, 10.0, 2.0), None);
    let g = grenade(&mut sim, M67, None, Vec3::new(0.0, 60.0, 0.0), Vec3::new(0.0, 0.0, 1550.0));
    let s = to_step(&mut sim, g, 1);
    let c = s.contacts[0];
    assert_near(c.at, Vec3::new(0.0, 59.35, 50.0), 0.05, "wall");
    assert_near(c.velocity, Vec3::new(0.0, -52.0, -1007.44), 0.05, "off the wall");
    let s = to_step(&mut sim, g, 7);
    let c = s.contacts[1];
    assert_eq!(c.step, 7);
    assert!((c.at.z - -204.75).abs() <= 0.05 && c.at.y.abs() <= 0.05, "{c:?}");
    let s = to_step(&mut sim, g, 48);
    let last = s.contacts.last().unwrap();
    assert!(last.rest && last.step == 48, "{last:?}");
    assert_near(last.at, Vec3::new(0.0, 0.0, -852.27), 0.05, "rest");
}

#[test]
fn t05_45_degree_throw() {
    let mut sim = sim();
    let g = grenade(&mut sim, M67, None, Vec3::new(0.0, 60.0, 0.0), Vec3::new(0.0, 1096.02, 1096.02));
    let s = to_step(&mut sim, g, 1);
    assert_near(s.pos, Vec3::new(0.0, 113.8, 54.8), 0.05, "step 1");
    let s = to_step(&mut sim, g, 10);
    assert_near(s.pos, Vec3::new(0.0, 418.01, 548.01), 0.05, "step 10");
    let s = to_step(&mut sim, g, 29);
    let c = s.contacts[0];
    assert_eq!(c.step, 29);
    assert!((c.at.z - 1585.67).abs() <= 0.05, "{c:?}");
    assert_near(c.velocity, Vec3::new(0.0, 795.52, 712.41), 0.05, "bounce");
}

#[test]
fn t06_rest_threshold() {
    let (v, rest) = bounce(Vec3::new(76.0, -480.0, 0.0), Vec3::Y, 0.65);
    assert!(rest && v == Vec3::ZERO, "49.4 < 50");
    let (v, rest) = bounce(Vec3::new(78.0, -480.0, 0.0), Vec3::Y, 0.65);
    assert!(!rest);
    assert_near(v, Vec3::new(50.7, 311.94, 0.0), 0.05, "bounces");
}

#[test]
fn t07_no_rest_on_walls() {
    let (v, rest) = bounce(Vec3::new(-20.0, -30.0, 0.0), Vec3::X, 0.65);
    assert!(!rest);
    assert_near(v, Vec3::new(12.94, -19.5, 0.0), 0.05, "wall");
}

#[test]
fn t08_soft_surface() {
    let sand = grenades::CaGrenadeSurfaces::built_in().hardness(Some("desert sand"));
    let (v, rest) = bounce(Vec3::new(600.0, -480.0, 0.0), Vec3::Y, sand);
    assert!(!rest);
    assert_near(v, Vec3::new(60.0, 47.99, 0.0), 0.05, "sand");
    // And on a sand floor in the world: a tenth of the speed kept.
    let mut sim = sim();
    solid(&mut sim, Vec3::new(-10.0, -0.5, -10.0), Vec3::new(10.0, 0.001, 10.0), Some("desert sand"));
    let g = grenade(&mut sim, M67, None, Vec3::new(0.0, 60.0, 0.0), Vec3::new(600.0, 0.0, 0.0));
    let s = to_step(&mut sim, g, 6);
    let c = s.contacts[0];
    assert!((c.velocity.x - 60.0).abs() <= 0.05, "{c:?}");
}

// ---------------------------------------------------------------------------
// Blasts (9-14)

#[test]
fn t09_frag_close() {
    let mut sim = sim();
    let thrower = far_thrower(&mut sim, 0);
    let victim = player(&mut sim, 0.0, 0.0, 0.0, 0);
    let at = in_front(&mut sim, victim, 100.0, true);
    explode_at(&mut sim, M67, Some(thrower), at);
    assert_eq!(health(&sim, victim), 0.0, "120 >= 99: dead");
    // The thrower itself.
    let mut sim = self::sim();
    let thrower = player(&mut sim, 0.0, 0.0, 0.0, 0);
    let at = in_front(&mut sim, thrower, 100.0, true);
    explode_at(&mut sim, M67, Some(thrower), at);
    assert_eq!(health(&sim, thrower), 0.0, "the thrower dies");
}

#[test]
fn t10_frag_mid() {
    let mut sim = sim();
    let thrower = far_thrower(&mut sim, 0);
    let victim = player(&mut sim, 0.0, 0.0, 0.0, 0);
    let at = in_front(&mut sim, victim, 300.0, true);
    explode_at(&mut sim, M67, Some(thrower), at);
    assert!((100.0 - health(&sim, victim) - 53.33).abs() < 0.01, "{}", health(&sim, victim));
}

#[test]
fn t11_frag_edge() {
    for d in [450.0, 460.0] {
        let mut sim = sim();
        let thrower = far_thrower(&mut sim, 0);
        let victim = player(&mut sim, 0.0, 0.0, 0.0, 0);
        let at = in_front(&mut sim, victim, d, true);
        explode_at(&mut sim, M67, Some(thrower), at);
        assert_eq!(health(&sim, victim), 100.0, "{d} u");
    }
}

#[test]
fn t12_frag_cover() {
    // Floor to ceiling: nothing.
    let mut sim = sim();
    let thrower = far_thrower(&mut sim, 0);
    let victim = player(&mut sim, 0.0, 0.0, 0.0, 0);
    let at = in_front(&mut sim, victim, 100.0, true);
    let mid = (at + body(&mut sim, victim).0) / 2.0;
    solid(&mut sim, Vec3::new(-2.0, 0.0, mid.z - 0.05), Vec3::new(2.0, 4.0, mid.z + 0.05), None);
    explode_at(&mut sim, M67, Some(thrower), at);
    assert_eq!(health(&sim, victim), 100.0, "covered");
    // Waist high: the centre hidden, the head top seen.
    let mut sim = self::sim();
    let thrower = far_thrower(&mut sim, 0);
    let victim = player(&mut sim, 0.0, 0.0, 0.0, 0);
    let at = in_front(&mut sim, victim, 100.0, true);
    let (centre, top) = body(&mut sim, victim);
    let mid = (at + centre) / 2.0;
    let wall_top = centre.y + 0.25 * (top - centre.y);
    solid(&mut sim, Vec3::new(-2.0, 0.0, mid.z - 0.05), Vec3::new(2.0, wall_top, mid.z + 0.05), None);
    explode_at(&mut sim, M67, Some(thrower), at);
    assert_eq!(health(&sim, victim), 0.0, "120: dead");
}

#[test]
fn t13_frag_vs_armour() {
    let mut sim = sim();
    let thrower = far_thrower(&mut sim, 0);
    let victim = player(&mut sim, 0.0, 0.0, 0.0, 0);
    sim.app.world_mut().entity_mut(victim).insert(CaArmor {
        points: 100.0,
        gear: vec![DEFAULT_BODY_GEAR],
    });
    let at = in_front(&mut sim, victim, 300.0, true);
    explode_at(&mut sim, M67, Some(thrower), at);
    assert!((health(&sim, victim) - 46.67).abs() < 0.01, "{}", health(&sim, victim));
    assert_eq!(sim.app.world().get::<CaArmor>(victim).unwrap().points, 100.0);
}

#[test]
fn t14_friendly_fire() {
    for (ff, want) in [(0, 100.0), (1, 0.0)] {
        let mut sim = sim();
        sim.app.insert_resource(FriendlyFire(ff));
        let thrower = far_thrower(&mut sim, 1);
        let mate = player(&mut sim, 0.0, 0.0, 0.0, 1);
        let at = in_front(&mut sim, mate, 100.0, true);
        explode_at(&mut sim, M67, Some(thrower), at);
        assert_eq!(health(&sim, mate), want, "friendly fire {ff}");
    }
}

// ---------------------------------------------------------------------------
// Flashes (15-23)

/// Flash `victim` with an XM84 at `d` u ahead of or behind it, thrown by
/// `thrower`; run 0.5 s. The flash it got: (time T, delay from the blast).
fn flashed(sim: &mut Sim, victim: Entity, thrower: Entity, d: f32, ahead: bool) -> Option<(f32, f64)> {
    sim.app.world_mut().entity_mut(victim).remove::<CaFlashed>();
    let at = in_front(sim, victim, d, ahead);
    let g = grenade(sim, XM84, Some(thrower), at / U, Vec3::ZERO);
    sim.app.world_mut().get_mut::<CaGrenade>(g).unwrap().params.fuse = 0.0;
    until_boom(sim, 1.0);
    let blast = sim.app.world().resource::<chimera_core::core::SimClock>().now;
    sim.seconds(0.5);
    sim.app.world().get::<CaFlashed>(victim).map(|f| (f.time, f.at - blast))
}

#[test]
fn t15_flash_facing_close() {
    let mut sim = sim();
    let thrower = far_thrower(&mut sim, 0);
    let victim = player(&mut sim, 0.0, 0.0, 0.0, 0);
    let (time, delay) = flashed(&mut sim, victim, thrower, 200.0, true).expect("flashed");
    assert!((time - 25.0).abs() < 1e-3, "{time}");
    // On the server frame at or after 0.25 s (ticks of 15 ms on frames of
    // 50 ms: a tick either way).
    assert!((0.25 - 0.016..0.25 + 0.05 + 0.016).contains(&delay), "sent {delay} s after");
    // The client's screen.
    for (t, w) in [(0.0, 1.0), (6.25, 0.7071), (8.333, 0.5), (12.5, 0.0)] {
        assert!((flash_white(t, time) - w).abs() < 1e-3, "w({t})");
    }
    let b = *sim.app.world().get::<Blinded>(victim).unwrap();
    assert!((b.end - b.fade_start - 12.5).abs() < 1e-3, "gone at 12.5 s");
    assert!((grenades::blinded_white(&b, b.fade_start + 6.25) - 0.7071).abs() < 1e-3);
    assert_eq!(health(&sim, victim), 100.0, "22: no damage");
}

#[test]
fn t16_flash_facing_mid() {
    let mut sim = sim();
    let thrower = far_thrower(&mut sim, 0);
    let victim = player(&mut sim, 0.0, 0.0, 0.0, 0);
    let (time, _) = flashed(&mut sim, victim, thrower, 625.0, true).expect("flashed");
    assert!((time - 12.5).abs() < 1e-3, "{time}");
    assert!((flash_white(3.125, time) - 0.7071).abs() < 1e-4);
    assert_eq!(flash_white(6.25, time), 0.0);
    assert_eq!(health(&sim, victim), 100.0, "22: no damage");
}

#[test]
fn t17_flash_facing_away() {
    let mut sim = sim();
    let thrower = far_thrower(&mut sim, 0);
    let victim = player(&mut sim, 0.0, 0.0, 0.0, 0);
    let (time, delay) = flashed(&mut sim, victim, thrower, 200.0, false).expect("flashed");
    assert!((time - 6.25).abs() < 1e-3, "{time}");
    assert!((0.0833 - 0.016..0.0833 + 0.05 + 0.016).contains(&delay), "sent {delay} s after");
    assert!((flash_white(1.5625, time) - 0.7071).abs() < 1e-4);
    assert_eq!(flash_white(3.125, time), 0.0);
    assert_eq!(health(&sim, victim), 100.0, "22: no damage");
}

#[test]
fn t18_flash_gear() {
    let mut sim = sim();
    let thrower = far_thrower(&mut sim, 0);
    let victim = player(&mut sim, 0.0, 0.0, 0.0, 0);
    sim.app.world_mut().entity_mut(victim).insert(CaFlashProtection(1.0));
    assert_eq!(flashed(&mut sim, victim, thrower, 200.0, true), None, "full goggles");
    sim.app.world_mut().entity_mut(victim).insert(CaFlashProtection(0.3));
    let (time, _) = flashed(&mut sim, victim, thrower, 200.0, true).expect("flashed");
    assert!((time - 17.5).abs() < 1e-3, "{time}");
    assert_eq!(flash_white(8.75, time), 0.0, "ends at 8.75 s");
    assert_eq!(health(&sim, victim), 100.0, "22: no damage");
}

#[test]
fn t19_flash_range() {
    let mut sim = sim();
    let thrower = far_thrower(&mut sim, 0);
    let victim = player(&mut sim, 0.0, 0.0, 0.0, 0);
    let (time, _) = flashed(&mut sim, victim, thrower, 990.0, true).expect("flashed");
    assert!((time - 0.3333).abs() < 1e-3, "{time}");
    assert_eq!(flash_white(0.1667, time), 0.0, "ends at 0.167 s");
    assert_eq!(flashed(&mut sim, victim, thrower, 998.0, true), None, "0.067 s: not sent");
    assert_eq!(flashed(&mut sim, victim, thrower, 1000.0, true), None);
    assert_eq!(health(&sim, victim), 100.0, "22: no damage");
}

#[test]
fn t20_flash_cover() {
    let mut sim = sim();
    let thrower = far_thrower(&mut sim, 0);
    let victim = player(&mut sim, 0.0, 0.0, 0.0, 0);
    let at = in_front(&mut sim, victim, 200.0, true);
    let mid = (at + body(&mut sim, victim).0) / 2.0;
    solid(&mut sim, Vec3::new(-2.0, 0.0, mid.z - 0.05), Vec3::new(2.0, 4.0, mid.z + 0.05), None);
    assert_eq!(flashed(&mut sim, victim, thrower, 200.0, true), None);
    assert_eq!(health(&sim, victim), 100.0, "22: no damage");
}

#[test]
fn t21_flash_self_and_team() {
    let mut sim = sim();
    let thrower = player(&mut sim, 0.0, 0.0, 0.0, 1);
    let (time, _) = flashed(&mut sim, thrower, thrower, 200.0, true).expect("the thrower");
    assert!((time - 25.0).abs() < 1e-3);
    for (ff, want) in [(0, None), (1, Some(25.0))] {
        let mut sim = self::sim();
        sim.app.insert_resource(FriendlyFire(ff));
        let thrower = far_thrower(&mut sim, 1);
        let mate = player(&mut sim, 0.0, 0.0, 0.0, 1);
        let got = flashed(&mut sim, mate, thrower, 200.0, true).map(|(t, _)| (t * 1000.0).round() / 1000.0);
        assert_eq!(got, want, "friendly fire {ff}");
        assert_eq!(health(&sim, mate), 100.0, "22: no damage");
    }
}

#[test]
fn t22_flash_no_damage() {
    // Checked in each of 15-21: hit points unchanged; and no damage at all.
    let mut sim = sim();
    let thrower = far_thrower(&mut sim, 0);
    let victim = player(&mut sim, 0.0, 0.0, 0.0, 0);
    flashed(&mut sim, victim, thrower, 50.0, true).expect("flashed");
    assert_eq!(health(&sim, victim), 100.0);
}

#[test]
fn t23_two_flashes() {
    let mut sim = sim();
    let thrower = far_thrower(&mut sim, 0);
    let victim = player(&mut sim, 0.0, 0.0, 0.0, 0);
    let (first, _) = flashed(&mut sim, victim, thrower, 200.0, true).expect("first");
    assert!((first - 25.0).abs() < 1e-3);
    sim.seconds(0.5);
    let (second, _) = flashed(&mut sim, victim, thrower, 625.0, true).expect("second");
    assert!((second - 12.5).abs() < 1e-3, "replaced by the weaker one");
    let b = *sim.app.world().get::<Blinded>(victim).unwrap();
    let f = *sim.app.world().get::<CaFlashed>(victim).unwrap();
    assert_eq!(b.fade_start, f.at, "white again from the second");
    assert!((b.end - b.fade_start - 6.25).abs() < 1e-3, "ends 6.25 s later");
}

// ---------------------------------------------------------------------------
// Fuses, smoke, the throw (24-28)

#[test]
fn t24_fuses() {
    for (id, fuse) in [(M67, 2.5), (XM84, 1.5), (M18, 3.0)] {
        let mut sim = sim();
        let g = grenade(&mut sim, id, None, Vec3::new(0.0, 20000.0, 0.0), Vec3::ZERO);
        sim.ticks(5);
        let created = state(&sim, g).created.expect("started");
        let (f, weapon, ..) = until_boom(&mut sim, 4.0);
        assert_eq!(weapon, id);
        assert_eq!(f - created, (fuse / 0.05f64).round() as u64, "{id}");
    }
}

#[test]
fn t25_smoke() {
    let mut sim = sim();
    let thrower = far_thrower(&mut sim, 0);
    let near_by = player(&mut sim, 0.0, 0.0, 0.0, 0);
    let at = in_front(&mut sim, near_by, 100.0, true);
    explode_at(&mut sim, M18, Some(thrower), at);
    sim.seconds(1.0);
    assert_eq!(health(&sim, near_by), 100.0, "no damage");
    let world = sim.app.world_mut();
    assert_eq!(world.query::<&SightBlocker>().iter(world).count(), 0, "nothing blocks sight (bots, name tags)");
    assert_eq!(world.query::<&chimera_sim::weapon::grenade::SmokeCloud>().iter(world).count(), 1, "the clients' cloud");
    // A bullet through the cloud hits the target behind it.
    let shooter = player(&mut sim, 0.0, 400.0, 0.0, 0);
    let target = player(&mut sim, 0.0, -400.0, 0.0, 0);
    sim.app.world_mut().entity_mut(near_by).despawn();
    let rifle = weapon::give(sim.app.world_mut(), shooter, combat_arms::game::weapons::RIFLE).unwrap();
    sim.seconds(1.5);
    assert_eq!(sim.app.world().get::<Inventory>(shooter).unwrap().active, Some(rifle));
    sim.intent(shooter).fire = true;
    sim.ticks(1);
    sim.intent(shooter).fire = false;
    sim.ticks(5);
    assert!(health(&sim, target) < 100.0, "hit through the smoke");
}

/// A player at the origin facing +Z (yaw π: ours faces −Z at 0) holding a
/// grenade of `id`, drawn.
fn holding(sim: &mut Sim, id: &str) -> (Entity, Entity) {
    let p = player(sim, 0.0, 0.0, std::f32::consts::PI, 0);
    let w = weapon::give(sim.app.world_mut(), p, id).unwrap();
    {
        let mut inv = sim.app.world_mut().get_mut::<Inventory>(p).unwrap();
        inv.wanted = Some(w);
    }
    sim.seconds(1.0);
    assert_eq!(sim.app.world().get::<Inventory>(p).unwrap().active, Some(w), "in hand");
    (p, w)
}

fn live(sim: &mut Sim) -> Vec<Entity> {
    let world = sim.app.world_mut();
    world.query_filtered::<Entity, With<CaGrenade>>().iter(world).collect()
}

#[test]
fn t26_no_thrower_velocity() {
    let mut sim = sim();
    let (p, _) = holding(&mut sim, "test:m67_exact");
    sim.intent(p).move_axis = Vec2::new(0.0, 1.0);
    sim.seconds(1.0);
    let v = sim.velocity(p);
    assert!(v.length() > 2.0, "moving: {v}");
    sim.intent(p).fire = true;
    for _ in 0..200 {
        sim.ticks(1);
        if !live(&mut sim).is_empty() {
            break;
        }
    }
    let g = live(&mut sim)[0];
    let s = state(&sim, g);
    assert_near(s.velocity, Vec3::new(0.0, 0.0, 1550.0), 0.05, "the view's direction at 1550, nothing carried");
    let start = s.pos;
    let s = to_step(&mut sim, g, 1);
    assert_near(s.pos - start, Vec3::new(0.0, -1.0, 77.5), 0.05, "test 1's step");
}

#[test]
fn t27_no_cooking() {
    let mut sim = sim();
    let (p, _) = holding(&mut sim, M67);
    let t0 = sim.tick();
    sim.intent(p).fire = true;
    let mut out = None;
    for _ in 0..200 {
        sim.ticks(1);
        if let Some(g) = live(&mut sim).first() {
            out = Some((*g, (sim.tick() - t0) as f64 * TICK_INTERVAL));
            break;
        }
    }
    let (g, left) = out.expect("thrown while held");
    // Pressed on tick t0 + 1: the release at 767 ms, the grenade a tick
    // after it.
    let since_press = left - TICK_INTERVAL;
    assert!(since_press >= 0.767 - 1e-6 && since_press <= 0.767 + 2.0 * TICK_INTERVAL, "left at {since_press}");
    sim.ticks(5);
    let created = state(&sim, g).created.unwrap();
    // Held 3 s in all; the fuse counts from the server's frame.
    let (f, ..) = until_boom(&mut sim, 3.5);
    assert_eq!(f - created, 50);
    sim.intent(p).fire = false;
}

#[test]
fn t28_switch_mid_throw() {
    let mut sim = sim();
    let (p, w) = holding(&mut sim, M67);
    let rifle = weapon::give(sim.app.world_mut(), p, combat_arms::game::weapons::RIFLE).unwrap();
    sim.intent(p).fire = true;
    sim.ticks(1);
    sim.intent(p).fire = false;
    sim.seconds(0.3);
    {
        let mut inv = sim.app.world_mut().get_mut::<Inventory>(p).unwrap();
        inv.wanted = Some(rifle);
    }
    sim.seconds(2.0);
    assert!(live(&mut sim).is_empty(), "no grenade");
    assert!(booms(&sim).is_empty());
    assert_eq!(sim.app.world().get::<Throwable>(w).unwrap().count, 1, "total stays 1");
}

// ---------------------------------------------------------------------------
// Death (29, 30), sky (31), chains (32)

fn kill(sim: &mut Sim, p: Entity) {
    let at = sim.position(p);
    sim.app.world_mut().write_message(Damage {
        target: p,
        attacker: None,
        amount: 10.0,
        point: at,
        dir: Vec3::NEG_Y,
        hitgroup: Hitgroup::Generic,
        kind: DamageKind::Generic,
        weapon: None,
        force: Vec3::ZERO,
    });
    sim.ticks(1);
}

#[test]
fn t29_death_after_the_pin_pull() {
    // The release: a live M67 at the hull centre, 500 u/s along the body's
    // facing, full fuse, the ammo untouched.
    let mut sim = sim();
    let (p, w) = holding(&mut sim, M67);
    sim.intent(p).fire = true;
    sim.seconds(0.72);
    assert!(sim.app.world().get::<grenades::CaPin>(w).unwrap().on, "pin pulled");
    let (centre, _) = body(&mut sim, p);
    kill(&mut sim, p);
    let g = live(&mut sim)[0];
    let s = state(&sim, g);
    assert_near(s.pos, centre / U, 0.05, "at the hull centre");
    assert_near(s.velocity, Vec3::new(0.0, 0.0, 500.0), 0.05, "500 u/s along the facing");
    assert_eq!(s.thrower, Some(p), "credited to the dead");
    assert_eq!(sim.app.world().get::<Throwable>(w).unwrap().count, 1, "ammo unchecked");
    sim.ticks(5);
    let created = state(&sim, g).created.unwrap();
    let (f, weapon, ..) = until_boom(&mut sim, 3.0);
    assert_eq!((weapon, f - created), (M67, 50), "2.5 s fuse");
    // Its flight from the spec's (0, 40, 0).
    let mut sim = self::sim();
    let g = grenade(&mut sim, M67, None, Vec3::new(0.0, 40.0, 0.0), Vec3::new(0.0, 0.0, 500.0));
    let s = to_step(&mut sim, g, 5);
    let c = s.contacts[0];
    assert!(c.step == 5 && (c.at.z - 117.65).abs() <= 0.05, "{c:?}");
    let s = to_step(&mut sim, g, 36);
    let last = s.contacts.last().unwrap();
    assert!(last.rest && last.step == 36, "{last:?}");
    assert_near(last.at, Vec3::new(0.0, 0.0, 361.19), 0.05, "rest");
}

#[test]
fn t30_death_during_the_pin_pull() {
    let mut sim = sim();
    let (p, w) = holding(&mut sim, M67);
    sim.intent(p).fire = true;
    sim.seconds(0.1);
    kill(&mut sim, p);
    sim.seconds(1.5);
    assert!(live(&mut sim).is_empty(), "no grenade");
    assert_eq!(sim.app.world().get::<Throwable>(w).unwrap().count, 1);
}

#[test]
fn t31_sky() {
    let mut sim = sim();
    solid(&mut sim, Vec3::new(-10.0, 300.0 * U, -10.0), Vec3::new(10.0, 400.0 * U, 10.0), Some("sky"));
    let thrower = far_thrower(&mut sim, 0);
    let victim = player(&mut sim, 50.0, 0.0, 0.0, 0);
    let g = grenade(&mut sim, M67, Some(thrower), Vec3::new(0.0, 100.0, 0.0), Vec3::new(0.0, 1000.0, 0.0));
    sim.ticks(30);
    assert!(sim.app.world().get_entity(g).is_err(), "removed");
    sim.seconds(3.0);
    assert!(booms(&sim).is_empty(), "no explosion");
    assert_eq!(health(&sim, victim), 100.0, "no damage");
}

#[test]
fn t32_chain() {
    for (d, goes) in [(150.0, true), (200.0, false)] {
        let mut sim = sim();
        let flash = grenade(&mut sim, XM84, None, Vec3::new(0.0, 5.0, 0.0), Vec3::ZERO);
        sim.seconds(0.6);
        assert!(state(&sim, flash).resting);
        explode_at(&mut sim, M67, None, Vec3::new(d * U, 0.05, 0.0));
        let b = booms(&sim);
        if goes {
            assert_eq!(b.len(), 2, "{b:?}");
            assert_eq!(b[0].0, b[1].0, "the same server frame");
            assert_eq!(b[1].1, XM84);
        } else {
            assert_eq!(b.len(), 1, "{b:?}");
            let left = state(&sim, flash).health;
            assert!((left - 5.56).abs() < 0.01, "{left}");
        }
    }
}

// ---------------------------------------------------------------------------
// Fire and gas (33, 34), water (35)

/// HP lost by a victim 100 u from a `id` sphere, moved out of it at
/// `leave` s (none: stays), and when the last damage landed (s).
fn over_time(id: &str, leave: Option<f64>) -> (f32, f64) {
    let mut sim = sim();
    let thrower = far_thrower(&mut sim, 0);
    let victim = player(&mut sim, 0.0, 0.0, 0.0, 0);
    let at = in_front(&mut sim, victim, 100.0, true);
    explode_at(&mut sim, id, Some(thrower), at);
    let t0 = sim.tick();
    let mut last = 0.0;
    let mut hp = health(&sim, victim);
    for _ in 0..((25.0 / TICK_INTERVAL) as u64) {
        sim.ticks(1);
        let t = (sim.tick() - t0) as f64 * TICK_INTERVAL;
        if leave.is_some_and(|l| (t - l).abs() < TICK_INTERVAL / 2.0) {
            sim.app.world_mut().get_mut::<Transform>(victim).unwrap().translation.x += 1000.0 * U;
        }
        let now = health(&sim, victim);
        if now < hp {
            last = t;
            hp = now;
        }
    }
    (100.0 - hp, last)
}

#[test]
fn t33_incendiary() {
    let (lost, last) = over_time(M23F, None);
    assert!((lost - 95.0).abs() <= 1.0, "{lost}");
    assert!((last - 19.0).abs() <= 0.2, "last at {last}");
    let (lost, _) = over_time(M23F, Some(4.0));
    assert!((lost - 70.0).abs() <= 1.0, "{lost}");
}

#[test]
fn t34_nerve_gas() {
    let (lost, _) = over_time(G7A, None);
    assert!((lost - 69.0).abs() <= 1.0, "{lost}");
}

#[test]
fn t35_water() {
    let mut sim = sim();
    sim.app.world_mut().spawn(CaLiquid {
        min: Vec3::new(-5.0, -1.0, -5.0),
        max: Vec3::new(5.0, 5.0, 5.0),
        gravity: grenades::LIQUID_GRAVITY,
        viscosity: 0.0,
        current: Vec3::ZERO,
    });
    let g = grenade(&mut sim, M67, None, Vec3::new(0.0, 200.0, 0.0), Vec3::ZERO);
    let s = to_step(&mut sim, g, 1);
    assert_near(s.velocity, Vec3::new(0.0, -10.0, 0.0), 0.05, "v");
    assert!((200.0 - s.pos.y - 0.125).abs() < 1e-3, "drop {}", 200.0 - s.pos.y);
}

// ---------------------------------------------------------------------------
// The spent grenade (inventory.md 10, its test case 40)

/// Throwing the last M67 keeps it in the inventory at total 0 (same
/// place), the auto-switch draws the rifle when the post-fire ends, and
/// respawn gives it back.
#[test]
fn spent_grenade_kept_at_total_0() {
    let mut sim = sim();
    let p = player(&mut sim, 0.0, 0.0, 0.0, 0);
    sim.seconds(1.0);
    let before = sim.app.world().get::<Inventory>(p).unwrap().weapons.clone();
    let ids: Vec<&str> = before.iter().map(|w| sim.app.world().get::<Weapon>(*w).unwrap().id).collect();
    assert_eq!(ids, [combat_arms::game::weapons::RIFLE, combat_arms::game::inventory::KNIFE, M67], "the loadout");
    let m67 = before[2];
    sim.intent(p).select = Some(3);
    sim.ticks(1);
    sim.intent(p).select = None;
    sim.seconds(0.5);
    assert_eq!(sim.app.world().get::<Inventory>(p).unwrap().active, Some(m67));
    sim.intent(p).fire = true;
    sim.ticks(1);
    sim.intent(p).fire = false;
    sim.seconds(1.5);
    let inv = sim.app.world().get::<Inventory>(p).unwrap().clone();
    assert_eq!(inv.weapons, before, "kept, same place");
    assert_eq!(sim.app.world().get::<Throwable>(m67).unwrap().count, 0);
    assert_eq!(inv.active, Some(before[0]), "auto-switched to the rifle");
    sim.intent(p).select = Some(3);
    sim.ticks(1);
    sim.intent(p).select = None;
    sim.seconds(0.5);
    assert_eq!(sim.app.world().get::<Inventory>(p).unwrap().active, Some(before[0]), "key 4: nothing");
    combat_arms::game::inventory::respawn_inventory(sim.app.world_mut(), p);
    sim.seconds(1.5);
    let w = sim.app.world().get::<Inventory>(p).unwrap().weapons.clone();
    let m67 = w.iter().find(|w| sim.app.world().get::<Weapon>(**w).unwrap().id == M67).unwrap();
    assert_eq!(sim.app.world().get::<Throwable>(*m67).unwrap().count, 1, "back at respawn");
}

// ---------------------------------------------------------------------------
// The network

/// Over the network: a client throws its M67; both clients draw it in
/// flight, the throw is predicted without error, and its blast's damage
/// reaches both (the victim's health replicated to each).
#[test]
fn net_thrown_grenade_seen_and_damaging() {
    let link = LinkConditions {
        latency: Duration::from_millis(30),
        jitter: Duration::ZERO,
        loss: 0.0,
    };
    let mut sim = NetSim::new(CaSim, link, 7, 2, |app| {
        app.add_plugins(GreyboxMapPlugin);
    });
    sim.set_frame(TICK_INTERVAL);
    sim.until_joined(600);
    sim.ticks(240);
    let me = sim.local_player(0).unwrap();
    for c in &mut sim.clients {
        *c.app.world_mut().resource_mut::<NetGraph>() = NetGraph::default();
    }
    // Key 4: the M67.
    sim.clients[0].app.world_mut().get_mut::<Intent>(me).unwrap().select = Some(3);
    sim.ticks(1);
    sim.clients[0].app.world_mut().get_mut::<Intent>(me).unwrap().select = None;
    sim.ticks(40);
    let active = sim.clients[0].app.world().get::<Inventory>(me).unwrap().active.unwrap();
    assert_eq!(sim.clients[0].app.world().get::<Weapon>(active).unwrap().id, M67, "drawn");
    sim.clients[0].app.world_mut().get_mut::<Intent>(me).unwrap().fire = true;
    sim.ticks(1);
    sim.clients[0].app.world_mut().get_mut::<Intent>(me).unwrap().fire = false;
    // Seen in flight on both clients.
    let mut seen = [false; 2];
    let ok = sim.until(300, |s| {
        for (i, c) in s.clients.iter_mut().enumerate() {
            let w = c.app.world_mut();
            if w.query::<&ShownItem>().iter(w).any(|i| i.0 == M67) {
                seen[i] = true;
            }
        }
        seen == [true, true]
    });
    assert!(ok, "drawn on both clients: {seen:?}");
    for i in 0..2 {
        let g = sim.clients[i].app.world().resource::<NetGraph>().clone();
        assert_eq!(g.errors, 0, "client {i} mispredicted the throw ({:?})", g.last_differing);
    }
    // Put it at the other player's feet on the server; let it go off.
    let victim = sim.character_of(1).unwrap();
    let at = sim.server.app.world().get::<Transform>(victim).unwrap().translation;
    {
        let w = sim.server.app.world_mut();
        let g = w.query_filtered::<Entity, With<CaGrenade>>().iter(w).next().expect("on the server");
        let mut s = w.get_mut::<CaGrenade>(g).unwrap();
        s.pos = (at + Vec3::new(0.5, 0.0, 0.0)) / U;
        s.velocity = Vec3::ZERO;
        s.resting = true;
    }
    let before = sim.server.app.world().get::<Health>(victim).unwrap().current;
    sim.ticks(300);
    let after = sim.server.app.world().get::<Health>(victim).unwrap().current;
    assert!(after < before, "the server's victim hurt");
    let theirs = sim.local_player(1).unwrap();
    let on_victim = sim.clients[1].app.world().get::<Health>(theirs).unwrap().current;
    assert_eq!(on_victim, after, "the victim's client has it");
    // The thrower's client sees the victim's health too.
    let w = sim.clients[0].app.world_mut();
    let others: Vec<f32> = w
        .query_filtered::<&Health, With<Intent>>()
        .iter(w)
        .map(|h| h.current)
        .collect();
    assert!(others.contains(&after), "the thrower's client has it: {others:?}");
}
