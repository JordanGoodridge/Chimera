//! Weapon parameters the server decides, over the network
//! (docs/plans/active/multiplayer.md, net32; `weapon::params`): when a
//! client's player first carries a weapon of some id, the server sends
//! that id's parameters (`WeaponParams`: the weapon's registry index and
//! each game codec's bytes, `weapon::params::ParamCodecs::encode` of the
//! server's weapon), once per id per connection (`ParamsSent`, on the
//! client's server-side entity), in the same send as the own state that
//! first lists it. The client keeps them (`weapon::params::receive`):
//! its weapons of that id take them, now and whenever built again, over
//! what its own data derived. Combat Arms' fire cycle, reload and draw
//! from the view model are the first (`combat_arms:timing`).
//!
//! Only the client's own player's weapons (the ones it predicts); others'
//! held guns are drawn, not predicted. `ParamsHeard` counts the messages
//! a client got, by weapon id (tests, the net graph).

use std::collections::{BTreeMap, BTreeSet};

use bevy::{ecs::message::MessageCursor, prelude::*};
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use super::server::Player;
use crate::{
    core::NetRole,
    weapon::{
        Inventory, Weapon, WeaponRegistry,
        params::{self, ParamCodecs},
    },
};

/// A weapon id's parameters, server to client: its registry index and,
/// per codec name, its bytes.
#[derive(Message, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WeaponParams {
    pub weapon: u16,
    pub params: Vec<(String, Vec<u8>)>,
}

/// The weapon ids a client has been sent parameters for (or found to
/// have none), on its server-side entity.
#[derive(Component, Default, Debug)]
pub struct ParamsSent(pub BTreeSet<&'static str>);

/// The `WeaponParams` messages this client got, by weapon id.
#[derive(Resource, Default, Clone, Debug)]
pub struct ParamsHeard(pub BTreeMap<&'static str, u32>);

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<ParamCodecs>()
        .add_systems(
            PostUpdate,
            send_params
                .before(ServerSystems::Send)
                .run_if(resource_equals(NetRole::Server)),
        )
        .add_systems(
            PreUpdate,
            receive_params
                .after(ClientSystems::Receive)
                .before(super::predict::rebuild_own_states)
                .run_if(resource_equals(NetRole::Client)),
        );
}

/// Each client's player's weapons whose id it hasn't been sent yet.
fn send_params(world: &mut World) {
    if world.resource::<ParamCodecs>().is_empty() {
        return;
    }
    let players: Vec<(Entity, Entity)> = world
        .query::<(Entity, &Player)>()
        .iter(world)
        .map(|(e, p)| (e, p.character))
        .collect();
    let mut out = Vec::new();
    for (client, character) in players {
        let Some(weapons) = world.get::<Inventory>(character).map(|i| i.weapons.clone()) else {
            continue;
        };
        let new: Vec<(Entity, &'static str)> = weapons
            .iter()
            .filter_map(|&w| world.get::<Weapon>(w).map(|x| (w, x.id)))
            .filter(|(_, id)| world.get::<ParamsSent>(client).is_none_or(|s| !s.0.contains(id)))
            .collect();
        if new.is_empty() {
            continue;
        }
        let codecs = world.resource::<ParamCodecs>().clone();
        for (w, id) in new {
            let mut c = world.entity_mut(client);
            if c.get::<ParamsSent>().is_none() {
                c.insert(ParamsSent::default());
            }
            if !c.get_mut::<ParamsSent>().unwrap().0.insert(id) {
                continue;
            }
            let p = codecs.encode(world, w);
            let Some(index) = world.resource::<WeaponRegistry>().index(id) else { continue };
            if !p.is_empty() {
                out.push(ToClients {
                    targets: SendTargets::Single(ClientId::Client(client)),
                    message: WeaponParams {
                        weapon: index as u16,
                        params: p,
                    },
                });
            }
        }
    }
    for m in out {
        world.write_message(m);
    }
}

/// The server's parameters, onto this client's weapons
/// (`weapon::params::receive`).
fn receive_params(world: &mut World, mut cursor: Local<MessageCursor<WeaponParams>>) {
    let got: Vec<WeaponParams> = cursor.read(world.resource::<Messages<WeaponParams>>()).cloned().collect();
    for m in got {
        let id = world.resource::<WeaponRegistry>().0.get(m.weapon as usize).map(|d| d.id);
        let Some(id) = id else {
            warn!("the server sent parameters for weapon {} this build doesn't have", m.weapon);
            continue;
        };
        *world.get_resource_or_init::<ParamsHeard>().0.entry(id).or_default() += 1;
        params::receive(world, id, m.params);
    }
}

/// Forget what the server sent (this client left it).
pub(super) fn reset(world: &mut World) {
    params::reset(world);
    world.remove_resource::<ParamsHeard>();
}
