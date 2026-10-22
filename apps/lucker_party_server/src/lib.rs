//! Lucker Party's dedicated server (apps/lucker_party_server, crate
//! `lucker_party_server`): the party's simulation (`lucker_party`'s
//! `PartyPartsPlugin` on Combat Arms' simulation) and the network playing a
//! minigame, with no window, render or audio, as `combat_arms_server` is
//! Combat Arms'. The binary (main.rs) adds the run loop, the log, the
//! console on stdin and its output on stdout; what it serves is
//! `ServerPlugin`, which the tests build the same way (tests/it/).
//!
//!   lucker_party_server [-port <n>] [+minigame <id>] [+map <name>] [+<command> ...]
//!
//! - Minigames: `+minigame <id>` (`originals:gun_game_dust2` without
//!   one), started once the command line's console lines run; the map is
//!   its pool's first unless `+map` names one (`greybox`, `cs_source:<name>`,
//!   `combat_arms:<name>`; a bare name is CS:S's), loaded without textures.
//!   `minigames`, `minigame <id>`, `minigame_status`, `minigame_restart`
//!   on the console.
//! - `changelevel <map>` (or `map <map>`) loads another map in the
//!   background and moves every player there, a fresh match.
//! - Bots: Combat Arms' (`MaxPlayer`, `BotLevel`, `ca_bot_add`), as the
//!   minigame's `bots` slot names them.
//! - Players join with the `lucker_party` client's `+connect <ip>:<port>`;
//!   a server takes sixteen unless told otherwise (`maxplayers`).
//!
//! Lifted nothing: the pieces are Chimera's (`net`, `map::MapSimPlugin`,
//! the minigame runtime), the party's (`lucker_party`) and Combat Arms'
//! (its simulation); `combat_arms_server` has the same shape.
//!
//! Known shortcuts: no `MapFiles` (a client loads the map from its own
//! install by id); the minigame's map pool isn't rotated; a map change
//! keeps the minigame (its match starts over).

use std::time::Duration;

use bevy::prelude::*;
use chimera_core::{
    console::{self, Console, ConsoleAppExt},
    core::{self, BuildVersion},
    gameplay::{Gameplay, GameplayAppExt},
};
use chimera_net::net::{self, NetPlugin, NetSettings};
use chimera_world::{
    greybox::{self, GreyboxMapPlugin},
    map::{self, LoadedMapName, MapData, MapLoadOptions, MapSimPlugin},
};
use combat_arms::game::{CombatArmsPlugins, INFO, TICK_INTERVAL};
use lucker_party::{DEFAULT_MINIGAME, PartyPartsPlugin};

/// This build's version (the banner, `version`): the party's stamp.
pub const VERSION: &str = lucker_party::VERSION;

pub const USAGE: &str = "\
usage: lucker_party_server [-port <n>] [+minigame <id>] [+map <name>] [+<command> [args...]]
  -port <n>          UDP port (hostport; default 27015)
  +minigame <id>     the minigame (default originals:gun_game_dust2); its first map unless +map
  +map <name>        greybox, cs_source:<name> or combat_arms:<name> (a bare name is CS:S's)
  +<command>         any console command or cvar, e.g. +maxplayers 8 +MaxPlayer 3 (bots)
Type console commands on stdin: status, minigames, minigame_status, minigame_restart,
changelevel <map>, quit.";

/// The command line: the minigame, the map, and the console lines to run
/// first (in order; `-port <n>` is `hostport <n>`, first; the minigame's
/// line last).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Options {
    pub minigame: String,
    pub map: String,
    pub console: Vec<String>,
}

/// What `parse` asks of the caller beyond options.
#[derive(Debug, PartialEq)]
pub enum Parsed {
    Run(Options),
    Help,
}

/// The command line (without the program's name).
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Parsed, String> {
    let mut map = None;
    let mut minigame = None;
    let mut console = Vec::new();
    let mut it = args.into_iter().peekable();
    while let Some(arg) = it.next() {
        if arg == "-h" || arg == "--help" {
            return Ok(Parsed::Help);
        }
        if arg == "-port" {
            let port = it.next().ok_or("-port: missing value")?;
            port.parse::<u16>().map_err(|_| format!("-port: not a port: {port}"))?;
            console.insert(0, format!("hostport {port}"));
            continue;
        }
        let Some(name) = arg.strip_prefix('+').filter(|n| !n.is_empty()) else {
            return Err(format!("unknown option {arg}"));
        };
        let mut words = vec![name.to_string()];
        while let Some(next) = it.peek() {
            if next.starts_with('+') && next.len() > 1 || next == "-port" {
                break;
            }
            words.push(it.next().unwrap());
        }
        if name.eq_ignore_ascii_case("map") {
            map = Some(words.get(1).cloned().ok_or("+map: missing name")?);
        } else if name.eq_ignore_ascii_case("minigame") {
            minigame = Some(words.get(1).cloned().ok_or("+minigame: missing id")?);
        } else {
            console.push(words.iter().map(|w| console::quote(w)).collect::<Vec<_>>().join(" "));
        }
    }
    let minigame = minigame.unwrap_or_else(|| DEFAULT_MINIGAME.to_string());
    let map = match map {
        Some(m) => m,
        None => lucker_party::first_map(&minigame)?,
    };
    console.push(format!("minigame {minigame}"));
    Ok(Parsed::Run(Options { minigame, map, console }))
}

/// A map's id as the server serves it: `greybox`, or `game:<name>` (a
/// bare name is CS:S's).
pub fn map_id(name: &str) -> String {
    let id = net::normalize_map(name);
    if id == net::GREYBOX || id.contains(':') {
        id
    } else {
        format!("cs_source:{id}")
    }
}

/// Map `id` as a server loads it: no textures.
pub fn load_map(id: &str) -> Result<MapData, String> {
    lucker_party::load_map_with(id, &MapLoadOptions::lean())
}

/// The map a server starts on: the greybox, or one loaded from an install.
pub enum StartMap {
    Greybox,
    Loaded { id: String, data: MapData },
}

impl StartMap {
    /// `options.map`, loaded.
    pub fn load(options: &Options) -> Result<Self, String> {
        let id = map_id(&options.map);
        if id == net::GREYBOX {
            return Ok(Self::Greybox);
        }
        let data = load_map(&id).map_err(|e| format!("+map {id}: {e}"))?;
        Ok(Self::Loaded { id, data })
    }
}

/// Everything the server runs above the headless base: Combat Arms'
/// simulation, the party's parts (CS:S's maps, logic and guns, the
/// minigames), the map's systems alone with the start map, the network,
/// the console's commands and the command line's console lines (the
/// minigame's last), submitted to run on the first frame. Listening is
/// `listen`'s.
pub struct ServerPlugin {
    pub map: std::sync::Mutex<Option<StartMap>>,
    pub console: Vec<String>,
}

impl ServerPlugin {
    pub fn new(map: StartMap, options: &Options) -> Self {
        Self {
            map: std::sync::Mutex::new(Some(map)),
            console: options.console.clone(),
        }
    }
}

impl Plugin for ServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(combat_arms::SimPlugins);
        match self.map.lock().unwrap().take() {
            Some(StartMap::Loaded { id, data }) => {
                app.add_plugins(MapSimPlugin::new(data)).insert_resource(LoadedMapName(id));
            }
            _ => {
                app.add_plugins((GreyboxMapPlugin, MapSimPlugin::empty()));
            }
        }
        // Combat Arms' gameplay until the minigame (its console line)
        // takes the rules slot.
        app.set_gameplay(Gameplay::of(&INFO))
            .insert_resource(BuildVersion(VERSION))
            .add_plugins((NetPlugin, CombatArmsPlugins, PartyPartsPlugin))
            .insert_resource(NetSettings {
                maxplayers: 16,
                ..default()
            })
            .add_systems(Update, finish_level_change)
            .console_command(
                "changelevel",
                "changelevel <map>: move the server and its players to another map (greybox, cs_source:<name>, combat_arms:<name>).",
                |w, a| change_level(w, a.first().ok_or("changelevel <map>")?),
            )
            .console_command("map", "map <map>: the same as changelevel on a dedicated server.", |w, a| {
                change_level(w, a.first().ok_or("map <map>")?)
            })
            .console_command("quit", "Stop the server.", quit)
            .console_command("exit", "Stop the server.", quit);
        let mut console = app.world_mut().resource_mut::<Console>();
        for line in &self.console {
            console.submit(line.clone());
        }
    }
}

fn quit(world: &mut World, _: &[String]) -> Result<Option<String>, String> {
    net::disconnect(world, "Server shutting down.");
    world.write_message(AppExit::Success);
    Ok(None)
}

/// Start listening (UDP on `hostport`, `ip`) and say so on the console.
pub fn listen(world: &mut World) -> Result<std::net::SocketAddr, String> {
    let addr = net::server::listen(world)?;
    let max = world.resource::<NetSettings>().maxplayers;
    let map = net::current_map(world);
    let minigame = world
        .get_resource::<chimera_sim::minigame::ActiveMinigame>()
        .map_or("no minigame".to_string(), |m| m.def.id.clone());
    world.resource_mut::<Console>().info(format!(
        "Lucker Party dedicated server {VERSION} ({}): {minigame} on {map}, UDP {addr}, {max} players",
        net::NET_VERSION
    ));
    Ok(addr)
}

/// A map loading in the background for `changelevel`.
#[derive(Resource)]
struct LevelLoad {
    id: String,
    task: bevy::tasks::Task<Result<MapData, String>>,
}

/// `changelevel <map>`: the greybox at once; a map from an install loaded
/// in the background, then swapped in (`finish_level_change`).
pub fn change_level(world: &mut World, name: &str) -> Result<Option<String>, String> {
    let id = map_id(name);
    if id == net::GREYBOX {
        swap_map(world, net::GREYBOX, None);
        return Ok(Some("changed level to the greybox".into()));
    }
    if world.contains_resource::<LevelLoad>() {
        return Err("a map is loading already".into());
    }
    world.insert_resource(LoadedMapName(id.clone()));
    let task = bevy::tasks::AsyncComputeTaskPool::get().spawn({
        let id = id.clone();
        async move { load_map(&id) }
    });
    world.insert_resource(LevelLoad { id: id.clone(), task });
    Ok(Some(format!("changing level to {id}...")))
}

/// The map `id` in place of the one playing (None: the greybox), and a
/// fresh match on it.
pub fn swap_map(world: &mut World, id: &str, data: Option<MapData>) {
    match data {
        Some(data) => {
            greybox::unload(world);
            map::change_map(world, data, map::MapDebugView::Normal);
            world.insert_resource(LoadedMapName(id.to_string()));
        }
        None => {
            map::unload_map(world);
            greybox::unload(world);
            greybox::respawn(world);
            world.remove_resource::<map::ActiveMapLook>();
            world.insert_resource(LoadedMapName(net::GREYBOX.into()));
        }
    }
    let tick = world
        .get_resource::<Gameplay>()
        .map_or(Duration::from_secs_f64(TICK_INTERVAL), |g| g.tick);
    core::set_tick_length(world, tick);
    core::new_game(world);
    if world.contains_resource::<chimera_sim::minigame::ActiveMinigame>() {
        chimera_sim::minigame::restart(world);
    }
}

fn finish_level_change(world: &mut World) {
    let Some(mut load) = world.get_resource_mut::<LevelLoad>() else {
        return;
    };
    let Some(result) = bevy::tasks::block_on(bevy::tasks::poll_once(&mut load.task)) else {
        return;
    };
    let id = load.id.clone();
    world.remove_resource::<LevelLoad>();
    match result {
        Ok(data) => {
            let summary = format!("loaded {id}: {} spawns", data.spawns.len());
            swap_map(world, &id, Some(data));
            world.resource_mut::<Console>().info(summary);
        }
        Err(e) => {
            let back = world
                .get_resource::<map::MapFile>()
                .map(|f| map_id(&f.name))
                .unwrap_or_else(|| net::GREYBOX.into());
            world.insert_resource(LoadedMapName(back));
            world
                .resource_mut::<Console>()
                .print(console::Level::Error, format!("changelevel {id}: {e}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Options {
        match parse(args.iter().map(|s| s.to_string())).unwrap() {
            Parsed::Run(o) => o,
            Parsed::Help => panic!("help"),
        }
    }

    #[test]
    fn the_command_line_names_the_minigame_and_its_map() {
        let o = run(&["+minigame", "originals:gun_game_dust2", "-port", "27040", "+maxplayers", "4"]);
        assert_eq!(o.minigame, "originals:gun_game_dust2");
        assert_eq!(o.map, "cs_source:de_dust2", "the pool's first");
        assert_eq!(o.console, ["hostport 27040", "maxplayers 4", "minigame originals:gun_game_dust2"]);
        let o = run(&["+map", "combat_arms:warehouse"]);
        assert_eq!((o.minigame.as_str(), o.map.as_str()), (DEFAULT_MINIGAME, "combat_arms:warehouse"));
        assert!(parse(["+minigame".to_string(), "originals:nothing".to_string()]).is_err());
        assert!(parse(["nope".to_string()]).is_err());
        assert_eq!(parse(["--help".to_string()]).unwrap(), Parsed::Help);
    }

    #[test]
    fn map_ids() {
        assert_eq!(map_id("de_dust2"), "cs_source:de_dust2");
        assert_eq!(map_id("combat_arms:warehouse"), "combat_arms:warehouse");
        assert_eq!(map_id("chimera:greybox"), "greybox");
    }
}
