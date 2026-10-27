//! The minigame scoreboard: hold `+showscores` (Tab) while a minigame
//! plays and every player and bot is listed, whatever game's gun or HUD
//! the local player holds: name, kills, deaths, the rung's gun (the
//! `chimera:ladder` state), ping and matches won
//! (`chimera_sim::minigame::wins`), best first (rung, then kills, then
//! fewest deaths). It is the shell's own overlay (not a `HudRoot`), so it
//! looks the same on CS:S's HUD, Combat Arms' and the minimal one; the
//! games' own scoreboards stay down while a minigame plays
//! (`MinigameHud::playing`), so plain CS:S keeps its own. An app adds
//! `MinigameScoreboardPlugin`.
//!
//! Known shortcuts: the gun is its id without the game's prefix (no
//! display-name registry in the shell); plain text, no team colours.

use bevy::prelude::*;
use chimera_sim::minigame::{
    hud::MinigameHud,
    ladder::{Ladder, Rung},
    runtime::InMinigame,
    wins::MatchesWon,
};

use super::console::HeldActions;
use crate::{
    core::{LocalPlayer, Score},
    net::{NetCharacter, NetScore, NetSettings, score_flags},
};

/// Adds the scoreboard overlay and its system.
pub struct MinigameScoreboardPlugin;

impl Plugin for MinigameScoreboardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HeldActions>().add_systems(Startup, spawn).add_systems(Update, draw);
    }
}

/// The overlay's root (hidden unless the key is held).
#[derive(Component)]
pub struct ScoreboardRoot;

/// The column titles' row.
#[derive(Component)]
pub struct ScoreboardHeader;

/// One player's row; the cells are its children.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ScoreboardRow {
    pub name: String,
    pub kills: i32,
    pub deaths: u32,
    /// The rung's gun (empty off a ladder).
    pub gun: String,
    /// Ping in ms (0: the host's, or not measured yet).
    pub ping: u16,
    pub bot: bool,
    pub wins: u32,
    pub local: bool,
}

const COLOR: Color = Color::srgb(0.95, 0.95, 0.95);
const LOCAL: Color = Color::srgb(1.0, 0.85, 0.3);
const WIDTHS: [f32; 6] = [220.0, 60.0, 70.0, 200.0, 70.0, 70.0];

fn spawn(mut commands: Commands) {
    commands.spawn((
        ScoreboardRoot,
        Visibility::Hidden,
        GlobalZIndex(60),
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.75)),
        Node {
            position_type: PositionType::Absolute,
            left: percent(50.0),
            top: percent(12.0),
            margin: UiRect::left(px(-385.0)),
            width: px(770.0),
            padding: UiRect::all(px(12.0)),
            flex_direction: FlexDirection::Column,
            row_gap: px(4.0),
            ..default()
        },
    ));
}

fn cell(parent: &mut ChildSpawnerCommands, text: String, width: f32, right: bool, color: Color) {
    parent.spawn((
        Text::new(text),
        TextFont { font_size: FontSize::Px(18.0), ..default() },
        TextColor(color),
        TextLayout::justify(if right { Justify::Right } else { Justify::Left }),
        Node { width: px(width), ..default() },
    ));
}

/// The rows, best first: rung, kills, fewest deaths, name.
pub fn sorted(mut rows: Vec<(usize, ScoreboardRow)>) -> Vec<ScoreboardRow> {
    rows.sort_by(|(ra, a), (rb, b)| {
        rb.cmp(ra)
            .then(b.kills.cmp(&a.kills))
            .then(a.deaths.cmp(&b.deaths))
            .then(a.name.cmp(&b.name))
    });
    rows.into_iter().map(|(_, r)| r).collect()
}

#[allow(clippy::type_complexity)]
fn draw(
    held: Res<HeldActions>,
    hud: Option<Res<MinigameHud>>,
    ladder: Option<Res<Ladder>>,
    settings: Option<Res<NetSettings>>,
    players: Query<
        (
            Entity,
            Option<&Rung>,
            Option<&Score>,
            Option<&MatchesWon>,
            Option<&NetScore>,
            Option<&NetCharacter>,
            Option<&Name>,
            Has<LocalPlayer>,
        ),
        With<InMinigame>,
    >,
    mut root: Single<(Entity, &mut Visibility), With<ScoreboardRoot>>,
    old: Query<Entity, Or<(With<ScoreboardRow>, With<ScoreboardHeader>)>>,
    mut commands: Commands,
) {
    let show = held.showscores && hud.is_some_and(|h| h.playing());
    root.1.set_if_neq(if show { Visibility::Inherited } else { Visibility::Hidden });
    for e in &old {
        commands.entity(e).despawn();
    }
    if !show {
        return;
    }
    let rows: Vec<(usize, ScoreboardRow)> = players
        .iter()
        .map(|(e, rung, score, won, net, ch, name, local)| {
            let rung = rung.map_or(0, |r| r.0);
            let s = score.copied().unwrap_or_default();
            let gun = ladder
                .as_ref()
                .and_then(|l| l.rungs.get(rung))
                .map(|id| id.split_once(':').map_or(*id, |(_, g)| g).to_string())
                .unwrap_or_default();
            (
                rung,
                ScoreboardRow {
                    name: super::shown_name(e, local, ch, name, settings.as_deref()),
                    kills: s.kills,
                    deaths: s.deaths,
                    gun,
                    ping: net.map_or(0, |n| n.ping),
                    bot: net.is_some_and(|n| n.flags & score_flags::BOT != 0),
                    wins: won.map_or(0, |w| w.0),
                    local,
                },
            )
        })
        .collect();
    let rows = sorted(rows);
    let root_e = root.0;
    commands.entity(root_e).with_children(|p| {
        p.spawn((ScoreboardHeader, Node::default())).with_children(|r| {
            for (i, (h, w)) in ["Player", "Kills", "Deaths", "Gun", "Ping", "Wins"].iter().zip(WIDTHS).enumerate() {
                cell(r, h.to_string(), w, i != 0 && i != 3, COLOR);
            }
        });
        for row in rows {
            let color = if row.local { LOCAL } else { COLOR };
            let ping = if row.bot { "BOT".to_string() } else { row.ping.to_string() };
            let cells = [
                (row.name.clone(), false),
                (row.kills.to_string(), true),
                (row.deaths.to_string(), true),
                (row.gun.clone(), false),
                (ping, true),
                (row.wins.to_string(), true),
            ];
            p.spawn((row, Node::default())).with_children(|r| {
                for ((text, right), w) in cells.into_iter().zip(WIDTHS) {
                    cell(r, text, w, right, color);
                }
            });
        }
    });
}
