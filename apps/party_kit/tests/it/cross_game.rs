//! What any game's gun does that every game shares (playtest 2026-10-24;
//! `chimera_sim::weapon::impact`), on the party's greybox: both games'
//! bullets push a physics object at the hit (one `BulletImpulse` each,
//! CS:S's from its measured damage force, Combat Arms' from its damage at
//! 2 N·s a hit point), and a swap to the other game's gun ends the old
//! game's recoil: the view punch is zeroed at the switch and the old
//! game's return doesn't drift the new game's view (both ways).

use avian3d::prelude::{Collider, LinearVelocity, Mass, RigidBody};
use bevy::prelude::*;
use chimera_sim::{
    minigame::ladder::{self, Ladder},
    testkit,
    weapon::{ViewPunch, impact::RecoilGame},
};
use chimera_world::{harness::Sim, map::PropSurface};
use combat_arms::game::weapons::CaKick;

use crate::gun_game::{held, party, place, soldier};

fn set_rung(sim: &mut Sim, e: Entity, r: usize) {
    let w = sim.app.world_mut();
    let l = w.resource::<Ladder>().clone();
    ladder::set_rung(w, e, r, &l);
}

fn punch(sim: &Sim, e: Entity) -> Vec3 {
    sim.app.world().get::<ViewPunch>(e).map_or(Vec3::ZERO, |p| p.0)
}

/// A soldier at the greybox's middle holding the ladder's rung `r`.
fn holding(sim: &mut Sim, r: usize, id: &str) -> Entity {
    let a = soldier(sim);
    let _b = soldier(sim);
    sim.seconds(1.5);
    place(sim, a, Vec3::new(0.0, 0.1, 0.0));
    set_rung(sim, a, r);
    sim.seconds(1.5);
    assert_eq!(held(sim, a), Some(id));
    a
}

/// Hold the trigger for `secs` (pressed every other tick: semi-automatics
/// fire too), aiming level ahead.
fn spray(sim: &mut Sim, a: Entity, secs: f64) {
    let at = sim.position(a) + Vec3::new(0.0, 1.5, -20.0);
    for t in 0..(secs * sim.tick_hz()) as u64 {
        testkit::aim_at(sim, a, at);
        sim.intent(a).fire = t % 2 == 0;
        sim.ticks(1);
    }
    sim.intent(a).fire = false;
}

#[test]
fn a_swap_to_cs_sources_gun_ends_combat_arms_recoil() {
    let mut sim = party();
    // Rung 7 (0-based): Combat Arms' M4A1; rung 6: CS:S's AK-47.
    let a = holding(&mut sim, 7, "combat_arms:m4a1");
    spray(&mut sim, a, 0.6);
    assert!(punch(&sim, a).x > 0.01, "Combat Arms' kick pulled the view up: {:?}", punch(&sim, a));
    assert_eq!(sim.app.world().get::<RecoilGame>(a).map(|g| g.0.as_str()), Some("combat_arms"));
    set_rung(&mut sim, a, 6);
    sim.ticks(1);
    assert_eq!(held(&sim, a), Some("cs_source:weapon_ak47"));
    assert_eq!(sim.app.world().get::<RecoilGame>(a).map(|g| g.0.as_str()), Some("cs_source"));
    assert_eq!(punch(&sim, a), Vec3::ZERO, "the switch ends Combat Arms' punch");
    // Combat Arms' return doesn't go on into CS:S's view (it drifted
    // down below level before).
    for _ in 0..(1.0 * sim.tick_hz()) as u32 {
        sim.ticks(1);
        assert!(punch(&sim, a).length() < 1e-6, "no drift: {:?}", punch(&sim, a));
    }
    let k = sim.app.world().get::<CaKick>(a).cloned().unwrap_or_default();
    assert_eq!((k.pitch, k.yaw), (0.0, 0.0), "Combat Arms' kick ended");
}

#[test]
fn a_swap_to_combat_arms_gun_ends_cs_sources_recoil() {
    let mut sim = party();
    let a = holding(&mut sim, 6, "cs_source:weapon_ak47");
    spray(&mut sim, a, 0.3);
    let kicked = punch(&sim, a);
    assert!(kicked.x > 0.01, "CS:S's kick: {kicked:?}");
    set_rung(&mut sim, a, 7);
    sim.ticks(1);
    assert_eq!(held(&sim, a), Some("combat_arms:m4a1"));
    assert_eq!(punch(&sim, a), Vec3::ZERO, "the switch ends CS:S's punch");
    // Combat Arms' own kick starts from level and returns to it (once
    // the M4A1 is drawn).
    sim.seconds(1.5);
    assert!(punch(&sim, a).length() < 1e-6, "no CS:S decay into it: {:?}", punch(&sim, a));
    spray(&mut sim, a, 0.3);
    assert!(punch(&sim, a).x > 0.0, "Combat Arms' kick: {:?}", punch(&sim, a));
    sim.seconds(2.0);
    assert!(punch(&sim, a).x.abs() < 1e-3, "returned to level: {:?}", punch(&sim, a));
}

/// A 20 kg physics box 4 m ahead at eye height, floating (no gravity
/// pull matters within the tick measured).
fn crate_ahead(sim: &mut Sim, a: Entity) -> Entity {
    let at = sim.position(a) + sim.state(a).eye_offset + Vec3::new(0.0, 0.0, -4.0);
    sim.app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::cuboid(0.6, 0.6, 0.6),
            Mass(20.0),
            PropSurface("metal".into()),
            Transform::from_translation(at),
        ))
        .id()
}

/// The box's speed away from the shooter after one round.
fn pushed_by_one_round(rung: usize, id: &str) -> f32 {
    let mut sim = party();
    let a = holding(&mut sim, rung, id);
    let b = crate_ahead(&mut sim, a);
    sim.ticks(2);
    let at = sim.app.world().get::<Transform>(b).unwrap().translation;
    testkit::aim_at(&mut sim, a, at);
    sim.ticks(1);
    let before = sim.app.world().get::<LinearVelocity>(b).unwrap().0;
    sim.intent(a).fire = true;
    sim.ticks(1);
    sim.intent(a).fire = false;
    let mut v = sim.app.world().get::<LinearVelocity>(b).unwrap().0;
    for _ in 0..4 {
        sim.ticks(1);
        let now = sim.app.world().get::<LinearVelocity>(b).unwrap().0;
        if -now.z > -v.z {
            v = now;
        }
    }
    -(v - before).z
}

#[test]
fn both_games_bullets_push_a_physics_object() {
    // CS:S's AK-47: 2400 kg·in/s (60.96 N·s) on 20 kg, 3.05 m/s.
    let cs = pushed_by_one_round(6, "cs_source:weapon_ak47");
    assert!((cs - 60.96 / 20.0).abs() < 0.3, "CS:S's push unchanged: {cs} m/s");
    // Combat Arms' M4A1: its damage at 4 m (about 30 HP, the roll's
    // +-20 %) at 2 N·s a hit point: about 3 m/s.
    let ca = pushed_by_one_round(7, "combat_arms:m4a1");
    assert!(ca > 1.5 && ca < 5.0, "Combat Arms' round pushes it too: {ca} m/s");
}

// ---------------------------------------------------------------------------
// Hits across games (docs/plans/active/generic-hits.md)

/// Every hit a weapon dealt: (target, hitgroup).
#[derive(Resource, Default)]
struct Hits(Vec<(Entity, chimera_core::core::Hitgroup)>);

fn collect_hits(mut events: MessageReader<chimera_sim::weapon::WeaponEvent>, mut out: ResMut<Hits>) {
    for e in events.read() {
        if let chimera_sim::weapon::WeaponEventKind::Hit { target, hitgroup, .. } = e.kind
            && !e.replay
        {
            out.0.push((target, hitgroup));
        }
    }
}

/// A shooter holding rung `shooter`'s gun and a target 4 m ahead holding
/// rung `target`'s, facing each other, the target unkillable; with
/// `bodies`, the map's systems (which give characters their bodies) and
/// those bodies.
fn duel(shooter: usize, target: usize, bodies: Option<fn(&mut App)>) -> (Sim, Entity, Entity) {
    let mut sim = match bodies {
        None => party(),
        Some(setup) => {
            let mut sim = Sim::new(
                combat_arms::harness::CaSim,
                (
                    chimera_world::greybox::GreyboxMapPlugin,
                    chimera_world::map::MapSimPlugin::empty(),
                    party_kit::PartyPartsPlugin,
                ),
            );
            setup(&mut sim.app);
            chimera_sim::minigame::start(sim.app.world_mut(), party_kit::DEFAULT_MINIGAME).expect("the gun game starts");
            sim
        }
    };
    sim.app.init_resource::<Hits>().add_systems(Update, collect_hits);
    let a = soldier(&mut sim);
    let b = soldier(&mut sim);
    sim.seconds(1.5);
    // Standing on the floor (the spawn's height, then settled).
    place(&mut sim, a, Vec3::new(0.0, 1.0, 0.0));
    place(&mut sim, b, Vec3::new(0.0, 1.0, -4.0));
    set_rung(&mut sim, a, shooter);
    set_rung(&mut sim, b, target);
    sim.intent(a).yaw = 0.0;
    sim.intent(b).yaw = std::f32::consts::PI;
    sim.seconds(1.5);
    let mut h = sim.app.world_mut().get_mut::<chimera_core::core::Health>(b).unwrap();
    h.current = 100.0;
    h.max = 100.0;
    (sim, a, b)
}

/// Where the centre of `b`'s first hitbox of `region` is in the world.
fn shape_at(sim: &Sim, b: Entity, region: chimera_core::core::BodyRegion) -> Vec3 {
    let w = sim.app.world();
    let boxes = w.get::<chimera_core::core::Hitboxes>(b).expect("hitboxes");
    let shape = boxes.0.iter().find(|h| h.region == region).unwrap_or_else(|| panic!("no {region:?}"));
    let t = w.get::<Transform>(b).unwrap().translation;
    let feet = w.get::<avian3d::prelude::ColliderAabb>(b).map_or(t, |a| t.with_y(a.min.y));
    feet + w.get::<chimera_core::core::Intent>(b).unwrap().yaw_rotation() * shape.center
}

/// One round from `a` at `point`; the hitgroups `b` was hit in.
fn shot_at(sim: &mut Sim, a: Entity, b: Entity, point: Vec3) -> Vec<chimera_core::core::Hitgroup> {
    sim.app.world_mut().resource_mut::<Hits>().0.clear();
    testkit::aim_at(sim, a, point);
    sim.ticks(1);
    sim.intent(a).fire = true;
    sim.ticks(1);
    sim.intent(a).fire = false;
    sim.ticks(6);
    let hits = sim.app.world().resource::<Hits>().0.iter().filter(|(t, _)| *t == b).map(|(_, g)| *g).collect();
    sim.seconds(1.0);
    hits
}

#[test]
fn a_cs_source_gun_hits_a_combat_arms_body_where_it_is_drawn() {
    use chimera_core::core::{BodyRegion as R, Hitgroup as G};
    // Rung 6: CS:S's AK-47; rung 7: Combat Arms' M4A1 (its body, its
    // hit spheres as the target's hitboxes).
    let (mut sim, a, b) = duel(6, 7, None);
    let boxes = sim.app.world().get::<chimera_core::core::Hitboxes>(b).expect("Combat Arms' body has hitboxes");
    assert!(boxes.0.iter().all(|h| h.radius.is_some()), "spheres");
    // The head is the head; the right hand and shin are the right arm and
    // leg (height bands on the collider would say left for both).
    let head = shape_at(&sim, b, R::Head);
    assert_eq!(shot_at(&mut sim, a, b, head), vec![G::Head]);
    let hand = shape_at(&sim, b, R::RightHand);
    assert_eq!(shot_at(&mut sim, a, b, hand), vec![G::RightArm]);
    let shin = shape_at(&sim, b, R::RightShin);
    assert_eq!(shot_at(&mut sim, a, b, shin), vec![G::RightLeg]);
}

/// The map's bodies: Combat Arms' (no hitboxes: it makes its own) and a
/// short CS:S stand-in whose head box is 1 m up, below where Combat Arms'
/// own skeleton has its chest, so only its own boxes give a head shot.
fn short_cs_body(app: &mut App) {
    use chimera_core::core::{BodyRegion as R, Hitbox};
    use chimera_world::map::{CharacterModels, MapCharacterModel};
    let body = |game, hitboxes| MapCharacterModel {
        game: Some(game),
        team: None,
        name: None,
        class: None,
        model: default(),
        hitboxes,
        bones: Vec::new(),
        root: Transform::IDENTITY,
        animations: None,
        boxes: Vec::new(),
        ragdoll: None,
    };
    let cube = |y: f32, x: f32, region| Hitbox::cuboid(Vec3::new(x, y, 0.0), Vec3::splat(0.12), Quat::IDENTITY, region);
    let boxes = vec![cube(1.0, 0.0, R::Head), cube(0.7, 0.0, R::Chest), cube(0.25, 0.12, R::RightShin)];
    app.insert_resource(CharacterModels(std::sync::Arc::new(vec![
        body("combat_arms", Vec::new()),
        body("cs_source", boxes),
    ])));
}

#[test]
fn a_combat_arms_gun_hits_a_cs_source_body_where_it_is_drawn() {
    use chimera_core::core::{BodyRegion as R, Hitgroup as G};
    // Rung 7: Combat Arms' M4A1 at rung 6's CS:S player, who wears the
    // short CS:S body: its boxes, not Combat Arms' skeleton standing.
    let (mut sim, a, b) = duel(7, 6, Some(short_cs_body));
    let boxes = sim.app.world().get::<chimera_core::core::Hitboxes>(b).expect("the CS:S body's hitboxes");
    assert!(boxes.0.iter().all(|h| h.radius.is_none()), "boxes");
    let head = shape_at(&sim, b, R::Head);
    assert_eq!(shot_at(&mut sim, a, b, head), vec![G::Head]);
    let chest = shape_at(&sim, b, R::Chest);
    assert_eq!(shot_at(&mut sim, a, b, chest), vec![G::Chest]);
    let shin = shape_at(&sim, b, R::RightShin);
    assert_eq!(shot_at(&mut sim, a, b, shin), vec![G::RightLeg]);
}
