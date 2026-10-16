//! Voice chat's transport (docs/plans/active/voice-chat.md): a client's
//! encoded voice to the server, and from there to each player who hears
//! it. The network never decodes: the codec, the microphone and the
//! speakers are the client shell's (`chimera_client`'s `client::voice`),
//! so a dedicated server relays voice without either.
//!
//! - **Sending**: the client layer writes `VoiceData` (the Opus packets
//!   encoded since its last frame, each 20 ms; `seq` the first one's
//!   number) on the unreliable channel. Nothing goes while nobody talks.
//!   A listen server's host writes it too (replicon hands it to the
//!   server as `ClientId::Server`'s).
//! - **Relaying**: the server takes it from a player with a character
//!   while `sv_voiceenable` is on, within limits (`MAX_FRAMES` frames of
//!   `MAX_FRAME_BYTES` a message, a byte allowance per talker:
//!   `BURST_BYTES`, `BYTES_PER_SECOND`), and sends `VoiceRelay` to every
//!   other player the game's rule lets hear it (`VoiceRules::can_hear`;
//!   CS:S's: `sv_alltalk`, teams and the dead, `cs_source::game::net`).
//! - **Hearing**: a client's `VoiceRelay`s are the client layer's to
//!   decode; `VoiceActivity` keeps who was heard when (and whether we are
//!   talking, `VoiceActivity::local`), for the HUD.
//!
//! Voice chat's transport: `VoiceData` (client, unreliable) relayed as
//! `VoiceRelay` (mapped, unreliable) to each listener `VoiceRules::can_hear`
//! allows (default: everyone), `sv_voiceenable`, per-talker limits,
//! `VoiceActivity` (who talks, for the HUD).
//!
//! Known shortcuts (docs/tech-debt.md): Voice transport (`net::voice`): the
//! per-talker byte allowance (16 kB burst, 8 kB/s) and the frame limits are
//! ours (Source's aren't known); no per-player mute (`vban`) on the server; a
//! talker is "talking" for `TALK_SECONDS` after its last packet (ours).

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use super::server::Player;
use crate::{
    console::resource_cvar,
    core::{Dead, Health, LocalPlayer, NetRole, Spectating},
};

/// Client -> server: the voice encoded since the last frame (each frame
/// one Opus packet of 20 ms), `seq` the first frame's number (wrapping).
#[derive(Message, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct VoiceData {
    pub seq: u16,
    pub frames: Vec<Vec<u8>>,
}

/// Server -> each player who hears it: a talker's voice as it sent it.
#[derive(Message, Serialize, Deserialize, Clone, Debug, PartialEq, bevy::ecs::entity::MapEntities)]
pub struct VoiceRelay {
    #[entities]
    pub speaker: Entity,
    pub seq: u16,
    pub frames: Vec<Vec<u8>>,
}

/// Most frames a message may carry (160 ms; a client sends what it
/// encoded since its last frame, one or two at 60 fps).
pub const MAX_FRAMES: usize = 8;
/// Most bytes of one frame (Opus's own limit is 1275; ours at 24 kbit/s
/// are ~60).
pub const MAX_FRAME_BYTES: usize = 1000;
/// Each talker's byte allowance: a burst, then this many a second (three
/// times what 24 kbit/s needs). Ours.
pub const BURST_BYTES: f64 = 16_000.0;
pub const BYTES_PER_SECOND: f64 = 8_000.0;
/// How long after its last voice a talker counts as talking (s).
pub const TALK_SECONDS: f64 = 0.25;

/// The server's voice settings and the game's rule of who hears whom.
#[derive(Resource, Clone, Copy)]
pub struct VoiceRules {
    /// `sv_voiceenable`: 1 relays voice (CS:S's default, measured on the
    /// probe server 2026-10-15), 0 drops it.
    pub enabled: u8,
    /// Whether `listener` hears `talker` (both characters, the server's).
    /// The default: everyone hears everyone.
    pub can_hear: fn(&World, Entity, Entity) -> bool,
}

impl Default for VoiceRules {
    fn default() -> Self {
        Self {
            enabled: 1,
            can_hear: |_, _, _| true,
        }
    }
}

/// Who is talking, as this process hears it: each talker's character and
/// when (real time, s) its voice last came, and whether this player is
/// sending (`local`, set by the client layer).
#[derive(Resource, Default, Debug)]
pub struct VoiceActivity {
    pub heard: HashMap<Entity, f64>,
    pub local: bool,
    /// Frames relayed (server) or received (client) so far.
    pub frames: u64,
}

impl VoiceActivity {
    /// The characters heard within `TALK_SECONDS` of `now`, in entity
    /// order.
    pub fn talking(&self, now: f64) -> Vec<Entity> {
        let mut out: Vec<Entity> = self
            .heard
            .iter()
            .filter(|(_, t)| now - **t <= TALK_SECONDS)
            .map(|(e, _)| *e)
            .collect();
        out.sort();
        out
    }
}

/// Whether a character is alive (not `Dead`, health above 0).
pub fn alive(world: &World, e: Entity) -> bool {
    world.get::<Dead>(e).is_none()
        && world.get::<Spectating>(e).is_none()
        && world.get::<Health>(e).is_none_or(|h| h.current > 0.0)
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<VoiceRules>()
        .init_resource::<VoiceActivity>()
        .init_resource::<Allowances>()
        .add_systems(
            PreUpdate,
            relay.after(ServerSystems::Receive).run_if(in_state(ServerState::Running)),
        )
        .add_systems(
            PreUpdate,
            heard
                .after(ClientSystems::Receive)
                .after(relay)
                .run_if(not(resource_equals(NetRole::Standalone))),
        );
    resource_cvar::<VoiceRules, u8>(app, "sv_voiceenable", "1: players' voice is relayed; 0: voice chat is off.", |r| {
        &mut r.enabled
    });
    app.world_mut()
        .resource_mut::<crate::console::Console>()
        .archive("sv_voiceenable");
}

/// Each talker's byte allowance (`BURST_BYTES`, `BYTES_PER_SECOND`).
#[derive(Resource, Default)]
struct Allowances(HashMap<Entity, (f64, f64)>);

/// Spend `bytes` of `who`'s allowance at `now`; false: over it.
fn spend(map: &mut HashMap<Entity, (f64, f64)>, who: Entity, now: f64, bytes: usize) -> bool {
    let (tokens, at) = map.entry(who).or_insert((BURST_BYTES, now));
    *tokens = (*tokens + (now - *at).max(0.0) * BYTES_PER_SECOND).min(BURST_BYTES);
    *at = now;
    if *tokens >= bytes as f64 {
        *tokens -= bytes as f64;
        true
    } else {
        false
    }
}

/// Players' voice to everyone who hears it.
fn relay(mut data: MessageReader<FromClient<VoiceData>>, mut commands: Commands) {
    for d in data.read() {
        let (client, seq) = (d.client_id, d.message.seq);
        let frames = &d.message.frames;
        if frames.is_empty() || frames.len() > MAX_FRAMES || frames.iter().any(|f| f.is_empty() || f.len() > MAX_FRAME_BYTES) {
            continue;
        }
        let frames = frames.clone();
        commands.queue(move |w: &mut World| relay_one(w, client, seq, frames));
    }
}

fn relay_one(w: &mut World, client: ClientId, seq: u16, frames: Vec<Vec<u8>>) {
    let rules = *w.resource::<VoiceRules>();
    if rules.enabled == 0 {
        return;
    }
    let Some(talker) = super::chat::speaker(w, client) else { return };
    let now = w.resource::<Time<Real>>().elapsed_secs_f64();
    let bytes: usize = frames.iter().map(Vec::len).sum();
    {
        // Forget talkers that are gone as new ones come.
        let gone: Vec<Entity> = w
            .resource::<Allowances>()
            .0
            .keys()
            .copied()
            .filter(|e| w.get_entity(*e).is_err())
            .collect();
        let mut allowances = w.resource_mut::<Allowances>();
        for e in gone {
            allowances.0.remove(&e);
        }
        if !spend(&mut allowances.0, talker, now, bytes) {
            return;
        }
    }
    let mut listeners: Vec<(ClientId, Entity)> = w
        .query::<(Entity, &Player)>()
        .iter(w)
        .map(|(c, p)| (ClientId::Client(c), p.character))
        .collect();
    if let Some(host) = w.query_filtered::<Entity, With<LocalPlayer>>().iter(w).next() {
        listeners.push((ClientId::Server, host));
    }
    let n = frames.len() as u64;
    let mut sent = false;
    for (to, listener) in listeners {
        if listener == talker || !(rules.can_hear)(w, listener, talker) {
            continue;
        }
        sent = true;
        w.write_message(ToClients {
            targets: SendTargets::Single(to),
            message: VoiceRelay {
                speaker: talker,
                seq,
                frames: frames.clone(),
            },
        });
    }
    if sent {
        w.resource_mut::<VoiceActivity>().frames += n;
    }
}

/// Voice that reached us: its talker is talking.
fn heard(mut relays: MessageReader<VoiceRelay>, time: Res<Time<Real>>, mut activity: ResMut<VoiceActivity>) {
    let now = time.elapsed_secs_f64();
    for r in relays.read() {
        if r.speaker == Entity::PLACEHOLDER {
            continue;
        }
        activity.heard.insert(r.speaker, now);
    }
    // Talkers silent for a while are forgotten.
    activity.heard.retain(|_, t| now - *t < 10.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_talker_gets_a_burst_then_a_rate() {
        let mut map = HashMap::new();
        let e = Entity::from_raw_u32(1).unwrap();
        // The burst at once, then no more at the same instant.
        assert!(spend(&mut map, e, 0.0, 16_000));
        assert!(!spend(&mut map, e, 0.0, 1));
        // Half a second later: half the rate.
        assert!(spend(&mut map, e, 0.5, 4_000));
        assert!(!spend(&mut map, e, 0.5, 1));
    }

    #[test]
    fn talking_lasts_a_moment() {
        let e = Entity::from_raw_u32(1).unwrap();
        let mut a = VoiceActivity::default();
        a.heard.insert(e, 1.0);
        assert_eq!(a.talking(1.1), vec![e]);
        assert!(a.talking(1.0 + TALK_SECONDS + 0.01).is_empty());
    }
}
