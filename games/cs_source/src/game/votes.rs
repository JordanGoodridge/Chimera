//! CS:S's vote issues (`callvote`, the engine's `net::vote`): `kick
//! <userid>` (a person, by the id `users` and `status` list; never the
//! caller or a listen server's host, "You may not vote to kick the server
//! admin."), `changelevel <map>` (a map of the server's map cycle,
//! `core::MapCycle`: "That map was not found in the server's map cycle.")
//! and `restartgame` (`mp_restartgame 1`). A kick goes the way `kickid`
//! does (`net::server::kick_userid`: told why, then dropped).
//!
//! With `sv_vote_bots_allowed 1` bots vote too (`bot_ballots`, on the
//! server): each a moment after it is shown (1 to 4 s), mostly yes on a
//! restart or a change of level, mostly no on kicking a person
//! (`bot_choice`), rolled with the bot's own seeded dice (`bot::Bot::roll`).
//!
//! Known shortcuts (docs/tech-debt.md): Votes (CS:S's issues): which issues
//! CS:S offers is unmeasured (its files name none: no issue strings in its
//! localization, the GameUI's failure words mention kicks, maps, the next
//! level and team scrambles): kick, change level and restart game are ours,
//! `nextlevel`, `scrambleteams` and `swapteams` aren't offered; the issue names
//! are TF2's spelling, in any case; bots can't be voted off (they have no user
//! id); the kick's reason ("Kicked by vote.") is ours; how bots vote is ours
//! (`bot_choice`: CS:S's bots' votes are unmeasured). Why it's OK for now:
//! a capture of CS:S's vote setup on its dedicated server
//! (docs/plans/active/windows-captures.md).

use bevy::prelude::*;

use crate::{
    console::Console,
    core::{Bot, LocalPlayer, MapCycle, NetRole},
    net::{
        HOST_ID, NetCharacter,
        vote::{self, CallRefusal, VoteAppExt, VoteIssue, Votes},
    },
};

/// What a player kicked by a vote is told (ours).
pub const KICK_REASON: &str = "Kicked by vote.";

/// CS:S's issues (`net::vote::VoteIssues`).
pub fn register(app: &mut App) {
    app.vote_issue(VoteIssue {
        name: "kick",
        check: check_kick,
        run: |w, id| {
            if let Ok(id) = id.parse::<u64>() {
                crate::net::server::kick_userid(w, id, KICK_REASON);
            }
        },
        choices: |w, caller| {
            let me = w.get::<NetCharacter>(caller).and_then(|c| c.owner);
            people(w)
                .into_iter()
                .filter(|(id, _)| Some(*id) != me && !(*id == HOST_ID && hosted(w)))
                .map(|(id, name)| (id.to_string(), name))
                .collect()
        },
    })
    .vote_issue(VoteIssue {
        name: "changelevel",
        check: |w, _, map| {
            if map.is_empty() {
                return Err(CallRefusal::MapNameRequired);
            }
            let cycle = w.get_resource::<MapCycle>().map(|c| c.maps.clone()).unwrap_or_default();
            cycle
                .iter()
                .find(|m| m.eq_ignore_ascii_case(map))
                .cloned()
                .ok_or(CallRefusal::MapNotValid)
        },
        run: |w, map| {
            w.resource_mut::<Console>().submit(format!("changelevel {}", map.to_lowercase()));
        },
        choices: |w, _| {
            let cycle = w.get_resource::<MapCycle>().map(|c| c.maps.clone()).unwrap_or_default();
            cycle.into_iter().map(|m| (m.clone(), m)).collect()
        },
    })
    .vote_issue(VoteIssue {
        name: "restartgame",
        check: |_, _, _| Ok(String::new()),
        run: |w, _| w.resource_mut::<Console>().submit("mp_restartgame 1"),
        choices: |_, _| Vec::new(),
    });
    app.add_systems(Update, bot_ballots.run_if(resource_equals(NetRole::Server)));
}

/// How long a bot takes to vote: 1 s and up to `BOT_DELAY` more (ours).
pub const BOT_DELAY: f32 = 3.0;

/// How a bot votes on `issue` given a roll of its dice (0 to 1), ours:
/// yes on a restart or a change of level 7 times in 10, on kicking a
/// person 1 in 10, on anything else half the time. 1 yes, 2 no.
pub fn bot_choice(issue: &str, roll: f32) -> u8 {
    let yes = match issue.to_lowercase().as_str() {
        "restartgame" | "changelevel" => 0.7,
        "kick" => 0.1,
        _ => 0.5,
    };
    if roll < yes { 1 } else { 2 }
}

/// Bots who may vote (`sv_vote_bots_allowed 1`) vote a moment after the
/// vote is shown (`bot_choice`, the delay and choice from their dice).
fn bot_ballots(world: &mut World, mut planned: Local<(u32, Vec<(Entity, f64, u8)>)>) {
    let Some(active) = world.resource::<Votes>().active.clone() else {
        planned.1.clear();
        return;
    };
    if planned.0 != active.id {
        *planned = (active.id, Vec::new());
    }
    let now = world.resource::<Time<Real>>().elapsed_secs_f64();
    for &voter in &active.voters {
        if active.ballots.contains_key(&voter) || planned.1.iter().any(|(e, ..)| *e == voter) {
            continue;
        }
        let Some(mut bot) = world.get_mut::<crate::bot::Bot>(voter) else { continue };
        let delay = 1.0 + BOT_DELAY * bot.roll();
        let choice = bot_choice(&active.issue, bot.roll());
        planned.1.push((voter, now + delay as f64, choice));
    }
    let due: Vec<(Entity, u8)> = planned
        .1
        .iter()
        .filter(|(_, at, _)| *at <= now)
        .map(|(e, _, c)| (*e, *c))
        .collect();
    planned.1.retain(|(_, at, _)| *at > now);
    for (bot, choice) in due {
        vote::cast_for(world, bot, choice);
    }
}

/// A listen server: the host plays here.
fn hosted(world: &mut World) -> bool {
    world.query_filtered::<(), With<LocalPlayer>>().iter(world).next().is_some()
}

/// The people in the game (not bots): user id and name.
fn people(world: &mut World) -> Vec<(u64, String)> {
    let mut out: Vec<(u64, String)> = world
        .query_filtered::<&NetCharacter, Without<Bot>>()
        .iter(world)
        .filter_map(|c| Some((c.owner?, c.name.clone())))
        .collect();
    out.sort();
    out
}

/// `kick <userid>`: a person there, not the caller, not the host.
fn check_kick(world: &mut World, caller: Entity, id: &str) -> Result<String, CallRefusal> {
    let id: u64 = id.parse().map_err(|_| CallRefusal::InvalidArgument)?;
    if world.get::<NetCharacter>(caller).and_then(|c| c.owner) == Some(id) {
        return Err(CallRefusal::KickYourself);
    }
    if id == HOST_ID && hosted(world) {
        return Err(CallRefusal::CannotKickAdmin);
    }
    people(world)
        .into_iter()
        .find(|(i, _)| *i == id)
        .map(|(_, name)| name)
        .ok_or(CallRefusal::InvalidArgument)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bots_mostly_agree_to_restarts_and_refuse_kicks() {
        assert_eq!(bot_choice("restartgame", 0.2), 1);
        assert_eq!(bot_choice("ChangeLevel", 0.69), 1);
        assert_eq!(bot_choice("changelevel", 0.75), 2);
        assert_eq!(bot_choice("kick", 0.05), 1);
        assert_eq!(bot_choice("kick", 0.5), 2);
        assert_eq!(bot_choice("nextlevel", 0.4), 1);
    }
}
