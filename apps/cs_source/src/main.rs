//! The game binary: `SimPlugins` + map + `NetPlugin` + `ClientPlugin`; the
//! per-frame schedules (main and render world) on single-threaded executors
//! (`MASHUP_EXECUTOR=multi`: Bevy's default; docs/performance.md).

use avian3d::prelude::*;
use bevy::prelude::*;
use cs_source_app::{
    SimPlugins, client, games, greybox,
    map::{MapDebugView, MapPlugin},
    slots::Loadout,
};

fn main() {
    let args = client::Args::parse();
    let mut app = App::new();
    match args.game_map() {
        // At the HDR level the console will start with (mat_hdr_level).
        Some(id) => match games::load_map_level(id, client::hdr::startup_level(&args.console)) {
            Ok(data) => {
                eprintln!(
                    "loaded {}: {} triangles, {} textures, {} spawns, {} warnings",
                    data.name,
                    data.triangle_count(),
                    data.textures.len(),
                    data.spawns.len(),
                    data.warnings.len()
                );
                for w in &data.warnings {
                    eprintln!("  warning: {w}");
                }
                let view = match (args.debug_view.as_deref(), args.lightmap_only) {
                    (Some("lighting"), _) => MapDebugView::Lighting { scale: 0.25 },
                    (Some("albedo"), _) => MapDebugView::Albedo,
                    (Some(other), _) => {
                        eprintln!("error: --debug-view {other}: expected lighting or albedo");
                        std::process::exit(2);
                    }
                    (None, true) => MapDebugView::Lighting { scale: 1.0 },
                    (None, false) => MapDebugView::Normal,
                };
                app.add_plugins(MapPlugin {
                    view,
                    ..MapPlugin::new(data)
                });
            }
            Err(e) => {
                eprintln!("error: --map {id}: {e}");
                std::process::exit(1);
            }
        },
        None => {
            // The greybox (behind the main menu unless the run starts
            // playing), and map systems without a map, so `map <name>` can
            // load one.
            app.add_plugins((greybox::GreyboxMapPlugin, MapPlugin::empty()));
        }
    }
    app.add_plugins((
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "mashup".into(),
                    // Physical pixels whatever the display's scaling.
                    // (`--scale`: as a display scaled that much.)
                    resolution: match (args.window, args.scale) {
                        (Some(s), scale) => bevy::window::WindowResolution::new(s.x, s.y)
                            .with_scale_factor_override(scale.unwrap_or(1.0)),
                        (None, Some(scale)) => bevy::window::WindowResolution::default().with_scale_factor_override(scale),
                        (None, None) => default(),
                    },
                    // A fixed size: tiling window managers float such
                    // windows instead of resizing them.
                    resizable: args.window.is_none(),
                    resize_constraints: args
                        .window
                        .map(|s| {
                            // In logical pixels.
                            let s = s.as_vec2() / args.scale.unwrap_or(1.0);
                            bevy::window::WindowResizeConstraints {
                                min_width: s.x,
                                min_height: s.y,
                                max_width: s.x,
                                max_height: s.y,
                            }
                        })
                        .unwrap_or_default(),
                    ..default()
                }),
                ..default()
            })
            // Warnings and errors also show in the in-game console.
            .set(bevy::log::LogPlugin {
                custom_layer: client::console::log_layer,
                ..default()
            }),
        // Rendering eases between ticks with `map::interp` (characters'
        // eyes and looks too), not avian's transform interpolation.
        PhysicsPlugins::default()
            .with_collision_hooks::<cs_source_app::map::MapCollisionHooks>()
            .build()
            .disable::<PhysicsInterpolationPlugin>(),
        SimPlugins,
        cs_source_app::games::cs_source::CsSourcePlugins,
        cs_source_app::render::world_material::WorldMaterialPlugin,
        cs_source_app::render::rope_material::RopeMaterialPlugin,
        cs_source_app::render::sprite_material::SpriteMaterialPlugin,
        cs_source_app::render::prop_material::PropMaterialPlugin,
        cs_source_app::render::shadows::ShadowMaterialPlugin,
        cs_source_app::render::decal::DecalMaterialPlugin,
        cs_source_app::render::spray::SprayMaterialPlugin,
        cs_source_app::render::water::WaterMaterialPlugin,
        cs_source_app::render::particles::ParticleMaterialPlugin,
    ))
    .add_plugins((
        cs_source_app::render::beams::BeamMaterialPlugin,
        cs_source_app::render::sky_occluder::SkyOccluderPlugin,
        cs_source_app::render::detail::DetailMaterialPlugin,
    ))
    // A game's maps run at that game's server tick.
    .insert_resource(match args.game_map() {
        Some(id) if id.starts_with("cs_source:") => {
            Time::<Fixed>::from_seconds(cs_source_app::games::cs_source::TICK_INTERVAL)
        }
        _ => Time::<Fixed>::from_hz(cs_source_app::DEFAULT_TICK_HZ),
    })
    .insert_resource(movement_config(&args))
    .insert_resource(Loadout {
        movement: cs_source_app::games::cs_source::movement::ID,
    })
    // Network play: a listen server, or a client (`connect`).
    .add_plugins(cs_source_app::net::NetPlugin)
    // The client shell with CS:S's HUD in it.
    .add_plugins(client::ClientPlugin { args })
    // CS:S's menus, panels and HUD (the shell reaches them through
    // `client::front_end`).
    .add_plugins(cs_source_app::games::cs_source::ui::CsSourceUiPlugin);
    if std::env::var("MASHUP_EXECUTOR").as_deref() != Ok("multi") {
        single_threaded_schedules(&mut app);
    }
    if let Some(path) = std::env::var_os("MASHUP_SYSTEM_DUMP") {
        dump_systems(&mut app, std::path::Path::new(&path));
        // Without the event loop having run, dropping the app crashes.
        std::process::exit(0);
    }
    app.run();
}

/// `MASHUP_SYSTEM_DUMP=<file>`: the app built as it would run (plugins
/// finished), every schedule's systems in the order they run, the main
/// world's then the render world's (`harness::system_order`), written to
/// the file instead of running; for refactors that move the client's
/// systems between plugins or crates, and for what an added system does
/// to the order of others (docs/OBSERVABILITY.md). `MASHUP_AMBIGUITY=1`
/// also logs the render world's `Render` schedule's ambiguous pairs.
fn dump_systems(app: &mut App, path: &std::path::Path) {
    use bevy::app::PluginsState;
    while app.plugins_state() == PluginsState::Adding {
        bevy::tasks::tick_global_task_pools_on_main_thread();
    }
    app.finish();
    // The render world before `cleanup`: pipelined rendering's cleanup
    // moves it to its own thread, out of the app. Its extract systems
    // need a main world to initialize against: an empty one.
    let render = app.get_sub_app_mut(bevy::render::RenderApp).map(|sub| {
        sub.world_mut().init_resource::<bevy::render::MainWorld>();
        // `MASHUP_AMBIGUITY=1`: log the `Render` schedule's ambiguous
        // system pairs (systems with conflicting access and no order;
        // the single-threaded executor runs them in whatever order the
        // graph sorts to, which other systems' constraints can change).
        if std::env::var("MASHUP_AMBIGUITY").as_deref() == Ok("1") {
            sub.edit_schedule(bevy::render::Render, |s| {
                let mut settings = s.get_build_settings();
                settings.ambiguity_detection = bevy::ecs::schedule::LogLevel::Warn;
                s.set_build_settings(settings);
            });
        }
        let text = cs_source_app::harness::system_order(sub.world_mut());
        sub.world_mut().remove_resource::<bevy::render::MainWorld>();
        text
    });
    app.cleanup();
    let mut text = cs_source_app::harness::system_order(app.world_mut());
    if let Some(render) = render {
        text += "render world:\n";
        text += &render;
    }
    match std::fs::write(path, text) {
        Ok(()) => eprintln!("systems in order written to {}", path.display()),
        Err(e) => eprintln!("error: MASHUP_SYSTEM_DUMP {}: {e}", path.display()),
    }
}

/// Run the per-frame schedules on one thread each instead of Bevy's
/// multi-threaded executor (`cs_source_app::single_threaded_schedules` for the
/// main world, the render world's here).
///
/// A frame runs about 1500 systems (main and render world), nearly all of
/// them tiny: handing each to a worker thread and waking the next cost more
/// than the systems did, and much more on a busy machine. With one thread
/// per schedule the main and render worlds took a quarter and a sixth less
/// time on de_dust2 (and bot matches) here; systems that iterate many
/// entities still spread over the task pool themselves (transform
/// propagation, visibility), and the main and render worlds still run side
/// by side (pipelined rendering). docs/performance.md, "Frame time pass".
/// `MASHUP_EXECUTOR=multi` keeps Bevy's default for comparisons.
fn single_threaded_schedules(app: &mut App) {
    use bevy::ecs::{
        intern::Interned,
        schedule::{ScheduleLabel, SingleThreadedExecutor},
    };
    cs_source_app::single_threaded_schedules(app);
    // On macOS the render world keeps Bevy's executor: it runs on the
    // pipelined render thread, and the window surface must be created on
    // the main thread (AppKit's NSView), which only the multi-threaded
    // executor hands its main-thread systems to; a single-threaded one runs
    // them where it is and panics ("can only access NSView on the main
    // thread").
    if cfg!(any(target_os = "macos", target_os = "ios")) {
        return;
    }
    let render: [Interned<dyn ScheduleLabel>; 4] = [
        bevy::render::Render.intern(),
        bevy::render::renderer::RenderGraph.intern(),
        bevy::core_pipeline::Core3d.intern(),
        bevy::core_pipeline::Core2d.intern(),
    ];
    if let Some(sub) = app.get_sub_app_mut(bevy::render::RenderApp) {
        for label in render {
            sub.edit_schedule(label, |s| {
                s.set_executor(SingleThreadedExecutor::new());
            });
        }
    }
}

/// Source movement settings: CS:S's, then `--exec` files, then `--cvar`s.
fn movement_config(args: &client::Args) -> cs_source_app::games::cs_source::movement::SourceMovementConfig {
    let mut config = cs_source_app::games::cs_source::movement::SourceMovementConfig::default();
    for file in &args.exec {
        match std::fs::read_to_string(file) {
            Ok(text) => {
                for problem in config.exec(&text) {
                    eprintln!("warning: {}: {problem}", file.display());
                }
            }
            Err(e) => {
                eprintln!("error: --exec {}: {e}", file.display());
                std::process::exit(2);
            }
        }
    }
    for (name, value) in &args.cvars {
        if let Err(e) = config.set_cvar(name, value) {
            eprintln!("error: --cvar {name}={value}: {e}");
            std::process::exit(2);
        }
    }
    config
}
