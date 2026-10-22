//! Bot chat (specs/combat_arms/bots.md 10, Open question 23): on its
//! events a bot may say a line of its chat set. The server checks the
//! bot's cooldown (15, 20, 25 s by `BotLevel`); if it has passed it
//! restarts it **before** the roll, then shows a line with probability by
//! the event's percent: the milliseconds since start mod 100 at most the
//! percent (`math::chat_roll`). The line is the string-table key
//! `$Message<set>_<Event>_<0..9>` (the set, 0-2, drawn once per bot), shown
//! in the chat box for 10 s as "nickname : line" (`CaBotChat`; the front
//! end resolves the key through the game's string table, `shown_line`:
//! our own text for a key the table lacks).
//!
//! Known shortcuts: the events played are a kill by the bot, its death
//! and its respawn (the battle, search, grenade and mine events of the
//! brain's states aren't hooked yet); the line isn't sent to network
//! clients.

use bevy::prelude::*;

use super::{BotSettings, CaBot, math};
use crate::core::{Died, Dead, SimClock};

/// A bot's chat line, for the chat box.
#[derive(Message, Clone, Debug, PartialEq)]
pub struct CaBotChat {
    pub bot: Entity,
    /// The string-table key (`$Message0_Kill_3`).
    pub key: String,
}

/// How long a line shows (s).
pub const LINE_LIFE: f64 = 10.0;

/// A chat event (10) and its percent (`<Event>_Message_PrintPrecent`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Kill,
    Death,
    Respawn,
    Battle,
    Search,
    Grenade,
    Mine,
}

impl Event {
    pub fn name(self) -> &'static str {
        match self {
            Event::Kill => "Kill",
            Event::Death => "Death",
            Event::Respawn => "Respawn",
            Event::Battle => "Battle",
            Event::Search => "Search",
            Event::Grenade => "Grenade",
            Event::Mine => "Mine",
        }
    }

    pub fn percent(self) -> u32 {
        match self {
            Event::Kill | Event::Death | Event::Mine => 60,
            Event::Respawn | Event::Battle | Event::Search | Event::Grenade => 25,
        }
    }
}

/// The nine events' names in a line's string key, with our own lines for
/// each (a key's number picks one): what a bot says when the install's
/// string table has no text for its key (the shipped table defines only
/// some of the sets) or there is no install.
const BUILT_IN: [(&str, [&str; 5]); 9] = [
    ("Kill", ["Got you!", "Nothing personal.", "Too easy.", "Better luck next time.", "One down."]),
    ("Death", ["Ouch!", "Nice shot.", "I'll be back.", "That one hurt.", "Well played."]),
    ("Respawn", ["Back in the fight!", "Here I come again.", "Round two.", "Ready.", "Let's go."]),
    ("Battle", ["Contact!", "Engaging!", "Enemy spotted!", "Open fire!", "They're here!"]),
    ("Search", ["Moving out.", "Keep your eyes open.", "Where are they?", "Searching.", "Stay sharp."]),
    ("Grenade", ["Grenade!", "Take cover!", "Watch out!", "Incoming!", "Get down!"]),
    ("Mine", ["Watch your step, a mine!", "Mine ahead!", "Careful, mines!", "Don't step there!", "Mine spotted!"]),
    ("Conceal", ["Staying low.", "Hiding.", "Out of sight.", "Quiet now.", "Wait for it."]),
    ("Escape", ["Falling back!", "Retreat!", "Get me out of here!", "Running!", "Pull back!"]),
];

/// The key `$Message<set>_<Event>_<n>` as (set, event, line).
fn parse_key(key: &str) -> Option<(u8, &str, usize)> {
    let rest = key.strip_prefix('$')?.strip_prefix("Message")?;
    let mut parts = rest.splitn(3, '_');
    let set = parts.next()?.parse().ok()?;
    let event = parts.next()?;
    let line = parts.next()?.parse().ok()?;
    Some((set, event, line))
}

/// Our own text for a bot line's key (`$Message2_Death_9`): None for a
/// key that isn't one of the nine events'.
pub fn built_in_line(key: &str) -> Option<String> {
    let (_, event, line) = parse_key(key)?;
    let (_, lines) = BUILT_IN.iter().find(|(e, _)| e.eq_ignore_ascii_case(event))?;
    Some(lines[line % lines.len()].to_string())
}

/// What a bot's line says: `lookup` (the game's string table, giving the
/// key unchanged for an id it lacks) of the key, else of the same line in
/// another chat set (the install's table holds only some sets), else
/// `built_in_line`; never the `$` key itself (a key that matches no event
/// shows as plain words).
pub fn shown_line(lookup: impl Fn(&str) -> String, key: &str) -> String {
    let found = |k: &str| Some(lookup(k)).filter(|t| !t.is_empty() && !t.starts_with('$'));
    if let Some(t) = found(key) {
        return t;
    }
    if let Some((_, event, line)) = parse_key(key) {
        for set in 0..3 {
            if let Some(t) = found(&format!("$Message{set}_{event}_{line}")) {
                return t;
            }
        }
    }
    built_in_line(key).unwrap_or_else(|| key.trim_start_matches('$').replace('_', " "))
}

/// The chat cooldown by `BotLevel` (s).
pub fn cooldown(level: u8) -> f64 {
    match level {
        0 => 15.0,
        2 => 25.0,
        _ => 20.0,
    }
}

/// A bot's chat: its set and when it may talk again.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct CaChat {
    pub set: u8,
    pub until: f64,
    /// Dead at the last check (its respawn).
    pub dead: bool,
}

impl CaChat {
    /// An event at `now` (s; `ms` the milliseconds since start): the key
    /// shown, if any (`line` 0..9).
    pub fn event(&mut self, event: Event, now: f64, ms: u64, cooldown: f64, line: u32) -> Option<String> {
        if now + 1e-9 < self.until {
            return None;
        }
        self.until = now + cooldown;
        math::chat_roll(ms, event.percent()).then(|| format!("$Message{}_{}_{}", self.set, event.name(), line.min(9)))
    }
}

/// The events of this tick: kills by bots, bots' deaths and respawns.
pub fn chat(
    mut died: MessageReader<Died>,
    mut bots: Query<(Entity, &mut CaBot, Option<&mut CaChat>, Has<Dead>)>,
    settings: Res<BotSettings>,
    clock: Res<SimClock>,
    mut out: MessageWriter<CaBotChat>,
    mut commands: Commands,
) {
    let now = clock.now;
    let ms = (now * 1000.0) as u64;
    let cool = cooldown(settings.level);
    let mut events: Vec<(Entity, Event)> = Vec::new();
    for d in died.read() {
        if let Some(k) = d.damage.attacker.filter(|k| *k != d.entity) {
            events.push((k, Event::Kill));
        }
        events.push((d.entity, Event::Death));
    }
    for (e, mut bot, chat, dead) in &mut bots {
        let Some(mut chat) = chat else {
            let set = (bot.dice.next_u64() % 3) as u8;
            commands.entity(e).insert(CaChat {
                set,
                until: now + cool,
                dead,
            });
            continue;
        };
        if chat.dead && !dead {
            events.push((e, Event::Respawn));
        }
        chat.dead = dead;
        for (_, ev) in events.iter().filter(|(who, _)| *who == e) {
            let line = bot.dice.int(0, 9);
            if let Some(key) = chat.event(*ev, now, ms, cool, line) {
                out.write(CaBotChat { bot: e, key });
            }
        }
    }
}
