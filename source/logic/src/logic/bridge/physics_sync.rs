use avian3d::prelude::{
    AngularVelocity, Collider, ColliderAabb, ColliderDisabled, CollisionLayers, CollisionStart, ComputedMass,
    LayerMask, LinearVelocity, PhysicsSystems, Position, RigidBody, RigidBodyDisabled, Rotation,
};
use bevy::{ecs::message::MessageCursor, prelude::*};
use chimera_core::query_cache::{cached_query, first};

use super::*;
use crate::console::{Console, ConsoleAppExt, Level};
use crate::core::{
    BaseVelocity, Damage, DamageKind, Damageable, EntityGravity, Explosion, Health, Hitgroup, Intent, LocalPlayer,
    MapBrush, MapBrushes, MapTerrain, MovementState, MovingSolid, RoundRestarts, SimSet, Team, Velocity,
};
use crate::logic::classes::ServerLine;
use crate::logic::hud::{HudEvent, HudMessages, HudShow, ScreenFades};
use crate::logic::prop_damage::{Hit, Motion, PropExplosion};
use crate::logic::world::{Collision, Effect, EntId, LogicWorld, Player, SOLID_SKIN, SWEEP_EPS, Who};
use crate::map::entities::{engine_to_entity, entity_rotation, entity_to_engine, rotation_to_engine};
use crate::map::{
    BreakProp, BrushPanes, EntityPart, LightStyles, LogicHidden, MapBrushEntity, MapEntities, PlaySound, PropEntity,
    PropHome, PropIndex, PropLook, PropSequence, SoundControl, SoundKey, SoundLevel, SoundscapeSwitches,
    SoundscapeTouches, StartSound, VisClusters,
};


/// The anchors' and physics bodies' poses into the logic (`set_anchor`).
pub(super) fn set_anchor_poses(world: &World, logic: &mut Logic) {
    let scale = logic.scale;
    let entity_pose = |t: &Transform| {
        let rotation = Quat::from_xyzw(t.rotation.x, -t.rotation.z, t.rotation.y, t.rotation.w);
        (engine_to_entity(t.translation, scale), crate::logic::anchors::angles_of(rotation))
    };
    let mut poses: Vec<(EntId, Vec3, Vec3)> = Vec::new();
    if let Some(anchors) = world.get_resource::<crate::map::entities::EntityAnchors>() {
        for (index, t) in &anchors.0 {
            if let Some(id) = logic.world.by_map_index(*index) {
                let (o, a) = entity_pose(t);
                poses.push((id, o, a));
            }
        }
    }
    // Physics brushes and props: where their bodies are (a moving body:
    // maps rename them, AddOutput classname, so the class can't tell).
    for (id, node) in &logic.props {
        let body = world.get::<RigidBody>(*node).is_some_and(|b| !b.is_static());
        if (body
            || logic
                .world
                .get(*id)
                .is_some_and(|e| crate::logic::anchors::is_physics_body(&e.classname)))
            && let Some(t) = world.get::<Transform>(*node)
        {
            let (o, a) = entity_pose(t);
            poses.push((*id, o, a));
        }
    }
    for (id, o, a) in poses {
        logic.world.set_anchor(id, o, a);
    }
}

/// Where each logic prop's body is now (its collider's world box), for
/// fire (flame size and place, damage boxes).
pub(super) fn prop_bounds(world: &mut World, logic: &mut Logic) {
    let scale = logic.scale;
    for (id, node) in logic.props.clone() {
        let aabb = std::iter::once(node)
            .chain(world.get::<Children>(node).map(|c| c.to_vec()).unwrap_or_default())
            .find_map(|e| world.get::<ColliderAabb>(e).copied());
        if let Some(a) = aabb {
            let (lo, hi) = (engine_to_entity(a.min, scale), engine_to_entity(a.max, scale));
            logic.world.set_prop_bounds(id, (lo.min(hi), lo.max(hi)));
        }
    }
}

/// Lit fires and burning entities for the game to draw
/// (`map::fire::MapFires`).
pub(super) fn sync_fires(world: &mut World, logic: &Logic) {
    use crate::map::fire::{FireLook, FlameLook, MapFires};
    let scale = logic.scale;
    let key = |id: EntId, serial: u32| (serial as u64) << 48 | (id.generation as u64 & 0xffff) << 32 | id.index as u64;
    let fires: Vec<FireLook> = logic
        .world
        .fire_looks()
        .into_iter()
        .filter(|f| !f.plasma)
        .map(|f| FireLook {
            key: key(f.id, f.serial),
            at: entity_to_engine(f.at, scale),
            size: f.size,
            smokeless: f.smokeless,
        })
        .collect();
    let flames: Vec<FlameLook> = logic
        .world
        .flame_looks()
        .into_iter()
        .filter_map(|f| {
            let (follow, lo, hi) = match f.target {
                Who::Player(p) => {
                    let pl = logic.world.player(p)?;
                    (Some(p), pl.origin + pl.mins, pl.origin + pl.maxs)
                }
                Who::Ent(e) => {
                    let node = logic.props.iter().find(|(id, _)| *id == e).map(|(_, n)| *n);
                    let (lo, hi) = logic.world.prop(e)?.bounds.unwrap_or_else(|| {
                        let o = logic.world.get(e).map_or(Vec3::ZERO, |x| x.origin);
                        (o - Vec3::splat(8.0), o + Vec3::splat(8.0))
                    });
                    (node, lo, hi)
                }
            };
            let (a, b) = (entity_to_engine(lo, scale), entity_to_engine(hi, scale));
            Some(FlameLook {
                key: key(f.id, 0),
                follow,
                min: a.min(b),
                max: a.max(b),
            })
        })
        .collect();
    let want = MapFires { fires, flames };
    if world.get_resource::<MapFires>() != Some(&want) {
        world.insert_resource(want);
    }
}

/// `chimera_logic_record 1`: the logic world records the outputs it fires
/// and the inputs it delivers (the debug UI's Logic tab lists them),
/// keeping the latest `RECORD_KEEP`.
#[derive(Resource, Default)]
pub(super) struct LogicRecord(pub(super) u8);

pub(super) const RECORD_KEEP: usize = 256;

pub(super) fn apply_record(rec: Res<LogicRecord>, logic: Option<ResMut<Logic>>, mut last: Local<u8>) {
    let Some(mut logic) = logic else { return };
    // Turned off: stop (tests that record by hand leave the cvar at 0).
    if rec.0 != *last {
        *last = rec.0;
        logic.world.record = rec.0 != 0;
    }
    if rec.0 == 0 {
        return;
    }
    if !logic.world.record {
        logic.world.record = true;
    }
    for len in [logic.world.fired.len(), logic.world.deliveries.len()] {
        if len > RECORD_KEEP * 2 {
            let w = &mut logic.world;
            let fired = w.fired.len().saturating_sub(RECORD_KEEP);
            w.fired.drain(..fired);
            let delivered = w.deliveries.len().saturating_sub(RECORD_KEEP);
            w.deliveries.drain(..delivered);
            break;
        }
    }
}

pub(super) fn pre(world: &mut World) {
    run_phase(world, |w, col| {
        w.break_orphan_joints();
        w.run_thinks();
        w.move_cameras();
        w.step_movers(col);
        w.player_uses(col);
        w.push_conveyors();
    });
}

pub(super) fn post(
    world: &mut World,
    mut awake: Local<MessageCursor<crate::map::prop_physics::PropAwakened>>,
    mut broken: Local<MessageCursor<crate::map::controllers::JointBroke>>,
    mut animated: Local<MessageCursor<crate::map::PropAnimationDone>>,
) {
    // Animated props whose sequence ended (map::props::animate_props).
    let ended: Vec<(Entity, u32)> = match world.get_resource::<Messages<crate::map::PropAnimationDone>>() {
        Some(m) => animated.read(m).map(|d| (d.node, d.serial)).collect(),
        None => Vec::new(),
    };
    let ended: Vec<(EntId, u32)> = match world.get_resource::<Logic>() {
        Some(l) => ended
            .into_iter()
            .filter_map(|(n, s)| l.props.iter().find(|(_, p)| *p == n).map(|(id, _)| (*id, s)))
            .collect(),
        None => Vec::new(),
    };
    // Props that started asleep and woke (map::prop_physics).
    let woke: Vec<Entity> = match world.get_resource::<Messages<crate::map::prop_physics::PropAwakened>>() {
        Some(m) => awake.read(m).map(|a| a.0).collect(),
        None => Vec::new(),
    };
    let woke: Vec<EntId> = match world.get_resource::<Logic>() {
        Some(l) => woke
            .into_iter()
            .filter_map(|n| l.props.iter().find(|(_, p)| *p == n).map(|(id, _)| *id))
            .collect(),
        None => Vec::new(),
    };
    // Constraints that passed their break limits (map::controllers).
    let broke: Vec<EntId> = match world.get_resource::<Messages<crate::map::controllers::JointBroke>>() {
        Some(m) => broken
            .read(m)
            .map(|b| EntId {
                index: (b.0 & 0xffff_ffff) as u32,
                generation: (b.0 >> 32) as u32,
            })
            .collect(),
        None => Vec::new(),
    };
    run_phase(world, |w, col| {
        for id in woke {
            w.prop_awakened(id);
        }
        for (id, serial) in ended {
            w.prop_animation_done(id, serial);
        }
        for id in broke {
            w.deliver(Who::Ent(id), "Break", crate::logic::Value::Void, None, Some(Who::Ent(id)));
        }
        w.touch_triggers(col);
        w.touch_breakables();
        w.pressure_props();
        w.untouch();
        w.service_queue();
        w.end_frame();
    });
}

/// Damage dealt to mover nodes (breakables) and prop nodes this tick, into
/// the logic (with the push that came with it, and its attacker: a
/// player, or a prop that hit it).
pub(super) fn damage(world: &mut World, mut cursor: Local<MessageCursor<Damage>>) {
    let hits: Vec<Damage> = match world.get_resource::<Messages<Damage>>() {
        Some(m) => cursor.read(m).cloned().collect(),
        None => return,
    };
    let Some(logic) = world.get_resource::<Logic>() else {
        return;
    };
    let scale = logic.scale;
    let node_id = |e: Entity| {
        logic
            .nodes
            .iter()
            .chain(&logic.props)
            .find(|(_, n)| *n == e)
            .map(|(id, _)| *id)
    };
    let targeted: Vec<(EntId, Damage, Option<Who>)> = hits
        .into_iter()
        .filter_map(|d| {
            let id = node_id(d.target)?;
            let by = d.attacker.and_then(node_id).map(Who::Ent);
            Some((id, d, by))
        })
        .collect();
    if targeted.is_empty() {
        return;
    }
    run_phase(world, |w, _| {
        for (id, d, by) in targeted {
            let attacker = d.attacker.filter(|a| w.player(*a).is_some()).map(Who::Player).or(by);
            let dir = Vec3::new(d.dir.x, -d.dir.z, d.dir.y);
            let point = engine_to_entity(d.point, scale);
            let hit = Hit {
                amount: d.amount * 100.0,
                kind: d.kind,
                attacker,
                point,
                dir,
                force: d.force.length() / scale,
                direct: false,
            };
            if !w.prop_hit(id, hit) {
                w.damage(id, hit.amount, d.kind, attacker, point, dir);
            }
        }
    });
}

/// The node of a logic prop.
pub(super) fn prop_node(world: &World, id: EntId) -> Option<Entity> {
    let logic = world.get_resource::<Logic>()?;
    logic.props.iter().find(|(p, _)| *p == id).map(|(_, n)| *n)
}

/// Where a prop node is (its body's pose when it has one) and the centre
/// of its bounds.
pub(super) fn prop_pose(world: &World, node: Entity) -> (Vec3, Quat, Vec3) {
    let (pos, rot) = match (world.get::<Position>(node), world.get::<Rotation>(node)) {
        (Some(p), Some(r)) => (p.0, r.0),
        _ => world
            .get::<GlobalTransform>(node)
            .map_or((Vec3::ZERO, Quat::IDENTITY), |g| {
                let (_, r, t) = g.to_scale_rotation_translation();
                (t, r)
            }),
    };
    let centre = std::iter::once(node)
        .chain(world.get::<Children>(node).map(|c| c.to_vec()).unwrap_or_default())
        .find_map(|e| world.get::<ColliderAabb>(e))
        .map_or(pos, |a| a.center());
    (pos, rot, centre)
}

/// A prop broke: its break sound where it is, its explosion from its
/// centre, its pieces from its pose and velocity.
pub(super) fn prop_broke(world: &mut World, id: EntId, sound: Option<String>, explode: Option<PropExplosion>, scale: f32) {
    let Some(node) = prop_node(world, id) else { return };
    let (pos, rot, centre) = prop_pose(world, node);
    if let Some(s) = sound {
        logic_sound(world, PlaySound::at(s, pos));
    }
    if let Some(x) = explode {
        let attacker = match x.attacker {
            Some(Who::Player(e)) => Some(e),
            Some(Who::Ent(e)) => prop_node(world, e),
            None => None,
        };
        world.write_message(Explosion {
            origin: centre,
            damage: x.damage / 100.0,
            radius: x.radius * scale,
            attacker,
            inflictor: Some(node),
            sound: x.sound,
            weapon: None,
            round_scorch: false,
            through_walls: false,
            bell_sigma: None,
            armor_ratio: None,
        });
    }
    if let Some(index) = world.get::<PropIndex>(node).map(|p| p.0) {
        let velocity = world.get::<LinearVelocity>(node).map_or(Vec3::ZERO, |v| v.0);
        let spin = world.get::<AngularVelocity>(node).map_or(Vec3::ZERO, |v| v.0);
        let skin = world.get::<PropLook>(node).map(|l| l.skin);
        world.write_message(BreakProp {
            prop: index,
            transform: Transform::from_translation(pos).with_rotation(rot),
            velocity,
            spin,
            skin,
        });
    }
}

/// Each moving body's velocity before this physics step (impact damage
/// needs the speed a collision took away).
#[derive(Resource, Default)]
pub(super) struct PreStep(std::collections::HashMap<Entity, Vec3>);

pub(super) fn record_velocities(mut pre: ResMut<PreStep>, bodies: Query<(Entity, &RigidBody, &LinearVelocity)>) {
    pre.0.clear();
    pre.0.extend(
        bodies
            .iter()
            .filter(|(_, rb, _)| rb.is_dynamic())
            .map(|(e, _, v)| (e, v.0)),
    );
}

/// A player's mass for impacts (kg; Source's player shadow).
pub(super) const PLAYER_MASS: f32 = 85.0;
/// A prop and a player meet as two contacts in CS:S (measured 2026-10-17,
/// prop_damage.md 6 and Q13, `tools/css_probe/impacts.py`); each counts
/// as the prop losing this share of its speed into the player, by the
/// prop's mass (kg): (mass, first, second). Fitted to the landings: a
/// 30 kg crate at 576, 848 and 1233 in/s hurt the player 5 + 5, 5 + 10 and
/// 10 + 20 (shares 0.31-0.35, then 0.43-0.52); a 500 kg cupboard at 259
/// to 1522 in/s 5 + 50 up to 500 + 500 (0.262-0.274, then 0.62-0.73;
/// 0.276 since props feel air drag, which slows the cupboard about
/// 0.6 in/s before it lands at 100 in/s and took its first share under
/// the 10-point band).
/// Between and beyond, by the logarithm of the mass, clamped.
pub(super) const PLAYER_CONTACT_SHARES: [(f32, [f32; 2]); 2] = [(30.0, [0.335, 0.48]), (500.0, [0.276, 0.67])];

/// `PLAYER_CONTACT_SHARES` for a prop of `mass` kg.
pub(super) fn player_contact_shares(mass: f32) -> [f32; 2] {
    let [(m0, a), (m1, b)] = PLAYER_CONTACT_SHARES;
    let t = ((mass.max(1e-3).ln() - m0.ln()) / (m1.ln() - m0.ln())).clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}
/// Damage (health points) that breaks any breakable: an impact on one
/// that breaks on the first physics impact (flag 512).
pub(super) const BREAK_NOW: f32 = 1.0e6;
/// An impact is measured over this many physics steps from its first
/// contact: avian's contacts are speculative and soft, so a body is
/// stopped over a few steps where Source's stops it in one.
pub(super) const IMPACT_STEPS: u32 = 4;

/// Who an impact damages.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Receiver {
    /// A logic prop: its table, energy scale and mass.
    Prop(crate::logic::prop_damage::ImpactTable, f32, f32),
    /// A breakable brush (static): its table, energy scale and mass, and
    /// whether the first impact breaks it.
    Breakable(crate::logic::prop_damage::ImpactTable, f32, f32, bool),
    Player,
}

/// An impact being measured: both bodies' velocities when it began and
/// the slowest each has been since (engine space).
#[derive(Clone, Debug)]
pub(super) struct Impact {
    me: Entity,
    other: Entity,
    receiver: Receiver,
    before: [Vec3; 2],
    slowest: [Vec3; 2],
    steps: u32,
    /// The contact's normal when it began, from `me` toward `other`
    /// (engine space; None: no contact points yet).
    normal: Option<Vec3>,
}

#[derive(Resource, Default)]
pub(super) struct Impacts(Vec<Impact>);

/// Physics impacts (prop_damage.md 5, 6): a new contact of a logic prop's
/// body damages it by the speed the collision took away (its table and
/// energy scale), and a moving prop hitting a player damages the player
/// (the player table, attacker = the prop), as crush damage applied next
/// tick. A prop and a player meet as two contacts (`PLAYER_CONTACT_SHARES`),
/// each hurting both: the prop by the player (attacker = the player) too.
#[allow(clippy::too_many_arguments)]
pub(super) fn impacts(
    mut started: MessageReader<CollisionStart>,
    logic: Option<Res<Logic>>,
    pre: Res<PreStep>,
    mut pending: ResMut<Impacts>,
    bodies: Query<(
        &RigidBody,
        &LinearVelocity,
        Option<&ComputedMass>,
        Has<crate::map::PhysicsProp>,
    )>,
    characters: Query<&MovementState, With<Intent>>,
    shadows: Query<&crate::map::prop_physics::PhysicsShadow>,
    positions: Query<&Position>,
    collisions: avian3d::prelude::Collisions,
    mut damage: MessageWriter<Damage>,
) {
    let Some(logic) = logic else {
        started.clear();
        pending.0.clear();
        return;
    };
    let scale = logic.scale;
    let velocity = |e: Entity| match bodies.get(e) {
        Ok((rb, v, ..)) if rb.is_dynamic() => v.0,
        _ => Vec3::ZERO,
    };
    // A player's physics shadow touches props for its player.
    let player = |e: Entity| shadows.get(e).map_or(e, |s| s.owner);
    // New contacts start being measured.
    for s in started.read() {
        let (b1, b2) = (
            player(s.body1.unwrap_or(s.collider1)),
            player(s.body2.unwrap_or(s.collider2)),
        );
        if b1 == b2 {
            continue;
        }
        for (me, other) in [(b1, b2), (b2, b1)] {
            let receiver = if let Some((id, _)) = logic.props.iter().find(|(_, n)| *n == me)
                && let Some((table, energy, mass)) = logic.world.prop_impact(*id)
            {
                // A player's contact counts too (prop_damage.md Q13; only
                // prop_physics reach players: multiplayer props never touch
                // them, `map::prop_physics::ShadowContacts`).
                Receiver::Prop(table, energy, mass.max(0.1))
            } else if let Some((id, _)) = logic.nodes.iter().find(|(_, n)| *n == me)
                && let Some((table, energy, mass, instant)) = logic.world.breakable_impact(*id)
                // Only physics objects (players break them by touch).
                && bodies.get(other).is_ok_and(|(rb, ..)| rb.is_dynamic())
                && !characters.contains(other)
            {
                Receiver::Breakable(table, energy, mass, instant)
            } else if characters.get(me).is_ok_and(|s| s.ground != Some(other))
                && bodies.get(other).is_ok_and(|(rb, _, _, prop)| rb.is_dynamic() && prop)
            {
                Receiver::Player
            } else {
                continue;
            };
            if pending.0.iter().any(|i| i.me == me && i.other == other) {
                continue;
            }
            let before = [me, other].map(|e| pre.0.get(&e).copied().unwrap_or_else(|| velocity(e)));
            // The manifolds' normals point from collider1 to collider2.
            let normal = collisions.get(s.collider1, s.collider2).and_then(|pair| {
                let n: Vec3 = pair.manifolds.iter().map(|m| m.normal * m.points.len() as f32).sum();
                let from_first = player(pair.body1.unwrap_or(pair.collider1)) == me;
                n.try_normalize().map(|n| if from_first { n } else { -n })
            });
            pending.0.push(Impact {
                me,
                other,
                receiver,
                before,
                slowest: before,
                steps: 0,
                normal,
            });
        }
    }
    // Measure; done ones deal their damage.
    let mut done = Vec::new();
    pending.0.retain_mut(|i| {
        for (k, e) in [i.me, i.other].into_iter().enumerate() {
            let v = velocity(e);
            if v.length_squared() < i.slowest[k].length_squared() {
                i.slowest[k] = v;
            }
        }
        i.steps += 1;
        if i.steps >= IMPACT_STEPS {
            done.push(i.clone());
            false
        } else {
            true
        }
    });
    for i in done {
        let fixed = |e: Entity| !bodies.get(e).is_ok_and(|(rb, ..)| rb.is_dynamic());
        let mass = |e: Entity| {
            bodies
                .get(e)
                .ok()
                .and_then(|(_, _, m, _)| m.map(|m| m.value()))
                .unwrap_or(1.0)
        };
        let side = |k: usize, e: Entity, m: f32| crate::logic::prop_damage::Impactor {
            mass: m,
            before: engine_to_entity(i.before[k], scale),
            after: engine_to_entity(i.slowest[k], scale),
            fixed: fixed(e),
        };
        // A player (through its shadow): an 85 kg body whose own speed
        // change isn't measured.
        let by_player = characters.contains(i.other);
        let theirs = if by_player {
            crate::logic::prop_damage::Impactor {
                mass: PLAYER_MASS,
                ..default()
            }
        } else {
            side(1, i.other, mass(i.other))
        };
        // A prop meeting a player counts as two contacts (CS:S, measured),
        // each the prop losing a share of its speed (by its mass).
        let player_contact = by_player || i.receiver == Receiver::Player;
        let shares: Vec<f32> = if player_contact {
            let prop_mass = match i.receiver {
                Receiver::Prop(_, _, m) => m,
                _ => theirs.mass,
            };
            player_contact_shares(prop_mass).to_vec()
        } else {
            vec![1.0]
        };
        let point = positions.get(i.me).map_or(Vec3::ZERO, |p| p.0);
        // Where the prop loses speed into the player: along the contact's
        // normal (entity space), when it has one.
        let toward = |n: Option<Vec3>| n.map(|n| engine_to_entity(n, 1.0).normalize_or_zero());
        for &share in &shares {
            let (amount, force) = match i.receiver {
                Receiver::Prop(table, energy, m) => {
                    let mine = crate::logic::prop_damage::Impactor {
                        // A frozen prop still counts its own mass.
                        fixed: false,
                        ..side(0, i.me, m)
                    };
                    let mine = if by_player {
                        mine.lost_toward(share, toward(i.normal))
                    } else {
                        mine.share(share)
                    };
                    let amount = crate::logic::prop_damage::impact_damage(&table, energy, mine, theirs, false);
                    (amount, velocity(i.me) * m)
                }
                Receiver::Breakable(table, energy, m, instant) => {
                    // A static brush: its speed never changes.
                    let mine = crate::logic::prop_damage::Impactor { mass: m, ..default() };
                    let amount = crate::logic::prop_damage::impact_damage(&table, energy, mine, theirs, false);
                    // "Break immediately on physics": any impact is lethal.
                    let amount = if instant { amount.max(BREAK_NOW) } else { amount };
                    (amount, i.slowest[1] * theirs.mass)
                }
                Receiver::Player => {
                    let player = crate::logic::prop_damage::Impactor {
                        mass: PLAYER_MASS,
                        ..default()
                    };
                    let amount = crate::logic::prop_damage::impact_damage(
                        &crate::logic::prop_damage::PLAYER_TABLE,
                        1.0,
                        player,
                        // The prop moves toward the player against the
                        // normal (from the player to it).
                        theirs.lost_toward(share, toward(i.normal.map(|n| -n))),
                        true,
                    );
                    (amount, Vec3::ZERO)
                }
            };
            if amount > 0.0 && bodies.contains(i.me) | characters.contains(i.me) {
                damage.write(Damage {
                    target: i.me,
                    attacker: (by_player || !theirs.fixed).then_some(i.other),
                    amount: amount / 100.0,
                    point,
                    dir: i.before[1].normalize_or(Vec3::NEG_Y),
                    hitgroup: Hitgroup::Generic,
                    kind: DamageKind::Crush,
                    weapon: None,
                    force,
                });
            }
        }
    }
}

/// Physics stress on players (prop_damage.md 6, "Stress"): what a
/// player's received stress is scaled by (`player_stress_scale`, over its
/// 85 kg), the stress past which it is crushed, the crush damage and the
/// mass of a "large object" one of its contacts must be.
pub(super) const PLAYER_STRESS_SCALE: f32 = 0.125;
pub(super) const STRESS_LIMIT: f32 = 5.0;
pub(super) const STRESS_DAMAGE: f32 = 200.0;
pub(super) const STRESS_LARGE_MASS: f32 = 500.0;
/// Avian 0.7's contact `normal_impulse` adds each substep's impulse after
/// both its solve and its relax pass (`normal_part.rs`, `total_impulse`):
/// twice what the step applied (a 4000 kg box resting on a player read
/// 7900 kg).
pub(super) const AVIAN_IMPULSE_PASSES: f32 = 2.0;

/// Player stress (prop_damage.md 6): each physics step, the force (kg
/// of weight) moving props press on a player with, through its own
/// collider or its physics shadow, not counting what it stands on; past
/// `STRESS_LIMIT` (scaled by `PLAYER_STRESS_SCALE` over its 85 kg) with a
/// `STRESS_LARGE_MASS` prop among them: `STRESS_DAMAGE` crush from the
/// world, pushing straight down.
#[allow(clippy::type_complexity)]
pub(super) fn stress(
    logic: Option<Res<Logic>>,
    collisions: avian3d::prelude::Collisions,
    time: Res<Time>,
    gravity: Option<Res<avian3d::prelude::Gravity>>,
    characters: Query<(Entity, &MovementState, &Health, Option<&crate::map::prop_physics::Shadowed>), With<Intent>>,
    props: Query<(&RigidBody, &crate::map::PhysicsProp)>,
    of: Query<&avian3d::prelude::ColliderOf>,
    impacts: Res<Impacts>,
    mut damage: MessageWriter<Damage>,
) {
    if logic.is_none() {
        return;
    }
    let dt = time.delta_secs();
    let g = gravity.map_or(0.0, |g| g.0.length());
    if dt <= 0.0 || g <= 0.0 {
        return;
    }
    let body = |e: Entity| of.get(e).map_or(e, |o| o.body);
    for (player, state, health, shadow) in &characters {
        if health.current <= 0.0 {
            continue;
        }
        let mut received = 0.0;
        let mut large = false;
        for me in std::iter::once(player).chain(shadow.map(|s| s.0)) {
            for pair in collisions.collisions_with(me) {
                if !pair.flags.contains(avian3d::prelude::ContactPairFlags::TOUCHING) {
                    continue;
                }
                let other = body(if body(pair.collider1) == me {
                    pair.collider2
                } else {
                    pair.collider1
                });
                // What it stands on doesn't count, nor a contact still
                // being measured as an impact (its first steps: the blow
                // of a landing is impact damage; CS:S's landings of the
                // 500 kg cupboard didn't crush, measured).
                if state.ground == Some(other) || impacts.0.iter().any(|i| i.me == player && i.other == other) {
                    continue;
                }
                let Ok((rb, prop)) = props.get(other) else { continue };
                if !rb.is_dynamic() {
                    continue;
                }
                received += pair.total_normal_impulse_magnitude() / AVIAN_IMPULSE_PASSES / dt / g;
                large |= prop.mass >= STRESS_LARGE_MASS;
            }
        }
        if large && received * PLAYER_STRESS_SCALE / PLAYER_MASS > STRESS_LIMIT {
            damage.write(Damage {
                target: player,
                attacker: None,
                amount: STRESS_DAMAGE / 100.0,
                point: Vec3::ZERO,
                dir: Vec3::NEG_Y,
                hitgroup: Hitgroup::Generic,
                kind: DamageKind::Crush,
                weapon: None,
                force: Vec3::NEG_Y * received * g * dt,
            });
        }
    }
}

/// Outputs game rules ask a map entity to fire (a bomb target's
/// `BombExplode`), with the player as activator.
pub(super) fn fire_outputs(mut asks: MessageReader<crate::map::entities::FireEntityOutput>, logic: Option<ResMut<Logic>>) {
    let Some(mut logic) = logic else {
        asks.clear();
        return;
    };
    for a in asks.read() {
        let id = logic
            .world
            .ids()
            .into_iter()
            .find(|id| logic.world.get(*id).is_some_and(|e| e.map_index == Some(a.map_index)));
        if let Some(id) = id {
            let who = a.activator.map(crate::logic::world::Who::Player);
            logic.world.fire_output(id, &a.output, who, crate::logic::Value::Void);
            // A bomb target's legacy `target` is used when it explodes
            // (spec objectives.md 6.3, *hyp.*).
            let target = logic.world.get(id).and_then(|e| e.kv("target")).map(str::to_string);
            if a.output.eq_ignore_ascii_case("BombExplode")
                && let Some(t) = target.filter(|t| !t.is_empty())
            {
                logic.world.queue_input(&t, "Use", crate::logic::Value::Void, 0.0, who);
            }
        }
    }
}

/// The logic's entities for tools (`map::logic_view::LogicInspect`).
pub(super) fn inspect_entities(world: &World) -> Option<Vec<crate::map::logic_view::LogicEntity>> {
    let logic = world.get_resource::<Logic>()?;
    Some(
        logic
            .world
            .ids()
            .into_iter()
            .filter_map(|id| logic.world.get(id))
            .map(|e| crate::map::logic_view::LogicEntity {
                name: e.targetname.clone(),
                class: e.classname.clone(),
                origin: e.origin,
                keys: e.keyvalues.clone(),
                outputs: e
                    .outputs
                    .iter()
                    .flat_map(|(o, cs)| {
                        cs.iter().map(move |c| {
                            format!(
                                "{o} -> {} {}{} after {} s{}",
                                c.target,
                                c.input,
                                c.param.as_ref().map_or(String::new(), |p| format!(" ({p})")),
                                c.delay,
                                if c.times > 0 { format!(", {} more times", c.times) } else { String::new() }
                            )
                        })
                    })
                    .collect(),
            })
            .collect(),
    )
}

/// The latest `n` outputs fired, newest first, for tools.
pub(super) fn inspect_fired(world: &World, n: usize) -> Vec<String> {
    let Some(logic) = world.get_resource::<Logic>() else {
        return Vec::new();
    };
    logic
        .world
        .fired
        .iter()
        .rev()
        .take(n)
        .map(|(tick, id, out)| {
            let who = logic.world.get(*id).map_or("?".to_string(), |e| {
                if e.targetname.is_empty() { e.classname.clone() } else { e.targetname.clone() }
            });
            format!("tick {tick}: {who} {out}")
        })
        .collect()
}

