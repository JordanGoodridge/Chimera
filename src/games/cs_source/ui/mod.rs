//! CS:S's own front end, drawn from the player's install: the GameUI main
//! and Esc menus and their dialogs (`game_menu`, `options`, `first_run`,
//! `server_browser`), the join flow (`join_menus`: the message of the day,
//! team and class menus), the buy, team and radio menus, the game HUD
//! (`objectives_hud`, `bomb_fx`, `target_id`), the spectator menu and freeze
//! cam panel, the
//! scoreboard and the window icon. Built on the client
//! shell (`client`: input, console, fonts, the VGUI-like widgets); the
//! shell reaches these only through `client::front_end`'s hooks, and the
//! app adds them (`CsSourceUiPlugin`). The simulation side of `cs_source`
//! never uses this module.

pub mod bomb_fx;
pub mod buy_menu;
pub mod debug;
pub mod first_run;
pub mod freeze_cam;
pub mod game_menu;
pub mod join_menus;
pub mod objectives_hud;
pub mod options;
pub mod radio;
pub mod scoreboard;
pub mod server_browser;
pub mod spectator_menu;
pub mod target_id;
pub mod team_menu;
pub mod window_icon;

use bevy::prelude::*;

/// CS:S's menus, panels and HUD for a windowed game (after
/// `client::ClientPlugin`).
pub struct CsSourceUiPlugin;

impl Plugin for CsSourceUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            scoreboard::ScoreboardPlugin,
            buy_menu::BuyMenuPlugin,
            team_menu::TeamMenuPlugin,
            radio::RadioPlugin,
            objectives_hud::ObjectivesHudPlugin,
            bomb_fx::BombFxPlugin,
            game_menu::GameMenuPlugin,
            window_icon::WindowIconPlugin,
            server_browser::ServerBrowserPlugin,
            first_run::FirstRunPlugin,
            target_id::TargetIdPlugin,
            join_menus::JoinMenusPlugin,
            spectator_menu::SpectatorMenuPlugin,
            freeze_cam::FreezeCamPlugin,
            debug::BotDebugPlugin,
        ));
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        client::binds::{self, SAMPLE_CONFIG_DEFAULT, cfg_binds, default_binds, defaults_from},
        games::cs_source::{DEFAULT_BINDS, INFO},
    };

    fn owned(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn the_default_binds_are_config_default_cfgs() {
        // The built-in table is the file's binds, in its order (Esc and `
        // aren't binds).
        assert_eq!(cfg_binds(SAMPLE_CONFIG_DEFAULT), owned(DEFAULT_BINDS));
        // Only with CS:S installed (MASHUP_CONFIG or a Steam library): the
        // install's file says the same.
        if binds::read_install_defaults(Some(&INFO)) {
            assert_eq!(default_binds(), defaults_from(&owned(DEFAULT_BINDS)));
        }
    }
}
