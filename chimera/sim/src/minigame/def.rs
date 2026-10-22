//! A minigame's definition as data (docs/plans/active/architecture-review.md
//! D2): our own TOML files (`minigames/`), parsed at run time and checked
//! against what the app registered (`check`). The shape:
//!
//! ```toml
//! id = "originals:gun_game_dust2"
//! name = "Gun Game: Dust II"
//! players = { min = 2, max = 16 }
//! maps = ["cs_source:de_dust2", "combat_arms:warehouse"]   # a pool, the first played first
//! teams = "free_for_all"                                  # or "two", "one_team"
//! time_limit = "10m"                                      # "90s", "1.5s", "500ms", or seconds
//!
//! [slots]                     # the gameplay's slots by game id (`chimera_core::gameplay`)
//! hud = "combat_arms"         # the HUD drawn
//! binds = "combat_arms"       # the default binds and movement cvars
//! tick = "combat_arms"        # whose fixed tick the match runs at
//! bots = "combat_arms"        # whose bots play (a game that gates them by `gameplay::bots_are`)
//! movement = "combat_arms:movement"   # checked to be registered; the app's characters use it
//! character = "combat_arms:soldier"   # informational (the kit isn't a slot yet)
//! weapons = ["cs_source", "combat_arms"]   # the weapon sets a definition may name
//!
//! [spawn]
//! respawn = { delay = "2s", protection = "1.5s" }
//! points = "far_from_enemies"   # or "map" (in turn), "team" (the player's team's)
//! kit = ["combat_arms:m9_knife"]   # given every life, before the rules' weapons
//!
//! [rules]
//! module = "chimera:ladder"   # a registered rules module; the other keys are its own
//! ladder = ["cs_source:weapon_glock", "combat_arms:m4a1"]
//!
//! [modifiers]                 # registered modifiers by name, each with its value
//! ```
//!
//! Unknown keys are errors (but under `[rules]`, which its module reads).
//! Durations are strings with a unit (`ms`, `s`, `m`) or plain seconds.

use serde::Deserialize;

use crate::mode::Teams;

/// A minigame's definition.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MinigameDef {
    /// `namespace:name` (`originals:gun_game_dust2`).
    pub id: String,
    /// The name players see.
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub players: Players,
    /// The map pool by id (`game:map`), the first played first.
    pub maps: Vec<String>,
    pub teams: TeamsDef,
    /// The match's length (seconds).
    #[serde(deserialize_with = "seconds")]
    pub time_limit: f32,
    #[serde(default)]
    pub slots: Slots,
    #[serde(default)]
    pub spawn: SpawnDef,
    pub rules: RulesDef,
    /// Modifiers by name, each with its value (`ModifierAppExt`).
    #[serde(default)]
    pub modifiers: toml::Table,
}

/// How many players a match takes: it waits for `min`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Players {
    pub min: u32,
    pub max: u32,
}

/// How a minigame makes teams.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamsDef {
    FreeForAll,
    Two,
    OneTeam,
}

impl TeamsDef {
    /// The mode runtime's teams.
    pub fn teams(self) -> Teams {
        match self {
            Self::FreeForAll => Teams::FreeForAll,
            Self::Two => Teams::Two,
            Self::OneTeam => Teams::AllAlpha,
        }
    }
}

/// The gameplay's slots (game ids), the movement and character asked for
/// and the weapon sets allowed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slots {
    pub hud: Option<String>,
    pub binds: Option<String>,
    pub tick: Option<String>,
    pub bots: Option<String>,
    pub movement: Option<String>,
    pub character: Option<String>,
    /// Game ids whose weapons the definition may name (empty: any).
    #[serde(default)]
    pub weapons: Vec<String>,
}

/// Where and how players come back.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnDef {
    #[serde(default)]
    pub respawn: Respawn,
    #[serde(default)]
    pub points: Points,
    /// Weapons given every life before the rules' own.
    #[serde(default)]
    pub kit: Vec<String>,
}

/// The dead respawn after `delay` s, protected from damage for
/// `protection` s (firing ends it).
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Respawn {
    #[serde(deserialize_with = "seconds")]
    pub delay: f32,
    #[serde(deserialize_with = "seconds", default)]
    pub protection: f32,
}

impl Default for Respawn {
    fn default() -> Self {
        Self {
            delay: 2.0,
            protection: 1.5,
        }
    }
}

/// Which spawn point a player comes back at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Points {
    /// The map's points in turn, skipping occupied ones.
    Map,
    /// The point farthest from the nearest living other player.
    #[default]
    FarFromEnemies,
    /// The player's team's points (any where there are none).
    Team,
}

/// The rules module and its own keys.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct RulesDef {
    pub module: String,
    /// The module's parameters (every other key under `[rules]`).
    #[serde(flatten)]
    pub params: toml::Table,
}

impl MinigameDef {
    /// Parse a definition (`name` for errors; checks its own consistency,
    /// not what the app registered: `check`).
    pub fn parse(name: &str, text: &str) -> Result<Self, String> {
        let def: Self = toml::from_str(text).map_err(|e| format!("{name}: {e}"))?;
        def.check_self().map_err(|e| format!("{name}: {e}"))?;
        Ok(def)
    }

    /// What the definition alone must satisfy.
    pub fn check_self(&self) -> Result<(), String> {
        let namespaced = |s: &str| s.split_once(':').is_some_and(|(a, b)| !a.is_empty() && !b.is_empty());
        if !namespaced(&self.id) {
            return Err(format!("id `{}` isn't `namespace:name`", self.id));
        }
        if self.players.min == 0 || self.players.min > self.players.max {
            return Err(format!("players: min {} and max {} (1 <= min <= max)", self.players.min, self.players.max));
        }
        if self.maps.is_empty() {
            return Err("maps: the pool is empty".into());
        }
        if let Some(m) = self.maps.iter().find(|m| !namespaced(m)) {
            return Err(format!("maps: `{m}` isn't `game:map`"));
        }
        if self.time_limit <= 0.0 {
            return Err("time_limit must be more than 0".into());
        }
        if self.spawn.respawn.delay < 0.0 || self.spawn.respawn.protection < 0.0 {
            return Err("spawn.respawn: negative times".into());
        }
        Ok(())
    }

    /// Whether weapon `id` is from a set the definition allows.
    pub fn allows_weapon(&self, id: &str) -> bool {
        let game = id.split_once(':').map_or("", |(g, _)| g);
        self.slots.weapons.is_empty() || self.slots.weapons.iter().any(|w| w == game)
    }
}

/// A duration: `"10m"`, `"90s"`, `"1.5s"`, `"500ms"`, or a number of
/// seconds.
pub fn parse_seconds(s: &str) -> Result<f32, String> {
    let s = s.trim();
    let (number, scale) = if let Some(n) = s.strip_suffix("ms") {
        (n, 0.001)
    } else if let Some(n) = s.strip_suffix('s') {
        (n, 1.0)
    } else if let Some(n) = s.strip_suffix('m') {
        (n, 60.0)
    } else {
        (s, 1.0)
    };
    let n: f32 = number.trim().parse().map_err(|_| format!("`{s}` isn't a duration (10m, 90s, 500ms)"))?;
    if !n.is_finite() || n < 0.0 {
        return Err(format!("`{s}`: not a duration"));
    }
    Ok(n * scale)
}

fn seconds<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Value {
        Number(f64),
        Text(String),
    }
    match Value::deserialize(d)? {
        Value::Number(n) if n >= 0.0 => Ok(n as f32),
        Value::Number(n) => Err(serde::de::Error::custom(format!("{n}: a negative duration"))),
        Value::Text(t) => parse_seconds(&t).map_err(serde::de::Error::custom),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEF: &str = r#"
id = "test:ladder"
name = "Ladder"
players = { min = 2, max = 8 }
maps = ["greybox:greybox"]
teams = "free_for_all"
time_limit = "2m"

[spawn]
respawn = { delay = "500ms", protection = 1 }
kit = ["kit:shotgun"]

[rules]
module = "chimera:ladder"
ladder = ["kit:pistol", "kit:rifle"]
"#;

    #[test]
    fn a_definition_reads() {
        let def = MinigameDef::parse("t", DEF).unwrap();
        assert_eq!(def.time_limit, 120.0);
        assert_eq!(def.spawn.respawn, Respawn { delay: 0.5, protection: 1.0 });
        assert_eq!(def.spawn.points, Points::FarFromEnemies);
        assert_eq!(def.teams.teams(), Teams::FreeForAll);
        assert_eq!(def.rules.module, "chimera:ladder");
        assert!(def.rules.params.contains_key("ladder"));
        assert!(def.allows_weapon("anything:at_all"), "no weapon sets named: any");
    }

    #[test]
    fn mistakes_are_errors() {
        let bad = |from: &str, to: &str| MinigameDef::parse("t", &DEF.replace(from, to)).unwrap_err();
        assert!(bad("time_limit = \"2m\"", "time_limit = \"2 minutes\"").contains("duration"));
        assert!(bad("min = 2", "min = 9").contains("min"));
        assert!(bad("id = \"test:ladder\"", "id = \"ladder\"").contains("namespace"));
        assert!(bad("teams = \"free_for_all\"", "teams = \"pairs\"").contains("unknown variant"));
        assert!(bad("[spawn]", "colour = 1\n[spawn]").contains("unknown field"));
        assert!(bad("maps = [\"greybox:greybox\"]", "maps = []").contains("empty"));
    }

    #[test]
    fn durations() {
        assert_eq!(parse_seconds("10m"), Ok(600.0));
        assert_eq!(parse_seconds("1.5s"), Ok(1.5));
        assert_eq!(parse_seconds("250ms"), Ok(0.25));
        assert_eq!(parse_seconds("3"), Ok(3.0));
        assert!(parse_seconds("-1s").is_err());
    }
}
