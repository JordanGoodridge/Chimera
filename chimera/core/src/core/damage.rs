use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::character_state::{God, Health, Noclip};
use super::hitbox::{BodyRegion, Hitgroup};
use super::map_controls::MapControls;
use super::Team;

/// Damage to apply: normalized like `Health` (1.0 = a standard player's
/// full health).
#[derive(Message, Clone, Debug)]
pub struct Damage {
    pub target: Entity,
    pub attacker: Option<Entity>,
    pub amount: f32,
    pub point: Vec3,
    /// Direction the damage travelled (unit).
    pub dir: Vec3,
    pub hitgroup: Hitgroup,
    /// How it was dealt (breakables scale damage by it).
    pub kind: DamageKind,
    /// The weapon that dealt it when that isn't what the attacker holds
    /// now (a thrown grenade), for kill notices; None: the held one.
    pub weapon: Option<&'static str>,
    /// The push that came with it (kg·m/s, engine space), already given
    /// to a moving body; zero for none. Props read it (a frozen prop
    /// that a strong enough push unfreezes).
    pub force: Vec3,
}

/// Damage a map deals a player (trigger_hurt) before armour and the
/// player's whole points: the weapon layer splits it with the armour worn
/// when `armour` (CS:S takes armour for every damage type but falls and
/// drowning: measured on trigger_hurt, specs/source/triggers.md question
/// 4), then health takes whole points (2.5 is 2, nothing carried). Without
/// a weapon layer (`MapDamageArmor` absent) only the truncation applies
/// (`map_damage_lands`).
#[derive(Message, Clone, Debug)]
pub struct MapDamage {
    pub damage: Damage,
    pub armour: bool,
}

/// The weapon layer turns `MapDamage` into `Damage` itself (with armour).
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct MapDamageArmor;

/// `amount` (normalized: 1.0 is 100 points) down to whole points.
pub fn whole_points(amount: f32) -> f32 {
    (amount * 100.0 + 1e-3).floor() / 100.0
}

/// `MapDamage` without a weapon layer: whole points, no armour.
pub fn map_damage_lands(mut map: MessageReader<MapDamage>, mut damage: MessageWriter<Damage>) {
    for m in map.read() {
        let mut d = m.damage.clone();
        d.amount = whole_points(d.amount);
        if d.amount > 0.0 {
            damage.write(d);
        }
    }
}

impl Damage {
    /// A suicide (the `kill` command: generic damage from the target
    /// itself), which CS:S carries out even in noclip (`Noclip`).
    pub fn is_suicide(&self) -> bool {
        self.attacker == Some(self.target) && self.kind == DamageKind::Generic && self.weapon.is_none()
    }
}

/// An explosion to apply (radius damage and pushes, as a grenade's blast
/// with this magnitude and radius): from map logic (exploding props).
/// `damage` is normalized like `Damage::amount`, `radius` in meters.
#[derive(Message, Clone, Debug)]
pub struct Explosion {
    pub origin: Vec3,
    pub damage: f32,
    pub radius: f32,
    /// Who the damage counts for (kill notices, friendly fire).
    pub attacker: Option<Entity>,
    /// What exploded (it takes none of the damage).
    pub inflictor: Option<Entity>,
    /// A sound entry to play at the origin, if any.
    pub sound: Option<String>,
    /// The weapon kills count as (kill notices); None: a map explosion
    /// (`weapon::grenade::EXPLOSION_WEAPON`).
    pub weapon: Option<&'static str>,
    /// Its scorch mark goes at the next round restart (the bomb's, as in
    /// CS:S; `map::PlaceDecal::round`); others stay with the map.
    pub round_scorch: bool,
    /// Falloff override: a bell of this σ (m) instead of the explosion
    /// rule's (`weapon::grenade::Blast::bell_sigma`); None: the rule's,
    /// its σ scaled with this radius.
    pub bell_sigma: Option<f32>,
    /// Armour override (`Blast::armor_ratio`); None: the rule's.
    pub armor_ratio: Option<f32>,
    /// Nothing between stops or shields it (`Blast::through_walls`).
    pub through_walls: bool,
}

/// How damage was dealt (Source damage types, as far as anything here
/// tells them apart).
#[derive(Reflect, Default, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DamageKind {
    #[default]
    Generic,
    /// Hitscan shots.
    Bullet,
    /// Melee swings (the knife).
    Melee,
    Blast,
    /// Crushed by a mover.
    Crush,
    Fall,
    /// Fire (Source's burn type): env_fire and entity flames.
    Burn,
    /// Drowning: head under water too long (`games::cs_source::drown`).
    Drown,
}

/// Damage kinds a character takes none of (a map's damage filter on it:
/// bhop and surf maps turn fall damage off this way).
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct DamageFilter {
    pub blocked: Vec<DamageKind>,
}

/// Takes `Damage` without having `Health`: something else (the logic
/// layer's breakables) reads the messages aimed at it. Weapons hit it as
/// a plain object (no hitgroups, no flesh sounds).
#[derive(Component, Reflect, Default, Clone, Copy, Debug)]
#[reflect(Component)]
pub struct Damageable;

/// Damage that reached a character's health (after the rules: not
/// refused, not under `God`): for the victim's HUD (the damage
/// indicators). A network client gets its own player's from the server
/// (`net::Hurt`), with `damage.attacker` None and `directed` saying
/// whether someone else dealt it.
#[derive(Message, Clone, Debug)]
pub struct DamageTaken {
    pub damage: Damage,
    /// Dealt by someone other than the victim (a shot, a grenade), so it
    /// has a side to come from.
    pub directed: bool,
}

/// Where on a body a gun's hit landed, in full (`BodyRegion`; its
/// `Damage` carries only the coarse `Hitgroup`, so a groin hit reads as
/// the stomach there): written by a gun's hit beside its `Damage`, on the
/// server, for what tells regions apart after the fact (the announcer's
/// headshot and groin-shot callouts, `chimera_sim::announce`).
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct RegionHit {
    pub target: Entity,
    pub region: BodyRegion,
}

/// Something's health reached zero.
#[derive(Message, Clone, Debug)]
pub struct Died {
    pub entity: Entity,
    pub attacker: Option<Entity>,
    /// The hit that killed it (direction and point push its ragdoll).
    pub damage: Damage,
}

/// Whether characters on the same team hurt each other (CS:S
/// `mp_friendlyfire`, default 0). Team 0 (no team) is never friendly.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FriendlyFire(pub u8);

/// Whether team rules refuse this damage: teammates don't hurt each other
/// unless friendly fire is on (your own damage, e.g. falling, always
/// counts).
pub fn refused_by_team(d: &Damage, teams: &Query<&Team>, friendly_fire: Option<&FriendlyFire>) -> bool {
    let teammate = d
        .attacker
        .filter(|a| *a != d.target)
        .is_some_and(|a| matches!((teams.get(a), teams.get(d.target)), (Ok(x), Ok(y)) if x == y && x.0 != 0));
    teammate && !friendly_fire.is_some_and(|f| f.0 != 0)
}

/// Why damage is refused (`DamageRules::refusal`). What a refused hit
/// still does depends on why (CS:S, measured 2026-10-17;
/// specs/cs_source/weapons.md "Refused hits"): a teammate's hit with
/// friendly fire off does nothing at all (no armour, sound, blood,
/// slowdown or flinch); a noclipping player's hit still bleeds and
/// flinches, nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The target noclips (`Noclip`).
    Noclip,
    /// A teammate's, friendly fire off (`refused_by_team`).
    Team,
    /// The map made the target invulnerable (`MapControls`).
    Invulnerable,
    /// The target's damage filter blocks this kind (`DamageFilter`).
    Filter,
}

/// What decides whether damage is refused before it reaches health:
/// team rules, a map's invulnerability and damage filters, a noclipping
/// target (`Noclip`); and whether it lands (`lands`: not refused, and not
/// on a `God` target, which `apply_damage`'s health query leaves out).
#[derive(bevy::ecs::system::SystemParam)]
pub struct DamageRules<'w, 's> {
    noclip: Query<'w, 's, (), With<Noclip>>,
    god: Query<'w, 's, (), With<God>>,
    teams: Query<'w, 's, &'static Team>,
    filters: Query<'w, 's, &'static DamageFilter>,
    controls: Query<'w, 's, &'static MapControls>,
    friendly_fire: Option<Res<'w, FriendlyFire>>,
}

impl DamageRules<'_, '_> {
    pub fn refuse(&self, d: &Damage) -> bool {
        self.refusal(d).is_some()
    }

    /// Why `d` is refused, if it is.
    pub fn refusal(&self, d: &Damage) -> Option<Refusal> {
        if self.noclip.contains(d.target) && !d.is_suicide() {
            Some(Refusal::Noclip)
        } else if refused_by_team(d, &self.teams, self.friendly_fire.as_deref()) {
            Some(Refusal::Team)
        } else if self.controls.get(d.target).is_ok_and(|c| c.invulnerable) {
            Some(Refusal::Invulnerable)
        } else if self.filters.get(d.target).is_ok_and(|f| f.blocked.contains(&d.kind)) {
            Some(Refusal::Filter)
        } else {
            None
        }
    }

    /// Whether `d` is taken: not refused, the target not under `God`.
    /// Only such hits cost armour (`weapon::HitArmor`), make hurt sounds
    /// and slow the victim down.
    pub fn lands(&self, d: &Damage) -> bool {
        !self.god.contains(d.target) && !self.refuse(d)
    }
}

/// Subtract damage from health; announce deaths once (public so other
/// systems can order themselves after it).
pub fn apply_damage(
    mut damage: MessageReader<Damage>,
    mut health: Query<&mut Health, Without<God>>,
    rules: DamageRules,
    mut died: MessageWriter<Died>,
    mut taken: MessageWriter<DamageTaken>,
) {
    for d in damage.read() {
        if rules.refuse(d) {
            continue;
        }
        let Ok(mut h) = health.get_mut(d.target) else { continue };
        if h.current <= 0.0 {
            continue;
        }
        h.current -= d.amount;
        taken.write(DamageTaken {
            damage: d.clone(),
            directed: d.attacker.is_some_and(|a| a != d.target),
        });
        if h.current <= 0.0 {
            h.current = 0.0;
            died.write(Died {
                entity: d.target,
                attacker: d.attacker,
                damage: d.clone(),
            });
        }
    }
}
