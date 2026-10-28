//! What Combat Arms lends a game made of several
//! (`chimera_core::game_parts`; docs/plans/active/ultimate-gun-game.md):
//! its guns by class and its knife (`weapons`, data/weapons/), its
//! movement (`movement::ID`), its soldiers (`characters`), its HUD
//! (`ui`, the composing app's), its maps and its announcer pack
//! (data/announcer.toml). The app registers it
//! (`register_parts(&PARTS)`).
//!
//! Known shortcuts: the announcer pack has no weapon skins yet (which
//! weapon wears which, `HUDEffectOverride` in the install's weapon list,
//! isn't read: kill_streaks.md B7).

use chimera_core::game_parts::{GameParts, GunClass, GunPart};

const fn gun(id: &'static str, class: GunClass) -> GunPart {
    GunPart { id, class }
}

/// Combat Arms' parts.
pub static PARTS: GameParts = GameParts {
    game: super::GAME,
    guns: &[
        gun("combat_arms:beretta_m92fs", GunClass::Pistol),
        gun("combat_arms:mp5a4", GunClass::Smg),
        gun("combat_arms:remington_870", GunClass::Shotgun),
        gun("combat_arms:m4a1", GunClass::Rifle),
        gun("combat_arms:m60", GunClass::Heavy),
        gun("combat_arms:dragunov_svd", GunClass::Sniper),
        gun("combat_arms:l96a1", GunClass::Sniper),
    ],
    knife: Some("combat_arms:m9_knife"),
    movement: Some(super::movement::ID),
    characters: Some("combat_arms:soldier"),
    hud: Some(super::GAME),
    maps: &["combat_arms:warehouse"],
    announcer: Some(ANNOUNCER),
    // Its result screen's win sound (`ResultWinSound`, music_ambience.md
    // section 4) is a file the spec doesn't name: none yet.
    match_won_sound: None,
};

/// Combat Arms' announcer pack (specs/combat_arms/kill_streaks.md Part B;
/// `chimera_core::announcer`): its streak steps, special kills, banner
/// layout and timings, the files' paths in the install.
pub const ANNOUNCER: &str = include_str!("../../data/announcer.toml");
