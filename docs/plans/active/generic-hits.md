# Generic hits: any gun on any body

Started 2026-10-27 (user's direction, approved). Backlog items 4 ("Hits
across games don't follow the drawn body") and 5 ("Armour isn't shared
across games"), from the parity audit
([parity-audit-2026-10.md](parity-audit-2026-10.md)).

Goal: one game-agnostic interface, so that any gun works on any body
without per-pair work. The body's game says where the body can be hit
(its hitboxes, converted from its own format into Chimera's); the gun's
game says what a hit does per body region (a damage table); armour is
one shared component with coverage per region, each game's rules as
data; and every gun lands its hits through one shared path (lag
compensation and network included). Chimera's interface covers what all
the games need: where one game tells regions apart that another doesn't,
the region set grows and the coarser game maps onto it.

Parity first: plain CS:S and plain Combat Arms keep their exact current
numbers. Both are pinned by tests before anything changes (step a), and
every later step keeps those tests green unchanged.

## Today

- `chimera_core::core::{Hitbox, Hitboxes, Hitgroup}`: oriented boxes in
  the character's frame with a Source hitgroup. CS:S bodies fill
  `Hitboxes` from the model's hitbox set (`cs_source::game::props`,
  `map::BoneBox`), posed on the animated skeleton (`map::bodies`,
  `pose_hitboxes`, a server's `pose_sim_hitboxes`) and kept for lag
  compensation (`weapon::lagcomp::HitHistory`).
- CS:S guns: `weapon::deliver` (the shared hitscan and melee delivery):
  nearest box entered, `DamageEffect::hitgroups` (`HitgroupScale`: head,
  chest, stomach, arm, leg), armour by `weapon::Armor` (kevlar, helmet)
  and the gun's `armor_ratio` (`armor_split`, `HitArmor`), whole points.
  A body without hitboxes (Combat Arms') falls back to height bands on
  its collider (`hitgroup_at`).
- Combat Arms guns: their own delivery (`combat_arms::game::weapons`,
  `client_test`, `server_walk`, `hurt`): hit spheres per skeleton
  (`HitSpheres`, data/characters.toml: radius, location, zone factor,
  priority) posed by `characters::CaPose` or the spec's measured
  skeleton (`geometry_at`), whatever body the victim wears, so a
  CS:S-dressed player is hit as a CA soldier standing. Armour
  `CaArmor` (points, gear pieces with per-location protection by damage
  type), applied before the zone factor (`armour_and_zone`).
- Every character gets `CaArmor` when CA's weapons are in the app (the
  party apps): CS:S guns ignore it, CA guns ignore CS:S kevlar.

## The interface (Chimera)

### Regions: `core::BodyRegion`

The superset of what the games tell apart. CS:S has eight hitgroups;
Combat Arms has five locations but per-sphere zone factors that tell
apart head, neck, chest, groin, hip, upper arm, forearm, hand, thigh,
shin and foot. Regions:

`Generic, Head, Neck, Chest, Stomach, Groin,
LeftUpperArm, LeftForearm, LeftHand, RightUpperArm, RightForearm, RightHand,
LeftThigh, LeftShin, LeftFoot, RightThigh, RightShin, RightFoot`

Two coarser views, both functions of the region:

- `hitgroup()`: the Source hitgroup (`core::Hitgroup`, unchanged), which
  `Damage`, `WeaponEvent::Hit`, `BulletHit` and everything downstream
  (pain sounds, flinch, kill feed headshot icon, death cams, the network)
  keep carrying: Neck to Chest, Groin to Stomach, arm regions to
  Left/RightArm, leg regions to Left/RightLeg.
- `part()`: `BodyPart` (Head, Torso, Arm, Leg, Groin): what armour
  covers (Generic, Neck, Chest and Stomach are torso).

`BodyRegion::from_hitgroup` gives a coarse hitgroup's representative
region (LeftArm to LeftUpperArm, LeftLeg to LeftThigh) for code that
only knows hitgroups (melee height bands, maps' damage).

### Hitboxes: one shape list per body, from the body's game

`core::Hitbox` grows to the superset of both games' shapes:

- `center`, `rotation`, `half` (a box) or `radius: Some(r)` (a sphere),
  in the character's frame (feet at the origin, facing -Z, meters).
- `region: BodyRegion` (replaces `group`; `group()` is
  `region.hitgroup()`).
- `priority: f32`: Combat Arms' `Node#Priority` for its server walk;
  other bodies give their region's default (`BodyRegion::priority`:
  torso and groin 3, head 2, limbs 1, CA's data defaults).
- `scale: Option<f32>`: the body's own damage factor for this shape
  (CA's zone factor: the female head is 1, the male 6). Guns whose table
  says so use it instead of their per-region value (below).

Shape queries live on `Hitbox`/`Hitboxes` (core), each working on boxes
and spheres alike, so a gun's rule works on any body:

- `ray_span(origin, dir)`: entry and exit (CS:S: the nearest box entered).
- `segment_passes(start, end)` and `power(start)`: Combat Arms' client
  rule (of the shapes the whole segment passes, the smallest
  |p|^2 - r^2; a box counts by its bounding sphere).
- `line_passes(at, dir)`: Combat Arms' server walk rule (centre within
  reach of the line; the highest priority wins).

Who fills `Hitboxes` (the per-game adapters, converting the native
format):

- **CS:S** (`cs_source::game::props`): the MDL hitbox set as today; the
  region from the hitgroup refined by the box's bone name (`*Forearm*`,
  `*Hand*`, `*Calf*`, `*Foot*`, else upper arm or thigh). The head
  group stays Head (its neck box too), the stomach group Stomach: CS:S
  on CS:S hits the same boxes with the same hitgroups as before.
- **Combat Arms** (`combat_arms::game::characters`): the skeleton's hit
  table as spheres, posed every tick from `CaPose` (a body model's pose)
  or the measured skeleton (`geometry_at`), in the body's frame; region
  from location and node (head; torso: the neck sphere Neck, the rest
  Chest, so Combat Arms' own hits keep reporting the chest hitgroup;
  groin; arms by side (`at.x`, as its hits report it) and
  upper/forearm/hand; legs by side, the pelvis spheres Thigh, then shin,
  foot: `HitSphere::region`), `priority` and `scale` (the zone factor) from the table.
  Characters without a model get the same from the measured skeleton,
  so every CA body has `Hitboxes`. Their `HitSpheres` stays as the
  table's index (which skeleton).

Lag compensation needs nothing new: `HitHistory` keeps `Hitboxes`
(interpolated per shape, spheres' radii too), so whatever the body's
game, a rewound shot sees the drawn pose of then. Combat Arms' own guns
keep posing Combat Arms bodies at the shot (this tick's pose, and the
server's pose for bots' shots: `CaPose::server_spheres`, no `Torso`
pitch), as the same shapes; only another game's body comes from its
`Hitboxes` (rewound: from `HitHistory`). A body without hitboxes keeps
each gun's own fallback (CS:S: height bands; Combat Arms: its measured
skeleton).

### Damage table: per gun, from the gun's game

`weapon::RegionDamage` (on `DamageEffect` and `Swing`; data:
`[damage.hitgroups]` and new `[damage.regions]`):

- `scale`: a multiplier per region. Data gives the coarse five (head,
  chest, stomach, arm, leg, as today) and may refine any region (neck,
  groin, upper_arm, forearm, hand, thigh, shin, foot); unset fine
  regions take their coarse group's value (CS:S: neck as chest, groin
  as stomach, every arm and leg part as arm and leg), Generic 1.
- `body_scales: bool`: a shape's own `scale` wins over the table (Combat
  Arms: the skeleton's zone factor; on a body that has none, CS:S's
  boxes, the table's value, which CA's data sets to the male
  skeleton's: head 6, neck, chest, stomach, groin 1, arms and thigh
  0.9, shin 0.75, foot 0.5).
- `after_armor: bool`: the multiplier applies to what armour leaves
  (Combat Arms) or to the raw damage before armour (CS:S).
- `armor_ratio: Option<f32>`: CS:S's `WeaponArmorRatio` (None: armour
  doesn't apply); Combat Arms' guns 1.0 (see armour).
- `class: DamageClass`: what armour coverage is looked up by (bullet,
  melee, blast, burn, poison, flash, other numbers for a game's own;
  Combat Arms maps its `InstDamageType` onto it).
- `quantum`: health's whole points (CS:S 1 point, CA 0), as today.

### Armour: one component, coverage per region, rules as data

`weapon::Armor` becomes the shared one:

- `amount` (normalized like `Health`, 1.0 = 100 points), and the pieces
  worn: `pieces: Vec<ArmorPiece>`, each a name and coverage entries
  `{ class: Option<DamageClass> (None: every class), head, torso, arm,
  leg, groin: f32, wear: f32 }`.
- `rules: ArmorRules` (the armour's game's, data): how it runs out
  (`Depletion::Cost`, CS:S: when what it would lose exceeds what is left,
  health takes raw - amount / wear; `Depletion::Blocked`, Combat Arms:
  when what it blocks reaches what is left, health takes raw - amount).
- CS:S: kevlar is one piece (torso, arm, groin 0.5, every class, wear
  0.5), the helmet another (head 0.5); `helmet()` and `with_helmet()`
  keep the old reads (HUD, buy menu, economy, demo, pain sounds).
- Combat Arms: each gear piece's protection entries as coverage (its
  fractions; wear 1 if consumable, else 0); `CaArmor` goes, its points
  becoming `amount`.

One function, `absorb(raw, region, gun, armour) -> (to_health, taken)`:
P = the sum of the region part's coverage for the gun's class (at most
1); blocked = raw x P x (2 - ratio); wear = the coverage-weighted wear;
health takes raw - blocked, armour loses blocked x wear, both truncated
by the gun's quantum (what armour has left is truncated, as CS:S
measured); running out per the rules. Zero armour covers nothing.

Checks: CS:S kevlar (P 0.5, wear 0.5) gives health raw x ratio x 0.5 and
armour half the rest, `armor_split` exactly. Combat Arms (ratio 1) gives
blocked = raw x P and armour loses blocked x consumable share,
`armour_and_zone` exactly. Across games: a CS:S AK-47 (ratio 1.55) on
CA's default body gear (torso 0.15) blocks 0.15 x 0.45 of the damage; a
CA M4A1 on CS:S kevlar blocks half. Neither is either game's; both are
the shared rule, and nothing is per pair.

Which armour a character wears is its identity's game's
(`slots::GameIdentity`): CS:S gives kevlar by buying, Combat Arms gives
its gear at spawn to its own characters only (today `give_armour` gives
it to everyone). One `Armor` per character, shown by whichever HUD is
up, applied to every gun.

### One hit path: `weapon::hits`

`land_hit(target, raw, region, shape_scale, gun: &RegionDamage, ...)`:
table multiplier (before or after armour), `absorb`, whole points,
refused hits taking no armour (`DamageRules::lands`), then the
`Damage`, `BulletHit` and `WeaponEvent::Hit` as today. Every gun calls
it: CS:S's (`deliver::Shot::apply`, melee too) and Combat Arms' (`hurt`
keeps its victim-side steps around it: the attachments' victim factor,
AI factor and cap, friendly-fire scale, infected groin, death
threshold). Blasts (`grenade`, CA's grenades) and maps' damage
(`MapDamage`) take armour through `absorb` too.

What stays the gun's game's: its ballistics, as data and hooks (spread,
pellets, CS:S's penetration and nearest-box rule, Combat Arms' client
test, server walk, widened boxes, behind bonus, range bands). Those are
the gun's behaviour and work on any body because they only ask the
shape queries above. Nothing is written per gun-body pair.

### Network

Nothing new goes over the wire for hits: the server poses `Hitboxes`
and decides damage; clients predict their own shots' effects only.
`Armor` replaces `CaArmor` in Combat Arms' replicated parts (its pieces
by name, as `GEARS` today) and is replicated for CS:S as it is now.

## What changes for players

- Plain CS:S, plain Combat Arms: nothing (pinned).
- Party apps (Lucker Party's gun game, the Ultimate Gun Game):
  - A CS:S gun on a Combat Arms soldier hits the drawn sphere body by
    region: a head shot is a head shot (x4), a leg shot x0.75, no more
    height bands on the collider.
  - A Combat Arms gun on a CS:S-dressed player hits CS:S's posed boxes:
    head x6, foot x0.5 and so on by the CA table; the CS:S body's pose
    (crouch, lean, animation), not CA's standing skeleton.
  - Armour: one per character, its game's. A CS:S-gun player's kevlar
    stops part of a CA round; a CA-gun player's gear (shown on its HUD)
    stops a little of a CS:S bullet. No more unseen armour.

## Steps (each committed with its tests)

Lanes: `chimera_core` and `chimera_sim` (types, delivery), `cs_source`
(adapter), `combat_arms` (adapter), `party_kit` (cross-game tests). Guard
tests per step: both games' parity pins, CS:S `weapons::`, CA
`weapon_firing::`, `characters::`, `gear_perks::`, `grenades::`, the
schedule and determinism ratchets.

- [x] **a. Pin parity.** CS:S: a table test through the sim (AK-47,
  Glock, AWP, knife; head, chest, stomach, arm, leg; no armour, kevlar,
  kevlar and helmet): health lost and armour left. Combat Arms: zones on
  both skeletons, armour by damage type, running out, the AI cap: health
  and armour after. Values recorded from the current code.
- [x] **b. Chimera types.** `BodyRegion`, `BodyPart`, `Hitbox` shapes and
  queries, `RegionDamage`, `DamageClass`, `ArmorPiece`, `ArmorRules`,
  `absorb`, with unit tests (including `absorb` against `armor_split` and
  `armour_and_zone` on their spec cases).
- [x] **c. CS:S onto them.** Hitbox regions from bone names; `DamageEffect`
  and `Swing` carry `RegionDamage`; `deliver` lands hits through
  `weapon::hits`; `Armor` with pieces (kevlar, helmet). Pins unchanged.
- [ ] **d. Combat Arms onto them.** (d1) CA bodies fill `Hitboxes`
  (spheres, every tick, `ServerHitboxes` for the server's pose): CS:S
  guns hit CA bodies by region. (d2) CA's delivery reads `Hitboxes` of
  any body through the shape queries (`HitSpheres` and its own sphere
  geometry go). (d3) CA's `hurt` lands through `weapon::hits` with CA's
  table (`body_scales`, `after_armor`). (d4) `CaArmor` becomes `Armor`
  (gear pieces as coverage; net, HUD, gear, perks, grenades, melee),
  given to CA characters only. Pins unchanged at every sub-step.
- [ ] **e. Cross-game tests** (party_kit or the lowest crate with both):
  a CS:S gun on a CA body and a CA gun on a CS:S body: head shot = head
  region, leg = leg; armour applies by the shared rule (CS:S kevlar
  against a CA round, CA gear against a CS:S round).
- [ ] Docs: backlog items 4 and 5, module `//!` docs, DESIGN.md
  ("Impacts across games" gains "Hits across games"), this plan's
  progress; delete it when done.

## Progress

- 2026-10-27, a: pins in `cs_source` tests/it/weapons.rs
  (`parity_pin_damage_and_armour_by_hitgroup`: AK-47, Glock, Deagle, AWP
  by hitgroup and armour, recorded from the code before any change) and
  `combat_arms` tests/it/weapon_firing.rs
  (`parity_pin_zones_through_armour`: every zone of the male skeleton
  through body and head gear).
- b and c (one commit): `core::BodyRegion`, `BodyPart`, `Hitbox` with
  `region`, `radius`, `priority`, `scale` and the shape queries
  (`ray_span` for spheres too, `segment_passes`, `power`,
  `line_passes`, `Hitboxes::{nearest, by_power, by_priority}`);
  `weapon::hits` (`RegionScale`, `DamageClass`, `Coverage`, `Depletion`,
  `absorb`, unit tests against `armor_split` bit for bit and Combat Arms'
  spec cases); `DamageEffect::regions`; `deliver` lands hits by region and
  `HitArmor` takes armour through `absorb` (`Armor::entries`: `KEVLAR`,
  `HELMET`); CS:S hitboxes get regions from their bones
  (`props::body_region`). Not yet: `RegionDamage` as one struct with
  `body_scales`/`after_armor` on `DamageEffect` (Combat Arms' guns don't
  go through `deliver`; their table is `weapons::zone_factor` and the
  body's own scale for now), `[damage.regions]` in the data.
- d1, d2: Combat Arms bodies fill `Hitboxes` (`characters::hitboxes`,
  `CaHitboxes`); Combat Arms' delivery tests shapes (`Body::shapes`:
  its own spheres posed at the shot as before, another game's body's
  `Hitboxes`, rewound too) with the shape queries, and its victim step
  takes the region's hitgroup, location and zone factor. Cross-game
  tests: `party_kit` tests/it/cross_game.rs (a CS:S gun on a Combat Arms
  body: head, right hand, right foot; a Combat Arms gun on a short CS:S
  body: head, chest, shin).
- d3: Combat Arms' armour step (`armour_and_zone`: its hits, melee and
  grenades) is `hits::absorb` with its gear as `Coverage`
  (`Gear::coverage`, `class_of`: its damage types as `DamageClass`),
  ratio 1, fractions kept, `Depletion::Blocked`; pins unchanged.
- d4 and the armour half of e: `CaArmor` is gone; Combat Arms' armour
  is `weapon::Armor` (points / 100) with its gear as
  `weapon::ArmorPieces` (`weapons::gear_armour`, `pieces_of`,
  `default_armour`; net replicates both; HUD, death cam, gear, perks,
  grenades, melee, special devices read them). `HitArmor` covers by the
  pieces when worn, else the default kit. Armour follows the identity
  (`weapons::armour_follows_identity`; DESIGN.md "Hits across games"):
  one value, Combat Arms' pieces on its own characters only. Tests in
  `party_kit` tests/it/cross_game.rs (kevlar against a Combat Arms round,
  Combat Arms' gear against a CS:S round, the armour across a switch on
  both HUDs). DESIGN.md has the section; this plan goes next.
