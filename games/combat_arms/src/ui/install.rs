//! The game's own UI data from the install (specs/combat_arms/
//! ui_system.md L1, R1, D): with the V1 key, the layouts in XML.rez
//! (`CBALAYOUT` files, read by `layout::xml`), the string table of the
//! player's language in Language_Pack.rez and the art those layouts name
//! (the `UI_*` archives' `.dtx` files, decoded by `lithtech_formats::dtx`)
//! take the place of ours (data/ui/); without them ours stay.
//!
//! - Layouts: the main HUD (`HUD_FILE`, whose panels `ui::hud::install`
//!   binds by name) and the in-game menu (`MENU_FILE`, `ui::menu`).
//! - Strings (R1): `language_pack/language_<code>.txt`, the code from
//!   `CHIMERA_CA_LANGUAGE` (ENG, POR, RUS, TUR, DEU, ITA, POL, ARB, SCN),
//!   else the system's language (`LC_ALL`, `LC_MESSAGES`, `LANG`: its
//!   two-letter code), else ENG; a missing table falls back to ENG. A
//!   value written `#key#fallback` takes its fallback (R1: the per-user
//!   override file isn't there). Our strings fill the ids the game's
//!   table lacks (ours: the pickup prompt).
//! - Fonts (T1): a layout's font missing from our list is added from the
//!   face its name's prefix names (the number its height).
//! - Art: every texture the two layouts name (buttons' four states), the
//!   HUD's stance silhouettes and fire-mode icons, and the kill feed's
//!   weapon icons and badges (`EXTRA_ART`, `kill_icon`), decoded to RGBA
//!   by their names (`texture_key`: lower case, `/`, `.dtx` added).
//!
//! Known shortcuts: the per-user override file and the pack's
//! per-language texture table (`localresource_<code>.txt`) aren't read;
//! the language comes from the environment, not an options screen; the
//! install's font list (`attributes/fontlist.txt`) isn't read (ours names
//! the same faces; heights from the names).

use std::{collections::HashMap, sync::Arc};

use bevy::{log::info, math::IVec2};

use super::layout::{Font, FontList, Kind, Layout, Strings, xml};
use crate::mount::Mount;

/// The main HUD layout (H: `character`, `weaponRoot`, `KillDeath`, ...).
pub const HUD_FILE: &str = "xml/hud_main.xml";
/// The in-game menu's layout (Esc, H).
pub const MENU_FILE: &str = "xml/popup_exit_ingame.xml";

/// Textures the HUD code picks by state (stance silhouettes, fire-mode
/// icons) and the kill feed's badges.
pub const EXTRA_ART: &[&str] = &[
    "ui_hud_character/icon/stand_nor.dtx",
    "ui_hud_character/icon/crouch_nor.dtx",
    "ui_hud_weapon/icon/firemode_1.dtx",
    "ui_hud_weapon/icon/firemode_2.dtx",
    "ui_hud_weapon/icon/firemode_3.dtx",
    "ui_hud_weapon/icon/firemode_auto.dtx",
    "ui_hud_message/killdeath/icon/revenge.dtx",
    "ui_hud_message/killdeath/icon/headshot.dtx",
    "ui_hud_message/killdeath/icon/nutshot.dtx",
    "ui_hud_message/killdeath/icon/flag_log_team.dtx",
    "ui_hud_message/killdeath/icon/vip_vip.dtx",
];

/// A texture's key: its archive path in lower case with `/`, `.dtx`
/// added to a name without an extension (a button's state texture).
pub fn texture_key(name: &str) -> String {
    let mut k = name.trim().replace('\\', "/").to_lowercase();
    let file = k.rsplit('/').next().unwrap_or("");
    if !file.contains('.') {
        k.push_str(".dtx");
    }
    k
}

/// A kill-feed weapon icon's texture for weapon `id` (`combat_arms:m4a1`
/// -> `kill_message_m4a1.dtx`).
pub fn kill_icon(id: &str) -> String {
    let short = id.rsplit(':').next().unwrap_or(id);
    format!("ui_hud_message/killdeath/weapon/kill_message_{short}.dtx")
}

/// A decoded texture.
#[derive(Clone)]
pub struct Art {
    pub size: IVec2,
    pub rgba: Vec<u8>,
}

/// The install's UI data (see the module doc).
#[derive(Clone, Default)]
pub struct InstallUi {
    pub hud: Option<Layout>,
    pub menu: Option<Layout>,
    pub strings: Option<Strings>,
    pub language: String,
    /// By `texture_key`.
    pub art: HashMap<String, Art>,
}

impl std::fmt::Debug for InstallUi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstallUi")
            .field("hud", &self.hud.is_some())
            .field("menu", &self.menu.is_some())
            .field("strings", &self.strings.as_ref().map(|s| s.values.len()))
            .field("language", &self.language)
            .field("art", &self.art.len())
            .finish()
    }
}

/// The two-letter language codes the pack's codes stand for.
const LANGUAGES: &[(&str, &str)] = &[
    ("en", "ENG"),
    ("pt", "POR"),
    ("ru", "RUS"),
    ("tr", "TUR"),
    ("de", "DEU"),
    ("it", "ITA"),
    ("pl", "POL"),
    ("ar", "ARB"),
    ("zh", "SCN"),
];

/// The player's language code (see the module doc).
pub fn language() -> String {
    if let Ok(code) = std::env::var("CHIMERA_CA_LANGUAGE")
        && !code.trim().is_empty()
    {
        return code.trim().to_uppercase();
    }
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        let Ok(v) = std::env::var(var) else { continue };
        let two: String = v.chars().take(2).collect::<String>().to_lowercase();
        if let Some((_, code)) = LANGUAGES.iter().find(|(l, _)| *l == two) {
            return code.to_string();
        }
    }
    "ENG".into()
}

/// A value's `#key#fallback` form as its fallback.
fn plain(v: &str) -> String {
    if let Some(rest) = v.strip_prefix('#')
        && let Some((_, fallback)) = rest.split_once('#')
    {
        return fallback.to_string();
    }
    v.to_string()
}

/// The string table for `language` (ENG when it has none), values in
/// their plain form.
pub fn strings(mount: &Mount, language: &str) -> Option<Strings> {
    let read = |code: &str| mount.read(&format!("language_pack/language_{}.txt", code.to_lowercase())).ok();
    let bytes = read(language).or_else(|| read("ENG"))?;
    let mut t = Strings::parse(&String::from_utf8_lossy(&bytes));
    for v in t.values.values_mut() {
        *v = plain(v);
    }
    Some(t)
}

/// Every font a layout names, added to `fonts` when missing: the face
/// whose prefix the name starts with (any case), the number its height.
pub fn add_fonts(layout: &Layout, fonts: &mut FontList) {
    for id in layout.tree.order() {
        let Kind::Text(t) = &layout.widget(id).kind else { continue };
        if fonts.get(&t.font).is_some() {
            continue;
        }
        let mut parts = t.font.rsplitn(2, ' ');
        let (Some(h), Some(prefix)) = (parts.next().and_then(|h| h.trim().parse::<i32>().ok()), parts.next()) else {
            continue;
        };
        let face = fonts
            .faces
            .iter()
            .find(|f| f.prefix.eq_ignore_ascii_case(prefix.trim()))
            .or(fonts.faces.first())
            .cloned();
        if let Some(face) = face {
            fonts.add(Font {
                name: t.font.clone(),
                height: h,
                average: (h as f32 * face.average).round().max(1.0) as i32,
                face: face.role.clone(),
            });
        }
    }
}

/// The textures a layout names (each button's four states).
pub fn layout_textures(layout: &Layout) -> Vec<String> {
    let mut out = Vec::new();
    for id in layout.tree.order() {
        match &layout.widget(id).kind {
            Kind::Window(l) | Kind::Image(l) => out.push(l.texture.clone()),
            Kind::Button(b) => out.extend(['A', 'B', 'C', 'D'].map(|s| format!("{}{s}", b.prefix))),
            Kind::Gauge(g) => out.extend([g.frame.clone(), g.bar.clone()]),
            Kind::List(l) | Kind::Column(l) => out.push(l.back.clone()),
            _ => {}
        }
    }
    out.retain(|t| !t.is_empty());
    out
}

/// A texture decoded (its first level), None when missing or unreadable.
pub fn decode(mount: &Mount, key: &str) -> Option<Art> {
    let bytes = mount.read(key).ok()?;
    let dtx = lithtech_formats::dtx::Dtx::parse(bytes).ok()?;
    let (w, h) = dtx.level_dims(0);
    Some(Art {
        size: IVec2::new(w as i32, h as i32),
        rgba: dtx.rgba(0),
    })
}

fn layout(mount: &Mount, path: &str) -> Option<Layout> {
    let bytes = mount.read(path).ok()?;
    match xml::read(&String::from_utf8_lossy(&bytes)) {
        Ok(l) => Some(l),
        Err(e) => {
            bevy::log::warn!("combat_arms ui: {path}: {e}; ours instead");
            None
        }
    }
}

/// Read the install's UI data through `mount`; None when its layouts
/// can't be read (no install, or XML.rez locked without the V1 key).
pub fn load(mount: &Mount, extra_art: &[String]) -> Option<InstallUi> {
    let hud = layout(mount, HUD_FILE);
    let menu = layout(mount, MENU_FILE);
    if hud.is_none() && menu.is_none() {
        return None;
    }
    let language = language();
    let strings = strings(mount, &language);
    let mut names: Vec<String> = hud.iter().chain(menu.iter()).flat_map(layout_textures).collect();
    names.extend(EXTRA_ART.iter().map(|s| s.to_string()));
    names.extend(extra_art.iter().cloned());
    let mut art = HashMap::new();
    for n in names {
        let key = texture_key(&n);
        if art.contains_key(&key) {
            continue;
        }
        if let Some(a) = decode(mount, &key) {
            art.insert(key, a);
        }
    }
    info!(
        "combat_arms ui: the install's layouts (HUD {}, menu {}), {} strings ({language}), {} textures",
        hud.is_some(),
        menu.is_some(),
        strings.as_ref().map_or(0, |s| s.values.len()),
        art.len()
    );
    Some(InstallUi {
        hud,
        menu,
        strings,
        language,
        art,
    })
}

/// The install's UI data for this process: None without an install, with
/// `game::data::use_builtin_data_only`, or when its layouts are locked.
pub fn installed() -> Option<Arc<InstallUi>> {
    static UI: std::sync::OnceLock<Option<Arc<InstallUi>>> = std::sync::OnceLock::new();
    UI.get_or_init(|| {
        if crate::game::data::builtin_only() {
            return None;
        }
        let mount = crate::game::INFO.mount()?;
        let icons: Vec<String> = crate::game::data::builtin_weapon_ids().iter().map(|id| kill_icon(id)).collect();
        load(&mount, &icons).map(Arc::new)
    })
    .clone()
}
