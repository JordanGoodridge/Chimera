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
//! Known shortcuts: the columns' places are ours (the spec leaves the
//! row's contents to the game modes and its layout file is keyed): name,
//! kills, deaths, assists, ping across the 766 px row; no level icon or
//! clan emblem; no suppression by popups (the in-game menu draws over
//! it); the panel and rows are flat placeholder boxes.

use bevy::prelude::*;

use super::{
    CaUi,
    hud::{HudScreen, PenParams, hud_text, layer, model::Screen, pen, rect, rebuild},
    layout::Effect,
};
use crate::{
    client::{
        canvas::{HAlign, VAlign},
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
    let next = CaScoreboard {
        shown,
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

/// The board drawn (after its state).
pub struct CaScoreboardPlugin;

impl Plugin for CaScoreboardPlugin {
    fn build(&self, app: &mut App) {
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

fn draw(
    board: Res<CaScoreboard>,
    screen: Option<Res<HudScreen>>,
    params: PenParams,
    root: Single<Entity, With<BoardRoot>>,
    mut last: Local<Option<(CaScoreboard, Screen)>>,
    mut commands: Commands,
) {
    let screen = screen.map_or(Screen::new(1024.0, 768.0), |s| s.0);
    if !rebuild(&mut last, (board.clone(), screen), *root, &mut commands) || !board.shown {
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
