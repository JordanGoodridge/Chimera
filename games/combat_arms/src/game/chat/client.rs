//! The local player's side of the chat (specs/combat_arms/radio_chat.md
//! C1 to C3, W1, W2, K1, T2, T3 and the radio's R2): the input bar and its
//! tab, what sending a line does in order, the text commands, whispers
//! and clan lines in and out, the block list and the radio menus. A state
//! machine on an explicit clock (ms): its methods return what has to go
//! out (`Out`); the lines it shows go to `log`.

use super::{
    limits::{ClientLimits, Region, WhisperLimits, WhisperRefusal},
    radio::{self, RadioData, RadioSend},
    show::{ChatLog, PanelLine, SAME_SENTENCE_LIFE, LINE_LIFE, colours},
    text::{BadWords, BlockList, Cmd, Commands},
    voice::VoiceKeys,
};

/// The in-match input box takes this many characters.
pub const INPUT_MAX: usize = 119;
/// What an auto-reply starts with (a string-table prefix; ours).
pub const AUTO_REPLY_TITLE: &str = "[Auto-reply]";
/// The GM admin commands a line starting `/` is offered to.
pub const ADMIN_COMMANDS: [&str; 12] = [
    "kick", "ban", "unban", "hwban", "hwunban", "trace", "join", "punish", "unpunish", "restrict", "unrestrict",
    "userscreen",
];

/// The input bar's tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    All,
    Team,
    Clan,
}

/// Keys the open input handles itself (C2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKey {
    Esc,
    Enter,
    Up,
    /// F2, F3, F4.
    Tab(Tab),
    /// Page Up, Page Down, Down: swallowed.
    Swallowed,
}

/// What a send puts on the wire or hands to a service.
#[derive(Clone, Debug, PartialEq)]
pub enum Out {
    /// All or team chat to the game server.
    Say { team: bool, text: String },
    /// A whisper to the lobby service.
    Whisper { to: String, body: String },
    /// A line to the clan chat room.
    Clan { text: String },
    /// A GM admin command (`/kick ...`).
    Admin { line: String },
    /// A radio message to the game server.
    Radio(RadioSend),
}

/// What the chat needs of the world when a key is pressed.
#[derive(Clone, Debug, PartialEq)]
pub struct Env {
    /// The cursor is grabbed: the player is in the game.
    pub playing: bool,
    pub fading: bool,
    /// The local team is "nothing".
    pub team_nothing: bool,
    /// The HUD is hidden (its on/off bind), or the referee's HUD.
    pub hud_hidden: bool,
    pub alive: bool,
    pub hud_built: bool,
    /// The respawn weapon-change prompt is mid-way, or a vote-kick
    /// request or vote is on screen.
    pub weapon_prompt: bool,
    pub vote_on: bool,
    /// The player's own name.
    pub own: String,
    /// Last Man Standing and the HUD in its dead state.
    pub lms_dead: bool,
}

impl Default for Env {
    fn default() -> Self {
        Self {
            playing: true,
            fading: false,
            team_nothing: false,
            hud_hidden: false,
            alive: true,
            hud_built: true,
            weapon_prompt: false,
            vote_on: false,
            own: String::new(),
            lms_dead: false,
        }
    }
}

/// The account's options and the data the chat reads.
#[derive(Clone, Debug)]
pub struct Settings {
    /// The bad-word filter on for what this client receives.
    pub filter_on: bool,
    /// "Accept whispers" (saved with the lobby).
    pub accept_whispers: bool,
    pub auto_reply: String,
    pub clan_name: String,
    pub gm: bool,
    /// Remaining chat punishment, minutes.
    pub punished_minutes: u32,
    pub commands: Commands,
    pub words: BadWords,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            filter_on: false,
            accept_whispers: true,
            auto_reply: String::new(),
            clan_name: String::new(),
            gm: false,
            punished_minutes: 0,
            commands: Commands::builtin(),
            words: BadWords::default(),
        }
    }
}

/// The local player's chat.
#[derive(Clone, Debug, Default)]
pub struct ChatClient {
    pub settings: Settings,
    /// The tab the input opens on; persists, All on entering the world.
    pub tab: Tab,
    /// The open input's text; None: closed.
    pub input: Option<String>,
    pub limits: ClientLimits,
    pub whisper: WhisperLimits,
    pub block: BlockList,
    pub log: ChatLog,
    /// The radio menu open (a class), and when the last radio went out.
    pub radio_open: Option<u8>,
    pub last_radio: Option<f64>,
    /// The voice set's index kept between sends (R3 step 2).
    pub voice_index: i32,
    /// The Enter that sent a line must not reopen the input this frame.
    pub sent_this_frame: bool,
    /// Push-to-talk and the voice output option.
    pub voice: VoiceKeys,
}

impl ChatClient {
    pub fn new(region: Region) -> Self {
        Self {
            limits: ClientLimits::new(region),
            whisper: WhisperLimits::new(region),
            ..Default::default()
        }
    }

    /// Entering the world: the tab is All, nothing open.
    pub fn enter_world(&mut self) {
        self.tab = Tab::All;
        self.input = None;
        self.radio_open = None;
    }

    /// The end of a rendered frame: the "sent this frame" flag resets
    /// once the input is closed.
    pub fn end_frame(&mut self) {
        if self.input.is_none() {
            self.sent_this_frame = false;
        }
    }

    pub fn is_open(&self) -> bool {
        self.input.is_some()
    }

    // --- Lines the client shows itself.

    fn system(&mut self, text: impl Into<String>, colour: [u8; 4], life: f64, now: f64) {
        self.log.push(PanelLine::new(text, colour, now, life));
    }

    /// A red reply or refusal, shown only if longer than one character.
    fn reply(&mut self, text: impl Into<String>, now: f64) {
        let text = text.into();
        if text.chars().count() > 1 {
            self.system(text, colours::SYSTEM, LINE_LIFE, now);
        }
    }

    // --- C2: opening.

    /// Enter: opens on the current tab while playing, not during a fade,
    /// with a team, not if already open and not on the frame a line was
    /// sent. Nothing opens with the HUD hidden.
    pub fn open_with_enter(&mut self, env: &Env) -> bool {
        if !env.playing
            || env.fading
            || env.team_nothing
            || env.hud_hidden
            || self.is_open()
            || self.sent_this_frame
        {
            return false;
        }
        self.input = Some(String::new());
        self.radio_open = None;
        true
    }

    /// A channel bind (F2, F3, F4): sets the tab, then opens. A GM
    /// account's binds do nothing.
    pub fn open_with_bind(&mut self, tab: Tab, env: &Env) -> bool {
        if self.settings.gm {
            return false;
        }
        let before = self.tab;
        self.tab = tab;
        if self.open_with_enter(env) {
            true
        } else {
            self.tab = before;
            false
        }
    }

    /// Typing into the open box: at most `INPUT_MAX` characters.
    pub fn type_text(&mut self, s: &str) {
        let Some(text) = &mut self.input else { return };
        for c in s.chars().filter(|c| !c.is_control()) {
            if text.chars().count() >= INPUT_MAX {
                return;
            }
            text.push(c);
        }
    }

    pub fn backspace(&mut self) {
        if let Some(t) = &mut self.input {
            t.pop();
        }
    }

    /// A key the open input handles (everything else is typed).
    pub fn key(&mut self, key: InputKey, env: &Env, now: f64) -> Vec<Out> {
        if !self.is_open() {
            return Vec::new();
        }
        match key {
            InputKey::Esc => {
                self.input = None;
                Vec::new()
            }
            InputKey::Enter => self.submit(env, now),
            InputKey::Up => {
                if let Some(text) = self.whisper.up() {
                    self.input = Some(text);
                }
                Vec::new()
            }
            InputKey::Tab(tab) => {
                self.tab = tab;
                Vec::new()
            }
            InputKey::Swallowed => Vec::new(),
        }
    }

    // --- C3: sending.

    /// Send the typed line (C3, in order); closes the input unless a
    /// whisper was refused (Quirk 6).
    pub fn submit(&mut self, env: &Env, now: f64) -> Vec<Out> {
        let typed = self.input.clone().unwrap_or_default();
        self.input = None;
        self.sent_this_frame = true;
        let mut out = Vec::new();
        // 1 An empty box closes.
        if typed.is_empty() {
            return out;
        }
        // 2 At most 119 characters.
        let text: String = typed.chars().take(INPUT_MAX).collect();
        // 3 Commands.
        let parsed = self.settings.commands.parse(&text).map(|(c, r)| (c, r.to_string()));
        let short = |r: &str| r.chars().count() <= 1;
        enum Route {
            Nothing,
            Tab(String),
            Normal(String),
            Clan(String),
            Whisper(String, String),
        }
        let route = match parsed {
            None => Route::Tab(text.clone()),
            Some((Cmd::WhisperEnable, rem)) if short(&rem) => {
                self.settings.accept_whispers = !self.settings.accept_whispers;
                let on = self.settings.accept_whispers;
                self.reply(if on { "Whispers enabled." } else { "Whispers disabled." }, now);
                Route::Nothing
            }
            // Not settings: the whole line goes out as chat (Quirk 4).
            Some((Cmd::WhisperEnable | Cmd::WhisperIgnore | Cmd::WhisperUnignore, _)) => Route::Tab(text.clone()),
            // A remainder of only spaces is no setting: the whole typed line
            // goes out as chat (corrections 2026-10-22, 3). A Whisper alias
            // followed by only spaces sends nothing, below.
            Some((Cmd::WhisperAutoAnswer | Cmd::Block | Cmd::UnBlock, rem)) if !short(&rem) && rem.trim().is_empty() => {
                Route::Tab(text.clone())
            }
            Some((Cmd::WhisperAutoAnswer, rem)) => {
                if short(&rem) {
                    self.settings.auto_reply.clear();
                    self.reply("Auto-reply reset.", now);
                } else {
                    self.reply(format!("\"{rem}\" is now your auto-reply."), now);
                    self.settings.auto_reply = rem;
                }
                Route::Nothing
            }
            Some((Cmd::Block, rem)) => {
                if short(&rem) {
                    self.reply("Usage: block <name>", now);
                } else {
                    let name = first_word(&rem).0.to_string();
                    if self.block.add(&name, &env.own) {
                        self.reply(format!("\"{name}\" is blocked."), now);
                    }
                }
                Route::Nothing
            }
            Some((Cmd::UnBlock, rem)) => {
                if short(&rem) {
                    self.reply("Usage: unblock <name>", now);
                } else {
                    let name = first_word(&rem).0.to_string();
                    self.block.remove(&name);
                    self.reply(format!("\"{name}\" is unblocked."), now);
                }
                Route::Nothing
            }
            Some((Cmd::Whisper, rem)) => {
                if short(&rem) {
                    Route::Nothing
                } else if !self.settings.accept_whispers {
                    self.reply("You have turned whispers off.", now);
                    Route::Nothing
                } else {
                    let (name, body) = first_word(&rem);
                    if body.is_empty() {
                        Route::Nothing
                    } else {
                        Route::Whisper(name.to_string(), body.to_string())
                    }
                }
            }
            Some((Cmd::Normal, rem)) if !rem.is_empty() => Route::Normal(rem),
            Some((Cmd::Clan, rem)) if !rem.is_empty() => Route::Clan(rem),
            Some(_) => Route::Nothing,
        };
        match route {
            Route::Nothing => {}
            Route::Whisper(to, body) => match self.whisper.try_send(&to, &body, now) {
                Err(why) => {
                    let id = match why {
                        WhisperRefusal::Global => "$BlockWhisper",
                        WhisperRefusal::Recipient => "$UserSpamBlockWhisper",
                    };
                    self.system(id, colours::SYSTEM, LINE_LIFE, now);
                    // The box stays open with its text.
                    self.input = Some(typed);
                    self.sent_this_frame = false;
                }
                Ok(()) => {
                    self.whisper.remember(&to, &env.own);
                    self.system(format!("To \"{to}\" : {body}"), colours::WHISPER_TO, LINE_LIFE, now);
                    out.push(Out::Whisper { to, body });
                }
            },
            Route::Normal(rem) => {
                let team = self.tab == Tab::Team;
                out.extend(self.send_chat(team, rem, env, now));
            }
            Route::Tab(line) => match self.tab {
                Tab::All => out.extend(self.send_chat(false, line, env, now)),
                Tab::Team => out.extend(self.send_chat(true, line, env, now)),
                Tab::Clan => out.extend(self.send_clan(line, now)),
            },
            Route::Clan(rem) => out.extend(self.send_clan(rem, now)),
        }
        out
    }

    /// The GM admin command in `line`, if it is one.
    fn admin(&self, line: &str) -> Option<Out> {
        let rest = line.strip_prefix('/')?;
        let word = rest.split_whitespace().next()?.to_ascii_lowercase();
        (self.settings.gm && ADMIN_COMMANDS.contains(&word.as_str())).then(|| Out::Admin { line: line.to_string() })
    }

    fn punished(&mut self, now: f64) -> bool {
        let m = self.settings.punished_minutes;
        if m == 0 {
            return false;
        }
        let text = if m >= 60 {
            format!("You are muted for {} hours.", m / 60)
        } else {
            "You are muted for less than an hour.".to_string()
        };
        self.system(text, colours::NOTICE, LINE_LIFE, now);
        true
    }

    /// C3 step 5: all or team chat.
    fn send_chat(&mut self, team: bool, text: String, env: &Env, now: f64) -> Option<Out> {
        // 1 Last Man Standing and dead: nothing, no notice.
        if env.lms_dead {
            return None;
        }
        // 2 The repeat lock.
        if self.limits.repeat.refused(now) {
            self.system("$SameSentence", colours::SAME_SENTENCE, SAME_SENTENCE_LIFE, now);
            return None;
        }
        // 3 GM admin commands.
        if let Some(a) = self.admin(&text) {
            return Some(a);
        }
        // 4 Chat punishment.
        if self.punished(now) {
            return None;
        }
        // 5 The rate limit.
        if self.limits.rate_refused(now) {
            self.system("$SpamBlockChat", colours::SYSTEM, LINE_LIFE, now);
            return None;
        }
        // 6 Send; bookkeeping.
        self.limits.rate_take(now);
        self.limits.repeat.sent(&text, now);
        Some(Out::Say { team, text })
    }

    /// C3 step 6: a clan line (no rate or repeat limit).
    fn send_clan(&mut self, text: String, now: f64) -> Option<Out> {
        if let Some(a) = self.admin(&text) {
            return Some(a);
        }
        if self.punished(now) {
            return None;
        }
        Some(Out::Clan { text })
    }

    /// A lobby line: the repeat lock (3 identical lines; GM exempt), the
    /// punishment and the same rate limit as the match. True when it goes
    /// out.
    pub fn lobby_send(&mut self, text: &str, now: f64) -> bool {
        if !self.settings.gm && self.limits.lobby.refused(now) {
            self.system("$SameSentence", colours::SAME_SENTENCE, SAME_SENTENCE_LIFE, now);
            return false;
        }
        if self.punished(now) {
            return false;
        }
        if self.limits.rate_refused(now) {
            self.system("$SpamBlockChat", colours::SYSTEM, LINE_LIFE, now);
            return false;
        }
        self.limits.rate_take(now);
        if !self.settings.gm {
            self.limits.lobby.sent(text, now);
        }
        true
    }

    // --- W2, K1: lines from the online services.

    /// A whisper arrives (W2); the whispers to send back (the auto-reply,
    /// which bypasses the whisper limits).
    pub fn whisper_in(&mut self, from: &str, body: &str, env: &Env, now: f64) -> Vec<Out> {
        // 1 A blocked sender: dropped, no auto-reply.
        if self.block.blocks(from, &env.own) {
            return Vec::new();
        }
        // 2 Filtered by the option: the body, then the whole line again.
        let line = if self.settings.filter_on {
            let b = self.settings.words.filter(body);
            self.settings.words.filter(&format!("From \"{from}\" : {b}"))
        } else {
            format!("From \"{from}\" : {body}")
        };
        self.system(line, colours::WHISPER_FROM, LINE_LIFE, now);
        // 3 The Up-arrow list.
        self.whisper.remember(from, &env.own);
        // 4 The auto-reply.
        let mut out = Vec::new();
        if !self.settings.auto_reply.is_empty() && from != env.own && !body.starts_with(AUTO_REPLY_TITLE) {
            out.push(Out::Whisper {
                to: from.to_string(),
                body: format!("{AUTO_REPLY_TITLE}{}", self.settings.auto_reply),
            });
        }
        out
    }

    /// A clan room line arrives (K1): the block list does not apply.
    pub fn clan_in(&mut self, room_id: &str, text: &str, now: f64) {
        let sender = room_id.split('@').next().unwrap_or(room_id);
        let mut line = format!("[{}]{sender} : {text}", self.settings.clan_name);
        if self.settings.filter_on {
            line = self.settings.words.filter(&line);
        }
        self.system(line, colours::CLAN, LINE_LIFE, now);
    }

    // --- R2, R3: the radio menus.

    /// A radio key (`class`): closes its own menu, else opens (or
    /// switches to) it when allowed. Not alive closes the menu instead.
    pub fn radio_key(&mut self, class: u8, env: &Env, now: f64) {
        if !env.alive {
            self.radio_open = None;
            return;
        }
        if self.radio_open == Some(class) {
            self.radio_open = None;
            return;
        }
        let cooled = self.last_radio.is_none_or(|t| now - t >= radio::COOLDOWN_MS);
        if cooled && env.hud_built && !env.weapon_prompt && !env.vote_on {
            self.radio_open = Some(class);
        }
    }

    /// A number key while a menu is open (`n` 0 to 9): 1 to 9 pick an
    /// entry, 0 does nothing. An entry the menu doesn't have leaves it
    /// open. `voice` is the sender's voice set's name, `at` its position;
    /// `draw(n)` the random line.
    #[allow(clippy::too_many_arguments)]
    pub fn radio_choose(
        &mut self,
        n: usize,
        data: &RadioData,
        voice: Option<&str>,
        at: [f32; 3],
        draw: impl FnOnce(usize) -> usize,
        now: f64,
    ) -> Option<Out> {
        let class = self.radio_open?;
        if n == 0 || n > radio::ENTRIES_SHOWN {
            return None;
        }
        let send = radio::pick(data, class, n - 1, voice, &mut self.voice_index, at, draw)?;
        self.radio_open = None;
        self.last_radio = Some(now);
        Some(Out::Radio(send))
    }

    /// The local player started dying: the menu goes.
    pub fn radio_close(&mut self) {
        self.radio_open = None;
    }
}

/// The first word of `rem` (after leading blanks) and what follows its
/// one separator.
fn first_word(rem: &str) -> (&str, &str) {
    let rem = rem.trim_start();
    match rem.char_indices().find(|(_, c)| c.is_whitespace()) {
        Some((i, c)) => (&rem[..i], &rem[i + c.len_utf8()..]),
        None => (rem, ""),
    }
}
