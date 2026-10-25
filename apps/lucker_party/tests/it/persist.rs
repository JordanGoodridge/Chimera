//! What a player sets in the party survives a restart: the mouse
//! sensitivity (the shell's, over Combat Arms' options, which used to set
//! the mouse look back to its defaults at each start), the gun view FOV of
//! each game, and the binds (shared and each game's own). The config is
//! written as `host_writeconfig` writes it (`config_text`) and read as the
//! shell reads it at start (its lines run from the console's queue after
//! the startup systems), headless.

use bevy::prelude::*;
use chimera_client::client::input::{MouseSettings, mouse_cvars};
use chimera_core::console::{Console, ConsolePlugin, config_text};
use chimera_world::map::{GameViewModelFovs, ViewModelSettings};
use combat_arms::{game::options::CaOptionsPlugin, ui::options_apply::CaOptionsApplyPlugin};
use lucker_party::{PartyMousePlugin, controls::cvar_value};

/// The party's settings, as far as they are saved: a console, the shell's
/// mouse, Combat Arms' options (and what applies them), the party's mouse
/// and both games' view-model FOV; `config` is the config.cfg to read at
/// start (None: first start).
fn app(config: Option<&str>) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, ConsolePlugin))
        .init_resource::<MouseSettings>()
        .insert_resource(combat_arms::game::view_model::settings());
    mouse_cvars(&mut app);
    app.add_plugins((CaOptionsPlugin, CaOptionsApplyPlugin, PartyMousePlugin));
    combat_arms::game::view_model::register_fov_cvar(&mut app);
    lucker_party::add_view_model_fovs(&mut app);
    if let Some(text) = config {
        let lines: Vec<String> = text.lines().map(String::from).collect();
        app.add_systems(Startup, move |mut console: ResMut<Console>| {
            for l in &lines {
                console.submit(l.clone());
            }
        });
    }
    app.update();
    app.update();
    app
}

fn value(app: &mut App, name: &str) -> String {
    cvar_value(app.world_mut(), name).unwrap_or_default()
}

fn run(app: &mut App, line: &str) {
    app.world_mut().resource_mut::<Console>().submit(line);
    app.update();
    app.update();
}

#[test]
fn sensitivity_fov_and_binds_survive_a_restart() {
    let mut first = app(None);
    // First start: the party's defaults.
    assert_eq!(value(&mut first, "sensitivity"), "1");
    assert_eq!(value(&mut first, "viewmodel_fov"), "54");
    for line in [
        "sensitivity 2.5",
        "viewmodel_fov 90",
        "ca_viewmodel_fov 100",
        "bind q lastinv",
        "bind x +duck",
        "bindgame cs_source shift +speed",
        "bindgame combat_arms c +walk",
    ] {
        run(&mut first, line);
    }
    // Combat Arms' options changing (a setting touched) must not take the
    // mouse from the shell, as they did.
    first.world_mut().resource_mut::<combat_arms::game::options::CaMouseLook>().sensitivity = 40;
    first.update();
    assert_eq!(value(&mut first, "sensitivity"), "2.5", "Combat Arms' options leave the shell's mouse alone");
    let config = config_text(first.world_mut());
    assert!(config.contains("sensitivity 2.5"), "{config}");
    assert!(config.contains("viewmodel_fov 90") && config.contains("ca_viewmodel_fov 100"), "{config}");

    // The next start reads it back.
    let mut second = app(Some(&config));
    assert_eq!(value(&mut second, "sensitivity"), "2.5");
    assert_eq!(second.world().resource::<MouseSettings>().sensitivity, 2.5);
    assert_eq!(value(&mut second, "viewmodel_fov"), "90");
    assert_eq!(value(&mut second, "ca_viewmodel_fov"), "100");
    let fovs = second.world().resource::<GameViewModelFovs>().clone();
    let base = *second.world().resource::<ViewModelSettings>();
    assert_eq!(fovs.settings_for(base, Some("cs_source")).fov, 90.0);
    assert_eq!(fovs.settings_for(base, Some("combat_arms")).fov, 100.0);
    let console = second.world().resource::<Console>();
    assert_eq!(console.binds.get("q").map(String::as_str), Some("lastinv"));
    assert_eq!(console.binds.get("x").map(String::as_str), Some("+duck"));
    assert_eq!(console.layers["cs_source"].get("shift").map(String::as_str), Some("+speed"));
    assert_eq!(console.layers["combat_arms"].get("c").map(String::as_str), Some("+walk"));
}

#[test]
fn the_fov_stays_in_its_range() {
    let mut app = app(None);
    run(&mut app, "viewmodel_fov 30");
    assert_eq!(value(&mut app, "viewmodel_fov"), "54", "CS:S's own is the least");
    run(&mut app, "viewmodel_fov 500");
    assert_eq!(value(&mut app, "viewmodel_fov"), "110");
    run(&mut app, "ca_viewmodel_fov 10");
    assert_eq!(value(&mut app, "ca_viewmodel_fov"), "63.74");
}
