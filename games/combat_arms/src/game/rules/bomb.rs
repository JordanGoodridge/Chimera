//! The mission bomb (specs/combat_arms/game_modes.md M3, M9): planting,
//! defusing and the detonation, shared by Search & Destroy and Bombing
//! Run, on the map's bomb sites (`objects::BombZone`).
//!
//! - Plant (Open question 4, answered): a player holding the bomb
//!   (`objectives::BOMB`, in hand) and fire, inside a site's box widened
//!   by `plant_margin` (50 u) on each axis, of a site its own team doesn't
//!   defend (`DefusableTeam` is the other team), while the round counts
//!   and is undecided and no bomb was planted this round, plays the bomb's
//!   fire animation up to its fire key (`bomb_plant`, 3.904 s of XK-2's
//!   `fire_0`; `objectives::CaAction`, the HUD's 4 s gauge) with its
//!   movement locked (`flow::hold_frozen`: turning still works); letting
//!   go of fire, leaving the box or putting the bomb away cancels it and
//!   keeps nothing. At the key: the bomb is planted
//!   where the planter stands, its fuse the site's `DetonateTime` (40 s),
//!   the round timer gets the fuse added to what is left, the planter's
//!   bomb is gone, its plants +1. Only one bomb a round.
//! - Defuse: a living player of the site's defending team inside the
//!   planted site's box (no margin) holding use fills the defuse gauge
//!   (the site's `DefuseTime`, 7 s); letting go cancels it (progress
//!   lost). When it fills while the round counts and is undecided: the
//!   defenders win the round, the defuser's defuses +1, the bomb is gone.
//! - Detonation (Open question 17, answered): at the fuse's end, while the
//!   round counts and is undecided, the planting team wins and the
//!   planter's detonations +1; the bomb is gone either way. Nobody is hurt
//!   (S15: the stock sites' commands only win the round and play the
//!   explosion show): `CaObjectives::detonation` says where and when, and
//!   every machine plays the show's effects (`effects::bomb_show`). A
//!   partial defuse keeps nothing (every attempt the full `DefuseTime`).
//!
//! - The planter's credit (Corrections 2026-10-22, 13): S&D keeps no plant
//!   statistic (Bombing Run counts plants); the planter is credited one
//!   detonation when the bomb explodes, and also when the round is
//!   decided while the bomb still ticks (`credit_planter`, the mode's
//!   `round_over`); a planter who left is not credited.
//!
//! Known shortcuts: the sites' `PlantedCommand`, `DetonatedCommand` and
//! the other command strings aren't run (no map commands yet: the other
//! site closes by the one-bomb rule, the show is ours from S15's
//! timings); the map camera (`Camera0`) doesn't take the views during
//! the show; the defuse is the use key inside the site's box (no look or
//! use-range test).

use bevy::{
    ecs::{entity::Entities, system::SystemParam},
    prelude::*,
};

use super::{
    CaRuleSettings,
    objectives::{self, ActionKind, BOMB, CaAction, CaCarry, CaObjectives, Edits, Planted, Roster},
    objects::CaMapObjects,
    scoring::stats,
    state::{CaMatch, CaStats},
};
use crate::{
    core::SimClock,
    weapon::{Inventory, Weapon},
};

/// Who planted this round's bomb (the server's; credited at the
/// detonation).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct CaPlanter(pub Option<Entity>);

/// The sites of `family` (S&D's `Doobic`, Bombing Run's `Sabotage`); every
/// site when the map has none of that family.
pub fn zones(world: &World, family: &str) -> Vec<(usize, super::objects::BombZone)> {
    zones_in(world.resource::<CaMapObjects>(), family)
}

/// `zones` of the map's objects.
pub fn zones_in(objects: &CaMapObjects, family: &str) -> Vec<(usize, super::objects::BombZone)> {
    let all = &objects.bomb_zones;
    let mine: Vec<_> = all.iter().cloned().enumerate().filter(|(_, z)| z.family == family).collect();
    if mine.is_empty() {
        all.iter().cloned().enumerate().collect()
    } else {
        mine
    }
}

/// The round counts and is undecided.
fn open(m: &CaMatch) -> bool {
    m.counting() && m.winner.is_none()
}

/// What a tick of the bomb reads and writes (a mode's per-tick system's:
/// S&D's, Bombing Run's `bombs`).
#[derive(SystemParam)]
pub struct Bombs<'w, 's> {
    clock: Res<'w, SimClock>,
    settings: Res<'w, CaRuleSettings>,
    objects: Res<'w, CaMapObjects>,
    objectives: ResMut<'w, CaObjectives>,
    m: ResMut<'w, CaMatch>,
    planter: ResMut<'w, CaPlanter>,
    entities: &'w Entities,
    roster: Roster<'w, 's>,
    inventories: Query<'w, 's, &'static mut Inventory>,
    weapons: Query<'w, 's, &'static Weapon>,
    actions: Query<'w, 's, &'static mut CaAction>,
    carries: Query<'w, 's, &'static mut CaCarry>,
    stats: Query<'w, 's, &'static mut CaStats>,
    commands: Commands<'w, 's>,
}

impl Bombs<'_, '_> {
    /// One tick of planting, defusing and the fuse on `family`'s sites.
    pub fn tick(&mut self, family: &str) {
        let mut gauges = Edits::default();
        let mut carry = Edits::default();
        self.tick_with(family, &mut gauges, &mut carry);
        gauges.apply(&mut self.actions, &mut self.commands);
        carry.apply(&mut self.carries, &mut self.commands);
    }

    fn tick_with(&mut self, family: &str, gauges: &mut Edits<CaAction>, carry: &mut Edits<CaCarry>) {
        let now = self.clock.now;
        let dt = self.clock.dt() as f32;
        let o = self.settings.objectives.clone();
        let sites = zones_in(&self.objects, family);
        let players = self.roster.takers();
        let planted = self.objectives.planted;
        // Plants.
        for p in &players {
            let site = sites.iter().find(|(_, z)| {
                z.defusers != p.team && z.area.contains(p.at, objectives::units(o.plant_margin))
            });
            let on = planted.is_none()
                && open(&self.m)
                && p.intent.fire
                && site.is_some()
                && objectives::holding_in(self.inventories.get(p.e).ok(), &self.weapons, BOMB);
            if objectives::hold_action_in(gauges, &self.actions, dt, p.e, ActionKind::Plant, o.bomb_plant, on)
                && let Some((index, z)) = site
            {
                let fuse = if z.detonate_time > 0.0 { z.detonate_time } else { o.bomb_fuse };
                self.objectives.planted = Some(Planted {
                    zone: *index,
                    number: z.number,
                    at: p.at,
                    team: p.team,
                    detonates: now + fuse as f64,
                });
                if let Some(ends) = &mut self.m.round_ends {
                    *ends += fuse as f64;
                }
                self.planter.0 = Some(p.e);
                objectives::take_weapon_in(
                    self.inventories.get_mut(p.e).ok().as_deref_mut(),
                    &self.weapons,
                    &mut self.commands,
                    BOMB,
                );
                let mut c = objectives::carry_in(carry, &self.carries, p.e);
                c.bomb = false;
                objectives::set_carry_in(carry, p.e, c);
                // S&D keeps no plant statistic (Corrections 2026-10-22, 13);
                // Bombing Run counts plants.
                if family == super::bombing_run::FAMILY {
                    objectives::add_stats(&mut self.stats, &mut self.commands, p.e, |s| s.plants += 1);
                }
                info!("bomb planted at site {} ({:?})", z.number, p.e);
                return;
            }
        }
        let Some(bomb) = planted else { return };
        // Defuses.
        let Some(z) = self.objects.bomb_zones.get(bomb.zone).cloned() else {
            self.objectives.planted = None;
            return;
        };
        let length = if z.defuse_time > 0.0 { z.defuse_time } else { o.bomb_defuse };
        for p in &players {
            let on = z.can_defuse && p.team == z.defusers && p.intent.use_key && z.area.contains(p.at, 0.0);
            if objectives::hold_action_in(gauges, &self.actions, dt, p.e, ActionKind::Defuse, length, on) {
                self.objectives.planted = None;
                self.objectives.defused = Some(bomb.at);
                if open(&self.m) {
                    objectives::add_stats(&mut self.stats, &mut self.commands, p.e, |s| s.defuses += 1);
                    self.m.win(z.defusers);
                    info!("bomb defused by {:?}", p.e);
                }
                return;
            }
        }
        // The fuse.
        if now + 1e-9 >= bomb.detonates {
            self.objectives.planted = None;
            self.objectives.detonation = Some((z.area.centre, now));
            if open(&self.m) {
                if let Some(planter) = self.planter.0
                    && self.entities.contains(planter)
                {
                    objectives::add_stats(&mut self.stats, &mut self.commands, planter, |s| s.detonations += 1);
                }
                self.m.win(bomb.team);
                info!("bomb detonated");
            }
        }
    }
}

/// A real round decided while the bomb still ticks (the defenders
/// eliminated, the attackers' last player leaving): the planter is
/// credited the detonation at the round result as when it exploded
/// (Corrections 2026-10-22, 13); not if they left, never in the warm-up.
/// The mode's `round_over`.
pub fn credit_planter(world: &mut World) {
    if world.resource::<CaMatch>().warmup || !ticking(world) {
        return;
    }
    if let Some(planter) = world.resource::<CaPlanter>().0
        && world.get_entity(planter).is_ok()
    {
        stats(world, planter).detonations += 1;
    }
}

/// A bomb is ticking.
pub fn ticking(world: &World) -> bool {
    world.resource::<CaObjectives>().planted.is_some()
}

/// The round's end: every bomb taken from its holder, the planted one
/// gone.
pub fn remove_all(world: &mut World) {
    let holders: Vec<Entity> = super::flow::players(world).into_iter().map(|(e, _)| e).collect();
    for e in holders {
        objectives::take_weapon(world, e, BOMB);
    }
    let mut o = world.resource_mut::<CaObjectives>();
    o.planted = None;
    o.loose_bomb = None;
    o.detonation = None;
    o.defused = None;
    world.resource_mut::<CaPlanter>().0 = None;
}
