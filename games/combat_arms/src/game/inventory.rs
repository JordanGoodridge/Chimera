//! Combat Arms' inventory: what a player carries and how it switches,
//! from specs/combat_arms/inventory.md (section numbers below are its).
//!
//! - Classes (1): a weapon's class is its data `slot` + 1 (0 primary,
//!   1 secondary, 2 melee, 3 throw, 4 special, 5 objective; any other
//!   slot is class 0: never selected by key, never dropped). The list is
//!   `weapon::Inventory::weapons` (its index is the equip index, at most
//!   `MAX_CARRIED`); a class's order slots are its weapons in that order.
//!   The loadout is `weapon::StartingWeapons` in loadout order (primary,
//!   secondary, melee, throw, then backpack items); at spawn the lowest
//!   class is drawn. Per-weapon data the frame doesn't have is the
//!   `combat_arms:inventory` hook (`CaItem`: hide-when-empty, drop on
//!   death, infinite ammo, the post-sprint time, the select-rate bonus).
//! - Switching (3, 4, 6): `CaSelection` on each character keeps the class
//!   cursors, each class's last chosen weapon and the previous weapon (Q).
//!   Class keys (`Intent::select`, slot k = class k + 1) select or cycle
//!   with no re-draw; Q (`Intent::last_weapon`) follows section 4 to the
//!   letter (it overwrites "previous" before checking). Chimera's
//!   selection is told to leave keys to us (`weapon::OwnSelection`) and
//!   only applies `Inventory::wanted`: the holster is instant (its reload,
//!   burst and zoom abandoned) and the draw is `Weapon::draw_time`, the
//!   select animation's length over the played rate (`played_rate`: above
//!   `DRAW_RATE_CAP` it plays at 1). A magazine weapon drawn with an empty
//!   clip and spare rounds reloads at once instead of drawing (6.6).
//! - What interrupts what (7): `hands` runs before the weapon frame: R
//!   during a magazine weapon's draw starts the reload at once (Quirk 2);
//!   sprint (`MovementState::sprinting`) puts an idle weapon in the sprint
//!   pose, which holds fire until sprint ends and the weapon's post-sprint
//!   time (`CaItem::sprint_out`) passes; a reload started or running under
//!   sprint completes; holding use holds fire.
//! - The knife (8): the `combat_arms:knife` hook (`Knife`), its own state
//!   machine per tick: the first update after a press arms it, the next
//!   starts the pre-fire, the next the swing; each hit key casts one ray
//!   from the eye (range, first thing hit, no penetration) dealing
//!   `B x u x b` (u uniform in 1 +- the damage spread, b = 1 + `BEHIND_BONUS`
//!   from the victim's rear sector, `behind`); a held button chains the
//!   next swing at once, fire beating alt-fire. No quick-knife (9).
//! - Auto-switch (10): a hide-when-empty weapon in hand that is spent (or
//!   gone with its last round) gives the last chosen primary, else
//!   secondary, else melee, and clears Q.
//! - Pickup (11): `pickups` aims the view ray at dropped items (`Dropped`,
//!   with a `map::loose::LooseItem` to draw) each tick
//!   (`CaSelection::pickup_target`, for the HUD's prompt); E on one calls
//!   `pick_up`: a same-id non-primary adds its ammo (clamped) and fills
//!   the clip; otherwise it swaps for the class's last chosen weapon in the
//!   same equip index (the old one dropped at the left foot with jitter
//!   and a random yaw) and draws it with the item's clip. Ammo travels
//!   with the gun (`Dropped::total`, `clip`). No walk-over pickup and no
//!   drop key.
//! - Death drop (12): the last chosen primary drops at the left foot (no
//!   jitter) when `CaDeathDrops` allows and its data does; items go after
//!   `DROPPED_LIFETIME`. Respawn (13): `respawn_inventory`.
//!
//! Known shortcuts:
//! - Units: 1 LithTech unit is taken as 1 cm (`UNIT`); the movement spec
//!   leaves the scale open (its Open question 3). Health 100 = 1.0 (`HP`).
//! - Updates are the game's fixed tick (movement.md's 15 ms,
//!   `game::TICK_INTERVAL`), not rendered frames as inventory.md has the
//!   weapon logic run (a question for both specs: one clock or two?), so
//!   every "next update" is a tick: the knife's first hit lands a tick
//!   plus the pre-fire plus 155 ms after the press (spec: "plus 2 or 3
//!   frames").
//! - The knife ray has no spread (the M9's 1/7 perturb is
//!   weapon_firing.md's), k_zone is 1 (hitgroup generic: the zone table
//!   is weapon_firing.md's), and m, g, v and k_range are 1 (no mods,
//!   buffs, figure types or range table). `behind` is public for the guns'
//!   x1.3 (Quirk 6), which this module doesn't apply.
//! - Fire-gating by sprint and use applies to magazine weapons and the
//!   knife; thrown weapons follow Chimera's `grenade::Throwable`, which
//!   also removes a spent grenade (CA keeps it at total 0, unselectable):
//!   Q to it then finds no class and stops, as section 4 does for a
//!   swapped weapon. The cooked-grenade release on death (12.1) is
//!   grenades.md's. A shell-by-shell weapon's reload during sprint or its
//!   draw goes to Chimera's frame (refused while held).
//! - Reloads started here (6.6, Quirk 2, under sprint) play no reload
//!   sounds (the frame's sound queue is private).
//! - The left foot is the hull's bottom, a quarter of its width to the
//!   left; the hitbox width is the hull's. No skeleton yet.
//! - Pickup: the fallback test (centre distance - 35 <= 100) applies to
//!   an item the view ray hits beyond 100 units (the most literal reading
//!   tying it to the pickup ray; the spec could also mean any item in
//!   reach). An item's box is its body's, else 10 units each way. The
//!   owner/team, human-model, infection, VIP and buff checks, map-placed
//!   items' respawn and the "better weapon" rule (Open question 6) aren't
//!   there; the non-primary pickup fills the clip as the server does
//!   (Open question 5).
//! - Class keys: the objective class (key 6) isn't reachable from the
//!   client's input (it reads slot1..slot5) and the bomb rule, "in use"
//!   weapons, forced-select and the menu/vote/weapons-disabled
//!   conditions aren't modelled (no such weapons or states yet). Q and a
//!   class's last chosen weapon are kept as entities (an equip index plus
//!   id in the game: the same outcome when a pickup replaces one).
//! - Death drop: every character counts as a human model; the backpack
//!   perk's extra drops (Open question 8) aren't there; dropped items
//!   fall only where a map gives them a body (`map::loose`).
//! - Respawn: `respawn_inventory` rebuilds the loadout; the game modes
//!   (game_modes.md) decide when.

use avian3d::prelude::{ColliderOf, SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;
use serde::Deserialize;

use super::weapons::RIFLE;
use crate::{
    core::{
        self, Damage, DamageKind, Died, FirstTimePredicted, Health, Hitgroup, Intent, MovementState, NetRole, Predict,
        SimClock, SimSet,
    },
    map::{PhysicsProp, interp::NetDrawn, loose::LooseItem},
    weapon::{
        Inventory, Magazine, OwnSelection, SelectWeapons, ShellReload, StartingWeapons, Weapon, WeaponEvent,
        WeaponEventKind, WeaponFrame, WeaponState,
        data::{WeaponData, WeaponDataAppExt},
        grenade::Throwable,
        random::{Rng, shared_random},
    },
};

/// Combat Arms' knife, the M9 (data/weapons/m9.toml).
pub const KNIFE: &str = "combat_arms:m9";

/// What every Combat Arms player starts with, in loadout order (primary,
/// secondary, melee, throw, backpack; 1). PLACEHOLDER: the skeleton's
/// rifle until weapon_firing.md's and the M9.
pub const STARTING_WEAPONS: &[&str] = &[RIFLE, KNIFE];

/// Metres per LithTech unit (ours: 1 cm; see Known shortcuts).
pub const UNIT: f32 = 0.01;
/// Health per damage point (100 points = 1.0).
pub const HP: f32 = 0.01;

/// Weapons a player carries at most (max_carried).
pub const MAX_CARRIED: usize = 16;
/// Seconds the outgoing weapon takes to go away (holster_time): none.
pub const HOLSTER_TIME: f32 = 0.0;
/// An animation rate above this plays at 1.0 (draw_rate_cap).
pub const DRAW_RATE_CAP: f32 = 2.0;
/// Farthest a pickup's surface hit may be from the camera, m
/// (activation_distance, 100 units).
pub const ACTIVATION_DISTANCE: f32 = 100.0 * UNIT;
/// Taken off the camera-to-centre distance for the fallback pickup test,
/// m (activation_fallback_slack, 35 units).
pub const ACTIVATION_FALLBACK_SLACK: f32 = 35.0 * UNIT;
/// Seconds a dropped item lasts (dropped_lifetime).
pub const DROPPED_LIFETIME: f64 = 20.0;
/// A hit from the victim's rear sector does x (1 + this) (behind_bonus).
pub const BEHIND_BONUS: f32 = 0.3;
/// The damage roll when a weapon's data gives none, and when its value
/// lies outside [0, 1] (damage_spread_default, damage_spread_fallback).
pub const DAMAGE_SPREAD_DEFAULT: f32 = 0.1;
pub const DAMAGE_SPREAD_FALLBACK: f32 = 0.2;
/// Total shown and used for infinite-ammo weapons (infinite_ammo).
pub const INFINITE_AMMO: u32 = 1000;

/// The classes (1), as `class_of` gives them; 0 is none.
pub const PRIMARY: u8 = 1;
pub const SECONDARY: u8 = 2;
pub const MELEE: u8 = 3;
pub const THROW: u8 = 4;
pub const SPECIAL: u8 = 5;
pub const OBJECTIVE: u8 = 6;
const CLASSES: usize = 7;

/// A dropped item's box when its body has none (ours: 10 units each way).
const ITEM_HALF: Vec3 = Vec3::splat(10.0 * UNIT);
/// Timers due on a tick fire on it (as the weapon frame's slack).
const SLACK: f64 = 1e-5;

/// The weapon data's hook for what this module reads of a weapon.
pub const ITEM_HOOK: &str = "combat_arms:inventory";
/// The weapon data's hook for a knife's attacks.
pub const KNIFE_HOOK: &str = "combat_arms:knife";

/// A weapon's class (1) from its data slot.
pub fn class_of(slot: u8) -> u8 {
    if slot < 6 { slot + 1 } else { 0 }
}

/// The rate an animation plays at for a summed rate `rate` (6.4: above
/// `DRAW_RATE_CAP` it plays at 1).
pub fn played_rate(rate: f32) -> f32 {
    if rate > DRAW_RATE_CAP { 1.0 } else { rate }
}

/// The damage roll's spread r for a weapon's data value (8.4).
pub fn damage_spread(value: Option<f32>) -> f32 {
    match value {
        None => DAMAGE_SPREAD_DEFAULT,
        Some(r) if (0.0..=1.0).contains(&r) => r,
        Some(_) => DAMAGE_SPREAD_FALLBACK,
    }
}

/// Whether a hit along `dir` (shooter towards victim) comes from the
/// rear sector of a victim facing `forward` with `right` its right
/// (8.4): theta = atan2(f.d, r.d), sector s = floor((pi + theta) / (pi/6)),
/// behind when 7 <= s <= 11 (a 150 degree sector, not symmetric).
pub fn behind(forward: Vec3, right: Vec3, dir: Vec3) -> bool {
    use std::f32::consts::PI;
    let theta = forward.dot(dir).atan2(right.dot(dir));
    let s = ((PI + theta) / (PI / 6.0)).floor() as i32;
    (7..=11).contains(&s)
}

// ---------------------------------------------------------------------------
// Parts

/// What this module reads of a weapon (`combat_arms:inventory` hook).
#[derive(Component, Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct CaItem {
    /// Unselectable at total 0, and auto-switched from when spent (1, 10).
    pub hide_when_empty: bool,
    /// The death drop takes it (12.4; a primary's data flag).
    pub drop_on_death: bool,
    /// Infinite ammo (knives): total `INFINITE_AMMO`.
    pub infinite: bool,
    /// The post-sprint animation, s (the stock table's sprint-out).
    pub sprint_out: f32,
    /// Summed select-rate bonuses (mods, model, sets; 6.4): the draw
    /// plays at `played_rate(1 + this)`.
    pub select_rate: f32,
}

impl Default for CaItem {
    fn default() -> Self {
        Self {
            hide_when_empty: false,
            drop_on_death: true,
            infinite: false,
            sprint_out: 0.0,
            select_rate: 0.0,
        }
    }
}

/// One knife attack as the data gives it (seconds, damage points).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackData {
    prefire: f32,
    length: f32,
    hits: Vec<f32>,
    damage: f32,
    postfire: f32,
}

/// The `combat_arms:knife` hook's parameters (lengths in the document's
/// units, damage in its damage points).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct KnifeData {
    range: f32,
    #[serde(default)]
    damage_spread: Option<f32>,
    /// The weapon's fire-rate bonus (f_weapon, 8.2).
    #[serde(default)]
    fire_rate: f32,
    primary: AttackData,
    #[serde(default)]
    secondary: Option<AttackData>,
}

/// One knife attack (8.2): seconds of animation time, damage as health.
#[derive(Clone, Debug, PartialEq)]
pub struct Attack {
    pub prefire: f32,
    pub length: f32,
    /// Hit events, seconds into the swing.
    pub hits: Vec<f32>,
    pub damage: f32,
    pub postfire: f32,
}

/// Where a knife attack is (8.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KnifePhase {
    #[default]
    Idle,
    /// "Firing" set; the pre-fire starts on the next update.
    Armed,
    PreFire,
    Swing,
    PostFire,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct KnifeState {
    pub phase: KnifePhase,
    /// 0 primary, 1 secondary.
    pub attack: usize,
    /// When the phase began.
    pub since: f64,
    /// Hit events of this swing already made.
    pub hits: usize,
}

/// A knife (`combat_arms:knife`): two attacks with timed hit events.
#[derive(Component, Clone, Debug)]
pub struct Knife {
    /// Metres.
    pub range: f32,
    /// The damage roll's r.
    pub spread: f32,
    /// The played fire rate (rho_fire after the cap).
    pub rate: f32,
    /// Primary, then the secondary if any.
    pub attacks: Vec<Attack>,
    pub state: KnifeState,
}

impl Knife {
    /// Whether an attack animation is playing (not idle).
    pub fn busy(&self) -> bool {
        self.state.phase != KnifePhase::Idle
    }
}

/// A character's selection state (1, 3, 4) and its hands' (7).
#[derive(Component, Clone, Debug, Default)]
pub struct CaSelection {
    /// Per class: the order slot last used.
    pub cursor: [usize; CLASSES],
    /// Per class: the weapon last chosen (the auto-switch and the death
    /// drop use it).
    pub last_chosen: [Option<Entity>; CLASSES],
    /// The previous weapon (Q).
    pub previous: Option<Entity>,
    /// The weapon in hand as last seen, and whether it hides when empty.
    pub in_hand: Option<Entity>,
    in_hand_hides: bool,
    /// In the sprint pose (7), and when the post-sprint animation ends.
    pub sprint_pose: bool,
    pub sprint_out_until: f64,
    /// The dropped item the view is on (the HUD's pickup prompt; 11).
    pub pickup_target: Option<Entity>,
    prev_select: Option<u8>,
    prev_last: bool,
    prev_use: bool,
}

/// A weapon lying in the world (11, 12): its weapon entity (mods and the
/// rest stay on it), when it was dropped, and the ammo it carries.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Dropped {
    pub weapon: Entity,
    pub since: f64,
    pub total: u32,
    pub clip: u32,
}

/// Whether the dead drop their primary (12.3): off in snowball mode and
/// the VIP weapon race (the rules set it).
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaDeathDrops(pub bool);

impl Default for CaDeathDrops {
    fn default() -> Self {
        Self(true)
    }
}

/// A weapon's total (the ammo model: the clip included).
pub fn total_of(item: Option<&CaItem>, mag: Option<&Magazine>, throw: Option<&Throwable>) -> u32 {
    if item.is_some_and(|i| i.infinite) {
        return INFINITE_AMMO;
    }
    match (mag, throw) {
        (Some(m), _) => m.clip + m.reserve,
        (None, Some(t)) => t.count,
        (None, None) => INFINITE_AMMO,
    }
}

// ---------------------------------------------------------------------------
// The plugin

/// Combat Arms' inventory: its hooks (registered before the data loads:
/// `CombatArmsPlugins` adds this before `weapons`), the starting weapons
/// unless the app set some first (a test's), and its systems.
pub struct CaInventoryPlugin;

impl Plugin for CaInventoryPlugin {
    fn build(&self, app: &mut App) {
        let set = app.world().get_resource::<StartingWeapons>().is_some_and(|s| !s.is_empty());
        if !set {
            app.insert_resource(StartingWeapons {
                team: Vec::new(),
                all: STARTING_WEAPONS.to_vec(),
            });
        }
        app.register_weapon_hook::<CaItem>(ITEM_HOOK, |e, d, p| {
            let mut item = p.clone();
            item.sprint_out = item.sprint_out.max(0.0);
            if let Some(mut w) = e.get_mut::<Weapon>() {
                w.draw_time = d.draw / played_rate(1.0 + item.select_rate);
            }
            e.insert(item);
        })
        .register_weapon_hook::<KnifeData>(KNIFE_HOOK, |e, d, p| {
            e.insert(knife(d, p));
        })
        .insert_resource(OwnSelection)
        .init_resource::<CaDeathDrops>()
        .add_systems(
            Predict::Select,
            (select.before(SelectWeapons), after_select.after(SelectWeapons)),
        )
        // The knife first: the hands see it idle on the update it ends.
        .add_systems(Predict::Weapons, (swing_knives, hands).chain().before(WeaponFrame))
        .add_systems(
            FixedUpdate,
            (
                pickups.before(SelectWeapons).before(SimSet::Movement),
                death_drops.after(core::apply_damage),
                expire_items.after(SimSet::Weapons),
            )
                .run_if(core::authoritative),
        );
    }
}

fn knife(d: &WeaponData, p: &KnifeData) -> Knife {
    let dmg = d.units.damage;
    let attack = |a: &AttackData| Attack {
        prefire: a.prefire,
        length: a.length,
        hits: a.hits.clone(),
        damage: a.damage * dmg,
        postfire: a.postfire,
    };
    Knife {
        range: p.range * d.units.length,
        spread: damage_spread(p.damage_spread),
        rate: played_rate(1.0 + p.fire_rate),
        attacks: std::iter::once(attack(&p.primary)).chain(p.secondary.iter().map(attack)).collect(),
        state: KnifeState::default(),
    }
}

type WeaponInfo<'a> = (
    &'a Weapon,
    Option<&'a CaItem>,
    Option<&'a Magazine>,
    Option<&'a Throwable>,
);

fn total(i: &WeaponInfo) -> u32 {
    total_of(i.1, i.2, i.3)
}

fn hides(i: &WeaponInfo) -> bool {
    i.1.is_some_and(|c| c.hide_when_empty)
}

/// A class's weapons in order (its order slots).
fn order(inv: &Inventory, class: u8, weapons: &Query<WeaponInfo>) -> Vec<Entity> {
    inv.weapons
        .iter()
        .copied()
        .filter(|w| weapons.get(*w).is_ok_and(|i| class_of(i.0.slot) == class))
        .collect()
}

// ---------------------------------------------------------------------------
// Selection (3, 4, 10)

/// Class keys, Q and the auto-switch, before Chimera's selection applies
/// `Inventory::wanted`; a new character draws its lowest class (1).
fn select(
    mut owners: Query<(Entity, &Intent, &mut Inventory, Option<&mut CaSelection>, Option<&Health>), Without<NetDrawn>>,
    weapons: Query<WeaponInfo>,
    mut commands: Commands,
) {
    for (owner, intent, mut inv, sel, health) in &mut owners {
        let Some(mut sel) = sel else {
            let lowest = inv
                .weapons
                .iter()
                .copied()
                .filter_map(|w| weapons.get(w).ok().map(|i| (w, class_of(i.0.slot))))
                .filter(|(_, c)| *c != 0)
                .min_by_key(|(_, c)| *c)
                .map(|(w, _)| w);
            if lowest.is_some() {
                inv.wanted = lowest;
            }
            commands.entity(owner).insert(CaSelection {
                prev_select: intent.select,
                prev_last: intent.last_weapon,
                prev_use: intent.use_key,
                ..default()
            });
            continue;
        };
        let alive = health.is_none_or(|h| h.current > 0.0);

        // 10. A spent hide-when-empty weapon in hand (or one gone with its
        // last round) gives the last chosen primary, secondary or melee.
        let gone = sel.in_hand.is_some_and(|w| !inv.weapons.contains(&w)) && sel.in_hand_hides;
        let spent = inv.active.and_then(|a| weapons.get(a).ok()).is_some_and(|i| {
            hides(&i) && total(&i) == 0 && !i.3.is_some_and(|t| t.primed() || t.redraw_at.is_some())
        });
        if alive && (gone || spent) {
            sel.previous = None;
            sel.in_hand_hides = false;
            let fallback = [PRIMARY, SECONDARY, MELEE].into_iter().find_map(|c| {
                sel.last_chosen[c as usize].filter(|w| inv.weapons.contains(w) && inv.active != Some(*w))
            });
            if let Some(w) = fallback {
                inv.wanted = Some(w);
            }
        }

        // 3. Class keys, on the press.
        let pressed = intent.select != sel.prev_select;
        sel.prev_select = intent.select;
        if alive
            && pressed
            && let Some(select) = intent.select
        {
            let (slot, nth) = Intent::picked(select);
            if let Some(next) = class_key(&sel, &inv, class_of(slot), nth, &weapons)
                && inv.active != Some(next)
            {
                sel.previous = inv.active;
                inv.wanted = Some(next);
            }
        }

        // 4. Q, on the press.
        let q = intent.last_weapon && !sel.prev_last;
        sel.prev_last = intent.last_weapon;
        if alive
            && q
            && let Some(prev) = sel.previous
            && inv.active != Some(prev)
        {
            let target = weapons.get(prev).ok().filter(|_| inv.weapons.contains(&prev));
            sel.previous = inv.active;
            if let Some(i) = target
                && class_of(i.0.slot) != 0
                && !(total(&i) == 0 && hides(&i))
            {
                inv.wanted = Some(prev);
            }
        }
    }
}

/// The weapon class key `class` picks (3): the next order slot when one
/// of its weapons is in hand (wrapping), else the cursor's; classes above
/// melee skip a hide-when-empty weapon at total 0 and search on for one
/// with rounds. `nth`: a pick of that order slot (the HUD's selection).
fn class_key(sel: &CaSelection, inv: &Inventory, class: u8, nth: Option<u8>, weapons: &Query<WeaponInfo>) -> Option<Entity> {
    if class == 0 || class as usize >= CLASSES {
        return None;
    }
    let list = order(inv, class, weapons);
    if list.is_empty() {
        return None;
    }
    let mut i = sel.cursor[class as usize];
    if class >= THROW && i >= list.len() {
        // Nothing left at the cursor (2).
        return None;
    }
    if let Some(n) = nth {
        i = (n as usize).min(list.len() - 1);
    } else if inv.active.is_some_and(|a| list.contains(&a)) {
        i += 1;
        if i >= list.len() {
            i = 0;
        }
    } else if i >= list.len() {
        i = 0;
    }
    if class <= MELEE {
        return Some(list[i]);
    }
    let info = |w: Entity| weapons.get(w).ok();
    for k in 0..list.len() {
        let w = list[(i + k) % list.len()];
        let Some(x) = info(w) else { continue };
        let usable = if k == 0 {
            !(total(&x) == 0 && hides(&x))
        } else {
            total(&x) > 0
        };
        if usable {
            return Some(w);
        }
    }
    None
}

/// After a switch: the destination's class cursor and last chosen (1),
/// and a magazine weapon drawn empty with spare rounds reloads at once
/// instead of drawing (6.6).
#[allow(clippy::type_complexity)]
fn after_select(
    mut owners: Query<(Entity, &mut Inventory, &mut CaSelection), Without<NetDrawn>>,
    slots: Query<&Weapon>,
    mut weapons: Query<(&Weapon, Option<&CaItem>, Option<&Magazine>, &mut WeaponState, Has<ShellReload>)>,
    mut events: MessageWriter<WeaponEvent>,
    clock: Res<SimClock>,
    first: Res<FirstTimePredicted>,
) {
    for (owner, mut inv, mut sel) in &mut owners {
        if inv.active == sel.in_hand {
            continue;
        }
        sel.in_hand = inv.active;
        sel.in_hand_hides = false;
        sel.sprint_pose = false;
        let Some(active) = inv.active else { continue };
        let Ok(slot) = slots.get(active).map(|w| w.slot) else { continue };
        let class = class_of(slot);
        let index = inv
            .weapons
            .iter()
            .filter(|w| slots.get(**w).is_ok_and(|x| class_of(x.slot) == class))
            .position(|w| *w == active);
        if class != 0 {
            sel.cursor[class as usize] = index.unwrap_or(0);
            sel.last_chosen[class as usize] = Some(active);
        }
        let Ok((_, item, mag, mut st, shells)) = weapons.get_mut(active) else { continue };
        sel.in_hand_hides = item.is_some_and(|i| i.hide_when_empty);
        if let Some(mag) = mag
            && mag.clip == 0
            && !shells
            && start_reload(&mut inv, &mut st, mag, clock.now)
        {
            events.write(WeaponEvent {
                owner,
                weapon: active,
                kind: WeaponEventKind::ReloadStarted,
                replay: !first.0,
            });
        }
    }
}

/// Start a magazine reload now (the frame's rule: refused when no round
/// would move); its end holds every attack.
fn start_reload(inv: &mut Inventory, st: &mut WeaponState, mag: &Magazine, now: f64) -> bool {
    if (mag.size - mag.clip).min(mag.reserve) == 0 {
        return false;
    }
    let end = now + mag.reload_time as f64;
    inv.next_attack = end;
    st.next_primary = end;
    st.next_secondary = end;
    st.reload_end = Some(end);
    st.fired_on_empty = false;
    st.burst_left = 0;
    true
}

// ---------------------------------------------------------------------------
// The hands: draw, reload, sprint and use (7)

/// Before the weapon frame: R during a magazine weapon's draw reloads at
/// once (Quirk 2); the sprint pose and the post-sprint animation hold
/// fire, as holding use does; reloads start and complete under them.
#[allow(clippy::type_complexity)]
fn hands(
    mut owners: Query<
        (Entity, &Intent, &mut Inventory, &mut CaSelection, &MovementState, Option<&Health>),
        Without<NetDrawn>,
    >,
    mut weapons: Query<(
        Option<&CaItem>,
        Option<&mut Magazine>,
        &mut WeaponState,
        Has<ShellReload>,
        Option<&Knife>,
    )>,
    mut events: MessageWriter<WeaponEvent>,
    clock: Res<SimClock>,
    first: Res<FirstTimePredicted>,
) {
    let now = clock.now;
    let due = now + SLACK;
    let dt = clock.dt() as f64;
    for (owner, intent, mut inv, mut sel, state, health) in &mut owners {
        if health.is_some_and(|h| h.current <= 0.0) {
            continue;
        }
        let Some(active) = inv.active else { continue };
        let Ok((item, mut mag, mut st, shells, knife)) = weapons.get_mut(active) else {
            continue;
        };
        let mut event = |kind| {
            events.write(WeaponEvent {
                owner,
                weapon: active,
                kind,
                replay: !first.0,
            })
        };
        // A reload due under sprint completes here (it plays and
        // completes under sprint; then the sprint pose).
        if state.sprinting
            && let Some(end) = st.reload_end
            && due >= end
            && let Some(m) = mag.as_mut()
        {
            let n = (m.size - m.clip).min(m.reserve);
            m.clip += n;
            m.reserve -= n;
            st.reload_end = None;
            st.next_primary = now;
            st.next_secondary = now;
            event(WeaponEventKind::Reloaded);
        }
        let reloading = st.reloading();
        let drawing = due < inv.next_attack && !reloading;
        let firing = match knife {
            Some(k) => k.busy(),
            None => due < st.next_primary,
        };
        if state.sprinting && !sel.sprint_pose && !drawing && !reloading && !firing {
            sel.sprint_pose = true;
        }
        if !state.sprinting && sel.sprint_pose {
            sel.sprint_pose = false;
            sel.sprint_out_until = now + item.map_or(0.0, |i| i.sprint_out) as f64;
        }
        let Some(m) = mag else { continue };
        let blocked_until = if sel.sprint_pose || intent.use_key {
            now + dt
        } else {
            sel.sprint_out_until
        };
        let held = blocked_until > due;
        if intent.reload && !reloading && !shells && (drawing || held) && start_reload(&mut inv, &mut st, &m, now) {
            event(WeaponEventKind::ReloadStarted);
        }
        if held && !st.reloading() {
            st.next_primary = st.next_primary.max(blocked_until);
            st.next_secondary = st.next_secondary.max(blocked_until);
        }
    }
}

// ---------------------------------------------------------------------------
// The knife (8)

/// Each knife in hand steps its attack (8.2) and makes its hit events
/// (8.3, 8.4). A knife put away loses what was left of its swing.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn swing_knives(
    owners: Query<
        (&Intent, &Inventory, &CaSelection, &Transform, &MovementState, Option<&Health>),
        Without<NetDrawn>,
    >,
    mut knives: Query<(Entity, &Weapon, &mut Knife)>,
    targets: Query<(Option<&Intent>, Has<Health>)>,
    colliders: Query<&ColliderOf>,
    spatial: SpatialQuery,
    mut damage: MessageWriter<Damage>,
    mut events: MessageWriter<WeaponEvent>,
    (clock, first, role): (Res<SimClock>, Res<FirstTimePredicted>, Option<Res<NetRole>>),
) {
    let now = clock.now;
    let due = now + SLACK;
    let server = role.as_deref().is_none_or(|r| *r != NetRole::Client);
    for (entity, weapon, mut knife) in &mut knives {
        let Some(owner) = weapon.owner else { continue };
        let Ok((intent, inv, sel, at, state, health)) = owners.get(owner) else {
            continue;
        };
        if inv.active != Some(entity) || health.is_some_and(|h| h.current <= 0.0) {
            knife.state = KnifeState::default();
            continue;
        }
        let ready = due >= inv.next_attack && !sel.sprint_pose && due >= sel.sprint_out_until && !state.sprinting;
        // 4.1: fire (not with use held) beats alt-fire.
        let request = if !ready {
            None
        } else if intent.fire && !intent.use_key {
            Some(0)
        } else if intent.secondary && knife.attacks.len() > 1 {
            Some(1)
        } else {
            None
        };
        // Animation time is exact, observed at updates: each animation
        // starts where the last one ended (Units and conventions).
        let rate = knife.rate.max(1e-3) as f64;
        let elapsed = |since: f64| (now - since) * rate + SLACK;
        let end_of = |since: f64, length: f32| since + length as f64 / rate;
        let st = knife.state;
        match st.phase {
            KnifePhase::Idle => {
                if let Some(a) = request {
                    knife.state = KnifeState {
                        phase: KnifePhase::Armed,
                        attack: a,
                        since: now,
                        hits: 0,
                    };
                }
            }
            KnifePhase::Armed => {
                knife.state.phase = KnifePhase::PreFire;
                knife.state.since = now;
            }
            KnifePhase::PreFire => {
                let prefire = knife.attacks[st.attack].prefire;
                if elapsed(st.since) >= prefire as f64 {
                    knife.state.phase = KnifePhase::Swing;
                    knife.state.since = end_of(st.since, prefire);
                    knife.state.hits = 0;
                }
            }
            KnifePhase::Swing => {
                let attack = knife.attacks[st.attack].clone();
                let t = elapsed(st.since);
                while knife.state.hits < attack.hits.len() && attack.hits[knife.state.hits] as f64 <= t {
                    let n = knife.state.hits;
                    knife.state.hits += 1;
                    let eye = at.translation + state.eye_offset;
                    let dir = crate::weapon::aim_of(intent.yaw, intent.pitch) * Vec3::NEG_Z;
                    let Ok(ray) = Dir3::new(dir) else { continue };
                    let filter = SpatialQueryFilter::from_excluded_entities([owner])
                        .with_mask(core::SOLID_LAYERS.0 & !core::ITEM_LAYER.0);
                    let hit = spatial.cast_ray(eye, ray, knife.range, true, &filter);
                    let end = hit.map_or(eye + dir * knife.range, |h| eye + dir * h.distance);
                    let body = hit.map(|h| colliders.get(h.entity).map_or(h.entity, |c| c.body));
                    events.write(WeaponEvent {
                        owner,
                        weapon: entity,
                        kind: WeaponEventKind::Swing {
                            hit: hit.is_some(),
                            secondary: st.attack == 1,
                            at: hit.zip(body).map(|(h, b)| (end, h.normal, b)),
                            line: (eye, end),
                        },
                        replay: !first.0,
                    });
                    let Some(target) = body else { continue };
                    let Ok((t_intent, alive)) = targets.get(target) else { continue };
                    if !alive || !server || !first.0 {
                        continue;
                    }
                    let mut roll = shared_random(intent.command, "combat_arms:knife", n as i32);
                    let u = roll.range(1.0 - knife.spread, 1.0 + knife.spread);
                    let rear = t_intent.is_some_and(|i| {
                        let yaw = i.yaw_rotation();
                        behind(yaw * Vec3::NEG_Z, yaw * Vec3::X, dir)
                    });
                    let b = if rear { 1.0 + BEHIND_BONUS } else { 1.0 };
                    damage.write(Damage {
                        target,
                        attacker: Some(owner),
                        amount: attack.damage * u * b,
                        point: end,
                        dir,
                        hitgroup: Hitgroup::Generic,
                        kind: DamageKind::Melee,
                        weapon: None,
                        force: Vec3::ZERO,
                    });
                }
                if t >= attack.length as f64 {
                    knife.state = match request {
                        // Held: the next swing at once, no pre-fire (8.1).
                        Some(a) => KnifeState {
                            phase: KnifePhase::Swing,
                            attack: a,
                            since: end_of(st.since, attack.length),
                            hits: 0,
                        },
                        None => KnifeState {
                            phase: KnifePhase::PostFire,
                            attack: st.attack,
                            since: end_of(st.since, attack.length),
                            hits: 0,
                        },
                    };
                }
            }
            KnifePhase::PostFire => {
                if elapsed(st.since) >= knife.attacks[st.attack].postfire as f64 {
                    knife.state = KnifeState::default();
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Pickup (11), drop (12) and respawn (13)

/// Where a ray from `origin` along `dir` enters the box `lo..hi`, if it
/// does (0 when it starts inside).
fn ray_box(origin: Vec3, dir: Vec3, lo: Vec3, hi: Vec3) -> Option<f32> {
    let inv = dir.recip();
    let a = (lo - origin) * inv;
    let b = (hi - origin) * inv;
    let near = a.min(b).max_element().max(0.0);
    let far = a.max(b).min_element();
    (far >= near).then_some(near)
}

/// The pickup target of every living character (11: the view ray within
/// `ACTIVATION_DISTANCE`, or the fallback), and E on it takes it.
#[allow(clippy::type_complexity)]
fn pickups(
    mut users: Query<(Entity, &Transform, &Intent, &MovementState, Option<&Health>, &mut CaSelection), Without<NetDrawn>>,
    items: Query<(Entity, &Transform, Option<&PhysicsProp>), With<Dropped>>,
    spatial: SpatialQuery,
    mut commands: Commands,
) {
    for (owner, at, intent, state, health, mut sel) in &mut users {
        let pressed = intent.use_key && !sel.prev_use;
        sel.prev_use = intent.use_key;
        if health.is_some_and(|h| h.current <= 0.0) {
            sel.pickup_target = None;
            continue;
        }
        let eye = at.translation + state.eye_offset;
        let dir = crate::weapon::aim_of(intent.yaw, intent.pitch) * Vec3::NEG_Z;
        let best = items
            .iter()
            .filter_map(|(e, t, body)| {
                let half = body.map_or(ITEM_HALF, |b| (b.bounds.1 - b.bounds.0) / 2.0);
                let hit = ray_box(eye, dir, t.translation - half, t.translation + half)?;
                let fallback = (t.translation - eye).length() - ACTIVATION_FALLBACK_SLACK <= ACTIVATION_DISTANCE;
                (hit <= ACTIVATION_DISTANCE || fallback).then_some((hit, e))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        // Something solid in front hides it.
        let target = best.and_then(|(hit, e)| {
            let filter = SpatialQueryFilter::from_excluded_entities([owner])
                .with_mask(core::SOLID_LAYERS.0 & !core::ITEM_LAYER.0);
            let ray = Dir3::new(dir).ok()?;
            match spatial.cast_ray(eye, ray, hit, true, &filter) {
                Some(h) if h.distance < hit => None,
                _ => Some(e),
            }
        });
        if sel.pickup_target != target {
            sel.pickup_target = target;
        }
        if pressed && let Some(item) = target {
            commands.queue(move |w: &mut World| {
                pick_up(w, owner, item);
            });
        }
    }
}

/// `owner` takes the dropped `item` (11). Returns whether it did.
pub fn pick_up(world: &mut World, owner: Entity, item: Entity) -> bool {
    let Some(d) = world.get::<Dropped>(item).copied() else { return false };
    if world.get::<Health>(owner).is_none_or(|h| h.current <= 0.0) {
        return false;
    }
    let Some((id, class)) = world.get::<Weapon>(d.weapon).map(|w| (w.id, class_of(w.slot))) else {
        return false;
    };
    let Some(inv) = world.get::<Inventory>(owner).cloned() else { return false };
    let Some(sel) = world.get::<CaSelection>(owner).cloned() else { return false };
    let in_class: Vec<Entity> = inv
        .weapons
        .iter()
        .copied()
        .filter(|w| world.get::<Weapon>(*w).is_some_and(|x| class_of(x.slot) == class))
        .collect();
    // A non-primary already carried: its ammo, and the clip refilled.
    if class != PRIMARY
        && let Some(mine) = in_class.iter().copied().find(|w| world.get::<Weapon>(*w).is_some_and(|x| x.id == id))
    {
        if let Some(mut m) = world.get_mut::<Magazine>(mine) {
            let total = (m.clip + m.reserve + d.total).min(m.size + m.reserve_max);
            m.clip = total.min(m.size);
            m.reserve = total - m.clip;
        } else if let Some(mut t) = world.get_mut::<Throwable>(mine) {
            t.count = (t.count + d.total).min(t.max);
        }
        despawn(world, item);
        despawn(world, d.weapon);
        return true;
    }
    // A swap within the class: the last chosen one goes.
    let replaced = (class != 0)
        .then(|| sel.last_chosen[class as usize])
        .flatten()
        .filter(|w| in_class.contains(w))
        .or_else(|| in_class.get(sel.cursor[class as usize % CLASSES]).copied())
        .or_else(|| in_class.first().copied());
    if replaced.is_none() && inv.weapons.len() >= MAX_CARRIED {
        return false;
    }
    let index = replaced.and_then(|r| drop_item(world, owner, r, true));
    despawn(world, item);
    // Its ammo: the item's, clamped; the clip with it, none at total 0.
    if let Some(mut m) = world.get_mut::<Magazine>(d.weapon) {
        let total = d.total.min(m.size + m.reserve_max);
        m.clip = if total == 0 { 0 } else { d.clip.min(total).min(m.size) };
        m.reserve = total - m.clip;
    } else if let Some(mut t) = world.get_mut::<Throwable>(d.weapon) {
        t.count = d.total.min(t.max);
    }
    if let Some(mut w) = world.get_mut::<Weapon>(d.weapon) {
        w.owner = Some(owner);
    }
    if let Some(mut st) = world.get_mut::<WeaponState>(d.weapon) {
        *st = WeaponState::default();
    }
    let Some(mut inv) = world.get_mut::<Inventory>(owner) else { return false };
    match index {
        Some(i) => {
            let i = i.min(inv.weapons.len());
            inv.weapons.insert(i, d.weapon);
        }
        None => inv.weapons.push(d.weapon),
    }
    // Drawn at once (the auto weapon switch option's default).
    inv.wanted = Some(d.weapon);
    world.write_message(crate::weapon::drop::PickedUp {
        who: owner,
        weapon: d.weapon,
    });
    true
}

fn despawn(world: &mut World, e: Entity) {
    if let Ok(e) = world.get_entity_mut(e) {
        e.despawn();
    }
}

/// Take `weapon` out of `owner`'s list and lay it at the left foot with a
/// random yaw (0 to 1 rad), jittered by +-half the hull's width on X and
/// Z for a swap (11, 12). Returns its equip index.
pub fn drop_item(world: &mut World, owner: Entity, weapon: Entity, jitter: bool) -> Option<usize> {
    let now = world.get_resource::<SimClock>().map_or(0.0, |c| c.now);
    let at = *world.get::<Transform>(owner)?;
    let intent = world.get::<Intent>(owner).cloned().unwrap_or_default();
    let (lo, hi) = world
        .get::<MovementState>(owner)
        .filter(|s| s.hull_max != s.hull_min)
        .map_or((Vec3::new(-0.4, -0.9, -0.4), Vec3::new(0.4, 0.9, 0.4)), |s| (s.hull_min, s.hull_max));
    let index = {
        let mut inv = world.get_mut::<Inventory>(owner)?;
        let i = inv.weapons.iter().position(|w| *w == weapon)?;
        inv.weapons.remove(i);
        if inv.active == Some(weapon) {
            inv.active = None;
        }
        if inv.wanted == Some(weapon) {
            inv.wanted = None;
        }
        i
    };
    // Put away: whatever it was doing is lost.
    if let Some(mut st) = world.get_mut::<WeaponState>(weapon) {
        *st = WeaponState::default();
    }
    let id = {
        let mut w = world.get_mut::<Weapon>(weapon)?;
        w.owner = None;
        w.id
    };
    let (total, clip) = match (world.get::<Magazine>(weapon), world.get::<Throwable>(weapon)) {
        (Some(m), _) => (m.clip + m.reserve, m.clip),
        (None, Some(t)) => (t.count, 0),
        _ => (INFINITE_AMMO, 0),
    };
    let mut rng: Rng = shared_random(intent.command, "combat_arms:drop", index as i32);
    let width = hi.x - lo.x;
    let yaw_rot = intent.yaw_rotation();
    let left = yaw_rot * Vec3::NEG_X;
    let mut spot = at.translation + Vec3::Y * lo.y + left * (width / 4.0);
    if jitter {
        spot.x += rng.range(-0.5, 0.5) * width;
        spot.z += rng.range(-0.5, 0.5) * width;
    }
    let yaw = rng.range(0.0, 1.0);
    world.spawn((
        Name::new(format!("Dropped {id}")),
        LooseItem(id.to_string()),
        Dropped {
            weapon,
            since: now,
            total,
            clip,
        },
        Transform::from_translation(spot).with_rotation(Quat::from_rotation_y(yaw)),
    ));
    Some(index)
}

/// A weapon item lying at `at` (a map's, a test's): a new `id` weapon
/// with `total` rounds, `clip` of them in the clip.
pub fn spawn_item(world: &mut World, id: &str, at: Vec3, total: u32, clip: u32) -> Option<Entity> {
    let weapon = crate::weapon::spawn_weapon(world, id, Entity::PLACEHOLDER)?;
    let id = world.get::<Weapon>(weapon)?.id;
    if let Some(mut w) = world.get_mut::<Weapon>(weapon) {
        w.owner = None;
    }
    let now = world.get_resource::<SimClock>().map_or(0.0, |c| c.now);
    Some(
        world
            .spawn((
                Name::new(format!("Dropped {id}")),
                LooseItem(id.to_string()),
                Dropped {
                    weapon,
                    since: now,
                    total,
                    clip,
                },
                Transform::from_translation(at),
            ))
            .id(),
    )
}

/// The dead drop their last chosen primary where they fell (12).
fn death_drops(mut died: MessageReader<Died>, allow: Res<CaDeathDrops>, mut commands: Commands) {
    for d in died.read() {
        let owner = d.entity;
        let allow = allow.0;
        commands.queue(move |w: &mut World| death_drop(w, owner, allow));
    }
}

fn death_drop(world: &mut World, owner: Entity, allow: bool) {
    let Some(sel) = world.get::<CaSelection>(owner).cloned() else { return };
    // The weapon in hand is put away.
    let held = world.get_mut::<Inventory>(owner).and_then(|mut inv| {
        inv.wanted = None;
        inv.active.take()
    });
    if let Some(h) = held
        && let Some(mut st) = world.get_mut::<WeaponState>(h)
    {
        *st = WeaponState::default();
    }
    if !allow {
        return;
    }
    let carried = world.get::<Inventory>(owner).map(|i| i.weapons.clone()).unwrap_or_default();
    let Some(primary) = sel.last_chosen[PRIMARY as usize].filter(|w| carried.contains(w)) else {
        return;
    };
    let droppable = world.get::<Weapon>(primary).is_some_and(|w| class_of(w.slot) == PRIMARY)
        && world.get::<CaItem>(primary).is_none_or(|i| i.drop_on_death);
    if droppable {
        drop_item(world, owner, primary, false);
    }
}

/// Dropped items go after `DROPPED_LIFETIME` (12).
fn expire_items(items: Query<(Entity, &Dropped)>, weapons: Query<&Weapon>, clock: Res<SimClock>, mut commands: Commands) {
    for (e, d) in &items {
        if clock.now - d.since + SLACK >= DROPPED_LIFETIME {
            commands.entity(e).despawn();
            if weapons.get(d.weapon).is_ok_and(|w| w.owner.is_none()) {
                commands.entity(d.weapon).despawn();
            }
        }
    }
}

/// Respawn (13): clear what `player` carries and its selection; the
/// loadout is given again on the next tick (`StartingWeapons`), with full
/// clips, cursors at 0, no previous weapon and the primary drawn.
pub fn respawn_inventory(world: &mut World, player: Entity) {
    let weapons = world.get::<Inventory>(player).map(|i| i.weapons.clone()).unwrap_or_default();
    for w in weapons {
        despawn(world, w);
    }
    if let Ok(mut e) = world.get_entity_mut(player) {
        e.remove::<(Inventory, CaSelection)>();
    }
}
