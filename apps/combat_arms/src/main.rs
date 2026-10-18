//! The Combat Arms game binary (`combat_arms`): Combat Arms' simulation
//! (`combat_arms::SimPlugins` + `CombatArmsPlugins`) on the greybox, the
//! presentation, the network and the client shell with its HUD. Wave 0:
//! walk and shoot the placeholder weapons. The options are the shell's,
//! as the CS:S app's (`--help`); `--map` takes only the greybox.

use avian3d::prelude::*;
use bevy::prelude::*;
use chimera_client::client::Args;
use chimera_present::{MapPlugin, render};
use chimera_world::greybox;
use combat_arms::game::CombatArmsPlugins;
use combat_arms_app::ClientPlugin;

fn main() {
    if std::env::args().skip(1).any(|a| a == "--help" || a == "-h") {
        println!(
            "combat_arms: the Combat Arms skeleton on Chimera (placeholder movement, rifle and knife\n\
             on the greybox; nothing of Combat Arms' behaviour yet). The options are the shell's:\n"
        );
    }
    let mut args = Args::parse();
    if let Some(id) = args.game_map() {
        eprintln!("error: --map {id}: {}", combat_arms_app::load_map(id, 0).unwrap_err());
        std::process::exit(1);
    }
    // No menus yet: always straight into the greybox.
    args.map.get_or_insert_with(|| "greybox".into());
    let mut app = App::new();
    app.add_plugins((greybox::GreyboxMapPlugin, MapPlugin::empty()))
        .add_plugins((
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "combat_arms".into(),
                        resolution: match (args.window, args.scale) {
                            (Some(s), scale) => bevy::window::WindowResolution::new(s.x, s.y)
                                .with_scale_factor_override(scale.unwrap_or(1.0)),
                            (None, Some(scale)) => {
                                bevy::window::WindowResolution::default().with_scale_factor_override(scale)
                            }
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
            // Rendering eases between ticks with `map::interp`, not
            // avian's transform interpolation.
            PhysicsPlugins::default()
                .with_collision_hooks::<chimera_world::map::MapCollisionHooks>()
                .build()
                .disable::<PhysicsInterpolationPlugin>(),
            combat_arms::SimPlugins,
            CombatArmsPlugins,
        ))
        // What the map's drawing needs (as the CS:S app adds them).
        .add_plugins((
            render::world_material::WorldMaterialPlugin,
            render::rope_material::RopeMaterialPlugin,
            render::sprite_material::SpriteMaterialPlugin,
            render::prop_material::PropMaterialPlugin,
            render::shadows::ShadowMaterialPlugin,
            render::decal::DecalMaterialPlugin,
            render::spray::SprayMaterialPlugin,
            render::water::WaterMaterialPlugin,
            render::particles::ParticleMaterialPlugin,
            render::beams::BeamMaterialPlugin,
            render::sky_occluder::SkyOccluderPlugin,
            render::detail::DetailMaterialPlugin,
        ))
        .insert_resource(Time::<Fixed>::from_seconds(combat_arms::game::TICK_INTERVAL))
        // Network play: a listen server, or a client (`connect`).
        .add_plugins(chimera_net::net::NetPlugin)
        // The client shell with Combat Arms' HUD, then its other looks.
        .add_plugins(ClientPlugin { args })
        .add_plugins(combat_arms::ui::CaUiPlugin);
    if std::env::var("MASHUP_EXECUTOR").as_deref() != Ok("multi") {
        chimera_world::harness::single_threaded_schedules(&mut app);
    }
    app.run();
}
