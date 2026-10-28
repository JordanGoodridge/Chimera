//! The announcer in a game made of several (specs/combat_arms/
//! kill_streaks.md; `chimera_client::client::announcer`): the party's
//! default pack is Combat Arms', drawn by the shell whatever the local
//! player's game, so a soldier holding a CS:S gun (CS:S's identity and
//! HUD) gets Combat Arms' FIRST KILL, then DOUBLE KILL with its voice;
//! with `sv_announcer none`, nothing. Headless, on the party's simulation
//! with both HUDs (`hud::party_with_huds`).

use bevy::prelude::*;
use chimera_client::client::announcer::{AnnouncerPlugin, AnnouncerView};
use chimera_core::{
    announcer::ActiveAnnouncer,
    console::Console,
    core::{Damage, DamageKind, Hitgroup},
};
use chimera_world::harness::Sim;

use crate::{
    gun_game::held,
    hud::{hud, party_with_huds_and, swap_to},
};

fn kill(sim: &mut Sim, me: Entity, victim: Entity) {
    let at = sim.position(victim);
    sim.app.world_mut().write_message(Damage {
        target: victim,
        attacker: Some(me),
        amount: 5.0,
        point: at,
        dir: Vec3::NEG_Z,
        hitgroup: Hitgroup::Chest,
        kind: DamageKind::Bullet,
        weapon: Some(cs_source::game::weapons::GLOCK),
        force: Vec3::ZERO,
    });
    sim.ticks(2);
}

/// The banner showing and the voices played so far.
fn shown(sim: &Sim) -> (Option<String>, Vec<String>) {
    let view = sim.app.world().resource::<AnnouncerView>();
    (view.player.shown.as_ref().map(|s| s.request.image.clone()), view.voices.clone())
}

/// Two kills a second apart with the Glock (back on the first rung
/// between them, so the CS:S gun and identity stay).
fn double_kill(sim: &mut Sim, me: Entity, first: Entity, second: Entity) -> [(Option<String>, Vec<String>); 2] {
    assert_eq!(held(sim, me), Some(cs_source::game::weapons::GLOCK));
    assert_eq!(hud(sim), cs_source::game::GAME, "CS:S's identity and HUD");
    kill(sim, me, first);
    let one = shown(sim);
    swap_to(sim, me, 0);
    assert_eq!(held(sim, me), Some(cs_source::game::weapons::GLOCK));
    assert_eq!(hud(sim), cs_source::game::GAME);
    kill(sim, me, second);
    [one, shown(sim)]
}

#[test]
fn a_cs_soldiers_double_kill_gets_combat_arms_announcer() {
    let (mut sim, me, other) = party_with_huds_and(|app| {
        app.add_plugins(AnnouncerPlugin);
    });
    let third = sim.spawn_character(Vec3::new(-3.0, 1.0, 0.0), combat_arms::game::movement::ID);
    sim.seconds(1.5);
    let active = sim.app.world().resource::<ActiveAnnouncer>().0.clone();
    assert_eq!(active.map(|p| p.id.clone()).as_deref(), Some("combat_arms"), "the party's default");
    let [one, two] = double_kill(&mut sim, me, other, third);
    assert_eq!(one.0.as_deref(), Some("UI_HUD_Message/Effect/FirstKill.dtx"), "the match's first kill");
    assert_eq!(one.1, ["UI_SND/FirstKill.wav"]);
    assert_eq!(two.0.as_deref(), Some("UI_HUD_Message/Effect/DoubleKill.dtx"));
    assert_eq!(two.1, ["UI_SND/FirstKill.wav", "UI_SND/doublekill.wav"]);
    // Gone after its 1.4 s.
    sim.seconds(1.5);
    assert_eq!(shown(&sim).0, None);
}

#[test]
fn no_announcer_when_the_server_says_none() {
    let (mut sim, me, other) = party_with_huds_and(|app| {
        app.add_plugins(AnnouncerPlugin);
    });
    let third = sim.spawn_character(Vec3::new(-3.0, 1.0, 0.0), combat_arms::game::movement::ID);
    sim.app.world_mut().resource_mut::<Console>().submit("sv_announcer none");
    sim.seconds(1.5);
    assert!(sim.app.world().resource::<ActiveAnnouncer>().0.is_none());
    let [one, two] = double_kill(&mut sim, me, other, third);
    assert_eq!(one, (None, Vec::new()));
    assert_eq!(two, (None, Vec::new()));
}
