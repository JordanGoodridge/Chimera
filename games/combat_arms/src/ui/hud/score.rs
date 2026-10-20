//! The score panel (hud_crosshair.md D6) and the round timer (D7), driven
//! by the match (`game::rules::CaMatchView`: the server's, or a network
//! client's copy): the mode's name and goal, each team's counter with its
//! progress bar (the mode's `Panel`: team counters, round wins; in a
//! free-for-all mode the local player's and the leader's), the local
//! player's row highlighted; the timer in the round modes while a round
//! is live; the objectives' lines (`game::rules::objectives`: the planted
//! bomb's fuse, the flags, the pole, the intel, what you carry; Hired
//! Guns' bars banked, in bags and unclaimed, your pocket, bank and
//! mercenaries with their order; VIP Escort's roles, the VIP, the exit,
//! the rounds until the swap and its notice; the clan modes' half) and the
//! action gauge (plant 4 s, defuse 7 s, seize 5 s: game_modes.md M3,
//! M10). `update_score` reads the world into `CaScoreHud` (headless, for
//! the tests); `draw_score` draws it.
//!
//! With the install's layouts the panel is the game's mode indicator
//! (`bound::ScorePanel`) and the gauge the game's action panel
//! (`bound::ActionPanel`); without them, ours at D6's places.
//!
//! Known shortcuts: the goal text is ours ("G kills" / "G wins": the
//! mode's goal string isn't in our string table); the round modes show
//! their round wins on the same counters (D6 describes TDM's only); the
//! objectives' lines are ours (text under the timer: the spec gives
//! their lengths, not their looks), the gauge too without the install;
//! the panel stays up while the scoreboard shows the round's result (no
//! separate result screen); without the install the panel's textures
//! are flat placeholder boxes.

use bevy::prelude::*;

use super::{PenParams, colour, hud_text, layer, model::*, pen, rect, rebuild};
use crate::{
    client::canvas::{HAlign, VAlign},
    core::{LocalPlayer, SimClock, Team},
    game::rules::{
        CaMatchView, CaStats, Panel, Phase,
        escort::CaVip,
        hired_guns::CaBank,
        mercs::{CaMerc, CaSquadOrder, MercOrder},
        objectives::{ActionKind, CaAction, CaCarry, CaObjectives, FlagAt},
    },
    ui::layout::Effect,
};

/// The panel as drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct ScorePanelView {
    /// The game mode's key (`game::rules`' `TDM`, `CTF`, ...).
    pub mode_key: String,
    pub mode_name: String,
    pub goal: String,
    /// Alpha's and Bravo's rows (a free-for-all mode's: yours, the
    /// leader's).
    pub rows: [ScoreRow; 2],
    /// The rows' labels.
    pub labels: [String; 2],
    /// The highlight's top-left (ref px) for the local player's team.
    pub highlight: Option<Vec2>,
}

/// The score panel and the round timer, as shown now.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct CaScoreHud {
    pub panel: Option<ScorePanelView>,
    /// The timer's text (D7); None: no timer.
    pub timer: Option<String>,
    /// The objectives' lines.
    pub objectives: Vec<String>,
    /// The action gauge: what, and how full (0 to 1).
    pub gauge: Option<(String, f32)>,
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
    /// Ours: the objectives' first line, one line's height.
    pub const OBJECTIVES: (Vec2, Vec2) = (Vec2::new(344.0, 84.0), Vec2::new(336.0, 16.0));
    /// Ours: the action gauge's frame (the bar inset by 1).
    pub const GAUGE: (Vec2, Vec2) = (Vec2::new(412.0, 470.0), Vec2::new(200.0, 12.0));
    pub const GAUGE_LABEL: (Vec2, Vec2) = (Vec2::new(412.0, 452.0), Vec2::new(200.0, 16.0));
}

/// The panel's looks (D6, D7).
pub mod looks {
    pub const MODE: &str = "number 14";
    pub const LABEL: &str = "bold 18";
    pub const SCORE: &str = "number 18";
    pub const TIMER: &str = "number 20";
    pub const HEADER: [u8; 4] = [218, 218, 218, 255];
}

/// The goal's unit as the panel prints it ("G kills").
pub fn goal_unit(key: &str) -> &'static str {
    match key {
        "TDM" | "FFA" | "VWRM" => "kills",
        "VTDM" => "points",
        "CTF" => "captures",
        "HQM" | "SQM2" | "BNT" => "min",
        "LMS" | "HG" | "QRT" | "ESL" | "SQM" | "CMM" | "CTSV" | "CESC" => "rounds",
        "CDM" => "kills",
        "CCTF" => "captures",
        _ => "wins",
    }
}

/// The objectives' lines (ours: the spec places only the gauges and the
/// dropped flag's seconds): the planted bomb's fuse, the bomb on the
/// ground, each flag, the pole, the intel; `mine` what the local player
/// carries.
pub fn objective_lines(o: &CaObjectives, mine: CaCarry, now: f64) -> Vec<String> {
    let team = |t: u8| if t == 1 { "Alpha" } else { "Bravo" };
    let secs = |until: f64| ((until - now).max(0.0)).trunc() as u64;
    let mut lines = Vec::new();
    if let Some(p) = o.planted {
        let site = if p.number == 0 { "A" } else { "B" };
        lines.push(format!("Bomb planted at {site}: {}", round_timer((p.detonates - now) * 1000.0)));
    }
    if o.loose_bomb.is_some() {
        lines.push("The bomb is on the ground".into());
    }
    for f in &o.flags {
        let at = match f.at {
            FlagAt::Home => "home".to_string(),
            FlagAt::Carried => "taken".to_string(),
            FlagAt::Dropped { returns, .. } => format!("dropped, home in {} s", secs(returns)),
        };
        lines.push(format!("{} flag: {at}", team(f.team)));
    }
    if let Some(p) = o.pole {
        lines.push(match p.team {
            Some(t) => format!("{}'s flag rising: {} s", team(t), (now - p.since).max(0.0).trunc() as u64),
            None => "The flag pole is free".into(),
        });
    }
    if o.stage == 1 && !o.intel.is_empty() {
        lines.push(format!("Intel on the ground: {}", o.intel.len()));
    }
    if o.stage == 2 {
        lines.push("The final spy is out".into());
    }
    if let Some(f) = mine.flag {
        lines.push(format!("You carry the {} flag", team(f)));
    }
    if mine.bomb {
        lines.push("You carry the bomb".into());
    }
    if mine.intel > 0 {
        lines.push(format!("You carry {} intel", mine.intel));
    }
    // Hired Guns (game_modes_2.md M11.10; the chat warning of M11.4 as a
    // line).
    if o.placed > 0 || !o.bullion.is_empty() {
        let (level, bags) = o.bars_on_ground();
        lines.push(format!("Banked {} - {}, in bags {bags}, unclaimed {level}", o.banked[0], o.banked[1]));
    }
    if mine.bars > 0 {
        lines.push(format!("Pocket {:02} bars", mine.bars));
        if mine.bars >= 11 {
            let pct = (mine.bars.min(25) - 10) * 3;
            lines.push(format!("Speed penalty {pct} % with {} bars", mine.bars));
        }
    }
    // VIP Escort (M13).
    if o.escort != 0 {
        lines.push(format!("{} escorts the VIP", team(o.escort)));
    }
    if let Some(x) = o.exit.filter(|x| x.open || x.opens.is_finite()) {
        lines.push(if x.open {
            "The exit is open".into()
        } else {
            format!("The exit opens in {} s", secs(x.opens))
        });
    }
    if let Some(n) = o.swap_notice.filter(|n| *n > now) {
        lines.push(format!("Sides swap in {} s", secs(n) + 1));
    } else if o.rounds_to_swap != 0 && o.rounds_to_swap != 255 {
        lines.push(format!("Rounds until the swap: {}", o.rounds_to_swap));
    }
    if o.half != 0 {
        lines.push(format!("Half {}", o.half));
    }
    lines
}

/// Hired Guns' own lines for the local player: its bank, its mercenaries
/// (health, %) and their order (M11.10).
pub fn squad_lines(bank: Option<i32>, order: Option<MercOrder>, units: &[(String, f32)]) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(b) = bank {
        lines.push(format!("Bank {b}"));
    }
    if !units.is_empty() {
        let order = match order.unwrap_or_default() {
            MercOrder::Default => "default",
            MercOrder::FollowMe => "follow me",
            MercOrder::Hold => "hold position",
            MercOrder::AttackMove(_) => "attack move",
        };
        lines.push(format!("Mercenaries ({order}):"));
        for (name, health) in units {
            lines.push(format!("  {name} {:.0} %", health * 100.0));
        }
    }
    lines
}

/// The match into `CaScoreHud`.
#[allow(clippy::too_many_arguments)]
pub(super) fn update_score(
    view: Res<CaMatchView>,
    objectives: Option<Res<CaObjectives>>,
    local: Query<(&Team, Option<&CaStats>, Option<&CaAction>, Option<&CaCarry>), With<LocalPlayer>>,
    squad: (
        Query<(Entity, Option<&CaBank>, Option<&CaSquadOrder>, Has<CaVip>), With<LocalPlayer>>,
        Query<(&CaMerc, Option<&Name>, Option<&crate::core::Health>)>,
        Query<Option<&Name>, With<CaVip>>,
    ),
    everyone: Query<&CaStats>,
    clock: Option<Res<SimClock>>,
    time: Res<Time>,
    mut hud: ResMut<CaScoreHud>,
) {
    let now = clock.map_or_else(|| time.elapsed_secs_f64(), |c| c.now);
    let me = local.single().ok();
    let next = match view.mode() {
        None => CaScoreHud::default(),
        Some(mode) => {
            let round_based = mode.timer_shown;
            let mine_stats = me.and_then(|(_, s, ..)| s.copied()).unwrap_or_default();
            let best = |f: fn(&CaStats) -> u32| everyone.iter().map(f).max().unwrap_or(0);
            let team_side = me.and_then(|(t, ..)| match t.0 {
                1 => Some(0),
                2 => Some(1),
                _ => None,
            });
            let (counters, labels, mine): ([u32; 2], [&str; 2], Option<usize>) = match mode.panel {
                Panel::TeamScore => (view.score.map(|s| s.max(0) as u32), ["Alpha", "Bravo"], team_side),
                Panel::RoundWins => (view.round_wins, ["Alpha", "Bravo"], team_side),
                Panel::Kills => ([mine_stats.kills, best(|s| s.kills)], ["You", "Leader"], Some(0)),
                Panel::PlayerRoundWins => {
                    ([mine_stats.round_wins, best(|s| s.round_wins)], ["You", "Leader"], Some(0))
                }
            };
            let (rows, highlight) = score_panel(counters, view.goal, mine);
            let goal = format!("{} {}", view.goal, goal_unit(mode.key));
            let timer = view
                .round_ends
                .filter(|_| view.phase == Phase::Live)
                .and_then(|ends| timer(round_based, (ends - now) * 1000.0));
            let carry = me.and_then(|(.., c)| c.copied()).unwrap_or_default();
            let gauge = me.and_then(|(_, _, a, _)| a.copied()).map(|a| {
                let what = match a.kind {
                    ActionKind::Plant => "Planting",
                    ActionKind::Defuse => "Defusing",
                    ActionKind::Seize => "Seizing",
                };
                (what.to_string(), a.fraction())
            });
            let mut lines = objectives.map_or_else(Vec::new, |o| objective_lines(&o, carry, now));
            let (local_squad, mercs, vips) = &squad;
            if let Ok((me, bank, order, vip)) = local_squad.single() {
                let units: Vec<(String, f32)> = mercs
                    .iter()
                    .filter(|(m, ..)| m.leader == me && m.died.is_none())
                    .map(|(m, n, h)| {
                        let name = n.map_or_else(|| format!("unit {}", m.unit), |n| n.as_str().to_string());
                        (name, h.map_or(0.0, |h| h.current.max(0.0) / h.max.max(1e-6)))
                    })
                    .collect();
                lines.extend(squad_lines(bank.map(|b| b.0), order.map(|o| o.0), &units));
                if vip {
                    lines.push("You are the VIP".into());
                } else if let Some(name) = vips.iter().next() {
                    lines.push(format!("VIP: {}", name.map_or("?", |n| n.as_str())));
                }
            }
            CaScoreHud {
                panel: Some(ScorePanelView {
                    mode_key: mode.key.to_string(),
                    mode_name: mode.shown_as.to_string(),
                    goal,
                    rows,
                    labels: labels.map(String::from),
                    highlight,
                }),
                timer,
                objectives: lines,
                gauge,
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

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_score(
    hud: Res<CaScoreHud>,
    screen: Res<super::HudScreen>,
    params: PenParams,
    art: Option<Res<crate::ui::draw::CaArt>>,
    bound: Res<super::BoundHud>,
    root: Single<Entity, With<ScoreRoot>>,
    mut last: Local<Option<(CaScoreHud, Screen, bool)>>,
    mut commands: Commands,
) {
    let screen = screen.0;
    let loaded = art.as_ref().is_some_and(|a| !a.0.is_empty());
    if !rebuild(&mut last, (hud.clone(), screen, loaded), *root, &mut commands) {
        return;
    }
    let Some(panel) = &hud.panel else { return };
    let pen = pen(&params, screen);
    let centred = (HAlign::Centre, VAlign::Centre);
    // The game's own mode panel, bound by name (`bound::ScorePanel`); the
    // objectives' lines stay ours, the gauge the action panel's.
    let installed = bound
        .score
        .as_ref()
        .and_then(|s| s.build(&panel.mode_key, panel, hud.timer.as_deref(), super::scale(screen)));
    if let Some(layout) = installed {
        let ui = &params.0;
        let draws = layout.draw(&ui.ctx());
        commands.entity(*root).with_children(|p| {
            crate::ui::draw::spawn_draws(p, &draws, &ui.stand_ins, art.as_deref(), params.1.as_deref(), params.2.as_deref());
            for (k, line) in hud.objectives.iter().enumerate() {
                let (at, size) = places::OBJECTIVES;
                let at = at + Vec2::Y * size.y * k as f32;
                hud_text(p, pen, (at, size, Vec2::ZERO), line, looks::MODE, centred, Effect::Shadow, looks::HEADER);
            }
            if bound.action.is_none() {
                draw_gauge(p, pen, screen, &hud, centred);
            }
        });
        return;
    }
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
            let label = panel.labels[i].as_str();
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
        for (k, line) in hud.objectives.iter().enumerate() {
            let (at, size) = places::OBJECTIVES;
            let at = at + Vec2::Y * size.y * k as f32;
            hud_text(p, pen, (at, size, Vec2::ZERO), line, looks::MODE, centred, Effect::Shadow, looks::HEADER);
        }
        if bound.action.is_none() {
            draw_gauge(p, pen, screen, &hud, centred);
        }
    });
}

/// Ours: the action gauge (its label and bar) when the game's action
/// panel isn't there.
fn draw_gauge(p: &mut ChildSpawnerCommands, pen: super::Pen, screen: Screen, hud: &CaScoreHud, centred: (HAlign, VAlign)) {
    if let Some((what, fill)) = &hud.gauge {
        let (at, size) = places::GAUGE_LABEL;
        hud_text(p, pen, (at, size, Vec2::ZERO), what, looks::MODE, centred, Effect::Shadow, looks::HEADER);
        let (at, size) = places::GAUGE;
        p.spawn(rect(screen.rect(at, size), Color::srgba(0.0, 0.0, 0.0, 0.55)));
        let bar = Vec2::new((size.x - 2.0) * fill, size.y - 2.0);
        p.spawn(rect(screen.rect(at + Vec2::ONE, bar), Color::srgba(0.9, 0.8, 0.2, 0.9)));
    }
}
