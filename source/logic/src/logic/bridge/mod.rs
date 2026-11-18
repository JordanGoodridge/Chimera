//! The logic world in the ECS: built from the loaded map's entities, run
//! each fixed tick in the spec's order around the characters' movement
//! (entity_io.md "Per-tick order"): thinks, movers and +use before
//! movement; trigger touches, untouch, the event queue and removals after.
//!
//! Covers: `carry_player`: a network client's own player pushed by its movers'
//! steps as `LogicSet::Pre` does; `LogicPlugin`: built from `map::MapEntities`,
//! runs `LogicSet::Pre` before movement, `LogicSet::Post` after, and
//! `LogicSet::Damage` after weapons (weapon `Damage` on breakable nodes); moves
//! mover nodes and their `core::MovingSolid` (those riding a physics body
//! again after the physics step, `follow_bodies`), disables the collider of what
//! isn't there, gives broken windows `map::BrushPanes` (on
//! `core::TRACE_ONLY_LAYER`: shots hit them, no game's characters meet them),
//! hides nodes of removed
//! entities, pairs prop nodes (`map::PropEntity`) with logic props (weapon
//! damage with its push and attacker, physics impacts on props, breakables and
//! players after the physics step (crush damage by the spec's tables, from each
//! body's velocity before and after it; a prop meeting a player as two
//! contacts that hurt both, as measured in CS:S; players crushed by the props
//! pressing on them past the stress limit), a broken prop's sound,
//! `core::Explosion` and `map::BreakProp` pieces from where its body is, motion
//! requests through `map::prop_physics`, `PropAwakened` into OnAwakened,
//! players' ground prop for pressure, hidden with `map::LogicHidden` and
//! not solid (collider and own `MovingSolid` brushes) once gone, put back at
//! `map::PropHome` at a restart; skin, body group and sequence as
//! `map::PropLook`/`map::PropSequence`), writes sprites' and dust volumes'
//! `map::EntityPart`, the light styles (`map::LightStyles`) and which
//! trigger_soundscape volumes the local player touches
//! (`map::SoundscapeTouches`); lit fires and flames as `map::fire::MapFires`,
//! props' collider boxes into the logic (fire), the `ent_fire` console command
//! and `chimera_logic_record` (the logic world records fired outputs and
//! deliveries, the latest 256), applies effects (burn damage as `core::Damage`
//! of kind `Burn`, a flame's `General.BurningObject` loop following what burns,
//! ambient sounds as `map::SoundControl`, keyed by entity, following the
//! source's mover or prop node); at a round restart (`core::RoundRestarts`)
//! stops the entities' sounds (a map change too) and re-creates the world from
//! the map's entities, `LogicWorld::round_restart`: all but
//! `world::ROUND_KEEP`, queue dropped, nodes shown again, game_text cleared:
//! gibs and shattered panes as `map` messages; players' AddOutput (targetname,
//! gravity, basevelocity, health, origin, rendermode, renderamt, rendercolor)
//! and the Alpha and Color inputs (`LogicWorld::player_looks`, written as
//! `map::RenderLook` on the character) and SetDamageFilter (as
//! `core::DamageFilter`), game_player_equip/player_weaponstrip (as
//! `core::Equip`).
//!
//! Known shortcuts (docs/tech-debt.md): avian (0.7 and 0.8) leaves "body
//! disabled" on a collider's query-tree proxy after `RigidBodyDisabled` is
//! removed (its `Discard` observer still sees the component), and the broad
//! phase pairs no proxy so marked: a restored prop would meet nothing (with
//! 0.7 a new contact that touched in its first step was also linked into an
//! island twice). `logic::bridge::set_prop_shown` re-enables the body before
//! its colliders and rebuilds the proxies of colliders that stayed solid.
//! Why it's OK for now: Contained to one function, guarded by
//! `map_logic::restored_prop_contacts_join_islands_once`;
//! `tests::avian_still_leaves_body_disabled_on_the_proxy` fails once avian
//! clears the flag itself, and the workaround can go.

use avian3d::prelude::{
    AngularVelocity, Collider, ColliderAabb, ColliderDisabled, CollisionLayers, CollisionStart, ComputedMass,
    LayerMask, LinearVelocity, PhysicsSystems, Position, RigidBody, RigidBodyDisabled, Rotation,
};
use bevy::{ecs::message::MessageCursor, prelude::*};
use chimera_core::query_cache::{cached_query, first};

use super::classes::ServerLine;
use super::hud::{HudEvent, HudMessages, HudShow, ScreenFades};
use super::prop_damage::{Hit, Motion, PropExplosion};
use super::world::{Collision, Effect, EntId, LogicWorld, Player, SOLID_SKIN, SWEEP_EPS, Who};
use crate::console::{Console, ConsoleAppExt, Level};
use crate::core::{
    BaseVelocity, Damage, DamageKind, Damageable, EntityGravity, Explosion, Health, Hitgroup, Intent, LocalPlayer,
    MapBrush, MapBrushes, MapTerrain, MovementState, MovingSolid, RoundRestarts, SimSet, Team, Velocity,
};
use crate::map::breakables::{FallingPane, GibPiece, GlassImpact, GlassShatter, SpawnGibs};
use crate::map::entities::{engine_to_entity, entity_rotation, entity_to_engine, rotation_to_engine};
use crate::map::{LogicHidden, VisClusters};
use crate::map::{
    BreakProp, BrushPanes, EntityPart, LightStyles, MapBrushEntity, MapEntities, PlaySound, PropEntity, PropHome,
    PropIndex, PropLook, PropSequence, SoundControl, SoundKey, SoundLevel, SoundscapeSwitches, SoundscapeTouches,
    StartSound,
};

/// The running logic world of the loaded map.
#[derive(Resource)]
pub struct Logic {
    pub world: LogicWorld,
    /// Meters per entity unit.
    pub scale: f32,
    /// Mover entity -> its ECS node.
    pub nodes: Vec<(EntId, Entity)>,
    /// Prop entity (`classes::Class::Prop`) -> its prop node
    /// (`map::PropEntity`).
    pub props: Vec<(EntId, Entity)>,
    /// Server settings a map changed, with their values before (put back
    /// when another map loads).
    pub restore: Vec<(String, String)>,
    /// The `MapEntities` this world was built from.
    pub(super) source: std::sync::Arc<Vec<crate::map::MapEntity>>,
    /// `core::RoundRestarts` this world has seen.
    pub(super) restarts: u32,
    /// The map's placed weapons (`weapon_*` entities), by index.
    pub(super) weapons: Vec<usize>,
}

/// Where the logic runs in the fixed tick.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum LogicSet {
    /// Thinks, movers, +use (before movement).
    Pre,
    /// Touches, untouch, the event queue (after movement).
    Post,
    /// Damage dealt to breakables this tick (after weapons).
    Damage,
}

// A one-shot sound map logic played here: `map::LogicOneShot` (the
// network forwards it without naming the logic), at its old path.
pub use crate::map::LogicOneShot;

/// Play a one-shot logic sound here and tell clients (`LogicOneShot`).
pub(super) fn logic_sound(world: &mut World, sound: PlaySound) {
    world.write_message(LogicOneShot(sound.clone()));
    world.write_message(sound);
}

pub struct LogicPlugin;

/// The map's hurting trigger volumes in engine space (m), for spawns to
/// keep out of (`map::SpawnHazards`).
pub(super) fn hurt_hazards(world: &mut World) -> Vec<crate::core::MapBrush> {
    let Some(logic) = world.get_resource::<Logic>() else { return Vec::new() };
    let s = logic.scale;
    // Entity space (x, y, z up) to engine (x, z, -y), times the scale.
    let to_engine = |v: Vec3| Vec3::new(v.x, v.z, -v.y) * s;
    super::triggers::hurt_zones(&logic.world)
        .into_iter()
        .map(|b| crate::core::MapBrush {
            planes: b.planes.iter().map(|(n, d)| (to_engine(*n) / s, d * s)).collect(),
            min: to_engine(Vec3::new(b.min.x, b.max.y, b.min.z)),
            max: to_engine(Vec3::new(b.max.x, b.min.y, b.max.z)),
            ladder: false,
            surface: None,
        })
        .collect()
}

impl Plugin for LogicPlugin {
    fn build(&self, app: &mut App) {
        use crate::map::SpawnHazardAppExt;
        app.add_spawn_hazards(hurt_hazards);
        // The logic's part of the network (movers, HUD messages).
        super::net::plugin(app);
        // Its entities for tools (the client's console and debug UI).
        app.insert_resource(crate::map::logic_view::LogicInspect {
            names: |w| {
                let logic = w.get_resource::<Logic>()?;
                let ids = logic.world.ids().into_iter();
                Some(ids.filter_map(|id| logic.world.get(id)).map(|e| (e.targetname.clone(), e.classname.clone())).collect())
            },
            entities: inspect_entities,
            summary: |w| w.get_resource::<Logic>().map(|l| (l.world.ids().len(), l.world.round)),
            fired: inspect_fired,
            messages: |w, n| {
                w.get_resource::<Logic>()
                    .map_or_else(Vec::new, |l| l.world.log.iter().rev().take(n).cloned().collect())
            },
        });
        app.init_resource::<HudMessages>()
            .init_resource::<ScreenFades>()
            .add_message::<HudEvent>()
            .add_message::<super::MapSay>()
            .add_message::<super::MapShake>()
            .add_message::<crate::map::SparkBurst>()
            .add_message::<crate::core::ScoreChange>()
            .add_message::<PlaySound>()
            .add_message::<LogicOneShot>()
            .add_message::<SoundControl>()
            .add_message::<SpawnGibs>()
            .add_message::<GlassShatter>()
            .add_message::<GlassImpact>()
            .add_message::<FallingPane>()
            .configure_sets(
                FixedUpdate,
                (
                    LogicSet::Pre
                        .after(SimSet::Rules)
                        .after(crate::core::apply_map_controls)
                        .before(SimSet::Movement),
                    LogicSet::Post
                        .after(SimSet::Movement)
                        .after(crate::map::entities::AnchorSet)
                        .before(SimSet::Weapons),
                    LogicSet::Damage.after(SimSet::Weapons),
                )
                    // Map logic is the server's.
                    .run_if(crate::core::authoritative),
            )
            .init_resource::<LogicRecord>()
            .add_systems(FixedUpdate, (load, apply_record, pre).chain().in_set(LogicSet::Pre))
            .add_systems(FixedUpdate, post.in_set(LogicSet::Post))
            .add_systems(FixedUpdate, (damage, fire_outputs).in_set(LogicSet::Damage))
            .add_message::<crate::map::entities::FireEntityOutput>()
            // Physics impacts on props and players (after the physics
            // step; the damage lands next tick).
            .add_message::<CollisionStart>()
            .add_message::<BreakProp>()
            .add_message::<Explosion>()
            .add_message::<crate::map::prop_physics::PropAwakened>()
            .init_resource::<PreStep>()
            .init_resource::<Impacts>()
            .add_systems(
                FixedPostUpdate,
                record_velocities.before(PhysicsSystems::StepSimulation),
            )
            .add_systems(FixedPostUpdate, (impacts, stress).chain().after(PhysicsSystems::Writeback))
            .add_systems(
                FixedPostUpdate,
                follow_bodies.after(PhysicsSystems::Writeback).run_if(crate::core::authoritative),
            );
        app.console_command(
            "ent_fire",
            "ent_fire <target> <input> [value]: send a map entity an input (names, wildcards, classnames), \
             the local player as activator.",
            |w, a| {
                let (Some(target), Some(input)) = (a.first(), a.get(1)) else {
                    return Err("ent_fire <target> <input> [value]".into());
                };
                let value = match a.get(2..).filter(|v| !v.is_empty()) {
                    Some(v) => super::Value::Str(v.join(" ")),
                    None => super::Value::Void,
                };
                let local = w
                    .query_filtered::<Entity, With<LocalPlayer>>()
                    .iter(w)
                    .next()
                    .map(super::world::Who::Player);
                let mut logic = w.get_resource_mut::<Logic>().ok_or("no map logic loaded")?;
                logic.world.queue_input(target, input, value.clone(), 0.0, local);
                info!("ent_fire {target} {input} {value:?}");
                Ok(None)
            },
        );
        app.console_command(
            "ent_dump",
            "ent_dump <target>: print map entities' logic state (class, place, a mover's pose and \
             speed, parent link), entity space.",
            |w, a| {
                let Some(target) = a.first() else {
                    return Err("ent_dump <target>".into());
                };
                let logic = w.get_resource::<Logic>().ok_or("no map logic loaded")?;
                let lines: Vec<String> = logic
                    .world
                    .resolve(target, None, None)
                    .into_iter()
                    .take(32)
                    .map(|who| logic.world.describe(who))
                    .collect();
                Ok(Some(if lines.is_empty() {
                    format!("no entity '{target}'")
                } else {
                    lines.join("\n")
                }))
            },
        );
        crate::console::resource_cvar::<LogicRecord, u8>(
            app,
            "chimera_logic_record",
            "1: record the outputs map entities fire and the inputs they get (the latest few hundred; \
             the debug UI's Logic tab lists them).",
            |r| &mut r.0,
        );
    }
}

/// A fixed seed for the map logic's random choices (`LogicWorld::seed`);
/// None (the game): a new one at every map load. The headless harness
/// fixes it so tests repeat.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LogicSeed(pub Option<u64>);

/// A seed from the clock.
pub(super) fn time_seed() -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    (now.as_nanos() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
}

/// Build (or drop) the logic world when the map's entities change, and
/// re-create it at a round restart (`core::RoundRestarts`).
pub(super) fn load(world: &mut World) {
    let current = world.get_resource::<MapEntities>().cloned();
    let built = world.get_resource::<Logic>().map(|l| l.source.clone());
    let restarts = world.get_resource::<RoundRestarts>().map_or(0, |r| r.0);
    match (current, built) {
        (None, Some(_)) => {
            restore_settings(world);
            world.write_message(SoundControl::StopAll);
            world.remove_resource::<Logic>();
            world.remove_resource::<LightStyles>();
            world.remove_resource::<crate::map::TonemapInputs>();
            world.remove_resource::<crate::map::AreaPortalStates>();
            world.remove_resource::<crate::map::OccluderStates>();
            world.remove_resource::<SoundscapeTouches>();
            world.remove_resource::<SoundscapeSwitches>();
            world.remove_resource::<crate::map::fire::MapFires>();
        }
        (Some(m), b) if b.as_ref().is_none_or(|b| !std::sync::Arc::ptr_eq(b, &m.entities)) => {
            restore_settings(world);
            world.write_message(SoundControl::StopAll);
            let dt = world.resource::<Time<Fixed>>().timestep().as_secs_f32();
            let mut logic = LogicWorld::new(dt);
            // The map's dice (logic_case PickRandom, logic_timer random
            // times): new at every map load, as the game's are, unless
            // fixed (tests).
            logic.seed(world.get_resource::<LogicSeed>().and_then(|s| s.0).unwrap_or_else(time_seed));
            logic.collision = static_collision(world, m.scale);
            let ids = logic.load_map(&m.entities);
            let nodes = attach_nodes(world, &logic, &ids);
            let props = attach_props(world, &logic, &ids, false);
            world.insert_resource(Logic {
                world: logic,
                scale: m.scale,
                nodes,
                props,
                restore: Vec::new(),
                weapons: m
                    .entities
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| crate::map::entities::anchor_class(e.classname()))
                    .map(|(i, _)| i)
                    .collect(),
                source: m.entities.clone(),
                restarts,
            });
        }
        _ => {
            let Some(mut logic) = world.remove_resource::<Logic>() else {
                return;
            };
            if logic.restarts != restarts {
                logic.restarts = restarts;
                // The entities' sounds stop; re-created ones start again.
                world.write_message(SoundControl::StopAll);
                // Templates' copies go; the map's entities start again.
                let copies: Vec<Entity> = world
                    .query_filtered::<Entity, With<crate::map::copies::MapCopy>>()
                    .iter(world)
                    .collect();
                for c in copies {
                    world.entity_mut(c).despawn();
                }
                let source = logic.source.clone();
                let ids = logic.world.round_restart(&source);
                logic.nodes = attach_nodes(world, &logic.world, &ids);
                logic.props = attach_props(world, &logic.world, &ids, true);
                // HUD messages from the last round go too.
                if let Some(mut hud) = world.get_resource_mut::<HudMessages>() {
                    *hud = HudMessages::default();
                }
            }
            world.insert_resource(logic);
        }
    }
}

/// Pair every mover node with its logic entity (`ids` in map order).
/// Nodes whose entity is gone are hidden; the others are shown and solid
/// (the next sync places them), and breakables take damage from weapons.
pub(super) fn attach_nodes(world: &mut World, logic: &LogicWorld, ids: &[EntId]) -> Vec<(EntId, Entity)> {
    let all: Vec<(Entity, usize)> = world
        .query::<(Entity, &MapBrushEntity)>()
        .iter(world)
        .map(|(e, n)| (e, n.0))
        .collect();
    let mut nodes = Vec::new();
    for (node, index) in all {
        let Some(id) = ids.get(index).copied().filter(|id| logic.get(*id).is_some()) else {
            hide_node(world, node);
            continue;
        };
        let breakable = matches!(
            logic.get(id).map(|e| &e.class),
            Some(super::classes::Class::Breakable(_))
        );
        let mut e = world.entity_mut(node);
        e.remove::<(ColliderDisabled, MovingSolid)>();
        set_node_shown(&mut e, true);
        if breakable {
            e.insert(Damageable);
        } else {
            e.remove::<Damageable>();
        }
        nodes.push((id, node));
    }
    nodes
}

/// A node whose logic entity is gone (killed, broken): not drawn, hit or
/// stood on, until a round restart brings the entity back.
pub(super) fn hide_node(world: &mut World, node: Entity) {
    if let Ok(mut e) = world.get_entity_mut(node) {
        // Only when missing: re-inserting ColliderDisabled replaces it,
        // and avian puts a replaced one's collider back in its query tree.
        if !e.contains::<ColliderDisabled>() {
            e.insert(ColliderDisabled);
        }
        set_node_shown(&mut e, false);
        e.insert(MovingSolid::default()).remove::<Damageable>();
    }
}

/// Show or hide a mover node for the logic (`LogicHidden`): shown, it is
/// drawn when visibility culling (`VisClusters`, by its bounds) allows.

pub mod carry;
pub mod effects;
pub mod physics_sync;
pub mod spawn;
#[cfg(test)]
mod tests;

pub use carry::*;
pub use effects::*;
pub use physics_sync::*;
pub use spawn::*;
