# Combat Arms: inventory and weapon switching

Source basis: the reference material the user pointed this session to. That
covers the client's weapon manager and per-weapon animation state machine,
the client's input and HUD layer that turns weapon keys into switches, the
client's movement input sampling (fire, alt-fire, sprint), the server's
player inventory, its weapon pickup objects and its death and respawn
handling, the game's weapon attribute data (plaintext copy of the US
client's data) and the default player profile's key bindings. Animation
lengths and event times come from the stock first-person weapon models of
the BR client. They were read with a throwaway parser that is not in the
repo. See Open questions for how far these match the Steam build.

Status: draft

Sections: Summary; Units and conventions; Constants; Behavior (1 slots and
inventory, 2 default bindings, 3 slot keys, 4 last weapon, 5 scroll wheel,
6 switching, holster and draw, 7 what interrupts what, 8 the knife, 9 no
quick-knife or quick-throw, 10 auto-switch when a weapon runs empty,
11 pickup, 12 drop and death drop, 13 respawn); Per-frame order; Edge cases;
Quirks; Corrections (2026-10-19); Corrections (2026-10-22); Test cases; Open questions.

---

## Summary

- A player carries a list of up to 16 weapons. Each weapon belongs to one
  class: primary, secondary, melee, throw (grenades and mines), special or
  objective. Keys 1 to 6 select a class. Pressing a class key again cycles
  through the weapons of that class. Q swaps to the previous weapon. There
  is no holster command and the mouse wheel does not switch weapons by
  default.
- Switching is instant on the outgoing side: there is no holster delay.
  The new weapon plays its draw animation, and it cannot fire until that
  animation ends. The draw time is the length of that animation, so it
  differs per weapon (stock: knife 450 ms, pistol 650 ms, rifle 1200 ms,
  grenade 400 ms). Players rely on the swap being instant.
- The knife (melee class) has two attacks. Primary (left mouse) is a
  650 ms swing that hits twice, 45 damage each. Secondary (right mouse) is
  a 1100 ms stab that hits once for 75 damage. Both reach 105 units.
  Damage is randomised by ±10 %. There is no knife-only backstab. Instead,
  every hitscan hit that lands from the victim's rear sector does ×1.3
  damage. That sector is asymmetric (see 8.4).
- Only primaries are dropped. On death the last-selected primary drops at
  the player's feet with its ammo. Using (E) on a dropped weapon swaps it
  for the weapon of the same class you carry, and draws the new one at
  once. Dropped weapons vanish after 20 s. There is no drop key.

## Units and conventions

- Distance: LithTech world units, Y up, left-handed (X right, Z forward).
  These are the same units as the other Combat Arms specs.
- Time: milliseconds. Weapon logic runs on the client once per rendered
  frame, just before the frame is drawn. Animation playback advances by
  real elapsed time. Animation events ("keys") fire when playback crosses
  their time, and the weapon logic consumes them on its next update. Every
  timing below is therefore exact in animation time, but is observed with
  the resolution of one frame.
- One clock or two (Open question 12): the weapon logic (draw, holster,
  reload, knife swing, firing, pickup requests) runs on the **rendered-frame
  clock**, not on movement.md's 15 ms tick. In each frame the movement ticks
  run first, then the weapon update reads the control flags (fire, alt-fire,
  sprint) left by the frame's last tick. A frame that ran no movement tick
  reuses the previous flags. "The next update" always means the next
  rendered frame's weapon update. The server does not run the weapon state
  machine for players: it applies each shot, hit, reload or pickup message
  when the message arrives in its own update.
- The server owns the inventory: which weapons, which order, and the ammo
  totals. The client owns the choice of what is in hand, the animation
  state, and the clip count. It reports changes to the server.
- Ammo model (also used by `weapon_firing.md`): every carried weapon has
  a **total** T, which includes the rounds in the clip, and a **clip**
  C ≤ T. A shot lowers both by 1. A reload sets C = min(T, clip size) when
  its animation completes. It moves nothing from a reserve, because the
  reserve is just T − C. Losing a reload therefore never loses ammo.
- Names of weapons, ammo and animations given in this spec ("M9", "M16A3",
  "select", "fire_0") are data names that the game's files expose.

## Constants

| Name (ours) | Value | Unit | Meaning |
|---|---|---|---|
| max_carried | 16 | weapons | Size of a player's weapon list (equip indices 0..15) |
| max_per_class | 32 | weapons | Order slots per class |
| holster_time | 0 | ms | Time the outgoing weapon takes to go away |
| draw_rate_cap | 2.0 | × | Any view-model animation rate above this is replaced by 1.0 |
| activation_distance | 100 | units | Maximum distance from the camera to a pickup's surface hit |
| activation_fallback_slack | 35 | units | Subtracted from the camera-to-centre distance for the fallback pickup test |
| dropped_lifetime | 20.0 | s | A dropped or spawned weapon item is removed after this |
| behind_bonus | 0.3 | × (additive) | Hit from the rear sector: damage × (1 + 0.3) |
| damage_spread_default | 0.1 | fraction | Damage roll ±r when a weapon's data gives none |
| damage_spread_fallback | 0.2 | fraction | Used when a weapon's value lies outside [0, 1] |
| infinite_ammo | 1000 | rounds | Total shown and used for weapons with infinite ammo (knives) |
| cooked_grenade_speed | 500 | units/s | Speed of the live grenade released when a player dies while cooking |

Stock per-weapon data (attribute data and first-person model animations):

| Weapon | Class | Clip / total | Draw (select) | Reload | Fire anim | Sprint-out (post-rush) | Move penalty |
|---|---|---|---|---|---|---|---|
| M9 (default knife) | melee | 1000 / 1000, infinite | 450 ms | — | 650 ms primary, 1100 ms secondary | 167 ms | 0 |
| M16A3 (default primary) | primary | 30 / 120 | 1200 ms | 2300 ms | 105 ms | 367 ms | 0.12 |
| Beretta M92FS (default secondary) | secondary | 15 / 45 | 650 ms | 1500 ms | 180 ms + 150 ms post-fire | 400 ms | 0.03 |
| M67 (default grenade) | throw | 1 / 1 | 400 ms | — | 700 ms pre-fire, 67 ms throw, 366 ms post-fire | 267 ms | 0.03 |

M9 attack data:

| Attack | Input | Animation | Hit events | Base damage | Range | Damage roll |
|---|---|---|---|---|---|---|
| Primary (slash) | fire (left mouse) | 1 ms pre-fire, then 650 ms swing | 155 ms and 402 ms into the swing | 45 per hit | 105 units | ±10 % |
| Secondary (stab) | zoom (right mouse) | 1 ms pre-fire, then 1100 ms stab | 367 ms into the stab | 75 | 105 units | ±10 % |

The range-damage table of the M9 is 1.0 at every band (bands 105/105/105),
so distance never changes knife damage. The movement penalty column feeds
`movement.md`. A knife's penalty of 0 is why players run with the knife
out.

## Behavior

### 1. Slots and inventory

- **Classes.** A weapon's class comes from its position in the weapon
  attribute data's class order list. The lists are, in this order:
  primary → 1, secondary → 2, melee → 3, throw → 4, special → 5,
  objective/mission → 6. A weapon in none of the lists has class 0. It can
  never be selected by key and is never dropped.

| Class | Key | Holds | Notes |
|---|---|---|---|
| 1 primary | 1 | rifles, SMGs, LMGs, snipers, shotguns, launchers | The only class dropped on death; pickups swap within it |
| 2 secondary | 2 | pistols | Selectable even with no ammo |
| 3 melee | 3 | knives | Infinite ammo |
| 4 throw | 4 | grenades, mines, air strikes | Usually hide-when-empty: unselectable at 0 |
| 5 special | 5 | M32, rockets, sensors, medic kits, deployables | Hide-when-empty per weapon |
| 6 objective | 6 | mission bomb | Key 6 does nothing once the bomb is planted |

- **The list.** At spawn the server builds the list from the player's
  equipped loadout. The loadout holds primary, secondary, melee and throw
  items, plus up to four backpack items, which can be extra primaries or
  specials. Each weapon gets the next equip index, starting at 0 (16 at
  most). It is also appended to its class's order list, at the first free
  order slot (32 per class). Several weapons can share a class. A
  backpack primary is a second primary.
- **Starting ammo.** Each weapon starts with total = its ammo's spawned
  amount (stock: equal to the maximum). Its first selection fills the clip
  instantly, without a reload animation.
- **In hand at spawn.** The weapon of the lowest class number is drawn,
  which is the primary.
- **Per-class cursor.** The client keeps, for each class, the order slot
  last used in that class. At spawn every cursor is 0. Each switch writes
  the destination's class cursor. Inside the weapon manager, a per-class
  "last chosen weapon" is also kept, and the auto-switch (10) and the death
  drop (12) use it.
- **Selectability.** A weapon can be switched to only if the player is
  alive and not spectating, and the weapon is in the list at that index
  with that id. A weapon flagged no-select is refused unless it is also
  flagged forced-select. The client toggles forced-select on
  controller/charge pairs such as a claymore and its detonator. A weapon
  marked "in use" (a deployed mine whose detonator is the selectable half)
  is skipped by slot keys and refused by Q.

### 2. Default key bindings

These come from the default profile. Every one can be rebound.

| Action | Default |
|---|---|
| Primary class (1) | 1 |
| Secondary class (2) | 2 |
| Melee class (3) | 3 |
| Throw class (4) | 4 |
| Special class (5) | 5 |
| Objective class (6) | 6 |
| Previous weapon | Q |
| Fire | left mouse |
| Zoom / knife secondary | right mouse |
| Reload | R |
| Use / pick up | E |
| Sprint | left Shift (only while also moving forward) |

- Keys 1 to 0 are also bound to "choose 1..0" items, which pick lines in
  the radio and emote menus. While such a menu is open, the number keys
  pick menu lines and the class keys do nothing.
- Class keys are also ignored when any of these hold: a vote prompt that
  can still be answered is on screen; the respawn weapon-change step is
  not open; the infection selection window is open; the player is frozen
  in snowball mode; the player's weapons are disabled; the player is
  disabling a gadget target.

### 3. Class keys (select and cycle)

Pressing class key k (k = 1..6) does the following:

1. Key 6 maps to the objective class while the bomb is not yet planted.
   After the plant it does nothing.
2. For throw, special and objective: if that class's cursor is invalid
   (nothing left in it), or if the bomb is already planted for the
   objective class, do nothing.
3. Find the target order slot:
   - If the weapon in hand is of class k, move the cursor to the next order
     slot. If that slot is empty, wrap to slot 0. This is how a second
     primary is reached.
   - Otherwise keep the cursor where it was. This gives "the weapon of
     that class I used last".
   - For classes above melee only: if the weapon at the cursor has
     total ≤ 0, it is skipped only when it is hide-when-empty. The search
     then moves forward, wrapping, to the next weapon with total > 0. If
     it comes back to where it started, the result is "nothing".
   - Weapons marked "in use" are skipped the same way.
4. If the result is nothing, or is the weapon already in hand, nothing
   happens. In particular there is **no re-draw** when you press the key of
   the only weapon of the class you are holding.
5. Otherwise record previous := the weapon in hand (for Q), and switch
   (section 6). The HUD shows that class's weapon list.

### 4. Previous weapon (Q)

The client stores one "previous weapon": an equip index plus a weapon id.
Pressing Q does the following, in order:

1. If there is no previous weapon, or its index is the index in hand, do
   nothing.
2. If the previous weapon is marked "in use", do nothing.
3. Read the previous weapon's total, then **overwrite previous := the
   weapon in hand**.
4. If the previous weapon's class cannot be found, or its total is ≤ 0 and
   it is hide-when-empty (for example the grenade you just threw), stop
   here. Because step 3 has already run, Q now points at the weapon in
   hand and does nothing until another switch happens (see Quirks).
5. Otherwise switch to it. Pressing Q repeatedly toggles between two
   weapons.

The previous weapon is cleared on respawn (the inventory reset) and by
every auto-switch (section 10). It is not touched by a pickup's
auto-draw, apart from the in-hand index changing.

### 5. Scroll wheel

The default profile binds nothing to the wheel. In play, the wheel only
zooms the spectator/death camera: 40 units per notch, clamped. The
binding screen can assign wheel-up and wheel-down to any command, class
keys and Q included. A bound wheel notch then behaves exactly like a key
press of that command. There is no built-in next/previous-in-list cycling.

### 6. Switching: holster and draw

A switch to a weapon W at equip index i (from a class key, Q, a pickup
auto-draw or the auto-switch) does the following:

1. The checks of 1 (Selectability) run. If any fails, nothing changes.
2. **Instant holster.** The weapon in hand is hidden at once. Its
   animation state is abandoned: an unfinished reload, swing, burst or
   draw is simply lost. Its clip keeps whatever value it had. No deselect
   animation plays and no time passes (holster_time = 0).
3. W becomes the weapon in hand, and the class cursor and per-class "last
   chosen" are set to i.
4. W's draw starts. Its state becomes "drawing" and its select animation
   restarts from time 0 at rate ρ_draw:

```math
\rho_{draw} = 1 + \sum_{\text{attached mods}} s_{mod} + s_{model} + s_{set}, \qquad
\rho_{played} = \begin{cases} 1 & \rho_{draw} > 2.0 \\ \rho_{draw} & \text{otherwise} \end{cases}
```

   Here s_mod, s_model and s_set are the select-rate bonuses of attached
   mods, of the character model and of equipped item sets. They are all 0
   in the stock data. The draw time is

```math
t_{draw} = \frac{L_{select}}{\rho_{played}}
```

   L_select is the length of W's select animation (table above).
5. Zoom is cancelled. Burst counters reset. An unfinished bomb plant is
   cancelled and movement is allowed again. The server is told the new
   index, id and ammo type.
6. If W's clip is 0 and its total is > 0, a reload starts at once. For a
   magazine weapon this replaces the draw (see 7). For a shell-by-shell
   weapon (shotgun) the reload waits until the draw ends.
7. If this switch is a pickup's auto-draw, W's clip is set to the clip
   that came with the dropped item.

While W is drawing, its fire is held off: holding fire does nothing until
the select animation reports done. **Corrected:** the update that sees the
draw done only switches the state to firing. The pre-fire animation (a
0 ms one counts) plays on the next update, the fire animation on the one
after, and the round is spent on the 3rd update after the draw ended (2nd
for a primary shotgun, which skips its pre-fire). Time to first shot after
a switch is therefore the draw time plus 3 updates (50 ms at 60 fps), see
Corrections 1. The outgoing weapon's speed does not matter.

### 7. What interrupts what

"Sprint" here means the movement layer's sprint (dash) state. Its own
rules (forward key, run, stamina, not crouching) are in `movement.md`.
This spec relies on two facts from it. Sprint never waits for the weapon,
because the weapon-state gate that could block it is compiled out in this
build. Holding backward ends sprint.

| While the weapon is… | Fire pressed | Reload pressed | Class key / Q | Sprint starts |
|---|---|---|---|---|
| drawing | ignored until the draw ends | **magazine weapon: reload replaces the draw at once** (if the clip is not full); shell-by-shell weapon: refused | switch (draw restarts on the new weapon) | the draw continues to its end, then the sprint pose begins |
| reloading, magazine weapon | ignored until the reload completes; if held, the update that completes the reload sets the firing state and the round follows 3 updates later (Corrections 1) | ignored (already reloading) | switch; the reload is lost, the clip keeps its old value, the total is unchanged | the reload continues and completes, then the sprint pose begins |
| reloading, shell-by-shell | stops the reload at once; shells already loaded stay; it fires | — | switch; loaded shells stay | as magazine |
| firing (any fire animation playing) | — | queued: the state becomes "reloading" at once, but the reload animation starts only when the fire animation ends | switch; the remaining hits of a knife swing or burst are lost | **the fire animation plays to its end, and its hit events still land**; then the sprint pose; grenades stay in their throw until it finishes |
| sprinting | ignored | allowed: the reload animation plays and completes under sprint | switch; the new weapon's draw plays, then the sprint pose | — |
| leaving sprint | ignored until the post-sprint animation ends (167–400 ms stock); the update that sees it end returns to idle, and a held fire then takes the 3-update chain, 4 updates in all (Corrections 1) | allowed | allowed | — |

- Reload is refused when the clip is already full or already equals the
  total (no spare rounds), for infinite-ammo weapons (knives), while
  zooming in or out, and while the outgoing deselect state is set (never
  in practice, see 6). If the weapon is zoomed, a reload request first zooms out, then
  reloads.
- Fire needs a clip > 0 and the use key not held. Pressing fire while
  holding E does nothing.

### 8. The knife

#### 8.1 Inputs

- Fire (left mouse) gives the primary attack. Zoom (right mouse) gives
  the secondary, but only while a knife (or another alt-fire weapon) is
  in hand. With guns that button zooms.
- If both are held, primary wins: the fire flag is tested first.
- Holding the button repeats the attack. When a swing animation completes
  with the button still held, the next swing starts at once, with no
  pre-fire and no gap. If the held button changed from primary to
  secondary (or back) during a swing, the next swing uses the other
  attack.

#### 8.2 Timeline of one attack (from idle, button pressed at t = 0)

1. The first weapon update after the press sets the state to "firing".
2. The next update starts the 1 ms pre-fire animation.
3. When it ends (the next update), the main animation starts at
   rate ρ_fire:

```math
\rho_{fire} = \text{mutation}\left(1 + \sum_{\text{mods}} f_{mod} + f_{weapon} + f_{set} + f_{convert}\right)
```

   f_weapon is the weapon's fire-rate bonus (0 for the M9), and "mutation"
   is the active buff/nerf transform (identity when none). The same
   2.0 → 1.0 cap applies.
4. Hit events happen at the key times, divided by ρ_fire:
   - primary: two hits, at 155 ms and 402 ms;
   - secondary: one hit, at 367 ms.
   Each hit is a separate attack ray and can hit a different target.
5. The animation ends at 650 ms (primary) or 1100 ms (secondary).
   Releasing the button does not stop a swing in progress: the swing plays
   to its end and all of its hit events land. If the button is not held at
   the moment the swing ends, a 1 ms post-fire plays and the knife returns
   to idle. While the main animation plays, the attack cannot restart.

So, from idle, the first primary hit lands about 155 ms plus 2 or 3 frames
after the press, and a held primary hits every 650 ms in a 155/402 ms
pattern. Precisely: with `u0` the first weapon update after the press, the
main animation starts at the time of `u2` (two frames later), and the hit is
consumed by the first weapon update at which at least 155 ms of the main
animation have played (if the engine reports the key after that frame's
weapon update, one frame later). At 60 fps with the press just before
`u0` = 0 ms: main animation from 33.3 ms, first hit at 200.0 ms (216.7 ms if
one frame late).

#### 8.3 The attack ray

- Each hit event casts one ray from the camera position (movement.md, Eye
  height) along the aim direction. The aim direction includes the weapon's
  spread (`weapon_firing.md`, same cone and square pattern as guns; the M9
  has min/max perturb 1/7). The M9's ammo record has no spread values
  (all firing caps and movement targets are 0), so both meters stay at 0
  and the cone is always the minimum, n = 1: each axis is offset by at most
  ±0.001, i.e. within ±0.105 units of the aim point at 105 units (Open
  question 16). The ray reaches range = 105 × (1 + Σ mod range bonus)
  units. Knives have no mods.
- The ray stops at the first object or world surface. Melee never
  penetrates.
- Any victim takes the hit, including teammates if friendly fire is on
  (`game_modes.md`). What it hit is decided client-side; the server
  re-checks the target and applies the damage.

#### 8.4 Damage

```math
D = B \cdot (1 + m) \cdot u \cdot g \cdot (1 + v) \cdot b \cdot k_{range} \cdot k_{zone},
\qquad u \sim \mathcal{U}[1 - r,\ 1 + r]
```

- B is the ammo's instant damage: 45 for the primary, 75 for the
  secondary (the alt value).
- m is the sum of the mods' damage bonuses plus the weapon conversion
  bonus (0 for stock knives).
- r is the weapon's damage spread: 0.1 for the M9. The default is 0.1 if
  the data omits it, and 0.2 if the value lies outside [0, 1]. u is drawn
  uniformly, once per hit.
- g is the attacker's damage factor (1 unless a mode or buff changes it).
- v is the victim figure type's variable damage (0 in the stock data).
- b is 1.3 if the hit is from behind (below), else 1.
- k_range comes from the weapon's range-damage table (1.0 for the M9).
- k_zone is the hit-zone factor of the body part hit, exactly as for
  bullets (`weapon_firing.md`). There is no separate knife table. Melee is
  one of the damage types that use zones (head ×6, shin ×0.75, foot ×0.5).
- Armour: the armour step runs, but no stock gear lists the melee damage
  type, so a knife is never reduced and uses no armour points
  (weapon_firing.md, Armour).
- v and b apply only when the hit recorded a valid body part.

**Rear sector ("backstab").** This rule applies to every hitscan hit on a
character, not only knives. Take d as the normalised shot direction
(shooter towards victim), and f and ρ as the victim's forward and right
unit vectors. Then:

```math
\theta = \operatorname{atan2}(f \cdot d,\ \rho \cdot d), \qquad
s = \left\lfloor \frac{\pi + \theta}{\pi / 6} \right\rfloor, \qquad
\text{behind} \iff 7 \le s \le 11 \iff \tfrac{\pi}{6} \le \theta < \pi
```

The test is a 150° rear sector. It is **not** symmetric about the
victim's back. Measured at the victim, it reaches from 60° to the
victim's left of dead-behind to 90° to the victim's right (the right flank,
square-on, is the open edge θ = π). See Quirks and Open questions.

With a 100-health target, k_zone = 1 and the stock M9:
- one primary hit does 40.5–49.5 (52.65–64.35 from behind);
- a full primary swing does 81–99 (105.3–128.7 from behind, so a rear
  slash kills);
- a secondary does 67.5–82.5 (87.75–107.25 from behind).

### 9. Quick-knife and quick-throw

Neither exists. No command attacks with the knife or throws a grenade
without first switching to it. The fastest knife is "3, then fire": first
hit about 450 + 155 ms after pressing 3. The fastest throw is "4, then
fire": 400 ms draw plus the grenade's own timings (`grenades.md`).

### 10. Auto-switch when a weapon runs empty

- Trigger: the weapon in hand is hide-when-empty (grenades, mines, most
  specials) and has just used ammo; its fire animation is not playing; and
  its total is ≤ 0. Controller/charge pairs (claymore and detonator, mine
  and switch) first hand over to their partner weapon instead.
- Result: the previous weapon (Q) is cleared. The weapon manager first
  tries another ammo type for the same weapon (none exists in the stock
  data). It then switches to the last chosen primary. If no primary was
  ever chosen, it tries the last chosen secondary. If that is also
  invalid, the last chosen melee weapon. This switch is a normal switch:
  draw time applies.
- Primaries and secondaries never auto-switch. At 0 total they stay in
  hand and dry-fire.
- **The spent weapon stays** (Open question 14). Throwing the last grenade
  does not remove it: it keeps its equip index and order slot at total 0.
  Afterwards:
  - its class key skips it (hide-when-empty, section 3); with no other
    usable weapon in the class the key does nothing;
  - Q refuses it and goes dead (Quirks 7);
  - it is never dropped (section 12);
  - the HUD weapon list keeps an entry for it (how an empty entry is drawn
    was not traced, see Open question 14); the bottom-right weapon panel
    follows the weapon in hand, so it shows the weapon switched to;
  - if a map item of the same grenade is picked up, the non-primary rule of
    section 11 adds to its total and it is selectable again;
  - respawn rebuilds the loadout, so it comes back with its spawned amount
    (1 for the M67).

### 11. Pickup

- **What can be picked up.** Weapon items on the ground. These are drops
  from dead players, weapons dropped by a swap, and map-placed weapons.
  Mines on the ground cannot be picked up.
- **How.** The client casts a ray from the camera along the view. A weapon
  item is targeted if the ray hits it within 100 units of the camera. If
  the ray hits nothing better, there is a fallback test: an item counts
  when (distance from the camera to the item's centre) − 35 ≤ 100. The
  HUD then shows the pickup prompt. Pressing use (E) sends the request,
  and the server acts on it. There is no walk-over pickup.
  - *Correction* (Open question 13): the fallback does **not** apply to
    weapon items. It only collects the hit boxes of dead bodies that the
    ray passes through (for body interactions), and only when the ray found
    no other object. A weapon item is targeted only when the ray actually
    enters its box and the entry point is at most 100 units from the camera
    (exactly 100 counts). The ray is not blocked by other non-solid objects
    that cannot be used, but stops at walls and solid objects.
  - The item's box is an axis-aligned box: the half-extents of its world
    model's "World" animation, times the item's scale. Our items without a
    model need a box of their own (see Open question 13).
- **Who may pick up.** The player must be alive, human-model, not
  infected, not the VIP, and not under a buff that replaces weapons. The
  item's owner and team restrictions must also pass: some items are
  visible to or usable by one player or team only, for a time.
- **Primary items (swap).** These run in order:
  1. Take the class's weapon to replace. This is the last-selected weapon
     of the item's class, not necessarily the one in hand.
  2. The weapon in hand is put away.
  3. The replaced weapon is dropped as a new item, with its total, clip,
     mods and sight style. It appears at the player's left-foot position,
     offset by a uniform random amount in [−0.5, 0.5] × the hitbox
     width on X and Z, with a random yaw of 0 to 1 radian. Precisely (Open
     question 17): the position is the world position of the character
     model's "Left_foot" node in its current animation pose, plus
     `u·halfX` on X and `u'·halfZ` on Z with `u, u'` uniform in
     [−0.5, 0.5] and `halfX`, `halfZ` the character hit box's half-extents
     (the hit box has the model's own size, 15 × 15 with the fallback box,
     so at most ±7.5 units).
  4. The picked weapon goes into the same equip index and order slot.
     Its total is the item's ammo amount, clamped to the weapon's
     maximum. Its clip is the item's clip, or 0 if the total is 0.
  5. The item is removed.
  6. The server orders a switch to the new weapon. With the "auto weapon
     switch" option on (the default), the client draws it at once, from
     the class key or not. Its clip is the item's clip, and its draw
     time is that of the new weapon.
     - Why the switch always happens (Open question 6): every pickup
       request first puts away the weapon in hand on the server (step 2),
       so when the server's "better weapon" rule runs at the end, the
       server has no weapon in hand. The rule orders a switch when **no
       weapon is in hand**, or when the picked weapon was not already
       carried (the class's last-selected slot did not hold a carried
       weapon with the same attachments and sight style) **and** its
       command id is higher than the in-hand weapon's. The command id is
       the weapon's position in the concatenated order lists (all
       primaries in list order, then secondaries, melee, throw, special,
       mission). Because of step 2 the first condition always holds, so a
       pickup always ends with a switch to the picked weapon; the command-id
       comparison is never reached in this build.
  7. If the player had no weapon of that class at all, the weapon is
     simply added and drawn. Nothing is dropped.
- **Non-primary items.** If the player already carries a weapon with the
  same id, its total is raised by the item's amount, clamped to its
  maximum. The server also refills that weapon's clip to full. The item
  is removed. *Correction:* the server puts away the weapon in hand for
  this request too, so the "better weapon" rule then finds no weapon in
  hand and orders a switch to the picked weapon (Open question 6); with the
  auto weapon switch option on, the client draws it. If the player does not
  carry that id, the request becomes the swap above, within the item's
  class.
- **Ammo always travels with the gun.** A dropped primary keeps exactly
  the dropper's total and clip. Picking it up gives exactly those.
  Nothing is merged with what you had.

### 12. Drop and death drop

- **No drop key.** A player cannot throw a weapon away. A weapon leaves
  the inventory only through a pickup swap (11) or death.
- **On death** (server), in this order:
  1. If the weapon in hand is a grenade that was being cooked (pre-fire
     held), a live grenade is released at the player's position, at
     500 units/s along the player's facing (`grenades.md`).
  2. The weapon in hand is put away.
  3. If the mode allows drops (not snowball, not the VIP weapon race), and
     the character is a human model, the **last-selected primary**
     drops. It drops even if a pistol or knife was in hand. Its item
     carries its total, clip, mods, sight style and conversion. It
     appears at the left-foot position with a random yaw of 0 to 1
     radian. Unlike the swap drop, it gets no position jitter: exactly the
     "Left_foot" node's world position in the pose at the moment of death.
  - **Without a skeleton** (our placeholder characters, Open question 17):
    the reference has no fallback for a missing foot node. Use the bottom
    centre of the hull (hull centre minus the half-height on Y); a foot
    node sits near the floor, so this is close, but the real sideways offset
    of the left foot needs the model or a live measurement. This is our
    choice, not the reference's.
  4. A primary whose data has "drop on death" off is not dropped. Neither
     is a weapon of class 0. Throw-type weapons (grenades, mines, kits,
     deployables) never drop at total 0. Primaries drop even at total 0.
  5. With a backpack perk, the other primaries are also recorded on the
     body as droppable, and the body creates them from its random drop
     list (see Open questions).
- Secondaries, knives, grenades and specials are never dropped on death.
- A dropped item exists for 20.0 s and is then removed. A player-drop
  item does not respawn.

### 13. Respawn

On respawn, the inventory and the client weapon list are cleared and
rebuilt from the loadout (1). Picked-up weapons are gone. The clips are
full, the class cursors are 0, the previous weapon is cleared, and the
primary is drawn.

## Per-frame order

Client, in one rendered frame:

1. **Discrete commands** are handled as their key-down events arrive,
   before the frame update:
   - class keys and Q (sections 3, 4) switch immediately (section 6);
   - R requests a reload;
   - E requests a pickup or use.
2. **Movement update**: held keys become control flags (fire, alt-fire,
   sprint and the rest). Sprint start or stop is decided here
   (`movement.md`).
3. Camera update, then the **target update**: the pickup ray of 11.
4. **Weapon update** (just before drawing):
   1. Decide the fire request. Fire counts if it is held, E is not held,
      no chooser is open, the player is alive and not spectating, the
      game is in play, and the clip is > 0. Otherwise alt-fire counts if
      it is held and the weapon has an alt attack.
   2. Detect fire-button edges. Clear the request if the clip is 0.
   3. Run the state machine, choosing one of three paths:
      - the sprint path, if sprinting: finish a reload, draw or fire
        animation, or else play the sprint pose;
      - the leaving-sprint path: play the post-sprint pose to its end;
      - the firing path or the non-firing path (draw, reload, fire,
        idle transitions).
   4. Consume a hit event raised by an animation key since the last
      update. This uses ammo, sends the fire message and casts the ray.
   5. Update spread (`weapon_firing.md`, fixed 80 Hz sub-steps).
   6. Run the auto-switch check (10).
   7. If total > 0, the clip is 0 and the weapon is neither reloading nor
      firing, request a reload.
5. **Weapon manager**: if 4.6 asked for it, select the fallback weapon.

The movement ticks of the frame (movement.md) run before all of this, in
step 2; the weapon update sees only the result of the frame's last tick.

Animation keys (hit and throw events, sounds) are raised by the engine as
the view-model's playback crosses them, between weapon updates.

Server: use requests (pickup swap, section 11) and deaths (section 12)
are handled when their messages arrive. Weapon-change reports from the
client update the server's in-hand weapon and its per-class
last-selected index, which section 12 uses.

## Edge cases

- **Switching mid-reload:** the reload is lost, but no ammo is lost. The
  clip keeps its pre-reload value and the total is unchanged (ammo
  model). Coming back to the weapon gives the normal draw. If the clip was
  0, the reload starts at once instead (6.6).
- **Switching to a weapon with an empty clip and spare ammo:** a magazine
  weapon skips its draw and goes straight into reload. Ready time is the
  reload length, not draw + reload.
- **Empty class:** a throw, special or objective key with nothing usable
  does nothing. A pistol or primary at total 0 is still selectable.
- **Same class, one weapon:** the key does nothing. There is no re-draw,
  so it does not cancel a reload.
- **Dropping the last gun:** impossible by key. On death the only primary
  drops, which does not matter because respawn rebuilds the loadout. A
  swap always leaves you with one primary.
- **Pickup while holding a pistol:** the primary is the weapon replaced,
  and the new primary is drawn. The pistol is put away.
- **Q after the previous weapon was swapped away by a pickup:** the
  previous index now holds a different id. The switch is refused because
  the id does not match, but Q has already overwritten "previous" (4.3).
- **Swing interrupted by a switch:** the hit events not yet reached never
  happen. Switching away 200 ms into a primary swing keeps the first hit
  (155 ms) and loses the second.
- **Death while cooking a grenade:** the grenade goes off as in 12.1. The
  primary is still dropped.
- **Sprint pressed during a swing:** the swing finishes and both hits
  land, with the player already moving at sprint speed.

## Quirks

Keep all of these unless noted.

1. **Instant holster.** There is no put-away time, only the draw of the
   incoming weapon. "Pistol is faster than reloading" works because the
   650 ms pistol draw beats the 2300 ms rifle reload. Keep.
2. **Reload cancels the draw.** Pressing R during a magazine weapon's
   draw (clip not full) starts the reload at once and throws the draw
   away. Keep.
3. **Two hits per knife slash.** The primary swing has two hit events.
   Keep.
4. **Sprint-knifing (players call it "shifting").** In this build sprint
   does not cut a knife swing short, and the swing's hits still land while
   the player moves at sprint speed. Some regional builds had a switch
   that made sprint cancel knife and double-barrel swings; it is off
   here. Keep it off.
5. **Asymmetric backstab sector.** The rear ×1.3 sector spans 60° on the
   victim's left of dead-behind and 90° on the right. Keep as specified;
   verify (Open questions).
6. **The ×1.3 rear bonus applies to every hitscan hit,** guns included.
   Keep.
7. **Dead Q after an empty grenade.** If Q's target is an empty grenade,
   Q overwrites "previous" with the weapon in hand and then refuses.
   After that Q does nothing until another switch. Keep.
8. **Q forgets after an auto-switch.** Throwing your last grenade
   switches you to your primary and clears Q. Keep.
9. **Death drops the last primary, not the weapon in hand.** Dying with
   the knife out still drops your rifle. Keep.
10. **The swap-drop jitter has no death-drop counterpart.** Swap drops
    scatter by ±half the hitbox. Death drops land exactly at the left
    foot. The random yaw is limited to 0–1 radian in both. Keep.
11. **Ammo pickup refills the clip** on the server (11). See Open
    questions for the client side.
12. **Animation-rate cap:** a total rate above 2.0 is played at 1.0, so
    stacking speed bonuses past ×2 makes a weapon slower. Keep.

## Corrections (2026-10-19)

From a second pass over the reference's client fire state machine and the
server's damage code, compared with this spec and our implementation.
Test cases 45–49.

1. **Ready times after a draw, a reload or sprint are 3 updates later.**
   - Reference: the update that sees a draw, a magazine reload or the
     post-sprint pose end does not fire. A draw or reload ending with fire
     held switches the state to firing on that update. The next update plays
     the pre-fire (the M16A3's and the M92FS's are 0 ms, but they are there),
     the one after starts the fire animation, and the update after that
     spends the round: 3 updates, 50 ms at 60 fps. A primary shotgun skips the
     pre-fire: 2 updates. Leaving sprint takes one update more, because the
     update that sees the post-sprint pose end only returns to idle: 4 updates.
     The knife's timeline of 8.2 already counts its own chain.
   - Ours: section 6 said fire "is accepted on the same update", table 7 said
     "it fires on the update the reload completes", and test cases 1, 2, 11–14
     and 16 used those times. In the implementation `start_reload` and the
     draw make the weapon ready exactly at their end time
     (`games/combat_arms/src/game/inventory.rs:795-806`, `456`; the frame's
     trigger then fires on that tick), and the sprint hold ends at
     `sprint_out_until` (`inventory.rs:877`, `880-891`).
   - Why it matters: a "pistol is faster than reloading" swap or a sprint-stop
     shot is 50–67 ms slower in the game than in ours.
   - Test cases with corrected expectations at 60 fps: case 1 → first shot
     1250 ms ±2f; case 2 → 700 ms; case 11 → 3250 ms; case 12 → ready
     (first shot with fire held) 2350 ms; case 13 → 2450 ms; case 14 → first
     shot on the 3rd update after the one that completes the reload,
     2350 ms ±1f; case 16 → rifle 367 + 66.7 ≈ 433 ms ±2f, pistol 467 ms.
2. **The knife's rear sector is measured from the victim's body.**
   - Reference: the behind test of 8.4 uses the victim's server rotation,
     its body yaw, which lags the view by the upper-body twist (up to ±45°
     with no movement key held; `weapon_firing.md` Corrections 6).
   - Ours: the knife's test uses the victim's view yaw
     (`inventory.rs:1009-1010`).
   - Why it matters: backstabbing a standing player who has turned the
     mouse without moving. The ×1.3 that decides one-swing kills follows the
     legs, not the head.

## Corrections (2026-10-22)

From a comparison of the reference's server pickup, weapon-list and spawn
code with this spec and our implementation (the inventory, attachments,
grenades and gear session). Each entry: what the reference does, what we
do (file:line), and a testable expectation.

1. **A primary item whose fitted parts match yours is not a swap: your
   weapon stays and only its ammunition is replaced.**
   - Reference: before swapping (section 11, "Primary items"), the server
     compares the item with the class's last-selected weapon. The
     comparison looks only at the five attachment slots (each slot empty on
     both, or the same part on both; the item's empty slots are first
     filled with its weapon's default parts) and at the sight style. It
     does **not** compare the weapon itself. When they match, nothing is
     swapped and nothing is dropped: the weapon you carry keeps its equip
     index, order slot and identity; its total is **set** to the item's
     amount (clamped to your maximum; not added, so it can go down), its
     clip is set to the item's clip (0 if the item's total is 0), and the
     item is removed. Only when the parts or the sight style differ does
     the swap of section 11 run. The client is sent the item's weapon and
     ammo ids for your equip index; what its HUD then shows was not
     traced (Open question 18). Stock weapons without parts all "match"
     each other, so a bare AK-47 item taken by a player with a bare M16A3
     leaves the M16A3 in hand with the AK-47's round count.
   - Ours: every primary item is a swap; the carried weapon is dropped and
     the item's weapon takes its place
     (`games/combat_arms/src/game/inventory.rs:1220-1263`).
   - Why it matters: picking up a gun identical to yours (same weapon,
     same parts, the common case with stock loadouts) leaves no second gun
     on the floor in the game, and can lower your ammunition.
   - Expect: carry an M16A3 with no parts, total 60, clip 10. (a) E on a
     bare M16A3 item with total 20, clip 20 → the same weapon at the same
     equip index, total 20, clip 20, no item on the floor. (b) E on a bare
     AK-47 item with total 90, clip 30 → still the M16A3, total 90, clip
     30, nothing dropped. (c) Fit a red dot to the carried M16A3 and repeat
     (b) → the normal swap: the M16A3 is dropped with total 60, clip 10,
     and the AK-47 is drawn.
2. **No pickup into a class you have not selected this life.**
   - Reference: the swap (primary items, and non-primary items whose id
     you do not carry) needs the class's "last chosen" weapon on the
     server, which is set only when a weapon of that class is switched to
     and is cleared at every respawn. If it is unset the request does
     nothing: the item stays and nothing is added. Step 7 of section 11 ("no
     weapon of that class at all: simply added and drawn") never happens.
     A primary is drawn at every spawn, so primary items are only affected
     for a loadout without one.
   - Ours: without a last-chosen weapon we fall back to the class's cursor
     slot or its first weapon, and with none at all we add the item
     (`inventory.rs:1221-1226`, `1252`).
   - Expect: carry an M67 not drawn since spawning; E on an XM84 item →
     nothing happens, the item stays. Press 4 (the M67 is drawn), switch
     back to the rifle, E again → the M67 is dropped and the XM84 taken.
     An M67 item still adds to a carried M67 whether or not it was drawn
     (the same-id rule is unchanged).
3. **Starting totals: the server counts `SpawnedAmount`, the client is
   told the maximum.** See `gear_perks.md` Corrections (2026-10-22) 1. For
   this spec: "Starting ammo" in section 1 is right for the server's
   count; at spawn the HUD shows the player's maximum instead, and a fire
   message that reaches the server at its own total 0 is ignored (no
   round, no grenade). With stock data the two differ only for the M-23F
   and G7A (1 real, 2 shown; `grenades.md` Corrections (2026-10-22, b) 1)
   and for set items with extra magazines.

## Test cases

Common setup unless stated: a stock loadout (M16A3 primary 30/120, M92FS
secondary 15/45, M9 knife, one M67), standing still, not sprinting, alive,
60 fps (frame ≈ 16.7 ms), every draw and reload rate 1.0, k_zone = 1.0 and
v = 0. "Ready" means the first time a held fire produces a shot. "±f"
means within the given number of frames.

1. **Draw rifle.** Pistol idle in hand. Press 1 at t = 0. → The M16A3 is
   the weapon in hand at t = 0 (same frame). Holding fire, the first shot
   comes at 1200 ms ±2f. The pistol's clip and total are unchanged.
2. **Draw pistol.** Rifle idle. Press 2 at t = 0. → The M92FS is in hand
   at t = 0 and ready at 650 ms ±2f.
3. **Draw knife.** Rifle idle. Press 3 at t = 0. → The M9 is in hand at
   t = 0. Holding left mouse from t = 0, the first hit lands at
   450 + 155 = 605 ms ±3f.
4. **Draw grenade.** Rifle idle. Press 4 at t = 0. → The M67 is in hand at
   t = 0. Fire is not accepted before 400 ms ±2f.
5. **No re-draw.** Rifle in hand, 500 ms into its draw. Press 1. → Nothing
   changes, and the rifle is ready at 1200 ms from the original press.
6. **Cycle primaries.** Loadout with two primaries A (order 0) and B
   (order 1), A in hand. Press 1. → B is drawn. Press 1 again. → A. Press
   2, then 1. → The last primary used (A) is drawn.
7. **Q toggle.** Rifle in hand at spawn, previous empty. Press Q. →
   Nothing. Press 2, then Q. → Rifle. Q. → Pistol. Q. → Rifle.
8. **Q after the last grenade.** Rifle in hand. Press 4, throw the only
   M67, and let the auto-switch return the rifle. Press Q. → Nothing (Q
   cleared by the auto-switch).
9. **Dead Q.** Rifle in hand. Press 4, then throw the only M67. After its
   ammo has been used (`grenades.md`) but before its throw animation ends,
   press 2. There is no auto-switch, because the grenade is no longer in
   hand. → Pistol, with previous = the M67 at total 0. Press Q. → Nothing.
   Press Q again. → Still nothing (previous is now the pistol itself).
   Press 1. → Rifle; Q. → Pistol.
10. **Empty grenade key.** M67 total 0. Press 4. → Nothing; the weapon in
    hand is unchanged.
11. **Switch mid-reload.** Rifle clip 10, total 100. Press R at t = 0;
    press 2 at t = 1000 ms; press 1 at t = 2000 ms. → Rifle clip 10, total
    100. The reload never completed. The rifle is ready at
    2000 + 1200 = 3200 ms ±2f.
12. **Switch to an empty clip.** Rifle clip 0, total 90, pistol in hand.
    Press 1 at t = 0. → The reload starts at once, with no draw. At
    2300 ms ±2f the rifle has clip 30, total 90, and is ready.
13. **Reload cancels the draw.** Pistol in hand, rifle clip 20, total
    110. Press 1 at t = 0, R at t = 100 ms. → The reload plays from
    100 ms. Ready at 2400 ms ±2f, with clip 30, total 110.
14. **Fire during reload.** Rifle clip 5, total 65. Press R at t = 0 and
    hold fire from t = 500 ms. → No shot before 2300 ms. The first shot
    comes on the update that completes the reload (2300 ms ±1f): clip
    30 → 29, total 65 → 64.
15. **Sprint during reload.** Rifle clip 5, total 65. Press R at t = 0 and
    start sprinting at t = 1000 ms. → The reload completes at 2300 ms ±1f
    (clip 30, total 65). Only then does the sprint pose begin.
16. **Leaving sprint.** Rifle idle, sprinting. Release sprint at t = 0
    while holding fire. → No shot before 367 ms. First shot at
    367 ms ±2f. With the pistol: 400 ms. With the knife: the first hit at
    167 + 1 + 155 ms ±3f.
17. **Knife primary timing.** Knife idle. Hold left mouse from t = 0 to
    t = 700 ms. → Hits at ≈156, 403, 806 and 1053 ms (each ±3f). The second
    swing starts at ≈651 ms, because the button is still held when the
    first swing ends. It plays to its end after the release. The knife is
    idle again at ≈1302 ms.
18. **Knife secondary timing.** Knife idle. Click right mouse once at
    t = 0. → One hit at ≈368 ms ±3f. Fire is not possible again until
    ≈1101 ms.
19. **Both buttons.** Knife idle. Hold both mouse buttons from t = 0. →
    A primary swing (hit at ≈156 ms), not a stab.
20. **Knife damage, front.** Target 50 units ahead, facing the attacker
    (θ = −π/2). One primary hit. → Damage in [40.5, 49.5]. A secondary
    → [67.5, 82.5].
21. **Knife damage, behind.** Target 50 units ahead, facing away
    (θ = +π/2). One primary hit. → [52.65, 64.35]. A secondary →
    [87.75, 107.25].
22. **Sector edges.** For a shot direction d with victim facing f and
    right ρ: θ = 29° (d = cos29°·ρ + sin29°·f) → not behind, ×1.0;
    θ = 31° → behind, ×1.3; θ = 179° → behind; θ = 180° → not behind;
    θ = −30° → not behind.
23. **Knife range.** Target surface at 104 units → hit. At 106 units →
    miss. The ray also stops at a wall at 60 units, so a target behind
    the wall is not hit.
24. **Swing cut by a switch.** Knife idle. Press left mouse at t = 0 and
    press 2 at t = 250 ms. → Exactly one hit (≈156 ms). The pistol is
    ready at 900 ms ±2f.
25. **Sprint-knife.** Knife idle. Press left mouse at t = 0 and start
    sprinting at t = 100 ms. → Both hits land (≈156 and ≈403 ms) while
    sprint speed applies. The sprint pose starts at ≈651 ms.
26. **Death drop.** Rifle clip 12, total 72; the pistol is in hand; the
    player dies. → One rifle item at the left foot: total 72, clip 12.
    The pistol, knife and grenade are not dropped. At t = +20.0 s the item
    is gone.
27. **Death drop exemptions.** Same as 26 in snowball mode, or with a
    primary whose "drop on death" is off. → No item.
28. **Pickup swap.** Rifle A clip 5, total 35, held in hand. Dropped
    rifle B (item total 60, clip 30), 80 units away and aimed at. Press E.
    → Item A appears near the left foot (within ±0.5 hitbox on X/Z) with
    total 35, clip 5. B is in A's equip index with total 60, clip 30, and
    is drawn. Ready after B's draw time. Item B is removed. ("±0.5 hitbox"
    means ±0.5 × the hit box's half-extent: ±7.5 units with the fallback
    box; see case 44.)
29. **Pickup while holding a pistol.** Same as 28, with the pistol in
    hand. → Same result: A dropped, B drawn. The pistol's ammo is
    unchanged.
30. **Pickup out of range.** Same as 28, but the ray hit is 101 units
    away and the item's centre is more than 135 units from the camera. →
    No prompt; E does nothing.
31. **Pickup clamp.** Dropped M16A3 item with total 150, picked up. → The
    total becomes 120.
32. **Pickup at total 0.** Dropped primary with total 0 (clip stored as
    7). → Picked up with total 0, clip 0.
33. **Same-id ammo pickup (non-primary).** Pistol total 20, clip 5. A
    pistol item of the same id with total 30. Press E. → Total
    min(20 + 30, 45) = 45. Server-side clip 15. The item is removed. The
    pistol is not drawn if it was not in hand.
34. **Auto-switch fallback.** No primary ever selected (primary cursor
    invalid), secondary selected earlier. Throw the last grenade. → The
    pistol is drawn when the throw animation ends.
35. **Respawn.** After test 28, die and respawn. → Loadout rebuilt
    (rifle A again, full ammo). Previous weapon empty: Q does nothing.
36. **Rate cap.** A weapon whose summed select rate is 2.1. → It draws at
    rate 1.0: M16A3 1200 ms, not 571 ms. With 1.9: 1200 / 1.9 ≈ 632 ms.
37. **Weapon logic on the frame clock.** 60 fps, knife idle, left mouse
    pressed just before the frame update at t = 0. Weapon updates at 0,
    16.7, 33.3 ms: firing, pre-fire, main swing from 33.3 ms. First hit on
    the update at 200.0 ms (or 216.7 ms if the key is reported one frame
    late). At 100 fps: main swing from 20 ms, first hit at 180 ms (or
    190 ms). The movement ticks of a frame never split a weapon step.
38. **Fallback is not for weapons.** A dropped rifle whose box the view ray
    misses by 1 unit, centre 60 units from the camera → no prompt, E does
    nothing. The ray entering the rifle's box 100.0 units from the camera
    → prompt; 100.5 units → no prompt (whatever the centre distance).
39. **Fallback for bodies.** The ray passes through a dead body's hit box
    whose centre is 130 units from the camera and hits only a wall behind
    it → the body is the target (130 − 35 = 95 ≤ 100). Centre at 140 → no
    target.
40. **Spent grenade.** Stock loadout, throw the only M67. → The M67 stays
    in the list at total 0 (same equip index). Press 4 → nothing. Respawn
    → M67 total 1, selectable.
41. **Pickup always switches.** Pistol in hand, a dropped rifle aimed at,
    E → the pistol is put away and the picked rifle is drawn. Rifle in hand,
    a same-id pistol ammo item (non-primary) aimed at, E → the pistol's
    total rises and the pistol is drawn.
42. **Better-weapon rule on its own** (state not reachable through a
    pickup in this build, see 11 step 6). Server in-hand weapon M4A1
    (position 1 in the primary list), new weapon not already carried:
    MP5A4 (position 0) → no switch; M16A3 (position 9) → switch; any
    pistol (the secondary list comes after all primaries) → switch. Already
    carried (same attachments and sight style) → no switch, whatever the
    positions. No weapon in hand → switch.
43. **Knife ray and zones.** M9 primary hit, roll 1.0, unarmoured: head →
    270 (45 × 6), torso → 45, shin → 33.75, foot → 22.5. The ray is
    deflected by at most ±0.001 on each axis (n = 1). AP 100 with default
    body gear, torso → 45 and AP stays 100.
44. **Drop positions.** Fallback hit box (half-extents 15 × 50 × 15). Death
    drop: exactly at the left-foot node's world position. Swap drop: X and
    Z each within ±7.5 units of the left-foot node, Y unchanged. Without a
    skeleton: at the hull's bottom centre (our fallback), death drop with no
    offset, swap drop within ±7.5 on X and Z.
45. **Shot after a reload, fire held (Corrections 1).** Rifle clip 5,
    total 65, R at t = 0, fire held from 500 ms, 60 fps. → The update at
    2300 ms completes the reload (clip 30) and sets firing; 2316.7 ms
    pre-fire; 2333.3 ms fire animation; the round on the update at
    2350.0 ms (clip 29, total 64).
46. **Shot after a draw (Corrections 1).** Pistol in hand, press 1 at t = 0
    with fire held, 60 fps. → No round before the update at 1250 ms; the
    round on that update.
47. **Shotgun after a draw (Corrections 1).** A primary shotgun with a
    1000 ms draw, fire held. → Round on the update at 1033.3 ms (2 updates).
48. **Leaving sprint (Corrections 1).** Rifle, fire held, sprint released;
    the post-sprint pose is seen done on the update at 367 ms (rounded to the
    frame: 383.3 ms at 60 fps when it starts at 16.7 ms). → Idle on that
    update; firing, pre-fire and fire animation on the next three; the round
    on the 4th update after it.
49. **Backstab by the body (Corrections 2).** Standing victim, no key held,
    body yaw 45° to the right of its view. A knife stab travelling 150° from
    the view's right (−0.8660, 0, 0.5 in the view's frame) → not behind
    (body sector 0): 75 at roll 1.0, not 97.5.

## Open questions

1. **Steam build vs reference.** The behaviour comes from a BR-region
   version of the game, the data from the US client's attributes, and the
   animation timings from BR client models. Check draw, swing
   and hit times on the Steam build, for example with a high-speed capture
   of the knife (155/402/650 ms) and the M16A3 draw (1200 ms).
2. **Sprint-knife switch.** The code has a compile-time switch that makes
   sprint cancel knife and double-barrel swings and blocks sprint while a
   knife is firing. It is off in this build. Was it on in the US or Steam
   release? Test: knife, press fire, then sprint at 100 ms. Do both hits
   still land?
3. **Rear sector handedness.** The asymmetric 150° sector depends on the
   engine's "right" vector and the atan2 argument order. Verify in game
   which flank gets the wider bonus: stab a stationary target from 45°
   behind its left, then from 45° behind its right.
4. **Server-side draw gate.** Nothing found on the server stops a shot
   during a client's draw. The server trusts the client's state. Confirm
   whether anti-cheat code elsewhere enforces fire timing.
5. **Non-primary pickup clip.** For a same-id ammo pickup, the server
   fills the clip but tells the client the item's stored clip. Which one
   the HUD shows until the next reload is unclear.
6. **"Better weapon" rule** after a non-primary ammo pickup: the server
   compares command ids (higher id = better) and may order a switch.
   With a weapon in hand it does nothing, since the condition needs "did
   not already have it". Confirm no auto-draw happens in practice.
   **Answered:** the rule is "switch if the server has no weapon in hand,
   or if the picked weapon was not already carried and its command id
   (position in the concatenated order lists) is higher than the in-hand
   weapon's". Every pickup request puts away the weapon in hand on the
   server before the rule runs, so the first condition always holds and
   every pickup, primary or not, ends with a switch order to the picked
   weapon. The reading above ("does nothing with a weapon in hand") was
   wrong for this reason. See 11, step 6 and "Non-primary items"; test
   cases 41 (switch) and 42 (the rule's no-switch outcomes). Live check:
   carry a pistol, pick up a same-pistol ammo item while holding the rifle,
   and see whether the pistol comes out.
7. **Auto weapon switch off.** With the profile option off, a primary
   pickup still puts the held weapon away on the server, but the client
   does not draw the new one. What does the player hold? Untested.
8. **Backpack-perk death drops.** The extra primaries recorded on the
   body ("random drop list") were not traced. How many drop, and where?
9. **Drop physics.** Drop items are created with gravity off and "move
   to floor" off, but with a velocity flag. Whether they fall, slide or
   stay at foot height belongs to the pickup-item physics. Not traced.
10. **Previous-weapon index after respawn.** The inventory reset clears
    it. Mode-specific respawns without a reset (no-reset respawn) were
    not traced.
11. **Wheel bindings.** The binding UI accepts wheel up and down. Confirm
    that the options screen exposes them for weapon commands on the Steam
    build.
12. **One clock or two.** Does the weapon logic run per rendered frame or
    per movement tick, and what does "the next update" mean?
    **Answered:** per rendered frame, after that frame's movement ticks,
    using the last tick's control flags; "the next update" is the next
    frame's weapon update. The server applies each message as it arrives
    and runs no weapon timing of its own for players. For the knife: main
    swing from the second weapon update after the first one that saw the
    press, hit on the first update with 155 ms played (or one frame
    later). See Units and conventions and 8.2; test case 37.
13. **Pickup fallback.** Does the "centre distance − 35 ≤ 100" fallback
    apply to items the ray misses? What box does an item without a model
    use?
    **Answered (first part):** the fallback is only for dead bodies' hit
    boxes; weapon items need a real ray hit with the entry point within
    100 units. Case 30 still holds; cases 38 and 39. **Open (second part):**
    a reference item's box is its world model's "World" animation extents ×
    scale; an item without a model has no defined box there. For our
    placeholder items use a box matching the weapon's size; to measure the
    real ones, aim at a dropped M16A3 from the side and from the end and
    note at which offsets from its centre the prompt appears and vanishes.
14. **Spent grenades.** Does the last grenade stay in the inventory at
    total 0, and what do the keys, the HUD and respawn do with it?
    **Answered:** it stays (same index, total 0): the class key skips it, Q
    refuses it, it is never dropped, respawn restores it. See 10, "The spent
    weapon stays"; test case 40. **Open:** how the HUD weapon list draws an
    empty entry (greyed, hidden or plain "0"); check by throwing the last
    grenade and pressing 4 or another class key to bring up the list.
15. **"Better weapon" rule:** see Q6 (answered).
16. **The knife's hit.** Does the knife ray use the M9's spread and the
    hit-zone multipliers?
    **Answered:** both. The ray goes through the same cone code as guns,
    but the M9's ammo has no spread values, so the cone is always its
    minimum (n = 1, ±0.001); and melee is a zone-using damage type (head
    ×6). weapon_firing.md's old line that melee never uses zones was wrong
    and is corrected there. See 8.3 and 8.4; test case 43.
17. **Where dropped items appear.** Drop origin and offset without a
    skeleton.
    **Answered for the reference:** both drops start at the world position
    of the model's "Left_foot" node in its current pose; the swap drop adds
    ±0.5 × the hit box's half-extent on X and Z (±7.5 with the fallback
    box), the death drop nothing; yaw uniform in [0, 1) rad. There is no
    deliberate-drop command. **Open:** the reference has no no-skeleton
    fallback; we use the hull's bottom centre. Measure the real offset live:
    stand still facing +Z on flat ground, die (or swap), and read the item's
    position relative to the player's last position (several tries for the
    swap jitter). See 11 step 3 and 12; test case 44.
18. **HUD after a matching-parts pickup** (Corrections 1). The server keeps
    your weapon but sends the item's weapon and ammo ids for your equip
    index. Does the weapon panel show the item's weapon name while the
    hands still hold yours? Live check: bare M16A3, pick up a bare AK-47.
