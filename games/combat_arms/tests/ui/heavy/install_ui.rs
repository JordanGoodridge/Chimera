//! The install's own UI data through the UI system's readers
//! (specs/combat_arms/ui_system.md L1 and R1): every layout in XML.rez
//! reads as a widget tree (`layout::xml::read`), the language pack's
//! string tables (Language_Pack.rez) as string tables
//! (`layout::Strings::parse`). Both archives are V1-keyed: skipped
//! without the key (or without an install in chimera.local.toml).

use chimera_core::mount::{Mount, config::LocalConfig};
use combat_arms::{
    game::{GAME, INFO, mount},
    ui::{
        install,
        layout::{Strings, xml},
    },
};

fn install() -> Option<Mount> {
    let Some(install) = LocalConfig::load().ok().and_then(|c| c.game_path(GAME)) else {
        eprintln!("skipping: no Combat Arms install configured");
        return None;
    };
    if !install.join("Game").is_dir() {
        eprintln!("skipping: no Combat Arms install at {}", install.display());
        return None;
    }
    if !mount::keys().is_ok_and(|k| k.contains_key("V1")) {
        eprintln!("skipping: no V1 key in chimera.local.toml");
        return None;
    }
    Some((INFO.open)(&install).unwrap())
}

/// The files of the archive layer named `archive` (its file name, any
/// case).
fn files_of(mount: &Mount, archive: &str) -> Vec<String> {
    let layer = mount
        .layers()
        .find(|l| l.name().to_lowercase().ends_with(&archive.to_lowercase()))
        .unwrap_or_else(|| panic!("no {archive}"));
    let mut files: Vec<String> = layer.entries().into_iter().map(|e| e.path).collect();
    files.sort();
    files
}

fn utf8(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// L1: every `.xml` file of XML.rez that is a layout (`CBALAYOUT` root)
/// reads into a tree with at least one widget; the other XML files are
/// counted by root.
#[test]
fn install_layouts_read() {
    let Some(mount) = install() else { return };
    let files = files_of(&mount, "XML.rez");
    let mut roots = std::collections::BTreeMap::<String, usize>::new();
    let (mut layouts, mut failed) = (0, Vec::new());
    for path in files.iter().filter(|p| p.ends_with(".xml")) {
        let text = utf8(&mount.read(path).unwrap());
        let root = match xml::parse(&text) {
            Ok(e) => e.name.clone(),
            Err(e) => {
                failed.push(format!("{path}: {e}"));
                continue;
            }
        };
        *roots.entry(root.clone()).or_default() += 1;
        if root != "CBALAYOUT" {
            continue;
        }
        match xml::read(&text) {
            Ok(l) => {
                assert!(l.tree.order().len() > 1, "{path}: no widgets");
                layouts += 1;
            }
            Err(e) => failed.push(format!("{path}: {e}")),
        }
    }
    eprintln!("{} files; XML roots {roots:?}; {layouts} layouts read", files.len());
    for f in &failed {
        eprintln!("failed: {f}");
    }
    assert!(failed.is_empty(), "{} of the install's XML files don't read", failed.len());
    assert!(layouts > 100, "{layouts} layouts");
}

fn installed_ui(mount: &Mount) -> (combat_arms::ui::CaUi, std::sync::Arc<install::InstallUi>) {
    let inst = std::sync::Arc::new(install::load(mount, &[install::kill_icon("combat_arms:m4a1")]).expect("the install's UI"));
    (combat_arms::ui::CaUi::with_install(inst.clone()), inst)
}

/// With the keys the HUD's health and weapon panels are the game's HUD
/// layout's (`hud_main.xml`'s `character` and `weaponRoot`), the HUD's
/// values bound to its named controls: the HP value is drawn in
/// `Hp_text`'s place (its window's box, centred), the weapon's name in
/// `name`'s, the clip in `Ammo`'s, the stamina gauge's bar at its full
/// range for full stamina, the stance silhouette is the install's
/// standing image, and the panels' textures (their nine-slice
/// backgrounds) are the install's art. At 1024 x 768 (scale 1).
#[test]
fn install_hud_panels_bind() {
    use combat_arms::ui::{
        hud::{
            StatusView,
            install::{HudPanels, STAND},
            model::{FireIcon, WeaponPanel, health_colours},
        },
        layout::{Draw, Kind},
    };
    let Some(mount) = install() else { return };
    let (ui, inst) = installed_ui(&mount);
    assert_eq!(inst.language, install::language());
    let hud = inst.hud.as_ref().expect("hud_main.xml");
    let mut panels = HudPanels::new(hud).expect("character and weaponRoot");
    let c = panels.controls.clone();
    for (name, id) in [("Hp_text", c.hp), ("name", c.name), ("Ammo", c.clip), ("char_sp_gauge", c.stamina), ("characterStatus", c.silhouette), ("fire_icon", c.fire_icon)] {
        assert!(id.is_some(), "no {name} in the game's HUD layout");
    }
    let status = StatusView {
        hp: "87".into(),
        ap: "45".into(),
        low: false,
        colours: health_colours(false),
        crouched: false,
        stamina_bar: 120.0,
    };
    let weapon = WeaponPanel {
        name: "M4A1".into(),
        ammo: Some(("030".into(), "090".into())),
        icon: Some(FireIcon::Auto),
    };
    panels.bind(Some(&status), Some(&weapon), chimera_client::client::canvas::Scale::ONE);
    let draws = panels.layout.draw(&ui.ctx());
    let text = |s: &str| {
        draws.iter().find_map(|d| match d {
            Draw::Text(t) if t.lines.iter().any(|(_, l)| l == s) => Some(t.clone()),
            _ => None,
        })
    };
    let hp = text("87").expect("the HP value drawn");
    let hp_box = panels.at(panels.layout.tree.get(c.hp.unwrap()).parent.unwrap());
    eprintln!("HP at {:?} (its box at {hp_box})", hp.reference_at);
    assert_eq!(hp.reference_at.y, hp_box.y, "HP at its box's top (DEFAULT vertical)");
    assert!(hp.reference_at.x >= hp_box.x && hp.reference_at.x < hp_box.x + 40, "centred in its 40-wide box");
    let name = text("M4A1").expect("the weapon's name drawn");
    let clip = text("030").expect("the clip drawn");
    eprintln!("name at {:?}, clip at {:?}", name.reference_at, clip.reference_at);
    assert!(name.reference_at.x > 700 && clip.reference_at.y > 680, "the weapon panel bottom right");
    assert!(text("100").is_none(), "the layout's sample values replaced");
    // The gauge's bar at its full range.
    let Kind::Gauge(g) = &panels.layout.widget(c.stamina.unwrap()).kind else { panic!("not a gauge") };
    let bar = draws
        .iter()
        .filter_map(|d| match d {
            Draw::Quad(id, q) if *id == c.stamina.unwrap() && install::texture_key(&q.texture) == install::texture_key(&g.bar) => Some(q.clone()),
            _ => None,
        })
        .next()
        .expect("the stamina bar drawn");
    assert_eq!(bar.size[0] as i32, g.max_range, "{bar:?}");
    // The silhouette and the backgrounds: the install's art.
    let Kind::Image(sil) = &panels.layout.widget(c.silhouette.unwrap()).kind else { panic!() };
    assert_eq!(sil.texture, STAND);
    let quads: Vec<_> = draws.iter().filter_map(|d| if let Draw::Quad(_, q) = d { Some(q) } else { None }).collect();
    assert!(quads.len() >= 8, "{} quads", quads.len());
    for q in &quads {
        assert!(inst.art.contains_key(&install::texture_key(&q.texture)), "{} not in the install's art", q.texture);
    }
    assert!(quads.iter().any(|q| q.slices.is_some()), "nine-slice backgrounds");
    // Hidden: no status, no panel.
    panels.bind(None, None, chimera_client::client::canvas::Scale::ONE);
    assert!(panels.layout.draw(&ui.ctx()).is_empty());
}

/// With the keys the in-game menu is the game's (`popup_exit_ingame.xml`),
/// centred, its buttons bound by name (return to lobby, cancel and the
/// title's close button work; report, record and options disabled) and
/// its labels from the player's string table.
#[test]
fn install_ingame_menu() {
    use combat_arms::ui::{
        layout::Draw,
        menu::{CMD_CANCEL, CMD_LOBBY, INSTALL_BOX, InGameMenu},
    };
    let Some(mount) = install() else { return };
    let (ui, _) = installed_ui(&mount);
    let mut menu = InGameMenu::new(&ui);
    assert_eq!(menu.box_name, INSTALL_BOX);
    let l = menu.layout();
    let lobby = l.find("button_lobby").unwrap();
    assert_eq!(l.widget(lobby).cmd, CMD_LOBBY);
    assert_eq!(l.widget(l.find("button_cancel").unwrap()).cmd, CMD_CANCEL);
    assert!(!l.widget(l.find("button_userreport").unwrap()).enabled);
    menu.screen = bevy::math::IVec2::new(1280, 720);
    menu.place();
    let b = menu.layout().find(INSTALL_BOX).unwrap();
    let at = menu.layout().tree.abs_pos(b);
    assert_eq!(at, bevy::math::IVec2::new(640 - 110, 360 - 87));
    menu.set_open(true);
    let draws = menu.layout().draw(&ui.ctx());
    let texts: Vec<String> = draws
        .iter()
        .filter_map(|d| if let Draw::Text(t) = d { Some(t.lines.iter().map(|l| l.1.clone()).collect::<String>()) } else { None })
        .collect();
    eprintln!("menu texts: {texts:?}");
    assert!(texts.len() >= 5);
    assert!(texts.iter().all(|t| !t.starts_with('$') && !t.starts_with('#')), "{texts:?}");
}

/// R1: the language pack's string tables read, with no duplicate keys
/// that would hide a value, and ids resolve through them (R2).
#[test]
fn install_string_tables_read() {
    let Some(mount) = install() else { return };
    let files = files_of(&mount, "Language_Pack.rez");
    eprintln!("Language_Pack.rez: {} files: {:?}", files.len(), files.iter().take(40).collect::<Vec<_>>());
    let mut tables = 0;
    for path in &files {
        let bytes = mount.read(path).unwrap();
        let Ok(text) = std::str::from_utf8(&bytes) else {
            eprintln!("{path}: not UTF-8 ({} bytes)", bytes.len());
            continue;
        };
        let t = Strings::parse(text);
        eprintln!("{path}: {} strings, {} duplicate keys", t.values.len(), t.duplicates.len());
        if t.values.len() > 100 {
            tables += 1;
        }
    }
    assert!(tables > 0, "no string table in Language_Pack.rez");
}
