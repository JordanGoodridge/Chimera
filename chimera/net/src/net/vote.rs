//! Player votes (Source's `callvote`): a player calls a vote on an issue a
//! game registers (`VoteIssue`, `VoteAppExt::vote_issue`: CS:S's kick,
//! change level, restart game in `cs_source::game::votes`), the players
//! who may vote answer with `vote option1` (yes) or `vote option2` (no)
//! (F1 and F2 while the vote waits for theirs: `client::console`), and
//! the server counts them, ends the vote and carries the issue out when it
//! passed. The server decides everything; a client shows it
//! (`VoteView`, which a game's front end draws: CS:S's `ui::vote_hud`).
//!
//! - **Calling** (`callvote <issue> [detail]`, `CallVote`): refused
//!   (`CallRefusal`, told to the caller only as `VoteNews::Refused`) with
//!   `sv_allow_votes 0`, while another vote runs, by a spectator
//!   (`sv_vote_allow_spectators 0`), by a player still loading, within
//!   `sv_vote_creation_timer` of the caller's last call, for an issue
//!   that failed within `sv_vote_failure_timer` (the same issue and
//!   detail), for an issue nobody registered, and by the issue's own check
//!   (`VoteIssue::check`: a player not there, the host, a map not in the
//!   cycle). `callvote` alone asks the server for its issues and their
//!   choices (`VoteNews::Setup`), printed to the console and counted in
//!   `VoteView::setups` (a front end opens its setup dialog on each:
//!   CS:S's `ui::game_menu`).
//! - **Who votes**: the people in the game when the vote starts (not
//!   bots unless `sv_vote_bots_allowed`, not spectators unless
//!   `sv_vote_allow_spectators`, not players still loading), and anyone
//!   who becomes able to while it runs (a player who joins or rejoins,
//!   once loaded; a spectator who picks a team; `join_running_vote`); one
//!   who leaves is taken off (and their vote with them). The caller's vote
//!   is yes from the start unless `sv_vote_holder_may_vote_no` (then they
//!   vote as anyone). A game's bots vote through `cast_for` (CS:S's rule:
//!   `cs_source::game::votes`).
//! - **Ending**: after `sv_vote_timer_duration`, or once everyone has
//!   voted (`sv_vote_timer_allow_early_finish`). It passes when the yes
//!   votes are at least `sv_vote_quorum_ratio` of those who may vote
//!   (rounded up, at least one) and more than the no votes (`result`);
//!   else it fails, "not enough players voted" when fewer than that voted
//!   at all, else "not enough voted yes".
//! - **News** (`VoteNews`, server -> clients): each voter gets the vote as
//!   it stands for them (`Shown`: the issue, the caller, the counts, time
//!   left, their own vote) when it starts, on every vote and on each
//!   change of who may vote (a player joining while it runs gets it once
//!   loaded); everyone gets
//!   how it ended (`Ended`). A listen server's host gets its copies through
//!   replicon's local resend, as chat's.
//!
//! The cvars and their defaults are CS:S's (measured on its dedicated
//! server, 2026-10-15): `sv_allow_votes` 1, `sv_vote_allow_spectators` 0,
//! `sv_vote_bots_allowed` 0, `sv_vote_creation_timer` 150,
//! `sv_vote_failure_timer` 300, `sv_vote_holder_may_vote_no` 0,
//! `sv_vote_quorum_ratio` 0.6 (0.1 to 1), `sv_vote_timer_allow_early_finish`
//! 1, `sv_vote_timer_duration` 15, `sv_vote_ui_hide_disabled_issues` 1.
//!
//! Known shortcuts (docs/tech-debt.md): Votes (`net::vote`; the cvars measured,
//! the rules not): the pass rule (yes at least the quorum's share of those who
//! may vote, rounded up, and more yes than no), which failure reason shows, an
//! early finish only once everyone has voted, the creation timer counted from
//! the call (not the vote's end), the failure timer keyed by issue and detail,
//! who may vote (those able when it starts, plus anyone who becomes able while
//! it runs, joiners and rejoiners included, a rejoiner's earlier vote lost with
//! their old character; one who becomes unable, e.g. by spectating, stays; the
//! kick's target may vote), players still loading neither voting nor calling,
//! and how bots vote (the game's rule) are ours; every `sv_vote_*` cvar is
//! replicated (our prefix rule; CS:S replicates `sv_vote_holder_may_vote_no`
//! only); `sv_vote_ui_hide_disabled_issues` is kept but nothing reads it (no
//! disabled issues); only two options (yes, no: `vote option3`-`option5` are
//! refused). Why it's OK for now: votes need CS:S's rules measured on its
//! dedicated server with players (docs/plans/active/windows-captures.md).

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use super::{NetCharacter, server::Player};
use crate::{
    console::{Console, ConsoleAppExt, Level, resource_cvar},
    core::{Bot, Connecting, LocalPlayer, NetRole, Spectating},
};

/// The vote rules (CS:S's cvars and defaults, measured).
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct VoteSettings {
    /// `sv_allow_votes`.
    pub allow: u8,
    /// `sv_vote_allow_spectators`.
    pub allow_spectators: u8,
    /// `sv_vote_bots_allowed`.
    pub bots_allowed: u8,
    /// `sv_vote_creation_timer`: seconds before a player may call another.
    pub creation_timer: f32,
    /// `sv_vote_failure_timer`: seconds before a failed vote may be called
    /// again.
    pub failure_timer: f32,
    /// `sv_vote_holder_may_vote_no`: the caller isn't made to vote yes.
    pub holder_may_vote_no: u8,
    /// `sv_vote_quorum_ratio` (0.1 to 1).
    pub quorum_ratio: f32,
    /// `sv_vote_timer_allow_early_finish`.
    pub early_finish: u8,
    /// `sv_vote_timer_duration`: seconds a vote runs.
    pub duration: f32,
    /// `sv_vote_ui_hide_disabled_issues`.
    pub hide_disabled: u8,
}

impl Default for VoteSettings {
    fn default() -> Self {
        Self {
            allow: 1,
            allow_spectators: 0,
            bots_allowed: 0,
            creation_timer: 150.0,
            failure_timer: 300.0,
            holder_may_vote_no: 0,
            quorum_ratio: 0.6,
            early_finish: 1,
            duration: 15.0,
            hide_disabled: 1,
        }
    }
}

/// Smallest and largest `sv_vote_quorum_ratio` (CS:S's range).
pub const QUORUM_RANGE: (f32, f32) = (0.1, 1.0);

/// Yes votes a vote needs: the quorum's share of those who may vote,
/// rounded up, at least one.
pub fn needed(eligible: u32, ratio: f32) -> u32 {
    let ratio = ratio.clamp(QUORUM_RANGE.0, QUORUM_RANGE.1);
    // A hair under, so 0.6 of 5 is 3 and not 4 after float rounding.
    ((eligible as f32 * ratio) - 1e-4).ceil().max(1.0) as u32
}

/// How a vote with these counts ends: passed, or why not.
pub fn result(yes: u32, no: u32, eligible: u32, ratio: f32) -> Result<(), VoteFailure> {
    let need = needed(eligible, ratio);
    if yes >= need && yes > no {
        Ok(())
    } else if yes + no < need {
        Err(VoteFailure::Quorum)
    } else {
        Err(VoteFailure::YesNo)
    }
}

/// Why a vote failed (the game's "Not enough players voted." and "Not
/// enough players voted \"Yes.\"").
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoteFailure {
    Quorum,
    YesNo,
}

/// Why a call was refused (Source's GameUI words for each: `#GameUI_vote_failed_*`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum CallRefusal {
    /// `sv_allow_votes 0`.
    Disabled,
    /// No such issue, or the server turned it off.
    IssueDisabled,
    InProgress,
    /// The caller called one less than `sv_vote_creation_timer` ago:
    /// seconds left.
    Spam(u32),
    /// This issue failed less than `sv_vote_failure_timer` ago: seconds
    /// left.
    Recently(u32),
    Spectator,
    /// The caller is still loading the map.
    Loading,
    /// A kick of the listen server's host.
    CannotKickAdmin,
    /// A kick of the caller.
    KickYourself,
    MapNameRequired,
    MapNotFound,
    /// A map the server's map cycle doesn't have.
    MapNotValid,
    /// A detail the issue can't take (no such player).
    InvalidArgument,
    /// Not in a network game, or no player to call it for.
    NoGame,
}

/// What an issue takes and does, the game's. `name` is `callvote`'s
/// word for it (any case).
#[derive(Clone, Copy)]
pub struct VoteIssue {
    pub name: &'static str,
    /// Whether `detail` works for `caller` (a character): what to show
    /// for it (a player's name, a map), or why not.
    pub check: fn(&mut World, Entity, &str) -> Result<String, CallRefusal>,
    /// Carry it out (the vote passed), with the detail it was called with.
    pub run: fn(&mut World, &str),
    /// The details `caller` may pick from (value, what to show): players,
    /// maps; empty when it takes none.
    pub choices: fn(&mut World, Entity) -> Vec<(String, String)>,
}

/// The issues games registered (`VoteAppExt::vote_issue`), in order.
#[derive(Resource, Default, Clone)]
pub struct VoteIssues(pub Vec<VoteIssue>);

impl VoteIssues {
    pub fn get(&self, name: &str) -> Option<VoteIssue> {
        self.0.iter().find(|i| i.name.eq_ignore_ascii_case(name)).copied()
    }
}

/// Registering a vote issue.
pub trait VoteAppExt {
    /// Add `issue` (one by the same name is replaced).
    fn vote_issue(&mut self, issue: VoteIssue) -> &mut Self;
}

impl VoteAppExt for App {
    fn vote_issue(&mut self, issue: VoteIssue) -> &mut Self {
        let mut issues = self.world_mut().get_resource_or_init::<VoteIssues>();
        issues.0.retain(|i| !i.name.eq_ignore_ascii_case(issue.name));
        issues.0.push(issue);
        self
    }
}

/// Client -> server: call a vote (`callvote <issue> [detail]`); an empty
/// issue asks for the issues (`VoteNews::Setup`).
#[derive(Message, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CallVote {
    pub issue: String,
    pub detail: String,
}

/// Client -> server: my vote (`vote option1`: 1, yes; `option2`: 2, no).
#[derive(Message, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CastVote {
    pub option: u8,
}

/// A vote as one player sees it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct VoteState {
    pub id: u32,
    pub issue: String,
    /// What the detail shows as (a player's name, a map).
    pub detail: String,
    pub caller: String,
    pub yes: u32,
    pub no: u32,
    /// Players who may vote.
    pub eligible: u32,
    pub seconds_left: f32,
    /// This player's vote (1 yes, 2 no), and whether they may vote.
    pub mine: Option<u8>,
    pub may_vote: bool,
}

/// An issue and its choices, for a vote setup dialog.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct IssueInfo {
    pub name: String,
    pub choices: Vec<(String, String)>,
}

/// Server -> clients: what happens with votes.
#[derive(Message, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum VoteNews {
    /// The vote as it stands for this player (its start, each vote).
    Shown(VoteState),
    /// It ended: passed, or why not.
    Ended {
        id: u32,
        issue: String,
        detail: String,
        failure: Option<VoteFailure>,
    },
    /// This player's call was refused.
    Refused(CallRefusal),
    /// The issues this player may call.
    Setup(Vec<IssueInfo>),
}

/// The vote running on the server.
#[derive(Clone, Debug)]
pub struct ActiveVote {
    pub id: u32,
    pub issue: String,
    pub value: String,
    pub detail: String,
    pub caller: Entity,
    pub caller_name: String,
    pub ends: f64,
    /// Those who may vote (characters), and their votes.
    pub voters: Vec<Entity>,
    pub ballots: HashMap<Entity, u8>,
    /// The characters told about it so far (a player joining while it
    /// runs is told once loaded).
    pub told: Vec<Entity>,
}

impl ActiveVote {
    pub fn count(&self, option: u8) -> u32 {
        self.ballots.values().filter(|b| **b == option).count() as u32
    }
}

/// The server's votes: the one running, when each caller (by user id)
/// last called, when each issue (and detail) last failed.
#[derive(Resource, Default, Debug)]
pub struct Votes {
    pub active: Option<ActiveVote>,
    next_id: u32,
    called: HashMap<u64, f64>,
    failed: HashMap<(String, String), f64>,
    /// Votes that passed and failed so far.
    pub passed: u32,
    pub failures: u32,
}

/// A client's view of votes (a listen server's host's too), from
/// `VoteNews`: the vote running, how the last one ended and when (real
/// seconds), the last refusal of our call, the issues we may call.
#[derive(Resource, Default, Clone, Debug, PartialEq)]
pub struct VoteView {
    pub active: Option<VoteState>,
    /// When the shown vote ends (real seconds, this process's clock).
    pub ends: f64,
    pub ended: Option<(VoteNews, f64)>,
    pub refused: Option<(CallRefusal, f64)>,
    pub setup: Vec<IssueInfo>,
    /// How many times the server sent them (`callvote` alone): a front
    /// end opens its vote setup dialog on each (Source's `VoteSetup`).
    pub setups: u32,
}

impl VoteView {
    /// A vote waits for our vote (F1 and F2 vote).
    pub fn awaiting(&self) -> bool {
        self.active.as_ref().is_some_and(|v| v.may_vote && v.mine.is_none())
    }
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<VoteSettings>()
        .init_resource::<Votes>()
        .init_resource::<VoteView>()
        .init_resource::<VoteIssues>()
        .add_systems(
            PreUpdate,
            (receive_calls, receive_ballots)
                .after(ServerSystems::Receive)
                .run_if(in_state(ServerState::Running)),
        )
        .add_systems(PostUpdate, run_votes.before(ServerSystems::Send))
        .add_systems(Update, show_news);
    resource_cvar::<VoteSettings, u8>(app, "sv_allow_votes", "Allow voting (callvote).", |s| &mut s.allow);
    resource_cvar::<VoteSettings, u8>(
        app,
        "sv_vote_allow_spectators",
        "Allow spectators to vote and call votes.",
        |s| &mut s.allow_spectators,
    );
    resource_cvar::<VoteSettings, u8>(app, "sv_vote_bots_allowed", "Allow bots to vote or not.", |s| {
        &mut s.bots_allowed
    });
    resource_cvar::<VoteSettings, f32>(
        app,
        "sv_vote_creation_timer",
        "How long before a player can attempt to call another vote (in seconds).",
        |s| &mut s.creation_timer,
    );
    resource_cvar::<VoteSettings, f32>(
        app,
        "sv_vote_failure_timer",
        "A vote that fails cannot be re-submitted for this long.",
        |s| &mut s.failure_timer,
    );
    resource_cvar::<VoteSettings, u8>(
        app,
        "sv_vote_holder_may_vote_no",
        "1 = Vote caller is not forced to vote yes on yes/no votes.",
        |s| &mut s.holder_may_vote_no,
    );
    resource_cvar::<VoteSettings, f32>(
        app,
        "sv_vote_quorum_ratio",
        "The minimum ratio of eligible players needed to pass a vote.",
        |s| &mut s.quorum_ratio,
    );
    resource_cvar::<VoteSettings, u8>(
        app,
        "sv_vote_timer_allow_early_finish",
        "End a vote early once everyone who may vote has.",
        |s| &mut s.early_finish,
    );
    resource_cvar::<VoteSettings, f32>(
        app,
        "sv_vote_timer_duration",
        "How long to allow voting on an issue.",
        |s| &mut s.duration,
    );
    resource_cvar::<VoteSettings, u8>(
        app,
        "sv_vote_ui_hide_disabled_issues",
        "Hide disabled issues from the vote setup.",
        |s| &mut s.hide_disabled,
    );
    app.world_mut()
        .resource_mut::<Console>()
        .set_range("sv_vote_quorum_ratio", QUORUM_RANGE.0, QUORUM_RANGE.1);
    app.console_command(
        "callvote",
        "callvote <issue> [detail]: start a vote on an issue (kick <userid>, changelevel <map>, restartgame); \
         alone: list the issues.",
        |w, a| {
            if !in_network_game(w) {
                return Err("Votes need a network game.".into());
            }
            let issue = a.first().cloned().unwrap_or_default();
            let detail = a.get(1..).map(|d| d.join(" ")).unwrap_or_default();
            w.write_message(CallVote { issue, detail });
            Ok(None)
        },
    )
    .console_command("vote", "vote <option1|option2>: vote yes (option1, F1) or no (option2, F2).", |w, a| {
        let option = a
            .first()
            .and_then(|o| o.to_lowercase().strip_prefix("option")?.parse::<u8>().ok())
            .filter(|n| (1..=5).contains(n))
            .ok_or("vote <option1|option2>")?;
        if !in_network_game(w) {
            return Err("No vote is running.".into());
        }
        w.write_message(CastVote { option });
        Ok(None)
    });
}

fn in_network_game(world: &World) -> bool {
    world
        .get_resource::<NetRole>()
        .is_some_and(|r| *r != NetRole::Standalone)
}

fn now(world: &World) -> f64 {
    world.resource::<Time<Real>>().elapsed_secs_f64()
}

/// The character a request came from (a client's, or the host's own) and
/// its user id.
fn requester(world: &mut World, client: ClientId) -> Option<(Entity, u64)> {
    let character = match client.entity() {
        Some(c) => world.get::<Player>(c).map(|p| p.character)?,
        None => world
            .query_filtered::<Entity, With<LocalPlayer>>()
            .iter(world)
            .next()?,
    };
    let id = world.get::<NetCharacter>(character)?.owner?;
    Some((character, id))
}

/// Whether `e` may vote now (`VoteSettings`): a person, or a bot with
/// `sv_vote_bots_allowed`; not a spectator unless allowed; not loading.
fn may_vote(world: &World, e: Entity, s: &VoteSettings) -> bool {
    let Some(c) = world.get::<NetCharacter>(e) else { return false };
    let person = c.owner.is_some() && world.get::<Bot>(e).is_none();
    (person || s.bots_allowed != 0)
        && (s.allow_spectators != 0 || world.get::<Spectating>(e).is_none())
        && world.get::<Connecting>(e).is_none()
}

/// Where each person's news goes: their client (the host's: the server's
/// local resend), by character.
fn recipients(world: &mut World) -> Vec<(Entity, ClientId)> {
    let mut out: Vec<(Entity, ClientId)> = world
        .query::<(Entity, &Player)>()
        .iter(world)
        .map(|(c, p)| (p.character, ClientId::Client(c)))
        .collect();
    if let Some(host) = world
        .query_filtered::<Entity, (With<LocalPlayer>, With<NetCharacter>)>()
        .iter(world)
        .next()
    {
        out.push((host, ClientId::Server));
    }
    out
}

fn tell(world: &mut World, client: ClientId, news: VoteNews) {
    world.write_message(ToClients {
        targets: SendTargets::Single(client),
        message: news,
    });
}

fn receive_calls(mut requests: MessageReader<FromClient<CallVote>>, mut commands: Commands) {
    for r in requests.read() {
        let (client, call) = (r.client_id, r.message.clone());
        if call.issue.len() > 32 || call.detail.len() > 128 {
            continue;
        }
        commands.queue(move |w: &mut World| {
            if call.issue.is_empty() {
                setup(w, client);
                return;
            }
            if let Err(refusal) = call_vote(w, client, &call.issue, &call.detail) {
                info!("callvote {} {}: refused ({refusal:?})", call.issue, call.detail);
                tell(w, client, VoteNews::Refused(refusal));
            }
        });
    }
}

/// The issues `client` may call, with their choices.
fn setup(world: &mut World, client: ClientId) {
    let Some((caller, _)) = requester(world, client) else { return };
    let issues = world.resource::<VoteIssues>().0.clone();
    let list = issues
        .iter()
        .map(|i| IssueInfo {
            name: i.name.to_string(),
            choices: (i.choices)(world, caller),
        })
        .collect();
    tell(world, client, VoteNews::Setup(list));
}

/// Start the vote `client` calls, or why not.
pub fn call_vote(world: &mut World, client: ClientId, issue: &str, detail: &str) -> Result<(), CallRefusal> {
    let settings = world.resource::<VoteSettings>().clone();
    if settings.allow == 0 {
        return Err(CallRefusal::Disabled);
    }
    let (caller, id) = requester(world, client).ok_or(CallRefusal::NoGame)?;
    if world.resource::<Votes>().active.is_some() {
        return Err(CallRefusal::InProgress);
    }
    if world.get::<Connecting>(caller).is_some() {
        return Err(CallRefusal::Loading);
    }
    if settings.allow_spectators == 0 && world.get::<Spectating>(caller).is_some() {
        return Err(CallRefusal::Spectator);
    }
    let now = now(world);
    let left = |since: Option<&f64>, timer: f32| {
        since
            .map(|t| timer as f64 - (now - t))
            .filter(|l| *l > 0.0)
            .map(|l| l.ceil() as u32)
    };
    if let Some(l) = left(world.resource::<Votes>().called.get(&id), settings.creation_timer) {
        return Err(CallRefusal::Spam(l));
    }
    let the_issue = world
        .resource::<VoteIssues>()
        .get(issue)
        .ok_or(CallRefusal::IssueDisabled)?;
    let value = detail.split_whitespace().next().unwrap_or_default().to_string();
    let key = (the_issue.name.to_lowercase(), value.to_lowercase());
    if let Some(l) = left(world.resource::<Votes>().failed.get(&key), settings.failure_timer) {
        return Err(CallRefusal::Recently(l));
    }
    let shown = (the_issue.check)(world, caller, &value)?;
    let voters: Vec<Entity> = {
        let mut q = world.query_filtered::<Entity, With<NetCharacter>>();
        let all: Vec<Entity> = q.iter(world).collect();
        all.into_iter().filter(|e| may_vote(world, *e, &settings)).collect()
    };
    let caller_name = world
        .get::<NetCharacter>(caller)
        .map(|c| c.name.clone())
        .unwrap_or_default();
    let mut votes = world.resource_mut::<Votes>();
    votes.called.insert(id, now);
    votes.next_id += 1;
    let mut vote = ActiveVote {
        id: votes.next_id,
        issue: the_issue.name.to_string(),
        value,
        detail: shown,
        caller,
        caller_name,
        ends: now + settings.duration.max(0.0) as f64,
        voters,
        ballots: HashMap::new(),
        told: Vec::new(),
    };
    if settings.holder_may_vote_no == 0 && vote.voters.contains(&caller) {
        vote.ballots.insert(caller, 1);
    }
    info!(
        "{} called a vote: {} {} ({} may vote)",
        vote.caller_name,
        vote.issue,
        vote.detail,
        vote.voters.len()
    );
    votes.active = Some(vote);
    send_state(world);
    Ok(())
}

fn receive_ballots(mut requests: MessageReader<FromClient<CastVote>>, mut commands: Commands) {
    for r in requests.read() {
        let (client, option) = (r.client_id, r.message.option);
        commands.queue(move |w: &mut World| {
            cast(w, client, option);
        });
    }
}

/// `client`'s vote: counted once, from a player who may vote, yes or no.
pub fn cast(world: &mut World, client: ClientId, option: u8) -> bool {
    let Some((voter, _)) = requester(world, client) else { return false };
    cast_for(world, voter, option)
}

/// A vote for `voter` (a character: a player's, or a bot's from the
/// game's bot rule), counted as `cast` counts it.
pub fn cast_for(world: &mut World, voter: Entity, option: u8) -> bool {
    if !(1..=2).contains(&option) {
        return false;
    }
    let mut votes = world.resource_mut::<Votes>();
    let Some(vote) = votes.active.as_mut() else { return false };
    if !vote.voters.contains(&voter) || vote.ballots.contains_key(&voter) {
        return false;
    }
    vote.ballots.insert(voter, option);
    send_state(world);
    true
}

/// Each person the running vote as it stands for them.
fn send_state(world: &mut World) {
    let Some(vote) = world.resource::<Votes>().active.clone() else { return };
    let left = (vote.ends - now(world)).max(0.0) as f32;
    let mut told = Vec::new();
    for (character, client) in recipients(world) {
        // Told once loaded (`join_running_vote`).
        if world.get::<Connecting>(character).is_some() {
            continue;
        }
        told.push(character);
        let state = VoteState {
            id: vote.id,
            issue: vote.issue.clone(),
            detail: vote.detail.clone(),
            caller: vote.caller_name.clone(),
            yes: vote.count(1),
            no: vote.count(2),
            eligible: vote.voters.len() as u32,
            seconds_left: left,
            mine: vote.ballots.get(&character).copied(),
            may_vote: vote.voters.contains(&character),
        };
        tell(world, client, VoteNews::Shown(state));
    }
    if let Some(v) = world.resource_mut::<Votes>().active.as_mut() {
        v.told = told;
    }
}

/// Those who may vote now and couldn't when it started (a player who
/// joined, or rejoined, and has loaded; a spectator who picked a team; a
/// bot added with `sv_vote_bots_allowed`) may vote too; players not told
/// about it yet are. Whether anything changed.
fn join_running_vote(world: &mut World, vote: &mut ActiveVote, settings: &VoteSettings) -> bool {
    let mut q = world.query_filtered::<Entity, With<NetCharacter>>();
    let all: Vec<Entity> = q.iter(world).collect();
    let mut changed = false;
    for e in all {
        if !vote.voters.contains(&e) && may_vote(world, e, settings) {
            vote.voters.push(e);
            changed = true;
        }
    }
    let untold = recipients(world)
        .into_iter()
        .any(|(c, _)| !vote.told.contains(&c) && world.get::<Connecting>(c).is_none());
    changed || untold
}

/// The running vote: voters who left taken off, ended when its time is up
/// or everyone voted, and carried out when it passed. The server's (none
/// runs elsewhere); a stopped server forgets its votes.
fn run_votes(world: &mut World) {
    if world.get_resource::<NetRole>() != Some(&NetRole::Server) {
        let votes = world.resource::<Votes>();
        if votes.active.is_some() || !votes.called.is_empty() || !votes.failed.is_empty() {
            *world.resource_mut::<Votes>() = Votes::default();
        }
        return;
    }
    let Some(mut vote) = world.resource::<Votes>().active.clone() else { return };
    let before = vote.voters.len();
    vote.voters.retain(|e| world.get_entity(*e).is_ok());
    vote.ballots.retain(|e, _| world.get_entity(*e).is_ok());
    let settings = world.resource::<VoteSettings>().clone();
    let joined = join_running_vote(world, &mut vote, &settings);
    let everyone = vote.ballots.len() >= vote.voters.len();
    let over = now(world) >= vote.ends || (settings.early_finish != 0 && everyone);
    if !over {
        if vote.voters.len() != before || joined {
            world.resource_mut::<Votes>().active = Some(vote);
            send_state(world);
        }
        return;
    }
    let (yes, no) = (vote.count(1), vote.count(2));
    let outcome = result(yes, no, vote.voters.len() as u32, settings.quorum_ratio);
    info!(
        "vote {} {}: {yes} yes, {no} no of {}: {}",
        vote.issue,
        vote.detail,
        vote.voters.len(),
        if outcome.is_ok() { "passed" } else { "failed" }
    );
    let at = now(world);
    {
        let mut votes = world.resource_mut::<Votes>();
        votes.active = None;
        match outcome {
            Ok(()) => votes.passed += 1,
            Err(_) => {
                votes.failures += 1;
                votes
                    .failed
                    .insert((vote.issue.to_lowercase(), vote.value.to_lowercase()), at);
            }
        }
    }
    world.write_message(ToClients {
        targets: SendTargets::All,
        message: VoteNews::Ended {
            id: vote.id,
            issue: vote.issue.clone(),
            detail: vote.detail.clone(),
            failure: outcome.err(),
        },
    });
    if outcome.is_ok()
        && let Some(issue) = world.resource::<VoteIssues>().get(&vote.issue)
    {
        (issue.run)(world, &vote.value);
    }
}

/// Our view of votes from the server's news (any role: a listen server's
/// host gets its copies too), with a line in the console for each; out of
/// a network game, nothing.
fn show_news(world: &mut World, mut cursor: Local<bevy::ecs::message::MessageCursor<VoteNews>>) {
    let news: Vec<VoteNews> = world
        .get_resource::<Messages<VoteNews>>()
        .map(|m| cursor.read(m).cloned().collect())
        .unwrap_or_default();
    if !in_network_game(world) {
        if *world.resource::<VoteView>() != VoteView::default() {
            *world.resource_mut::<VoteView>() = VoteView::default();
        }
        return;
    }
    let now = now(world);
    for n in news {
        let line = match &n {
            VoteNews::Shown(s) => {
                let first = world.resource::<VoteView>().active.as_ref().is_none_or(|a| a.id != s.id);
                let mut view = world.resource_mut::<VoteView>();
                view.ends = now + s.seconds_left as f64;
                view.active = Some(s.clone());
                first.then(|| format!("{} called a vote: {} {}", s.caller, s.issue, s.detail))
            }
            VoteNews::Ended { issue, detail, failure, .. } => {
                let mut view = world.resource_mut::<VoteView>();
                view.active = None;
                view.ended = Some((n.clone(), now));
                Some(match failure {
                    None => format!("Vote passed: {issue} {detail}"),
                    Some(VoteFailure::Quorum) => format!("Vote failed: {issue} {detail} (not enough players voted)"),
                    Some(VoteFailure::YesNo) => format!("Vote failed: {issue} {detail} (not enough players voted yes)"),
                })
            }
            VoteNews::Refused(r) => {
                world.resource_mut::<VoteView>().refused = Some((r.clone(), now));
                Some(format!("Vote refused: {r:?}"))
            }
            VoteNews::Setup(issues) => {
                let mut view = world.resource_mut::<VoteView>();
                view.setup = issues.clone();
                view.setups += 1;
                let mut lines = vec!["callvote <issue> [detail]; issues:".to_string()];
                for i in issues {
                    let choices: Vec<String> = i.choices.iter().map(|(v, s)| format!("{v} ({s})")).collect();
                    lines.push(if choices.is_empty() {
                        format!("  {}", i.name)
                    } else {
                        format!("  {}: {}", i.name, choices.join(", "))
                    });
                }
                Some(lines.join("\n"))
            }
        };
        if let Some(line) = line
            && let Some(mut console) = world.get_resource_mut::<Console>()
        {
            console.print(Level::Info, line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_quorum_is_a_share_of_who_may_vote_rounded_up() {
        assert_eq!(needed(3, 0.6), 2);
        assert_eq!(needed(5, 0.6), 3);
        assert_eq!(needed(10, 0.6), 6);
        assert_eq!(needed(1, 0.6), 1);
        assert_eq!(needed(0, 0.6), 1);
        assert_eq!(needed(4, 1.0), 4);
        // Out of range: CS:S's 0.1 to 1.
        assert_eq!(needed(10, 0.0), 1);
        assert_eq!(needed(4, 2.0), 4);
    }

    #[test]
    fn a_vote_passes_with_the_quorum_and_more_yes_than_no() {
        assert_eq!(result(2, 0, 3, 0.6), Ok(()));
        assert_eq!(result(2, 1, 3, 0.6), Ok(()));
        // Two of five isn't 0.6 of them: too few voted at all, or too few
        // yes.
        assert_eq!(result(2, 0, 5, 0.6), Err(VoteFailure::Quorum));
        assert_eq!(result(2, 2, 5, 0.6), Err(VoteFailure::YesNo));
        assert_eq!(result(1, 2, 3, 0.6), Err(VoteFailure::YesNo));
        // A tie fails even with the quorum.
        assert_eq!(result(3, 3, 6, 0.5), Err(VoteFailure::YesNo));
        // Alone: the caller's own yes.
        assert_eq!(result(1, 0, 1, 0.6), Ok(()));
        assert_eq!(result(0, 0, 1, 0.6), Err(VoteFailure::Quorum));
    }
}
