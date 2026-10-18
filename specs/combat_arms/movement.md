# Combat Arms: movement

Source basis: the client's local player movement manager and the game's
movement-penalty rules on top of it; the engine's shared motion integrator,
player collision mover and fixed-rate movement clock; the server's player
object (fall damage, crouch-spam configuration as shipped with a dedicated
server); the HUD stamina gauge and the player stats it drives; the game's
character (figure type, vest, head piece) and weapon attribute data.
Status: draft

Sections: Summary; Units and conventions; Constants; Behavior (tick clock,
input to control state, speed limit, acceleration, integration and friction,
stopping, jumping, gravity and air movement, crouching, crouch-spam penalty,
sprint and stamina, slopes and steps, falling and fall damage, other
modifiers); Per-tick order; Edge cases; Quirks; Test cases; Open questions.

---

## Summary

Combat Arms movement is the LithTech "player physics" model: each fixed
15 ms tick the game clamps horizontal velocity to a stance speed limit,
sets an acceleration from the held keys (ramped up from a small start
value), and hands both to the engine, which integrates them in closed form
with exponential ground friction and adds gravity in the air. Because the
clamp happens *before* integration, the real ground speed settles a little
above the nominal limit (about 296.5 u/s for a 285 u/s run). Speeds are
scaled down by a combined penalty from the held weapon and worn gear.
Sprint multiplies the run speed and drains a stamina gauge; crouching blends
the speed limit and the hull height over about 150 ms and is rate-limited by
a "crouch spam" penalty. Air control is strong (full ground acceleration,
no friction), and a short-lived jump factor weakens back-to-back jumps.

## Units and conventions

- Distance: LithTech world units ("u"). No conversion is applied anywhere in
  this spec.
- Axes: +Y is up. Horizontal plane is X/Z. Gravity points along -Y.
- Player hull: an axis-aligned box given by its **half**-extents
  (half-width X, half-height Y, half-depth Z) around the object's position,
  which is the hull's centre. The collision mover treats it as an upright
  cylinder of radius `max(halfX, halfZ)` and half-height `halfY`.
- Angles: radians. Movement uses the player's horizontal facing (yaw);
  pitch is ignored for walking.
- Time: the movement tick is 15 ms (`dt = 0.015 s`), about 66.67 Hz.
  Velocities are u/s, accelerations u/s².
- "Tick n" counts from 1 for the first tick a condition holds. Values
  quoted "after tick n" are the object's state at the end of that tick.

## Constants

| Name (ours) | Value | Unit | Meaning |
|---|---|---|---|
| tick | 15 | ms | Fixed movement step (engine default movement rate 1000/15 Hz, step truncated to whole ms) |
| max ticks per frame | 6 | ticks | `floor(100 ms / tick)`; extra due ticks are dropped |
| run speed forward / back / strafe | 285 / 285 / 285 | u/s | Console variables `FRunVel`, `BRunVel`, `SRunVel` |
| walk speed | 70 | u/s | Console variable `WalkVel`; also used when zoomed |
| crouch speed | 50 | u/s | Console variable `DuckVel` |
| jump speed | 330 | u/s | Console variable `JumpVel`; vertical velocity set on jump |
| ladder speed | 200 | u/s | Speed limit on a ladder, also upward velocity cap |
| player gravity | -1100 | u/s² | Along Y; console variable `PlayerGravity` |
| ground friction coefficient k | 5 | 1/s | `5 × min(friction of containers touched)`, normally 5 |
| base move accel | 3000 | u/s² | Console variable `BaseMoveAccel` |
| start accel | 500 | u/s² | Console variable `StartAccel` |
| max accel | 3000 | u/s² | Console variable `MaxAccel`; also the cap on combined accel |
| accel ramp rate | 6000 | u/s³ | Console variable `AccelInc` (90 u/s² per tick) |
| crouch accel factor | 0.5 | × | Base accel halved while the crouch flag is set |
| in-air accel factor | 0.1 | × | Console variable `InAirAccelMultiplier`; see Quirks for when it actually applies |
| slide-to-stop time | 0.1 | s | Console variable `SlideToStopTime`; key-release braking |
| stop snap speed | 5 | u/s | Below this horizontal speed, braking zeroes velocity |
| engine rest threshold | 0.1 | u²/s² | `|v|²` and `|a|²` below this count as zero |
| jump factor after a jump | 0.3 | × | Set on every jump |
| jump factor recovery | 1.2 | 1/s | Added per second while not jumping, until ≥ 1 |
| jump stance tolerance | 0.5 | u | Jump needs current half-height ≥ standing half-height - 0.5 |
| walkable normal | 0.7071 | normal.y | Ground steeper than this (normal.y < 0.7071) is not ground |
| stair step height | 30 (+0.5) | u | Server console variable `StairStepHeight`; mover uses 30.5 |
| standing probe | 0.02 × halfY | u | Distance below the hull that still counts as standing (1 u at halfY 50) |
| fall-probe ray | 150 | u | Downward ray for "falling" status (sound/animation only) |
| stand half-extents (fallback) | 15, 50, 15 | u | Used only if the model's stand animation has no box; normally from the model |
| crouch half-extents (fallback) | 15, 40, 15 | u | Same, from the model's crouch animation |
| crouch duration | 13/75 ≈ 0.17333 | s | Crouch speed `1/duration = 75/13 ≈ 5.76923` per s |
| crouch curve tangents | 3, 1 | | Start and end slope of the crouch rate curve |
| crouch release delay | 100 | ms (wall clock) | Crouch stays on until this long after it was pressed |
| fall damage start height | 300 | u | Drops shorter than this do no damage |
| fall damage full height | 700 | u | Drop that does 200 damage |
| fall damage at full height | 200 | HP | Linear, not clamped above |
| fall minimum | 50 | u | Second, redundant lower bound |
| pusher air factor | 0.1 | × | Explosion push in the air is a tenth |
| pusher max upward speed | 100 | u/s | Console variable `PusherMaxYVelocity` |

Character and weapon data (attribute files; field names as stored):

| Data field | Where | Typical value | Meaning |
|---|---|---|---|
| `DashSpeed` | figure type | 1.0 (1.4 if absent) | Sprint multiplier on run speed |
| `DashSpeed` | vest model, head piece, goggles, backpack, gear mods | vest 0.4, others 0 | Added to `1 +` sum, multiplies sprint speed |
| `DashGauge` | figure type | 30 | Stamina drained per second while sprinting |
| `GaugeRec` | figure type | 25 | Stamina recovered per second |
| `Maxstamina` | figure type | 100 | Stamina capacity before gear bonuses |
| `MaxStamina` | vest, head piece, goggles, backpack, set items, gear mods | 0 (some vests 2.0) | Added to `1 +` sum, multiplies capacity |
| `MovePenalty` | weapon | 0 (knife) to 0.33 (LMG); e.g. pistol 0.03, M4A1 0.10, AK-47 0.13, MK.48 0.30 | Fraction of speed lost while held |
| `MovePenalty` | vest model, head piece, goggles, backpack, gear mods | default vest 0.1 | Added into the same penalty |
| `VariableSpeed`, `VariableWeight`, `VariableJump` | figure type | 0 | Speed bonus, weight penalty, jump bonus |

Crouch-spam penalty parameters (the server sends them to the client on
join; the client's built-in defaults have both penalties switched off). Values
are the dedicated server's shipped configuration; the server's own defaults
before reading it are the client defaults with both penalties switched on.

| Server config key | Shipped value | Client default | Meaning |
|---|---|---|---|
| `CrouchingDuration` | 0.1733333 | 13/75 | Crouch transition time |
| `CrouchingTangent0` / `1` | 3.0 / 1.0 | 3 / 1 | Crouch curve tangents |
| `EnableSlowdownPenalty` | 1 | 0 | Slowdown stamina on/off |
| `SlowdownSP_IncreaseDuration` | 1.7 | 2.0 | s to refill slowdown stamina |
| `SlowdownSP_IncreaseTangent0` / `1` | 0.5 / 3.0 | 0.5 / 3.0 | Refill curve |
| `SlowdownSP_DecreaseAmount` | 0.5 | 0.1225 | Cost per crouch press |
| `MaxSlowdownRate` | 5.0 | 5.0 | Crouch transition at most 5× slower |
| `MinSlowdownSP` | 0.875 | 0.875 | Slowdown starts below this |
| `EnableDisableCrouchingPenalty` | 1 | 0 | Lock-out stamina on/off |
| `DisableSP_IncreaseDuration` | 1.5 | 1.0 | s to refill lock-out stamina |
| `DisableSP_IncreaseTangent0` / `1` | 1.0 / 1.0 | 1 / 1 | Refill curve (constant rate) |
| `DisableSP_DecreaseAmount` | 0.4 | 0.5 | Cost per crouch press |
| `DisableSP_*_InPenalty` | 2 / 1.0 / 1.0 / 0.9 | same as above | Refill time/tangents and cost while locked out (only the cost is used, see Quirks) |
| `*_DecreaseDuringJumpingCrouch` | 1 | 0 | Presses in the air also cost |
| `*_DecreaseByKeyStroke` | 1 | 0 | Cost is charged per key press, not per crouch start |
| `*_DecreaseException_NoMovement` | 1 | 0 | Presses while standing still are free |
| `NoMovementTimeLimit` | 0.2 | 0.2 | s of history for "standing still" |
| `NoMovementDevationLimit` | 0.1 | 0.1 | u moved in that window below which you count as still |

## Behavior

### Tick clock

- Movement runs on a fixed clock. An accumulator adds real elapsed time
  each rendered frame; every whole 15 ms in it is one movement tick. At most
  6 ticks run per rendered frame; ticks beyond that are discarded (time is
  lost, the game slows down rather than catching up).
- Each tick uses `dt = 0.015 s` exactly. The rendered player position is
  interpolated between the last two tick positions by the leftover
  accumulator fraction.
- If the movement rate is set to 1 Hz or less, the clock is off and one tick
  per frame runs with `dt` = frame time clamped to 0–100 ms.
- Stamina, the crouch release delay and some sound timers run on the
  wall clock per rendered frame, not per tick (see the sections below).

### Input to control state

Each tick the held commands become a set of flags. If the game has
no focus for movement (menus, shop open, external camera), all flags clear
except crouch, and sprint ends.

1. **Crouch.** Set while the crouch key is held (the "crouch toggle" binding
   behaves as hold). When the key is released, crouch stays set if less than
   100 ms of wall-clock time has passed since it was pressed, or if the
   standing hull does not fit (a test resize to standing size fails). The
   raw "crouch key pressed" state for the spam penalty is taken here. Then,
   if the lock-out penalty is active and the crouch rate is 0, crouch is
   cleared.
2. **Run.** Only when crouch is not set: run is set when `walk key held XOR
   always-run`. Always-run is on by default, so run is the default state and
   the walk key turns it off.
3. **Jump.** Only when crouch is not set, the player is on ground, not on a
   ladder and not already in a jump: if the jump key is held and no jump has
   been "used" since the key was last up on the ground, the jump is used,
   and the jump flag is set if the hull's current half-height is at least
   standing half-height - 0.5. If the jump key is up on such a tick, the
   "used" mark clears. A held key therefore jumps once; it must be up for at
   least one grounded tick before the next jump.
4. If the crouch rate is not 0 (still crouched or standing up), jump and
   sprint are cleared.
5. **Forward.** If held: forward flag. Then sprint is decided (see Sprint).
   If forward is not held, sprint ends.
6. **Back, left, right** flags from their keys. Holding back also ends
   sprint.
7. **Moving** flag: set if any of forward, back, left, right or jump is set.
8. **Jump request:** if the jump flag is set now but was not last tick, and
   the player is not in a jump and jumping is allowed, a jump is requested.
9. While a hit-stagger timer runs (only infected characters in the infection
   mode), run and sprint are cleared.

### Speed limit

The horizontal speed limit `V` for the tick, from the flags after step 9:

- On a ladder: 200.
- Else, with `K` = the last direction speed (forward key held: forward run
  speed; else back held: back run speed; else a strafe key held: strafe run
  speed; else unchanged from before; all 285), and `r` the crouch rate after
  this tick's crouch update:
  - "crouching" means the crouch flag is set or `r = 1`.
  - Sprinting (sprint flag set **and** the server has confirmed sprint),
    not crouching, not zoomed:

```math
V_0 = K \cdot D_{fig} \cdot \left(1 + D_{head} + D_{goggle} + D_{vest} + D_{pack} + D_{gear}\right)
```

  - Else running (run flag), not sprinting, not crouching, not zoomed:
    `V_0 = K`.
  - Else (walking, crouching, zoomed): `V_0 = WalkVel = 70`.
  - Then blend toward crouch speed by the crouch rate:

```math
V_1 = \max(V_0, 50) + (50 - \max(V_0, 50)) \cdot r
```

- Then the penalty:

```math
P = \mathrm{clamp}\left(P_{weapon}(1 + c_{mod}) + P_{vest} + P_{head} + P_{goggle} + P_{pack} + P_{gear} + w_{fig} - s_{fig} + P_{custom} + P_{stun},\ -1,\ 1\right)
```

  where `c_mod` is the weapon modification's own penalty factor (normally 0),
  `w_fig`/`s_fig` the figure's weight and speed values (0), `P_stun` a
  temporary stun penalty from certain ammo. A VIP account uses `P = P_vest`
  only, and its sprint formula drops `D_fig` and every bonus except the vest's.

```math
V = V_1 \cdot (1 - P + S_{set}) \cdot f_{slow} \cdot f_{container}
```

  `S_set` is a set-item speed bonus (0), `f_slow` a temporary slow from
  damage (1 when inactive), `f_container` the product of speed factors of
  volumes the player is in (1).

The reference character (figure DashSpeed 1.0, default vest MovePenalty 0.1
and DashSpeed 0.4, no other gear) holding an M4A1-class rifle (0.1) has
`P = 0.2`: run 228, sprint 319.2, walk 56, crouch 40 u/s.

At the start of the motion step, if the horizontal velocity magnitude
exceeds `V`, it is scaled down to exactly `V` (direction kept, vertical
untouched). There is no other speed cap; see Integration for why the
measured speed ends up higher.

### Acceleration

Basis: forward `F` = the player's facing with its vertical part removed and
renormalized; right `R` = the facing's right vector with vertical part
removed.

- Base magnitude `A = 3000`; halved to 1500 if the crouch flag is set (not
  on ladders); then × 0.1 if in a jump.
- If the tick is the jump-request tick or the crouch flag is set, the
  directional accelerations are simply `A·F` and `A·R`.
- Otherwise a per-axis ramp replaces their magnitude. Two remembered values,
  `L_f` (forward/back) and `L_r` (strafe), start at 500:
  - On the tick the moving flag turns on, `L_f = L_r = 500` and the whole
    result is later rescaled to 500 (see below).
  - Forward/back axis: if the key direction reversed since last tick
    (forward now, back last tick, or the opposite), the forward accel is
    500 and `L_f = 500`. Else, if forward or back is held (and the moving
    flag did not just turn on): if `L_f + 6000·dt < 3000`, the magnitude is
    `L_f + 6000·dt` and `L_f` takes that value; otherwise 3000 and
    `L_f = 3000`.
  - Strafe axis: same with left/right and `L_r` (this axis ramps even on the
    tick the moving flag turns on, so `L_r` is 590 after that tick).
  - An axis with neither key held resets its `L` to 500.
- Sum: `a = (+F·a_f if forward) (−F·a_f if back)`; then each held strafe
  key adds `±R·a_r` and, after each addition, clamps `|a|` to 3000.
- If the moving flag just turned on and `a ≠ 0`, rescale `a` to magnitude
  500.
- If the jump factor `j < 1` (see Jumping), `a ← a·j`.
- Holding opposing keys gives `a = 0` on that axis but still counts as
  moving.

### Integration and ground friction

The engine integrates the object's velocity `v` and acceleration `a` over
`dt`, with gravity `g = (0, -1100, 0)`:

- First, a velocity with `|v|² < 0.1` is zeroed, and the same for `a`.
- **Standing on something**, and both are below threshold: the object
  stops (velocity and acceleration zero, no move).
- **Standing on ground that is too steep** (`n.y < 0.7071` for the surface
  normal `n`): the acceleration including gravity is projected onto the
  plane, any up-slope part along gravity removed, and rescaled to its
  original magnitude; no friction. The player slides.
- **Standing on walkable ground:** let `v' = v + g·dt`.
  - If `v'·n > 0.01` (moving off the surface, e.g. the jump tick), gravity
    is added to `a` and there is no friction.
  - Else if `a·n > 0.01` (accelerating out of the surface, e.g. walking
    downhill), `a` is pushed into the surface, `a ← a − n(n·a + 1)`, rescaled
    to its original magnitude, with friction.
  - Else (normal walking) `a` is used as is (no gravity), with friction.
- **Not standing:** `a ← a + g`, no friction.

With friction (coefficient `k = 5`), exact closed form:

```math
v_{new} = \frac{a}{k} + \left(v - \frac{a}{k}\right) e^{-k\,dt}
```

```math
\Delta x = \frac{a}{k}\,dt + \frac{(k v - a)\left(1 - e^{-k\,dt}\right)}{k^2}
```

With `dt = 0.015`, `e^{-k dt} = 0.9277434863`.

Without friction:

```math
v_{new} = v + a\,dt, \qquad \Delta x = v\,dt + \tfrac{1}{4} a\,dt^2
```

(the displacement uses a quarter, not a half; see Quirks).

The displacement is then moved through the world by the collision mover
(slide along up to 5 planes, stair-step, see Slopes and steps). After the
move:

- If the attempted move had both horizontal components above 0.01, the
  player is not on ground and the position did not change at all, the
  horizontal velocity is zeroed (stuck against a wall in the air).
- Otherwise, if the attempted vertical move was above 0.01 in size but the
  actual vertical change was below 0.01, the vertical velocity is zeroed
  (landing, hitting a ceiling, or resting on a slope you would slide on).

Because the speed clamp is applied before this integration, a player
holding a direction ends each tick slightly above `V` and is clamped back
next tick. The steady state per tick, with `v = V` and `|a| = 3000` (1500
crouched):

```math
v_{end} = 600 + (V - 600)\,e^{-0.075}, \qquad \Delta x = 9 + \frac{(5V - 3000)(1 - e^{-0.075})}{25}
```

Run 285 → 4.44784 u per tick, 296.52 u/s. Walk 70 → 89.39 u/s. Crouch 50
(accel 1500) → 59.14 u/s.

### Stopping

When the moving flag is off, the player is on ground, not in a jump, and in
no current or viscous volume, braking runs after the motion step (before
integration) and replaces the velocity:

- Let `h` = horizontal velocity. If `|h| > 5`: reduce its magnitude by
  `dt · K / 0.1` (42.75 u/s per tick at `K = 285`); if that would reach or
  pass zero, the whole velocity becomes zero. Vertical velocity is kept.
- If `|h| ≤ 5`: the whole velocity, vertical included, becomes zero.

The engine friction then also applies (acceleration is 0, so the velocity
decays by `e^{-k dt}` and moves `v(1 - e^{-k dt})/k`). From a steady run the
player stops in 6 ticks over 9.4034 u (see Test cases).

Braking is skipped while the infection-mode stagger timer runs.

### Jumping

- On the jump-request tick, if the player is not dead, on ground and not on
  a ladder: vertical velocity is set (not added) to

```math
v_y = 330 \cdot (1 + J_{fig} + J_{model}) \cdot j
```

  where `j` is the jump factor at the start of the tick and `J` are the
  figure's and vest's jump bonuses (0). Horizontal velocity is kept. The
  player is now "in a jump"; the request clears. A jump pad volume instead
  adds its own launch velocity to the whole velocity.
- **Jump factor `j`.** Starts at 1. On a jump tick, `j` becomes 0.3 (if it was
  already below 0.3 it decreases by `0.05·dt` instead, which cannot happen in
  practice). On every other tick, if `j < 1` it increases by `1.2·dt`
  (0.018), else it is set to 1. It can overshoot to 1.002 for one tick. It
  scales the horizontal acceleration whenever below 1, and the next jump's
  speed. After a jump it is back to 1 after 40 ticks (0.6 s), just before a
  flat-ground landing.
- **Landing.** At the start of each tick's motion step the game asks the
  engine what the player stands on (from last tick's move). If standing on
  something and the player was in a jump, the jump ends (landing). Ground
  is then denied if the surface normal is steeper than 0.7071 or the thing
  stood on is another character. While in a jump the player is never
  "on ground" for movement purposes.
- **Repeat delay.** There is no timer. A held key jumps once; a new jump
  needs one grounded tick with the key up, then a press. With the landing
  detected one tick after contact, the earliest re-jump is on the third
  tick after the contact tick.
- Jumping needs the standing hull: no jump while the crouch flag is set,
  while the crouch rate is not 0, or while the hull is more than 0.5 u
  below standing half-height.
- Jump stamina: a custom-room option can make jumps cost stamina; in normal
  play it is 0 and jumps are free (see Quirks for the client's handling).

### Gravity and air movement

- Gravity is -1100 u/s² on the player (other objects use -1600). Gravity
  volumes replace it with the sum of their gravities.
- In the air there is no friction and no braking; horizontal velocity is
  kept when no key is held.
- The speed clamp still applies every tick, using the current stance's
  limit (releasing sprint in the air drops the cap at once).
- Air acceleration is the same ramped value as on the ground (up to 3000),
  times `j`. The 0.1 in-air factor only survives when the ramp is bypassed:
  crouching in the air after a jump (1500 × 0.1 = 150 u/s²). A player who
  crouches during a jump has almost no air control. (Walking off a ledge is
  not "in a jump": crouched air accel is then 1500.)
- A flat-ground jump from rest rises to 50.7375 u above the start on
  tick 20 and touches down on tick 41 (see Test cases).

### Crouching

- **Crouch rate** `r ∈ [0, 1]` (0 standing, 1 fully crouched), updated once
  per tick from the crouch flag, before the motion step. Transition speed
  `c = 1/duration` (75/13 per second), possibly slowed by the spam penalty.
  With tangents `T0 = 3`, `T1 = 1`, the shape function is

```math
s(t) = (3t^2 - 4t + 1)\,T_0 + (-6t^2 + 6t) + (3t^2 - 2t)\,T_1 = 6t^2 - 8t + 3
```

  - Crouch flag set and `c > 0`: on the first tick of going down the
    elapsed time `e` is 0, then it grows by 15 ms per tick. `t = e·c` (e in
    seconds), `r ← r + s(t)·dt·c`; if `r ≥ 1 - 10⁻⁶`, `r = 1`.
  - Else if `r > 0`: going up, same with its own elapsed time,
    `r ← r - s(t)·dt·c'` where `c' = c`, or 75/13 if `c` is 0 (the lock-out
    stops crouching down but you still stand up at normal speed); if
    `r ≤ 10⁻⁶`, `r = 0`.
  - Else: idle, both elapsed times reset.
  - On a ladder, while dead, or in free-movement volumes (water), `r` is
    reset to 0 directly.
  - `t` is not clamped. Full crouch and full stand each take 10 ticks
    (150 ms) at full speed.
- **Hull height.**

```math
h_y = H_{stand} - r\,(H_{stand} - H_{crouch})
```

  on the half-height, other half-extents stay at standing size. Applied twice
  per tick (before the motion step and again before integration). If the
  resize fails (no room), the old size and position are restored and then
  the new size is forced without collision. When the hull shrinks and the
  player was standing on something before the resize, the player is moved
  down by the shrink amount so the feet stay put. When it shrinks in the
  air, the centre stays and the feet rise.
- **Crouch-jump:** jump first (standing), then crouch in the air. The feet
  rise by `H_stand - H_crouch` (10 u with the fallback boxes) over the
  10-tick crouch, which lets the player clear higher edges. Standing back up
  in the air grows the hull around the centre again.
- **Speed and accel while crouched:** limit as in Speed limit; base accel
  halved and no ramp. Run and jump flags are not set while crouched; sprint
  ends.

### Crouch-spam penalty

Two "crouch stamina" values in [0, 1], both starting at 1: **slowdown**
(slows the crouch transition) and **lock-out** (forbids crouching). Updated
every tick after the crouch rate, with:

- `Δr` = this tick's change of crouch rate;
- a **crouch start** happens when `|Δr| > 10⁻⁶`, `Δr > 0`, and the last
  non-zero change was not upward; it resets a timer `e_c` to 0, otherwise
  `e_c` grows by 15 ms per tick;
- a **key press** is the raw crouch state turning on this tick;
- **still**: the sum of per-tick horizontal distances moved over the last
  0.2 s (ticks no more than 200 ms older than the newest) is below 0.1 u;
  this uses positions up to the previous tick.

Slowdown (if enabled):
- If no crouch start, no key press and `|Δr| ≤ 10⁻⁶` (fully up or fully down
  and resting): refill by `s_s(e_c/1.7) · dt / 1.7`, with the shape
  `s_s(t) = 4.5t² - 2t + 0.5` (tangents 0.5 and 3); full refill from 0 takes
  1.7 s.
- Else, if (not still, or the still exception is off) and (on ground, or
  air presses count): subtract 0.5 on a key press (or on a crouch start when
  "by keystroke" is off).
- Clamp to [0, 1]. If disabled, it is held at 1.

Lock-out (if enabled), with `L` = whether lock-out was active at the start
of the tick:
- If no crouch start, no key press, and on ground: refill by `dt / 1.5`
  (constant shape 1, tangents 1 and 1; 0.01 per tick). If it reaches 1,
  lock-out ends.
- Else, if (not still, or the *slowdown's* still exception is off) and (on
  ground, or air presses count): subtract 0.4 (0.9 if `L`) on a key press
  (or crouch start, as above). If it reaches 0 or below, lock-out begins.
- Clamp to [0, 1]. If disabled, it is held at 1 and lock-out is off.
- No refill happens in the air.

Effect on crouch speed `c`:
- Lock-out active (and enabled): `c = 0`, so the crouch rate can only
  return to 0, and once it is 0 the crouch flag is cleared each tick. The
  HUD shows a "crouch prohibited" marker.
- Else if slowdown is enabled and `MinSlowdownSP` is not ~0 and the slowdown
  value `S ≤ 0.875`:

```math
c = \frac{c_0}{5 + (1 - 5)\cdot \frac{\mathrm{clamp}(S, 0, 0.875)}{0.875}}
```

  (5× slower at `S = 0`, normal at `S = 0.875`). Else `c = c_0`.

With the shipped values, three crouch presses about 0.2 s apart while
moving lock crouching out for 1.5 s.

### Sprint and stamina

Sprint (the game's "dash") is decided on ticks where forward is held and
movement is allowed:

- A kill-streak perk that grants free sprint instead forces sprint speed
  whenever running, and the sprint flag itself stays off.
- "Can sprint": the run flag is set (walk key not held, not crouched), not
  on a ladder, crouch rate is 0, not carrying a mode flag (always false in
  this build), not snowball-frozen, and (in one build variant) not mid-swing
  with a knife.
- If the sprint key is held, stamina as an integer is at least 1, and can
  sprint: the sprint flag is set, the stamina state is **draining**.
- Else, if the sprint key is held and can sprint (so stamina < 1): the
  sprint flag is off and the stamina state is **stopped**.
- Else: the sprint flag is off and the stamina state is **recovering** (or
  draining at a perk rate if a perk drains stamina passively).
- Not holding forward, or holding back, ends sprint (state recovering).
- Sprint speed takes effect only while the server confirms the local
  player is sprinting; the server ends sprint itself when its own copy of
  the gauge runs out.

Stamina gauge, updated once per rendered frame on the wall clock, with `Δt`
the milliseconds since the previous update:

- Capacity:

```math
S_{max} = \left\lfloor \mathrm{Maxstamina}_{fig} \cdot \left(1 + M_{head} + M_{goggle} + M_{vest} + M_{pack} + M_{set} + M_{gear}\right) \right\rfloor
```

  (stored as an integer; VIP accounts use the figure value alone).
- Draining: `S ← max(0, S - DashGauge·Δt/1000)` (30/s).
- Recovering: `S ← min(S_max, S + GaugeRec·Δt/1000)` (25/s).
- Stopped: `S ← 0` (pinned at zero while the key stays held), except during
  a stamina-kit effect.
- A stamina kit (damage of the stamina type) sets `S = S_max`.
- Spawning and round resets set `S = S_max`.

So a full 100-point gauge gives 3.3 s of sprint (it ends when it drops
below 1) and refills from 0 in 4 s once the sprint key is released.

### Slopes and steps

- Ground is walkable when the surface normal has `n.y ≥ 0.7071` (45°).
  Steeper surfaces are not ground: no jumping, no braking, and the
  integrator slides the player down them.
- The player stands on whatever the hull touches within `0.02·halfY` below
  it (1 u for a 50 u half-height). Surfaces with `n.y < 0.001` are never
  stood on.
- **Stair stepping** is tried on any move with a horizontal part, on the
  ground or in the air:
  1. Slide toward the target at the current height (along the floor
     plane when standing). If nothing was hit, or what was hit is not a
     step, keep that result.
  2. Otherwise lift by 30.5 u (if blocked, keep the low result), slide
     toward the target at that height, then drop back down by 30.5 u.
  3. If the surface found on the way down is steep, the step is refused
     unless a probe further forward (half the hull radius, or
     `2·30.5·(1 - n_y²) + 0.1` if larger) finds a non-steep surface.
  4. Take the high result if it got further along the move than 1.01 × the
     low result (or, if both ended at the same X/Z, if it is not lower).
- Moving up a steep slope while standing on it: the up-slope part of the
  move is removed, leaving only the sideways part.
- A step that is reachable from below (≤ 30.5 u) is climbed without
  jumping; mid-air, the same rule lets the player climb onto ledges up to
  30.5 u above the hull's current base.

### Falling and fall damage

Fall damage is decided on the server from positions:

- While not standing on anything, the highest Y reached is tracked; it
  starts at the Y of the last grounded tick and only increases.
- On each tick the player is standing on something, the drop
  `d = highest Y - current Y` is taken (only if positive), the highest Y is
  set to the current Y, and damage is checked for that tick's drop.
- On a ladder, the highest Y is reset to a very low value (no fall
  accumulates); in spectator mode likewise.
- If `d > 50` and `d ≥ 300`, the player is not on a ladder, not an infected
  character, and not inside a safety-net volume:

```math
\text{damage} = 200 \cdot \frac{d - 300}{700 - 300} = \frac{d - 300}{2}
```

  as crush damage, upward direction, from the player itself. There is no
  upper clamp (a 1000 u drop does 350).
- A jump's height counts: jumping off a ledge measures from the apex.
- Walking down stairs or slopes never hurts: each grounded tick's small
  drop is checked on its own.
- The client separately marks "falling" (for sounds and animation only)
  when not in a jump and a 150 u ray straight down hits nothing.

### Other modifiers (brief)

- **Ladders:** gravity off, speed limit 200, upward velocity capped at 200,
  movement uses the camera's full direction.
- **Containers (water and other volumes):** viscosity removes
  `viscosity_max · viscosity · dt` of speed per tick (skipped right after a
  jump); currents add `current·dt` to velocity; friction coefficient is
  `5 × min(volume friction)`. Leaving a free-movement volume zeroes the
  velocity.
- **Explosion pushers:** within radius, with a clear line, add
  `strength·(1 - dist/radius)` along the push direction to velocity (a tenth
  in the air), vertical capped at 100.
- **Movement disallowed** (frozen, round start): horizontal velocity halved
  each tick, upward velocity removed, acceleration cleared except gravity
  (at most -1100 downward).

## Per-tick order

1. (Networked play) other characters and moving world models are rewound
   to the time the player sees, for collision only; restored at the end.
2. Note the volume type the player is in (and last tick's).
3. Update control flags from input (Input to control state, steps 1–9).
4. Update the crouch rate, then the crouch-spam values (using movement
   history up to last tick).
5. Resize the hull for the crouch rate.
6. Update the last direction speed `K`.
7. Motion step:
   a. if movement is disallowed, damp and skip to step 8;
   b. zero acceleration; process volumes (gravity on/off, ladder, viscosity,
      gravity override, speed factor, currents, jump pads);
   c. ground check from last tick's standing state; handle landing;
   d. compute the speed limit and clamp horizontal velocity;
   e. compute the acceleration (base, crouch, air factor, ramp, sum, clamp,
      start rescale), then scale by the jump factor if below 1;
   f. if a jump is requested and allowed, set vertical velocity, start the
      jump, set the jump factor to 0.3; otherwise recover the jump factor;
   g. add volume currents to velocity; if dying or movement is blocked,
      zero velocity and acceleration.
8. Braking when no movement keys are held (Stopping).
9. Explosion pushers.
10. Resize the hull for the crouch rate again.
11. Integrate with gravity and friction, move through the world with
    sliding and stair-stepping, update what the player stands on, apply the
    stuck-at-wall and blocked-vertical fixes.
12. Determine the "falling" status for sounds.
13. Record the tick (position, velocity, hull, flags) for the server and
    for the "still" history.

Stamina and the HUD update once per rendered frame after the ticks.

## Edge cases

- **Rising edge of moving while in the air** also uses the 500 start accel.
- **Reversal:** pressing forward on the tick after back (or the reverse)
  resets that axis to 500 even if both keys overlap.
- **Diagonal movement** is not faster: accel is capped at 3000 total and
  the speed limit applies to the horizontal magnitude.
- **Backpedal and strafe** use their own run speeds (all 285 by default);
  sprint needs forward, so strafing or backpedalling never sprints.
- **Zoomed** (scoped): speed limit is walk speed (70 × penalty factor) even
  while running or sprinting.
- **Crouch release with no headroom:** crouch stays forced on; the player
  stays crouched until there is room.
- **Pressing jump while standing up:** the press is consumed and no jump
  happens; it must be released and pressed again once fully up.
- **Landing:** no landing speed loss; horizontal velocity carries into the
  ground tick. The vertical velocity is zeroed on the first tick whose
  downward move is fully blocked (usually the tick after contact).
- **Standing on another player** is not ground (cannot jump off heads,
  braking does not apply), though the hull does rest there.
- **Speed limit change mid-air** applies immediately (no air speed is kept
  above the current limit).
- **Penalty clamp:** `P` is clamped to [-1, 1], so a speed can at most
  double from penalties (negative penalties exist on some vests).
- **Frame hitch:** a frame longer than 100 ms runs only 6 ticks; the rest
  of the time is dropped.
- **Spawn:** the client's local hull is placed at the server's last
  acknowledged position before the first tick.

## Quirks

- **Speed above the limit.** The clamp comes before integration, so the
  real speed is about 4 % above the nominal run speed (296.5 vs 285) and
  about 28 % above the nominal walk speed (89.4 vs 70). Keep: this is the
  game's feel.
- **Quarter-step displacement in the air.** Without friction the engine
  moves `v·dt + a·dt²/4` instead of `/2`. Jumps go slightly higher
  (50.74 u instead of the 49.5 u the continuous formula gives) and fall
  slightly slower. Keep.
- **Strong air control.** The in-air factor is overwritten by the ramp, so
  air acceleration is the full 3000 (times the jump factor). Only a player
  crouching during a jump gets the reduced 150. Keep.
- **Jump factor recovery while airborne.** The 0.3 jump factor recovers
  during the jump itself, so on flat ground the next jump is full height;
  it only bites on short hops (landing on something higher). Keep.
- **Lock-out refill ignores its "in penalty" timing.** The configuration has
  separate refill values for while locked out, but only the separate cost
  (0.9) is ever used; refill always uses the normal 1.5 s. Keep (it is what
  players experienced).
- **Lock-out uses the slowdown's still exception flag.** Both flags are 1 in
  the shipped config, so there is no visible difference.
- **Jump stamina on the client.** With the custom-room jump-stamina option,
  the client never subtracts the cost and never blocks the jump; only the
  server subtracts it. In normal play the cost is 0. Implement the server
  behaviour (subtract on jump) and no client block.
- **Stamina at zero while holding sprint is pinned to 0** until the key is
  released; players learn to let go of sprint to recover.
- **Sprint speed waits for the server.** Locally the sprint flag is set at
  once, but the higher speed limit applies only after the server echoes
  sprint on. In a local or listen game this is immediate; keep a one
  round-trip delay only if we simulate latency.
- **Crouch release delay** is measured on the wall clock (100 ms), not in
  ticks; a tap holds crouch for at least 7 ticks.

## Test cases

All on flat ground (normal (0, 1, 0)), `k = 5`, `dt = 0.015`, standing hull,
`P = 0` and no gear bonuses unless stated, always-run on, facing +Z.
Numbers rounded to 4 decimals where not exact.

1. **Run start, tick 1.** At rest; hold forward from tick 1. After tick 1:
   accel 500, speed 7.2257 u/s, distance 0.0549 u.
2. **Run ramp.** Same, after tick 2: accel 590, speed 15.2298, total
   distance 0.2240. After tick 3: accel 680, speed 23.9563, distance 0.5187.
   After tick 10: accel 1310, speed 101.3233, distance 6.8853.
3. **Ramp reaches max.** Same: accel is 2930 on tick 28 and 3000 from
   tick 29 on.
4. **Steady run.** Running at 285 u/s with accel 3000. Per tick: end speed
   307.7608 (clamped back to 285 at the next tick's start), distance
   4.44784 u (296.523 u/s). From rest, total distance after 60 ticks:
   208.9518 u.
5. **Steady walk.** Hold the walk key and forward, steady state: end speed
   108.2960, distance 1.34081 u per tick (89.387 u/s).
6. **Steady crouch walk.** Hold crouch and forward, fully crouched, steady:
   accel 1500, end speed 68.0641, distance 0.88717 u per tick
   (59.145 u/s).
7. **Reference loadout caps.** `P = 0.2` (vest 0.1, rifle 0.1), vest sprint
   bonus 0.4, figure sprint 1.0: limits run 228, sprint 319.2, walk 56,
   crouch 40. Steady distances per tick: run 3.62412, sprint 4.94207, walk
   1.13849, crouch 0.74266.
8. **Weapon caps.** Run limit with only a weapon penalty: knife (0) 285;
   pistol (0.03) 276.45; M4A1 (0.10) 256.5; AK-47 (0.13) 247.95; MK.48 (0.30)
   199.5. With `P = 1.2` the clamp gives `P = 1`: limit 0.
9. **Stop.** Steady run (speed 307.7608 at the end of the last held tick),
   release all keys. End speeds per tick: 224.7459, 168.8455, 116.9843,
   68.8703, 24.2330, then 0 on tick 6. Distance covered over ticks 1–5:
   9.4034 u; none on tick 6.
10. **Opposing keys.** Hold forward and back from rest: accel 0, moving flag
    on, no braking, speed stays 0, no start accel rescale.
11. **Diagonal.** Hold forward + right at steady state: total accel 3000
    (clamped), horizontal speed limit 285; per-tick distance 4.44784 u, the
    same as straight.
12. **Reversal.** Steady forward run, then switch to back only. Tick 1:
    back accel 500 (axis reset); tick 2: 590.
13. **Jump arc.** At rest, press jump on tick 1. After tick 1: height
    4.888125 u, vertical speed 313.5. After tick 2: 9.52875, 297.0. After
    tick 20: apex 50.7375 u, vertical speed 0. After tick 21: 50.675625,
    -16.5. Tick 41 is the first tick that reaches the floor (it starts at
    2.475 u and would move -5.011875). Landing (jump ends) is detected on
    tick 42.
14. **Jump factor.** Values at the start of each tick after a jump on tick
    0: tick 1: 0.3, tick 2: 0.318, tick n: `0.3 + 0.018(n - 1)`; tick 39:
    0.984; tick 40: 1.002; tick 41 on: 1.0.
15. **Air accel after a jump.** Steady run, jump on tick 0: the jump tick's
    horizontal end speed is 330 (accel 3000, no friction), distance 4.44375 u.
    Tick 1: clamped to 285, accel `3000 × 0.3 = 900`, end speed 298.5,
    horizontal distance 4.325625 u.
16. **Hold jump.** Hold jump continuously: exactly one jump. After landing,
    release on one grounded tick and press on the next: the second jump
    happens on that press tick (earliest: the third tick after the contact
    tick, tick 44 in case 13).
17. **Jump while standing up.** Fully crouched; release crouch and hold jump
    from the same tick: no jump during the 10 stand-up ticks nor after; a new
    press after full standing jumps.
18. **Air fall.** At rest in the air, no keys: after 20 ticks fallen
    48.2625 u, vertical speed -330; after 49 ticks 294.0919 u; after 50
    ticks 306.2813 u, -825 u/s.
19. **Air control from rest.** In the air (walked off, not a jump), hold
    forward: tick 1 accel 500, horizontal speed 7.5, distance 0.028125; tick
    2 accel 590, speed 16.35, distance 0.173813 total; tick 3 accel 680,
    speed 26.55.
20. **Crouched air control.** In the air, holding crouch and forward
    (crouch flag set): accel `3000 × 0.5 × 0.1` = 150 only if in a jump
    (speed +2.25 u/s per tick); if walked off (not in a jump), 1500
    (+22.5 u/s per tick).
21. **Crouch rate.** Hold crouch from standing, full speed: rate after
    ticks 1–10: 0.259615, 0.463208, 0.618555, 0.733433, 0.815619, 0.872890,
    0.913022, 0.943794, 0.972982, 1.0. Release when full: rate after ticks
    1–10: 0.740385, 0.536792, 0.381445, 0.266567, 0.184381, 0.127110,
    0.086978, 0.056206, 0.027018, 0.0.
22. **Crouch hull and speed.** Fallback boxes (half-heights 50 and 40). Tick
    1 of crouching: half-height 47.40385; on ground the centre moves down
    2.59615 so the feet stay. Speed limit tick 1 (P = 0): 64.8077; when full,
    50.
23. **Crouch-jump.** Jump from rest; start holding crouch on tick 20 (apex).
    After 10 crouch ticks the half-height is 40 and the centre is on the same
    path as without crouching, so the feet are 10 u higher than an uncrouched
    jump at the same tick.
24. **Crouch tap delay.** Press crouch for one tick at wall time 15 ms,
    release at 30 ms: crouch flag stays set until wall time reaches 115 ms
    (ticks at 15…105 ms keep it), cleared at the tick at 120 ms.
25. **Crouch spam (shipped config).** Running on flat ground (not still).
    Crouch key held for ticks 1–7, up 8–14, held 15–21, up 22–28, held 29–35,
    up 36–42, held 43 on. Lock-out value after tick 1: 0.6; after tick 14:
    0.73; after 15: 0.33; after 28: 0.46; after 29: 0.06; after 42: 0.19;
    after 43: 0 and lock-out begins. Slowdown value: 0.5 after tick 1, 0 after
    tick 15. Crouch rate (slowed from tick 2 on, since the slowdown value is
    read before it is updated): 0.259615 after tick 1, 0.680425 after tick 7,
    0.163968 after tick 14, 0.259615 after tick 15, 0.207692 after tick 28,
    0.259615 after tick 43, 0 after tick 44 (locked out: stands up at full
    speed); crouch flag cleared from tick 45 while held. Lock-out ends after about 1.5 s on the ground (100
    ticks at +0.01, ±1 for float rounding).
26. **Crouch spam while still.** Same key pattern, standing still: both
    values stay 1, no lock-out.
27. **Sprint drain.** Reference character (capacity 100), full gauge, hold
    forward + sprint for 1.000 s: gauge 70. Sprint flag drops when the gauge
    falls below 1, after 3.3 s; while the key stays held the gauge reads 0.
28. **Sprint recover.** Gauge 0, sprint released, 2.000 s: gauge 50; full
    (100) after 4.000 s, then stays at 100.
29. **Sprint blocked.** With a full gauge and sprint held: walk key held →
    no sprint; crouch held or crouch rate > 0 → no sprint; back held → no
    sprint; strafe only (no forward) → no sprint; on a ladder → no sprint;
    zoomed → sprint flag may be on but the limit is walk speed.
30. **Sprint speed.** Reference character with `P = 0` instead (no weapon
    penalty, vest penalty removed): sprint limit `285 × 1.0 × 1.4 = 399`;
    steady end speed 413.5236, distance 6.09529 u per tick (406.353 u/s).
31. **Fall damage.** Server, landing after a drop of: 299 u → none; 300 u →
    0 (no damage); 400 u → 50; 500 u → 100; 700 u → 200; 1000 u → 350. On a
    ladder or in a safety-net volume: none.
32. **Jump-off fall.** Jump from rest at the edge of a 300 u ledge, reach
    the apex (50.7375 u up) before falling: drop 350.7375 u, damage 25.36875.
33. **Stairs down.** Walk down a staircase of 20 steps of 16 u: no damage
    (each grounded tick's drop < 300).
34. **Step up.** Run into a 30 u high box: the player ends on top without
    jumping. A 31 u box blocks (no step, since 31 > 30.5).
35. **Slope.** Ground with normal.y 0.70: not ground (jump ignored, braking
    off, slides down). Normal.y 0.71: ground.
36. **Tick clock.** A 100 ms frame runs 6 ticks; a 200 ms frame also runs 6
    (the other 100 ms is dropped); a 20 ms frame after a 10 ms remainder
    runs 2 ticks and leaves 0 ms (interpolation factor 0).

## Open questions

1. **Hull sizes.** The stand and crouch boxes come from each character
   model's stand and crouch animations; only the fallback (half-extents
   15×50×15 and 15×40×15) is known here. Read the real values from the
   default character model once the model reader exists (models spec), or
   measure in game with a wall-height test.
2. **Eye height** and the camera's crouch offset are not part of this
   movement code; they belong with the camera spec.
3. **Unit scale.** The spec stays in world units; whether 1 u is meant as
   1 cm (as in later LithTech games) is not stated anywhere here. Only
   matters when mixing with other games' maps.
4. **Movement basis yaw.** The movement facing is the body rotation plus the
   upper-body twist, which should equal the view yaw; confirm there is no
   case where they differ (e.g. while the twist is clamped).
5. **Standing up in the air** grows the hull around its centre; whether the
   collision resize pushes the player up when the feet would enter the
   floor, or fails and is then forced (leaving the hull in the floor for a
   tick), needs a live check: jump, crouch, stand up just before landing.
6. **Live movement rate.** The engine default is 15 ms; the server sends the
   value to the client at connect. No shipped server configuration
   overriding it was found, but the live servers may have used another
   value. Check by measuring jump airtime (41 ticks ≈ 0.615 s).
7. **Live crouch-spam values.** Taken from one shipped dedicated-server
   configuration; the official servers may have used other numbers.
8. **Weapon modification penalty factor** (`c_mod`) defaults to 0; check
   attribute data for weapons whose modifications change it.
9. **Stamina timing granularity.** The gauge uses the OS millisecond tick
   count, which on Windows advances in 10–16 ms steps; we use exact frame
   time. Effects should be invisible.
10. **Jump pads** add their launch velocity; the pad's data and activation
    rules belong to a world-objects spec.
