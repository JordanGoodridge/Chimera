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
`inventory.md`. Nothing was checked against a running game; see Open
questions.

Status: draft

Sections: Summary; Units and conventions; Constants; Data: one grenade of
each kind; Behavior (1 kinds, 2 carrying and starting counts, 3 pin pull and
"cooking", 4 the throw, 5 flight, 6 bouncing and coming to rest, 7 water,
8 detonation, 9 blast damage, 10 flash, 11 smoke, 12 fire and gas, 13 the
grenade as a target, 14 death release and the drop perk); Per-tick order;
Edge cases; Quirks; Test cases; Open questions.

---

## Summary

- A thrown grenade is a server object with a **fixed fuse that starts when
  it leaves the hand**. There is no cooking: holding the button does not
  shorten the fuse, and the first-person throw happens when the 700 ms
  pin-pull animation ends whether the button is still held or not. Frag
  2.5 s, flash 1.5 s, smoke 3.0 s, all from data.
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
| surface_push | 0.05 | u/s | Extra speed pushed into a surface by the engine's collision response |
| default_dampen | 0.35 | fraction | Console variable `GrenadeDampenPercent`: speed lost per bounce when the hit surface has no table entry (so × 0.65) |
| rest_speed | 50 | u/s | Console variable `GrenadeMinVelMag`: horizontal speed below which a floor hit stops the grenade |
| rest_floor_normal | 0.75 | — | A surface counts as floor when its normal's y > this (about 41° slope or flatter) |
| max_bounces | 0 (off) | bounces | Debug option; when ≥ 1 the grenade detonates after that many bounces. Off in shipped builds |
| spin_pitch | uniform [−π, π] | rad/s | Visual spin, re-rolled at every bounce |
| spin_yaw, spin_roll | uniform [−2π, 2π] | rad/s | Same |
| blast_full_fraction | 0.25 | × radius | Blast and flash are at full strength inside this fraction of the radius |
| big_object_slack | 100 | u | For objects whose dims vector is longer than this, the excess is subtracted from their blast distance (not players) |
| flash_delay | 0.25 | s | Console variable `FlashBangDelay`: time from blast to blindness, victim facing the blast |
| flash_delay_back | flash_delay / 3 | s | Same, victim facing away (0.0833 s) |
| flash_back_factor | ¼ | × | Flash time when facing away |
| flash_min_time | 0.1 | s | Shorter flashes are not sent to the client |
| white_decl_default | 2.0 | — | Weapon field `WhiteScreenDecl` default (no shipped weapon sets it) |
| captured_decl_default | 1.0 | — | Weapon field `CapturedDecl` default (no shipped weapon sets it) |
| death_release_speed | 500 | u/s | Speed of the live grenade released when its holder dies |
| liquid_gravity_default | −200 | u/s² | Gravity inside a water volume unless the level sets another |
| liquid_viscosity_scale | 5000 | u/s² | Speed loss per unit of a water volume's viscosity |
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
  and the auto-switch of `inventory.md` section 10 fires when the throw
  animation ends.

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
3. When the pre-fire ends, the main **throw** animation (67 ms for the M67)
   starts **whether or not the button is still held**. Releasing early does
   not cancel the throw, and holding does not delay it.
4. The **release** is the throw animation's fire event (key time k inside
   the 67 ms; see Open questions). At that moment the client builds the
   throw (section 4), uses the ammo and sends the fire message.
5. The 366 ms post-fire animation then plays. If the total is now 0 the
   auto-switch follows when it ends; otherwise a reload request refills
   the clip (see Open questions for its length).

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
  the origin the client sends.
- **Direction**: the camera's forward vector, perturbed by the weapon's
  spread exactly as for bullets (`weapon_firing.md`, Spread). With the M67's
  perturb of 1–10 thousandths the throw can deviate by up to about 0.6°;
  the flash and smoke (1/1, dynamic factor 0) by about 0.06°. No upward
  angle is added. Looking level throws level.
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
t_{release} = t_{update} + \frac{700\ \text{ms} + k}{\rho_{fire}} + \text{one-way latency} + \le 1\ \text{server frame}
```

  where t_update is the one or two client frames before the pre-fire starts.
  From a weapon switch, add the 400 ms draw.

### 5. Flight

Each server frame the engine integrates the grenade with the object gravity
g = (0, −1600, 0) (the data's `GravityOverride` is not used for thrown
weapons). With v the velocity at the start of the step:

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
  invisible (clip brushes): grenades fly through them.
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
   clamped to [0, 1]. The −0.1 n is the engine's surface push; it is
   negligible. Stock hardness: 0.65 for nearly every surface, 0.5 chain-link
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
last direction of motion by its own half-height × max(0.1, h) (so a grenade
that came down onto a floor lifts slightly off it; see Open questions for
the half-height). A grenade at rest is also announced to bots as a danger
(`bots.md`).

**Rolling**: there is no friction for grenades. A grenade with little
vertical speed hits the floor every step, and each hit multiplies its whole
velocity by h. Its horizontal speed therefore decays by 0.65 per bounce
until it falls under 50 u/s.

**Objects** (players, doors and other moving brushes, other grenades):
grenades are not solid, so the engine reports a touch but does not block
them. On such a touch the grenade is snapped to the contact point found by a
short trace and its velocity is scaled by the touched surface's hardness
(0.65 for flesh) with **no mirror** (the engine gives no push-back). It
then carries on through the object. A character only counts when the
grenade's point passes through one of the character's hit spheres. Frags
(`CanHitSameKind` 1) touch other grenades of their thrower; flash and smoke
(0) pass through them. This behaviour needs live checking (Open questions).

### 7. Water

- A liquid volume replaces the grenade's gravity with the volume's own
  (−200 u/s² unless the level sets another) for every step it is inside.
- If the volume has a viscosity c in [0, 1], each step first removes
  5000·c·dt of speed along the direction of motion (to zero if that is
  more than the speed), then adds the volume's current × dt.
- A water surface that is solid world bounces the grenade with hardness 0.1.
- Bounce sounds play at half volume in water. Nothing else changes: the
  fuse runs, the rest test is the same and the blast is not reduced.

### 8. Detonation

- **When**: in the first server frame whose time is at least the creation
  time + `Lifetime`, in that frame's object update, before that frame's
  physics. With dt = 0.05 and a 2.5 s fuse, the grenade gets 49 physics
  steps and goes off at the start of step 50. A `Lifetime` ≤ 0 detonates
  at creation.
- Grenades do **not** go off on impact. Other triggers: being destroyed by
  damage (section 13), the debug bounce cap, and the end of a round's
  object phase.
- **Where**: the grenade's position, plus 0.1 u back along its motion if it
  touched something in the previous step. In mid-air, the surface type is
  "air".
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
- **Cover**: for characters, hitboxes and projectiles (not for the
  character a grenade struck directly, which never happens with bouncing
  grenades), two traces are cast, blast → object centre and object centre
  → blast. Only solid world geometry and moving brushes block; characters,
  props and invisible surfaces do not. If either trace is blocked and the
  object is a character, both traces are tried again to the character's
  head-top point (centre x and z, y = the top of its highest head sphere).
  If either trace is still blocked, there is no damage. So partial cover
  that leaves the head visible does not protect.
- **Falloff**:

```math
r_0 = 0.25R,\qquad m(d) = \begin{cases} 1 & d \le r_0 \\ 1 - \dfrac{d - r_0}{R - r_0} & r_0 < d \le R \end{cases},\qquad D = \texttt{AreaDamage}\cdot m(d)
```

  For the M67: full 60 within 112.5 u, 0 at 450 u, linear between. There
  is no random roll, no range table, no hit zone (zone factor 1, armour
  checks torso, `weapon_firing.md`) and no rear bonus.
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
- **Protection**: the sum of the victim's gear torso entries for the flash
  type, clamped to [0, 1], removes that fraction:
  D′ = D·(1 − clamp(P, 0, 1)). Armour points play no part. Stock anti-flash
  goggles protect 1.0 (fully), other models 0.3, 0.5 or 0.7. If
  D′ ≤ 0.001, nothing happens.
- **Angle**: let f be the victim's body facing (server-side rotation) and
  u = victim centre − blast point.

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
  (Quirk 6), 2.0 for every stock weapon. With W = 2:

```math
w(t) = \cos\frac{\pi t}{T}\quad (0 \le t < T/2), \qquad w = 0\ \text{and the effect ends at}\ t = T/2
```

  Each frame draws a snapshot of the screen taken on the effect's second
  frame with alpha w·C (C = `CapturedDecl` of the held weapon, 1.0 stock),
  then a full-screen white rectangle with alpha w. The HUD is hidden for
  the effect's first frame only. The effect also stops if the victim dies
  (unless the death camera is the first-person chase view) or leaves the
  game.
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
- Whether bullets can hit a flying grenade was not traced (Open questions).

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

## Per-tick order

Server, one frame (dt = 1/20 s):

1. Client messages are handled as they arrive. A fire message from a
   thrower creates the grenade at the sent origin, sets its velocity,
   records its start time and tells clients to draw it. A death runs
   section 14.
2. Every object is updated in turn. For a grenade:
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
  14), even if the first-person throw has already happened.
- **Switching away mid-throw.** Before the release event: no grenade, ammo
  unchanged, no reload needed. After it: the grenade is already flying and
  the ammo is used. The switch also clears the server's pin-pulled flag.
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
6. **Flash curve from the victim's weapon.** The fade constants are read
   from whatever weapon the victim holds. All stock weapons use the
   defaults, so the effect is the same. Keep the defaults; reading the flash
   grenade's own values is acceptable.
7. **Flash is half its time.** With the default constant the white is gone
   at T/2. Keep.
8. **Facing away is a quarter, and sooner.** The code calls it "half", but
   it is a quarter of the time, with a third of the delay. Keep.
9. **Grenades pass through clip brushes and characters** (sections 5, 6).
   Keep; verify the character part.
10. **Smoke blocks nothing.** Bots and name tags see through smoke. Keep.

## Test cases

Common setup unless stated: server dt = 0.05 s; flat floor y = 0 with
hardness 0.65 (normal (0, 1, 0)); no water; stock data; positions are the
grenade's position at the end of the numbered physics step, with step 1
being the first server frame after creation; values ±0.05 u, ±0.05 u/s;
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
    server, not 2.5 s after the press.
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
   They need the view-model animation keys, which were not readable here.
   High-speed capture: press to grenade visible.
4. **Hold behaviour.** The third-person state holds the pose while fire is
   held, but the first-person code throws at the end of the pre-fire. Does
   holding fire on the live build keep the grenade in hand? If it does, the
   release would move to the button release and test 27 changes.
5. **Second grenade on re-ready.** With two flashes or smokes, after the
   first throw the clip is 0 and the total 1. Does the reload request play a
   re-draw animation, and how long until the second can be thrown?
6. **Server frame rate.** 20 Hz comes from the shipped dedicated-server
   configuration. Confirm the live servers run that. Also whether the new
   grenade gets its first physics step in the frame it is created or the
   next (tests 1–5 assume the next).
7. **Grenades vs characters and doors.** Section 6 says grenades pass
   through players and moving brushes, slowed to 65 %, because grenades are
   not solid. Verify: throw at a teammate's chest from 100 u, and at a
   closed door.
8. **Shooting grenades.** Can a bullet hit a flying or resting grenade (it
   accepts ray hits and has 50/300 HP)? Shoot a resting XM84 twice with a
   rifle.
9. **Grenade half-height** for the at-rest nudge: the model's animation dims
   × the model scale 2. Read from the grenade's world model.
10. **Smoke emission.** Does emission interval −1 mean one burst of 10
    sprites, or continuous emission for 2 s? Is the velocity box in world or
    effect axes, and is the friction 0.98 per frame or per second? Count
    sprites in a capture.
11. **Flash facing.** Is the server body rotation yaw-only (as assumed) or
    does it include pitch? Test: look straight up with a flash at your feet.
12. **Flash and HUD draw order.** Is the white drawn over the HUD? The HUD
    is re-shown every frame of the effect except the first.
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
