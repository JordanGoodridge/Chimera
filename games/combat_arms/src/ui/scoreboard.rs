//! The scoreboard (specs/combat_arms/ui_system.md's team list, I2 and its
//! test case 18; game_modes.md S13 for its rows): while the score key is
//! held (`+showscores`, Tab) and, as the round-result banner, for
//! `RESULT_MS` after a round's result. Alpha's rows in the upper list,
//! every other team's in the lower (a free-for-all mode's in one list,
//! "Players"), each in the mode's order (`game::rules::scoreboard`); the
//! team scores (the score panel's numbers) in the lists' headers; the
//! mode's column (captures, mission points) where it has one.
//!
//! `CaScoreboardStatePlugin` reads the world into `CaScoreboard` with no
//! window (the tests'); `CaScoreboardPlugin` adds it and draws it with
//! the shell's UI nodes, stretched by x_r, y_r like the HUD's panels.
//!
//! With the install's layouts (V1 key) the board is the game's own
//! (`BoundBoard`: the common frame with the result banner, the team or
//! no-team list, one clone of the row template per player); without
//! them ours below.
//!
//! Known shortcuts: without the install the columns' places are ours
//! (name, kills, deaths, assists, ping across the 766 px row) and the
//! panel and rows flat placeholder boxes; with it no level icon, clan,
//! nation flag, name background or infection mark (hidden), the ping
//! icon's buckets ours (`ping_icon`), the frame's room and operation
//! texts hidden (samples), the team title images the layout's (its
//! "win" ones); no suppression by popups (the in-game menu draws over
//! it).

use bevy::prelude::*;

use super::{
    CaUi,
    hud::{HudScreen, PenParams, bound, hud_text, layer, model::Screen, pen, rect, rebuild},
    layout::{Effect, Layout},
};
use crate::{
    client::{
        canvas::{HAlign, Scale, VAlign, WidgetId},
        console::HeldActions,
    },
    game::rules::{
        CaMatchView, Panel, Phase, ScoreOrder, Teams,
        scoreboard::{ScoreRow, board_for, rows},
    },
};

/// How long the board shows a round's result, ms (ui_system.md).
pub const RESULT_MS: f64 = 3000.0;

/// The panel and its lists (ref px).
pub mod places {
    use bevy::math::Vec2;

    pub const PANEL: (Vec2, Vec2) = (Vec2::new(93.0, 161.0), Vec2::new(838.0, 432.0));
    /// The lists' first rows' top-left: Alpha, the rest.
    pub const LISTS: [Vec2; 2] = [Vec2::new(164.0, 192.0), Vec2::new(164.0, 392.0)];
    pub const ROW: Vec2 = Vec2::new(766.0, 25.0);
    /// Ours: each column's x offset in a row and width: name, the mode's
    /// column, kills, deaths, assists, ping.
    pub const COLUMNS: [(f32, f32); 6] = [
        (8.0, 320.0),
        (335.0, 80.0),
        (420.0, 80.0),
        (510.0, 80.0),
        (600.0, 80.0),
        (690.0, 70.0),
    ];
    /// Ours: the lists' headers (team and score) above their rows.
    pub const HEADERS: [Vec2; 2] = [Vec2::new(164.0, 168.0), Vec2::new(164.0, 368.0)];
}

/// The board as shown.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct CaScoreboard {
    pub shown: bool,
    /// Alpha's rows, then every other team's.
    pub lists: [Vec<ScoreRow>; 2],
    /// The team scores (Alpha, Bravo).
    pub scores: [i32; 2],
    /// The lists' names ("Alpha", "Bravo"; "Players" in a free-for-all
    /// mode).
    pub headers: [String; 2],
    /// The mode's order (its column).
    pub order: Option<ScoreOrder>,
    /// A free-for-all mode (one list).
    pub ffa: bool,
    /// The round's result for the local player while the banner shows
    /// (`RESULT_MS`).
    pub result: Option<RoundResult>,
}

/// A round's result as the local player sees it (the result banner).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoundResult {
    Win,
    Lose,
    Draw,
}

/// When the last round's result came in (real ms), and how many results
/// there were.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
struct ResultShown {
    since: Option<f64>,
    phase_over: bool,
}

/// The board's state without a window.
pub struct CaScoreboardStatePlugin;

impl Plugin for CaScoreboardStatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CaScoreboard>()
            .init_resource::<ResultShown>()
            .init_resource::<HeldActions>()
            .init_resource::<CaMatchView>()
            .add_systems(Update, update);
    }
}

fn update(world: &mut World) {
    let view = world.resource::<CaMatchView>().clone();
    let now = world.resource::<Time<Real>>().elapsed().as_secs_f64() * 1000.0;
    let over = matches!(view.phase, Phase::RoundOver { .. });
    let mut result = *world.resource::<ResultShown>();
    if over && !result.phase_over {
        result.since = Some(now);
    }
    result.phase_over = over;
    world.insert_resource(result);
    let banner = result.since.is_some_and(|s| now - s < RESULT_MS);
    let mode = view.mode();
    let shown = mode.is_some() && (world.resource::<HeldActions>().showscores || banner);
    let order = mode.map_or(ScoreOrder::Kills, |m| m.order);
    let ffa = mode.is_some_and(|m| m.teams == Teams::FreeForAll);
    let mine = world
        .query_filtered::<&crate::core::Team, With<crate::core::LocalPlayer>>()
        .iter(world)
        .next()
        .map(|t| t.0);
    let result = banner.then(|| match view.winner {
        None => RoundResult::Draw,
        Some(w) if Some(w) == mine => RoundResult::Win,
        Some(_) => RoundResult::Lose,
    });
    let next = CaScoreboard {
        shown,
        ffa,
        result,
        lists: if shown { board_for(rows(world), order, ffa) } else { Default::default() },
        scores: if mode.is_some_and(|m| m.panel == Panel::RoundWins) {
            view.round_wins.map(|w| w as i32)
        } else {
            view.score
        },
        headers: if ffa {
            ["Players".into(), String::new()]
        } else {
            ["Alpha".into(), "Bravo".into()]
        },
        order: mode.map(|m| m.order),
    };
    if *world.resource::<CaScoreboard>() != next {
        world.insert_resource(next);
    }
}

// ---------------------------------------------------------------------------
// The game's own scoreboard layouts (ui_system.md H)

/// The scoreboard from the install's layouts, bound by name: the common
/// frame (`Result_Normal`: its `Game_Result` image the round's result
/// banner, the HUD layout attributes' `scoreboardwintex`,
/// `scoreboardlosetex`, `scoreboarddrawtex` and the free-for-all
/// `scoreboardffawintex` / `scoreboardffalosetex`) and the team list
/// (`Team_UserList`: Alpha's rows from `UserList_UP`'s template
/// `User_Column`, the others' from `UserList_Down`'s `DUser_Column`, the
/// team scores in `alpha_point` and `bravo_point`) or
/// the no-team list (`NoTeam_UserList`'s `UserList`), one clone of the
/// template per row, 25 ref px apart (the list's gap and blank, 0).
#[derive(Resource, Clone, Debug, Default)]
pub struct BoundBoard {
    pub frame: Option<Layout>,
    pub team: Option<Layout>,
    pub no_team: Option<Layout>,
    /// The result banners' textures: win, lose, draw, free-for-all win
    /// and lose.
    pub banners: [Option<String>; 5],
}

impl BoundBoard {
    pub fn of(ui: &CaUi) -> Self {
        let Some(inst) = ui.install.as_ref() else { return Self::default() };
        let t = |k: &str| inst.hud_texture(k);
        Self {
            frame: inst.result.clone(),
            team: inst.team_list.clone(),
            no_team: inst.no_team_list.clone(),
            banners: [
                t("scoreboardwintex"),
                t("scoreboardlosetex"),
                t("scoreboarddrawtex"),
                t("scoreboardffawintex"),
                t("scoreboardffalosetex"),
            ],
        }
    }

    /// The board drawn at `scale`; None without the lists' layout.
    pub fn build(&self, board: &CaScoreboard, scale: Scale) -> Option<Layout> {
        let (src, panel, lists): (&Layout, &str, &[(&str, &str)]) = if board.ffa {
            (self.no_team.as_ref()?, "NoTeam_UserList", &[("UserList", "User_Column")])
        } else {
            (
                self.team.as_ref()?,
                "Team_UserList",
                &[("UserList_UP", "User_Column"), ("UserList_Down", "DUser_Column")],
            )
        };
        let mut l = Layout::new();
        l.scale = scale;
        if let Some(frame) = &self.frame
            && let Some(f) = frame.find("Result_Normal")
        {
            let id = l.tree.graft(0, &frame.tree, f);
            let game_result = l.tree.find_from(id, "Game_Result");
            let banner = board.result.and_then(|r| {
                let i = match (r, board.ffa) {
                    (RoundResult::Win, false) => 0,
                    (RoundResult::Lose, false) => 1,
                    (_, _) if r == RoundResult::Draw => 2,
                    (RoundResult::Win, true) => 3,
                    _ => 4,
                };
                self.banners[i].clone()
            });
            let root1 = game_result.and_then(|g| l.tree.get(g).parent);
            bound::show(&mut l, root1, banner.is_some());
            if let Some(b) = &banner {
                bound::set_texture(&mut l, game_result, b);
            }
            bound::hide_samples(&mut l, id);
        }
        let from = src.find(panel)?;
        let panel = l.tree.graft(0, &src.tree, from);
        for (i, (list_name, row_name)) in lists.iter().enumerate() {
            let Some(list) = l.tree.find_from(panel, list_name) else { continue };
            let Some(template) = src.find(row_name) else { continue };
            // The templates in the list: hidden.
            for c in l.tree.get(list).children.clone() {
                l.tree.get_mut(c).show = false;
            }
            for (k, row) in board.lists[i].iter().enumerate() {
                let id = l.tree.graft(list, &src.tree, template);
                let n = l.tree.get_mut(id);
                n.show = true;
                n.pos = IVec2::new(0, places::ROW.y as i32 * k as i32);
                bind_row(&mut l, id, k, row, board.order);
            }
        }
        // The team scores (`title_alpha`'s and `title_bravo`'s).
        for (i, name) in ["alpha_point", "bravo_point"].into_iter().enumerate() {
            let id = l.tree.find_from(panel, name);
            bound::set_text(&mut l, id, Some(&board.scores[i].to_string()), None);
        }
        // The headers: the mode's column only where it has one.
        let mission = board.order.and_then(|o| board.lists.iter().flatten().next().and_then(|r| r.mode_column(o))).is_some();
        for name in ["menu_Mission"] {
            let id = l.tree.find_from(panel, name).and_then(|t| l.tree.get(t).parent);
            bound::show(&mut l, id, mission);
        }
        for name in ["menu_SubMission", "menu_Clan"] {
            let id = l.tree.find_from(panel, name).and_then(|t| l.tree.get(t).parent);
            bound::show(&mut l, id, false);
        }
        if let Some(vip) = l.tree.find_from(panel, "menu_VIP_Eli") {
            l.tree.get_mut(vip).show = false;
        }
        for name in ["menu_Intels"] {
            let id = l.tree.find_from(panel, name).and_then(|t| l.tree.get(t).parent);
            bound::show(&mut l, id, false);
        }
        bound::hide_samples(&mut l, panel);
        Some(l)
    }
}

/// The ping icon for `ping` ms (ours: 4 under 50 ms, then 3, 2, 1 under
/// 100, 150, 250, else 0; the local player's, unknown, 4).
pub fn ping_icon(ping: Option<u32>) -> String {
    let n = match ping {
        None => 4,
        Some(p) if p < 50 => 4,
        Some(p) if p < 100 => 3,
        Some(p) if p < 150 => 2,
        Some(p) if p < 250 => 1,
        _ => 0,
    };
    format!("ui_hud_scoreboard/icon_ping_{n}.dtx")
}

/// One row's controls: name, kills, deaths, assists, the mode's column
/// (`User_Mission`, hidden where the mode has none), rank (its place, two
/// digits) and ping icon; what nothing drives hidden (level, clan, flag,
/// infection, name backgrounds, sub-mission).
fn bind_row(l: &mut Layout, row_id: WidgetId, k: usize, row: &ScoreRow, order: Option<ScoreOrder>) {
    let find = |l: &Layout, name: &str| l.tree.find_from(row_id, name);
    for (name, value) in [
        ("User_Name", row.name.clone()),
        ("User_Kill", row.kills.to_string()),
        ("User_Death", row.deaths.to_string()),
        ("User_Assist", row.assists.to_string()),
        ("User_Rank", format!("{:02}", k + 1)),
    ] {
        let id = find(l, name);
        bound::set_text(l, id, Some(&value), None);
    }
    let mission = order.and_then(|o| row.mode_column(o));
    let id = find(l, "User_Mission");
    match mission {
        Some(v) => bound::set_text(l, id, Some(&v.to_string()), None),
        None => {
            let parent = id.and_then(|i| l.tree.get(i).parent);
            bound::show(l, parent, false);
        }
    }
    let id = find(l, "User_Ping");
    bound::set_texture(l, id, &ping_icon(row.ping));
    for name in [
        "Infection",
        "User_LevelImg",
        "User_NationFlag",
        "User_ClanBG",
        "User_ClanEmblem",
        "User_Em/Sem",
        "NameBG_Camo",
        "NameBG_Line",
    ] {
        let id = find(l, name);
        bound::show(l, id, false);
    }
    for name in ["Clan_Name", "User_SubMission", "User_Spy_Kill", "User_Intel", "User_Score"] {
        let id = find(l, name).and_then(|i| l.tree.get(i).parent);
        bound::show(l, id, false);
    }
}

/// The board drawn (after its state).
pub struct CaScoreboardPlugin;

impl Plugin for CaScoreboardPlugin {
    fn build(&self, app: &mut App) {
        let ui = app.world().get_resource::<CaUi>().cloned().unwrap_or_else(CaUi::installed);
        app.insert_resource(BoundBoard::of(&ui));
        app.add_plugins(CaScoreboardStatePlugin)
            .init_resource::<CaUi>()
            .add_systems(Startup, |mut commands: Commands| {
                commands.spawn((BoardRoot, layer(45)));
            })
            .add_systems(Update, draw.after(update));
    }
}

#[derive(Component)]
struct BoardRoot;

#[allow(clippy::too_many_arguments)]
fn draw(
    board: Res<CaScoreboard>,
    screen: Option<Res<HudScreen>>,
    params: PenParams,
    art: Option<Res<super::draw::CaArt>>,
    bound_board: Res<BoundBoard>,
    root: Single<Entity, With<BoardRoot>>,
    mut last: Local<Option<(CaScoreboard, Screen, bool)>>,
    mut commands: Commands,
) {
    let screen = screen.map_or(Screen::new(1024.0, 768.0), |s| s.0);
    let loaded = art.as_ref().is_some_and(|a| !a.0.is_empty());
    if !rebuild(&mut last, (board.clone(), screen, loaded), *root, &mut commands) || !board.shown {
        return;
    }
    // The game's own scoreboard, bound by name (`BoundBoard`).
    if let Some(layout) = bound_board.build(&board, crate::ui::hud::scale(screen)) {
        let ui = &params.0;
        let draws = layout.draw(&ui.ctx());
        commands.entity(*root).with_children(|c| {
            super::draw::spawn_draws(c, &draws, &ui.stand_ins, art.as_deref(), params.1.as_deref(), params.2.as_deref());
        });
        return;
    }
    let pen = pen(&params, screen);
    let left = (HAlign::Default, VAlign::Centre);
    let centred = (HAlign::Centre, VAlign::Centre);
    let white = [230, 230, 230, 255];
    commands.entity(*root).with_children(|p| {
        p.spawn(rect(screen.rect(places::PANEL.0, places::PANEL.1), Color::srgba(0.04, 0.05, 0.06, 0.82)));
        for (i, list) in board.lists.iter().enumerate() {
            if board.headers[i].is_empty() {
                continue;
            }
            let header = if board.headers[i] == "Players" {
                board.headers[i].clone()
            } else {
                format!("{}  {}", board.headers[i], board.scores[i])
            };
            hud_text(p, pen, (places::HEADERS[i], places::ROW, Vec2::ZERO), &header, "bold 16", left, Effect::Shadow, white);
            for (k, row) in list.iter().enumerate() {
                let at = places::LISTS[i] + Vec2::Y * places::ROW.y * k as f32;
                p.spawn(rect(screen.rect(at, places::ROW - Vec2::Y), Color::srgba(1.0, 1.0, 1.0, 0.05)));
                let ping = row.ping.map_or(String::new(), |p| p.to_string());
                let mode_column = board
                    .order
                    .and_then(|o| row.mode_column(o))
                    .map_or(String::new(), |v| v.to_string());
                let cells = [
                    row.name.clone(),
                    mode_column,
                    row.kills.to_string(),
                    row.deaths.to_string(),
                    row.assists.to_string(),
                    ping,
                ];
                for (c, ((x, w), text)) in places::COLUMNS.iter().zip(cells).enumerate() {
                    let align = if c == 0 { left } else { centred };
                    let cell = (at + Vec2::X * *x, Vec2::new(*w, places::ROW.y), Vec2::ZERO);
                    hud_text(p, pen, cell, &text, "bold 14", align, Effect::Shadow, white);
                }
            }
        }
    });
}
