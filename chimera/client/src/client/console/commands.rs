use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, OnceLock},
};

use bevy::{
    app::AppExit,
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
        mouse::{MouseScrollUnit, MouseWheel},
    },
    log::{BoxedLayer, tracing_subscriber::Layer},
    prelude::*,
    window::CursorOptions,
};

use super::super::{FirstPersonCamera, fonts::UiFonts, widgets::Interaction};
use super::*;
use crate::{
    console::{Console, ConsoleAppExt, Level, cfg_dir, parse, resource_cvar},
    core::{LocalPlayer, MovementState, Velocity},
    game_registry::{GameInfo, Games},
    map::PlaySound,
};

// ------------------------------------------------------------- commands

/// Map names found by `map_names` (for `map` completion), with their
/// game's id, until an import changes them.
pub(super) static MAP_NAMES: Mutex<Option<(&'static str, Vec<String>)>> = Mutex::new(None);

/// The install changed: find its maps again.
pub fn forget_map_names() {
    *MAP_NAMES.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Every map `game` (the default game: `game_registry`) can load: the
/// install's, its downloads and Chimera's cache (cached; `import` refreshes
/// it). None without a game.
pub fn map_names(game: Option<&'static GameInfo>) -> Vec<String> {
    let Some(game) = game else { return Vec::new() };
    let mut cached = MAP_NAMES.lock().unwrap_or_else(|e| e.into_inner());
    if cached.as_ref().is_some_and(|(id, _)| *id != game.id) {
        *cached = None;
    }
    cached
        .get_or_insert_with(|| {
            let Some(mount) = game.mount() else {
                return (game.id, Vec::new());
            };
            let mut names = (game.map_names)(&mount);
            names.sort();
            names.dedup();
            (game.id, names)
        })
        .1
        .clone()
}

/// The default game's map cycle (`GameInfo::map_cycle`), its maps those
/// installed (cached).
pub(super) fn map_cycle(game: &'static GameInfo) -> Vec<String> {
    static CYCLE: Mutex<Option<(&'static str, Vec<String>)>> = Mutex::new(None);
    let mut cached = CYCLE.lock().unwrap_or_else(|e| e.into_inner());
    if cached.as_ref().is_some_and(|(id, _)| *id != game.id) {
        *cached = None;
    }
    cached
        .get_or_insert_with(|| (game.id, game.mount().map(|m| (game.map_cycle)(&m)).unwrap_or_default()))
        .1
        .clone()
}

/// The rules' map cycle follows the map loaded (the end of a game moves
/// on to the next one: `rules::game_end`).
pub(super) fn follow_map_cycle(
    name: Option<Res<crate::map::LoadedMapName>>,
    games: Option<Res<Games>>,
    mut cycle: ResMut<crate::core::MapCycle>,
) {
    let Some(name) = name.filter(|n| n.is_changed()) else { return };
    let short = crate::net::maps::map_name(&name.0).to_string();
    if cycle.current != short {
        cycle.current = short;
    }
    if cycle.maps.is_empty()
        && let Some(game) = games.and_then(|g| g.default_game())
    {
        cycle.maps = map_cycle(game);
    }
}

/// Where a map of `game`'s comes from: the game's own content, its
/// download folder, or Chimera's cache.
pub(super) fn map_source(game: &GameInfo, name: &str) -> &'static str {
    let Some(file) = (game.map_file)(name) else { return "game" };
    let in_dir = |dir: Option<std::path::PathBuf>| dir.is_some_and(|d| d.join(&file).is_file());
    if in_dir(game.content_cache()) {
        "imported"
    } else if in_dir(game.download_dir()) {
        "downloaded"
    } else {
        "game"
    }
}

/// Whether a `map` command's load is under way.
pub fn map_loading(w: &World) -> bool {
    w.contains_resource::<MapLoad>()
}

/// The map a `map` command is loading (its name, without the game).
pub fn loading_map(w: &World) -> Option<String> {
    let id = &w.get_resource::<MapLoad>()?.id;
    Some(id.rsplit(':').next().unwrap_or(id).to_string())
}

/// A map loading in the background for the `map` command.
#[derive(Resource)]
pub(super) struct MapLoad {
    id: String,
    task: bevy::tasks::Task<Result<(crate::map::MapData, crate::map::prepare::Prepared), String>>,
}

/// Load map `id` in the background (from `file` when given: a copy
/// downloaded from a server), then swap it in place (`finish_map_load`):
/// everyone respawns at the new spawn points.
pub fn start_map_load(w: &mut World, id: &str, file: Option<std::path::PathBuf>) {
    w.remove_resource::<FreshGame>();
    w.insert_resource(crate::map::LoadedMapName(id.to_string()));
    // At the HDR level set now (mat_hdr_level), as the game does.
    let hdr_level = w.get_resource::<crate::client::hdr::HdrSettings>().map_or(0, |s| s.level);
    crate::map::loading::reset();
    // The app loads it (`AppHooks`), then the spawn's own work is done
    // there too (`map::prepare`): the frame that swaps it in only spawns.
    let hooks = w.get_resource::<crate::client::AppHooks>().copied();
    let prepare = crate::map::prepare::preparation(w);
    let task = bevy::tasks::AsyncComputeTaskPool::get().spawn({
        let id = id.to_string();
        async move {
            let hooks = hooks.ok_or("this app loads no maps")?;
            let data = match file {
                Some(f) => (hooks.load_map_file)(&id, &f, hdr_level),
                None => (hooks.load_map)(&id, hdr_level),
            }?;
            let prepared = prepare.run(&data);
            Ok((data, prepared))
        }
    });
    w.insert_resource(MapLoad { id: id.to_string(), task });
}

/// The server's map, for joining it (`net::NetEvent::LoadMap`): the
/// greybox at once, another in the background (from the downloaded file
/// when there is one; a bare name is the default game's). The handshake
/// checks the result.
pub fn load_server_map(w: &mut World, map: &str, file: Option<std::path::PathBuf>) {
    if map == crate::net::GREYBOX {
        load_greybox(w);
        return;
    }
    let id = match crate::game_registry::default_game(w) {
        Some(game) if !map.contains(':') => game.map_id(map),
        _ => map.to_string(),
    };
    start_map_load(w, &id, file);
}

/// Swap in a map once its background load finishes.
pub(super) fn finish_map_load(w: &mut World) {
    let Some(mut load) = w.get_resource_mut::<MapLoad>() else {
        return;
    };
    let Some(result) = bevy::tasks::block_on(bevy::tasks::poll_once(&mut load.task)) else {
        return;
    };
    let id = load.id.clone();
    w.remove_resource::<MapLoad>();
    let (data, prepared) = match result {
        Ok(d) => d,
        Err(e) => {
            w.resource_mut::<Console>()
                .print(crate::console::Level::Error, format!("map {id}: {e}"));
            crate::client::front_end::map_load_failed(w);
            // Joining a server needs its map.
            match w.get_resource::<crate::core::NetRole>() {
                Some(crate::core::NetRole::Client) => {
                    crate::net::disconnect(w, &format!("couldn't load the server's map {id}: {e}"));
                }
                Some(crate::core::NetRole::Server) => {
                    // Still on the map that plays (its players were told of a
                    // change: `net::maps` calls it off).
                    let game = crate::game_registry::default_game(w);
                    let back = w
                        .get_resource::<crate::map::MapFile>()
                        .zip(game)
                        .map(|(f, game)| game.map_id(&f.name))
                        .unwrap_or_else(|| GREYBOX.into());
                    w.insert_resource(crate::map::LoadedMapName(back));
                    crate::client::front_end::entered_game(w);
                }
                _ => {}
            }
            return;
        }
    };
    let summary = format!(
        "loaded {id}: {} triangles, {} spawns",
        data.triangle_count(),
        data.spawns.len()
    );
    unload_greybox(w);
    crate::map::prepare::change_map_prepared(w, data, prepared, crate::map::MapDebugView::Normal);
    // At the gameplay's server tick (a mashup plays another game's map at
    // its gameplay's tick); without a gameplay the default game's, else
    // the map's game's.
    let tick = w.get_resource::<crate::gameplay::Gameplay>().map(|g| g.tick).or_else(|| {
        crate::game_registry::default_game(w)
            .or_else(|| {
                id.split_once(':')
                    .and_then(|(game, _)| w.get_resource::<Games>()?.get(game))
            })
            .map(|g| g.tick())
    });
    let tick = tick.unwrap_or(std::time::Duration::from_secs_f64(1.0 / crate::core::DEFAULT_TICK_HZ));
    crate::core::set_tick_length(w, tick);
    // Follow the map's presentation (Source LDR: no tonemapping).
    let tonemapping = w.resource::<crate::map::ActiveMapLook>().0.tonemapping;
    let cams: Vec<Entity> = w
        .query_filtered::<Entity, With<crate::client::FirstPersonCamera>>()
        .iter(w)
        .collect();
    for c in cams {
        let t = if tonemapping {
            bevy::core_pipeline::tonemapping::Tonemapping::default()
        } else {
            bevy::core_pipeline::tonemapping::Tonemapping::Linear
        };
        w.entity_mut(c).insert(t);
    }
    crate::core::new_game(w);
    w.resource_mut::<Console>().info(summary);
    crate::client::front_end::entered_game(w);
    crate::client::net::listen_if_hosting(w);
    if w.remove_resource::<FreshGame>().is_some() {
        crate::client::front_end::start_game(w);
    }
}

/// The map loading is a game the `map` command starts (not a server's
/// map change or a server's map we join): the local player joins it as
/// `rules::JoinChoice` says (`join_menus::start_game`).
#[derive(Resource)]
pub(super) struct FreshGame;

/// Take the app's greybox out (`AppHooks::unload_greybox`) for a map.
fn unload_greybox(w: &mut World) {
    if let Some(hooks) = w.get_resource::<crate::client::AppHooks>().copied() {
        (hooks.unload_greybox)(w);
    }
}

/// The greybox map in place of whatever map is loaded (`map greybox`, and
/// `disconnect`, which leaves it behind the main menu), everyone respawned
/// on it at the default tick.
pub fn load_greybox(w: &mut World) {
    w.remove_resource::<MapLoad>();
    crate::map::unload_map(w);
    unload_greybox(w);
    if let Some(hooks) = w.get_resource::<crate::client::AppHooks>().copied() {
        (hooks.respawn_greybox)(w);
    }
    w.remove_resource::<crate::map::ActiveMapLook>();
    w.insert_resource(crate::map::LoadedMapName(GREYBOX.into()));
    crate::core::set_tick_length(w, std::time::Duration::from_secs_f64(1.0 / crate::core::DEFAULT_TICK_HZ));
    let cams: Vec<Entity> = w
        .query_filtered::<Entity, With<crate::client::FirstPersonCamera>>()
        .iter(w)
        .collect();
    for c in cams {
        let mut e = w.entity_mut(c);
        e.insert(bevy::core_pipeline::tonemapping::Tonemapping::default());
        // The map's sky drew under the view (no clear); the greybox has none.
        if let Some(mut camera) = e.get_mut::<Camera>() {
            camera.clear_color = ClearColorConfig::Default;
        }
    }
    let mut skies = w.query_filtered::<&mut Camera, With<crate::map::SkyboxCamera>>();
    for mut camera in skies.iter_mut(w) {
        camera.is_active = false;
    }
    crate::core::new_game(w);
}

/// The greybox map's name for `map` and `--map`.
pub const GREYBOX: &str = "greybox";

/// What `map <name>` and `changelevel <name>` load: None for the greybox
/// (`greybox`, `chimera:greybox`), else the map's id and whether to check
/// it against the install's list. A map of the default game (`game`) by
/// its name (`de_dust2`) or its id (`cs_source:de_dust2`, as the command
/// line and `--map` take it); another game's id as given (a bare name
/// too, without a default game).
pub fn map_target(game: Option<&GameInfo>, name: &str) -> Option<(String, bool)> {
    if crate::net::normalize_map(name) == crate::net::GREYBOX {
        return None;
    }
    match (name.split_once(':'), game) {
        (Some((id, map)), Some(game)) if id.eq_ignore_ascii_case(game.id) => Some((game.map_id(map), true)),
        (None, Some(game)) => Some((game.map_id(name), true)),
        _ => Some((name.to_string(), false)),
    }
}

/// The player a command acts on (a network client's on its server).
pub(super) fn local_player(w: &mut World) -> Result<Entity, String> {
    crate::console::command_player(w)
}

/// The eye and the feet relative to a character's origin
/// (`MovementState::eye_offset`, `hull_min`). As CS:S's, `getpos` prints
/// the view's origin (the eye; noclip keeps the offset it was entered
/// with) and `setpos` takes the player's origin, Source's: the feet. So a
/// pasted `setpos` line puts the camera where CS:S's was (64 units above
/// the numbers standing), and `setpos` with `getpos`'s numbers lands one
/// view offset higher, as in CS:S.
pub(super) fn eye_and_feet(w: &World, p: Entity) -> (Vec3, Vec3) {
    w.get::<MovementState>(p)
        .map_or((Vec3::ZERO, Vec3::ZERO), |s| (s.eye_offset, Vec3::Y * s.hull_min.y))
}

/// The client's console commands (noclip, getpos/setpos/setang, god, kill,
/// map, ...), registered by `ConsoleUiPlugin`; public for tests.
pub fn client_commands(app: &mut App) {
    crate::client::binds::commands(app);
    for a in ACTIONS {
        let a = *a;
        for (sign, on) in [("+", true), ("-", false)] {
            app.console_command(
                &format!("{sign}{a}"),
                &format!("{} {a} (for binds).", if on { "Start" } else { "Stop" }),
                move |w, _| {
                    let mut h = w.resource_mut::<HeldActions>();
                    match a {
                        "jump" => h.jump = on,
                        "duck" => h.duck = on,
                        "speed" => h.speed = on,
                        "walk" => h.walk = on,
                        "sprint" => h.sprint = on,
                        "forward" => h.forward = on,
                        "back" => h.back = on,
                        "moveleft" => h.moveleft = on,
                        "moveright" => h.moveright = on,
                        "attack2" => h.attack2 = on,
                        "reload" => h.reload = on,
                        "use" => h.use_key = on,
                        "showscores" => h.showscores = on,
                        "freelook" => h.freelook = on,
                        _ => h.attack = on,
                    }
                    Ok(None)
                },
            );
        }
    }
    // noclip, god, setpos, setang: a dedicated server runs them too (for a
    // client's player).
    crate::net::cheats::player_commands(app);
    app.console_command(
        "chimera_host_if_asked",
        "Host the loaded map for other players when maxplayers asks for them (what map and the menu's \
         Create Server do after a load); the command line's --map runs it after its +commands.",
        |w, _| {
            crate::client::net::listen_if_hosting(w);
            Ok(None)
        },
    )
    .console_command(
        "getpos",
        "Your position and view, as a setpos/setang line (CS:S units).",
        |w, _| {
            let p = local_player(w)?;
            let t = *w.get::<Transform>(p).ok_or("no transform")?;
            // The view's origin, as CS:S prints it: in third person the
            // camera's (a CS:S capture's getpos was its camera).
            let third = w.get_resource::<crate::client::view::CameraMode>().is_some_and(|m| m.third_person);
            let camera = third
                .then(|| {
                    let mut q = w.query_filtered::<&GlobalTransform, With<FirstPersonCamera>>();
                    q.iter(w).next().map(|g| g.translation())
                })
                .flatten();
            let eye = camera.unwrap_or(t.translation + eye_and_feet(w, p).0);
            let i = w.get::<crate::core::Intent>(p).ok_or("no intent")?;
            let s = Vec3::new(eye.x, -eye.z, eye.y) / 0.0254;
            Ok(Some(format!(
                "setpos {:.2} {:.2} {:.2};setang {:.2} {:.2} 0.00",
                s.x,
                s.y,
                s.z,
                -i.pitch.to_degrees(),
                (i.yaw.to_degrees() + 90.0).rem_euclid(360.0)
            )))
        },
    )
    .console_command(
        "chimera_hurtme",
        "chimera_hurtme <head|chest|stomach|leftarm|rightarm|leftleg|rightleg|generic> [amount=100] [from yaw, degrees: 0 = shot from the front] [attacker name]: a bullet hit on yourself at that hitbox (e.g. to see your ragdoll in thirdperson; killed by a named player, its death cam and freeze cam).",
        |w, a| {
            use crate::core::{Damage, DamageKind, Health, Hitboxes, Hitgroup, Intent};
            let group = match a.first().map(|s| s.to_ascii_lowercase()).as_deref() {
                Some("head") => Hitgroup::Head,
                Some("chest") => Hitgroup::Chest,
                Some("stomach") => Hitgroup::Stomach,
                Some("leftarm") => Hitgroup::LeftArm,
                Some("rightarm") => Hitgroup::RightArm,
                Some("leftleg") => Hitgroup::LeftLeg,
                Some("rightleg") => Hitgroup::RightLeg,
                Some("generic") => Hitgroup::Generic,
                _ => return Err("chimera_hurtme <hitgroup> [amount] [from yaw] [attacker]".into()),
            };
            let attacker = match a.get(3) {
                Some(name) => {
                    let mut q = w.query::<(Entity, &Name)>();
                    let found = q
                        .iter(w)
                        .find(|(_, n)| n.as_str().eq_ignore_ascii_case(name))
                        .map(|(e, _)| e);
                    Some(found.ok_or_else(|| format!("nobody is called {name}"))?)
                }
                None => None,
            };
            let amount = a.get(1).and_then(|v| v.parse::<f32>().ok()).unwrap_or(100.0) / 100.0;
            let from = a.get(2).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0).to_radians();
            let p = local_player(w)?;
            let t = *w.get::<Transform>(p).ok_or("no transform")?;
            let yaw = w.get::<Intent>(p).ok_or("no intent")?.yaw_rotation();
            let feet = w
                .get::<avian3d::prelude::ColliderAabb>(p)
                .map_or(t.translation, |b| t.translation.with_y(b.min.y));
            let point = w
                .get::<Hitboxes>(p)
                .and_then(|h| h.0.iter().find(|b| b.group() == group).map(|b| feet + yaw * b.center))
                .unwrap_or(t.translation);
            // Shot from `from` around the facing: travelling toward it.
            let dir = yaw * Quat::from_rotation_y(from) * Vec3::Z;
            let _ = w.get::<Health>(p).ok_or("no health")?;
            w.write_message(Damage {
                force: bevy::math::Vec3::ZERO,
                target: p,
                attacker,
                amount,
                point,
                dir,
                hitgroup: group,
                kind: DamageKind::Bullet,
                weapon: None,
            });
            Ok(None)
        },
    )
    .console_command(
        "maps",
        "maps [filter]: list the maps that can be loaded (the game's, its downloads, imported).",
        |w, a| {
            let filter = a.first().map(|f| f.to_lowercase()).unwrap_or_default();
            let Some(game) = crate::game_registry::default_game(w) else {
                return Err("no game registered".into());
            };
            let lines: Vec<String> = map_names(Some(game))
                .into_iter()
                .filter(|m| m.contains(&filter))
                .map(|m| {
                    let from = map_source(game, &m);
                    format!(
                        "{m}{}",
                        if from == "game" {
                            String::new()
                        } else {
                            format!("  ({from})")
                        }
                    )
                })
                .collect();
            Ok(Some(if lines.is_empty() {
                "no maps match".into()
            } else {
                format!("{}\n{} maps", lines.join("\n"), lines.len())
            }))
        },
    )
    .console_command(
        "import",
        "import <file.bsp|file.bsp.bz2>: copy a map into Chimera's content cache (never the game folder).",
        |w, a| {
            let path = a.join(" ");
            if path.is_empty() {
                return Err("import <file.bsp|file.bsp.bz2>".into());
            }
            let game = crate::game_registry::default_game(w).ok_or("no game registered")?;
            let name = (game.import_map)(std::path::Path::new(&path))?;
            *MAP_NAMES.lock().unwrap_or_else(|e| e.into_inner()) = None;
            Ok(Some(format!("imported {name}; load it with: map {name}")))
        },
    )
    .console_command(
        "map",
        "map <name>: load a map of the game (Tab lists the install's maps); map greybox: Chimera's test map. \
         Hosting, it takes the players along (as changelevel); on a client, it leaves the server first.",
        |w, a| {
            let name = a.first().ok_or("map <name>")?.clone();
            if w.get_resource::<crate::core::NetRole>() == Some(&crate::core::NetRole::Client) {
                crate::net::disconnect(w, crate::net::client::BY_USER);
            }
            let game = crate::game_registry::default_game(w);
            let Some((id, check)) = map_target(game, &name) else {
                w.resource_mut::<Console>().submit("bot_kick");
                load_greybox(w);
                crate::client::front_end::entered_game(w);
                crate::client::net::listen_if_hosting(w);
                crate::client::front_end::start_game(w);
                return Ok(Some("loaded the greybox".into()));
            };
            let short = crate::net::maps::map_name(&id).to_string();
            let names = map_names(game);
            if check && !names.is_empty() && !names.iter().any(|m| *m == short) {
                return Err(format!("no map \"{short}\" in the install"));
            }
            start_map_load(w, &id, None);
            // A game of our own starts: we join it once it's loaded.
            w.insert_resource(FreshGame);
            Ok(Some(format!("loading {id}...")))
        },
    )
    .console_command(
        "changelevel",
        "changelevel <name>: the server moves to another map; connected players load it and play on there \
         (bots stay).",
        |w, a| {
            let name = a.first().ok_or("changelevel <name>")?.clone();
            if w.get_resource::<crate::core::NetRole>() != Some(&crate::core::NetRole::Server) {
                // Source's words.
                return Err("Can't changelevel, not running server.".into());
            }
            let game = crate::game_registry::default_game(w);
            let Some((id, check)) = map_target(game, &name) else {
                load_greybox(w);
                crate::client::front_end::entered_game(w);
                return Ok(Some("changed level to the greybox".into()));
            };
            let short = crate::net::maps::map_name(&id).to_string();
            let names = map_names(game);
            if check && !names.is_empty() && !names.iter().any(|m| *m == short) {
                return Err(format!("no map \"{short}\" in the install"));
            }
            start_map_load(w, &id, None);
            Ok(Some(format!("changing level to {id}...")))
        },
    )
    .console_command(
        "disconnect",
        "Leave the game (or stop hosting it): bots kicked, the map unloaded, back to the main menu.",
        |w, _| {
            crate::net::disconnect(w, "Disconnect by user.");
            w.resource_mut::<Console>().submit("bot_kick");
            load_greybox(w);
            crate::client::front_end::left_game(w);
            Ok(None)
        },
    )
    .console_command("toggleconsole", "Open or close the console.", |w, _| {
        w.resource_mut::<ConsoleUi>().toggle_request = true;
        Ok(None)
    })
    .console_command(
        "quit",
        "quit [prompt]: quit (binds and changed cvars are saved); prompt (CS:S's F10): open the game menu, \
         whose Quit quits (no quit dialog yet).",
        |w, a| {
            if a.first().is_some_and(|x| x.eq_ignore_ascii_case("prompt")) {
                w.resource_mut::<Console>().submit("menu main");
                return Ok(None);
            }
            crate::net::disconnect(w, "Quit.");
            w.write_message(AppExit::Success);
            Ok(None)
        },
    )
    .console_command("exit", "Quit.", |w, _| {
        crate::net::disconnect(w, "Quit.");
        w.write_message(AppExit::Success);
        Ok(None)
    })
    .console_command(
        "con_input",
        "con_input \"<text>\" [n]: open the console with text on its input line and its nth suggestion picked \
         (1: the first), as typing and Down would (for screenshots of the console).",
        |w, a| {
            let text = a.first().cloned().unwrap_or_default();
            let pick = a.get(1).and_then(|n| n.parse::<usize>().ok()).filter(|n| *n > 0);
            let mut ui = w.resource_mut::<ConsoleUi>();
            ui.open = true;
            ui.cursor = text.chars().count();
            ui.input = text;
            ui.highlight = pick.map(|n| n - 1);
            ui.browsing = None;
            ui.dismissed = None;
            ui.cycle = None;
            let mut roots = w.query_filtered::<&mut Visibility, With<ConsoleRoot>>();
            for mut v in roots.iter_mut(w) {
                *v = Visibility::Visible;
            }
            Ok(None)
        },
    )
    .console_command(
        "con_filter",
        "con_filter [text]: show only lines containing text (none: all).",
        |w, a| {
            w.resource_mut::<ConsoleUi>().filter = a.join(" ");
            Ok(None)
        },
    )
    .console_command(
        "con_dump",
        "con_dump [file]: write the console output to the cfg folder (condump.txt).",
        |w, a| {
            let name = a.first().map_or("condump.txt", |s| s.as_str());
            let text: Vec<String> = w.resource::<Console>().output.iter().map(|l| l.text.clone()).collect();
            let path = cfg_dir().ok_or("no config folder")?.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
            std::fs::write(&path, text.join("\n")).map_err(|e| e.to_string())?;
            Ok(Some(format!("wrote {}", path.display())))
        },
    )
    .console_command(
        "con_copy",
        "con_copy [lines]: copy the console output (or its last lines) to the clipboard.",
        |w, a| {
            let output = &w.resource::<Console>().output;
            let n = match a.first() {
                Some(n) => n.parse::<usize>().map_err(|_| format!("bad line count \"{n}\""))?,
                None => output.len(),
            };
            let text: Vec<String> = output.iter().skip(output.len().saturating_sub(n)).map(|l| l.text.clone()).collect();
            let count = text.len();
            arboard::Clipboard::new()
                .and_then(|mut c| c.set_text(text.join("\n")))
                .map_err(|e| format!("clipboard: {e}"))?;
            Ok(Some(format!("copied {count} lines")))
        },
    );
}

/// Remote protocol method `chimera/console` (and its old name, `mashup/console`): params `{"line": "..."}` runs
/// the line now and returns the output lines it printed.
#[cfg(feature = "dev")]
pub fn remote_exec(
    bevy::ecs::system::In(params): bevy::ecs::system::In<Option<serde_json::Value>>,
    world: &mut World,
) -> bevy::remote::BrpResult {
    let line = params
        .as_ref()
        .and_then(|p| p.get("line"))
        .and_then(|l| l.as_str())
        .ok_or(bevy::remote::BrpError {
            code: bevy::remote::error_codes::INVALID_PARAMS,
            message: "expected {\"line\": \"command\"}".into(),
            data: None,
        })?
        .to_string();
    let start = world.resource::<Console>().printed;
    {
        let mut c = world.resource_mut::<Console>();
        c.print(Level::Input, format!("] {line}"));
        c.submit(line);
    }
    crate::console::run_queue(world);
    // The lines printed since (the output keeps only the last ones, and
    // logs print into it too).
    let c = world.resource::<Console>();
    let new = ((c.printed - start).saturating_sub(1) as usize).min(c.output.len());
    let out: Vec<String> = c.output[c.output.len() - new..]
        .iter()
        .map(|l| l.text.clone())
        .collect();
    Ok(serde_json::json!({ "output": out }))
}


