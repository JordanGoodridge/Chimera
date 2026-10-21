//! The profile's key tokens (specs/combat_arms/options.md, Units and
//! conventions; K2): a keyboard key is `##n` (the PC set-1 scan code,
//! 200 and up for the extended keys), a mouse button `##3` (left) or
//! `##4` (right) on the mouse slot, the wheel `#U` or `#D`. This maps a
//! token to Chimera's key name (what `Console::binds` holds) and to the
//! text the list shows (the key-name attribute file's `TriggerName`, else
//! a built-in name).
//!
//! Known shortcuts: the middle and side mouse buttons' tokens (`##5` to
//! `##7`) are guessed (the spec names only left and right); the grave key
//! (`##41`) has a token but no Chimera key (the console's), so a bind on
//! it is stored and shown but never fires; the joystick slot is kept as
//! the profile gives it and never read.

use lithtech_formats::attributes::Document;

/// The key-name attribute file in the attributes archive (K2).
pub const KEY_NAMES_FILE: &str = "attributes/keymappingstring.txt";

/// Escape, Pause and F8: never capturable (K3).
pub const RESERVED: &[&str] = &["##1", "##197", "##66"];

/// The wheel's tokens.
pub const WHEEL_UP: &str = "#U";
pub const WHEEL_DOWN: &str = "#D";

/// Keyboard tokens: scan code, Chimera's key name, the list's built-in
/// text.
pub const KEYBOARD: &[(u16, &str, &str)] = &[
    (1, "escape", "ESC"),
    (2, "1", "1"),
    (3, "2", "2"),
    (4, "3", "3"),
    (5, "4", "4"),
    (6, "5", "5"),
    (7, "6", "6"),
    (8, "7", "7"),
    (9, "8", "8"),
    (10, "9", "9"),
    (11, "0", "0"),
    (12, "-", "-"),
    (13, "=", "="),
    (14, "backspace", "BACKSPACE"),
    (15, "tab", "TAB"),
    (16, "q", "Q"),
    (17, "w", "W"),
    (18, "e", "E"),
    (19, "r", "R"),
    (20, "t", "T"),
    (21, "y", "Y"),
    (22, "u", "U"),
    (23, "i", "I"),
    (24, "o", "O"),
    (25, "p", "P"),
    (26, "[", "["),
    (27, "]", "]"),
    (28, "enter", "ENTER"),
    (29, "ctrl", "LEFT CTRL"),
    (30, "a", "A"),
    (31, "s", "S"),
    (32, "d", "D"),
    (33, "f", "F"),
    (34, "g", "G"),
    (35, "h", "H"),
    (36, "j", "J"),
    (37, "k", "K"),
    (38, "l", "L"),
    (39, "semicolon", ";"),
    (40, "'", "'"),
    (41, "`", "`"),
    (42, "shift", "LEFT SHIFT"),
    (43, "\\", "\\"),
    (44, "z", "Z"),
    (45, "x", "X"),
    (46, "c", "C"),
    (47, "v", "V"),
    (48, "b", "B"),
    (49, "n", "N"),
    (50, "m", "M"),
    (51, ",", ","),
    (52, ".", "."),
    (53, "/", "/"),
    (54, "rshift", "RIGHT SHIFT"),
    (55, "kp_multiply", "NUM *"),
    (56, "alt", "LEFT ALT"),
    (57, "space", "SPACE"),
    (58, "capslock", "CAPS LOCK"),
    (59, "f1", "F1"),
    (60, "f2", "F2"),
    (61, "f3", "F3"),
    (62, "f4", "F4"),
    (63, "f5", "F5"),
    (64, "f6", "F6"),
    (65, "f7", "F7"),
    (66, "f8", "F8"),
    (67, "f9", "F9"),
    (68, "f10", "F10"),
    (71, "kp_home", "NUM 7"),
    (72, "kp_uparrow", "NUM 8"),
    (73, "kp_pgup", "NUM 9"),
    (74, "kp_minus", "NUM -"),
    (75, "kp_leftarrow", "NUM 4"),
    (76, "kp_5", "NUM 5"),
    (77, "kp_rightarrow", "NUM 6"),
    (78, "kp_plus", "NUM +"),
    (79, "kp_end", "NUM 1"),
    (80, "kp_downarrow", "NUM 2"),
    (81, "kp_pgdn", "NUM 3"),
    (82, "kp_ins", "NUM 0"),
    (83, "kp_del", "NUM ."),
    (87, "f11", "F11"),
    (88, "f12", "F12"),
    (156, "kp_enter", "NUM ENTER"),
    (157, "rctrl", "RIGHT CTRL"),
    (181, "kp_slash", "NUM /"),
    (184, "ralt", "RIGHT ALT"),
    (197, "pause", "PAUSE"),
    (199, "home", "HOME"),
    (200, "uparrow", "UP"),
    (201, "pgup", "PAGE UP"),
    (203, "leftarrow", "LEFT"),
    (205, "rightarrow", "RIGHT"),
    (207, "end", "END"),
    (208, "downarrow", "DOWN"),
    (209, "pgdn", "PAGE DOWN"),
    (210, "ins", "INSERT"),
    (211, "del", "DELETE"),
];

/// Mouse tokens: number, Chimera's name, the list's built-in text.
pub const MOUSE: &[(u16, &str, &str)] = &[
    (3, "mouse1", "LEFT MOUSE"),
    (4, "mouse2", "RIGHT MOUSE"),
    (5, "mouse3", "MIDDLE MOUSE"),
    (6, "mouse4", "MOUSE 4"),
    (7, "mouse5", "MOUSE 5"),
];

/// The keyboard token of a scan code.
pub fn token(code: u16) -> String {
    format!("##{code}")
}

/// The number in a `##n` token.
pub fn number(token: &str) -> Option<u16> {
    token.trim().strip_prefix("##")?.parse().ok()
}

/// Chimera's key name for a keyboard-slot token.
pub fn keyboard_name(token: &str) -> Option<&'static str> {
    let n = number(token)?;
    KEYBOARD.iter().find(|(c, ..)| *c == n).map(|(_, name, _)| *name)
}

/// Chimera's key name for a mouse-slot token (a button or the wheel).
pub fn mouse_name(token: &str) -> Option<&'static str> {
    match token.trim() {
        WHEEL_UP => Some("mwheelup"),
        WHEEL_DOWN => Some("mwheeldown"),
        t => {
            let n = number(t)?;
            MOUSE.iter().find(|(c, ..)| *c == n).map(|(_, name, _)| *name)
        }
    }
}

/// The keyboard token of Chimera's key name.
pub fn keyboard_token(name: &str) -> Option<String> {
    KEYBOARD.iter().find(|(_, n, _)| *n == name).map(|(c, ..)| token(*c))
}

/// The mouse token of Chimera's mouse button or wheel name.
pub fn mouse_token(name: &str) -> Option<String> {
    match name {
        "mwheelup" => Some(WHEEL_UP.into()),
        "mwheeldown" => Some(WHEEL_DOWN.into()),
        n => MOUSE.iter().find(|(_, x, _)| *x == n).map(|(c, ..)| token(*c)),
    }
}

/// The key-name attribute file's names (K2): sections `Keyboard0`,
/// `Keyboard1`, ... and `Mouse0`, ..., read until the first missing
/// number, each with `RealName` (the stored token) and `TriggerName` (the
/// text shown, trimmed).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeyNames {
    pub keyboard: Vec<(String, String)>,
    pub mouse: Vec<(String, String)>,
}

impl KeyNames {
    /// Read the file's records.
    pub fn parse(doc: &Document) -> Self {
        let list = |prefix: &str| {
            doc.records(prefix)
                .into_iter()
                .map(|s| {
                    let text = |k: &str| s.string(k).map(|v| String::from_utf8_lossy(v).trim().to_string()).unwrap_or_default();
                    (text("RealName"), text("TriggerName"))
                })
                .collect()
        };
        Self {
            keyboard: list("Keyboard"),
            mouse: list("Mouse"),
        }
    }

    /// The names with each text that is a string id (`$Keyboard_Name_4`)
    /// looked up by `lookup`; an id it cannot resolve (it returns the id)
    /// is dropped, so the built-in name shows instead of the raw id.
    pub fn resolved(&self, lookup: impl Fn(&str) -> String) -> Self {
        let fix = |list: &[(String, String)]| -> Vec<(String, String)> {
            list.iter()
                .map(|(real, t)| {
                    let t = if t.starts_with('$') { lookup(t) } else { t.clone() };
                    // "ABCDE" is the table's placeholder for names it has
                    // not been given.
                    (real.clone(), if t.starts_with('$') || t == "ABCDE" { String::new() } else { t })
                })
                .collect()
        };
        Self { keyboard: fix(&self.keyboard), mouse: fix(&self.mouse) }
    }

    /// The text of a keyboard token: the file's, else the built-in name,
    /// else the token itself (K2: a token missing from the file shows the
    /// device's own control name).
    pub fn keyboard_text(&self, token: &str) -> String {
        self.keyboard
            .iter()
            .find(|(real, t)| real == token && !t.is_empty())
            .map(|(_, t)| t.clone())
            .or_else(|| {
                let n = number(token)?;
                KEYBOARD.iter().find(|(c, ..)| *c == n).map(|(_, _, t)| t.to_string())
            })
            .unwrap_or_else(|| token.to_string())
    }

    /// The text of a mouse token.
    pub fn mouse_text(&self, token: &str) -> String {
        self.mouse
            .iter()
            .find(|(real, t)| real == token && !t.is_empty())
            .map(|(_, t)| t.clone())
            .or_else(|| match token {
                WHEEL_UP => Some("WHEEL UP".into()),
                WHEEL_DOWN => Some("WHEEL DOWN".into()),
                t => {
                    let n = number(t)?;
                    MOUSE.iter().find(|(c, ..)| *c == n).map(|(_, _, t)| t.to_string())
                }
            })
            .unwrap_or_else(|| token.to_string())
    }
}

/// The install's key names (`KEY_NAMES_FILE`); the built-in names without
/// an install (or with `data::use_builtin_data_only`).
pub fn read_installed() -> KeyNames {
    if super::super::data::builtin_only() {
        return KeyNames::default();
    }
    super::super::INFO
        .mount()
        .and_then(|m| m.read(KEY_NAMES_FILE).ok())
        .and_then(|b| Document::parse(&b).ok())
        .map(|d| KeyNames::parse(&d))
        .unwrap_or_default()
}
