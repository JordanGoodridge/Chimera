# Combat Arms: crosshair and basic HUD

Source basis: the client's in-game HUD (crosshair, damage feedback, health/armour/stamina block, weapon and ammo panel, kill feed, mode score panel, player name tags), the client's weapon spread bookkeeping that drives the crosshair, the HUD layout attribute data and the HUD XML layouts of a US-region data set. Behaviour that differs between regions or later Steam builds is not covered (see Open questions).
Status: draft

Sections: Summary; Units and conventions; Constants; Behavior (crosshair: A1 to A12; spread fraction: B; damage feedback: C; HUD: D1 to D8); Per-frame order; Edge cases; Quirks; Test cases; Open questions.

---

## Summary

- The crosshair is drawn from 1-pixel lines, not a texture. It is a plus by default, in solid green. The gap between the arms is the weapon's current spread, expressed in the same perturb units the shot code uses: one perturb unit is one reference pixel at 1024 px width. A crosshair that looks tight therefore means the shot really is tight.
- Each shot gives the crosshair a short kick lasting 0.1 s: the arms jump 4 px outwards and grow 6 px longer, then return linearly. The crosshair turns dark red for 0.5 s after you damage an enemy, which is the hit indicator.
- When a scope overlay is up there is no crosshair at all. Aiming at another player does not change the crosshair. It changes their name tag instead: an enemy's tag appears only while your aim ray is on them.
- The HUD is laid out on a 1024x768 reference canvas and stretched to the real resolution. X and Y are stretched independently.
  - Bottom left: health, armour and stamina, with a stance silhouette.
  - Bottom right: weapon name, clip/reserve as zero-padded 3-digit numbers, and the fire-mode icon.
  - Top left: kill feed.
  - Top centre: the mode's score panel.
- Low health (30 % or less) turns the health block red and plays a heartbeat. There is no low-ammo warning of any kind.

## Units and conventions

- **Reference canvas:** 1024 x 768 "reference pixels" (ref px), origin at the top left, x to the right, y **down**.
- **Scale factors:** for a W x H screen,

```math
x_r = \frac{W}{1024}, \qquad y_r = \frac{H}{768}
```

- **Screen pixels (px):** a rect given as [x0, x1) x [y0, y1) covers the pixels whose integer coordinates fall in it.
  - The original draws every crosshair rect and line 0.5 px up and left of the positions given here. That shift exists only to land on pixel centres under the original renderer's rasterisation rules; an implementation should hit the same pixels.
- **Colours:** (R, G, B) or (R, G, B, A), each 0 to 255. A is 255 unless stated.
- **Time:**
  - Crosshair firing kick: game client time in seconds.
  - Hit indicator, damage overlay, kill feed, stamina and spread step timing: a millisecond wall clock.
  - The HUD itself runs once per rendered frame. There is no fixed HUD tick.
  - Precisely (Open question 13): every HUD timer is *checked* once per rendered frame, in the HUD update, but none of them counts frames except the damage overlay's alpha step. The hit indicator and the kill-feed lines compare the OS millisecond clock (whole ms, real time) with their start stamp and switch off on the first frame where the difference is **greater than** their life (500 ms, 30000 ms). The crosshair kick compares the game client's time in seconds (the time the frame is simulated at) with the weapon's last fire time. None of them is tied to movement.md's 15 ms tick.
- **Angles:** radians. Player yaw increases when turning right (see Open questions).
- **Game distance:** world units. The scope range readout divides by 40 (see A9).
- **Spread fraction r:** dimensionless, normally in [0, 1] (section B).
- **Perturb units:** weapon spread units, read from weapon data (MinPerturb, MaxPerturb, ...).

## Constants

| Name (ours) | Value | Unit | Meaning |
|---|---|---|---|
| ref canvas | 1024 x 768 | ref px | HUD and crosshair reference resolution |
| arm length L0 | 7 | ref px | crosshair arm length (console variable `CrossHair_DefaultLength`) |
| kick offset K_off | 4 | ref px | outward jump of the arms on a shot (`CrossHair_FiringOffset`) |
| kick length K_len | 6 | ref px | extra arm length on a shot (`CrossHair_FiringAddLength`) |
| kick duration T_kick | 0.1 | s | linear return of the kick (`CrossHair_FiringDuration`) |
| line thickness | 1 | px | every crosshair line, at every resolution (not scaled) |
| default crosshair colour | (0, 255, 0, 255) | | weapons without CrossHairColor |
| alt weapon colour seen in data | (110, 191, 228) | | used by 5 weapons, together with CrossHairOffset = 45 |
| hit-indicator colour | (208, 0, 0, 255) | | crosshair colour while the hit indicator is on |
| hit-indicator time | 500 | ms | after the latest damaging hit on an enemy |
| damage overlay life | 1000 | ms | damage-taken overlay and direction indicator |
| damage overlay alpha step | -2 | alpha per HUD frame | frame-rate dependent fade |
| direction indicator square side S | 338 | ref px | layout DamageIndicatorUVSize.x |
| direction indicator texture size | 512 x 256 | texels | layout DamageIndicatorSize |
| damage sectors | 12 | | 30 degrees each |
| sector-to-turn factor | 0.1666 | turns of pi per sector | used literally, not 1/6 |
| dot-sight overlay | centre (512, 384), 128 x 128 | ref px | layout DotSightPos/DotSightSize |
| hit overlay rect | (0, 0), 1024 x 1024 | ref px | layout DamagePos/DamageSize |
| range readout position | (490, 400) | ref px | right-aligned scope range text |
| range readout divisor | 40 | units per displayed unit | shown with 1 decimal |
| range readout colours | (255, 198, 0) in range, (255, 0, 0) out of range | | |
| low-health threshold | 30 | % | floor(100 h / h_max) <= 30 |
| low-health colour | (255, 0, 0) | | layout HPMaxRedColor |
| heartbeat period | 1.0 | s | layout HPHearbeatSound_Velocity |
| kill feed lines | 5 | | maximum visible |
| kill feed line life | 30000 | ms | layout KillMessageLifeTime |
| kill feed line height | 20 | ref px | |
| weapon list show time | 4000 | ms | weapon list shown after a slot key |
| name-tag hit flash | 0.2 | s | layout DamagedForcemarkDelay |
| name-tag occluded alpha | 125 | | 255 when visible |
| spread step rate | 80 | steps/s | spread bookkeeping (section B) |
| max spread steps per frame | 100 | | |

Layout keys CrosshairBarMin = 8, CrosshairBarMax = 32, CrosshairGapMin = 5, CrosshairGapMax = 25, HitCrossHairSize = 6, HitCrossHairMAXPerturb = 50 and HITCrossHairTime = 0.2 exist in the data but have **no visible effect** (see Quirks).

## Behavior

### A1. Crosshair placement and scaling

- **Centre:** cx = floor(W / 2), cy = floor(H / 2), in px. The single "centre pixel" used by the cross shapes is (cx - 1, cy - 1).
- **Scaling:** every length is multiplied by x_r, **including vertical ones**, so the crosshair keeps its proportions on any aspect ratio. Line thickness is always 1 px.
- **Exception:** the weapon's CrossHairOffset (A4) is added in raw px and is not scaled.

### A2. Spread gap

- Inputs: the weapon's minimum and maximum perturb, m and M (A3), and the current spread fraction r (section B).

```math
g = \big(m + r\,(M - m)\big)\,x_r
```

- g is the gap in px.
- r is not clamped here. The data keeps it in [0, 1], but a larger r widens past M.
- r is used **unrounded** (Open question 14). The shot code instead rounds the spread part of the cone down to whole perturb units, `n = m + ⌊(M − m)·p⌋·DynamicPerturbFactor` (weapon_firing.md), and the crosshair ignores `DynamicPerturbFactor`. So the gap can sit up to one unit (×x_r px) wider than the cone the next shot will use, and differs more on a weapon with a factor other than 1.
- r is read in the HUD update, **after** this frame's spread sub-steps, while the shot of this frame used the meters from before them. On the frame of a shot the crosshair already shows the shot's own growth.
- The bars of the plus never change length with spread. Only the gap changes.

### A3. Which min and max perturb apply

- Start from the weapon's MinPerturb and MaxPerturb, each scaled by (1 + the weapon-upgrade modifier, 0 by default) and truncated to an integer.
  - MaxPerturb is also scaled by a set-item rate, 1 by default.
- **Zoomed** (zoomed in, with no scope overlay) and the weapon has ZoomedMaxPerturb > 0: max = ZoomedMaxPerturb + S_max.
  - Likewise, if ZoomedMinPerturb > 0: min = ZoomedMinPerturb + S_min.
  - S_min and S_max are summed from the attachments (section A3 note).
- **Otherwise, if burst mode is active** and TripleShotMaxPerturb (resp. TripleShotMinPerturb) is > 0, that value is used instead.
- **Crouch override:** if the weapon has **both** DuckPerturb > 0 and MoveDuckPerturb > 0, and the crouch input is held:
  - min = MoveDuckPerturb while moving or running;
  - min = DuckPerturb when still.
  - This replaces the zoomed or burst min.
- **Scope attachment:** if a scope attachment is fitted, min = trunc(min + min * p), where p is that scope's minimum-perturb percentage.
- **Perks:** a perk hook may reduce min, max and r afterwards. Without perks it does nothing.
- All of these are integers before they enter A2.
- **Attachment sum quirk:** S_min and S_max are accumulated over the fitted attachments in slot order as S <- 2S + v_k. One attachment gives S = v; two give 2 v_1 + v_2. Kept deliberately.

### A4. Firing kick and weapon offset

- Let t be the time since the weapon last fired, from game time in seconds, and let u = t / T_kick.
- While 0 <= t < T_kick:

```math
A = K_{len}\,(1-u)\,x_r, \qquad O = K_{off}\,(1-u)\,x_r
```

- Otherwise A = O = 0.
- Every shot restarts t, so at fire intervals of 0.1 s or less the kick never fully relaxes.
- Let o be the weapon's CrossHairOffset: an integer, in px, unscaled, default 0. Then

```math
D = g + O + o, \qquad \ell = L_0\,x_r + A
```

- D is the inner offset of the arms and ℓ is the arm length.

### A5. Shapes

The shape comes from a list of seven, selected by index (A6). The geometry of each element:

- **Left arm:** row cy - 1, 1 px tall, x in [cx - D - 2 - ℓ, cx - D - 2).
- **Right arm:** row cy - 1, x in [cx + D + 1, cx + D + 1 + ℓ).
- **Top arm:** column cx - 1, 1 px wide, y in [cy - D - 2 - ℓ, cy - D - 2).
- **Bottom arm:** column cx - 1, y in [cy + D + 1, cy + D + 1 + ℓ).
- These leave D + 1 empty pixels between each arm and the centre pixel, on every side.
- **Dot:** the single pixel (cx - 1, cy - 1).
- **Diagonal arms (X):**
  - Four 1-px lines centred on (cx, cy), not on the centre pixel. They point up-left, up-right, down-right and down-left at exactly 45 degrees.
  - Each runs from radius D + 2 to radius D + 2 + ℓ.
  - For example, the up-left one goes from (cx - (D+2)/√2, cy - (D+2)/√2) to (cx - (D+2+ℓ)/√2, cy - (D+2+ℓ)/√2).
- **Circle:**
  - A 32-segment polyline circle centred on (cx, cy), radius R = D + A + 1.
  - Vertices sit at angles k * 2π/32 starting from straight up.
  - Unlike the bars, the circle also grows by A when firing.
- **Brackets ("< >"):**
  - The left bracket's apex is at (cx - (2 L0 x_r + g + A + 1), cy). The apex uses g, not D.
  - Two arms of length λ = D + A + L0 x_r + 2 leave the apex at 45 degrees, towards lower-right and upper-right.
  - The right bracket is the mirror image about x = cx: its apex is at cx + (2 L0 x_r + g + A + 1), and its arms go towards upper-left and lower-left.

Shape list (the index is weapon data CrossHairType, or the shape number of a crosshair item):

| Index | Elements | Look |
|---|---|---|
| 0 | left, right, top, bottom | plus, open centre (default) |
| 1 | plus + dot | |
| 2 | left, right, bottom, dot | plus without the top arm |
| 3 | four diagonals + dot | X |
| 4 | circle + dot | |
| 5 | circle | "double circle" (see below) |
| 6 | left and right brackets + dot | < • > |

- **Double circle:** when the **weapon's** CrossHairType is 5, a second circle is drawn on top. It uses:
  - max' = M + floor(M / 2);
  - min' = m + m * N_wide, where N_wide is the weapon's wide-fire pellet-count field;
  - the same r;
  - its own firing kick, timed from the last **alternate** fire.
- A crosshair item that selects shape 5 gets one circle only, with no dot.

### A6. Shape and colour selection

- **Shape:**
  1. If the current weapon's CrossHairType is 1 to 6, use that shape.
  2. Otherwise, if the player has a crosshair item equipped with a shape 1 to 6, use the item's shape.
     - The item is ignored in rooms restricted to tournament items, unless it is on the allowed list.
  3. Otherwise use shape 0.
  - Index 0 or any out-of-range value means "no preference".
- **Colour, re-evaluated every frame, first match wins:**
  1. Hit indicator on (A10): (208, 0, 0, 255).
  2. Dot-sight mode is engaged and a dot-sight scope attachment with a valid colour is fitted: that attachment's crosshair ARGB.
  3. A crosshair-colour item is equipped and its colour is nonzero: that colour, from the colour table.
  4. The weapon's CrossHairColor (default (0, 255, 0)), if weapon data exists. Otherwise the previous frame's colour is kept.
- The colour applies to every element, including the diagonals, the circle and the brackets.

### A7. When the crosshair is drawn

The crosshair is drawn only if all of these hold:

- No cutscene video is playing and the player is not AI-frozen.
- The crosshair has not been toggled off. It starts on.
- No scope overlay is active. With a scope overlay the only thing drawn is the range readout (A9), if that scope has one.
- The player is not dead in the free-look or chase death camera. In first-person spectating it **is** drawn, using the spectated player's scope attachment and weapon.
- A current client weapon exists.
- The weapon's DisableCrosshair is not 1.

### A8. Dot-sight overlay

- Weapons with a DotSightName draw that texture (role: weapon's reticle image) as a rect:

```math
x = \lfloor (512-64)\,x_r \rfloor,\; y = \lfloor (384-64)\,y_r \rfloor,\; w = \lfloor 128\,x_r \rfloor + 1,\; h = \lfloor 128\,y_r \rfloor + 1
```

- At 1024x768 this is (448, 320), 129 x 129.
- It is drawn **before** the DisableCrosshair test, so crosshair-less weapons still show it.
- On weapons that also have a crosshair it is drawn a second time, after the crosshair. See Quirks.

### A9. Scope range readout

- **When it shows:** the fitted scope attachment has a range readout style and a target range R_s > 0.
- **Measurement:** each frame a ray is cast from the camera along its forward direction, R_s units long, against world and objects. Non-solid objects are ignored.
- **Text:** if the ray hits at distance d (truncated to an integer) the text is d / 40 with one decimal. Otherwise the text is empty.
- **Colour:**
  - (255, 198, 0) when d <= Range_eff;
  - (255, 0, 0) otherwise.
  - Range_eff = Range + trunc(Range * Σ attachment range %), with Range from the weapon data.
- **Placement:** right-aligned at (490, 400) ref, 20-point HUD font. It scales with x_r and y_r.
- It is drawn whether or not the scope overlay is up.

### A10. Hit indicator (you hit someone)

- **Trigger:** the client is told that the local player damaged a player whose team differs from the local player's team. Damaging a teammate does nothing.
  - "Enemy" is purely a comparison of team ids: the victim's team id differs from the local player's (Open question 15). There is no mode check in the hit indicator or in the name tags' enemy/teammate rules. In modes without teams the result depends on which team id the server gives each player, which was not traced (see Open question 15).
- **On:** the crosshair colour is (208, 0, 0, 255) while now - t_hit <= 500 ms, where t_hit is the time of the **latest** such hit.
- **Off:** on the first HUD frame where the elapsed time exceeds 500 ms.
- **Reset:** the end of a round clears it immediately.
- **Name tag:** the same hit starts the 0.2 s name-tag flash on the victim (A11).
- There is no separate hit-marker graphic.

### A11. Aiming at players

- **The crosshair:** does not change shape or colour when aimed at a teammate or an enemy.
- **The aim target:** the first thing hit by a ray from the camera along its forward direction, up to 100000 units. Walls block it. Hitting a character's hit box counts as that character.
- **Name tags:**
  - A tag floats over each player, positioned at the model's name socket and projected to the screen. It is centred horizontally and shifted up by half its height.
  - Precisely (Open question 10): the anchor is the world position of the model socket named "NAME_socket" in the character's current pose, every frame. A model without that socket uses the character object's position (the hull centre). There is no world-space offset added; the only offset is the half-height shift on screen.
  - There is **no distance limit**. The projection's only hide rule is a tag behind the camera: when the anchor's view-space depth is ≤ 0 the tag is hidden. Enemies are further limited by the aim-target rule (a ray of up to 100000 units).
  - The tag holds the name, an HP bar and badges.
- **While alive, outside special modes:**
  - **Teammates:** always shown. Name colour (161, 197, 233) at alpha 200.
  - **Enemies:** shown only while they are the aim target, or carry the flag, or are revealed by a scanner item. Name colour (215, 17, 18) at alpha 200.
  - **Enemy hit flash:** for 0.2 s after you hit an enemy, their name colour is (250, 80, 0).
  - **Dead players:** (80, 80, 80).
  - **The local player:** never shown.
- **Occlusion:** a shown tag is drawn at alpha 255 if the camera can see the tag point and at alpha 125 if the view is blocked.
- **HP bar:** shown only while that player is the aim target and alive, and only for teammates. Enemies show it only with a "state viewer" weapon or item.
- **While dead:** teammates, the aim target, and the chase-camera subject are shown.

### A12. Present in the data or code but not drawn

- An over-crosshair "friendly" icon and a teammate HP number, at reference (496, 368) and (530, 359), are computed every frame but never rendered.
- A drawn scope reticle (black posts with gold highlights) is never called.
- The "hit crosshair" size, perturb and time keys are read and never used.
- Do not implement these as visible features.

### B. Spread fraction r (summary; normative in weapon_firing.md)

The crosshair reads r = max(r_fire, r_move), the same value the shot code uses. The crosshair-relevant behaviour, with this data set's values:

- **Stepping:**
  - r is advanced in fixed steps of Δ = 1/80 s.
  - Each frame computes n = floor(elapsed_ms / 12.5) since the last reset, capped at 100.
  - If n = 0 and the weapon fired this frame, n = 1.
  - When n > 0 the reference time is reset to "now". The **fraction left over is discarded.**
  - Only the first step of a frame counts as "fired".
- **Firing spread:**
  - A firing step adds 3.0 * Δ * PerturbIncSpeed, then caps at the firing-state limit:
    - FirePerturb when still;
    - FireMovePerturb when moving;
    - FireDuckPerturb when crouched;
    - FireMoveDuckPerturb when crouched and moving.
    - Zoomed variants apply when present. The body-type modifiers are 0 in this data.
  - After the last shot, r_fire holds for the ammo's DecDelay ms.
  - It then falls by 3.0 * Δ * PerturbDecSpeed per step, linearly, to 0.
- **Movement spread:**
  - target = movement fraction (0 to 1) * state value. The state value is WalkPerturb when walking, RunPerturb when running and JumpPerturb after a jump.
  - **Rising:** r_move rises by min(3.0 Δ, |target - r_move|) * PerturbIncSpeed per step, capped at the target.
  - **Falling:** r_move falls by min(9.0 Δ, |target - r_move|) * PerturbDecSpeed per step. Near zero this makes the tail geometric (r <- (1 - PerturbDecSpeed) r).
  - While airborne after a jump the falling branch rises instead.
  - Camera rotation can stop movement spread from falling but can never raise it above the target. Turning on the spot therefore never widens the crosshair.
- **Global speeds:** the rising and falling speed scalars come from the weapon file's global section. MovementPerturbIncreaseSpeed = 3.0, MovementPerturbDecreaseSpeed = 3.0, FiringPerturbIncreaseSpeed = 3.0 and FiringPerturbDecreaseSpeed = 9.0, but **cross-wired**:
  - movement spread uses the "Firing" pair (3.0 / 9.0);
  - firing spread uses the "Movement" pair (3.0 / 3.0).
  - Console variables `PerturbIncreaseSpeed` and `PerturbDecreaseSpeed` override the movement-side pair; `PerturbFiringIncreaseSpeed` and `PerturbFiringDecreaseSpeed` override the firing-side pair.
- **Zero spread:** a weapon whose MaxPerturb is 0 has r = 0 at all times.
- **Example rifle (M4A1):**
  - MinPerturb 3, MaxPerturb 100, DuckPerturb 2, MoveDuckPerturb 3.
  - ZoomedMinPerturb 2, ZoomedMaxPerturb 30.
  - PerturbIncSpeed 1.3, PerturbDecSpeed 0.3.
  - Ammo: FirePerturb 0.30, FireMovePerturb 0.38, WalkPerturb 0.04, RunPerturb 0.25, JumpPerturb 0.70, DecDelay 95 ms.

### C. Damage-taken feedback

- **Trigger:**
  - The server reports damage to the local player with a health loss > 0, and the type is not a heal.
  - Hits fully absorbed by armour send no report, so they show nothing.
  - Same-team burn or poison damage is not reported when friendly fire is off, unless it is self-inflicted.
- **Two pieces:**
  - a hit overlay (role: red edge vignette) over the screen;
  - a direction indicator (role: arc pointing at the source), around the screen centre.
- **Both pieces share one alpha:**
  - set to 255 on each new hit;
  - lowered by 2 every HUD frame (floor 0) while now - t_dmg < 1000 ms;
  - removed on the first frame at or after 1000 ms.
- **Hit overlay rect:** (0, 0, floor(1024 x_r), floor(1024 y_r)). It is taller than the screen, so only the top 768/1024 of the texture is visible (quirk).
- **Direction:**
  - The report carries the direction the damage travelled, d, from the source towards the player.
  - With F and Rt the player's forward and right vectors at the moment of the hit:

```math
\theta = \pi + \operatorname{atan2}(F\cdot d,\; Rt\cdot d) \in [0, 2\pi], \qquad s = \frac{\theta}{\pi/6}
```

  - s is continuous, not rounded to a sector: 3 = front, 0 or 12 = right, 6 = left, 9 = behind.
  - Each frame the indicator's rotation is

```math
\varphi = \pi\,\big(0.1666\,(s-3) + 0.25\big) + (\psi_{now} - \psi_{hit})
```

  - ψ is player yaw. ψ_hit is stored when the hit arrives, so the indicator keeps pointing at the source while you turn.
- **Geometry:**
  - The indicator is a square of side S = 338 ref px, centred on (512, 384) ref.
  - Its corners k = 0..3 are at

```math
P_k = \Big( \big(512 + \tfrac{S}{\sqrt 2}\cos\alpha_k\big)\,x_r,\;\; \big(384 + \tfrac{S}{\sqrt 2}\sin\alpha_k\big)\,y_r \Big), \qquad \alpha_k = -\big(\varphi + k\tfrac{\pi}{2}\big)
```

  - P0, P1, P2 and P3 carry the texture's top-left, top-right, bottom-right and bottom-left corners.
  - The texture's top edge (P0 to P1) therefore faces the source. For a source straight ahead, P0 is upper-right and P1 is upper-left, so the image is mirrored left-to-right.
  - UVs: u in [0, 338/512], v in [0, 338/256]. The v range overruns the texture (quirk).
  - On non-4:3 screens the square is stretched by x_r and y_r.
- Only the latest hit is shown. A new hit replaces the direction and restarts the fade.

### D1. HUD layout model

- **Coordinates:** every HUD widget has a position and size on the 1024x768 canvas. A child's position is relative to its parent; the absolute reference positions below already include all parents.
- **Scaling:** on screen, x and width are multiplied by x_r, and y and height by y_r. All basic-HUD widgets are anchored top-left with proportional ("rate") movement, so the whole HUD stretches with the screen and nothing is pinned to an edge in px.
- **Separators in layout data:** both "," and "." separate the numbers. "340.80" means (340, 80), and "216.216.216" means (216, 216, 216).

### D2. Health, armour, stance

- **Visibility:** shown while alive and not spectating. Hidden while the death camera is up, in letterbox, during video, during AI freeze, and in observer mode.
- **Update:** every frame. Value texts change only when the value changes.
- **Background panel:** (-7, 717), 238 x 49 ref. Its inner plates are at (67, 728) 76 x 16, (150, 728) 76 x 16, (67, 747) 159 x 16 and (2, 747) 62 x 16.
- **Labels:** localized "HP" at (72, 728), "AP" at (155, 728), "SP" at (72, 747) and a set-bonus label at (5, 747). Bold 16, colour (216, 216, 216), with a shadow.
- **HP value:**
  - Centred in a box at (97, 721), 40 x 30. Numeric font 25, white (255, 255, 255), with a shadow.
  - h_max is the player's max health and I is the model's InterfaceMaxHealth (100 for standard soldiers; falls back to h_max).

```math
\text{shown} = \begin{cases} \lfloor \max(1,\; h\,I/h_{max}) \rfloor & h > 0 \\ 0 & h \le 0 \end{cases}
```

- **AP value:** armour as a plain integer, clamped at 0. Centred in a box at (180, 721), 40 x 30, same font.
- **Stance silhouette:**
  - At (0, 654), 64 x 128.
  - Shows the crouch image when the crouch blend is >= 0.5 and the standing image otherwise.
  - Special variants exist for carrying the flag, gold or bomb, and for being infected.
- **Low health:**
  - Condition: p = floor(100 h / h_max) <= 30, and the player is not infected.
  - The HP value, AP value, silhouette tint, and the "HP" and "AP" labels turn (255, 0, 0, 255).
  - The "SP" and set labels do not change.
  - Above 30 %, everything returns to its default colour (values white, labels (216, 216, 216), silhouette untinted).
  - While low, a heartbeat sound plays whenever at least 1.0 s has passed since the previous heartbeat. The timer survives between low-health episodes.
- Armour has no threshold colour.

### D3. Stamina gauge

- **Placement:** at (97, 751), 124 x 16. The bar starts at (99, 753), is 16 tall and is at most 120 ref px wide.

```math
\text{fill} = \Big\lfloor \frac{\text{stamina}}{\text{stamina}_{max}} \cdot 100 \Big\rfloor, \qquad \text{bar width} = \Big\lfloor \text{fill}\cdot\frac{120}{100} \Big\rfloor
```

- The bar is hidden at fill <= 0 and full at fill >= 100.
- **Update:** every frame. The stamina value itself is advanced in the same per-frame step, by elapsed milliseconds times the drain or refill rate (movement.md owns the rates). So the gauge has no lag of its own.
- No colour change at low stamina.

### D4. Weapon and ammo panel

**Panel and visibility**

- The panel's origin is (780, 696).
- Hidden while infected, and in all cases where D2 is hidden.
- Shown only when the current weapon's class is one of the gun classes (assault, SMG, sniper, support, pistol and the like, not "etc") and it is not the mission bomb.
  - *Correction* (Open question 16): "class" here is the weapon's **inventory class**, the order list it appears in under the weapons file's weapon order (attribute_data.md): 1 primary, 2 secondary, 3 melee, 4 throw, 5 special, 6 mission. It is not `Guntype`. The panel is shown for classes 1 to 6 unless the weapon's `Guntype` is 8 (mission bomb), and hidden for a weapon in no list (class 0). So the knife, grenades and specials do show the panel (name, plus counts if their ammo is finite); only the fire-mode icon is limited (below).

**Weapon name**

- The weapon's display name, right-aligned in a 250 x 10 box at (769, 716), so it ends at x = 1019 and is centred on y = 721.
- Bold 16, colour (222, 222, 222), with a stroke.
- It changes only when the weapon ID changes.

**Ammo text**

- Clip: at (910, 742), numeric font 26, colour (240, 240, 240).
- "/": at (962, 742), bold 25, colour (180, 180, 180).
- Reserve: at (974, 741), numeric font 25, colour (180, 180, 180).
- All three have a stroke.
- reserve = total rounds held for that weapon - rounds in the clip.
- Each number is printed as floor(n/100), then floor(n/10) mod 10, then n mod 10, concatenated. So 7 -> "007", 30 -> "030", 1200 -> "1200".
- Updated every frame.

**Infinite-ammo weapons (knife etc.)**

- The clip, slash, reserve and fire-mode icon are hidden.
- The name stays.

**No low-ammo state:** no colour, blink or text changes at low or zero ammo.

**Fire-mode icon**

- At (926, 731), 128 x 16. Only for the assault and SMG classes; hidden for other classes.
  - *Correction* (Open question 16): those two "classes" are inventory classes 1 and 2, i.e. every weapon in the primary list (rifles, SMGs, snipers, machine guns, shotguns...) and every weapon in the secondary list (pistols). Snipers and pistols get an icon.
- Which icon:
  - **Burst active:** the 2-round burst icon if the weapon's burst count is 2, else the 3-round icon.
  - **Otherwise, weapon has selectable auto:** the full-auto icon in auto mode, the single-shot icon in single mode.
  - **Otherwise, auto-only weapon:** the full-auto icon.
- Exactly, from the attribute data (attribute_data.md):
  1. `TripleShotMode` = 1 and burst mode is on: the 2-round icon if `TripleShotRoundNum` = 2, else the 3-round icon.
  2. Else `RapidFireCount` = 1 (a semi-automatic weapon): the full-auto icon if its continuous-fire mode (`ContinuousFire` = 1) is switched on, else the single-shot icon.
  3. Else `RapidFireCount` = 0 (fully automatic): the full-auto icon.
  4. Else (`RapidFireCount` ≥ 2): no icon.
  - "Selectable fire modes" are therefore `TripleShotMode` (burst toggle) and `ContinuousFire` (continuous-fire toggle on a semi-automatic weapon). The `HudFireMode` field is read from the data but nothing uses it.

**Weapon icon and list**

- There is no weapon icon in this panel.
- The weapon's large icon appears only in the weapon list. The list replaces this panel for 4000 ms after a slot key, stacked upwards from y = 768. inventory.md owns it.

### D5. Kill feed

- **Placement:** top left. Lines are 350 x 20 ref; line i (0 = oldest visible) is at (0, 45 + 20 i).
- **Capacity:** at most 5 lines. A sixth kill drops the oldest line and moves the rest up.
- **Lifetime:** each line lasts 30000 ms from its arrival. When it expires the remaining lines close up.
- **Visibility:**
  - Shown even while dead.
  - Not used in co-op mission modes.
- **Line content, left to right, from x = 5:**
  1. **Killer name.** Bold 12 with a stroke, at y + 3.
  2. **Weapon icon,** 64 x 20. Replacements for the weapon:
     - crush or fall-out-of-world deaths: the suicide icon;
     - no killer: the barrel icon (explosion or bump), the fire icon (burn), the poison icon or the bleeding icon;
     - AI killer: the NPC icon.
     - If none of these applies and the weapon has no icon, the slot is hidden but its 64 ref px are still skipped.
  3. **Badge icons,** 32 x 20, in this order: revenge, headshot, groin shot, flag carrier (colour by flag team), gold carrier, ice crush, VIP, VIP rank.
     - The first badge takes the first slot. Every later badge is written into the second slot, so with three or more only the last one survives.
     - x still advances by 32 for each badge (quirk).
  4. **Victim name.**
- **Name placement:** each element is placed at the running x, which then advances by that element's width (a text's width is its rendered width).
- **Name colours:**
  - names on your team: (124, 190, 221);
  - other teams: (218, 115, 115);
  - text alpha 150.
- **No killer** (environment or self): the killer slot shows the victim's name in the victim's colour, and the victim slot still shows the victim name as well (see Open questions).

### D6. Score panel (team deathmatch)

- **Panel:** a top-centre panel of 340 x 115 at (342, 0).
- **Mode name:** centred in (347, 3), 220 x 16. Numeric font 14, colour (218, 218, 218).
- **Goal text:** centred in (572, 3), 105 x 16, same style. The text is the mode's goal string for the room's kill target.
- **Team rows:**
  - Alpha: label box (347, 23) 90 x 18, score box (612, 23) 65 x 18.
  - Bravo: label box (347, 43), score box (612, 43).
  - Labels are bold 18 and scores numeric 18, centred, normal colour (170, 170, 170).
- **Own-team highlight:**
  - Your team's label and score use their highlight colour: (223, 31, 20) for Alpha, (13, 109, 255) for Bravo.
  - A highlight background appears behind your row: at (344, 18) 336 x 40 for Alpha, at (344, 26) for Bravo.
- **Progress bars:**
  - Alpha: (448, 32) 157 x 16. Bravo: (448, 43). The bar is inset by (1, 1), 8 tall and at most 155 wide.

```math
\text{gauge} = 100\cdot\frac{\text{kills}}{\text{goal}}, \qquad \text{bar width} = \Big\lfloor \text{gauge}\cdot\frac{155}{100}\Big\rfloor
```

  - The bar is hidden at 0 kills or goal 0, and full at kills >= goal.
- **Updates:** on every kill message and when the goal is set. Scores are plain integers.
- **Hidden:** while the round-result screen is up.

### D7. Round timer

- **Team deathmatch:** this panel has **no timer**.
- **Round-based modes** (elimination pro, search and destroy, capture the flag, ...):
  - They add a strip at (344, 63), 336 x 17, with the time centred at about (444, 60). Numeric font 20, white.
  - Format, with t the remaining time in ms, truncated:

```math
\text{MM} = \lfloor t / 60000 \rfloor, \qquad \text{SS} = \lfloor (t \bmod 60000)/1000 \rfloor
```

  - Printed as two digits each, separated by " : " (space, colon, space).
  - It counts down, seconds are truncated (never rounded up), and it stops at 00 : 00.
  - Minutes of 100 or more print three digits.
  - The timer has no low-time colour.

### D8. What updates when

| Element | Update cadence |
|---|---|
| Crosshair geometry, colour | every frame |
| Hit-indicator expiry | every frame |
| Damage overlay fade | every frame |
| Health, armour, stamina, low health | every frame |
| Ammo, fire mode | every frame |
| Kill feed | on each kill message (add); expiry checked every frame |
| Score panel | on each kill message, on goal set, on team change |
| Name tags | every frame |
| Range readout | every frame |

## Per-frame order

1. Weapon update. The spread steps of section B run here, so r is current before the HUD reads it.
2. Targeting ray (A11). This also schedules the health block for this frame.
3. HUD update, for each HUD item in turn:
   - crosshair bookkeeping: hit-indicator expiry, damage-indicator corner positions and alpha step, range readout, target team check;
   - health and stamina, including integrating stamina;
   - weapon panel;
   - kill-feed expiry;
   - score panel;
   - name tags.
4. HUD render, in item order. For the crosshair item the order is:
   1. infected overlay (mode-specific);
   2. damage-taken overlay and direction indicator;
   3. frozen overlay (mode-specific);
   4. if the scope overlay is up: range readout only, then stop;
   5. if dead in the free-look or chase camera: stop;
   6. dot-sight image (A8);
   7. if crosshair disabled: stop;
   8. crosshair shape, plus the double-circle extra;
   9. range readout;
   10. dot-sight image again.
- The damage overlay therefore draws even when the crosshair is hidden by scope or DisableCrosshair. It does not draw when the crosshair item is toggled off or during video.

## Edge cases

- **Odd screen widths:** the crosshair centre floor(W/2) and the damage-indicator centre 512 x_r can differ by half a pixel.
- **r = 0 and m = 0:** g = 0, so each arm starts 1 px from the centre pixel. The arms never touch the centre.
- **Large CrossHairOffset at high resolution:** it is unscaled, so the 45 px offset looks relatively smaller at 4K than at 1024.
- **Burst mode while zoomed:** the zoomed limits take priority over the burst limits.
- **Crouch override:** applies only to min, and only if both crouch fields are nonzero. A weapon with DuckPerturb > 0 but MoveDuckPerturb = 0 ignores crouching in the crosshair.
- **Health above max:** HP displays above I and is not clamped. A percentage over 100 is not red.
- **Health at 0 while still alive:** shows "0" and red.
- **Kill feed burst:** five lines arriving in one frame fill the feed. A sixth replaces the oldest at once.
- **Spectating in first person:** the crosshair uses the spectated player's scope attachment and current weapon. The spectated player's r arrives over the network and is not stepped locally.
- **Spread at low frame rates:** at frame rates below 80, one or more spread steps run per frame, up to 100 steps (1.25 s) after a hitch. Each one is a full step.

## Quirks

Keep these unless a reviewer decides otherwise:

1. **Bar length ignores spread.** The layout's bar min/max (8 and 32) are read but not used. The arm length is always 7 ref px plus the firing kick.
2. **Unused layout keys.** The gap min/max layout keys (5 and 25) are unused. The gap comes from weapon perturb.
3. **Spread steps lose time.** Leftover time is thrown away after every spread step, so the effective step rate depends on frame rate:
   - at 60 fps, 60 steps/s;
   - at 100 fps, 50 steps/s;
   - at 144 fps, about 72 steps/s;
   - at 200 fps, about 67 steps/s;
   - only when the frame time is a multiple of 12.5 ms do you get 80 steps/s.
   - Spread grows and recovers more slowly at most frame rates. Players feel this as frame-rate-dependent recoil recovery; keep it.
4. **Damage overlay fades per frame.** It fades by 2 alpha per frame for exactly 1000 ms, then vanishes:
   - at 60 fps it pops out from alpha 135;
   - only at 128 fps or more does it reach 0 first.
5. **Hit overlay is 1024 tall in reference units,** so a third of the texture hangs below the screen.
6. **The direction indicator's v coordinate runs to 1.32,** past the texture edge (wrap or clamp, see Open questions). The image is also mirrored left-to-right.
7. **The sector factor is 0.1666, not 1/6.** This skews the indicator by up to 0.0005π rad (about 0.1 degree) on the right side.
8. **Cross-wired speed scalars:** movement spread uses the firing scalars and firing spread uses the movement scalars (section B).
9. **Turning in place never widens the crosshair** (section B).
10. **The dot-sight image is drawn twice** on weapons that also have a crosshair. A translucent image looks denser there than on crosshair-less weapons.
11. **The double circle** comes only from the weapon's shape, not from a crosshair item's shape 5.
12. **Kill-feed badges:** the third and later badges overwrite the second one, but still leave gaps.
13. **The attachment perturb sum doubles** per attachment (A3).
14. **Crosshair colour can stick.** With no weapon data it keeps the previous frame's colour, which can be the hit red.
15. **Kill-feed expiry lag.** When a line expires, the feed removes the front entry of its internal ring. That may be an already-hidden line, so a visibly expired line can linger for up to 4 extra frames. Negligible; there is no need to copy it.

## Test cases

All at 1024x768 (x_r = y_r = 1, cx = 512, cy = 384) unless stated. The "M4A1" values are in section B. Rects are [x0, x1) x [y0, y1) in px.

1. M4A1, r = 0, not fired for > 0.1 s, no items, shape 0 -> four arms, no dot.
   - Left x [500, 507) on row 383.
   - Right x [516, 523) on row 383.
   - Top y [372, 379) on column 511.
   - Bottom y [388, 395) on column 511.
   - Colour (0, 255, 0, 255).
2. As 1 with r = 1 -> g = 100.
   - Left [403, 410); right [613, 620).
   - Top [275, 282); bottom [485, 492).
3. As 1 with r = 0.5 -> g = 51.5.
   - Left [451.5, 458.5); right [564.5, 571.5).
4. As 1, at 1920x1080 -> x_r = 1.875, cx = 960, cy = 540, g = 5.625, ℓ = 13.125.
   - Left [939.25, 952.375) on row 539.
   - Right [966.625, 979.75).
   - Top y [519.25, 532.375) on column 959.
   - Bottom y [546.625, 559.75).
   - Thickness 1 px.
5. As 1, at 1024x600 -> y_r differs, but the crosshair is unchanged from case 1 apart from cy = 300.
   - Top y [288, 295), bottom [304, 311).
   - The arms are not squashed.
6. As 1, the weapon fired at t = 0, measured at t = 0 -> D = 7, ℓ = 13.
   - Left [490, 503); right [520, 533).
   - Top [362, 375); bottom [392, 405).
7. As 6 at t = 0.05 s -> D = 5, ℓ = 10.
   - Left [495, 505); right [518, 528).
8. As 6 at t = 0.1 s -> identical to case 1.
9. M4A1 zoomed in (no scope overlay), standing, r = 0 -> min 2, max 30, g = 2.
   - Left [501, 508); right [515, 522).
10. M4A1 zoomed, r = 0.5 -> g = 2 + 0.5 * 28 = 16.
    - Left [487, 494); right [529, 536).
11. M4A1 crouched and still, not zoomed, r = 0 -> min = DuckPerturb 2. Same rects as case 9.
12. M4A1 crouched and moving, r = 0 -> min = MoveDuckPerturb 3. Same as case 1.
13. Weapon with CrossHairOffset 45 and colour (110, 191, 228), m = 3, r = 0 -> D = 48.
    - Left [455, 462); right [561, 568).
    - Colour (110, 191, 228, 255).
14. As 13 at 2048x1536 -> x_r = 2, g = 6, D = 51 (offset not doubled), ℓ = 14.
    - Left [1024 - 51 - 2 - 14, 1024 - 53) = [957, 971).
15. Shape 1 (CrossHairType 1), r = 0, M4 numbers -> case 1 plus the pixel (511, 383) lit.
16. Shape 2 -> as 15 but with no top arm.
17. Shape 3, r = 0, M4 -> no bars.
    - Four 1-px diagonal segments centred on (512, 384), running from radius 5 to 12.
    - The up-left segment runs from (508.46, 380.46) to (503.51, 375.51).
    - The dot is at (511, 383).
18. Shape 4, r = 0, M4 -> circle of radius 4 centred on (512, 384), 32 segments, the first vertex at (512, 380). Plus the dot.
19. Shape 4, just fired (t = 0), r = 0, M4 -> circle radius D + A + 1 = 7 + 6 + 1 = 14.
20. Shape 6, r = 0, M4 (g = 3, D = 3, ℓ = 7).
    - Left bracket apex at (494, 384), arms of length 12 to about (502.49, 375.51) and (502.49, 392.49).
    - Right apex at (530, 384), arms to about (521.51, 375.51) and (521.51, 392.49).
    - Dot at (511, 383).
21. Weapon CrossHairType 5, M = 100, m = 3, N_wide = 4, r = 0.
    - Two circles, no dot.
    - Inner radius 4.
    - Outer: m' = 15, M' = 150, D' = 15, radius 16.
22. Crosshair item with shape 5 on a weapon without a CrossHairType -> one circle of radius 4, no dot.
23. Weapon CrossHairType 6 and crosshair item shape 3 -> brackets (the weapon wins).
24. Crosshair item colour (255, 255, 0) on a default weapon -> yellow crosshair.
    - With dot-sight mode on and a dot-sight scope of colour (255, 0, 255) fitted, the crosshair is magenta.
25. Local player damages an enemy at t_hit -> crosshair (208, 0, 0, 255) on every frame with now - t_hit <= 500 ms. Green again on the first frame past 500 ms.
26. As 25, a second hit at t_hit + 400 ms -> red until t_hit + 900 ms.
27. Local player damages a teammate -> crosshair colour unchanged.
28. Hit indicator on, then the round ends -> green on the next frame.
29. Scope overlay active, scope without a range readout -> nothing from the crosshair item except damage feedback.
30. Weapon with DisableCrosshair = 1 and a DotSightName -> no lines. The reticle image is drawn once at (448, 320), size 129 x 129.
31. Same image on a weapon with a crosshair -> the image is drawn twice per frame, the second time after the lines.
32. Range-readout scope, target range 10000, wall hit at 2000 units, weapon Range 3500 -> text "50.0", right-aligned at (490, 400), colour (255, 198, 0).
33. As 32 with the hit at 4000 units -> "100.0", colour (255, 0, 0).
34. As 32 with nothing within the target range -> empty text.
35. Damage from straight ahead (d = -F), no turning -> direction indicator corners P0 (681, 215), P1 (343, 215), P2 (343, 553), P3 (681, 553).
36. Damage from directly behind (d = +F) -> s = 9.
    - P0 is about (342.8, 552.8) and P1 about (680.8, 553.2).
    - The texture's top edge is along the bottom of the screen.
37. Damage from the right (d = -Rt, F·d = +0) -> s = 12.
    - P0 is about (680.7, 553.3) and P1 about (681.3, 215.3).
    - The top edge is on the right.
38. As 35, then the player turns right by π/2 -> φ = 3π/4.
    - P0 (343, 215), P1 (343, 553).
    - The top edge is on the left: the indicator points left, where the attacker now is.
39. Damage taken at t_dmg, rendering at a steady 60 fps.
    - Alpha 255 - 2n after the n-th HUD frame, for example 195 after 30 frames.
    - Overlay and indicator gone on the first frame at or after t_dmg + 1000 ms.
40. A hit fully absorbed by armour -> no overlay and no indicator.
41. Hit overlay rect at 1920x1080 -> (0, 0, 1920, 1440).
42. Health 30 of 100, standard soldier -> HP text "30", colour (255, 0, 0).
    - The "HP" and "AP" labels and the silhouette are red; "SP" stays (216, 216, 216).
    - A heartbeat at least every 1.0 s.
43. Health 31 of 100 -> "31", white, labels (216, 216, 216), no heartbeat.
44. Health 1 of 200, InterfaceMaxHealth 100 -> "1". Health 199 of 200 -> "99".
45. Health 0 -> "0", red. Health -5 -> "0".
46. Health 30 of 100 while infected -> white, no heartbeat.
47. Armour 75 -> AP text "75". Armour -3 -> "0".
48. Stamina 50 of 100 -> bar 60 px wide at 1024 (120 * 1.875 = 225 px at 1920).
    - Stamina 0 -> bar hidden.
    - Stamina 100 -> 120 px.
49. M4A1 with 7 in the clip and 90 in reserve -> "007", "/", "090" at (910, 742), (962, 742) and (974, 741).
    - Colours (240, 240, 240) and (180, 180, 180).
50. Clip 30, reserve 1200 -> "030" and "1200".
51. Clip 0, reserve 0 -> "000" and "000", same colours as case 49 (no low-ammo state).
52. Knife equipped -> name shown; clip, slash, reserve and fire icon hidden.
53. Mission bomb or a non-gun class equipped -> whole panel hidden. (*Correction:* only the mission bomb and weapons in no order list hide the panel; see case 77.)
54. Rifle with selectable fire, in auto mode -> full-auto icon at (926, 731). Switch to single -> single-shot icon.
    - Burst mode with burst count 2 -> 2-round icon.
    - Sniper rifle -> no icon. (*Correction:* a sniper rifle is in the primary list, so it gets an icon; see case 76.)
55. Weapon name on a 1024 screen -> right edge of the text at x = 1019, vertically centred on y = 721.
56. Kill feed after three kills A, B, C at t = 0, 1 s and 2 s.
    - Lines at y 45 (A), 65 (B), 85 (C).
    - At t = 30.0 s + one frame, A is gone and B, C sit at y 45 and 65.
57. Six kills within a second -> five lines. The first kill is no longer shown, and the sixth is at y 125.
58. Kill line where a teammate kills an enemy with a headshot.
    - Killer name in (124, 190, 221) at x = 5.
    - Weapon icon right after the name, 64 wide.
    - Headshot badge 32 wide.
    - Victim name in (218, 115, 115).
59. Kill line with revenge, headshot and groin-shot badges.
    - The first badge slot shows revenge and the second shows groin shot.
    - The victim name is 96 px (3 * 32) after the weapon icon's end.
60. TDM, goal 100, Alpha 25 kills, Bravo 0, local player on Alpha.
    - Alpha score "25" in (223, 31, 20); Alpha bar 38 px wide.
    - Bravo score "0" in (170, 170, 170); Bravo bar hidden.
    - Highlight background at (344, 18).
61. TDM, Alpha kills 120 with goal 100 -> bar 155 px (full).
62. Round timer with 61.9 s remaining -> "01 : 01". 0.4 s -> "00 : 00". 600.0 s -> "10 : 00".
63. TDM -> no round timer anywhere in the score panel.
64. Aiming at an enemy behind no cover -> their name tag appears, name (215, 17, 18) at alpha 200, no HP bar. It disappears on the first frame the aim ray leaves them.
65. After hitting that enemy -> name colour (250, 80, 0) for 0.2 s, then (215, 17, 18).
66. Teammate behind a wall, not aimed at -> tag shown at alpha 125, no HP bar.
    - Aiming at them with clear sight -> alpha 255, HP bar shown.
67. Aiming at a teammate or an enemy -> the crosshair geometry and colour equal those of the same state aiming at a wall.
68. Spread, M4A1 standing still, one shot at 60 fps, r_move = 0.
    - Right after the shot frame, r_fire = 3.0 * (1/80) * 1.3 = 0.04875, so g = 3 + 0.04875 * 97 ≈ 7.73 (plus the kick).
    - It holds for 95 ms.
    - It then falls 0.01125 per step, reaching 0 about 5 steps later (about 83 ms at 60 steps/s).
69. Spread, M4A1 held fully automatic standing still -> r_fire climbs 0.04875 per shot and caps at FirePerturb 0.30, so g caps at 3 + 0.3 * 97 = 32.1.
70. Turning the mouse quickly while standing still with r = 0 -> r stays 0 and the crosshair is unchanged.
71. Hit-indicator edge, OS millisecond clock: hit stamped at 10000 ms. A frame at 10500 ms -> still red (500 is not greater than 500); the next frame at 10516 ms -> green. A kill-feed line stamped at 0 is still shown on a frame at 30000 ms and gone on the first frame after.
72. Crosshair kick on the game clock: the weapon fired at game time 12.000 s; a frame at game time 12.050 s -> D = 5, ℓ = 10 (as case 7), whatever the OS clock says.
73. Gap unrounded, M4A1 standing still, r = 0.04875 (one shot) -> g = 3 + 0.04875 × 97 = 7.72875 px at 1024; the shot cone for the next shot is n = 3 + ⌊4.72875⌋ = 7 units. At r = 0.30: g = 32.1 px, cone 32.
74. Name-tag anchor: a model whose "NAME_socket" is at world (100, 172, 50) -> the tag is centred on that point's projection, shifted up by half the tag's height. A model with no such socket and its hull centre at (100, 100, 50) -> anchored at (100, 100, 50).
75. Name tags have no distance limit: a teammate 20000 units away in front of the camera -> tag shown (alpha by visibility as in case 66). The same teammate directly behind the camera -> hidden.
76. Fire-mode icons from the data: M16A3 (primary, `RapidFireCount` 0, `TripleShotMode` 1) -> full-auto icon, burst icon when toggled; M24 sniper (primary, `RapidFireCount` 1) -> single-shot icon; Beretta M92FS (secondary, `RapidFireCount` 1) -> single-shot icon; a primary with `RapidFireCount` 3 and no burst mode -> no icon; M67 grenade (throw class) -> no icon.
77. Panel visibility by inventory class: M67 in hand -> panel shown, name "M67", counts "001" / "000" (finite ammo), no fire-mode icon. M9 knife -> panel shown, name only (case 52). Mission bomb (`Guntype` 8) -> panel hidden. A weapon in no order list (class 0) -> panel hidden.

## Open questions

1. **Build match:** the layouts and numbers come from a US-region data set of the classic client. The Steam build's layout and XML files are encrypted. It is unverified that positions, colours, KillMessageLifeTime and the weapon crosshair fields match the Steam build.
   - Check: screenshot the Steam client at 1024x768 and compare cases 1, 49 and 56.
2. **Font scaling:** how fonts scale with resolution (with y_r, x_r or not at all), and the rendered widths that drive kill-feed placement. The UI system spec (ui_system.md) should settle this.
3. **Aligned text inside its box:** right- and centre-aligned texts carry a dummy position of (100, 100). We assume they align within their parent box (the weapon name right-aligned to the box's right edge, centred labels centred in their boxes).
   - Check against a screenshot.
4. **Yaw sign:** case 38 assumes yaw increases when turning right. Check by taking damage from the front and turning right: the indicator should swing left.
5. **Texture addressing:** whether the direction indicator's texture wraps or clamps beyond v = 1.
6. **Kill-feed text on suicides and world kills:** does the caller pass an empty killer name, giving "Victim [icon] Victim"? Check a fall-death line in the live game.
7. **Goal string:** the exact goal text for TDM (for example "100 Kills") comes from the room data string table and was not traced.
8. **Spread data units:** whether the shot code converts perturb units to an angle in a way that makes "1 perturb = 1 px at 1024" visually exact for the game's field of view. This belongs to weapon_firing.md.
9. **Unused features:** confirm the over-crosshair friendly icon and the teammate HP number are really absent in the Steam build (they are disabled in the reference).
10. **Name-tag distance:** name tags may hide beyond some distance (the projection call has a distance-hide option). The threshold was not traced.
    **Answered:** the "distance-hide" option only hides a tag whose anchor is behind the camera (view-space depth ≤ 0); there is no distance threshold. The anchor is the model socket "NAME_socket" in the current pose, or the character's position (hull centre) when the model has no such socket, with no extra world offset. See A11; test cases 74 and 75. The socket's height above the feet is model data (models spec); to measure it live, stand a teammate on flat ground and compare the tag's screen position with a wall mark of known height at the same distance.
11. **Wide-fire pellet field:** the meaning of N_wide (double-circle min) beyond "the weapon's wide-fire count field". Check one shotgun with CrossHairType 5.
12. **Crosshair toggle:** which console command or key toggles the crosshair off, and whether the damage overlay then also hides (in the reference it does).
13. **The HUD clock.** Do the hit indicator, kill-feed life and crosshair kick run on the rendered-frame clock in milliseconds?
    **Answered:** they are evaluated once per rendered frame, but against clocks, not frame counts: the hit indicator and the kill feed against the OS millisecond clock (off on the first frame with elapsed > 500 ms or > 30000 ms), the kick against the game client time in seconds. Only the damage overlay's alpha step counts frames. See Units and conventions ("Time"); test cases 71 and 72.
14. **The crosshair gap and rounding.** Is the gap drawn from the shot cone after whole-unit rounding, or from the unrounded spread?
    **Answered:** unrounded, `g = (m + r(M − m))·x_r`, without `DynamicPerturbFactor`, from the meters after this frame's sub-steps. The shot cone rounds down. See A2; test case 73.
15. **"Enemy" in modes without teams.** For the hit indicator and name tags, is everyone else an enemy?
    **Answered in part:** the client never checks the mode: "enemy" means "team id differs from mine", for the hit indicator, the tag colour and the enemy tag rules alike. **Open:** which team id each player gets in free-for-all style modes is assigned outside the game client and was not traced (one detail hints at per-player ids: an admin view colours free-for-all ids 1 to 8 one way and others another). Check live in a free-for-all room: hit another player and see whether the crosshair turns red, and check whether players you are not aiming at show name tags (they would if all shared your team id).
16. **Weapon panel class and fire-mode icon.** Which data field gives a weapon's class and whether it has selectable fire modes?
    **Answered:** the class is the inventory class, from which list of the weapons file's weapon order the weapon is in (1 primary ... 6 mission, 0 none), not `Guntype`; the panel shows for classes 1–6 except the mission bomb, and the fire-mode icon only for classes 1 and 2. The icon comes from `TripleShotMode` and `TripleShotRoundNum` (burst), `RapidFireCount` (0 auto, 1 single, other none) and `ContinuousFire` (selectable continuous fire on a semi-automatic weapon). `HudFireMode` is unused. See D4 (cases 53 and 54 corrected); test cases 76 and 77. Also recorded in attribute_data.md.
