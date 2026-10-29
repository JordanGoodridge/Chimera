//! Each install weapon's own fire cycle, reload and draw from its own
//! view model's animations (docs/plans/active/ca-weapon-timings.md;
//! weapon_firing.md "Trigger and fire cycle" and "Reload",
//! weapon_classes.md "Typical weapons" and 5.5, view_models.md 7): the
//! cycle is `fire_0`'s length, the magazine reload `reload`'s, a shell
//! reload `PreReload`, `reload` per shell and `PostReload`, the draw
//! `select`'s, all at rate 1, each found by name in the model's combined
//! list (its own animations, then its children's: `model::load`).
//!
//! The `combat_arms:timing` hook (written by `data`'s merge on the
//! install's copies, never on our documents: their numbers are the
//! spec's) reads the weapon's `[models] view` when the weapon is built,
//! on the server and the client alike (both read the same install, so
//! both fire, reload and draw on the same timing and prediction holds),
//! once per model file per world (`ViewTimes`, failures too). A
//! model that doesn't read, or an animation it lacks, keeps the
//! template's number. The rates on top are the other modules': the cycle
//! is divided by `attachments::fire_animation_rate` (the record's
//! `FireAniRate`, `CaItem::fire_rate`, + the parts'), the draw by the
//! item's select rate; with an attachments table the lengths become its
//! `Stock` (the parts' rates applied there). The hook runs after the
//! attachments, damage and inventory hooks (hooks build in name order),
//! so it rewrites what they took from the template: the stock numbers
//! and the server's reload window (`CaDamage::reload_third_person`, when
//! it was the template's reload).
//!
//! Known shortcuts (docs/tech-debt.md):
//! - The first build of a weapon reads its view model on the frame it is
//!   built (the gun game's next weapon; about 2 ms, at most ~18 ms: the plan's
//!   measurement); nothing reads ahead.
//! - A side that can't read the model (no install, or no key for its
//!   archive) keeps the template's timing while the other side has the
//!   model's: the client then mispredicts. The timing isn't sent.
//! - Not taken from the model yet: a semi-automatic's `postfire` (our
//!   trigger has one cycle), the pre-fire's presence (`trigger.latency`
//!   stays the template's), a minigun's spin lengths (`combat_arms:spin`)
//!   and the knives' and grenades' timings (their own hooks; the merge
//!   writes no timing table for them).

use std::collections::BTreeMap;

use bevy::prelude::*;
use lithtech_formats::anim::AnimList;
use serde::Deserialize;

use super::{
    super::{
        attachments::{CaAttachSlots, animation_rate, fire_animation_rate},
        inventory::CaItem,
        model,
    },
    CaDamage,
};
use crate::weapon::{Magazine, ShellReload, Trigger, Weapon, data::WeaponData};

/// The hook's name: after `combat_arms:attachments`, `:damage` and
/// `:inventory` in name order, so their components are there.
pub const TIMING: &str = "combat_arms:timing";

/// The hook's parameters: none (the model is the document's `[models]
/// view`).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimingParams {}

/// A view model's action lengths at rate 1, s (None: the model has no
/// such animation, or it is 0 ms long).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AnimTimes {
    pub fire: Option<f32>,
    pub reload: Option<f32>,
    pub pre_reload: Option<f32>,
    pub post_reload: Option<f32>,
    pub select: Option<f32>,
}

impl AnimTimes {
    /// The lengths in a model's combined animation list (first match by
    /// name, case ignored: view_models.md 7).
    pub fn of_list(list: &AnimList) -> AnimTimes {
        let len = |name: &str| {
            list.find(name)
                .map(|i| list.entries[i].length())
                .filter(|&ms| ms > 0)
                .map(|ms| ms as f32 / 1000.0)
        };
        AnimTimes {
            fire: len("fire_0"),
            reload: len("reload"),
            pre_reload: len("PreReload"),
            post_reload: len("PostReload"),
            select: len("select"),
        }
    }
}

/// The view models' lengths this world has read, by archive path
/// (normalized; None: it didn't read). The weapons plugin inserts it;
/// tests set a model's lengths in it (`set`).
#[derive(Resource, Clone, Debug, Default)]
pub struct ViewTimes(pub BTreeMap<String, Option<AnimTimes>>);

impl ViewTimes {
    /// Model `path`'s lengths, as if read.
    pub fn set(&mut self, path: &str, times: AnimTimes) {
        self.0.insert(model::normalize(path), Some(times));
    }
}

/// View model `path`'s lengths, read from the install (`world::mount`);
/// None when it doesn't read.
pub fn read(path: &str) -> Option<AnimTimes> {
    let key = model::normalize(path);
    let read = super::super::world::mount()
        .ok_or_else(|| "no Combat Arms install".to_string())
        .and_then(|m| model::load(&key, &|p: &str| m.read(p).ok()));
    match read {
        Ok(l) => Some(AnimTimes::of_list(&l.list)),
        Err(e) => {
            warn!("Combat Arms: {key}: {e} (the template's timing stands)");
            None
        }
    }
}

/// The hook: weapon `d`'s parts take its view model's lengths.
pub fn apply(e: &mut EntityWorldMut, d: &'static WeaponData, _: &TimingParams) {
    let Some(path) = d.models.view.as_deref().map(model::normalize) else { return };
    let cached = e.get_resource_mut::<ViewTimes>().and_then(|c| c.0.get(&path).copied());
    let times = cached.unwrap_or_else(|| {
        let t = read(&path);
        if let Some(mut c) = e.get_resource_mut::<ViewTimes>() {
            c.0.insert(path, t);
        }
        t
    });
    if let Some(t) = times {
        apply_times(e, &t);
    }
}

/// Parts take lengths `t` (rate 1; the rates applied as the module doc
/// says).
pub fn apply_times(e: &mut EntityWorldMut, t: &AnimTimes) {
    let (fire_rate, select_rate) = e.get::<CaItem>().map_or((0.0, 0.0), |i| (i.fire_rate, i.select_rate));
    let old_reload = e.get::<Magazine>().map(|m| m.reload_time);
    if let Some(mut s) = e.get_mut::<CaAttachSlots>() {
        if let Some(f) = t.fire {
            s.stock.cycle = f;
        }
        if let Some(r) = t.reload {
            s.stock.reload = r;
        }
        if let Some(sel) = t.select {
            s.stock.draw = sel;
        }
    }
    if let (Some(f), Some(mut tr)) = (t.fire, e.get_mut::<Trigger>()) {
        tr.cycle = f / fire_animation_rate(fire_rate, 0.0);
    }
    if let (Some(r), Some(mut m)) = (t.reload, e.get_mut::<Magazine>()) {
        m.reload_time = r;
    }
    if let Some(mut s) = e.get_mut::<ShellReload>() {
        if let Some(p) = t.pre_reload {
            s.start = p;
        }
        if let Some(r) = t.reload {
            s.insert = r;
        }
        if let Some(p) = t.post_reload {
            s.finish = p;
        }
    }
    if let (Some(sel), Some(mut w)) = (t.select, e.get_mut::<Weapon>()) {
        w.draw_time = sel / animation_rate(1.0 + select_rate);
    }
    // The server's reload window, when it was the template's reload.
    if let (Some(r), Some(old), Some(mut dmg)) = (t.reload, old_reload, e.get_mut::<CaDamage>())
        && dmg.reload_third_person == old
    {
        dmg.reload_third_person = r;
    }
}
