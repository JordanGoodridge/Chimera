//! The bots' formulas (specs/combat_arms/bots.md), as plain functions on
//! numbers in the spec's units (world units, seconds), so each test case
//! can check them: the roster's dice, the reaction delay, the camping
//! bonus, the hit/miss cycle and the aim point, the movement inaccuracy,
//! the notice and hearing rolls, the weapon choice, the grenade's throw,
//! the attack moves and the patrol link choice.

use bevy::prelude::*;

use super::data::{ItemSet, SlotList};

// ---------------------------------------------------------------------------
// Roster ("Choosing the bots")

/// Step 2: the profile index for draw `rho` among `count` profiles,
/// stepping past those `used` (cleared first when all are used); the
/// pick is added to `used`.
pub fn pick_profile(rho: u64, count: usize, used: &mut Vec<usize>) -> usize {
    if count == 0 {
        return 0;
    }
    if used.len() >= count {
        used.clear();
    }
    let mut i = (rho % count as u64) as usize;
    while used.contains(&i) {
        i = (i + 1) % count;
    }
    used.push(i);
    i
}

/// Step 4 for one slot: the item it gets with draws `rho` and `sigma`
/// (None: empty), and `sigma` for the next slot.
pub fn fill_slot(slot: &SlotList, rho: u64, sigma: u64) -> (Option<String>, u64) {
    if slot.percent <= 0.0 || slot.items.is_empty() {
        return (None, sigma);
    }
    let given = slot.percent >= 100.0 || slot.percent > (sigma % 100) as f32;
    let item = given.then(|| slot.items[(rho % slot.items.len() as u64) as usize].clone());
    let sigma = if slot.percent < 100.0 { sigma / 10 } else { sigma };
    (item, sigma)
}

/// Step 4: every slot of an item set (model, backpack, head, goggle, then
/// the weapon slots 0-5) with the same `rho`, `sigma` starting at it.
pub fn fill_set(set: &ItemSet, rho: u64) -> [Option<String>; 10] {
    let mut sigma = rho;
    let mut out: [Option<String>; 10] = Default::default();
    for (k, slot) in set.slots().into_iter().enumerate() {
        let (item, next) = fill_slot(slot, rho, sigma);
        out[k] = item;
        sigma = next;
    }
    out
}

// ---------------------------------------------------------------------------
// Camping (5.5), reaction (7.2)

/// The camping bonus B(v) after `t` s within the camping radius.
pub fn camping_bonus(t: f32, v: f32) -> f32 {
    if t < 10.0 { 0.0 } else { t / 10.0 * v }
}

/// The reaction delay (7.2) with the template's limits, the camping time
/// `camp` of the target and a uniform draw `u` in [0, 1].
pub fn reaction_delay(t_min: f32, t_max: f32, camp: f32, u: f32) -> f32 {
    let t_max = (t_max - camping_bonus(camp, 0.2)).max(0.0);
    if t_min < t_max { t_min + u * (t_max - t_min) } else { t_max }
}

// ---------------------------------------------------------------------------
// The hit/miss cycle (7.3)

/// The accuracy for a shot: the weapon's `a` plus the camping bonus,
/// halved when only the head was visible, clamped to 0..1.
pub fn accuracy(a_w: f32, camp: f32, head_only: bool) -> f32 {
    ((a_w + camping_bonus(camp, 0.1)) * if head_only { 0.5 } else { 1.0 }).clamp(0.0, 1.0)
}

/// Hits and misses per cycle (h, m) for accuracy `a`.
pub fn cycle(a: f32) -> (u32, u32) {
    if a <= 0.0 {
        (0, 1)
    } else if a >= 1.0 {
        (1, 0)
    } else if a < 0.5 {
        (1, ((1.0 - a) / a + 0.5).floor() as u32)
    } else {
        (((a / (1.0 - a)) + 0.5).floor() as u32, 1)
    }
}

/// The per-bot counters of the cycle (kept across targets).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HitCycle {
    pub hits: u32,
    pub misses: u32,
}

impl HitCycle {
    /// One shot with accuracy `a` and roll `r100` (0-99): true for an
    /// aimed shot (a hit), false for a miss.
    pub fn shot(&mut self, a: f32, r100: u32) -> bool {
        let (h, m) = cycle(a);
        if self.hits >= h && self.misses >= m {
            *self = Self::default();
        }
        let want_miss = (r100 as f32) > 100.0 * a && self.misses < m;
        if self.hits < h && !want_miss {
            self.hits += 1;
            true
        } else {
            self.misses += 1;
            false
        }
    }
}

impl HitCycle {
    /// One shot of a mercenary (PvE AI, bots_objectives.md 3.3): every cycle
    /// is its `h` aimed shots first, then its `m` misses, whatever the
    /// draws.
    pub fn ordered_shot(&mut self, a: f32) -> bool {
        let (h, m) = cycle(a);
        if self.hits >= h && self.misses >= m {
            *self = Self::default();
        }
        if self.hits < h {
            self.hits += 1;
            true
        } else {
            self.misses += 1;
            false
        }
    }
}

/// A mercenary's aimed shot (PvE AI): off by exactly 100 u while the
/// movement inaccuracy `c` is above zero, in the direction of the random
/// offset (U(−c, c), U(−c/2, c/2), U(−c, c)) scaled to length 1; on the
/// base point once `c` has decayed to 0.
pub fn merc_hit_point(base: Vec3, c: f32, u: Vec3) -> Vec3 {
    if c <= 0.0 {
        return base;
    }
    let s = |u: f32, r: f32| (u * 2.0 - 1.0) * r;
    base + 100.0 * Vec3::new(s(u.x, c), s(u.y, c * 0.5), s(u.z, c)).normalize_or_zero()
}

/// An aimed shot's point: `base` moved by 100 x (U(−c, c), U(−c/2, c/2),
/// U(−c, c)) for draws `u` in [0, 1]³ (7.3 step 6).
pub fn hit_point(base: Vec3, c: f32, u: Vec3) -> Vec3 {
    if c <= 0.0 {
        return base;
    }
    let s = |u: f32, r: f32| (u * 2.0 - 1.0) * r;
    base + 100.0 * Vec3::new(s(u.x, c), s(u.y, c * 0.5), s(u.z, c))
}

/// A missed shot's point (7.3 step 7): `offset` (u) to one side of the
/// line from `origin` to `base` (`right`: the sign), raised by `up`
/// (U(−5, 15)).
pub fn miss_point(origin: Vec3, base: Vec3, offset: f32, right: bool, up: f32) -> Vec3 {
    let d = (base - origin).normalize_or_zero();
    let r = d.cross(Vec3::Y);
    let mut s = base + if right { r } else { -r } * offset;
    s.y += up;
    s
}

/// Whether the target is close enough for exact aim (Quirk 5: the
/// full-accuracy value against the squared distance).
pub fn full_accuracy(distance: f32, value: f32) -> bool {
    distance * distance < value
}

// ---------------------------------------------------------------------------
// Movement inaccuracy (7.5)

/// The error that shrinks while tracking a target (u, per unit of the
/// 100 u scale).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Tracking {
    pub c: f32,
    pub prev: Option<Vec3>,
    pub velocity: Vec3,
}

impl Tracking {
    /// A new target: c at its cap.
    pub fn select(&mut self, c_max: f32) {
        *self = Tracking {
            c: c_max,
            prev: None,
            velocity: Vec3::ZERO,
        };
    }

    /// One visibility update: target at `p` (u), `dt` s since the last.
    pub fn update(&mut self, p: Vec3, dt: f32, c_max: f32, k: f32) {
        let Some(prev) = self.prev else {
            self.prev = Some(p);
            return;
        };
        if dt <= 0.0 {
            return;
        }
        let predicted = prev + self.velocity * dt;
        let err = p.distance(predicted);
        self.c = err.min(c_max).max(self.c - k * dt).max(0.0);
        self.velocity = (p - prev) / dt;
        self.prev = Some(p);
    }
}

// ---------------------------------------------------------------------------
// Rolls (5.2, 5.3)

/// The notice roll's percent at distance `d` (5.2): 100 up to `near`,
/// linear to `far_percent` at `far`, plus the camping bonus (percent
/// points).
pub fn notice_percent(d: f32, near: f32, far: f32, far_percent: f32, bonus: f32) -> f32 {
    let p = if d < near {
        100.0
    } else if d <= far {
        100.0 - (d - near) * (100.0 - far_percent) / (far - near)
    } else {
        far_percent
    };
    p + bonus
}

/// A percent roll: fails if `r100` > `p`.
pub fn roll(p: f32, r100: u32) -> bool {
    (r100 as f32) <= p
}

/// Hearing's percent at distance `d`: 100 up to 100 u, linear to `far_p`
/// at `far` (footsteps 60 at 800, gunfire 50 at 1000), then `far_p`.
pub fn hearing_percent(d: f32, far: f32, far_p: f32) -> f32 {
    notice_percent(d, 100.0, far, far_p, 0.0)
}

// ---------------------------------------------------------------------------
// Weapons (8.2, 8.5)

/// A ranged weapon a bot may choose: it has ammunition, its range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Choice {
    pub has_ammo: bool,
    pub range_max: f32,
}

/// Which weapon (8.2): 0 mid primary, 1 long primary, 2 secondary, 3
/// melee, for a target at `d`.
pub fn choose_weapon(mid: Option<Choice>, long: Option<Choice>, secondary: Option<Choice>, d: f32) -> usize {
    let mut chosen = None;
    if let Some(m) = mid
        && m.has_ammo
    {
        chosen = Some(0);
    }
    if let Some(l) = long
        && l.has_ammo
        && (chosen.is_none() || mid.is_some_and(|m| d * d > 0.8 * m.range_max * m.range_max))
    {
        chosen = Some(1);
    }
    if chosen.is_none()
        && let Some(s) = secondary
        && s.has_ammo
    {
        chosen = Some(2);
    }
    chosen.unwrap_or(3)
}

/// The throw's velocity (8.5): hang time `t_h`, gravity `g` (u/s², down
/// negative), the key's power `k` and angle `angle` (rad about +Y).
pub fn throw_velocity(from: Vec3, to: Vec3, t_h: f32, g: f32, k: f32, angle: f32) -> Vec3 {
    let gv = Vec3::new(0.0, g, 0.0);
    let v = (to - from - 0.5 * gv * t_h * t_h) / t_h * k;
    Quat::from_rotation_y(angle) * v
}

// ---------------------------------------------------------------------------
// Attack moves (9.1), patrol (4)

/// One of the seven attack moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    ShuffleLeft,
    BackUp,
    ShuffleRight,
    FlankLeft,
    FlankRight,
}

/// The seven picks in order (back-up 3 of 7).
pub const MOVES: [Move; 7] = [
    Move::ShuffleLeft,
    Move::BackUp,
    Move::ShuffleRight,
    Move::BackUp,
    Move::FlankLeft,
    Move::BackUp,
    Move::FlankRight,
];

/// Where move `m` goes from `pos` facing `forward` (horizontal, unit),
/// with the template's shuffle distance and the bot's radius (u); a flank
/// goes back (`flank_back`) or forward by `flank_forward`.
pub fn move_target(m: Move, pos: Vec3, forward: Vec3, shuffle: f32, radius: f32, flank_back: bool, flank_forward: f32) -> Vec3 {
    let right = forward.cross(Vec3::Y).normalize_or_zero();
    match m {
        Move::ShuffleLeft => pos - right * (shuffle + radius),
        Move::ShuffleRight => pos + right * (shuffle + radius),
        Move::BackUp => pos - forward * (50.0 + radius),
        Move::FlankLeft | Move::FlankRight => {
            let side = if m == Move::FlankLeft { -right } else { right } * 50.0;
            let along = if flank_back { -forward * 50.0 } else { forward * flank_forward };
            pos + side + along
        }
    }
}

/// A patrol link's score (4): lowest wins.
pub fn patrol_score(t_used_ms: f64, forward: Vec3, dir: Vec3) -> f64 {
    let dot = forward.normalize_or_zero().dot(dir.normalize_or_zero()).max(0.0) as f64;
    (t_used_ms - 10000.0 * dot).max(0.0)
}

/// The link to take among `(t_used_ms, direction)` (ties: the first).
pub fn patrol_choice(links: &[(f64, Vec3)], forward: Vec3) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    for (i, &(t, d)) in links.iter().enumerate() {
        let s = patrol_score(t, forward, d);
        if best.is_none_or(|(_, b)| s < b) {
            best = Some((i, s));
        }
    }
    best.map(|b| b.0)
}

/// Whether a bot stops at a patrol node (4): a 0..99 draw at or below the
/// template's `StopLookAroundPercentage`.
pub fn patrol_stops(draw: u32, percent: f32) -> bool {
    draw as f32 <= percent
}

/// The patrol mine timer (4): U(min, max) for a draw `u`.
pub fn mine_timer(min: f32, max: f32, u: f32) -> f32 {
    min + u * (max - min)
}

/// Whether a placed device is seen at all (5.2 step 1): not within its
/// blast radius times the template's safe factor.
pub fn device_seen(distance: f32, blast_radius: f32, safe: f32) -> bool {
    distance >= blast_radius * safe
}

/// The attack chase timer (5.6): 10.5 + U(−10.5, 5) for a draw `u`.
pub fn attack_chase_timer(u: f32) -> f32 {
    10.5 + (-10.5 + u * 15.5)
}

// ---------------------------------------------------------------------------
// Dice

/// The bots' dice: xorshift64*, seeded per bot (never from entity ids).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dice(pub u64);

impl Dice {
    pub fn new(seed: u64) -> Self {
        Dice(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// U(a, b).
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.unit()
    }

    /// Ri(a, b) inclusive.
    pub fn int(&mut self, a: u32, b: u32) -> u32 {
        if b <= a {
            return a;
        }
        a + (self.next_u64() % (b - a + 1) as u64) as u32
    }

    /// r100: 0-99.
    pub fn r100(&mut self) -> u32 {
        (self.next_u64() % 100) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycles() {
        assert_eq!(cycle(0.17), (1, 5));
        assert_eq!(cycle(0.46), (1, 1));
        assert_eq!(cycle(0.8), (4, 1));
        assert_eq!(cycle(0.95), (19, 1));
        assert_eq!(cycle(0.14), (1, 6));
        assert_eq!(cycle(0.08), (1, 12));
    }

    #[test]
    fn throw_solution() {
        let v = throw_velocity(Vec3::new(0.0, 60.0, 0.0), Vec3::new(0.0, 60.0, 800.0), 0.5, -1600.0, 1.0, 0.0);
        assert!((v - Vec3::new(0.0, 400.0, 1600.0)).length() < 1e-3);
    }
}

/// The bot's fire interval by weapon class (`AniType`; 8.3, Open question
/// 3): its NPC fire animation loops with one fire key at 0, so the
/// interval is that animation's length (ms): rifles, SMGs and pistols
/// 100, snipers 1000, automatic and pump shotguns 466, the double barrel
/// 1000, rocket launchers 1200, the chain gun 133, knives 500.
pub fn fire_interval_ms(ani_type: i32) -> u32 {
    match ani_type {
        5 | 27 => 1000,
        6 | 7 | 15 | 29 => 466,
        12 | 19 => 1200,
        28 => 133,
        14 | 16 => 500,
        _ => 100,
    }
}

/// The footstep keys (ms) of a leg animation (5.3, Open question 18): the
/// run (`LR` and its directions) at 50 and 350, the sprint (`LRR`) at 63
/// and 312, the jumps (`LSJ`, `LJ`) at their landing, 600; walking and
/// crouching carry none (silent to bots).
pub fn footstep_keys(name: &str) -> &'static [u32] {
    let name = name.trim_start_matches("VG").trim_start_matches('V');
    if name.starts_with("LRR") {
        &[63, 312]
    } else if name.starts_with("LR") {
        &[50, 350]
    } else if name.starts_with("LSJ") || name.starts_with("LJ") {
        &[600]
    } else {
        &[]
    }
}

/// Whether a tracker that went from `before` to `now` ms (wrapping at
/// `length` when looping) passed a key at `key` ms.
pub fn passed_key(before: u32, now: u32, length: u32, key: u32) -> bool {
    if now >= before {
        key > before && key <= now
    } else {
        // Wrapped round a looping animation.
        (key > before && key <= length) || key <= now
    }
}

/// A lower-threshold sense (5.1, 5.3: footsteps, threshold 0.5-1.0): it
/// fires on reaching 1.0; a rise past 0.5 that decays back below 0.5
/// without firing is a false stimulation, and the third makes it fire
/// (`FalseStimulationLimit` 3, the counter restarted).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sense {
    pub level: f32,
    pub false_count: u8,
    above: bool,
}

impl Sense {
    pub const MIN: f32 = 0.5;
    pub const MAX: f32 = 1.0;
    pub const FALSE_LIMIT: u8 = 3;

    /// A stimulus adds `x`; true when the sense fires (it is then reset).
    pub fn add(&mut self, x: f32) -> bool {
        self.level += x;
        if self.level >= Self::MAX - 1e-6 {
            self.level = 0.0;
            self.above = false;
            return true;
        }
        if self.level >= Self::MIN - 1e-6 {
            self.above = true;
        }
        false
    }

    /// A pass without one decays it by `d`; true when a third false
    /// stimulation makes it fire.
    pub fn decay(&mut self, d: f32) -> bool {
        self.level = (self.level - d).max(0.0);
        if self.above && self.level < Self::MIN - 1e-6 {
            self.above = false;
            self.false_count += 1;
            if self.false_count >= Self::FALSE_LIMIT {
                self.false_count = 0;
                self.level = 0.0;
                return true;
            }
        }
        false
    }
}

/// The grenade dodge's directions (5.4, Open question 19): straight away
/// from the grenade, 45° and 90° to one side, then 90° and 45° to the
/// other (the three toward it never), as eighths of a turn.
pub const DODGE_STEPS: [i32; 5] = [0, 1, 2, -2, -1];

/// The chat roll (10): the line shows when the milliseconds since start
/// mod 100 are at most the event's percent.
pub fn chat_roll(ms: u64, percent: u32) -> bool {
    (ms % 100) as u32 <= percent
}
