//! What Counter-Strike: Source lends a game made of several
//! (`chimera_core::game_parts`; docs/plans/active/ultimate-gun-game.md):
//! its guns by class and its knife (`weapons`), its movement
//! (`movement::ID`), its player models (as a guest on other games' maps
//! too: `game::dress_guest`), its HUD (`ui`, the composing app's), the
//! stock maps a composed app loads and a minigame cue pack of stock
//! sounds (data/cues.toml). The app registers it
//! (`register_parts(&PARTS)`).

use chimera_core::game_parts::{GameParts, GunClass, GunPart};

const fn gun(id: &'static str, class: GunClass) -> GunPart {
    GunPart { id, class }
}

/// CS:S's parts.
pub static PARTS: GameParts = GameParts {
    game: super::GAME,
    guns: &[
        gun("cs_source:weapon_glock", GunClass::Pistol),
        gun("cs_source:weapon_usp", GunClass::Pistol),
        gun("cs_source:weapon_p228", GunClass::Pistol),
        gun("cs_source:weapon_deagle", GunClass::Pistol),
        gun("cs_source:weapon_fiveseven", GunClass::Pistol),
        gun("cs_source:weapon_elite", GunClass::Pistol),
        gun("cs_source:weapon_mac10", GunClass::Smg),
        gun("cs_source:weapon_tmp", GunClass::Smg),
        gun("cs_source:weapon_mp5navy", GunClass::Smg),
        gun("cs_source:weapon_ump45", GunClass::Smg),
        gun("cs_source:weapon_p90", GunClass::Smg),
        gun("cs_source:weapon_m3", GunClass::Shotgun),
        gun("cs_source:weapon_xm1014", GunClass::Shotgun),
        gun("cs_source:weapon_galil", GunClass::Rifle),
        gun("cs_source:weapon_famas", GunClass::Rifle),
        gun("cs_source:weapon_ak47", GunClass::Rifle),
        gun("cs_source:weapon_m4a1", GunClass::Rifle),
        gun("cs_source:weapon_sg552", GunClass::Rifle),
        gun("cs_source:weapon_aug", GunClass::Rifle),
        gun("cs_source:weapon_m249", GunClass::Heavy),
        gun("cs_source:weapon_scout", GunClass::Sniper),
        gun("cs_source:weapon_awp", GunClass::Sniper),
        gun("cs_source:weapon_sg550", GunClass::Sniper),
        gun("cs_source:weapon_g3sg1", GunClass::Sniper),
    ],
    knife: Some("cs_source:weapon_knife"),
    movement: Some(super::movement::ID),
    characters: Some("cs_source:players"),
    hud: Some(super::GAME),
    maps: &["cs_source:de_dust2", "cs_source:cs_office"],
    // No announcer of its own (CS:S has none).
    announcer: None,
    cues: Some(CUES),
};

/// CS:S's minigame cue pack (data/cues.toml; `chimera_core::cues`): stock
/// sounds of the install for a gun game's rung ups, last rung, lead and
/// win.
pub const CUES: &str = include_str!("../../data/cues.toml");
