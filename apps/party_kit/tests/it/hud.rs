//! The HUD follows the local player's game (the gun game's `identity =
//! "weapon"`): with a CS:S gun CS:S's HUD is up (its panels from the map's
//! CS:S HUD look, the minigame's panel on it) and Combat Arms' is down;
//! with a Combat Arms gun the other way round, Combat Arms' with the
//! minigame's panel; and back. Headless, on the party's simulation with
//! both HUDs' plugins and the shell's switching (`client::hud::
//! HudSwitchPlugin`), a stand-in CS:S HUD look as the map's.

use std::{collections::HashMap, sync::Arc};

use bevy::prelude::*;
use chimera_client::client::{
    fonts::UiFonts,
    hud::{GameDraws, HudDraws, HudLookOwner, HudRoot, HudSwitchPlugin},
};
use chimera_core::{core::LocalPlayer, gameplay::Gameplay};
use chimera_sim::minigame::{
    self,
    ladder::{self, Ladder},
};
use chimera_world::{
    greybox::GreyboxMapPlugin,
    harness::Sim,
    map::hud::{ActiveHud, GameHud, HudCoord, HudPanel},
};
use combat_arms::{
    game::movement,
    harness::CaSim,
    ui::hud::{CaHud, CaHudStatePlugin},
};
use cs_source::ui::{
    game_hud::{GameHudPlugin, Part},
    minigame_hud::{MinigameHudPlugin, PanelLine},
};
use party_kit::{DEFAULT_MINIGAME, PartyPartsPlugin};

use crate::gun_game::held;

/// A CS:S HUD look with a health panel, as a CS:S map's.
fn cs_look() -> ActiveHud {
    let mut hud = GameHud::default();
    hud.panels.insert(
        "HudHealth".into(),
        HudPanel {
            x: HudCoord::Start(16.0),
            y: HudCoord::End(42.0),
            wide: 102.0,
            tall: 36.0,
            background: Some([0, 0, 0, 96]),
            icon: Vec2::new(8.0, 2.0),
            digit: Vec2::new(50.0, 2.0),
            digit2: Vec2::ZERO,
            keys: HashMap::new(),
        },
    );
    ActiveHud(Arc::new(hud), HashMap::new())
}

/// The party with both HUDs (Combat Arms' state and a stand-in root for
/// its drawn panels, CS:S's panels and the minigame's panel on it), a
/// local soldier and another, the gun game playing.
pub(crate) fn party_with_huds() -> (Sim, Entity, Entity) {
    let mut sim = Sim::with(CaSim, |app| {
        app.add_plugins((GreyboxMapPlugin, PartyPartsPlugin))
            .init_resource::<UiFonts>()
            .add_plugins((HudSwitchPlugin, CaHudStatePlugin, GameHudPlugin, MinigameHudPlugin))
            .insert_resource(HudLookOwner(cs_source::game::GAME))
            .insert_resource(cs_look());
        // Combat Arms' drawn HUD: a root and what it draws instead of the
        // shell's (`ui::hud::CaHudPlugin` spawns its roots so, with
        // `HudRoot`, and registers its `GameDraws`).
        app.world_mut().spawn((Node::default(), HudRoot(combat_arms::game::GAME)));
        app.insert_resource(HudDraws(vec![(combat_arms::game::GAME, combat_arms::ui::hud::DRAWS)]));
    });
    minigame::start(sim.app.world_mut(), DEFAULT_MINIGAME).expect("the gun game starts");
    let me = sim.spawn_character(Vec3::new(0.0, 1.0, 0.0), movement::ID);
    sim.app.world_mut().entity_mut(me).insert(LocalPlayer);
    let other = sim.spawn_character(Vec3::new(3.0, 1.0, 0.0), movement::ID);
    sim.seconds(1.5);
    (sim, me, other)
}

pub(crate) fn hud(sim: &Sim) -> &'static str {
    sim.app.world().resource::<Gameplay>().hud
}

/// Whether a root of `game`'s HUD shows.
pub(crate) fn shown(sim: &mut Sim, game: &str) -> bool {
    let w = sim.app.world_mut();
    let roots: Vec<Visibility> = w
        .query::<(&HudRoot, &Visibility)>()
        .iter(w)
        .filter(|(r, _)| r.0 == game)
        .map(|(_, v)| *v)
        .collect();
    assert!(!roots.is_empty(), "{game}'s HUD has roots");
    roots.iter().all(|v| *v != Visibility::Hidden)
}

fn cs_panels(sim: &mut Sim) -> usize {
    let w = sim.app.world_mut();
    w.query_filtered::<(), With<Part>>().iter(w).count()
}

/// The CS:S minigame panel's lines, as text.
pub(crate) fn cs_minigame_lines(sim: &mut Sim) -> Vec<(PanelLine, String)> {
    let w = sim.app.world_mut();
    w.query::<(&PanelLine, &Text)>().iter(w).map(|(k, t)| (*k, t.0.clone())).collect()
}

pub(crate) fn swap_to(sim: &mut Sim, me: Entity, rung: usize) {
    let w = sim.app.world_mut();
    let l = w.resource::<Ladder>().clone();
    ladder::set_rung(w, me, rung, &l);
    sim.seconds(0.5);
}

/// CS:S's HUD with a CS:S gun, Combat Arms' with the Beretta, CS:S's
/// again with the Desert Eagle: each time only the active one's nodes are
/// up and the minigame's panel is on it.
#[test]
fn the_hud_follows_the_gun_with_the_minigame_on_both() {
    let (mut sim, me, _other) = party_with_huds();
    let title = sim.app.world().resource::<chimera_sim::minigame::ActiveMinigame>().def.name.clone();
    // The Glock: CS:S's HUD.
    assert_eq!(held(&sim, me), Some(cs_source::game::weapons::GLOCK));
    assert_eq!(hud(&sim), cs_source::game::GAME);
    assert!(!shown(&mut sim, combat_arms::game::GAME), "Combat Arms' HUD down");
    assert!(cs_panels(&mut sim) > 0, "CS:S's health panel up");
    assert_eq!(*sim.app.world().resource::<GameDraws>(), GameDraws::default(), "the shell's crosshair for CS:S");
    let lines = cs_minigame_lines(&mut sim);
    assert!(lines.contains(&(PanelLine::Title, title.clone())), "the minigame on CS:S's HUD: {lines:?}");
    assert!(lines.iter().filter(|(k, _)| *k == PanelLine::Status).count() >= 2, "its rung and more: {lines:?}");
    assert!(shown(&mut sim, cs_source::game::GAME), "the panel shows");
    // The Beretta: Combat Arms'.
    swap_to(&mut sim, me, 1);
    assert_eq!(held(&sim, me), Some("combat_arms:beretta_m92fs"));
    assert_eq!(hud(&sim), combat_arms::game::GAME);
    assert!(shown(&mut sim, combat_arms::game::GAME), "Combat Arms' HUD up");
    assert!(!shown(&mut sim, cs_source::game::GAME), "CS:S's minigame panel down");
    assert_eq!(cs_panels(&mut sim), 0, "CS:S's panels taken down");
    assert!(!sim.app.world().contains_resource::<ActiveHud>(), "CS:S's look put away");
    assert!(sim.app.world().resource::<GameDraws>().crosshair, "Combat Arms draws its own");
    let ca = sim.app.world().resource::<CaHud>().minigame.clone().expect("the minigame on Combat Arms' HUD");
    assert_eq!(ca.title, title);
    assert!(ca.lines.len() >= 2, "{ca:?}");
    // The Desert Eagle: CS:S's again, built anew.
    swap_to(&mut sim, me, 2);
    assert_eq!(held(&sim, me), Some(cs_source::game::weapons::DEAGLE));
    assert_eq!(hud(&sim), cs_source::game::GAME);
    assert!(sim.app.world().contains_resource::<ActiveHud>(), "the look back");
    assert!(cs_panels(&mut sim) > 0, "CS:S's panels built again");
    assert!(!shown(&mut sim, combat_arms::game::GAME));
    assert!(shown(&mut sim, cs_source::game::GAME));
    let lines = cs_minigame_lines(&mut sim);
    assert!(lines.iter().any(|(_, t)| t.starts_with("Rung 3/")), "the third rung shown: {lines:?}");
}
