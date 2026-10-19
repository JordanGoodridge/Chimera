//! Combat Arms' front end on the client shell (the `ui` feature), built
//! the game's way (specs/combat_arms/ui_system.md) on Chimera's reference
//! canvas (`client::canvas`):
//!
//! - `layout`: widget trees from the game's layout format (`CBALAYOUT`,
//!   `layout::xml`), placed and scaled by its rules (HUD panels stretched
//!   by x_r, y_r; menus and popups 1:1), fonts scaled with both factors,
//!   texts aligned in their parent's box, string tables (`layout::text`).
//! - `desk`: screens and popups as the pointer sees them (draw order,
//!   modal capture, hover and click sounds, buttons firing on press).
//! - `draw`: a laid-out tree as the shell's UI nodes (the install's font
//!   faces, stand-ins for the game's keyed art).
//! - `hud` (crosshair and basic HUD, hud_crosshair.md): its texts placed
//!   and sized by `layout`, with the game mode's score panel and round
//!   timer (`hud::score`); `scoreboard`: the scoreboard (Tab, and a
//!   round's result); `menu`: the in-game menu (Esc) in place of the
//!   shell's default; `chat`: the match chat's queue (the model);
//!   `visuals`: placeholder looks (box guns); `effects`: decals, tracers,
//!   casings, the first-person muzzle flash and effect lights
//!   (game::effects).
//!
//! `with_hud` puts Combat Arms' HUD among the shell's plugins (after its
//! fallback HUD, `client::hud::HudPlugin`) and the shell's weapon
//! selection (`client::weapon_menu`, as CS:S's: `slot1`..`slot5`,
//! `invnext` / `invprev`, here with no HUD row, so they switch at once;
//! see `game::inventory`), as CS:S's `with_hud` does; `CaUiPlugin` adds
//! the rest after the shell: the in-game menu, the scoreboard and the
//! visuals.
//!
//! `CaUi` is the UI's data: our font list (`data/ui/fonts.toml`), string
//! table (`data/ui/strings.txt`) and stand-in textures.
//!
//! Known shortcuts: no other screens (lobby, shop, result screen: the
//! game has them as menus at 1024 x 768; no loading screen, kill cam or
//! vote panel yet); the HUD's panels keep their places as
//! code (`hud::model`, from hud_crosshair.md) rather than binding named
//! controls of the game's HUD layout, which sits in a keyed archive.

pub mod chat;
pub mod desk;
pub mod draw;
pub mod effects;
pub mod hud;
pub mod layout;
pub mod menu;
pub mod scoreboard;
pub mod visuals;

use bevy::{app::PluginGroupBuilder, prelude::*};

use draw::StandIns;
use layout::{Ctx, FontList, Strings};

/// The shell's plugins with Combat Arms' HUD in its place, and the
/// shell's weapon selection where CS:S has its own (after the views).
pub fn with_hud(shell: PluginGroupBuilder) -> PluginGroupBuilder {
    shell
        // Combat Arms draws its own shot effects (`effects`).
        .disable::<crate::client::effects::ShotEffectsPlugin>()
        .add_after::<crate::client::hud::HudPlugin>(hud::CaHudPlugin)
        .add_after::<crate::client::view::ViewPlugin>(crate::client::weapon_menu::WeaponMenuPlugin)
}

/// Combat Arms' front end but its HUD: the in-game menu and the
/// placeholder visuals.
pub struct CaUiPlugin;

impl Plugin for CaUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            visuals::CaVisualsPlugin,
            effects::CaEffectsDrawPlugin,
            menu::CaMenuPlugin,
            scoreboard::CaScoreboardPlugin,
        ));
    }
}

/// Combat Arms' UI data: the font list, the string table and the
/// stand-in textures.
#[derive(Resource, Clone, Debug)]
pub struct CaUi {
    pub fonts: FontList,
    pub strings: Strings,
    pub stand_ins: StandIns,
}

impl Default for CaUi {
    fn default() -> Self {
        let mut stand_ins = StandIns::default();
        let mut add = |name: &str, size: (i32, i32), c: [u8; 4]| {
            stand_ins.0.insert(name.to_string(), (IVec2::new(size.0, size.1), c));
        };
        // PLACEHOLDERS for the in-game menu's art: flat boxes.
        add("igm_back", (220, 145), [22, 26, 30, 235]);
        add("igm_button_A", (196, 24), [52, 58, 64, 255]);
        add("igm_button_B", (196, 24), [84, 92, 100, 255]);
        add("igm_button_C", (196, 24), [120, 96, 30, 255]);
        add("igm_button_D", (196, 24), [38, 40, 44, 200]);
        Self {
            fonts: FontList::ours(),
            strings: Strings::ours(),
            stand_ins,
        }
    }
}

impl CaUi {
    pub fn ctx(&self) -> Ctx<'_, StandIns> {
        Ctx {
            fonts: &self.fonts,
            strings: &self.strings,
            textures: &self.stand_ins,
        }
    }
}
