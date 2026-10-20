//! The scanner items (specs/combat_arms/radar_minimap.md S1 to S3): the
//! heartbeat sensor and detector, the satellite scanner and Hunted's
//! system scan, as far as the radar reads them.
//!
//! - The items are weapons (`combat_arms:scanner`, `ScannerParams`: the
//!   weapon data's `SensorOption`, `ScanDist`, battery and `ScanSendTime`).
//!   `scanners` (the server's, 20 Hz-free: every fixed tick) keeps each
//!   holder's `CaScanner` (on or off, the battery): a heartbeat item is
//!   on while it is in hand, fire is held, the player lives, doesn't sprint,
//!   the round plays and the battery isn't empty; it drains
//!   `using` a second and refills `refill` per `refill_time` while off.
//!   A satellite scanner's press starts a shared scan for the holder's team
//!   (`CaScans`), `send_time` long; a new press replaces the timer.
//! - Contacts (`contact`): an enemy inside the scanner owner's view frustum
//!   (in front, within the field of view, near to far) and within
//!   `scan_dist` horizontally; walls don't block. The local player's own
//!   contacts are worked out by its HUD from its camera
//!   (`ui::hud::radar`); a detector (option 1) also shares its list with
//!   its team: the server works it out from the owner's view (`share`, at
//!   most every 100 ms, only on change) and marks each contact for the
//!   owner's team on the enemy's `CaSeen`, until the item is off.
//! - `CaSeen` (replicated, on each player): which teams see it by a shared
//!   scan (satellite, system) and which by a shared heartbeat list; the
//!   radar reads both, the full map only the scans (F4).
//!
//! Known shortcuts: the shared list is worked out on the server from the
//! owner's position and aim (no camera there: a 75 degree vertical field at
//! 4:3, near 0, far 100000 u, the eye at the hull's centre), not from the
//! owner's client; the satellite item isn't used up and has no cooldown
//! (the spec has no charges); the scan notices ("... is uploading a
//! Satellite Scan ...", the warning to the other team) aren't sent (the
//! chat spec's); Hunted's periodic system scan is disabled in the build the
//! spec measured, so only the radar's reading of it (`CaScans::system`) is
//! here, set by a test or a future caller.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::super::{
    inventory::UNIT,
    rules::{CaMatch, state::Phase},
};
use crate::{
    core::{Dead, Intent, SimClock, Team},
    weapon::Inventory,
};

/// Hook name (docs/weapon-data.md, "Hooks").
pub const SCANNER: &str = "combat_arms:scanner";

/// `SensorOption`: 0 `Heartbeat_sensor`, 1 `HeartBeat_Detector`, 2
/// `Satellite_Scanner`.
pub const HEARTBEAT: u8 = 0;
pub const DETECTOR: u8 = 1;
pub const SATELLITE: u8 = 2;

/// The least time between a detector's list messages (ms).
pub const LIST_SEND_MS: f64 = 100.0;

/// `combat_arms:scanner`: a scanner item's attribute fields (S1, S2).
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ScannerParams {
    /// `SensorOption`.
    pub option: u8,
    /// `ScanDist`, world units.
    #[serde(default = "scan_dist")]
    pub scan_dist: f32,
    /// `BatteryMaxQuantity`, `BatteryUsingQuantity` (per s while on),
    /// `BatteryRefillQuantity` per `BatteryRefillTime` (ms) while off.
    #[serde(default = "battery_max")]
    pub battery_max: f32,
    #[serde(default = "battery_using")]
    pub using: f32,
    #[serde(default = "refill")]
    pub refill: f32,
    #[serde(default = "refill_time")]
    pub refill_time: f32,
    /// `ScanSendTime`, ms (the satellite scan's length).
    #[serde(default = "send_time")]
    pub send_time: f32,
}

fn scan_dist() -> f32 {
    1000.0
}
fn battery_max() -> f32 {
    100.0
}
fn battery_using() -> f32 {
    10.0
}
fn refill() -> f32 {
    5.0
}
fn refill_time() -> f32 {
    1000.0
}
fn send_time() -> f32 {
    7000.0
}

/// A scanner weapon's numbers.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct CaScannerData(pub ScannerParams);

/// A holder's scanner state (replicated: the radar's own contacts, the
/// battery).
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct CaScanner {
    pub option: u8,
    pub on: bool,
    pub battery: f32,
    pub scan_dist: f32,
}

/// The server's bookkeeping of a holder's scanner: fire last tick, the time
/// off (for the refill steps). Not replicated.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct ScannerMemory {
    pub prev_fire: bool,
    pub idle: f32,
}

/// Which teams see a player through a scan (bit 0 Alpha, bit 1 Bravo),
/// and through a teammate's shared heartbeat list.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CaSeen {
    pub scan: u8,
    pub contact: u8,
}

impl CaSeen {
    pub fn scanned_by(&self, team: u8) -> bool {
        bit(team).is_some_and(|b| self.scan & b != 0)
    }
    pub fn contact_of(&self, team: u8) -> bool {
        bit(team).is_some_and(|b| self.contact & b != 0)
    }
}

fn bit(team: u8) -> Option<u8> {
    matches!(team, 1 | 2).then(|| 1 << (team - 1))
}

/// The shared scans (S2, S3): until when (game seconds) each team's
/// satellite scan lasts, and Hunted's system scan.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct CaScans {
    pub satellite: [Option<f64>; 2],
    /// The system scan is on (everyone sees everyone).
    pub system: bool,
}

/// The detectors' shared lists: sender -> (sent at in ms of game time,
/// the enemies on it).
#[derive(Resource, Clone, Debug, Default)]
pub struct CaDetectors {
    pub lists: HashMap<Entity, (f64, Vec<Entity>)>,
}

/// The view frustum a scan tests: the eye, yaw and pitch (Intent's),
/// the vertical field of view (degrees) and the aspect ratio.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub eye: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub fov_v: f32,
    pub aspect: f32,
}

/// The shell's camera: 75 degrees vertical.
pub const FOV_VERTICAL: f32 = 75.0;
pub const NEAR: f32 = 0.0;
pub const FAR: f32 = 100_000.0 * UNIT;

impl View {
    /// The point is inside all six planes of the frustum.
    pub fn sees(&self, p: Vec3) -> bool {
        let q = Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0);
        let local = q.inverse() * (p - self.eye);
        // Camera space: -Z forward, +X right, +Y up.
        let d = -local.z;
        if d <= NEAR || d > FAR {
            return false;
        }
        let th = (self.fov_v.to_radians() / 2.0).tan();
        local.y.abs() <= d * th && local.x.abs() <= d * th * self.aspect
    }
}

/// S1: whether the enemy at `target` is a contact for a scan of `dist`
/// world units from `view`: inside the frustum and within `dist`
/// horizontally.
pub fn contact(view: &View, target: Vec3, dist: f32) -> bool {
    let d = Vec2::new(target.x - view.eye.x, target.z - view.eye.z).length() / UNIT;
    d <= dist && view.sees(target)
}

/// The play phase of a running round.
fn playing(m: &CaMatch) -> bool {
    matches!(m.phase, Phase::Live)
}

/// S1, S2: every holder's scanner for this tick (the server's).
pub fn scanners(
    mut players: Query<(
        Entity,
        &Inventory,
        &Intent,
        Option<&Team>,
        Has<Dead>,
        Option<&CaScanner>,
        Option<&ScannerMemory>,
    )>,
    items: Query<&CaScannerData>,
    clock: Res<SimClock>,
    m: Res<CaMatch>,
    mut scans: ResMut<CaScans>,
    mut detectors: ResMut<CaDetectors>,
    mut commands: Commands,
) {
    let dt = clock.dt();
    let live = playing(&m);
    let mut gone: HashSet<Entity> = HashSet::new();
    for (e, inv, intent, team, dead, state, memory) in &mut players {
        let held = inv.active.and_then(|w| items.get(w).ok()).map(|d| d.0);
        let Some(p) = held else {
            if state.is_some() {
                commands.entity(e).remove::<(CaScanner, ScannerMemory)>();
            }
            gone.insert(e);
            continue;
        };
        let mut mem = memory.copied().unwrap_or_default();
        let before = state.copied();
        let mut s = before.unwrap_or(CaScanner {
            option: p.option,
            on: false,
            battery: p.battery_max,
            scan_dist: p.scan_dist,
        });
        s.option = p.option;
        s.scan_dist = p.scan_dist;
        let alive = !dead;
        if p.option == SATELLITE {
            if intent.fire && !mem.prev_fire && alive && live {
                if let Some(side) = team.and_then(|t| bit(t.0)) {
                    let i = side.trailing_zeros() as usize;
                    scans.satellite[i] = Some(clock.now + p.send_time as f64 / 1000.0);
                }
            }
            s.on = false;
        } else {
            let want = intent.fire && alive && !intent.sprint && live && s.battery > 0.0;
            if want {
                s.on = true;
                mem.idle = 0.0;
                s.battery = (s.battery - p.using * dt).max(0.0);
                if s.battery <= 0.0 {
                    s.on = false;
                }
            } else {
                s.on = false;
                mem.idle += dt;
                let step = p.refill_time / 1000.0;
                while step > 0.0 && mem.idle >= step {
                    mem.idle -= step;
                    s.battery = (s.battery + p.refill).min(p.battery_max);
                }
            }
        }
        mem.prev_fire = intent.fire;
        if !(s.on && p.option == DETECTOR) {
            gone.insert(e);
        }
        if before != Some(s) {
            commands.entity(e).insert(s);
        }
        if memory.copied() != Some(mem) {
            commands.entity(e).insert(mem);
        }
    }
    for e in gone {
        detectors.lists.remove(&e);
    }
}

/// S1 (sharing): each detector that is on works out its list at most every
/// 100 ms and keeps it when it changed.
pub fn share(
    senders: Query<(Entity, &CaScanner, &Intent, &Transform, Option<&Team>)>,
    others: Query<(Entity, &Transform, &Team, Has<Dead>), With<Intent>>,
    clock: Res<SimClock>,
    mut detectors: ResMut<CaDetectors>,
) {
    let now_ms = clock.now * 1000.0;
    for (e, s, intent, t, team) in &senders {
        if !(s.on && s.option == DETECTOR) {
            continue;
        }
        let due = detectors.lists.get(&e).is_none_or(|(at, _)| now_ms - at >= LIST_SEND_MS);
        if !due {
            continue;
        }
        let my = team.map_or(0, |t| t.0);
        let view = View {
            eye: t.translation,
            yaw: intent.yaw,
            pitch: intent.pitch,
            fov_v: FOV_VERTICAL,
            aspect: 4.0 / 3.0,
        };
        let mut list: Vec<Entity> = others
            .iter()
            .filter(|(o, ot, team, dead)| *o != e && team.0 != my && !dead && contact(&view, ot.translation, s.scan_dist))
            .map(|(o, ..)| o)
            .collect();
        list.sort();
        match detectors.lists.get_mut(&e) {
            // Sent only on change; the timer still moves.
            Some((at, old)) if *old == list => *at = now_ms,
            _ => {
                detectors.lists.insert(e, (now_ms, list));
            }
        }
    }
}

/// S1 to S3: the scans and shared lists folded into each player's
/// `CaSeen` (when it changed), and an expired satellite scan ended.
pub fn mark(
    mut scans: ResMut<CaScans>,
    detectors: Res<CaDetectors>,
    clock: Res<SimClock>,
    senders: Query<&Team>,
    players: Query<(Entity, &Team, Option<&CaSeen>), With<Intent>>,
    mut commands: Commands,
) {
    for s in &mut scans.satellite {
        if s.is_some_and(|end| clock.now >= end) {
            *s = None;
        }
    }
    let mut contact: HashMap<Entity, u8> = HashMap::new();
    for (sender, (_, list)) in &detectors.lists {
        let Some(b) = senders.get(*sender).ok().and_then(|t| bit(t.0)) else { continue };
        for e in list {
            *contact.entry(*e).or_default() |= b;
        }
    }
    for (e, team, have) in &players {
        let mut scan = 0u8;
        for (i, until) in scans.satellite.iter().enumerate() {
            let viewer = i as u8 + 1;
            if until.is_some() && team.0 != viewer {
                scan |= 1 << i;
            }
        }
        if scans.system {
            scan = 0b11;
        }
        let now = CaSeen {
            scan,
            contact: contact.get(&e).copied().unwrap_or(0),
        };
        if have.copied().unwrap_or_default() != now {
            commands.entity(e).insert(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> View {
        View {
            eye: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            fov_v: FOV_VERTICAL,
            aspect: 4.0 / 3.0,
        }
    }

    #[test]
    fn contacts_are_in_front_and_in_range() {
        let v = view();
        // Yaw 0 looks down -Z.
        assert!(contact(&v, Vec3::new(0.0, 0.0, -900.0 * UNIT), 1000.0));
        assert!(!contact(&v, Vec3::new(0.0, 0.0, -1100.0 * UNIT), 1000.0));
        assert!(!contact(&v, Vec3::new(0.0, 0.0, 500.0 * UNIT), 1000.0));
    }
}
