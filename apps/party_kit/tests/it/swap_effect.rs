//! A rung's swap with `party_swap_effect` (`chimera_client::client::
//! swap_effect`) on the party's simulation: the local player's gun going
//! from CS:S's to Combat Arms' flashes once per swap; off (the default)
//! nothing.

use chimera_client::client::swap_effect::{SwapEffectPlugin, SwapEffectSetting, SwapEffectView};
use chimera_core::core::LocalPlayer;
use chimera_sim::minigame;
use chimera_world::{greybox::GreyboxMapPlugin, harness::Sim};
use combat_arms::harness::CaSim;
use cs_source::game::weapons as cs_weapons;
use party_kit::{DEFAULT_MINIGAME, PartyPartsPlugin};

use crate::gun_game::{held, soldier};

const M4: &str = "combat_arms:m4a1";

fn swaps(on: bool) -> u32 {
    let mut sim = Sim::new(CaSim, (GreyboxMapPlugin, PartyPartsPlugin, SwapEffectPlugin));
    minigame::start(sim.app.world_mut(), DEFAULT_MINIGAME).expect("the gun game starts");
    sim.app.world_mut().resource_mut::<SwapEffectSetting>().on = u8::from(on);
    let a = soldier(&mut sim);
    sim.app.world_mut().entity_mut(a).insert(LocalPlayer);
    sim.seconds(1.5);
    assert_eq!(held(&sim, a), Some(cs_weapons::GLOCK));
    assert_eq!(sim.app.world().resource::<SwapEffectView>().swaps, 0, "joining is no swap");
    for gun in [M4, cs_weapons::GLOCK] {
        chimera_sim::weapon::give(sim.app.world_mut(), a, gun);
        sim.seconds(1.0);
        assert_eq!(held(&sim, a), Some(gun));
    }
    sim.app.world().resource::<SwapEffectView>().swaps
}

#[test]
fn a_swap_flashes_once_when_on() {
    assert_eq!(swaps(false), 0, "off: nothing");
    assert_eq!(swaps(true), 2, "on: one flash per swap (CS:S to Combat Arms and back)");
}
