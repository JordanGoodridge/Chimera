# Combat Arms: effects and sounds

Source basis: the reference material the user pointed this session to. That
covers the client's weapon-effect path (muzzle flash, tracer, impact, decal,
shell casing, fly-by and hit-confirm handling), the client's effect manager
and effect plug-ins (particle systems, sprites, dynamic lights, camera
jitter), the client's character effects (footsteps, hurt and team call-outs),
the client's sound manager and the engine's sound mixer, the server's weapon,
projectile, explosion, damage and sound code, and the shipped data: the
effect, surface, weapon/ammo, modification, model, sound-set, auto-cast sound
and client attribute tables of the US client's attribute data, the effect
files (`CLIENTFX.FXF`, read as text) of the US 1501.04 and BR 1810.04 client
archives (identical for every group used here except the grenade
explosions' camera jitter, see section 9), texture headers from the
`FX` archive and footstep keys from the male character animation file.
Nothing was checked against a running game; see Open questions.

Status: draft

Sections: Summary; Units and conventions; Constants; How effects are
described as data (the effect file, the impact/fire/tracer tables, the
surface table); Behavior (1 who makes what, 2 visibility gates, 3 muzzle
flash, 4 tracers, 5 surface type of a hit, 6 impacts and decals, 7 blood,
8 shell casings, 9 grenade explosion, flash and smoke looks, 10 how a sound
is played, 11 sounds by event, 12 who hears what); Per-frame order; Edge
cases; Quirks; Test cases; Open questions.

---

## Summary

- Every visible and audible consequence of a shot is built on the **client**
  from one "weapon effect" event: the shooter's own client builds it at once
  from its own ray, every other client builds it from the copy the server
  sends to **all** clients. Grenade detonations use the same event. The
  look of almost everything (muzzle flashes, impact bursts, explosions,
  smoke) is data: named effect groups in the game's effect file, picked by
  the weapon, the ammo, the attachment and the hit surface's type.
- Bullet holes are 64 px sprites at about 3–4 u, solid for 3 s, fading out
  over the next 3 s, at most 101 at a time. Tracers are flat textured
  strips drawn **without depth test** (visible through walls), one every
  n-th shot per weapon, flying at 6000–10000 u/s.
- Sounds are 3D samples with an inner radius of 7 % of the outer radius and
  a game-side gain that falls with the **square** of distance and reaches
  zero at about 93 % of the outer radius. Beyond 1200 u a low-pass filter
  muffles them; that is the only "far" variant of any sound. Nothing is
  occluded by walls. A footstep is heard through walls up to about 1119 u.
- Footsteps of other players come from their running and jumping animations
  only: walking (walk key) and crouch-walking are silent for everyone. Your
  own footsteps are timed by the view sway at 2.8 steps/s while running.

## Units and conventions

- Distance: LithTech world units ("u"), the same as the other Combat Arms
  specs. +Y is up; left-handed frame (+X right, +Z forward).
- Time: seconds unless stated. Effects run on the **client's real frame
  time**, so everything below is frame-rate dependent where said.
- "Effect group" means a named entry of the effect file (`.fxf`), for
  example `AK47_PV_Muzz`; "key" means one effect inside a group.
- Colours in the effect file are 0..1. Their alpha value is stored
  **inverted**: 0 means opaque, 1 means transparent. Tables below give the
  file's numbers and say "opaque"/"transparent" where it matters.
- Volume: 0..100 as in the data; gain: 0..1 as sent to the mixer.
- Field and record names in `code font` are keys and names of the shipped
  data or console variables a player can set.
- The client's special-effects detail value is the constant 3 in the
  release build, which equals none of the three levels the weapon-effect
  code tests (0 low, 1 medium, 2 high). Everything below is written for that
  value (see Quirk 9). The separate effect-file detail level
  (`ClientFXDetailLevel`, 0 low, 1 medium, 2 high) is the player's option.

## Constants

Fixed in code (console variables in `code font` can be changed by the
player; the value is the default).

| Name (ours) | Value | Unit | Meaning |
|---|---|---|---|
| view_cone_dot | 0.6 | cos | `WeaponFXMinFireDot`, `WeaponFXMinImpactDot`: a point is "seen" if camera forward · direction to it ≥ 0.6 (≈ 53°) |
| far_fire_dist | 1000 | u | `WeaponFXMaxFireDist`: beyond, no shell casing |
| far_impact_dist | 1000 | u | `WeaponFXMaxImpactDist`: beyond, own decals only while zoomed |
| near_other_impact | 300 | u | `WeaponFXMaxMultiImpactDist`: other players' impacts this close count as seen even behind you |
| tracer_min_dist | 100 | u | No tracer for a shot shorter than this |
| mark_solid_time | 3.0 | s | `MarkSolidTime` |
| mark_fade_time | 3.0 | s | `MarkFadeTime` |
| mark_max | 101 | marks | Client decal list size (100 + 1); oldest replaced |
| mark_region | 10 | u | Radius for the "too many marks here" rule (100 as a squared distance) |
| mark_region_max | 10 | marks | More than this inside the region: move one instead of adding |
| server_marks_per_mover | 40 | marks | Decals on one moving world model (doors), server side |
| tracer_list_max | 50 | tracers | Oldest replaced |
| shell_list_max | 600 | shells | Oldest replaced |
| shell_life | 3.0 | s | Every casing |
| shell_bounces | 3 | — | Then it rests |
| shell_grow_time / factor | 0.5 / 2 | s / × | Casing scale grows to 2× its data scale |
| shell_gravity | −1000 | u/s² | Client object gravity (applied at half strength, Shell physics) |
| shell_bounce_damp | 0.5 | × | Speed kept per bounce (water: 0.25, gravity −500) |
| flyby_radius | 600 | u | `FlyByRadius` |
| flyby_impact_guard | 0.75 | × | No fly-by if the impact is within 0.75 × its sound radius of you |
| inner_radius_fraction | 0.07 | × | Inner radius = 7 % of outer when not given |
| lowpass_start / span | 1200 / 600 | u | Distance low-pass ramp |
| voice_max | 255 | sounds | Mixer sample handles and live sound instances |
| footstep_radius | 1200 | u | `FootStepSoundRadius` |
| quiet_factor | 0.65 | × | `QuietMovementVolumeFactor` (effectively unused, Quirk 12) |
| ding_slots | 5 | — | Pending hit-confirm sounds |
| server_sound_send | 2 | × outer | Server sounds are sent only to clients within 2 × outer radius |

## How effects are described as data

### The effect file (`CLIENTFX.FXF`, in the `CLIENTFX` archive)

- One text file (a binary variant exists; the reader tells them apart by
  whether the file starts with `Groups:`). The archive also holds
  `CLIENTFX.FCF`, the editor's layout, which the game does not read.
- Text grammar, one `Tag: value` per line, indentation ignored:
  - `Groups: G`, then G groups.
  - A group: `GroupName: name`, `FxInGroup: K` (number of keys), `Phase: P`
    (group length in **ms**), then K keys.
  - A key: `FxName: Type;Label` (the effect type, optionally a label after
    `;`), `FxID`, `Linked` (0/1), `LinkID`, `LinkNode`, `StartTime` and
    `EndTime` (s, clamped to 0..P/1000), `Repeat`, then three values the
    game ignores (`TrackID`, `MinScale`, `MaxScale`), `Properties: N`, then
    N properties.
  - A property: `PropName: name`, `Type: t`, then one value line, by type:
    0 string, 1 integer, 2 float, 3 combo (`index,choice,choice…`; the index
    is the value), 4 vector (3 floats), 5 four floats, 6 colour key
    (`time r g b a`, alpha inverted), 7 path (`ext|in-archive path`).
  - Keys named `Ck` are colour keys and `Sk` are scale keys
    (`time value 0 0`). Both use time 0..1 over the key's own lifetime, or
    over each particle's lifetime for particle systems, with linear
    interpolation. Particle colour keys are evaluated per particle.
- Group names are looked up without regard to case. A name that is missing
  from the file plays **nothing**, silently.
- A key that lasts the whole group (length ≥ group length − 0.01 s) loops
  when the group is created as looping; otherwise every key plays once and
  then shuts down. With `SmoothShutDown` = Yes a shutting particle system
  stops emitting but keeps its live particles until they die.
- Common properties of every key: `UpdatePos` (0 fixed, 1 follow,
  2 player view, 3 node attach, 4 socket attach, 5/6 the same on the view
  model), `AttachName` (socket, e.g. `Flash`), `Offset`, `DetailLevel`
  (0 all, 1 high, 2 medium, 3 low, 4 medium+high, 5 low+medium, 6 low+high;
  a key whose level does not include the player's `ClientFXDetailLevel` is
  not created), `DisableAtDist`.
- Effect types used by the shipped weapon effects, with what their
  properties mean:
  - **ParticleSystem**: `Texture`, `EmissionInterval` (s; ≤ 0 means
    **one burst** on the key's first frame and nothing after),
    `ParticlesPerEmission`, `MinParticleLifeSpan`..`MaxParticleLifeSpan`,
    `Type` (0 sphere, 1 point, 2 circle, 3 cone, 4 cylinder) with
    `MinRadius`..`MaxRadius` and `EmissionPlane`, `MinParticleVelocity`..
    `MaxParticleVelocity` (per-axis uniform box in the effect's frame),
    `GravityAcceleration` (added to the Y acceleration; positive is up),
    `AdditionalAcceleration`, `Friction` f (velocity × (1 − f)^dt each
    update, dt in s), `PercentToBounce` (chance a particle bounces off the
    world, keeping 75 % of its speed), `SplatPercent` and `SplatEffect`
    (chance a particle that hits the world spawns that group at the hit
    point, oriented to the hit normal; `KillOnSplat` removes it),
    `BlendMode` (0 none, 1 additive, 2 multiply), `MoveParticlesWithSystem`,
    `RotateParticles` with angular velocity in degrees/s. Scale keys give
    the particle size s; a particle is drawn as a camera-facing square of
    side 4·s (Open questions 3).
  - **Sprite**: a sprite file, `Facing` (0 camera, 1 along the normal,
    2 parent aligned), `DisableZ` (draw on top of the world),
    `CastVisibleRay` (hide the sprite while the line from it to the camera
    is blocked). Scale keys scale the sprite.
  - **DynaLight**: a point light; scale keys are its radius in u, colour
    keys its colour. It lights the world only, with no shadows (Open
    questions 4).
  - **CamJitter**: each frame moves the camera by a random offset in
    [−a, a] on each axis, a = scale × falloff; falloff is 1 within
    `InnerRadius`, 0 beyond `OuterRadius`, and linear in **squared**
    distance between.
  - **LightningFX** (spark arcs), **CreateFX** (plays another group,
    `FXName`), **PlaySound**.
- The effect's frame: an effect created with a target normal (impacts,
  explosions) gets its local +Z along that normal; its local +Y and +X are
  built from the world up axis (from world +X when the normal is exactly
  vertical). For a floor (normal +Y) the local axes are X → world +Z,
  Y → world +X, Z → world +Y.

### Effect tables in the attribute data (`FX.TXT`)

- **ImpactFX** records (the ammo's `ImpactFXName`, `UWImpactFXName` under
  water): `Sound` (a file, or `surface` = pick by surface and ammo kind),
  `SoundRadius`, `CreateMark`, `Mark` (`surface` = the surface's bullet-hole
  sprite; empty = none), `MarkScale`, `MarkColorRatio` (default 0.5),
  `DoSurfaceFX` (also play the surface's impact group), `IgnoreLiquid`,
  `ScreenTint` with tint fields, `BlastMark` with blast-light fields,
  `PusherName`, `FXName` (an effect group always played), `ImpactOnSky`.
  AI fields are for bots.
- **FireFX** records (the ammo's `FireFXName`): `FXName` (a group the
  **shooter alone** sees at their muzzle), `EjectShells`, `ShellModel`,
  `ShellSkin`, `ShellScale`, `Velocity` (non-zero replaces the random
  casing velocity), `FireSound`, `ExitMark`, `ExitDebris`.
- **TracerFX** records (the ammo's or an attachment's `TracerFXName`):
  `Texture`, `Frequency`, `Velocity` (u/s), `Width` (u), `Length` (u),
  `Color` (0..255), `InitialAlpha`, `FinalAlpha`.
- The weapon record names the muzzle groups (`PVMuzzleFXName`,
  `HHMuzzleFXName`) and the sounds (`FireSnd`, `FireSndRadius`,
  `SilencedFireSnd1..`, `SilencedFireRadius1..`, `DryFireSnd`,
  `ReloadSnd1..3`, `ReloadSndRadius`, `SelectSnd`, `DeselectSnd`,
  `SelectSndRadius`). The muzzle attachment can name its own
  `PVMuzzlefxname`, `HHMuzzlefxname` and `TracerFXName`
  (`attachments.md`).

Stock records used by the common weapons:

| Record | Used by | Content |
|---|---|---|
| ImpactFX `BulletFmj` | every gun | sound `surface`, radius 900; decal `surface`, `MarkScale` 0.075; surface FX on; pusher `BulletExpPusher`; no group |
| ImpactFX `Melee` | knives | sound `surface`, radius 153; no decal; no surface FX |
| ImpactFX `handgrenade` | M67 | sound `gren.wav`, radius 1500; `CreateMark` 1 but `Mark` empty (no scorch); surface FX on; no group |
| ImpactFX `flashbang_exp` | XM-84 | sound `flash.wav`, radius 1500; group `Flashbang_Ini`; no surface FX |
| ImpactFX `Smoke_Grenade` | M18 | sound `Smoke_Exp_GAS.wav`, radius 1500; group `Smoke_Grenade_01`; no surface FX |
| FireFX `5.56mm`, `7.62mm`, `9mm`, `45ACP`, `12Gauge_G` | rifles, SMGs, pistols, shotgun | own-muzzle group `test`, shells on (casing model by calibre) |
| FireFX `FireSoundOnly` | knives | nothing but the fire sound |

Missing vs empty ammo keys: a present but empty `ImpactFXName`,
`UWImpactFXName` or `TracerFXName` falls back to `BulletFmj`, `UWBullet`
and `Standard` (`attribute_data.md`); a missing key gives no effect.

### The surface table (`SURFACE.TXT`)

Per surface record (`Id` = the surface type of `world_dat.md`): `ShowsMark`,
`BulletHoleSpr` (with `BulletHoleMinScale`/`MaxScale`), `ImpactFXName`,
`UWImpactFXName`, `GrenadeFXName`, `UWGrenadeFXName`, `ExitFXName`,
`BulletImpactSnd1..4`, `ProjectileImpactSnd1..4`, `MeleeImpactSnd1..4`,
`HandsImpactSnd1..4`, `FistImpactSnd0..3`, `BowImpactSnd0..4`,
`ShellImpactSnd1` with `ShellSndRadius`, `GrenadeImpactSnd` with
`GrenadeSndRadius`, `RtFootSnd1..2` and `LtFootSnd1..2`, body-fall sounds,
`Hardness` (grenades), `Invisible`, and penetration fields
(`weapon_firing.md`). A `[CustomSound]` list names extra per-surface sound
lists for special characters (robots, bosses, VIP).

| Id | Name | Decal (64 px sprite, scale range) | Impact group | Grenade group | Bullet sounds defined | Footsteps |
|---|---|---|---|---|---|---|
| 0 | Unknown | none | none | `Grenade_Dirt` | 3 (stone) | tile |
| 1 | Air | none | none | none | 0 | none |
| 4 / 6 | ImpactAdultHead / Body | none (characters take no decal) | `Impact_Head` / `Impact_Body` | `Grenade_Stone` | 4 (cloth) | — |
| 5 / 7 | ImpactTeenHead / Body | none | `T_Impact_Head` / `T_Impact_Body` | `Grenade_Stone` | 4 | — |
| 10 | Ceramic tile | `im_concrete` 0.6–0.8 | `Impact_Stone` | `Grenade_Stone` | 4 | tile |
| 11 | Cement | `imark_ceramic` 0.6–0.8 | `Impact_Stone` | `Grenade_Stone` | 4 | stone |
| 20 | Metal | `im_metal` 0.6–0.7 | `Impact_Metal` | `Grenade_Stone` | 4 (metal_bg) | metal |
| 30 | Wood | `im_wood` 0.8–0.9 | `Impact_Wood` | `Grenade_Dirt` | 4 | wood |
| 40 | Glass | `im_glass` 0.6–0.7 | none | `Grenade_Stone` | 4 | glass |
| 41 | Bullet-Proof Glass | `im_glass_bp` 1.0–1.1 | `Impact_Glass_BulletProof` | `Grenade_Stone` | 4 | glass |
| 60 | Desert_Sand | `im_sand` 1.2–1.3 | `Impact_Sand` | `Grenade_Sand` | 4 | sand |
| 70 / 74 | Dirt / Dirt Road | `imarkbasic` 0.6–1.0 / 0.6–0.8 | `Impact_Dirt` | `Grenade_Dirt` | 4 | dirt |
| 71 | Snow | none | `Impact_Snow` | `Grenade_Snow` | 2 | snow |
| 73 | Grass | none | `Impact_Dirt` | `Grenade_Stone` | 4 | grass |
| 80 / 81 | carpet / Cloth (shoot-through) | `cloth` / none | `Impact_Cloth` | `Grenade_Stone` | 4 | cloth |
| 90 | Linoleum | `imarkbasic` | `Impact_Stone` | `Grenade_Stone` | 2 | linoleum |
| 100 / 101 | Wood Door / Metal Door | none (`ShowsMark` 0) | `Impact_Wood` / `Impact_Metal` | `Grenade_Stone` | 4 | wood / metal |
| 110 | Sky | none | none | none | 0 | — |
| 200 | Ladder | none | none | `Grenade_Dirt` | 0 | ladder |
| 201 / 204 | Liquid / Water | none | `Impact_Water` | `Grenade_Water` | 4 | water |
| 202 | Invisible | none | none | none | 0 | none |

The other 40-odd records follow the same pattern (see the table for door,
cabinet, trash-bag, paper, foliage and so on). Scale ranges are
`BulletHoleMinScale`..`BulletHoleMaxScale`.

## Behavior

### 1. Who makes what

- **The shooter's client**, at the moment a round is fired (`weapon_firing.md`
  step 6), builds one weapon effect per ray it casts, from its own result:
  fire position = its first-person flash point, impact point = its ray hit
  backed off **1 u** along the ray, surface type and normal from its own
  test. It plays its own fire sound immediately (section 11).
- **The server**, for every shot it accepts and for every grenade
  detonation, sends one weapon-effect event to **all** clients (no
  distance or visibility filter). It carries the weapon, ammo, surface
  type, fire position, impact position, the normal compressed to one byte
  per axis, the five attachment ids, the impact kind, and a set of
  "ignore" bits the server chose: exit effects and alternate fire sounds
  always off; the normal fire sound off and the silenced one on when the
  weapon has a silencer; the tracer bit off unless this is a tracer shot
  (section 4); hit-confirm off unless the hit object is a character in a
  multiplayer game; decals off on movable objects (the server makes
  persistent decals on movable world models itself, section 6); the fire
  effects off for thrown and launched ammo.
- **The shooter's client receiving its own shot back** does nothing but the
  hit-confirm sound (section 11), except for critical hits, which it
  rebuilds in full. Thrown and launched ammo is not skipped.
- Everyone else builds the full effect from the event.

### 2. Visibility gates

Computed once per weapon effect from the camera at that moment:

- Fire point **seen** if camera forward · (fire point − camera)‾ ≥ 0.6; a
  fire point less than 0.001 u from the camera counts as straight up.
  **Far** if more than 1000 u away.
- Impact point **seen** if the same test passes, or, for another player's
  shot, if it is within 300 u of the camera. **Far** if more than 1000 u
  away.
- Muzzle flash and casing need a seen fire point; the casing also needs a
  near one. Decals and surface bursts need a seen impact. An impact's
  own-ammo group (smoke cloud, flash burst) is made when the impact is seen
  **or** the ammo does area or lasting damage, so smoke and flash looks
  always appear; a frag's actual explosion look comes from the surface
  burst and therefore needs a seen impact.
- No other culling: effects are not limited by distance.

### 3. Muzzle flash

- **First person** (your own gun): the view model carries a looping effect
  group named by the muzzle attachment's flash-hider level (unused in
  stock data), else the attachment's `PVMuzzlefxname`, else the weapon's
  `PVMuzzleFXName`. Nearly every stock weapon uses `AK47_PV_Muzz` (1353 of
  1361 weapon records; the rest are event and snowball weapons). It is shown for
  the group's length (50 ms) after each shot, at the view-model flash point,
  and hidden otherwise, when dead, and under water unless the weapon allows
  an under-water flash.
  - `AK47_PV_Muzz`: one additive particle of texture `AKPV_FX.DTX`
    (128×128), size 0.327 → 0.414 → 0.500 over its 40–50 ms life, white
    fading to black (additive, so fading out) from 28 % of its life; plus a
    dynamic light of radius 170 u, colour (0.97, 0.95, 0.87) → (0.56, 0.50,
    0.38), 50 ms, medium and high detail only.
- **Your own muzzle, extra**: the ammo's FireFX group (`test` for all guns)
  is created at the flash point, for you only: one smoke puff
  (`SHOOT_SMOKE.DTX`, size 4 → 16, life 0.5–0.8 s, velocity box
  (±64, ±64, 100..300) in the shot's frame, rising at 30 u/s², friction
  0.9998, medium/high detail) and a second light, radius 256 → 170 over
  50 ms, medium/high detail.
- **Third person** (other players, and you in a third-person camera): the
  muzzle attachment's `HHMuzzlefxname` if it has one, else the weapon's
  `HHMuzzleFXName`, created at the weapon's `Flash` socket (`Flash01` for a
  left-hand dual weapon), aligned to the shot. Not shown while you
  spectate in first-person chase view.
  - `M4_HH_Muzz` (M4A1, AK-47, MK.48 …): one additive particle
    `M4_02.DTX`, size 7.59 → 9.38 → 10.0, life 40–50 ms, moving 200–400 u/s
    along the shot; light radius 170 u, colour (0.89, 0.84, 0.69) → (0.23,
    0.20, 0.16), 50 ms, medium/high.
  - `WEAPON_HH` (SMGs, snipers, shotguns, and every A/B silencer):
    additive particle `GUN_DE.DTX` (64×64), size 10.6 → 13.1 → 15.0, life
    40–50 ms, 200–400 u/s; light radius 170 u, (1.0, 0.98, 0.90) →
    (0.49, 0.49, 0.49) fading to transparent, 50 ms, medium/high.
- **Silencers**: grades A and B (base and Gold) name `AK47_pv_muzz` and
  `WEAPON_HH`, so they still flash and light. Grade C (all colours) and
  every Red silencer name `Silenced01_PV_Muzz` / `Silenced01_HH_Muzz`,
  which do not exist in either effect file checked: **no flash and no light
  at all**, first or third person.
- The flash always lasts 50 ms regardless of fire rate; at 105 ms per M4A1
  round it is lit about half the time while firing.

### 4. Tracers

- **Which shots**, on the shooter's own client: a per-weapon counter
  c (starting at 0 when the weapon object is created or reset) advances
  once per ray:
  c ← (c + 1) mod F, with c ← 0 when F = 0; the shot gets a tracer when the
  new c is 0. F is the `Frequency` of the **last** fitted attachment (slot
  order) that names a tracer, else of the ammo's tracer. F = 1 and F = 0
  both give a tracer on every shot. Only the first ray of a shot can carry
  one; extra pellets advance c but cannot draw.
- **Which shots, as others see them**: the server keeps its own counter per
  weapon, the same rule but always with the **ammo's** F.
- **Which look**: the muzzle-slot attachment's tracer if it names one, else
  the ammo's. A shot whose ammo has no tracer never draws one, whatever the
  attachments say. Stock: pistols, shotguns, knives and grenades have no
  tracer.
- **Shape and motion**: a flat strip of constant width `Width` from the
  fire position toward the impact, textured with `Texture`, colour `Color`,
  alpha `InitialAlpha` → `FinalAlpha` over its life, not additive, drawn
  **with depth test off**. With t the time since its first update, v
  `Velocity`, L `Length`, D the fire-to-impact distance:

```math
\text{tail}(t) = P_{fire} + \hat d\, v t, \qquad \text{head}(t) = \text{tail}(t) + \hat d\,L\ \ (\text{clamped to } P_{impact}), \qquad \text{life} = D / v
```

- Not drawn when D ≤ 100 u, when the impact point is in liquid, or with
  `Tracers` 0. `TracerVeloctity` (sic) and `TracerLength` override v and L
  when > 0.

| Tracer | Used by | F | v (u/s) | Width | Length | Texture (px) |
|---|---|---|---|---|---|---|
| AR01 | AK-47, every sight and A silencer | 3 | 6000 | 25 | 300 | red 64×16 |
| AR02 | M4A1 | 4 | 6000 | 15 | 600 | yellow 64×16 |
| AR03 | G36E, C silencers | 4 | 6000 | 20 | 300 | yellow |
| AR04 | B silencers | 6 | 6000 | 15 | 600 | yellow |
| SMG01 / SMG03 | MP5 family / B SMG silencer | 6 / 8 | 10000 | 20 | 600 | yellow 128×16 |
| SMG02 | K1A | 6 | 8000 | 30 | 300 | red 128×16 |
| MG01 | machine guns | 3 | 6000 | 20 | 300 | yellow |
| SR01 / SR03 | M24 / B sniper silencer | 1 / 0 | 8000 | 20 | 600 | yellow |
| SR02 | SR-25 | 2 | 8000 | 20 | 600 | red |
| Pisol01 / Pisol02 | pistol silencers | 0 | 8000 | 20 | 600 | red (never drawn: pistol ammo has none) |

### 5. Surface type of a hit

- **World and world models**: the hit polygon's surface type, the u16 of
  `world_dat.md`'s surface record.
- **If that is 0 (unknown)** and the hit object is a model:
  - a character: by hit location, the model's `HeadSurfaceType` for a head
    hit, else its `BodySurfaceType`; when the model sets none (stock player
    models), 4 (ImpactAdultHead) for heads and 6 (ImpactAdultBody)
    otherwise. Robot-like models set 20 (metal). Low-violence and the
    Brazilian "white blood" option swap 4/6 for the teen types 5/7.
  - any other model: the object's own surface type (set by the server from
    its properties).
- Other client tests (blood, casings, footsteps) look at the object first:
  a hit object that is not the main world gives its object surface type,
  and the polygon is used only if that is 0.
- **Liquid**: a ray that hits surface 201 (Liquid) makes that impact and
  then continues through the water, making a second impact at whatever is
  beneath.
- **Under water**: an impact whose point + normal is inside a liquid
  volume uses the ammo's under-water impact record, the surface's
  under-water groups (`UWImpactFXName`, `UWGrenadeFXName`, none defined in
  stock data) and half volume for the impact sound.
- A type that has no record in the surface table falls back to the record
  with `Id` 0 (Unknown).
- No ray hit at all: the client treats it as a sky impact at the end of the
  ray (type 110), and the server's event says sky too.

### 6. Impacts and decals

For a non-sky impact (or any impact when the ImpactFX has `ImpactOnSky`),
in this order:

1. **Sound** (section 11).
2. **Ammo effects**: the pusher (if any), the ImpactFX's own `FXName` group
   at the impact with the surface normal, and, for `BlastMark` records at
   high detail (never, Quirk 9), a light. A movable object hit with an
   ammo override record, a ricochet or a "blocked" impact uses those
   records instead.
3. If the impact is seen:
   - **Decal**, when the ImpactFX has `CreateMark`, the surface
     `ShowsMark`, `MarkShow` is on, the hit object can take decals (the
     main world and static world models; not characters, not movable
     objects), and, for your own shot at a far impact, only while zoomed.
   - **Surface burst**: when the ImpactFX has `DoSurfaceFX`, the surface's
     `ImpactFXName` (or `GrenadeFXName` for thrown ammo; under water the
     `UW…` names) is created at the impact with the surface normal. A
     critical hit from a bullet attachment with a critical effect uses that
     effect instead.

**Decal details** (client):

- Sprite: the surface's `BulletHoleSpr` (all stock decals are 64×64 px),
  laid flat on the surface, rotated by a uniform random angle about the
  normal, at the impact point (already 1 u off the surface along the ray).
- Size: scale = `MarkScale` × U(`BulletHoleMinScale`, `BulletHoleMaxScale`).
  A sprite at scale k is drawn 64·k u wide (Open questions 3): 0.075 ×
  0.6..0.8 gives 2.9–3.8 u.
- Colour: grey `MarkColorRatio` (0.5) on each channel, lit by the world
  unless `MarkLight` is 0. Not clipped to the polygon unless `MarksClip`
  is 1.
- Life, with t the decal's age (real frame time, frozen while the game is
  paused):

```math
\alpha(t) = \begin{cases} 1 & t \le T_s \\ \min\!\left(\alpha,\ \dfrac{T_s + T_f - t}{T_s}\right) & T_s < t \le T_s + T_f \\ \text{removed} & t > T_s + T_f\end{cases},\qquad T_s = 3,\ T_f = 3
```

  (the divisor is the solid time, not the fade time; equal by default).
- **Crowding**: before adding, every existing decal closer than 10 u is
  counted, in list order. When the count passes 10, the closest decal seen
  so far is moved to the new point (keeping its age, rotation and size)
  and no decal is added.
- **Cap**: 101 decals; a new one replaces the one that has been in the list
  longest.
- **Movable world models** (doors and the like): the server attaches
  persistent decal objects, up to 40 per object (the same 10-u crowding
  rule; at the cap the closest or farthest is moved), fading with the
  server's own solid/fade times (Open questions 9).

**Surface bursts** (effect groups; detail "m/h" = medium and high only):

| Group | Length | Content |
|---|---|---|
| `Impact_Stone` | 1.0 s | plays `Impact_Dirt` |
| `Impact_Dirt` | 1.0 s | 3 smoke puffs (`SHOOT_SMOKE`, sizes 5→10, 8→15, 10→20, lives 0.4–1.0 s, rising 300–700 u/s along the normal, 2 of them m/h); 6 dirt chips (size 0.5–1.2, gravity −1000, friction 0.5, 25 % bounce, m/h); a spark arc (m/h); an additive flash sprite 0→0.1 over 0.1 s |
| `Impact_Metal` | 1.25 s | spark arcs; additive arc sprite 0→0.25; 6 additive sparks (size →0.6, 0.2–0.3 s, gravity −100, m/h); 2 smoke puffs (m/h); a glow sprite (m/h) |
| `Impact_Wood` | 0.5 s | 6 wood splinters (3 textures, gravity −1000, 25–75 % bounce, 4 of them m/h); 2 brown smoke puffs (m/h); spark arc (m/h); flash sprite |
| `Impact_Cloth` | 1.25 s | 4 dust puffs (2 m/h); flash sprite; glow (m/h) |
| `Impact_Snow` / `Impact_Sand` | 1.0 s | like `Impact_Dirt` in white / sand colours |
| `Impact_Glass_BulletProof` | 0.5 s | dust puff; 2 glass chips and 2 debris (m/h); spark; flash sprite |
| `Impact_Water` | 2.0 s | splash ring sprite (1 s); 4 splash particles (gravity −500, m/h); a column particle |
| `Impact_Body` / `Impact_Head` | 1.0 / 0.5 s | blood (section 7) |

### 7. Blood

- **On hit**: a hit on a character uses the character surface types of
  section 5, whose bursts are blood groups. Only the bullet's impact
  effects make blood; there is no separate damage blood.
  - `Impact_Body`: 3 mist puffs (`128XBLOODMIST01`, sizes 5→15, lives
    0.4–1.0 s), 3 splash particles (sizes 7→20, 0.3–0.6 s, gravity −300),
    2 blobs (`BLOOD_BLOB02`, size 2→6, life 1.0–1.2 s, gravity −1000,
    friction 0.8), an additive flash sprite, and a red-orange light of
    radius 0 → 150 u over 0.1 s (m/h).
  - `Impact_Head`: 3 (low detail: 1) mist puffs growing to 24, 3 large
    splashes (10→30), 7 blobs (3 textures), an impact sprite drawn on top
    of the world (hidden when its line to the camera is blocked), a splash
    sprite (m/h), a light of radius 0 → 300 u (m/h), and a burst particle
    growing to 80 over 0.67 s.
  - Detail "m/h" pieces vanish at low effect detail.
- **On walls and floors behind**: blood blobs that hit the world (any
  object that is not a character) splat with `SplatPercent` (body blobs
  50 %, head blobs 50 % / 70 % / 70 %) and are removed. A splat creates a
  stain group at the hit point, flat against the hit surface with a random
  spin: `Blood_Stain01..03`, one 128×128 sprite at scale 0.25 (32 u),
  fading linearly from opaque to transparent over 3.0 s. Expected stains:
  body hit 1.0, head hit 4.3. Stains are not decals: they ignore
  `ShowsMark`, the decal cap and the crowding rule.
- **At death**: nothing extra. The older "blood splats along the shot"
  routine and the death and gib effects exist but are never triggered in
  this build.
- **Footprints**: running characters leave footprint sprites on surfaces
  that define them (none in stock data); infected characters leave blood
  footprints (`bloodfs_l/r`, 7 s, scale 0.125).

### 8. Shell casings

- **When**: with a FireFX that has `EjectShells`, `ShellCasings` 1, and a
  seen, near fire point (section 2). Bolt-action and pump weapons whose
  FireFX does not eject instead drop a casing on the view model's
  `SHELL_CASING` animation key (first person only).
- **Where from**: first person, the view model's shell-eject point; third
  person, the shooter's `Breach` socket (`Breach01` for a left-hand dual
  weapon).
- **Velocity**, in the frame of the shot (third person) or of the camera
  (first person), each uniform:

```math
\vec v_0 = \hat u\,U(30,75) \pm \hat r\,U(50,75) + \hat f\,U(20,50)\quad(\text{u/s};\ -\hat r \text{ for a left-hand dual weapon})
```

  replaced by FireFX `Velocity` when that is non-zero. Spin about the up
  and right axes at U(2, 10) rad/s each, random sign.
- **First-person casings move with you**: they start without your
  velocity, but every frame until they rest your own displacement
  (velocity × frame time) is added to their position.
- **Motion** per frame with frame time Δ (frame-rate dependent):

```math
\vec x \leftarrow \vec x + \vec v\,\Delta, \qquad \vec v \leftarrow \vec v + \tfrac12\,\vec g\,\Delta, \qquad \vec g = (0,-1000,0)\ \text{u/s}^2\ \ (\text{water: } -500)
```

- **Bounce**: on a world hit between the old and new position (characters
  ignored), the casing is placed 0.3 u off the surface, its velocity is
  mirrored about the normal and multiplied by 0.5 (water 0.25), pitch spin
  × 0.75 and yaw spin × −0.75. It rests after 3 bounces, or at once on a
  floor (normal y > 0.6) when |v|² < 225/Δ (|v| < 116 u/s at 60 fps). A
  casing that hits sky or an invisible surface, or leaves the world, is
  removed.
- **Look**: the FireFX's casing model and skin at `ShellScale`, growing
  linearly to 2 × that over 0.5 s. Removed after exactly **3.0 s**.
- **Sound**: on the **first** bounce only, out of water: the surface's
  `ShellImpactSnd1` at radius `ShellSndRadius` (200 u on stock surfaces),
  low priority.

### 9. Grenade explosion, flash and smoke looks

`grenades.md` defines when and where; the client turns the detonation event
into the following.

- **Frag (M67)**: sound `gren.wav`, radius 1500 (section 11); then, if the
  impact is seen, the surface's `GrenadeFXName` group: `Grenade_Dirt` (dirt,
  wood, plaster, rock and unknown surfaces), `Grenade_Stone` (stone, metal, glass, most
  others), `Grenade_Water`, `Grenade_Snow`, `Grenade_Sand`. No decal (the
  record's mark is empty), no screen tint. In mid-air the surface is Air,
  which has no grenade group: **a mid-air frag makes only its sound.**
  - `Grenade_Stone` / `Grenade_Dirt` (0.9 s): from 0.1 s an additive core
    of 4 particles (20→60), a glow sprite (0→10) drawn on top of the
    world but hidden when blocked, 3 rising smoke columns (2/4/4 particles,
    sizes up to 100, 1.5–2.0 s, 500–2000 u/s along the normal), a smoke
    ring of 10 particles on a circle of radius 64, all with velocity
    (500, 500, 0..50) in the effect's frame, an
    expanding ground ring, 3 burst puffs (80→160), 18 debris pieces with
    gravity −1000 (12 of them always bouncing), a light of radius 0 → 512
    (Dirt) or 0 → 256 (Stone) peaking at 32 % of 0.3 s, and a heat-shimmer
    particle at high detail. Low detail swaps the smoke for fewer
    particles and drops debris.
  - **Camera jitter** in both (US effect file): amplitude 60 u at the key's
    start falling linearly to 0 over 0.6 s, full within 200 u of the blast,
    zero beyond 800 u (falloff linear in squared distance). `Grenade_Dirt`'s
    jitter is high-detail only, `Grenade_Stone`'s at every detail. The BR
    file has `Grenade_Dirt` at amplitude 20 u at every detail (Open
    questions 16).
- **Flash (XM-84)**: sound `flash.wav`, radius 1500; group
  `Flashbang_Ini` (0.5 s), always created: two additive sprites drawn on top
  of the world and hidden while their line to the camera is blocked (a
  splash growing to full in 0.025 s and shrinking by 0.23 s, a glow 10 → 0
  over 0.35 s), a light of radius 0 → 512 → 0 between 0.059 and 0.28 s
  (peak at 0.127 s, m/h), 17 smoke particles and a heat-shimmer burst. The
  blinding white screen is `grenades.md` section 10.
- **Smoke (M18)**: sound `Smoke_Exp_GAS.wav`, radius 1500; group
  `Smoke_Grenade_01` (2.0 s), always created, at the detonation point with
  the detonation normal:
  - a 0.1 s additive splash sprite and a world light of radius 150 → 0
    over 0.1 s (m/h);
  - the cloud, one particle system (all details), answering
    `grenades.md` open question 10:
    - **one burst of 10 particles on its first frame**; `EmissionInterval`
      −1 means no further emission. The 2.0 s key only decides when it
      stops trying to emit; with smooth shut-down the 10 particles live
      out their own lives;
    - start points uniform in a sphere of radius 0–10 u;
    - velocities uniform in the box (−1000..1000, −1000..1000, −200..400)
      **in the effect's frame** (local +Z along the detonation normal; on a
      floor: horizontal ±1000 on both axes, vertical −200..400);
    - acceleration +10 u/s² along world +Y (rising);
    - friction 0.98 is **per second**: velocity × 0.02^Δ each update, so
      a particle travels at most v₀/ln 50 = 0.2556·v₀ (≤ 256 u per
      horizontal axis, −51..+102 u vertically) and then drifts up at
      10/ln 50 = 2.56 u/s (exact value depends on frame time);
    - every particle may bounce (100 %), keeping 75 % of its speed; there is
      no "full bounce";
    - life U(8, 10) s each; size 200 → 300 over its life (drawn as a square
      of side 800 → 1200 u, Open questions 3); colour keys fade it in from
      transparent to opaque over the first 5.1 % of its life, hold it
      opaque, and fade it out over the last 14.9 %;
    - non-additive, unlit; random spin up to 1°/s.
  - So the cloud is fully formed after about 0.5 s, starts thinning at
    0.85 × 8 = 6.8 s at the earliest, and is gone **10 s** after detonation
    at the latest (not 12 s), unless frozen off-screen (Quirk 7).
- **Incendiary and gas**: their own impact records and groups; not covered
  here.

### 10. How a sound is played

Every sound is one of:

- **Local** (2D, in your head, no distance effect): your own fire, reload,
  draw and dry-fire sounds, your jump voice, hit-confirm dings, interface
  sounds, and your own footsteps (played "client local").
- **3D at a point** (attached to nothing; a moving source keeps its start
  position unless it is a player sound that follows its player): every
  other gameplay sound. Map ambient sounds may instead be "ambient": they
  use the same distance rule but have no direction (always in front of
  you at their distance).

**Radii.** Each 3D sound has an outer radius R and an inner radius r. When
the caller gives no inner radius, r = 0.07·R. Sounds from a named sound set
(`SOUNDBUTES.TXT`) take the set's radii when the caller gives none, and pick
one of the set's files by weight. Server sounds reach clients with R
truncated to whole units and r quantised to ⌊255·r/R⌋/255 of R (7 % arrives
as 6.67 %).

**Gain from distance** (game side, recomputed only when the listener or the
source moved, with d the listener-to-source distance):

```math
g(d) = \operatorname{clamp}\!\left(1 - \frac{d^2 - r^2}{(R - r)^2},\ 0,\ 1\right)\cdot\frac{V}{100}\quad\text{for } r < d < R
```

- For d ≤ r the gain keeps its previous value (1 for a sound that started
  there). The gain reaches 0 at d₀ = √(r² + (R − r)²) ≈ 0.9326·R for
  r = 0.07·R, so the last 7 % of the radius is silent. Beyond R the audio
  library mutes the sound (Open questions 1).
- V is the sound's volume (100 unless stated). The player's master and
  per-class sliders (weapons, speech, default) multiply on top.
- No pitch variation is audible: a random pitch stored in a sound file and
  any requested pitch only change the engine's timer for the sound, never
  its playback (Quirk 10).

**Distance low-pass**, also only for r < d < R:

```math
u = \operatorname{clamp}\!\left(1 - \frac{d - 1200}{600},\,0,\,1\right),\qquad c = \operatorname{clamp}\!\left(1 - \cos\frac{\pi u}{2},\ 0.1,\ 1\right)
```

c is the filter cut-off as a fraction of full bandwidth: no filtering up to
1200 u, heavy muffling from about 1650 u on. This is the only "far" version
of any sound.

**Occlusion**: none. Walls and floors do not lower or muffle anything (the
engine's occlusion and obstruction calls are stubs).

**How many at once**: at most 255 live sounds and 255 mixer voices. The
priority values the game passes (misc 0–2, AI 2–4, player 4–6) are stored
but never used to pick: when every voice is taken a new sound plays
silently and when 255 sounds are live a new one is refused. Gameplay never
gets close; the per-source limits below are what drop sounds in practice:

- a new unsilenced fire sound from a shooter stops that shooter's previous
  fire sound of the same weapon;
- each grenade has one bounce sound; a new bounce stops the old one;
- server hurt and death sounds: one at a time **per kind, for the whole
  server** (section 11);
- team call-outs: one at a time per kind on each client.

### 11. Sounds by event

| Event | Sound | Played as | Radius R (heard to ≈ 0.93 R) |
|---|---|---|---|
| Own shot | `FireSnd` (silenced: `SilencedFireSnd`ℓ) | local, volume 100 | — |
| Other's shot | same file | 3D at their flash socket, volume 100 | `FireSndRadius` (silenced: `SilencedFireRadius`ℓ) |
| Dry fire | `DryFireSnd` | local; others 3D at your flash point | `FireSndRadius` (Quirk 13) |
| Reload / draw keys | view-model `sound_key n`: 1–3 → `ReloadSnd1..3`, 4 → `SelectSnd`, 5 → `DeselectSnd`, 10–14 → misc | local; others 3D at your flash point | `ReloadSndRadius` / `SelectSndRadius` (300) |
| Bullet impact | surface list by ammo kind (below) | 3D at the impact, volume 100 (50 under water) | ImpactFX `SoundRadius` (900; melee 153) |
| Grenade bounce | surface `GrenadeImpactSnd` | server 3D, volume per `grenades.md` | `GrenadeSndRadius` (1000) |
| Grenade detonation | ImpactFX `Sound` (`gren.wav`, `flash.wav`, `Smoke_Exp_GAS.wav`) | 3D at the blast | 1500 |
| Casing | surface `ShellImpactSnd1` | 3D, first bounce | `ShellSndRadius` (200) |
| Footstep | surface `RtFootSnd`/`LtFootSnd` 1 or 2 | yours local, others' 3D | 1200 |
| Fly-by | sound set `bulletflyby` (`flyby.wav`) | 3D (Quirk 6) | set: r 100, R 300 |
| Hurt | auto-cast sound by damage kind and by hit zone (below) | server 3D at the victim | 500 |
| Hit confirm | `MultiDing1.wav` or `MultiDing2.wav` (50/50) | local | — |
| Jump | your voice from the auto-cast `Jump` list (`jump1.wav`/`jump2.wav`), you only | local | — |
| Kill call-outs | team auto-cast sounds (`EnemyDown` …) | 3D, **own team only** | 500–1000 |
| Map sounds | placed `SoundFX` objects | server 3D or ambient | object's radii |

Details:

- **Fire sound choice**: a muzzle attachment of type silencer or "muzzle
  attachment" with `SilencerLevel` ℓ in 1..9 selects the ℓ-th silenced file
  and radius; level 0 plays nothing. Otherwise `FireSnd`. There is no
  separate near/far or first/third-person file (the code for those is
  disabled); far shots differ only by gain and low-pass. Stock radii: M4A1
  2000 (silenced 1400), AK-47 2000 (1600), MP5A4 1800 (1200), MK.48 2200,
  SPAS-12 1600, M24 and L96A1 3000 (1500), M9 knife 300. Your own fire
  sound plays even when the shot does not fire on the server.
- **Impact sound choice** when the ImpactFX says `surface`: a surface
  custom list named by the ammo (bow ammo) if the surface has one; else for
  ray ammo by damage kind: melee and sword → `MeleeImpactSnd`, infection →
  `HandsImpactSnd`, fist → `FistImpactSnd`, anything else →
  `BulletImpactSnd`; for thrown and launched ammo: burn → none, crossbow →
  `BowImpactSnd`, otherwise `ProjectileImpactSnd`. The slot is uniform among
  **all four** slots 1..4, so a surface that defines fewer than four plays
  nothing on the empty draws: Unknown (3 defined) is silent 25 % of the
  time, Snow and Linoleum (2) 50 %, melee lists (usually 2) 50 %.
- **Footsteps**:
  - Others: each `FOOTSTEP_KEY` in a third-person animation (`animation.md`)
    plays one. Stock male keys: running `LRF`/`LRB`/`LRL`/`LRR` and
    diagonals two per 600 ms loop (e.g. 50 and 350 ms); sprinting `LRR*`
    two per 500 ms; jumps `LJ*` one at 600 ms (the landing). Walking `LW*`
    and crouching `LC*` carry only footprint keys: **silent**. Cadence
    scales with the animation rate.
  - You (first person): the view sway phase φ advances by 2π·1.4·Δ while
    the run flag is set (2π·0.6 otherwise) whenever you move (speed·Δ >
    0.1 u with a movement key held) and can step (on the ground, on a
    ladder, or within 150 u above ground and not jumping); a step plays
    when φ enters (π/2, 3π/2] (right foot) and again when it passes 3π/2
    (left foot): 2.8 steps/s running, about 0.357 s apart. Starting to
    move, jumping and landing each add one immediate step.
  - Your own steps are silent while the run flag is off (walk key held or
    crouched) and in spectator mode; dead players make none.
  - Surface: the surface under the foot (a ray from 50 u above the foot node
    to 50 u below, world and world models only); for you, the surface you
    stand on. Ladders use the ladder volume's override surface; an unknown
    surface inside a water volume counts as Water; invisible surfaces are
    silent. File: `RtFootSnd` or `LtFootSnd` for the foot, slot 1 or 2 at
    random. Special characters (robots, VIP, bosses) use their custom list.
  - Volume 100 × the character's stealth value (1.0), then + the model's
    `FootprintSndVolume` share (−0.5 for the 65 ghillie-type models, so
    volume 50); perks can change volume and radius. Radius 1200.
- **Fly-by**: for another player's shot with bullet-type ray ammo, when
  your camera is not within 0.75 × the impact sound radius (675 u) of the
  impact, and the perpendicular distance from your camera to the **infinite
  line** of the shot is under 600 u (also behind the shooter or past the
  impact). See Quirk 6 for where it is played.
- **Hurt and death** (server): on every damage > 0 to a player, two sounds
  at the victim, radius 500, each only if no sound of the same kind is
  still playing anywhere on the server:
  - by damage type: bullet, melee, crossbow → `DeathbyGuns`
    (`Hurtpain_A_*`, 3 files per sex); explosion → `DeathbyExplosions`
    (`Hurtpain_B_*`); burn and bleed → `Burning`; poison → `CoughinGaszone`;
    falls → `FallingDMG`; freeze → `Frozen`;
  - by hit zone: head → `HitHead` (`HEADSHOT.WAV`), groin → `HitNut`
    (`NutPain.wav`), torso, arm, leg → empty (none).
  - The file set follows the victim's sound template (male, female,
    infected types, VIP). The killing hit is just another hurt; the
    dedicated death cry is never played in this build.
- **Hit confirm**: when your client receives the server's copy of your own
  hit on a character in a multiplayer game, a ding is queued (heal and
  stamina ammo play their own sounds instead). It plays on the next
  character update (Quirk 11), local, priority high. At most 5 can be
  pending; more in one frame are dropped.
- **Jumps of others**: the server sends a jump event whose sounds
  (`CharJumpSound1/2`) are not defined in the shipped client attribute
  data, and the server's own jump entry is empty, so a jump take-off is
  silent for others; the landing is a footstep key (above).
- **Kill call-outs**: on a kill the server sends the killer's team an
  `EnemyDown` call-out at the killer (radius 1000) and the victim's nearest
  teammate's team a `ManDown` call-out (empty in stock data); clients of
  the other team ignore them.
- **Map sounds**: a placed `SoundFX` object has `Sound`, `OuterRadius`
  (default 500), `InnerRadius` (100), `Volume` (100), `PitchShift`,
  `Ambient` (default on), `Loop` (on), `PlayAttached`, `StartOn` and
  `Priority`; it is a server sound. Messages
  ON, OFF, TOGGLE, TRIGGER, KILL control it.

### 12. Who hears what

- Client-built sounds (fire, impacts, casings, footsteps, explosions,
  fly-bys) are made by every client from events that reach every client, so
  everyone hears them by distance alone. Teams do not matter.
- Server sounds (hurt, grenade bounces, map sounds) are sent only to
  clients whose view position is within 2R of the sound.
- Team call-outs are dropped by clients of the other team.
- Nothing is occluded; positions are not checked against walls.
- Your own weapon sounds are never doubled: the server's echo of your shot
  and of your reload, draw and dry-fire sounds is ignored by your client.

## Per-frame order

Client, one frame, as far as effects and sounds go:

1. Incoming server events are processed as they arrive: each weapon-effect
   event runs the steps in "One weapon effect" at once; player-sound events
   (others' reload, draw, dry fire, jump, team call-outs) start their
   sounds; server sounds start.
2. Player update: movement (`movement.md`), view sway and your footsteps
   (section 11), weapon update (`weapon_firing.md`): on a fired round your
   own weapon effect is built at once and your fire sound starts; view-model
   animation keys (reload, draw, casing) fire.
3. Effect objects update: decals (fade), tracers (move), casings (move,
   bounce, sound), character effects (pending dings play, footstep keys of
   others' animations).
4. Effect groups update (particles, sprites, lights, camera jitter).
5. Render. Then the mixer updates the listener and every playing 3D sound's
   gain and low-pass.

One weapon effect (client), in order:

1. Visibility gates (section 2); liquid tests at impact + normal and at
   fire point + 1 u along the shot.
2. Fire effects (unless suppressed): muzzle flash if the fire point is
   seen; casing if seen and near; fire sound (not for your own shot).
3. If this is the server's echo of your own non-thrown shot: queue the
   ding and stop (critical hits continue).
4. If not sky: impact sound; ammo effects; if the impact is seen, decal
   then surface burst (and exit decal/debris, Edge cases).
5. Bubble trail if the fire point is in liquid (bullet ammo).
6. Tracer if its bit is set, `Tracers` is on and the impact is not in
   liquid.
7. Fly-by check (others' bullet shots).

## Edge cases

- **Surface with no type** (0): no decal, no burst, stone impact sounds
  with a 25 % chance of silence, tile footsteps, `Grenade_Dirt` explosion,
  silent grenade bounces (its `GrenadeImpactSnd` is empty), casings play
  `linoleum.wav`.
- **Surface id not in the table**: treated as Unknown (0).
- **Sky**: no impact sound, no effects, no decal (no stock ImpactFX has
  `ImpactOnSky`); tracers still fly to the ray's end. A grenade detonating
  on sky makes no sound at all.
- **Effect group name not in the effect file**: nothing is shown (the C and
  Red silencers' flashes).
- **Many impacts at once** (shotgun): every pellet is a full weapon effect
  (sound, decal, burst); only the first carries the fire effects and the
  tracer. Ten pellets on one spot add decals until 10 are within 10 u,
  then move existing ones. Ten impact sounds start at once (no limit).
- **Sound beyond range**: started anyway (the "play only if heard" option
  `SoundPlayOnlyIfHeard` is off); muted by the audio library while beyond R
  and recomputed when someone moves into range.
- **Sound starting inside the inner radius**: full gain; if the listener
  then moves out past r the formula takes over.
- **Shot from inside a wall**: an exit decal can appear only when the 2 u
  segment centred on the **fire position** crosses a shoot-through surface
  that shows decals, because the exit test is made at the muzzle, not at the
  impact. Exit debris and the light shaft through thin walls are off in all
  stock FireFX records.
- **Your own far shot**: a decal beyond 1000 u appears only while zoomed;
  the burst and sound still play.
- **Impact behind you by another player**: seen only within 300 u; a frag
  exploding behind you farther away shows no explosion even if you turn at
  once (its sound plays).
- **Mid-air frag**: sound only (Air has no grenade group). Mid-air smoke and
  flash: full look (their groups come from the ammo).
- **Grenade in water**: impact point + normal inside liquid → under-water
  records (none for grenades) and half-volume sound; on the water surface
  from above → `Grenade_Water`.
- **Paused**: decals, casings and effects stop ageing.
- **Dead or spectating**: no own footsteps; others' footsteps still play;
  your muzzle flash is hidden while dead unless you chase-cam another
  player, whose silencer then decides your flash and fire sound.

## Quirks

1. **Tracers ignore depth**: drawn over everything, so a tracer stream
   shows through walls from the muzzle to the impact. Keep.
2. **Silent walkers**: others' walking and crouching animations have no
   footstep keys, and your own steps stop while the run flag is off. Walk
   or crouch to move silently. Keep.
3. **Footsteps and shots through walls**: no occlusion; the only falloff is
   distance (footsteps fade out at ≈ 1119 u, an M4A1 at ≈ 1865 u). Keep.
4. **Muzzle and explosion lights give positions away**: every unsilenced
   (and A/B silenced) shot lights the world within 170 u for 50 ms at
   medium/high detail, and engine lights have no shadows, so they also
   light the near side of a wall the shooter stands behind. Keep.
5. **Squared-distance gain**: gain falls with d², and reaches 0 at about
   93 % of the radius. Keep (it sets how far things are audible).
6. **Fly-by played near the world origin**: the fly-by position is computed
   as the camera position **normalised to length 1** plus the vector from
   the camera to the nearest point of the shot line. It therefore lands
   within about 600 u of the world origin, not near you, and with R = 300
   is effectively inaudible except near the origin. Recommendation: play it
   at the nearest point of the line; offer the original as an option
   (Open questions 2).
7. **Off-screen particle systems freeze**: a particle system that was drawn
   once and is not drawn in a frame skips its update, so its particles stop
   moving and ageing until seen again. A smoke cloud you are not looking at
   does not age. Keep, but check (Open questions 5).
8. **Muzzle flash length**: always 50 ms; automatic fire flickers. Keep.
9. **Detail value 3**: the weapon-effect code's detail value is 3 in
   release builds, so its "low" sound thinning (half or a third of impact
   sounds) and "medium" bubble count never apply, and blast lights (high
   only) never appear. Keep the "3" behaviour.
10. **No pitch variation**: pitch values never reach the mixer. Keep.
11. **Hit-confirm delay inverted**: the ding is meant to wait `DingDelay`
    (1 s) but the check fires while its time is still in the future, so it
    plays on the next character update. Keep.
12. **Quiet movement factor unused**: the 0.65 "moving quietly" factor
    applies only to your own steps while walking or crouched, which are
    already silent. Omit.
13. **Dry fire heard far**: others hear your empty-gun click with the
    weapon's **fire** sound radius (2000 u on an M4A1). Keep.
14. **Server-wide hurt limit**: one hurt sound per kind across the whole
    server; while one player's `DeathbyGuns` plays, nobody else's does.
    Keep.
15. **Casings ride along**: first-person casings are carried with you while
    they fall. Keep.
16. **Empty sound slots**: random slot choice includes undefined slots
    (impact sounds silent 25–50 % of the time on some surfaces). Keep.

## Test cases

Distances are listener (camera) to source unless stated. "Gain" is g(d) of
section 10 with V = 100 and default sliders. Detail: effect detail high.

1. **Gain, M4A1 shot by another player**, R 2000, r 140: d = 100 → 1.0;
   500 → 0.9334; 1000 → 0.7166; 1500 → 0.3553; 1800 → 0.0691; 1900 → 0.
   Zero point 1865.26 u.
2. **Gain, silenced M4A1** (level 1, `SILENCED1.wav`), R 1400, r 98:
   d = 500 → 0.8582; 1000 → 0.4158; 1300 → 0.0087; zero at 1305.68.
3. **Gain, footstep** of a running enemy on Metal, R 1200, r 84: file
   `metal1a.wav` or `metal2a.wav` (right foot) at 50/50; d = 500 → 0.8049;
   1000 → 0.2027; 1150 → 0; through a wall the same values.
4. **Gain, ghillie footstep**: same as 3 with V = 50 → d = 500 → 0.4025.
5. **Gain, server hurt sound**, R 500, r = 17/255·500 = 33.33: d = 100 →
   0.9592; 300 → 0.5918; 460 → 0.0335; a client 1001 u away receives
   nothing.
6. **Gain, bullet impact** on Metal (`metal_bg\impact1..4.wav`, one of four
   uniformly), R 900, r 63: d = 200 → 0.9486; 500 → 0.6488; 800 → 0.0921.
7. **Gain, frag detonation** `gren.wav`, R 1500, r 105: d = 500 → 0.8772;
   1000 → 0.4918; 1400 → 0.
8. **Low-pass**: d = 1000 → c = 1; 1200 → 1; 1350 → 0.6173;
   1500 → 0.2929; 1650 → 0.1; 1800 → 0.1 (only while d < R).
9. **Impact silence**: 10 000 bullet impacts on surface 0 → about 2500
   without sound; on Snow (71) about 5000; on Metal (20) none.
10. **M4A1 tracers, own view**: no attachments, 8 shots → tracers on shots
    4 and 8 (AR02: 15 u wide, 600 u long, 6000 u/s). AK-47 (AR01): shots 3,
    6, 9. M24 (SR01, F = 1): every shot.
11. **Tracer timing**: M4A1 shot hitting 1500 u away → tracer life 0.25 s;
    at t = 0.1 s its tail is 600 u and its head 1200 u from the muzzle; at
    t = 0.2 s tail 1200, head clamped at 1500. A shot hitting 90 u away →
    no tracer.
12. **Tracer with a dot sight**: M4A1 with any sight (sights name AR01,
    F 3) and no muzzle attachment → own client draws tracers on shots 3, 6,
    9 with the **AR02** look; other players see the server's cadence
    (F 4: shots 4, 8) with the AR02 look.
13. **Tracer with A silencer** on M4A1: own cadence F 3 (shots 3, 6, 9) and
    AR01 look (red, 25 u, 300 u); others: shots 4, 8 with the AR01 look.
14. **Pistol with silencer**: Colt 1911 with `Silencer_A_Pistol` (Pisol01,
    F 0) → no tracer on any shot (ammo has none).
15. **Decal size**: M4A1 on Metal (20) → `im_metal`, scale 0.075 ×
    U(0.6, 0.7), width 2.88–3.36 u; on Wood (30) 3.84–4.32 u; on Cement
    (11) 2.88–3.84 u; on Glass (40) 2.88–3.36 u and no burst.
16. **Decal life**: a decal at t = 2.9 s has alpha 1; t = 4.5 s → 0.5;
    t = 6.01 s → removed.
17. **Decal crowding**: 10 decals within 10 u of a point, shoot that point
    → 11 decals. One more shot → still 11, and the closest decal has moved
    to the new point with its old age.
18. **Decal cap**: 101 decals alive, a 102nd anywhere → 101, the oldest
    gone.
19. **No decal on doors client side**: shooting a Wood Door (100) → burst
    `Impact_Wood` and wood sounds, no client decal (`ShowsMark` 0).
20. **No decal on characters**: torso hit → `Impact_Body`, cloth impact
    sound (radius 900), no decal; expected blood stains on the wall behind
    1.0 (2 blobs × 50 %), each 32 u wide, gone after 3.0 s.
21. **Head hit**: `Impact_Head`; light radius rises to 300 u in 0.1 s
    (medium/high); expected stains 4.3.
22. **Muzzle flash, others**: an enemy fires an M4A1 at you from 500 u
    straight ahead → `M4_HH_Muzz` for 50 ms with a 170 u light (none at low
    effect detail). The same enemy 500 u **behind** you → no flash, but the
    fire sound at gain 0.9334.
23. **Silencer flash**: enemy with `Silencer_C_AR` fires → no flash, no
    light; with `Silencer_A_AR` → `WEAPON_HH` with light. Both sound
    `SILENCED1.wav`, R 1400.
24. **Own muzzle**: firing the M4A1 → `AK47_PV_Muzz` visible 50 ms after
    each shot plus the `test` smoke puff (0.5–0.8 s) and a 256 → 170 u light.
25. **Casing**: M4A1 third-person shot seen from 300 u → a 5.56 casing from
    the `Breach` socket, initial speed components up 30–75, right 50–75,
    forward 20–50 u/s; it grows to 2× in 0.5 s, plays `metal.wav`
    (R 200) at its first bounce if that is on Metal and nothing at later
    bounces, and is removed at 3.0 s. The
    same shot seen from 1100 u → no casing.
26. **Casing rest**: at 60 fps a casing reaching a floor at 100 u/s rests
    at once; at 144 fps (threshold 180 u/s) also; at 30 fps (82.2 u/s) it
    bounces (at most 3 times).
27. **Fly-by condition**: another player's rifle shot whose line passes
    400 u from your camera with its impact 2000 u from you → fly-by
    triggered (played per Quirk 6); impact 600 u from you → no fly-by.
28. **Hit confirm**: your M4A1 hits an enemy in a multiplayer match → one
    `MultiDing1.wav` or `MultiDing2.wav` on the frame after the server's
    echo arrives. Eight shotgun pellets confirmed in one frame → 5 dings.
29. **Hurt limit**: two players are shot in the same server frame → only
    the first plays `Hurtpain_A_*`; a head hit also plays `HEADSHOT.WAV`
    (separate kind).
30. **Others' footsteps by stance**: an enemy running forward at animation
    rate 1 → steps at 50 and 350 ms of each 600 ms loop (3.33 steps/s);
    the same enemy walking with the walk key or crouching → no step sound.
31. **Own footsteps**: running in first person from rest → one step at
    once, then steps every 0.357 s; holding the walk key → none.
32. **Frag on stone, 500 u away, seen**: `Grenade_Stone`; camera jitter
    amplitude 60 × 0.65 = 39 u at its start, 0 at 800 u; light radius
    peaks at 256 u 0.199 s after detonation. The same frag in mid-air →
    `gren.wav` only.
33. **Frag behind you, 500 u**: `gren.wav` at gain 0.8772, no explosion
    look.
34. **Smoke cloud** on a floor: 10 particles at the first frame; each
    starts within 10 u; horizontal travel ≤ 256 u per axis; size 200 → 300
    over 8–10 s; opaque from 5.1 % of its life; all gone 10 s after
    detonation. No second emission at any time.
35. **Flash look**: `Flashbang_Ini` light radius 512 u at 0.127 s, 0 at
    0.28 s; sprites hidden when a wall is between them and you.
36. **Dry fire**: an enemy pulls the trigger of an empty M4A1 1500 u away
    → `dryfire.wav` at gain 0.3553.
37. **Reload keys**: M4A1 reload → `reload1.wav` at 403 ms, `reload2.wav`
    at 834 ms, `reload3.wav` at 1725 ms; others hear them with R 300 (to
    ≈ 280 u). Drawing the M4A1 plays `reload3.wav` at 666 ms (its select
    animation's key 3).
38. **Grenade bounce on unknown surface**: silent; on Cement →
    `stone\impactgrenade.wav`, R 1000.
39. **Melee impact**: knife on Wood → one of `melee\wood\impact1..2.wav`,
    R 153, silent in half of the hits.

## Open questions

1. **Beyond the outer radius**: the game's gain is not recomputed beyond R,
   and the audio library's own distance roll-off is set to 0. We assume the
   library mutes samples beyond their maximum distance (R + 0.01). If it
   does not, sounds started out of range play at full gain. Test: stand
   2100 u from a firing M4A1 in an open map.
2. **Fly-by position** (Quirk 6): confirm the fly-by is inaudible away from
   the map origin. Test on a map with the origin far from the play area.
3. **Particle and sprite size convention**: we read a particle of size s as
   a square of side 4s, and a sprite at scale k as texture pixels × k
   units wide. Both affect every number given as a size here (a bullet hole
   2.9–3.8 u, a smoke particle 800–1200 u). Check against a screenshot:
   a bullet hole beside a known 64 u crate.
4. **Light through walls**: engine dynamic lights are assumed unshadowed.
   Check a muzzle flash behind a thin wall at medium detail.
5. **Frozen off-screen smoke** (Quirk 7): confirm that a smoke cloud lasts
   longer when you look away (look away for 10 s after detonation, turn
   back).
6. **Server-side surface type**: others' impacts use the server's surface
   type and normal; whether the server re-derives the character surface
   (head/body) for client-reported hits or sends what the shooter reported
   was not traced.
7. **Hit confirm on teammates**: whether the server sends the hit event (and
   so a ding) for hits on teammates with friendly fire off.
8. **Others' footstep keys while unseen**: footsteps of characters you
   cannot see depend on their animation being advanced on your client; not
   traced.
9. **Server decal times on movers**: the server's solid-time setter writes
   the fade time instead, so the solid time keeps its initial value (not
   traced). Check how long a decal on a door lasts.
10. **Silenced C/Red flashes**: confirm in the Steam build's effect file
    that `Silenced01_PV_Muzz` and `Silenced01_HH_Muzz` are still missing.
11. **Frame order**: the exact place where incoming events are handled in
    the client frame (before or after the player update) was not traced.
12. **Contradiction with `grenades.md`, section 11 (smoke)**: it says the
    particle system "runs 2.0 s", has "full bounce off the world" and that
    the cloud "lasts at most about 12 s". From the effect code: one burst at
    the first frame, bounce keeps 75 % of speed, and the cloud ends at most
    10 s after detonation (barring Quirk 7). The velocity box is in the
    effect's frame (local +Z along the detonation normal) and the friction
    is per second (×0.02 per s). This answers its open question 10; that
    spec should be updated by its owner.
13. **Contradiction with `attachments.md`, "Muzzle flash"**: it says
    shipped silencers name ordinary effects and still flash in first
    person. True for grades A and B; grade C and all Red silencers name
    effects that do not exist, so they show nothing. Its "Sound" section
    also says the muzzle light of a silenced shot is not shown to others;
    the routine that would hide it is never called, the light is part of the
    flash group, so A/B silencers show their light and C/Red show neither.
14. **Contradiction with `attachments.md`, "Tracer"**: the last fitted
    attachment's tracer decides only the **cadence** on the shooter's own
    screen; the look comes from the muzzle-slot attachment (else the ammo),
    nothing is drawn when the ammo has no tracer, and other players see
    the server's cadence from the ammo alone (section 4).
15. **Contradiction with `weapon_firing.md`, penetration table**: it lists
    the shoot-through surfaces and says all others stop the ray. The
    surface table also marks Unknown (0: limit 10, re-perturb 10, damage
    × 0.6), trussing (23: limit 10, damage × 1.0), Invisible metal (206:
    always, damage × 0.9), Air (1) and Boss3_weak (65) as shoot-through.
    Unknown matters: untyped world polygons would be penetrable up to 10 u.
    Check which build's table the live game uses.
16. **Build differences**: data from the US client; effect files identical
    in the US 1501.04 and BR 1810.04 archives for every group named here
    except the grenade groups' camera jitter (BR `Grenade_Dirt`: 20 u at all
    details instead of 60 u at high detail; small end-time differences).
    Re-check against the Steam build's archives when the readers work.
