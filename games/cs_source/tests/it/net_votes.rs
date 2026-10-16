//! Player votes over the network (`net::vote`, CS:S's issues in
//! `game::votes`): three clients on a server with no host (a dedicated
//! server's case), one calls a kick vote on another, the votes go to the
//! server and the kick goes through `kickid`'s path; a failed vote can't
//! be called again for `sv_vote_failure_timer`, a caller can't call again
//! for `sv_vote_creation_timer`, bots don't vote, the host can't be voted
//! off.

use std::time::Duration;

use bevy::prelude::*;

use crate::stack::{
    console::{Console, execute, parse},
    core::{LocalPlayer, NetRole},
    games::cs_source::movement::{self as source, SourceMovementPlugin},
    greybox::GreyboxMapPlugin,
    harness::{CsSim, NetSim},
    net::{
        LastDisconnect,
        memory::LinkConditions,
        vote::{CallRefusal, VoteFailure, VoteNews, VoteView, Votes},
    },
    slots::Loadout,
};

fn setup(app: &mut App) {
    app.add_plugins((GreyboxMapPlugin, SourceMovementPlugin))
        .insert_resource(Loadout { movement: source::ID });
}

fn lan() -> LinkConditions {
    LinkConditions {
        latency: Duration::from_millis(30),
        jitter: Duration::from_millis(5),
        loss: 0.0,
    }
}

/// Run a console line in a world now; its output lines.
fn run(world: &mut World, line: &str) -> Vec<String> {
    let before = world.resource::<Console>().output.len();
    for words in parse(line) {
        execute(world, &words, 0);
    }
    world.resource::<Console>().output[before..].iter().map(|l| l.text.clone()).collect()
}

/// Three clients joined and in the game (their maps loaded).
fn three() -> NetSim {
    let mut sim = NetSim::new(CsSim, lan(), 11, 3, setup);
    sim.until_joined(300);
    sim.ticks(40);
    sim
}

fn view(sim: &NetSim, i: usize) -> VoteView {
    sim.clients[i].app.world().resource::<VoteView>().clone()
}

fn voting(sim: &NetSim) -> bool {
    sim.server.app.world().resource::<Votes>().active.is_some()
}

/// Client `i` calls a vote; true once the server runs one.
fn call(sim: &mut NetSim, i: usize, line: &str) -> bool {
    run(sim.clients[i].app.world_mut(), &format!("callvote {line}"));
    sim.until(40, |s| voting(s))
}

/// Client `i`'s refusal, once it comes.
fn refusal(sim: &mut NetSim, i: usize) -> Option<CallRefusal> {
    sim.until(40, |s| view(s, i).refused.is_some());
    sim.clients[i].app.world_mut().resource_mut::<VoteView>().refused.take().map(|r| r.0)
}

#[test]
fn two_yes_of_three_kick_a_player() {
    let mut sim = three();
    let target = sim.client_id(2);
    assert!(call(&mut sim, 0, &format!("kick {target}")), "the vote didn't start");
    // Everyone sees it: the caller's yes counted, the target named.
    assert!(sim.until(40, |s| (0..3).all(|i| view(s, i).active.is_some())));
    for i in 0..3 {
        let v = view(&sim, i).active.unwrap();
        assert_eq!((v.issue.as_str(), v.yes, v.no, v.eligible), ("kick", 1, 0, 3), "client {i}");
        assert!(v.may_vote);
        assert_eq!(v.mine, (i == 0).then_some(1), "client {i}");
        assert!(v.detail.contains("Player") || !v.detail.is_empty());
    }
    assert!(view(&sim, 1).awaiting() && !view(&sim, 0).awaiting());
    run(sim.clients[1].app.world_mut(), "vote option1");
    run(sim.clients[2].app.world_mut(), "vote option2");
    // Everyone voted: it ends early (sv_vote_timer_allow_early_finish), 2
    // of 3 is the quorum (0.6), and the target is dropped.
    assert!(sim.until(60, |s| !voting(s)), "the vote didn't end");
    assert_eq!(sim.server.app.world().resource::<Votes>().passed, 1);
    assert!(sim.until(40, |s| matches!(
        view(s, 0).ended,
        Some((VoteNews::Ended { failure: None, .. }, _))
    )));
    assert!(
        sim.until(200, |s| s.clients[2].app.world().resource::<NetRole>() == &NetRole::Standalone),
        "the target wasn't dropped"
    );
    let why = sim.clients[2].app.world().resource::<LastDisconnect>().0.clone();
    assert!(why.contains("Kicked by vote"), "{why}");
    sim.ticks(20);
    assert!(sim.character_of(2).is_none(), "the target's character stayed");
    assert!(sim.character_of(1).is_some());
}

#[test]
fn a_failed_vote_waits_for_the_failure_timer() {
    let mut sim = three();
    run(
        sim.server.app.world_mut(),
        "sv_vote_failure_timer 3; sv_vote_creation_timer 0",
    );
    let target = sim.client_id(2);
    assert!(call(&mut sim, 0, &format!("kick {target}")));
    sim.ticks(10);
    run(sim.clients[1].app.world_mut(), "vote option2");
    run(sim.clients[2].app.world_mut(), "vote option2");
    assert!(sim.until(60, |s| !voting(s)));
    assert!(sim.until(40, |s| matches!(
        view(s, 1).ended,
        Some((VoteNews::Ended { failure: Some(VoteFailure::YesNo), .. }, _))
    )));
    // The same kick again, by someone else: refused for the timer's rest.
    run(sim.clients[1].app.world_mut(), &format!("callvote kick {target}"));
    match refusal(&mut sim, 1) {
        Some(CallRefusal::Recently(s)) => assert!((1..=3).contains(&s), "{s}"),
        r => panic!("{r:?}"),
    }
    assert!(!voting(&sim));
    // Another issue is fine meanwhile; after the timer the kick is too.
    assert!(call(&mut sim, 1, "restartgame"));
    run(sim.clients[0].app.world_mut(), "vote option2");
    run(sim.clients[2].app.world_mut(), "vote option2");
    assert!(sim.until(60, |s| !voting(s)));
    sim.ticks(3 * 64);
    assert!(call(&mut sim, 1, &format!("kick {target}")), "refused after the timer");
}

#[test]
fn a_caller_waits_for_the_creation_timer() {
    let mut sim = three();
    assert!(call(&mut sim, 0, "restartgame"));
    sim.ticks(10);
    run(sim.clients[1].app.world_mut(), "vote option2");
    run(sim.clients[2].app.world_mut(), "vote option2");
    assert!(sim.until(60, |s| !voting(s)));
    // The caller again, on another issue: sv_vote_creation_timer's 150 s.
    let other = sim.client_id(1);
    run(sim.clients[0].app.world_mut(), &format!("callvote kick {other}"));
    match refusal(&mut sim, 0) {
        Some(CallRefusal::Spam(s)) => assert!((140..=150).contains(&s), "{s}"),
        r => panic!("{r:?}"),
    }
    assert!(!voting(&sim));
    // Someone else may (not the restart that just failed: the failure
    // timer).
    let third = sim.client_id(2);
    assert!(call(&mut sim, 1, &format!("kick {third}")));
    // While it runs, nobody can call another.
    run(sim.clients[2].app.world_mut(), "callvote restartgame");
    assert_eq!(refusal(&mut sim, 2), Some(CallRefusal::InProgress));
}

#[test]
fn bots_dont_vote() {
    let mut sim = three();
    run(sim.server.app.world_mut(), "bot_add 1; bot_add 2");
    sim.ticks(20);
    assert!(call(&mut sim, 0, "restartgame"));
    let vote = sim.server.app.world().resource::<Votes>().active.clone().unwrap();
    assert_eq!(vote.voters.len(), 3, "bots may vote");
    assert_eq!(view_after(&mut sim, 1).eligible, 3);
    run(sim.clients[1].app.world_mut(), "vote option1");
    run(sim.clients[2].app.world_mut(), "vote option1");
    // Every person voted: over, without waiting for the bots.
    assert!(sim.until(60, |s| !voting(s)));
    assert_eq!(sim.server.app.world().resource::<Votes>().passed, 1);
}

/// Client `i`'s vote as shown, once it has one.
fn view_after(sim: &mut NetSim, i: usize) -> crate::stack::net::vote::VoteState {
    assert!(sim.until(40, |s| view(s, i).active.is_some()));
    view(sim, i).active.unwrap()
}

#[test]
fn the_host_cant_be_voted_off() {
    let mut sim = NetSim::new(CsSim, lan(), 12, 2, setup);
    let host = sim.server.spawn_character(Vec3::new(6.0, 1.0, 0.0), source::ID);
    sim.server.app.world_mut().entity_mut(host).insert(LocalPlayer);
    sim.until_joined(300);
    sim.ticks(40);
    run(sim.clients[0].app.world_mut(), "callvote kick 0");
    assert_eq!(refusal(&mut sim, 0), Some(CallRefusal::CannotKickAdmin));
    let me = sim.client_id(0);
    run(sim.clients[0].app.world_mut(), &format!("callvote kick {me}"));
    assert_eq!(refusal(&mut sim, 0), Some(CallRefusal::KickYourself));
    run(sim.clients[0].app.world_mut(), "callvote kick 99");
    assert_eq!(refusal(&mut sim, 0), Some(CallRefusal::InvalidArgument));
    run(sim.clients[0].app.world_mut(), "callvote changelevel de_nowhere");
    assert_eq!(refusal(&mut sim, 0), Some(CallRefusal::MapNotValid));
    // The host votes too: the caller's yes and the host's make it.
    let other = sim.client_id(1);
    assert!(call(&mut sim, 0, &format!("kick {other}")));
    assert_eq!(sim.server.app.world().resource::<Votes>().active.as_ref().unwrap().voters.len(), 3);
    assert!(sim.until(20, |s| s.server.app.world().resource::<VoteView>().awaiting()));
    run(sim.server.app.world_mut(), "vote option1");
    run(sim.clients[1].app.world_mut(), "vote option2");
    assert!(sim.until(60, |s| !voting(s)));
    assert!(sim.until(200, |s| s.clients[1].app.world().resource::<NetRole>() == &NetRole::Standalone));
}
