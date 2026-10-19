//! The score panel (hud_crosshair.md D6) and the round timer (D7), driven
//! by the match (`game::rules::CaMatchView`: the server's, or a network
//! client's copy): the mode's name and goal, each team's counter with its
//! progress bar, the local player's team highlighted; the timer in the
//! round modes while a round is live. `update_score` reads the world into
//! `CaScoreHud` (headless, for the tests); `draw_score` draws it.
//!
//! Known shortcuts: the goal text is ours ("G kills" / "G wins": the
//! mode's goal string isn't in our string table); Elimination Pro shows
//! its round wins on the same panel (D6 describes TDM's only); the panel
//! stays up while the scoreboard shows the round's result (no separate
//! result screen); the panel's textures are flat placeholder boxes.

use bevy::prelude::*;

use super::{PenParams, colour, hud_text, layer, model::*, pen, rect, rebuild};
use crate::{
    client::canvas::{HAlign, VAlign},
    core::{LocalPlayer, SimClock, Team},
    game::rules::{CaMatchView, Phase},
    ui::layout::Effect,
};

/// The panel as drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct ScorePanelView {
    pub mode_name: String,
    pub goal: String,
    /// Alpha's and Bravo's rows.
    pub rows: [ScoreRow; 2],
    /// The highlight's top-left (ref px) for the local player's team.
    pub highlight: Option<Vec2>,
}

/// The score panel and the round timer, as shown now.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct CaScoreHud {
    pub panel: Option<ScorePanelView>,
    /// The timer's text (D7); None: no timer.
    pub timer: Option<String>,
}

/// The panel's places (D6, D7), ref px.
pub mod places {
    use bevy::math::Vec2;

    pub const PANEL: (Vec2, Vec2) = (Vec2::new(342.0, 0.0), Vec2::new(340.0, 115.0));
    pub const MODE: (Vec2, Vec2) = (Vec2::new(347.0, 3.0), Vec2::new(220.0, 16.0));
    pub const GOAL: (Vec2, Vec2) = (Vec2::new(572.0, 3.0), Vec2::new(105.0, 16.0));
    /// Alpha's and Bravo's label and score boxes.
    pub const LABELS: [(Vec2, Vec2); 2] = [
        (Vec2::new(347.0, 23.0), Vec2::new(90.0, 18.0)),
        (Vec2::new(347.0, 43.0), Vec2::new(90.0, 18.0)),
    ];
    pub const SCORES: [(Vec2, Vec2); 2] = [
        (Vec2::new(612.0, 23.0), Vec2::new(65.0, 18.0)),
        (Vec2::new(612.0, 43.0), Vec2::new(65.0, 18.0)),
    ];
    /// The bars' frames; the bar is inset by (1, 1), 8 tall.
    pub const BARS: [(Vec2, Vec2); 2] = [
        (Vec2::new(448.0, 32.0), Vec2::new(157.0, 16.0)),
        (Vec2::new(448.0, 43.0), Vec2::new(157.0, 16.0)),
    ];
    pub const HIGHLIGHT: Vec2 = Vec2::new(336.0, 40.0);
    pub const TIMER_STRIP: (Vec2, Vec2) = (Vec2::new(344.0, 63.0), Vec2::new(336.0, 17.0));
    /// The timer's text, centred at about (444, 60).
    pub const TIMER: (Vec2, Vec2) = (Vec2::new(344.0, 52.0), Vec2::new(200.0, 17.0));
}

/// The panel's looks (D6, D7).
pub mod looks {
    pub const MODE: &str = "number 14";
    pub const LABEL: &str = "bold 18";
    pub const SCORE: &str = "number 18";
    pub const TIMER: &str = "number 20";
    pub const HEADER: [u8; 4] = [218, 218, 218, 255];
}

/// The match into `CaScoreHud`.
pub(super) fn update_score(
    view: Res<CaMatchView>,
    local: Query<&Team, With<LocalPlayer>>,
    clock: Option<Res<SimClock>>,
    time: Res<Time>,
    mut hud: ResMut<CaScoreHud>,
) {
    let now = clock.map_or_else(|| time.elapsed_secs_f64(), |c| c.now);
    let next = match view.mode() {
        None => CaScoreHud::default(),
        Some(mode) => {
            let round_based = mode.timer_shown;
            let counters: [u32; 2] = if round_based {
                view.round_wins
            } else {
                view.score.map(|s| s.max(0) as u32)
            };
            let mine = local.single().ok().and_then(|t| match t.0 {
                1 => Some(0),
                2 => Some(1),
                _ => None,
            });
            let (rows, highlight) = score_panel(counters, view.goal, mine);
            let goal = format!("{} {}", view.goal, if round_based { "wins" } else { "kills" });
            let timer = view
                .round_ends
                .filter(|_| view.phase == Phase::Live)
                .and_then(|ends| timer(round_based, (ends - now) * 1000.0));
            CaScoreHud {
                panel: Some(ScorePanelView {
                    mode_name: mode.shown_as.to_string(),
                    goal,
                    rows,
                    highlight,
                }),
                timer,
            }
        }
    };
    if *hud != next {
        *hud = next;
    }
}

/// The panel's layer.
#[derive(Component)]
pub(super) struct ScoreRoot;

pub(super) fn spawn_root(mut commands: Commands) {
    commands.spawn((ScoreRoot, layer(40)));
}

pub(super) fn draw_score(
    hud: Res<CaScoreHud>,
    screen: Res<super::HudScreen>,
    params: PenParams,
    root: Single<Entity, With<ScoreRoot>>,
    mut last: Local<Option<(CaScoreHud, Screen)>>,
    mut commands: Commands,
) {
    let screen = screen.0;
    if !rebuild(&mut last, (hud.clone(), screen), *root, &mut commands) {
        return;
    }
    let Some(panel) = &hud.panel else { return };
    let pen = pen(&params, screen);
    let centred = (HAlign::Centre, VAlign::Centre);
    commands.entity(*root).with_children(|p| {
        // PLACEHOLDER backgrounds for the panel's textures.
        p.spawn(rect(screen.rect(places::PANEL.0, places::PANEL.1), Color::srgba(0.05, 0.06, 0.07, 0.6)));
        if let Some(at) = panel.highlight {
            p.spawn(rect(screen.rect(at, places::HIGHLIGHT), Color::srgba(1.0, 1.0, 1.0, 0.08)));
        }
        for ((at, size), text) in [(places::MODE, &panel.mode_name), (places::GOAL, &panel.goal)] {
            hud_text(p, pen, (at, size, Vec2::ZERO), text, looks::MODE, centred, Effect::Shadow, looks::HEADER);
        }
        for (i, row) in panel.rows.iter().enumerate() {
            let label = if i == 0 { "Alpha" } else { "Bravo" };
            let (at, size) = places::LABELS[i];
            hud_text(p, pen, (at, size, Vec2::ZERO), label, looks::LABEL, centred, Effect::Shadow, row.colour);
            let (at, size) = places::SCORES[i];
            hud_text(p, pen, (at, size, Vec2::ZERO), &row.score, looks::SCORE, centred, Effect::Shadow, row.colour);
            let (at, size) = places::BARS[i];
            p.spawn(rect(screen.rect(at, size), Color::srgba(0.0, 0.0, 0.0, 0.45)));
            if row.bar > 0.0 {
                let fill = if i == 0 { SCORE_ALPHA } else { SCORE_BRAVO };
                p.spawn(rect(screen.rect(at + Vec2::ONE, Vec2::new(row.bar, 8.0)), colour(fill)));
            }
        }
        if let Some(t) = &hud.timer {
            let (at, size) = places::TIMER_STRIP;
            p.spawn(rect(screen.rect(at, size), Color::srgba(0.0, 0.0, 0.0, 0.45)));
            let (at, size) = places::TIMER;
            hud_text(p, pen, (at, size, Vec2::ZERO), t, looks::TIMER, centred, Effect::Shadow, [255, 255, 255, 255]);
        }
    });
}
