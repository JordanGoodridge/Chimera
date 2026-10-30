//! `chimera:ladder`, the weapon ladder (Gun Game; Combat Arms' Arms Race
//! is its first user in shape, architecture-review.md D2): every player a
//! team of their own, each on a rung of the definition's ladder of weapons
//! (guns, or a grenade), holding that rung's weapon (with the definition's
//! kit). An enemy kill (any weapon's, the grenade's blast too)
//! moves the killer a rung up, its weapon swapped at once; a kill on the
//! last rung wins the match. With `demote_on = "melee_death"` a player
//! killed by an enemy's melee swing goes a rung down (not below the first):
//! melee damage, or any damage from a melee weapon (CS:S's knife deals
//! bullet damage); the knifer still climbs, so a knife kill steals a rung.
//! A death of one's own doing costs a rung too (`demote_on_suicide`, on
//! by default; never below the first): a suicide (one's own grenade or
//! blast, `kill`) or a death with no killer (a fall, the world), so `kill`
//! is no free way out of a bad spawn. It is no kill for anyone: the
//! announcer's streaks count enemy kills only (`announce`).
//! A player who joins a live match starts on the lowest rung of the
//! others (`join_rung = "lowest"`, the default), their average rounded
//! down (`"average"`) or the first (`"first"`), never the last rung; at a
//! match's start everyone is on the first. With `knife_finale` the
//! ladder ends with a knife-only rung: after the last gun the player
//! holds only a knife (the kit's melee weapon of the last gun's game, or
//! the one named), and a knife kill wins (off by default).
//! Combat Arms' own Arms Race (`games/combat_arms`, its rules) is not
//! this module and keeps its spec's rules (no cost for a suicide).
//!
//! **Rung extras** (`RungExtras`): when the ladder is made (each match's
//! start on the host: every map, every new ladder) each rung whose
//! weapon's game registered a hook (`RungExtrasHook`,
//! `RungExtrasAppExt::add_rung_extras`) gets that game's draw for it,
//! from the match's seeded generator (`RungSeed`, the definition's id and
//! the host's count of ladders made): the same for every player on the
//! rung, kept in `Ladder::extras` (replicated with the match,
//! `chimera_net::minigame_net`), so a late joiner, a respawn and a
//! network client get the same. What they hold is the game's (its own
//! key/value pairs, which only its hook reads), but for two things
//! Chimera does itself: a melee weapon in place of the kit's
//! (`RungExtras::melee`, `runtime::KitMelee`) and a label after the
//! rung's weapon in the HUD's rung line. A player is armed for its rung
//! (`arm`: the melee, then every hook's `apply`, with its own extras or
//! none) before a life's weapons (`RulesModule::arming`) and at each rung
//! swap. The knife finale's rung draws none (it keeps its knife), nor
//! does a rung of a game without a hook. `rung_extras = false`
//! turns them all off; a table turns a hook's options off by key
//! (`rung_extras = { attachments = false }`; every key is on by default;
//! a key no hook reads is warned about at the start), or tunes one with
//! a table of its own, which only its hook reads (`RungOptions::table`:
//! `attachments = { sight = 0.4 }`; on).
//! **Rung characters** (`RungExtras::character`, `RungCharacter`): a
//! game's draw may make a rung a special character's, tied to its one
//! weapon (Combat Arms' infected and their hands): on that rung a player
//! holds the character's weapon alone (no gun, no kit: `no_kit`; the HUD
//! names it as the rung's weapon, `Ladder::weapon_at`), and the next rung,
//! up or down, takes it back and gives the kit again. Becoming and
//! leaving the character are the game's moments (`RungCharacterHook`,
//! `RungExtrasAppExt::add_rung_character`): `enter` once when a player is
//! armed for the character's rung and wasn't it (`AsRungCharacter`, the
//! host's), `leave` once when armed for another; a respawn on the rung
//! fires neither. A draw sees every rung's weapon and the rungs drawn
//! before it (`RungDraw::rungs`, `drawn`), so a game can put one such
//! rung where it wants.
//! The ladder gives the weapons, so nobody drops one at death
//! (`weapon::drop::NoDeathDrops`).
//! At the time limit the highest rung wins, then the most kills; a tie is
//! a draw.
//!
//! ```toml
//! [rules]
//! module = "chimera:ladder"
//! ladder = ["cs_source:weapon_glock", "combat_arms:m4a1"]   # the rungs, in order
//! advance_on = "kill"              # the only one so far
//! demote_on = "melee_death"        # or "none" (the default)
//! demote_on_suicide = true         # a suicide, fall or `kill` costs a rung (default)
//! join_rung = "lowest"             # a late joiner's rung: "lowest" (default), "average", "first"
//! knife_finale = false             # true (the kit's knife) or a weapon id: a knife-only
//!                                  # last rung (default: false)
//! refill_ammo = true               # a rung's gun never runs dry, its grenade comes
//!                                  # back once thrown (default: false)
//! rung_extras = true               # the games' per-rung draws (default: on); or by
//!                                  # option: { attachments = false, character = true },
//!                                  # or tuned: { attachments = { sight = 0.4 } }
//! win = { kind = "ladder_end" }    # the only one so far
//! ```
//!
//! Events (`MinigameEvent`): `rung_up`, `rung_down` (knifed) and
//! `suicide_down` (one's own death) with the new rung,
//! `last_rung` (a player reached the last rung), and the lead (the
//! highest rung, then the most kills; nobody leads before a kill):
//! `lead_taken` (alone in it), `lead_lost` (no longer in it),
//! `lead_tied` (newly sharing it, or a sole leader caught up), each with
//! the player's rung. The viewer's callouts (`callout`):
//! "Level up: <gun>", "Knifed! Level down: <gun>", "Suicide! Level down:
//! <gun>", "Last level!", "<name> is on
//! the last level!", "You took the lead", "You lost the lead", "Tied for
//! the lead"; the HUD's notices (`notice`) stay the small text.

use std::collections::BTreeMap;

use bevy::prelude::*;
use chimera_core::query_cache::cached_query;
use serde::{Deserialize, Serialize};

use super::{
    ActiveMinigame, MinigameDef, MinigameEvent, RulesModule,
    auto::Rng,
    hud::{Callout, HudLine},
    runtime,
};
use crate::{
    core::{DamageKind, Died, Score, Team},
    mode::{Death, DeathKind, Hooks, Match, Mode, NO_HOOKS, Teams},
    weapon::{
        Inventory, Magazine, Weapon,
        data::WeaponCatalog,
        give,
        grenade::{KeepSpent, Throwable},
    },
};

/// The module's name.
pub const LADDER: &str = "chimera:ladder";

/// The rules module.
pub const RULES: RulesModule = RulesModule {
    mode: &MODE,
    check,
    start,
    loadout,
    no_kit,
    joined,
    arming,
    status,
    notice,
    callout,
};

/// The mode on the runtime.
pub static MODE: Mode = Mode {
    id: LADDER,
    key: LADDER,
    shown_as: "Weapon ladder",
    description: "every player for themselves up a ladder of weapons: a kill a rung up, the last rung's kill wins",
    respawns: true,
    warmup: false,
    warmup_revives: false,
    forced_setup: Some(0.0),
    teams: Teams::FreeForAll,
    on_death,
    time_out,
    match_end: |w| Some(w.resource::<Match>().winner),
    hooks: Hooks {
        died,
        before_spawns,
        update,
        ..NO_HOOKS
    },
};

/// The `[rules]` keys.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    #[allow(dead_code)]
    module: String,
    ladder: Vec<String>,
    #[serde(default)]
    advance_on: Advance,
    #[serde(default)]
    demote_on: Demote,
    #[serde(default)]
    win: Win,
    #[serde(default)]
    refill_ammo: bool,
    #[serde(default = "yes")]
    demote_on_suicide: bool,
    #[serde(default)]
    join_rung: JoinRung,
    #[serde(default)]
    knife_finale: Finale,
    #[serde(default)]
    rung_extras: ExtrasParam,
}

fn yes() -> bool {
    true
}

/// A late joiner's rung (`join_rung`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinRung {
    /// The lowest rung of the others.
    #[default]
    Lowest,
    /// The others' average, rounded down.
    Average,
    /// The first rung.
    First,
}

/// `rung_extras`: all on or off, or by option key (on or off, or a
/// table: on, tuned).
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum ExtrasParam {
    All(bool),
    Keys(BTreeMap<String, ExtrasKey>),
}

/// One option key's value.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum ExtrasKey {
    On(bool),
    Tuned(toml::Table),
}

impl Default for ExtrasParam {
    fn default() -> Self {
        Self::All(true)
    }
}

/// Which rung extras a definition wants (`rung_extras`): every option
/// key a hook reads is on unless turned off, none when all are off.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RungOptions {
    /// `rung_extras = false`.
    pub off: bool,
    /// Each key turned on or off (a tuned one: on).
    pub keys: BTreeMap<String, bool>,
    /// The tuned keys' tables (`{ attachments = { ... } }`), which only
    /// the hook that reads the key reads.
    pub tables: BTreeMap<String, toml::Table>,
}

impl RungOptions {
    /// Whether option `key` is on.
    pub fn on(&self, key: &str) -> bool {
        !self.off && self.keys.get(key).copied().unwrap_or(true)
    }

    /// Option `key`'s table, when the definition tunes it.
    pub fn table(&self, key: &str) -> Option<&toml::Table> {
        self.tables.get(key)
    }

    fn of(p: &ExtrasParam) -> Self {
        match p {
            ExtrasParam::All(on) => Self {
                off: !on,
                ..Self::default()
            },
            ExtrasParam::Keys(keys) => {
                let mut out = Self::default();
                for (k, v) in keys {
                    match v {
                        ExtrasKey::On(on) => {
                            out.keys.insert(k.clone(), *on);
                        }
                        ExtrasKey::Tuned(t) => {
                            out.keys.insert(k.clone(), true);
                            out.tables.insert(k.clone(), t.clone());
                        }
                    }
                }
                out
            }
        }
    }
}

/// What a game adds to one rung (its `RungExtrasHook`'s draw), the same
/// for every player on it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RungExtras {
    /// The game whose hook drew it (only its `apply` gets it).
    pub game: String,
    /// A melee weapon in place of the kit's on this rung: the kit's
    /// weapon of its slot (the knife's) becomes it (a weapon id in a set
    /// the definition allows; anything else is dropped).
    pub melee: Option<String>,
    /// The game's own values, which only its hook reads.
    pub values: Vec<(String, String)>,
    /// Shown after the rung's weapon in the HUD's rung line.
    pub label: Option<String>,
    /// A character every player on the rung becomes (`RungCharacter`):
    /// they hold its weapon alone, in place of the rung's gun.
    #[serde(default)]
    pub character: Option<RungCharacter>,
}

/// A special character a rung makes its players (a game's: Combat Arms'
/// infected), tied to its one weapon: on the rung a player holds that
/// weapon alone (no kit, no gun) and is the character; the next rung
/// (up or down) makes them themselves again. Its game's
/// `RungCharacterHook` plays the moments of becoming and leaving it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RungCharacter {
    /// `game:name` (its game: the prefix).
    pub id: String,
    /// The weapon it holds (a weapon id the definition allows).
    pub weapon: String,
}

impl RungCharacter {
    /// Its game (the id's prefix).
    pub fn game(&self) -> &str {
        self.id.split_once(':').map_or(self.id.as_str(), |(g, _)| g)
    }
}

/// A game's moments of a player becoming a rung's character and leaving
/// it (registered with `RungExtrasAppExt::add_rung_character`), the
/// host's: `enter` once when a player is armed for a rung of that
/// character (`arm`) and was not it, `leave` once when it is armed for
/// another rung (another character's, or none). A respawn on the same
/// rung fires neither.
#[derive(Clone, Copy)]
pub struct RungCharacterHook {
    pub game: &'static str,
    pub enter: fn(&mut World, Entity, &RungCharacter),
    pub leave: fn(&mut World, Entity, &RungCharacter),
}

/// The registered `RungCharacterHook`s.
#[derive(Resource, Clone, Default)]
pub struct RungCharacterHooks(pub Vec<RungCharacterHook>);

/// The character a player is now (`RungCharacter`; the host's).
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct AsRungCharacter(pub RungCharacter);

impl RungExtras {
    /// Value `key`'s (the first).
    pub fn value(&self, key: &str) -> Option<&str> {
        self.values.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
}

/// A rung being drawn (`RungExtrasHook::draw`).
pub struct RungDraw<'a> {
    pub def: &'a MinigameDef,
    /// The rung (0: the first) and its weapon.
    pub rung: usize,
    pub weapon: &'static str,
    pub options: &'a RungOptions,
    /// Every rung's weapon, in order (the knife finale's last).
    pub rungs: &'a [&'static str],
    /// The extras drawn for the rungs before this one.
    pub drawn: &'a [Option<RungExtras>],
}

/// A game's per-rung draw for the ladder (registered with
/// `RungExtrasAppExt::add_rung_extras`): for the rungs whose weapon is
/// its own.
#[derive(Clone, Copy)]
pub struct RungExtrasHook {
    /// The game (a weapon id's prefix).
    pub game: &'static str,
    /// The option keys it reads (`rung_extras = { <key> = false }`).
    pub options: &'static [&'static str],
    /// The rung's extras (the host's, once per ladder), drawn on `rng`.
    pub draw: fn(&World, &RungDraw, &mut Rng) -> Option<RungExtras>,
    /// Player `e` is armed for a rung: what this hook drew for it, or
    /// none (another game's rung, or nothing drawn: what it set comes
    /// off). The host's; before the rung's weapons are given.
    pub apply: fn(&mut World, Entity, Option<&RungExtras>),
}

/// The registered `RungExtrasHook`s.
#[derive(Resource, Clone, Default)]
pub struct RungExtrasHooks(pub Vec<RungExtrasHook>);

/// Registering a game's `RungExtrasHook`.
pub trait RungExtrasAppExt {
    /// `hook` (one per game: a second replaces the first).
    fn add_rung_extras(&mut self, hook: RungExtrasHook) -> &mut Self;
    /// A game's rung character moments (one per game).
    fn add_rung_character(&mut self, hook: RungCharacterHook) -> &mut Self;
}

impl RungExtrasAppExt for App {
    fn add_rung_extras(&mut self, hook: RungExtrasHook) -> &mut Self {
        let mut hooks = self.world_mut().get_resource_or_init::<RungExtrasHooks>();
        hooks.0.retain(|h| h.game != hook.game);
        hooks.0.push(hook);
        self
    }

    fn add_rung_character(&mut self, hook: RungCharacterHook) -> &mut Self {
        let mut hooks = self.world_mut().get_resource_or_init::<RungCharacterHooks>();
        hooks.0.retain(|h| h.game != hook.game);
        hooks.0.push(hook);
        self
    }
}

/// The seed rung extras are drawn from (an app's launch seed; none: 0,
/// the tests'). Each ladder made draws anew from it, the definition's id
/// and how many ladders this host made (`LaddersMade`).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RungSeed(pub u64);

/// How many ladders this host made (its rung extras' draws).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LaddersMade(pub u64);

/// `knife_finale`: off, the kit's knife, or a named melee weapon.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum Finale {
    On(bool),
    Weapon(String),
}

impl Default for Finale {
    fn default() -> Self {
        Self::On(false)
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Advance {
    #[default]
    Kill,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Demote {
    #[default]
    None,
    MeleeDeath,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Win {
    #[serde(default)]
    kind: WinKind,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WinKind {
    #[default]
    LadderEnd,
}

/// The ladder playing.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct Ladder {
    pub rungs: Vec<&'static str>,
    pub demote_on_melee: bool,
    /// The rungs' guns keep a full reserve (a ladder has no ammo to find).
    pub refill_ammo: bool,
    /// A suicide or a death with no killer costs a rung.
    pub demote_on_suicide: bool,
    /// A late joiner's rung.
    pub join_rung: JoinRung,
    /// The last rung is a knife alone (`knife_finale`): its weapon is the
    /// last of `rungs`, held without the kit.
    pub knife_finale: bool,
    /// Each rung's extras (`RungExtras`; empty: none drawn). The host
    /// draws them; a network client takes the server's.
    pub extras: Vec<Option<RungExtras>>,
    /// The definition's `rung_extras`.
    pub options: RungOptions,
}

impl Ladder {
    /// Whether `id` is one of the ladder's guns (a rung's weapon, but not
    /// the knife finale's, which is a kit weapon).
    pub fn is_gun(&self, id: &str) -> bool {
        let guns = self.rungs.len() - usize::from(self.knife_finale && !self.rungs.is_empty());
        self.rungs[..guns].contains(&id)
    }

    /// Whether `rung` is the knife-only last rung.
    pub fn knife_only(&self, rung: usize) -> bool {
        self.knife_finale && rung + 1 == self.rungs.len()
    }

    /// Rung `rung`'s extras.
    pub fn extras(&self, rung: usize) -> Option<&RungExtras> {
        self.extras.get(rung).and_then(Option::as_ref)
    }

    /// Rung `rung`'s character (`RungExtras::character`).
    pub fn character(&self, rung: usize) -> Option<&RungCharacter> {
        self.extras(rung)?.character.as_ref()
    }

    /// What a player on `rung` holds as the rung's weapon: its
    /// character's weapon (as the catalogue has it), else the rung's gun.
    pub fn weapon_at(&self, rung: usize, catalog: Option<&WeaponCatalog>) -> Option<&'static str> {
        if let Some(c) = self.character(rung) {
            return catalog?.get(&c.weapon).map(|w| w.id.as_str());
        }
        self.rungs.get(rung).copied()
    }

    /// Every rung character's weapon, as the catalogue has them.
    fn character_weapons(&self, catalog: &WeaponCatalog) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = Vec::new();
        for c in self.extras.iter().flatten().filter_map(|x| x.character.as_ref()) {
            if let Some(w) = catalog.get(&c.weapon)
                && !out.contains(&w.id.as_str())
            {
                out.push(&w.id);
            }
        }
        out
    }

    /// Every rung's melee weapon (`RungExtras::melee`), as the catalogue
    /// has them.
    fn melees(&self, catalog: &WeaponCatalog) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = Vec::new();
        for m in self.extras.iter().flatten().filter_map(|x| x.melee.as_deref()) {
            if let Some(w) = catalog.get(m)
                && !out.contains(&w.id.as_str())
            {
                out.push(&w.id);
            }
        }
        out
    }
}

/// A player's rung (0: the first). Replicated to clients (`minigame_net`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Rung(pub usize);

fn params(def: &MinigameDef) -> Result<Params, String> {
    let mut table = def.rules.params.clone();
    table.insert("module".into(), toml::Value::String(def.rules.module.clone()));
    let p: Params = table.try_into().map_err(|e| format!("[rules]: {e}"))?;
    let (Advance::Kill, WinKind::LadderEnd) = (p.advance_on, p.win.kind);
    Ok(p)
}

fn check(def: &MinigameDef, catalog: &WeaponCatalog) -> Result<(), String> {
    let p = params(def)?;
    if p.ladder.is_empty() {
        return Err("[rules] ladder: no rungs".into());
    }
    for id in &p.ladder {
        super::weapon(def, catalog, id)?;
    }
    finale(def, catalog, &p)?;
    Ok(())
}

/// The knife finale's weapon (None: off): the named one, or the kit's
/// first melee weapon for the last gun's game; a melee weapon either way.
fn finale(def: &MinigameDef, catalog: &WeaponCatalog, p: &Params) -> Result<Option<&'static str>, String> {
    let melee = |id: &str| catalog.get(id).is_some_and(|w| w.melee.is_some());
    let id = match &p.knife_finale {
        Finale::On(false) => return Ok(None),
        Finale::Weapon(id) => super::weapon(def, catalog, id)?,
        Finale::On(true) => {
            let game = p.ladder.last().and_then(|g| g.split_once(':')).map(|(g, _)| g);
            let knife = def.spawn.kit.for_game(game).into_iter().find(|id| melee(id));
            let Some(knife) = knife else {
                return Err(format!(
                    "[rules] knife_finale: the kit has no melee weapon for {}; name one (knife_finale = \"<id>\")",
                    game.unwrap_or("the last gun")
                ));
            };
            super::weapon(def, catalog, knife)?
        }
    };
    if !melee(id) {
        return Err(format!("[rules] knife_finale: {id} is not a melee weapon"));
    }
    Ok(Some(id))
}

fn start(world: &mut World, def: &MinigameDef) {
    let Ok(p) = params(def) else { return };
    let catalog = world.get_resource::<WeaponCatalog>().cloned().unwrap_or_default();
    let mut rungs: Vec<&'static str> =
        p.ladder.iter().filter_map(|id| super::weapon(def, &catalog, id).ok()).collect();
    let knife = finale(def, &catalog, &p).ok().flatten();
    rungs.extend(knife);
    world.insert_resource(crate::weapon::drop::NoDeathDrops);
    let options = RungOptions::of(&p.rung_extras);
    let extras = draw_extras(world, def, &rungs, knife.is_some(), &options);
    world.insert_resource(Ladder {
        rungs,
        demote_on_melee: p.demote_on == Demote::MeleeDeath,
        refill_ammo: p.refill_ammo,
        demote_on_suicide: p.demote_on_suicide,
        join_rung: p.join_rung,
        knife_finale: knife.is_some(),
        extras,
        options,
    });
}

/// The rungs' extras (`RungExtras`): each rung whose weapon's game has a
/// hook, but the knife finale's, drawn by it from this ladder's seeded
/// generator; none on a network client (it takes the server's) or with
/// `rung_extras = false`.
fn draw_extras(
    world: &mut World,
    def: &MinigameDef,
    rungs: &[&'static str],
    knife_finale: bool,
    options: &RungOptions,
) -> Vec<Option<RungExtras>> {
    let client = world.get_resource::<crate::core::NetRole>() == Some(&crate::core::NetRole::Client);
    let hooks = world.get_resource::<RungExtrasHooks>().cloned().unwrap_or_default();
    if client || options.off || hooks.0.is_empty() {
        return Vec::new();
    }
    for key in options.keys.keys() {
        if !hooks.0.iter().any(|h| h.options.contains(&key.as_str())) {
            warn!("{}: [rules] rung_extras: no game reads `{key}`", def.id);
        }
    }
    let made = {
        let mut n = world.get_resource_or_init::<LaddersMade>();
        n.0 += 1;
        n.0
    };
    let seed = world.get_resource::<RungSeed>().map_or(0, |s| s.0);
    let id = def.id.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3));
    let mut rng = Rng::new(seed ^ id ^ made.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let catalog = world.get_resource::<WeaponCatalog>().cloned().unwrap_or_default();
    let melee = |m: &str| super::weapon(def, &catalog, m).ok();
    let world: &World = world;
    let mut drawn: Vec<Option<RungExtras>> = Vec::new();
    for (rung, weapon) in rungs.iter().enumerate() {
        let x = (|| {
            if knife_finale && rung + 1 == rungs.len() {
                return None;
            }
            let game = weapon.split_once(':')?.0;
            let hook = hooks.0.iter().find(|h| h.game == game)?;
            let draw = RungDraw {
                def,
                rung,
                weapon,
                options,
                rungs,
                drawn: &drawn,
            };
            let mut x = (hook.draw)(world, &draw, &mut rng)?;
            x.game = game.to_string();
            x.melee = x.melee.as_deref().and_then(melee).map(str::to_string);
            // A character whose weapon the definition doesn't allow: none.
            x.character = x.character.filter(|c| melee(&c.weapon).is_some());
            Some(x)
        })();
        drawn.push(x);
    }
    drawn
}

/// `e` armed for `rung` (the host's, before its weapons are given): the
/// rung's melee weapon in place of the kit's (`runtime::KitMelee`), then
/// every hook's `apply` with its own extras for the rung, or none.
pub fn arm(world: &mut World, e: Entity, ladder: &Ladder, rung: usize) {
    let extras = ladder.extras(rung).cloned();
    let melee = extras
        .as_ref()
        .and_then(|x| x.melee.as_deref())
        .and_then(|m| world.get_resource::<WeaponCatalog>()?.get(m).map(|w| w.id.as_str()));
    match melee {
        Some(m) => {
            world.entity_mut(e).insert(runtime::KitMelee(m));
        }
        None => {
            world.entity_mut(e).remove::<runtime::KitMelee>();
        }
    }
    let hooks = world.get_resource::<RungExtrasHooks>().cloned().unwrap_or_default();
    for h in &hooks.0 {
        (h.apply)(world, e, extras.as_ref().filter(|x| x.game == h.game));
    }
    become_character(world, e, extras.and_then(|x| x.character));
}

/// `e` becomes `want` (none: itself): the old character's `leave`, then
/// the new one's `enter`, each once, only when it changes.
fn become_character(world: &mut World, e: Entity, want: Option<RungCharacter>) {
    let have = world.get::<AsRungCharacter>(e).map(|c| c.0.clone());
    if have == want {
        return;
    }
    let hooks = world.get_resource::<RungCharacterHooks>().cloned().unwrap_or_default();
    let hook = |c: &RungCharacter| hooks.0.iter().find(|h| h.game == c.game()).copied();
    if let Some(old) = have {
        world.entity_mut(e).remove::<AsRungCharacter>();
        if let Some(h) = hook(&old) {
            (h.leave)(world, e, &old);
        }
    }
    if let Some(new) = want {
        world.entity_mut(e).insert(AsRungCharacter(new.clone()));
        if let Some(h) = hook(&new) {
            (h.enter)(world, e, &new);
        }
    }
}

/// A life's arming (`RulesModule::arming`): for the player's rung.
fn arming(world: &mut World, e: Entity) {
    let Some(ladder) = world.get_resource::<Ladder>().cloned() else { return };
    let rung = world.get::<Rung>(e).map_or(0, |r| r.0).min(ladder.rungs.len().saturating_sub(1));
    arm(world, e, &ladder, rung);
}

/// The knife-only last rung's life has no kit (the knife is its
/// weapon), nor does a character's rung (its weapon alone).
fn no_kit(world: &World, e: Entity) -> bool {
    let Some(ladder) = world.get_resource::<Ladder>() else { return false };
    let rung = world.get::<Rung>(e).map_or(0, |r| r.0);
    ladder.knife_only(rung) || ladder.character(rung).is_some()
}

/// A joiner's rung: in a live match the others' lowest (or average, or
/// the first: `join_rung`), never the last; else the first (the match's
/// start puts everyone there anyway).
fn joined(world: &mut World, e: Entity) {
    let Some(ladder) = world.get_resource::<Ladder>().cloned() else { return };
    let rung = if world.resource::<Match>().counting() { join_rung(world, e, &ladder) } else { 0 };
    world.entity_mut(e).insert(Rung(rung));
}

/// The rung `join_rung` gives joiner `e`.
pub fn join_rung(world: &mut World, e: Entity, ladder: &Ladder) -> usize {
    let others: Vec<usize> = runtime::players(world)
        .into_iter()
        .filter(|(p, _)| *p != e)
        .map(|(p, _)| world.get::<Rung>(p).map_or(0, |r| r.0))
        .collect();
    let rung = match (ladder.join_rung, others.is_empty()) {
        (_, true) | (JoinRung::First, _) => 0,
        (JoinRung::Lowest, false) => others.iter().copied().min().unwrap_or(0),
        (JoinRung::Average, false) => others.iter().sum::<usize>() / others.len(),
    };
    rung.min(ladder.rungs.len().saturating_sub(2))
}

/// The rung's weapon (its character's, on a character's rung).
fn loadout(world: &World, e: Entity) -> Vec<&'static str> {
    let Some(ladder) = world.get_resource::<Ladder>() else { return Vec::new() };
    let rung = world.get::<Rung>(e).map_or(0, |r| r.0).min(ladder.rungs.len().saturating_sub(1));
    ladder.weapon_at(rung, world.get_resource::<WeaponCatalog>()).into_iter().collect()
}

/// The ladder's tick: the guns refilled, the lead followed.
fn update(world: &mut World) {
    refill(world);
    follow_lead(world);
}

/// Who leads (the players at the highest rung, then most kills), as last
/// told: `lead_taken`, `lead_lost`, `lead_tied` when it changes.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct Leaders(pub Vec<Entity>);

/// The players in the lead now (none before anyone has a rung or a kill).
pub fn leaders(world: &mut World) -> Vec<Entity> {
    let keyed: Vec<((usize, i32), Entity)> = runtime::players(world)
        .into_iter()
        .map(|(e, _)| ((world.get::<Rung>(e).map_or(0, |r| r.0), world.get::<Score>(e).map_or(0, |s| s.kills)), e))
        .collect();
    let Some(best) = keyed.iter().map(|(k, _)| *k).max() else { return Vec::new() };
    if best <= (0, 0) {
        return Vec::new();
    }
    let mut out: Vec<Entity> = keyed.into_iter().filter(|(k, _)| *k == best).map(|(_, e)| e).collect();
    out.sort();
    out
}

/// The lead's events from what changed since `Leaders`.
fn follow_lead(world: &mut World) {
    if !world.resource::<Match>().counting() {
        return;
    }
    let now = leaders(world);
    let before = world.get_resource::<Leaders>().map(|l| l.0.clone()).unwrap_or_default();
    if now == before {
        return;
    }
    let rung = |w: &World, e: Entity| w.get::<Rung>(e).map_or(0, |r| r.0) as i32;
    let mut events = Vec::new();
    for e in before.iter().filter(|e| !now.contains(e)) {
        events.push(("lead_lost", *e));
    }
    match now.as_slice() {
        [] => {}
        [one] => events.push(("lead_taken", *one)),
        many => {
            // The newcomers, and a sole leader now sharing.
            let alone = before.len() == 1;
            for e in many.iter().filter(|e| !before.contains(e) || alone) {
                events.push(("lead_tied", *e));
            }
        }
    }
    for (name, e) in events {
        let value = rung(world, e);
        world.write_message(MinigameEvent {
            name,
            player: Some(e),
            value,
        });
    }
    world.insert_resource(Leaders(now));
}

/// With `refill_ammo`, a held rung gun's reserve is topped up every tick:
/// the ladder gives each gun once, and a 200-round machine gun used up
/// (misses count) would otherwise end the climb; a rung's grenade is
/// given back once thrown.
fn refill(world: &mut World) {
    let Some(ladder) = world.get_resource::<Ladder>() else { return };
    if !ladder.refill_ammo {
        return;
    }
    let rungs = ladder.rungs.clone();
    // Kept query states (`query_cache`): this runs every tick.
    cached_query::<(&Weapon, &mut Magazine), (), _>(world, |guns, world| {
        for (w, mut m) in guns.iter_mut(world) {
            if w.owner.is_some() && m.reserve < m.reserve_max && rungs.contains(&w.id) {
                m.reserve = m.reserve_max;
            }
        }
    });
    // A grenade rung's grenade comes back once thrown (kept in the hand
    // meanwhile: `KeepSpent`), so a player throws until a kill.
    let keep: Vec<Entity> = cached_query::<(Entity, &Weapon, &Throwable), Without<KeepSpent>, _>(world, |thrown, world| {
        thrown.iter(world).filter(|(_, w, _)| w.owner.is_some() && rungs.contains(&w.id)).map(|(e, ..)| e).collect()
    });
    for e in keep {
        world.entity_mut(e).insert(KeepSpent);
    }
    cached_query::<(&Weapon, &mut Throwable), (), _>(world, |grenades, world| {
        for (w, mut t) in grenades.iter_mut(world) {
            let full = t.max.max(1);
            if w.owner.is_some() && t.count < full && rungs.contains(&w.id) {
                t.count = full;
            }
        }
    });
}

/// Everyone back to the first rung as the match starts.
fn before_spawns(world: &mut World) {
    for (e, _) in runtime::players(world) {
        world.entity_mut(e).insert(Rung(0));
    }
    world.insert_resource(Leaders::default());
}

/// An enemy kill: the killer a rung up (its weapon swapped), or the match
/// won from the last rung; one's own death (a suicide, a fall): a rung
/// down with `demote_on_suicide`.
fn on_death(world: &mut World, d: &Death) {
    match d.kind {
        DeathKind::Enemy => {}
        DeathKind::Suicide | DeathKind::NoKiller => return own_death(world, d.victim),
        DeathKind::TeamKill => return,
    }
    let Some(k) = d.killer else { return };
    let Some(ladder) = world.get_resource::<Ladder>().cloned() else { return };
    let rung = world.get::<Rung>(k).map_or(0, |r| r.0) + 1;
    if rung >= ladder.rungs.len() {
        if let Some(team) = world.get::<Team>(k).map(|t| t.0) {
            world.resource_mut::<Match>().win(team);
        }
        return;
    }
    set_rung(world, k, rung, &ladder);
    world.write_message(MinigameEvent {
        name: "rung_up",
        player: Some(k),
        value: rung as i32,
    });
    if rung + 1 == ladder.rungs.len() {
        world.write_message(MinigameEvent {
            name: "last_rung",
            player: Some(k),
            value: rung as i32,
        });
    }
}

/// `victim` died by its own doing (a suicide, a fall, the world): a rung
/// down (not below the first; dead, the weapon is the next life's).
fn own_death(world: &mut World, victim: Entity) {
    let Some(ladder) = world.get_resource::<Ladder>() else { return };
    if !ladder.demote_on_suicide {
        return;
    }
    let rung = world.get::<Rung>(victim).map_or(0, |r| r.0);
    if rung == 0 {
        return;
    }
    world.entity_mut(victim).insert(Rung(rung - 1));
    world.write_message(MinigameEvent {
        name: "suicide_down",
        player: Some(victim),
        value: rung as i32 - 1,
    });
}

/// A melee death at an enemy's hands: a rung down.
fn died(world: &mut World, d: &Died) {
    let Some(ladder) = world.get_resource::<Ladder>().cloned() else { return };
    if !ladder.demote_on_melee || !world.resource::<Match>().counting() || !melee_kill(world, d) {
        return;
    }
    let team = |e: Entity| world.get::<Team>(e).map(|t| t.0);
    let Some(k) = d.attacker.filter(|k| *k != d.entity) else { return };
    if team(k) == team(d.entity) {
        return;
    }
    let rung = world.get::<Rung>(d.entity).map_or(0, |r| r.0);
    if rung == 0 {
        return;
    }
    // Dead: the weapon is the next life's.
    world.entity_mut(d.entity).insert(Rung(rung - 1));
    world.write_message(MinigameEvent {
        name: "rung_down",
        player: Some(d.entity),
        value: rung as i32 - 1,
    });
}

/// Whether `d` was a melee kill: melee damage, or a melee weapon's (the
/// damage's weapon, else what the attacker holds; CS:S's knife deals
/// bullet damage).
fn melee_kill(world: &World, d: &Died) -> bool {
    if d.damage.kind == DamageKind::Melee {
        return true;
    }
    let id = d.damage.weapon.or_else(|| {
        let inv = world.get::<Inventory>(d.attacker?)?;
        world.get::<Weapon>(inv.active?).map(|w| w.id)
    });
    let Some(id) = id else { return false };
    world
        .get_resource::<crate::weapon::data::WeaponCatalog>()
        .and_then(|c| c.get(id))
        .is_some_and(|w| w.melee.is_some())
}

/// `e` on `rung`, holding its weapon in place of the ladder's others (on
/// the knife-only last rung: that knife alone).
pub fn set_rung(world: &mut World, e: Entity, rung: usize, ladder: &Ladder) {
    world.entity_mut(e).insert(Rung(rung));
    arm(world, e, ladder, rung);
    if ladder.knife_only(rung) {
        return knife_alone(world, e, ladder.rungs[rung]);
    }
    if ladder.character(rung).is_some() {
        if let Some(w) = ladder.weapon_at(rung, world.get_resource::<WeaponCatalog>()) {
            return knife_alone(world, e, w);
        }
    }
    let before = carried_slots(world, e, ladder);
    let old: Vec<Entity> = world
        .get::<Inventory>(e)
        .map(|i| {
            i.weapons
                .iter()
                .copied()
                .filter(|w| world.get::<Weapon>(*w).is_some_and(|w| ladder.is_gun(w.id)))
                .collect()
        })
        .unwrap_or_default();
    for w in old {
        runtime::take_weapon(world, e, w);
    }
    // A kit by game (the gun game's knife) goes with the new gun's game,
    // a rung's melee weapon in place of the kit's.
    // The rungs' melee weapons and characters' weapons: taken when not
    // this rung's (the kit given back after a character's rung).
    let melees = world
        .get_resource::<WeaponCatalog>()
        .map(|c| {
            let mut m = ladder.melees(c);
            m.extend(ladder.character_weapons(c));
            m
        })
        .unwrap_or_default();
    runtime::refit_kit(world, e, ladder.rungs[rung], &melees);
    give(world, e, ladder.rungs[rung]);
    keep_slot_order(world, e, &before, ladder);
    // No command of the player's did this: its client takes the state
    // as a restart, not a misprediction.
    crate::core::rebase(world, e);
}

/// `e` holding `knife` alone, drawn: every other weapon taken (the gun,
/// the kit's others), the knife kept if carried, else given.
fn knife_alone(world: &mut World, e: Entity, knife: &'static str) {
    let held: Vec<(Entity, &'static str)> = world
        .get::<Inventory>(e)
        .map(|i| i.weapons.iter().filter_map(|w| world.get::<Weapon>(*w).map(|x| (*w, x.id))).collect())
        .unwrap_or_default();
    let mut kept = None;
    for (w, id) in held {
        if id == knife && kept.is_none() {
            kept = Some(w);
        } else {
            runtime::take_weapon(world, e, w);
        }
    }
    let knife = kept.or_else(|| give(world, e, knife));
    if let (Some(k), Some(mut inv)) = (knife, world.get_mut::<Inventory>(e)) {
        inv.wanted = Some(k);
    }
    crate::core::rebase(world, e);
}

/// What `e` carries as (weapon, slot, is one of the ladder's guns: else
/// the kit's, the knife).
fn carried_slots(world: &World, e: Entity, ladder: &Ladder) -> Vec<(Entity, u8, bool)> {
    let Some(inv) = world.get::<Inventory>(e) else { return Vec::new() };
    inv.weapons
        .iter()
        .filter_map(|w| {
            let weapon = world.get::<Weapon>(*w)?;
            Some((*w, weapon.slot, ladder.is_gun(weapon.id)))
        })
        .collect()
}

/// `e`'s slot order for `lastinv` (`Inventory::slot_mru`) across a swap:
/// the weapons it no longer holds since `before` (the old gun, a knife of
/// the other game) hand their slot's place to the ones that came in (the
/// new gun, the other game's knife), a gun to a gun and a kit weapon to a
/// kit weapon, so Q still goes gun <-> knife whichever slots they are in.
fn keep_slot_order(world: &mut World, e: Entity, before: &[(Entity, u8, bool)], ladder: &Ladder) {
    let now = carried_slots(world, e, ladder);
    for gun in [true, false] {
        let gone = before.iter().filter(|(w, _, g)| *g == gun && !now.iter().any(|(n, ..)| n == w));
        let came = now.iter().filter(|(w, _, g)| *g == gun && !before.iter().any(|(b, ..)| b == w));
        let pairs: Vec<(u8, u8)> = gone.zip(came).map(|(g, c)| (g.1, c.1)).collect();
        if let Some(mut inv) = world.get_mut::<Inventory>(e) {
            // Through a spare number first: two slots may trade places.
            for (i, (old, _)) in pairs.iter().enumerate() {
                inv.rename_slot(*old, 200 + i as u8);
            }
            for (i, (_, new)) in pairs.iter().enumerate() {
                inv.rename_slot(200 + i as u8, *new);
            }
        }
    }
}

/// The time limit: the highest rung, then the most kills; a tie, a draw.
fn time_out(world: &mut World) {
    let mut best: Option<((usize, i32), u8)> = None;
    let mut tie = false;
    for (e, team) in runtime::players(world) {
        let key = (world.get::<Rung>(e).map_or(0, |r| r.0), world.get::<Score>(e).map_or(0, |s| s.kills));
        match best {
            Some((b, _)) if key == b => tie = true,
            Some((b, _)) if key < b => {}
            _ => {
                best = Some((key, team));
                tie = false;
            }
        }
    }
    if let Some((_, team)) = best.filter(|_| !tie) {
        world.resource_mut::<Match>().winner = Some(team);
    }
}

/// `e`'s place on the ladder, for `minigame_status`.
pub fn standing(world: &World, e: Entity) -> Option<String> {
    let active = world.get_resource::<ActiveMinigame>()?;
    if active.rules.mode.id != LADDER {
        return None;
    }
    let ladder = world.get_resource::<Ladder>()?;
    let rung = world.get::<Rung>(e).map_or(0, |r| r.0);
    Some(format!(", rung {}/{} ({})", rung + 1, ladder.rungs.len(), ladder.rungs.get(rung).unwrap_or(&"?")))
}

/// The HUD's lines for `viewer`: their rung and gun, the next one, who
/// leads (the highest rung, then the most kills; "tied" when level).
fn status(world: &mut World, viewer: Entity) -> Vec<HudLine> {
    let Some(ladder) = world.get_resource::<Ladder>().cloned() else { return Vec::new() };
    let total = ladder.rungs.len();
    let rung = world.get::<Rung>(viewer).map_or(0, |r| r.0).min(total.saturating_sub(1));
    let mut lines = Vec::new();
    let catalog = world.get_resource::<WeaponCatalog>();
    let (now, next) = (ladder.weapon_at(rung, catalog), ladder.weapon_at(rung + 1, catalog));
    if let Some(w) = &now {
        // The rung's label (its fitted parts) after the weapon.
        let label = ladder.extras(rung).and_then(|x| x.label.as_deref()).map(|l| format!(" ({})", l.replace(['{', '}'], "")));
        lines.push(HudLine::new(format!("Rung {}/{total}: {{weapon}}{}", rung + 1, label.unwrap_or_default())).weapon(w));
    }
    match next {
        Some(w) => lines.push(HudLine::new("Next: {weapon}").weapon(w)),
        None => lines.push(HudLine::new("Last rung: a kill wins")),
    }
    let mut best: Option<((usize, i32), Entity)> = None;
    let mut tied = false;
    for (e, _) in runtime::players(world) {
        let key = (world.get::<Rung>(e).map_or(0, |r| r.0), world.get::<Score>(e).map_or(0, |s| s.kills));
        match best {
            Some((b, _)) if key == b => tied = true,
            Some((b, _)) if key < b => {}
            _ => {
                best = Some((key, e));
                tied = false;
            }
        }
    }
    if let Some(((r, _), who)) = best {
        lines.push(if tied {
            HudLine::new("Tied for the lead")
        } else if who == viewer {
            HudLine::new("You lead")
        } else {
            HudLine::new(format!("Leader: {{player}}, rung {}/{total}", r + 1)).player(who)
        });
    }
    lines
}

/// The viewer's own rung changes (`rung_up`, `rung_down`, `suicide_down`).
fn notice(world: &World, viewer: Entity, event: &MinigameEvent) -> Option<HudLine> {
    if event.player != Some(viewer) {
        return None;
    }
    let ladder = world.get_resource::<Ladder>()?;
    let total = ladder.rungs.len();
    let weapon = ladder.weapon_at(usize::try_from(event.value).ok()?, world.get_resource::<WeaponCatalog>())?;
    let text = match event.name {
        "rung_up" => format!("Rung up! {}/{total}: {{weapon}}", event.value + 1),
        "rung_down" => format!("Knifed! Down to {}/{total}: {{weapon}}", event.value + 1),
        "suicide_down" => format!("Suicide! Down to {}/{total}: {{weapon}}", event.value + 1),
        _ => return None,
    };
    Some(HudLine::new(text).weapon(weapon))
}

/// The viewer's big callouts (`hud::Callout`): its own rung changes, the
/// last rung (its own and others'), its lead.
fn callout(world: &World, viewer: Entity, event: &MinigameEvent) -> Option<Callout> {
    let ladder = world.get_resource::<Ladder>()?;
    let total = ladder.rungs.len();
    let rung = usize::try_from(event.value).ok()?;
    let weapon = ladder.weapon_at(rung, world.get_resource::<WeaponCatalog>());
    let own = event.player == Some(viewer);
    let level_line = || HudLine::new(format!("Level {}/{total}", rung + 1));
    let gun = |text: &str| {
        let line = HudLine::new(text);
        match weapon {
            Some(w) => line.weapon(w),
            None => line,
        }
    };
    Some(match (event.name, own) {
        // The last rung's own callout says it.
        ("rung_up", true) if rung + 1 < total => Callout::new(2, "rung_up", vec![gun("Level up: {weapon}"), level_line()]),
        ("rung_down", true) => Callout::new(3, "rung_down", vec![gun("Knifed! Level down: {weapon}"), level_line()]),
        // The same denied cue as a knifing.
        ("suicide_down", true) => Callout::new(3, "rung_down", vec![gun("Suicide! Level down: {weapon}"), level_line()]),
        ("last_rung", true) => Callout::new(4, "last_rung", vec![HudLine::new("Last level!"), gun("{weapon}: a kill wins")]),
        ("last_rung", false) => Callout::new(
            4,
            "last_rung_other",
            vec![HudLine::new("{player} is on the last level!").player(event.player?)],
        ),
        ("lead_taken", true) => Callout::new(1, "lead_taken", vec![HudLine::new("You took the lead")]),
        ("lead_lost", true) => Callout::new(1, "lead_lost", vec![HudLine::new("You lost the lead")]),
        ("lead_tied", true) => Callout::new(1, "lead_tied", vec![HudLine::new("Tied for the lead")]),
        _ => return None,
    })
}
