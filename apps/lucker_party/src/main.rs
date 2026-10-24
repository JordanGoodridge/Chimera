//! The Lucker Party game binary (`lucker_party`): a minigame (the gun game
//! on de_dust2 without `+minigame`) played as Combat Arms' soldiers with
//! both games' guns on its pool's first map (`--map` picks another). The
//! options are the shell's (`--help`); `+minigame <id>` the minigame,
//! `minigames` (console) lists them.

use bevy::prelude::*;
use chimera_client::client::Args;
use chimera_core::gameplay::{Gameplay, GameplayAppExt};
use chimera_present::{MapPlugin, PresentationPlugins};
use lucker_party::{ClientPlugin, PartyPlugins};

fn main() {
    if std::env::args().skip(1).any(|a| a == "--help" || a == "-h") {
        println!(
            "lucker_party: minigames with Counter-Strike: Source's and Combat Arms' parts. Starts {} on\n\
             its first map; +minigame <id> picks another ({}), --map cs_source:<name> (or\n\
             combat_arms:<name>) the map. The options are the shell's (below, named after the CS:S app).\n",
            lucker_party::DEFAULT_MINIGAME,
            lucker_party::ids().join(", "),
        );
    }
    let args = lucker_party::with_defaults(Args::parse());
    // What this machine lacks, said exactly (a joiner needs both games
    // too: each player loads the maps, models and sounds from their own).
    let problems = lucker_party::requirements();
    if !problems.is_empty() {
        let (text, fatal) = lucker_party::report(&problems);
        eprintln!("{text}");
        if fatal {
            std::process::exit(1);
        }
    }
    let mut app = App::new();
    let id = args.game_map().unwrap_or("cs_source:de_dust2").to_string();
    match lucker_party::load_map(&id, 0) {
        Ok(data) => {
            eprintln!(
                "loaded {id}: {} triangles, {} textures, {} spawns, {} warnings",
                data.triangle_count(),
                data.textures.len(),
                data.spawns.len(),
                data.warnings.len()
            );
            for w in &data.warnings {
                eprintln!("  warning: {w}");
            }
            app.add_plugins(MapPlugin::new(data));
        }
        Err(e) => {
            eprintln!("error: --map {id}: {e}");
            std::process::exit(1);
        }
    }
    app.add_plugins((
        // Both installs as `cs_source://` and `combat_arms://` assets
        // (before `AssetPlugin`).
        chimera_present::assets::InstallSourcesPlugin(vec![
            chimera_present::assets::InstallSource::of(&cs_source::game::INFO),
            chimera_present::assets::InstallSource::shared(combat_arms::game::INFO.id, combat_arms::game::world::mount),
        ]),
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Lucker Party".into(),
                    resolution: match (args.window, args.scale) {
                        (Some(s), scale) => bevy::window::WindowResolution::new(s.x, s.y)
                            .with_scale_factor_override(scale.unwrap_or(1.0)),
                        (None, Some(scale)) => bevy::window::WindowResolution::default().with_scale_factor_override(scale),
                        (None, None) => default(),
                    },
                    resizable: args.window.is_none(),
                    ..default()
                }),
                ..default()
            })
            // Warnings and errors also show in the in-game console.
            .set(bevy::log::LogPlugin {
                custom_layer: chimera_client::client::console::log_layer,
                ..default()
            }),
        // Avian as Chimera runs it (rendering eases between ticks).
        chimera_world::physics_plugins(),
        PartyPlugins,
    ))
    // What the map's drawing needs.
    .add_plugins(PresentationPlugins)
    // Combat Arms' gameplay until the minigame (`+minigame`, its console
    // line) takes the rules slot and names the others.
    .set_gameplay(Gameplay::of(&combat_arms::game::INFO))
    .insert_resource(chimera_core::core::BuildVersion(lucker_party::VERSION))
    // Network play: a listen server, or a client (`connect`).
    .add_plugins(chimera_net::net::NetPlugin)
    // The client shell with Combat Arms' HUD, then its other looks.
    .add_plugins(ClientPlugin { args })
    .add_plugins(combat_arms::ui::CaUiPlugin);
    // The executors, `CHIMERA_SYSTEM_DUMP`, then the game, as every app.
    chimera_client::client::app::run(app);
}
