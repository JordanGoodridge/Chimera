//! The client's side of Lucker Party (feature `present`): the shell with
//! this app's hooks and Combat Arms' HUD (`ClientPlugin`), and the command
//! line as the app plays it (`with_defaults`: the default minigame and its
//! first map).

use bevy::prelude::*;
use chimera_client::client::{AppHooks, Args, ClientPlugins};
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
        ));
    }
}

/// The command line as the app plays it: `DEFAULT_MINIGAME` without
/// `+minigame`, and without `--map` that minigame's first map (a minigame
/// that isn't built in keeps the default's).
pub fn with_defaults(mut args: Args) -> Args {
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
        assert_eq!(args.console, ["minigame originals:gun_game_dust2"]);
        let mut own = Args::default();
        own.map = Some("combat_arms:warehouse".into());
        own.console.push("minigame originals:gun_game_dust2".into());
        let own = super::with_defaults(own);
        assert_eq!(own.map.as_deref(), Some("combat_arms:warehouse"));
        assert_eq!(own.console.len(), 1);
        assert_eq!(super::client_hooks().build, crate::BUILD);
    }
}
