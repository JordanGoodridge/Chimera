//! Combat Arms' lobby: rooms, ready and start, and the loadout screen's
//! rules (specs/combat_arms/lobby.md; section letters below are its). The
//! reference's lobby is an online service; here the service's part
//! (`service`) is a state machine a game process hosts (a listen game, the
//! dedicated server, or a game of one), and the client's part is `client`
//! and the screens (`ui::lobby`).
//!
//! - `data`: the modes with their goal, player and option ranges, the maps
//!   and the modes each offers, the arms options and the default room
//!   titles (data/lobby.toml, ours; the install's room data file is
//!   unread) (D).
//! - `room`: the vocabulary: rooms, seats, the requests a client makes,
//!   the notices the lobby sends, the refusals with their texts, the
//!   events the launcher gets (M).
//! - `service`: `Lobby`, the lobby service: create, enter, leave and host
//!   change (K), room settings (O), teams (T), ready, start, the 1 s
//!   tick's countdown, "go" and the give-up (S), joining a match in
//!   progress (J), the match's end (G), chat and invite.
//! - `check`: the client's own checks before it sends (the start count,
//!   the team switch, the mode change) and the room creation popup's
//!   fields (C).
//! - `list`: the room list's copy, filters, sort and columns (L).
//! - `client`: `ClientLobby`, what a client knows of the lobby and what
//!   notices do to it, with its timers (the start timeout, idle).
//! - `items`, `loadout`: the item list (data/lobby_items.toml, and gear
//!   from data/gear.toml by item number), what may go in which slot, the
//!   1st Arms check at READY and START, what the game server keeps in a
//!   match, item lifetimes and what an expired item does (E, P), and how
//!   a set becomes the gear's `Selection` and `CaLoadout`.
//! - `host`: the lobby on the engine: `CaLobbyHost` (the service in a
//!   process, off until `ca_lobby 1`), the messages over the network, the
//!   launcher that makes a room's start the process's match, and the
//!   players held out of the match until "go".
//!
//! Left out (online services; clean seams): login and the account (a local
//! profile name), the server and channel lists (one local server with one
//! channel), the shop (prices, purchases, currency: items are all owned,
//! `items::Owned` has the expiry clock), premium items, clans, friends,
//! jobs, quick join, rewards, and whisper delivery (`game::chat`'s
//! `CaWhisperOut` is the seam).
//!
//! Known shortcuts (docs/tech-debt.md; the most literal reading where the
//! spec is unclear):
//! - The room data file is ours (data/lobby.toml): the maps and the modes
//!   each offers, the arms list and the "US" ranges are the spec's, the
//!   map keys (the world names) are guesses. Mode 20-25 have no rules.
//! - Pure Combat rooms cannot be created (open question 8) unless the
//!   lobby's config allows it; their team-switch queue is not modelled.
//!   Super Soldiers, the Go4CA Cup's allowed list, Fireteam's
//!   co-op-allowed weapons, the moderator kick (off unless the config
//!   gives the host `host_kick`, open question 5), the room's moderator
//!   level and the premium 2nd Arms are not there.
//! - A mode change from a one-team mode to a two-team one keeps the teams
//!   as they are (the spec says nothing of balancing them).
//! - The Fireteam room limit per channel (10) and the weapons' item
//!   numbers beyond the four base ones are ours; the weapons stand on our
//!   weapon documents (`items`).
//! - Cases of the spec that contradict each other: test 3 says the sort
//!   after TITLE twice, then Map, is descending; the sort section and
//!   quirk 2 give a direction that flips at every click, which is
//!   implemented (the Map click after two TITLE clicks is ascending).
//! - The launcher runs the match in the lobby's own process: one match
//!   at a time, the room's settings applied to `CaRuleSettings`, the map
//!   changed by `changelevel` when the host loads maps; abilities, Super
//!   Soldiers and a room's player limit are not passed to the rules.
//! - The loadout reaches the match as the kit of weapons (`OwnKit`, the
//!   arms options' filter applied by the lobby); the look, perks and gear
//!   items go the existing way (`CaOwnLook`, `CaOwnGear`); the
//!   fast-respawn counter and rental expiry stay the console's.

pub mod check;
pub mod client;
pub mod data;
pub mod host;
pub mod items;
pub mod list;
pub mod loadout;
pub mod room;
pub mod service;

pub use host::{CaLobbyClient, CaLobbyHost, CaLobbyNotice, CaLobbyPlugin, CaLobbyRequest};
