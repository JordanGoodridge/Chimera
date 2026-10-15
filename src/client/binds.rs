//! Key binds, as Source keeps them: key name -> console line
//! (`console::Console::binds`), saved in config.cfg as `bind` lines.
//! Every game key is a bind: the defaults (`default_binds`: the default
//! game's (`game_registry::GameInfo`), from its install's
//! `cfg/config_default.cfg`, else its built-in table, plus ours on the
//! keys it leaves free, `MASHUP_BINDS`) go in before
//! config.cfg runs, `binddefaults` puts them back (the options' "Use
//! Defaults"), and the options' keyboard tab rebinds them.
//!
//! Binds whose whole line is one of `POLLED` (`+forward`, `slot1`,
//! `buymenu` ...) are read by the system that owns the action, from the
//! keys held this frame (`pressed`, `just_pressed`), with that system's
//! own rules (only while playing, not while typing ...); `console::run_binds`
//! leaves them alone. Every other bind runs its line through the console.
//!
//! Every game key is a bind (`Console::binds`, saved in config.cfg):
//! `default_binds` (the default game's: CS:S's install's
//! `cfg/config_default.cfg`, read at start by `read_install_defaults`, else
//! the same table in `GameInfo::default_binds`; plus `MASHUP_BINDS` on keys
//! the game leaves free: Alt free
//! look, V noclip, F9 `bug`, the debug keys F3 `toggle mashup_drawcollision`,
//! F8 `debugui`, F11 `mashup_inspector`) go in before config.cfg runs
//! (`binddefaults`; `binddefaults missing` for configs written before, which
//! lack `CONFIG_MARK`; `binddefaults new` once after a config lacking
//! `DEFAULTS_MARK` puts `added_defaults` on free keys, so older configs get
//! defaults added since; saved binds win); binds whose line is one of `POLLED`
//! (`+forward`, `slot1`, `buymenu`, `radio1`, ...) are read from the held keys
//! by the system that owns the action (`pressed`, `just_pressed`,
//! `wheel_notches`), all others run through the console (`console::run_binds`);
//! Source key names, rebind and clear lines for the options.

use std::{collections::BTreeMap, sync::RwLock};

use bevy::{
    input::mouse::{AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
};

use crate::{
    console::{Console, ConsoleAppExt},
    game_registry::GameInfo,
};

/// Source key names for keyboard keys.
pub const KEY_NAMES: &[(KeyCode, &str)] = &[
    (KeyCode::KeyA, "a"),
    (KeyCode::KeyB, "b"),
    (KeyCode::KeyC, "c"),
    (KeyCode::KeyD, "d"),
    (KeyCode::KeyE, "e"),
    (KeyCode::KeyF, "f"),
    (KeyCode::KeyG, "g"),
    (KeyCode::KeyH, "h"),
    (KeyCode::KeyI, "i"),
    (KeyCode::KeyJ, "j"),
    (KeyCode::KeyK, "k"),
    (KeyCode::KeyL, "l"),
    (KeyCode::KeyM, "m"),
    (KeyCode::KeyN, "n"),
    (KeyCode::KeyO, "o"),
    (KeyCode::KeyP, "p"),
    (KeyCode::KeyQ, "q"),
    (KeyCode::KeyR, "r"),
    (KeyCode::KeyS, "s"),
    (KeyCode::KeyT, "t"),
    (KeyCode::KeyU, "u"),
    (KeyCode::KeyV, "v"),
    (KeyCode::KeyW, "w"),
    (KeyCode::KeyX, "x"),
    (KeyCode::KeyY, "y"),
    (KeyCode::KeyZ, "z"),
    (KeyCode::Digit0, "0"),
    (KeyCode::Digit1, "1"),
    (KeyCode::Digit2, "2"),
    (KeyCode::Digit3, "3"),
    (KeyCode::Digit4, "4"),
    (KeyCode::Digit5, "5"),
    (KeyCode::Digit6, "6"),
    (KeyCode::Digit7, "7"),
    (KeyCode::Digit8, "8"),
    (KeyCode::Digit9, "9"),
    (KeyCode::Space, "space"),
    (KeyCode::Enter, "enter"),
    (KeyCode::Tab, "tab"),
    (KeyCode::Backspace, "backspace"),
    (KeyCode::CapsLock, "capslock"),
    (KeyCode::ShiftLeft, "shift"),
    (KeyCode::ShiftRight, "rshift"),
    (KeyCode::ControlLeft, "ctrl"),
    (KeyCode::ControlRight, "rctrl"),
    (KeyCode::AltLeft, "alt"),
    (KeyCode::AltRight, "ralt"),
    (KeyCode::ArrowUp, "uparrow"),
    (KeyCode::ArrowDown, "downarrow"),
    (KeyCode::ArrowLeft, "leftarrow"),
    (KeyCode::ArrowRight, "rightarrow"),
    (KeyCode::Insert, "ins"),
    (KeyCode::Delete, "del"),
    (KeyCode::Home, "home"),
    (KeyCode::End, "end"),
    (KeyCode::PageUp, "pgup"),
    (KeyCode::PageDown, "pgdn"),
    (KeyCode::Pause, "pause"),
    (KeyCode::F1, "f1"),
    (KeyCode::F2, "f2"),
    (KeyCode::F3, "f3"),
    (KeyCode::F4, "f4"),
    (KeyCode::F5, "f5"),
    (KeyCode::F6, "f6"),
    (KeyCode::F7, "f7"),
    (KeyCode::F8, "f8"),
    (KeyCode::F9, "f9"),
    (KeyCode::F10, "f10"),
    (KeyCode::F11, "f11"),
    (KeyCode::F12, "f12"),
    (KeyCode::Numpad0, "kp_ins"),
    (KeyCode::Numpad1, "kp_end"),
    (KeyCode::Numpad2, "kp_downarrow"),
    (KeyCode::Numpad3, "kp_pgdn"),
    (KeyCode::Numpad4, "kp_leftarrow"),
    (KeyCode::Numpad5, "kp_5"),
    (KeyCode::Numpad6, "kp_rightarrow"),
    (KeyCode::Numpad7, "kp_home"),
    (KeyCode::Numpad8, "kp_uparrow"),
    (KeyCode::Numpad9, "kp_pgup"),
    (KeyCode::NumpadDecimal, "kp_del"),
    (KeyCode::NumpadDivide, "kp_slash"),
    (KeyCode::NumpadMultiply, "kp_multiply"),
    (KeyCode::NumpadSubtract, "kp_minus"),
    (KeyCode::NumpadAdd, "kp_plus"),
    (KeyCode::NumpadEnter, "kp_enter"),
    (KeyCode::Minus, "-"),
    (KeyCode::Equal, "="),
    (KeyCode::BracketLeft, "["),
    (KeyCode::BracketRight, "]"),
    (KeyCode::Backslash, "\\"),
    (KeyCode::Semicolon, "semicolon"),
    (KeyCode::Quote, "'"),
    (KeyCode::Comma, ","),
    (KeyCode::Period, "."),
    (KeyCode::Slash, "/"),
];

/// Source names for mouse buttons.
pub const MOUSE_NAMES: &[(MouseButton, &str)] = &[
    (MouseButton::Left, "mouse1"),
    (MouseButton::Right, "mouse2"),
    (MouseButton::Middle, "mouse3"),
    (MouseButton::Back, "mouse4"),
    (MouseButton::Forward, "mouse5"),
];

/// The wheel's two "keys".
pub const WHEEL_UP: &str = "mwheelup";
pub const WHEEL_DOWN: &str = "mwheeldown";

/// Mashup's own binds, each on a key CS:S's defaults leave free (and only
/// while it is free): Alt looks around, V flies, F9 files a bug report
/// (as F4 does), and the debug keys: F3 collision shapes, F8 the debug UI,
/// F11 the entity inspector.
pub const MASHUP_BINDS: &[(&str, &str)] = &[
    ("alt", "+freelook"),
    ("v", "noclip"),
    ("f3", "toggle mashup_drawcollision"),
    ("f8", "debugui"),
    ("f9", "bug"),
    ("f11", "mashup_inspector"),
];

/// The defaults before they followed CS:S's `config_default.cfg`
/// (2026-10-13): every default not among these is new to a config written
/// before then (`added_defaults`).
const OLD_DEFAULTS: &[(&str, &str)] = &[
    ("w", "+forward"),
    ("s", "+back"),
    ("a", "+moveleft"),
    ("d", "+moveright"),
    ("space", "+jump"),
    ("ctrl", "+duck"),
    ("shift", "+speed"),
    ("e", "+use"),
    ("r", "+reload"),
    ("mouse1", "+attack"),
    ("mouse2", "+attack2"),
    ("1", "slot1"),
    ("2", "slot2"),
    ("3", "slot3"),
    ("4", "slot4"),
    ("5", "slot5"),
    ("q", "lastinv"),
    ("g", "drop"),
    ("b", "buymenu"),
    (",", "buyammo1"),
    (".", "buyammo2"),
    ("m", "chooseteam"),
    ("tab", "+showscores"),
    ("z", "radio1"),
    ("x", "radio2"),
    ("c", "radio3"),
    ("y", "messagemode"),
    ("u", "messagemode2"),
    ("alt", "+freelook"),
    ("v", "noclip"),
    ("f9", "bug"),
];

/// The default game's binds (`read_install_defaults`, `use_game_defaults`);
/// empty before (and without a game).
static GAME_DEFAULTS: RwLock<Vec<(String, String)>> = RwLock::new(Vec::new());

/// Whether a key name is one we know (keyboard, mouse or wheel).
pub fn known_key(name: &str) -> bool {
    KEY_NAMES.iter().any(|(_, n)| *n == name)
        || MOUSE_NAMES.iter().any(|(_, n)| *n == name)
        || [WHEEL_UP, WHEEL_DOWN].contains(&name)
}

/// The binds a config's text leaves (`bind`, `unbind`, `unbindall`), in
/// the order first bound; keys we don't know (Esc, `) are left out.
pub fn cfg_binds(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        for words in crate::console::parse(line) {
            let Some(cmd) = words.first() else { continue };
            match (cmd.to_lowercase().as_str(), words.get(1)) {
                ("unbindall", _) => out.clear(),
                ("unbind", Some(key)) => {
                    let key = key.to_lowercase();
                    out.retain(|(k, _)| *k != key);
                }
                ("bind", Some(key)) if words.len() > 2 => {
                    let key = key.to_lowercase();
                    if !known_key(&key) {
                        continue;
                    }
                    let line = words[2..].join(" ");
                    match out.iter_mut().find(|(k, _)| *k == key) {
                        Some(b) => b.1 = line,
                        None => out.push((key, line)),
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// The defaults from the game's: those, then mashup's own on the keys
/// they leave free (`MASHUP_BINDS`).
pub fn defaults_from(game: &[(String, String)]) -> Vec<(String, String)> {
    let mut out = game.to_vec();
    for (key, command) in MASHUP_BINDS {
        if !out.iter().any(|(k, _)| k == key) {
            out.push((key.to_string(), command.to_string()));
        }
    }
    out
}

/// The binds a fresh config starts with (and "Use Defaults" restores):
/// the default game's (from its install's file when it was read, else its
/// built-in table), plus mashup's own (`MASHUP_BINDS`).
pub fn default_binds() -> Vec<(String, String)> {
    let game = GAME_DEFAULTS.read().map(|d| d.clone()).unwrap_or_default();
    defaults_from(&game)
}

/// Make `binds` (key, command) the game's defaults.
pub fn use_game_defaults(binds: &[(&str, &str)]) {
    let owned = binds.iter().map(|(k, c)| (k.to_string(), c.to_string())).collect();
    set_game_defaults(owned);
}

fn set_game_defaults(binds: Vec<(String, String)>) {
    if let Ok(mut d) = GAME_DEFAULTS.write() {
        *d = binds;
    }
}

/// Read the default game's defaults from its install's file
/// (`GameInfo::default_binds_cfg`: CS:S's `cfg/config_default.cfg`, in the
/// configured, saved or found install). Without the file they are the
/// game's built-in table (`GameInfo::default_binds`); without a game, none
/// (mashup's own only). Returns whether the file was read.
pub fn read_install_defaults(game: Option<&GameInfo>) -> bool {
    let Some(game) = game else {
        set_game_defaults(Vec::new());
        return false;
    };
    let binds = game
        .default_binds_text()
        .map(|t| cfg_binds(&t))
        .filter(|b| !b.is_empty());
    let found = binds.is_some();
    match binds {
        Some(b) => set_game_defaults(b),
        None => use_game_defaults(game.default_binds),
    }
    found
}

/// Defaults a config written before they followed CS:S's lacks (see
/// `OLD_DEFAULTS`): `binddefaults new` (run after such a config) puts
/// each on its key when the key is free and no key runs the command.
pub fn added_defaults() -> Vec<(String, String)> {
    default_binds()
        .into_iter()
        .filter(|(k, c)| !OLD_DEFAULTS.contains(&(k.as_str(), c.as_str())))
        .collect()
}

/// Put the added defaults on free keys (see `added_defaults`).
pub fn bind_added_defaults(binds: &mut BTreeMap<String, String>) {
    for (key, command) in added_defaults() {
        if !binds.contains_key(&key) && keys_for(binds, &command).is_empty() {
            binds.insert(key, command);
        }
    }
}

/// Commands the systems that own them read from the held keys instead of
/// the console running them (see the module docs).
pub const POLLED: &[&str] = &[
    "+forward",
    "+back",
    "+moveleft",
    "+moveright",
    "+jump",
    "+duck",
    "+speed",
    "+use",
    "+reload",
    "+attack",
    "+attack2",
    "+freelook",
    "+showscores",
    "slot1",
    "slot2",
    "slot3",
    "slot4",
    "slot5",
    "lastinv",
    "drop",
    "buymenu",
    "chooseteam",
    "radio1",
    "radio2",
    "radio3",
    "messagemode",
    "messagemode2",
    "bug",
    "bugreport",
];

/// A bind's line normalised for comparing (trimmed, lower-case).
fn norm(line: &str) -> String {
    line.trim().to_lowercase()
}

/// Whether a bind's line is read by its system rather than run.
pub fn is_polled(line: &str) -> bool {
    let l = norm(line);
    POLLED.contains(&l.as_str())
}

/// The keys bound to exactly `command`, in key order.
pub fn keys_for<'a>(binds: &'a BTreeMap<String, String>, command: &str) -> Vec<&'a str> {
    let c = norm(command);
    binds
        .iter()
        .filter(|(_, v)| norm(v) == c)
        .map(|(k, _)| k.as_str())
        .collect()
}

/// A key name as the options list shows it (`SPACE`, `MOUSE1`).
pub fn display(key: &str) -> String {
    key.to_uppercase()
}

/// Whether a key (by Source name) is held.
pub fn key_held(name: &str, keys: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>) -> bool {
    KEY_NAMES
        .iter()
        .find(|(_, n)| *n == name)
        .is_some_and(|(k, _)| keys.pressed(*k))
        || MOUSE_NAMES
            .iter()
            .find(|(_, n)| *n == name)
            .is_some_and(|(b, _)| mouse.pressed(*b))
}

fn key_just_pressed(name: &str, keys: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>) -> bool {
    KEY_NAMES
        .iter()
        .find(|(_, n)| *n == name)
        .is_some_and(|(k, _)| keys.just_pressed(*k))
        || MOUSE_NAMES
            .iter()
            .find(|(_, n)| *n == name)
            .is_some_and(|(b, _)| mouse.just_pressed(*b))
}

/// Whether a key bound to `command` is held.
pub fn pressed(
    binds: &BTreeMap<String, String>,
    keys: &ButtonInput<KeyCode>,
    mouse: &ButtonInput<MouseButton>,
    command: &str,
) -> bool {
    keys_for(binds, command).into_iter().any(|k| key_held(k, keys, mouse))
}

/// Whether a key bound to `command` went down this frame.
pub fn just_pressed(
    binds: &BTreeMap<String, String>,
    keys: &ButtonInput<KeyCode>,
    mouse: &ButtonInput<MouseButton>,
    command: &str,
) -> bool {
    keys_for(binds, command)
        .into_iter()
        .any(|k| key_just_pressed(k, keys, mouse))
}

/// Wheel notches this frame on wheel directions bound to `command`.
pub fn wheel_notches(binds: &BTreeMap<String, String>, scroll: &AccumulatedMouseScroll, command: &str) -> u32 {
    /// Pixel-unit scrolling (touchpads, smooth wheels): pixels per notch.
    const PIXELS_PER_NOTCH: f32 = 40.0;
    let y = scroll.delta.y;
    if y == 0.0 {
        return 0;
    }
    let key = if y > 0.0 { WHEEL_UP } else { WHEEL_DOWN };
    if !keys_for(binds, command).contains(&key) {
        return 0;
    }
    (match scroll.unit {
        MouseScrollUnit::Line => y.abs().round(),
        MouseScrollUnit::Pixel => (y.abs() / PIXELS_PER_NOTCH).ceil(),
    }) as u32
}

/// The first key, button or wheel notch that went down this frame, by
/// Source name (for rebinding). Esc and ` are not offered.
pub fn first_pressed(
    keys: &ButtonInput<KeyCode>,
    mouse: &ButtonInput<MouseButton>,
    scroll: Option<&AccumulatedMouseScroll>,
) -> Option<&'static str> {
    KEY_NAMES
        .iter()
        .find(|(k, _)| keys.just_pressed(*k))
        .map(|(_, n)| *n)
        .or_else(|| MOUSE_NAMES.iter().find(|(b, _)| mouse.just_pressed(*b)).map(|(_, n)| *n))
        .or_else(|| {
            let y = scroll?.delta.y;
            (y != 0.0).then_some(if y > 0.0 { WHEEL_UP } else { WHEEL_DOWN })
        })
}

/// Put the default binds in: every one (`all`, after clearing the rest),
/// or only those whose key is free and whose command no key runs (for a
/// config written before its keys were binds).
pub fn bind_defaults(binds: &mut BTreeMap<String, String>, all: bool) {
    if all {
        binds.clear();
    }
    for (key, command) in default_binds() {
        if all || (!binds.contains_key(&key) && keys_for(binds, &command).is_empty()) {
            binds.insert(key, command);
        }
    }
}

/// The line that sets `command`'s key to `key` alone: its other keys
/// unbound, `key` bound to it (whatever it ran before).
pub fn rebind_line(binds: &BTreeMap<String, String>, command: &str, key: &str) -> String {
    let mut parts: Vec<String> = keys_for(binds, command)
        .into_iter()
        .filter(|k| *k != key)
        .map(|k| format!("unbind {}", crate::console::quote(k)))
        .collect();
    parts.push(format!(
        "bind {} {}",
        crate::console::quote(key),
        crate::console::quote(command)
    ));
    parts.join("; ")
}

/// The line that clears every key of `command` (None: it has none).
pub fn clear_line(binds: &BTreeMap<String, String>, command: &str) -> Option<String> {
    let keys = keys_for(binds, command);
    (!keys.is_empty()).then(|| {
        keys.iter()
            .map(|k| format!("unbind {}", crate::console::quote(k)))
            .collect::<Vec<_>>()
            .join("; ")
    })
}

/// A config.cfg written since every key became a bind says so; one without
/// the mark gets the default binds its `unbindall` dropped back.
pub const CONFIG_MARK: &str = "// binds: all";

/// A config.cfg written since the defaults followed CS:S's says so; one
/// with `CONFIG_MARK` but not this gets the defaults added since
/// (`binddefaults new`) once, on free keys.
pub const DEFAULTS_MARK: &str = "// bind defaults: cs";

pub fn commands(app: &mut App) {
    app.console_command(
        "binddefaults",
        "binddefaults [missing|new]: put the default binds back (missing: only on free keys for unbound commands; \
         new: only the defaults added lately, likewise).",
        |w, a| {
            let mode = a.first().map(String::as_str);
            let mut c = w.resource_mut::<Console>();
            let before = c.binds.clone();
            match mode {
                Some("new") => {
                    bind_added_defaults(&mut c.binds);
                    // Written again with `DEFAULTS_MARK`, so it runs once.
                    c.dirty = true;
                }
                Some("missing") => bind_defaults(&mut c.binds, false),
                _ => bind_defaults(&mut c.binds, true),
            }
            if c.binds != before {
                c.dirty = true;
            }
            Ok(None)
        },
    )
    .console_command("bug", "bug [note]: file a bug report (CS:S's name for `bugreport`).", |w, a| {
        let note: Vec<String> = a.iter().map(|x| crate::console::quote(x)).collect();
        w.resource_mut::<Console>()
            .submit(format!("bugreport {}", note.join(" ")).trim_end().to_string());
        Ok(None)
    });
    // CS:S's number keys 6-9 and 0: no weapon slot there; the menus read
    // the number keys themselves.
    for n in 6..=10 {
        app.console_command(
            &format!("slot{n}"),
            "Weapon slot (CS:S has no weapons there: does nothing; menus take the number keys).",
            |_, _| Ok(None),
        );
    }
}

/// For tests of systems that read binds: the mouse's buttons and `game`'s
/// built-in default binds in an app with the console.
#[cfg(test)]
pub fn test_binds(app: &mut App, game: &GameInfo) {
    app.init_resource::<ButtonInput<MouseButton>>();
    use_game_defaults(game.default_binds);
    bind_defaults(&mut app.world_mut().resource_mut::<Console>().binds, true);
}

/// CS:S's `cfg/config_default.cfg` as it reads (its bind lines; the
/// file itself is the install's and never in the repo).
#[cfg(test)]
pub const SAMPLE_CONFIG_DEFAULT: &str = r#"// first run
unbindall

bind "ESCAPE" "cancelselect"
bind "`" "toggleconsole"

bind "TAB" "+showscores"
bind "SPACE" "+jump"
bind "," "buyammo1"
bind "." "buyammo2"
bind "0" "slot10"
bind "1" "slot1"
bind "2" "slot2"
bind "3" "slot3"
bind "4" "slot4"
bind "5" "slot5"
bind "6" "slot6"
bind "7" "slot7"
bind "8" "slot8"
bind "9" "slot9"
bind "a" "+moveleft"
bind "b" "buymenu"
bind "c" "radio3"
bind "d" "+moveright"
bind "e" "+use"
bind "f" "impulse 100"
bind "g" "drop"
bind "h" "commandmenu"
bind "i" "showbriefing"
bind "j" "cheer"
bind "k" "+voicerecord"
bind "m" "chooseteam"
bind "n" "nightvision"
bind "o" "buyequip"
bind "q" "lastinv"
bind "r" "+reload"
bind "s" "+back"
bind "t" "impulse 201"
bind "u" "messagemode2"
bind "w" "+forward"
bind "x" "radio2"
bind "y" "messagemode"
bind "z" "radio1"
bind "CTRL" "+duck"
bind "SHIFT" "+speed"
bind "F1" "autobuy"
bind "F2" "rebuy"
bind "F4" "bug"
bind "F5" "jpeg"
bind "F6" "save quick"
bind "F7" "load quick"
bind "F10" "quit prompt"
bind "MWHEELDOWN" "invnext"
bind "MWHEELUP" "invprev"
bind "MOUSE1" "+attack"
bind "MOUSE2" "+attack2"
bind "PAUSE" "pause"
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn binds(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn owned(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    /// The sample file's binds as the game's defaults (every test that
    /// reads `default_binds` sets the same, so tests in parallel agree).
    fn sample_defaults() -> Vec<(String, String)> {
        let read = cfg_binds(SAMPLE_CONFIG_DEFAULT);
        set_game_defaults(read.clone());
        read
    }


    #[test]
    fn defaults_are_config_default_cfgs_binds() {
        // The file's binds, in its order (Esc and ` aren't binds here).
        let read = sample_defaults();
        assert_eq!(read.first(), Some(&("tab".into(), "+showscores".into())));
        assert!(!read.iter().any(|(k, _)| k == "escape" || k == "`"));
        // Mashup's own go only on keys the game leaves free.
        let defaults = defaults_from(&read);
        assert_eq!(&defaults[..read.len()], &read[..]);
        for (k, c) in MASHUP_BINDS {
            assert!(!read.iter().any(|(rk, _)| rk == k), "{k} is CS:S's");
            assert!(defaults.contains(&(k.to_string(), c.to_string())));
        }
        // A file binding one of mashup's keys keeps it.
        let mut changed = read.clone();
        changed.push(("f8".into(), "say eight".into()));
        let d = defaults_from(&changed);
        assert_eq!(d.iter().filter(|(k, _)| k == "f8").count(), 1);
        assert!(d.contains(&("f8".into(), "say eight".into())));
        // Later lines win; unbind and unbindall drop keys; unknown keys
        // are left out.
        let b = cfg_binds("bind w +back\nbind W \"+forward\"; bind nope x\nbind q lastinv\nunbind Q\n");
        assert_eq!(b, owned(&[("w", "+forward")]));
        assert!(cfg_binds("bind a +jump\nunbindall").is_empty());
    }

    #[test]
    fn defaults_have_known_keys_and_one_line_per_key() {
        let game = sample_defaults();
        let d = defaults_from(&game);
        for (k, _) in &d {
            assert!(known_key(k), "{k} is not a key name");
        }
        let mut keys: Vec<&str> = d.iter().map(|(k, _)| k.as_str()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), d.len());
        // The debug keys moved off CS:S's: F1, F2 and the wheel are its.
        assert!(d.contains(&("f1".into(), "autobuy".into())));
        assert!(d.contains(&("f2".into(), "rebuy".into())));
        assert!(d.contains(&("mwheelup".into(), "invprev".into())));
        assert!(d.contains(&("mwheeldown".into(), "invnext".into())));
        for c in ["debugui", "mashup_inspector", "toggle mashup_drawcollision"] {
            let k = &d.iter().find(|(_, v)| v == c).unwrap().0;
            assert!(!game.iter().any(|(ck, _)| ck == k), "{c} on the game's {k}");
        }
    }

    #[test]
    fn added_defaults_reach_old_configs_on_free_keys() {
        // A config listing every bind, from before the defaults were CS:S's.
        sample_defaults();
        let mut b = binds(&[("w", "+forward"), ("f2", "debugui"), ("mwheelup", "+jump")]);
        bind_added_defaults(&mut b);
        assert_eq!(b.get("f1").map(String::as_str), Some("autobuy"));
        assert_eq!(b.get("f11").map(String::as_str), Some("mashup_inspector"));
        // The key taken, or the command on another key: left alone.
        assert_eq!(b["f2"], "debugui");
        assert!(keys_for(&b, "rebuy").is_empty());
        assert_eq!(b["mwheelup"], "+jump");
        assert_eq!(b.get("f8"), None, "debugui already has a key");
        let added = added_defaults();
        assert!(!added.iter().any(|(k, _)| k == "w"), "old defaults aren't added");
        for a in &added {
            assert!(default_binds().contains(a), "{a:?} is a default too");
        }
    }

    #[test]
    fn rebinding_moves_the_action_to_the_new_key() {
        let b = binds(&[("w", "+forward"), ("uparrow", "+forward"), ("f", "+use")]);
        assert_eq!(keys_for(&b, "+FORWARD "), ["uparrow", "w"]);
        assert_eq!(
            rebind_line(&b, "+forward", "f"),
            "unbind uparrow; unbind w; bind f +forward"
        );
        assert_eq!(rebind_line(&b, "+forward", "w"), "unbind uparrow; bind w +forward");
        assert_eq!(clear_line(&b, "+use").as_deref(), Some("unbind f"));
        assert_eq!(clear_line(&b, "+jump"), None);
        // Keys that need quoting stay one word.
        assert_eq!(rebind_line(&b, "say hi", ";"), "bind \";\" \"say hi\"");
    }

    #[test]
    fn missing_defaults_fill_only_free_keys_for_unbound_commands() {
        // An old config: only the player's own binds survived unbindall.
        sample_defaults();
        let mut b = binds(&[("f", "+jump"), ("w", "say hello")]);
        bind_defaults(&mut b, false);
        assert_eq!(b.get("space"), None, "+jump already has a key");
        assert_eq!(b["w"], "say hello", "a taken key is kept");
        assert!(keys_for(&b, "+forward").is_empty(), "w was taken, so +forward stays unbound");
        assert_eq!(b["a"], "+moveleft");
        bind_defaults(&mut b, true);
        assert_eq!(b.len(), default_binds().len());
        assert_eq!(b["w"], "+forward");
    }

    #[test]
    fn binds_save_to_config_and_load_back() {
        sample_defaults();
        let mut app = App::new();
        app.add_plugins(crate::console::ConsolePlugin);
        commands(&mut app);
        app.world_mut()
            .resource_mut::<Console>()
            .submit("binddefaults; unbind space; bind k +jump; bind [ \"say hi\"");
        app.update();
        let saved = app.world().resource::<Console>().binds.clone();
        assert_eq!(saved.get("k").map(String::as_str), Some("+jump"));
        let text = crate::console::config_text(app.world_mut());
        assert!(text.contains(CONFIG_MARK) && text.contains(DEFAULTS_MARK), "{text}");
        assert!(text.contains("bind k +jump\n"), "{text}");

        // A fresh start: the defaults, then the config.
        let mut fresh = App::new();
        fresh.add_plugins(crate::console::ConsolePlugin);
        commands(&mut fresh);
        let mut c = fresh.world_mut().resource_mut::<Console>();
        c.submit("binddefaults");
        for line in text.lines() {
            c.submit(line);
        }
        fresh.update();
        assert_eq!(fresh.world().resource::<Console>().binds, saved);
    }

    #[test]
    fn held_and_pressed_keys_by_command() {
        let b = binds(&[("f", "+duck"), ("mouse4", "+duck"), ("b", "buymenu")]);
        let mut keys = ButtonInput::<KeyCode>::default();
        let mut mouse = ButtonInput::<MouseButton>::default();
        assert!(!pressed(&b, &keys, &mouse, "+duck"));
        mouse.press(MouseButton::Back);
        assert!(pressed(&b, &keys, &mouse, "+duck"));
        keys.press(KeyCode::KeyB);
        assert!(just_pressed(&b, &keys, &mouse, "buymenu"));
        keys.clear();
        mouse.clear();
        assert!(!just_pressed(&b, &keys, &mouse, "buymenu"));
        assert_eq!(first_pressed(&keys, &mouse, None), None);
        keys.press(KeyCode::Numpad5);
        assert_eq!(first_pressed(&keys, &mouse, None), Some("kp_5"));
    }
}
