//! The music's state and rules (music_ambience.md 2-4, 8; section
//! numbers below are the spec's): the player (Chimera's `MusicPlayer`),
//! the menu playlist and its per-frame update, the cues each moment of a
//! match starts, the interface sounds they name and the order of events
//! at phase changes. Pure: every method takes the wall clock in
//! milliseconds, so the tests step a day in a line; `observe` turns the
//! match's facts into the moments.

use std::collections::HashMap;

use bevy::prelude::*;

use super::super::rules::{
    self, Teams,
    state::{ALPHA, BRAVO},
};
use super::{
    data::{MusicData, normalize},
    playlist::Playlist,
};
use crate::music::{MusicPlayer, Stream};

/// Which screen the client shows (the playlist runs on the menu and
/// result screens; a match has cues only).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Screen {
    /// Lobby screens: the room list, room, shop and every other menu (and
    /// the title, where there is no map).
    #[default]
    Menu,
    /// Joining a server: the loading screen.
    Loading,
    /// In a match.
    Match,
    /// The result screen after a match.
    Result,
}

/// The music files the client can open and their lengths (ms; None until
/// known).
#[derive(Resource, Clone, Debug, Default)]
pub struct MusicFiles {
    files: HashMap<String, Option<f64>>,
}

impl MusicFiles {
    /// Every file the built-in table lists, with its length: what the
    /// tests use, and an install that holds them all.
    pub fn from_data(d: &MusicData) -> Self {
        Self {
            files: d.files.iter().map(|(k, v)| (k.clone(), Some(v * 1000.0))).collect(),
        }
    }

    pub fn insert(&mut self, file: &str, length_ms: Option<f64>) {
        self.files.insert(normalize(file), length_ms);
    }

    pub fn remove(&mut self, file: &str) {
        self.files.remove(&normalize(file));
    }

    pub fn exists(&self, file: &str) -> bool {
        self.files.contains_key(&normalize(file))
    }

    pub fn length_ms(&self, file: &str) -> Option<f64> {
        self.files.get(&normalize(file)).copied().flatten()
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

/// The music master (`m`, 8) applied: a stream as the mixer plays it.
#[derive(Clone, Debug, PartialEq)]
pub struct Mixed {
    pub file: String,
    pub looping: bool,
    pub started: f64,
    /// Music master times the slot's gain.
    pub gain: f32,
}

/// The client's music.
#[derive(Resource, Clone, Debug)]
pub struct CaMusic {
    pub data: &'static MusicData,
    pub player: MusicPlayer,
    pub playlist: Playlist,
    pub files: MusicFiles,
    /// The menu layer's "current music" name.
    pub current: String,
    /// The result screen silences the playlist.
    pub paused: bool,
    pub screen: Screen,
    /// Interface sounds started since the bridge last took them (attribute
    /// keys: `RoundStartSound`, ...).
    pub sounds: Vec<String>,
    /// Every interface sound ever started, for the console and tests.
    pub heard: Vec<String>,
    /// The first real round start of this map was seen (its mode music
    /// played or not: it does not start later).
    mode_music_done: bool,
    /// Quarantine's end cue played this round.
    qrt_end_played: bool,
    /// Seize and Secure's cue is on.
    capture_on: bool,
    /// Arms Race: a player is on the last stage.
    last_stage_on: bool,
    /// The mode under way, for the cues that restart its music.
    mode: String,
    rng: u64,
}

impl Default for CaMusic {
    fn default() -> Self {
        Self::new(MusicData::built_in())
    }
}

impl CaMusic {
    pub fn new(data: &'static MusicData) -> Self {
        Self {
            data,
            player: MusicPlayer::default(),
            playlist: Playlist::new(
                &data.version,
                data.playlist.iter().map(|e| (e.name.clone(), e.file.clone())),
            ),
            files: MusicFiles::default(),
            current: String::new(),
            paused: false,
            screen: Screen::Menu,
            sounds: Vec::new(),
            heard: Vec::new(),
            mode_music_done: false,
            qrt_end_played: false,
            capture_on: false,
            last_stage_on: false,
            mode: String::new(),
            rng: 0x9E37_79B9_7F4A_7C15,
        }
    }

    /// With every file of the table present (tests, an install with the
    /// whole folder).
    pub fn with_table_files(mut self) -> Self {
        self.files = MusicFiles::from_data(self.data);
        self
    }

    fn opens(&self, file: &str) -> bool {
        self.files.exists(file)
    }

    // --- The player through the menu layer (3).

    /// A start through the menu layer (the playlist, loading, mode and cue
    /// starts): the name already current is ignored; fade in seconds.
    /// (The seasonal substitution of `clientevent.txt` isn't read.)
    pub fn start(&mut self, file: &str, looping: bool, fade: f64, now: f64) {
        if file.is_empty() || self.current.eq_ignore_ascii_case(file) {
            return;
        }
        self.current = file.to_string();
        let opens = self.opens(file);
        self.player.start(file, looping, fade * 1000.0, now, opens);
    }

    /// A start straight on the player (a map music trigger, 5): the menu
    /// layer's name is bypassed.
    pub fn start_direct(&mut self, file: &str, looping: bool, fade: f64, now: f64) {
        let opens = self.opens(file);
        self.player.start(file, looping, fade * 1000.0, now, opens);
    }

    /// Close both slots and forget the current name.
    pub fn stop(&mut self) {
        self.player.stop();
        self.current.clear();
    }

    /// An interface sound (2D, full gain, the effects channel): its
    /// attribute key.
    pub fn sound(&mut self, key: &str) {
        self.sounds.push(key.to_string());
        self.heard.push(key.to_string());
    }

    /// The next playlist track, fading in over the menu fade.
    fn start_next(&mut self, now: f64) {
        let files = self.files.clone();
        let mut playlist = std::mem::take(&mut self.playlist);
        let mut rng = self.rng;
        let next = playlist.next(&|f| files.exists(f), |n| {
            let mut x = rng;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            rng = x;
            (x % n.max(1) as u64) as usize
        });
        self.playlist = playlist;
        self.rng = rng;
        if let Some((file, looping)) = next {
            self.start(&file, looping, self.data.menu_fade, now);
        }
    }

    /// The menu update (3), once a frame while a menu screen is shown.
    pub fn menu_frame(&mut self, now: f64) {
        if self.paused {
            self.stop();
            return;
        }
        if self.player.playing() {
            let (eligible, near) = {
                let files = &self.files;
                let playable = |f: &str| files.exists(f);
                (
                    self.playlist.cursor_eligible(&playable),
                    self.player.near_end(now, |f| files.length_ms(f)),
                )
            };
            if eligible {
                if near {
                    self.start_next(now);
                }
            } else {
                self.stop();
                self.start_next(now);
            }
        } else {
            self.start_next(now);
        }
    }

    /// One client frame: the menu update on a menu screen, then the fade.
    pub fn frame(&mut self, now: f64) {
        if matches!(self.screen, Screen::Menu | Screen::Result) {
            self.menu_frame(now);
        }
        self.player.update(now);
    }

    /// The streams with the music master `m` (0..1) applied.
    pub fn mixed(&self, m: f32) -> Vec<Mixed> {
        self.player
            .gains()
            .map(|(s, g): (&Stream, f32)| Mixed {
                file: s.file.clone(),
                looping: s.looping,
                started: s.started,
                gain: g * m,
            })
            .collect()
    }

    // --- Moments (4).

    fn mode_music(&mut self, now: f64) {
        let Some((file, looping)) = self.data.mode_track(&self.mode, None).map(|(f, l)| (f.to_string(), l)) else {
            return;
        };
        self.stop();
        self.start(&file, looping, 0.0, now);
    }

    /// Room to match: stop, the loading track (full volume, no loop).
    pub fn room_start(&mut self, now: f64) {
        self.screen = Screen::Loading;
        self.reset_map();
        self.stop();
        let f = self.data.cues.loading.clone();
        self.start(&f, false, 0.0, now);
    }

    /// The loading screen closes into the world; Quarantine's mode music
    /// fades in over 10 s.
    pub fn loading_closed(&mut self, mode: &str, now: f64) {
        self.screen = Screen::Match;
        self.mode = mode.to_string();
        self.stop();
        if mode == "QRT" {
            let (f, fade) = (self.data.cues.quarantine_mode.clone(), self.data.quarantine_mode_fade);
            self.start(&f, false, fade, now);
        }
    }

    /// A map begins (or the world is entered without a loading screen):
    /// the per-map cue state is fresh.
    pub fn reset_map(&mut self) {
        self.mode_music_done = false;
        self.qrt_end_played = false;
        self.capture_on = false;
        self.last_stage_on = false;
    }

    /// A round starts. `alive`: the local player lives. A warm-up only
    /// plays `FirstSpawnSound`; the first real round start plays
    /// `RoundStartSound` and the mode's music (stop, then start); later
    /// ones the sound alone.
    pub fn round_start(&mut self, mode: &str, warmup: bool, alive: bool, now: f64) {
        self.mode = mode.to_string();
        self.qrt_end_played = false;
        self.capture_on = false;
        if warmup {
            if alive {
                self.sound("FirstSpawnSound");
            }
            return;
        }
        let first = !self.mode_music_done;
        self.mode_music_done = true;
        if !alive {
            return;
        }
        self.sound("RoundStartSound");
        if first {
            self.mode_music(now);
        }
    }

    /// Quarantine's real round has `left` seconds on the clock.
    pub fn quarantine_clock(&mut self, left: f64, now: f64) {
        let [lo, hi] = self.data.quarantine_end_window;
        if self.qrt_end_played || !(left > lo && left <= hi) {
            return;
        }
        self.qrt_end_played = true;
        self.stop();
        let f = self.data.cues.quarantine_end.clone();
        self.start(&f, false, 0.0, now);
    }

    /// Seize and Secure: the capture timer (seconds left; None: it is not
    /// running).
    pub fn capture_timer(&mut self, left: Option<f64>, now: f64) {
        match left {
            Some(t) if t <= self.data.hq_countdown => {
                if !self.capture_on {
                    self.capture_on = true;
                    self.stop();
                    let f = self.data.cues.capture_progress.clone();
                    self.start(&f, false, 0.0, now);
                }
            }
            _ => {
                if self.capture_on {
                    self.capture_on = false;
                    self.stop();
                }
            }
        }
    }

    /// Arms Race: `n` players are on the last stage.
    pub fn last_stage(&mut self, n: u32, now: f64) {
        if n > 0 && !self.last_stage_on {
            self.last_stage_on = true;
            self.sound("last_stage_sound");
            let f = self.data.cues.last_stage.clone();
            if !self.current.eq_ignore_ascii_case(&f) {
                self.stop();
                self.start(&f, true, 0.0, now);
            }
        } else if n == 0 && self.last_stage_on {
            self.last_stage_on = false;
            self.stop();
            if self.data.modes.get(&self.mode).is_some_and(|m| m.looping) {
                self.mode_music(now);
            }
        }
    }

    /// A perk with a sound file as music (war cry, eagle eye): replaces
    /// whatever plays, and the mode's music doesn't come back (Quirk 5).
    pub fn perk_cue(&mut self, war_cry: bool, now: f64) {
        let f = if war_cry {
            &self.data.cues.war_cry
        } else {
            &self.data.cues.eagle_eye
        }
        .clone();
        self.stop();
        self.start(&f, false, 0.0, now);
    }

    /// A round's result (also the warm-up's): the win sound by the
    /// winning team, the same for both teams (Quirk 6).
    pub fn round_result(&mut self, mode: &str, winner: Option<u8>, my_team: u8) {
        let Some(m) = rules::mode_by_key(mode) else { return };
        let key = if mode == "QRT" {
            match winner {
                Some(ALPHA) => "QRTHumanWinSound",
                Some(BRAVO) => "QRTInfectorWinSound",
                _ => return,
            }
        } else {
            match (m.teams, winner) {
                (Teams::FreeForAll, _) => return,
                (Teams::AllAlpha, _) if my_team != ALPHA => return,
                (_, Some(ALPHA)) => "RoundAlphaTeamWinSound",
                (_, Some(BRAVO)) => "RoundBravoTeamWinSound",
                _ => return,
            }
        };
        self.sound(key);
    }

    /// The match ends: the client leaves the world for the result screen,
    /// music stopped and paused; one win or lose sound (a draw: none).
    /// `first`: the local player is first on the scoreboard.
    pub fn match_end(&mut self, mode: &str, result: Option<Option<u8>>, my_team: u8, first: bool) {
        self.screen = Screen::Result;
        self.stop();
        self.paused = true;
        let Some(m) = rules::mode_by_key(mode) else { return };
        let win = if mode == "QRT" {
            Some(true)
        } else {
            match (m.teams, result) {
                (_, Some(None)) | (_, None) => None,
                (Teams::FreeForAll, Some(_)) => Some(first),
                (_, Some(Some(w))) => Some(w == my_team),
            }
        };
        match win {
            Some(true) => self.sound("ResultWinSound"),
            Some(false) => self.sound("ResultLoseSound"),
            None => {}
        }
    }

    /// Leaving the result screen: unpaused, the playlist starts its next
    /// track at the next menu frame.
    pub fn leave_result(&mut self) {
        self.paused = false;
        self.screen = Screen::Menu;
    }

    /// Leaving a match from the in-game menu: stop.
    pub fn leave_match(&mut self) {
        self.screen = Screen::Menu;
        self.reset_map();
        self.stop();
    }
}

/// What the client knows of the match each frame.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct MusicFacts {
    /// The screen the client shows.
    pub screen: Screen,
    /// The room key; empty: no mode.
    pub mode: String,
    pub phase: rules::Phase,
    pub warmup: bool,
    pub winner: Option<u8>,
    pub result: Option<Option<u8>>,
    /// The round clock's seconds left while live.
    pub round_left: Option<f64>,
    pub my_team: u8,
    pub alive: bool,
    /// Seize and Secure: the capture timer's seconds left.
    pub capture_left: Option<f64>,
    /// Arms Race: players on the last stage.
    pub last_stage: u32,
    /// The local player is first on the scoreboard.
    pub first: bool,
    /// The local player has War Cry's or Eagle Eye's buff.
    pub war_cry: bool,
    pub eagle_eye: bool,
}

fn in_round(p: &rules::Phase) -> bool {
    matches!(p, rules::Phase::Setup { .. } | rules::Phase::Live)
}

/// The moments between two frames' facts (order of events at phase
/// changes): the screen's changes first, then the round's start, clocks
/// and results.
pub fn observe(m: &mut CaMusic, prev: &MusicFacts, now: &MusicFacts, ms: f64) {
    use rules::Phase;
    match (prev.screen, now.screen) {
        (a, Screen::Loading) if a != Screen::Loading => m.room_start(ms),
        (Screen::Loading, Screen::Match) => m.loading_closed(&now.mode, ms),
        (Screen::Menu, Screen::Match) => {
            // A map entered with no loading screen (a local `map`).
            m.reset_map();
            m.loading_closed(&now.mode, ms);
        }
        (Screen::Match, Screen::Menu) => m.leave_match(),
        (Screen::Match, Screen::Result) => m.match_end(&now.mode, now.result, now.my_team, now.first),
        (Screen::Result, Screen::Menu) => m.leave_result(),
        (Screen::Result, Screen::Match) => {
            // A new match without the lobby in between (a restart).
            m.leave_result();
            m.reset_map();
            m.loading_closed(&now.mode, ms);
        }
        _ => {}
    }
    m.screen = now.screen;
    if now.screen != Screen::Match {
        return;
    }
    let before = if prev.screen == Screen::Match {
        prev.phase
    } else {
        Phase::Waiting
    };
    // A round starts: the phase enters setup or live from outside them.
    if in_round(&now.phase) && !in_round(&before) {
        m.round_start(&now.mode, now.warmup, now.alive, ms);
    }
    if now.mode == "QRT"
        && now.phase == Phase::Live
        && !now.warmup
        && let Some(left) = now.round_left
    {
        m.quarantine_clock(left, ms);
    }
    if now.mode == "HQM" {
        let live = now.phase == Phase::Live && !now.warmup;
        m.capture_timer(now.capture_left.filter(|_| live), ms);
    }
    if now.mode == "VWRM" && now.phase == Phase::Live {
        m.last_stage(now.last_stage, ms);
    }
    // A perk with a sound file as music: when the buff begins.
    if prev.screen == Screen::Match {
        if now.war_cry && !prev.war_cry {
            m.perk_cue(true, ms);
        } else if now.eagle_eye && !prev.eagle_eye {
            m.perk_cue(false, ms);
        }
    }
    // A round's result (the warm-up's too).
    if matches!(now.phase, Phase::RoundOver { .. }) && !matches!(before, Phase::RoundOver { .. }) {
        m.round_result(&now.mode, now.winner, now.my_team);
    }
}
