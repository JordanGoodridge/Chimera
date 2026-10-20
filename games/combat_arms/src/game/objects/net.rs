//! The world objects over the network (specs/combat_arms/world_objects.md
//! 17): the server runs every state machine, timer, trigger, counter,
//! command and hit point; clients get the objects' poses and flags. One
//! replicated entity carries every world model and volume that differs
//! from the map's start (`NetCaObjects`: position, rotation, visible,
//! solid, ray-hit, removed), so a late joiner gets open doors, risen
//! water and broken breakables as they are; a client writes them into its
//! own object world (built from the same map) and onto its nodes and
//! volumes. Registered as the network part `combat_arms_objects`.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use super::{CaObjects, apply_effects, movers::Pose};
use crate::{
    core::NetRole,
    map::MapFile,
    net::{NetAppExt, ordered::OrderedAppExt},
};

/// The objects that differ from the map's start, on one replicated
/// entity.
#[derive(Component, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct NetCaObjects {
    /// The map they belong to (`MapFile::name`).
    pub map: String,
    pub objects: Vec<NetObject>,
}

/// One object's state (the world file's frame, u).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct NetObject {
    pub id: u32,
    pub pos: [f32; 3],
    pub rot: [f32; 4],
    /// `VISIBLE` | `SOLID` | `RAYHIT` | `REMOVED`.
    pub flags: u8,
}

pub const VISIBLE: u8 = 1;
pub const SOLID: u8 = 2;
pub const RAYHIT: u8 = 4;
pub const REMOVED: u8 = 8;

pub(super) fn register(app: &mut App) {
    app.net_part("combat_arms_objects", part);
}

fn part(app: &mut App) {
    app.replicate::<NetCaObjects>()
        .ordered::<NetCaObjects>()
        .add_systems(FixedLast, write.run_if(resource_equals(NetRole::Server)))
        .add_systems(
            PreUpdate,
            read.after(ClientSystems::Receive).run_if(resource_equals(NetRole::Client)),
        );
}

pub(super) fn flags_of(o: &super::runtime::Object) -> u8 {
    ((o.visible as u8) * VISIBLE) | ((o.solid as u8) * SOLID) | ((o.rayhit as u8) * RAYHIT) | ((o.removed as u8) * REMOVED)
}

/// The server's objects that differ from the start.
fn write(world: &mut World) {
    let Some(map) = world.get_resource::<MapFile>().map(|m| m.name.clone()) else { return };
    let o = world.resource::<CaObjects>();
    let objects: Vec<NetObject> = o
        .world
        .objects
        .iter()
        .enumerate()
        .filter(|(i, ob)| (ob.node || o.volumes.contains_key(&(*i as u32))) && o.start.get(*i).is_some_and(|s| (ob.pose, flags_of(ob)) != *s))
        .map(|(i, ob)| NetObject {
            id: i as u32,
            pos: ob.pose.pos.to_array(),
            rot: ob.pose.rot.to_array(),
            flags: flags_of(ob),
        })
        .collect();
    let now = NetCaObjects { map, objects };
    let have = world.query::<(Entity, &NetCaObjects)>().iter(world).next().map(|(e, n)| (e, n.clone()));
    match have {
        Some((e, n)) if n != now => {
            world.entity_mut(e).insert(now);
        }
        Some(_) => {}
        None => {
            world.spawn((Name::new("World objects"), Replicated, now));
        }
    }
}

/// A client: the server's objects into its own, and onto the nodes.
fn read(world: &mut World) {
    let Some(map) = world.get_resource::<MapFile>().map(|m| m.name.clone()) else { return };
    let Some(net) = world.query::<&NetCaObjects>().iter(world).next().cloned() else { return };
    if net.map != map {
        return;
    }
    {
    let mut o = world.resource_mut::<CaObjects>();
    if o.from.is_none() {
        return;
    }
    let n = o.world.objects.len();
    let mut want: Vec<Option<(Pose, u8)>> = vec![None; n];
    for x in &net.objects {
        let Some(ob) = o.world.objects.get(x.id as usize) else { continue };
        want[x.id as usize] = Some((
            Pose {
                pos: Vec3::from(x.pos),
                rot: Quat::from_array(x.rot),
                half: ob.pose.half,
            },
            x.flags,
        ));
    }
    for (i, wanted) in want.into_iter().enumerate() {
        let Some(start) = o.start.get(i).copied() else { continue };
        let (pose, flags) = wanted.unwrap_or(start);
        let ob = &mut o.world.objects[i];
        if ob.pose != pose || flags_of(ob) != flags {
            ob.pose = pose;
            ob.visible = flags & VISIBLE != 0;
            ob.solid = flags & SOLID != 0;
            ob.rayhit = flags & RAYHIT != 0;
            ob.removed = flags & REMOVED != 0;
            ob.dirty = true;
        }
    }
    }
    apply_effects(world);
}
