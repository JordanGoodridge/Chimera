//! The client's side of Lucker Party (feature `present`): the shell with
//! this app's hooks and Combat Arms' HUD (`ClientPlugin`; CS:S's body
//! animation for a player holding a CS:S gun), and the command line as the
//! app plays it (`with_defaults`: the default minigame and its first map).

use bevy::prelude::*;
use chimera_client::client::{AppHooks, Args, ClientPlugins};
use chimera_core::game_registry::Games;
use combat_arms::ui::hud::ItemNames;
use chimera_world::greybox;

/// What this app tells the client shell: its commit, its maps
/// (`MAP_SOURCES`) and the greybox's own hooks.
pub fn client_hooks() -> AppHooks {
    AppHooks {
        build: super::BUILD,
        load_map: super::load_map,
        load_map_file: |id, _, _| Err(format!("{id}: Lucker Party loads maps by id only, not from a file")),
        map_file_bytes: |_| None,
        unload_greybox: greybox::unload,
        respawn_greybox: greybox::respawn,
    }
}

/// The client: the shell's plugins with the app's hooks and Combat Arms'
/// HUD.
pub struct ClientPlugin {
    pub args: Args,
}

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(combat_arms::ui::with_hud(
            ClientPlugins {
                args: self.args.clone(),
                hooks: client_hooks(),
            }
            .build(),
        ))
        // What CS:S's bodies play, for a player holding a CS:S gun (the
        // gun game's `identity = "weapon"`).
        .add_plugins(cs_source::game::player_anim::PlayerAnimPlugin)
        .add_systems(Startup, cs_source_names);
    }
}

/// CS:S's guns' names for Combat Arms' HUD: its weapon panel and kill feed
/// call a weapon by `ItemNames` (else its data's `hud.name`, which for CS:S
/// is a string token), so the names come from CS:S's own strings in its
/// install (`weapons::display_names`; none without the install, the guns
/// then show their ids).
fn cs_source_names(games: Res<Games>, mut names: ResMut<ItemNames>) {
    let Some(mount) = games.get(cs_source::game::GAME).and_then(|g| g.mount()) else {
        return;
    };
    names.0.extend(cs_source::game::weapons::display_names(&mount));
}

/// The command line as the app plays it: `DEFAULT_MINIGAME` without
/// `+minigame`, and without `--map` that minigame's first map (a minigame
/// that isn't built in keeps the default's); `DEFAULT_BOTS` bots unless it
/// sets `MaxPlayer`.
pub fn with_defaults(mut args: Args) -> Args {
    if !args.console.iter().any(|c| c.to_lowercase().starts_with("maxplayer")) {
        args.console.insert(0, format!("MaxPlayer {}", super::DEFAULT_BOTS));
    }
    let minigame = match super::minigame_of(&args.console) {
        Some(id) => id,
        None => {
            args.console.push(format!("minigame {}", super::DEFAULT_MINIGAME));
            super::DEFAULT_MINIGAME.to_string()
        }
    };
    if args.map.is_none() {
        let map = super::first_map(&minigame).or_else(|_| super::first_map(super::DEFAULT_MINIGAME));
        args.map = map.ok();
    }
    args
}

#[cfg(test)]
mod tests {
    use chimera_client::client::Args;

    #[test]
    fn it_starts_the_gun_game_on_its_first_map() {
        let args = super::with_defaults(Args::default());
        assert_eq!(args.map.as_deref(), Some("cs_source:de_dust2"));
        assert_eq!(args.console, ["MaxPlayer 3", "minigame originals:gun_game_dust2"]);
        let mut own = Args::default();
        own.map = Some("combat_arms:warehouse".into());
        own.console.push("minigame originals:gun_game_dust2".into());
        own.console.push("maxplayer 0".into());
        let own = super::with_defaults(own);
        assert_eq!(own.map.as_deref(), Some("combat_arms:warehouse"));
        assert_eq!(own.console.len(), 2);
        assert_eq!(super::client_hooks().build, crate::BUILD);
    }
}
