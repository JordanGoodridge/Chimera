# Combat Arms: grenades and other thrown weapons

Source basis: the reference material the user pointed this session to. That
covers the server's projectile, bouncing-grenade, explosion and damage
handling (including friendly fire, armour gear and the flash timer), the
shared engine's object motion integration and point collision, the server's
container (water) physics, the client's weapon fire path and its full-screen
flash effect, the shared third-person animation state that the server uses
to know whether a grenade is in the hand, the weapon/ammo/projectile/impact
effect/surface/gear tables of the US client's attribute data, the shipped
dedicated-server configuration, and the smoke effect definition in the BR
client's effect archive (read as text). Animation lengths are taken from
`inventory.md`. A follow-up session added the client's view-model fire
state machine, the server frame loop and object list, the engine's
object-to-object and world-model collision, the volume classes, the
explosion cover traces, the gear table, the round logic, the client's
flash drawing order, and the three grenades' view-model animation files
and world models and the older regional client's maps (read with
throwaway readers, numbers only). Nothing was checked against a running
game; see Open questions.

Status: draft

Sections: Summary; Units and conventions; Constants; Data: one grenade of
each kind; Behavior (1 kinds, 2 carrying and starting counts, 3 pin pull and
"cooking", 4 the throw, 5 flight, 6 bouncing and coming to rest, 7 water,
8 detonation, 9 blast damage, 10 flash, 11 smoke, 12 fire and gas, 13 the
grenade as a target, 14 death release and the drop perk, 15 the end of a
round); Per-tick order;
Edge cases; Quirks; Corrections (2026-10-19); Corrections (2026-10-22);
Test cases; Open questions.

---

## Summary

- A thrown grenade is a server object with a **fixed fuse that starts when
  it leaves the hand**. There is no cooking: holding the button does not
  shorten the fuse. Holding fire after the 700 ms pin pull keeps the
  grenade in hand (the short throw animation loops); the grenade leaves
  133 ms into the post-throw animation that follows the button release,
  at the earliest 900 ms after the pin pull starts. Frag 2.5 s, flash
  1.5 s, smoke 3.0 s, all from data.
- It leaves the camera at 1550 u/s along the aim (plus a tiny spread). The
  thrower's own velocity is **not** added and there is no secondary throw.
  It falls under the engine's object gravity of 1600 u/s² (not the
  player's 1100) and is integrated with a quirk that moves it only a
  quarter of the textbook gravity drop each tick.
- It is a point. On each surface hit its velocity is mirrored and scaled by
  the surface's hardness (0.65 on almost everything). It stops dead the
  first time it touches a floor (normal y > 0.75) with a horizontal speed
  under 50 u/s. There is no rolling friction: "rolling" is a run of small
  bounces.
- A frag does 60 damage, full within 112.5 u and falling linearly to 0 at
  450 u. Walls block it (two traces, a second try to the head). Because both
  a player and that player's hitbox are inside the blast, **a player takes
  the blast twice** (up to 120). Self damage is full; teammates take full
  blast damage with friendly fire on and none with it off. Stock body armour
  does not reduce explosions.
- A flash blinds every player within 1000 u who has line of sight: up to
  25 s of "flash time" facing it, a quarter of that facing away. The screen
  turns white and fades as cos(π t / T), so it is gone after half the flash
  time. There is no deafening in this build.
- Smoke is only a client-side particle cloud. In game logic it blocks
  nothing: not bullets, not bots, not name tags.

## Units and conventions

- Distance: LithTech world units ("u"), the same units as the other Combat
  Arms specs. +Y is up, the frame is left-handed (+X right, +Z forward).
- Velocity u/s, acceleration u/s², angles radians unless stated.
- Time: seconds. The grenade, the explosion and all damage run on the
  **server**, once per server frame. The shipped dedicated-server
  configuration locks the server to **20 frames per second (dt = 0.05 s)**.
  All server timings below therefore resolve to 50 ms steps. Formulas are
  written for a general dt; the test cases use dt = 0.05.
- The throw itself (animations, the moment of release, the aim) is decided
  on the **client**, per rendered frame, exactly as in `inventory.md` and
  `weapon_firing.md`. The flash screen effect also runs on the client, on
  real time.
- Field names in `code font` are keys of the attribute data or console
  variables a server operator can set; weapon and ammo names ("M67",
  "XM84") are data names.

## Constants

Fixed in code (not in the attribute data):

| Name (ours) | Value | Unit | Meaning |
|---|---|---|---|
| object_gravity | (0, −1600, 0) | u/s² | Engine gravity for every non-player object, grenades included |
| gravity_integration_factor | ¼ | — | Displacement per step uses ¼·g·dt², not ½ (Quirk 1) |
| slow_stop | 0.1 | (u/s)² | An object whose \|v\|² is below this has its velocity zeroed before the step |
| surface_push | 0.05 | u/s | Extra speed pushed into a surface by the engine's collision response, per contact. The grenade bounce doubles it, so the bounce formula of section 6 has 0.1 (Open question 18) |
| grenade_half_height | M67 6, XM84 10, M18 10 | u | Half-height of the grenade's box: the y dimension of the first animation (`Hand`) of the projectile's world model, × `ModelScale` 2 (Open question 9) |
| grenade_box | M67 (6, 6, 8), XM84 (4, 10, 10), M18 (6, 10, 10) | u | Half-extents of the grenade's box, from the same animation × 2. Used for touches by other moving grenades and for bullet hits (sections 6, 13) |
| release_key | 133 | ms | Time of the release event inside the 366 ms post-throw animation, at fire rate 1 (section 3) |
| throw_loop | 67 | ms | Length of the throw animation, which repeats while fire is held (section 3) |
| default_dampen | 0.35 | fraction | Console variable `GrenadeDampenPercent`: speed lost per bounce when the hit surface has no table entry (so × 0.65) |
| rest_speed | 50 | u/s | Console variable `GrenadeMinVelMag`: horizontal speed below which a floor hit stops the grenade |
| rest_floor_normal | 0.75 | — | A surface counts as floor when its normal's y > this (about 41° slope or flatter) |
| max_bounces | 0 (off) | bounces | Debug option; when ≥ 1 the grenade detonates after that many bounces. Off in shipped builds |
| spin_pitch | uniform [−π, π] | rad/s | Visual spin, re-rolled at every bounce |
| spin_yaw, spin_roll | uniform [−2π, 2π] | rad/s | Same |
| blast_full_fraction | 0.25 | × radius | Blast and flash are at full strength inside this fraction of the radius |
| big_object_slack | 100 | u | For objects whose dims vector is longer than this, the excess is subtracted from their blast distance (every object, players and hit boxes included; their stock dims are far below 100, Corrections 2) |
| flash_delay | 0.25 | s | Console variable `FlashBangDelay`: time from blast to blindness, victim facing the blast |
| flash_delay_back | flash_delay / 3 | s | Same, victim facing away (0.0833 s) |
| flash_back_factor | ¼ | × | Flash time when facing away |
| flash_min_time | 0.1 | s | Shorter flashes are not sent to the client |
| white_decl_default | 2.0 | — | Weapon field `WhiteScreenDecl` default (no shipped weapon sets it) |
| captured_decl_default | 1.0 | — | Weapon field `CapturedDecl` default (no shipped weapon sets it) |
| death_release_speed | 500 | u/s | Speed of the live grenade released when its holder dies |
| liquid_gravity | −200 | u/s² | Gravity inside a liquid volume. Fixed in code; no map property changes it (Open question 19) |
| liquid_viscosity_scale | 5000 | u/s² | Speed loss per unit of a volume's viscosity |
| viscosity_min_speed | 1 | u/s | Viscosity and current act only when the speed is above this |
| destroyed_at | ≤ 1 | HP | A grenade (like any damageable object) is destroyed when its HP drops to 1 or less after positive damage |

## Data: one grenade of each kind

From the attribute data (US client). "Weapon" fields are the weapon record,
"ammo" fields the ammo record it links to, "proj." the projectile-effect
record that ammo names, "impact" the impact-effect record.

| | M67 (frag) | XM84 (flash) | M18 (smoke) | M-23F (incendiary) | G7A (nerve gas) |
|---|---|---|---|---|---|
| Weapon `Guntype` | 7 (thrown) | 7 | 7 | 7 | 7 |
| Weapon `ShotsPerClip` | 1 | 1 | 1 | 1 | 1 |
| Weapon `MinPerturb` / `MaxPerturb` | 1 / 10 | 1 / 1 | 1 / 1 | — | — |
| Weapon `DynamicPerturbFactor` | 1.0 | 0.0 | 0.0 | — | — |
| Weapon `MovePenalty` | 0.03 | 0.03 | 0.03 | — | — |
| Weapon `HideWhenEmpty` | 1 | 1 | 1 | — | — |
| Weapon `GrenadeSpeed`, `GrenadeAttach` | 0, unset | 0, unset | 0, unset | unset, 0 | unset, 0 |
| Ammo `Type` | 2 (grenade) | 1 (projectile) | 1 (projectile) | 1 | 1 |
| Ammo `SpawnedAmount` / `MaxAmount` | 1 / 1 | 2 / 2 | 2 / 2 | 1 / 2 | 1 / 2 |
| Ammo `AreaDamage` | 60 | 25 | 1 | 0 | 0 |
| Ammo `AreaDamageRadius` | 450 | 1000 | 0 | 0 | 0 |
| Ammo `AreaDamageType` | 7 (explode) | 34 (flash) | 0 | 0 | 0 |
| Ammo `ProgDamage` (HP/s) | 0 | 0 | 0 | 5 | 6 |
| Ammo `ProgDamageType` | — | — | — | 3 (burn) | 9 (poison) |
| Ammo `ProgDamageRadius` | — | — | — | 350 | 350 |
| Ammo `ProgDamageLifetime` (s) | — | — | — | 9 | 9 |
| Ammo `ProgDamageDuration` (s) | — | — | — | 10 | 2.5 |
| Ammo `Health` (HP of the grenade) | 300 | 50 | 50 | 50 | 50 |
| Ammo `DamageMovePenalty`, `...Duration` | 135, 0.4 s | — | — | — | — |
| Proj. `Lifetime` (fuse, s) | 2.5 | 1.5 | 3 | 3 | 3 |
| Proj. `Velocity` / `AltVelocity` (u/s) | 1550 / 1550 | 1550 / 1550 | 1550 / 1550 | 1550 / 1550 | 1550 / 1550 |
| Proj. `DropVelocity` (u/s) | 500 | — | — | — | — |
| Proj. `Gravity` | 1 | 1 | 1 | 1 | 1 |
| Proj. `GravityOverride` | −1000 (ignored, see 5) | −1000 | −1000 | — | — |
| Proj. `CanHitSameKind` | 1 | 0 | 0 | — | — |
| Proj. `MaxRicochetAngle`, `MaxRicochets` | −1, unset | 1°, unset | unset | — | — |
| Proj. `ModelScale` | 2 | 2 | 2 | — | — |
| Proj. class | bouncing | bouncing | bouncing | bouncing | bouncing |
| Impact effect | handgrenade | flashbang_exp | Smoke_Grenade | Incendiary | Nerve |

Notes on the data:

- Every other thrown weapon in the table is a re-skin of one of these
  (event frags with `AreaDamage` 60–135 and radius 450 or 540, more smoke
  grenades, mode-only infected and ice grenades). All use the bouncing
  class except one mode item that uses the mine class (`mines.md`, not this
  spec).
- `GrenadeSpeed`, `GrenadeAttach`, `GrenadeSlide`, `Range`, `Effectrange*`,
  the ammo's `Rangedamage*`, `AltVelocity` (for players), `GravityOverride`
  (for thrown weapons) and the weapon's `DamageRFacter` are **read but have
  no effect on thrown grenades**. There is no sticky grenade and no impact
  grenade in the data. The code has an impact variant (detonates on its
  first touch, as a rocket does) that no shipped record uses.
- Values fixed in code: everything in Constants. Values from data:
  everything in the table above, plus surface hardness and gear protection
  (sections 6, 9, 10).

## Behavior

### 1. Kinds

| Kind | What detonation does |
|---|---|
| Frag | One instant blast: `AreaDamage` with linear falloff over `AreaDamageRadius` (section 9) |
| Flash | One instant flash test over `AreaDamageRadius`; no hit-point damage (section 10) |
| Smoke | Plays the smoke effect on clients; no damage (`AreaDamageRadius` 0 disables it) (section 11) |
| Incendiary / gas | Leaves an invisible damage sphere for `ProgDamageLifetime` that keeps applying damage over time (section 12) |

All five fly, bounce and time out identically (sections 4–8); only the
numbers differ.

### 2. Carrying limits and starting counts

- The loadout holds one throw-class item (`inventory.md`, section 1). It
  spawns with total = `SpawnedAmount`: M67 1, XM84 2, M18 2. `MaxAmount`
  caps the total for pickups (`inventory.md`, section 11).
- `ShotsPerClip` is 1, so the clip is 1 while total ≥ 1. A throw uses one
  from both clip and total.
- With total 0 the grenade is hide-when-empty: its slot key does nothing,
  and the auto-switch of `inventory.md` section 10 fires when the throw's
  post-fire animation ends (section 3).

### 3. Pin pull and "cooking"

There is no cooking in this build.

1. Fire (left mouse) starts the throw. It needs clip > 0 and the grenade
   not still drawing (400 ms, `inventory.md`). Fire is ignored while
   sprinting. Right mouse does nothing: grenades are not alt-fire weapons,
   so there is no underhand or short throw.
2. The first weapon update after the press sets the state to firing. The
   next one starts the **pre-fire** (pin pull) animation: 700 ms for the M67
   at the fire animation rate (1.0 stock; the 2.0 cap of `inventory.md`
   applies).
3. When the pre-fire ends, the main **throw** animation (67 ms) starts,
   **whether or not the button is still held**. Releasing early does not
   cancel the throw. At its first key (time 0) the client tells the server
   the throw animation has started (this drives the third-person throw);
   nothing is released yet.
4. **Holding.** Each time the throw animation ends, the weapon update
   looks at the fire button. If it is held (and the grenade total is
   above 0), the 67 ms throw animation starts again from 0; the grenade
   stays in hand for as long as fire is held. Each restart happens on the
   first client frame after the animation ended, so a loop lasts 67 ms
   rounded up to whole frames. If the button is up when the throw
   animation ends, the 366 ms **post-fire** animation starts.
5. The **release** is the fire event of the post-fire animation, 133 ms
   into it (`release_key`). At that moment the client builds the throw
   (section 4), uses the ammo and sends the fire message. The post-fire
   then plays to its end (233 ms more).
6. When the post-fire ends the weapon goes idle. If the total is now 0
   the auto-switch follows. Otherwise the clip (0) is refilled by the
   grenade's **reload** animation: 400 ms at the reload animation rate
   (1.0 stock), started on the frame the post-fire ends; the clip is 1
   again when it ends. This is not the draw animation, though both are
   400 ms. Pressing fire during it queues the next throw, which starts
   when it ends.

All timings above are for the M67, XM84 and M18 alike: their view-model
animation files have the same lengths and key times (pre-fire 700 ms,
throw 67 ms, post-fire 366 ms with the release at 133 ms, draw 400 ms,
reload 400 ms; only the pre-fire's sound key differs, at 513, 497 and
507 ms). All are divided by the fire animation rate ρ_fire (1.0 stock) and
the reload by ρ_reload (1.0 stock), as in `inventory.md`.

Shortest cycle at rate 1, from the first weapon update of the pre-fire:
release at 700 + 67 + 133 = 900 ms, post-fire ends at 1133 ms, second
grenade ready at 1533 ms, second release at the earliest 1533 + (two
weapon updates) + 900 ms.

The fuse is the projectile's `Lifetime` and starts on the server when the
grenade object is created from the fire message. It is never shortened by
how long the button was held.

While the grenade is in hand the server keeps its own "grenade in hand,
pin pulled" flag. It comes from the third-person animation state:

- pressing fire with the grenade at rest starts the third-person pre-fire;
- when that animation ends the flag turns **on** and stays on while the
  fire button is held;
- the update after the button is up (or when the post-fire starts) turns it
  **off**, and so does any weapon change.

The flag only matters on death (section 14).

### 4. The throw

At release (client):

- **Origin**: the camera position in first person. In third person or an
  external camera, the hand-held weapon's fire position. The server uses
  the origin the client sends, moved forward along the throw by the
  projectile record's `FireOffset` when that segment is clear (Corrections 1).
- **Direction**: the camera's forward vector, perturbed by the weapon's
  spread exactly as for bullets (`weapon_firing.md`, Spread), computed on
  the client and sent as the throw direction. The cone limits are
  multiplied by the dynamic factor g (truncated to integers) before use,
  and then n = n_min + ⌊(n_max − n_min)·p⌋·g; if n_max is 0 after the
  multiplication there is no spread at all:

```math
n_{min} = \lfloor \texttt{MinPerturb}\cdot g \rfloor,\quad n_{max} = \lfloor \texttt{MaxPerturb}\cdot g \rfloor,\quad n = n_{min} + \lfloor (n_{max}-n_{min})\,p \rfloor \cdot g\ \ (\text{only if } n_{max} > 0)
```

  The XM84 and M18 have g = 0 (`DynamicPerturbFactor` 0.0), so n_max = 0
  and they fly **exactly** along the camera's forward vector. The M67 has
  g = 1, n_min = 1, n_max = 10. Its spread fraction p is in practice
  always 0 (Open question 17): its ammo sets no movement targets
  (`WalkPerturb`, `RunPerturb`, `JumpPerturb` all default 0), its only
  firing cap is `FireMoveDuckPerturb` 1 (crouched and moving), a throw
  reads the meters before its own growth, and the 150 ms recovery delay and
  the ≥ 1.5 s between throws return the meter to 0. So an M67 is offset by
  a uniform ±0.001 on each of the right and up axes: up to about 0.057°
  per axis. No upward angle is added. Looking level throws level.
- **Speed**:

```math
\vec v_0 = s \cdot f_{perk}\cdot \hat d, \qquad s = \texttt{Velocity} = 1550\ \text{u/s}
```

  f_perk is 1 unless the thrower owns the throwing perk, in which case the
  client sends the perk's force and the server accepts it (perk data, not
  traced). The **thrower's velocity is not added**: a sprinting throw goes
  exactly as far as a standing one, relative to the world.
- **Spin**: random spin rates (Constants), visual only.
- **No secondary throw**: `AltVelocity` is never used by a player.
- **Self**: the grenade never collides with its thrower, for its whole
  life. It can still hurt the thrower when it goes off.
- **Release delay summary**: from pressing fire with the grenade ready to
  the grenade existing on the server is

```math
t_{release} = t_{update} + \frac{700\ \text{ms} + 67\ \text{ms}\cdot m + 133\ \text{ms}}{\rho_{fire}} + \text{one-way latency} + \le 1\ \text{server frame}
```

  where t_update is the one or two client frames before the pre-fire
  starts and m ≥ 1 is the number of throw-animation plays (1 if the button
  is up before the first one ends, more while it is held; each play
  rounded up to whole client frames). From a weapon switch, add the 400 ms
  draw.

### 5. Flight

Each server frame the engine integrates the grenade with the object gravity
g = (0, −1600, 0) (the data's `GravityOverride` is not used for thrown
weapons).

**First step.** The grenade takes its first physics step in the server
frame it is created in (Open question 6). The fire message is handled at
the start of the frame, before the object updates; the new object joins
the list of objects to update, gets its update (fuse test, spin) and then
its physics step in that same frame. Its start time, though, is read
before the frame's clock advances, so it is the previous frame's time.
With dt = 0.05 the two effects cancel in the step count: 49 physics steps,
in the creation frame and the 48 after it, and detonation at the start of
the 50th frame's update, 2.45 s after the creation frame (2.5 s after the
recorded start time).

With v the velocity at the start of the step:

```math
\Delta\vec p = \vec v\,\Delta t + \tfrac14\,\vec g\,\Delta t^2, \qquad \vec v' = \vec v + \vec g\,\Delta t
```

- The ¼ is a quirk (Quirk 1). Over n steps from rest the drop is
  800 n² dt² − 400 n dt² instead of 800 n² dt². At 20 Hz that is 1 u per
  step higher than a parabola, so a grenade lands later and further than
  the textbook arc, and the error depends on the server frame rate.
- Before the step, a velocity with |v|² < 0.1 is zeroed.
- There is no air drag.
- The new velocity v′ is stored **before** the move, so a bounce in this
  step works on v′ (section 6).
- The grenade is a **point** for collision with the world: it has no
  radius, so it fits through any gap. It ignores surfaces flagged
  invisible (clip brushes): grenades fly through them. Every other
  polygon of the level's physics BSP stops it, **including polygons whose
  surface lacks the solid bit** (non-solid detail such as trees and
  decoration that players walk through): the point test does not look at
  that bit (`world_dat.md`, Open question 5). The same point test runs
  against the BSP of every solid world model (doors, platforms, solid
  breakables), so those bounce it exactly like the level (section 6).
- Touching a sky surface removes the grenade silently: no explosion, no
  damage, no effect.

### 6. Bouncing and coming to rest

When the step's segment from p to p + Δp crosses a world surface with unit
normal n:

1. The grenade is placed at the crossing point. The rest of the step's
   movement is lost.
2. Its velocity becomes

```math
\vec v'' = h\left(\vec v' - 2(\vec v'\cdot\hat n)\,\hat n - 0.1\,\hat n\right)
```

   which is a mirror bounce of the post-gravity velocity, scaled as a whole
   (normal and tangential parts alike) by h, the surface's `Hardness`,
   clamped to [0, 1]. The −0.1 n is the engine's surface push (0.05 per
   contact, `surface_push`) taken twice, and it sits **inside** the
   hardness factor (Open question 18): the engine's contact response
   cancels the inward normal speed and adds 0.05 u/s into the surface,
   s = −(v′·n)n − 0.05 n; the grenade adds 2s to v′, scales by h,
   subtracts s, and the engine then adds s back, which leaves
   h(v′ + 2s). It is negligible. Stock hardness: 0.65 for nearly every surface, 0.5 chain-link
   fence, 0.1 water, liquid, invisible and desert sand, 0 air and sky. If
   the surface has no table entry, `GrenadeDampenPercent` applies
   (× 0.65).
3. **Rest test** (on the damped velocity):

```math
\sqrt{v''^2_x + v''^2_z} < 50 \ \text{and}\ n_y > 0.75 \;\Rightarrow\; \vec v = 0,\ \text{gravity and collision off}
```

   Only the world-horizontal speed counts. A grenade landing almost
   vertically therefore stops dead on its first floor contact, however fast
   it falls. On walls and on slopes steeper than about 41° it never comes to
   rest by this test.
4. The spin rates are re-rolled and a bounce sound plays: volume 100 (50
   when the grenade is in water), scaled down by |v″|·h / 200 when that is
   below 1, radius the surface's grenade sound radius (1000 u on stock
   surfaces). Bots hear it.

At rest the grenade stays exactly where it stopped, even if the floor under
it moves. On the server frame after it stops, it is nudged back along its
last direction of motion by its own half-height × max(0.1, h), once:

```math
\vec p \leftarrow \vec p - \hat u\cdot y_{half}\cdot\max(0.1,\,h),\qquad y_{half} = 6\ (\text{M67}),\ 10\ (\text{XM84, M18})
```

û is the unit velocity at the start of the step in which it stopped
(before that step's gravity), and h the hardness of the surface it stopped
on. So an M67 that comes straight down onto a 0.65 floor rises 3.9 u off
it. The half-height is the grenade's box from section 4's model: the
first animation of the projectile's world model (`Hand`) has half-extents
(3, 3, 4) for the M67 and (2, 5, 5) / (3, 5, 5) for the XM84 / M18, times
the `ModelScale` 2. The later switch to the model's `World` animation does
not change the box. A grenade at rest is also announced to bots as a
danger (`bots.md`).

**Rolling**: there is no friction for grenades. A grenade with little
vertical speed hits the floor every step, and each hit multiplies its whole
velocity by h. Its horizontal speed therefore decays by 0.65 per bounce
until it falls under 50 u/s.

**Objects** (Open question 7). Grenades are not solid. What happens on
contact depends on what is touched:

- **Solid world models** (doors, sliding and rotating brushes, solid
  `WorldModel`s and breakables, whatever their motion): the engine treats
  them as part of the world for a point mover. The grenade's segment is
  tested against the world model's own BSP and it **bounces exactly as
  off the level** (section 6, with that surface's hardness, and the rest
  test). A grenade never pushes a door (its blocking priority is 0). A
  non-solid world model is ignored.
- **Characters.** The engine reports a touch whenever the grenade's step
  segment crosses the box of the character object, and separately the box
  of the character's hit-box object. Each report is then checked against
  the **hit spheres**: it counts only if the infinite line through the
  grenade's position along its velocity passes within a node's hit radius
  (the test starts 100 u behind the grenade, but a line test has no ends).
  A counted touch snaps the grenade to the point a short trace finds
  (from p − v·dt to p) and scales its whole velocity by the character's
  surface hardness (0.65, flesh) with **no mirror**; the rest test cannot
  stop it (the contact has no plane). It then carries on through the
  character. A step can therefore be slowed once or twice (character box
  and hit-box box), and **every later step whose segment still crosses a
  box and whose line still passes a sphere slows it again**. A character
  whose spheres the line misses does not affect it at all. The thrower is
  never touched.
- **Solid props and other solid models**: a touch with no hit-sphere test
  and the same no-mirror slow by the prop's surface hardness.
- **Other grenades.** Only grenades in flight take part: a resting grenade
  has its touch reporting switched off and is skipped. A moving grenade's
  segment is tested against the other grenade's box (`grenade_box`); both
  get a touch. Each is slowed by the other's surface (grenades use the
  metal surface, hardness 0.65), no mirror. `CanHitSameKind` decides only
  between grenades **of the same thrower**: with 0 (flash, smoke) they
  pass through each other; with 1 (frag) they touch. Grenades of
  different throwers always touch, whatever the flag.

### 7. Water

How a map marks water (Open question 19): a volume brush object, a world
model whose `Name` is its BSP's name (`world_dat.md`, Objects), of class
`Water`, `CorrosiveFluid` or `FreezingWater`. Those three are the liquid
classes; every other volume class (`VolumeBrush`, `PoisonGas`, `Ladder`,
`EndlessFall`, …) is not liquid. The properties the grenade uses are
`Viscosity` (real, class default 0.035, clamped to [0, 1] on load) and
`Current` (vector, u/s, default (0, 0, 0)); `Hidden` (boolean) switches the
volume off. There is no gravity property: liquid gravity is fixed at
−200 u/s². Observed in the older regional client's 65 maps: 9 `Water`
objects in 7 maps; 6 of them have `Viscosity` 0.1 (one 0.035) and
`Current` (0, 1500, 0), the two in the dam map 0.03 and (0, 0, 0).

Each physics step, for each volume the grenade's position is inside, in
this order, before the integration of section 5:

1. **Viscosity and current**, for any volume class with viscosity c > 0,
   and only when the speed is above 1 u/s: the speed along the direction
   of motion drops by 5000·c·dt (to zero if that is more than the speed),
   **then** the current × dt is added. A volume with viscosity 0 never
   applies its current, and neither does a nearly still grenade.

```math
|\vec v| > 1:\quad \vec v \leftarrow \hat v\cdot\max\!\left(0,\ |\vec v| - 5000\,c\,\Delta t\right) + \vec C\,\Delta t
```

2. **Liquid gravity**, liquid classes only, and only while the grenade's
   gravity is on (it is off once at rest): the object gravity is replaced
   by (0, −200, 0) for this step (it enters section 5's formula in place
   of g, including the ¼ term).

Notes:

- With stock water (c = 0.1, C = (0, 1500, 0)) a moving grenade loses
  25 u/s and gains 75 u/s upward per 50 ms step, against 10 u/s of
  gravity: it is carried **up** and out of the volume.
- A resting grenade (speed 0, gravity off) is not moved by any volume.
- A water surface that is part of the solid world bounces the grenade with
  hardness 0.1. The volume itself is not a surface: entering it is not a
  bounce.
- Bounce sounds play at half volume in water. Nothing else changes: the
  fuse runs, the rest test is the same and the blast is not reduced.

### 8. Detonation

- **When**: in the first server frame whose time is at least the creation
  time + `Lifetime`, in that frame's object update, before that frame's
  physics. With dt = 0.05 and a 2.5 s fuse, the grenade gets 49 physics
  steps and goes off at the start of step 50. A `Lifetime` ≤ 0 detonates
  at creation.
- Grenades do **not** go off on impact. Other triggers: being destroyed by
  damage (section 13) and the debug bounce cap. The end of a round is
  **not** a trigger (Open question 20; this corrects an earlier reading).
- **Silent removal while a map camera runs.** If any map camera object is
  switched on (a scripted cut-scene camera) when the grenade detonates, it
  is removed with no effect, no blast and no flash.
- **Where**: the grenade's position, plus 0.1 u back along its motion if it
  touched something in the previous step. The surface type passed to the
  impact effect is the surface of the grenade's **last bounce or object
  touch**; a grenade
  that never bounced reports the metal surface (the grenade's own surface),
  not "air".
- **What**: the impact effect is sent to every client (explosion visuals and
  sound, `SoundRadius` 1500). Then, if `AreaDamage` > 0 (and the surface is
  not sky) or `ProgDamageLifetime` > 0, an explosion is created at that
  point. Its instant part runs at once, inside the same update (sections
  9–10), and any lasting part stays (section 12). The grenade is then
  removed at the end of the frame.

### 9. Blast damage (frag)

All objects whose bounding box touches the sphere of radius R =
`AreaDamageRadius` around the blast point are considered, in no particular
order. For each one:

- **Who**: players, plus any other object if the damage type is explode
  (or bullet, burn, claymore, infected explode or EMP). So for a frag, a
  player's **hitbox object** is considered as well as the player, and the
  hitbox passes its damage on to the player. Both sit at the same position,
  so **every player in range is hit twice by the same blast** (Quirk 3).
  Other grenades, breakable props and so on are also hit.
- **Distance**: d = |object position − blast point|, to the object's
  centre. For objects whose dims vector is longer than 100 u, subtract the
  excess. Nothing happens if d > R.
- **Cover** (Open question 21): for characters, hitboxes and projectiles
  (not for the character a grenade struck directly, which never happens
  with bouncing grenades), two traces are cast, blast → object centre and
  object centre → blast. What blocks them:
  - every polygon of the level's physics BSP, **whatever its surface
    flags**: the trace does not look at the solid or invisible bits, so
    non-solid detail and invisible (clip) walls that are in the BSP block
    too. The trace's surface filter would skip surface types the surface
    table marks invisible, but no stock surface type is marked, so it
    skips nothing;
  - the polygons of every world model object whose solid flag is on:
    doors, moving brushes, solid `WorldModel`s and breakables (while still
    solid), wherever they are in their motion.

  Nothing else blocks: not characters or hit boxes, not models and props,
  not other grenades, not non-solid world models, not volumes and not the
  collision blockers (`world_dat.md`). A trace of zero length is never
  blocked. If either trace is blocked and the object is a character, both
  traces are tried again to the character's head-top point (centre x and
  z, y = the top of its highest head sphere). If either trace is still
  blocked, there is no damage. So partial cover that leaves the head
  visible does not protect.
- **Objects without a cover test**: everything else in range (breakables,
  doors, props and other damageable world models) takes the blast with
  **no cover test at all**, by distance alone (minus the excess over 100 u
  of a long dims vector).
- **Falloff**:

```math
r_0 = 0.25R,\qquad m(d) = \begin{cases} 1 & d \le r_0 \\ 1 - \dfrac{d - r_0}{R - r_0} & r_0 < d \le R \end{cases},\qquad D = \texttt{AreaDamage}\cdot m(d)
```

  For the M67: full 60 within 112.5 u, 0 at 450 u, linear between. There
  is no random roll, no range table, no hit zone (zone factor 1) and no
  rear bonus. Gear protection always reads the **torso** entries for
  explode, flash, burn, poison and the other area and over-time types
  (and for any damage from a thrown weapon), whatever node was hit last
  (Open question 22).
- **Damage path**: the damage is dealt by the thrower (kill credit and team
  checks use the thrower). Then, in order:
  1. Friendly-fire filter (players only). If the thrower has a team and it
     is the victim's team, and the victim is not the thrower: with friendly
     fire **off** the damage becomes 0. With it **on**, explosions, flash,
     burn, poison and bleeding keep their full damage (bullets would be
     scaled to 25 %). Without a team (free-for-all) nothing changes.
  2. Buff and set modifiers of thrower and victim (identity in stock play).
  3. Armour (`weapon_firing.md`, Armour), with damage type explode. No stock
     body gear lists explode, so P = 0: the full damage reaches hit points
     and no armour points are used. Only a few mode gears (infected, one
     event suit) protect against explode.
- **Self**: the thrower is hurt like anyone else (twice, like any player),
  whatever the friendly-fire setting.
- A player dies when HP ≤ 1 after positive damage (`weapon_firing.md`).

### 10. Flash

- The blast test of section 9 runs with R = 1000 and damage type flash.
  Only **player objects** are considered (not hitboxes), so a player is
  flashed once. The same cover traces apply: if the blast point and the
  victim's centre (or head top) cannot see each other, there is no flash.
  The same falloff gives the **flash amount**:

```math
D = 25 \cdot m(d),\quad r_0 = 250,\ R = 1000
```

- The friendly-fire filter of section 9 applies (teammates with friendly
  fire off are not flashed, the thrower always is).
- **Protection** (Open question 22): the sum of the victim's gear torso
  entries for the flash type (34), plus any set-item bonus (none in stock
  play), clamped to [0, 1], removes that fraction:
  D′ = D·(1 − clamp(P, 0, 1)). Armour points play no part and are not used.
  If D′ ≤ 0.001, nothing happens. Gear with a flash entry (torso value,
  which equals the head value in every record):

  | Gear (data name) | Flash protection |
  |---|---|
  | `Anti-Flash`, `Skull_head`, `Codename_Raven`, `Prison_Recon_S_grade`, `Assassin_F`, `Assassin_M`, `Protect_flash_100` | 1.0 |
  | `Ballistic_Mask`, `SnowMan_Head`, `Davil_Mask`, `Guy_Fawkes_mask` | 0.8 |
  | `Protect_flash_75` | 0.75 |
  | `Anti-Flash_70`, `Recruit_Survival_Bandana` | 0.7 |
  | `Protect_flash_45`, `Protect_flash_55`, `Protect_fire_24` | 0.55 |
  | `Anti-Flash_50`, `Codename_Viper`, `Powered_Suit_Helmet_Extreme`, `Stek_Headgear`, `RoboSkull_HauserRider_G` | 0.5 |
  | `Protect_flash_25`, `Protect_flash_35`, `Protect_fire_22`, `Protect_fire_20` (second record) | 0.45 |
  | `RoboSkull_HauserRider_S` | 0.4 |
  | `Protect_fire_20` (first record), `Protect_gas_20` (second record) | 0.35 |
  | `Anti-Flash_30`, `Eyepatch_Skull`, `Powered_Suit_Helmet_Hard` | 0.3 |
  | `Protect_fire_16` (second record), `Protect_gas_16` (second record), `Protect_gas_20` (first record) | 0.25 |
  | `Eyepatch`, `Protect_flash_15`, `Protect_fire_16` (first record), `Protect_gas_16` (first record) | 0.15 |
  | `Protect_flash_12` | 0.12 |
  | `Protect_flash_9` | 0.09 |
  | `Infected`, `Host_Infected` (mode gear) | 1.0 |

  The default body vest and helmets have no flash entry (P = 0).
- **Angle** (Open question 11): let f be the victim's body facing and
  u = victim centre − blast point. The body facing is **yaw only**: the
  client sends the server only the yaw of the player's rotation (the
  player model never pitches), compressed for the network, so
  f = (sin yaw, 0, cos yaw) in file axes and only the horizontal part of u
  matters. Looking up or down changes nothing. A blast exactly above or
  below the victim's centre gives f·u = 0, which counts as facing away.

```math
f\cdot u < 0\ (\text{facing the blast}) \Rightarrow \text{delay } 0.25\ \text{s},\ T = D' \qquad\qquad f\cdot u \ge 0 \Rightarrow \text{delay } 0.0833\ \text{s},\ T = D'/4
```

  This is a hard 180° split; there is no gradual angle term.
- **Timing**: the server starts a timer of that delay. On the first server
  frame after it runs out it sends the client the flash time T, unless
  T < 0.1 s. A new flash hit before that restarts the delay and **replaces**
  T, even with a weaker flash.
- **Screen effect** (client, real time from the message's arrival):
  receiving a flash stops any flash in progress and starts a new one.
  With u = t / T:

```math
S(u) = \begin{cases}\tfrac12\left(1 - \cos\pi u\right) & u < 0.5\\ u & u \ge 0.5\end{cases},\qquad w(t) = \operatorname{clamp}\left(1 - W\cdot S(u),\,0,\,1\right)
```

  W is the `WhiteScreenDecl` of the weapon **the victim is holding**
  (Quirk 6). No record in the weapon data sets `WhiteScreenDecl` or
  `CapturedDecl`, so every weapon (guns, knives, grenades, specials) uses
  the defaults W = 2.0 and C = 1.0, and the held weapon never changes the
  effect (Open question 11). With W = 2:

```math
w(t) = \cos\frac{\pi t}{T}\quad (0 \le t < T/2), \qquad w = 0\ \text{and the effect ends at}\ t = T/2
```

  Each frame draws a snapshot of the screen taken on the effect's second
  frame with alpha w·C (C = `CapturedDecl` of the held weapon, 1.0 stock),
  then a full-screen white rectangle with alpha w. **Draw order**
  (Open question 12): the HUD is drawn first, then the snapshot and the
  white, so the white covers the HUD; pop-up windows (menus, chat and
  message boxes) are drawn after and stay on top. The HUD is hidden for
  the effect's first frame only, so the snapshot (taken on the second
  frame, before that frame's HUD is shown again) holds the scene without
  the HUD. The effect also stops if the victim dies (unless the death
  camera is the first-person chase view) or leaves the game.
- Worst case for a stock XM84: facing it within 250 u, T = 25 s, fully
  white at first, half strength (cos 60°) at 8.33 s, gone at 12.5 s.
  Facing away: T = 6.25 s, gone at 3.125 s.
- **Deafening**: none. The code that would play and fade a ringing sound
  (volume 1 − t/(T/2)) is disabled; only the normal flash explosion sound
  (radius 1500) plays.
- Bots are not blinded by this path (players only); see `bots.md`.

### 11. Smoke

- **Start**: the smoke grenade detonates when its 3.0 s fuse runs out,
  wherever it is (mid-air included). Its blast radius is 0, so the server
  does nothing but send the impact effect.
- **The cloud** is drawn by every client from the effect data (BR client's
  effect archive; the presentation belongs to the effects spec): a 0.1 s
  flash sprite and light, then a particle system that runs 2.0 s from the
  detonation. It emits 10 smoke sprites (emission interval −1, which we
  read as "one burst at the start"; see Open questions) from a sphere of
  radius 0–10 u. Each sprite lives 8–10 s and grows from scale 200 to 300
  over its life. Initial velocities are random in the box
  (−1000…1000, −1000…1000, −200…400) in the effect's frame, damped by a
  friction of 0.98, with an extra acceleration of 10 and full bounce off
  the world. So the cloud lasts at most about 12 s after detonation.
  (Superseded: one burst on the first frame, 75 % bounces, gone by 10 s;
  Corrections (2026-10-22) 2.)
- **What it blocks**: nothing in game logic. The server has no smoke
  object. Bullet traces, bot sight, name tags and crosshair identification
  all ignore it. It only hides things visually.

### 12. Fire and gas (incendiary, nerve gas)

- On detonation an invisible damage sphere of radius `ProgDamageRadius`
  (350 u) is created at the blast point. It lives `ProgDamageLifetime` (9 s)
  and is removed early if its thrower leaves the game.
- Every server frame, each player inside (centre distance ≤ 350 u, with the
  cover traces of section 9 cast from 1 u toward the target) has an
  over-time damage entry set or **refreshed** to: rate `ProgDamage`
  (HP/s) for `ProgDamageDuration` seconds. There is no falloff. For burn,
  hitboxes are also considered, but they refresh the same entry, so there
  is no doubling.
- The entry deals rate × frame time every server frame until its duration
  runs out. Leaving the sphere or the sphere expiring therefore leaves
  `ProgDamageDuration` more seconds of damage: burn 10 s, poison 2.5 s.
- Teammates with friendly fire off are exempt from burn and poison; the
  thrower is not. Armour applies per hit (`weapon_firing.md`): the stock
  body gear has 0.5 burn protection, so with armour points left burn is
  halved and uses armour points. Poison is not protected.

### 13. The grenade as a target

- A live grenade has `Health` hit points (frag 300, flash/smoke 50). Any
  damage that brings it to ≤ 1 HP detonates it at once, as in section 8.
  Team does not matter.
- Blasts hit grenades (section 9, with cover). A frag never sets off
  another frag (60 < 300), but it sets off a flash or smoke grenade within
  about 174 u (60·m(d) ≥ 49).
- **Bullets** (Open question 8): yes, in flight and at rest. A grenade
  keeps the ray-hit flag its whole life (coming to rest clears only its
  gravity, touch and solid flags), so the server's bullet trace (and a
  client-reported object hit, `weapon_firing.md`) can stop on its box
  (`grenade_box`, centred on its position). The hit is an ordinary bullet
  impact: the grenade's surface is metal (not shoot-through, so the bullet
  stops there), and the bullet's damage, with its range factor and no hit
  zone, comes off the grenade's hit points. The shooter's team does not
  matter (no stock grenade ammo sets the ally-ignore flag). When the hit
  points drop to 1 or less it **detonates at once, exactly as when its
  fuse ends** (section 8): full blast, flash or smoke at its position,
  credited to the thrower, not the shooter. A frag needs 299+ damage, a
  flash or smoke 49+.

### 14. Death release and the drop perk

- **Death with the pin pulled**: when a player dies while the server's
  "grenade in hand, pin pulled" flag (section 3) is on, a live grenade of
  that weapon is created at the player's position (the hull centre),
  moving at 500 u/s along the body's facing. It has the **full fuse**
  (2.5 s for the M67), not what was left of anything, and it is credited
  to the dead player. The ammo count is **not checked** (Quirk 5). This
  happens before the death drop of `inventory.md` section 12.
- **Drop-grenade perk**: with that perk, on death and with the perk's
  probability, a grenade of the perk's thrown weapon is created at the body
  position moving straight up at that grenade's `DropVelocity` (500 for the
  M67, 0 if unset), full fuse.

### 15. Grenades and the end of a round

(Open question 20.) Nothing in the round logic touches thrown grenades or
lasting explosions:

- A grenade **in flight or at rest** when a round is decided keeps its
  fuse and detonates normally, with normal damage and kill credit (only
  the end-of-game roulette phase, `game_modes.md`, ignores all damage).
  The end-of-round cleanup removes bodies and dropped weapons, not
  projectiles.
- The round-based modes start the next round by respawning the players in
  the **same, unreloaded world**, so a grenade, or a fire or gas sphere
  (section 12, 9 s), still alive at that moment **carries over** into the
  next round and hits the respawned players. With the reference server
  configuration's 3 s end-of-round phase and 0 s round start, a frag
  (2.5 s fuse) thrown at the moment of the decision is gone before the
  respawn, a smoke (3.0 s) goes off about as the next round starts, and
  fire and gas spheres always carry over.
- A grenade **in hand** is released only by a death (section 14); a
  respawn into the next round is not a death, so nothing is released.
- A map camera switched on by the map's script when a grenade's fuse ends
  removes it without effect (section 8).

## Per-tick order

Server, one frame (dt = 1/20 s):

1. Client messages are handled as they arrive, before the frame's clock
   advances. A fire message from a thrower creates the grenade at the sent
   origin, sets its velocity, records its start time (the previous frame's
   time) and tells clients to draw it. A death runs section 14.
2. The frame's clock advances. Every object is updated in turn, grenades
   created in step 1 included. For a grenade:
   1. Update. If it stopped last frame and touched something, nudge it
      (section 6). Then the fuse test: if now ≥ start + `Lifetime`,
      detonate (section 8). The blast or flash is applied to everything in
      range right here, and grenades it destroys detonate inside this call.
   2. Physics, unless the grenade is at rest: water volume physics
      (section 7), then the integration of section 5, then the move. On a
      surface crossing the bounce, the rest test and the sound of section 6
      run inside the move.
3. An explosion that lasts (fire, gas) applies its over-time refresh in its
   own update every frame (section 12).
4. Each player's damage bookkeeping checks the flash timer and sends the
   flash time to the client when it has run out (section 10), then applies
   over-time damage.
5. Removed objects (detonated grenades, finished explosions) leave the world
   at the end of the frame.

Client, per rendered frame: the throw follows `inventory.md`'s per-frame
order (the release is a weapon-update animation event). The flash effect is
drawn after the scene, on real time.

## Edge cases

- **Throwing into a wall at point blank.** The grenade starts at the eye.
  If the wall is closer than one step of travel (77.5 u at 1550 u/s and
  20 Hz), it reaches the wall on its first step, mirrors at 65 % speed and
  flies back past the thrower, whom it cannot touch. It does not explode on
  the wall. A grenade can only start inside geometry if the camera is.
- **Throwing straight down.** The first floor contact has horizontal speed
  0, so it stops dead at the contact point.
- **Cooking to zero.** Not possible; the fuse is fixed (section 3).
- **Dying while holding.** During the 700 ms pre-fire, before the
  third-person pre-fire has ended: nothing is released. After that, while
  fire is still held: a full-fuse grenade at 500 u/s is released (section
  14). While fire is held the first-person throw has not happened yet
  (section 3), so this is the held grenade, not a second one.
- **Holding indefinitely.** Fire held after the pin pull keeps the grenade
  in hand with no limit; the fuse does not run. The throw animation
  restarts every 67 ms.
- **Switching away mid-throw.** Before the release event (133 ms into the
  post-fire): no grenade, ammo unchanged, no reload needed. After it: the
  grenade is already flying and the ammo is used. The switch also clears
  the server's pin-pulled flag.
- **Sprint during a throw.** The throw plays to its end and releases
  (`inventory.md`, section 7). Starting a throw while sprinting is not
  possible.
- **Fuse ends in mid-air.** It detonates where it is; the blast is a full
  sphere.
- **Grenade on a slope.** Slopes with normal y ≤ 0.75 never stop it. It
  keeps bouncing down until it reaches a flatter surface or the fuse ends.
- **Grenade into the sky.** Removed silently (section 5).
- **Two flashes.** The later one replaces the earlier one, at the server
  timer and again on the client, even if it is weaker.
- **Victim exactly at the blast point.** d = 0 gives m = 1. The cover
  traces have zero length and are not blocked.
- **Frame-rate dependence.** Flight (the ¼ term), the 50 ms fuse and flash
  timer resolution, and how many bounces "rolling" takes all depend on the
  server frame rate. The rest test and bounce factors do not.

## Quirks

Keep all of these unless noted.

1. **Quarter-gravity displacement.** Position uses ¼·g·dt² per step instead
   of ½. The arc is slightly higher and longer than a parabola, more so at
   low server frame rates. Keep: the throw distances players learned depend
   on it, together with the 1600 gravity (the engine's comment says thrown
   weapon speeds were tuned for 1600).
2. **No thrower velocity.** Running throws do not go further. Keep.
3. **Double blast on players.** A player and that player's hitbox are both
   hit, so frags (and other explode-type blasts) do twice the listed damage
   to players: up to 120 for the M67. The developers knew and chose not to
   change the data. Keep, and verify (Open questions).
4. **Vertical landings stop dead.** The rest test ignores vertical speed.
   Keep.
5. **Extra grenade on death.** The death release does not check the ammo
   count. A player who throws and keeps fire held can release a second,
   free grenade by dying before the post-fire. Keep as specified; verify.
   Note (follow-up): with the hold behaviour of section 3 the first-person
   throw only happens after the button is up, so this needs the server's
   pin-pulled flag to still be on after the release; the flag goes off
   when the third-person post-fire starts. The window is at most the time
   between the button release and the third-person post-fire, and may be
   empty on the live game.
6. **Flash curve from the victim's weapon.** The fade constants are read
   from whatever weapon the victim holds. No weapon in the data sets them,
   so every weapon uses the defaults (W = 2.0, C = 1.0) and the effect is
   the same. Keep the defaults; reading the flash grenade's own values is
   acceptable.
7. **Flash is half its time.** With the default constant the white is gone
   at T/2. Keep.
8. **Facing away is a quarter, and sooner.** The code calls it "half", but
   it is a quarter of the time, with a third of the delay. Keep.
9. **Grenades pass through clip brushes and characters** (sections 5, 6),
   but bounce off non-solid level detail and off solid doors and moving
   brushes. Keep; verify the character part (Open question 7).
10. **Smoke blocks nothing.** Bots and name tags see through smoke. Keep.

## Corrections (2026-10-19)

From a second pass over the reference's server projectile creation and
explosion code, compared with this spec and our implementation. Test cases
59–60.

1. **The server moves the spawn point by the projectile's `FireOffset`.**
   - Reference: when the projectile-effect record's `FireOffset` (number,
     units, default 0) is non-zero, the server tests the segment from the
     client's origin to origin + `FireOffset` × the throw direction against
     objects and the level, ignoring the thrower. If nothing is hit, the
     grenade is created at the far end of that segment; if something is hit,
     at the client's origin. Speed, direction and fuse are unchanged.
   - Ours: section 4 said "The server uses the origin the client sends",
     and `launch` spawns at the eye (`games/combat_arms/src/game/grenades.rs:826-837`).
     The field is in no table of this spec or `attribute_data.md`.
   - Why it matters: with a positive offset the grenade starts ahead of the
     face, so it clears a lip or railing right in front of the thrower, and
     it lands that much further. With the stock value unknown (Open
     question below), the size of the effect is unknown; 0 changes nothing.
2. **The big-object slack applies to players too, by their dims.**
   - Reference: the "length of the dims vector over 100" subtraction of
     section 9 is made for every game object, players and hit boxes
     included. Stock player and hit-box dims are well under 100 long, so
     it changes nothing for them. It would apply to an oversized
     character model.
   - Ours: section 9 said "(not players)". The implementation applies it as
     the spec said (no player case to check). Text fix only.

## Corrections (2026-10-22)

From the effects-and-sounds comparison (`effects_sounds.md` Corrections
(2026-10-22)), the parts that touch this spec: the flash's screen curve,
the smoke cloud's life and the explosion look of a mid-air grenade. Each
entry: the reference behaviour, what we do now (file:line), and a testable
expectation. Test cases 61–63.

1. **The flash fades on a cosine, not a straight line** (section 10, Screen
   effect).
   - Reference: as section 10 says, w(t) = cos(π t / T) until T/2. The
     white stays near full for the first part, so the victim is fully blind
     for longer than a linear fade would make them.
   - Ours: the server sets Chimera's `Blinded` with alpha 1 fading
     linearly from the moment of arrival to T/2
     (`games/combat_arms/src/game/grenades.rs:1450-1454`;
     `chimera/core/src/core.rs:1556`, `alpha_at`). The client overlay draws
     that linear alpha, and `flash_white` (the cosine) is not used for
     drawing.
   - Expect: T = 25 s (facing, within 250 u): white 0.924 at 3.125 s,
     0.707 at 6.25 s, 0.5 at 8.333 s, 0 at 12.5 s. Ours today: 0.75, 0.5,
     0.333, 0. Facing away (T = 6.25 s): 0.707 at 1.5625 s (ours 0.5).
2. **The smoke cloud is gone 10 s after detonation, not 12 s** (section
   11; `effects_sounds.md` 9 and its Open question 12).
   - Reference: one burst of 10 particles on the first frame (no emission
     over 2.0 s). Each particle lives 8–10 s, so the cloud is fully gone by
     10 s (barring the off-screen freeze of `effects_sounds.md` Quirk 7).
     It is spread out within about 0.5 s: friction 0.98 per second, at most
     about 256 u per horizontal axis. Bounces keep 75 % of the speed (not a
     full bounce).
   - Ours: the placeholder cloud grows to 300 u over 2 s and fades out
     from 8 s to 12 s (`games/combat_arms/src/game/grenades.rs:397-410`,
     `smoke_look`). The section 11 text above says "about 12 s" and "full
     bounce"; both are superseded here.
   - Expect: smoke detonation at t = 0 → cloud at full size by about 0.5 s;
     nothing of it left at t = 10.0 s; at t = 11 s the view through the
     spot is clear.
3. **A grenade's explosion look in mid-air** (section 8, "Where").
   - Reference: section 8 is right. The surface sent with the detonation
     is that of the last bounce or touch, or the grenade's own when it
     never touched anything, and the normal is world up. Clients show that
     surface's grenade group even in mid-air.
   - Ours: the clients' look uses the nearest map triangle within 16 u,
     else Air, which has no explosion group
     (`games/combat_arms/src/game/effects/mod.rs:1010`). Section 8's rule
     is not implemented for the look.
   - Expect: as `effects_sounds.md` case 58.

## Test cases

Common setup unless stated: server dt = 0.05 s; flat floor y = 0 with
hardness 0.65 (normal (0, 1, 0)); no water; stock data; positions are the
grenade's position at the end of the numbered physics step, with step 1
being the physics step of the server frame the grenade is created in
(Open question 6; the numbers are the same either way); values ±0.05 u,
±0.05 u/s;
victims are standing players with 100 HP whose centre is the stated distance
from the blast point, in the open, armour points 0, no gear, no buffs.

1. **Level throw.** M67 created at (0, 60, 0) with v = (0, 0, 1550). → After
   step 1: (0, 59, 77.5), v = (0, −80, 1550). After step 5: (0, 15, 387.5).
   Step 6 hits the floor at (0, 0, 442.86) with v = (0, 311.94, 1007.5).
2. **Same throw, later.** Floor contacts in steps 15 (z = 859.68), 23
   (1092.65), 30 (1232.09), 36 (1309.53), 41 (1354.01) and 46 (1377.87).
   It never comes to rest. It detonates at the start of step 50 at
   (0, 9.95, 1389.27).
3. **Dropped straight down.** v = 0 at (0, 60, 0). → Steps 1–5: y = 59, 54,
   45, 32, 15. Step 6: contact at (0, 0, 0), v = 0, at rest. It stays there
   (apart from the one-time nudge of section 6) until it detonates at
   t = 2.5 s.
4. **Point-blank wall.** Wall plane z = 50 facing −Z. Created at (0, 60, 0),
   v = (0, 0, 1550). → Step 1: contact at (0, 59.35, 50),
   v = (0, −52, −1007.44). It lands behind the thrower: floor contact in
   step 7 at (0, 0, −204.75), and comes to rest in step 48 at
   (0, 0, −852.27).
5. **45° throw.** Created at (0, 60, 0), v = (0, 1096.02, 1096.02). → Step 1:
   (0, 113.8, 54.8). Step 10: (0, 418.01, 548.01). Step 29: floor contact at
   z = 1585.67, v = (0, 795.52, 712.41).
6. **Rest threshold.** At a floor contact the post-gravity velocity is
   (76, −480, 0). → After the bounce the horizontal speed is 49.4 < 50: at
   rest, v = 0. With (78, −480, 0): horizontal 50.7, so it bounces with
   v = (50.7, 311.94, 0).
7. **No rest on walls.** Contact with a wall of normal (1, 0, 0) at
   post-gravity velocity (−20, −30, 0). → v = (12.94, −19.5, 0) (the 0.1
   push included), still moving.
8. **Soft surface.** Floor of desert sand (hardness 0.1). Post-gravity
   velocity (600, −480, 0) at contact. → v = (60, 47.99, 0): horizontal 60,
   so it keeps going.
9. **Frag, close.** M67 blast 100 u from a victim. → Two applications of 60:
   the victim dies (120 ≥ 99). The same at 100 u from the thrower: the
   thrower dies.
10. **Frag, mid.** Victim at 300 u. → m = 0.4444, 26.67 per application,
    53.33 HP lost in total.
11. **Frag, edge.** Victim at 450 u → 0 damage. At 460 u → not affected.
12. **Frag, cover.** A wall from floor to ceiling between the blast and the
    victim at 100 u → 0 damage. A waist-high wall that blocks the centre
    trace but not the head-top trace → 120 (dies).
13. **Frag vs armour.** Victim at 300 u with 100 armour points and the
    default body gear. → HP 100 → 46.67, armour still 100.
14. **Friendly fire.** Teammate at 100 u. Friendly fire off → 0 damage. On →
    120 (dies), not 25 %.
15. **Flash, facing, close.** XM84 blast 200 u in front of the victim (body
    facing the blast). → Flash time 25 s, sent 0.25 s after the blast (on
    the server frame at or after that). On the client: w = 1 at t = 0,
    0.7071 at 6.25 s, 0.5 at 8.333 s, 0 and finished at 12.5 s.
16. **Flash, facing, mid.** Blast 625 u in front. → T = 12.5 s;
    w(3.125 s) = 0.7071; ends at 6.25 s.
17. **Flash, facing away.** Blast 200 u behind the victim. → Delay 0.0833 s,
    T = 6.25 s, w(1.5625 s) = 0.7071, ends at 3.125 s.
18. **Flash, gear.** As 15 with full anti-flash goggles (protection 1.0) →
    no flash. With 0.3 goggles → T = 17.5 s, ends at 8.75 s.
19. **Flash, range.** Blast 990 u in front → T = 0.333 s, ends at 0.167 s.
    At 998 u: T = 0.067 s < 0.1, so nothing is sent. At 1000 u → none.
20. **Flash, cover.** As 15 with a wall between blast and victim (centre and
    head top both hidden) → no flash.
21. **Flash, self and team.** The thrower stands 200 u from the blast,
    facing it → T = 25 s. A teammate in the same spot: friendly fire off →
    none; on → T = 25 s.
22. **Flash, no damage.** Any of 15–21 → hit points unchanged.
23. **Two flashes.** Victim facing a blast at 200 u (T = 25 s), then 1 s
    later facing one at 625 u (T = 12.5 s). → After the second message the
    effect restarts with T = 12.5 s (w = 1 again, ends 6.25 s later).
24. **Fuses.** Created at t = 0, left in the air with no floor. → M67
    detonates at 2.5 s, XM84 at 1.5 s, M18 at 3.0 s (each on the first
    server frame at or after that time).
25. **Smoke.** M18 detonates 100 u from a player → no damage. A bullet fired
    through the cloud hits a target behind it. A bot behind the cloud still
    sees the player. The enemy name tag still shows through it.
26. **No thrower velocity.** Thrower moving at (0, 0, 300) throws along +Z
    from (0, 60, 0) → initial grenade velocity (0, 0, 1550), and the
    positions of test 1.
27. **No cooking.** Hold fire for 3 s with the M67 ready. → The grenade
    leaves at 700 + k ms (k ≤ 67), and detonates 2.5 s after it reaches the
    server, not 2.5 s after the press. **Wrong release time:** holding
    keeps the grenade in hand; see case 37 for the corrected timing. The
    fuse part stands.
28. **Switch mid-throw.** Press fire with the M67 ready, press 1 at
    t = 300 ms. → No grenade is created; M67 total stays 1.
29. **Death after the pin pull.** Player hull centre at (0, 40, 0), body
    facing +Z, holding fire after the third-person pre-fire has ended,
    killed. → A live M67 at (0, 40, 0) with v = (0, 0, 500) and a 2.5 s
    fuse: floor contact in step 5 at z = 117.65, at rest in step 36 at
    (0, 0, 361.19), detonates at 2.5 s.
30. **Death during the pin pull.** Killed 100 ms after pressing fire → no
    grenade.
31. **Sky.** A grenade whose path crosses a sky surface → removed at once,
    no explosion, no damage.
32. **Chain.** An XM84 at rest 150 u from an M67 blast, in the open → takes
    53.33, so it detonates (flashes) in the same server frame. At 200 u it
    takes 44.44 (5.56 HP left) and does not.
33. **Incendiary.** M-23F detonates 100 u from a victim with no armour, who
    stays put. → 5 HP/s for the sphere's 9 s, then 10 s more: about 95 HP
    lost in total, the last at about 19 s. Walking out at 4 s instead: about
    20 + 50 = 70 HP.
34. **Nerve gas.** G7A, the same setup → 6 HP/s for 9 s + 2.5 s: about 69 HP.
35. **Water.** A grenade inside a water volume (default gravity, viscosity
    0) with v = (0, 0, 0) → after step 1, v = (0, −10, 0) and the drop is
    0.125 u (not 1 u): gravity −200.
36. **Tap throw timing.** M67, XM84 or M18 ready, ρ_fire = 1, ideal frame
    timing. Press and release fire within 100 ms; t = 0 is the first weapon
    update of the pre-fire. → Throw animation 700–767 ms, post-fire from
    767 ms, release (fire message, ammo used) at 900 ms, post-fire ends at
    1133 ms. XM84 with 2: the reload animation runs 1133–1533 ms and the
    second grenade can start its pre-fire on the next weapon updates after
    1533 ms.
37. **Hold throw timing.** As 36, but fire held until t = 3000 ms. → The
    throw animation restarts at 767, 834, …; the play running at 3000 ms
    ends at 700 + 67·35 = 3045 ms, the post-fire starts then and the
    release is at 3178 ms. The fuse starts on the server when that fire
    message arrives.
38. **Release event placement.** A view-model animation set with the throw
    animation's first key at 0 ms and the post-fire's fire key at 133 ms →
    no grenade at the end of the 67 ms throw animation; the grenade leaves
    on the post-fire key.
39. **Flash and smoke fly straight.** XM84 or M18, camera forward (0, 0, 1),
    thrower running, jumping or crouch-walking → throw direction exactly
    (0, 0, 1) (g = 0 makes n_max = 0).
40. **Frag spread.** M67, camera forward (0, 0, 1), right (1, 0, 0), up
    (0, 1, 0), p = 0 → n = 1: direction normalize((a, b, 1)) with a, b
    uniform in [−0.001, 0.001]; never more than 0.081° from forward
    (both axes at 0.001). With p = 0.5 (not reachable in stock play):
    n = 1 + ⌊9·0.5⌋ = 5.
41. **Bounce push inside hardness.** Floor normal (0, 1, 0), hardness 0.65,
    post-gravity velocity (100, −300, 0) at contact. → v = (65, 194.935, 0)
    (0.05 in place of 0.1 would give 194.9675).
42. **First step in the creation frame.** Server frames at 9.95 s and
    10.00 s; the fire message is handled in the 10.00 s frame. → The
    grenade's start time is 9.95 s; it takes its first physics step in the
    10.00 s frame; with a 2.5 s fuse it detonates at the start of the
    12.45 s frame after 49 steps.
43. **Rest nudge, vertical.** Case 3: at rest at (0, 0, 0) in step 6. → At
    the next frame's update it moves to (0, 3.9, 0) (6 × 0.65) and stays
    there. An XM84 in the same drop: (0, 6.5, 0). An XM84 coming to rest on
    sand (hardness 0.1): 1.0 u up.
44. **Rest nudge, slanted.** Case 6, first variant (contact velocity
    (76, −480, 0), velocity at the start of that step (76, −400, 0)), M67 on
    a 0.65 floor. → After the stop, the next update moves it by
    (−0.728, +3.831, 0) from the contact point.
45. **Characters: sphere test.** A grenade whose step segment crosses a
    standing player's box but whose line of motion misses every hit sphere
    → velocity and path unchanged. One whose line passes through the chest
    sphere and whose segment crosses only the character's box in that step
    → velocity × 0.65, no change of direction.
46. **Solid door bounces.** As case 4 with the wall replaced by a closed,
    solid sliding door whose face is the plane z = 50 (surface hardness
    0.65) → the same contact point and velocity as case 4. The same door
    with its solid flag off → no contact; the grenade flies on.
47. **Non-solid level detail bounces.** As case 4 with the wall made of a
    physics-BSP polygon whose surface lacks the solid bit (players walk
    through it) → the same numbers as case 4.
48. **Grenades and grenades.** Frags of two different players crossing in
    flight, segment of A crossing B's box → A's velocity × 0.65 (and B's
    × 0.65). Two XM84 of the same player → no effect on either. An M67
    flying through the box of a resting XM84 → no effect.
49. **Stock water.** Inside a `Water` volume with `Viscosity` 0.1 and
    `Current` (0, 1500, 0), v = (0, 0, −1000) at the start of a step. →
    After the step v = (0, 65, −975) and the displacement is
    (0, 3.625, −48.75). With v = (0, 0, 0.5) instead (speed ≤ 1): no drag,
    no current, v = (0, −10, 0.5).
50. **Cover blockers.** M67 blast 100 u from a victim. Between them a
    full-height wall made of polygons with the invisible bit → 0 damage. Of
    non-solid level polygons → 0. A closed solid door → 0. Another player
    standing between → 120 (no cover). A prop model → 120.
51. **Breakables skip the cover test.** A breakable whose centre is 100 u
    from an M67 blast, dims vector shorter than 100 u, behind a full wall →
    takes 60 once.
52. **Flash gear.** As case 15 with `Ballistic_Mask` (0.8) → T = 5 s, gone at
    2.5 s. With `Eyepatch` (0.15) → T = 21.25 s, gone at 10.625 s. With
    `Anti-Flash_50` and `Eyepatch` together (0.65) → T = 8.75 s.
53. **Flash facing is yaw only.** XM84 blast 200 u horizontally in front of
    a victim looking 80° up → T = 25 s (facing). Blast 200 u directly below
    the victim's centre → f·u = 0: delay 0.0833 s, T = 6.25 s, whatever the
    victim's pitch.
54. **Held weapon does not change the flash.** Case 15 with the victim
    holding a knife, a rifle or a grenade → the same w(t).
55. **Flash over the HUD.** During case 15 at t = 1 s (w ≈ 0.99): the HUD is
    under the white; an open chat or message pop-up is drawn on top of it.
56. **Round end.** An M67 thrown 1.0 s before a round is decided, with a
    3 s end-of-round phase → explodes 1.5 s into that phase; a player 100 u
    away takes 120 and dies. An M-23F that detonated 1 s before the
    decision → its sphere lasts 5 s into the next round (respawn at 3 s)
    and burns respawned players inside it.
57. **Map camera.** A grenade whose fuse ends while a map camera object is
    on → removed: no blast, no flash, no damage.
58. **Shooting grenades.** A resting XM84 hit by rifle rounds dealing 25
    each → after one hit 25 HP left; the second hit detonates it (flash) at
    once, credited to its thrower. A flying M67 hit by twelve such rounds →
    detonates on the twelfth (11 leave 25 HP).
59. **Fire offset (Corrections 1).** Projectile `FireOffset` 20 (for the
    test), thrower's eye at (0, 64, 0) looking along +Z, nothing within
    20 units → the M67 starts at (0, 64, 20) with velocity (0, 0, 1550)
    (before the ±0.001 spread). A wall 10 units ahead → it starts at the
    eye. `FireOffset` 0 → at the eye.
60. **Slack on a large character (Corrections 2).** A character whose dims
    vector is 130 units long, 300 units from a frag's blast point →
    distance 270: 60 × (1 − (270 − 112.5)/337.5) = 32.0 per hit.
61. **Flash curve (Corrections 2026-10-22, 1).** Facing a stock XM84 at
    200 u (T = 25 s) → screen white 1.0 at 0 s, 0.924 at 3.125 s, 0.707 at
    6.25 s, 0.5 at 8.333 s, 0 at 12.5 s.
62. **Smoke life (Corrections 2026-10-22, 2).** Smoke detonation on a floor
    at t = 0 → no smoke particle left at t = 10.0 s (looking at it the
    whole time).
63. **Mid-air frag look (Corrections 2026-10-22, 3).** M67 exploding in the
    air without having touched anything, seen at 500 u → the grenade's own
    surface group (metal: `Grenade_Stone`) plus `gren.wav`.

## Open questions

1. **Steam build vs reference.** Behaviour comes from a BR-region code
   base, data from the US client, the smoke effect from the BR client.
   Check the M67 numbers (60 / 450 / 2.5 s / 1550) on the Steam build's
   data.
2. **Double blast (Quirk 3).** Confirm in game: an M67 within 112 u of a
   100 HP enemy with no armour should kill (120). If it leaves 40 HP, the
   hitbox is not inside the blast on the live build and the damage is
   single.
3. **Release key time k** inside the 67 ms throw animation, and the
   lengths of the XM84 and M18 animations (assumed the same as the M67).
   **Answered:** the release is not in the throw animation at all. The
   throw animation's only key (at 0 ms) tells the server the third-person
   throw has started; the release is the fire key of the 366 ms post-fire,
   at 133 ms. So the release comes 700 + 67 + 133 = 900 ms after the
   pre-fire starts (at fire rate 1), not 767 ms. The M67, XM84 and M18
   view-model animation files have identical lengths and key times
   (pre-fire 700, throw 67, post-fire 366 with the release at 133, draw
   400, reload 400 ms); only a sound key inside the pre-fire differs.
   Applies in sections 3 and 4 (release delay), the Constants table
   (`release_key`, `throw_loop`) and cases 36–38; case 27's release time is
   corrected by case 37.
4. **Hold behaviour.** **Answered:** holding fire keeps the grenade in hand.
   When the throw animation ends with fire held (and grenades left), the
   weapon update restarts the throw animation instead of starting the
   post-fire; the post-fire, and with it the release, only follows the end
   of the throw animation that is playing when the button is up. The fuse
   still starts at the release, so there is still no cooking. Applies in
   the Summary, section 3 (step 4), section 4, Edge cases and case 37.
5. **Second grenade on re-ready.** With two flashes or smokes, after the
   first throw the clip is 0 and the total 1. Does the reload request play a
   re-draw animation, and how long until the second can be thrown?
   **Answered:** when the post-fire ends the weapon goes idle and the empty
   clip starts the grenade's **reload** animation (400 ms at the reload
   rate), not the draw; the clip is refilled when it ends. A fire press
   during it is queued. The second pre-fire can start at the earliest
   1533 ms after the first pre-fire started (900 + 233 + 400). Applies in
   section 3 (step 6) and case 36.
6. **Server frame rate.** 20 Hz comes from the shipped dedicated-server
   configuration. Confirm the live servers run that. Also whether the new
   grenade gets its first physics step in the frame it is created or the
   next (tests 1–5 assume the next). **Answered (first step), open (frame
   rate):** the fire message is handled at the start of a server frame,
   before the clock advances and before the object updates; the new
   grenade is updated and moved **in that same frame**, but its start time
   is the previous frame's time. The step count is unchanged: 49 physics
   steps before a 2.5 s fuse ends at 20 Hz, so cases 1–5 keep their
   numbers with step 1 relabelled as the creation frame's step (Test cases
   common setup, section 5, Per-tick order, case 42). The live server rate
   still needs a measurement: drop a grenade from a known height and count
   the bounce frames, or read the server's frame time from a packet
   capture.
7. **Grenades vs characters and doors.** Section 6 says grenades pass
   through players and moving brushes, slowed to 65 %, because grenades are
   not solid. Verify: throw at a teammate's chest from 100 u, and at a
   closed door. **Answered from the reference, live check still useful:**
   solid world models (doors, moving brushes, solid breakables and
   `WorldModel`s) are collided like the level and **bounce** the grenade;
   characters are reported by their boxes (the character object and its
   hit-box object) and accepted only if the line of motion passes a hit
   sphere, each accepted touch scaling the velocity by 0.65 with no
   mirror, again on later steps while the grenade is still inside; solid
   props slow it the same way without the sphere test; other grenades in
   flight touch through their boxes, resting ones never, and
   `CanHitSameKind` only matters between grenades of the same thrower
   (section 6, cases 45–48). Still worth measuring: how many times a frag
   thrown through a standing player's chest from 100 u is slowed (expect
   1–3 factors of 0.65, visible as its landing distance).
8. **Shooting grenades.** Can a bullet hit a flying or resting grenade (it
   accepts ray hits and has 50/300 HP)? Shoot a resting XM84 twice with a
   rifle. **Answered:** yes, in flight and at rest (the ray-hit flag is
   never cleared); bullet damage comes off its hit points and at ≤ 1 HP it
   detonates exactly as at the end of its fuse, credited to its thrower.
   Applies in section 13 and case 58.
9. **Grenade half-height** for the at-rest nudge: the model's animation dims
   × the model scale 2. Read from the grenade's world model. **Answered:**
   the projectile's box is set from the world model's first animation
   (`Hand`) × 2 when the grenade is set up, and is not changed by the later
   `World` animation: half-extents M67 (6, 6, 8), XM84 (4, 10, 10), M18
   (6, 10, 10); half-heights 6, 10, 10. Applies in the Constants table
   (`grenade_half_height`, `grenade_box`), section 6 and cases 43–44.
10. **Smoke emission.** Does emission interval −1 mean one burst of 10
    sprites, or continuous emission for 2 s? Is the velocity box in world or
    effect axes, and is the friction 0.98 per frame or per second? Count
    sprites in a capture.
11. **Flash facing.** Is the server body rotation yaw-only (as assumed) or
    does it include pitch? Test: look straight up with a flash at your feet.
    **Answered:** yaw only. The client sends the server only the yaw of the
    player rotation (a compressed angle) and the server builds the body
    rotation from it alone, so f is horizontal and a blast straight above
    or below the centre counts as behind (f·u = 0). The fade factors W and
    C come from the held weapon, but no weapon record sets them, so every
    weapon gives W = 2.0 and C = 1.0. Applies in section 10 and cases
    53–54.
12. **Flash and HUD draw order.** Is the white drawn over the HUD? The HUD
    is re-shown every frame of the effect except the first. **Answered:**
    yes. Each frame the HUD is drawn first and the snapshot and white
    rectangle after it; only pop-up windows are drawn on top. Applies in
    section 10 and case 55.
13. **Throw perk force** and the drop-grenade perk's probability and weapon
    are in perk data not traced here.
14. **M67 knock-back** (`DamageMovePenalty` 135, 0.4 s) is applied only to
    infected players in the infection mode. Its effect belongs to the game
    modes spec.
15. **Contradiction with `inventory.md`, section 12.1.** That spec calls the
    death release "a grenade that was being cooked (pre-fire held)". The
    code has no cooking. The release depends on the server's third-person
    flag (on after the third-person pre-fire, while fire is held), gives a
    full fuse, and does not check ammo (section 14). Speed (500 u/s) and
    direction (facing) agree.
16. **Contradiction with `attribute_data.md`.** It lists `GrenadeSpeed`
    ("throw speed scale") and `GrenadeAttach` ("thrown object sticks") as
    read by grenades. They are parsed, but no game code uses them; the M67
    even has `GrenadeSpeed` 0. They should be marked unused there.
17. **Throw spread rule.** Implementers used n = n_min + ⌊(n_max −
    n_min)·p⌋·g with the limits not multiplied by g, so the flash's 0.06°
    survived g = 0. **Answered:** `weapon_firing.md`'s rule is the right
    one for throws too. The client multiplies both limits by g (truncating)
    before the formula and applies no spread when n_max is then 0, so the
    XM84 and M18 (g = 0) fly exactly along the aim. Stock grenades never
    have p > 0 in practice: no grenade ammo sets movement targets, the
    frag's only firing cap (`FireMoveDuckPerturb` 1) lets the meter grow
    only from a throw, a throw reads the meter from before its own growth,
    and the meter is back to 0 long before the next throw. So the M67
    always uses n = 1.
    Applies in section 4 and cases 39–40.
18. **Bounce push value.** The bounce formula had −0.1 n and the Constants
    table 0.05. **Answered:** both are right: 0.05 u/s is the engine's push
    per contact, and the grenade's bounce takes it twice, inside the
    hardness factor: v″ = h(v′ − 2(v′·n)n − 0.1 n), applied in the bounce
    step on the post-gravity velocity. Applies in section 6, the Constants
    table and case 41 (case 7 already uses it).
19. **Water volumes.** How a liquid volume is marked in a map and its
    default gravity, viscosity and current. **Answered:** class `Water`,
    `CorrosiveFluid` or `FreezingWater` (the only liquid classes), a world
    model named after its BSP; properties `Viscosity` (default 0.035,
    clamped to [0, 1]) and `Current` (default (0, 0, 0)); gravity is fixed
    at −200 u/s² with no property. Viscosity and current work in any
    volume class, only above 1 u/s, and the current only when viscosity
    is above 0. Stock maps mostly use 0.1 and (0, 1500, 0). Applies in
    section 7, the Constants table and case 49.
20. **Round end.** Does a grenade in flight or in hand explode, vanish or
    carry over? **Answered:** nothing in the round logic touches
    projectiles; they detonate on their fuse during the end-of-round phase
    with normal damage, and the next round respawns players into the same
    world, so anything still alive (notably 9 s fire and gas spheres)
    carries over. A grenade in hand is not released. Separately, any
    grenade detonating while a map camera is on is removed without effect.
    This corrects section 8's earlier "end of a round's object phase"
    trigger. Applies in sections 8 and 15 and cases 56–57. The end-of-round
    phase length on the live servers needs a measurement (time from the
    round result to the respawn).
21. **Cover traces.** What blocks the blast's line of sight, and whether
    breakables get a cover test. **Answered:** every physics-BSP polygon
    blocks, whatever its solid or invisible bits, and so do the polygons of
    solid world models (doors, moving brushes, solid breakables) in their
    current position; characters, hit boxes, props, grenades, volumes,
    non-solid world models and collision blockers do not. Only characters,
    hit boxes and projectiles get the cover test; breakables and other
    damageable objects take the blast by distance alone. Applies in
    section 9 and cases 50–51.
22. **Armour zone and flash gear.** **Answered:** for explode, flash, burn,
    poison and the other area and over-time damage types (and anything
    from a thrown weapon) the gear's **torso** entries are used, whatever
    node was hit last. Flash protection comes from gear with damage type
    34 entries; the full list with values is in section 10 (stock
    anti-flash goggles 1.0 / 0.7 / 0.5 / 0.3, masks 0.8, eyepatch 0.15, and
    others). Applies in sections 9–10 and case 52.
23. **Projectile `FireOffset`** (Corrections 1). The stock value for the
    M67, XM84, M18, M-23F and G7A projectile records was not read (no data
    on the reference machine). Read it from the install. Live check: on flat
    ground, throw level from a standing position and measure where the
    first bounce lands; compare with section 5's arc from the eye. A landing
    further by about the offset means it is positive.
