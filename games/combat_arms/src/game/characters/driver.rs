//! The third-person animation driver (specs/combat_arms/characters.md
//! sections 4 to 7 and 9): each update the four trackers advance, the
//! posture (upper body) and movement (legs) are decided from the weapon
//! status, the fire buttons and the movement flags, the animation names
//! are built from the weapon class, stance, action and direction, and
//! the trackers are started, restarted and kept in step by section 7's
//! rules, on LithTech's own millisecond trackers (`lithtech_formats::
//! anim::Tracker`) over the body's combined animation list.
//!
//! A name the library lacks starts nothing (Quirk 1). Without a body
//! model the library is `spec_library`: every name section 6 can build,
//! with the spec's measured lengths, minus the names it says the US
//! libraries lack.

use std::sync::{Arc, OnceLock};

use lithtech_formats::{
    anim::{AnimList, Entry, Tracker},
    ltb::{Animation, Key},
};
use serde::{Deserialize, Serialize};

/// The upper body's action (5, "Posture").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Posture {
    #[default]
    Aim,
    PreFire,
    Fire,
    PostFire,
    Select,
    Reload,
    Hit,
    Hit2,
    AltFire,
}

impl Posture {
    /// `HHAniFrame_#`'s posture index (7).
    pub fn frame_index(self) -> usize {
        match self {
            Posture::Aim => 0,
            Posture::PreFire => 1,
            Posture::Fire => 2,
            Posture::PostFire => 3,
            Posture::Select => 4,
            Posture::Reload => 5,
            Posture::Hit => 6,
            Posture::Hit2 => 7,
            Posture::AltFire => 9,
        }
    }

    fn looping(self) -> bool {
        self == Posture::Aim
    }
}

/// The legs' movement (5, "Movement").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Movement {
    #[default]
    Walk,
    Run,
    Sprint,
    Jump,
}

/// The direction from the movement keys.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Direction {
    #[default]
    None,
    F,
    B,
    L,
    R,
    FL,
    FR,
    BL,
    BR,
}

impl Direction {
    /// From the keys: right strafe with forward or backward gives the
    /// diagonals; both strafes held, the left one wins.
    pub fn of(forward: bool, back: bool, left: bool, right: bool) -> Self {
        let side = if left {
            Some(true)
        } else if right {
            Some(false)
        } else {
            None
        };
        match (side, forward, back) {
            (None, true, _) => Direction::F,
            (None, false, true) => Direction::B,
            (None, false, false) => Direction::None,
            (Some(l), true, _) => {
                if l {
                    Direction::FL
                } else {
                    Direction::FR
                }
            }
            (Some(l), false, true) => {
                if l {
                    Direction::BL
                } else {
                    Direction::BR
                }
            }
            (Some(l), false, false) => {
                if l {
                    Direction::L
                } else {
                    Direction::R
                }
            }
        }
    }

    /// The upper body's direction suffix: diagonals take forward or back.
    fn upper(self) -> &'static str {
        match self {
            Direction::None => "S",
            Direction::F | Direction::FL | Direction::FR => "F",
            Direction::B | Direction::BL | Direction::BR => "B",
            Direction::L => "L",
            Direction::R => "R",
        }
    }

    fn legs(self) -> &'static str {
        match self {
            Direction::None => "",
            Direction::F => "F",
            Direction::B => "B",
            Direction::L => "L",
            Direction::R => "R",
            Direction::FL => "FL",
            Direction::FR => "FR",
            Direction::BL => "BL",
            Direction::BR => "BR",
        }
    }
}

/// The weapon status the client reports (5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    #[default]
    None,
    Select,
    Deselect,
    Idle,
    Hit,
    /// The weapon reloads: no firing choice until the reload ends.
    Reloading,
}

/// The jump status (5, "Movement").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Jump {
    #[default]
    None,
    Jumped,
    Falling,
}

/// What the driver is given each update.
#[derive(Clone, Debug)]
pub struct Inputs {
    /// The current weapon's `AniType` (14 without a weapon) and its leg
    /// set (`LowerAniType`).
    pub class: i32,
    pub legs: i32,
    /// A status reported since the last update.
    pub status: Option<Status>,
    /// The weapon is reloading, and whether that reload may finish (it
    /// left the reload state).
    pub reloading: bool,
    pub reload_may_finish: bool,
    pub fire: bool,
    pub alt_fire: bool,
    /// The current ammo count is 0.
    pub empty: bool,
    pub run: bool,
    pub sprint: bool,
    pub zoomed: bool,
    /// A jump started, or the player landed, since the last update.
    pub jumped: bool,
    pub landed: bool,
    pub crouch_key: bool,
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    /// The crouch fraction r.
    pub crouch: f32,
    /// The last damage was above 0, and spawn protection.
    pub damaged: bool,
    pub protected: bool,
    /// The hit reaction's coin: `D_HIT` when set, else `D_HITT`.
    pub coin: bool,
    /// `HHAniFrame_0..9` (ms, 0: none) and the weapon rates per posture
    /// (reload, select, fire) for the upper rate (7).
    pub frames: Vec<u32>,
    pub rates: Rates,
}

impl Default for Inputs {
    fn default() -> Self {
        Self {
            class: 0,
            legs: 0,
            status: None,
            reloading: false,
            reload_may_finish: false,
            fire: false,
            alt_fire: false,
            empty: false,
            run: true,
            sprint: false,
            zoomed: false,
            jumped: false,
            landed: false,
            crouch_key: false,
            forward: false,
            back: false,
            left: false,
            right: false,
            crouch: 0.0,
            damaged: false,
            protected: false,
            coin: true,
            frames: Vec::new(),
            rates: Rates::default(),
        }
    }
}

/// The weapon rate r_w of each posture kind (7).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rates {
    pub reload: f32,
    pub select: f32,
    pub fire: f32,
}

impl Default for Rates {
    fn default() -> Self {
        Self {
            reload: 1.0,
            select: 1.0,
            fire: 1.0,
        }
    }
}

/// The upper rate (7): L r_w / F for a weapon whose `HHAniFrame` for the
/// posture is F > 0, else 1.
pub fn upper_rate(length: u32, rate: f32, frame: u32) -> f32 {
    if frame > 0 { length as f32 * rate / frame as f32 } else { 1.0 }
}

/// Which animations a state names (6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Names {
    pub upper: String,
    /// The walk and sprint variants, used when they exist.
    pub upper_walk: Option<String>,
    pub upper_sprint: Option<String>,
    pub lower: Option<String>,
    pub crouch: String,
}

/// The class code and its reload/fire digit (6, "Class code").
pub fn class_code(class: i32) -> (&'static str, &'static str) {
    match class {
        0 | 1 => ("RF", ""),
        2 => ("RH", ""),
        3 => ("SM", ""),
        4 => ("SH", ""),
        5 => ("SN", ""),
        6 => ("SG", ""),
        7 => ("G", ""),
        8 => ("M", ""),
        9 => ("V", ""),
        10 => ("P", ""),
        11 => ("T", ""),
        12 => ("RL", ""),
        13 => ("TB", ""),
        14 => ("K", ""),
        15 => ("F", ""),
        16 => ("BK", ""),
        17 => ("FT", ""),
        18 => ("HS", ""),
        19 => ("RW", ""),
        20 => ("LM", ""),
        21 => ("CM", ""),
        22 => ("CT", ""),
        23 => ("RA", ""),
        24 => ("P", "1"),
        25 => ("P9", ""),
        26 => ("SM", "2"),
        27 => ("SG", "1"),
        28 => ("GG", ""),
        29 => ("SG", "2"),
        _ => ("RF", ""),
    }
}

fn knife(class: i32) -> bool {
    matches!(class, 14 | 16)
}

fn thrown(class: i32) -> bool {
    matches!(class, 11 | 21)
}

/// The upper-body names of a state (6): the run name and its walk and
/// sprint variants.
pub fn upper_names(class: i32, posture: Posture, dir: Direction) -> (String, Option<String>, Option<String>) {
    let (code, digit) = class_code(class);
    let d = dir.upper();
    let moving = dir != Direction::None;
    // Sniper select and reload reuse the rifle's.
    let sr = if class == 5 { "RF" } else { code };
    let name = |stance: &str, action: &str, suffix: &str| format!("U{code}{stance}{action}{suffix}");
    match posture {
        Posture::Hit => ("D_HIT".into(), None, None),
        Posture::Hit2 => ("D_HITT".into(), None, None),
        Posture::Aim => (name("R", "Am", d), Some(name("W", "Am", d)), Some(name("RR", "Am", "F"))),
        Posture::Fire | Posture::PreFire | Posture::PostFire => {
            let part = match posture {
                Posture::PreFire => "_A",
                Posture::Fire if thrown(class) => "_B",
                Posture::PostFire => "_C",
                _ => "",
            };
            if knife(class) {
                (name("R", "Fi", "S"), None, None)
            } else {
                let suffix = format!("{d}{digit}{part}");
                (name("R", "Fi", &suffix), Some(name("W", "Fi", &suffix)), Some(name("RR", "Am", "F")))
            }
        }
        Posture::AltFire => {
            if knife(class) {
                (name("R", "Fi", "S2"), None, None)
            } else {
                (name("R", "Am", "S"), None, None)
            }
        }
        Posture::Select | Posture::Reload => {
            let action = if posture == Posture::Select { "Se" } else { "Re" };
            let digit = if posture == Posture::Reload { digit } else { "" };
            if !moving {
                (format!("U{sr}RS{action}S{digit}"), None, Some(format!("U{sr}RR{action}S{digit}")))
            } else if class == 1 && posture == Posture::Reload {
                ("URBRReS".into(), None, Some(format!("U{sr}RR{action}S{digit}")))
            } else {
                (format!("U{sr}R{action}S{digit}"), None, Some(format!("U{sr}RR{action}S{digit}")))
            }
        }
    }
}

/// The legs' names (6): the lower tracker's (None: sprint in a direction
/// without one) and the crouch tracker's, with the leg set's prefix.
pub fn leg_names(legs: i32, movement: Movement, dir: Direction) -> (Option<String>, String) {
    let prefix = match legs {
        1 => "V",
        2 => "VG",
        _ => "",
    };
    let d = dir.legs();
    let lower = match (movement, dir) {
        (Movement::Walk | Movement::Run, Direction::None) => Some("LST".to_string()),
        (Movement::Walk, _) => Some(format!("LW{d}")),
        (Movement::Run, _) => Some(format!("LR{d}")),
        (Movement::Jump, Direction::None) => Some("LSJ".into()),
        (Movement::Jump, Direction::F) => Some("LJJ".into()),
        (Movement::Jump, Direction::FL) => Some("LJJL".into()),
        (Movement::Jump, Direction::FR) => Some("LJJR".into()),
        (Movement::Jump, _) => Some(format!("LJ{d}")),
        (Movement::Sprint, Direction::F) => Some("LRRF".into()),
        (Movement::Sprint, Direction::FL) => Some("LRRL".into()),
        (Movement::Sprint, Direction::FR) => Some("LRRR".into()),
        (Movement::Sprint, _) => None,
    };
    let crouch = format!("LC{d}");
    (lower.map(|l| format!("{prefix}{l}")), format!("{prefix}{crouch}"))
}

/// The four values the driver last acted on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decided {
    pub class: i32,
    pub posture: Posture,
    pub movement: Movement,
    pub direction: Direction,
}

/// One character's driver.
#[derive(Clone, Debug)]
pub struct Driver {
    pub list: Arc<AnimList>,
    pub main: Tracker,
    pub upper: Tracker,
    pub lower: Tracker,
    pub crouch: Tracker,
    /// The values of the last update (5: remembered as "last").
    pub last: Decided,
    /// The posture whose animation the upper tracker plays (None: none
    /// started).
    pub playing: Option<Posture>,
    pub status: Status,
    pub jump: Jump,
    /// The reload in progress was finished by its animation.
    reload_finished: bool,
    /// The body (file) is new: the next update starts every tracker.
    fresh: bool,
}

impl Driver {
    /// A driver on `list`: the main tracker on entry 0, looping.
    pub fn new(list: Arc<AnimList>) -> Self {
        Self {
            list,
            main: Tracker::on(0, true),
            upper: Tracker::on(0, true),
            lower: Tracker::on(0, true),
            crouch: Tracker::on(0, true),
            last: Decided {
                class: 0,
                ..Default::default()
            },
            playing: None,
            status: Status::None,
            jump: Jump::None,
            reload_finished: false,
            fresh: true,
        }
    }

    /// The entry named `name`, if the library has it.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.list.find(name)
    }

    /// The name of the entry a tracker plays.
    pub fn name_of(&self, t: &Tracker) -> &str {
        self.list.entries.get(t.current).map_or("", |e| e.name())
    }

    fn done(&self, t: &Tracker) -> bool {
        t.done(&self.list)
    }

    /// The upper animation of posture `p` plays and is done / still plays.
    fn finished(&self, p: Posture) -> bool {
        self.playing == Some(p) && self.done(&self.upper)
    }

    fn running(&self, p: Posture) -> bool {
        self.playing == Some(p) && !self.done(&self.upper)
    }

    /// Advance every tracker by `ms` (the engine's per-frame advance).
    pub fn advance(&mut self, ms: u32) {
        let list = self.list.clone();
        for t in [&mut self.main, &mut self.upper, &mut self.lower, &mut self.crouch] {
            t.update(&list, ms, &mut |_| {});
        }
    }

    /// The posture (5).
    fn posture(&mut self, i: &Inputs) -> Posture {
        let last = self.last.posture;
        let mut posture = Posture::Aim;
        if self.status == Status::Idle {
            self.status = Status::None;
            return Posture::Aim;
        }
        if !i.reloading {
            self.reload_finished = false;
            if self.status == Status::Reloading {
                self.status = Status::None;
            }
        }
        // 2. Reloading.
        if i.reloading && !self.reload_finished {
            if self.status == Status::None {
                self.status = Status::Reloading;
            }
            if i.reload_may_finish || self.finished(Posture::Reload) {
                self.reload_finished = true;
                if self.status == Status::Reloading {
                    self.status = Status::None;
                }
            } else if last == Posture::PostFire && self.running(Posture::PostFire) {
                posture = last;
            } else {
                posture = Posture::Reload;
            }
        }
        // 3. Select, deselect, hits.
        if self.status == Status::Select || self.finished(Posture::PostFire) || self.running(Posture::Select) {
            posture = Posture::Select;
            // Done, or (a library without the name) never started.
            let none = last == Posture::Select && self.playing != Some(Posture::Select);
            if self.finished(Posture::Select) || none {
                self.status = Status::None;
            }
        } else if self.status == Status::Deselect {
            self.status = Status::None;
        } else if self.status == Status::Hit || self.running(Posture::Hit) || self.running(Posture::Hit2) {
            if self.running(Posture::Hit) || self.running(Posture::Hit2) {
                posture = last;
            } else if matches!(last, Posture::Hit | Posture::Hit2) && self.done(&self.upper) {
                self.status = Status::None;
            } else if i.damaged && !i.protected {
                posture = if i.coin { Posture::Hit } else { Posture::Hit2 };
            }
        }
        // 4. The firing choice.
        if self.status == Status::None {
            posture = self.firing(i, posture);
            if i.empty && self.done(&self.upper) {
                posture = Posture::Aim;
            }
        }
        posture
    }

    fn firing(&self, i: &Inputs, current: Posture) -> Posture {
        let last = self.last.posture;
        let wait = |p: Posture| matches!(last, Posture::Reload | Posture::Select | Posture::AltFire) && self.running(last) && p != last;
        match i.class {
            5 | 20 => {
                if i.fire || self.running(Posture::Fire) {
                    Posture::Fire
                } else {
                    Posture::Aim
                }
            }
            11 | 21 => match last {
                Posture::Aim if i.fire && current != Posture::Select => Posture::PreFire,
                Posture::PreFire if !self.done(&self.upper) => Posture::PreFire,
                Posture::PreFire => Posture::Fire,
                Posture::Fire if i.fire => Posture::Fire,
                Posture::Fire => Posture::PostFire,
                Posture::PostFire if !self.done(&self.upper) => Posture::PostFire,
                _ => Posture::Aim,
            },
            _ => {
                if i.fire || self.running(Posture::Fire) {
                    if wait(Posture::Fire) { last } else { Posture::Fire }
                } else if i.alt_fire || self.running(Posture::AltFire) {
                    if wait(Posture::AltFire) { last } else { Posture::AltFire }
                } else {
                    Posture::Aim
                }
            }
        }
    }

    /// The movement and direction (5), or None to leave the jump's
    /// direction as it was.
    fn movement(&mut self, i: &Inputs) -> (Movement, Direction) {
        if i.landed {
            self.jump = Jump::None;
        }
        if i.jumped {
            self.jump = Jump::Jumped;
        }
        match self.jump {
            Jump::Jumped if !i.crouch_key => {
                if self.last.movement == Movement::Jump && self.done(&self.lower) {
                    self.jump = Jump::Falling;
                    return (Movement::Walk, Direction::None);
                }
                // A sprinting jump plays the run jump (Quirk 3).
                return (Movement::Jump, self.last.direction);
            }
            Jump::Falling => return (Movement::Walk, Direction::None),
            _ => {}
        }
        let movement = if i.sprint {
            Movement::Sprint
        } else if i.run && !i.zoomed {
            Movement::Run
        } else {
            Movement::Walk
        };
        (movement, Direction::of(i.forward, i.back, i.left, i.right))
    }

    /// Start `entry` on a tracker: switch when another plays; restart a
    /// done one when `restart`; else nothing.
    fn start(list: &AnimList, t: &mut Tracker, entry: usize, restart: bool) -> bool {
        if t.current != entry {
            t.start(entry);
            true
        } else if restart && t.done(list) {
            t.reset();
            true
        } else {
            false
        }
    }

    /// One update: advance by `ms`, decide, start (sections 5 to 7).
    pub fn update(&mut self, ms: u32, i: &Inputs) {
        self.advance(ms);
        if let Some(s) = i.status {
            self.status = s;
        }
        let posture = self.posture(i);
        let (movement, direction) = self.movement(i);
        let class = i.class;
        let (lower, crouch) = leg_names(i.legs, movement, direction);
        // Sprint in a direction without legs: nothing started or kept.
        let Some(lower) = lower else { return };
        let (run, walk, sprint) = upper_names(class, posture, direction);
        let exists = |n: &Option<String>| n.as_deref().and_then(|n| self.find(n));
        let upper = match movement {
            Movement::Sprint => exists(&sprint).or_else(|| self.find(&run)),
            Movement::Walk => exists(&walk).or_else(|| self.find(&run)),
            _ => self.find(&run),
        };
        let list = self.list.clone();
        let fresh = std::mem::take(&mut self.fresh);
        if let Some(c) = self.find(&crouch) {
            self.crouch.looping = true;
            Self::start(&list, &mut self.crouch, c, true);
        }
        if let Some(l) = self.find(&lower) {
            self.lower.looping = movement != Movement::Jump;
            Self::start(&list, &mut self.lower, l, true);
        }
        let changed = fresh || class != self.last.class || posture != self.last.posture;
        if let Some(u) = upper {
            self.upper.looping = posture.looping();
            let free = matches!(posture, Posture::Aim | Posture::Fire | Posture::AltFire);
            // The grenade class never restarts a finished animation.
            let restart = !thrown(class);
            let started = if changed || free {
                Self::start(&list, &mut self.upper, u, restart)
            } else {
                false
            };
            if started || (changed && self.upper.current == u) {
                self.playing = Some(posture);
            }
            if changed && started {
                let length = list.entries[u].length();
                let frame = i.frames.get(posture.frame_index()).copied().unwrap_or(0);
                let rate = match posture {
                    Posture::Reload => i.rates.reload,
                    Posture::Select => i.rates.select,
                    Posture::Fire | Posture::PreFire | Posture::PostFire => i.rates.fire,
                    _ => 1.0,
                };
                self.upper.set_rate(upper_rate(length, rate, frame));
            }
        }
        // Keep the torso in step with the legs.
        if !fresh && (movement != self.last.movement || direction != self.last.direction) {
            let last = self.last.posture;
            let synced = match last {
                Posture::Fire | Posture::AltFire => false,
                Posture::Aim => true,
                _ => self.done(&self.upper),
            };
            if synced {
                let time = self.lower.time;
                self.upper.set_time(&list, time);
            }
        }
        self.crouch.multiplier = i.crouch.clamp(0.0, 1.0);
        self.last = Decided {
            class,
            posture,
            movement,
            direction,
        };
    }

    /// Change the library (a new body file): every tracker starts again
    /// on the next update.
    pub fn set_list(&mut self, list: Arc<AnimList>) {
        *self = Self::new(list);
    }
}

/// Lengths (ms) the spec measured on the US male library (7, 10), for
/// the library used without a body model.
const LENGTHS: &[(&str, u32)] = &[
    ("LST", 4000),
    ("LC", 4000),
    ("LWF", 1000),
    ("LRF", 600),
    ("LRB", 600),
    ("LRL", 600),
    ("LRR", 600),
    ("LCF", 1400),
    ("LSJ", 666),
    ("LJJ", 666),
    ("LJB", 666),
    ("LRRF", 500),
    ("URFRAmS", 4000),
    ("URFRFiS", 100),
    ("URFRSSeS", 733),
    ("URFRSReS", 1866),
    ("UKRFiS", 500),
    ("UKRFiS2", 800),
    ("UTRFiS_A", 267),
    ("UTRFiS_B", 1),
    ("UTRFiS_C", 900),
    ("D_HIT", 467),
    ("D_HITT", 400),
    ("DH", 1433),
    ("DH_1", 1700),
    ("DT", 3033),
    ("DT_1", 1333),
    ("DHB", 1566),
    ("DTB", 1766),
    ("DLSH", 1766),
    ("DLT", 1333),
    ("DRSH", 1233),
    ("DRT", 1433),
    ("DC", 1900),
    ("D_S", 3033),
    ("D_P", 5633),
    ("D_W", 7800),
    ("D_BO_F", 1500),
    ("NS1", 5566),
    ("NS2", 3166),
];

/// Names the spec says the US libraries lack (6, Quirk 1), and whole
/// class families (`SH`, `G`, `M`).
fn missing(name: &str) -> bool {
    const ABSENT: &[&str] = &[
        "URFWAmS", "URFRFiF", "URFRFiB", "URFRFiL", "URFRFiR", "URFWFiF", "URFWFiB", "URFWFiL", "URFWFiR",
    ];
    ABSENT.iter().any(|a| a.eq_ignore_ascii_case(name))
        || ["USH", "UG", "UM"].iter().any(|f| {
            name.strip_prefix(f)
                .is_some_and(|rest| rest.starts_with(['R', 'W', 'C']))
        })
}

/// The length the stand-in library gives a name: the spec's measured one,
/// else the same kind's (legs 600, crouch 1400, jumps 666, aims 4000,
/// fires 100, selects 733, reloads 1866, deaths 1900).
fn length_of(name: &str) -> u32 {
    if let Some((_, l)) = LENGTHS.iter().find(|(n, _)| *n == name) {
        return *l;
    }
    if name.starts_with("LC") {
        1400
    } else if name.starts_with("LJ") || name.starts_with("LSJ") {
        666
    } else if name.starts_with("LRR") {
        500
    } else if name.starts_with('L') {
        600
    } else if name.contains("Am") {
        4000
    } else if name.contains("Fi") {
        100
    } else if name.contains("Se") {
        733
    } else if name.contains("Re") {
        1866
    } else {
        1900
    }
}

/// Interpolation times (ms) of the US male library (7): the hits' 200.
fn interp_of(name: &str) -> u32 {
    if name == "D_HIT" || name == "D_HITT" { 200 } else { 0 }
}

/// A stand-in entry: two keys (0 and `length`) on no nodes.
pub fn stand_in(name: &str, length: u32, interp: u32) -> Entry {
    Entry {
        animation: Animation {
            dims: [15.0, 50.0, 15.0],
            name: name.to_string(),
            compression: 0,
            interp_ms: interp,
            keys: vec![
                Key {
                    time: 0,
                    string: String::new(),
                },
                Key {
                    time: length.max(1),
                    string: String::new(),
                },
            ],
            channels: Vec::new(),
        },
        child: 0,
        dims: [15.0, 50.0, 15.0],
        translation: [0.0; 3],
    }
}

/// The library without a body model: entry 0 the body's own animation
/// (`baseAnim`, 4800 ms), then every name section 6 can build for the
/// classes 0 to 29 (lengths and blend times as measured, the rest by
/// kind), minus those the US libraries lack; the death animations (10);
/// for `female`, no `NS1`/`NS2`.
pub fn spec_library(female: bool) -> Arc<AnimList> {
    static LISTS: OnceLock<[Arc<AnimList>; 2]> = OnceLock::new();
    LISTS.get_or_init(|| [build_library(false), build_library(true)])[usize::from(female)].clone()
}

fn build_library(female: bool) -> Arc<AnimList> {
    let mut names: Vec<String> = Vec::new();
    let mut add = |n: String| {
        if !missing(&n) && !names.iter().any(|m| m == &n) {
            names.push(n);
        }
    };
    const DIRS: [Direction; 9] = [
        Direction::None,
        Direction::F,
        Direction::B,
        Direction::L,
        Direction::R,
        Direction::FL,
        Direction::FR,
        Direction::BL,
        Direction::BR,
    ];
    for legs in 0..3 {
        for m in [Movement::Walk, Movement::Run, Movement::Sprint, Movement::Jump] {
            for d in DIRS {
                let (lower, crouch) = leg_names(legs, m, d);
                if let Some(l) = lower {
                    add(l);
                }
                add(crouch);
            }
        }
    }
    const POSTURES: [Posture; 9] = [
        Posture::Aim,
        Posture::PreFire,
        Posture::Fire,
        Posture::PostFire,
        Posture::Select,
        Posture::Reload,
        Posture::Hit,
        Posture::Hit2,
        Posture::AltFire,
    ];
    for class in 0..30 {
        for p in POSTURES {
            for d in DIRS {
                let (run, walk, sprint) = upper_names(class, p, d);
                add(run);
                walk.into_iter().for_each(&mut add);
                sprint.into_iter().for_each(&mut add);
            }
        }
    }
    for (n, _) in LENGTHS.iter().filter(|(n, _)| n.starts_with('D') || n.starts_with("NS")) {
        if !(female && n.starts_with("NS")) {
            add(n.to_string());
        }
    }
    for n in ["DC_1", "DCH", "DCH_1", "DCB", "DCB_1", "DCBH", "DCBH_1", "DHB_1", "DTB_1", "D_BO_B", "D_BO_L", "D_BO_R", "SD"] {
        add(n.to_string());
    }
    let mut list = AnimList::default();
    list.offsets.push(Some(0));
    list.entries.push(stand_in(if female { "W_VST_NM" } else { "baseAnim" }, if female { 3333 } else { 4800 }, 0));
    for n in names {
        let (length, interp) = (length_of(&n), interp_of(&n));
        list.entries.push(stand_in(&n, length, interp));
    }
    Arc::new(list)
}
