//! Who and what the radar and the full map show, and with which icon
//! (specs/combat_arms/radar_minimap.md R6, R9, F4, F5, F6): plain
//! functions on what the viewer and the subject are, no ECS. The icons
//! are the install's files under `UI_HUD_Map\icon\` (`icon`), by the role
//! keys R9 lists.

use super::super::rules::objectives::CaCarry;

/// The directory of the icons in the archives (a texture key's form).
pub const ICON_DIR: &str = "ui_hud_map/icon/";

/// A texture key for the icon file `file` (`character_ally.dtx`).
pub fn icon(file: &str) -> String {
    format!("{ICON_DIR}{}", file.to_lowercase())
}

/// A texture key for an icon the attribute data names: a bare file name is
/// under the icon folder, a path stays as it is.
pub fn map_icon_key(name: &str) -> String {
    let n = name.trim().replace('\\', "/").to_lowercase();
    if n.contains('/') { n } else { icon(&n) }
}

/// The radar art archive's names (R5, R7, R8; the zoom bar and frame).
pub const GUIDE: &str = "ui_hud_radarmap/guide.dtx";
pub const ARROW: &str = "ui_hud_radarmap/icon/locationindicator.dtx";
pub const FRAME: &str = "ui_hud_radarmap/bg.dtx";
pub const MASK: &str = "ui_hud_radarmap/masking.dtx";
pub const ZOOM_TRACK: &str = "ui_hud_radarmap/magnification_bg.dtx";
pub const ZOOM_KNOB: &str = "ui_hud_radarmap/magnification_bar.dtx";
/// The full map's grid over the image (F2).
pub const GRID: &str = "ui_hud_map/dotguide.dtx";
/// A dead player's mark on the full map (54 x 54, F4).
pub const DEATH_MARK: &str = "area_death.dtx";

/// The map image's texture key for the image file `name` the mission
/// names (`UI_HUD_Map\mapImage\Map_Showdown.dtx`).
pub fn map_image_key(name: &str) -> String {
    name.trim().replace('\\', "/").to_lowercase()
}

/// How a player is seen by the one viewing: the icon, the wedge, the flag
/// badge, whether an edge arrow stands in when out of range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Look {
    pub icon: String,
    /// The facing wedge (none for a dead player's mark).
    pub view: Option<String>,
    /// A flag carrier's badge (`character_flag_A.dtx`).
    pub badge: Option<String>,
    pub arrow: bool,
}

/// What the viewing player is (R6's "mine"): its team and whether it
/// carries Spy Hunt's intel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Viewer {
    pub team: u8,
    pub intel: bool,
}

/// What a player shown is, to the viewer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Subject {
    /// Another player on the viewer's team (a teamless player is an
    /// enemy).
    pub ally: bool,
    pub alive: bool,
    pub carry: CaCarry,
    pub vip: bool,
    /// Arms Race: it has entered the last stage.
    pub last_stage: bool,
    /// A shared scan is on for the viewer's team (satellite, system).
    pub shared_scan: bool,
    /// The viewer's own heartbeat sensor has it as a contact.
    pub contact: bool,
    /// A teammate shared it on a heartbeat list.
    pub team_contact: bool,
}

fn carries_bullion(c: &CaCarry) -> bool {
    c.bars > 0
}

/// The flag badge of the flag a player carries: the carried flag's team
/// (`character_flag_A.dtx` for Alpha's flag).
pub fn badge(flag_team: u8) -> String {
    icon(if flag_team == 2 { "character_flag_B.dtx" } else { "character_flag_A.dtx" })
}

fn ally(s: &Subject) -> Look {
    let (mut file, view) = if s.vip {
        ("character_VIP.dtx", "character_VIP_View.dtx")
    } else {
        ("character_ally.dtx", "character_ally_view.dtx")
    };
    let mut badge_ = None;
    let mut arrow = false;
    if carries_bullion(&s.carry) {
        file = "character_ally_money.DTX";
    } else if let Some(f) = s.carry.flag {
        badge_ = Some(badge(f));
        arrow = true;
    } else if s.carry.bomb {
        file = "character_ally_bomb.dtx";
        arrow = true;
    }
    Look {
        icon: icon(file),
        view: Some(icon(view)),
        badge: badge_,
        arrow,
    }
}

/// R6: how the radar shows `s` to the viewer `v`, None when not drawn
/// (the dead; an enemy nothing reveals). The viewer itself is never in
/// the radar's list.
pub fn radar_player(v: &Viewer, s: &Subject) -> Option<Look> {
    if !s.alive {
        return None;
    }
    if s.ally {
        return Some(ally(s));
    }
    enemy(v, s, true)
}

fn enemy(v: &Viewer, s: &Subject, heartbeat: bool) -> Option<Look> {
    let plain = |file: &str, view: &str, badge: Option<String>, arrow: bool| Look {
        icon: icon(file),
        view: Some(icon(view)),
        badge,
        arrow,
    };
    if carries_bullion(&s.carry) {
        return Some(plain("character_enemy_money.dtx", "character_enemy_view.dtx", None, false));
    }
    if v.intel && s.carry.intel > 0 {
        return Some(plain("character_spy_pre.DTX", "character_spy_view.dtx", None, false));
    }
    if let Some(f) = s.carry.flag {
        return Some(plain("character_enemy.dtx", "character_enemy_view.dtx", Some(badge(f)), true));
    }
    if s.carry.bomb {
        return Some(plain("character_enemy_bomb.dtx", "character_enemy_view.dtx", None, true));
    }
    if s.last_stage || s.shared_scan || (heartbeat && (s.contact || s.team_contact)) {
        return Some(plain("character_enemy.dtx", "character_enemy_view.dtx", None, false));
    }
    None
}

/// F4: how the full map shows another player `s` to the viewer: allies
/// always (dead ones as a death icon); enemies by the radar's reasons but
/// the heartbeat contacts.
pub fn map_player(v: &Viewer, s: &Subject) -> Option<Look> {
    if s.ally {
        if !s.alive {
            return Some(Look {
                icon: icon("area_death_ally.DTX"),
                view: None,
                badge: None,
                arrow: false,
            });
        }
        // Carrying the bomb overrides the VIP's icon (F4): `ally` does.
        let mut look = ally(s);
        look.arrow = false;
        return Some(look);
    }
    if !s.alive {
        return None;
    }
    let mut look = enemy(v, s, false)?;
    look.arrow = false;
    Some(look)
}

/// F4: the local player's own icon on the full map.
pub fn map_local(alive: bool, carry: &CaCarry) -> Look {
    if !alive {
        return Look {
            icon: icon("area_death_player.DTX"),
            view: None,
            badge: None,
            arrow: false,
        };
    }
    let file = if carries_bullion(carry) {
        "character_player_money.dtx"
    } else if carry.bomb {
        "character_player_bomb.dtx"
    } else {
        "character_player.dtx"
    };
    Look {
        icon: icon(file),
        view: Some(icon("character_player_view.dtx")),
        badge: carry.flag.map(badge),
        arrow: false,
    }
}

/// R6: an NPC (co-op): normal or elite.
pub fn npc(elite: bool) -> Look {
    Look {
        icon: icon(if elite { "character_NPC03.dtx" } else { "character_NPC.dtx" }),
        view: Some(icon("character_NPC_view.dtx")),
        badge: None,
        arrow: false,
    }
}

/// R6: a bomb site's icon: `area_bomb_A.dtx` for zone 0, `_B` for zone 1
/// (the letter is 'A' + the zone number); Bombing Run's `area_bomb.dtx`.
pub fn bomb_site(zone: i32, bombing_run: bool) -> String {
    if bombing_run {
        return icon("area_bomb.dtx");
    }
    let letter = (b'A' + zone.clamp(0, 25) as u8) as char;
    icon(&format!("area_bomb_{letter}.dtx"))
}

/// R6: a flag base's icon: the team's flag at home or away.
pub fn flag_base(team: u8, home: bool) -> String {
    let letter = if team == 2 { 'B' } else { 'A' };
    icon(&if home {
        format!("area_flag_{letter}.dtx")
    } else {
        format!("area_flagPoint_{letter}.dtx")
    })
}

/// R6: a loose (dropped) flag of `team`.
pub fn loose_flag(team: u8) -> String {
    badge(team)
}

/// F5: a team's spawn marker (`area_respawn_A.dtx` / `_B`; VIP Escort's
/// `_DF` for the escort side, `_OF` for the other; co-op no suffix).
pub fn respawn(team: u8, escort: Option<u8>) -> String {
    let suffix = match escort {
        Some(e) if e != 0 => {
            if team == e {
                "_DF"
            } else {
                "_OF"
            }
        }
        _ => {
            if team == 2 {
                "_B"
            } else {
                "_A"
            }
        }
    };
    icon(&format!("area_respawn{suffix}.dtx"))
}

/// R6: the goal / activate zone icon.
pub fn goal() -> String {
    icon("area_goal.dtx")
}

/// R6: Spy Hunt's intel on the ground, Bombing Run's bomb, dropped bullion.
pub fn intel() -> String {
    icon("item_intel.dtx")
}
pub fn loose_bomb() -> String {
    icon("item_bomb.DTX")
}
pub fn bullion() -> String {
    icon("item_money.DTX")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn me() -> Viewer {
        Viewer { team: 1, intel: false }
    }

    #[test]
    fn firing_never_reveals() {
        let s = Subject { alive: true, ..Default::default() };
        assert_eq!(radar_player(&me(), &s), None);
    }

    #[test]
    fn flag_carrier_enemy_has_the_badge_and_an_arrow() {
        let s = Subject {
            alive: true,
            carry: CaCarry { flag: Some(1), ..Default::default() },
            ..Default::default()
        };
        let l = radar_player(&me(), &s).unwrap();
        assert_eq!(l.icon, icon("character_enemy.dtx"));
        assert_eq!(l.badge, Some(icon("character_flag_A.dtx")));
        assert!(l.arrow);
    }
}
