//! Combat Arms over the network (docs/plans/active/combat-arms.md, "Network
//! part"): what the game adds to Chimera's (`net`), so a listen server or
//! a client joining one plays Combat Arms as single player does.
//!
//! Most of it is Chimera's and needs nothing here: characters, health,
//! teams and deaths replicated; each client predicting its own player
//! (movement, the inventory and its weapons' parts) and corrected from the
//! server's state; others' shots, swings and held weapons sent to the
//! other clients; lag compensation's history; the default join flow
//! (`net::parts::enter_at_spawn`: on the smaller team, in at a free spawn
//! point once the map is loaded).
//!
//! What the game's parts register themselves, so prediction runs them on a
//! client (with or without this plugin):
//!
//! - `movement`: `CaMovement`, the whole movement state, as a networked
//!   predicted component;
//! - `weapons`: the spread meters (`CaSpread`), the kick (`CaRecoil`), the
//!   zoom (`CaZoom`; the owner's `Zoomed` and sway on its `ViewPunch` are
//!   Chimera's predicted parts) and a minigun's spin (`CaSpin`) as
//!   predicted weapon parts (`weapon::NetPart`), beside Chimera's
//!   (timers, burst, clip, fire mode: `WeaponState`, `Magazine`,
//!   `AltModes`); the held weapon's movement penalty in the predicted
//!   tick; the server's rounds lag compensated (`weapon::lagcomp`, the
//!   shooter's view tick); a client deals no damage;
//! - `inventory`: the selection (`CaSelection`: class cursors, last
//!   chosen, Q, the sprint pose; weapons as inventory indices) and the
//!   knife's attack (`Knife`'s `KnifeState`).
//!
//! What this part adds (`CaNetPlugin`, registered as `combat_arms`):
//!
//! - **Armour** (`weapons::CaArmor`): the server's, replicated in tick order
//!   to every client (the HUD's armour points; others' for spectating).
//! - **The pickup prompt** (`CaPickup`): the weapon id of the dropped item
//!   a player's view is on, the server's (`inventory::pickups` runs there),
//!   for its HUD.
//!
//! Known shortcuts: the knife's ray isn't lag compensated (it hits others
//! where the server has them); armour and the pickup prompt go to every
//! client, not only the player's own; no game modes yet, so no rule state
//! is replicated and the dead stay dead (as in single player); listen
//! server only (no Combat Arms dedicated server binary).

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use super::{
    inventory::{CaSelection, Dropped},
    weapons::CaArmor,
};
use crate::{
    core::NetRole,
    net::{NetAppExt, NetHooks, ordered::OrderedAppExt},
    weapon::Weapon,
};

/// Combat Arms' part of the network (`register`), named `combat_arms`.
pub struct CaNetPlugin;

impl Plugin for CaNetPlugin {
    fn build(&self, app: &mut App) {
        app.net_part("combat_arms", register);
    }
}

/// The weapon id of the dropped item a player's view is on (the server's
/// `CaSelection::pickup_target`), for its HUD's pickup prompt.
#[derive(Component, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CaPickup(pub Option<String>);

/// The protocol (in this order on every side) and the systems.
fn register(app: &mut App) {
    app.replicate::<CaArmor>()
        .ordered::<CaArmor>()
        .replicate::<CaPickup>()
        .ordered::<CaPickup>()
        .add_systems(FixedLast, write_pickups.run_if(resource_equals(NetRole::Server)));
    app.world_mut()
        .resource_mut::<NetHooks>()
        .stopped_serving
        .push(stopped_serving);
}

/// Each player's pickup target as the weapon it is.
fn write_pickups(
    players: Query<(Entity, &CaSelection, Option<&CaPickup>)>,
    dropped: Query<&Dropped>,
    weapons: Query<&Weapon>,
    mut commands: Commands,
) {
    for (e, sel, have) in &players {
        let id = sel
            .pickup_target
            .and_then(|i| dropped.get(i).ok())
            .and_then(|d| weapons.get(d.weapon).ok())
            .map(|w| w.id.to_string());
        if have.is_none_or(|h| h.0 != id) {
            commands.entity(e).insert(CaPickup(id));
        }
    }
}

/// Serving stopped: the prompts go (armour stays: single player uses it).
fn stopped_serving(world: &mut World) {
    let with: Vec<Entity> = world.query_filtered::<Entity, With<CaPickup>>().iter(world).collect();
    for e in with {
        world.entity_mut(e).remove::<CaPickup>();
    }
}
