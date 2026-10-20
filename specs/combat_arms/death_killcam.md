# Combat Arms: death, death camera and kill cam

Source basis: reference material the user pointed the spec session to: the client's player manager (player states, the death-camera phase machine, spectate-target choice, respawn wait and request), the client's player camera (pull-back, orbit, zoom, wall clipping), the client's base player manager (damage report, head-attached death view, map camera take-over), the client's free-spectator movement, the client's kill-cam HUD panel, the in-game tip data manager, the name-tag HUD, the client's key handling while dead, the client's handling of the kill message, and the server's player death, state change and kill message. The kill-cam layout file itself was not available to the session, so on-screen geometry is not given (Open questions). Nothing was measured on the Steam install.
Status: draft

Sections: Summary; Units and conventions; Constants; Behaviour (1 player states, 2 the phases after a death, 3 the pull-back camera, 4 the watch (chase) camera, 5 whom the dead watch, 6 keys while dead, 7 Hunted's hold and free cameras, 8 the kill-cam screen, 9 the respawn countdown, 10 respawn and round end, 11 what the server sends and what the client works out); Per-frame order; Edge cases; Quirks; Test cases; Open questions.

---

## Summary

- There is **no replay kill cam**. What the room option "Kill Cam" (`KillCam_On` / `KillCam_Off`, `lobby.md`) switches is the **enemy view**: whether the dead player's camera may follow the killer (and, when no teammate is alive, other enemies) and whether the death screen shows the killer's remaining health and armour. The room option reaches the client as the console variable `Enemydeadcam`.
- After a death the client runs three camera phases on its own clock: a **pull-back** of 3.5 s (`DyingTime`) in which the camera rises 20 u above the dead player and backs away from the killer's direction while looking towards it; then a **watch** phase, a third-person orbit around a living player (the killer first, if the enemy view is on and the killer is alive); and at 8 s (`DeadCameraTime`) a forced move from the killer to a teammate. Space, Ctrl or Shift cycles the watched player; the mouse orbits; the wheel zooms.
- The **death screen** (layout `Hud_KillCam.xml`, root `KillCam`) shows: the killer's name and rank icon (plus health and armour with the enemy view on), the killing weapon's name and large icon, the last damage taken and its hit zone, the watched player's name with "killed you N times", and, in respawn modes, a respawn gauge with a seconds-and-hundredths countdown, a fast-respawn counter and a tip banner. There is no distance, no loadout, no stat line.
- In respawn modes the countdown starts at the moment of death (7 s stock, 2 s in the reference build's edit); the client sends its respawn request itself on the first frame after the wait. In round modes there is no countdown: the dead watch until the next round setup.
- The server sends only the facts (player state, the damage report, the kill message, respawns). Every camera, timer and screen decision is the client's.

## Units and conventions

- Distances in LithTech world units (u). +Y up, left-handed: +X right, +Z forward (`movement.md`, `characters.md`).
- Angles in radians unless degrees are stated. Yaw ψ: +ψ turns +Z towards +X ("right"). Pitch θ: + looks down. "Heading" of a vector v is atan2(v_x, v_z).
- "Rotation about axis a by φ" uses the matrix
  M(a, φ) = cos φ · I + sin φ · [a]× + (1 − cos φ) · a aᵀ applied to column vectors, with [a]× the cross-product matrix of a. With a = (0, 1, 0) and φ > 0 this turns +Z towards +X. Where a is **not** of unit length the formula is still used as written (section 4).
- Player position P: the player object's position, which is the centre of its movement hull (`characters.md` 10: on death the dead player's object is first snapped to the floor, so a standing body's P is about 50 u above the floor).
- Time: the client's death clocks (pull-back length, the 8 s switch, the respawn wait) use the OS millisecond tick and are tested once per rendered frame. The respawn gauge's display uses client game time in seconds. The camera moves once per rendered frame, so the pull-back's speed depends on frame rate (Quirk 1).
- Mode names as in `game_modes.md` (room keys `TDM`, `TSV`, `TMM`, ...). Layout control names, string ids, data file names and console variables are data the game exposes.

## Constants

| Name (ours) | Value | Unit | Meaning |
|---|---|---|---|
| pull-back time | 3500 (`DyingTime`) | ms | Length of the pull-back phase (every mode); in Hunted also the hold before it (7) |
| killer-view end | 8000 (`DeadCameraTime`) | ms | After this time since death the camera is moved off the killer once |
| anchor lift | 20 | u | The pull-back starts, and the orbit looks at, a point this far above P |
| pull-back step | 5 (`DC_TV`) | u per frame | Distance the pull-back moves away from the killer's direction each frame, before the 0.9 pull-in |
| pull-back start lift | 20 (`DC_TY`) | u | Height of the pull-back's start above P |
| pull-back stop length | 220 (`DC_TL`) | u | Stop when the pull-back is farther than this from its start (never reached, Quirk 2) |
| pull-back freeze switch | −1 (`DC_FC_OU`) | | A positive value stops the pull-back after its first frame |
| orbit back | 165 (`DeadCamX`) | u | Orbit offset, horizontal part, before zoom and pull-in |
| orbit up | 90 (`DeadCamY`) | u | Orbit offset, vertical part |
| orbit base length L₀ | √(165² + 90²) = 187.95 | u | |
| clip pull-in | 0.9 | × | Every pull-back and orbit position is placed at 0.9 of its distance from the anchor (or from the wall hit) |
| zoom step | 40 | u per wheel notch | Wheel forward (away from the player) zooms out |
| zoom range | −L₀/2 … 800 = −93.97 … 800 | u | Added to the orbit length |
| orbit pitch clamp | ±(45° − 0.1 rad) = ±39.27° | ° | The dead player's view pitch while not in first person |
| mouse factor | 0.5 | × | The client's look delta is halved before it is added to yaw and pitch (dead or alive) |
| respawn wait (client) | 7000 (reference edit 2000) | ms | `game_modes.md` S5; Relentless lowers it to `RespawnTime` (`gear_perks.md`) |
| respawn gauge length | ⌊wait / 1000⌋ | s | Integer seconds (Quirk 3) |
| countdown text refresh | 40 | ms | |
| last-damage display range | 1 … 999 | | |
| free-camera start height | 1000 | u | Hunted's "static" free camera starts this far above the body (7) |
| free-camera acceleration | 300000 (×2 with run; Hunted +200000 per map load) | u/s² | Hunted free cameras and held joiners (7, 10) |
| head-view time limit | 5.0 (`MultiplayerDeathAttachCamMaxTime`) | s | Longest time the view rides the body's `Eyes` socket (Hunted hold only, 7) |

## Behaviour

### 1. Player states

The server tells the victim's client its own state (a guaranteed message to that client only). The states that matter here:

| State (ours) | Meaning | What the client does on entering it |
|---|---|---|
| alive | playing | death screen and countdown hidden, camera back to first person on the own player, death-camera phase reset to "none", free-camera choices cleared |
| dying | the death has started | radio menu closed, HUD level set to "dead" (below) |
| dead | dead and waiting | own movement stopped; the death time t₀ (OS ms) is stored; the "first watch" flag is set; in Hunted the camera's rotation at this moment is saved |
| waiting | joined a running round and held (`game_modes.md` S1), or an observer slot | free first-person spectator: weapons hidden, HUD level "none", free flight (10) |

On a death the server sends "dying" and then "dead" in the same server frame, then the kill message to everyone (11). The client handles them in the frame they arrive.

**HUD level while dead.** The HUD has four levels (always, text, dead, full). While dying or dead the client draws everything up to "dead": the kill feed (`hud_crosshair.md` D5), the mode's score panel, chat, notices, name tags and the death screen. Crosshair, health and ammo panels, radar and perk panel are "full" and are hidden. A live map camera (game_modes.md S15, the end-of-round view) lowers the level to "text", which hides the death screen too.

**Sound.** While the death camera runs, the 3D sound listener is the camera (its position and rotation), not the dead player.

### 2. The phases after a death

Let t = now − t₀ in ms. "Killer known" means the kill message named a killer other than the local player; a death with no player killer (fall, world) carries killer id 255, which the client also treats as known here (Quirk 4); a suicide (own grenade, telefrag) has no killer known.

1. **First frame dead.** The camera is detached from the own player. If the killer is known, the phase becomes **pull-back** (3). If not (suicide), the camera goes straight to **watch** (4) on the first living player chosen as in 5 (skipping the killer step). In the respawn modes listed in 9 the countdown starts in this same frame.
   - Hunted only: if the killer is known, the client first waits in **hold** (7) until t ≥ 3500, then restarts its death clock (t₀ := now) and enters the pull-back, so in Hunted the pull-back ends 7 s after the death.
2. **Pull-back** while t < 3500 (`DyingTime`).
3. At the first frame with t ≥ 3500 the phase becomes **watch**. Its first target is chosen this way:
   - if the killer id is valid and not 255 and not the local player: the killer. If the kill message's damaging object is a character other than the killer's player object (a Hired Guns mercenary, `bots_objectives.md`), the camera follows that object, labelled with the killer's name;
   - otherwise the next living player by 5.
   - If that target is alive, one frame later the client checks again: target is the killer, it is alive and `Enemydeadcam` is on → the camera follows the killer ("enemy view"); otherwise → the next living player by 5, counted from the killer's place in the list.
4. **Killer-view end.** On the first camera update with t > 8000 while still dead and the phase is not "none", the camera moves to the next living player by 5 (counted from the current target), once. If the phase was not already the watch, it is set to watch.
5. The watch continues until respawn (10), until the round's setup respawns everyone, or the map is left. The watched target changes automatically when it dies or when it is the own player (5), and by keys (6).

There is no first-person spectating for players in this build: the first-person chase view exists in the client but nothing a player does reaches it (Open question 1). Game-master observers have their own first-person, third-person and free views on separate observer keys (outside this spec).

### 3. The pull-back camera

Inputs: the own player position P (the dead, floor-snapped player object), the damage direction d of the last damage report (the direction the damage travelled, from the source towards the victim, `hud_crosshair.md` C), and the world.

On its first frame the pull-back sets its start S = P + (0, 20, 0) and puts the camera there. Every frame after that, while it has not frozen:

1. u = −d / |d| (pointing back towards where the damage came from). The camera looks along u with world up (+Y) as up: heading atan2(u_x, u_z), pitch −asin(u_y) in our sign convention.
2. Step: c' = c − 5 u (5 u straight away from the source). If a ray from c to c' hits anything, the pull-back **freezes** after this frame (no further moves).
3. Place (the shared placement rule, also used by the orbit): from the anchor A = P + (0, 20, 0) cast a ray to c' against world geometry only; let h be the hit point, or c' if nothing is hit. The camera is put at

```math
c_{new} = A + 0.9\,(h - A)
```

4. If the distance |c' − S| exceeds 220 (`DC_TL`), freeze. (It never does: see Quirk 2.)

With no wall, the offset o = c − A obeys o ← 0.9 (o − 5u), so after n frames

```math
\lvert o_n \rvert = 45\,(1 - 0.9^{\,n}) \quad \text{u}, \qquad \lim_{n\to\infty} \lvert o_n \rvert = 45\ \text{u}
```

The camera slides 45 u back from a point 20 u above the dead player, looking past the body towards the killer's side; at 60 frames per second it has covered 99 % of that in 0.73 s. Mouse movement during the pull-back does not turn the view, but it still changes the dead player's yaw and pitch, which the orbit uses next (4).

If d is zero (a damage report without a direction), u is undefined: the step moves nothing and the camera stays at S (Edge cases).

### 4. The watch (orbit) camera

The orbit is a third-person camera around the watched player W, driven by the **dead player's own** view yaw ψ and pitch θ (the mouse, with the 0.5 factor and the ±39.27° pitch clamp of the constants table). It does **not** follow W's facing.

Each frame, with A = P_W + (0, 20, 0) and zoom distance z (5, wheel):

```math
o_0 = (-165,\ 90,\ 0), \qquad o_1 = o_0\left(1 + \frac{z}{L_0}\right), \qquad L_0 = \sqrt{165^2 + 90^2}
```

```math
o_2 = M\big((0,1,0),\ -\psi\big)\, o_1
```

```math
n = \frac{o_2}{\lvert o_2 \rvert}, \qquad a = (n_z,\ 0,\ -n_x), \qquad o_3 = M(a,\ -\theta)\, o_2
```

The axis a is used without normalising it (its length is √(1 − n_y²) ≈ 0.878 at zoom 0), so the pitch step also shortens the offset slightly (Quirk 5). The desired position A + o₃ is then placed by the rule of 3 step 3 (world-only ray from A, 0.9 pull-in). The camera looks from its final position c at A:

```math
\text{heading} = \operatorname{atan2}(A_x - c_x,\ A_z - c_z), \qquad \text{pitch} = -\operatorname{atan2}\!\Big(A_y - c_y,\ \sqrt{(A_x-c_x)^2 + (A_z-c_z)^2}\Big)
```

(pitch positive looks down). There is no smoothing: the camera is placed exactly each frame. Without walls, at ψ = θ = z = 0 the camera sits 148.5 u to the −X side of A and 81 u above it, looking along +X and 28.61° down. Turning the mouse to raise ψ moves the camera so that its heading is 90° − ψ (Quirk 6: the horizontal orbit turns the opposite way to the alive view, by the formula).

**Zoom.** Each wheel notch adds 40 × notches to z (notches positive when the wheel is rolled away from the player), clamped to [−93.97, 800]. z resets to 0 whenever the watched target changes. The wheel does nothing else in play (`inventory.md` 5).

**Walls.** Only the anchor-to-camera ray is tested (world geometry, not players or props). There is no separate collision box for the orbit.

**When W has no object or no target exists**, the camera is detached and keeps its last position and rotation.

### 5. Whom the dead watch

The candidate list is the client's player list in ascending client id order. "Next living player after X in team T" means: the first player after X in that order (no wrap) who is not dead, has a player object, is not an observer (team 255), and, when T ≠ 0, is on team T.

To choose the next target from the current target X (X may be none):

1. T = the dead player's own team; in Last Man Standing and Hunted T = 0 (anyone).
2. Next living player after X in T. If none: the first living player in T from the start of the list.
3. If still none and `Enemydeadcam` is on: the first living player of the "other" team from the start: team 2 if T = 1, team 1 if T = 2, any team (0) if T = 255, otherwise team 1 (Quirk 7).
4. If still none: no target. The camera detaches and holds its view; the target panel is hidden.

The target is re-chosen automatically, every frame, whenever the current target is dead or is the own player. Choosing the killer for the enemy view clears when any re-choice happens.

**Name tags while dead** (`hud_crosshair.md` A11 for the rules when alive): shown for the watched player during the watch (whatever its team), for every teammate, and for whoever is under the view's aim ray. In Hunted, every other player's tag is shown while dead. Occluded tags are drawn at alpha 125 as when alive.

### 6. Keys while dead

Only in the "dead" state (not dying, not waiting), and only these physical keys (not rebindable commands):

| Key | Effect |
|---|---|
| Space, Ctrl, Shift | **Next target** (5) when in the watch phase. During the pull-back nothing happens (the pull-back length is not shortened). In Hunted they also drive the free cameras (7). |
| F7 | **Fast respawn**: only in TDM, One Man Army, CTF, Spy Hunt, Bombing Run, Seize and Secure, Hired Guns, Arms Race and VIP Elimination, only while the countdown runs, and only with the premium pass or a fast-respawn count > 0. The client then sends its respawn request on the next frame with the "fast" flag (`gear_perks.md` 16). |
| mouse | orbit (4) |
| wheel | zoom (4) |
| score key, chat keys | as when alive |

Fire, movement and the weapon keys do nothing while dead. There is no key to respawn (respawn is automatic) and no key to skip the pull-back.

### 7. Hunted: the hold and the free cameras

**Hold** (only Hunted, only when the killer is known). For the first 3500 ms after death the death-camera phase stays "none". While the dead body's death animation plays, the view rides the body's `Eyes` socket (position and rotation of the socket, moved out of walls by the engine's camera-collision step), at most 5.0 s (`MultiplayerDeathAttachCamMaxTime`). After the hold the pull-back runs from t₀ + 3500 for another 3500 ms, during which the view's yaw and pitch are reset every frame to those saved at the moment of death; then the watch. This answers `characters.md` open question 7: the head-attached death view is used only here, so a female body's low `Eyes` socket is seen only in Hunted. What the view shows between the end of a short death animation and the end of the hold was not settled (Open question 4).

**Free cameras** (Hunted only, from the watch phase). The death screen's target panel shows a key help line (`FreeCam_info`) whenever the death-camera phase is not "none".

- **Ctrl**: a free-flying camera starting at the current camera position and rotation (only when no free camera was used before in this death and the phase is the watch).
- **Shift**: a "static" free camera (not from the pull-back or "none" phases; not twice in a row). The first time in a map it starts 1000 u above the dead player with the view pitched a further 90° down; later times it returns to the position and rotation it had when last left.
- **Space**: back to the orbit (watch), and to the next target.

A free camera flies through the world without gravity: each movement frame the velocity is zeroed and the four movement keys give an acceleration of 300000 u/s² along the view's forward and right (doubled while run is held; console multiplier `SpectatorSpeedMul`, 1.0; Hunted adds 200000 to the base on every map load, Quirk 8), then velocity is damped by 0.9 per frame and zeroed below 0.1 u/s.

### 8. The kill-cam screen

One HUD panel loaded from `Hud_KillCam.xml` (root control `KillCam`), scaled like the rest of the HUD (x_r, y_r, `ui_system.md` H). Its child controls by role:

| Control | Role | Shown when | Contents |
|---|---|---|---|
| `KillerInfo` | killer | a killer other than self exists | two variants: `CamOn` (with health and armour) when `Enemydeadcam` is on or the killer is a teammate, else `CamOff`. Both: `name` (killer's name), `Rank` (the killer's level icon, or their rank-cover icon if they have one; in ladder matches the MMR badge or hidden). `CamOn` also: `hp` and `ap`, the killer's health and armour as the client last knew them, ⌊value⌋ clamped at 0, printed as three zero-padded digits (`075`) |
| `KillerWeapon` | killing weapon | the item for the kill message's weapon id has a large icon | `name` (item name), `weapon_img` (the item's large icon) |
| `DamageWnd` | last hit | the kill message had arrived when the dead state began | `Damage`: the last damage report's damage amount, truncated to an integer, ×2 for explosion, claymore, infected-explosion, burn and EMP damage types, then clamped to 1…999. `Damage_img`: the hit-zone icon of the last report's hit location, from the layout's `Observe_Mode` strings: `DamagePointIconHeadTex`, `...BodyTex`, `...ArmsTex`, `...LegTex`, `...NutTex` (groin); any other location uses the body icon |
| `Target` | watched player | a target exists and the camera is on it | `name`; `KillCount`: the string `$DeadCamKillCount` filled with N when N > 0 (N = how many times this player has killed you in this match, below), else empty; `FreeCam_info` (Hunted, 7) |
| `Banner` | tip | with the respawn gauge | `Banner_icon` and `banner_description`: one tip from the in-game tip data (below) |
| `Respawn` / `Respawn_EXPass` | countdown | respawn modes (9) | `respawn_gauge`, `timer`; fast-respawn texts `des_FastRespawn_2` and `des_FastRespawn` |

- **Visibility.** The panel switches itself on while the player is dying, dead or waiting (but in "waiting" the HUD level is "none", which draws only the always-drawn items, so the panel is not actually drawn), and off while alive, during a letterboxed cinematic or video, during an AI freeze, and for game masters. The killer, weapon and damage parts are filled from the kill message (11) and do not change afterwards: the killer's health and armour are a snapshot.
- **Kill-me count N.** The server keeps, per victim and per other player, the number of times that player killed the victim in this match (team kills included, suicides not, warm-up kills not; never reset during the match). It sends the killer's count with each kill message. The client stores it on the killer's entry, so later, when the camera watches that player, the target panel still shows it; players who never killed you show nothing.
- **Tips.** The tip data file `Guide.txt` (attributes archive) has records `InGame0`, `InGame1`, ... with `Enable`, `Order` (default 255), `DTXPath` (icon), `Desc` (a list of texts) and condition pairs `ConditionType` / `ConditionData<i>`. Condition kinds, in their stored order: user type, mode, map, damage type, item type, weapon, suicide, infected, spy. At room set-up the user-type, mode and map conditions pre-select records; at death the remaining conditions are tested against the death (damage type, the killing weapon's item type and id, suicide flag; "infected" and "spy" also require Quarantine or Spy Hunt and test whether the player's team is Alpha). Among passing records with the **lowest** `Order` one is picked uniformly at random, then one of its `Desc` texts at random. None passing → banner icon and text hidden.
- **Layout.** All positions, sizes, fonts and art come from `Hud_KillCam.xml`; the session could not read that file (Open question 2).

### 9. The respawn countdown

**Which modes count.** The countdown starts on the first frame dead (2 step 1) in: TDM, VIP Elimination, clan TDM, One Man Army, Arms Race, CTF, clan CTF, Bombing Run, Seize and Secure, Hired Guns, Squad Match 2, bot Elimination; co-op modes while the player has lives left (otherwise when a life or a respawn token is granted later); Spy Hunt in stage 1 only; Snowball Fight. In every other mode there is no countdown, no gauge and no tip banner. (Infected Ship and Cabin Fever 2 count only through the co-op rule.)

**Length.** The client's wait W is 7000 ms (reference build: 2000), lowered to min(W, `RespawnTime` × 1000) by the Relentless perk (`gear_perks.md`). It does not depend on the mode. Nothing else shortens it except F7 (6).

**The gauge.** Shown with the countdown's start. Its total is G = ⌊W / 1000⌋ s. With e the seconds since the start on the client's game clock:

```math
\text{gauge} = \Big\lfloor \frac{100\,e}{G} \Big\rfloor\ \%, \qquad r = G - e
```

- The `timer` text is "SS:hh": ⌊r⌋ and ⌊100 r⌋ mod 100, two digits each, separated by ":" (for example 6.53 s left → `06:53`). It is refreshed at most every 40 ms. When e ≥ G the gauge is 100 and the text `00:00`.
- A tick sound (interface attribute `RespawnCountSound`) plays whenever ⌊e⌋ changes (Quirk 9).
- Gauge bar art: layout strings `RespawnGauge_Perk` with Relentless equipped, else `RespawnGauge_Normal`. In the fast-respawn modes listed in 6, a premium-pass holder sees the separate `Respawn_EXPass` panel with a gold bar; others see `des_FastRespawn_2` filled from string `FastRespawnFreeDescription` (remaining count) and `des_FastRespawn` from `FastRespawnFreeCount` (remaining / maximum). The counts come from the lobby.
- The respawn perk (Relentless) is told the countdown started.

**The request.** On the first frame where the player is dead, the countdown runs, no request has been sent, and (elapsed OS ms > W, or the fast flag is set), the client sends one respawn request (`game_modes.md` S5: TDM-type, CTF-type or Snowball message; the fast flag only on the first two) and sets its death-camera phase back to "none". In Snowball Fight the request also waits for its team's ticket count to be above 0. Because the phase is "none" while the player is still dead, the phase machine of 2 starts over in the same frame with the **original** death time: the camera jumps back to the pull-back start above the body, then within two frames (t is already past 3.5 s) re-enters the watch on the next living teammate (the killer step is used up), and the 8 s killer-view end is armed again. This lasts until the "alive" message arrives, normally one round trip later (Quirk 12). A refused request is never retried (`game_modes.md` S5): the player keeps watching, with the gauge full at `00:00`, until the next round setup.

### 10. Respawn, round end, held joiners

- **Respawn accepted**: the server respawns the player (`game_modes.md` S5, S6) and sends "alive" and the new loadout. The client hides the death screen and countdown, returns to first person on its own player, and clears every death-camera flag.
- **Round end while dead** (round modes): nothing changes for the dead at the round-over phase, except that a live map camera (the stock end triggers switch on `Camera0`, `game_modes.md` S15) takes over every client's view and hides the death screen while it is live. At the next round setup everyone on teams 1 and 2 is respawned ("alive", `game_modes.md` S3 step 3) and the death camera ends.
- **Held joiners and observers** ("waiting" state): no death camera; a free first-person flying camera that starts 50 u above the player object's position, passes through the world, with the free-camera motion of 7. The HUD level is "none": only the always-drawn items (kill feed, scoreboard, item and objective markers) are drawn; the death screen, chat and score panel are not.

### 11. What the server sends and what the client works out

| Information | Who | How |
|---|---|---|
| Player state (dying, dead, alive, waiting) | server → victim only | guaranteed state message, same frame as the death |
| Damage direction d, damage type, hit location, damage amount of the last hit | server → victim | the damage report (`hud_crosshair.md` C), before the death |
| Kill: victim id, killer id (255 = none), whether a mercenary killed, the damaging object, multi-kill count, weapon id, hit location (head downgraded to torso for damage types that cannot head-shot), carried flag or gold, ice crush, infected, first kill, death damage type, warm-up flag, revenge flags, kill-me count, the victim's longest life and most kills in one life, Hunted's total hunted time | server → everyone | the kill message (`game_modes.md` S7) |
| Positions and health of living players | server → everyone | normal replication; the death screen reads the killer's copy |
| Respawn acceptance or refusal | server | respawn, or nothing |
| Fast-respawn counts | lobby → client | |
| Death-camera phases, timers, targets, zoom, orbit, death-screen contents, tip choice, countdown, respawn request | client only | |

The server is not told which player the dead are watching. (Only the unreachable first-person chase view would tell it, 2.)

## Per-frame order

Client, one rendered frame while dead (as read):

1. Network messages: state changes, damage report, kill message, respawn, round phase.
2. Player update: if dead with a countdown, the respawn request test (9); then, if a death time is stored, the phase machine (2, 7), the automatic target re-choice (5), and the sound listener moved to the camera.
3. Camera update: the dead player's yaw and pitch from the mouse (in the watch phase), the camera position (3 or 4), then the 8 s killer-view end test (2 step 4).
4. HUD update: the death screen picks up a state change (shows or hides parts), the watched-target panel when the camera's object changed, and the countdown (9).

Server, the death frame: damage applied → the death (floor snap, death animation, "dying", weapon drop, "dead", the dead flag to all clients, `characters.md` 10) → kill counting and the kill message (`game_modes.md` S7).

## Edge cases

- **Suicide** (own grenade, telefrag): no pull-back, the watch starts at once on the next living teammate; `KillerInfo` hidden; `KillerWeapon` is not updated, so it keeps whatever it showed for the previous death (Quirk 10); `DamageWnd` shown.
- **Fall or world death** (no player killer, id 255): `KillerInfo` hidden; the pull-back runs (Quirk 4) with d from the last damage report (for a fall, the fall's own direction if it had one); at 3.5 s the camera goes to the next living teammate; the weapon panel shows the weapon id in the kill message, usually none (hidden).
- **No direction** (d = 0): the pull-back stays at 20 u above the body; its look direction is undefined in the formula (Open question 5).
- **The killer dies first** (before 3.5 s, or during the enemy view): the target is dead, so the camera moves to the next living teammate by 5.
- **The killer is a teammate** (team kill): `KillerInfo` uses `CamOn` (health and armour shown) whatever the option; with the option off the camera then goes to the next teammate after the killer in the list.
- **The round ends during the pull-back**: the pull-back and watch run on; only a live map camera hides them (10).
- **Last teammate dies in an elimination mode**: the round ends at once (`game_modes.md` M2). Every dead teammate's camera loses its target; with `Enemydeadcam` on it moves to the first living enemy, else it detaches and holds its last view until the next round setup.
- **Watched player leaves**: their entry disappears from the list; the camera re-chooses on the next frame.
- **Respawn refused** (wait too short on the server, no fast respawns left): the player stays dead, the camera stays in the watch, the gauge full at `00:00`, until the next round setup (round modes) or forever in a single-round respawn mode until the match ends (`game_modes.md` S5).
- **A Hired Guns mercenary kills**: the killer is its leader; the pull-back looks along the damage, and the enemy view follows the mercenary itself, under the leader's name.
- **Free-for-all modes** (One Man Army, Arms Race, Spy Hunt stage 1): no teammates, so after the killer the camera falls back to step 3 of 5 (team 1 players only, Quirk 7) or holds.

## Quirks

1. **Frame-rate dependent pull-back.** The 5 u step and the 0.9 pull-in are per frame, so the pull-back finishes faster at high frame rates; its end distance (45 u) does not depend on frame rate. Keep (parity), or make it time-based behind an option.
2. **The 220 u stop never fires.** The tested distance is at most 50 u (45 + the 5 u step). Keep.
3. **The gauge is shorter than the wait when the wait is not whole seconds.** With Relentless (5.6 s) the gauge reaches 100 % and `00:00` at 5.0 s, and the request goes out after 5.6 s. Keep.
4. **"No killer" counts as a known killer for the pull-back.** The none id 255 passes the first test, so fall and world deaths get the 3.5 s pull-back while suicides do not. Keep.
5. **The orbit's pitch step uses an unnormalised axis**, so pitching shortens the orbit radius a little (at θ = 0.3 rad from 187.95 to 186.06 before the pull-in). Keep (it is the shape players saw).
6. **Inverted horizontal orbit (by the formula).** The orbit offset is turned by −ψ about +Y, so the camera's heading is 90° − ψ: moving the mouse in the direction that turns the living view right turns the orbit view left. Players saw this; confirm in the live game (Open question 3) before deciding.
7. **Enemy fallback in odd teams.** The "other team" of step 3 in 5 is team 1 for any team id other than 1, 2 or 255; in free-for-all modes this only finds the player whose team id is 1, if any. Keep.
8. **Hunted's free-camera boost accumulates**: +200000 u/s² per map load in Hunted, never reset. Keep for parity only if it is visible in play; it makes the free camera faster on each Hunted map in one session.
9. **Tick sounds.** The tick counter is shared across deaths: on the first death of a session ticks sound at 1, 2, …, G − 1 s; on later deaths also at 0 s. Keep.
10. **Stale weapon on suicide.** The killing-weapon panel is not refreshed for a suicide, so it shows the previous death's weapon (or the layout's default if there was none). Keep.
11. **Snapshot health.** The killer's health and armour on the death screen are taken once, from the client's copy when the kill message arrives. Keep.
12. **Camera blip at the respawn request.** Sending the request resets the death-camera phase while still dead, so for the round trip until "alive" the camera flicks to the pull-back start and back to a teammate (9). Players see a one-frame jump before respawning. Keep for parity, or hold the watch until "alive" behind an option.

## Test cases

Positions are in world units; A means P + (0, 20, 0) of the camera's anchor. "No walls" means the anchor rays hit nothing.

1. **Pull-back motion.** TDM, enemy kill, d = (0, 0, 1), own P = (0, 50, 0), no walls, one frame per step. → camera looks along −Z, level; after frame 1 at (0, 70, 4.5), frame 2 (0, 70, 8.55), frame 3 (0, 70, 12.195), frame 10 (0, 70, 29.309); never beyond 45 u from (0, 70, 0).
2. **Pull-back into a wall.** As 1 with a wall plane at z = 20 behind the body. → frames 1–4 as above (4.5, 8.55, 12.195, 15.476); frame 5's step crosses the wall: the camera is placed at z = 0.9 × 20 = 18.0 and stays there until 3.5 s.
3. **Pull-back length.** Any mode except Hunted, killer known, alive. t = 3499 ms → pull-back; t = 3500 ms (next frame) → watch phase begins.
4. **Enemy view on.** Elimination Pro, `Enemydeadcam` 1, killed by B (id 4, alive, health 42, armour 0 when the kill message arrives), teammates C (id 3) and D (id 5) alive. t = 3.5 s → camera orbits B; target panel shows B's name; `KillerInfo` uses `CamOn` with `042` / `000`. t = 8.0 s → camera orbits D (next living teammate after id 4).
5. **Enemy view off.** Same, `Enemydeadcam` 0. t = 3.5 s → camera orbits D (next living teammate after the killer's id 4), never B; `KillerInfo` uses `CamOff` (name and rank only).
6. **Respawn timing.** TDM, wait 7000, no perk, no F7. Death at t₀. → gauge starts at t₀ (total 7 s); `timer` reads `06:53` when 6.53 s remain; first frame with elapsed > 7000 ms sends one respawn request; in that same frame the camera is put back at the pull-back start (20 u above the body), and within the next two frames it is on the next living teammate, until "alive" arrives; no second request.
7. **Relentless.** TDM, `RespawnTime` 5.6. → gauge total 5 s: gauge 100 % and `00:00` at 5.0 s; request after 5.6 s.
8. **Fast respawn.** One Man Army, 2 fast respawns left, dead 1.0 s, press F7. → request with the fast flag on the next frame; the gauge panel showed "2" remaining and "2/max" before.
9. **No countdown in round modes.** Elimination Pro, killed by an enemy. → no gauge, no timer, no banner; pull-back 3.5 s; then watch; no respawn request ever.
10. **Suicide.** TDM, own grenade. → no pull-back: the watch starts on the first frame dead, on the next living teammate; `KillerInfo` hidden; the countdown runs.
11. **Fall death.** TDM, endless fall, no killer (id 255). → pull-back for 3.5 s, then the next living teammate; `KillerInfo` hidden.
12. **Orbit placement.** Watched P_W = (0, 50, 0), ψ = 0, θ = 0, z = 0, no walls. → camera (−148.5, 151, 0), heading +90° (looking along +X), pitch 28.61° down.
13. **Orbit yaw.** As 12 with ψ = 90°. → camera (0, 151, −148.5), heading 0°, pitch 28.61°.
14. **Orbit pitch.** As 12 with θ = 0.3 rad. → camera (−120.85, 185.91, 0), heading 90°, pitch 43.80°. With θ = −0.3 rad → (−162.88, 108.86, 0), pitch 13.42°.
15. **Orbit yaw and pitch.** ψ = 45°, θ = 0.3 rad. → camera (−85.46, 185.91, −85.46), heading 45°, pitch 43.80°.
16. **Pitch clamp.** Dead player pushes the mouse far down in the watch. → θ stops at 0.6854 rad (39.27°); camera (−69.95, 215.23, 0), pitch 64.28°.
17. **Zoom.** As 12, wheel rolled away 2 notches. → z = 80, camera (−211.71, 185.48, 0). Rolled towards the player 3 notches from 0 → z clamped to −93.97, camera (−74.25, 110.5, 0). Change of target → z back to 0.
18. **Cycling.** Watch on teammate id 3; teammates alive at ids 5 and 7; press Space twice. → id 5, then id 7; a third press → id 3 (restart from the start of the list).
19. **Target dies.** Watching id 5, which dies. → next frame the camera is on the next living teammate after id 5.
20. **Last teammate.** Elimination Pro, last living teammate killed, `Enemydeadcam` 1. → camera moves to the first living enemy in id order. With 0 → camera holds its last view, target panel hidden.
21. **Last Man Standing.** Dead player, living players ids 2 (team 7) and 4 (team 9). → watch cycles 2 → 4 → 2 regardless of teams.
22. **Damage panel.** Killed by a rocket blast whose last damage report says 37.8. → `Damage` shows `75` (⌊37.8⌋ × 2), `Damage_img` the body icon for a torso location. A head-shot of 140 → `140`, head icon. A report of 0.4 → `1`. A blast report of 600 → `999`.
23. **Kill-me count.** Player B kills you three times in one match. → at the third death the target panel, when watching B, shows `$DeadCamKillCount` with 3; watching teammate C (never killed you) shows no count.
24. **Hunted timeline.** Hunted round, killed by an enemy. t = 0–3.5 s: hold (view on the body's `Eyes` socket while the death animation plays); t = 3.5–7.0 s: pull-back; t ≥ 7.0 s: watch on any living player; no countdown; at t = 11.5 s the killer-view end moves the camera off the killer.
25. **Hunted free camera.** Hunted, watch phase, press Shift (first time in the map). → camera 1000 u above the own body, view pitched 90° further down; movement keys fly it; Space returns to the orbit on the next target.
26. **Held joiner.** S&D, live round with 5 players, join. → "waiting": free first-person flying camera 50 u above the player's position; no death screen, no chat or score panel drawn (kill feed drawn); at the next round setup → alive.
27. **Map camera.** S&D round won, the end trigger switches `Camera0` on while you are dead. → view from `Camera0`, death screen hidden while it is live; at the next round setup → alive.
28. **Name tags.** Dead in TDM watching an enemy. → that enemy's tag is shown, every teammate's tag is shown, other enemies' tags only under the aim ray.

## Open questions

1. **First-person spectating.** The client has a first-person chase view of the watched player (it mirrors that player's weapon, zoom and scope, `attachments.md`, `view_models.md`, `weapon_classes.md`), but no player input reaches it in this build: the only switch to it is from the third-person watch on a key path that was commented out, and the round-over message only switches away from it. `game_modes.md` M2 says the dead see "a chase camera behind the target or the target's first-person view (the player's camera mode)"; this spec says players get the third-person orbit only. Check in the live game whether any key gives a first-person view while dead.
2. **Death-screen layout.** `Hud_KillCam.xml` was not available. Read it from the install (positions, sizes, fonts, colours, art of `KillerInfo`/`CamOn`/`CamOff`, `KillerWeapon`, `DamageWnd`, `Target`, `Banner`, `Respawn`, `Respawn_EXPass`, `bg`) and add the geometry here.
3. **Orbit direction.** By the formula, moving the mouse to turn right turns the orbit view left (Quirk 6). Check in the live game; if it does not, the yaw sign used by the dead camera differs from the one assumed.
4. **Hunted's hold after the death animation.** During the hold the view rides the body's `Eyes` socket while the death animation plays. After the animation ends (before 3.5 s), the code read moves the camera to a first-person position on whichever living player the automatic re-choice picked, with the dead player's own view angles; that looks unintended. Check what is on screen between the end of a short death animation (about 1.4 s) and 3.5 s in Hunted.
5. **Zero damage direction.** When the last damage report had no direction (some world damage), the pull-back's look direction is built from a zero vector. Check what the view does (probably the previous rotation or a default).
6. **`DyingTime` in `game_modes.md`.** Its constants table calls `DyingTime` "Death-camera hold (Hunted only)" and S5 says only Hunted delays the respawn wait by it. The pull-back length is `DyingTime` in every mode; only the hold before it is Hunted's. The respawn wait itself starts at death in every counting mode (Hunted does not count at all). Reconcile `game_modes.md` S5 and its constants row.
7. **`Enemydeadcam` default.** `game_modes.md` M2 calls the enemy view "on by default": that is the console variable's default; the room writes it at match start from the room option, whose default is Off (Hunted On) per `lobby.md`. So in a normal room the enemy view is off unless the host turns Kill Cam on. Confirm in a stock room.
8. **Death-screen statistics.** `game_modes.md` S13 says "the local player's best kill streak, longest life and killer are shown on the death and result screens". The kill-cam panel read shows none of these; the kill message's longest life and most kills in one life update the scoreboard rows. Check whether the live death screen shows them (perhaps in a layout control filled elsewhere).
9. **Order of the kill message and the "dead" state.** The damage panel is decided on the first HUD frame after the state change, using whether the kill message had arrived. Both are sent in the same server frame; if they could arrive in different client frames, the damage panel would stay hidden for that death. Check under packet loss.
10. **Killer health source.** The `CamOn` health and armour come from the client's copy of the killer's health (the one the name tags use). Whether enemies' health is replicated exactly or only for teammates was not traced; if not exact, the shown value may be stale.
11. **Spectator speed.** The free-camera motion (zero velocity, 300000 u/s² for one movement step, then 0.9 damping) gives a speed that depends on the movement step; the resulting speed in u/s was not worked out. Measure in Hunted's free camera.
