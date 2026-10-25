//! Ultimate Gun Game (apps/ultimate_gun_game, crate `ultimate_gun_game`,
//! binary `ultimate_gun_game`; docs/plans/active/ultimate-gun-game.md): the
//! gun game with guns and maps from every game the app composes; each rung
//! makes the player its gun's game (movement, body, HUD: `identity =
//! "weapon"`). A test bed for games as optional parts: a game takes part
//! with what it lends (`chimera_core::game_parts`), its guns and movement
//! required, its characters, HUD and maps optional (the fallbacks:
//! `chimera_sim::minigame::auto`).
//!
//! Built on the party's glue (`lucker_party`: both games composed, the
//! minigame runtime and its network part, the client with both HUDs), not
//! a fork of it: this crate adds only `UltimateGunGamePlugin`, which makes
//! the definition `MINIGAME` from the registered games' parts once the app
//! is built (`Plugin::finish`: the weapon catalogue and movements are
//! there), and again with the players' installs at the first frame and on
//! `ugg_new [seed] [rungs] [map]` (a new ladder for the players connected,
//! played at once: `minigame_net::change_match`). Its ladder has
//! `RUNGS` rungs spread across the games' guns (seeded, `SEED` unless
//! `ugg_new` names one), filtered to the games every connected player has
//! (`minigame_net::installed_by_all`, L3's mount reports); its pool every
//! game's maps; its kit each gun's game's knife (the base game's, `BASE`,
//! for a game without one). `parts` (console)
//! prints each game's capability report.
//!
//! Known shortcuts: the ladder is made again only by `ugg_new` (not at
//! each rotation or when a player without a game joins); with no CS:S install the
//! client still stops (`lucker_party::requirements`), its pool's first map
//! being CS:S's.

use bevy::prelude::*;
use chimera_core::{
    console::{Console, ConsoleAppExt},
    game_parts::PartsRegistry,
};
use chimera_sim::{
    minigame::{
        self, MinigameDef,
        auto::{self, AutoOptions, Capabilities},
    },
    weapon::data::WeaponCatalog,
};

/// The automatic gun game's id.
pub const MINIGAME: &str = "ultimate_gun_game:auto";

/// Rungs in a ladder.
pub const RUNGS: usize = 12;

/// The seed without `ugg_new <seed>`.
pub const SEED: u64 = 1;

/// The base game: binds, tick and bots, the knife and HUD of a game that
/// lends none, the HUD before a gun's game takes it (Combat Arms', as
/// Lucker Party's).
pub const BASE: &str = "combat_arms";

/// The options of a ladder of `rungs` from `seed`.
pub fn options(seed: u64, rungs: usize) -> AutoOptions {
    AutoOptions {
        id: MINIGAME.into(),
        name: "Ultimate Gun Game".into(),
        rungs,
        seed,
        base: BASE,
        time_limit: 600.0,
    }
}

/// The games' claimed parts as reports without the app (the maps are
/// right; the guns aren't checked): for the command line's first map.
fn claimed() -> Vec<Capabilities> {
    let empty = WeaponCatalog::default();
    lucker_party::GAME_PARTS.iter().map(|p| Capabilities::of(p, &empty, None)).collect()
}

/// The first map of minigame `id`'s pool: the automatic gun game's (every
/// game's maps, in turn), else an original's (`lucker_party::first_map`).
pub fn first_map(id: &str) -> Result<String, String> {
    if id == MINIGAME {
        return Ok(auto::map_pool(&claimed(), &|_| true).remove(0));
    }
    lucker_party::first_map(id)
}

/// The automatic gun game from the games' parts in `world`, its ladder of
/// `rungs` from `seed` over the games every player has, added as
/// `MINIGAME` (replacing one made before).
pub fn generate(world: &mut World, seed: u64, rungs: usize) -> Result<&'static MinigameDef, String> {
    let caps = auto::report(world);
    let games: Vec<&'static str> = caps.iter().map(|c| c.game).collect();
    let installed = chimera_net::minigame_net::installed_by_all(world, &games);
    let text = auto::definition(&caps, &options(seed, rungs), &|g| installed.contains(&g))?;
    minigame::add(world, MINIGAME, &text)
}

/// The seed and rungs the definition was last made with.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutoLadder {
    pub seed: u64,
    pub rungs: usize,
}

/// The automatic gun game made from the composed games' parts, the
/// fallback HUD for a game without one (the base game's), and its console
/// commands (`ugg_new`, `parts`).
pub struct UltimateGunGamePlugin {
    pub seed: u64,
    pub rungs: usize,
}

impl Default for UltimateGunGamePlugin {
    fn default() -> Self {
        Self { seed: SEED, rungs: RUNGS }
    }
}

impl Plugin for UltimateGunGamePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(AutoLadder {
            seed: self.seed,
            rungs: self.rungs,
        })
        .console_command(
            "ugg_new",
            "ugg_new [seed] [rungs] [map]: a new Ultimate Gun Game ladder for the players connected, played at once \
             (on `map`, else this one if in its pool, else its pool's first).",
            |w, a| {
                let last = *w.resource::<AutoLadder>();
                let seed = a.first().map(|s| s.parse().map_err(|_| format!("not a seed: {s}"))).transpose()?;
                let rungs = a.get(1).map(|s| s.parse().map_err(|_| format!("not a count: {s}"))).transpose()?;
                let ladder = AutoLadder {
                    seed: seed.unwrap_or(last.seed.wrapping_add(1)),
                    rungs: rungs.unwrap_or(last.rungs),
                };
                w.insert_resource(ladder);
                generate(w, ladder.seed, ladder.rungs)?;
                chimera_net::minigame_net::change_match(w, MINIGAME, a.get(2).map(String::as_str)).map(Some)
            },
        )
        .console_command("parts", "parts: what each composed game lends (its capability report).", |w, _| {
            Ok(Some(auto::report(w).iter().map(Capabilities::line).collect::<Vec<_>>().join("\n")))
        })
        .add_systems(PostStartup, with_installs);
    }

    fn finish(&self, app: &mut App) {
        let world = app.world_mut();
        // A game without a HUD of its own shows the base game's.
        world.get_resource_or_init::<PartsRegistry>().fallback_hud = BASE;
        if let Err(e) = generate(world, self.seed, self.rungs) {
            world.resource_mut::<Console>().print(chimera_core::console::Level::Error, e);
        }
    }
}

/// The ladder again once this machine's mount doctor has reported
/// (`LocalMounts`, inserted in a client's Startup): its own installs
/// filter it too.
fn with_installs(world: &mut World) {
    if !world.contains_resource::<chimera_net::net::manifest::LocalMounts>() {
        return;
    }
    let l = *world.resource::<AutoLadder>();
    if let Err(e) = generate(world, l.seed, l.rungs) {
        world.resource_mut::<Console>().print(chimera_core::console::Level::Error, e);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_first_map_is_the_pools() {
        assert_eq!(super::first_map(super::MINIGAME).unwrap(), "cs_source:de_dust2");
        assert_eq!(super::first_map(lucker_party::DEFAULT_MINIGAME).unwrap(), "cs_source:de_dust2");
    }
}
