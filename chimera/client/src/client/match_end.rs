//! The match-end screen of a minigame (Ultimate Gun Game, Lucker Party),
//! drawn by the shell over whichever HUD is up (CS:S's, Combat Arms', the
//! minimal one) for the match-end hold: from the match decided with its
//! results (`chimera_sim::minigame::MatchResults`, through `hud::
//! MinigameHud::results`) while `minigame::NextMatch` is in, through the
//! map change (`MatchHold`) until the next match starts. The win itself
//! (`match_won`) has the round over to its own callout, everyone already
//! frozen (`minigame::runtime`'s end freeze); this comes up at the
//! round's end, after it.
//!
//! - **The winner** big ("You win the match!" under it for the winner
//!   itself), or "Draw".
//! - **The final standings**, best first, in the Tab scoreboard's
//!   columns (`minigame_scoreboard`: name as the board names it, kills,
//!   deaths, the final rung's gun by `minigame_scoreboard::gun_name`,
//!   matches won), with the place; the local player's row highlighted
//!   (found by its character, which the network maps to the client's).
//! - **The next map** and a countdown to the change ("Next map: de_dust2
//!   in 5", then "Changing map to de_dust2"), from `NextMatch` (a
//!   server's, replicated: `chimera_net::minigame_net`).
//! - **A winner sound** once as the screen comes up after a decided match:
//!   a file from an install, chosen by data, each game's
//!   `GameParts::match_won_sound`, or to a player who lost its
//!   `match_lost_sound` where it has one (`PartsRegistry::
//!   match_end_sounds`: the HUD's game first, then the others in the
//!   order registered; the first that loads plays, none if none does), an
//!   interface sound at the mixer's master.
//!
//! While it is up the games' panels leave the results and notices out
//! (`MinigameHud::results_shown`). `MatchEndView` is the state, headless
//! too (tests read it); drawing is
//! Bevy UI. Nothing shows without a minigame, so plain CS:S and Combat
//! Arms are unchanged. Part of `ClientPlugins`.
//!
//! Known shortcuts: no map vote (the server's rotation picks the next
//! map); plain text and colours, no game's look or art; no music pause.

use std::sync::Arc;

use bevy::prelude::*;
use chimera_core::game_parts::PartsRegistry;
use chimera_sim::{
    minigame::{NextMatch, hud::MinigameHud},
    weapon::data::WeaponCatalog,
};

use super::minigame_scoreboard::{GunNames, gun_name};
use crate::{
    audio::{
        clip_asset::SoundAsset,
        live_sound::{Gains, LiveClip},
    },
    core::{LocalPlayer, SimClock},
    gameplay::Gameplay,
    net::{NetCharacter, NetSettings},
};

/// Adds the screen's nodes, its state and its winner sound.
pub struct MatchEndPlugin;

impl Plugin for MatchEndPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MatchEndView>()
            .add_systems(Startup, spawn)
            .add_systems(Update, (follow, draw).chain());
        if app.world().contains_resource::<AssetServer>() {
            app.init_resource::<WinSound>()
                .add_systems(Update, play_sound.after(follow).run_if(resource_exists::<Assets<SoundAsset>>));
        }
    }
}

/// One player's line of the final standings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MatchEndRow {
    /// From 1.
    pub place: usize,
    pub name: String,
    pub kills: i32,
    pub deaths: u32,
    /// The final rung's gun's display name (empty off a ladder).
    pub gun: String,
    pub wins: u32,
    pub winner: bool,
    pub local: bool,
}

/// What the screen shows (headless too).
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct MatchEndView {
    pub shown: bool,
    /// The minigame's name.
    pub title: String,
    /// The winner's name; none for a draw.
    pub winner: Option<String>,
    /// The local player won.
    pub you_won: bool,
    pub rows: Vec<MatchEndRow>,
    /// The next map and the countdown.
    pub next: String,
    /// The winner sound's candidates `(game, file)` asked for when the
    /// screen came up, in order (the player takes them).
    pub sound: Vec<(String, String)>,
    /// How many times a winner sound was asked for.
    pub sounds_asked: u32,
}

/// The countdown line `left` seconds before the change to `next`'s map.
pub fn next_line(next: &NextMatch, left: f64) -> String {
    let left = left.ceil() as i64;
    match (next.map_name(), left > 0) {
        (Some(m), true) => format!("Next map: {m} in {left}"),
        (Some(m), false) => format!("Changing map to {m}"),
        (None, true) => format!("Next match in {left}"),
        (None, false) => "Next match starting".into(),
    }
}

/// The winner sound's candidates: each game's `match_won_sound`, `first`
/// (the HUD's game) before the others.
pub fn sound_candidates(parts: &PartsRegistry, first: Option<&str>) -> Vec<(String, String)> {
    parts.match_end_sounds(first, true)
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn follow(
    mut hud: Option<ResMut<MinigameHud>>,
    next: Option<Res<NextMatch>>,
    clock: Option<Res<SimClock>>,
    gun_names: Option<Res<GunNames>>,
    catalog: Option<Res<WeaponCatalog>>,
    settings: Option<Res<NetSettings>>,
    parts: Option<Res<PartsRegistry>>,
    gameplay: Option<Res<Gameplay>>,
    chars: Query<(Option<&NetCharacter>, Option<&Name>, Has<LocalPlayer>)>,
    mut view: ResMut<MatchEndView>,
) {
    let results = hud.as_ref().filter(|h| h.playing()).and_then(|h| h.results.clone());
    let shown = results.is_some() && next.is_some();
    // The games' panels leave the results to this screen while it is up.
    if let Some(h) = hud.as_mut().filter(|h| h.results_shown != shown) {
        h.results_shown = shown;
    }
    let (Some(results), Some(next)) = (results, next) else {
        if view.shown {
            *view = MatchEndView {
                sounds_asked: view.sounds_asked,
                ..default()
            };
        }
        return;
    };
    let rows: Vec<MatchEndRow> = results
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let found = r.player.and_then(|e| chars.get(e).ok().map(|c| (e, c)));
            let local = found.is_some_and(|(_, (_, _, l))| l);
            let name = match found {
                Some((e, (net, name, l))) => super::shown_name(e, l, net, name, settings.as_deref()),
                None => r.name.clone(),
            };
            MatchEndRow {
                place: i + 1,
                name,
                kills: r.kills,
                deaths: r.deaths,
                gun: r.gun.as_deref().map(|g| gun_name(g, gun_names.as_deref(), catalog.as_deref())).unwrap_or_default(),
                wins: r.wins,
                winner: r.winner,
                local,
            }
        })
        .collect();
    let winner = rows.iter().find(|r| r.winner);
    let now = clock.map_or(0.0, |c| c.now);
    let line = next_line(&next, next.at - now);
    let came_up = !view.shown;
    let mut v = MatchEndView {
        shown: true,
        title: results.title.clone(),
        winner: winner.map(|w| w.name.clone()),
        you_won: winner.is_some_and(|w| w.local),
        rows: rows.clone(),
        next: line,
        sound: view.sound.clone(),
        sounds_asked: view.sounds_asked,
    };
    if came_up && v.winner.is_some() {
        v.sound = parts.map_or_else(Vec::new, |p| p.match_end_sounds(gameplay.as_ref().map(|g| g.hud), v.you_won));
        v.sounds_asked += 1;
    }
    view.set_if_neq(v);
}

/// The screen's root (hidden while there is nothing to show).
#[derive(Component)]
pub struct MatchEndRoot;

/// The headline, the line under it, the standings and the countdown.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchEndPart {
    Title,
    Winner,
    Subtitle,
    Standings,
    Next,
}

const TEXT: Color = Color::srgb(0.95, 0.95, 0.95);
const DIM: Color = Color::srgb(0.7, 0.7, 0.72);
const GOLD: Color = Color::srgb(1.0, 0.82, 0.25);
const LOCAL: Color = Color::srgb(1.0, 0.85, 0.3);
const LOCAL_BG: Color = Color::srgba(1.0, 0.8, 0.2, 0.18);
/// Place, Name, Kills, Deaths, Gun, Wins.
pub const WIDTHS: [f32; 6] = [50.0, 240.0, 80.0, 90.0, 260.0, 80.0];
const GAP: f32 = 10.0;

fn text(part: MatchEndPart, size: f32, color: Color) -> impl Bundle {
    (
        part,
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
        TextLayout::justify(Justify::Center),
    )
}

fn spawn(mut commands: Commands) {
    commands
        .spawn((
            MatchEndRoot,
            Visibility::Hidden,
            GlobalZIndex(75),
            BackgroundColor(Color::srgba(0.02, 0.02, 0.03, 0.82)),
            Node {
                position_type: PositionType::Absolute,
                left: px(0.0),
                top: px(0.0),
                width: percent(100.0),
                height: percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::top(percent(8.0)),
                row_gap: px(10.0),
                ..default()
            },
        ))
        .with_children(|c| {
            c.spawn(text(MatchEndPart::Title, 22.0, DIM));
            c.spawn(text(MatchEndPart::Winner, 64.0, GOLD));
            c.spawn(text(MatchEndPart::Subtitle, 26.0, TEXT));
            c.spawn((
                MatchEndPart::Standings,
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(4.0),
                    margin: UiRect::vertical(px(18.0)),
                    padding: UiRect::all(px(12.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
            ));
            c.spawn(text(MatchEndPart::Next, 28.0, TEXT));
        });
}

/// One standings row's node.
#[derive(Component)]
struct StandingsRow;

fn cell(parent: &mut ChildSpawnerCommands, text: String, width: f32, right: bool, color: Color) {
    parent.spawn((
        Text::new(text),
        TextFont {
            font_size: FontSize::Px(20.0),
            ..default()
        },
        TextColor(color),
        TextLayout::justify(if right { Justify::Right } else { Justify::Left }),
        Node {
            width: px(width),
            flex_shrink: 0.0,
            overflow: Overflow::clip(),
            ..default()
        },
    ));
}

fn draw(
    view: Res<MatchEndView>,
    mut root: Query<&mut Visibility, With<MatchEndRoot>>,
    mut parts: Query<(Entity, &MatchEndPart, Option<&mut Text>)>,
    old: Query<Entity, With<StandingsRow>>,
    mut commands: Commands,
    mut drawn: Local<Option<(Option<String>, Vec<MatchEndRow>)>>,
) {
    if !view.is_changed() {
        return;
    }
    for mut v in &mut root {
        v.set_if_neq(if view.shown { Visibility::Inherited } else { Visibility::Hidden });
    }
    let (headline, sub) = match (&view.winner, view.you_won) {
        (Some(w), true) => (w.clone(), "You win the match!".to_string()),
        (Some(w), false) => (w.clone(), "wins the match".to_string()),
        (None, _) => ("Draw".to_string(), "Nobody wins the match".to_string()),
    };
    let mut standings = None;
    for (e, part, t) in &mut parts {
        let want = match part {
            MatchEndPart::Title => view.title.clone(),
            MatchEndPart::Winner => headline.clone(),
            MatchEndPart::Subtitle => sub.clone(),
            MatchEndPart::Next => view.next.clone(),
            MatchEndPart::Standings => {
                standings = Some(e);
                continue;
            }
        };
        if let Some(mut t) = t
            && t.0 != want
        {
            t.0 = want;
        }
    }
    // The standings again only when they changed (not each second).
    let now = (view.shown.then(|| view.title.clone()), view.rows.clone());
    if drawn.as_ref() == Some(&now) {
        return;
    }
    *drawn = Some(now);
    for e in &old {
        commands.entity(e).despawn();
    }
    let Some(board) = standings.filter(|_| view.shown) else { return };
    let header = ["#", "Name", "Kills", "Deaths", "Gun", "Wins"];
    commands.entity(board).with_children(|p| {
        p.spawn((StandingsRow, Node { column_gap: px(GAP), ..default() })).with_children(|r| {
            for (i, (h, w)) in header.iter().zip(WIDTHS).enumerate() {
                cell(r, h.to_string(), w, !matches!(i, 1 | 4), DIM);
            }
        });
        for row in &view.rows {
            let color = if row.local { LOCAL } else if row.winner { GOLD } else { TEXT };
            let cells = [
                (row.place.to_string(), true),
                (row.name.clone(), false),
                (row.kills.to_string(), true),
                (row.deaths.to_string(), true),
                (row.gun.clone(), false),
                (row.wins.to_string(), true),
            ];
            p.spawn((
                StandingsRow,
                Node {
                    column_gap: px(GAP),
                    padding: UiRect::vertical(px(2.0)),
                    ..default()
                },
                BackgroundColor(if row.local { LOCAL_BG } else { Color::NONE }),
            ))
            .with_children(|r| {
                for ((text, right), w) in cells.into_iter().zip(WIDTHS) {
                    cell(r, text, w, right, color);
                }
            });
        }
    });
}

/// The winner sound being loaded: the candidates left and the one
/// loading, since when.
#[derive(Resource, Default)]
struct WinSound {
    left: Vec<(String, String)>,
    loading: Option<(String, Handle<SoundAsset>, f64)>,
}

/// The first candidate that loads plays once (a missing file: the next;
/// none after 3 s of loading).
fn play_sound(
    mut view: ResMut<MatchEndView>,
    mut state: ResMut<WinSound>,
    server: Res<AssetServer>,
    time: Res<Time>,
    sounds: Res<Assets<SoundAsset>>,
    clips: Option<ResMut<Assets<LiveClip>>>,
    mut commands: Commands,
) {
    if !view.sound.is_empty() {
        state.left = std::mem::take(&mut view.bypass_change_detection().sound);
        state.loading = None;
    }
    let now = time.elapsed_secs_f64();
    loop {
        if state.loading.is_none() {
            if state.left.is_empty() {
                return;
            }
            let (game, file) = state.left.remove(0);
            let path = chimera_present::assets::install_path(&game, &file);
            state.loading = Some((path.clone(), server.load(path), now));
        }
        let Some((path, handle, since)) = state.loading.clone() else { return };
        if let Some(sound) = sounds.get(&handle) {
            if let Some(mut clips) = clips {
                let gains = Arc::new(Gains::new(1.0, 1.0));
                let clip = clips.add(LiveClip::new(sound.0.clone(), gains, default()));
                commands.spawn((AudioPlayer(clip), PlaybackSettings::DESPAWN));
                info!("match end: winner sound {path}");
            }
            *state = WinSound::default();
            return;
        }
        if server.load_state(&handle).is_failed() {
            info!("match end: no winner sound at {path}");
            state.loading = None;
            continue;
        }
        if now - since > 3.0 {
            *state = WinSound::default();
        }
        return;
    }
}
