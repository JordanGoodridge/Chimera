//! The numbers behind `net_graph` (Source's net graph, as the Valve
//! Developer Community's pages and the community's guides describe it):
//!
//! - **Traffic.** A client's transport (`udp::UdpClient`, `memory`'s link)
//!   counts each packet it hands to renet and each it puts on the wire
//!   (`Traffic`): the last one's size, the mean size, packets and bytes a
//!   second over the last second (renet sends a packet per channel with
//!   something to send and its acknowledgements: ~100 packets a second
//!   each way at 64 ticks, many a few bytes, so `net_graph` shows the
//!   mean). The commands a second (`cmd`) are the `UserCmds` messages
//!   `predict::send_commands` writes.
//! - **The server's side.** Once a second the server tells each remote
//!   client its frame rate and the deviation of its frame time
//!   (`ServerFrames`, Source's `sv` and its `+-`), its tick rate, the
//!   updates it sent that client a second and the share its `rate` held
//!   back (`ServerStats`: Source's `up` and `choke`, from
//!   `server::UpdatePacing`).
//! - **Where they go.** `predict::NetGraph` (with renet's round trip and
//!   loss), read by the client shell's `net_graph` overlay.
//!
//! Known shortcuts (docs/tech-debt.md): sizes are of the transport's
//! payload (netcode's packet over UDP, renet's on the test link), without
//! the UDP and IP headers; whether Source's `in`/`out` count those is
//! unmeasured. A server's own traffic to its clients isn't counted (a
//! listen server's host shows none, as there is no loopback channel).

use std::collections::{HashMap, VecDeque};

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::core::{NetRole, SimClock};

/// Seconds the rates are taken over.
pub const WINDOW: f64 = 1.0;

/// Values over the last `WINDOW` seconds (real time).
#[derive(Debug, Default, Clone)]
pub struct Recent {
    samples: VecDeque<(f64, f64)>,
    /// The first sample's time: the rates' span until a window has passed.
    since: Option<f64>,
}

impl Recent {
    pub fn push(&mut self, now: f64, value: f64) {
        self.since.get_or_insert(now);
        self.samples.push_back((now, value));
        self.prune(now);
    }

    pub fn prune(&mut self, now: f64) {
        while self.samples.front().is_some_and(|(t, _)| now - t > WINDOW) {
            self.samples.pop_front();
        }
    }

    pub fn count(&self) -> usize {
        self.samples.len()
    }

    pub fn sum(&self) -> f64 {
        self.samples.iter().map(|(_, v)| v).sum()
    }

    /// Samples and their sum a second, at `now`.
    pub fn per_second(&mut self, now: f64) -> (f64, f64) {
        self.prune(now);
        let Some(since) = self.since else { return (0.0, 0.0) };
        let span = (now - since).clamp(1e-3, WINDOW);
        (self.count() as f64 / span, self.sum() / span)
    }

    /// The values' mean and standard deviation.
    pub fn mean_dev(&self) -> (f64, f64) {
        let n = self.samples.len();
        if n == 0 {
            return (0.0, 0.0);
        }
        let mean = self.sum() / n as f64;
        let var = self.samples.iter().map(|(_, v)| (v - mean).powi(2)).sum::<f64>() / n as f64;
        (mean, var.sqrt())
    }
}

/// Packets one way: the last one's size, totals, and the last second.
#[derive(Debug, Default, Clone)]
pub struct Flow {
    pub last: usize,
    pub packets: u64,
    pub bytes: u64,
    window: Recent,
}

/// A flow's numbers at a moment.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct FlowRate {
    /// The last packet's size, and the mean over the last second, bytes.
    pub last: usize,
    pub mean: f64,
    pub packets_per_sec: f64,
    pub bytes_per_sec: f64,
}

impl Flow {
    pub fn record(&mut self, now: f64, bytes: usize) {
        self.last = bytes;
        self.packets += 1;
        self.bytes += bytes as u64;
        self.window.push(now, bytes as f64);
    }

    pub fn rate(&mut self, now: f64) -> FlowRate {
        let (packets_per_sec, bytes_per_sec) = self.window.per_second(now);
        FlowRate {
            last: self.last,
            mean: if packets_per_sec > 0.0 { bytes_per_sec / packets_per_sec } else { 0.0 },
            packets_per_sec,
            bytes_per_sec,
        }
    }
}

/// A client's packets as its transport sees them, and the commands it
/// sends.
#[derive(Resource, Debug, Default, Clone)]
pub struct Traffic {
    pub incoming: Flow,
    pub outgoing: Flow,
    pub commands: Recent,
}

/// This process's frame times (s), for the server's `sv` numbers.
#[derive(Resource, Debug, Default, Clone)]
pub struct ServerFrames(pub Recent);

impl ServerFrames {
    /// Frames a second and the frame time's deviation (ms).
    pub fn rate_and_dev(&mut self, now: f64) -> (f32, f32) {
        let (fps, _) = self.0.per_second(now);
        let (_, dev) = self.0.mean_dev();
        (fps as f32, (dev * 1e3) as f32)
    }
}

/// Server -> a client, once a second: the server's frame rate and the
/// deviation of its frame time (ms), its tick rate, the updates it sent
/// this client a second and the share of updates due that its `rate`
/// held back (0-1).
#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct ServerStats {
    pub fps: f32,
    pub frame_dev_ms: f32,
    pub tick_hz: f32,
    pub updates: f32,
    pub choke: f32,
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Traffic>()
        .init_resource::<ServerFrames>()
        .add_systems(First, time_frames)
        .add_systems(Update, send_stats.run_if(resource_equals(NetRole::Server)))
        .add_systems(PreUpdate, receive_stats.after(ClientSystems::Receive));
}

fn time_frames(time: Res<Time<Real>>, mut frames: ResMut<ServerFrames>) {
    let dt = time.delta_secs_f64();
    if dt > 0.0 {
        frames.0.push(time.elapsed_secs_f64(), dt);
    }
}

/// Each second, each remote client's `ServerStats`.
fn send_stats(
    time: Res<Time<Real>>,
    clock: Res<SimClock>,
    mut frames: ResMut<ServerFrames>,
    pacing: Query<(Entity, &super::server::UpdatePacing)>,
    mut last: Local<(f64, HashMap<Entity, (u64, u64)>)>,
    mut out: MessageWriter<ToClients<ServerStats>>,
) {
    let now = time.elapsed_secs_f64();
    let span = now - last.0;
    if span < WINDOW {
        return;
    }
    last.0 = now;
    let (fps, frame_dev_ms) = frames.rate_and_dev(now);
    let tick_hz = 1.0 / clock.delta.as_secs_f32().max(1e-4);
    let mut seen = HashMap::new();
    for (client, p) in &pacing {
        let (sent, choked) = last.1.get(&client).copied().unwrap_or((p.sent, p.choked));
        let (sent, choked) = (p.sent - sent.min(p.sent), p.choked - choked.min(p.choked));
        seen.insert(client, (p.sent, p.choked));
        out.write(ToClients {
            targets: SendTargets::Single(ClientId::Client(client)),
            message: ServerStats {
                fps,
                frame_dev_ms,
                tick_hz,
                updates: (sent as f64 / span) as f32,
                choke: choke_share(sent, choked),
            },
        });
    }
    last.1 = seen;
}

/// The share of updates due that were held back.
pub fn choke_share(sent: u64, choked: u64) -> f32 {
    if sent + choked == 0 { 0.0 } else { choked as f32 / (sent + choked) as f32 }
}

fn receive_stats(
    mut stats: MessageReader<ServerStats>,
    role: Option<Res<NetRole>>,
    graph: Option<ResMut<super::predict::NetGraph>>,
) {
    let latest = stats.read().last().copied();
    if role.is_none_or(|r| *r != NetRole::Client) {
        return;
    }
    if let (Some(s), Some(mut g)) = (latest, graph) {
        g.server = Some(s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_over_the_last_second() {
        let mut f = Flow::default();
        // 100-byte packets every 10 ms for 2 s.
        for i in 0..200 {
            f.record(i as f64 * 0.01, 100);
        }
        let r = f.rate(1.99);
        assert!((r.packets_per_sec - 101.0).abs() < 1.5, "{r:?}");
        assert!((r.bytes_per_sec - 10_100.0).abs() < 150.0, "{r:?}");
        assert_eq!(r.last, 100);
        assert!((r.mean - 100.0).abs() < 1e-9);
        // Nothing for a while: the window empties.
        assert_eq!(f.rate(5.0).packets_per_sec, 0.0);
        assert_eq!(f.packets, 200);
    }

    #[test]
    fn a_young_window_counts_over_its_age() {
        let mut w = Recent::default();
        w.push(10.0, 1.0);
        w.push(10.5, 1.0);
        let (n, _) = w.per_second(10.5);
        assert!((n - 4.0).abs() < 1e-9, "two in half a second: {n}");
    }

    #[test]
    fn deviation_and_choke() {
        let mut w = Recent::default();
        for (i, v) in [0.010, 0.020, 0.010, 0.020].iter().enumerate() {
            w.push(i as f64 * 0.1, *v);
        }
        let (mean, dev) = w.mean_dev();
        assert!((mean - 0.015).abs() < 1e-9 && (dev - 0.005).abs() < 1e-9);
        assert_eq!(choke_share(0, 0), 0.0);
        assert_eq!(choke_share(3, 1), 0.25);
    }
}
