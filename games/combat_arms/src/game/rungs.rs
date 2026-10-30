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
//!   stock six), with that body's default face; it plays with the
//!   baseline vest and its own body type only (`gear::vest_of`: no
//!   drawn armour or speed); a body no stock group
//!   has loads when first worn (`characters::bodies_on_demand`) and poses
//!   on its skeleton's stock rig meanwhile (`models::alias_rig`);
//! - `melee`: a Combat Arms melee weapon (every weapon of ours with the
//!   knife's data, `inventory::KNIFE_HOOK`, in a set the definition allows: knives, the shovel, the
//!   machete, ...) in place of the kit's knife on the rung (Chimera's
//!   `runtime::KitMelee`). The knife finale's rung draws nothing: it
//!   keeps its knife.
//!
//! - `infected`: one rung becomes the infected's (game_modes.md M8's
//!   infected, tied to their hands; Chimera's `ladder::RungCharacter`):
//!   of the Combat Arms rungs in the top third (never the last), one
//!   drawn alike (`infected_turn`), when the data has an infected's
//!   weapon (a knife-class weapon with the `combat_arms:infected` table,
//!   `INFECTED`: the install's Quarantine hands, `data::infected_table`;
//!   the stock ones, records 191 and 190 of missions.md F4.1, first). A
//!   rung whose own weapon is such a hand is the infected's too. On it
//!   everyone holds the hands alone (no gun, no kit) and wears an
//!   infected body (`CharacterData::infected_bodies`, its override's
//!   face: `infected_look`); the next rung, up or down, makes them
//!   themselves again. They keep the gun game's baseline (the rung
//!   vest's armour and health, `gear::vest_of`; no `movement::CaInfected`,
//!   so no knock-back from bullets: S14). Becoming one is the
//!   infection's moment (`INFECTED_HOOK`, `infection_moments`, on every
//!   machine that shows effects): the infection sound (data/effects.toml
//!   `infection`, by the body's sex) and the change effect
//!   (`INFECTION_FX`, M8's `Quarantine_Exp_01`) at the player. A melee
//!   rung's draw never gives a hand.
//!
//! Each is on by default and turned off in the definition's `[rules]`
//! (`rung_extras = { attachments = false, character = false, melee =
//! false, infected = false }`, or `rung_extras = false` for all); off,
//! that part is as without this module. The draws are the server's; what they put on a
//! player replicates as the look, the fitted parts and the inventory
//! always do.
//!
//! Known shortcuts (docs/tech-debt.md): the attachment data states no
//! requirements or conflicts between parts (none to respect), and a
//! slot's "none" is the weapon's stock part (fitting can't leave a slot
//! with a default empty); the rung's character keeps camouflage 0 and the
//! body's default face and head piece; a player keeps the last rung look
//! (and an infected its marker) if the ladder stops for a minigame of
//! other rules; the infected's override render styles (the virus blend,
//! the glow), first-person hands and glowing eye trails aren't drawn,
//! nor the infected's screen overlay (unspecified:
//! docs/plans/active/combat-arms.md); the infection sound's files and
//! radius are ours (the spec names none).

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use chimera_sim::minigame::{
    auto::Rng,
    ladder::{RungCharacter, RungCharacterHook, RungDraw, RungExtras, RungExtrasAppExt, RungExtrasHook},
};
use serde::{Deserialize, Serialize};

use super::{
    GAME,
    attachments::{ATTACHMENTS, AttachParams, CaModTable, CaRungParts, SLOTS},
    characters::{self, CaRungLook, models, records::Selection},
    data::CaData,
    effects::{CaFxRequest, FxKind, data::CaFxTables, sounds::CaSound},
    inventory::KNIFE_HOOK,
};
use crate::{net::NetAppExt, weapon::data::WeaponCatalog};

/// The option keys (`rung_extras = { <key> = false }`).
pub const ATTACHMENTS_KEY: &str = "attachments";
pub const CHARACTER_KEY: &str = "character";
pub const MELEE_KEY: &str = "melee";
pub const INFECTED_KEY: &str = "infected";

/// The weapon table marking an infected's weapon (`data::infected_table`).
pub const INFECTED: &str = "combat_arms:infected";
/// The infected as a rung's character (`ladder::RungCharacter::id`).
pub const INFECTED_CHARACTER: &str = "combat_arms:infected";
/// The infected's weapons of the stock Quarantine mission (missions.md
/// F4.1: infected 191, hosts 190), preferred when the install has them.
pub const STOCK_HANDS: [i32; 2] = [191, 190];
/// The change effect group (game_modes.md M8, the mission's
/// `PlayerToInfected` stock value).
pub const INFECTION_FX: &str = "Quarantine_Exp_01";

/// The extras' values: the rung's gun, its parts (comma-separated), the
/// body record, the infected's rung ("1").
pub const WEAPON: &str = "weapon";
pub const PARTS: &str = "parts";
pub const MODEL: &str = "model";
pub const INFECTED_VALUE: &str = "infected";

/// The hook.
pub const HOOK: RungExtrasHook = RungExtrasHook {
    game: GAME,
    options: &[ATTACHMENTS_KEY, CHARACTER_KEY, MELEE_KEY, INFECTED_KEY],
    draw,
    apply,
};

/// The infected's moments (Chimera's `ladder::RungCharacterHook`).
pub const INFECTED_HOOK: RungCharacterHook = RungCharacterHook {
    game: GAME,
    enter: |world, e, _| {
        world.entity_mut(e).insert(CaRungInfected);
    },
    leave: |world, e, _| {
        world.entity_mut(e).remove::<CaRungInfected>();
    },
};

/// A player who is a ladder rung's infected (the server's, replicated):
/// what each machine plays the infection's moment by. Not
/// `movement::CaInfected`: no knock-back, no infected rules.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CaRungInfected;

/// Combat Arms' rung extras on the ladder.
pub struct CaRungsPlugin;

impl Plugin for CaRungsPlugin {
    fn build(&self, app: &mut App) {
        app.add_rung_extras(HOOK)
            .add_rung_character(INFECTED_HOOK)
            .net_part("combat_arms_rungs", |app| {
                app.replicate::<CaRungInfected>();
            });
    }
}

/// Whether weapon `id` is an infected's (its `INFECTED` table).
pub fn is_infected_weapon(catalog: &WeaponCatalog, id: &str) -> bool {
    catalog.get(id).is_some_and(|w| w.has_hook(INFECTED))
}

/// The infected's weapons a rung may give: every infected's weapon of
/// ours the definition allows, the stock Quarantine hands only when the
/// data has them (`STOCK_HANDS`).
pub fn infected_hands(world: &World, d: &RungDraw) -> Vec<&'static str> {
    let Some(catalog) = world.get_resource::<WeaponCatalog>() else { return Vec::new() };
    let prefix = format!("{GAME}:");
    let mut pool: Vec<&'static str> = Vec::new();
    for set in &catalog.sets {
        for w in &set.weapons {
            let Some(data) = catalog.get(&w.id) else { continue };
            let id: &'static str = &data.id;
            if id.starts_with(&prefix) && data.has_hook(INFECTED) && data.has_hook(KNIFE_HOOK) && d.def.allows_weapon(id) && !pool.contains(&id) {
                pool.push(id);
            }
        }
    }
    let stock: Vec<&'static str> = world
        .get_resource::<CaData>()
        .map(|data| STOCK_HANDS.iter().filter_map(|i| data.weapon_for_index(*i)).filter(|id| pool.contains(id)).collect())
        .unwrap_or_default();
    if stock.is_empty() { pool } else { stock }
}

/// Whether rung `d` is the ladder's infected one: of the Combat Arms gun
/// rungs in the top third, never the last, one chosen alike (each in turn
/// with one chance in those left, so exactly one when none before it
/// was), unless an earlier rung already is.
fn infected_turn(world: &World, d: &RungDraw, rng: &mut Rng) -> bool {
    if d.drawn.iter().flatten().any(|x| x.character.is_some()) {
        return false;
    }
    let n = d.rungs.len();
    let catalog = world.get_resource::<WeaponCatalog>();
    let prefix = format!("{GAME}:");
    let band: Vec<usize> = (n * 2 / 3..n.saturating_sub(1))
        .filter(|&i| d.rungs[i].starts_with(&prefix) && !catalog.is_some_and(|c| c.get(d.rungs[i]).is_some_and(|w| w.has_hook(KNIFE_HOOK))))
        .collect();
    let Some(j) = band.iter().position(|&i| i == d.rung) else { return false };
    rng.below(band.len() - j) == 0
}

/// A Combat Arms rung's draw.
fn draw(world: &World, d: &RungDraw, rng: &mut Rng) -> Option<RungExtras> {
    let mut x = RungExtras::default();
    x.values.push((WEAPON.into(), d.weapon.into()));
    // The infected's rung: its own weapon a hand, or the one drawn.
    let own = world.get_resource::<WeaponCatalog>().is_some_and(|c| is_infected_weapon(c, d.weapon));
    let hands = if own {
        Some(d.weapon)
    } else if d.options.on(INFECTED_KEY) {
        let pool = infected_hands(world, d);
        (!pool.is_empty() && infected_turn(world, d, rng)).then(|| pool[rng.below(pool.len())])
    } else {
        None
    };
    if let Some(hands) = hands {
        x.values.push((INFECTED_VALUE.into(), "1".into()));
        if let Some(m) = draw_infected_body(world, rng) {
            x.values.push((MODEL.into(), m.to_string()));
        }
        x.character = Some(RungCharacter {
            id: INFECTED_CHARACTER.into(),
            weapon: hands.into(),
        });
        return Some(x);
    }
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

/// A body for the infected's rung (`CharacterData::infected_bodies`).
pub fn draw_infected_body(world: &World, rng: &mut Rng) -> Option<i32> {
    let pool = world.get_resource::<CaData>()?.characters().infected_bodies();
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
            let knife = data.has_hook(KNIFE_HOOK) && !data.has_hook(INFECTED);
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
    let infected = extras.is_some_and(|x| x.value(INFECTED_VALUE).is_some());
    match model {
        Some(id) => {
            let look = match (infected, world.get_resource::<CaData>()) {
                (true, Some(data)) => data.characters().infected_look(id),
                _ => look_of(world, id),
            };
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

/// The infection's moment on this machine (each frame where effects are
/// shown): a player who just became a rung's infected (`CaRungInfected`
/// added to a character already here: not one a client first sees) gets
/// the infection sound (data/effects.toml `infection`, the female list
/// for a female body) and the change effect (`INFECTION_FX`) where it
/// stands.
#[allow(clippy::type_complexity)]
pub fn infection_moments(
    infected: Query<(&GlobalTransform, Option<Ref<Selection>>), Added<CaRungInfected>>,
    tables: Res<CaFxTables>,
    data: Option<Res<CaData>>,
    mut sounds: MessageWriter<CaSound>,
    mut fx: MessageWriter<CaFxRequest>,
) {
    for (at, look) in &infected {
        if look.as_ref().is_some_and(|l| l.is_added()) {
            continue;
        }
        let at = at.translation();
        let female = look
            .zip(data.as_deref())
            .and_then(|(l, d)| d.characters().model(l.model).map(|m| m.figure == 1))
            .unwrap_or(false);
        let s = &tables.sounds;
        let files = if female && !s.infection_female.is_empty() { &s.infection_female } else { &s.infection };
        if let Some(file) = files.first() {
            sounds.write(CaSound::at(file.clone(), at, s.infection_radius));
        }
        fx.write(CaFxRequest {
            group: INFECTION_FX.into(),
            at,
            normal: Vec3::Y,
            kind: FxKind::Ammo,
        });
    }
}
