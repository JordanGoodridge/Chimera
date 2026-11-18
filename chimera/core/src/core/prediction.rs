use bevy::prelude::*;
use serde::{Serialize, de::DeserializeOwned};

/// How many times the server changed a character's predicted state
/// outside its commands while it lived (a rules' weapon swap: a gun
/// game's rung): a network client predicting it takes the server's state
/// after such a change as a restart, not a prediction error (as it does a
/// spawn or a death), since no command of its led to it. `rebase` counts
/// one.
#[derive(Component, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rebased(pub u32);

/// Count a change to `e`'s predicted state the server made outside its
/// commands (`Rebased`).
pub fn rebase(world: &mut World, e: Entity) {
    if let Ok(mut ent) = world.get_entity_mut(e) {
        let n = ent.get::<Rebased>().map_or(0, |r| r.0);
        ent.insert(Rebased(n.wrapping_add(1)));
    }
}

/// Whether the commands running now run for the first time (Source's
/// `IsFirstTimePredicted`). A client re-running commands after a server
/// correction sets it false: side effects (sounds, damage, pushes) happen
/// only the first time. True outside a replay.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FirstTimePredicted(pub bool);

impl Default for FirstTimePredicted {
    fn default() -> Self {
        Self(true)
    }
}

/// The parts of a tick prediction runs for a client's own player, as
/// schedules: weapon selection, movement and the weapon frame. A fixed tick
/// runs each from its place in `FixedUpdate` (`weapon::SelectWeapons`,
/// `SimSet::Movement`, `weapon::WeaponFrame`); a replay runs them in order
/// per command (`predict`). Their systems read time from `SimClock`,
/// write side effects only under `FirstTimePredicted`, and act on every
/// entity with the components they need (on a client, only its own player
/// will have them; others are drawn from snapshots).
#[derive(bevy::ecs::schedule::ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Predict {
    Select,
    Movement,
    Weapons,
}

impl Predict {
    pub const ALL: [Predict; 3] = [Predict::Select, Predict::Movement, Predict::Weapons];
}

/// An exclusive system that runs the `stage` schedule, placed in
/// `FixedUpdate` where that stage belongs.
pub fn run_predicted(stage: Predict) -> impl FnMut(&mut World) {
    move |world: &mut World| {
        let _ = world.try_run_schedule(stage);
    }
}

/// Run one command's predicted tick (every `Predict` stage, in order) with
/// the intents, clock and flag already set.
pub fn predict(world: &mut World) {
    for stage in Predict::ALL {
        let _ = world.try_run_schedule(stage);
    }
}

/// The components prediction re-simulates (`PredictedAppExt::predicted`),
/// saved and restored for the entities a client predicts (its player and
/// the weapons it carries): restore the server's state, then replay the
/// commands since.
#[derive(Resource, Default)]
pub struct PredictedComponents {
    all: Vec<PredictedComponent>,
    /// The networked ones by name, kept sorted as they are registered (a
    /// blob's order), so encoding and comparing don't sort.
    net: Vec<(&'static str, NetCodec)>,
}

type SavedComponent = Option<Box<dyn std::any::Any + Send + Sync>>;

struct PredictedComponent {
    name: &'static str,
    save: fn(&World, Entity) -> SavedComponent,
    restore: fn(&mut World, Entity, &SavedComponent),
    /// How the server sends it to the client that predicts it
    /// (`PredictedAppExt::predicted_net`), if it does.
    net: Option<NetCodec>,
}

/// A predicted component's network form: the server's state of a
/// client's own player is sent as these bytes, compared byte for byte
/// with the client's prediction, and decoded over it on a mismatch.
#[derive(Clone, Copy)]
struct NetCodec {
    encode: fn(&World, Entity) -> Option<Vec<u8>>,
    decode: fn(&mut World, Entity, Option<&[u8]>),
}

/// The length `PredictedComponents::encode` writes for a component the
/// entity doesn't have.
const ABSENT: u16 = u16::MAX;

/// Some entities' predicted components as they were
/// (`PredictedComponents::save`).
#[derive(Default)]
pub struct PredictedSnapshot(Vec<(Entity, Vec<SavedComponent>)>);

impl PredictedComponents {
    /// Rebuild the sorted networked list after a registration.
    fn reindex(&mut self) {
        self.net = self.all.iter().filter_map(|c| Some((c.name, c.net?))).collect();
        self.net.sort_by_key(|(name, _)| *name);
    }

    /// The registered components' type names, in order.
    pub fn names(&self) -> Vec<&'static str> {
        self.all.iter().map(|c| c.name).collect()
    }

    pub fn save(&self, world: &World, entities: &[Entity]) -> PredictedSnapshot {
        PredictedSnapshot(
            entities
                .iter()
                .map(|e| (*e, self.all.iter().map(|c| (c.save)(world, *e)).collect()))
                .collect(),
        )
    }

    /// The networked predicted components of `entity` as one blob
    /// (`predicted_net`), in name order: per component a little-endian
    /// `u16` length (`u16::MAX`: absent) and its postcard bytes. Equal
    /// blobs are equal states, bit for bit.
    pub fn encode(&self, world: &World, entity: Entity) -> Vec<u8> {
        let mut out = Vec::new();
        for (_, net) in self.net.iter() {
            match (net.encode)(world, entity) {
                Some(bytes) if bytes.len() < ABSENT as usize => {
                    out.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
                    out.extend_from_slice(&bytes);
                }
                _ => out.extend_from_slice(&ABSENT.to_le_bytes()),
            }
        }
        out
    }

    /// Write a blob from `encode` over `entity`'s components (inserting
    /// or removing them as the blob says). A blob of another layout fails
    /// and changes nothing.
    pub fn decode(&self, world: &mut World, entity: Entity, blob: &[u8]) -> Result<(), String> {
        let parts = self.split(blob)?;
        for ((_, net), part) in self.net.iter().zip(parts) {
            (net.decode)(world, entity, part);
        }
        Ok(())
    }

    /// The names of the networked components two blobs disagree on
    /// (`cl_showerror`).
    pub fn differing(&self, a: &[u8], b: &[u8]) -> Vec<&'static str> {
        let (Ok(pa), Ok(pb)) = (self.split(a), self.split(b)) else {
            return vec!["(layout)"];
        };
        self.net
            .iter()
            .zip(pa.into_iter().zip(pb))
            .filter(|(_, (x, y))| x != y)
            .map(|((name, _), _)| *name)
            .collect()
    }

    /// A blob's parts by component name (None: absent), in its order:
    /// for tools that look inside a prediction error.
    pub fn parts<'a>(&self, blob: &'a [u8]) -> Result<Vec<(&'static str, Option<&'a [u8]>)>, String> {
        Ok(self.net.iter().map(|(n, _)| *n).zip(self.split(blob)?).collect())
    }

    fn split<'a>(&self, mut blob: &'a [u8]) -> Result<Vec<Option<&'a [u8]>>, String> {
        let short = || "predicted state blob too short".to_string();
        let mut parts = Vec::new();
        for _ in 0..self.net.len() {
            let (len, rest) = blob.split_first_chunk::<2>().ok_or_else(short)?;
            blob = rest;
            let len = u16::from_le_bytes(*len);
            if len == ABSENT {
                parts.push(None);
                continue;
            }
            let (part, rest) = blob.split_at_checked(len as usize).ok_or_else(short)?;
            parts.push(Some(part));
            blob = rest;
        }
        if !blob.is_empty() {
            return Err("predicted state blob too long".into());
        }
        Ok(parts)
    }

    /// Put each saved entity's predicted components back as they were
    /// (removing the ones it didn't have). Entities gone since are skipped.
    pub fn restore(&self, world: &mut World, snapshot: &PredictedSnapshot) {
        for (e, saved) in &snapshot.0 {
            if world.get_entity(*e).is_err() {
                continue;
            }
            for (c, value) in self.all.iter().zip(saved) {
                (c.restore)(world, *e, value);
            }
        }
    }
}

pub trait PredictedAppExt {
    /// Register a component prediction saves and restores.
    fn predicted<C: Component<Mutability = bevy::ecs::component::Mutable> + Clone>(&mut self) -> &mut Self;

    /// The same, and the server sends it to the client predicting it,
    /// which compares it with its own (`PredictedComponents::encode`).
    /// Entity fields stay out of its serde form (`#[serde(skip)]`): ids
    /// differ between server and client.
    fn predicted_net<C: Component<Mutability = bevy::ecs::component::Mutable> + Clone + Serialize + DeserializeOwned>(
        &mut self,
    ) -> &mut Self;

    /// A networked part of the predicted state that isn't one component
    /// (e.g. what a character carries: its inventory and the weapon
    /// entities in it), under `name` (a component already registered with
    /// `predicted` by that name keeps its save and restore): `encode` it
    /// for an entity (None: absent), `decode` it over the entity (None:
    /// absent).
    fn predicted_codec(
        &mut self,
        name: &'static str,
        encode: fn(&World, Entity) -> Option<Vec<u8>>,
        decode: fn(&mut World, Entity, Option<&[u8]>),
    ) -> &mut Self;
}

impl PredictedAppExt for App {
    fn predicted<C: Component<Mutability = bevy::ecs::component::Mutable> + Clone>(&mut self) -> &mut Self {
        register_predicted::<C>(self, None);
        self
    }

    fn predicted_net<C: Component<Mutability = bevy::ecs::component::Mutable> + Clone + Serialize + DeserializeOwned>(
        &mut self,
    ) -> &mut Self {
        register_predicted::<C>(
            self,
            Some(NetCodec {
                encode: |w, e| w.get::<C>(e).and_then(|c| postcard::to_allocvec(c).ok()),
                decode: |w, e, bytes| match bytes.and_then(|b| postcard::from_bytes::<C>(b).ok()) {
                    Some(c) => {
                        w.entity_mut(e).insert(c);
                    }
                    None => {
                        w.entity_mut(e).remove::<C>();
                    }
                },
            }),
        );
        self
    }

    fn predicted_codec(
        &mut self,
        name: &'static str,
        encode: fn(&World, Entity) -> Option<Vec<u8>>,
        decode: fn(&mut World, Entity, Option<&[u8]>),
    ) -> &mut Self {
        let net = Some(NetCodec { encode, decode });
        let mut registry = self.world_mut().get_resource_or_init::<PredictedComponents>();
        if let Some(c) = registry.all.iter_mut().find(|c| c.name == name) {
            c.net = net;
        } else {
            registry.all.push(PredictedComponent {
                name,
                net,
                save: |_, _| None,
                restore: |_, _, _| {},
            });
        }
        registry.reindex();
        self
    }
}

fn register_predicted<C: Component<Mutability = bevy::ecs::component::Mutable> + Clone>(
    app: &mut App,
    net: Option<NetCodec>,
) {
    let name = std::any::type_name::<C>();
    let mut registry = app.world_mut().get_resource_or_init::<PredictedComponents>();
    let save: fn(&World, Entity) -> SavedComponent = |w, e| {
        w.get::<C>(e)
            .map(|c| Box::new(c.clone()) as Box<dyn std::any::Any + Send + Sync>)
    };
    let restore: fn(&mut World, Entity, &SavedComponent) = |w, e, v| match v.as_ref().and_then(|v| v.downcast_ref::<C>()) {
        Some(c) => {
            w.entity_mut(e).insert(c.clone());
        }
        None => {
            w.entity_mut(e).remove::<C>();
        }
    };
    if let Some(c) = registry.all.iter_mut().find(|c| c.name == name) {
        c.net = c.net.or(net);
        c.save = save;
        c.restore = restore;
    } else {
        registry.all.push(PredictedComponent {
            name,
            net,
            save,
            restore,
        });
    }
    registry.reindex();
}
