//! The announcer's banner and voice, drawn by the shell for any game
//! (specs/combat_arms/kill_streaks.md A2-A6; the pack and the facts are
//! `chimera_core::announcer`): whatever the local player's game, HUD or
//! gun, the server's pack (`KillFacts::pack`) announces the local
//! player's enemy kills, so Ultimate Gun Game shows Combat Arms'
//! announcer to a CS:S soldier with a CS:S gun, and plain CS:S (no pack)
//! nothing.
//!
//! - **Which callouts** (`requests`, A2): the first kill (instead of the
//!   step), else the step of the multi-kill count; a head or groin shot;
//!   revenge. A weapon skin's art and voices replace the pack's by key.
//! - **One banner** (`BannerPlayer`, A4): a request is dropped while a
//!   banner with a higher level shows; otherwise it replaces it at once.
//!   No queue. Every callout's voice plays whether its banner showed or
//!   not, overlapping (A6).
//! - **The animation** (`main_copy`, `echo_copy`, A5): slam-in from the
//!   pack's start size with an ease-in quartic, an echo copy growing and
//!   fading while the main copy shakes, hold, blow-out; scaled about the
//!   layout rectangle's centre (`rect_on_screen`), each axis stretched
//!   from the pack's reference screen.
//! - `AnnouncerView` is the state, headless too (tests read it); drawing
//!   (Bevy UI images, `<game>://` install files through the asset
//!   server) and the voices (`SoundAsset`s as local interface sounds) run
//!   where there is an asset server. A new map hides the banner.
//!
//! Known shortcuts: the phases start on their exact times (not a frame
//! late, Quirk 1); no digits (co-op's "N KILLS"), side log (A7), lucky
//! shot or the "+N Points" line; the voice plays at the master volume
//! (no separate effects volume).

use std::{collections::BTreeMap, sync::Arc};

use bevy::{prelude::*, window::PrimaryWindow};
use chimera_core::announcer::{AnnouncerPack, AnnouncerPacks, Callout, KillAnnounced, KillFacts, Rect, SpecialShot, Timing};

use crate::{
    audio::{
        clip_asset::SoundAsset,
        live_sound::{Gains, LiveClip},
    },
    core::LocalPlayer,
    map::LoadedMapName,
};

pub struct AnnouncerPlugin;

impl Plugin for AnnouncerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AnnouncerView>()
            .init_resource::<AnnouncerPacks>()
            .add_message::<KillAnnounced>()
            .add_systems(Update, announce);
        if app.world().contains_resource::<AssetServer>() {
            app.init_resource::<AnnouncerAssets>()
                .add_systems(
                    Update,
                    (
                        draw_banner.run_if(resource_exists::<Assets<Image>>),
                        play_voices.run_if(resource_exists::<Assets<SoundAsset>>),
                    )
                        .after(announce),
                );
        }
    }
}

/// A banner request and its voice (one callout of a kill).
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub key: String,
    pub name: String,
    pub image: String,
    pub top: Option<String>,
    pub voice: Option<String>,
    pub level: i32,
}

/// A callout as requested, with a skin's replacements.
fn request(c: &Callout, female: bool, skin: Option<&chimera_core::announcer::Skin>) -> Request {
    let image = |key: &str, default: Option<&String>| {
        skin.and_then(|s| s.images.get(key)).or(default).cloned()
    };
    let voice_key = if female && c.voice_female.is_some() {
        format!("{}_female", c.key)
    } else {
        c.key.clone()
    };
    let voice_default = if female { c.voice_female.as_ref().or(c.voice.as_ref()) } else { c.voice.as_ref() };
    Request {
        key: c.key.clone(),
        name: c.name.clone(),
        image: image(&c.key, Some(&c.banner)).unwrap_or_default(),
        top: image(&format!("{}_top", c.key), c.top.as_ref()),
        voice: skin.and_then(|s| s.voices.get(&voice_key)).or(voice_default).cloned(),
        level: c.level,
    }
}

/// The callouts of a kill, in the order they are raised (A2).
pub fn requests(pack: &AnnouncerPack, facts: &KillFacts, weapon: Option<&str>) -> Vec<Request> {
    if facts.multi == 0 {
        return Vec::new();
    }
    let skin = weapon.and_then(|w| pack.skin(w));
    let female = facts.victim_female;
    let step = if facts.first { pack.first_kill.as_ref() } else { pack.step(facts.multi) };
    let special = match facts.shot {
        SpecialShot::Head => pack.headshot.as_ref(),
        SpecialShot::Groin => pack.groin.as_ref(),
        SpecialShot::None => None,
    };
    let revenge = pack.revenge.as_ref().filter(|_| facts.revenge);
    [step, special, revenge]
        .into_iter()
        .flatten()
        .map(|c| request(c, female, skin))
        .collect()
}

/// The banner showing: its request and when it started.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub request: Request,
    pub since: f64,
}

/// The one banner area (A4).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BannerPlayer {
    pub shown: Option<Shown>,
}

impl BannerPlayer {
    /// Show `r` from `now` unless a banner of a higher level shows
    /// (equal levels replace). Whether it shows.
    pub fn request(&mut self, r: Request, now: f64) -> bool {
        if self.shown.as_ref().is_some_and(|s| s.request.level > r.level) {
            return false;
        }
        self.shown = Some(Shown { request: r, since: now });
        true
    }

    /// Hidden on the first update at or past the total time.
    pub fn update(&mut self, now: f64, timing: &Timing) {
        if self.shown.as_ref().is_some_and(|s| now - s.since >= timing.total as f64) {
            self.shown = None;
        }
    }

    /// The showing banner's time since its start.
    pub fn t(&self, now: f64) -> Option<f64> {
        self.shown.as_ref().map(|s| now - s.since)
    }
}

/// Alpha of a copy at scale `s` going from 1 (opaque) to `far`
/// (transparent), truncated.
fn alpha_at(s: f64, far: f64) -> u8 {
    if far <= 1.0 {
        return 255;
    }
    (255.0 - (s - 1.0) * 255.0 / (far - 1.0)).clamp(0.0, 255.0) as u8
}

/// The main copy at `t`: scale and alpha (A5).
pub fn main_copy(t: f64, tm: &Timing) -> (f64, u8) {
    let (s0, l) = (tm.start_size as f64, tm.last_size as f64);
    let (start, c, end, d) = (tm.start as f64, tm.creating as f64, tm.end as f64, tm.destroying as f64);
    if t >= end && t - end < d {
        let s = l - (l - 1.0) * ((d - (t - end)) / d).powi(4);
        return (s, alpha_at(s, l));
    }
    if t >= end + d {
        return (l, 0);
    }
    if t < start {
        return (s0, 0);
    }
    if t - start < c {
        let s = s0 - (s0 - 1.0) * ((t - start) / c).powi(4);
        return (s, alpha_at(s, s0));
    }
    (1.0, 255)
}

/// Whether `t` is in the echo phase (and the main copy shakes).
pub fn in_echo(t: f64, tm: &Timing) -> bool {
    let tau = t - (tm.start + tm.creating) as f64;
    (0.0..tm.blowing as f64).contains(&tau)
}

/// The echo copy at `t`: scale and alpha (0 outside its phase).
pub fn echo_copy(t: f64, tm: &Timing) -> (f64, u8) {
    if !in_echo(t, tm) {
        return (1.0, 0);
    }
    let (b, big) = (tm.blowing as f64, tm.blow_size as f64);
    let tau = t - (tm.start + tm.creating) as f64;
    let s = big - (big - 1.0) * ((b - tau) / b).powi(4);
    (s, alpha_at(s, big))
}

/// A layout rectangle on a `screen` (each axis stretched from the
/// `reference`), scaled by `scale` about its centre: x, y, width, height.
pub fn rect_on_screen(rect: Rect, reference: [f32; 2], screen: Vec2, scale: f64) -> [f64; 4] {
    let (sx, sy) = (screen.x as f64 / reference[0] as f64, screen.y as f64 / reference[1] as f64);
    let [x, y, w, h] = rect.map(|v| v as f64);
    let (cx, cy) = ((x + w / 2.0) * sx, (y + h / 2.0) * sy);
    let (w, h) = (w * sx * scale, h * sy * scale);
    [cx - w / 2.0, cy - h / 2.0, w, h]
}

/// The announcer's state: the pack it shows, the banner, the voices it
/// played (newest last, the last 32) and those to play.
#[derive(Resource, Default)]
pub struct AnnouncerView {
    pub pack: Option<Arc<AnnouncerPack>>,
    pub player: BannerPlayer,
    pub voices: Vec<String>,
    pub to_play: Vec<String>,
    map: Option<String>,
}

/// The local player's kills into banners and voices.
fn announce(
    mut kills: MessageReader<KillAnnounced>,
    packs: Res<AnnouncerPacks>,
    me: Query<Entity, With<LocalPlayer>>,
    map: Option<Res<LoadedMapName>>,
    time: Res<Time>,
    mut view: ResMut<AnnouncerView>,
) {
    let now = time.elapsed_secs_f64();
    let map_name = map.map(|m| m.0.clone());
    if view.map != map_name {
        view.map = map_name;
        view.player = BannerPlayer::default();
    }
    for k in kills.read() {
        if !me.contains(k.killer) {
            continue;
        }
        let Some(pack) = packs.get(&k.facts.pack).cloned() else { continue };
        for r in requests(&pack, &k.facts, k.weapon) {
            if let Some(v) = &r.voice {
                view.voices.push(v.clone());
                view.to_play.push(v.clone());
            }
            view.player.request(r, now);
        }
        if view.voices.len() > 32 {
            let extra = view.voices.len() - 32;
            view.voices.drain(..extra);
        }
        view.pack = Some(pack);
    }
    if let Some(pack) = view.pack.clone() {
        view.player.update(now, &pack.timing);
    }
}

/// The pack's files as assets (loaded ahead, kept while it is shown), the
/// voices waiting for their files, and the banner's nodes.
#[derive(Resource, Default)]
struct AnnouncerAssets {
    pack: Option<String>,
    images: BTreeMap<String, Handle<Image>>,
    sounds: BTreeMap<String, Handle<SoundAsset>>,
    waiting: Vec<(Handle<SoundAsset>, f64)>,
    root: Option<Entity>,
    shake: u32,
}

impl AnnouncerAssets {
    /// Load every file of `pack` once it is the one shown.
    fn follow(&mut self, pack: &AnnouncerPack, server: &AssetServer, sounds: bool) {
        if self.pack.as_deref() == Some(pack.id.as_str()) {
            return;
        }
        let path = |f: &str| chimera_present::assets::install_path(&pack.game, f);
        let (images, voices) = pack.files();
        self.images = images.into_iter().map(|f| (f.clone(), server.load(path(&f)))).collect();
        self.sounds = if sounds {
            voices.into_iter().map(|f| (f.clone(), server.load(path(&f)))).collect()
        } else {
            BTreeMap::new()
        };
        self.pack = Some(pack.id.clone());
    }
}

/// One of the banner's images: the echo copy or the main one, the banner
/// or its top image.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
struct BannerImage {
    echo: bool,
    top: bool,
}

/// The banner on screen (A5).
fn draw_banner(
    view: Res<AnnouncerView>,
    server: Res<AssetServer>,
    time: Res<Time>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut assets: ResMut<AnnouncerAssets>,
    mut images: Query<(&BannerImage, &mut Node, &mut ImageNode, &mut Visibility)>,
    sounds: Option<Res<Assets<SoundAsset>>>,
    mut commands: Commands,
) {
    if let Some(pack) = &view.pack {
        assets.follow(pack, &server, sounds.is_some());
    }
    if assets.root.is_none() {
        let root = commands
            .spawn((
                Name::new("announcer"),
                Node {
                    position_type: PositionType::Absolute,
                    width: percent(100.0),
                    height: percent(100.0),
                    ..default()
                },
                Pickable::IGNORE,
                GlobalZIndex(35),
            ))
            .id();
        // Main copy first, the echo on top (A5).
        for (echo, top) in [(false, false), (false, true), (true, false), (true, true)] {
            commands.spawn((
                BannerImage { echo, top },
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                ImageNode::default(),
                Visibility::Hidden,
                Pickable::IGNORE,
                ChildOf(root),
            ));
        }
        assets.root = Some(root);
        return;
    }
    let now = time.elapsed_secs_f64();
    let shown = view.pack.as_ref().zip(view.player.shown.as_ref());
    let screen = window.single().map_or(Vec2::new(1024.0, 768.0), |w| w.size());
    // The shake: a fresh whole-pixel offset each frame (xorshift).
    let mut draw = || {
        let mut x = assets.shake.max(1);
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        assets.shake = x;
        x
    };
    let shake = shown.map(|(p, _)| p.timing.shake.max(0)).unwrap_or(0);
    let span = (2 * shake + 1) as u32;
    let offset = Vec2::new(
        (draw() % span) as f32 - shake as f32,
        (draw() % span) as f32 - shake as f32,
    );
    for (part, mut node, mut image, mut vis) in &mut images {
        let Some((pack, s)) = shown else {
            *vis = Visibility::Hidden;
            continue;
        };
        let t = now - s.since;
        let file = if part.top { s.request.top.as_ref() } else { Some(&s.request.image) };
        let handle = file.and_then(|f| assets.images.get(f));
        let (scale, alpha) = if part.echo { echo_copy(t, &pack.timing) } else { main_copy(t, &pack.timing) };
        let (Some(handle), true) = (handle, alpha > 0) else {
            *vis = Visibility::Hidden;
            continue;
        };
        let rect = if part.top { pack.layout.top } else { pack.layout.banner };
        let [x, y, w, h] = rect_on_screen(rect, pack.layout.reference, screen, scale);
        let shift = if !part.echo && in_echo(t, &pack.timing) { offset } else { Vec2::ZERO };
        node.left = px(x as f32 + shift.x);
        node.top = px(y as f32 + shift.y);
        node.width = px(w as f32);
        node.height = px(h as f32);
        if image.image != *handle {
            image.image = handle.clone();
        }
        image.color = Color::srgba(1.0, 1.0, 1.0, alpha as f32 / 255.0);
        *vis = Visibility::Inherited;
    }
}

/// The voices, as local interface sounds, once their files are loaded
/// (given up after 3 s).
fn play_voices(
    mut view: ResMut<AnnouncerView>,
    time: Res<Time>,
    mut assets: ResMut<AnnouncerAssets>,
    sounds: Option<Res<Assets<SoundAsset>>>,
    clips: Option<ResMut<Assets<LiveClip>>>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    for v in std::mem::take(&mut view.to_play) {
        if let Some(h) = assets.sounds.get(&v).cloned() {
            assets.waiting.push((h, now));
        }
    }
    let (Some(sounds), Some(mut clips)) = (sounds, clips) else { return };
    assets.waiting.retain(|(h, since)| {
        if let Some(sound) = sounds.get(h) {
            let gains = Arc::new(Gains::new(1.0, 1.0));
            let clip = clips.add(LiveClip::new(sound.0.clone(), gains, default()));
            commands.spawn((AudioPlayer(clip), PlaybackSettings::DESPAWN));
            return false;
        }
        now - since < 3.0
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACK: &str = r#"
id = "stand_in"
name = "Stand-in"
game = "stand_in"
window_ms = 5000
revenge_after = 2
steps = [
    { count = 2, key = "double", name = "DOUBLE", banner = "double.img", voice = "double.snd", level = 4 },
    { count = 3, key = "multi", name = "MULTI", banner = "multi.img", voice = "multi.snd", level = 4 },
    { count = 4, key = "ultra", name = "ULTRA", banner = "ultra.img", voice = "ultra.snd", level = 4 },
    { count = 5, key = "fantastic", name = "FANTASTIC", banner = "fantastic.img", voice = "fantastic.snd", level = 4 },
    { count = 6, key = "unbelievable", name = "UNBELIEVABLE", banner = "unbel.img", voice = "unbel.snd", level = 4 },
    { count = 7, key = "unbelievable_plus", name = "UNBELIEVABLE+", banner = "unbel_plus.img", voice = "unbel.snd", level = 4 },
]
first_kill = { key = "first", name = "FIRST KILL", banner = "first.img", voice = "first.snd", level = 4 }
headshot = { key = "head", name = "HEADSHOT", banner = "head.img", top = "skull.img", voice = "head.snd", level = 5 }
groin = { key = "groin", name = "NUTSHOT", banner = "groin.img", top = "nut.img", voice = "groin_m.snd", voice_female = "groin_f.snd", level = 5 }
revenge = { key = "revenge", name = "REVENGE", banner = "revenge.img", level = 3 }
[layout]
reference = [1024.0, 768.0]
banner = [256.0, 76.0, 512.0, 256.0]
top = [256.0, 93.0, 512.0, 256.0]
[timing]
total = 1.4
start = 0.0
creating = 0.1
blowing = 0.15
end = 1.2
destroying = 0.2
start_size = 5.0
blow_size = 3.0
last_size = 5.0
shake = 5
[[skins]]
name = "psy"
weapons = ["kit:psy_gun"]
images = { double = "double_psy.img" }
[[skins]]
name = "robo"
weapons = ["kit:robo_gun"]
images = { head = "head_robo.img", head_top = "skull_robo.img" }
voices = { head = "head_robo.snd" }
[[skins]]
name = "phantom"
weapons = ["kit:phantom_gun"]
images = { head = "head_phantom.img" }
"#;

    fn pack() -> AnnouncerPack {
        AnnouncerPack::parse(PACK).unwrap()
    }

    fn facts(multi: u8) -> KillFacts {
        KillFacts {
            pack: "stand_in".into(),
            multi,
            ..default()
        }
    }

    fn names(r: &[Request]) -> Vec<&str> {
        r.iter().map(|r| r.name.as_str()).collect()
    }

    /// Cases 1, 2, 6: no step at 1, the steps 2 to 7, Unbelievable+ again
    /// above with Unbelievable's voice.
    #[test]
    fn steps_and_voices_by_count() {
        let p = pack();
        assert!(requests(&p, &facts(1), None).is_empty());
        let got: Vec<(String, Option<String>)> = (2..=8)
            .map(|n| {
                let r = &requests(&p, &facts(n), None)[0];
                (r.name.clone(), r.voice.clone())
            })
            .collect();
        let want = [
            ("DOUBLE", "double.snd"),
            ("MULTI", "multi.snd"),
            ("ULTRA", "ultra.snd"),
            ("FANTASTIC", "fantastic.snd"),
            ("UNBELIEVABLE", "unbel.snd"),
            ("UNBELIEVABLE+", "unbel.snd"),
            ("UNBELIEVABLE+", "unbel.snd"),
        ];
        assert_eq!(got, want.map(|(n, v)| (n.to_string(), Some(v.to_string()))));
        assert!(requests(&p, &KillFacts::default(), None).is_empty(), "not an enemy kill");
    }

    /// Cases 7-9 and the first kill, revenge: the first kill instead of
    /// the step; head with its skull; groin by the victim's body.
    #[test]
    fn special_kills() {
        let p = pack();
        let first = requests(&p, &KillFacts { first: true, multi: 2, ..facts(2) }, None);
        assert_eq!(names(&first), ["FIRST KILL"], "the step ignored");
        assert_eq!(first[0].voice.as_deref(), Some("first.snd"));
        let head = requests(&p, &KillFacts { shot: SpecialShot::Head, ..facts(1) }, None);
        assert_eq!(names(&head), ["HEADSHOT"]);
        assert_eq!(head[0].top.as_deref(), Some("skull.img"));
        assert_eq!(head[0].voice.as_deref(), Some("head.snd"));
        let groin = |female| requests(&p, &KillFacts { shot: SpecialShot::Groin, victim_female: female, ..facts(1) }, None);
        assert_eq!(groin(true)[0].voice.as_deref(), Some("groin_f.snd"));
        assert_eq!(groin(false)[0].voice.as_deref(), Some("groin_m.snd"));
        assert_eq!(groin(true)[0].top.as_deref(), Some("nut.img"));
        let both = requests(&p, &KillFacts { shot: SpecialShot::Head, revenge: true, ..facts(2) }, None);
        assert_eq!(names(&both), ["DOUBLE", "HEADSHOT", "REVENGE"]);
        assert_eq!(both[2].voice, None, "revenge is silent");
        // A headshot double kill shows HEADSHOT (its level is higher).
        let mut b = BannerPlayer::default();
        for r in both.into_iter().take(2) {
            b.request(r, 0.0);
        }
        assert_eq!(b.shown.unwrap().request.name, "HEADSHOT");
    }

    /// Cases 29-31: a skin's art and voices by key, the pack's for the
    /// keys it lacks.
    #[test]
    fn skins() {
        let p = pack();
        let r = &requests(&p, &facts(2), Some("kit:psy_gun"))[0];
        assert_eq!((r.image.as_str(), r.voice.as_deref()), ("double_psy.img", Some("double.snd")));
        let head = KillFacts { shot: SpecialShot::Head, ..facts(1) };
        let r = &requests(&p, &head, Some("kit:robo_gun"))[0];
        assert_eq!(
            (r.image.as_str(), r.top.as_deref(), r.voice.as_deref()),
            ("head_robo.img", Some("skull_robo.img"), Some("head_robo.snd"))
        );
        let r = &requests(&p, &head, Some("kit:phantom_gun"))[0];
        assert_eq!(
            (r.image.as_str(), r.top.as_deref(), r.voice.as_deref()),
            ("head_phantom.img", Some("skull.img"), Some("head.snd"))
        );
    }

    fn req(name: &str, level: i32) -> Request {
        Request {
            key: name.into(),
            name: name.into(),
            image: String::new(),
            top: None,
            voice: None,
            level,
        }
    }

    /// Cases 10, 17-21: shown for 1.4 s; a lower level dropped, equal or
    /// higher replace at once; the last of a frame's requests shows.
    #[test]
    fn one_banner_by_level() {
        let tm = pack().timing;
        let mut b = BannerPlayer::default();
        assert!(b.request(req("DOUBLE", 4), 0.0));
        b.update(1.399, &tm);
        assert!(b.shown.is_some());
        b.update(1.4, &tm);
        assert!(b.shown.is_none(), "hidden at T");
        b.request(req("HEADSHOT", 5), 0.0);
        assert!(!b.request(req("DOUBLE", 4), 0.5), "dropped under a higher level");
        assert_eq!(b.shown.as_ref().unwrap().request.name, "HEADSHOT");
        b.update(1.39, &tm);
        assert!(b.shown.is_some(), "HEADSHOT plays on");
        let mut b = BannerPlayer::default();
        b.request(req("DOUBLE", 4), 0.0);
        assert!(b.request(req("MULTI", 4), 0.6));
        assert_eq!(b.t(0.6), Some(0.0), "restarted");
        let mut b = BannerPlayer::default();
        b.request(req("REVENGE", 3), 0.0);
        assert!(b.request(req("DOUBLE", 4), 0.2));
        assert!(b.request(req("LUCKY", 11), 0.3));
        let mut b = BannerPlayer::default();
        b.request(req("DOUBLE", 4), 0.0);
        b.request(req("MULTI", 4), 0.0);
        assert_eq!(b.shown.unwrap().request.name, "MULTI");
    }

    /// Cases 11-15: the copies' scale and alpha at sample times.
    #[test]
    fn animation_at_sample_times() {
        let tm = pack().timing;
        let (s, a) = main_copy(0.05, &tm);
        assert!((s - 4.75).abs() < 1e-4 && a == 15, "{s} {a}");
        assert_eq!(main_copy(0.5, &tm), (1.0, 255));
        let (s, a) = main_copy(1.3, &tm);
        assert!((s - 4.75).abs() < 1e-4 && a == 15, "{s} {a}");
        let (s, a) = echo_copy(0.1 + 0.075, &tm);
        assert!((s - 2.875).abs() < 1e-4 && a == 15, "{s} {a}");
        assert_eq!(echo_copy(0.05, &tm).1, 0);
        assert_eq!(echo_copy(0.5, &tm).1, 0);
        assert!(in_echo(0.2, &tm) && !in_echo(0.5, &tm) && !in_echo(0.05, &tm), "the shake's phase");
    }

    /// Cases 24, 25: the banner's rectangle stretched and scaled about
    /// its centre.
    #[test]
    fn banner_rect() {
        let l = pack().layout;
        let [x, y, w, h] = rect_on_screen(l.banner, l.reference, Vec2::new(1920.0, 1080.0), 1.0);
        assert!((x - 480.0).abs() < 1e-3 && (y - 106.875).abs() < 1e-3 && (w - 960.0).abs() < 1e-3 && (h - 360.0).abs() < 1e-3);
        let [x, y, w, h] = rect_on_screen(l.banner, l.reference, Vec2::new(1024.0, 768.0), 4.75);
        assert!((w - 2432.0).abs() < 1e-3 && (h - 1216.0).abs() < 1e-3);
        assert!((x + w / 2.0 - 512.0).abs() < 1e-3 && (y + h / 2.0 - 204.0).abs() < 1e-3);
    }
}
