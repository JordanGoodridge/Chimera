# Combat Arms: radar (minimap) and full-screen map

Source basis: the client's radar HUD panel, its full-screen ("tactical") map panel and the per-mode icon legend data, the server's per-round mission message that carries the map data, the mission files' map fields, the scanner items (heartbeat sensors, satellite scanner, Hunted's system scan) on client and server, the HUD layouts of both panels and the HUD layout attributes that name their icons; checked against the Steam install's archives (Steam build 25595979; see Test cases).
Status: draft

Sections: Summary; Units and conventions; Constants; Behavior (R1 to R12: radar; F1 to F6: full-screen map; S1 to S3: scanners; N: network); Per-frame order; Edge cases; Quirks; Test cases; Open questions; Corrections (2026-10-22); Corrections (2026-10-28).

---

## Summary

- The radar is a square window in the top-right corner (186 x 186 ref px) showing a 1500 x 1500 world-unit square centred on the local player at the default zoom. North (world +Z) is up and the map does not turn, unless the player has switched on "rotate radar". The player's own position and facing are a fixed view-wedge image in the centre that turns with the view.
- The radar has **no image of its own**: it shows a moving, clipped window of the same per-map image the full-screen map uses, through a 372 x 372 quad (twice the window) so that it can turn without showing its corners.
- Teammates are always on the radar (alive, in range). Enemies are **never** shown for firing, silenced or not. An enemy appears only when something reveals them: carrying a flag, the Bombing Run bomb, bullion or (to another intel carrier) intel, being in Arms Race's last stage, or a scanner item (heartbeat sensor, satellite scan, Hunted's system scan). The dead vanish from the radar.
- Objectives (bomb sites, flags and flag bases, intel, the Bombing Run bomb, goals) are drawn as icons in range and as an arrow on the radar's edge when out of range. Nothing shows height above or below.
- Holding M shows a full-screen map: the central 75 % of the per-map image at 512 x 512 ref px, with names under player icons, death marks, team spawn areas and a legend of the mode's icons. It shows fewer enemies than the radar (no heartbeat contacts).
- All of it is worked out on the client from ordinary object positions. The server sends only the per-round map data (image file and two corner points), the team spawn centres, carrier and scan states.

## Units and conventions

- **World:** LithTech world units (u), +X right (east), +Y up, +Z forward (north), left-handed, as in world_dat.md. Only X and Z matter here; Y is ignored everywhere except in one quirk (R8).
- **Yaw ψ:** radians. Facing direction (sin ψ, 0, cos ψ): ψ = 0 faces +Z (north), ψ = π/2 faces +X (east); turning right increases ψ. Another character's yaw is taken from its rotation as ψ = atan2(F_x, F_z) of its forward vector; the local player's from the player's view yaw.
- **Editor space:** a position in the map's original editor space is the file position plus the world file's world offset (world_dat.md, "World offset"). The map image's corner points are in editor space; all object positions are file positions.
- **Reference canvas:** 1024 x 768 ref px, origin top-left, y down; HUD scaling x_r = W/1024, y_r = H/768 (ui_system.md S1, S2). All positions below are absolute reference positions (all parents added).
- **Screen rotation angles:** measured on screen from +x towards +y (y down), so a positive angle turns clockwise as seen.
- **Truncation:** "trunc" rounds toward zero (float to integer conversion). Several pixel positions are integers produced this way.
- **Time:** the radar and the map run once per rendered frame (no fixed tick). Scanner timings are in ms.
- **Archive file names** below are names inside the game's archives, written as the data writes them (case-insensitive).

## Constants

| Name (ours) | Value | Unit | Meaning |
|---|---|---|---|
| radar world size S | 1500 | u | `DefaultWorldSize` in the large-map attribute file's `[RadarInfo]`; world width and height of the radar window at zoom 1 |
| radar window M | (818, 5), 186 x 186 | ref px | layout `maskingRect`: clip (scissor) rect and the frame of reference for icon positions |
| radar centre C | (911, 98) | ref px | M's position + 93 |
| radar scale k | 186 / 1500 = 0.124 | ref px per u | M's width / S, at zoom 1 |
| radar map quad | (725, -88), 372 x 372 | ref px | layout `Map`, twice M, same centre; alpha 190 |
| view wedge ("guide") | (653, -161), 512 x 512 | ref px | layout `guide`, centre (909, 95) (2 and 3 px off C, quirk) |
| radar frame | (782, -30), 227 x 226 | ref px | layout `bg`, drawn over everything. The 256 x 256 `bg.dtx` is **not** stretched into this rect: the rect shows the texture's top-left 227 x 226 texels at one texel per ref px (Corrections (2026-10-28) 1) |
| zoom bar track | (1004, 4), 16 x 192 | ref px | layout `magnification` |
| zoom bar knob | x 1004, 16 x 10, y 18 to 168 | ref px | layout `ScaleBar`; travel 150 |
| zoom z range | 0.5 to 2.0 | | default 1.0 |
| zoom step | 0.01 | per rendered frame | while a zoom key is held |
| icon box | 64 x 32 | ref px | every radar and map icon, centred on its point |
| edge-arrow inset | 24 | ref px | (64 - 16) / 2 |
| full map frame | (90, 93), 844 x 584 | ref px | layout `Root1` |
| full map legend panel | (101, 103), 252 x 562 | ref px | |
| full map image | (397, 139), 512 x 512 | ref px | layout `Map`; alpha 230 |
| full map crop h | 128 / 1024 = 0.125 | | fraction of the image cut from each side |
| death marks | 20 | | at most, 54 x 54 ref px each |
| legend rows | 20 | | at most per mode; 400 x 32, gap 5 |
| heartbeat list send | ≥ 100 | ms | between a scanner's list messages, and only on change |

## Behavior

### R1. Per-map data

- **Image:** one image per map, shared by the radar and the full-screen map. It lives in the plain (unencrypted) HUD map archive under `UI_HUD_Map\mapImage\`, for example `Map_Junkflea.dtx`, `Map_Showdown.dtx` (the Warehouse world), `Map_JunkFlea2.dtx`. Format: a DTX (textures_dtx.md). In this install almost all are 1024 x 1024, one mip, format code 6 (DXT5), 1,048,740 bytes; six are 1024 x 1024 32-bit (code 3, 4,194,468 bytes) and two are DXT1 (code 4, 524,452 bytes). The image is a top-down picture of the map, north (+Z) up, east (+X) right.
- **Where the numbers live:** in each mission file (the `.gms` files of the GMS archive, one per map and mode), in each objective section `[ObjectN]`:

| Field | Type | Default | Meaning |
|---|---|---|---|
| `LargeMap` | string | "" | the image's archive file name |
| `LargeMapWidth`, `LargeMapHeight` | integer | 0 | the image's size in pixels (1024 in every file) |
| `LargeMapPoint0_2D` | point (x, y) | (0, 0) | a pixel of the image |
| `LargeMapPoint0_3D` | vector <x, y, z> | <0, 0, 0> | the editor-space world point under that pixel; y unused |
| `LargeMapPoint1_2D`, `LargeMapPoint1_3D` | as above | | a second, lower-right pair |

- **Corners.** From the two pairs (p0, P0) and (p1, P1), image size W x H, the server (when it loads the mission file) computes world units per pixel and the world positions of the image's top-left (LT) and bottom-right (RB) corners; y is set to 0:

```math
f_x = \frac{P1_x - P0_x}{p1_x - p0_x},\qquad f_y = \frac{P0_z - P1_z}{p1_y - p0_y}
```

```math
LT = \big(P0_x - p0_x f_x,\; P0_z + p0_y f_y\big),\qquad RB = \big(P1_x + (W - p1_x) f_x,\; P1_z - (H - p1_y) f_y\big)
```

- Most files give the points at pixels (128, 128) and (895, 895), which makes f = span / 767, not span / 768, so LT and RB come out slightly asymmetric (quirk; test cases).
- **Per objective:** each `[ObjectN]` has its own fields; the server sends the current objective's (N).
- **Maps without data:** a mission file whose fields are missing gives LT = RB = NaN (0 / 0) and an empty image name: no image is drawn; radar icons still work (they do not use LT/RB); full-map icons land nowhere visible. Of the 820 mission files, 3 have no fields at all and one gives an empty image name.

### R2. When the radar is shown

- Shown while the "alive HUD" is: hidden while the death/kill camera is up, in letterbox, while a cutscene video plays and while AI-frozen. So it is hidden while dead and spectating through the death camera.
- Hidden also while drawing is suppressed by video or AI freeze at render time.
- When it becomes hidden, the current zoom is saved to the player profile (`RadarScale`).
- On entering a world (map load): visible, its object list is emptied, zoom restored from the profile (default 1.0, clamped), and the rotate setting read (R10).

### R3. Zoom

- Two actions, `IncRadarScale` and `DecRadarScale` (shown in the key-binding menu as "Expand Radar" and "Decrease Radar"). Default keys in the engine's default bindings: `=` (scancode 13) expands, `-` (scancode 12) decreases.
- While a key is held and chat input is not open, z changes by ±0.01 **per rendered frame** (frame-rate dependent, quirk), then is clamped to [0.5, 2.0].
- The world window is S/z wide: 3000 u at z = 0.5, 1500 at 1.0, 750 at 2.0. Icons grow with z (R6).
- Zoom-bar knob: y = 18 + trunc(150 · (2.0 − z) / 1.5), x 1004. So z = 2.0 → 18 (top), 1.0 → 118, 0.5 → 168.

### R4. World to radar pixel

Inputs: the local player's position P and yaw ψ, zoom z, object position V. Let d = (V_x − P_x, V_z − P_z).

- **Fixed north (default):** d' = d.
- **Rotating radar (R10):**

```math
d'_x = d_x\cos\psi - d_z\sin\psi,\qquad d'_z = d_x\sin\psi + d_z\cos\psi
```

  This puts whatever is straight ahead straight up.
- **In range:** the object is drawn only if both |d'_x| ≤ 750/z and |d'_z| ≤ 750/z. The range is a **square**, not a circle (corners reach 1061/z u).
- **Pixel:** with k = 0.124,

```math
X = 818 + \operatorname{trunc}\big(k z\,(d'_x + 750/z)\big),\qquad Y = 5 + \operatorname{trunc}\big(-k z\,(d'_z - 750/z)\big)
```

  equivalently X = 818 + trunc(93 + k z d'_x), Y = 5 + trunc(93 − k z d'_z), so the player is at (911, 98).
- The object's icon box (64 x 32) is placed with its top-left at (X − 32, Y − 16), then scaled by z about its centre (so its centre stays at (X, Y)). On screen everything is multiplied by (x_r, y_r).
- Implementation note: X and Y are truncated, so a point exactly on an integer boundary is sensitive to float rounding; tests avoid such points.

### R5. The map image under the radar

Every frame, with Wr = S / z and the world offset O (world_dat.md):

- The image quad covers the world square of side 2·Wr centred on P (twice the radar window, matching the quad being twice M). Its texture window, as fractions of the image (u to the right, v down):

```math
u_0 = \frac{P_x - W_r + O_x - LT_x}{|LT_x - RB_x|},\quad v_0 = \frac{LT_z - (P_z + W_r + O_z)}{|LT_z - RB_z|},\quad u_w = \frac{2W_r}{|LT_x - RB_x|},\quad v_h = \frac{2W_r}{|LT_z - RB_z|}
```

  Corners: top-left (u0, v0), top-right (u0 + uw, v0), bottom-right (u0 + uw, v0 + vh), bottom-left (u0, v0 + vh).
- Texture coordinates outside [0, 1] occur near the map's edge. They are **clamped**: the sampler repeats the image's outermost texel row or column (Corrections (2026-10-28) 2). Almost every image has a fully transparent margin, so near any edge the window simply shows no picture.
- **Fixed north:** the quad is its layout rect (725, −88) 372 x 372, scaled.
- **Rotating radar:** the quad is turned about (911, 98) by −ψ (counter-clockwise by ψ), using the corner formula of R7 with angle π − ψ and size 372 x 372, so the facing direction is up.
- The quad is drawn at alpha 190 and clipped to M (in screen px: M scaled by x_r, y_r).

### R6. What is drawn, and the icons

Every object the full-screen map knows (F2) is also given to the radar, **except the local player** (and observers). Each frame, per object:

**Players.** Let "mine" be the local player's team id and "ally" any other player with the same team id.

- **Ally:** drawn. Icon `character_ally.dtx`, wedge `character_ally_view.dtx`. Overrides, first match:
  1. carrying bullion (Hired Guns): icon `character_ally_money.DTX`;
  2. carrying a flag: the flag badge (below), plus an edge arrow when out of range;
  3. carrying the Bombing Run bomb: icon `character_ally_bomb.dtx`, plus an edge arrow.
  - A VIP ally (VIP Escort) uses `character_VIP.dtx` and `character_VIP_View.dtx` (set before the overrides).
- **Enemy:** **not drawn**, icon `character_enemy.dtx`, wedge `character_enemy_view.dtx`, unless, first match:
  1. carrying bullion: drawn, icon `character_enemy_money.dtx`;
  2. the local player carries intel (Spy Hunt) and this enemy also carries intel: drawn, icon `character_spy_pre.DTX`, wedge `character_spy_view.dtx`;
  3. carrying a flag: drawn, flag badge, edge arrow;
  4. carrying the Bombing Run bomb: drawn, icon `character_enemy_bomb.dtx`, edge arrow;
  5. Arms Race (game type 22) and this enemy has entered the last stage: drawn;
  6. otherwise drawn only if one of: a shared scan is active for my team (S2, S3); my own heartbeat sensor is on and the enemy is a heartbeat contact (S1); or the enemy is on a heartbeat list a teammate shared (S1).
- **Firing does not reveal anyone.** There is no fire ping, so a silenced weapon makes no difference.
- **Dead players are not drawn** (any team), from the death message until all are set alive (R11).
- **Flag badge:** a second image over the icon: an Alpha carrier carries Bravo's flag, badge `character_flag_B.dtx`; a Bravo carrier `character_flag_A.dtx`. The edge arrow then shows the badge image.
- Team ids only: in free-for-all modes whether others count as allies depends on the ids the server gives (Open question 6).

**Objectives and items** (drawn when on, alive and in range; an edge arrow when out of range for the types marked ↗):

| Object | When it exists | Icon (archive file under `UI_HUD_Map\icon\`) |
|---|---|---|
| S&D bomb site ↗ | each unlocked bomb-zone trigger, found once at the first non-training round | `area_bomb_A.dtx` for zone 0, `area_bomb_B.dtx` zone 1 (letter = 'A' + zone number); Bombing Run: `area_bomb.dtx` |
| goal / activate zone ↗ | activate triggers, except in Seize and Secure | `area_goal.dtx` |
| CTF flag base ↗ | each flag-zone trigger, every frame | own flag at home: `area_flag_A.dtx` / `area_flag_B.dtx`; flag away: `area_flagPoint_A.dtx` / `_B` |
| CTF loose flag ↗ | a flag pick-up that has moved from home (dropped) | `character_flag_A.dtx` / `_B` |
| CTF flag at home | (hidden; the base shows it) | |
| Spy Hunt intel ↗ | intel pick-ups, while visible | `item_intel.dtx` |
| Bombing Run bomb ↗ | the bomb pick-up, while visible | `item_bomb.DTX` |
| bullion on the ground | Hired Guns dropped bullion | `item_money.DTX` |
| mission weapon | weapon pick-ups flagged to show an indicator | from the trigger-resource data |
| mission target ↗ | target triggers, on/off by server state | from the trigger-resource data |
| NPCs (co-op) | AI characters | `character_NPC.dtx` (normal), `character_NPC03.dtx` (elite), wedge `character_NPC_view.dtx` |

- Every icon is drawn as is: no tint, no team colour applied by code; the colours are the art's.
- **Draw order inside the window:** map image, then items and objectives, then NPCs, then players, then the view wedge on top; then, unclipped, the zoom bar and the frame.

### R7. Facing wedges

- Players and NPCs in range also show a wedge image (the "view" texture, 64 x 32, whose cone points along the texture's +u, to the right). It is drawn as a rotated quad centred on (X, Y): with θ the angle below, w × h = 64 × 32, α = atan(h/w) = 0.46365 rad, r = ½√(w² + h²) = 35.777, and size factor s (z on the radar, 1 on the full map):

```math
\theta = \tfrac{\pi}{2} + \psi_{obj} - \psi_{rot}
```

  where ψ_rot = ψ (the local yaw) in rotating mode and 0 otherwise. Corner k gets texture corner k (top-left, top-right, bottom-right, bottom-left):

```math
c_0 = \theta + \alpha,\quad c_1 = \theta + \pi - \alpha,\quad c_2 = \theta + \pi + \alpha,\quad c_3 = \theta + 2\pi - \alpha
```

```math
Q_k = \big((X + r s\cos c_k)\,x_r,\; (Y + r s\sin c_k)\,y_r\big)
```

- The result: the texture's +u axis points along screen angle θ − π = ψ_obj − ψ_rot − π/2, i.e. towards where the character faces. At θ = π the image is upright.
- The quad is in screen px after the (x_r, y_r) multiplication, so on a non-4:3 screen a turned wedge is stretched.
- **Own wedge (the "guide"):** the 512 x 512 `guide.dtx` image (radar art archive) centred on (909, 95), using the same corner rule with size 512 x 512 (α = π/4) and s = 1: angle π + ψ in fixed-north mode (it turns clockwise with the view; upright facing north); in rotating mode it is not turned and stays upright. It is clipped to M.

### R8. Edge arrows

For an object of a ↗ type (R6) or a flag or bomb carrier that is drawn but out of range:

- Let D = V' − P, using the rotated V' in rotating mode, **normalised in 3D, including height** (quirk), and β = atan2(D_x, D_z), b = |β|.
- Distance from the radar centre:

```math
L = \operatorname{trunc}\Big(\Big|\frac{93}{\sin b}\Big| - 24\Big)\ \text{if}\ \tfrac{\pi}{4} < b < \tfrac{3\pi}{4},\qquad L = \operatorname{trunc}\Big(\Big|\frac{93}{\cos b}\Big| - 24\Big)\ \text{otherwise}
```

- Point: (911 + trunc(D_x L), 98 + trunc(−D_z L)). The arrow's 64 x 32 box (top-left at point − (32, 16)) holds two images: the background arrow `locationIndicator.dtx` (radar art archive), turned with the corner rule of R7 at angle π/2 + β, size 64 x 32, s = 1 (so it points from the centre towards the object), and the object's icon, upright, over it. Neither scales with zoom.
- With no height difference the arrow sits on a square 24 px inside M's edge; with height difference it moves inwards (|D_x|, |D_z| < 1).

### R9. Icons by role (summary)

All role names below are keys of the HUD layout attributes' `[HUDLargeMap]` section; values are archive file names under `UI_HUD_Map\icon\`:

| Role key | File |
|---|---|
| `LargeMapPlayer` / `LargeMapPlayerView` | `character_player.dtx` / `character_player_view.dtx` (full map only; the radar uses the guide) |
| `LargeMapPlayerBomb`, `LargeMapPlayerMoney`, `LargeMapPlayerDeath` | `character_player_bomb.dtx`, `character_player_money.dtx`, `area_death_player.DTX` |
| `LargeMapAlly`, `LargeMapAllyView`, `LargeMapAllyDeath`, `LargeMapAllyBomb`, `LargeMapAllyMoney` | `character_ally.dtx`, `character_ally_view.dtx`, `area_death_ally.DTX`, `character_ally_bomb.dtx`, `character_ally_money.DTX` |
| `LargeMapEnemy`, `LargeMapEnemyView`, `LargeMapEnemyBomb`, `LargeMapEnemyMoney` | `character_enemy.dtx`, `character_enemy_view.dtx`, `character_enemy_bomb.dtx`, `character_enemy_money.dtx` |
| `LargeMapSpy`, `LargeMapSpyView` | `character_spy_pre.DTX`, `character_spy_view.dtx` |
| `LargeMapVIP`, `LargeMapVIPView` | `character_VIP.dtx`, `character_VIP_View.dtx` |
| `LargeMapAlphaFlagMove`, `LargeMapBravoFlagMove` | `character_flag_A.dtx`, `character_flag_B.dtx` |
| `LargeMapAlphaFlagArea`, `LargeMapBravoFlagArea` | `area_flagPoint_A.dtx`, `area_flagPoint_B.dtx` (base, flag away) |
| `LargeMapAlphaFlagInArea`, `LargeMapBravoFlagInArea` | `area_flag_A.dtx`, `area_flag_B.dtx` (base, flag home) |
| `LargeMapBombArea` | `area_bomb` + `_A`/`_B` + `.dtx`, or + `.dtx` in Bombing Run |
| `LargeMapBomb`, `LargeMapIntel`, `LargeMapGoal`, `LargeMapMoney` | `item_bomb.DTX`, `item_intel.dtx`, `area_goal.dtx`, `item_money.DTX` |
| `LargeMapRespawn` | `area_respawn` + `_A`/`_B` (team 1/2) + `.dtx`; VIP Escort `_DF` (Alpha) / `_OF` (Bravo); co-op no suffix |
| `LargeMapNPCNormal`, `LargeMapNPCElite`, `LargeMapNPCView` | `character_NPC.dtx`, `character_NPC03.dtx`, `character_NPC_view.dtx` |

Every icon file is 64 x 32, 32-bit, except `area_death.dtx` (64 x 64, the full map's death mark).

### R10. Options

- **Rotate radar:** console variable `RotateRadar` (0 off, default; 1 on), set by the options screen's map-rotation on/off pair and saved in the profile. Read only when entering a world, so a change applies from the next map.
- **Radar zoom:** R3; saved per profile as `RadarScale`.
- There is no option to hide the radar.

### R11. Life cycle and state changes

- **Objects** are added when their client objects appear (players and NPCs when their character is created or the player list updates; triggers and pick-ups as R6 says) and removed when the server removes the object.
- **Death:** the kill message marks the victim dead (radar: hidden; full map: death icon at the position at that moment, F3). A respawned character's creation marks it alive. At the end of each round and at round start everyone is set alive.
- **Team swap** (VIP Escort, clan modes): every Alpha entry becomes Bravo and vice versa, including the local team.
- **Per round:** the round-start message (N) sets the image and corners again and places the spawn markers.

### R12. HUD mode change

The radar panel has a second look for the co-op robot HUD mode (a separate layout with its own frame and zoom bar; a battery gauge for the robot's cooldown). On a mode change the frame and zoom-bar images fade between looks over 2000 ms (ui_system.md W6) and the knob is re-placed at the current zoom. The geometry above is the normal look's.

### F1. When the full-screen map is shown

- Shown while the `FullTacticalMap` action (key binding "Tactical Map"; default key M, scancode 50) is **held**, and not while chat input is open, an in-game popup is up, in letterbox, during video or AI freeze. Pressing it after the round has ended does not open it. It shows while dead too.
- Drawn in the modal channel (ui_system.md M2, render order), scaled by (x_r, y_r).
- While shown, the radar keeps drawing underneath.

### F2. Layout

- Frame (90, 93) 844 x 584; legend panel (101, 103) 252 x 562; map panel (361, 103) 562 x 562.
- Map image (397, 139), 512 x 512, alpha 230, showing the image's texture window u, v ∈ [0.125, 0.875]: the outer 128 pixels of a 1024 image on every side are never shown on the full map (they are on the radar).
- A grid image `DotGuide.dtx` (392, 134) 523 x 523 over the map; column labels A to M at x = 406 + 40 i, y 110, and row labels 1 to 13 at x 371, y = 144 + 40 i (bold 20, (179, 179, 179)). They are decoration; nothing refers to them.
- Draw order over the map: grid and labels, death marks, items/objectives/spawn markers, NPCs, other players, the local player.
- **Legend:** the large-map attribute file's `[IconListN]` sections, each with `GameType` and up to 20 entries `ItemI_Icon`, `ItemI_View` (optional) and `ItemI_Desc` (a string id); entries end at the first empty icon. The section whose `GameType` equals the current mode's type is used; if none, the first section (`[IconList0]`, Elimination). Row i: 400 x 32 at (101, 109 + 37 i); icon 64 x 32 at the row's left, the view image over it if given, text at (+69, +11), bold 13, (179, 179, 179), shadow. Elimination's six rows read "ALLY VIEW", "ENEMY VIEW", "Current Location", "Last Point of Death", "ALPHA Respawn Area", "BRAVO Respawn Area" (US strings).

### F3. World to full-map pixel

- Shrink the corners by the crop: with e_x = |LT_x − RB_x|, e_z = |LT_z − RB_z|,

```math
LT' = (LT_x + 0.125 e_x,\; LT_z - 0.125 e_z),\qquad RB' = (RB_x - 0.125 e_x,\; RB_z + 0.125 e_z)
```

```math
s_x = \frac{512}{RB'_x - LT'_x},\qquad s_y = \frac{512}{RB'_z - LT'_z}\ (<0),\qquad c = \big(\operatorname{trunc}(-s_x LT'_x),\ \operatorname{trunc}(-s_y LT'_z)\big)
```

- An object at file position V, with world offset O:

```math
X = \operatorname{trunc}\big(397 + c_x + s_x (V_x + O_x)\big),\qquad Y = \operatorname{trunc}\big(139 + c_y + s_y (V_z + O_z)\big)
```

  (c is truncated separately first, quirk: up to 1 px off.) The icon box is placed at (X − 32, Y − 16), not scaled. Nothing is clipped to the map image: an object outside the image's covered square is drawn outside it.
- Wedges as R7 with s = 1 and ψ_rot = 0. The local player's wedge uses the view yaw.

### F4. What the full map shows

- **Local player:** always. Icon `character_player.dtx` with wedge; dead: `area_death_player.DTX` at the death position, no wedge; carrying bullion or the Bombing Run bomb: the money or bomb variant.
- **Allies:** always, alive or dead. Dead: `area_death_ally.DTX` at their death position. Bullion: money icon. Carrying the Bombing Run bomb: bomb icon (this overrides the VIP icon); VIP: VIP icon and wedge.
- **Enemies:** shown only if carrying a flag, a shared scan is active for my team (S2, S3), carrying the Bombing Run bomb, carrying bullion, (when I hold intel) holding intel, or Arms Race last stage. **Heartbeat contacts (mine or shared) are not shown on the full map**, unlike the radar.
- **Flag badge** as R6, for any drawn carrier.
- **Names:** each player icon has the player's name under it: centred text at the icon box's (+16, +24), bold 11, white, black stroke.
- **Objectives and items:** as R6's table (and the same per-frame discovery), plus the team spawn markers (F5). A dead or hidden item behaves as on the radar.
- **Death marks:** every kill of a player by another player (not a suicide or world death; in Quarantine not the kills that infect) leaves a mark `area_death.dtx`, 54 x 54, centred on the victim's position, for both teams. At most 20: the 21st reuses the oldest. Marks are cleared only when entering a world, not per round (Open question 9).
- **Observers** (GM spectators) see players by team colour (Alpha as enemies, Bravo as allies; everyone as allies in free-for-all modes). Out of scope beyond this note.

### F5. Spawn markers

- At each round start the server sends, per team, the mean position of that team's unlocked start points for the mode (start points of a mode family: clan modes and Go4CA count as S&D's, clan CTF as CTF's, clan TDM, Snowball and VIP Elimination as TDM's). Not sent in One Man Army and Arms Race.
- Each is drawn as the `LargeMapRespawn` icon for its team (R9) at that point (F3). In Hired Guns it is moved 37 ref px down (icon height 32 + 5). In VIP Escort the escort team gets Alpha's marker (`_DF`).
- Only on the full map, not the radar.

### F6. Mode notes

| Mode | Radar and map content beyond players |
|---|---|
| Elimination (TDM) | spawn markers (map) |
| S&D, Go4CA, clan S&D | bomb sites A and B; spawn markers |
| CTF, clan CTF | both bases (flag home / away), loose flags, carriers with badges visible to all |
| Spy Hunt | intel items; intel carriers see each other as spies |
| Bombing Run | the bomb (on the ground or on its carrier, visible to all), its site(s) `area_bomb.dtx` |
| Hired Guns | bullion carriers visible to all, dropped bullion; spawn markers moved down |
| VIP Escort | VIP icon for the own VIP; spawn markers `_DF`/`_OF` |
| Arms Race | enemies in the last stage visible to all |
| Hunted | system scans (S3) |
| co-op | NPCs, goals, mission targets, supply points |

### S1. Heartbeat sensors

- Items with `Guntype` 11 in the weapons data, `SensorOption` 0 (`Heartbeat_sensor`) or 1 (`HeartBeat_Detector`), `ScanDist` 1000 u, battery `BatteryMaxQuantity` 100, `BatteryUsingQuantity` 10 (per s while scanning), `BatteryRefillQuantity` 5 per `BatteryRefillTime` 1000 ms (refilling while not scanning).
- **On:** while the item is in hand, the fire input is held, the player is alive, not sprinting, in the play phase, and the battery is not empty. It goes off on release, sprint, death or an empty battery.
- **Contacts:** an enemy is a contact when its position is inside the local camera's view frustum (all six planes: in front, within the field of view, between near and far) and its horizontal distance (X, Z) from the local player is ≤ `ScanDist`. Walls do not block. Contacts are shown on **the radar only** (R6 rule 6).
- **Sharing** (`SensorOption` 1): while on, the client works out its contact list at most every 100 ms and, when it differs from the last one sent, sends it to the server, which forwards it to the sender's teammates. Teammates show those enemies on their radars until the sender's scanner turns off (the server announces on/off to the team).
- Option 0 shares nothing.

### S2. Satellite scanner

- `Satellite_Scanner` (`SensorOption` 2, `ScanSendTime` 7000 ms, `ShoutMsgShowTime` 7000 ms). On use the server tells everyone; the user's team gets a shared scan: **every enemy on radar and full map** (and an outline effect out of scope here) for `ScanSendTime`, after which the server sends "off" to that team. The user's team gets the message "… is uploading a Satellite Scan of enemy positions! (Check using the M-key.)"; the other team "WARNING: Enemy satellite scan detected! …".
- A new scan by the same team replaces the running one's timer.

### S3. Hunted's system scan

- The server's system scan (Hunted, mission fields `SystemScanPlayTime` and `SystemScanDelayTime`, missions.md) sends "on" with a duration and later "off" to everyone: while on, every player sees every other as revealed (shared scan) on radar and full map.

### N. What the server sends and what the client works out

- **Server, per round start** (the play-phase message): the current objective's image name (string, ≤ 255 bytes), LT and RB (y = 0), and the spawn-marker points (F5).
- **Server, ordinary traffic the radar reads:** positions and rotations of characters and pick-ups and triggers, trigger on/off and lock states, kill messages, flag and intel holders, bullion counts, VIP, Arms Race stage, scanner on/off and forwarded heartbeat lists.
- **Client:** everything else: which objects are drawn, the transforms, zoom, rotation, edge arrows, death marks, heartbeat contacts (from its own camera).
- Whether the server sends enemy positions when they are not visible to the client was not traced: the radar assumes every player object's position is available (Open question 5).

## Per-frame order

1. HUD update, in panel order:
   - full-screen map: discover flags (CTF), intel (Spy Hunt), the Bombing Run bomb, mission weapons and dropped bullion, adding or re-placing them in both the map's and the radar's lists; then, if the map key is held (F1), its own visibility and per-object icons and positions;
   - radar: visibility (R2); zoom keys (R3); own position and yaw, the map window (R5) and the guide or map turn; then each object in list order (R6, R4, R7, R8).
2. Render: world, code-drawn HUD, HUD panels: the radar's window content clipped to M, then its frame and zoom bar; later the modal channel with the full-screen map.

## Edge cases

- **Teammate out of range:** not drawn, no arrow (players get arrows only as flag or bomb carriers).
- **Object exactly at a range edge:** |d'| = 750/z is in range.
- **Object directly at the player's position:** in range, drawn under the guide.
- **Edge arrow with D_z = 0 and b = π/2 exactly:** uses the sine branch; b = π/4 exactly falls in the cosine branch (strict inequalities).
- **Huge height difference:** a carrier straight above or below the player has D_x, D_z near 0: the arrow sits near the centre.
- **Map without data:** R1. The radar still works on a black/empty window.
- **No start points for a team:** the server divides by zero: the marker gets NaN and is not visibly placed.
- **Zoom at a low frame rate:** fewer steps per second (R3).
- **Chat open:** zoom keys ignored; the full map closes.
- **Rotation turned on mid-match:** no effect until the next map.
- **Non-4:3 screens:** M, icon boxes and rotated quads stretch independently in x and y; rotated wedges are skewed.

## Quirks

Keep these unless a reviewer decides otherwise:

1. Range is a square, not a circle (R4).
2. Zoom steps per rendered frame (R3): at 60 fps a full sweep 0.5 → 2.0 takes 2.5 s, at 144 fps about 1.04 s.
3. Corner points at pixels 128 and 895 give a 767-pixel span (R1): images are scaled about 0.13 % off.
4. The guide's centre is (909, 95), 2 and 3 ref px off the radar centre (911, 98).
5. Edge arrows use a 3D-normalised direction, so height pulls them inwards (R8).
6. Full-map positions truncate the centre offset separately (F3): up to 1 px.
7. Heartbeat contacts show on the radar but not on the full map (F4).
8. Heartbeat "range" is a camera-frustum test plus horizontal distance: an enemy behind you is never a contact, walls never block.
9. Death marks persist across rounds (F4).
10. A mission file (Warehouse's Bombing Run file) carries another map's corner points, so icons there are misplaced; keep data as shipped.
11. The `TurnOn` state set by triggers does not affect players' visibility (their team rules decide).

## Test cases

All at 1024 x 768 (x_r = y_r = 1) unless stated. "rel" = object position minus player position, (Δx, Δz); height equal unless stated. Pixel results are icon centres (X, Y); the icon box top-left is (X − 32, Y − 16).

| # | Setup | Input | Expected |
|---|---|---|---|
| 1 | z = 1, fixed north, ally rel (300, 400) | | in range; (X, Y) = (948, 48) |
| 2 | z = 2, ally rel (300, −200) | | (985, 147) |
| 3 | z = 0.5, ally rel (−1010, 1210) | | (848, 22) |
| 4 | z = 1, ally rel (751, 0) | | not drawn, no arrow |
| 5 | z = 1, ally rel (700, 700) (990 u away) | | drawn (square range) |
| 6 | z = 2, ally rel (400, 0) | | not drawn (limit 375) |
| 7 | rotating radar, ψ = π/2, ally rel (300, 100) | | (898, 60); same ally fixed north: (948, 85) |
| 8 | enemy rel (100, 100), no scan, enemy firing a silenced or unsilenced weapon | any time | not drawn |
| 9 | enemy rel (100, 100) carrying a flag (enemy on Bravo) | | drawn, icon `character_enemy.dtx` with badge `character_flag_A.dtx` |
| 10 | enemy carrying a flag at rel (2000, 0), height equal, z = 1 | | edge arrow box top-left (948, 82), point (980, 98); arrow image unturned (+u right) |
| 11 | flag base at rel (1000, 2000) | | arrow point (946, 28) (L = 79) |
| 12 | flag carrier at rel (2000, 0) and 2000 u higher | | arrow point (959, 98) |
| 13 | ally facing ψ_obj = 0 at rel (300, 400), z = 1 | | wedge corners Q0 (932, 80), Q1 (932, 16), Q2 (964, 16), Q3 (964, 80); texture +u points up |
| 14 | as 13 at 1920 x 1080 | | Q0 (1747.5, 112.5), Q1 (1747.5, 22.5) |
| 15 | as 13 with z = 2 | | corners at ±32 / ±64 from (X, Y) of the z = 2 position |
| 16 | guide, fixed north, ψ = 0 | | corners (653, −161), (1165, −161), (1165, 351), (653, 351) (upright) |
| 17 | guide, ψ = π/2 | | rotated 90° clockwise about (909, 95): Q0 (1165, −161), Q1 (1165, 351) |
| 18 | radar map window, Junk Flea data (R1 test 31), P = (0, 0), z = 1, O = 0 | | u0 = v0 = 0.108176, uw = vh = 0.548601 |
| 19 | as 18, P = (500, −300), z = 2 | | u0 = 0.336760, v0 = 0.300187, uw = vh = 0.274301 |
| 20 | Showdown data, O = (−480, 1216), P = (0, 0), z = 1 | | u0 = 0.139211, v0 = 0.038896, uw = vh = 0.626973 |
| 21 | full map, Junk Flea, O = 0, V = (1000, −500) | | (696, 376) |
| 22 | full map, Showdown, O = (−480, 1216), V = (0, 0) | | (620, 293); V = (1000, −500): (763, 364) |
| 23 | full map, Junk Flea 2, V = (1000, −500) | | (751, 443) |
| 24 | full map, WarHead, O = (1570.5, 640), V = (0, 0) | | (697, 340) |
| 25 | zoom 1.0, hold `=` for 30 frames | | z = 1.30, knob y = 88; 100 more frames: z = 2.0, knob y = 18 |
| 26 | zoom 1.0, hold `-` 80 frames | | z = 0.5 (clamped), knob 168 |
| 27 | zoom 1.4, radar hidden by death, map reload | | profile `RadarScale` 1.4; restored z = 1.4 |
| 28 | heartbeat (option 0) on, enemy 900 u ahead in view | | enemy on own radar, not on the full map, not on teammates' radars |
| 29 | as 28, enemy 1100 u ahead | | not shown |
| 30 | as 28, enemy 500 u behind | | not shown |
| 31 | heartbeat detector (option 1), enemy 900 u ahead | | also on every teammate's radar within ≈ 100 ms; gone when the sender releases fire |
| 32 | satellite scan by Alpha at t = 0 | | every Bravo player on Alpha's radars (in range) and full maps until t ≈ 7000 ms |
| 33 | battery 100, hold heartbeat fire | 10 s | battery 0, scanner off |
| 34 | ally killed by enemy at V | | radar: ally vanishes; full map: `area_death_ally.DTX` and a 54 x 54 death mark centred at V |
| 35 | 21 kills in a map | | 20 death marks; the first kill's mark has moved to the 21st position |
| 36 | suicide by fall | | no death mark (ally death icon still shown) |
| 37 | S&D, bomb zones 0 and 1 | | icons `area_bomb_A.dtx`, `area_bomb_B.dtx`; out of range: edge arrows |
| 38 | Bombing Run bomb zone | | `area_bomb.dtx` |
| 39 | Elimination, round start | full map open | spawn markers `area_respawn_A.dtx`, `area_respawn_B.dtx` at the team start-point means; legend six rows from y 109, 37 apart |
| 40 | VIP Escort, escort team Bravo | | Bravo's marker `area_respawn_DF.dtx`, Alpha's `_OF` |
| 41 | mode with no matching `[IconListN]` | | legend of `[IconList0]` |
| 42 | mission file without map fields | | radar shows icons over no image; full map shows no image |
| 43 | Spy Hunt: local player and enemy E both carry intel | | E drawn with `character_spy_pre.DTX` on radar and map; once the local player drops intel, E hidden |
| 44 | CTF: Alpha flag at home | | Alpha base icon `area_flag_A.dtx`; flag dropped: base `area_flagPoint_A.dtx` and the loose flag `character_flag_A.dtx` at its position |
| 45 | rotate radar switched on in options mid-map | | no change until the next map |

**Install checks** (Steam build 25595979, the coordinator's Linux box):

46. The HUD map archive (title `LithTech Resource File`, payloads raw) holds `UI_HUD_MAP\MAPIMAGE\MAP_SHOWDOWN.DTX`, `MAP_JUNKFLEA2.DTX` and `MAP_WARHEAD.DTX`, each 1,048,740 bytes, 1024 x 1024, 1 mip, format code 6; `MAP_CARGOSHIP.DTX` 4,194,468 bytes, code 3; `MAP_DEATHDESERT.DTX` 524,452 bytes, code 4. 78 map images in all.
47. The radar art archive holds `BG.DTX` 256 x 256, `MASKING.DTX` 256 x 256, `GUIDE.DTX` 512 x 512, `MAGNIFICATION_BG.DTX` 16 x 256, `MAGNIFICATION_BAR.DTX` 16 x 16, `ICON\LOCATIONINDICATOR.DTX` 64 x 32, all code 3.
48. Mission file `GMS\SHOWDOWN_T.GMS` (V1, world `Worlds2\Warehouse.dat`): `LargeMap` `UI_HUD_Map\mapImage\Map_showdown.dtx`, 1024 x 1024, points (128, 128) ↔ <−2048, 0, 2304> and (895, 895) ↔ <1536, 0, −1280> → f = 4.672751, LT = (−2646.112, 2902.112), RB = (2138.785, −1882.785). The Warehouse world's offset is (−480, 5984, 1216).
49. `GMS\JUNKFLEA2_T.GMS` (world `Worlds2\Junk_Flea2.dat`, offset 0): points (128, 128) ↔ <−2560, 0, 2560>, (895, 895) ↔ <2560, 0, −2560> → f = 6.675359, LT = (−3414.446, 3414.446), RB = (3421.121, −3421.121).
50. `GMS\WARHEAD_T.GMS` (world `Worlds2\WarHead.dat`, offset (1570.5, −3543, 640)): (128, 128) ↔ <−2048, 0, 3072>, (895, 895) ↔ <4096, 0, −3072> → f = 8.010430, LT = (−3073.335, 4097.335), RB = (5129.346, −4105.346).
51. `GMS\JUNKFLEA_T.GMS`: (128, 128) ↔ <−1408, 0, 1408>, (895, 895) ↔ <2688, 0, −2688> → f = 5.340287, LT = (−2091.557, 2091.557), RB = (3376.897, −3376.897) (used by tests 18, 19, 21).
52. The large-map attribute file's `[RadarInfo]` `DefaultWorldSize` is 1500.

## Open questions

1. **Build match:** the panel code and the default key bindings were read from reference material; the layouts, attributes and mission files were read from the Steam build. Check that M opens the map, `-`/`=` zoom and the radar sits top right at 1024 x 768 in the Steam client.
2. **Where icons fall at a screenshot level:** check test 1 against the live game by standing a teammate 300 u east and 400 u north (e.g. measured with positions from a demo or the console).
3. **Texture addressing** of the radar's map window outside [0, 1] near the map's edge (wrap, clamp or border). **Answered (2026-10-28):** clamp, and the images' transparent margins make the area outside the picture empty (R5; Corrections (2026-10-28) 2). Only a screenshot check at a map corner is left.
4. **world_dat.md contradiction:** it says the world offset is kept only for texture-animation scripts and "gameplay uses file positions as they are". The radar's map window and the full map's icon positions also add it (R5, F3). world_dat.md should mention this use.
5. **Player positions sent to clients:** whether the server sends every player's position always, or culls by visibility. If it culls, revealed enemies (flag carriers, scans) may freeze on the radar. Not traced.
6. **Team ids in free-for-all modes** (hud_crosshair.md Open question 15): the server picks round winners by team id, which suggests per-player ids, so other players are enemies and stay hidden. Check in One Man Army that nobody shows on the radar.
7. **Panel update order:** whether the full map's per-frame object discovery runs before or after the radar in the same frame (one-frame lag for a newly dropped flag at most).
8. **Default keys in the Steam build:** the engine's built-in defaults give M, `-` and `=`; the shipped config may override.
9. **Death marks across rounds:** they are cleared only on entering a world. Check whether the client re-enters the world between rounds (then marks are per round).
10. **Satellite scan timer resolution:** the server checks the 7000 ms in its message handling; the exact moment the "off" arrives was not settled (at most one server frame late is assumed).
11. **A different round-start message order** exists in a mode family not in the room list (the image name before the corners); harmless for the modes that play, but a reader of that family would swap fields.

## Corrections (2026-10-22)

From a comparison pass over the reference's radar panel, full-screen map
panel and the map key's handling against this spec and our
implementation (`games/combat_arms/src/`, file:line relative to that
folder). They change R6, R8, F1 and F4; the numbers and transforms above
stand.

1. **Arms Race: scanners never reveal an enemy on the radar** (R6 rule 5
   and 6).
   - Reference: in Arms Race the radar's enemy rules stop at the
     last-stage test: an enemy in the last stage is drawn; any other enemy
     is not drawn, and the scan rules (shared scan, own heartbeat contact,
     a teammate's shared heartbeat list) are not consulted at all in that
     mode. In every other mode the scan rules apply as written. The full
     map is not affected: there a shared scan still shows every enemy in
     Arms Race (F4).
   - Ours: `game/radar/look.rs:148` reveals an enemy when it is in the
     last stage **or** under any scan, in every mode.
   - Expect: Arms Race, Alpha viewer, Bravo enemy at rel (100, 100) on
     ladder step 3 of 5, satellite scan for Alpha running → not on Alpha's
     radar, but on Alpha's full map. Same with the viewer's heartbeat
     sensor on and the enemy 500 u ahead in view → not on the radar. The
     same enemy on the last step → on the radar, with or without a scan.
     Elimination, the same scan → on the radar (unchanged).

2. **A flag carrier's edge arrow shows only the flag badge** (R6 "Flag
   badge", R8).
   - Reference: when a carrier is out of range, the icon drawn over the
     turned arrow is the flag badge **instead of** the character icon (the
     arrow's icon slot is given the badge's image). In range, the
     character icon and the badge are both drawn, as R6 says. Bomb and
     bullion carriers' arrows keep their own icon (no badge).
   - Ours: `ui/hud/radar.rs:574` draws the character icon in the arrow's
     box and then `:575-577` the badge over it.
   - Expect: test 10's arrow (enemy carrier at rel (2000, 0)) draws two
     images: the arrow `locationIndicator.dtx` and `character_flag_A.dtx`
     in the same 64 x 32 box; no `character_enemy.dtx` in that box. In
     range (rel (100, 100)) both `character_enemy.dtx` and the badge are
     drawn.

3. **The full map opens and stays open after the round has ended** (F1).
   - Reference: the key press handler refuses to open the map after the
     round has ended, but the map's own per-frame check, which runs every
     frame and decides visibility alone, has no round-end condition: while
     the key is held and none of F1's other conditions (chat input,
     in-game popup, letterbox, video, AI freeze) holds, the map is shown.
     So holding M during the round-end and intermission pause shows it
     (from the next frame on), and a map held across the round end stays.
   - Ours: `ui/hud/full_map.rs:124-127` hides it in the round-over,
     intermission and match-over phases.
   - Expect: hold the map key from 1 s before a round ends until 3 s after
     → visible the whole time. Press it 2 s after the round ended → visible
     from the frame after the press while held. F1's sentence "Pressing it
     after the round has ended does not open it" is replaced by this.

4. **Dead players on the full map** (F4; R11).
   - Reference:
     - A dead player's mark (the death icon, or any icon a dead player is
       still shown with) is placed at the position the player had **when
       the kill message arrived**, kept until the player is set alive
       again; it does not follow the body afterwards. No wedge is drawn
       for a dead player.
     - **Enemies are not hidden for being dead**: an enemy that a rule of
       F4 still reveals while dead (a shared scan running, Arms Race's
       last stage, a bullion count not yet cleared) stays on the full map
       with its living icon (`character_enemy.dtx` or the variant), no
       wedge, at its death position, until respawn or until the rule
       stops.
     - **A dead VIP ally shows the VIP icon** (`character_VIP.dtx`) at its
       death position, not `area_death_ally.DTX`: the VIP icon is chosen
       after the death icon and replaces it. (A dead ally holding the
       Bombing Run bomb would likewise show the bomb icon; the bomb is
       dropped on death, so this does not occur in play.)
     - The radar is unchanged: every dead player is hidden there.
   - Ours: `game/radar/look.rs:172` drops every dead enemy from the full
     map; `:159-165` gives every dead ally the death icon, VIP included;
     `ui/hud/scene.rs:233` places dead players at their current transform.
   - Expect: satellite scan by Alpha running, Bravo enemy E killed at V →
     Alpha's full map shows `character_enemy.dtx` at V with no wedge until
     E respawns or the scan ends; E is not on Alpha's radar. VIP Escort,
     the own VIP killed at V → `character_VIP.dtx` at V on the full map, no
     wedge. An ally killed at V whose body is then moved 200 u (a physics
     push) → the death icon stays at V.

5. **Alpha flag carriers may show no badge on the radar** (R6 "Flag
   badge"; to verify before implementing).
   - Reference: the radar looks up Bravo's flag-badge image (for an Alpha
     carrier, who carries Bravo's flag) under a layout-attribute key whose
     name is misspelt (two letters swapped) in the radar code only; the
     full map, the observer map and the loose-flag icons use the correct
     key `LargeMapBravoFlagMove`. Unless the HUD layout attributes also
     carry the misspelt key, an Alpha carrier on the radar gets an empty
     badge (nothing drawn over the icon) and an empty icon on its edge
     arrow (correction 2: only the turned arrow image), while Bravo
     carriers show `character_flag_A.dtx` normally.
   - Ours: both teams' carriers get their badge (`game/radar/look.rs:87-89`).
   - Expect (only once the install's `[HUDLargeMap]` section is checked
     and has no misspelt key): an Alpha player carrying Bravo's flag at rel
     (100, 100) → radar shows its character icon without a badge; out of
     range → an arrow with no icon. Full map: badge `character_flag_B.dtx`
     as now. Until checked, keep ours.

Could not determine here: whether the install's layout attributes carry
the misspelt key of correction 5; whether the HUD (and so the map's
per-frame check) keeps updating during every kind of round-end screen
(the scoreboard shown at a match's end may sit over it).

## Corrections (2026-10-28)

From a pass over the reference's UI image drawing, the HUD manager's
render states and the radar panel, checked against the unkeyed radar art
and map-image archives of the Steam install (decoded in memory). The
HUD layout file itself (`xml/hud_main.xml`) is in a keyed archive and was
not re-read here; the layout rects above come from the first pass.

1. **The radar frame is drawn at one texel per ref px, cropped, not
   stretched** (Constants "radar frame"; R6 draw order).
   - Rule (ui_system.md W1): an image control without custom UVs shows
     the **top-left w x h texels** of its texture, u from 0 to w / T_w and
     v from 0 to h / T_h, stretched only by the screen scale (x_r, y_r).
     So the frame control (782, −30) 227 x 226 over the 256 x 256
     `bg.dtx` shows texels 0..226 x 0..225 at their own size, u 0 to
     227/256 = 0.88671875, v 0 to 226/256 = 0.8828125. Columns 227..255
     and rows 226..255 are not drawn.
   - The art (Steam install, 32-bit, one mip): `bg.dtx` is transparent
     except a one-texel black outline at alpha 128 whose columns are
     texels 35 and 221 and whose rows are 34 and 220 (a 187 x 187 square,
     both ends included). It is **not** centred in the texture: 35 texels
     from the left, 34 from the right, 34 from the top, 35 from the
     bottom. Everything outside the 227 x 226 crop is transparent, so the
     crop changes nothing visible.
   - On screen at 1024 x 768 (the drawing subtracts half a pixel, so
     texel i lands exactly on pixel x + i): the outline's left and right
     columns are pixels **817 and 1003**, its top and bottom rows **4 and
     190**. With the window M = (818, 5) 186 x 186 (pixels 818..1003,
     5..190), the left and top lines sit one pixel **outside** M and the
     right and bottom lines lie **on** M's last column and row. The frame
     is drawn after the clipped window content and is not clipped.
   - Why the numbers looked wrong: the table gave the control rect but not
     the texturing rule. Stretching the 256 x 256 texture into 227 x 226
     shrinks the outline to about 165 x 164 px, which does not fit the
     window. The layout numbers were right. Ours
     (`games/combat_arms/src/ui/hud/radar.rs:71`, `FRAME_AT`) draws the
     whole texture at (783, −30) 256 x 256. That puts the outline at
     columns 818 and 1004, **one pixel right** of the stock frame, and its
     rows at 4 and 190 (right). Fix: (782, −30) and a 227 x 226 quad with
     UVs (0, 0)..(227/256, 226/256). Drawing the whole texture at
     (782, −30) 256 x 256 gives the same picture, because the cropped part
     is transparent.
   - The same rule holds for the other radar images: the zoom bar's track
     (`magnification_bg.dtx`, 16 x 256, with art in rows 0..186) shows its
     top 192 rows in its 16 x 192 rect. The knob (16 x 16 art, 16 x 10
     rect) shows its top 10 rows. `guide.dtx` (512 x 512 in a 512 x 512
     rect) and the 64 x 32 icons show whole. The map quad sets its UVs
     itself (R5).

2. **Texture addressing of the map window: clamp** (R5; Open question 3).
   - Reference: before the HUD panels draw, the HUD manager sets the draw
     primitive's addressing to clamp, along with its other states, and the
     radar panel does not change it. Panels that switch to wrap for a
     moment (the recording overlay, the gun-smith screen) set clamp again
     afterwards. Wrap is the primitive's own default and the menus' mode
     (ui_system.md Open question 5). The HUD panels never draw with it.
   - Effect: a window texel coordinate below 0 or above 1 reads the
     image's outermost texel column or row (with the usual bilinear
     filtering inside the image). It is neither wrapped to the opposite
     side nor a border colour.
   - Why Warehouse's top shows no picture: the images are padded. In the
     Steam install 73 of the 78 map images have a fully transparent
     outermost row and column (alpha 0, colour 0). Clamping therefore
     reads transparent texels outside [0, 1]. Each picture also sits inside
     a wide transparent margin. `Map_Showdown.dtx` (Warehouse) has non-zero
     alpha only in texels x 216..797, y 161..897, so well before the
     window reaches the image's edge it shows the transparent margin, at
     alpha 190 × 0, so nothing of the map is drawn there. Five images
     have opaque or partly opaque edges (`map_overdose`, `map_lastvegas`,
     `map_deathdesert`, `map_gravedigger`, `map_darkforest`). Beyond their
     edges, clamping smears the edge texels outward in streaks.
   - Ours should sample with clamp-to-edge. A transparent clear colour, or
     discarding samples outside [0, 1], matches the 73 padded images but
     not the five opaque-edged ones.

Test cases added by these corrections (1024 x 768 unless stated):

| # | Setup | Expected |
|---|---|---|
| 53 | radar frame drawn | quad (782, −30) 227 x 226, UVs (0, 0) to (0.88671875, 0.8828125) of `bg.dtx`. Outline pixels: columns 817 and 1003 (rows 4 to 190), rows 4 and 190 (columns 817 to 1003), black at alpha 128. Nothing else drawn by the frame |
| 54 | as 53 at 1920 x 1080 | quad x 1466.25, y −42.1875, w 425.625, h 317.8125, same UVs |
| 55 | frame vs window | outline column 817 = M's left edge − 1, row 4 = M's top − 1; column 1003 and row 190 are M's last column and row |
| 56 | zoom track drawn | `magnification_bg.dtx` texels rows 0..191 in (1004, 4) 16 x 192, v 0 to 0.75; not stretched |
| 57 | Warehouse (Showdown data, O_z = 1216), z = 1, fixed north, player at file (0, y, 1000) | window top edge samples v = (2902.112 − 2966) / 4784.897 = −0.01335 (clamped to row 0, transparent). The picture's first opaque row (texel 161, file z 933.80) is at screen y = 5 + (1750 − 933.80) · 0.124 ≈ 106.2. The window above that row shows no map picture. Only its top ≈ 7.9 px (screen y 5 to 12.9) is outside [0, 1] |
| 58 | a map whose image has an opaque edge (e.g. `map_lastvegas`), player near that edge | outside [0, 1] the window repeats the edge texel's colour outward (clamp), not the opposite side of the image (wrap) and not empty |
| 59 | any padded image, texel coordinate (−0.3, 0.5) | reads texel column 0 of row 512: transparent, nothing drawn |
