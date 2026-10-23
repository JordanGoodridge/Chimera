//! The lobby as a client keeps it (specs/combat_arms/lobby.md L, R, S, K,
//! M): the room list, the room it is in, the lines of the room chat, and
//! what the notices it receives do to them, with the client's own timers
//! (the start timeout, the idle clock). A pure state machine the screens
//! read and the net layer feeds.

use std::sync::Arc;

use super::{
    check,
    data::{self, LobbyData},
    list::RoomListView,
    room::*,
};

/// The text of a countdown notice (S step 2): "Game starting in... N
/// seconds." (N = 5 - counter; "second" for 1).
pub fn countdown_text(counter: u32) -> String {
    let n = data::COUNTDOWN_TICKS.saturating_sub(counter);
    format!("Game starting in... {n} {}.", if n == 1 { "second" } else { "seconds" })
}

/// A line of the room chat.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatLine {
    pub text: String,
    /// A system line (red): the countdown, mode and arms changes.
    pub system: bool,
}

/// The most room chat lines kept.
pub const CHAT_KEPT: usize = 60;

/// What the client knows of the lobby.
#[derive(Clone, Debug)]
pub struct ClientLobby {
    pub data: Arc<LobbyData>,
    pub me: PlayerId,
    pub name: String,
    pub list: RoomListView,
    /// The room the player is in.
    pub room: Option<RoomDetail>,
    pub chat: Vec<ChatLine>,
    /// The message box a refusal or a failure opens.
    pub message: Option<String>,
    /// The last countdown counter shown (the buttons stay disabled while
    /// the room starts).
    pub countdown: Option<u32>,
    /// The start timeout: 15000 ms after the last countdown notice; kept
    /// when the room returns to waiting (quirk 14).
    deadline: Option<f64>,
    /// "Go": the game server's address, until the screen has taken it.
    pub go: Option<String>,
    /// An invite to a room.
    pub invite: Option<(String, RoomId, String)>,
    /// The start/ready buttons are enabled again (a host change during a
    /// countdown does that, quirk 10).
    pub buttons_enabled: bool,
    /// Requests to send, in order.
    pub outbox: Vec<Request>,
    /// Expired item numbers not yet shown (the notice on the next visit to
    /// the waiting room).
    pub expired: Vec<i32>,
    /// The result screen is open since this time (ms).
    pub result_since: Option<f64>,
    /// The lobby answered (the server runs one).
    pub active: bool,
    /// `Hello` has been sent.
    pub hello_sent: bool,
    /// READY: the flag the player set and the lobby has not answered yet;
    /// further presses are ignored until it does (corrections 2026-10-22,
    /// 3).
    ready_pending: Option<bool>,
    last_input: f64,
    idle: bool,
}

impl ClientLobby {
    pub fn new(data: Arc<LobbyData>, me: PlayerId, name: &str) -> Self {
        Self {
            list: RoomListView::new(data.clone()),
            data,
            me,
            name: name.to_string(),
            room: None,
            chat: Vec::new(),
            message: None,
            countdown: None,
            deadline: None,
            go: None,
            invite: None,
            buttons_enabled: true,
            outbox: Vec::new(),
            expired: Vec::new(),
            result_since: None,
            active: false,
            hello_sent: false,
            ready_pending: None,
            last_input: 0.0,
            idle: false,
        }
    }

    pub fn send(&mut self, r: Request) {
        self.outbox.push(r);
    }

    pub fn take_outbox(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.outbox)
    }

    /// The client's seat in the room it is in.
    pub fn seat(&self) -> Option<&Seat> {
        self.room.as_ref().and_then(|r| r.seat(self.me))
    }

    /// The player's ready flag: the one just set while the lobby has not
    /// answered, else the seat's.
    pub fn is_ready(&self) -> bool {
        self.ready_pending.unwrap_or_else(|| self.seat().is_some_and(|s| s.state == PlayerState::Ready))
    }

    pub fn is_host(&self) -> bool {
        self.room.as_ref().is_some_and(|r| r.host == self.me)
    }

    fn line(&mut self, text: impl Into<String>, system: bool) {
        self.chat.push(ChatLine { text: text.into(), system });
        let over = self.chat.len().saturating_sub(CHAT_KEPT);
        self.chat.drain(..over);
    }

    /// A notice arrives at `now_ms` on the client's clock.
    pub fn apply(&mut self, n: Notice, now_ms: f64) {
        match n {
            Notice::Welcome(id) => {
                self.me = id;
                self.active = true;
            }
            Notice::RoomList(l) => self.list.set_all(l),
            Notice::RoomCreated(r) => self.list.created(r),
            Notice::RoomDestroyed(id) => self.list.destroyed(id),
            Notice::RoomChanged(r) => self.list.changed(r),
            Notice::Entered(d) => {
                self.ready_pending = None;
                self.room = Some(d);
                self.chat.clear();
                self.countdown = None;
                self.buttons_enabled = true;
                self.last_input = now_ms;
                self.idle = false;
            }
            Notice::Left(p) => {
                if let Some(r) = &mut self.room {
                    r.seats.retain(|s| s.id != p);
                }
            }
            Notice::LeftRoom { kicked } => {
                self.ready_pending = None;
                self.room = None;
                self.countdown = None;
                if kicked {
                    self.message = Some(Refusal::Kicked.text());
                }
            }
            Notice::PlayerEntered(seat) => {
                if let Some(r) = &mut self.room {
                    r.seats.retain(|s| s.id != seat.id);
                    r.seats.push(seat);
                }
            }
            Notice::Settings(s) => {
                if let Some(r) = &mut self.room {
                    r.settings = s;
                }
            }
            Notice::Team(p, t) => {
                if let Some(s) = self.room.as_mut().and_then(|r| r.seats.iter_mut().find(|s| s.id == p)) {
                    s.team = t;
                }
            }
            Notice::State(p, state, extra) => {
                if p == self.me {
                    self.ready_pending = None;
                }
                if let Some(s) = self.room.as_mut().and_then(|r| r.seats.iter_mut().find(|s| s.id == p)) {
                    s.state = state;
                    s.extra = extra;
                }
            }
            Notice::HostChanged(p) => {
                // Quirk 10: a client that was counting down prints
                // "Cancel..." and enables its buttons, though the lobby
                // keeps counting.
                let counting = self.room.as_ref().is_some_and(|r| r.state == RoomState::Starting);
                if let Some(r) = &mut self.room {
                    r.host = p;
                }
                if counting && self.countdown.is_some() {
                    self.line("Cancel...", true);
                    self.buttons_enabled = true;
                }
            }
            Notice::RoomStateChanged(s) => {
                if let Some(r) = &mut self.room {
                    r.state = s;
                }
                if s == RoomState::Starting {
                    self.buttons_enabled = false;
                }
                if s == RoomState::Waiting || s == RoomState::InGame {
                    self.countdown = None;
                    self.buttons_enabled = true;
                }
            }
            Notice::Countdown(c) => {
                self.countdown = Some(c);
                self.line(countdown_text(c), true);
                if c + 1 >= data::COUNTDOWN_TICKS {
                    self.deadline = Some(now_ms + data::CLIENT_GO_TIMEOUT_MS);
                }
            }
            Notice::Go { address } => {
                self.deadline = None;
                self.go = Some(address);
            }
            Notice::System(t) => self.line(t, true),
            Notice::Chat { from, text } => self.line(format!("{from} : {text}"), false),
            Notice::Invite { from, room, password } => self.invite = Some((from, room, password)),
            Notice::ItemsExpired(items) => self.expired.extend(items),
            Notice::Refused(r) => {
                // A refused request turns the flag back (the seat's state).
                self.ready_pending = None;
                self.message = Some(r.text());
            }
        }
    }

    /// The client's clocks at `now_ms`: the start timeout, which shows
    /// "Failed to join the battle. Connection timed out." and returns to
    /// the room with a timeout reason; and the idle clock (60 s without
    /// input).
    pub fn tick(&mut self, now_ms: f64) {
        if let Some(d) = self.deadline
            && now_ms >= d
        {
            self.deadline = None;
            self.message = Some("Failed to join the battle. Connection timed out.".into());
            self.buttons_enabled = true;
            self.send(Request::ReturnToRoom(ReturnReason::Timeout));
        }
        if self.room.is_some() && !self.idle && now_ms - self.last_input >= data::IDLE_AFTER_MS {
            self.idle = true;
            self.send(Request::Extra(Extra::Idle));
        }
    }

    /// The result screen opens at `now_ms` (G): it returns to the room on
    /// its own after 60 s, or at Exit.
    pub fn show_result(&mut self, now_ms: f64) {
        self.result_since = Some(now_ms);
    }

    /// The result screen's Exit button (or the 60 s): back to the room.
    /// True when the screen was open.
    pub fn exit_result(&mut self, now_ms: f64) -> bool {
        let open = self.result_since.take().is_some();
        self.last_input = now_ms;
        open
    }

    /// Whether the result screen has shown for its time (60000 ms).
    pub fn result_timed_out(&self, now_ms: f64) -> bool {
        self.result_since.is_some_and(|t| now_ms - t >= data::RESULT_TIMEOUT_MS)
    }

    /// Leave the match (X): EXIT, Yes. The client tells the game server it
    /// leaves, disconnects (the process's own match: nothing to drop),
    /// sends return-to-room (normal) and shows the waiting room.
    pub fn leave_match(&mut self) {
        self.go = None;
        self.send(Request::ReturnToRoom(ReturnReason::Normal));
    }

    /// Input in the waiting room: the idle clock restarts (and an idle
    /// player says so no more).
    pub fn input(&mut self, now_ms: f64) {
        self.last_input = now_ms;
        if self.idle {
            self.idle = false;
            self.send(Request::Extra(Extra::None));
        }
    }

    // --- What the buttons do.

    /// JOIN (or a double-click): the room to enter, a password to ask
    /// for, or the message to show.
    pub fn join(&mut self, id: Option<RoomId>) -> Option<super::list::Joining> {
        match self.list.join(id) {
            Ok(j) => Some(j),
            Err(m) => {
                self.message = Some(m);
                None
            }
        }
    }

    /// The password box's OK.
    pub fn enter(&mut self, room: RoomId, password: &str) {
        self.send(Request::Enter {
            room,
            password: password.to_string(),
        });
    }

    /// READY (non-host) flips the player's ready flag at once and sends the
    /// matching on or off; the loadout checks (`check`, the caller runs
    /// `loadout::ready_check`) run on every press, also when un-readying;
    /// presses are ignored until the lobby answers the request.
    pub fn ready(&mut self, check: Result<(), String>) {
        if self.seat().is_none() || self.ready_pending.is_some() {
            return;
        }
        if let Err(m) = check {
            self.message = Some(m);
            return;
        }
        let on = !self.is_ready();
        self.ready_pending = Some(on);
        self.send(Request::Ready(on));
    }

    /// START: while the room is *waiting* the host's starts it (the player
    /// count check, then the loadout's); while it is starting, in game or
    /// closing, START from anyone, the host included, is a join of the
    /// match in progress, with the loadout's checks (corrections
    /// 2026-10-22, 1 and 2).
    pub fn start(&mut self, check: Result<(), String>) {
        let Some(room) = self.room.clone() else { return };
        if room.state == RoomState::Waiting && room.host == self.me {
            if let Err(r) = check::start_check(&self.data, &room) {
                self.message = Some(r.text());
                return;
            }
        }
        if let Err(m) = check {
            self.message = Some(m);
            return;
        }
        self.send(Request::Start);
    }

    /// F5: START when the room is not waiting or for the host, READY for
    /// the others.
    pub fn f5(&mut self, check: Result<(), String>) {
        let waiting = self.room.as_ref().is_some_and(|r| r.state == RoomState::Waiting);
        if self.is_host() || !waiting { self.start(check) } else { self.ready(check) }
    }

    /// A team button or the arrows (`team`), or the toggle (`None`).
    pub fn change_team(&mut self, team: Option<Team>) {
        let Some(room) = self.room.clone() else { return };
        if let Err(r) = check::switch_check(&self.data, &room, self.me) {
            if r != Refusal::NoTeamChange {
                self.message = Some(r.text());
            }
            return;
        }
        let Some(seat) = self.seat() else { return };
        let to = team.unwrap_or(seat.team.other());
        if to != seat.team {
            self.send(Request::SetTeam(to));
        }
    }

    /// A setting the host changes: the client's own check first for the
    /// mode.
    pub fn change(&mut self, what: Setting) {
        let Some(room) = self.room.clone() else { return };
        if let Setting::Mode(m) = &what
            && let Err(r) = check::mode_change_check(&self.data, &room, *m)
        {
            self.message = Some(r.text());
            return;
        }
        self.send(Request::Change {
            mode: room.settings.mode,
            what,
        });
    }

    /// The inventory or shop button in the waiting room: a ready player
    /// is un-readied first, then the player is outfitting.
    pub fn open_outfitting(&mut self) {
        if self.seat().is_some_and(|s| s.state == PlayerState::Ready) {
            self.send(Request::Ready(false));
        }
        if self.room.is_some() {
            self.send(Request::Extra(Extra::Outfitting));
        }
    }

    /// Back from an outfitting screen: extra state none.
    pub fn close_outfitting(&mut self) {
        if self.room.is_some() {
            self.send(Request::Extra(Extra::None));
        }
    }

    /// The room chat: a line of at most 120 characters goes to the lobby.
    pub fn say(&mut self, text: &str) {
        let text: String = text.chars().take(data::CHAT_MAX).collect();
        if !text.is_empty() {
            self.send(Request::Chat(text));
        }
    }
}
