//! A minigame's big callouts, drawn and sounded by the shell for any
//! game (docs/DESIGN.md, "Announcers"): the callout the minigame keeps
//! for the local player (`chimera_sim::minigame::hud::MinigameHud::
//! callout`: "Level up: <gun>", "Last level!", "<name> is on the last
//! level!", "You took the lead", "You win!", ...) centre screen, its
//! first line big, for `hud::CALLOUT_SECONDS`, and its cue's sound once
//! (`chimera_core::cues`: the pack the definition's `cues` picks, else the
//! pack of the game whose HUD is up, else the first a game registered;
//! a cue the pack lacks, or no pack, is silent).
//! It is the shell's own overlay (not a `HudRoot`), so it looks the same
//! on CS:S's HUD, Combat Arms' and the minimal one; plain CS:S and Combat
//! Arms play no minigame, so it never shows there. `{player}` is the
//! player's shown name (`super::shown_name`), `{weapon}` the gun's display
//! name (`minigame_scoreboard::gun_name`). Part of `ClientPlugins`.
//!
//! `chimera_callout <event> [other] [rung]` makes one of the minigame's
//! events (`rung_up`, `last_rung`, `lead_taken`, `match_won`, ...) for the
//! local player, or another player in it, as if the server had sent it (a
//! test, for screenshots). `CalloutView` is the state, headless too (tests read the text and the
//! cues heard); the sounds (`SoundAsset`s as local interface sounds at
//! the pack's volume) play where there is an asset server.
//!
//! Known shortcuts: plain text on a dark band, no animation; the
//! files load on first use (the first cue of each kind may come up to
//! a load late, given up after 3 s).

use std::{collections::BTreeMap, sync::Arc};

use bevy::prelude::*;
use chimera_core::cues::{CuePack, CuePacks};
use chimera_sim::{
    minigame::{
        ActiveMinigame,
        hud::{HudLine, MinigameHud},
    },
    weapon::data::WeaponCatalog,
};

use super::minigame_scoreboard::{GunNames, gun_name};
use crate::{
    audio::{
        clip_asset::SoundAsset,
        live_sound::{Gains, LiveClip},
    },
    core::LocalPlayer,
    net::{NetCharacter, NetSettings},
};

/// Adds the callout's node, its drawing and its sounds.
pub struct CalloutsPlugin;

impl Plugin for CalloutsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CalloutView>()
            .init_resource::<CuePacks>()
            .add_systems(Startup, spawn)
            .add_systems(Update, draw);
        crate::console::ConsoleAppExt::console_command(
            app,
            "chimera_callout",
            "Make a minigame event for you, or another player, as the server would (a test): \
             chimera_callout <rung_up|rung_down|suicide_down|last_rung|lead_taken|lead_lost|lead_tied|match_won> [other] [rung].",
            test_callout,
        );
        if app.world().contains_resource::<AssetServer>() {
            app.init_resource::<CueAssets>()
                .add_systems(Update, play.after(draw).run_if(resource_exists::<Assets<SoundAsset>>));
        }
    }
}

/// What the callout shows and the cues it sounded.
#[derive(Resource, Clone, Debug, Default)]
pub struct CalloutView {
    /// The lines showing (empty: hidden).
    pub lines: Vec<String>,
    /// The number of the last callout seen.
    pub id: u32,
    /// Every cue sounded (its key), for the console and tests (the last
    /// 32).
    pub heard: Vec<String>,
    /// Cues waiting for the sound system: (pack, file).
    pub to_play: Vec<(Arc<CuePack>, String)>,
}

/// The callout's root (hidden while there is none).
#[derive(Component)]
pub struct CalloutRoot;

/// One of its lines (by index; the first is big).
#[derive(Component)]
pub struct CalloutLine(pub usize);

fn spawn(mut commands: Commands) {
    commands
        .spawn((
            CalloutRoot,
            Visibility::Hidden,
            GlobalZIndex(65),
            Pickable::IGNORE,
            Node {
                position_type: PositionType::Absolute,
                left: px(0.0),
                width: percent(100.0),
                top: percent(31.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(4.0),
                ..default()
            },
        ))
        .with_children(|c| {
            for i in 0..chimera_sim::minigame::hud::CALLOUT_LINES {
                c.spawn((
                    CalloutLine(i),
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(if i == 0 { 44.0 } else { 24.0 }),
                        ..default()
                    },
                    TextColor(if i == 0 { Color::srgb(1.0, 0.85, 0.3) } else { Color::WHITE }),
                    TextShadow::default(),
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
                    Node {
                        padding: UiRect::axes(px(18.0), px(4.0)),
                        ..default()
                    },
                    Visibility::Hidden,
                ));
            }
        });
}

/// `chimera_callout`: the event, for the local player (or `other`: the
/// first other player in the minigame); the rung by default the
/// ladder's last.
fn test_callout(world: &mut World, args: &[String]) -> Result<Option<String>, String> {
    let name = args.first().ok_or("chimera_callout <event> [other] [rung]")?;
    let name = *chimera_sim::minigame::EVENT_NAMES
        .iter()
        .find(|n| n.eq_ignore_ascii_case(name))
        .ok_or_else(|| format!("no event `{name}` ({})", chimera_sim::minigame::EVENT_NAMES.join(" ")))?;
    let other = args.iter().any(|a| a.eq_ignore_ascii_case("other"));
    let player = if other {
        world
            .query_filtered::<Entity, (With<chimera_sim::minigame::InMinigame>, Without<LocalPlayer>)>()
            .iter(world)
            .next()
            .ok_or("no other player in the minigame")?
    } else {
        world.query_filtered::<Entity, With<LocalPlayer>>().iter(world).next().ok_or("no local player")?
    };
    let last = world
        .get_resource::<chimera_sim::minigame::ladder::Ladder>()
        .map_or(0, |l| l.rungs.len().saturating_sub(1));
    let value = args.iter().find_map(|a| a.parse().ok()).unwrap_or(last as i32);
    world.write_message(chimera_sim::minigame::MinigameEvent {
        name,
        player: Some(player),
        value,
    });
    Ok(None)
}

/// `line`'s text with `{player}` and `{weapon}` filled in.
pub fn fill(line: &HudLine, player: impl Fn(Entity) -> String, weapon: impl Fn(&str) -> String) -> String {
    let mut text = line.text.clone();
    if let Some(p) = line.player {
        text = text.replace("{player}", &player(p));
    }
    if let Some(w) = line.weapon {
        text = text.replace("{weapon}", &weapon(w));
    }
    text
}

#[allow(clippy::too_many_arguments)]
fn draw(
    hud: Option<Res<MinigameHud>>,
    active: Option<Res<ActiveMinigame>>,
    packs: Res<CuePacks>,
    mut view: ResMut<CalloutView>,
    names: Option<Res<GunNames>>,
    catalog: Option<Res<WeaponCatalog>>,
    settings: Option<Res<NetSettings>>,
    gameplay: Option<Res<chimera_core::gameplay::Gameplay>>,
    players: Query<(Option<&NetCharacter>, Option<&Name>, Has<LocalPlayer>)>,
    mut root: Query<&mut Visibility, (With<CalloutRoot>, Without<CalloutLine>)>,
    mut lines: Query<(&CalloutLine, &mut Text, &mut Visibility), Without<CalloutRoot>>,
) {
    let shown = hud.as_ref().and_then(|h| h.callout.clone());
    let player = |e: Entity| {
        let (ch, name, local) = players.get(e).unwrap_or((None, None, false));
        super::shown_name(e, local, ch, name, settings.as_deref())
    };
    let weapon = |id: &str| gun_name(id, names.as_deref(), catalog.as_deref());
    let text: Vec<String> = shown
        .as_ref()
        .map(|s| s.callout.lines.iter().map(|l| fill(l, player, weapon)).collect())
        .unwrap_or_default();
    if let Some(s) = &shown
        && s.id != view.id
    {
        view.id = s.id;
        if let Some(cue) = s.callout.cue {
            view.heard.push(cue.to_string());
            let excess = view.heard.len().saturating_sub(32);
            view.heard.drain(..excess);
            let pack = packs
                .choose_for(active.as_ref().and_then(|a| a.def.cues.as_deref()), gameplay.as_ref().map(|g| g.hud))
                .cloned();
            if let Some(pack) = pack
                && let Some(file) = pack.sound(cue).map(str::to_string)
            {
                view.to_play.push((pack, file));
            }
        }
    }
    if view.to_play.len() > 8 {
        let excess = view.to_play.len() - 8;
        view.to_play.drain(..excess);
    }
    for mut v in &mut root {
        v.set_if_neq(if text.is_empty() { Visibility::Hidden } else { Visibility::Inherited });
    }
    for (line, mut t, mut v) in &mut lines {
        let s = text.get(line.0);
        v.set_if_neq(if s.is_some() { Visibility::Inherited } else { Visibility::Hidden });
        if let Some(s) = s
            && t.0 != *s
        {
            t.0 = s.clone();
        }
    }
    if view.lines != text {
        view.lines = text;
    }
}

/// The cue files loaded (kept once asked for) and the sounds waiting for
/// theirs.
#[derive(Resource, Default)]
struct CueAssets {
    sounds: BTreeMap<String, Handle<SoundAsset>>,
    waiting: Vec<(Handle<SoundAsset>, f32, f64)>,
}

/// The cues, as local interface sounds, once their files are loaded
/// (given up after 3 s).
fn play(
    mut view: ResMut<CalloutView>,
    time: Res<Time>,
    server: Res<AssetServer>,
    mut assets: ResMut<CueAssets>,
    sounds: Res<Assets<SoundAsset>>,
    clips: Option<ResMut<Assets<LiveClip>>>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    for (pack, file) in std::mem::take(&mut view.to_play) {
        let path = chimera_present::assets::install_path(&pack.game, &file);
        let h = assets.sounds.entry(path.clone()).or_insert_with(|| server.load(path)).clone();
        assets.waiting.push((h, pack.volume, now));
    }
    let Some(mut clips) = clips else { return };
    assets.waiting.retain(|(h, volume, since)| {
        if let Some(sound) = sounds.get(h) {
            let gains = Arc::new(Gains::new(*volume, *volume));
            let clip = clips.add(LiveClip::new(sound.0.clone(), gains, default()));
            commands.spawn((AudioPlayer(clip), PlaybackSettings::DESPAWN));
            return false;
        }
        now - since < 3.0
    });
}
