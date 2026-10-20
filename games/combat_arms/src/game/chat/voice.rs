//! Voice chat's rules (specs/combat_arms/radio_chat.md V1): which channel
//! a player is in by mode, team and life, push-to-talk and what it says
//! when there is no channel, and the output toggle. The audio itself (an
//! external service in the game) is not here: this is the channel choice
//! and the key's state.

use super::show::{LINE_LIFE, PanelLine, colours};

/// A voice channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    Alpha,
    Bravo,
}

/// Why a player has no voice channel (the red line on a press).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoChannel {
    /// Spy Hunt, not on Alpha.
    NotAlpha,
    /// Last Man Standing or Hunted, dead.
    DeadInMode,
    /// Anything else: not connected to the service.
    Disconnected,
}

/// How a mode picks channels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceMode {
    /// One shared channel: One Man Army, Arms Race.
    Shared,
    /// One shared channel, the dead leave: Last Man Standing, Hunted.
    SharedAlive,
    /// Alpha players only.
    SpyHunt,
    /// A player moved to Bravo leaves.
    Fireteam,
    /// Alpha or Bravo by team.
    Teams,
}

/// The room keys of Fireteam (not known: no mode of ours is one yet).
pub const FIRETEAM_KEYS: &[&str] = &[];

/// The voice mode of a game mode's room key.
pub fn mode_of(key: &str) -> VoiceMode {
    match key {
        "FFA" | "VWRM" => VoiceMode::Shared,
        "LMS" | "HG" => VoiceMode::SharedAlive,
        "BNT" => VoiceMode::SpyHunt,
        k if FIRETEAM_KEYS.contains(&k) => VoiceMode::Fireteam,
        _ => VoiceMode::Teams,
    }
}

/// What the channel choice reads of a player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Voicer {
    /// 0 Alpha, 1 Bravo, 255 a GM spectator.
    pub team: u8,
    pub alive: bool,
}

/// The channel `p` is in under `mode`, or why none.
pub fn channel(mode: VoiceMode, p: Voicer) -> Result<Channel, NoChannel> {
    let by_team = |p: Voicer| if p.team == 1 { Channel::Bravo } else { Channel::Alpha };
    match mode {
        VoiceMode::Shared => Ok(Channel::Alpha),
        VoiceMode::SharedAlive if p.alive => Ok(Channel::Alpha),
        VoiceMode::SharedAlive => Err(NoChannel::DeadInMode),
        VoiceMode::SpyHunt if p.team == 0 => Ok(Channel::Alpha),
        VoiceMode::SpyHunt => Err(NoChannel::NotAlpha),
        // GM spectators listen to Alpha.
        VoiceMode::Fireteam | VoiceMode::Teams if p.team == 255 => Ok(Channel::Alpha),
        VoiceMode::Fireteam if p.team == 1 => Err(NoChannel::Disconnected),
        VoiceMode::Fireteam | VoiceMode::Teams => Ok(by_team(p)),
    }
}

/// The local player's voice keys: push-to-talk and the output toggle.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoiceKeys {
    /// `EnableVoiceChat` (the seed profile: off).
    pub output: bool,
    /// The microphone is unmuted (the key held); the talk icon shows.
    pub talking: bool,
}

impl VoiceKeys {
    /// The push-to-talk key goes down: the microphone is unmuted anyway
    /// (harmless without a channel); the red line, if any.
    pub fn press(&mut self, ch: Result<Channel, NoChannel>, now: f64) -> Option<PanelLine> {
        self.talking = true;
        let text = match ch {
            Ok(_) => return None,
            Err(NoChannel::NotAlpha) => "Voice chat is disabled for players not on Alpha in this mode.",
            Err(NoChannel::DeadInMode) => "Voice chat is disabled in this mode for dead players.",
            Err(NoChannel::Disconnected) => "Voice chat is not connected.",
        };
        Some(PanelLine::new(text, colours::SYSTEM, now, LINE_LIFE))
    }

    /// The frame the key is no longer held: muted again.
    pub fn release(&mut self) {
        self.talking = false;
    }

    /// F8: voice output on or off, saved; the red line and the panel's
    /// string.
    pub fn toggle_output(&mut self, now: f64) -> [PanelLine; 2] {
        self.output = !self.output;
        let (text, id) = if self.output {
            ("Voice chat enabled.", "$VoiceChatEnable")
        } else {
            ("Voice chat disabled.", "$VoiceChatDisable")
        };
        [
            PanelLine::new(text, colours::SYSTEM, now, LINE_LIFE),
            PanelLine::new(id, colours::SYSTEM, now, LINE_LIFE),
        ]
    }
}

/// Who hears a speaker: the players in the same channel.
pub fn listeners<T: Copy>(speaker: Channel, players: &[(T, Result<Channel, NoChannel>)]) -> Vec<T> {
    players.iter().filter(|(_, c)| *c == Ok(speaker)).map(|(t, _)| *t).collect()
}
