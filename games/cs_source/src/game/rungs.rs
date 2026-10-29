//! CS:S's rung extras (docs/DESIGN.md "Gun game rules"; Chimera's
//! `minigame::ladder::RungExtrasHook`, registered by `CsGunsPlugin`, so
//! wherever CS:S's guns are on a ladder): when a ladder is made, each CS:S
//! rung draws, from the ladder's seeded generator, one player model that
//! everyone on the rung wears alike:
//!
//! - `character`: a player class (`core::PlayerClass` 1-8: the four T and
//!   four CT models CS:S ships, `bsp::CLASS_MODELS`; they are the install's
//!   player models), put on the player at arming (a respawn, a rung swap,
//!   a late join) over its own class and the team's model, and replicated
//!   as the class is (a network client sees it as the player's body). The
//!   class gives the body and its hitboxes through the class's model
//!   (`map::CharacterModels::for_identity`; in a free-for-all the class
//!   counts whatever the team). The rung's HUD line names the model.
//!
//! On by default; `rung_extras = { character = false }` (or `rung_extras =
//! false`) leaves the players' own look. The key is the same one Combat
//! Arms' rungs read for their bodies. A rung of another game's weapon
//! takes it off again.
//!
//! Known shortcuts (docs/tech-debt.md): the pool is the eight stock
//! models, not other player models an install may carry (only these are
//! loaded); a player whose own class was chosen from the class menu gets
//! it back only by choosing again after leaving a CS:S rung; the eight
//! models' hitboxes are the stock layout, which is not compared between
//! them here.

use bevy::prelude::*;
use chimera_sim::minigame::{
    auto::Rng,
    ladder::{RungDraw, RungExtras, RungExtrasAppExt, RungExtrasHook},
};

use super::GAME;
use crate::core::PlayerClass;

/// The option key (`rung_extras = { character = false }`).
pub const CHARACTER_KEY: &str = "character";

/// The extras' value: the class (1-8).
pub const CLASS: &str = "class";

/// The models' names by class, in the HUD's rung line.
pub const NAMES: [&str; 8] = [
    "Phoenix Connexion",
    "Elite Crew",
    "Arctic Avengers",
    "Guerilla Warfare",
    "SEAL Team 6",
    "GSG-9",
    "SAS",
    "GIGN",
];

/// The hook.
pub const HOOK: RungExtrasHook = RungExtrasHook {
    game: GAME,
    options: &[CHARACTER_KEY],
    draw,
    apply,
};

/// Marks a player whose `PlayerClass` is a rung's (so leaving the rung
/// takes it off).
#[derive(Component, Clone, Copy, Debug)]
pub struct RungClass;

/// A CS:S rung's draw.
fn draw(_world: &World, d: &RungDraw, rng: &mut Rng) -> Option<RungExtras> {
    if !d.options.on(CHARACTER_KEY) {
        return None;
    }
    let i = rng.below(NAMES.len());
    Some(RungExtras {
        values: vec![(CLASS.into(), (i + 1).to_string())],
        label: Some(NAMES[i].into()),
        ..RungExtras::default()
    })
}

/// `e` armed for a rung: the rung's class on it, or (another game's rung,
/// or the option off) taken off.
fn apply(world: &mut World, e: Entity, extras: Option<&RungExtras>) {
    let class = extras.and_then(|x| x.value(CLASS)?.parse::<u8>().ok()).filter(|c| (1..=8).contains(c));
    match class {
        Some(c) => {
            world.entity_mut(e).insert((PlayerClass(c), RungClass));
        }
        None => {
            if world.entity_mut(e).take::<RungClass>().is_some() {
                world.entity_mut(e).remove::<PlayerClass>();
            }
        }
    }
}

/// CS:S's rung extras on the ladder.
pub struct CsRungsPlugin;

impl Plugin for CsRungsPlugin {
    fn build(&self, app: &mut App) {
        app.add_rung_extras(HOOK);
    }
}
