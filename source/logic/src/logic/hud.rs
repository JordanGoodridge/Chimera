//! What map logic shows on players' HUDs (`HudShow`, `show_on_hud`): HUD
//! messages (game_text), screen fades, hints and shakes, whose data is
//! `map::logic_view`'s (the client draws it: `client::game_text`,
//! `client::map_screen`, `client::senses`).

use bevy::prelude::*;

// What the logic shows on screen as plain data (`map::logic_view`: the
// client shell draws it without naming the logic), at its old paths.
pub use crate::map::logic_view::{
    FADE_TIME_UNIT, HUD_CHANNELS, HUD_TEXT_MAX, HudMessage, HudMessages, MapShake, ScreenFade, ScreenFades,
};

// A map's `say` line: `map::MapSay` (the network and the client read
// it without naming the logic), at its old path.
pub use crate::map::MapSay;

/// What map logic shows on one player's HUD (`to`) or everyone's (None):
/// the bridge shows it for the local player, and a network server sends
/// it to the client the player belongs to.
#[derive(Message, Clone, Debug, PartialEq)]
pub struct HudEvent {
    pub to: Option<Entity>,
    pub what: HudShow,
}

/// One thing a `HudEvent` shows.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum HudShow {
    Text(HudMessage),
    Fade(ScreenFade),
    /// The key hint panel's text (env_hudhint); empty hides it.
    Hint(String),
    /// A full-screen overlay material (env_screenoverlay) for `secs`
    /// seconds (0 or less: until replaced); an empty name takes it away.
    Overlay {
        material: String,
        secs: f32,
    },
}

/// Show a `HudShow` on this client's HUD at `now` (`Time` elapsed).
pub fn show_on_hud(world: &mut World, what: HudShow, now: f64) {
    match what {
        HudShow::Text(m) => world.get_resource_or_init::<HudMessages>().show(m, now),
        HudShow::Fade(f) => world.get_resource_or_init::<ScreenFades>().show(f, now),
        HudShow::Hint(text) => world.get_resource_or_init::<ScreenFades>().hint = Some((text, now)),
        HudShow::Overlay { material, secs } => {
            world.get_resource_or_init::<ScreenFades>().overlay =
                (!material.is_empty()).then(|| (material.to_ascii_lowercase(), now, secs));
        }
    }
}
