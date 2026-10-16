//! CS:S's vote issues (`callvote`, the engine's `net::vote`): `kick
//! <userid>` (a person, by the id `users` and `status` list; never the
//! caller or a listen server's host, "You may not vote to kick the server
//! admin."), `changelevel <map>` (a map of the server's map cycle,
//! `core::MapCycle`: "That map was not found in the server's map cycle.")
//! and `restartgame` (`mp_restartgame 1`). A kick goes the way `kickid`
//! does (`net::server::kick_userid`: told why, then dropped).
//!
//! Known shortcuts (docs/tech-debt.md): Votes (CS:S's issues): which issues
//! CS:S offers is unmeasured (its files name none: no issue strings in its
//! localization, the GameUI's failure words mention kicks, maps, the next
//! level and team scrambles): kick, change level and restart game are ours,
//! `nextlevel`, `scrambleteams` and `swapteams` aren't offered; the issue names
//! are TF2's spelling, in any case; bots can't be voted off (they have no user
//! id); the kick's reason ("Kicked by vote.") is ours. Why it's OK for now:
//! a capture of CS:S's vote setup on its dedicated server
//! (docs/plans/active/windows-captures.md).

use bevy::prelude::*;

use crate::{
    console::Console,
    core::{Bot, LocalPlayer, MapCycle},
    net::{
        HOST_ID, NetCharacter,
        vote::{CallRefusal, VoteAppExt, VoteIssue},
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
