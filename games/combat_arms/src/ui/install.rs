//! The game's own UI data from the install (specs/combat_arms/
//! ui_system.md L1, R1, D): with the V1 key, the layouts in XML.rez
//! (`CBALAYOUT` files, read by `layout::xml`), the string table of the
//! player's language in Language_Pack.rez and the art those layouts name
//! (the `UI_*` archives' `.dtx` files, decoded by `lithtech_formats::dtx`)
//! take the place of ours (data/ui/); without them ours stay.
//!
//! - Layouts: the death screen (`KILLCAM_FILE`, `ui::hud::death`), the
//!   main HUD (`HUD_FILE`, whose panels `ui::hud::install`
//!   and `ui::hud::bound` bind by name), the mode score panels
//!   (`INDICATOR_FILE`), the scoreboard's frame and lists
//!   (`RESULT_FILE`, `TEAM_LIST_FILE`, `NO_TEAM_LIST_FILE`,
//!   `ui::scoreboard`), the loading screen (`LOADING_FILE`,
//!   `ui::loading`) and the in-game menu (`MENU_FILE`, `ui::menu`).
//! - The HUD layout attributes (`LAYOUT_ATTRIBUTES`' `[HUDLayout0]` and
//!   `[Observe_Mode]`,
//!   `hud_layout`): the textures the HUD code draws by name
//!   (`HUD_LAYOUT_ART`: the hit overlay and direction indicator, the kill
//!   feed's special icons and badges, the result banners, the action
//!   icons).
//! - Strings (R1): `language_pack/language_<code>.txt`, the code from
//!   `CHIMERA_CA_LANGUAGE` (ENG, POR, RUS, TUR, DEU, ITA, POL, ARB, SCN),
//!   else the system's language (`LC_ALL`, `LC_MESSAGES`, `LANG`: its
//!   two-letter code), else ENG; a missing table falls back to ENG. A
//!   value written `#key#fallback` takes its fallback (R1: the per-user
//!   override file isn't there). Our strings fill the ids the game's
//!   table lacks (ours: the pickup prompt).
//! - Fonts (T1): the install's font list (`FONT_LIST`: `SysFont#`'s
//!   name, file, height and width), each in our face of the same file
//!   (`apply_font_list`); a layout's font still missing is added from
//!   the face its name's prefix names (the number its height).
//! - Art: every texture the two layouts name (buttons' four states), the
//!   HUD's stance silhouettes and fire-mode icons, the kill feed's
//!   weapon icons and badges (`EXTRA_ART`, `kill_icon`) and the gauge
//!   sight's images (`GAUGE_SIGHT_PREFIXES` with 0 to 100), decoded to
//!   RGBA by their names (`texture_key`: lower case, `/`, `.dtx` added).
//!
//! The pack's per-language texture table (`localresource_<code>.txt`)
//! isn't read: the game applies none (ui_system.md R1, Corrections 8),
//! textures are drawn by their own names in every language.
//!
//! Known shortcuts: the per-user override file isn't read, nor the client
//! event attributes' event table (the one texture-name swap the game
//! has: no events are configured); the language comes
//! from the environment, not an options screen; a font's average glyph
//! width (when its `Width` is 0) is our face's estimate; the map
//! pictures are decoded when the loading screen opens, not here (they
//! are large).

use std::{collections::HashMap, sync::Arc};

use bevy::{log::info, math::IVec2};

use super::layout::{Font, FontList, Kind, Layout, Strings, xml};
use crate::mount::Mount;

/// The main HUD layout (H: `character`, `weaponRoot`, `KillDeath`, ...).
pub const HUD_FILE: &str = "xml/hud_main.xml";
/// The in-game menu's layout (Esc, H).
pub const MENU_FILE: &str = "xml/popup_exit_ingame.xml";
/// The mode score panels' layout (H: one `indicator` control, a child per
/// mode).
pub const INDICATOR_FILE: &str = "xml/hud_indicator.xml";
/// The scoreboard's layouts (H): the common frame (`Result_Normal`), the
/// team list (`Team_UserList`) and the no-team list.
pub const RESULT_FILE: &str = "xml/hud_result.xml";
pub const TEAM_LIST_FILE: &str = "xml/hud_result_team.xml";
pub const NO_TEAM_LIST_FILE: &str = "xml/hud_result_noteam.xml";
/// The loading screen's layout (H).
pub const LOADING_FILE: &str = "xml/hud_maploading.xml";
/// The death screen's layout (`Hud_KillCam`, root `KillCam`;
/// death_killcam.md 8).
pub const KILLCAM_FILE: &str = "xml/hud_killcam.xml";
/// The attribute file with the HUD layout attributes (`[HUDLayout0]`:
/// the damage overlays' and kill feed's textures, the result banners)
/// and the font list (T1).
pub const LAYOUT_ATTRIBUTES: &str = "attributes/layout.txt";
pub const FONT_LIST: &str = "attributes/fontlist.txt";
/// The HUD layout attributes' section.
pub const HUD_LAYOUT: &str = "HUDLayout0";
/// The section with the death screen's zone icons
/// (`DamagePointIcon<Zone>Tex`; death_killcam.md 8), read into the same
/// map as `HUD_LAYOUT`'s.
pub const OBSERVE_MODE: &str = "Observe_Mode";
/// What the death screen draws from those attributes: the hit-zone icons
/// and the respawn gauge's bars (`[HUDLayout0]`'s).
pub const DEATH_LAYOUT_ART: &[&str] = &[
    "damagepointiconheadtex",
    "damagepointiconbodytex",
    "damagepointiconarmstex",
    "damagepointiconlegtex",
    "damagepointiconnuttex",
    "respawngauge_normal",
    "respawngauge_perk",
];

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
    pub indicator: Option<Layout>,
    pub result: Option<Layout>,
    pub team_list: Option<Layout>,
    pub no_team_list: Option<Layout>,
    pub loading: Option<Layout>,
    /// The death screen (`ui::hud::death`).
    pub kill_cam: Option<Layout>,
    /// The HUD layout attributes' strings (`[HUDLayout0]`), by key in
    /// lower case: texture names.
    pub hud_layout: HashMap<String, String>,
    /// The install's font list (T1), in its order.
    pub fonts: Vec<InstallFont>,
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
            .field("indicator", &self.indicator.is_some())
            .field("scoreboard", &self.team_list.is_some())
            .field("loading", &self.loading.is_some())
            .field("kill_cam", &self.kill_cam.is_some())
            .field("fonts", &self.fonts.len())
            .field("strings", &self.strings.as_ref().map(|s| s.values.len()))
            .field("language", &self.language)
            .field("art", &self.art.len())
            .finish()
    }
}

/// One font of the install's font list (T1: `SysFont#`'s `Name`,
/// `FileName`, `Height` and `Width`, 0 natural).
#[derive(Clone, Debug, PartialEq)]
pub struct InstallFont {
    /// Its section's number (`SysFont<n>`).
    pub number: usize,
    pub name: String,
    pub file: String,
    pub height: i32,
    pub width: i32,
}

/// The font list (T1): `SysFont0`, `SysFont1`, ... up to the first
/// missing number.
pub fn font_list(mount: &Mount) -> Vec<InstallFont> {
    let Some(doc) = mount
        .read(FONT_LIST)
        .ok()
        .and_then(|b| lithtech_formats::attributes::Document::parse(&b).ok())
    else {
        return Vec::new();
    };
    let text = |s: &lithtech_formats::attributes::Section, k: &str| {
        s.string(k).map(|v| String::from_utf8_lossy(v).trim().to_string()).unwrap_or_default()
    };
    doc.records("SysFont")
        .into_iter()
        .enumerate()
        .map(|(number, s)| InstallFont {
            number,
            name: text(s, "Name"),
            file: text(s, "FileName"),
            height: s.int("Height", 0),
            width: s.int("Width", 0),
        })
        .filter(|f| !f.name.is_empty() && f.height > 0)
        .collect()
}

/// The install's fonts over `fonts` (T1: a layout's Font matches a name
/// exactly): each in the face whose file is its file (any case and
/// slashes; else the face its name's prefix names), its height, its
/// width when given (else the face's average for the height).
pub fn apply_font_list(list: &[InstallFont], fonts: &mut FontList) {
    let norm = |p: &str| p.trim().replace('\\', "/").to_lowercase();
    for f in list {
        let prefix = f.name.rsplit_once(' ').map_or(f.name.as_str(), |(p, _)| p);
        let face = fonts
            .faces
            .iter()
            .find(|x| x.file.as_deref().is_some_and(|file| norm(file) == norm(&f.file) && x.prefix.eq_ignore_ascii_case(prefix)))
            .or_else(|| fonts.faces.iter().find(|x| x.file.as_deref().is_some_and(|file| norm(file) == norm(&f.file))))
            .or_else(|| fonts.faces.iter().find(|x| x.prefix.eq_ignore_ascii_case(prefix)))
            .cloned();
        let Some(face) = face else { continue };
        let average = if f.width > 0 { f.width } else { (f.height as f32 * face.average).round().max(1.0) as i32 };
        fonts.add(Font {
            name: f.name.clone(),
            height: f.height,
            average,
            face: face.role.clone(),
        });
    }
}

/// The HUD layout attributes' strings (`HUD_LAYOUT` of
/// `LAYOUT_ATTRIBUTES`), keys in lower case.
pub fn hud_layout(mount: &Mount) -> HashMap<String, String> {
    let Some(doc) = mount
        .read(LAYOUT_ATTRIBUTES)
        .ok()
        .and_then(|b| lithtech_formats::attributes::Document::parse(&b).ok())
    else {
        return HashMap::new();
    };
    [HUD_LAYOUT, OBSERVE_MODE]
        .into_iter()
        .filter_map(|name| doc.section(name))
        .flat_map(|s| s.iter())
        .filter_map(|(k, v)| match v {
            lithtech_formats::attributes::Value::Str(b) => {
                Some((k.to_lowercase(), String::from_utf8_lossy(b).trim().to_string()))
            }
            _ => None,
        })
        .collect()
}

/// The HUD layout textures the HUD code draws (`[HUDLayout0]` keys):
/// the damage overlays, the kill feed's special icons and badges, the
/// round-result banners, the stance silhouettes and the action icons.
pub const HUD_LAYOUT_ART: &[&str] = &[
    "damagetex",
    "damageindicatortex",
    "killsuicideimg",
    "killdrumdeathimg",
    "killburndeathimg",
    "killpoisondeathimg",
    "killbleedingdeathimg",
    "killerisnpcimg",
    "killrevengeimg",
    "killheadshotimg",
    "killnutshotimg",
    "killenemyflagimg",
    "killteamflagimg",
    "killenemygoldimg",
    "killicecrushimg",
    "killlogviprankimg1",
    "scoreboardwintex",
    "scoreboardlosetex",
    "scoreboarddrawtex",
    "scoreboardffawintex",
    "scoreboardffalosetex",
    "guage_bombplant_tex",
    "guage_bombdefuse_tex",
    "guage_teamchagne_tex",
    "humanroundwintex",
    "infectorroundwintex",
];

/// The gauge sight's image name prefixes (`[HUDLayout0]` keys, lower
/// case; hud_crosshair.md A12, Corrections 13).
pub const GAUGE_SIGHT_PREFIXES: &[&str] = &["crosshairlefttex", "crosshairrighttex"];

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
    let (indicator, result) = (layout(mount, INDICATOR_FILE), layout(mount, RESULT_FILE));
    let (team_list, no_team_list) = (layout(mount, TEAM_LIST_FILE), layout(mount, NO_TEAM_LIST_FILE));
    let loading = layout(mount, LOADING_FILE);
    let kill_cam = layout(mount, KILLCAM_FILE);
    let hud_layout = hud_layout(mount);
    let fonts = font_list(mount);
    let mut names: Vec<String> = [&hud, &menu, &indicator, &result, &team_list, &no_team_list, &loading, &kill_cam]
        .into_iter()
        .flatten()
        .flat_map(layout_textures)
        .collect();
    names.extend(EXTRA_ART.iter().map(|s| s.to_string()));
    names.extend(HUD_LAYOUT_ART.iter().filter_map(|k| hud_layout.get(*k).cloned()));
    names.extend(DEATH_LAYOUT_ART.iter().filter_map(|k| hud_layout.get(*k).cloned()));
    // The death screen's tips' banners.
    if let Ok(bytes) = mount.read(crate::game::death_cam::GUIDE_FILE)
        && let Ok(doc) = lithtech_formats::attributes::Document::parse(&bytes)
    {
        names.extend(crate::game::death_cam::screen::parse_guide(&doc).into_iter().map(|t| t.icon).filter(|i| !i.is_empty()));
    }
    // The gauge sight's images: each prefix with 0, 10, ... 100.
    for k in GAUGE_SIGHT_PREFIXES {
        if let Some(prefix) = hud_layout.get(*k) {
            names.extend((0..=10).map(|n| format!("{}{}.dtx", prefix.trim(), n * 10)));
        }
    }
    names.extend(extra_art.iter().cloned());
    let mut art = HashMap::new();
    for n in names {
        let key = texture_key(&n);
        if art.contains_key(&key) || key.contains("/mapimage/") {
            continue;
        }
        if let Some(a) = decode(mount, &key) {
            art.insert(key, a);
        }
    }
    info!(
        "combat_arms ui: the install's layouts (HUD {}, menu {}, score panels {}, scoreboard {}, loading {}), {} strings ({language}), {} fonts, {} textures",
        hud.is_some(),
        menu.is_some(),
        indicator.is_some(),
        team_list.is_some(),
        loading.is_some(),
        strings.as_ref().map_or(0, |s| s.values.len()),
        fonts.len(),
        art.len()
    );
    Some(InstallUi {
        hud,
        menu,
        indicator,
        result,
        team_list,
        no_team_list,
        loading,
        kill_cam,
        hud_layout,
        fonts,
        strings,
        language,
        art,
    })
}

impl InstallUi {
    /// A HUD layout attribute's texture (`[HUDLayout0]` key, any case),
    /// as a texture key.
    pub fn hud_texture(&self, key: &str) -> Option<String> {
        self.hud_layout.get(&key.to_lowercase()).map(|t| texture_key(t))
    }

    /// A `[HUDLayout0]` texture name prefix by key (lower case, `/`, no
    /// extension added): the gauge sight's `CrossHairLeftTex` and
    /// `CrossHairRightTex` (hud_crosshair.md A12).
    pub fn hud_prefix(&self, key: &str) -> Option<String> {
        self.hud_layout
            .get(&key.to_lowercase())
            .map(|t| t.trim().replace('\\', "/").to_lowercase())
    }
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
