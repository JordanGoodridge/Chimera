//! Combat Arms' rung extras (docs/DESIGN.md "Gun game rules"; Chimera's
//! `minigame::ladder::RungExtrasHook`, registered by `CaRungsPlugin`):
//! when a ladder is made, each Combat Arms rung draws, from the ladder's
//! seeded generator, what every player who reaches it gets alike:
//!
//! - `attachments`: a fitted set from the gun's accepted parts
//!   (`attachments::AttachParams::accepts`): per slot the weapon accepts
//!   (a listed part that can go on a weapon, `ModRecord::weapon_part`,
//!   in its `Kinds` slot), one of those parts or the weapon's stock (its
//!   default part, or nothing), drawn alike; given to the rung's gun as
//!   `attachments::CaRungParts` (fitting, `fit_given`, takes it over the
//!   player's own choice), named in the HUD's rung line;
//! - `character`: a body for everyone on the rung
//!   (`characters::CaRungLook`, the look `wear_looks` dresses them in
//!   over their own, a bot's and a client's too), drawn from every player
//!   body the install has, specialists and mercenaries included
//!   (`CharacterData::rung_bodies`; without the install's models file the
//!   stock six), with that body's default face; a body no stock group
//!   has loads when first worn (`characters::bodies_on_demand`) and poses
//!   on its skeleton's stock rig meanwhile (`models::alias_rig`);
//! - `melee`: a Combat Arms melee weapon (every weapon of ours with the
//!   knife's data, `inventory::KNIFE_HOOK`, in a set the definition allows: knives, the shovel, the
//!   machete, ...) in place of the kit's knife on the rung (Chimera's
//!   `runtime::KitMelee`). The knife finale's rung draws nothing: it
//!   keeps its knife.
//!
//! Each is on by default and turned off in the definition's `[rules]`
//! (`rung_extras = { attachments = false, character = false, melee =
//! false }`, or `rung_extras = false` for all); off, that part is as
//! without this module. The draws are the server's; what they put on a
//! player replicates as the look, the fitted parts and the inventory
//! always do.
//!
//! Known shortcuts (docs/tech-debt.md): the attachment data states no
//! requirements or conflicts between parts (none to respect), and a
//! slot's "none" is the weapon's stock part (fitting can't leave a slot
//! with a default empty); the rung's character keeps camouflage 0 and the
//! body's default face and head piece; a player keeps the last rung look
//! if the ladder stops for a minigame of other rules; a rung's body is
//! also its vest (gear_perks.md: armour, movement penalty), which the
//! gear derives a tick after the swap on each side, so a network client
//! corrects its movement once at a swap to another body (the party's
//! tests of other things play without the extras:
//! apps/party_kit/tests/it/gun_game.rs `plain_rungs`).

use bevy::prelude::*;
use chimera_sim::minigame::{
    auto::Rng,
    ladder::{RungDraw, RungExtras, RungExtrasAppExt, RungExtrasHook},
};

use super::{
    GAME,
    attachments::{ATTACHMENTS, AttachParams, CaModTable, CaRungParts, SLOTS},
    characters::{self, CaRungLook, models, records::Selection},
    data::CaData,
    inventory::KNIFE_HOOK,
};
use crate::weapon::data::WeaponCatalog;

/// The option keys (`rung_extras = { <key> = false }`).
pub const ATTACHMENTS_KEY: &str = "attachments";
pub const CHARACTER_KEY: &str = "character";
pub const MELEE_KEY: &str = "melee";

/// The extras' values: the rung's gun, its parts (comma-separated), the
/// body record.
pub const WEAPON: &str = "weapon";
pub const PARTS: &str = "parts";
pub const MODEL: &str = "model";

/// The hook.
pub const HOOK: RungExtrasHook = RungExtrasHook {
    game: GAME,
    options: &[ATTACHMENTS_KEY, CHARACTER_KEY, MELEE_KEY],
    draw,
    apply,
};

/// Combat Arms' rung extras on the ladder.
pub struct CaRungsPlugin;

impl Plugin for CaRungsPlugin {
    fn build(&self, app: &mut App) {
        app.add_rung_extras(HOOK);
    }
}

/// A Combat Arms rung's draw.
fn draw(world: &World, d: &RungDraw, rng: &mut Rng) -> Option<RungExtras> {
    let mut x = RungExtras::default();
    x.values.push((WEAPON.into(), d.weapon.into()));
    if d.options.on(ATTACHMENTS_KEY) {
        let parts = draw_parts(world, d.weapon, rng);
        if !parts.is_empty() {
            x.label = Some(parts.join(", "));
            x.values.push((PARTS.into(), parts.join(",")));
        }
    }
    if d.options.on(CHARACTER_KEY)
        && let Some(m) = draw_body(world, rng)
    {
        x.values.push((MODEL.into(), m.to_string()));
    }
    if d.options.on(MELEE_KEY) {
        x.melee = draw_melee(world, d, rng).map(str::to_string);
    }
    Some(x)
}

/// Per slot `weapon` accepts parts for: one of them or its stock part,
/// alike; the parts chosen, in slot order.
pub fn draw_parts(world: &World, weapon: &str, rng: &mut Rng) -> Vec<String> {
    let Some(params) = world
        .get_resource::<WeaponCatalog>()
        .and_then(|c| c.get(weapon))
        .and_then(|w| w.hook::<AttachParams>(ATTACHMENTS))
    else {
        return Vec::new();
    };
    let table = world.get_resource::<CaModTable>().unwrap_or(CaModTable::built_in());
    let mut out = Vec::new();
    for slot in 0..SLOTS {
        let mut candidates: Vec<&str> = Vec::new();
        for a in &params.accepts {
            let Some(r) = table.find(a.name()).and_then(|i| table.get(i)) else { continue };
            if r.slot() != Some(slot) || !r.weapon_part() || candidates.iter().any(|c| c.eq_ignore_ascii_case(&r.name)) {
                continue;
            }
            candidates.push(&r.name);
        }
        if candidates.is_empty() {
            continue;
        }
        // The last choice: the weapon's stock.
        let pick = rng.below(candidates.len() + 1);
        if let Some(name) = candidates.get(pick) {
            out.push(name.to_string());
        }
    }
    out
}

/// A body record from the rungs' pool (`CharacterData::rung_bodies`).
pub fn draw_body(world: &World, rng: &mut Rng) -> Option<i32> {
    let pool = world.get_resource::<CaData>()?.characters().rung_bodies();
    (!pool.is_empty()).then(|| pool[rng.below(pool.len())])
}

/// A melee weapon of ours (a knife-class weapon: `inventory::KNIFE_HOOK`)
/// in a set the definition allows.
fn draw_melee(world: &World, d: &RungDraw, rng: &mut Rng) -> Option<&'static str> {
    let catalog = world.get_resource::<WeaponCatalog>()?;
    let prefix = format!("{GAME}:");
    let mut pool: Vec<&'static str> = Vec::new();
    for set in &catalog.sets {
        for w in &set.weapons {
            let Some(data) = catalog.get(&w.id) else { continue };
            let id: &'static str = &data.id;
            let knife = data.has_hook(KNIFE_HOOK);
            if id.starts_with(&prefix) && knife && d.def.allows_weapon(id) && !pool.contains(&id) {
                pool.push(id);
            }
        }
    }
    (!pool.is_empty()).then(|| pool[rng.below(pool.len())])
}

/// The look a rung's body record `id` gives: that body, its default face.
pub fn look_of(world: &World, id: i32) -> Selection {
    let face = world.get_resource::<CaData>().map_or(0, |d| d.characters().default_face(id));
    Selection {
        model: id,
        face,
        ..Selection::default()
    }
}

/// `e` armed for a rung: a Combat Arms rung's parts and look on it, or
/// (another game's rung, or the option off) taken off: a bot back in its
/// own look.
fn apply(world: &mut World, e: Entity, extras: Option<&RungExtras>) {
    let parts = extras.and_then(|x| Some((x.value(WEAPON)?, x.value(PARTS)?)));
    match parts {
        Some((weapon, parts)) => {
            let parts = CaRungParts {
                weapon: weapon.to_string(),
                parts: parts.split(',').filter(|p| !p.is_empty()).map(str::to_string).collect(),
            };
            world.entity_mut(e).insert(parts);
        }
        None => {
            world.entity_mut(e).remove::<CaRungParts>();
        }
    }
    let model = extras.and_then(|x| x.value(MODEL)?.parse::<i32>().ok());
    match model {
        Some(id) => {
            let look = look_of(world, id);
            if let Some(data) = world.get_resource::<CaData>() {
                models::alias_rig(data.characters(), id);
            }
            // Worn at once (not at `wear_looks`' next tick): the rung's
            // swap restarts the player's prediction (`core::rebase`) with
            // the look's gear in it.
            world.entity_mut(e).insert((CaRungLook(look), look));
        }
        None => {
            if world.entity_mut(e).take::<CaRungLook>().is_some() && world.get::<crate::core::Bot>(e).is_some() {
                let look = characters::bot_look_of(world.get::<Name>(e), e);
                world.entity_mut(e).insert(look);
            }
        }
    }
}
