//! CS:S's own front end, drawn from the player's install: the GameUI main
//! and Esc menus and their dialogs (`game_menu`, `options`, `first_run`,
//! `server_browser`), the join flow (`join_menus`: the message of the day,
//! team and class menus), the buy, team and radio menus, the game HUD
//! (`objectives_hud`, `bomb_fx`, `target_id`, `voice_hud`), the spectator
//! menu and freeze cam panel, the
//! scoreboard, the window icon and `playdemo` (`demo`). Built on the client
//! shell (`client`: input, console, fonts, the VGUI-like widgets); the
//! shell reaches these only through `client::front_end`'s hooks, and the
//! app adds them (`CsSourceUiPlugin`). The simulation side of `cs_source`
//! never uses this module.

pub mod bomb_fx;
pub mod buy_menu;
pub mod debug;
pub mod demo;
pub mod first_run;
pub mod freeze_cam;
pub mod game_hud;
pub mod game_menu;
pub mod hud_sprites;
pub mod join_menus;
pub mod objectives_hud;
pub mod options;
pub mod overview;
pub mod radar;
pub mod radio;
pub mod scoreboard;
pub mod server_browser;
pub mod spectator_hud;
pub mod spectator_menu;
pub mod target_id;
pub mod team_menu;
pub mod voice_hud;
pub mod weapon_select;
pub mod win_panel;
pub mod window_icon;

use bevy::{app::PluginGroupBuilder, prelude::*};

use crate::client;

/// The client shell's plugins (`client::ClientPlugins`) with CS:S's HUD
/// among them, each where it was when the HUD was the shell's (system
/// order kept): the HUD panels and sprites after the fallback HUD, the
/// radar after the map's HUD text, the weapon selection after the camera
/// views, the spectator's HUD after spectating, the overview after the
/// gamma, the bots' debug views after the shell's.
pub fn with_hud(shell: PluginGroupBuilder) -> PluginGroupBuilder {
    shell
        .add_after::<client::debug_views::DebugViewsPlugin>(debug::BotDebugPlugin)
        .add_after::<client::hud::HudPlugin>(game_hud::GameHudPlugin)
        .add_after::<game_hud::GameHudPlugin>(hud_sprites::HudSpritesPlugin)
        .add_after::<client::game_text::GameTextPlugin>(radar::RadarPlugin)
        .add_after::<client::view::ViewPlugin>(weapon_select::WeaponSelectPlugin)
        .add_after::<client::spectate::SpectatePlugin>(spectator_hud::SpectatorHudPlugin)
        .add_after::<client::gamma::GammaPlugin>(overview::OverviewPlugin)
}

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
            demo::DemoUiPlugin,
        ));
        app.add_plugins(voice_hud::VoiceHudPlugin);
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        client::binds::{self, SAMPLE_CONFIG_DEFAULT, cfg_binds, default_binds, defaults_from},
        game::{DEFAULT_BINDS, INFO},
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
