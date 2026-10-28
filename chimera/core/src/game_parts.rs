//! What each game can lend a game made of several (docs/plans/active/
//! ultimate-gun-game.md, "The game parts contract"): a game's own claim
//! of its parts (`GameParts`: guns with their class and its knife, its
//! movement, its character kit, its HUD, its maps, its announcer pack, its minigame cue pack), registered by the app
//! that composes the game (`GamePartsAppExt::register_parts`) into
//! `PartsRegistry`, in order. Plain data, no game named: a game fills its
//! own `GameParts` (`cs_source::game::parts::PARTS`,
//! `combat_arms::game::parts::PARTS`), an app reads them all.
//!
//! The claim is checked against what the app really composed by
//! `chimera_sim::minigame::auto::report` (each gun in the weapon
//! catalogue, the movement registered): that report decides what a game
//! may take part in. Required for a game to be in an automatic ladder:
//! guns and movement; optional, with fallbacks (`PartsRegistry::
//! fallback_hud`, the shell's capsule body, other games' maps): its
//! characters, HUD and maps.

use bevy::prelude::*;

/// A gun's class: the order an automatic ladder climbs (weakest first).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GunClass {
    Pistol,
    Smg,
    Shotgun,
    Rifle,
    Heavy,
    Sniper,
}

impl GunClass {
    /// Every class, in climbing order.
    pub const ALL: [GunClass; 6] = [
        GunClass::Pistol,
        GunClass::Smg,
        GunClass::Shotgun,
        GunClass::Rifle,
        GunClass::Heavy,
        GunClass::Sniper,
    ];
}

/// One gun a game lends: its weapon id (`game:name`, in the weapon
/// catalogue) and its class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GunPart {
    pub id: &'static str,
    pub class: GunClass,
}

/// A game's parts, as the game itself claims them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameParts {
    /// The game's id (`GameInfo::id`; a stand-in may have no `GameInfo`).
    pub game: &'static str,
    /// Its guns (weapon set, held and view models, sounds), any order.
    pub guns: &'static [GunPart],
    /// Its melee weapon, given beside a gun of its game.
    pub knife: Option<&'static str>,
    /// Its movement's id (`slots::MovementRegistry`).
    pub movement: Option<&'static str>,
    /// Its character kit (bodies a map of any game can wear: its own maps'
    /// and, as a guest, others': `MapSources::players`/`guests`).
    pub characters: Option<&'static str>,
    /// Its HUD's id for the gameplay's `hud` slot (its game id).
    pub hud: Option<&'static str>,
    /// Its maps by id (`game:map`) that load in a composed app.
    pub maps: &'static [&'static str],
    /// Its announcer pack, a TOML document (`announcer::AnnouncerPack`),
    /// registered with its parts.
    pub announcer: Option<&'static str>,
    /// Its minigame cue pack, a TOML document (`cues::CuePack`: the
    /// sounds of a gun game's rung ups, last rung, lead and win),
    /// registered with its parts.
    pub cues: Option<&'static str>,
}

impl GameParts {
    /// Guns and movement: what a game needs to be in a ladder.
    pub fn playable(&self) -> bool {
        !self.guns.is_empty() && self.movement.is_some()
    }
}

/// The HUD of a game that has none (the shell's own: crosshair, health,
/// ammo; no game's HUD drawn).
pub const MINIMAL_HUD: &str = "chimera";

/// The registered games' parts, in the order registered, and the HUD a
/// game without one falls back to.
#[derive(Resource, Clone, Debug)]
pub struct PartsRegistry {
    parts: Vec<&'static GameParts>,
    /// For a game whose parts have no HUD: another game's (an app's
    /// choice) or `MINIMAL_HUD`.
    pub fallback_hud: &'static str,
}

impl Default for PartsRegistry {
    fn default() -> Self {
        Self {
            parts: Vec::new(),
            fallback_hud: MINIMAL_HUD,
        }
    }
}

impl PartsRegistry {
    pub fn get(&self, game: &str) -> Option<&'static GameParts> {
        self.parts.iter().copied().find(|p| p.game.eq_ignore_ascii_case(game))
    }

    pub fn all(&self) -> impl Iterator<Item = &'static GameParts> + '_ {
        self.parts.iter().copied()
    }

    /// Add (or replace, by game) `parts`.
    pub fn register(&mut self, parts: &'static GameParts) {
        match self.parts.iter_mut().find(|p| p.game == parts.game) {
            Some(p) => *p = parts,
            None => self.parts.push(parts),
        }
    }

    /// The HUD a player of `game` sees: its own, else `fallback_hud`.
    /// None for a game not registered here.
    pub fn hud_of(&self, game: &str) -> Option<&'static str> {
        self.get(game).map(|p| p.hud.unwrap_or(self.fallback_hud))
    }
}

pub trait GamePartsAppExt {
    /// Register a game's parts (`PartsRegistry`).
    fn register_parts(&mut self, parts: &'static GameParts) -> &mut Self;
}

impl GamePartsAppExt for App {
    fn register_parts(&mut self, parts: &'static GameParts) -> &mut Self {
        self.world_mut().get_resource_or_init::<PartsRegistry>().register(parts);
        if let Some(pack) = parts.announcer {
            crate::announcer::AnnouncerAppExt::register_announcer(self, pack);
        }
        if let Some(pack) = parts.cues {
            crate::cues::CueAppExt::register_cues(self, pack);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static GUNS_ONLY: GameParts = GameParts {
        game: "stand_in",
        guns: &[GunPart {
            id: "stand_in:gun",
            class: GunClass::Rifle,
        }],
        knife: None,
        movement: Some("stand_in:movement"),
        characters: None,
        hud: None,
        maps: &[],
        announcer: None,
        cues: None,
    };

    #[test]
    fn a_game_without_a_hud_gets_the_fallback() {
        let mut r = PartsRegistry::default();
        r.register(&GUNS_ONLY);
        assert!(GUNS_ONLY.playable());
        assert_eq!(r.hud_of("stand_in"), Some(MINIMAL_HUD));
        r.fallback_hud = "other";
        assert_eq!(r.hud_of("STAND_IN"), Some("other"));
        assert_eq!(r.hud_of("nobody"), None);
        r.register(&GUNS_ONLY);
        assert_eq!(r.all().count(), 1, "registered again: replaced");
    }
}
