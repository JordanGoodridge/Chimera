//! The host's settings over the minigames' definitions
//! (`MinigameOverrides`): keys of a definition's TOML (dotted paths:
//! `rules.knife_finale`, `rules.rung_extras.melee`, `rotation`,
//! `avoid_recent`, `spawn.respawn.protection`) set to a value of the
//! host's, whatever minigame plays. Each is a server cvar an app
//! registers (`override_cvar`: empty for the definition's own value), so
//! a host sets it from the console, the command line or a menu (the party
//! apps' Escape menu).
//!
//! - **Applied when a match starts** (`minigame::start`, so the first
//!   match, a rotation and `change_match`; never a network client's
//!   `join_remote`): the playing definition is made again from its own
//!   text with the overrides set (`apply_text`), checked and put back
//!   under its id (`minigame::add`), so the match manifest carries the
//!   text a client plays (`chimera_net::minigame_net`). An override the
//!   definition can't take (a knife finale without a melee weapon in the
//!   kit) is said in the log and the match plays the definition as it is.
//!   The definition's own text is kept (`MinigameOverrides::originals`),
//!   so clearing an override brings its value back.
//! - **The next map's pick** reads them too (`effective`: `rotation`,
//!   `avoid_recent`): a change there takes effect at the match's end.
//!
//! Known shortcuts: a key under a value that isn't a table replaces that
//! value by a table (`rung_extras = false` with one part turned on turns
//! the others on too); the overrides last for the process (they aren't
//! archived: a host sets them again, or puts them in its config).

use std::collections::BTreeMap;

use bevy::prelude::*;

use super::MinigameDef;
use crate::console::{Console, ConsoleAppExt};

/// The host's overrides: dotted key to its TOML value (as typed).
#[derive(Resource, Default, Clone, Debug)]
pub struct MinigameOverrides {
    pub keys: BTreeMap<String, String>,
    /// Each overridden definition's own text and the text made from it
    /// (by id): a text there other than the made one is a new own text
    /// (an automatic minigame made again).
    originals: BTreeMap<String, (&'static str, &'static str)>,
}

impl MinigameOverrides {
    /// Set `key` to `value` (a TOML value: `true`, `3`, `"random"`); an
    /// empty value clears it.
    pub fn set(&mut self, key: &str, value: &str) {
        if value.trim().is_empty() {
            self.keys.remove(key);
        } else {
            self.keys.insert(key.to_string(), value.trim().to_string());
        }
    }
}

/// A TOML value as typed: a number, a boolean, a quoted string, or a bare
/// word as a string (`random`).
fn value_of(text: &str) -> toml::Value {
    toml::from_str::<toml::Table>(&format!("v = {text}"))
        .ok()
        .and_then(|mut t| t.remove("v"))
        .unwrap_or_else(|| toml::Value::String(text.trim_matches('"').to_string()))
}

/// `text` (a definition) with each of `keys` set, as TOML text.
pub fn apply_text(text: &str, keys: &BTreeMap<String, String>) -> Result<String, String> {
    let mut doc: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
    for (key, value) in keys {
        let path: Vec<&str> = key.split('.').collect();
        let (last, parents) = path.split_last().ok_or("an empty key")?;
        let mut table = &mut doc;
        for p in parents {
            let entry = table.entry(p.to_string()).or_insert_with(|| toml::Value::Table(toml::Table::new()));
            if !entry.is_table() {
                *entry = toml::Value::Table(toml::Table::new());
            }
            table = entry.as_table_mut().expect("a table");
        }
        table.insert(last.to_string(), value_of(value));
    }
    toml::to_string(&doc).map_err(|e| e.to_string())
}

/// `def` as the overrides make it (for the next map's pick), else as it
/// is.
pub fn effective(world: &World, def: &'static MinigameDef) -> std::borrow::Cow<'static, MinigameDef> {
    let Some(o) = world.get_resource::<MinigameOverrides>().filter(|o| !o.keys.is_empty()) else {
        return std::borrow::Cow::Borrowed(def);
    };
    let Some(own) = own_text(world, o, &def.id) else {
        return std::borrow::Cow::Borrowed(def);
    };
    apply_text(own, &o.keys)
        .and_then(|t| MinigameDef::parse(&def.id, &t))
        .map_or(std::borrow::Cow::Borrowed(def), std::borrow::Cow::Owned)
}

/// Definition `id`'s own text (before any override).
fn own_text(world: &World, o: &MinigameOverrides, id: &str) -> Option<&'static str> {
    let now = world.get_resource::<super::Minigames>()?.text(id)?;
    Some(match o.originals.get(id) {
        Some((own, made)) if *made == now => own,
        _ => now,
    })
}

/// Before a match of `id` starts (a host's): the definition made again
/// with the overrides (or its own text back when none are set).
pub(super) fn apply(world: &mut World, id: &str) {
    let Some(o) = world.get_resource::<MinigameOverrides>() else { return };
    let Some(own) = own_text(world, o, id) else { return };
    let now = world.resource::<super::Minigames>().text(id);
    let keys = o.keys.clone();
    let text = if keys.is_empty() {
        own.to_string()
    } else {
        match apply_text(own, &keys) {
            Ok(t) => t,
            Err(e) => {
                warn!("{id}: the host's settings: {e}");
                return;
            }
        }
    };
    if now == Some(text.as_str()) {
        return;
    }
    match super::add(world, id, &text) {
        Ok(_) => {
            let made = world.resource::<super::Minigames>().text(id).expect("just added");
            world.resource_mut::<MinigameOverrides>().originals.insert(id.to_string(), (own, made));
        }
        Err(e) => {
            let line = format!("{id}: the host's settings don't fit this minigame ({e}); it plays as written");
            warn!("{line}");
            if let Some(mut c) = world.get_resource_mut::<Console>() {
                c.print(crate::console::Level::Warn, line);
            }
            // Its own text back, if an earlier match changed it.
            if now != Some(own) {
                let _ = super::add(world, id, own);
            }
        }
    }
}

/// Register cvar `name` (a server one: name it `mp_*` or `sv_*`) setting
/// override `key`; empty (the default) is the definition's own value.
pub fn override_cvar(app: &mut App, name: &'static str, key: &'static str, help: &str) {
    app.init_resource::<MinigameOverrides>();
    app.console_cvar(
        name,
        &format!("{help} Empty: the minigame's own. From the next match."),
        "",
        move |w| Some(w.get_resource::<MinigameOverrides>().and_then(|o| o.keys.get(key).cloned()).unwrap_or_default()),
        move |w, v| {
            w.get_resource_or_init::<MinigameOverrides>().set(key, v);
            Ok(())
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEF: &str = r#"
id = "t:x"
name = "X"
players = { min = 1, max = 4 }
maps = ["chimera:greybox"]
teams = "free_for_all"
time_limit = 60
avoid_recent = 4

[rules]
module = "chimera:ladder"
rung_extras = false
ladder = ["a"]
"#;

    #[test]
    fn overrides_set_keys_deep_and_typed() {
        let keys: BTreeMap<String, String> = [
            ("rules.knife_finale", "true"),
            ("rules.rung_extras.melee", "false"),
            ("rotation", "random"),
            ("avoid_recent", "2"),
            ("spawn.respawn.protection", "\"3s\""),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let text = apply_text(DEF, &keys).unwrap();
        let def = MinigameDef::parse("t", &text).unwrap();
        assert_eq!(def.rotation, super::super::def::Rotation::Random);
        assert_eq!(def.avoid_recent, 2);
        assert_eq!(def.spawn.respawn.protection, super::super::def::Protection::both(3.0));
        assert_eq!(def.rules.params.get("knife_finale").and_then(|v| v.as_bool()), Some(true));
        let extras = def.rules.params.get("rung_extras").and_then(|v| v.as_table()).unwrap();
        assert_eq!(extras.get("melee").and_then(|v| v.as_bool()), Some(false));
        // None: the text as it was (in meaning).
        let same = MinigameDef::parse("t", &apply_text(DEF, &BTreeMap::new()).unwrap()).unwrap();
        assert_eq!(same, MinigameDef::parse("t", DEF).unwrap());
    }
}
