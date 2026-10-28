//! The opt-in swap effect (`client::swap_effect`): off by default
//! (`party_swap_effect 0`), nothing on a swap; on, the local player's
//! identity changing flashes once per swap, another character's gets one
//! puff, and a first identity, an unchanged one or a dead character's
//! shows nothing.

use bevy::prelude::*;
use chimera_client::client::swap_effect::{FLASH_SECONDS, SwapEffectPlugin, SwapEffectSetting, SwapEffectView};
use chimera_core::{
    core::{Dead, LocalPlayer},
    slots::GameIdentity,
};
use chimera_sim::SimBasePlugins;
use chimera_world::{greybox::GreyboxMapPlugin, harness::Sim};

fn world(on: bool) -> (Sim, Entity, Entity) {
    let mut sim = Sim::with(SimBasePlugins, |app| {
        app.add_plugins((GreyboxMapPlugin, SwapEffectPlugin));
    });
    if on {
        sim.app.world_mut().resource_mut::<SwapEffectSetting>().on = 1;
    }
    let w = sim.app.world_mut();
    let me = w.spawn((LocalPlayer, GameIdentity("cs_source".into()), Transform::default(), GlobalTransform::default())).id();
    let other = w.spawn((GameIdentity("cs_source".into()), Transform::default(), GlobalTransform::default())).id();
    sim.ticks(2);
    (sim, me, other)
}

fn set(sim: &mut Sim, e: Entity, game: &str) {
    sim.app.world_mut().entity_mut(e).insert(GameIdentity(game.into()));
    sim.ticks(1);
}

fn view(sim: &Sim) -> SwapEffectView {
    *sim.app.world().resource::<SwapEffectView>()
}

#[test]
fn off_by_default_nothing_shows() {
    let (mut sim, me, other) = world(false);
    assert_eq!(sim.app.world().resource::<SwapEffectSetting>().on, 0);
    set(&mut sim, me, "combat_arms");
    set(&mut sim, other, "combat_arms");
    let v = view(&sim);
    assert_eq!((v.swaps, v.puffs), (0, 0));
    assert_eq!(v.flash, 0.0);
}

#[test]
fn on_once_per_swap() {
    let (mut sim, me, other) = world(true);
    let v = view(&sim);
    assert_eq!((v.swaps, v.puffs), (0, 0), "a first identity is no swap");
    set(&mut sim, me, "combat_arms");
    let v = view(&sim);
    assert_eq!(v.swaps, 1);
    assert!(v.flash > 0.0 && v.flash <= FLASH_SECONDS, "flashing: {}", v.flash);
    // The same identity again (a replicated rewrite) is no swap.
    set(&mut sim, me, "combat_arms");
    assert_eq!(view(&sim).swaps, 1);
    // The flash fades.
    sim.seconds(f64::from(FLASH_SECONDS) + 0.1);
    assert_eq!(view(&sim).flash, 0.0);
    set(&mut sim, me, "cs_source");
    assert_eq!(view(&sim).swaps, 2);
    // Another character: a puff, no flash.
    sim.seconds(0.5);
    set(&mut sim, other, "combat_arms");
    let v = view(&sim);
    assert_eq!((v.swaps, v.puffs), (2, 1));
    assert_eq!(v.flash, 0.0);
    // A dead character's change shows nothing.
    sim.app.world_mut().entity_mut(other).insert(Dead { since: 0.0 });
    set(&mut sim, other, "cs_source");
    assert_eq!(view(&sim).puffs, 1);
}
