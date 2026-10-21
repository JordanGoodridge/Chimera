//! The client's own checks (specs/combat_arms/lobby.md C, T, S, O): what
//! the screens hide, disable or refuse before a request is sent, and the
//! room creation popup's fields with their lists and defaults. The
//! service (`service`) checks the same rules again.

use std::sync::Arc;

use super::{
    data::{self, LobbyData, ModeRec},
    room::*,
    service::defaults,
};

/// The room creation popup (C): its fields, what each offers and its
/// defaults. `single` is the PLAY WITH BOTS tab.
#[derive(Clone, Debug, PartialEq)]
pub struct CreateForm {
    data: Arc<LobbyData>,
    pub single: bool,
    pub name: String,
    pub use_password: bool,
    pub password: String,
    pub mode: u16,
    pub map: u16,
    pub goal: u32,
    pub players: u32,
    pub arms: [u8; 2],
    /// The single tab's difficulty (0 NORMAL, 1 HARD, 2 EXTREME).
    pub difficulty: u32,
    /// The channel's room limit.
    pub limit: u32,
}

/// The default difficulty of bot play (`DefaultDifficulty` 1: HARD).
pub const DEFAULT_DIFFICULTY: u32 = 1;

impl CreateForm {
    /// The popup as it opens: Elimination (bot Elimination on the single
    /// tab), the mode's defaults, a default title picked by `now_ms`.
    pub fn open(data: Arc<LobbyData>, single: bool, now_ms: u64, limit: u32) -> Self {
        let mode = if single { 17 } else { 0 };
        let mut f = Self::with_mode(data, single, mode, limit);
        f.name = f.data.default_title(now_ms);
        f
    }

    /// The popup with `mode` picked first: every field its defaults.
    pub fn with_mode(data: Arc<LobbyData>, single: bool, mode: u16, limit: u32) -> Self {
        let m = data.mode(mode).expect("a mode").clone();
        Self {
            single,
            name: String::new(),
            use_password: false,
            password: String::new(),
            mode,
            map: m.default_map,
            goal: m.goal_default(),
            players: m.recommended().min(limit.min(data::ROOM_PLAYERS_MAX)),
            arms: [m.arms_default, 0],
            difficulty: DEFAULT_DIFFICULTY,
            limit,
            data,
        }
    }

    pub fn data(&self) -> &Arc<LobbyData> {
        &self.data
    }

    /// The single tab's modes (empty: the tab is disabled).
    pub fn modes_single(&self) -> Vec<&ModeRec> {
        self.data.modes_offered(true)
    }

    pub fn mode_rec(&self) -> &ModeRec {
        self.data.mode(self.mode).unwrap()
    }

    /// The Mode dropdown: the tab's modes by sorting order.
    pub fn modes(&self) -> Vec<&ModeRec> {
        self.data.modes_offered(self.single)
    }

    /// The Map dropdown: the mode's maps by name.
    pub fn maps(&self) -> Vec<(u16, String)> {
        self.data.maps_for(self.mode_rec()).into_iter().map(|m| (m.id, m.name.clone())).collect()
    }

    pub fn map_text(&self) -> String {
        self.data.map_name(self.map)
    }

    /// The Goal dropdown: values and their texts. On the single tab
    /// Fireteam's goal is the difficulty.
    pub fn goals(&self) -> Vec<(u32, String)> {
        let m = self.mode_rec();
        m.goal_values().into_iter().map(|g| (g, m.goal_text(g))).collect()
    }

    /// The row's title: "Goal", or "Difficulty" for Fireteam.
    pub fn goal_title(&self) -> &'static str {
        if self.mode_rec().goal_is_difficulty() { "Difficulty" } else { "Goal" }
    }

    /// The Players dropdown: values (capped at the channel's limit) and
    /// texts.
    pub fn player_options(&self) -> Vec<(u32, String)> {
        let m = self.mode_rec();
        m.player_values()
            .into_iter()
            .filter(|p| *p <= self.limit.min(data::ROOM_PLAYERS_MAX))
            .map(|p| (p, m.players_text(p)))
            .collect()
    }

    /// The 1st Arms dropdown (listed options by display order; the cup's
    /// only in its mode).
    pub fn arms_options(&self) -> Vec<(u8, String)> {
        let cup = self.mode_rec().arms_default == data::CUP_ARMS;
        self.data.arms_listed(cup).into_iter().map(|a| (a.id, a.name.clone())).collect()
    }

    /// 1st Arms is enabled when the mode allows a weapon restriction
    /// (else forced to All Permitted).
    pub fn arms_enabled(&self) -> bool {
        !self.single && self.mode_rec().weapon_use
    }

    /// 2nd Arms is disabled offline (it needs a premium item): always "-".
    pub fn second_arms_enabled(&self) -> bool {
        false
    }

    /// Whether the password, 1st and 2nd Arms rows show (hidden on the
    /// single tab).
    pub fn rows_shown(&self) -> bool {
        !self.single
    }

    /// Pick a mode: its default map unless the old map is also its,
    /// its default goal, the old player count if valid and on the new
    /// step grid, else its recommended, and arms reset.
    pub fn set_mode(&mut self, mode: u16) {
        let Some(m) = self.data.mode(mode).cloned() else { return };
        if !(if self.single { m.offered_single() } else { m.offered_multi() }) {
            return;
        }
        let keep_map = self.data.map_open(self.map, &m);
        self.mode = mode;
        if !keep_map {
            self.map = m.default_map;
        }
        self.goal = m.goal_default();
        let valid = m.player_values().contains(&self.players) && self.players <= self.limit.min(data::ROOM_PLAYERS_MAX);
        if !valid {
            self.players = m.recommended().min(self.limit.min(data::ROOM_PLAYERS_MAX));
        }
        self.arms = [m.arms_default, 0];
    }

    pub fn set_map(&mut self, map: u16) {
        if self.data.map_open(map, self.mode_rec()) {
            self.map = map;
        }
    }

    pub fn set_goal(&mut self, goal: u32) {
        if self.mode_rec().goal_values().contains(&goal) {
            self.goal = goal;
        }
    }

    pub fn set_players(&mut self, n: u32) {
        if self.player_options().iter().any(|(p, _)| *p == n) {
            self.players = n;
        }
    }

    pub fn set_arms(&mut self, a: u8) {
        if self.arms_enabled() && self.arms_options().iter().any(|(id, _)| *id == a) {
            self.arms = [a, 0];
        }
    }

    /// OK (C step 1 and 2): the name must not be empty or only spaces and,
    /// unless it is still the offered default title, pass the word filter;
    /// the password is empty with the box unchecked (and always on the
    /// single tab). `bad` says whether the filter rejects a name.
    pub fn submit(&self, bad: &dyn Fn(&str) -> bool) -> Result<CreateSpec, Refusal> {
        if self.name.trim().is_empty() {
            return Err(Refusal::NameEmpty);
        }
        let is_default = self.data.titles.iter().any(|t| *t == self.name);
        if !is_default && bad(&self.name) {
            return Err(Refusal::NameBad);
        }
        let password = if self.use_password && !self.single { self.password.clone() } else { String::new() };
        Ok(CreateSpec {
            name: self.name.clone(),
            password,
            mode: self.mode,
            pure_combat: false,
            map: Some(self.map),
            goal: self.goal,
            players: self.players,
            arms: self.arms,
        })
    }
}

/// The players that count as ready on each team (the host counts as
/// ready on their own).
pub fn ready_counts(room: &RoomDetail) -> (u32, u32) {
    let n = |team: Team| {
        room.seats
            .iter()
            .filter(|s| s.team == team && (s.state == PlayerState::Ready || s.id == room.host))
            .count() as u32
    };
    (n(Team::Alpha), n(Team::Bravo))
}

/// S, the client's rule for START (before the arms and abilities checks):
/// the count of ready players the mode needs.
pub fn start_check(data: &LobbyData, room: &RoomDetail) -> Result<(), Refusal> {
    let mode = data.mode(room.settings.mode).expect("a mode");
    let (a, b) = ready_counts(room);
    let total = a + b;
    let ok = match mode.key.as_str() {
        "FFA" | "LMS" | "VWRM" => total >= 2,
        "HG" => total >= 3,
        "BNT" | "COOP" | "QRT" => total >= 4,
        _ => {
            let each = mode.min_players() / 2;
            a >= each && b >= each
        }
    };
    if ok { Ok(()) } else { Err(Refusal::NotEnoughPlayers) }
}

/// T: the client's refusal to send a team switch: one-team modes (the
/// button is disabled), and in a match in progress unless the player's
/// team without them has at least as many players as the other.
pub fn switch_check(data: &LobbyData, room: &RoomDetail, me: PlayerId) -> Result<(), Refusal> {
    let mode = data.mode(room.settings.mode).expect("a mode");
    if mode.one_team {
        return Err(Refusal::NoTeamChange);
    }
    let Some(seat) = room.seat(me) else { return Err(Refusal::NotInRoom) };
    if room.state == RoomState::InGame {
        let own = room.team_count(seat.team) as i64 - 1;
        let other = room.team_count(seat.team.other()) as i64;
        if own < other {
            return Err(Refusal::TeamImbalance);
        }
    }
    Ok(())
}

/// The button in the READY / START place (R, "Who sees which button").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Primary {
    /// READY (non-host, the room waiting); its text turns red while ready.
    Ready,
    /// START: the host's, or a non-host joining a match in progress.
    Start,
}

pub fn primary_button(room: &RoomDetail, me: PlayerId) -> Primary {
    if room.host == me {
        return Primary::Start;
    }
    match room.state {
        RoomState::Waiting | RoomState::Starting => Primary::Ready,
        RoomState::InGame | RoomState::Closing => Primary::Start,
    }
}

/// O: before a mode change the client refuses it when the mode's max
/// players is below the number of players in the room.
pub fn mode_change_check(data: &LobbyData, room: &RoomDetail, new: u16) -> Result<(), Refusal> {
    let m = data.mode(new).ok_or(Refusal::ModeNoRules)?;
    if room.seats.len() as u32 > m.max_players() {
        return Err(Refusal::TooManyPlayers {
            mode: m.name.clone(),
            max: m.max_players(),
        });
    }
    Ok(())
}

/// Which option controls the host can use (O): the mode's flags, and only
/// the host in a waiting room. Returns, in order, Join Mid Combat, Friendly
/// Fire, Kill Cam, Abilities, Super Soldiers, 1st Arms (Swap Teams is
/// never).
pub fn option_enabled(data: &LobbyData, room: &RoomDetail, me: PlayerId) -> [bool; 6] {
    let m = data.mode(room.settings.mode).expect("a mode");
    let host = room.host == me && room.state == RoomState::Waiting;
    let pure = room.settings.pure_combat;
    [
        host && m.join_combat,
        host && m.team_kill && !pure,
        host && m.view_enemy && !pure,
        host && m.perk && !pure,
        host && m.mutation,
        host && m.weapon_use,
    ]
}

/// What `defaults` gives a mode (re-exported for the screens).
pub fn mode_defaults(m: &ModeRec) -> Settings {
    defaults(m, false)
}
