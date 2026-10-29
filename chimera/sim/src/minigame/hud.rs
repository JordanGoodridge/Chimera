//! What a playing minigame tells the local player's HUD (D2, L2): data, not
//! drawing. `MinigameHud` is rebuilt every frame by `update` from the
//! active minigame: its name, the status lines its rules module gives the
//! local player (`RulesModule::status`: the ladder's rung, the next gun,
//! the leader), the match's phase and time, and brief notices made of the
//! match's `MinigameEvent`s (the module's `notice` for its own events,
//! `match_won` for every minigame; no HUD draws them now: each says a
//! moment the callout below says big, and drawn too they were a second
//! banner). Empty while no minigame plays. Any
//! game's HUD may show it (Combat Arms' and CS:S's do); it names nobody
//! and no weapon itself: a line's text has `{player}` and `{weapon}` where
//! the HUD puts the player's name and the weapon's display name, which
//! only the HUD's game knows (`HudLine`). The big moments for the viewer
//! (`Callout`: a rung up or down, the last rung, yours or another's, the
//! lead taken, lost or tied, the match won or lost) are kept apart
//! (`MinigameHud::callout`): the client shell draws them centre screen
//! and plays their cue's sound (`chimera_client::client::callouts`), or
//! the HUD up draws them in its own looks (Combat Arms'). The callouts of one frame merge into one: the highest
//! level's lines, then the others' first lines (at most `CALLOUT_LINES`),
//! with its cue; it replaces the one showing unless that one has a
//! higher level and is younger than `CALLOUT_HOLD`. `update` also tells the shell
//! when the dead local player comes back (`core::RespawnDue`, from
//! `runtime::respawn_due`): the shell's countdown and death view read it,
//! and then the match's moments for a music director (`moments::update`).
//!
//! Known shortcuts: the notices (`RulesModule::notice`) are still made
//! though no HUD draws them any more (only tests read them); drop them,
//! or give them a place that is no banner (a log), when one is wanted.

use bevy::{ecs::message::MessageCursor, prelude::*};

use super::{ActiveMinigame, MinigameEvent};
use crate::{
    core::{LocalPlayer, RespawnAt, RespawnDue, SimClock},
    mode::{Match, Phase},
};

/// How long a notice stays up (s).
pub const NOTICE_SECONDS: f64 = 4.0;

/// How long a callout stays up (s).
pub const CALLOUT_SECONDS: f64 = 3.0;

/// How long a callout keeps a lower-level one from replacing it (s).
pub const CALLOUT_HOLD: f64 = 1.5;

/// The most lines a merged callout has.
pub const CALLOUT_LINES: usize = 3;

/// A big moment for the viewer: lines (the first big), a cue key for
/// the sound (`chimera_core::cues`: `rung_up`, `last_rung`, ...) and a
/// level (a higher one wins).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Callout {
    pub lines: Vec<HudLine>,
    pub cue: Option<&'static str>,
    pub level: i32,
}

impl Callout {
    pub fn new(level: i32, cue: &'static str, lines: Vec<HudLine>) -> Self {
        Self {
            lines,
            cue: Some(cue),
            level,
        }
    }
}

/// The callout showing: since when (`Time`'s seconds) and its number
/// (each new one counts up: the shell plays its cue once).
#[derive(Clone, Debug, PartialEq)]
pub struct ShownCallout {
    pub callout: Callout,
    pub since: f64,
    pub id: u32,
}

/// One frame's callouts merged into one (module doc); None for none.
pub fn merge(mut callouts: Vec<Callout>) -> Option<Callout> {
    callouts.sort_by_key(|c| -c.level);
    let mut it = callouts.into_iter();
    let mut first = it.next()?;
    for c in it {
        if first.lines.len() >= CALLOUT_LINES {
            break;
        }
        first.lines.extend(c.lines.into_iter().next());
    }
    first.lines.truncate(CALLOUT_LINES);
    Some(first)
}

/// One line of the minigame's HUD: text with `{player}` (the HUD's name
/// for `player`) and `{weapon}` (the weapon's display name) in it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HudLine {
    pub text: String,
    pub player: Option<Entity>,
    pub weapon: Option<&'static str>,
}

impl HudLine {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..default()
        }
    }

    pub fn player(mut self, e: Entity) -> Self {
        self.player = Some(e);
        self
    }

    pub fn weapon(mut self, id: &'static str) -> Self {
        self.weapon = Some(id);
        self
    }
}

/// What the minigame shows the local player this frame.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct MinigameHud {
    /// The minigame's name; empty while none plays.
    pub title: String,
    /// Its status, top to bottom.
    pub lines: Vec<HudLine>,
    /// Brief notices, oldest first, each with when (`Time`'s seconds) it
    /// goes.
    pub notices: Vec<(HudLine, f64)>,
    /// The finished match's results while the match is decided.
    pub results: Option<super::results::MatchResults>,
    /// The shell's match-end screen shows the results over the whole
    /// screen (`chimera_client::client::match_end` sets it): a game's
    /// panel leaves the results and notices out meanwhile.
    pub results_shown: bool,
    /// The callout showing, for `CALLOUT_SECONDS` (module doc).
    pub callout: Option<ShownCallout>,
    /// The last callout's number.
    pub callouts: u32,
}

impl MinigameHud {
    /// Whether a minigame is playing.
    pub fn playing(&self) -> bool {
        !self.title.is_empty()
    }
}

/// The match's own line: waiting for players, the time left, the end.
fn phase_line(world: &World) -> Option<HudLine> {
    let m = world.resource::<Match>();
    match m.phase {
        Phase::Waiting | Phase::Starting => Some(HudLine::new("Waiting for players")),
        Phase::Live => {
            let now = world.get_resource::<SimClock>()?.now;
            let left = (m.round_ends? - now).max(0.0).ceil() as u32;
            Some(HudLine::new(format!("Time {}:{:02}", left / 60, left % 60)))
        }
        Phase::RoundOver { .. } | Phase::Intermission { .. } | Phase::MatchOver => Some(HudLine::new("Match over")),
        Phase::Setup { .. } => None,
    }
}

/// `core::RespawnDue`, written only when it changes.
fn set_respawn_due(world: &mut World, due: Option<RespawnAt>) {
    match world.get_resource_mut::<RespawnDue>() {
        Some(mut r) => {
            r.set_if_neq(RespawnDue(due));
        }
        None if due.is_some() => world.insert_resource(RespawnDue(due)),
        None => {}
    }
}

/// `MinigameHud` for this frame (an exclusive system, `Update`), and the
/// local player's `core::RespawnDue` (`runtime::respawn_due`).
pub fn update(world: &mut World, mut cursor: Local<MessageCursor<MinigameEvent>>) {
    let events: Vec<MinigameEvent> = world
        .get_resource::<Messages<MinigameEvent>>()
        .map(|m| cursor.read(m).cloned().collect())
        .unwrap_or_default();
    let Some(active) = world.get_resource::<ActiveMinigame>().copied() else {
        if world.get_resource::<MinigameHud>().is_some_and(MinigameHud::playing) {
            world.insert_resource(MinigameHud::default());
        }
        set_respawn_due(world, None);
        super::moments::update(world);
        return;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    let viewer = chimera_core::query_cache::first::<With<LocalPlayer>>(world);
    let due = viewer.and_then(|v| {
        let at = super::runtime::respawn_due(world, v)?;
        Some(RespawnAt {
            at,
            delay: active.def.spawn.respawn.delay as f64,
        })
    });
    set_respawn_due(world, due);
    let mut hud = world.remove_resource::<MinigameHud>().unwrap_or_default();
    hud.title = active.def.name.clone();
    hud.notices.retain(|(_, until)| *until > now);
    if hud.callout.as_ref().is_some_and(|c| now - c.since >= CALLOUT_SECONDS) {
        hud.callout = None;
    }
    hud.lines.clear();
    hud.results = world.get_resource::<super::results::MatchResults>().cloned();
    if let Some(v) = viewer {
        hud.lines.extend((active.rules.status)(world, v));
        hud.lines.extend(phase_line(world));
        let mut callouts = Vec::new();
        for event in &events {
            callouts.extend(match event.name {
                "match_won" => Some(match event.player {
                    Some(p) if p == v => Callout::new(5, "match_won", vec![HudLine::new("You win!")]),
                    Some(p) => Callout::new(5, "match_lost", vec![HudLine::new("{player} wins!").player(p)]),
                    None => Callout {
                        lines: vec![HudLine::new("Match over")],
                        cue: None,
                        level: 5,
                    },
                }),
                _ => (active.rules.callout)(world, v, event),
            });
            let notice = match event.name {
                "match_won" => Some(match event.player {
                    Some(p) if p == v => HudLine::new("You win the match!"),
                    Some(p) => HudLine::new("{player} wins the match").player(p),
                    None => HudLine::new("Match over"),
                }),
                _ => (active.rules.notice)(world, v, event),
            };
            if let Some(n) = notice {
                hud.notices.push((n, now + NOTICE_SECONDS));
            }
        }
        if let Some(c) = merge(callouts) {
            let held = hud.callout.as_ref().is_some_and(|s| s.callout.level > c.level && now - s.since < CALLOUT_HOLD);
            if !held {
                hud.callouts = hud.callouts.wrapping_add(1);
                hud.callout = Some(ShownCallout {
                    callout: c,
                    since: now,
                    id: hud.callouts,
                });
            }
        }
    }
    world.insert_resource(hud);
    super::moments::update(world);
}
