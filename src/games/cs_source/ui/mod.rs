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
        ));
    }
}
