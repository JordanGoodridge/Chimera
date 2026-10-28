//! What a playing minigame tells the local player's HUD (D2, L2): data, not
//! drawing. `MinigameHud` is rebuilt every frame by `update` from the
//! active minigame: its name, the status lines its rules module gives the
//! local player (`RulesModule::status`: the ladder's rung, the next gun,
//! the leader), the match's phase and time, and brief notices made of the
//! match's `MinigameEvent`s (the module's `notice` for its own events,
//! `match_won` for every minigame). Empty while no minigame plays. Any
//! game's HUD may show it (Combat Arms' does, `ui::hud`); it names nobody
//! and no weapon itself: a line's text has `{player}` and `{weapon}` where
//! the HUD puts the player's name and the weapon's display name, which
//! only the HUD's game knows (`HudLine`). `update` also tells the shell
//! when the dead local player comes back (`core::RespawnDue`, from
//! `runtime::respawn_due`): the shell's countdown and death view read it.

use bevy::{ecs::message::MessageCursor, prelude::*};

use super::{ActiveMinigame, MinigameEvent};
use crate::{
    core::{LocalPlayer, RespawnAt, RespawnDue, SimClock},
    mode::{Match, Phase},
};

/// How long a notice stays up (s).
pub const NOTICE_SECONDS: f64 = 4.0;

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
        return;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    let viewer = world.query_filtered::<Entity, With<LocalPlayer>>().iter(world).next();
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
    hud.lines.clear();
    hud.results = world.get_resource::<super::results::MatchResults>().cloned();
    if let Some(v) = viewer {
        hud.lines.extend((active.rules.status)(world, v));
        hud.lines.extend(phase_line(world));
        for event in &events {
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
    }
    world.insert_resource(hud);
}
