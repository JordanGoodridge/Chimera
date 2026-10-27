//! The weapon mode switch's centre notice, for any game's HUD: when the
//! local player's weapon steps to another mode (`WeaponEventKind::ModeChanged`)
//! and its data names what to say (`ModeTexts`, string tokens by mode), the
//! text goes to `CenterNotice` for a few seconds. A game fills
//! `NoticeStrings` with the words of its tokens (a token without words shows
//! as itself); a HUD without its own string table (the minimal HUD, Combat
//! Arms') draws `CenterNotice`. CS:S's HUD prints from the install's strings
//! itself (`objectives_hud`).

use std::collections::BTreeMap;

use bevy::prelude::*;

use super::{ModeTexts, WeaponEvent, WeaponEventKind};
use crate::core::LocalPlayer;

/// How long the notice stays up (seconds).
pub const NOTICE_SECONDS: f32 = 3.0;

/// The words of string tokens (`#Name`, lowercase without the `#`).
#[derive(Resource, Default, Clone, Debug)]
pub struct NoticeStrings(pub BTreeMap<String, String>);

impl NoticeStrings {
    pub fn add(&mut self, token: &str, text: &str) {
        self.0.insert(token.trim_start_matches('#').to_lowercase(), text.to_string());
    }

    /// The words of `token`, else the token itself.
    pub fn words(&self, token: &str) -> String {
        self.0
            .get(&token.trim_start_matches('#').to_lowercase())
            .cloned()
            .unwrap_or_else(|| token.to_string())
    }
}

/// The text to show at the screen's centre, and until when (`Time`'s
/// elapsed seconds).
#[derive(Resource, Default, Clone, Debug)]
pub struct CenterNotice(pub Option<(String, f32)>);

impl CenterNotice {
    /// The text while it lasts.
    pub fn text(&self, now: f32) -> Option<&str> {
        self.0.as_ref().filter(|(_, until)| *until > now).map(|(s, _)| s.as_str())
    }
}

pub(super) fn mode_notice(
    mut events: MessageReader<WeaponEvent>,
    local: Query<(), With<LocalPlayer>>,
    texts: Query<&ModeTexts>,
    strings: Res<NoticeStrings>,
    mut notice: ResMut<CenterNotice>,
    time: Res<Time>,
) {
    for ev in events.read().filter(|e| e.shown() && local.contains(e.owner)) {
        let WeaponEventKind::ModeChanged { mode } = ev.kind else {
            continue;
        };
        let Some(token) = texts.get(ev.weapon).ok().and_then(|t| t.0.get(mode as usize)) else {
            continue;
        };
        notice.0 = Some((strings.words(token), time.elapsed_secs() + NOTICE_SECONDS));
    }
}
