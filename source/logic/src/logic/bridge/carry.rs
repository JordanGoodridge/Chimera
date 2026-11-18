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
/// A mover a network client moves itself this tick (`net::movers`), as
/// the server's logic steps it: where it was (entity space, angles in
/// degrees) and how far it moves and turns.
#[derive(Clone, Copy, Debug)]

pub struct CarriedMover<'a> {
    pub node: Entity,
    /// Its map entity index (`map::MapBrushEntity`).
    pub index: u32,
    /// Its hulls (`map::MapEntity::hulls`, entity space).
    pub hulls: &'a [crate::map::MapHull],
    pub origin: Vec3,
    pub angles: Vec3,
    pub d: Vec3,
    pub da: Vec3,
    pub solid: bool,
    pub physics_solid: bool,
    pub unblockable: bool,
}

/// A mover's world brushes at a pose (entity space), as the logic keeps
/// them.
pub fn mover_brushes_at(hulls: &[crate::map::MapHull], origin: Vec3, angles: Vec3) -> Vec<MapBrush> {
    let rot = entity_rotation(angles);
    hulls.iter().map(|h| crate::logic::world::place_hull(h, rot, origin)).collect()
}

/// A network client predicting its own player (`net::movers`): the push
/// of this tick's mover steps (`movers`, in map order) on `player`, as
/// the server's logic does it before movement (riders carried, players in
/// the way shoved; `movers::push_players`, the same code). A step the
/// player would block is left out (the server's mover would stop or
/// reverse: the next correction says how). Returns whether it moved.
pub fn carry_player(world: &mut World, player: Entity, scale: f32, movers: &[CarriedMover]) -> bool {
    let ground_of = |g: Entity| {
        movers.iter().find(|m| m.node == g).map(|m| EntId {
            index: m.index,
            generation: 0,
        })
    };
    let before = cached_query::<CharacterQuery, (), _>(world, |q, world| {
        q.get(world, player).ok().map(|row| as_player(row, scale, ground_of))
    });
    let Some(before) = before else {
        return false;
    };
    let mut players = vec![before.clone()];
    let mut solids: Vec<Option<Vec<MapBrush>>> = movers
        .iter()
        .map(|m| m.solid.then(|| mover_brushes_at(m.hulls, m.origin, m.angles)))
        .collect();
    {
        let col = WorldCollision {
            brushes: world.get_resource::<MapBrushes>(),
            terrain: world.get_resource::<MapTerrain>(),
            scale,
        };
        for (k, m) in movers.iter().enumerate() {
            let (o_new, a_new) = (m.origin + m.d, m.angles + m.da);
            if m.solid && (m.d != Vec3::ZERO || m.da != Vec3::ZERO) {
                let moved = mover_brushes_at(m.hulls, o_new, a_new);
                let others: Vec<&MapBrush> = solids
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != k)
                    .filter_map(|(_, b)| b.as_ref())
                    .flatten()
                    .collect();
                let step = crate::logic::movers::PushStep {
                    id: EntId {
                        index: m.index,
                        generation: 0,
                    },
                    origin: m.origin,
                    angles: m.angles,
                    d: m.d,
                    da: m.da,
                    physics_solid: m.physics_solid,
                    unblockable: m.unblockable,
                };
                let _ = crate::logic::movers::push_players(&mut players, &step, &moved, &others, &col);
                solids[k] = Some(moved);
            }
        }
    }
    let moved = players[0].moved;
    write_back(world, scale, std::slice::from_ref(&before), &players);
    moved
}

/// Write back what the logic changed on players.
pub(super) fn write_back(world: &mut World, scale: f32, before: &[Player], after: &[Player]) {
    for p in after {
        let Some(old) = before.iter().find(|b| b.entity == p.entity) else {
            continue;
        };
        let Ok(mut e) = world.get_entity_mut(p.entity) else {
            continue;
        };
        if p.moved || p.teleported {
            if let Some(mut t) = e.get_mut::<Transform>() {
                t.translation = entity_to_engine(p.origin, scale);
            }
        }
        if p.velocity != old.velocity
            && let Some(mut v) = e.get_mut::<Velocity>()
        {
            v.0 = entity_to_engine(p.velocity, scale);
        }
        if p.view != old.view
            && let Some(mut i) = e.get_mut::<Intent>()
        {
            i.yaw = (p.view.y - 90.0).to_radians().rem_euclid(std::f32::consts::TAU);
            i.pitch = (-p.view.x).to_radians().clamp(-89f32.to_radians(), 89f32.to_radians());
        }
        if p.base_velocity != old.base_velocity || p.base_touched != old.base_touched || p.unground || p.teleported {
            let unground = p.unground || p.teleported;
            let base = BaseVelocity {
                velocity: entity_to_engine(p.base_velocity, scale),
                touched: p.base_touched,
                unground,
            };
            e.insert(base);
        }
        if p.gravity != old.gravity {
            e.insert(EntityGravity(p.gravity));
        }
    }
}

/// Mover nodes follow their logic entity; nodes of removed entities
/// (killed, broken) are hidden (a round restart shows them again).
pub(super) fn sync_movers(world: &mut World, logic: &mut Logic) {
    logic.nodes.retain(|(id, node)| {
        let alive = logic.world.get(*id).is_some();
        if !alive {
            hide_node(world, *node);
        }
        alive
    });
    let logic = &*logic;
    let nodes: std::collections::HashMap<EntId, Entity> = logic.nodes.iter().copied().collect();
    for (id, origin, angles, velocity, visible, solid) in logic.world.mover_poses() {
        let Some(node) = nodes.get(&id) else {
            continue;
        };
        // A brush riding a physics brush or prop touches no physics body
        // (it would shove the body carrying it) but shots still hit it
        // (cs_havana's monitor screen); players meet its brushes
        // (`MovingSolid`).
        let shootable = logic.world.shootable(id);
        let rides = logic.world.rides_body(id);
        let panes = logic.world.window(id).filter(|w| w.window_broken).map(|w| {
            let e = logic.world.get(id).unwrap();
            let s = logic.scale;
            let n = w.normal;
            let depth = e
                .hulls
                .iter()
                .flat_map(|h| &h.points)
                .map(|p| (*p - w.corner).dot(n))
                .fold((f32::MAX, f32::MIN), |(lo, hi), d| (lo.min(d), hi.max(d)));
            let depth = if depth.0 > depth.1 {
                (0.0, 0.0)
            } else {
                (depth.0 * s, depth.1 * s)
            };
            BrushPanes {
                corner: entity_to_engine(w.corner, s),
                u: entity_to_engine(w.u, s),
                v: entity_to_engine(w.v, s),
                depth,
                cols: w.cols,
                rows: w.rows,
                broken: w.broken.clone(),
            }
        });
        let Ok(mut e) = world.get_entity_mut(*node) else {
            continue;
        };
        let transform = Transform::from_translation(entity_to_engine(origin, logic.scale))
            .with_rotation(rotation_to_engine(entity_rotation(angles)));
        // A broken window's panes are for shots only
        // (`map::breakables::update_panes`): every game's characters pass
        // them, and touching them shatters them (`touch_breakables`).
        let layers = if panes.is_some() {
            CollisionLayers::new(crate::core::TRACE_ONLY_LAYER, LayerMask::NONE)
        } else if rides {
            CollisionLayers::new(LayerMask::DEFAULT, LayerMask::NONE)
        } else {
            CollisionLayers::default()
        };
        if e.get::<CollisionLayers>().is_some_and(|l| *l != layers) || (rides && !e.contains::<CollisionLayers>()) {
            e.insert(layers);
        }
        let mut moved = true;
        if let Some(mut t) = e.get_mut::<Transform>() {
            moved = *t != transform;
            if moved {
                *t = transform;
            }
        }
        set_node_shown(&mut e, visible);
        // Its render colour (rendercolor, the Color input).
        let look = logic.world.get(id).map_or_else(Default::default, |e| e.render_look());
        let frame = logic.world.get(id).map_or(0, |e| e.texture_frame);
        let tint = crate::map::BrushTint(look, frame);
        if e.get::<crate::map::BrushTint>()
            .map_or(!tint.is_white(), |t| *t != tint)
        {
            e.insert(tint);
        }
        // Its brushes follow its pose: rebuilt (and the component written,
        // which movement reads) only when it moved or changed.
        let solids = logic.world.mover_solid(id);
        let velocity = entity_to_engine(velocity, logic.scale);
        let same = !moved
            && e.get::<MovingSolid>().is_some_and(|m| {
                m.solid == solid && m.velocity == velocity && m.brushes.len() == solids.map_or(0, |b| b.len())
            });
        if !same {
            let brushes: Vec<MapBrush> = solids
                .map(|b| b.iter().map(|b| brush_to_engine(b, logic.scale)).collect())
                .unwrap_or_default();
            e.insert(MovingSolid {
                brushes,
                velocity,
                solid,
            });
        }
        // Shots and physics pass what is not there (a broken breakable,
        // a disabled func_brush); a broken window keeps its panes.
        if let Some(p) = panes {
            if e.get::<BrushPanes>() != Some(&p) {
                e.insert(p);
            }
        } else if shootable == e.contains::<ColliderDisabled>() {
            if shootable {
                e.remove::<ColliderDisabled>();
            } else {
                e.insert(ColliderDisabled);
            }
        }
    }
}

