//! Physics props' motion for the logic layer (specs/source/prop_damage.md
//! 4 step 13, 8; specs/cs_source/physics_props.md 3.1, 3.5): a frozen prop
//! ("motion disabled") stays put like a static prop until its motion is
//! enabled, then becomes a simulated body; bodies can be put to sleep and
//! woken; a prop that started asleep reports its first wake
//! (`PropAwakened`, Source OnAwakened). A round restart puts each back as
//! it spawned.
//!
//! Physics props' motion for the logic: frozen props (`FrozenBody`, static with
//! their brushes) set moving by `enable_motion`, `disable_motion`,
//! `wake`/`sleep`, props that start asleep (`StartAsleep`: put to sleep once in
//! the simulation, `PropAwakened` on their first wake), `reset` at a round
//! restart; players' physics shadows (`PhysicsShadow` on the shadow body,
//! `Shadowed` on its character; `ShadowContacts` keeps a shadow's pairs to
//! `prop_physics` bodies it can push (up to 350 kg) and keeps a shadowed
//! character's own pairs only to `prop_physics` bodies too heavy to push; the
//! shadow is on `core::SHADOW_LAYER`, which no spatial query sees);
//! `settle_props`: a moving prop that keeps within 1 in and 5 degrees of
//! one pose for 1 s is stopped and put to sleep (`Settling`), as Source's
//! physics sleeps nearly still bodies.
//!
//! Known shortcuts (docs/tech-debt.md): The settle thresholds (1 in, 5
//! degrees, 1 s) are ours: spec 3.5 leaves vphysics' sleep thresholds open.
//! They stand in for a fix in the solver, which on uneven ground (a convex
//! hull bridging two displacement triangles) puts back the energy its
//! contacts take out, so such a prop rocks at speeds above avian's sleep
//! threshold until `settle_props` stops it (up to about 3 s after a bullet
//! push for de_dust2's oil drum 390). With avian's warm starting off it
//! settles, but other props then misbehave (one flew off at 50 m/s). Why
//! it's OK for now: the rocking is about a degree, and every de_dust2 prop
//! sleeps within a few seconds of a push (apps/cs_source/tests/it/heavy/map_prop_settle.rs).

use avian3d::{
    dynamics::solver::islands::{BodyIslandNode, SleepBody},
    prelude::*,
};
use bevy::{ecs::system::SystemParam, prelude::*};

use super::{MapBrushCollider, MapPhysics, MapPropCollider, PhysicsProp};
use crate::core::MovingSolid;

/// The simulated body of a physics prop with these settings.
pub fn dynamic_body(p: &MapPhysics, bounds: (Vec3, Vec3)) -> impl Bundle {
    (
        RigidBody::Dynamic,
        Mass(p.mass),
        Friction::new(p.friction),
        Restitution::new(p.elasticity),
        LinearDamping(p.damping),
        AngularDamping(p.rotdamping),
        // Source clamps every body to 2000 units/s and 3600 deg/s.
        MaxLinearSpeed(2000.0 * 0.0254),
        MaxAngularSpeed(3600f32.to_radians()),
        PhysicsProp {
            push: p.push,
            mass: p.mass,
            bounds,
        },
        MapPropCollider,
        // Impact sounds and damage listen for its contacts.
        CollisionEventsEnabled,
    )
}

/// A physics prop placed frozen: the body it becomes when its motion is
/// enabled (`enabled` once it has), and what it was while frozen.
#[derive(Component, Clone, Debug)]
pub struct FrozenBody {
    pub physics: MapPhysics,
    pub bounds: (Vec3, Vec3),
}

/// A frozen prop now moving: its still brushes, kept for a round restart.
#[derive(Component, Clone, Debug)]
pub struct Unfrozen {
    solid: Option<MovingSolid>,
    brush_collider: bool,
}

/// A prop that spawned asleep: held still (a static body) until something
/// wakes it (the Wake input, a push from damage, a moving body running
/// into it), which reports `PropAwakened`. (Avian's own sleep can't be
/// set before a body joins the simulation, and props placed touching
/// each other would wake each other on the first step.)
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct StartAsleep;

/// A prop that started asleep woke up (its node).
#[derive(Message, Clone, Copy, Debug)]
pub struct PropAwakened(pub Entity);

/// Set a frozen prop moving; false if it isn't one (or already moves).
pub fn enable_motion(world: &mut World, node: Entity) -> bool {
    if super::placed_ragdoll::set_motion(world, node, true) {
        return true;
    }
    let Ok(mut e) = world.get_entity_mut(node) else {
        return false;
    };
    if e.contains::<Unfrozen>() {
        // Disabled again by DisableMotion: just let it move.
        if e.get::<RigidBody>().is_some_and(|b| b.is_static()) {
            e.insert(RigidBody::Dynamic);
            return true;
        }
        return false;
    }
    let Some(frozen) = e.get::<FrozenBody>().cloned() else {
        if e.get::<RigidBody>().is_some_and(|b| b.is_static()) && e.contains::<PhysicsProp>() {
            e.insert(RigidBody::Dynamic);
            return true;
        }
        return false;
    };
    let solid = e.take::<MovingSolid>();
    let brush_collider = e.take::<MapBrushCollider>().is_some();
    // Its visibility clusters follow it as it moves; its occluder bounds
    // were where it stood.
    let (min, max) = frozen.bounds;
    e.remove::<super::vis::Occludee>();
    e.insert(super::BrushEntityBounds { min, max });
    if !e.contains::<super::VisClusters>() {
        e.insert(super::VisClusters::new(Vec::new()));
    }
    e.insert((
        Unfrozen { solid, brush_collider },
        dynamic_body(&frozen.physics, frozen.bounds),
        Visibility::Inherited,
    ));
    true
}

/// Freeze a moving prop where it is.
pub fn disable_motion(world: &mut World, node: Entity) {
    if super::placed_ragdoll::set_motion(world, node, false) {
        return;
    }
    if let Ok(mut e) = world.get_entity_mut(node)
        && e.get::<RigidBody>().is_some_and(|b| b.is_dynamic())
    {
        e.insert(RigidBody::Static);
    }
}

/// Wake a sleeping body (one that started asleep reports it).
pub fn wake(world: &mut World, node: Entity) {
    if super::placed_ragdoll::wake(world, node) {
        return;
    }
    let Ok(mut e) = world.get_entity_mut(node) else { return };
    if e.take::<StartAsleep>().is_some() {
        e.insert(RigidBody::Dynamic);
        world.write_message(PropAwakened(node));
    } else if e.contains::<Sleeping>() {
        e.remove::<Sleeping>();
    }
}

/// Whether a prop node is still asleep since it spawned.
pub fn asleep(world: &World, node: Entity) -> bool {
    world.get::<StartAsleep>(node).is_some()
}

/// Put a body to sleep.
pub fn sleep(world: &mut World, node: Entity) {
    if let Ok(mut e) = world.get_entity_mut(node)
        && e.get::<RigidBody>().is_some_and(|b| b.is_dynamic())
        && !e.contains::<Sleeping>()
    {
        e.insert(Sleeping);
    }
}

/// A round restart: a prop that was frozen is frozen again (its still
/// brushes back), one that started asleep sleeps again and will report
/// its next wake.
pub fn reset(world: &mut World, node: Entity, asleep: bool) {
    let Ok(mut e) = world.get_entity_mut(node) else { return };
    if let Some(u) = e.take::<Unfrozen>() {
        e.remove::<PhysicsProp>().insert(RigidBody::Static);
        if let Some(s) = u.solid {
            e.insert(s);
        }
        if u.brush_collider {
            e.insert(MapBrushCollider);
        }
    } else if e.contains::<FrozenBody>() && e.get::<RigidBody>().is_some_and(|b| b.is_dynamic()) {
        e.insert(RigidBody::Static);
    }
    if asleep && e.contains::<PhysicsProp>() {
        e.insert((StartAsleep, RigidBody::Static));
    }
}

/// A moving body running into a prop that sleeps since it spawned wakes
/// it.
pub fn wake_on_contact(
    mut started: MessageReader<CollisionStart>,
    asleep: Query<(), With<StartAsleep>>,
    bodies: Query<(&RigidBody, &LinearVelocity)>,
    mut commands: Commands,
    mut awakened: MessageWriter<PropAwakened>,
) {
    for s in started.read() {
        let (b1, b2) = (s.body1.unwrap_or(s.collider1), s.body2.unwrap_or(s.collider2));
        for (me, other) in [(b1, b2), (b2, b1)] {
            let moving = bodies
                .get(other)
                .is_ok_and(|(rb, v)| rb.is_dynamic() && v.0.length_squared() > WAKE_SPEED * WAKE_SPEED);
            if moving && asleep.contains(me) {
                commands.entity(me).remove::<StartAsleep>().insert(RigidBody::Dynamic);
                awakened.write(PropAwakened(me));
            }
        }
    }
}

/// A body slower than this (m/s) doesn't wake what it touches.
const WAKE_SPEED: f32 = 0.05;

/// A player's physics shadow (specs/cs_source/physics_props.md 4.1 step 3):
/// an invisible 85 kg body the size of the player's box that the game
/// drives to the player each tick. It is what touches physics props for
/// the player: it pushes the light ones and holds up the ones resting on
/// or against the player. It is on its own layer (`core::SHADOW_LAYER`),
/// which no spatial query sees. Inactive (a dead player's), it touches
/// nothing.
#[derive(Component, Clone, Copy, Debug)]
pub struct PhysicsShadow {
    pub owner: Entity,
    pub active: bool,
}

/// A character whose contacts with physics props go through its
/// `PhysicsShadow` (this entity): its own collider never touches them.
#[derive(Component, Clone, Copy, Debug)]
pub struct Shadowed(pub Entity);

/// The shadow's mass, kg (spec 4.1 step 3).
pub const SHADOW_MASS: f32 = 85.0;
/// The heaviest prop a shadow pushes, kg (spec: the player push mass
/// limit). Heavier props never touch the shadow: the player's movement
/// treats them as walls, and the character's own collider (kinematic)
/// stops them when they move into the player.
pub const SHADOW_PUSH_MASS: f32 = 350.0;

/// Which pairs players' shadows and shadowed characters keep.
#[derive(SystemParam)]
pub struct ShadowContacts<'w, 's> {
    shadows: Query<'w, 's, &'static PhysicsShadow>,
    shadowed: Query<'w, 's, (), With<Shadowed>>,
    of: Query<'w, 's, &'static ColliderOf>,
    props: Query<'w, 's, &'static PhysicsProp>,
}

impl ShadowContacts<'_, '_> {
    /// Some(keep) when collider `a` or `b` is a shadow or a shadowed
    /// character: an active shadow touches only the `prop_physics` bodies
    /// it can push (multiplayer props are in a group players never touch,
    /// spec 4.2.1); a shadowed character touches only the heavier ones.
    /// None: neither is one.
    pub fn filter(&self, a: Entity, b: Entity) -> Option<bool> {
        let body = |e: Entity| self.of.get(e).map_or(e, |o| o.body);
        let (a, b) = (body(a), body(b));
        for (me, other) in [(a, b), (b, a)] {
            if let Ok(s) = self.shadows.get(me) {
                return Some(s.active && self.props.get(other).is_ok_and(pushable));
            }
        }
        for (me, other) in [(a, b), (b, a)] {
            if self.shadowed.contains(me)
                && let Ok(p) = self.props.get(other)
            {
                return Some(p.push == super::PushAway::Collide && !pushable(p));
            }
        }
        None
    }
}

/// Whether players' shadows push this prop: a `prop_physics` (not a
/// multiplayer one) up to `SHADOW_PUSH_MASS`.
pub fn pushable(p: &PhysicsProp) -> bool {
    p.push == super::PushAway::Collide && p.mass <= SHADOW_PUSH_MASS
}

/// A moving physics prop that stays within `SETTLE_DISTANCE` and
/// `SETTLE_ANGLE` of one pose for `SETTLE_TIME` is stopped and put to
/// sleep (with its island), as Source's physics puts nearly still bodies
/// to sleep (spec 3.5; its thresholds are inside vphysics, an open
/// question, so these are ours). Avian's own sleep needs the speed below
/// its threshold for half a second, which a prop rocking on uneven ground
/// never gets: a de_dust2 oil drum knocked onto the crease between two
/// displacement triangles (a flat one and one 3.7 degrees steeper) rocked
/// by about 1.5 degrees at up to 0.5 rad/s forever, the solver putting
/// back each tick the energy its contacts take out
/// (apps/cs_source/tests/it/heavy/map_prop_settle.rs).
pub const SETTLE_DISTANCE: f32 = 0.0254;
/// See `SETTLE_DISTANCE`, radians.
pub const SETTLE_ANGLE: f32 = 0.087;
/// See `SETTLE_DISTANCE`, seconds.
pub const SETTLE_TIME: f64 = 1.0;

/// Where an awake physics prop has stayed since when (`settle_props`).
#[derive(Component, Clone, Copy, Debug)]
pub struct Settling {
    at: Vec3,
    rotation: Quat,
    since: f64,
}

/// After each physics step: props that kept still (`SETTLE_DISTANCE`)
/// sleep. An island sleeps as a whole, so it waits for every prop in it
/// to have kept still and every other body in it to be resting by avian's
/// own measure. Their velocities are zeroed without marking them changed
/// (a change wakes a sleeping body); a prop that sleeps (by this or by
/// avian) forgets its pose, so one woken by a push starts its wait again.
#[allow(clippy::type_complexity)]
pub fn settle_props(
    time: Res<Time>,
    time_to_sleep: Res<TimeToSleep>,
    mut props: Query<
        (
            Entity,
            &RigidBody,
            &Position,
            &Rotation,
            &BodyIslandNode,
            &mut LinearVelocity,
            &mut AngularVelocity,
            Option<&mut Settling>,
        ),
        (With<PhysicsProp>, Without<Sleeping>),
    >,
    others: Query<(&RigidBody, &BodyIslandNode, &SleepTimer), (Without<PhysicsProp>, Without<Sleeping>)>,
    asleep: Query<Entity, (With<Settling>, Or<(With<Sleeping>, Without<PhysicsProp>)>)>,
    mut commands: Commands,
    mut still: Local<Vec<(Entity, u32)>>,
    mut moving: Local<bevy::platform::collections::HashSet<u32>>,
) {
    for e in &asleep {
        commands.entity(e).remove::<Settling>();
    }
    let now = time.elapsed_secs_f64();
    still.clear();
    moving.clear();
    for (e, rb, p, r, island, _, _, settling) in &mut props {
        if !rb.is_dynamic() {
            if settling.is_some() {
                commands.entity(e).remove::<Settling>();
            }
            continue;
        }
        let fresh = Settling {
            at: p.0,
            rotation: r.0,
            since: now,
        };
        let Some(mut s) = settling else {
            commands.entity(e).insert(fresh);
            moving.insert(island.island_id().0);
            continue;
        };
        if s.at.distance(p.0) > SETTLE_DISTANCE || s.rotation.angle_between(r.0) > SETTLE_ANGLE {
            *s = fresh;
        }
        if now - s.since >= SETTLE_TIME {
            still.push((e, island.island_id().0));
        } else {
            moving.insert(island.island_id().0);
        }
    }
    for (rb, island, timer) in &others {
        if rb.is_dynamic() && timer.0 < time_to_sleep.0 {
            moving.insert(island.island_id().0);
        }
    }
    let mut slept = bevy::platform::collections::HashSet::new();
    for &(e, island) in still.iter() {
        if moving.contains(&island) {
            continue;
        }
        if let Ok((.., mut v, mut w, _)) = props.get_mut(e) {
            v.bypass_change_detection().0 = Vec3::ZERO;
            w.bypass_change_detection().0 = Vec3::ZERO;
        }
        // Forgotten now: a push before the next tick wakes it before this
        // runs again, and it must not count as still from before.
        commands.entity(e).remove::<Settling>();
        if slept.insert(island) {
            commands.queue(SleepBody(e));
        }
    }
}
