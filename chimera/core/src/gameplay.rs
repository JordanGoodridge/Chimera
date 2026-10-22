//! The gameplay: which game's implementation fills each match-wide slot
//! (docs/plans/active/architecture-review.md, A1): `Gameplay` names the
//! rules (whose match-wide systems run: rules, objectives, bots, the
//! match state on the network), the HUD drawn, the binds and movement
//! cvars the shell offers, and the fixed tick. Set from the match (a
//! minigame definition later; the app's default now: `set_gameplay`),
//! changed at run time with `switch_gameplay` (the tick follows,
//! `core::set_tick_length`). `Games` stays the registry of installs and
//! map loaders; its default game keeps only its meaning for bare map
//! names.
//!
//! Gating is by slot, not by plugin presence: a game's match-wide systems
//! carry the run condition `gameplay_is(<its id>)`, its HUD's
//! `hud_is(<its id>)`, so an app with several games' parts runs only the
//! active one's. Without a `Gameplay` both are true (every part added
//! runs, as before there was one); a game's gated parts make themselves
//! the gameplay when the app hasn't chosen (`default_gameplay`), so the
//! first game's parts added play, as the first game registered was the
//! default. Per-entity behaviour (movement, a weapon's hooks, a game's
//! bot marker) is gated by its components and needs no condition.
//!
//! Conditions on the systems, not a set per game: Bevy flattens every
//! system set out of the dependency graph with a swap-remove, so a new
//! set reorders systems no edge orders (the single-threaded executors
//! run them in that order); a condition adds no node, and the apps'
//! system order stays the same. A tuple that already has a collective
//! `run_if` takes the gameplay's as a second one (its anonymous set
//! exists already); one without takes it with `distributive_run_if`.
//!
//! Known shortcuts: the slots are game ids and a tick; the character kit
//! and the weapon sets offered (A1, D2) aren't slots yet (a weapon's
//! hooks are per entity, any registered set is offered). Hooks a game
//! keeps in single-slot resources (`core::NewGame`, the network's join,
//! bot and hold hooks) and its console commands aren't switched with the
//! gameplay, and the shell reads the binds at start only. A HUD gated off
//! keeps the nodes it spawned at start (only its updates stop).

use std::time::Duration;

use bevy::prelude::*;

use crate::game_registry::{GameInfo, Games};

/// The active implementation of each match-wide slot, by id.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct Gameplay {
    /// The game whose rules run (its systems gated by `gameplay_is`).
    pub rules: &'static str,
    /// The HUD drawn (its systems gated by `hud_is`): a game's id for
    /// its own HUD.
    pub hud: &'static str,
    /// The game whose default binds and movement cvars the shell uses.
    pub binds: &'static str,
    /// The fixed tick.
    pub tick: Duration,
}

impl Gameplay {
    /// Every slot `game`'s own, at its tick.
    pub fn of(game: &'static GameInfo) -> Self {
        Self {
            rules: game.id,
            hud: game.id,
            binds: game.id,
            tick: game.tick(),
        }
    }
}

/// Run condition: `game` is the gameplay's rules (or no gameplay is
/// set): a game's match-wide systems.
pub fn gameplay_is(game: &'static str) -> impl FnMut(Option<Res<Gameplay>>) -> bool + Clone {
    move |g: Option<Res<Gameplay>>| g.is_none_or(|g| g.rules == game)
}

/// Run condition: `hud` is the gameplay's HUD (or no gameplay is set).
pub fn hud_is(hud: &'static str) -> impl FnMut(Option<Res<Gameplay>>) -> bool + Clone {
    move |g: Option<Res<Gameplay>>| g.is_none_or(|g| g.hud == hud)
}

/// Change the gameplay at run time: the slots, and the fixed tick
/// (`core::set_tick_length`).
pub fn switch_gameplay(world: &mut World, gameplay: Gameplay) {
    crate::core::set_tick_length(world, gameplay.tick);
    world.insert_resource(gameplay);
}

/// The registered game filling a slot (`|g| g.binds`), else the default
/// game (the first registered).
pub fn slot_game(world: &World, slot: impl Fn(&Gameplay) -> &'static str) -> Option<&'static GameInfo> {
    let games = world.get_resource::<Games>()?;
    world
        .get_resource::<Gameplay>()
        .and_then(|g| games.get(slot(g)))
        .or_else(|| games.default_game())
}

/// Setting the gameplay on an app.
pub trait GameplayAppExt {
    /// The app's gameplay (replacing any) and its tick (`Time<Fixed>`).
    fn set_gameplay(&mut self, gameplay: Gameplay) -> &mut Self;
    /// `gameplay` unless the app has one: a game's gated parts.
    fn default_gameplay(&mut self, gameplay: Gameplay) -> &mut Self;
}

impl GameplayAppExt for App {
    fn set_gameplay(&mut self, gameplay: Gameplay) -> &mut Self {
        self.insert_resource(Time::<Fixed>::from_duration(gameplay.tick)).insert_resource(gameplay)
    }

    fn default_gameplay(&mut self, gameplay: Gameplay) -> &mut Self {
        if !self.world().contains_resource::<Gameplay>() {
            self.insert_resource(gameplay);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_registry::GameAppExt;

    const A_INFO: GameInfo = GameInfo {
        id: "a_game",
        short_name: "A",
        content_dir: "a",
        tick_interval: 0.015,
        open: |_| Err(std::io::ErrorKind::NotFound.into()),
        map_cycle: |_| Vec::new(),
        map_names: |_| Vec::new(),
        map_file: |n| Some(format!("maps/{n}.bsp")),
        steam: None,
        import_map: |_| Err("no".into()),
        default_binds_cfg: "cfg/default.cfg",
        default_binds: &[],
        old_default_binds: &[],
        movement_cvars: &[],
        meters_per_unit: 0.0254,
    };
    static A: GameInfo = A_INFO;
    static B: GameInfo = GameInfo {
        id: "b_game",
        tick_interval: 0.03125,
        ..A_INFO
    };

    #[derive(Resource, Default)]
    struct Ran(Vec<&'static str>);

    fn ran(name: &'static str) -> impl FnMut(ResMut<Ran>) {
        move |mut r: ResMut<Ran>| r.0.push(name)
    }

    fn frame(app: &mut App) -> Vec<&'static str> {
        app.update();
        std::mem::take(&mut app.world_mut().resource_mut::<Ran>().0)
    }

    /// Each game's systems run only while it is the gameplay's (rules or
    /// HUD); everything runs without a gameplay.
    #[test]
    fn only_the_active_gameplays_systems_run() {
        let mut app = App::new();
        app.init_resource::<Ran>().add_systems(
            Update,
            (
                ran("a rules").run_if(gameplay_is("a_game")),
                ran("b rules").run_if(gameplay_is("b_game")),
                (ran("a hud"), ran("a hud 2")).chain().distributive_run_if(hud_is("a_game")),
                ran("b hud").run_if(hud_is("b_game")),
            )
                .chain(),
        );
        assert_eq!(frame(&mut app), ["a rules", "b rules", "a hud", "a hud 2", "b hud"]);
        app.set_gameplay(Gameplay::of(&A));
        assert_eq!(frame(&mut app), ["a rules", "a hud", "a hud 2"]);
        switch_gameplay(app.world_mut(), Gameplay { hud: "a_game", ..Gameplay::of(&B) });
        assert_eq!(frame(&mut app), ["b rules", "a hud", "a hud 2"]);
    }

    /// The tick follows the gameplay; the first default holds; a slot's
    /// game is looked up in `Games`, the default game without a gameplay.
    #[test]
    fn the_tick_and_slots_follow_the_gameplay() {
        let mut app = App::new();
        app.register_game(&A).register_game(&B);
        assert_eq!(slot_game(app.world(), |g| g.binds).map(|g| g.id), Some("a_game"));
        app.default_gameplay(Gameplay::of(&B)).default_gameplay(Gameplay::of(&A));
        assert_eq!(app.world().resource::<Gameplay>().rules, "b_game");
        assert_eq!(slot_game(app.world(), |g| g.binds).map(|g| g.id), Some("b_game"));
        app.set_gameplay(Gameplay::of(&A));
        assert_eq!(app.world().resource::<Time<Fixed>>().timestep(), A.tick());
        switch_gameplay(app.world_mut(), Gameplay::of(&B));
        assert_eq!(app.world().resource::<Time<Fixed>>().timestep(), B.tick());
        assert_eq!(slot_game(app.world(), |g| g.rules).map(|g| g.id), Some("b_game"));
    }
}
