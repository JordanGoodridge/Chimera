//! Slot registries. A game plugin registers its implementations here; the
//! loadout picks one per slot by ID. Nothing outside a game plugin names that
//! plugin's types.
//!
//! `MovementRegistry`, `Loadout`, `set_movement` (swap an entity's movement).
//!
//! Per-character identity: a character with `FollowsWeapon` takes its game
//! slots from the game of the weapon it holds (a minigame's `identity =
//! "weapon"`): its movement (the weapon's game's own,
//! `MovementRegistry::of_game`; `chimera_sim::identity` switches it in the
//! predicted tick, on the server and on the client predicting it alike),
//! and its `GameIdentity` (the server's, replicated: whose body and look
//! it wears, whose HUD its player sees).

use bevy::prelude::*;

/// One Movement implementation. Each implementation is driven by a marker
/// component: its systems only act on entities that have it, so swapping
/// movement is "remove one marker, insert another".
pub struct MovementImpl {
    /// Namespaced ID, e.g. `combat_arms:movement`.
    pub id: &'static str,
    pub insert: fn(&mut EntityWorldMut),
    pub remove: fn(&mut EntityWorldMut),
}

#[derive(Resource, Default)]
pub struct MovementRegistry(pub Vec<MovementImpl>);

impl MovementRegistry {
    /// The implementation registered as `id` (Chimera's own also by their
    /// old namespace: `mashup:noclip` is `chimera:noclip`).
    pub fn get(&self, id: &str) -> Option<&MovementImpl> {
        let renamed = id.strip_prefix("mashup:").map(|rest| format!("chimera:{rest}"));
        let id = renamed.as_deref().unwrap_or(id);
        self.0.iter().find(|m| m.id == id)
    }

    /// Game `game`'s own movement: the first registered whose id is
    /// `game:...` (`cs_source:movement` for `cs_source`).
    pub fn of_game(&self, game: &str) -> Option<&MovementImpl> {
        self.0
            .iter()
            .find(|m| m.id.split_once(':').is_some_and(|(g, _)| g.eq_ignore_ascii_case(game)))
    }

    /// The implementation registered after `id`, wrapping around.
    pub fn next_after(&self, id: &str) -> Option<&MovementImpl> {
        let i = self.0.iter().position(|m| m.id == id)?;
        self.0.get((i + 1) % self.0.len())
    }
}

/// Which Movement implementation an entity currently uses.
#[derive(Component, Debug, Clone, Copy)]
pub struct MovementSlot(pub &'static str);

/// A character whose game slots follow the game of the weapon it holds
/// (a minigame's `identity = "weapon"`): its movement and its
/// `GameIdentity`. Predicted with the player (its client has it too), so
/// both sides switch its movement on the same tick.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FollowsWeapon;

/// The game a character is now (a game id, `cs_source`): whose body and
/// look it wears and whose HUD its player sees, where that follows the
/// character (`FollowsWeapon`). Set by the server, replicated to every
/// client. Without it: the app's (the gameplay's) everywhere.
#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GameIdentity(pub String);

impl GameIdentity {
    /// Whether `e`'s identity (None: none) lets game `game`'s per-character
    /// parts act on it: no identity, or `game`'s.
    pub fn allows(identity: Option<&GameIdentity>, game: &str) -> bool {
        identity.is_none_or(|i| i.0.eq_ignore_ascii_case(game))
    }
}

/// The match loadout: one implementation ID per slot. Owned by the server in
/// multiplayer (README, "Multiplayer").
#[derive(Resource, Debug, Clone)]
pub struct Loadout {
    pub movement: &'static str,
}

pub trait RegisterSlots {
    /// Register a Movement implementation driven by marker component `M`.
    fn register_movement<M: Component + Default>(&mut self, id: &'static str) -> &mut Self;
}

impl RegisterSlots for App {
    fn register_movement<M: Component + Default>(&mut self, id: &'static str) -> &mut Self {
        fn insert<M: Component + Default>(e: &mut EntityWorldMut) {
            e.insert(M::default());
        }
        fn remove<M: Component>(e: &mut EntityWorldMut) {
            e.remove::<M>();
        }
        self.init_resource::<MovementRegistry>()
            .world_mut()
            .resource_mut::<MovementRegistry>()
            .0
            .push(MovementImpl {
                id,
                insert: insert::<M>,
                remove: remove::<M>,
            });
        self
    }
}

/// Switch `entity` to the Movement implementation `id`. Velocity and
/// movement state carry over; the new implementation takes it from there.
pub fn set_movement(entity: Entity, id: &'static str) -> impl Command {
    move |world: &mut World| {
        let registry = world.resource::<MovementRegistry>();
        let Some(new) = registry.get(id) else {
            warn!("unknown movement implementation {id}");
            return;
        };
        let insert = new.insert;
        let removes: Vec<_> = registry.0.iter().map(|m| m.remove).collect();
        let Ok(mut e) = world.get_entity_mut(entity) else {
            return;
        };
        for remove in removes {
            remove(&mut e);
        }
        insert(&mut e);
        e.insert(MovementSlot(id));
        info!("{entity}: movement -> {id}");
    }
}
