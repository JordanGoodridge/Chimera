//! A character's game identity following the weapon it holds
//! (`slots::FollowsWeapon`; a minigame's `identity = "weapon"`, D2/L2):
//! holding one game's weapon makes the character that game's.
//!
//! - Movement, in the predicted tick (`core::Predict::Select`, right
//!   after the weapon selection, `follow_weapon`): the held weapon's game
//!   (its id's `game:` prefix) gives the movement (`MovementRegistry::
//!   of_game`), switched with `slots::set_movement`. It reads only the
//!   predicted inventory, so the server and the client predicting the
//!   player switch on the same tick (a replay too) and prediction holds;
//!   a weapon whose game has no movement, or a character in noclip,
//!   keeps the one it has. A switch the server makes outside the
//!   player's commands (a rung's swap) counts as `core::rebase`, a
//!   restart for its client.
//! - `GameIdentity` (the server's, replicated): the held weapon's game,
//!   set in the same system where the world is authoritative.
//! - `hud_follows_identity`: the local player's identity's game made the
//!   gameplay's `hud` slot (for an app that draws both games' HUDs).
//!
//! `held_game`: the game of what a character holds (the weapon it
//! switches to this tick, else the one in hand); `follow` and `unfollow`
//! for the minigame runtime's spawns.

use bevy::prelude::*;

use crate::{
    core::{LocalPlayer, NetRole, Playback, Predict, PredictedAppExt},
    map::interp::NetDrawn,
    movement::noclip,
    slots::{FollowsWeapon, GameIdentity, MovementRegistry, MovementSlot, set_movement},
    weapon::{Inventory, SelectWeapons, Weapon},
};
use chimera_core::{game_registry::Games, gameplay::Gameplay};

/// Identity following the weapon: predicted with the player, its movement
/// switched in the predicted tick, its identity set by the server, the
/// local player's HUD following it.
pub struct IdentityPlugin;

impl Plugin for IdentityPlugin {
    fn build(&self, app: &mut App) {
        app.predicted_net::<FollowsWeapon>()
            .add_systems(
                Predict::Select,
                follow_weapon.in_set(SelectWeapons).after(crate::weapon::select_weapons),
            );
    }
}

/// The game of what `inv` holds: the weapon it switches to this tick, else
/// the one in hand (its id's prefix, `cs_source` of `cs_source:weapon_ak47`).
pub fn held_game<'a>(inv: &Inventory, weapon: impl Fn(Entity) -> Option<&'a Weapon>) -> Option<&'a str> {
    let w = weapon(inv.wanted.or(inv.active)?)?;
    w.id.split_once(':').map(|(g, _)| g)
}

/// What `e` should switch to for holding a weapon of `game`: the movement
/// (None: keep its own) and whether its identity changes.
fn wanted_movement(registry: Option<&MovementRegistry>, slot: Option<&MovementSlot>, game: &str) -> Option<&'static str> {
    let current = slot.map(|s| s.0);
    if current == Some(noclip::ID) {
        return None;
    }
    let id = registry?.of_game(game)?.id;
    (current != Some(id)).then_some(id)
}

/// Each following character's movement (both sides) and identity (the
/// server's) from what it holds.
#[allow(clippy::type_complexity)]
pub fn follow_weapon(
    characters: Query<
        (Entity, &Inventory, Option<&MovementSlot>, Option<&GameIdentity>),
        (With<FollowsWeapon>, Without<NetDrawn>),
    >,
    weapons: Query<&Weapon>,
    registry: Option<Res<MovementRegistry>>,
    role: Option<Res<NetRole>>,
    playback: Option<Res<Playback>>,
    mut commands: Commands,
) {
    let authoritative = crate::core::authoritative(role, playback);
    for (e, inv, slot, identity) in &characters {
        let Some(game) = held_game(inv, |w| weapons.get(w).ok()) else { continue };
        if let Some(id) = wanted_movement(registry.as_deref(), slot, game) {
            commands.queue(set_movement(e, id));
        }
        if authoritative && identity.is_none_or(|i| i.0 != game) {
            commands.entity(e).insert(GameIdentity(game.to_string()));
        }
    }
}

/// `follow_weapon` for one character at once (a respawn's weapons given:
/// the life starts as its weapon's game).
pub fn follow(world: &mut World, e: Entity) {
    if world.get::<FollowsWeapon>(e).is_none() {
        return;
    }
    let Some(inv) = world.get::<Inventory>(e) else { return };
    let Some(game) = held_game(inv, |w| world.get::<Weapon>(w)).map(str::to_string) else { return };
    let id = wanted_movement(world.get_resource::<MovementRegistry>(), world.get::<MovementSlot>(e), &game);
    if let Some(id) = id {
        set_movement(e, id).apply(world);
    }
    if world.get::<GameIdentity>(e).is_none_or(|i| i.0 != game) {
        world.entity_mut(e).insert(GameIdentity(game));
    }
}

/// Stop following: the character keeps its movement, its identity goes
/// (the app's again).
pub fn unfollow(world: &mut World, e: Entity) {
    if let Ok(mut ent) = world.get_entity_mut(e) {
        ent.remove::<(FollowsWeapon, GameIdentity)>();
    }
}

/// The gameplay's HUD is the local player's identity's game, while it
/// follows one (a registered game; else the HUD stays).
pub fn hud_follows_identity(
    player: Query<&GameIdentity, (With<LocalPlayer>, With<FollowsWeapon>)>,
    games: Option<Res<Games>>,
    gameplay: Option<ResMut<Gameplay>>,
) {
    let (Ok(identity), Some(games), Some(mut gameplay)) = (player.single(), games, gameplay) else { return };
    let Some(game) = games.get(&identity.0) else { return };
    if gameplay.hud != game.id {
        gameplay.hud = game.id;
    }
}
