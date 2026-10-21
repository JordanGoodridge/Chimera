//! The lobby's vocabulary (specs/combat_arms/lobby.md M, "Events"): the
//! rooms and seats, the requests a client makes, the notices the lobby
//! sends back, the refusals with the texts the screens show, and the
//! events the lobby hands to whatever launches the match.

use serde::{Deserialize, Serialize};

/// A player of the lobby (the transport's id for them; 0: the process's
/// own local player).
pub type PlayerId = u64;
/// A room's id, 0-99 (the list shows id + 1).
pub type RoomId = u16;

/// A room team (T): Alpha is `JSF` (team 1) in the match, Bravo `Creed`
/// (team 2).
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Team {
    Alpha,
    Bravo,
}

impl Team {
    pub fn other(self) -> Self {
        match self {
            Team::Alpha => Team::Bravo,
            Team::Bravo => Team::Alpha,
        }
    }

    /// The match's team id.
    pub fn match_team(self) -> u8 {
        match self {
            Team::Alpha => 1,
            Team::Bravo => 2,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Team::Alpha => "Alpha",
            Team::Bravo => "Bravo",
        }
    }
}

/// A player's state in a room (our names; "Player states").
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum PlayerState {
    Waiting,
    Ready,
    /// Picked for a match that is counting down.
    Starting,
    InGame,
}

impl PlayerState {
    /// The status column's text ("Ready", "Starting", "In game").
    pub fn text(self) -> &'static str {
        match self {
            PlayerState::Waiting => "",
            PlayerState::Ready => "Ready",
            PlayerState::Starting => "Starting",
            PlayerState::InGame => "In game",
        }
    }
}

/// The extra state of a waiting player.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Extra {
    None,
    /// In the inventory, shop, clan or jobs screen.
    Outfitting,
    /// No input for 60 s.
    Idle,
}

/// A room's state.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum RoomState {
    Waiting,
    Starting,
    InGame,
    /// The game server said the match is near its end.
    Closing,
}

impl RoomState {
    /// The room list's text ("Closing" also shows "In game").
    pub fn text(self) -> &'static str {
        match self {
            RoomState::Waiting => "Waiting",
            RoomState::Starting => "Starting",
            RoomState::InGame | RoomState::Closing => "In game",
        }
    }
}

/// A room's settings (R, O).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Settings {
    pub name: String,
    /// Empty: public.
    pub password: String,
    pub mode: u16,
    pub pure_combat: bool,
    /// A map id, or `RANDOM_MAP`.
    pub map: u16,
    pub goal: u32,
    pub max_players: u32,
    /// 1st and 2nd Arms.
    pub arms: [u8; 2],
    pub join_mid: bool,
    /// Display only (the control is dead, quirk 7).
    pub swap_display: bool,
    pub friendly_fire: bool,
    pub kill_cam: bool,
    pub abilities: bool,
    pub super_soldiers: bool,
}

/// A player in a room, as everyone in it sees them.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Seat {
    pub id: PlayerId,
    pub name: String,
    pub team: Team,
    pub state: PlayerState,
    pub extra: Extra,
    /// Join order (the host change walks it).
    pub order: u64,
}

/// A room as the room list shows it (L).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct RoomSummary {
    pub id: RoomId,
    pub name: String,
    pub locked: bool,
    pub map: u16,
    pub mode: u16,
    pub pure_combat: bool,
    pub super_soldiers: bool,
    pub arms: [u8; 2],
    pub users: u32,
    pub max: u32,
    pub state: RoomState,
    pub join_mid: bool,
    /// The host's Elite Moderator level (0: none; such rooms float to the
    /// top).
    pub moderator: u8,
}

impl RoomSummary {
    /// The client's test for the grey style and the Joinable filter (L):
    /// not full, not closing, and if starting or in game, Join Mid
    /// Combat on.
    pub fn can_enter(&self) -> bool {
        self.users < self.max
            && self.state != RoomState::Closing
            && (self.state == RoomState::Waiting || self.join_mid)
    }
}

/// A room as its members see it (the room-settings and player-list reply).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct RoomDetail {
    pub id: RoomId,
    pub settings: Settings,
    pub host: PlayerId,
    pub state: RoomState,
    pub seats: Vec<Seat>,
}

impl RoomDetail {
    pub fn seat(&self, id: PlayerId) -> Option<&Seat> {
        self.seats.iter().find(|s| s.id == id)
    }

    pub fn team_count(&self, team: Team) -> usize {
        self.seats.iter().filter(|s| s.team == team).count()
    }
}

/// What a create-room request carries (C).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct CreateSpec {
    pub name: String,
    pub password: String,
    pub mode: u16,
    pub pure_combat: bool,
    /// None: the mode's default map; `RANDOM_MAP`: a random open one.
    pub map: Option<u16>,
    pub goal: u32,
    pub players: u32,
    pub arms: [u8; 2],
}

/// One room setting a change-setting request carries (O).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub enum Setting {
    Mode(u16),
    Map(u16),
    Goal(u32),
    Players(u32),
    Arms(u8, u8),
    JoinMid(bool),
    FriendlyFire(bool),
    KillCam(bool),
    Abilities(bool),
    SuperSoldiers(bool),
    Name(String),
    Password(String),
}

/// Why a client goes back to the room (X, S step 6).
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum ReturnReason {
    Normal,
    Timeout,
    Failure,
}

/// A request a client makes (M).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub enum Request {
    /// Enter the channel under a name (the local profile's).
    Hello { name: String },
    /// The room list request.
    ListRooms,
    Create(CreateSpec),
    Enter { room: RoomId, password: String },
    Leave,
    Change { mode: u16, what: Setting },
    SetTeam(Team),
    Ready(bool),
    /// START: the host's, or a waiting player joining a match.
    Start,
    Extra(Extra),
    EntrustHost(PlayerId),
    Kick { target: PlayerId, reason: u8 },
    Invite,
    Chat(String),
    ReturnToRoom(ReturnReason),
    /// The loadout (both sets), stored by the lobby and checked again.
    Loadout(Box<super::loadout::Loadout>),
}

/// Who a notice goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Audience {
    Player(PlayerId),
    /// The members of a room.
    Room(RoomId),
    /// A room's members but one.
    RoomExcept(RoomId, PlayerId),
    /// Everyone in the channel (the room list updates).
    Channel,
    /// The channel's players who are not in a room (lobby chat).
    Lobby,
}

/// A notice the lobby pushes (M).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub enum Notice {
    /// The reply to `Hello`: your id in the lobby.
    Welcome(PlayerId),
    /// The room list reply.
    RoomList(Vec<RoomSummary>),
    RoomCreated(RoomSummary),
    RoomDestroyed(RoomId),
    RoomChanged(RoomSummary),
    /// The reply to entering or creating: the room's settings and players.
    Entered(RoomDetail),
    /// The player left the room (a kicked one's exit says so).
    Left(PlayerId),
    /// You left (or were removed from) the room.
    LeftRoom { kicked: bool },
    PlayerEntered(Seat),
    /// The settings changed (the new ones whole).
    Settings(Settings),
    Team(PlayerId, Team),
    State(PlayerId, PlayerState, Extra),
    HostChanged(PlayerId),
    RoomStateChanged(RoomState),
    /// A countdown message: the counter, 0-4.
    Countdown(u32),
    /// The match is ready: connect to the address.
    Go { address: String },
    /// A system line for the room chat.
    System(String),
    Chat { from: String, text: String },
    Invite { from: String, room: RoomId, password: String },
    /// Items of the player's expired (the service's clock, checked at
    /// login and after a match).
    ItemsExpired(Vec<i32>),
    /// The request was refused.
    Refused(Refusal),
}

/// Why the lobby (or the client's own check) refuses.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub enum Refusal {
    NotInLobby,
    AlreadyInRoom,
    NotInRoom,
    NoRoom,
    Closing,
    WrongPassword,
    Full,
    NoMidCombat,
    Kicked,
    CannotEnter,
    NameEmpty,
    NameBad,
    ModeNoRules,
    PureCombat,
    MapNotOpen,
    Players,
    NoFreeRoom,
    FireteamLimit,
    NotHost,
    NotWaiting,
    Stale,
    OptionRefused,
    NoChange,
    BadValue,
    TooManyPlayers { mode: String, max: u32 },
    TeamSwitch,
    TeamImbalance,
    NoTeamChange,
    NotEnoughPlayers,
    FinalRound,
    NoModerator,
    BadTarget,
    HostCannotReady,
    Loadout { option: String },
}

impl Refusal {
    /// The text the screens show (US string table; "…" where the spec
    /// elides the rest).
    pub fn text(&self) -> String {
        match self {
            Refusal::WrongPassword => "You enter the wrong password,please retry.".into(),
            Refusal::Kicked => "You have been kicked from this room. Please select another room to enter.".into(),
            Refusal::CannotEnter | Refusal::Full | Refusal::NoMidCombat | Refusal::Closing | Refusal::NoRoom => {
                "You cannot enter the room that you have currently selected.".into()
            }
            Refusal::NameEmpty => "Please enter the room name.".into(),
            Refusal::NameBad => "The room name is inappropriate or invalid. Please choose another.".into(),
            Refusal::NoFreeRoom => "You cannot create any more rooms. Please use another channel.".into(),
            Refusal::FireteamLimit => "This channel has reached capacity for Fireteam games.".into(),
            Refusal::TooManyPlayers { mode, max } => {
                format!("You have too many players to change the Mode to {mode} (Max: {max} players)")
            }
            Refusal::TeamImbalance => "Unable to switch teams due to team imbalance.".into(),
            // Quirk 5: a message box whose text is a single space.
            Refusal::TeamSwitch => " ".into(),
            Refusal::NotEnoughPlayers => "There aren't enough players to start the game.".into(),
            Refusal::FinalRound => "Please wait... the game is currently in the final round or is just ending.".into(),
            Refusal::Loadout { option } => option.clone(),
            other => format!("{other:?}"),
        }
    }
}

/// What the lobby asks of whatever runs the matches (S, J), in order.
#[derive(Clone, Debug, PartialEq)]
pub enum LobbyEvent {
    /// Start a game server with the room's settings; the players picked.
    Launch {
        room: RoomId,
        settings: Settings,
        players: Vec<(PlayerId, Team)>,
    },
    /// Stop the game server (the launch timed out).
    StopLauncher(RoomId),
    /// Expect a player (start, or a join in progress) on a team.
    Expect {
        room: RoomId,
        player: PlayerId,
        team: Team,
        /// The weapons (item numbers) the game server builds the
        /// inventory from: the loadout set the room uses, filtered by
        /// the room's arms options.
        weapons: Vec<i32>,
    },
    /// A player left the room (the match may have to drop them).
    Dropped { room: RoomId, player: PlayerId },
}
