//! Combat Arms' player characters (specs/combat_arms/characters.md;
//! section numbers below are its): which body a player's selection draws,
//! the third-person animation state, the posed skeleton with its hit
//! spheres and sockets, death animations and bodies, and the network's
//! copy of it all for other players.
//!
//! - `records`: data/characters.toml (bodies, head pieces, goggles, team
//!   marks, each weapon's `AniType`, the skeletons with their hit tables
//!   and sockets) and the model choice (1, 2): cosmetics, the face
//!   redirect, skins by camouflage index, team marks, head pieces and
//!   goggles by skeleton and figure type. A player's selection is
//!   `CaLook`: the local player's `CaOwnLook` (the console's `ca_model`,
//!   `ca_face`, `ca_head`, `ca_camo`), a bot's drawn from the loadout
//!   pool (`CharacterData::bot_look`), else the default. The same choice
//!   is the player's gear (gear_perks.md, `gear`): the body record is the
//!   vest, the head piece the helmet, the goggle and backpack as such.
//! - `driver`: the animation driver (4-7, 9) on LithTech's millisecond
//!   trackers: posture and movement from the weapon status, the fire
//!   buttons and the movement flags; names from the class code, stance,
//!   action and direction; starting, restarting, the torso kept in step
//!   with the legs, the upper rate from `HHAniFrame`; hit reactions.
//! - `skeleton`: the four trackers layered by the body's weight sets (4),
//!   the aim's bone rotations (8), hit-sphere centres on nodes or sockets
//!   (11) and sockets (3); without a body model, the spec's skeleton as
//!   measured (`skeleton::fallback`).
//! - `death`: the death animation (10), a jumping body's turn, the body's
//!   removal time.
//! - `models`: the install's bodies and third-person weapons (2) as
//!   Chimera's character and held models, and their rigs.
//!
//! Each tick (`FixedUpdate`, after the weapons: the spec's server runs the
//! driver per movement frame) every living character's `CaAnim` advances
//! by the tick and decides (a network client only its own player), then
//! its skeleton is posed into `CaPose`: hit-sphere centres and the sockets
//! the game uses (`NAME_socket` for the name tag, `Left_foot` for drops,
//! `RightHand` for the weapon and the third-person flash), in the
//! character's frame. `weapons` hits the spheres, `inventory` drops at the
//! left foot, the HUD anchors name tags at the name socket. With a body
//! model (`models::rig` has the selection's body file) the pose is the
//! model's own, drawn through Chimera's body (`map::BodyName`,
//! `Animator::fixed`) with the weapon held at `RightHand` (`map::Held`).
//!
//! On death (`core::Died`) the driver plays the death animation chosen by
//! `death::choose` from the killing hit, and a body (`CaBody`) is left at
//! the player's floor-snapped position + 0.1 u; it goes 5 s after its
//! owner respawned and its animation ended in the modes that remove
//! bodies (data's `body_removal`), or at a new round.
//!
//! Network (`CaCharactersNetPlugin`, part `combat_arms_characters`): each
//! client sends its own look (`CaLookRequest`, from `CaOwnLook`: when its
//! player appears and whenever it changes); the server validates it
//! (`CharacterData::validated`: an unknown body refused, unknown pieces
//! none), keeps it on the client's entity (`CaClientLook`) and dresses
//! that client's player in it (`wear_looks`). The server sends each
//! character's selection (`CaLook`: players', bots'), its trackers' names
//! and start ticks with the crouch, pitch and twist bytes (`CaAnimNet`,
//! 8's legacy transport), and bodies (`CaBody`); a client plays other
//! players' trackers from it and poses them with the received aim, as the
//! spec's clients do.
//!
//! Known shortcuts (docs/tech-debt.md; the most literal reading where the
//! spec is unclear, listed for its open questions):
//! - No V2 key: no body, head piece, goggle or third-person weapon model
//!   loads (the unkeyed corpses and zombie bodies on the stock trees
//!   aren't player records; `heavy::characters` poses them) (characters are the shell's capsules) and the library is the
//!   spec's names and measured lengths (`driver::spec_library`; names
//!   the spec doesn't measure get their kind's length). The skeleton is
//!   the spec's measured `LST` t = 0 pose for nodes and spheres and the
//!   "stand" pose for sockets, crouched by squashing heights towards the
//!   feet (the hull's height over the standing one), turned by the aim:
//!   so spheres don't follow the walking legs or the weapon's aim pose,
//!   and the female `Bip01 Neck` and right-hand socket places are ours.
//! - With models: the six stock bodies load (`models::LOADED`), one
//!   character model per sex: the first body with its library, the
//!   other bodies (the Light and Heavy vests), the faces of the loadout
//!   pool and the head pieces they may wear merged onto its skeleton as
//!   body-part choices (`models::BodyGroup`, `map::BodyNumber`), so the
//!   Light and Heavy bodies pose on the Normal one's library; another
//!   selection's body stays a capsule on the spec's skeleton, a face or
//!   head piece outside the pool shows the body's default; the pose is
//!   computed per tick (not per rendered frame) and drawn as is; with the
//!   default head piece (`HBasic`, no model) a body wears the head piece
//!   its face names in the install's `HairIndex` (our reading: the spec
//!   doesn't say what `HairIndex` does; the female's hair); goggles and
//!   backpacks (the console's `ca_goggle`, `ca_backpack`; the bots'
//!   pool) are body-part choices too, rigid on their socket's node at its
//!   bind transform (`models::attach`), only the pool's (another shows
//!   none), and not hidden for a first-person owner (bodies aren't
//!   drawn there anyway); cosmetics, the clan pieces and infection
//!   overrides aren't loaded (nothing selects them); a face's skins by camouflage
//!   index (its colour overrides not read); render styles as
//!   `render_style` reads them; the drawn body's feet sit at the standing hull's bottom; a
//!   corpse (`CaBody`) isn't drawn and has no hit spheres (Open question
//!   9); the stock bodies' skins other than the default male's are not in
//!   our data (the install's models file gives them). Bots' looks are
//!   ours (the spec has no loadouts for them). The look a client sends
//!   is its selection's ids only (no figure type, clan or team mark,
//!   colour indices, infection override or default-goggle flag: nothing
//!   uses them yet); without the install's models file a face is known
//!   when the body's face redirect covers it and a head piece when ours
//!   or the bots' pool names it (`validated`). A weapon names its third-person model only with the V1 key (the
//!   install's `HHModel`, `data`); ours name none.
//! - The driver: emotes (9) and the hit kick on remote characters (9:
//!   display only) aren't there; `HHAniFrame` is absent from our data
//!   (every upper rate 1); "fire held" is the fire button with a magazine
//!   weapon not reloading and not empty (the client's "firing state");
//!   a reload ends when the weapon's does (the server's reload timer and
//!   its 500 ms fallback, Open question 2, are the weapon frame's
//!   business); classes 24, 26, 27 and 29's digits are our reading of the
//!   examples (revolver 1, FAMAS 2, double barrel 1, snowball 2); the
//!   weapons' classes are ours by kind (data/characters.toml) unless the
//!   install's weapons file (V1) gives their `AniType`.
//! - Aim: the client's shares everywhere (`Torso` pitched on the server
//!   too, Open question 5: the hit spheres are the client's, where our one
//!   ray walk stands for the client's test); pitch and twist from the
//!   look and the weapon's body yaw (`weapons::CaSpread::body_yaw`).
//! - Death: poison and the endless fall can't be told from other damage
//!   (core's damage kinds), so they play the bullet choices; the coin is
//!   our dice, not the clock in seconds (Quirk 6); the dead player is not
//!   moved onto the floor (only its body is placed there).

pub mod death;
pub mod driver;
pub mod models;
pub mod records;
pub mod skeleton;

use std::sync::Arc;

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use self::{
    driver::{Driver, Inputs, Status},
    records::Selection,
    skeleton::{Posed, Rig},
};
use super::{
    movement::CaMovement,
    rules::spawns::CaProtected,
    weapons::{CaSpread, FEMALE, HitSpheres, Location, MALE, zoom::CaZoom},
};
use crate::{
    core::{
        self, Damage, DamageKind, Dead, Died, Health, Hitgroup, Intent, LocalPlayer, MovementState, NetRole, RoundRestarts,
        SimClock, SimSet, Team,
    },
    map::{BodyName, BodyNumber, BodySkin, CharacterModels, Held, anim::Animator},
    net::{NetAppExt, game::ServerTime, ordered::OrderedAppExt, server::Player},
    weapon::{Inventory, Magazine, Weapon, WeaponEvent, WeaponEventKind, WeaponState},
};

/// A player's model selection (1); the default (model 0, face 0, no
/// gear) when a character has none.
pub type CaLook = Selection;

/// The standing hull's height in units (the stock 2 x 50).
pub const STAND_HEIGHT: f32 = 100.0;
/// A body is created this far above the dead player (u).
pub const BODY_LIFT: f32 = 0.1;

/// One character's animation state and what it was told since the last
/// update.
#[derive(Component, Clone, Debug)]
pub struct CaAnim {
    pub driver: Driver,
    /// The effective body record and its skeleton (hit table) index.
    pub body: i32,
    pub skeleton: usize,
    /// The body's rig (None: no model, the spec's skeleton).
    pub rig: Option<Arc<Rig>>,
    /// A weapon status reported since the last update.
    pub pending: Option<Status>,
    /// The last damage was above 0.
    pub damaged: bool,
    /// In a jump at the last update (for "jumped" and "landed").
    pub in_jump: bool,
    /// The aim as posed (spec frame: pitch positive down, twist positive
    /// right; radians).
    pub pitch: f32,
    pub twist: f32,
    /// The death animation playing (None: alive).
    pub death: Option<String>,
    dice: u32,
}

impl CaAnim {
    pub fn new(body: i32, skeleton: usize, rig: Option<Arc<Rig>>) -> Self {
        let list = rig.as_ref().map_or_else(|| driver::spec_library(skeleton == 1), |r| Arc::new(r.list.clone()));
        Self {
            driver: Driver::new(list),
            body,
            skeleton,
            rig,
            pending: None,
            damaged: false,
            in_jump: false,
            pitch: 0.0,
            twist: 0.0,
            death: None,
            dice: 0x9e37_79b9,
        }
    }

    /// The next coin (true: 1).
    fn coin(&mut self) -> bool {
        self.dice ^= self.dice << 13;
        self.dice ^= self.dice >> 17;
        self.dice ^= self.dice << 5;
        self.dice & 1 == 1
    }

    /// The upper, lower and crouch trackers' animation names.
    pub fn playing(&self) -> (&str, &str, &str) {
        let d = &self.driver;
        (d.name_of(&d.upper), d.name_of(&d.lower), d.name_of(&d.crouch))
    }

    /// The posed skeleton now, with aim (`server`: the server's shares).
    pub fn posed(&self, crouch: f32, squash: f32, server: bool) -> Posed {
        let d = &self.driver;
        let skel = records::data().skeleton(self.skeleton);
        let mut posed = match &self.rig {
            Some(rig) => {
                let pose = if self.death.is_some() {
                    // A body has only its main tracker (10).
                    (0..rig.model.nodes.len()).map(|j| d.main.pose(&rig.list, j)).collect()
                } else {
                    rig.layered(&d.main, &d.upper, &d.lower, &d.crouch, crouch)
                };
                rig.posed(&pose, crouch)
            }
            None => skeleton::fallback(skel, squash),
        };
        posed.aim(&skel.aim, self.pitch, self.twist, server);
        posed
    }
}

/// The posed skeleton's results in the character's frame (right-handed,
/// facing -Z, +X its right, feet at the origin; world units), turned by
/// `body_yaw` in the world.
#[derive(Component, Clone, Debug, Default)]
pub struct CaPose {
    /// Per entry of the skeleton's hit table: its centre (None: the body
    /// has no node or socket of that name, or its radius is 0).
    pub spheres: Vec<Option<Vec3>>,
    /// The skeleton (hit table) index.
    pub skeleton: usize,
    /// `NAME_socket`, `Left_foot`, `RightHand`.
    pub name_tag: Option<Vec3>,
    pub left_foot: Option<Vec3>,
    pub right_hand: Option<Vec3>,
    /// The legs' yaw (as `Intent::yaw`).
    pub body_yaw: f32,
    /// From a body model (not the spec's skeleton).
    pub modelled: bool,
}

impl CaPose {
    /// A point of the pose in the world for a character at `t` with
    /// `state`'s hull, `unit` metres per unit.
    pub fn world(&self, t: &Transform, state: &MovementState, unit: f32, p: Vec3) -> Vec3 {
        let feet = t.translation + Vec3::Y * if state.hull_max != state.hull_min { state.hull_min.y } else { -50.0 * unit };
        feet + Quat::from_rotation_y(self.body_yaw) * (p * unit)
    }
}

/// What other players are told of a character (the server's): its
/// trackers' names with the server tick each one's time 0 fell on, the
/// crouch fraction, pitch and twist bytes (8's legacy transport), the
/// death animation.
#[derive(Component, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CaAnimNet {
    pub upper: (String, f64),
    pub lower: (String, f64),
    pub crouch: (String, f64),
    pub r: u8,
    pub pitch: i8,
    pub twist: i8,
    pub death: Option<String>,
    pub body_yaw: f32,
}

/// A dead player's body (10): where it lies, facing, its owner, the death
/// animation and when it started (s, the simulation's clock), when its
/// owner respawned.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CaBody {
    pub owner: Entity,
    pub look: Selection,
    pub at: Vec3,
    pub yaw: f32,
    pub animation: String,
    pub since: f64,
    /// The animation's length, s.
    pub length: f64,
    pub respawned: Option<f64>,
}

/// Combat Arms' characters.
pub struct CaCharactersPlugin;

impl Plugin for CaCharactersPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<WeaponEvent>()
            .add_message::<Damage>()
            .add_message::<Died>()
            .net_part("combat_arms_characters", register)
            .add_systems(
                FixedUpdate,
                (setup, listen, die.after(core::apply_damage), drive, bodies.run_if(core::authoritative))
                    .chain()
                    .after(SimSet::Weapons),
            )
            .init_resource::<CaOwnLook>()
            .add_systems(FixedUpdate, dress.after(drive))
            .add_systems(FixedUpdate, wear_looks.before(setup).run_if(core::authoritative))
            .add_systems(FixedUpdate, client_clock.before(drive).run_if(core::shows_others));
        look_cvars(app);
    }
}

/// The protocol: the selection, the animation state and bodies, from the
/// server.
fn register(app: &mut App) {
    app.replicate::<Selection>()
        .ordered::<Selection>()
        .replicate::<CaAnimNet>()
        .ordered::<CaAnimNet>()
        .replicate::<CaBody>()
        .ordered::<CaBody>()
        .add_client_message::<CaLookRequest>(Channel::Ordered)
        .add_systems(
            PreUpdate,
            receive_looks
                .after(ServerSystems::Receive)
                .run_if(in_state(ServerState::Running)),
        )
        .add_systems(
            PostUpdate,
            send_look.before(ClientSystems::Send).run_if(resource_equals(NetRole::Client)),
        );
}

/// A client's own look, sent to the server with its character info
/// (characters.md 1, "The selection").
#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct CaLookRequest(pub Selection);

/// The look a connected client chose (the server's, on the client's
/// entity, validated: `CharacterData::validated`); its player's
/// characters wear it (`wear_looks`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct CaClientLook(pub Selection);

/// A client sends its look when its player appears and whenever it
/// changes (`CaOwnLook`, the console's `ca_*`).
fn send_look(
    loadout: Res<CaOwnLook>,
    me: Query<Entity, With<LocalPlayer>>,
    mut sent: Local<Option<(Entity, Selection)>>,
    mut out: MessageWriter<CaLookRequest>,
) {
    let Ok(e) = me.single() else {
        *sent = None;
        return;
    };
    if *sent != Some((e, loadout.0)) {
        *sent = Some((e, loadout.0));
        out.write(CaLookRequest(loadout.0));
    }
}

/// The server keeps each client's look, validated (an unknown body is
/// refused: the client keeps its previous look).
fn receive_looks(
    mut requests: MessageReader<FromClient<CaLookRequest>>,
    clients: Query<(), With<Player>>,
    mut commands: Commands,
) {
    for r in requests.read() {
        let Some(client) = r.client_id.entity() else { continue };
        if clients.get(client).is_err() {
            continue;
        }
        match records::data().validated(r.message.0) {
            Some(look) => {
                commands.entity(client).insert(CaClientLook(look));
            }
            None => debug!("look refused: body record {} unknown", r.message.0.model),
        }
    }
}

/// Metres per world unit.
fn unit() -> f32 {
    super::movement::movement_data().meters_per_unit
}

/// The selection's body record, skeleton and rig.
fn body_of(look: &Selection) -> (i32, usize, Option<Arc<Rig>>) {
    let (record, _) = records::data().effective(look);
    (record.id, record.skeleton, models::rig(&record.file))
}

/// The local player's own look (characters.md 1, "The selection": what
/// its client sends; ours from the console, `ca_model`, `ca_face`,
/// `ca_head`, `ca_camo`: the record ids and the camouflage index).
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaOwnLook(pub Selection);

impl Default for CaOwnLook {
    fn default() -> Self {
        Self(Selection::default())
    }
}

/// The console's look settings (`CaOwnLook`).
pub fn look_cvars(app: &mut App) {
    use crate::console::resource_cvar;
    resource_cvar::<CaOwnLook, i32>(app, "ca_model", "Your body record (0 Normal, 1 Light, 2 Heavy).", |l| &mut l.0.model);
    resource_cvar::<CaOwnLook, i32>(app, "ca_face", "Your face (0-3 male, 4-7 female).", |l| &mut l.0.face);
    resource_cvar::<CaOwnLook, i32>(app, "ca_head", "Your head piece (0: none).", |l| &mut l.0.head);
    resource_cvar::<CaOwnLook, i32>(app, "ca_camo", "Your camouflage index (0-6).", |l| &mut l.0.camouflage);
    resource_cvar::<CaOwnLook, i32>(app, "ca_goggle", "Your goggles or mask (-1: none).", |l| &mut l.0.goggle);
    resource_cvar::<CaOwnLook, i32>(app, "ca_backpack", "Your backpack (-1: none).", |l| &mut l.0.backpack);
}

/// A seed for a bot's look: its name's (stable across rounds), else its
/// entity's.
fn look_seed(name: Option<&Name>, e: Entity) -> u64 {
    match name {
        Some(n) => n.as_str().bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3)),
        None => e.to_bits(),
    }
}

/// Looks (the server's): a bot without one draws its own from the
/// loadout pool (`CharacterData::bot_look`); the local player wears
/// `CaOwnLook` (changed from the console at once); a client's player the
/// look its client sent (`CaClientLook`); others keep the default.
#[allow(clippy::type_complexity)]
fn wear_looks(
    loadout: Res<CaOwnLook>,
    clients: Query<(&Player, &CaClientLook)>,
    characters: Query<
        (Entity, Option<&Selection>, Option<&Name>, Has<core::Bot>, Has<LocalPlayer>),
        (With<Intent>, With<MovementState>),
    >,
    mut commands: Commands,
) {
    for (e, look, name, bot, local) in &characters {
        let sent = clients.iter().find(|(p, _)| p.character == e).map(|(_, l)| l.0);
        let want = if local {
            loadout.0
        } else if let Some(sent) = sent {
            sent
        } else if bot && look.is_none() {
            records::data().bot_look(look_seed(name, e))
        } else {
            continue;
        };
        if look != Some(&want) {
            commands.entity(e).insert(want);
        }
    }
}

/// Characters get a selection (the default) and an animation state; a
/// changed selection, a new driver (a new body file).
#[allow(clippy::type_complexity)]
fn setup(
    characters: Query<(Entity, Option<&Selection>, Option<&CaAnim>), (With<Intent>, With<MovementState>)>,
    mut commands: Commands,
) {
    for (e, look, anim) in &characters {
        let look = look.copied().unwrap_or_default();
        let (body, skeleton, rig) = body_of(&look);
        let same = anim.is_some_and(|a| a.body == body && a.rig.as_ref().map(Arc::as_ptr) == rig.as_ref().map(Arc::as_ptr));
        if !same {
            let table = if skeleton == 1 { *FEMALE } else { *MALE };
            commands
                .entity(e)
                .insert((look, CaAnim::new(body, skeleton, rig), CaPose::default(), HitSpheres(table)));
        }
    }
}

/// Weapon statuses (a draw starts: "select") and damage ("hit") since
/// the last update.
fn listen(mut events: MessageReader<WeaponEvent>, mut damage: MessageReader<Damage>, mut anims: Query<&mut CaAnim>) {
    for ev in events.read().filter(|e| e.shown()) {
        if matches!(ev.kind, WeaponEventKind::Deployed)
            && let Ok(mut a) = anims.get_mut(ev.owner)
        {
            a.pending = Some(Status::Select);
        }
    }
    for d in damage.read() {
        if let Ok(mut a) = anims.get_mut(d.target) {
            a.damaged = d.amount > 0.0;
            a.pending = Some(Status::Hit);
        }
    }
}

/// The location of a hit group (the weapons' sphere locations).
fn location_of(h: Hitgroup) -> Option<Location> {
    match h {
        Hitgroup::Head => Some(Location::Head),
        Hitgroup::Chest => Some(Location::Torso),
        Hitgroup::Stomach => Some(Location::Groin),
        Hitgroup::LeftArm | Hitgroup::RightArm => Some(Location::Arm),
        Hitgroup::LeftLeg | Hitgroup::RightLeg => Some(Location::Leg),
        Hitgroup::Generic => None,
    }
}

/// The spec frame's forward and right of a character facing `yaw` (ours),
/// in the world.
fn axes(yaw: f32) -> (Vec3, Vec3) {
    let q = Quat::from_rotation_y(yaw);
    (q * Vec3::NEG_Z, q * Vec3::X)
}

/// Death (10): the death animation on the main tracker, and the body.
#[allow(clippy::type_complexity)]
fn die(
    mut died: MessageReader<Died>,
    mut characters: Query<(&mut CaAnim, &Transform, &Intent, Option<&CaPose>, Option<&CaMovement>, &Selection)>,
    clock: Res<SimClock>,
    authoritative: Option<Res<NetRole>>,
    mut commands: Commands,
) {
    let server = authoritative.is_none_or(|r| *r != NetRole::Client);
    for d in died.read() {
        let Ok((mut anim, t, intent, pose, movement, look)) = characters.get_mut(d.entity) else {
            continue;
        };
        let yaw = pose.map_or(intent.yaw, |p| p.body_yaw);
        let (forward, right) = axes(yaw);
        let kind = match d.damage.kind {
            DamageKind::Blast => death::DeathKind::Explosion,
            DamageKind::Melee => death::DeathKind::Melee,
            _ => death::DeathKind::Other,
        };
        let location = location_of(d.damage.hitgroup);
        let coin = anim.coin();
        let jumping = movement.is_some_and(|m| m.in_jump);
        let inputs = death::DeathInputs {
            kind,
            dir: d.damage.dir,
            forward,
            right,
            head: location == Some(Location::Head),
            groin: location == Some(Location::Groin),
            coin,
            jumping,
            special: None,
        };
        let list = anim.driver.list.clone();
        let name = death::choose(&inputs, |n| list.find(n).is_some());
        if let Some(entry) = list.find(&name) {
            let main = &mut anim.driver.main;
            main.start(entry);
            main.looping = false;
        }
        anim.death = Some(name.clone());
        anim.pitch = 0.0;
        if !server {
            continue;
        }
        // The body: the dead player's own yaw, or turned away from the
        // shot when it died in a jump (spec yaw is ours negated).
        let body_yaw = if jumping {
            let dir = Quat::from_rotation_y(-yaw) * d.damage.dir;
            yaw - death::jump_yaw(Vec3::new(dir.x, dir.y, -dir.z))
        } else {
            yaw
        };
        let length = list.find(&name).map_or(0.0, |e| list.entries[e].length() as f64 / 1000.0);
        let owner = d.entity;
        let look = *look;
        let at = t.translation;
        let since = clock.now;
        commands.queue(move |w: &mut World| {
            let at = floor_snapped(w, owner, at) + Vec3::Y * BODY_LIFT * unit();
            w.spawn((
                Name::new("Body"),
                Replicated,
                Transform::from_translation(at).with_rotation(Quat::from_rotation_y(body_yaw)),
                CaBody {
                    owner,
                    look,
                    at,
                    yaw: body_yaw,
                    animation: name,
                    since,
                    length,
                    respawned: None,
                },
            ));
        });
    }
}

/// Where the hull centre `at` of `owner` comes to rest on the floor
/// below (10, step 1): the same when standing or nothing is below.
fn floor_snapped(world: &mut World, owner: Entity, at: Vec3) -> Vec3 {
    let Some(state) = world.get::<MovementState>(owner).cloned() else { return at };
    if state.on_ground || state.hull_max == state.hull_min {
        return at;
    }
    let bottom = at + Vec3::Y * state.hull_min.y;
    let mut system_state: bevy::ecs::system::SystemState<avian3d::prelude::SpatialQuery> =
        bevy::ecs::system::SystemState::new(world);
    let Ok(spatial) = system_state.get(world) else { return at };
    let filter = avian3d::prelude::SpatialQueryFilter::from_mask(core::SOLID_LAYERS).with_excluded_entities([owner]);
    match spatial.cast_ray(bottom, Dir3::NEG_Y, 1000.0, true, &filter) {
        Some(hit) => at - Vec3::Y * hit.distance,
        None => at,
    }
}

/// Each tick: every living character's driver (a client's own player
/// only; others follow the server's `CaAnimNet`), then its pose.
#[allow(clippy::type_complexity)]
fn drive(
    clock: Res<SimClock>,
    role: Option<Res<NetRole>>,
    server_time: Option<Res<ClientClock>>,
    mut characters: Query<(
        Entity,
        (&mut CaAnim, &mut CaPose, Option<&mut CaAnimNet>),
        (&Intent, &MovementState, Option<&CaMovement>, &Health, Has<Dead>, Has<LocalPlayer>),
        (Option<&Inventory>, Option<&CaProtected>, Option<&Transform>),
        Option<&mut Animator>,
    )>,
    weapons: Query<(&Weapon, Option<&WeaponState>, Option<&Magazine>, Option<&CaZoom>, Option<&CaSpread>)>,
    mut commands: Commands,
) {
    let client = role.as_deref().is_some_and(|r| *r == NetRole::Client);
    let ms = (clock.delta.as_secs_f64() * 1000.0).round() as u32;
    let unit = unit();
    for (e, (mut anim, mut pose, net), (intent, state, movement, health, dead, local), (inventory, protected, _), animator) in
        &mut characters
    {
        let alive = health.current > 0.0 && !dead;
        if alive && anim.death.is_some() {
            // Respawned: the trackers start over.
            let list = anim.driver.list.clone();
            anim.driver.set_list(list);
            anim.death = None;
        }
        let weapon = inventory.and_then(|i| i.active).and_then(|w| weapons.get(w).ok());
        let r = movement.map_or(0.0, |m| m.crouch_rate);
        let height = (state.hull_max.y - state.hull_min.y) / unit;
        let squash = if height > 1.0 { (height / STAND_HEIGHT).min(1.0) } else { 1.0 };
        let followed = client && !local;
        if !alive {
            let list = anim.driver.list.clone();
            anim.driver.main.update(&list, ms, &mut |_| {});
            anim.pending = None;
        } else if followed {
            if let (Some(net), Some(clock)) = (net.as_deref(), server_time.as_deref()) {
                follow(&mut anim, net, clock.0, clock.1);
            }
        } else {
            let inputs = inputs(&mut anim, intent, movement, weapon, protected, r);
            anim.driver.update(ms, &inputs);
            // The aim: pitch from the look, the twist from the body yaw.
            anim.pitch = skeleton::clamp_aim(-intent.pitch);
            let body = weapon.and_then(|w| w.4).and_then(|s| s.body_yaw).unwrap_or(intent.yaw);
            anim.twist = skeleton::clamp_aim(-wrap(intent.yaw - body));
        }
        let body_yaw = if followed {
            net.as_deref().map_or(intent.yaw, |n| n.body_yaw)
        } else {
            intent.yaw + anim.twist
        };
        let posed = anim.posed(r, squash, false);
        *pose = pose_of(&anim, &posed, body_yaw, squash);
        if let (true, Some(mut animator)) = (anim.rig.is_some(), animator) {
            animator.fixed = Some(Arc::new(posed.locals_rh()));
            if animator.main.is_none() {
                animator.main = Some(0);
            }
            animator.yaw = Some(body_yaw);
        }
        if !client {
            let next = net_of(&anim, &clock, r, body_yaw);
            match net {
                Some(mut n) => {
                    if *n != next {
                        *n = next;
                    }
                }
                None => {
                    commands.entity(e).insert(next);
                }
            }
        }
    }
}

/// An angle into (-pi, pi].
fn wrap(a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let a = a.rem_euclid(TAU);
    if a > PI { a - TAU } else { a }
}

/// The driver's inputs from the character's state (5).
fn inputs(
    anim: &mut CaAnim,
    intent: &Intent,
    movement: Option<&CaMovement>,
    weapon: Option<(&Weapon, Option<&WeaponState>, Option<&Magazine>, Option<&CaZoom>, Option<&CaSpread>)>,
    protected: Option<&CaProtected>,
    r: f32,
) -> Inputs {
    let look = records::data().weapon(weapon.map(|w| w.0.id));
    let reloading = weapon.and_then(|w| w.1).is_some_and(|s| s.reloading());
    let empty = weapon.and_then(|w| w.2).is_some_and(|m| m.clip == 0);
    let magazine = weapon.is_some_and(|w| w.2.is_some());
    let in_jump = movement.is_some_and(|m| m.in_jump);
    let (jumped, landed) = (in_jump && !anim.in_jump, !in_jump && anim.in_jump);
    anim.in_jump = in_jump;
    let coin = anim.coin();
    let m = movement.cloned().unwrap_or_default();
    Inputs {
        class: look.ani_type,
        legs: look.lower_ani_type,
        status: anim.pending.take(),
        reloading,
        reload_may_finish: false,
        fire: intent.fire && !(magazine && (reloading || empty)),
        alt_fire: intent.secondary && matches!(look.ani_type, 14 | 16),
        empty,
        run: m.run,
        sprint: m.sprint && m.moving,
        zoomed: weapon.and_then(|w| w.3).is_some_and(CaZoom::zoomed),
        jumped,
        landed,
        crouch_key: m.crouch,
        forward: m.forward,
        back: m.back,
        left: m.left,
        right: m.right,
        crouch: r,
        damaged: anim.damaged,
        protected: protected.is_some(),
        coin,
        frames: look.frames,
        rates: Default::default(),
    }
}

/// The pose's results in the character's frame.
pub fn pose_of(anim: &CaAnim, posed: &Posed, body_yaw: f32, squash: f32) -> CaPose {
    let skel = records::data().skeleton(anim.skeleton);
    let body = |p: Vec3| skeleton::to_body(p, posed.feet);
    let left_foot = if anim.rig.is_some() {
        posed.node("Left_foot").map(|j| posed.globals[j].0)
    } else {
        // The spec's stand pose, squashed (the aim never moves the legs).
        let [x, y, z] = skel.left_foot;
        Some(Vec3::new(x, skeleton::STAND_FEET + (y - skeleton::STAND_FEET) * squash, z))
    };
    CaPose {
        spheres: skel
            .spheres
            .iter()
            .map(|s| (s.radius > 0.0).then(|| posed.sphere(&s.name)).flatten().map(body))
            .collect(),
        skeleton: anim.skeleton,
        name_tag: posed.place("NAME_socket").map(|p| body(p.0)),
        left_foot: left_foot.map(body),
        right_hand: posed.place("RightHand").map(|p| body(p.0)),
        body_yaw,
        modelled: anim.rig.is_some(),
    }
}

/// The server's animation state for the network: each tracker's name
/// and the server tick its time 0 fell on.
fn net_of(anim: &CaAnim, clock: &SimClock, r: f32, body_yaw: f32) -> CaAnimNet {
    let d = &anim.driver;
    let step = clock.delta.as_secs_f64().max(1e-9);
    let at = |t: &lithtech_formats::anim::Tracker| {
        let start = clock.tick as f64 - t.time as f64 / 1000.0 / (t.rate.max(1e-3) as f64) / step;
        (d.name_of(t).to_string(), start.round())
    };
    CaAnimNet {
        upper: at(&d.upper),
        lower: at(&d.lower),
        crouch: at(&d.crouch),
        r: (r.clamp(0.0, 1.0) * 255.0).round() as u8,
        pitch: skeleton::aim_byte(anim.pitch),
        twist: skeleton::aim_byte(anim.twist),
        death: anim.death.clone(),
        body_yaw,
    }
}

/// A client's view of the server's tick (now) and tick length, for
/// other players' trackers.
#[derive(Resource, Clone, Copy, Debug)]
pub struct ClientClock(pub f64, pub f64);

/// Other players' trackers from the server's state (a client).
fn follow(anim: &mut CaAnim, net: &CaAnimNet, tick: f64, step: f64) {
    let list = anim.driver.list.clone();
    let d = &mut anim.driver;
    for (t, (name, start)) in [(&mut d.upper, &net.upper), (&mut d.lower, &net.lower), (&mut d.crouch, &net.crouch)] {
        let Some(entry) = list.find(name) else { continue };
        if t.current != entry {
            t.start(entry);
        }
        let ms = ((tick - start) * step * 1000.0).max(0.0) as u32;
        t.set_time(&list, ms);
    }
    d.crouch.multiplier = net.r as f32 / 255.0;
    anim.pitch = skeleton::from_aim_byte(net.pitch);
    anim.twist = skeleton::from_aim_byte(net.twist);
}

/// A client keeps `ClientClock` from the network's server time.
pub fn client_clock(world: &mut World) {
    if let Some(t) = ServerTime::of(world) {
        world.insert_resource(ClientClock(t.tick, t.step));
    }
}

/// Bodies: their owner's respawn noted, removed when their time comes or
/// at a new round.
fn bodies(
    mut bodies: Query<(Entity, &mut CaBody)>,
    owners: Query<(&Health, Has<Dead>)>,
    clock: Res<SimClock>,
    restarts: Res<RoundRestarts>,
    mut round: Local<Option<u32>>,
    settings: Option<Res<super::rules::CaRuleSettings>>,
    mut commands: Commands,
) {
    let new_round = round.is_some_and(|r| r != restarts.0);
    *round = Some(restarts.0);
    let delay = settings.and_then(|s| records::data().body_removal(&s.mode));
    for (e, mut b) in &mut bodies {
        if new_round {
            commands.entity(e).despawn();
            continue;
        }
        let alive = owners.get(b.owner).is_ok_and(|(h, dead)| h.current > 0.0 && !dead);
        if alive && b.respawned.is_none() {
            b.respawned = Some(clock.now);
        }
        // "Done" one millisecond after its last key.
        let end = b.since + b.length + 0.001;
        if death::removal_time(end, b.respawned, delay).is_some_and(|t| clock.now + 1e-9 >= t) {
            commands.entity(e).despawn();
        }
    }
}

/// Characters wear their body model when the map has it (`BodyName`,
/// with the body number for their body, face and head piece:
/// `models::drawn`), with their skin family for the local player
/// (camouflage, a teammate's mark), and hold their weapon (`Held`).
#[allow(clippy::type_complexity)]
fn dress(
    models: Option<Res<CharacterModels>>,
    characters: Query<(
        Entity,
        &Selection,
        Option<&Team>,
        (Option<&BodyName>, Option<&BodyNumber>, Option<&BodySkin>),
        Option<&Inventory>,
        Option<&Held>,
    )>,
    local: Query<Option<&Team>, With<LocalPlayer>>,
    weapons: Query<&Weapon>,
    mut commands: Commands,
) {
    let Some(models) = models else { return };
    let mine = local.iter().next().flatten().copied();
    for (e, look, team, (name, number, skin), inventory, held) in &characters {
        let Some((want, body)) = models::drawn(look) else { continue };
        if !models.0.iter().any(|m| m.name.as_deref() == Some(want.as_str())) {
            continue;
        }
        if name.is_none_or(|n| n.0 != want) {
            commands.entity(e).insert(BodyName(want));
        }
        if number.is_none_or(|n| n.0 != body) {
            commands.entity(e).insert(BodyNumber(body));
        }
        let teammate = mine.is_some() && team.copied() == mine;
        let family = BodySkin(models::family(look.camouflage, teammate));
        if skin != Some(&family) {
            commands.entity(e).insert(family);
        }
        let id = inventory.and_then(|i| i.active).and_then(|w| weapons.get(w).ok()).map(|w| w.id.to_string());
        if held.is_none_or(|h| h.0 != id) {
            commands.entity(e).insert(Held(id));
        }
    }
}
