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

pub fn set_node_shown(e: &mut EntityWorldMut, visible: bool) {
    let hidden = e.contains::<LogicHidden>();
    if visible {
        let culled = e.get::<VisClusters>().is_some_and(|v| !v.potentially_visible);
        if hidden {
            e.remove::<LogicHidden>();
        }
        let want = if culled { Visibility::Hidden } else { Visibility::Inherited };
        if e.get::<Visibility>() != Some(&want) {
            e.insert(want);
        }
    } else if !hidden || e.get::<Visibility>() != Some(&Visibility::Hidden) {
        e.insert((LogicHidden, Visibility::Hidden));
    }
}

/// Pair every prop node (`map::PropEntity`) with its logic prop (`ids` in
/// map order). They are shown and solid again; with `restart`, a moved
/// physics prop goes back where the map placed it, at rest.
pub(super) fn attach_props(world: &mut World, logic: &LogicWorld, ids: &[EntId], restart: bool) -> Vec<(EntId, Entity)> {
    let all: Vec<(Entity, usize, Option<Transform>)> = world
        .query::<(Entity, &PropEntity, Option<&PropHome>)>()
        .iter(world)
        .map(|(e, p, h)| (e, p.0, h.map(|h| h.0)))
        .collect();
    let states = logic.prop_states();
    let mut out = Vec::new();
    for (node, index, home) in all {
        let Some(id) = ids.get(index).copied() else { continue };
        let Some(state) = states.iter().find(|s| s.id == id) else {
            // Taken out of the map (a template's member): never there.
            if logic.get(id).is_none() {
                set_prop_shown(world, node, false, false, false);
            }
            continue;
        };
        let (visible, solid, damageable) = (state.visible, state.solid, state.damageable);
        if restart
            && let Some(home) = home
            && let Ok(mut e) = world.get_entity_mut(node)
            && e.contains::<RigidBody>()
        {
            if let Some(mut t) = e.get_mut::<Transform>() {
                *t = home;
            }
            if let Some(mut p) = e.get_mut::<Position>() {
                p.0 = home.translation;
            }
            if let Some(mut r) = e.get_mut::<Rotation>() {
                *r = Rotation::from(home.rotation);
            }
            if let Some(mut v) = e.get_mut::<LinearVelocity>() {
                v.0 = Vec3::ZERO;
            }
            if let Some(mut v) = e.get_mut::<AngularVelocity>() {
                v.0 = Vec3::ZERO;
            }
        }
        if restart {
            crate::map::prop_physics::reset(world, node, state.asleep);
        }
        set_prop_shown(world, node, visible, solid, true);
        set_prop_look(world, node, state, logic.round);
        if let Ok(mut e) = world.get_entity_mut(node) {
            if damageable {
                e.insert(Damageable);
            } else {
                e.remove::<Damageable>();
            }
        }
        out.push((id, node));
    }
    out
}

/// Show or hide a prop node and turn its collision (its own collider and
/// its children's) on or off; `exists` false also stops its body.
pub(super) fn set_prop_shown(world: &mut World, node: Entity, visible: bool, solid: bool, exists: bool) {
    let Ok(e) = world.get_entity(node) else { return };
    let hidden = e.contains::<LogicHidden>();
    if visible == hidden {
        let shown_by_vis = e.get::<VisClusters>().is_none_or(|v| v.potentially_visible);
        let mut e = world.entity_mut(node);
        if visible {
            e.remove::<LogicHidden>();
            if shown_by_vis {
                e.insert(Visibility::Inherited);
            }
        } else {
            e.insert((LogicHidden, Visibility::Hidden));
        }
    }
    // The body comes back before its colliders do: avian's query tree
    // copies "body disabled" into a collider's proxy when the collider
    // joins it and doesn't clear it when RigidBodyDisabled goes away
    // (`tests::avian_still_leaves_body_disabled_on_the_proxy`). The broad
    // phase skips proxies so marked; with avian 0.7 a stale flag also made
    // each new contact pair of that collider link into an island twice
    // (games/cs_source/tests/it/map_logic.rs
    // `restored_prop_contacts_join_islands_once`).
    let enable_body = {
        let mut e = world.entity_mut(node);
        let enable = exists && e.contains::<RigidBody>() && e.contains::<RigidBodyDisabled>();
        if enable {
            e.remove::<RigidBodyDisabled>();
        }
        enable
    };
    let colliders: Vec<Entity> = std::iter::once(node)
        .chain(world.get::<Children>(node).map(|c| c.to_vec()).unwrap_or_default())
        .filter(|c| world.get::<Collider>(*c).is_some())
        .collect();
    for c in colliders {
        let mut e = world.entity_mut(c);
        // Only on change: a re-inserted ColliderDisabled re-adds the
        // collider to avian's query tree (see `hide_node`).
        if solid == e.contains::<ColliderDisabled>() {
            if solid {
                e.remove::<ColliderDisabled>();
            } else {
                e.insert(ColliderDisabled);
            }
        } else if solid && enable_body {
            // Solid all along under a disabled body: rejoin the tree so
            // its proxy forgets the disabled body.
            e.insert(ColliderDisabled).remove::<ColliderDisabled>();
        }
    }
    let mut e = world.entity_mut(node);
    // A prop's own brushes (movement) follow its collision.
    if let Some(mut m) = e.get_mut::<MovingSolid>()
        && m.solid != solid
    {
        m.solid = solid;
    }
    if !exists && e.contains::<RigidBody>() && !e.contains::<RigidBodyDisabled>() {
        e.insert(RigidBodyDisabled);
    }
    if !exists {
        e.remove::<Damageable>();
    }
    // A map ragdoll's bodies follow (`map::placed_ragdoll::sync`).
    if let Some(mut r) = e.get_mut::<crate::map::placed_ragdoll::PlacedRagdoll>()
        && (r.gone == exists || r.solid != solid)
    {
        r.gone = !exists;
        r.solid = solid;
    }
}

/// Placed weapons the logic removed (`map::entities::RemovedWeapons`;
/// written only when it changes).
pub(super) fn sync_removed_weapons(world: &mut World, logic: &Logic) {
    let gone: Vec<usize> = logic
        .weapons
        .iter()
        .copied()
        .filter(|i| !logic.world.map_entity_alive(*i))
        .collect();
    let now = world.get_resource::<crate::map::entities::RemovedWeapons>();
    if now.is_none_or(|n| n.0 != gone) {
        world.insert_resource(crate::map::entities::RemovedWeapons(gone));
    }
}

/// Physics clips (`map::physics_clip`) follow their logic entity: solid
/// to physics objects while it exists and is enabled; a filtered one
/// blocks the props and brush entities its filter passes (of those, only
/// physics bodies meet it). Components
/// are written only when they change.
pub(super) fn sync_physics_clips(world: &mut World, logic: &Logic) {
    use crate::map::physics_clip::{ClipPasses, PhysicsClip};
    let clips: Vec<(Entity, usize)> = world
        .query::<(Entity, &PhysicsClip)>()
        .iter(world)
        .map(|(e, c)| (e, c.0))
        .collect();
    for (clip, index) in clips {
        let w = &logic.world;
        let id = w.by_map_index(index).filter(|id| w.get(*id).is_some_and(|e| !e.killed));
        let on = id.is_some_and(|id| {
            matches!(w.get(id).map(|e| &e.class), Some(crate::logic::classes::Class::Part(p)) if p.on)
        });
        let filter = id
            .and_then(|id| w.get(id))
            .and_then(|e| e.kv("filtername"))
            .filter(|f| !f.trim().is_empty())
            .map(|f| w.find(f));
        let passes = ClipPasses(filter.map(|f| {
            // Props and physics brushes, templates' copies too (the
            // football's ball is a func_physbox a template spawns).
            logic
                .props
                .iter()
                .chain(&logic.nodes)
                .filter(|(p, _)| f.is_none_or(|f| crate::logic::classes::filter_passes(w, f, Some(Who::Ent(*p)))))
                .map(|(_, node)| *node)
                .collect()
        }));
        let Ok(mut e) = world.get_entity_mut(clip) else { continue };
        if on == e.contains::<ColliderDisabled>() {
            if on {
                e.remove::<ColliderDisabled>();
            } else {
                e.insert(ColliderDisabled);
            }
        }
        if e.get::<ClipPasses>() != Some(&passes) {
            e.insert(passes);
        }
    }
}

/// Prop nodes follow their logic prop: hidden and not solid once it is
/// gone (broken, killed) until a round restart, or as its inputs say.
pub(super) fn sync_props(world: &mut World, logic: &mut Logic) {
    let states = logic.world.prop_states();
    for (id, node) in logic.props.clone() {
        match states.iter().find(|s| s.id == id) {
            Some(state) => {
                set_prop_shown(world, node, state.visible, state.solid, true);
                set_prop_look(world, node, state, logic.world.round);
            }
            None => set_prop_shown(world, node, false, false, false),
        }
    }
}

/// A prop node shows the logic prop's skin, body group and sequence
/// (`map::PropLook`, `map::PropSequence`; written only when they change).
pub(super) fn set_prop_look(world: &mut World, node: Entity, state: &crate::logic::props::PropState, round: u32) {
    let Ok(mut e) = world.get_entity_mut(node) else { return };
    let look = PropLook {
        skin: state.skin,
        body_group: state.body_group,
    };
    if e.get::<PropLook>() != Some(&look) {
        e.insert(look);
    }
    let sequence = PropSequence {
        play: state.sequence.clone(),
        default: state.default_sequence.clone(),
        round,
    };
    if e.get::<PropSequence>() != Some(&sequence) {
        e.insert(sequence);
    }
}

/// Sprites and dust volumes (`map::EntityPart`) follow their entity: on,
/// off, or gone (killed) until a round restart; light styles follow the
/// lights (`map::LightStyles`), areaportals their doors and inputs
/// (`map::AreaPortalStates`), occluders their inputs
/// (`map::OccluderStates`).
pub(super) fn sync_visuals(world: &mut World, logic: &Logic) {
    let mut states = logic.world.part_states();
    states.sort_by_key(|s| s.0);
    // Kept query states (`query_cache`): this runs every tick.
    cached_query::<&mut EntityPart, (), _>(world, |parts, world| {
        for mut part in parts.iter_mut(world) {
            let want = match states.binary_search_by_key(&part.entity, |s| s.0) {
                Ok(i) => (states[i].2, true),
                Err(_) => (false, false),
            };
            if (part.on, part.exists) != want {
                part.on = want.0;
                part.exists = want.1;
            }
        }
    });
    // Steam jets' value inputs (meters; the Rate input does nothing).
    let mut jets = logic.world.steam_jets();
    jets.sort_by_key(|j| j.0);
    let scale = logic.scale;
    let mut changes = Vec::new();
    cached_query::<(Entity, &EntityPart, Option<&crate::map::SteamInputs>), (), _>(world, |parts, world| {
        for (e, part, have) in parts.iter(world) {
            let want = jets
                .binary_search_by_key(&part.entity, |j| j.0)
                .ok()
                .map(|i| crate::map::SteamInputs {
                    length: jets[i].1.length.map(|v| v * scale),
                    spread: jets[i].1.spread.map(|v| v * scale),
                    speed: jets[i].1.speed.map(|v| v * scale),
                });
            if want.as_ref() != have {
                changes.push((e, want));
            }
        }
    });
    for (e, want) in changes {
        match want {
            Some(w) => world.entity_mut(e).insert(w),
            None => world.entity_mut(e).remove::<crate::map::SteamInputs>(),
        };
    }
    // Parts whose entity follows a parent go where it is now (a trail
    // behind a spinning prop, a sprite on a lift). Their visibility
    // clusters were where they spawned: dropped once they move.
    let mut poses = logic.world.part_poses();
    if !poses.is_empty() {
        poses.sort_by_key(|p| p.0);
        let scale = logic.scale;
        let mut moved = Vec::new();
        let mut parts =
            world.query_filtered::<(Entity, &EntityPart, &mut Transform), With<crate::map::FollowsEntity>>();
        for (e, part, mut t) in parts.iter_mut(world) {
            let Ok(i) = poses.binary_search_by_key(&part.entity, |p| p.0) else {
                continue;
            };
            let (_, origin, angles) = poses[i];
            let at = crate::map::entities::entity_to_engine(origin, scale);
            let rotation = crate::map::entities::rotation_to_engine(crate::map::entities::entity_rotation(angles));
            if t.translation.distance_squared(at) > 1e-10 || t.rotation.dot(rotation).abs() < 1.0 - 1e-7 {
                t.translation = at;
                t.rotation = rotation;
                moved.push(e);
            }
        }
        for e in moved {
            let mut part = world.entity_mut(e);
            if part.take::<crate::map::VisClusters>().is_some() && !part.contains::<LogicHidden>() {
                part.insert(Visibility::Inherited);
            }
        }
    }
    // How players are drawn (invisibility, colours).
    for (player, look) in &logic.world.player_looks {
        if let Ok(mut e) = world.get_entity_mut(*player)
            && e.get::<crate::map::RenderLook>() != Some(look)
        {
            e.insert(*look);
        }
    }
    let styles = LightStyles {
        styles: logic.world.light_styles().to_vec(),
    };
    if world.get_resource::<LightStyles>() != Some(&styles) {
        world.insert_resource(styles);
    }
    let tonemap = logic.world.tonemap();
    if world.get_resource::<crate::map::TonemapInputs>() != Some(tonemap) {
        world.insert_resource(tonemap.clone());
    }
    let portals = crate::map::AreaPortalStates {
        closed: logic.world.closed_area_portals(),
    };
    if world.get_resource::<crate::map::AreaPortalStates>() != Some(&portals) {
        world.insert_resource(portals);
    }
    let occluders = crate::map::OccluderStates {
        inactive: logic.world.inactive_occluders(),
    };
    if world.get_resource::<crate::map::OccluderStates>() != Some(&occluders) {
        world.insert_resource(occluders);
    }
}

/// The trigger_soundscape volumes the local player touches, for the
/// soundscape selection (`map::SoundscapeTouches`).
pub(super) fn sync_soundscapes(world: &mut World, logic: &Logic) {
    let local = first::<With<LocalPlayer>>(world);
    let touched: Vec<usize> = match local {
        Some(p) => logic
            .world
            .ids()
            .into_iter()
            .filter_map(|id| {
                let e = logic.world.get(id)?;
                if !e.classname.eq_ignore_ascii_case("trigger_soundscape") {
                    return None;
                }
                let crate::logic::classes::Class::Trigger(t) = &e.class else {
                    return None;
                };
                (t.enabled && t.touching.contains(&crate::logic::world::Who::Player(p))).then_some(e.map_index?)
            })
            .collect(),
        None => Vec::new(),
    };
    let want = SoundscapeTouches(Some(touched));
    if world.get_resource::<SoundscapeTouches>() != Some(&want) {
        world.insert_resource(want);
    }
    // env_soundscapes the Enable/Disable inputs left on.
    let enabled: Vec<usize> = logic
        .world
        .part_states()
        .into_iter()
        .filter(|(_, kind, on)| *kind == crate::logic::visuals::PartKind::Soundscape && *on)
        .map(|(i, ..)| i)
        .collect();
    let switches = SoundscapeSwitches(Some(enabled));
    if world.get_resource::<SoundscapeSwitches>() != Some(&switches) {
        world.insert_resource(switches);
    }
}

pub(super) fn restore_settings(world: &mut World) {
    let Some(restore) = world
        .get_resource_mut::<Logic>()
        .map(|mut l| std::mem::take(&mut l.restore))
    else {
        return;
    };
    if let Some(mut c) = world.get_resource_mut::<Console>() {
        for (name, value) in restore {
            info!("point_servercommand: map unloaded, {name} back to {value}");
            c.info(format!("point_servercommand: map unloaded, {name} back to {value}"));
            c.submit(format!("{name} {}", crate::console::quote(&value)));
        }
    }
}

/// Static collision through the map's brushes and terrain.
pub(super) struct WorldCollision<'a> {
    pub(super) brushes: Option<&'a MapBrushes>,
    pub(super) terrain: Option<&'a MapTerrain>,
    pub(super) scale: f32,
}

impl WorldCollision<'_> {
    /// Engine centre and half size of an entity-space box.
    fn engine_box(&self, mins: Vec3, maxs: Vec3, at: Vec3) -> (Vec3, Vec3) {
        let h = (maxs - mins) / 2.0;
        (
            entity_to_engine(at + (mins + maxs) / 2.0, self.scale),
            Vec3::new(h.x, h.z, h.y) * self.scale,
        )
    }

    fn near(&self, lo: Vec3, hi: Vec3) -> Vec<&MapBrush> {
        let mut out: Vec<&MapBrush> = self.brushes.map(|b| b.0.iter().collect()).unwrap_or_default();
        if let Some(t) = self.terrain {
            let mut idx = Vec::new();
            t.near(lo, hi, &mut idx);
            out.extend(idx.into_iter().map(|i| &t.brushes[i as usize]));
        }
        out
    }
}

impl Collision for WorldCollision<'_> {
    fn sweep(&self, mins: Vec3, maxs: Vec3, from: Vec3, to: Vec3) -> f32 {
        let (a, half) = self.engine_box(mins, maxs, from);
        let (b, _) = self.engine_box(mins, maxs, to);
        let eps = SWEEP_EPS * self.scale;
        self.near(a.min(b) - half, a.max(b) + half)
            .into_iter()
            .filter_map(|br| br.sweep_box(half, a, b, eps))
            .map(|(f, _)| f)
            .fold(1.0, f32::min)
    }

    fn solid(&self, mins: Vec3, maxs: Vec3, at: Vec3) -> bool {
        let (c, half) = self.engine_box(mins, maxs, at);
        let skin = SOLID_SKIN * self.scale;
        self.near(c - half, c + half)
            .into_iter()
            .any(|b| b.overlaps_box(c, half, skin))
    }
}

/// The static world owned (the logic keeps it for traces outside a
/// phase: fires dropping inside an input, their line-of-sight checks).
pub(super) struct StaticCollision {
    brushes: MapBrushes,
    terrain: Option<MapTerrain>,
    /// The static props' brushes (`map::MapPropBrushStart`).
    props: Vec<MapBrush>,
    scale: f32,
}

impl StaticCollision {
    fn view(&self) -> WorldCollision<'_> {
        WorldCollision {
            brushes: Some(&self.brushes),
            terrain: self.terrain.as_ref(),
            scale: self.scale,
        }
    }
}

impl Collision for StaticCollision {
    fn sweep(&self, mins: Vec3, maxs: Vec3, from: Vec3, to: Vec3) -> f32 {
        self.view().sweep(mins, maxs, from, to)
    }
    fn solid(&self, mins: Vec3, maxs: Vec3, at: Vec3) -> bool {
        self.view().solid(mins, maxs, at)
    }
    fn inside_prop(&self, at: Vec3) -> bool {
        let p = entity_to_engine(at, self.scale);
        let skin = SOLID_SKIN * self.scale;
        self.props.iter().any(|b| b.overlaps_box(p, Vec3::ZERO, skin))
    }
}

/// The map's static world for the logic, once the map has one.
pub(super) fn static_collision(world: &World, scale: f32) -> Option<std::sync::Arc<dyn Collision + Send + Sync>> {
    let mut brushes = world.get_resource::<MapBrushes>()?.clone();
    let props = world
        .get_resource::<crate::map::MapPropBrushStart>()
        .and_then(|s| brushes.0.get(s.0..))
        .map(<[MapBrush]>::to_vec)
        .unwrap_or_default();
    // Fire's traces pass player clips and grates.
    if let Some(skip) = world.get_resource::<crate::map::MapTraceSkip>() {
        let skip: std::collections::HashSet<usize> = skip.0.iter().copied().collect();
        let mut i = 0;
        brushes.0.retain(|_| {
            i += 1;
            !skip.contains(&(i - 1))
        });
    }
    Some(std::sync::Arc::new(StaticCollision {
        brushes,
        terrain: world.get_resource::<MapTerrain>().cloned(),
        props,
        scale,
    }))
}

/// An entity-space brush in engine space.
pub fn brush_to_engine(b: &MapBrush, scale: f32) -> MapBrush {
    let planes = b
        .planes
        .iter()
        .map(|(n, d)| (Vec3::new(n.x, n.z, -n.y), d * scale))
        .collect();
    let (a, c) = (entity_to_engine(b.min, scale), entity_to_engine(b.max, scale));
    MapBrush {
        planes,
        min: a.min(c),
        max: a.max(c),
        ladder: b.ladder,
        surface: b.surface.clone(),
    }
}

pub(super) type CharacterQuery<'a> = (
    Entity,
    &'a Transform,
    &'a Velocity,
    &'a MovementState,
    &'a Intent,
    Option<&'a Health>,
    Option<&'a Team>,
    Option<&'a BaseVelocity>,
    Option<&'a EntityGravity>,
    Option<&'a LocalPlayer>,
    Option<&'a ColliderAabb>,
    Option<&'a crate::core::MapControls>,
    Option<&'a crate::core::Bot>,
);

/// The characters as logic players: the local player first (`!player`
/// is slot 1), then by entity.
pub(super) fn snapshot(world: &mut World, logic: &Logic) -> Vec<Player> {
    let scale = logic.scale;
    let ground_of = |g: Entity| {
        logic
            .nodes
            .iter()
            .chain(&logic.props)
            .find(|(_, n)| *n == g)
            .map(|(id, _)| *id)
    };
    let mut list: Vec<(bool, Player)> = cached_query::<CharacterQuery, (), _>(world, |q, world| {
        q.iter(world)
            .map(|row| (row.9.is_some(), as_player(row, scale, ground_of)))
            .collect()
    });
    list.sort_by_key(|(local, p)| (!*local, p.entity));
    list.into_iter().map(|(_, p)| p).collect()
}

/// A character as a logic player; `ground_of` names the logic entity of
/// what it stands on.
pub(super) fn as_player(
    (e, t, v, state, intent, health, team, base, gravity, _, aabb, controls, bot): bevy::ecs::query::QueryItem<
        '_,
        '_,
        CharacterQuery<'static>,
    >,
    scale: f32,
    ground_of: impl Fn(Entity) -> Option<EntId>,
) -> Player {
    let origin = engine_to_entity(t.translation, scale);
    let (lo, hi) = if state.hull_min != state.hull_max {
        (state.hull_min, state.hull_max)
    } else if let Some(a) = aabb {
        (a.min - t.translation, a.max - t.translation)
    } else {
        (Vec3::splat(-0.4), Vec3::splat(0.4))
    };
    let (a, b) = (engine_to_entity(lo, scale), engine_to_entity(hi, scale));
    let mut p = Player::new(e, origin);
    p.mins = a.min(b);
    p.maxs = a.max(b);
    p.eye = engine_to_entity(state.eye_offset, scale);
    p.velocity = engine_to_entity(v.0, scale);
    if let Some(b) = base {
        p.base_velocity = engine_to_entity(b.velocity, scale);
        p.base_touched = b.touched;
    }
    p.on_ground = state.on_ground;
    p.ground = state.ground.and_then(ground_of);
    p.view = Vec3::new(-intent.pitch.to_degrees(), intent.yaw.to_degrees() + 90.0, 0.0);
    p.alive = health.is_none_or(|h| h.current > 0.0);
    // Our teams 1 and 2 are Source's 2 (T) and 3 (CT).
    p.team = team.map_or(0, |t| if t.0 == 0 { 0 } else { t.0 + 1 });
    p.gravity = gravity.map_or(1.0, |g| g.0);
    p.use_key = intent.use_key;
    p.buttons = controls.map_or_else(|| crate::core::buttons::of(intent), |c| c.buttons);
    p.bot = bot.is_some();
    p
}

