//! Weapon parameters the server decides (docs/plans/active/multiplayer.md,
//! "Server-pushed weapon parameters"): values a game derives for a weapon
//! when it is built from what this machine has (Combat Arms' fire cycle,
//! reload and draw from the weapon's view model in the install:
//! `combat_arms::game::weapons::timing`), which a client without that
//! data (no key, a missing file) or with other data (another install
//! version) would derive differently, and then mispredict its own fire,
//! reload or draw.
//!
//! - **Codecs** (`ParamsAppExt::weapon_param`): a game registers one by
//!   name: `encode` reads the derived values off a weapon the server built
//!   (None: nothing to send), `apply` writes them onto a client's weapon.
//!   Kept sorted by name (`ParamCodecs`).
//! - **The network** (chimera_net's `net::weapon_params`) sends each
//!   weapon id's parameters to a client once per connection, when that
//!   client's player first carries a weapon of that id, and gives them to
//!   `receive` on the client.
//! - **On the client** they are kept by weapon id (`PushedParams`) and
//!   every weapon of that id built after (`spawn_weapon`: a rebuilt
//!   inventory, a remote player's held gun) takes them after its build,
//!   over whatever its own hooks derived; weapons already built take them
//!   when they arrive. The server's values always win.

use std::collections::BTreeMap;

use bevy::prelude::*;

use super::Weapon;

/// One game's parameters (`ParamsAppExt::weapon_param`).
#[derive(Clone, Copy)]
pub struct ParamCodec {
    pub name: &'static str,
    /// The server: the parameters of a weapon it built (None: none).
    pub encode: fn(&World, Entity) -> Option<Vec<u8>>,
    /// The client: write them onto one of its weapons.
    pub apply: fn(&mut World, Entity, &[u8]),
}

/// The registered codecs, sorted by name.
#[derive(Resource, Default, Clone)]
pub struct ParamCodecs(Vec<ParamCodec>);

impl ParamCodecs {
    pub fn names(&self) -> Vec<&'static str> {
        self.0.iter().map(|c| c.name).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Weapon `weapon`'s parameters, by codec name (only the codecs that
    /// have some).
    pub fn encode(&self, world: &World, weapon: Entity) -> Vec<(String, Vec<u8>)> {
        self.0
            .iter()
            .filter_map(|c| (c.encode)(world, weapon).map(|b| (c.name.to_string(), b)))
            .collect()
    }

    fn apply_fn(&self, name: &str) -> Option<fn(&mut World, Entity, &[u8])> {
        self.0.iter().find(|c| c.name == name).map(|c| c.apply)
    }
}

/// Registering a game's weapon parameters (`ParamCodecs`).
pub trait ParamsAppExt {
    /// See `ParamCodec`. A name registered again replaces the first.
    fn weapon_param(
        &mut self,
        name: &'static str,
        encode: fn(&World, Entity) -> Option<Vec<u8>>,
        apply: fn(&mut World, Entity, &[u8]),
    ) -> &mut Self;
}

impl ParamsAppExt for App {
    fn weapon_param(
        &mut self,
        name: &'static str,
        encode: fn(&World, Entity) -> Option<Vec<u8>>,
        apply: fn(&mut World, Entity, &[u8]),
    ) -> &mut Self {
        let mut codecs = self.world_mut().get_resource_or_init::<ParamCodecs>();
        let codec = ParamCodec { name, encode, apply };
        match codecs.0.binary_search_by_key(&name, |c| c.name) {
            Ok(i) => codecs.0[i] = codec,
            Err(i) => codecs.0.insert(i, codec),
        }
        self
    }
}

/// The server's parameters a client holds, by weapon id (`receive`).
#[derive(Resource, Default, Clone, Debug)]
pub struct PushedParams(pub BTreeMap<&'static str, Vec<(String, Vec<u8>)>>);

/// Weapon `weapon` takes the server's parameters for its id, if this
/// world holds any (`spawn_weapon` calls it after the build).
pub fn apply_pushed(world: &mut World, weapon: Entity) {
    let Some(id) = world.get::<Weapon>(weapon).map(|w| w.id) else { return };
    let Some(params) = world.get_resource::<PushedParams>().and_then(|p| p.0.get(id)).cloned() else {
        return;
    };
    apply(world, weapon, &params);
}

fn apply(world: &mut World, weapon: Entity, params: &[(String, Vec<u8>)]) {
    let Some(codecs) = world.get_resource::<ParamCodecs>().cloned() else { return };
    for (name, bytes) in params {
        match codecs.apply_fn(name) {
            Some(f) => f(world, weapon, bytes),
            None => warn!("the server sent weapon parameters `{name}` no game here reads"),
        }
    }
}

/// The server's parameters for weapon `id` arrived: kept for every weapon
/// of that id built from now on, and taken by those already built.
pub fn receive(world: &mut World, id: &'static str, params: Vec<(String, Vec<u8>)>) {
    let built: Vec<Entity> = world
        .query::<(Entity, &Weapon)>()
        .iter(world)
        .filter(|(_, w)| w.id == id)
        .map(|(e, _)| e)
        .collect();
    for e in built {
        apply(world, e, &params);
    }
    world.get_resource_or_init::<PushedParams>().0.insert(id, params);
}

/// Forget the server's parameters (the client left it).
pub fn reset(world: &mut World) {
    world.remove_resource::<PushedParams>();
}
