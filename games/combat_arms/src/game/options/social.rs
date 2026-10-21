//! The Other tab (specs/combat_arms/options.md O): the social options the
//! account service keeps, the language choice and the block list. There
//! is no account or messenger service here, so `CaSocial` holds the
//! values the service would, and `requests` the changes it was sent; the
//! block list is a per-account file.
//!
//! Known shortcuts: the service is a stand-in (`CaSocial::requests` is
//! what a client would send; nothing receives it); `[OtherOption]`'s
//! booleans are read as each switch's "on" state (Whisper: block
//! whispers, BlockInvite: block invitations, CurseFilter: chat filter,
//! AllowNote: receive notes from friends, DailyQuest: the daily-job popup
//! on), where the spec leaves the mapping open (its Open question 10);
//! the language drop-down's file (`UserLanguage.cfg`) is read and written
//! as a profile-format file; the block list's file format is ours
//! (`[BlackList]`, `NameN = "name"`).

use bevy::prelude::*;

use crate::profile::{self, Profile};

/// What the messenger service says about receiving notes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Notes {
    /// The state isn't known yet: the switch is disabled.
    #[default]
    Unknown,
    /// From friends.
    Friends,
    /// From nobody.
    Nobody,
}

/// The account's facts the screen needs.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct CaAccount {
    /// The account's name (the block list's file).
    pub name: String,
    pub age: Option<u32>,
    pub region: super::video::Region,
    pub low_violence: bool,
    /// A game master: sees the spectator switch on the server-select
    /// screen, has no help popup.
    pub game_master: bool,
    /// The messenger service is reachable.
    pub messenger: bool,
}

impl Default for CaAccount {
    fn default() -> Self {
        Self {
            name: "player".into(),
            age: None,
            region: super::video::Region::Europe,
            low_violence: false,
            game_master: false,
            messenger: true,
        }
    }
}

/// The social options.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct CaSocial {
    pub block_whispers: bool,
    pub block_invites: bool,
    pub chat_filter: bool,
    pub spectator_mode: bool,
    pub notes: Notes,
    /// The daily-job popup (the service stores it inverted: bit set =
    /// popup off).
    pub daily_job_popup: bool,
    /// `UserLanguage.cfg`'s `[Config] LanguageCode` ("" none).
    pub language: String,
    /// What was sent to the service, in order (`whisper 1`, `note 0`).
    pub requests: Vec<String>,
}

impl Default for CaSocial {
    fn default() -> Self {
        Self {
            block_whispers: false,
            block_invites: false,
            chat_filter: true,
            spectator_mode: false,
            notes: Notes::Unknown,
            daily_job_popup: true,
            language: String::new(),
            requests: Vec::new(),
        }
    }
}

impl CaSocial {
    /// The service's option bit for the daily-job popup: set = off.
    pub fn daily_bit(&self) -> bool {
        !self.daily_job_popup
    }

    /// Send a change to the service (recorded).
    pub fn send(&mut self, what: &str, on: bool) {
        self.requests.push(format!("{what} {}", on as u8));
    }

    /// The Default button (O): the switches back to `[OtherOption]`'s
    /// values, which come from the profile, not the default profile.
    /// Receive notes is only reset while the messenger service is
    /// reachable.
    pub fn reset(&mut self, profile: &Profile, messenger: bool) {
        let b = |k: &str, d: bool| profile.bool("OtherOption", k).unwrap_or(d);
        self.block_whispers = b("Whisper", false);
        self.block_invites = b("BlockInvite", false);
        self.chat_filter = b("CurseFilter", true);
        self.daily_job_popup = b("DailyQuest", true);
        if messenger {
            self.notes = if b("AllowNote", true) { Notes::Friends } else { Notes::Nobody };
        }
    }
}

/// `UserLanguage.cfg`: the language code.
pub fn language_of(file: &Profile) -> Option<String> {
    file.string("Config", "LanguageCode")
}

/// The language file's text for a code.
pub fn language_file(code: &str) -> Profile {
    let mut p = Profile::default();
    p.set("Config", "LanguageCode", &profile::quote(code));
    p
}

// ---------------------------------------------------------------------------
// The block list

/// The block list (O): names, each with a check, sorted ascending or
/// descending by name ignoring case.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlockList {
    pub names: Vec<(String, bool)>,
    /// The sort toggle: the next `sort` goes descending.
    pub descending: bool,
}

/// Why a name couldn't be registered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// Names matching `GM*` can never be blocked.
    GameMaster,
    Empty,
    AlreadyThere,
}

impl BlockList {
    /// "Register": add a name.
    pub fn register(&mut self, name: &str) -> Result<(), Refused> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Refused::Empty);
        }
        if name.len() >= 2 && name[..2].eq_ignore_ascii_case("GM") {
            return Err(Refused::GameMaster);
        }
        if self.names.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)) {
            return Err(Refused::AlreadyThere);
        }
        self.names.push((name.to_string(), false));
        Ok(())
    }

    /// "Clear": remove every checked name; whether any were checked.
    pub fn clear_checked(&mut self) -> bool {
        let before = self.names.len();
        self.names.retain(|(_, checked)| !checked);
        self.names.len() != before
    }

    /// "Sort": the names ascending, then descending on the next click
    /// (case-insensitive).
    pub fn sort(&mut self) {
        self.names.sort_by_key(|(n, _)| n.to_lowercase());
        if self.descending {
            self.names.reverse();
        }
        self.descending = !self.descending;
    }

    /// The header check sets every row's check.
    pub fn check_all(&mut self, on: bool) {
        for (_, c) in &mut self.names {
            *c = on;
        }
    }

    pub fn blocked(&self, name: &str) -> bool {
        self.names.iter().any(|(n, _)| n.eq_ignore_ascii_case(name))
    }

    /// The file's form.
    pub fn to_profile(&self) -> Profile {
        let mut p = Profile::default();
        for (i, (n, _)) in self.names.iter().enumerate() {
            p.set("BlackList", &format!("Name{i}"), &profile::quote(n));
        }
        p
    }

    pub fn from_profile(p: &Profile) -> Self {
        let mut list = Self::default();
        let mut i = 0;
        while let Some(n) = p.string("BlackList", &format!("Name{i}")) {
            list.names.push((n, false));
            i += 1;
        }
        list
    }

    /// The per-account file: `<account>_BlackList.txt` in the game's
    /// `Etc` folder (here, beside the profile).
    pub fn file_name(account: &str) -> String {
        format!("{account}_BlackList.txt")
    }
}

// ---------------------------------------------------------------------------
// The voice system's devices (S1)

/// The voice system the Sound tab's Voice page talks to. There is none
/// here (`built` false: every voice control is disabled), so the lists
/// are empty and not ready.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct CaVoice {
    /// The build has voice chat.
    pub built: bool,
    /// The voice system has found its devices.
    pub ready: bool,
    pub microphones: Vec<String>,
    pub speakers: Vec<String>,
}

/// A device's text (S1): `finding` while the voice system isn't ready,
/// `N/A` for an index past the list's end, a name longer than 24
/// characters as its first 22 and "...".
pub fn device_text(devices: &[String], index: i32, ready: bool) -> String {
    if !ready {
        return "finding".into();
    }
    match usize::try_from(index).ok().and_then(|i| devices.get(i)) {
        Some(n) if n.chars().count() > 24 => format!("{}...", n.chars().take(22).collect::<String>()),
        Some(n) => n.clone(),
        None => "N/A".into(),
    }
}
