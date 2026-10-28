//! A playing minigame on CS:S's HUD (`chimera_sim::minigame::hud::
//! MinigameHud`: its name, the rules' status lines, the gun game's rung,
//! next gun and leader among them, the match's time and the notices), for
//! an app whose minigames may show CS:S's HUD (Lucker Party): top centre,
//! in the client scheme's fonts and the HUD's colour, as CS:S prints its
//! centre messages. A line's `{player}` is the name the scoreboard shows,
//! its `{weapon}` the gun's name in CS:S's own strings (the install's
//! `resource/cstrike_english.txt`, `weapons::display_names`), else the
//! id's last part in capitals. A finished match's results (`MinigameHud::
//! results`: the winner, then a line a player) take the notices' place
//! under the status lines. Its root is a `client::hud::HudRoot`: down
//! while another game's HUD is the gameplay's.
//!
//! Known shortcuts: the panel's place, sizes and colours are ours (CS:S
//! has no such panel; nothing to measure).

use std::collections::BTreeMap;

use bevy::prelude::*;
use chimera_sim::minigame::hud::{HudLine, MinigameHud};

use crate::{
    client::{fonts::UiFonts, hud::HudRoot},
    core::LocalPlayer,
    game_registry::Games,
    map::hud::ActiveHud,
};

/// The minigame's panel on CS:S's HUD.
pub struct MinigameHudPlugin;

impl Plugin for MinigameHudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WeaponNames>()
            .add_systems(Startup, read_names)
            .add_systems(Update, draw.run_if(crate::gameplay::hud_is(crate::game::GAME)));
    }
}

/// CS:S's guns' names (weapon id to its name in the install's strings).
#[derive(Resource, Clone, Debug, Default)]
pub struct WeaponNames(pub BTreeMap<String, String>);

/// The panel's root.
#[derive(Component)]
pub struct MinigamePanel;

/// The text of one line of the panel (title, status, notice).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelLine {
    Title,
    Status,
    Notice,
    /// The finished match's results: the winner, then a line a player.
    Result,
}

fn read_names(games: Option<Res<Games>>, mut names: ResMut<WeaponNames>) {
    let Some(mount) = games.and_then(|g| g.get(crate::game::GAME)).and_then(|g| g.mount()) else { return };
    names.0.extend(crate::game::weapons::display_names(&mount));
}

/// A weapon id as a readable name when nothing names it: its last part
/// in capitals, CS:S's `weapon_` left off, `_` as spaces.
pub fn plain_name(id: &str) -> String {
    let last = id.rsplit(':').next().unwrap_or(id);
    let last = last.strip_prefix("weapon_").unwrap_or(last);
    last.chars().map(|c| if c == '_' { ' ' } else { c.to_ascii_uppercase() }).collect()
}

/// The panel's lines this frame: `{player}` and `{weapon}` filled in.
pub fn panel_lines(
    m: &MinigameHud,
    player: impl Fn(Entity) -> String,
    weapon: impl Fn(&str) -> String,
) -> Vec<(PanelLine, String)> {
    panel_lines_with(m, player, weapon, |r| r.name.clone())
}

/// `panel_lines` with `row_name` wording a result's player who isn't
/// a character on this side (a network client's results).
pub fn panel_lines_with(
    m: &MinigameHud,
    player: impl Fn(Entity) -> String,
    weapon: impl Fn(&str) -> String,
    row_name: impl Fn(&chimera_sim::minigame::results::ResultRow) -> String,
) -> Vec<(PanelLine, String)> {
    if !m.playing() {
        return Vec::new();
    }
    let word = |l: &HudLine| {
        let mut text = l.text.clone();
        if let Some(p) = l.player {
            text = text.replace("{player}", &player(p));
        }
        if let Some(w) = l.weapon {
            text = text.replace("{weapon}", &weapon(w));
        }
        text
    };
    let mut out = vec![(PanelLine::Title, m.title.clone())];
    out.extend(m.lines.iter().map(|l| (PanelLine::Status, word(l))));
    match &m.results {
        // The shell's match-end screen has them.
        _ if m.results_shown => {}
        Some(r) => out.extend(
            r.lines(|row| row.player.map_or_else(|| row_name(row), &player)).into_iter().map(|t| (PanelLine::Result, t)),
        ),
        None => out.extend(m.notices.iter().map(|(l, _)| (PanelLine::Notice, word(l)))),
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn draw(
    minigame: Option<Res<MinigameHud>>,
    names: Res<WeaponNames>,
    who: Query<(Option<&Name>, Has<LocalPlayer>, Option<&crate::net::NetCharacter>)>,
    settings: Option<Res<crate::net::NetSettings>>,
    (fonts, hud, windows): (Option<Res<UiFonts>>, Option<Res<ActiveHud>>, Query<&Window>),
    panel: Query<Entity, With<MinigamePanel>>,
    mut last: Local<Option<(Vec<(PanelLine, String)>, f32)>>,
    mut commands: Commands,
) {
    let lines = minigame.as_deref().map_or_else(Vec::new, |m| {
        panel_lines_with(
            m,
            |e| match who.get(e) {
                Ok((name, local, net)) => crate::client::shown_name(e, local, net, name, settings.as_deref()),
                Err(_) => e.to_string(),
            },
            |w| names.0.get(w).cloned().unwrap_or_else(|| plain_name(w)),
            |r| r.name.clone(),
        )
    });
    let h = windows.iter().next().map_or(480.0, |w| w.height());
    let now = (lines.clone(), h);
    if last.as_ref() == Some(&now) && (lines.is_empty() || !panel.is_empty()) {
        return;
    }
    *last = Some(now);
    for e in &panel {
        commands.entity(e).despawn();
    }
    if lines.is_empty() {
        return;
    }
    let colour = hud
        .as_deref()
        .and_then(|h| h.0.color("FgColor"))
        .unwrap_or(Color::srgba_u8(255, 176, 0, 255));
    let font = |name: &str, fallback: f32| match fonts.as_deref() {
        Some(f) => f.client(name, h, fallback),
        None => TextFont {
            font_size: bevy::text::FontSize::Px(fallback),
            ..default()
        },
    };
    let root = commands
        .spawn((
            MinigamePanel,
            HudRoot(crate::game::GAME),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Percent(2.0),
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            },
            GlobalZIndex(42),
        ))
        .id();
    let mut first_result = true;
    for (kind, text) in lines {
        let (f, c) = match kind {
            PanelLine::Title => (font("CenterPrintText", 18.0), colour),
            PanelLine::Status => (font("ChatFont", 14.0), Color::WHITE),
            PanelLine::Notice => (font("CenterPrintText", 18.0), Color::srgba_u8(255, 230, 120, 255)),
            PanelLine::Result if first_result => {
                first_result = false;
                (font("CenterPrintText", 18.0), Color::srgba_u8(255, 230, 120, 255))
            }
            PanelLine::Result => (font("ChatFont", 14.0), Color::WHITE),
        };
        commands.spawn((kind, Text::new(text), f, TextColor(c), TextShadow::default(), ChildOf(root)));
    }
}
