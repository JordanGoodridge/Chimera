//! The death camera's phases, targets, keys and the respawn countdown
//! (specs/combat_arms/death_killcam.md 2, 5, 6, 7, 9), as a state machine
//! with no engine in it: one `frame` a rendered frame, the world and the
//! players handed in. Ids are the client ids (`CaSlot`) of the players
//! (and an object's key for a mercenary the enemy view follows); the
//! positions are the game's own frame and units (`math`).

use bevy::prelude::*;

use super::math::{self, Clip, Pose, PullBack};

/// A client id, or an object's key.
pub type Id = u32;

/// The pull-back's length and the hold before it in Hunted (`DyingTime`),
/// s.
pub const DYING_TIME: f64 = 3.5;
/// After this long since the death the camera leaves the killer, once
/// (`DeadCameraTime`), s.
pub const KILLER_VIEW_END: f64 = 8.0;
/// Hunted's static free camera starts this far above the body (u).
pub const STATIC_HEIGHT: f32 = 1000.0;
/// A free camera's acceleration, u/s²; doubled with run; Hunted adds
/// `HUNTED_BOOST` on every map load (Quirk 8).
pub const FREE_ACCEL: f32 = 300_000.0;
pub const HUNTED_BOOST: f32 = 200_000.0;
/// A free camera's velocity damping per frame, and the speed below which
/// it stops (u/s).
pub const FREE_DAMPING: f32 = 0.9;
pub const FREE_STOP: f32 = 0.1;
/// The countdown text's refresh period (s).
pub const TEXT_REFRESH: f64 = 0.040;
/// The client's respawn wait (ms; the reference build's edit is 2000).
pub const RESPAWN_WAIT_MS: f64 = 7000.0;
/// The team id of observers.
pub const OBSERVER: u8 = 255;
/// A fall or the world has no killer: the id 255 (Quirk 4).
pub const NO_KILLER: u32 = 255;

/// A player (or an object the camera may follow) as the camera sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cand {
    pub id: Id,
    pub team: u8,
    /// Not dead and with an object.
    pub alive: bool,
    /// Where it is (the game's frame).
    pub at: Vec3,
}

impl Cand {
    fn watchable(&self) -> bool {
        self.alive && self.team != OBSERVER
    }
}

/// The next player to watch from `from` (5): after it in id order in team
/// `team` (0: anyone), else the first from the start; else, with the enemy
/// view, the first living of the other team.
pub fn choose_next(players: &[Cand], from: Option<Id>, team: u8, enemy_view: bool) -> Option<Id> {
    let in_team = |c: &&Cand| c.watchable() && (team == 0 || c.team == team);
    let after = from.and_then(|x| players.iter().filter(in_team).find(|c| c.id > x));
    if let Some(c) = after.or_else(|| players.iter().find(in_team)) {
        return Some(c.id);
    }
    if !enemy_view {
        return None;
    }
    // Quirk 7: team 1 for any team id but 1, 2 or 255.
    let other = match team {
        1 => 2,
        2 => 1,
        OBSERVER => 0,
        _ => 1,
    };
    players
        .iter()
        .find(|c| c.watchable() && (other == 0 || c.team == other))
        .map(|c| c.id)
}

/// How the player was killed (2).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Killer {
    /// No death yet.
    #[default]
    Unknown,
    /// Own grenade, telefrag: no pull-back.
    Suicide,
    /// A fall or the world (id 255): counts as known for the pull-back.
    World,
    /// Another player; `object` is the character that did the damage when
    /// it isn't the player's own object (a Hired Guns mercenary).
    Player { id: Id, object: Option<Id> },
}

impl Killer {
    fn known(&self) -> bool {
        !matches!(self, Killer::Suicide)
    }

    pub fn id(&self) -> Option<Id> {
        match self {
            Killer::Player { id, .. } => Some(*id),
            _ => None,
        }
    }
}

/// The phases after a death (2, 7).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Phase {
    #[default]
    None,
    /// Hunted: the view rides the body's `Eyes` for 3.5 s.
    Hold,
    PullBack,
    Watch,
    /// Hunted's free-flying camera (Ctrl).
    FreeFly,
    /// Hunted's static free camera (Shift).
    FreeStatic,
}

/// What the player did this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Input {
    /// The mouse look in radians as the shell gives it (yaw left, pitch
    /// up).
    pub look: Vec2,
    /// The wheel's notches, positive away from the player.
    pub wheel: f32,
    /// Keys pressed this frame.
    pub space: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub fast: bool,
    /// Movement keys held (x right, y forward) and run, for a free camera.
    pub axis: Vec2,
    pub run: bool,
    /// The frame's length, s.
    pub dt: f32,
}

/// What the machine reads of the world each frame.
pub struct Frame<'a> {
    /// Game time, s.
    pub now: f64,
    /// The own player is dead and waiting for the next life ("dead"), not
    /// alive or held.
    pub dead: bool,
    /// Held joiners and observers ("waiting"): a free camera of its own.
    pub waiting: bool,
    pub me: Id,
    pub my_team: u8,
    /// The dead body (the floor-snapped player object) and the last damage
    /// report's direction (the way the damage travelled).
    pub body: Vec3,
    pub damage: Vec3,
    /// Players in ascending id order, and the objects the camera may
    /// follow besides.
    pub players: &'a [Cand],
    pub objects: &'a [Cand],
    pub enemy_view: bool,
    /// Last Man Standing and Hunted: any team.
    pub any_team: bool,
    pub hunted: bool,
    /// The mode counts a respawn countdown, its wait (ms, Relentless taken
    /// in) and whether F7 may ask now (the mode, a fast respawn left).
    pub counts: bool,
    pub wait_ms: f64,
    pub fast_ok: bool,
    /// The dead player's view when it died, for Hunted's hold.
    pub view: (f32, f32),
    pub input: Input,
    pub clip: &'a dyn Clip,
}

impl Frame<'_> {
    fn find(&self, id: Id) -> Option<&Cand> {
        self.players.iter().chain(self.objects).find(|c| c.id == id)
    }

    fn team(&self) -> u8 {
        if self.any_team { 0 } else { self.my_team }
    }

    fn next(&self, from: Option<Id>) -> Option<Id> {
        choose_next(self.players, from, self.team(), self.enemy_view)
    }
}

/// The respawn countdown (9).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Countdown {
    /// Game time it started, s.
    pub started: f64,
    /// The wait W, ms.
    pub wait_ms: f64,
    /// The text last drawn, s, and its string.
    shown: f64,
}

impl Countdown {
    pub fn new(now: f64, wait_ms: f64) -> Self {
        Self {
            started: now,
            wait_ms,
            shown: f64::MIN,
        }
    }

    /// The gauge's total G = ⌊W / 1000⌋ s (Quirk 3).
    pub fn total(&self) -> u32 {
        (self.wait_ms / 1000.0).floor() as u32
    }

    pub fn elapsed(&self, now: f64) -> f64 {
        (now - self.started).max(0.0)
    }

    /// The bar, whole percent: ⌊100 e / G⌋, 100 once e >= G.
    pub fn gauge(&self, now: f64) -> u32 {
        let g = self.total() as f64;
        let e = self.elapsed(now);
        if g <= 0.0 || e >= g {
            return 100;
        }
        ((100.0 * e / g + 1e-9).floor() as u32).min(100)
    }

    /// The text "SS:hh", `00:00` from G on.
    pub fn text(&self, now: f64) -> String {
        let g = self.total() as f64;
        let e = self.elapsed(now);
        if e >= g {
            return "00:00".into();
        }
        let r = g - e;
        let cs = (r * 100.0 + 1e-6).floor() as i64;
        format!("{:02}:{:02}", cs / 100, cs % 100)
    }

    /// The text to show, refreshed at most every 40 ms.
    pub fn refresh(&mut self, now: f64) -> Option<String> {
        if now - self.shown < TEXT_REFRESH && self.elapsed(now) < self.total() as f64 {
            return None;
        }
        self.shown = now;
        Some(self.text(now))
    }

    /// The request is due: strictly more than W ms since the start (the
    /// death time; the countdown starts at the death).
    pub fn due(&self, now: f64) -> bool {
        self.elapsed(now) * 1000.0 > self.wait_ms + 1e-6
    }
}

/// A free camera (7).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Free {
    pub pose: Pose,
    vel: Vec3,
}

/// The death camera (the client's), one per machine.
#[derive(Clone, Debug, Default)]
pub struct Machine {
    pub phase: Phase,
    /// The death time (game s) and the "dead" state's flags.
    t0: Option<f64>,
    first_watch: bool,
    switched: bool,
    hold_done: bool,
    pub killer: Killer,
    pub target: Option<Id>,
    /// The enemy view's target (the killer) was chosen for the camera.
    pub on_killer: bool,
    /// The dead player's view yaw ψ and pitch θ (4) and the zoom.
    pub psi: f32,
    pub theta: f32,
    pub zoom: f32,
    saved_view: (f32, f32),
    pull: Option<PullBack>,
    /// What the camera shows now (None: nothing placed yet).
    pub pose: Option<Pose>,
    free: Free,
    free_used: bool,
    pub static_last: Option<Pose>,
    pub countdown: Option<Countdown>,
    asked: bool,
    fast: bool,
    /// The request the frame sent (fast flag), taken by the sender.
    pub request: Option<bool>,
    /// The countdown's tick counter, shared across deaths (Quirk 9); the
    /// seconds whose tick sounded.
    pub ticks: Vec<i64>,
    last_tick: i64,
    /// The free cameras' speed boost (Hunted, per map load).
    pub boost: f32,
    prev_target: Option<Id>,
    /// The countdown's text as last refreshed.
    pub text: Option<String>,
    /// A held joiner's or observer's free camera is on.
    waiting_on: bool,
}

impl Machine {
    /// The own player is alive again (or never was dead): everything of
    /// the death cleared.
    pub fn alive(&mut self) {
        let keep = (self.ticks.clone(), self.last_tick, self.boost, self.static_last);
        *self = Machine {
            ticks: keep.0,
            last_tick: keep.1,
            boost: keep.2,
            static_last: keep.3,
            ..Default::default()
        };
    }

    /// A new map: Hunted's boost grows, the static camera forgets.
    pub fn map_loaded(&mut self, hunted: bool) {
        if hunted {
            self.boost += HUNTED_BOOST;
        }
        self.static_last = None;
    }

    /// The "dead" state came (1): the death time, the kill message's facts
    /// and the view at that moment.
    pub fn died(&mut self, now: f64, killer: Killer, view: (f32, f32)) {
        let ticks = std::mem::take(&mut self.ticks);
        let (last, boost, stat) = (self.last_tick, self.boost, self.static_last);
        *self = Machine {
            t0: Some(now),
            first_watch: true,
            killer,
            saved_view: view,
            psi: view.0,
            theta: view.1,
            ticks,
            last_tick: last,
            boost,
            static_last: stat,
            ..Default::default()
        };
    }

    /// "Waiting" (1, 10): a held joiner's or an observer's free
    /// first-person camera, starting 50 u over the player's object.
    pub fn wait_frame(&mut self, f: &Frame) {
        if !self.waiting_on {
            self.waiting_on = true;
            self.free = Free {
                pose: Pose {
                    at: f.body + Vec3::Y * 50.0,
                    heading: f.view.0,
                    pitch: f.view.1,
                },
                vel: Vec3::ZERO,
            };
        }
        self.fly(f);
    }

    /// Dead and running.
    pub fn is_dead(&self) -> bool {
        self.t0.is_some()
    }

    /// The death time (game s).
    pub fn t0(&self) -> Option<f64> {
        self.t0
    }

    /// Milliseconds since the death clock started.
    fn t(&self, now: f64) -> f64 {
        (now - self.t0.unwrap_or(now)) * 1000.0
    }

    /// One rendered frame (per-frame order, 2 and 3).
    pub fn frame(&mut self, f: &Frame) {
        self.player_update(f);
        self.camera_update(f);
    }

    /// The player update (per-frame order step 2): the respawn request,
    /// the phase machine, the automatic target choice.
    pub fn player_update(&mut self, f: &Frame) {
        if !f.dead || self.t0.is_none() {
            return;
        }
        self.request = None;
        if f.input.fast && f.fast_ok && self.countdown.is_some() && !self.asked {
            self.fast = true;
        }
        // The respawn request (9).
        if let Some(c) = self.countdown
            && !self.asked
            && (c.due(f.now) || self.fast)
        {
            self.asked = true;
            self.request = Some(self.fast);
            // The phase starts over with the original death time (Quirk
            // 12), the 8 s end armed again.
            self.phase = Phase::None;
            self.pull = None;
            self.switched = false;
        }
        let t = self.t(f.now);
        match self.phase {
            Phase::None => self.start(f),
            Phase::Hold => {
                if t >= DYING_TIME * 1000.0 - 1e-6 {
                    self.t0 = Some(f.now);
                    self.hold_done = true;
                    self.begin_pull_back(f);
                }
            }
            Phase::PullBack => {
                if t >= DYING_TIME * 1000.0 - 1e-6 {
                    self.enter_watch(f);
                }
            }
            Phase::Watch | Phase::FreeFly | Phase::FreeStatic => {}
        }
        // The target is chosen again whenever it is dead or is the own
        // player (5).
        if self.phase == Phase::Watch
            && let Some(t) = self.target
            && f.find(t).is_none_or(|c| !c.alive || c.id == f.me)
        {
            self.on_killer = false;
            self.target = f.next(Some(t));
        }
        // The countdown's start, gauge and ticks.
        if f.counts && self.countdown.is_none() && !self.asked {
            self.countdown = Some(Countdown::new(f.now, f.wait_ms));
        }
        if let Some(c) = self.countdown.as_mut() {
            self.text = c.refresh(f.now).or(self.text.take());
            if !self.asked {
                let e = c.elapsed(f.now);
                if e < c.total() as f64 {
                    let whole = e.floor() as i64;
                    if whole != self.last_tick {
                        self.last_tick = whole;
                        self.ticks.push(whole);
                    }
                }
            }
        }
    }

    /// The first frame dead, or the restart after the request: the phase
    /// from the kill (2 step 1).
    fn start(&mut self, f: &Frame) {
        if self.killer.known() {
            if f.hunted && !self.hold_done {
                self.phase = Phase::Hold;
            } else {
                self.begin_pull_back(f);
            }
        } else {
            // A suicide: the watch at once, the killer step skipped.
            self.first_watch = false;
            self.phase = Phase::Watch;
            self.set_target(f.next(None));
        }
    }

    fn begin_pull_back(&mut self, f: &Frame) {
        let pb = PullBack::begin(f.body);
        self.phase = Phase::PullBack;
        self.pose = Some(Pose {
            at: pb.at,
            heading: self.pose.map_or(f.view.0, |p| p.heading),
            pitch: self.pose.map_or(f.view.1, |p| p.pitch),
        });
        self.pull = Some(pb);
    }

    /// Pull-back done: the watch, on the killer (the enemy view) or the
    /// next living player (2 step 3, with its second look in the same
    /// frame).
    fn enter_watch(&mut self, f: &Frame) {
        self.phase = Phase::Watch;
        let first = std::mem::take(&mut self.first_watch);
        let killer = match self.killer {
            Killer::Player { id, object } if first && id != f.me => Some((id, object)),
            _ => None,
        };
        let Some((id, object)) = killer else {
            self.on_killer = false;
            let from = self.target;
            self.set_target(f.next(from));
            return;
        };
        let alive = f.find(id).is_some_and(|c| c.alive);
        if alive && f.enemy_view {
            self.on_killer = true;
            self.set_target(Some(object.unwrap_or(id)));
        } else {
            self.on_killer = false;
            self.set_target(f.next(Some(id)));
        }
    }

    fn set_target(&mut self, t: Option<Id>) {
        self.target = t;
    }

    /// The camera update (step 3).
    pub fn camera_update(&mut self, f: &Frame) {
        if !f.dead || self.t0.is_none() {
            return;
        }
        let i = f.input;
        // The dead player's yaw and pitch from the mouse; in Hunted's
        // pull-back they are reset to the saved view every frame.
        let hunted_pull = f.hunted && self.phase == Phase::PullBack;
        if hunted_pull {
            (self.psi, self.theta) = self.saved_view;
        } else if !matches!(self.phase, Phase::FreeFly | Phase::FreeStatic) {
            (self.psi, self.theta) = math::turn(self.psi, self.theta, i.look);
        }
        self.keys(f);
        if self.target != self.prev_target {
            self.zoom = 0.0;
            self.prev_target = self.target;
        }
        if self.phase == Phase::Watch {
            self.zoom = math::zoom(self.zoom, i.wheel);
        }
        match self.phase {
            Phase::None => {}
            Phase::Hold => {
                // PLACEHOLDER for the body's `Eyes` socket: the body.
                self.pose = Some(Pose {
                    at: f.body,
                    heading: self.saved_view.0,
                    pitch: self.saved_view.1,
                });
            }
            Phase::PullBack => {
                if let Some(pb) = self.pull.as_mut() {
                    pb.step(f.body, f.damage, f.clip);
                    let (heading, pitch) = pb.look.unwrap_or_else(|| {
                        self.pose.map_or(self.saved_view, |p| (p.heading, p.pitch))
                    });
                    self.pose = Some(Pose { at: pb.at, heading, pitch });
                }
            }
            Phase::Watch => {
                if let Some(c) = self.target.and_then(|t| f.find(t)) {
                    self.pose = Some(math::orbit(c.at, self.psi, self.theta, self.zoom, f.clip));
                }
            }
            Phase::FreeFly | Phase::FreeStatic => self.fly(f),
        }
        // The killer-view end (2 step 4), once.
        if !self.switched && self.t(f.now) > KILLER_VIEW_END * 1000.0 && !matches!(self.phase, Phase::None | Phase::Hold) {
            self.switched = true;
            let from = self.target;
            self.on_killer = false;
            self.set_target(f.next(from));
            self.phase = Phase::Watch;
        }
    }

    /// Keys while dead (6, 7).
    fn keys(&mut self, f: &Frame) {
        let i = f.input;
        if f.hunted {
            if i.ctrl && !self.free_used && self.phase == Phase::Watch {
                self.free_used = true;
                self.free.pose = self.pose.unwrap_or(Pose {
                    at: f.body,
                    heading: self.saved_view.0,
                    pitch: self.saved_view.1,
                });
                self.free.vel = Vec3::ZERO;
                self.phase = Phase::FreeFly;
            } else if i.shift && matches!(self.phase, Phase::Watch | Phase::FreeFly) {
                self.free_used = true;
                self.free.pose = self.static_last.unwrap_or_else(|| {
                    let p = self.pose.unwrap_or_default();
                    Pose {
                        at: f.body + Vec3::Y * STATIC_HEIGHT,
                        heading: p.heading,
                        pitch: p.pitch + std::f32::consts::FRAC_PI_2,
                    }
                });
                self.free.vel = Vec3::ZERO;
                self.phase = Phase::FreeStatic;
            } else if i.space && matches!(self.phase, Phase::Watch | Phase::FreeFly | Phase::FreeStatic) {
                if self.phase == Phase::FreeStatic {
                    self.static_last = Some(self.free.pose);
                }
                self.phase = Phase::Watch;
                self.on_killer = false;
                self.set_target(f.next(self.target));
            }
        } else if self.phase == Phase::Watch && (i.space || i.ctrl || i.shift) {
            self.on_killer = false;
            self.set_target(f.next(self.target));
        }
    }

    /// A free camera: turned by the mouse, flown by the movement keys.
    fn fly(&mut self, f: &Frame) {
        let i = f.input;
        let p = &mut self.free.pose;
        p.heading -= i.look.x * math::MOUSE_FACTOR;
        if i.look.y != 0.0 {
            p.pitch = (p.pitch - i.look.y * math::MOUSE_FACTOR).clamp(-89f32.to_radians(), 89f32.to_radians());
        }
        let forward = Vec3::new(p.heading.sin() * p.pitch.cos(), -p.pitch.sin(), p.heading.cos() * p.pitch.cos());
        let right = Vec3::new(p.heading.cos(), 0.0, -p.heading.sin());
        let accel = (FREE_ACCEL + self.boost) * if i.run { 2.0 } else { 1.0 };
        // The velocity is zeroed each movement frame; the keys accelerate
        // it, then it is damped (Open question 11: the step is the frame).
        let dir = forward * i.axis.y + right * i.axis.x;
        let mut v = dir * accel * i.dt * FREE_DAMPING;
        if v.length() < FREE_STOP {
            v = Vec3::ZERO;
        }
        self.free.vel = v;
        p.at += v * i.dt;
        self.pose = Some(*p);
    }
}
