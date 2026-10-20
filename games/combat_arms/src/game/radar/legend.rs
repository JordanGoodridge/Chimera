//! The full map's legend (specs/combat_arms/radar_minimap.md F2): the
//! large-map attribute file's `[IconListN]` sections, each with a
//! `GameType` and up to 20 entries `ItemI_Icon`, `ItemI_View` (optional)
//! and `ItemI_Desc` (a string id); entries end at the first empty icon.
//! The section whose `GameType` is the current mode's is used, else the
//! first (`IconList0`, Elimination). Without the install, ours: the
//! Elimination list (F2's six rows, US strings).

use lithtech_formats::attributes::Document;

use super::look::icon;

/// Rows per list, at most.
pub const MAX_ROWS: usize = 20;

/// One row: the icon, the wedge over it, the description's string id and
/// its text when the string table lacks it.
#[derive(Clone, Debug, PartialEq)]
pub struct LegendItem {
    pub icon: String,
    pub view: Option<String>,
    pub desc: String,
    pub text: String,
}

/// One mode's list.
#[derive(Clone, Debug, PartialEq)]
pub struct IconList {
    pub game_type: i32,
    pub items: Vec<LegendItem>,
}

/// The lists of a large-map attribute file.
pub fn parse(doc: &Document) -> Vec<IconList> {
    doc.records("IconList")
        .into_iter()
        .map(|s| {
            let text = |k: &str| String::from_utf8_lossy(s.string(k).unwrap_or_default()).trim().to_string();
            let mut items = Vec::new();
            for i in 0..MAX_ROWS {
                let file = text(&format!("Item{i}_Icon"));
                if file.is_empty() {
                    break;
                }
                let view = text(&format!("Item{i}_View"));
                items.push(LegendItem {
                    icon: super::look::map_icon_key(&file),
                    view: (!view.is_empty()).then(|| super::look::map_icon_key(&view)),
                    desc: text(&format!("Item{i}_Desc")),
                    text: String::new(),
                });
            }
            IconList {
                game_type: s.int("GameType", -1),
                items,
            }
        })
        .collect()
}

/// F2: the list for `game_type`, else the first.
pub fn pick(lists: &[IconList], game_type: i32) -> Option<&IconList> {
    lists.iter().find(|l| l.game_type == game_type).or(lists.first())
}

/// The built-in list: Elimination's six rows.
pub fn builtin() -> Vec<IconList> {
    let row = |i: &str, v: Option<&str>, t: &str| LegendItem {
        icon: icon(i),
        view: v.map(icon),
        desc: String::new(),
        text: t.to_string(),
    };
    vec![IconList {
        game_type: 1,
        items: vec![
            row("character_ally.dtx", Some("character_ally_view.dtx"), "ALLY VIEW"),
            row("character_enemy.dtx", Some("character_enemy_view.dtx"), "ENEMY VIEW"),
            row("character_player.dtx", Some("character_player_view.dtx"), "Current Location"),
            row("area_death_player.DTX", None, "Last Point of Death"),
            row("area_respawn_A.dtx", None, "ALPHA Respawn Area"),
            row("area_respawn_B.dtx", None, "BRAVO Respawn Area"),
        ],
    }]
}

/// The game-type number of a mode's room key (game_modes.md, "Modes
/// found"); 0 for a mode without one.
pub fn game_type(mode_key: &str) -> i32 {
    match mode_key {
        "TDM" => 1,
        "TMM" => 2,
        "CMM" => 3,
        "CDM" => 4,
        "COOP" | "SCOP" => 5,
        "FFA" => 6,
        "CTF" => 7,
        "CCTF" => 8,
        "BNT" => 9,
        "SFM" => 10,
        "QRT" => 11,
        "TSV" => 12,
        "CTSV" => 13,
        "LMS" => 14,
        "SBT" => 15,
        "HQM" => 17,
        "SQM" => 19,
        "SQM2" => 20,
        "ESL" => 21,
        "VWRM" => 22,
        "ESC" => 23,
        "CESC" => 24,
        "VTDM" => 25,
        "IFS" => 26,
        "COOP_CF" => 27,
        "BDM" => 28,
        "HG" => 29,
        "BTRN" => 30,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_ends_at_the_first_empty_icon() {
        let doc = Document::parse(
            b"[IconList0]\nGameType = 1\nItem0_Icon = \"character_ally.dtx\"\nItem0_View = \"character_ally_view.dtx\"\nItem0_Desc = \"IDS_A\"\nItem1_Icon = \"\"\nItem2_Icon = \"x.dtx\"\n[IconList1]\nGameType = 7\nItem0_Icon = \"area_flag_A.dtx\"\n",
        )
        .unwrap();
        let lists = parse(&doc);
        assert_eq!(lists.len(), 2);
        assert_eq!(lists[0].items.len(), 1);
        assert_eq!(pick(&lists, 7).unwrap().items[0].icon, icon("area_flag_A.dtx"));
        // No match: the first list.
        assert_eq!(pick(&lists, 99).unwrap().game_type, 1);
    }
}
