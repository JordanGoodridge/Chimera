//! Cue packs: the sounds a minigame's moments play (docs/DESIGN.md,
//! "Announcers"), as data. A pack names, by cue key, a sound file in the
//! install of its `game` (loaded at run time, never shipped): the keys
//! are the minigame's callouts' (`chimera_sim::minigame::hud::Callout::
//! cue`: `rung_up`, `rung_down`, `last_rung`, `last_rung_other`,
//! `lead_taken`, `lead_lost`, `lead_tied`, `match_won`, `match_lost`). A
//! key the pack lacks is silent.
//!
//! ```toml
//! id = "cs_source"
//! name = "Counter-Strike: Source"
//! game = "cs_source"
//! volume = 0.8                 # every cue's gain (default 1)
//! [sounds]
//! rung_up = "sound/items/itempickup.wav"
//! ```
//!
//! A game contributes its pack as one of its parts (`GameParts::cues`),
//! registered in order (`CuePacks`); a minigame definition picks one by
//! id (`cues = "<id>"`), `none`, or by default the first registered. The
//! client shell plays them (`chimera_client::client::callouts`).

use std::{collections::BTreeMap, sync::Arc};

use bevy::prelude::*;
use serde::Deserialize;

fn one() -> f32 {
    1.0
}

/// A cue pack (module doc).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CuePack {
    /// Its id (a minigame's `cues = "<id>"`).
    pub id: String,
    pub name: String,
    /// The game whose install holds its files (`<game>://<path>`).
    pub game: String,
    /// Every cue's gain.
    #[serde(default = "one")]
    pub volume: f32,
    /// The sound file of each cue key.
    #[serde(default)]
    pub sounds: BTreeMap<String, String>,
}

impl CuePack {
    /// A pack from its TOML document.
    pub fn parse(toml: &str) -> Result<Self, String> {
        toml::from_str(toml).map_err(|e| e.to_string())
    }

    /// The sound file of cue `key` (None: silent).
    pub fn sound(&self, key: &str) -> Option<&str> {
        self.sounds.get(key).map(String::as_str)
    }
}

/// The packs the app's games contributed, in the order registered.
#[derive(Resource, Clone, Debug, Default)]
pub struct CuePacks(pub Vec<Arc<CuePack>>);

impl CuePacks {
    /// Add `pack` (one of the same id replaced in place).
    pub fn register(&mut self, pack: CuePack) {
        let pack = Arc::new(pack);
        match self.0.iter_mut().find(|p| p.id == pack.id) {
            Some(p) => *p = pack,
            None => self.0.push(pack),
        }
    }

    pub fn get(&self, id: &str) -> Option<&Arc<CuePack>> {
        self.0.iter().find(|p| p.id.eq_ignore_ascii_case(id))
    }

    /// The pack a minigame's `cues` names: None or `default` the first
    /// registered, `none` none, else the pack of that id.
    pub fn choose(&self, choice: Option<&str>) -> Option<&Arc<CuePack>> {
        match choice {
            None => self.0.first(),
            Some(c) if c.eq_ignore_ascii_case("default") => self.0.first(),
            Some(c) if c.eq_ignore_ascii_case("none") => None,
            Some(id) => self.get(id),
        }
    }

    /// Whether `choice` names a pack there is (or `default`, `none`).
    pub fn valid(&self, choice: &str) -> bool {
        ["default", "none"].iter().any(|c| c.eq_ignore_ascii_case(choice)) || self.get(choice).is_some()
    }
}

pub trait CueAppExt {
    /// Register a pack from its TOML document (`CuePacks`). Panics on a
    /// pack that doesn't parse (it is the game's own data).
    fn register_cues(&mut self, toml: &str) -> &mut Self;
}

impl CueAppExt for App {
    fn register_cues(&mut self, toml: &str) -> &mut Self {
        let pack = CuePack::parse(toml).unwrap_or_else(|e| panic!("cue pack: {e}"));
        self.world_mut().get_resource_or_init::<CuePacks>().register(pack);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACK: &str = r#"
id = "stand_in"
name = "Stand-in"
game = "stand_in"
[sounds]
rung_up = "up.wav"
"#;

    #[test]
    fn a_pack_reads_and_is_chosen_by_id_default_or_none() {
        let pack = CuePack::parse(PACK).unwrap();
        assert_eq!(pack.volume, 1.0);
        assert_eq!(pack.sound("rung_up"), Some("up.wav"));
        assert_eq!(pack.sound("match_won"), None, "silent");
        let mut packs = CuePacks::default();
        assert!(packs.choose(None).is_none(), "nothing registered");
        packs.register(pack.clone());
        packs.register(CuePack {
            id: "other".into(),
            ..pack
        });
        assert_eq!(packs.choose(None).unwrap().id, "stand_in", "the first");
        assert_eq!(packs.choose(Some("default")).unwrap().id, "stand_in");
        assert_eq!(packs.choose(Some("OTHER")).unwrap().id, "other");
        assert!(packs.choose(Some("none")).is_none());
        assert!(packs.choose(Some("nobody")).is_none());
        assert!(packs.valid("none") && packs.valid("other") && !packs.valid("nobody"));
        assert!(CuePack::parse("id = \"x\"\nname = \"x\"\ngame = \"x\"\nbogus = 1").is_err());
    }
}
