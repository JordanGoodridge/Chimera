//! The lobby service's part (specs/combat_arms/lobby.md R, O, T, S, J, K,
//! M): rooms and their players, and the rules the service enforces when
//! the client's checks (`check`) let a request through. A pure state
//! machine: a request goes in (`request`), notices and events come out
//! (`Lobby::notices`, `Lobby::events`), and a 1 s tick (`tick`) runs the
//! countdown. The Bevy side (`host`) carries the notices to the clients
//! and the events to whatever launches the match.
//!
//! One channel with 100 rooms; every rule is the US branch the spec marks.

use std::{collections::BTreeMap, sync::Arc};

use super::{
    data::{self, LobbyData, ModeRec, RANDOM_MAP},
    items::Catalog,
    loadout::{self, Loadout, SetKind},
    room::*,
};

/// What differs between lobbies (the channel, our additions).
#[derive(Clone, Debug, PartialEq)]
pub struct LobbyConfig {
    /// The channel's room limit (16 at most).
    pub channel_limit: u32,
    /// Fireteam rooms the channel may hold (the number is ours: the spec
    /// says only "limited").
    pub fireteam_rooms: usize,
    /// A host may kick (the Elite Moderator item's right; open question 5:
    /// LAN hosts may be given it).
    pub host_kick: bool,
    /// Pure Combat rooms may be made (open question 8: none offline).
    pub pure_combat: bool,
    /// The address "go" carries.
    pub address: String,
}

impl Default for LobbyConfig {
    fn default() -> Self {
        Self {
            channel_limit: data::ROOM_PLAYERS_MAX,
            fireteam_rooms: 10,
            host_kick: false,
            pure_combat: false,
            address: "local".into(),
        }
    }
}

/// A room in the lobby.
#[derive(Clone, Debug, PartialEq)]
pub struct Room {
    pub id: RoomId,
    pub settings: Settings,
    /// In join order.
    pub seats: Vec<Seat>,
    pub host: PlayerId,
    pub state: RoomState,
    /// The start counter, while it runs.
    pub count: Option<u32>,
    /// The game server has reported ready.
    pub server_ready: bool,
    /// Players this host's moderator rights keep out.
    pub blocked: Vec<PlayerId>,
}

impl Room {
    pub fn seat(&self, p: PlayerId) -> Option<&Seat> {
        self.seats.iter().find(|s| s.id == p)
    }

    fn seat_mut(&mut self, p: PlayerId) -> Option<&mut Seat> {
        self.seats.iter_mut().find(|s| s.id == p)
    }

    pub fn team_count(&self, team: Team) -> usize {
        self.seats.iter().filter(|s| s.team == team).count()
    }

    /// Ready players on a team (the host counts as ready on their own).
    pub fn ready_on(&self, team: Team) -> usize {
        self.seats
            .iter()
            .filter(|s| s.team == team && (s.state == PlayerState::Ready || s.id == self.host))
            .count()
    }

    pub fn detail(&self) -> RoomDetail {
        RoomDetail {
            id: self.id,
            settings: self.settings.clone(),
            host: self.host,
            state: self.state,
            seats: self.seats.clone(),
        }
    }

    pub fn summary(&self) -> RoomSummary {
        let s = &self.settings;
        RoomSummary {
            id: self.id,
            name: s.name.clone(),
            locked: !s.password.is_empty(),
            map: s.map,
            mode: s.mode,
            pure_combat: s.pure_combat,
            super_soldiers: s.super_soldiers,
            arms: s.arms,
            users: self.seats.len() as u32,
            max: s.max_players,
            state: self.state,
            join_mid: s.join_mid,
            moderator: 0,
        }
    }
}

struct Member {
    name: String,
    room: Option<RoomId>,
}

/// The lobby service.
pub struct Lobby {
    pub data: Arc<LobbyData>,
    pub catalog: Arc<Catalog>,
    pub config: LobbyConfig,
    /// Each player's stored loadout (the service keeps it per account).
    pub loadouts: BTreeMap<PlayerId, Loadout>,
    pub rooms: BTreeMap<RoomId, Room>,
    members: BTreeMap<PlayerId, Member>,
    /// Notices pushed, in order, until taken.
    pub notices: Vec<(Audience, Notice)>,
    /// Events for the launcher, until taken.
    pub events: Vec<LobbyEvent>,
    next_order: u64,
    rng: u64,
}

impl Lobby {
    pub fn new(data: Arc<LobbyData>, catalog: Arc<Catalog>, config: LobbyConfig) -> Self {
        Self {
            data,
            catalog,
            loadouts: BTreeMap::new(),
            config,
            rooms: BTreeMap::new(),
            members: BTreeMap::new(),
            notices: Vec::new(),
            events: Vec::new(),
            next_order: 0,
            rng: 0x9e37_79b9_7f4a_7c15,
        }
    }

    pub fn builtin() -> Self {
        Self::new(LobbyData::built_in(), Catalog::built_in(), LobbyConfig::default())
    }

    pub fn take_notices(&mut self) -> Vec<(Audience, Notice)> {
        std::mem::take(&mut self.notices)
    }

    pub fn take_events(&mut self) -> Vec<LobbyEvent> {
        std::mem::take(&mut self.events)
    }

    fn push(&mut self, to: Audience, n: Notice) {
        self.notices.push((to, n));
    }

    fn rand(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }

    // --- Queries.

    pub fn has_player(&self, p: PlayerId) -> bool {
        self.members.contains_key(&p)
    }

    pub fn name_of(&self, p: PlayerId) -> Option<&str> {
        self.members.get(&p).map(|m| m.name.as_str())
    }

    pub fn room_of(&self, p: PlayerId) -> Option<RoomId> {
        self.members.get(&p).and_then(|m| m.room)
    }

    pub fn players(&self) -> Vec<PlayerId> {
        self.members.keys().copied().collect()
    }

    pub fn room(&self, id: RoomId) -> Option<&Room> {
        self.rooms.get(&id)
    }

    pub fn summaries(&self) -> Vec<RoomSummary> {
        self.rooms.values().map(Room::summary).collect()
    }

    /// Who a notice goes to.
    pub fn recipients(&self, a: Audience) -> Vec<PlayerId> {
        match a {
            Audience::Player(p) => vec![p],
            Audience::Room(r) => self.rooms.get(&r).map(|r| r.seats.iter().map(|s| s.id).collect()).unwrap_or_default(),
            Audience::RoomExcept(r, x) => self
                .rooms
                .get(&r)
                .map(|r| r.seats.iter().map(|s| s.id).filter(|p| *p != x).collect())
                .unwrap_or_default(),
            Audience::Channel => self.members.keys().copied().collect(),
            Audience::Lobby => self.members.iter().filter(|(_, m)| m.room.is_none()).map(|(p, _)| *p).collect(),
        }
    }

    fn mode_of(&self, room: &Room) -> &ModeRec {
        self.data.mode(room.settings.mode).expect("a room's mode is in the table")
    }

    // --- Requests.

    /// A client's request (M). A refusal is returned and also pushed as a
    /// `Notice::Refused` to the requester.
    pub fn request(&mut self, from: PlayerId, req: Request) -> Result<(), Refusal> {
        let r = self.handle(from, req);
        if let Err(e) = &r {
            self.push(Audience::Player(from), Notice::Refused(e.clone()));
        }
        r
    }

    fn handle(&mut self, from: PlayerId, req: Request) -> Result<(), Refusal> {
        if let Request::Hello { name } = req {
            self.hello(from, name);
            return Ok(());
        }
        if !self.members.contains_key(&from) {
            return Err(Refusal::NotInLobby);
        }
        match req {
            Request::Hello { .. } => unreachable!(),
            Request::ListRooms => {
                let list = self.summaries();
                self.push(Audience::Player(from), Notice::RoomList(list));
                Ok(())
            }
            Request::Create(spec) => self.create(from, spec),
            Request::Enter { room, password } => self.enter(from, room, &password),
            Request::Leave => {
                self.room_id(from)?;
                self.leave_room(from, false);
                Ok(())
            }
            Request::Change { mode, what } => self.change(from, mode, what),
            Request::SetTeam(t) => self.set_team(from, t),
            Request::Ready(on) => self.ready(from, on),
            Request::Start => self.start(from),
            Request::Extra(x) => self.extra(from, x),
            Request::EntrustHost(t) => self.entrust(from, t),
            Request::Kick { target, .. } => self.kick(from, target),
            Request::Invite => self.invite(from),
            Request::Chat(text) => {
                self.chat(from, &text);
                Ok(())
            }
            Request::ReturnToRoom(reason) => {
                self.return_to_room(from, reason);
                Ok(())
            }
            Request::Loadout(l) => {
                let clean = l.sanitized(&self.catalog, &super::items::Owned::all());
                self.loadouts.insert(from, clean);
                Ok(())
            }
        }
    }

    fn room_id(&self, p: PlayerId) -> Result<RoomId, Refusal> {
        self.members.get(&p).and_then(|m| m.room).ok_or(Refusal::NotInRoom)
    }

    fn hello(&mut self, p: PlayerId, name: String) {
        let name = if name.trim().is_empty() { format!("Player{p}") } else { name };
        self.members.entry(p).or_insert(Member { name, room: None });
        self.push(Audience::Player(p), Notice::Welcome(p));
        let list = self.summaries();
        self.push(Audience::Player(p), Notice::RoomList(list));
    }

    /// A player is gone (a dropped connection): out of the room and the
    /// channel.
    pub fn disconnect(&mut self, p: PlayerId) {
        if self.room_of(p).is_some() {
            self.leave_room(p, false);
        }
        self.members.remove(&p);
    }

    // --- Rooms.

    fn create(&mut self, from: PlayerId, spec: CreateSpec) -> Result<(), Refusal> {
        if self.members[&from].room.is_some() {
            return Err(Refusal::AlreadyInRoom);
        }
        let data = self.data.clone();
        let mode = data.mode(spec.mode).ok_or(Refusal::ModeNoRules)?;
        if !mode.rules || mode.single_play {
            return Err(Refusal::ModeNoRules);
        }
        if spec.pure_combat && !self.config.pure_combat {
            return Err(Refusal::PureCombat);
        }
        let map = match spec.map {
            Some(RANDOM_MAP) => {
                let open = data.maps_for(mode);
                if open.is_empty() {
                    return Err(Refusal::MapNotOpen);
                }
                let i = (self.rand() % open.len() as u64) as usize;
                open[i].id
            }
            Some(m) if data.map_open(m, mode) => m,
            Some(_) => return Err(Refusal::MapNotOpen),
            None => mode.default_map,
        };
        let n = spec.players;
        if n > self.config.channel_limit.min(data::ROOM_PLAYERS_MAX)
            || n < mode.min_players()
            || n > mode.max_players()
            || (mode.two_team() && n % 2 != 0)
        {
            return Err(Refusal::Players);
        }
        let id = (0..data::ROOMS_PER_CHANNEL)
            .find(|i| !self.rooms.contains_key(i))
            .ok_or(Refusal::NoFreeRoom)?;
        if mode.key == "COOP" && self.rooms.values().filter(|r| self.mode_of(r).key == "COOP").count() >= self.config.fireteam_rooms {
            return Err(Refusal::FireteamLimit);
        }
        // Silent replacements (quirk 12): a goal outside the lobby's range
        // is the mode's default, an invalid arms value All Permitted.
        let (lo, hi) = mode.lobby_goal_range();
        let goal = if (lo..=hi).contains(&spec.goal) { spec.goal } else { mode.goal_default() };
        let arms = self.arms_for(mode, spec.arms);
        let name = if spec.name.chars().count() > data::ROOM_NAME_MAX { String::new() } else { spec.name };
        let password: String = spec.password.chars().take(data::ROOM_PASSWORD_MAX).collect();
        let settings = Settings {
            name,
            password,
            mode: mode.id,
            pure_combat: spec.pure_combat,
            map,
            goal,
            max_players: n,
            arms,
            ..defaults(mode, spec.pure_combat)
        };
        let order = self.order();
        let name = self.members[&from].name.clone();
        let room = Room {
            id,
            settings,
            seats: vec![Seat {
                id: from,
                name,
                team: Team::Alpha,
                state: PlayerState::Waiting,
                extra: Extra::None,
                order,
            }],
            host: from,
            state: RoomState::Waiting,
            count: None,
            server_ready: false,
            blocked: Vec::new(),
        };
        let summary = room.summary();
        let detail = room.detail();
        self.rooms.insert(id, room);
        self.members.get_mut(&from).unwrap().room = Some(id);
        self.push(Audience::Player(from), Notice::Entered(detail));
        self.push(Audience::Channel, Notice::RoomCreated(summary));
        Ok(())
    }

    /// An arms request as the lobby keeps it: invalid values (11 and
    /// more, or any in a mode that takes none) are All Permitted; the cup
    /// forces its own.
    fn arms_for(&self, mode: &ModeRec, arms: [u8; 2]) -> [u8; 2] {
        if mode.arms_default != 0 {
            return [mode.arms_default, 0];
        }
        if mode.refuse_arms || arms[0] > 10 || arms[1] > 10 || arms[0] == data::CUP_ARMS || arms[1] == data::CUP_ARMS {
            return [0, 0];
        }
        arms
    }

    fn order(&mut self) -> u64 {
        self.next_order += 1;
        self.next_order
    }

    fn enter(&mut self, from: PlayerId, room: RoomId, password: &str) -> Result<(), Refusal> {
        if self.members[&from].room.is_some() {
            return Err(Refusal::AlreadyInRoom);
        }
        let r = self.rooms.get(&room).ok_or(Refusal::NoRoom)?;
        // The lobby decides in this order (L).
        if r.state == RoomState::Closing {
            return Err(Refusal::Closing);
        }
        if !r.settings.password.is_empty() && r.settings.password != password {
            return Err(Refusal::WrongPassword);
        }
        if r.seats.len() as u32 >= r.settings.max_players {
            return Err(Refusal::Full);
        }
        if r.state != RoomState::Waiting && !r.settings.join_mid {
            return Err(Refusal::NoMidCombat);
        }
        if r.blocked.contains(&from) {
            return Err(Refusal::Kicked);
        }
        let one_team = self.mode_of(r).one_team;
        // T: the smaller team, Alpha on a tie; one-team modes fill Alpha
        // to 8 first.
        let team = if one_team {
            if r.team_count(Team::Alpha) < data::ONE_SIDE_ALPHA { Team::Alpha } else { Team::Bravo }
        } else if r.team_count(Team::Bravo) < r.team_count(Team::Alpha) {
            Team::Bravo
        } else {
            Team::Alpha
        };
        let order = self.order();
        let name = self.members[&from].name.clone();
        let seat = Seat {
            id: from,
            name,
            team,
            state: PlayerState::Waiting,
            extra: Extra::None,
            order,
        };
        let r = self.rooms.get_mut(&room).unwrap();
        r.seats.push(seat.clone());
        let detail = r.detail();
        let summary = r.summary();
        self.members.get_mut(&from).unwrap().room = Some(room);
        self.push(Audience::Player(from), Notice::Entered(detail));
        self.push(Audience::RoomExcept(room, from), Notice::PlayerEntered(seat));
        self.push(Audience::Channel, Notice::RoomChanged(summary));
        Ok(())
    }

    /// K: leave the room; the last one out destroys it, the host leaving
    /// hands the room to the first in join order who is free.
    fn leave_room(&mut self, p: PlayerId, kicked: bool) {
        let Some(id) = self.members.get_mut(&p).and_then(|m| m.room.take()) else { return };
        let Some(room) = self.rooms.get_mut(&id) else { return };
        room.seats.retain(|s| s.id != p);
        self.events.push(LobbyEvent::Dropped { room: id, player: p });
        self.push(Audience::Player(p), Notice::LeftRoom { kicked });
        let room = self.rooms.get_mut(&id).unwrap();
        if room.seats.is_empty() {
            self.rooms.remove(&id);
            self.push(Audience::Channel, Notice::RoomDestroyed(id));
            return;
        }
        let was_host = room.host == p;
        self.push(Audience::Room(id), Notice::Left(p));
        if was_host {
            let room = self.rooms.get_mut(&id).unwrap();
            let pick = room
                .seats
                .iter()
                .find(|s| s.extra == Extra::None)
                .or_else(|| room.seats.iter().find(|s| s.extra == Extra::Outfitting))
                .or_else(|| room.seats.first())
                .map(|s| s.id)
                .unwrap();
            room.host = pick;
            // Quirk 13: a ready new host is quietly un-readied.
            let seat = room.seat_mut(pick).unwrap();
            let unready = seat.state == PlayerState::Ready;
            if unready {
                seat.state = PlayerState::Waiting;
            }
            let extra = seat.extra;
            self.push(Audience::Room(id), Notice::HostChanged(pick));
            if unready {
                self.push(Audience::Room(id), Notice::State(pick, PlayerState::Waiting, extra));
            }
        }
        let summary = self.rooms[&id].summary();
        self.push(Audience::Channel, Notice::RoomChanged(summary));
    }

    // --- Settings (O).

    fn change(&mut self, from: PlayerId, believed: u16, what: Setting) -> Result<(), Refusal> {
        let id = self.room_id(from)?;
        let room = &self.rooms[&id];
        if room.host != from {
            return Err(Refusal::NotHost);
        }
        if room.state != RoomState::Waiting {
            return Err(Refusal::NotWaiting);
        }
        // A stale request (the client's idea of the mode differs).
        if believed != room.settings.mode {
            return Err(Refusal::Stale);
        }
        let data = self.data.clone();
        let mode = data.mode(room.settings.mode).unwrap().clone();
        let pure = room.settings.pure_combat;
        let mut unready = false;
        let mut line: Option<String> = None;
        match what {
            Setting::Mode(m) => {
                let new = data.mode(m).ok_or(Refusal::ModeNoRules)?;
                if m == mode.id {
                    return Err(Refusal::NoChange);
                }
                if !new.rules || new.single_play {
                    return Err(Refusal::ModeNoRules);
                }
                let count = room.seats.len() as u32;
                let mut max = new.recommended().max(count);
                if new.two_team() && max % 2 != 0 {
                    max += 1;
                }
                if max > new.max_players() {
                    return Err(Refusal::TooManyPlayers {
                        mode: new.name.clone(),
                        max: new.max_players(),
                    });
                }
                if count > new.max_players() {
                    return Err(Refusal::TooManyPlayers {
                        mode: new.name.clone(),
                        max: new.max_players(),
                    });
                }
                if new.key == "COOP" && self.rooms.values().filter(|r| self.mode_of(r).key == "COOP").count() >= self.config.fireteam_rooms {
                    return Err(Refusal::FireteamLimit);
                }
                let map = if data.map_open(room.settings.map, new) { room.settings.map } else { new.default_map };
                let name = new.name.clone();
                let r = self.rooms.get_mut(&id).unwrap();
                let d = defaults(new, pure);
                let s = &mut r.settings;
                s.mode = m;
                s.map = map;
                s.goal = new.goal_default();
                s.max_players = max;
                s.arms = [new.arms_default, 0];
                s.join_mid = d.join_mid;
                s.swap_display = d.swap_display;
                s.friendly_fire = d.friendly_fire;
                s.kill_cam = d.kill_cam;
                s.abilities = d.abilities;
                s.super_soldiers = d.super_soldiers;
                // One-team modes: Bravo moves to Alpha in join order until
                // Alpha has 8.
                if new.one_team {
                    let mut alpha = r.team_count(Team::Alpha);
                    for seat in r.seats.iter_mut() {
                        if alpha >= data::ONE_SIDE_ALPHA {
                            break;
                        }
                        if seat.team == Team::Bravo {
                            seat.team = Team::Alpha;
                            alpha += 1;
                        }
                    }
                }
                unready = true;
                line = Some(format!("The game has been changed to {name} mode."));
            }
            Setting::Map(m) => {
                if m == room.settings.map {
                    return Err(Refusal::NoChange);
                }
                if !data.map_open(m, &mode) {
                    return Err(Refusal::MapNotOpen);
                }
                let r = self.rooms.get_mut(&id).unwrap();
                r.settings.map = m;
                r.settings.goal = mode.goal_default();
            }
            Setting::Goal(g) => {
                let (lo, hi) = mode.lobby_goal_range();
                if !(lo..=hi).contains(&g) {
                    return Err(Refusal::BadValue);
                }
                self.rooms.get_mut(&id).unwrap().settings.goal = g;
            }
            Setting::Players(n) => {
                let bigger = room.team_count(Team::Alpha).max(room.team_count(Team::Bravo)) as u32;
                let total = room.seats.len() as u32;
                let fits = if mode.two_team() { bigger <= n / 2 } else { total <= n };
                if n > self.config.channel_limit
                    || n < mode.min_players()
                    || n > mode.max_players()
                    || (mode.two_team() && n % 2 != 0)
                    || !fits
                {
                    return Err(Refusal::Players);
                }
                self.rooms.get_mut(&id).unwrap().settings.max_players = n;
            }
            Setting::Arms(a, b) => {
                if mode.refuse_arms || a >= 11 || b >= 11 {
                    return Err(Refusal::OptionRefused);
                }
                if [a, b] == room.settings.arms {
                    return Err(Refusal::NoChange);
                }
                let name = data.arms_option(a).map(|o| o.name.clone()).unwrap_or_default();
                self.rooms.get_mut(&id).unwrap().settings.arms = [a, b];
                unready = true;
                line = Some(format!("Arms Setting changed to : {name}"));
            }
            Setting::JoinMid(on) => {
                self.rooms.get_mut(&id).unwrap().settings.join_mid = on;
            }
            Setting::FriendlyFire(on) => {
                if mode.refuse_ff || pure {
                    return Err(Refusal::OptionRefused);
                }
                self.rooms.get_mut(&id).unwrap().settings.friendly_fire = on;
            }
            Setting::KillCam(on) => {
                if mode.refuse_cam || pure {
                    return Err(Refusal::OptionRefused);
                }
                self.rooms.get_mut(&id).unwrap().settings.kill_cam = on;
            }
            Setting::Abilities(on) => {
                if mode.refuse_perk || pure {
                    return Err(Refusal::OptionRefused);
                }
                self.rooms.get_mut(&id).unwrap().settings.abilities = on;
            }
            Setting::SuperSoldiers(on) => {
                if !mode.mutation {
                    return Err(Refusal::OptionRefused);
                }
                self.rooms.get_mut(&id).unwrap().settings.super_soldiers = on;
            }
            Setting::Name(n) => {
                if n.chars().count() > data::ROOM_NAME_MAX {
                    return Err(Refusal::BadValue);
                }
                self.rooms.get_mut(&id).unwrap().settings.name = n;
            }
            Setting::Password(p) => {
                if p.chars().count() > data::ROOM_PASSWORD_MAX {
                    return Err(Refusal::BadValue);
                }
                self.rooms.get_mut(&id).unwrap().settings.password = p;
            }
        }
        let room = self.rooms.get_mut(&id).unwrap();
        let settings = room.settings.clone();
        let mut states = Vec::new();
        if unready {
            for s in room.seats.iter_mut().filter(|s| s.state == PlayerState::Ready) {
                s.state = PlayerState::Waiting;
            }
            states = room.seats.iter().map(|s| (s.id, s.team, s.state, s.extra)).collect();
        }
        let summary = room.summary();
        self.push(Audience::Room(id), Notice::Settings(settings));
        for (p, team, state, extra) in states {
            self.push(Audience::Room(id), Notice::Team(p, team));
            self.push(Audience::Room(id), Notice::State(p, state, extra));
        }
        if let Some(l) = line {
            self.push(Audience::Room(id), Notice::System(l));
        }
        self.push(Audience::Channel, Notice::RoomChanged(summary));
        Ok(())
    }

    // --- Teams (T).

    fn set_team(&mut self, from: PlayerId, team: Team) -> Result<(), Refusal> {
        let id = self.room_id(from)?;
        let room = &self.rooms[&id];
        let mode = self.mode_of(room);
        let seat = room.seat(from).unwrap();
        if seat.state != PlayerState::Waiting || mode.one_team || seat.team == team {
            return Err(Refusal::TeamSwitch);
        }
        if room.team_count(team) as u32 >= room.settings.max_players / 2 {
            return Err(Refusal::TeamSwitch);
        }
        self.rooms.get_mut(&id).unwrap().seat_mut(from).unwrap().team = team;
        self.push(Audience::Room(id), Notice::Team(from, team));
        Ok(())
    }

    // --- Ready, start, countdown (S, J).

    fn ready(&mut self, from: PlayerId, on: bool) -> Result<(), Refusal> {
        let id = self.room_id(from)?;
        let room = &self.rooms[&id];
        if room.host == from {
            return Err(Refusal::HostCannotReady);
        }
        let seat = room.seat(from).unwrap();
        let (state, extra) = (seat.state, seat.extra);
        match room.state {
            RoomState::Waiting => {
                let new = match (on, state) {
                    (true, PlayerState::Waiting) => PlayerState::Ready,
                    (false, PlayerState::Ready) => PlayerState::Waiting,
                    _ => return Err(Refusal::NotWaiting),
                };
                self.set_state(id, from, new, extra);
                Ok(())
            }
            // J: READY while the room is starting puts a late joiner in
            // with the others (Join Mid Combat on).
            RoomState::Starting => {
                if !on || state != PlayerState::Waiting || !room.settings.join_mid {
                    return Err(Refusal::FinalRound);
                }
                self.set_state(id, from, PlayerState::Starting, extra);
                Ok(())
            }
            RoomState::InGame | RoomState::Closing => Err(Refusal::FinalRound),
        }
    }

    fn set_state(&mut self, id: RoomId, p: PlayerId, state: PlayerState, extra: Extra) {
        if let Some(s) = self.rooms.get_mut(&id).and_then(|r| r.seat_mut(p)) {
            s.state = state;
            s.extra = extra;
        }
        self.push(Audience::Room(id), Notice::State(p, state, extra));
    }

    fn extra(&mut self, from: PlayerId, x: Extra) -> Result<(), Refusal> {
        let id = self.room_id(from)?;
        let state = self.rooms[&id].seat(from).unwrap().state;
        self.set_state(id, from, state, x);
        Ok(())
    }

    /// The lobby's start rule for the room (S): the host's team counts the
    /// host and its ready players.
    pub fn start_rule_met(&self, room: &Room) -> bool {
        let mode = self.mode_of(room);
        let host_team = room.seat(room.host).map_or(Team::Alpha, |s| s.team);
        let mine = room.ready_on(host_team) as u32;
        let other = room.ready_on(host_team.other()) as u32;
        match (&mode.start.teams, mode.start.total) {
            (Some([a, b]), _) => mine >= *a && other >= *b,
            (None, Some(n)) => mine + other >= n,
            _ => true,
        }
    }

    fn start(&mut self, from: PlayerId) -> Result<(), Refusal> {
        let id = self.room_id(from)?;
        let room = &self.rooms[&id];
        if room.host == from {
            if room.state != RoomState::Waiting {
                return Err(Refusal::NotWaiting);
            }
            if !self.start_rule_met(room) {
                return Err(Refusal::NotEnoughPlayers);
            }
            return self.begin(id);
        }
        // J: a waiting player joins a match in progress.
        let seat = room.seat(from).unwrap();
        match room.state {
            RoomState::Waiting => Err(Refusal::NotHost),
            RoomState::Closing => Err(Refusal::FinalRound),
            RoomState::Starting => {
                if seat.state != PlayerState::Waiting || !room.settings.join_mid {
                    return Err(Refusal::FinalRound);
                }
                let extra = seat.extra;
                self.set_state(id, from, PlayerState::Starting, extra);
                Ok(())
            }
            RoomState::InGame => {
                if seat.state != PlayerState::Waiting || !room.settings.join_mid {
                    return Err(Refusal::FinalRound);
                }
                let (team, extra) = (seat.team, seat.extra);
                self.set_state(id, from, PlayerState::InGame, extra);
                let weapons = self.weapons_for(id, from);
                self.events.push(LobbyEvent::Expect { room: id, player: from, team, weapons });
                let address = self.config.address.clone();
                self.push(Audience::Player(from), Notice::Go { address });
                Ok(())
            }
        }
    }

    /// S step 1: the host's start is accepted.
    fn begin(&mut self, id: RoomId) -> Result<(), Refusal> {
        let room = self.rooms.get_mut(&id).unwrap();
        room.state = RoomState::Starting;
        room.count = Some(0);
        room.server_ready = false;
        let host = room.host;
        let mut picked = Vec::new();
        for s in room.seats.iter_mut() {
            if s.id == host || s.state == PlayerState::Ready {
                s.state = PlayerState::Starting;
                picked.push((s.id, s.team, s.extra));
            }
        }
        let settings = room.settings.clone();
        let summary = room.summary();
        self.events.push(LobbyEvent::Launch {
            room: id,
            settings,
            players: picked.iter().map(|(p, t, _)| (*p, *t)).collect(),
        });
        self.push(Audience::Room(id), Notice::RoomStateChanged(RoomState::Starting));
        for (p, _, x) in picked {
            self.push(Audience::Room(id), Notice::State(p, PlayerState::Starting, x));
        }
        self.push(Audience::Channel, Notice::RoomChanged(summary));
        Ok(())
    }

    /// The weapons a player takes into the room's match (E): the set the
    /// room uses (Pure Combat rooms the Pure Combat set), the arms
    /// options' filter applied.
    pub fn weapons_for(&self, room: RoomId, player: PlayerId) -> Vec<i32> {
        let Some(r) = self.rooms.get(&room) else { return Vec::new() };
        let fresh;
        let l = match self.loadouts.get(&player) {
            Some(l) => l,
            None => {
                fresh = Loadout::new_player(&self.catalog);
                &fresh
            }
        };
        let kind = if r.settings.pure_combat { SetKind::PureCombat } else { SetKind::Normal };
        loadout::match_weapons(&self.data, &self.catalog, l.set(kind), r.settings.arms)
    }

    /// Items of a player expired (P): they leave the stored loadout (the
    /// weapon slots fall back) and the player is told, to see the notice
    /// on their next visit to the room. A match never loses an item
    /// halfway: the in-match kit is the copy sent at entry.
    pub fn items_expired(&mut self, player: PlayerId, gone: &[i32]) {
        if gone.is_empty() {
            return;
        }
        if let Some(l) = self.loadouts.get_mut(&player) {
            l.expired(&self.catalog, gone);
        }
        self.push(Audience::Player(player), Notice::ItemsExpired(gone.to_vec()));
    }

    /// The game server has reported ready (M).
    pub fn server_ready(&mut self, room: RoomId) {
        if let Some(r) = self.rooms.get_mut(&room) {
            r.server_ready = true;
        }
    }

    /// The lobby tick, once per second (Order of events).
    pub fn tick(&mut self) {
        let ids: Vec<RoomId> = self.rooms.iter().filter(|(_, r)| r.count.is_some()).map(|(i, _)| *i).collect();
        for id in ids {
            let room = self.rooms.get_mut(&id).unwrap();
            let c = room.count.unwrap();
            if c < data::COUNTDOWN_TICKS {
                self.push(Audience::Room(id), Notice::Countdown(c));
            }
            let room = self.rooms.get_mut(&id).unwrap();
            if c >= data::COUNTDOWN_TICKS && room.server_ready {
                self.go(id);
                continue;
            }
            if c == data::LAUNCH_TIMEOUT {
                self.events.push(LobbyEvent::StopLauncher(id));
            }
            if c >= data::LAUNCH_GIVE_UP {
                self.give_up(id);
                continue;
            }
            self.rooms.get_mut(&id).unwrap().count = Some(c + 1);
        }
    }

    /// Step 3: the room is in game; each starting player gets "go".
    fn go(&mut self, id: RoomId) {
        let address = self.config.address.clone();
        let room = self.rooms.get_mut(&id).unwrap();
        room.state = RoomState::InGame;
        room.count = None;
        let mut going = Vec::new();
        for s in room.seats.iter_mut().filter(|s| s.state == PlayerState::Starting) {
            s.state = PlayerState::InGame;
            going.push((s.id, s.team, s.extra));
        }
        let summary = room.summary();
        self.push(Audience::Room(id), Notice::RoomStateChanged(RoomState::InGame));
        for (p, team, x) in going {
            self.push(Audience::Room(id), Notice::State(p, PlayerState::InGame, x));
            self.push(Audience::Player(p), Notice::Go { address: address.clone() });
            let weapons = self.weapons_for(id, p);
            self.events.push(LobbyEvent::Expect { room: id, player: p, team, weapons });
        }
        self.push(Audience::Channel, Notice::RoomChanged(summary));
    }

    /// Step 5: no game server by count 17; everyone waits again.
    fn give_up(&mut self, id: RoomId) {
        self.back_to_waiting(id);
    }

    fn back_to_waiting(&mut self, id: RoomId) {
        let room = self.rooms.get_mut(&id).unwrap();
        room.state = RoomState::Waiting;
        room.count = None;
        room.server_ready = false;
        let mut changed = Vec::new();
        for s in room.seats.iter_mut().filter(|s| s.state != PlayerState::Waiting) {
            s.state = PlayerState::Waiting;
            changed.push((s.id, s.extra));
        }
        let summary = room.summary();
        self.push(Audience::Room(id), Notice::RoomStateChanged(RoomState::Waiting));
        for (p, x) in changed {
            self.push(Audience::Room(id), Notice::State(p, PlayerState::Waiting, x));
        }
        self.push(Audience::Channel, Notice::RoomChanged(summary));
    }

    /// The game server says the match is near its end: the room closes to
    /// joiners.
    pub fn closing(&mut self, room: RoomId) {
        let Some(r) = self.rooms.get_mut(&room) else { return };
        if r.state != RoomState::InGame {
            return;
        }
        r.state = RoomState::Closing;
        let summary = r.summary();
        self.push(Audience::Room(room), Notice::RoomStateChanged(RoomState::Closing));
        self.push(Audience::Channel, Notice::RoomChanged(summary));
    }

    /// G: the game server reports the match over; the room is waiting and
    /// everyone who was starting or in game is waiting (ready states are
    /// lost).
    pub fn match_over(&mut self, room: RoomId) {
        if matches!(self.rooms.get(&room).map(|r| r.state), Some(RoomState::InGame | RoomState::Closing)) {
            self.back_to_waiting(room);
        }
    }

    /// A player returns to the room (X, G, S step 6): set waiting at once;
    /// ignored when they already are.
    fn return_to_room(&mut self, from: PlayerId, _reason: ReturnReason) {
        let Ok(id) = self.room_id(from) else { return };
        let seat = self.rooms[&id].seat(from).unwrap();
        if matches!(seat.state, PlayerState::Starting | PlayerState::InGame) {
            let extra = seat.extra;
            self.set_state(id, from, PlayerState::Waiting, extra);
            self.events.push(LobbyEvent::Dropped { room: id, player: from });
        }
    }

    // --- Host, kick, invite, chat (K).

    fn entrust(&mut self, from: PlayerId, target: PlayerId) -> Result<(), Refusal> {
        let id = self.room_id(from)?;
        let room = &self.rooms[&id];
        if room.host != from {
            return Err(Refusal::NotHost);
        }
        let Some(seat) = room.seat(target) else { return Err(Refusal::BadTarget) };
        if target == from {
            return Err(Refusal::BadTarget);
        }
        let (was_ready, extra) = (seat.state == PlayerState::Ready, seat.extra);
        self.rooms.get_mut(&id).unwrap().host = target;
        self.push(Audience::Room(id), Notice::HostChanged(target));
        if was_ready {
            self.set_state(id, target, PlayerState::Waiting, extra);
        }
        Ok(())
    }

    fn kick(&mut self, from: PlayerId, target: PlayerId) -> Result<(), Refusal> {
        let id = self.room_id(from)?;
        let room = &self.rooms[&id];
        if room.host != from {
            return Err(Refusal::NotHost);
        }
        if !self.config.host_kick {
            return Err(Refusal::NoModerator);
        }
        let Some(seat) = room.seat(target) else { return Err(Refusal::BadTarget) };
        if target == from || matches!(seat.state, PlayerState::Starting | PlayerState::InGame) {
            return Err(Refusal::BadTarget);
        }
        self.rooms.get_mut(&id).unwrap().blocked.push(target);
        self.leave_room(target, true);
        Ok(())
    }

    fn invite(&mut self, from: PlayerId) -> Result<(), Refusal> {
        let id = self.room_id(from)?;
        let room = &self.rooms[&id];
        if room.host != from {
            return Err(Refusal::NotHost);
        }
        let password = room.settings.password.clone();
        let host = self.members[&from].name.clone();
        let targets: Vec<PlayerId> = self
            .members
            .iter()
            .filter(|(p, m)| **p != from && m.room.is_none())
            .map(|(p, _)| *p)
            .take(data::INVITE_MAX)
            .collect();
        for t in targets {
            self.push(
                Audience::Player(t),
                Notice::Invite {
                    from: host.clone(),
                    room: id,
                    password: password.clone(),
                },
            );
        }
        Ok(())
    }

    fn chat(&mut self, from: PlayerId, text: &str) {
        let text: String = text.chars().filter(|c| !c.is_control()).take(data::CHAT_MAX).collect();
        if text.is_empty() {
            return;
        }
        let name = self.members[&from].name.clone();
        let to = match self.members[&from].room {
            Some(r) => Audience::Room(r),
            None => Audience::Lobby,
        };
        self.push(to, Notice::Chat { from: name, text });
    }
}

/// A mode's defaults after a create or a mode change (O).
pub fn defaults(mode: &ModeRec, pure: bool) -> Settings {
    Settings {
        name: String::new(),
        password: String::new(),
        mode: mode.id,
        pure_combat: pure,
        map: mode.default_map,
        goal: mode.goal_default(),
        max_players: mode.recommended(),
        arms: [mode.arms_default, 0],
        join_mid: true,
        swap_display: mode.swap_display,
        friendly_fire: false,
        // Pure Combat: Abilities and Kill Cam off, locked.
        kill_cam: mode.cam_default && !pure,
        abilities: mode.perk_default && !pure,
        super_soldiers: false,
    }
}
