# Counter-Strike: Source: objectives (bomb and hostages)

Source basis: Valve's public Source SDK 2013 only for code: the shared
player "use" search (which has a Counter-Strike branch for its trace mask
and excludes the C4 from use-pushing), the client HUD's audio-message
handler (CS-only), the nav-mesh "no hostages" attribute, and the generic
radius damage already described in grenades.md. CS:S's own game DLL (the C4
weapon, the planted bomb, hostages, round rules, money) is **not** public.
Everything about it comes from data in the user's install, read as values
only: the BSP entity lumps of all 18 stock maps (bomb targets, rescue zones,
hostages, map parameters, brush-model bounds), `resource/modevents.res`
(game event fields), the sound scripts (`game_sounds_weapons.txt`,
`game_sounds_radio.txt`, `game_sounds_hostages.txt`) and the list of sound
files actually shipped, the view model, world models and third-person
sequences (`v_c4`, `w_c4`, `w_c4_planted`, `w_defuser`, `cs_player_shared`,
`hostage_01..04`), `scripts/hudlayout.res`, `resource/clientscheme.res`,
`scripts/mod_textures.txt`, `scripts/vgui_screens.txt` and the C4 screen
layouts, the C4 panel colour scheme, the HUD sprite headers, the English
localization (`cstrike_english.txt`), and the server's log-line formats and
console-variable names (what a server log and `cvarlist` show). Numbers that
come from well-known public CS:S facts rather than from data are marked
*public*, and anything not checkable here is *hyp.* with an in-game check
under Open questions.
Status: draft

## Summary

- **Bomb defusal (de_ maps, 12 stock maps):** at round start one random
  Terrorist gets the C4. Only Terrorists can carry it; it drops on death or
  with the drop key and any Terrorist picks it up by touching it. Holding
  primary fire inside a bomb target (a `func_bomb_target` brush), on the
  ground, plants it in 3 s (*public*); letting go, leaving the zone or the
  ground cancels. The planted bomb ticks for `mp_c4timer` seconds (45
  *public*), beeping faster and faster, then explodes with damage equal to
  the map's bomb radius (500 by default, from `info_map_parameters`), falling
  to 0 at 3.5 × that distance (*hyp.*). A Counter-Terrorist holding Use on it
  defuses it in 10 s, or 5 s with a $200 defusal kit; any interruption
  restarts from zero.
- **Round ends:** bomb exploded → T win; bomb defused → CT win; time runs out
  with no bomb planted → CT win; all Ts dead with no bomb planted → CT win;
  all CTs dead → T win (even after a plant). Once the bomb is planted the
  round clock no longer ends the round, and killing every T doesn't either:
  only the bomb does.
- **Hostage rescue (cs_ maps, 6 stock maps, 3–4 hostages each):** a CT
  presses Use on a hostage to make it follow him (again to make it stay);
  Ts cannot lead hostages. A hostage that touches a `func_hostage_rescue`
  brush is rescued (removed, money to the rescuer). All hostages rescued →
  CT win; time runs out → T win. Hurting and killing hostages costs money,
  and killing too many gets a player kicked (`mp_hostagepenalty`).
- **No VIP:** no stock CS:S map has VIP (as_) entities; the game keeps only
  leftover strings and radio sounds. Out of scope.

## Units and conventions

- Distance: inches ("units"); Source axes X forward, Y left, Z up. Player
  hull standing (−16, −16, 0)–(16, 16, 72), eye at 64 (movement.md).
- Time: seconds; server tick 0.015 s (CS:S ignores `-tickrate`,
  tools/css_probe/README.md). "After T seconds" means the first tick whose
  time is at or after start + T unless a rule says otherwise (Q14).
- Money in dollars, clamped to 0–16000 (rounds plan; `mp_startmoney`).
- HUD coordinates are in the 640 × 480 virtual screen of `hudlayout.res`
  (proportional); `c-150` means "screen centre − 150".
- Teams: T = attackers on de_ maps and defenders on cs_ maps; CT = the
  opposite. Our `rules::rounds` has fixed `ATTACKERS = Team(1)` (T) and
  `DEFENDERS = Team(2)` (CT); keep T/CT as the meaning and let the map
  type decide who wins on time (see Mapping).

## Constants

### Bomb

| Name (ours) | Value | Unit | Meaning / source |
|---|---|---|---|
| plant_time | 3.0 | s | primary held in zone until the bomb is planted (*public*; Q1). 200 ticks |
| plant_vm_anim | 2.7667 | s | `v_c4` press-button sequence (84 frames at 30 fps), ACT primary attack |
| plant_vm_clicks | 0.9, 1.2333, 1.5, 1.7, 1.9, 2.1, 2.2333 | s into the animation | `c4.click` sound events (7 key presses) |
| plant_vm_code | "7", "73", "735", "7355", "73556", "735560", "7355608" at the same times; "*******" at 2.6 s | – | text written to the C4's little screen (model event 7001) |
| plant_3p_anim | 2.8333 | s | third-person `{Idle,Walk,Run,Crouch_Idle,Crouch_Walk}_Shoot_C4` (86 frames at 30 fps) |
| c4_timer | 45 | s | `mp_c4timer` default (*public*; help text "how long from when the C4 is armed until it blows"); bounds 10–90 *hyp.* (Q2) |
| defuse_time | 10 | s | without kit (*public*) |
| defuse_time_kit | 5 | s | with kit (*public*) |
| kit_price | 200 | $ | localization "DEFUSAL KIT: $200"; CT only |
| bomb_radius_default | 500 | in | `info_map_parameters` `bombradius` when the map has none (*public*; Q3) |
| blast_damage | bomb_radius | hp | *hyp.* (Q3) |
| blast_range | 3.5 × bomb_radius | in | *hyp.*, same 3.5 factor as the HE grenade (grenades.md); 1750 by default |
| use_radius | 80 | in | shared player use search (SDK) |
| use_cone | 0.8 | cos | sphere-found objects must be within ~36.9° of the view |
| use_ray | 1024 | in | straight-ahead use trace length |
| use_probe | half-size 16 box swept 72 in | in | the tilted use probes |
| c4_carry_speed | 250 | in/s | weapon_c4 max speed (weapons.md) |
| c4_slot | 4 (HUD key 5) | – | weapons.md |
| c4_draw | 1.0 | s | `v_c4` draw |
| c4_drop_anim | 1.0 | s | `v_c4` secondary activity (31 frames at 30 fps); cosmetic |
| beep_interval_start / end | ~1 / ~0.1 | s | *hyp.* (Q4); provisional rule below |

Measured 2026-10-15 (probe server, de_dust2 site B, `mp_c4timer 45`, the
plugin logging every sound the C4 entities emit):
- Holding attack with the C4 drawn: seven `weapons/c4/c4_click.wav` keypad
  clicks (channel 1, level 326) at 0.86, 1.13, 1.35, 1.50, 1.67, 1.83 and
  1.94 s after the press (gaps 18, 15, 10, 11, 11, 7 ticks), then
  `c4_plant.wav` (channel 2) and the plant at 3.09 s. The press came 0.9 s
  after drawing the C4, so arming started when its 1 s deploy ended (tick
  67): arming took 3.0 s, and the clicks fall 0.765, 1.035, 1.26, 1.41,
  1.575, 1.74 and 1.845 s into it: the server's own timer, quicker than
  the view model's press-button events (0.9 to 2.233 s at 30 fps), which
  still pace the digits on the bomb's screen. Ours had the click sounds on
  the events too (fixed: sounds on the measured times).
- The explosion (`c4_explode1.wav`, level 140) 45.18 s after the plant.
- **No beep is emitted by the server**: the planted bomb's beeps are the
  client's own, from the bomb's timer, so Q4's schedule needs the client's
  audio (docs/plans/active/windows-captures.md, sitting 2).
- **The blast (Q3, H2)**: `mp_c4timer 10`, planted at site B
  (−1478, 2768, 4), hovering targets of 1000 health on clear lines, every
  `player_hurt`: unarmoured damage 486 at 136 units, 447 at 286, 427 at 331,
  376 at 436, 330 at 529, 283 at 625, 238 at 714, 145 at 917, 111 at 1015,
  58 at 1213, 26 at 1411, 11 at 1609 (distances from 16 above the bomb's
  origin to 28 above the target's chest). That is the bell
  `500 × exp(−d² / 2σ²)`, σ = 1750 / 3 (reach = 3.5 × radius), within
  1.5 points everywhere; the linear H1 is off by up to 112. The planter at
  the bomb took 497–498. Armour (100, no helmet): health takes half, the
  armour half of the rest (376 → 188 and 94; 26 → 13 and 6), and when the
  armour can't cover its part it is all used and health takes the rest
  minus twice the armour (447 on 100 armour → 247 health, armour 0).
  Every hurt from the blast has **attacker 0** (the world, not the
  planter): with `mp_friendlyfire 0` it still killed two terrorists near
  it and the planter (2026-10-16), so no one is spared and the planter
  isn't credited. Walls between bomb and target and other radii not
  measured.

Stock map bomb radii (from the entity lumps):

| Map | `bombradius` | blast range (×3.5, hyp.) |
|---|---|---|
| de_chateau | 300 | 1050 |
| de_inferno | 400 | 1400 |
| de_nuke | 400 | 1400 |
| de_piranesi | 500 | 1750 |
| de_aztec, de_cbble, de_dust, de_dust2, de_port, de_prodigy, de_tides, de_train | none → 500 | 1750 |

`info_map_parameters` also has `buying` (inferno, nuke, piranesi set 0).
Public FGD meaning: 0 everyone may buy, 1 only CTs, 2 only Ts, 3 nobody.

### Money (all *public*, unverified for CS:S; Q5)

These are the long-standing Counter-Strike values (1.6 and CS:S shared one
economy until CS:GO). The first three rows are already in our
`RoundSettings`.

| Name (ours) | Value | Who gets it |
|---|---|---|
| win_elimination | 3250 | winning team, elimination win (in `RoundSettings::win_bonus`) |
| loss_bonus | 1400 + 500 per earlier consecutive loss, max 3400 | losing team (in `RoundSettings`) |
| kill_reward / teamkill_penalty | 300 / −3300 | killer (in `RoundSettings`) |
| win_bomb_exploded | 3500 | every T |
| win_bomb_defused | 3500 | every CT |
| win_target_saved | 3250 | every CT (time ran out, no plant) |
| plant_loss_bonus | 800 | every T on top of the loss bonus when the bomb was planted but the Ts lost |
| planter_reward | 300 *hyp.* | the planter (Q5) |
| defuser_reward | 0 *hyp.* (CS:GO pays 300) | the defuser (Q5) |
| win_all_rescued | 3500 *hyp.* | every CT |
| win_hostages_not_rescued | 3250 *hyp.* | every T (time ran out) |
| hostage_touch | 150 *hyp.* | CT who makes a hostage follow him for the first time this round |
| hostage_rescue | 1000 *hyp.* | the CT whose hostage is rescued |
| hostage_kill | −1500 *hyp.* | the killer, either team |
| hostage_hurt | small, scaled by damage *hyp.* | the attacker ("You injured a hostage!") |

### Hostages

| Name (ours) | Value | Unit | Meaning |
|---|---|---|---|
| hostage_health | 100 | hp | measured (Q8); a float; damage by the rule below |
| hostage_models | `characters/hostage_01..04` | – | the only hostage models shipped; maps name `models/hostage.mdl` (cs_havana, cs_office) or `HostageType` 0/1 (cs_assault, cs_compound), neither of which selects them: each hostage spawned takes the next of the four in turn, over rounds and maps (measured, Q9) |
| hostage_hull | 26 × 26 × 72 | in | its collision box, −13..13 by 0..72 (measured, Q11) |
| hostage_rest | floor + 1/32 | in | where a hostage's origin (its feet) settles: −159.968 on cs_office's −160 floor (measured, "Spawn" below) |
| hostage_anims | HL2 `Humans/male_shared`, `male_gestures`, `male_postures` (included) | – | own file has only a 101-frame ragdoll sequence |
| hostage_follow_* | see Behaviour, *hyp.* | – | Q10 |
| hostage_penalty | `mp_hostagepenalty` (default *hyp.* 13) | kills | kick after this many hostage kills; a warning hint one kill before |

Stock maps: cs_assault 4, cs_compound 3, cs_havana 4, cs_italy 4,
cs_militia 4, cs_office 4 hostages. Every cs_ map has 1–4
`func_hostage_rescue` brushes; no stock map uses the point entity
`info_hostage_rescue`, `info_bomb_target`, or any VIP entity.

Measured 2026-10-16, **hostage damage (Q8), the rule** (probe server,
cs_office, hostage 121 as below, AK-47 and Desert Eagle from 150 units, a
SourceMod hook on the hostage's TraceAttack and OnTakeDamage logging each
hit's damage before the multiplier, its hitgroup, and the damage taken;
three shots per aim height 66, 60, 48, 36, 24, 12):

- Each hit takes the shot's damage (after range falloff) × a hostage-only
  hitgroup multiplier: **head 2.5, chest 1.5, stomach 1.75, left and right
  arm 0.75, left and right leg 0.6** (exact to 3 decimals on both guns,
  e.g. AK 35.79 → head 89.47, chest 53.69; Deagle 50.82 → stomach 88.94).
- A bullet often hits the same hostage **twice**: after the first box it
  goes on through the hostage and hits another box (or the same group
  again) with half the damage, range falloff applied again (AK 35.79 →
  17.78, Deagle 50.85 → 23.78: × 0.5 × RangeModifier^(d/500)), the
  rule M13 found for a player a bullet passes; whether it does depends
  on where the spread puts the line (z 60: chest, then chest again in 2
  of 3 shots).
- Health is a float: the hits add up and the lost health shown is
  truncated after (53.69 + 26.67 → 80, not 79).
- Every damage below and in the repeat fits this; the table that follows
  is the first, single-shot pass, kept for the record (its "ours"
  columns are from before ours followed the rule; ours now does: the
  heavy test `map_objectives::office_hostage_shots_from_150_units`
  checks each shot against the rule on the boxes ours crosses).

Measured 2026-10-16, **hostage damage (Q8), no rule found**: probe
server, cs_office, hostage 121 (the map's first `hostage_entity`) with its
feet at (1784, 734, −159.9), health 100; the shooter standing at
(1634, 734, −159.9), eye 64 above his feet; one shot per round restart at
the hostage's feet + (0, 0, z), its health read after. Ours: the same
shots on our cs_office (heavy test
`map_objectives::office_hostage_shots_from_150_units`), the hostage's
player hitgroups on its model's first hitbox set (all four hostage
models have the same boxes) and the player multipliers.

| Weapon | z | CS:S damage | ratio to the shot's damage after falloff | ours | our hitgroup (boxes the ray crosses) |
|---|---|---|---|---|---|
| AK-47 (36) | 66 | 89 | 2.49 | killed (143) | head (head only) |
| AK-47 | 60 | 53 | 1.49 | 35 | chest (chest only) |
| AK-47 | 48 | 53 | 1.49 | 35 | chest (chest, left arm) |
| AK-47 | 36 | 26 | 0.73 | 44 | stomach (stomach, both legs) |
| AK-47 | 18 | 21 | 0.59 | 26 | right leg (both legs) |
| Desert Eagle (54) | 66 | killed | ≥ 1.97 | killed (203) | head (head only) |
| Desert Eagle | 60 | 76 | 1.50 | 50 | chest (chest only) |
| Desert Eagle | 48 | 38 | 0.75 | 50 | chest (chest, stomach) |
| Desert Eagle | 36 | 38 | 0.75 | 63 | stomach (stomach, both legs) |
| Desert Eagle | 18 | 44 | 0.87 | 37 | right leg (both legs) |

(Falloff at ~145 units: AK 35.8, Deagle 50.9.) No single rule fits all
ten: at z 48 and 18 the two weapons, aimed at the same point, take
different shares (1.49 vs 0.75; 0.59 vs 0.87), and per-weapon falloff
would only make the Deagle's share smaller, so whatever decides it isn't
the aim point alone: most likely the first-shot spread (about ±1.5 units
for the AK, ±2.5 for the Deagle at this range) landing on different
boxes, or the bullet hitting the hostage more than once (the probe bot
logged up to 3 impacts per shot). On our geometry no set of hitgroup
multipliers fits either: the Deagle's z 60 shot crosses only the chest
(share 1.5) but its z 48 shot, entering the chest first, took 0.75. The
shares that agree between weapons (head about 2.5, chest 1.5, z 36 0.75)
hint at a hostage-only table or a second hit at half damage (head 2 +
0.5, chest 1 + 0.5), but ten single shots can't pin it. Ours keeps the
player rule. To settle it: several shots per point, from close range (32
units, where spread is under 0.5 units) at each hitbox's centre, with
the hitgroup logged (`hostage_hurt` has none: hook the hostage's damage
with the hit's hitgroup) and every impact per shot.

Repeated 2026-10-16 (the same hostage; four AK-47 shots per aim point,
each after a round restart; damage taken per shot): from 150 units, z 60:
80, 53, 53, 53; z 48: 53, 53, 53, 67; z 36: 62, 62, 26, 21; z 24: 21, 21,
32, 37. From 40 units, z 60: 80, 53, 53, 53; z 48: 85, 53, 26, 85; z 36:
62 ×4; z 24: 79, 73, 62, 21. Every shot logged 2–3 bullet impacts. The
same aim gives different damage (spread lands on different boxes), and
the values cluster on 21, 26, 53, 62, 80, 85: chains of hits at halving
damage explain several (chest + chest 53, stomach + chest 62, leg 26 alone)
but not 21 or 32. Still open; settling it needs the hitgroup of each hit
(a plugin hook on the hostage's damage, not `hostage_hurt`).

### HUD (from `hudlayout.res`, `clientscheme.res`, `mod_textures.txt`)

| Element | Position, size (640×480) | Look |
|---|---|---|
| Carried-bomb icon | (16, 240), 40 × 40 | icon-font glyph `j`, green 0 160 0 255; flashes to red 160 0 0 255 (Q12: when inside a bomb target) |
| Defusal-kit icon | (16, 240), 40 × 40 | glyph `f`, green |
| Rescue-zone icon | (16, 240), 40 × 40 | glyph `g`, green, flash red |
| Buy-zone cart (existing) | (64, 240), 40 × 40 | glyph `c`, green |
| Scenario icon | (c+110, 443), 40 × 44 | colour 255 176 0 120 (the "dim orange" panel colour); glyph `d` = bomb, `k` = hostage |
| Progress bar (planting, defusing) | (c−150, 300), 300 × 15 | border 1; fill colour 255 30 13 255 (scheme "progress bar" colour; Q12); client option `cl_c4progressbar` "Show a progress bar when defusing the C4" |
| Scoreboard | – | 64 × 64 "bomb" icon next to the carrier (Ts only), "defuser" icon next to CTs with a kit |
| C4 screen (on the planted and view-model bomb) | 200 × 100 panel, text centred | armed text colour 255 30 13 200, defused 200 200 200 200 |
| Planted-bomb sprite | `sprites/c4` 64 × 64, additive, drawn through walls | the bomb's position marker (Q12) |
| LED glow | `sprites/ledglow` 64 × 64 sprite | blinks with the beep (Q4) |

### Sounds

| Moment | Entry | Notes |
|---|---|---|
| each key press while planting | `c4.click` | weapon channel, from the view-model events (wave `weapons/c4/c4_click.wav`) |
| bomb placed | `c4.plant` | voice channel, `c4_plant.wav` (*hyp.* that it plays at plant) |
| each beep | `C4.PlantSound` | voice channel, attenuation 1.0, `c4_beep1.wav`; the name is misleading, it is the beep |
| defuse starts | `c4.disarmstart` | voice channel, `c4_disarm.wav` |
| defuse completes | `c4.disarmfinish` | item channel, same wave |
| explosion | `c4.explode` | voice channel, level 140 dB ("gunfire"), `)weapons/c4/c4_explode1.wav` (stereo, spatialized) |
| buying / picking up a kit | `defuser.equip` | weapon channel |
| announcer: planted | `Event.BombPlanted` | `radio/bombpl.wav` |
| announcer: defused | `Event.BombDefused` | `radio/bombdef.wav` |
| announcer: hostage taken | `Event.HostageTouched` | `radio/hostagecompromised.wav` (to Ts, *hyp.*) |
| announcer: hostage rescued | `Event.HostageRescued` | `radio/rescued.wav` |
| announcer: hostage killed | `Event.HostageKilled` | `radio/hosdown.wav` |
| round end | `Event.TERWin` / `Event.CTWin` / `Event.RoundDraw` | already played by `map::RoundSounds` |
| hostage pain | `Hostage.Pain` | 6 waves |
| hostage starts following a CT | `Hostage.StartFollowCT` | 8 waves |
| hostage stops following | `Hostage.StopFollowCT` | 5 waves |

All announcer entries are static channel, volume 1, and are sent to clients
as a "play this entry" message that each client plays on itself, with no
position (SDK HUD handler). The sound files `weapons/c4/c4_exp_deb1.wav` and
`c4_exp_deb2.wav` ship but no script entry names them (debris; unused or
played by name, Q4).

## Behavior

### 1. Who gets the bomb

- A map "has bomb targets" when its entity lump has at least one
  `func_bomb_target` (or `info_bomb_target`). Only then does anyone get a
  C4, and only then do the bomb round-end rules below apply.
- At each round start (after everyone is respawned and re-equipped, before
  the freeze ends), pick one Terrorist uniformly at random among those
  alive and give them `weapon_c4` in slot 4. Bots and humans are treated
  alike (*hyp.*, Q6). With no Terrorists, nobody gets one.
- A C4 left on the ground from the previous round is removed with the
  other loose weapons (our round restart already clears them). Surviving
  carriers do not keep a second bomb: there is exactly one per round.
- The carrier gets the hint "You have the bomb. Find the target zone or DROP
  the bomb for another Terrorist." and the HUD bomb icon. Teammates see the
  bomb icon next to his name on the scoreboard; CTs see nothing.
- The C4 can't be bought (price 0, not in the buy menu), weighs 0, and does
  not slow its carrier (max speed 250 = knife speed).

### 2. Carrying, dropping, picking up

- Drop: the drop command with any weapon selected drops the *active*
  weapon (weapons.md T21); with the C4 active it is thrown like a gun
  (velocity 400 along the view, touchable again after 1 s). Message to
  Terrorists only: "<name> dropped the bomb." Log: `triggered
  "Dropped_The_Bomb"`. Game event `bomb_dropped`.
- Death: the carrier always drops the C4 where he died (as with his
  primary), whichever weapon was active.
- Pickup: a living Terrorist touching a loose C4 takes it (into slot 4,
  not switched to). Message to Ts: "<name> picked up the bomb."; to the
  picker: "You picked up the bomb." Log `triggered "Got_The_Bomb"`, event
  `bomb_pickup`. CTs walk over it without effect.
- A loose C4 is never removed during the round and can't be pushed with
  Use (SDK: the C4 is excluded from use-pushing).
- Who sees a loose bomb: Terrorists see it on the radar (Q12).

### 3. Planting

Inputs: carrier's active weapon is the C4, primary fire held, the
carrier's "in bomb target" flag, on-ground flag.

- **In a bomb target** means the player's hull overlaps a `func_bomb_target`
  brush (a trigger, so the brush's own shape counts, not just its box) or is
  within a radius of an `info_bomb_target` point (radius *hyp.*, unused by
  stock maps).
- **Start:** pressing primary fire with the C4 active starts arming if the
  player is in a bomb target and on the ground. If not in a target: centre
  message "C4 must be planted at a bomb site." (nothing starts). If in a
  target but airborne: "You must be standing on the ground to plant the
  C4." On start: the view model plays the press-button animation, the
  third-person body plays `*_Shoot_C4`, event `bomb_beginplant` (site),
  and the progress bar runs for 3 s.
- **While arming** (held each tick): the player can't move (*hyp.*: speed
  forced to 0, crouching allowed, Q1); the key presses click on the view
  model's schedule and the little screen fills with 7355608.
- **Cancel:** releasing primary fire, switching weapons, leaving the
  target or the ground before 3 s ends arming with no bomb: the progress bar
  disappears, the view model returns to idle, event `bomb_abortplant`.
  Leaving the zone also prints "Arming sequence canceled. C4 can only be
  placed at a bomb target." Progress is not kept: a new press starts again
  from 0.
- **Plant:** when 3 s have passed with the button still held and the
  conditions still true:
  1. The carried C4 is removed from the player (his previous weapon is
     selected, *hyp.* the last-used one).
  2. A planted bomb appears at the player's feet (origin, on the ground he
     stands on), yawed with the player, model `w_c4_planted`, not solid to
     players (*hyp.*, Q7). It remembers the planter and the bomb target.
  3. Its timer starts at `mp_c4timer` read now (later changes don't affect
     a planted bomb).
  4. `c4.plant` plays, the announcer `Event.BombPlanted` plays for
     everyone, centre text "The bomb has been planted." for everyone,
     event `bomb_planted` (site, x, y), log `triggered "Planted_The_Bomb"`.
     The bomb target fires its `BombPlanted` output if present (*hyp.*
     from the public FGD; no stock map uses it).
  5. The round enters the "bomb planted" state (section 7).
- You can plant at any time in the live round, but not during the freeze
  (nobody can fire) or after the round has ended.

### 4. The planted bomb's timer, beeps and light

- `explode_at = plant_time + mp_c4timer`.
- Beeps: each beep plays `C4.PlantSound` at the bomb, flashes the LED glow
  sprite, and fires the game event `bomb_beep`. The interval shortens as
  `remaining` falls. Exact schedule: Q4. **Provisional rule** until
  measured: `interval = 0.1 + 0.9 · remaining / c4_timer` (1.0 s right
  after the plant, 0.1 s at the end); about 115 beeps for a 45 s bomb
  (`∫ dt / interval = 50 ln 10`).
- Whether the beep gets louder (lower attenuation) as time runs out, as in
  older Counter-Strike, is Q4.
- The bomb's small screen shows the armed code in red (C4 panel colour
  255 30 13 200) and turns grey (200 200 200 200) when defused.
- The HUD scenario icon (bottom centre, glyph `d`) shows the planted bomb
  to everyone, and the round timer is hidden (*hyp.*, Q12).

### 5. Defusing

Inputs: a living CT, his Use button, the use search (below), his
on-ground flag, whether he has a kit.

- **Finding the bomb** is the standard player use search (SDK, shared
  code with a Counter-Strike mask: brushes that block NPCs, opaque things
  and NPCs):
  1. From the eye, trace a ray 1024 in straight along the view. Then sweep
     a ±16 in box 72 in along seven directions tilted from the view
     towards the player's feet by tangents 1, 0.5774, 0.3640, 0.2679,
     0.1763 (45°, 30°, 20°, 15°, 10°) and upward by 0.1763, 0.2679
     (10°, 15°), where "down" uses the player's up vector.
  2. For each hit on a usable entity, its distance is the horizontal
     distance from the eye to the hit point, combined with the vertical
     distance from the hit point to the player's own hull's vertical range
     (0 if the hit point's height lies within the hull). If that is < 80,
     it is a candidate; a hit on the straight ray wins at once.
  3. Otherwise also check every usable entity within 80 in of the eye whose
     nearest point is within the use cone (unit direction · view ≥ 0.8),
     choosing the one nearest to the view line, if a line to it is not
     blocked.
  4. Nothing found: Use plays the deny sound.
  The planted bomb is "held" use (acts every tick the button is down,
  *hyp.*); a hostage is "pressed" use (once per press).
- **Start:** a CT whose use search finds the planted bomb, on the ground,
  starts defusing if no one else is: "Defusing bomb WITH defuse kit." or
  "... WITHOUT defuse kit."; `c4.disarmstart`; event `bomb_begindefuse`
  (haskit); the progress bar runs for 5 or 10 s. Airborne: "You must be on
  the ground to defuse the bomb." If another CT is already defusing: "The
  bomb is already being defused." A T using it does nothing.
- **While defusing** the CT can't move (*hyp.*, Q7), can look around a
  little (the use search must keep finding the bomb), can't fire.
- **Interrupt:** the defuse stops with no progress kept when the CT
  releases Use, the use search stops finding the bomb (turned away, moved),
  he leaves the ground, dies, or the round ends. Event
  `bomb_abortdefuse`. Starting again needs the full 5 / 10 s.
- **Success:** when 5 or 10 s have passed: the bomb stops beeping and its
  screen goes grey; `c4.disarmfinish`; announcer `Event.BombDefused`;
  centre text "The bomb has been defused."; event `bomb_defused`; log
  `triggered "Defused_The_Bomb"`; the bomb target fires `BombDefused` if
  present (*hyp.*); the round ends (CT win, "Bomb defused").
- **Race with the explosion:** if the defuse would end after `explode_at`,
  the bomb explodes. Exactly equal: Q14.
- **Kit:** `buy defuser` (and the buy menu's equipment page), CT only,
  $200, refused if he already has one; only in the buy zone and buy time,
  as other purchases. Shown as the HUD kit icon and on the scoreboard.
  Plays `defuser.equip`. Dropped on death as a pickup (`item_defuser`,
  model `w_defuser`); any living CT without a kit walking over it takes it
  ("You picked up a defuse kit!", `defuser.equip`). Ts can't take it.
  A kit lasts until death; survivors keep it into the next round.

### 6. Explosion

At `explode_at` (if not defused):

1. Effects at the bomb: a large fireball and smoke (the base explosion
   effect, scale *hyp.*), screen shake for players nearby (Q3), the
   `c4.explode` sound (140 dB, heard map-wide in practice).
2. Damage: **H2, measured** (see Measured above). Was *hyp.* H1 (template radius damage, as the HE grenade in
   grenades.md 5.3) with `D = bomb_radius` and `R = 3.5 · D`:
   `damage = D − d · D / R` for each player/entity within R whose
   line from the bomb is not blocked by world brushes. Alternative H2
   (CS:GO's bell curve): `damage = D · exp(−d² / (2 σ²))`, `σ = R / 3`,
   within R. Whether walls block the C4 blast at all is also open: Q3.
   Planter and teammates are hurt too. Armour: Q3.
3. The bomb target fires its `BombExplode` outputs (stock maps use them for
   fire, breakables, sparks, lights, alarms; see below) and its `target`
   key (*hyp.*: fired like an output; chateau `glass`, piranesi `cratesa`,
   tides `bomb_b_mm`/`bomb_a_mm` crossed over A/B).
4. Centre text "Target successfully bombed!"; event `bomb_exploded`; round
   ends: T win, "Bomb detonated". Deaths from the blast count as kills for
   the planter (*hyp.*, Q5) with weapon "c4".
5. The bomb entity is removed.

`BombExplode` outputs in stock maps (target, input, parameter, delay):

| Map | Site | Outputs |
|---|---|---|
| de_dust2 | B (`*2`) | `fireb` StartFire 0.2; `ag_fire_b` PlaySound 0.2 |
| de_dust2 | A (`*5`) | `targeta` Explode 0.5; `firea` StartFire 0.2; `ag_fire_a` PlaySound 0.2 |
| de_dust | `*1` | `fire_b` StartFire 0; `exp_a` Explode 0.5; `filter_A` Kill 0; `ag_fire_a` PlaySound 0 |
| de_cbble | A | `bomb_a` Break 0; `bomb_a_e1..e3` Explode at 0, 0.4, 1.2, 1.9, 2.1, 3.1, 3.7; `bomb_a_boxes` Break 0 |
| de_nuke | A, B | alarm: `emerg_rot1..9` StartForward, `emerg_sprite` ShowSprite, `emerg_glow`/`emerg_base` Skin 1, `radiationalarm` PlaySound; A also `bomba_fire` StartFire |
| de_inferno | A | `bomb_oranges` Shoot 0, `bomba_fire` StartFire 0, `bomb_orangesb` Shoot 0.5 |
| de_prodigy | both | Break glass/lens, Skin 1, Kill sprites, TurnOff lights, StartSpark 0.2, StartFire, fire light TurnOn |
| others | – | fire StartFire, breakables Break, lights TurnOn (similar); de_aztec and de_port have none |

Our logic layer (entity_io) already fires outputs; the bomb target just
needs `BombExplode` (and `BombPlanted`/`BombDefused` if present) wired.

### 7. Round flow with objectives

Live-round checks, every tick, in this order (the first that matches ends
the round):

| Map type | Condition | Winner | Reason text | Win panel |
|---|---|---|---|---|
| bomb | planted bomb exploded | T | "Target successfully bombed!" | "Bomb detonated" |
| bomb | planted bomb defused | CT | "The bomb has been defused." | "Bomb defused" |
| any | all CTs dead (and at least one T alive or a bomb planted) | T | "Terrorists Win!" | "CTs eliminated" |
| any | all Ts dead **and no bomb planted** | CT | "Counter-Terrorists Win!" | "Terrorists eliminated" |
| hostage | every hostage still alive has been rescued (at least one rescued) | CT | "All hostages have been rescued!" | "All hostages rescued" |
| bomb | round time out, no bomb planted | CT | "Target has been saved!" | "Bombing failed" |
| hostage | round time out | T | "Hostages have not been rescued!" | "Rescue failed" |
| neither | round time out | draw (ours: defenders) | "Round Draw!" | "Round Draw" |
| any | both teams dead together | draw | "Round Draw!" | – |

- After a plant, the round clock is ignored: the round lasts until the bomb
  explodes or is defused, or all CTs die. Ts dying doesn't end it.
- All CTs dead after a plant: T win immediately (*hyp.* that the round
  doesn't wait for the bomb; Q6).
- Hostage maps: "all rescued" counts only living hostages; if all
  remaining hostages are dead and some were rescued, CT win; if all are
  dead and none rescued, the round continues to elimination or time
  (measured, Q11: 3 rescued and the 4th dead, or 2 and 2, end the round at
  the last rescue, CT win, reason 10 "all hostages rescued"; all 4 dead:
  it goes on).
- Win announcements: `Event.TERWin` / `Event.CTWin` as now; on a defuse
  `Event.BombDefused` plays first (*hyp.* both play, Q6).

### 8. Money at round end and for objectives

- Winners get the reason's bonus (table above); losers the loss bonus as
  now. If the bomb was planted and the Ts still lost (after a plant that
  can only be a defuse), every T also gets +800.
- Planter and defuser rewards, hostage rewards and penalties: table
  above, all Q5.
- A defused/exploded round counts toward the loss streak like any other.

### 9. Hostages

- **Spawn:** each `hostage_entity` spawns at its origin with its yaw,
  standing, health 100 (*hyp.*), as an NPC that players collide with (Q10).
  The origin is its feet. It is not dropped to the floor: it falls under
  gravity (moving every 7 ticks, Q10) and rests 1/32 unit above the floor
  (measured 2026-10-17 on the probe: `mashup_spawn hostage_entity` 36 or
  50 units over cs_office's floor at (1700, 760) fell in 3–4 steps to
  −159.968; with its box in the ceiling, 60+ units up, it stayed there,
  stuck). Several stock hostages are placed above the floor, so they drop
  at each round start: cs_office's first two by 36 (origin z −124, floor
  −160), its other two by 4. Where they stand, `mashup_hostages` after a
  map load (x y z):
  cs_office 1784 734 / 1744 802 / 2048 −344 / 1984 −344, all −159.97;
  cs_italy 976 2392 0, 833 2339 128, 944 2320 0, 917 2336 128;
  cs_assault 6718.3 3895.2 / 6639.3 3889.9 / 6482.5 3839.5 /
  6537.5 3836.8, all −607.97; cs_compound 1578.8 −1581.6 / 1768 −1465 /
  1776 −1592, all 16; cs_havana 80 814 / 348 817 / 128 1280 / 240 1280,
  all 248; cs_militia 608 896 / 552 864 / 1256 400 / 832 600, all
  −151.97.
  Model: the next of `characters/hostage_01..04` in turn (a counter that
  goes on over rounds and maps; the map's model and `HostageType` don't
  matter; measured, Q9). Hostages are restored at
  every round start (they are map entities; our round restart re-creates
  them).
- **Use:** pressing Use (one press, use search §5) on a hostage:
  - CT, hostage idle: it starts following him. `Hostage.StartFollowCT`,
    event `hostage_follows`, log `triggered "Touched_A_Hostage"`,
    announcer `Event.HostageTouched` (*hyp.* to Ts), the first time this
    round: +$150 to the CT (*hyp.*). Hint: "Lead the hostage to the rescue
    point! You may USE the hostage again to stop him from following."
  - CT, hostage following him: it stops and stays. `Hostage.StopFollowCT`,
    event `hostage_stops_following`.
  - CT, hostage following another CT: it switches to the new leader
    (*hyp.*).
  - T: "Only Counter-Terrorists can move the hostages." Nothing else
    (*hyp.*: the hostage does not stop following its CT).
- **Follow (basic, *hyp.*, Q10):** a following hostage paths to its leader
  over the nav mesh, avoiding areas flagged "no hostages" (nav attribute
  0x0800, nav.md). It walks when close and runs when farther, stops
  within about 100 in of the leader, and jumps/crouches over small
  obstacles. If the leader dies or the hostage loses him for a while, it
  stops following and waits where it is. A provisional rule for a first
  slice: target speed 0 within 100 in, walk speed (~100 in/s) within
  250 in, run speed (~250 in/s) beyond; give up after the leader has been
  out of sight for 10 s or farther than 1500 in.
- **Damage:** hostages take bullet, grenade and bomb damage. Hurt: the
  attacker sees "You injured a hostage!" (and the hint "Be careful around
  hostages. You will lose money if you kill a hostage.") and loses money
  (Q5); `Hostage.Pain`; event `hostage_hurt`. Killed: ragdoll; "You killed
  a hostage!"; announcer `Event.HostageKilled`; event `hostage_killed`;
  log `triggered "Killed_A_Hostage"`; money penalty; the killer's hostage
  kill count rises, and at `mp_hostagepenalty` − 1 he is warned ("If you
  kill one more hostage, you will be removed from the server."), at
  `mp_hostagepenalty` he is kicked (0 = never).
- **Rescue:** a hostage that follows a leader and is walking (not
  standing near him) and whose hull (26 units wide) touches a
  `func_hostage_rescue` brush (or comes near an `info_hostage_rescue`
  point, Q13) is rescued (measured, Q11: an idle hostage, or one standing
  by its leader, inside a zone is not; a map without rescue zones rescues
  nobody, CT spawns included): it
  stops, disappears, its leader gets the rescue reward, announcer
  `Event.HostageRescued` for everyone, event `hostage_rescued` (site), log
  `triggered "Rescued_A_Hostage"`. When the last one is rescued: event
  `hostage_rescued_all` and the round check above.
- **HUD:** CTs inside a rescue zone see the rescue-zone icon at (16, 240)
  (like the buy-zone cart). The scenario icon area shows the hostages (one
  hostage glyph per hostage remaining, *hyp.*, Q12). Hostages show on the
  CT radar (Q12).
- **Voice:** CS:S ships waves only for `Hostage.Pain`,
  `Hostage.StartFollowCT` and `Hostage.StopFollowCT`. The script also
  defines greetings, reactions, grenade/smoke/flashbang lines, rescue
  lines and win lines whose waves are missing (the game's own console
  says it can't find them). So in practice hostages only speak those
  three.

### 10. VIP

No stock CS:S map has VIP entities (`func_vip_safetyzone`,
`info_vip_start`): checked all 18 maps. Leftovers: "VIP" strings, the
`vip_escaped`/`vip_killed` events, `radio/vip.wav`, and "You have been
rewarded $2500 for killing the VIP!". Not part of this spec.

## Per-tick order

Within the rules step (our `SimSet::Rules`, after movement and weapons):

1. Weapons run first: a held primary on the C4 updates the arming state
   (start / continue / cancel / plant). A plant creates the planted bomb
   this tick.
2. Use handling: each player's use search; a CT holding Use on the planted
   bomb starts or continues a defuse; release or loss of target aborts.
   Use presses on hostages toggle following.
3. Defuse completion check (`elapsed ≥ defuse_time`) **before** the
   explosion check (Q14).
4. Planted bomb: beep if due; explode if `now ≥ explode_at`.
5. Hostages: follow movement, rescue-zone touch.
6. Round-end checks (§7 table, in order).
7. Money for this tick's events (kills, plant, defuse, rescue, hostage
   damage) as they happen; round-end bonuses when the round ends.

## Edge cases

- **Planting on a slope or a prop:** on ground means standing on anything
  (world or entity). The planted bomb sits at the feet; whether it follows
  a moving platform is Q7.
- **Carrier in the zone during freeze:** can't plant (can't fire).
- **Bomb dropped into an unreachable place** (out of the map, deep water):
  stays there; no reset in CS:S (*hyp.*).
- **Two carriers:** impossible; giving a second C4 by command is allowed in
  CS:S (`give weapon_c4` with cheats) and then two bombs exist; only the
  first planted one counts (*hyp.*, not needed).
- **Last T planting dies:** if he dies before the plant completes, no bomb;
  if the bomb is planted the round continues even with no Ts alive.
- **Defuser killed at 4.9/5 s:** no defuse, another CT must start over.
- **Kit picked up mid-defuse:** the current defuse keeps its time (*hyp.*).
- **Teammate damage from the blast** obeys `mp_friendlyfire` like other
  damage (*hyp.*; Q3).
- **Hostage inside a rescue zone at spawn:** none in stock maps.
- **A T using a following hostage:** no effect (*hyp.*).

## Quirks

- The view model's planting animation is 2.7667 s but the plant takes 3 s:
  the last "*******" appears at 2.6 s and the hands hold still to the end.
  Keep both numbers.
- The code typed on the bomb is always 7355608.
- `C4.PlantSound` is the beep, not the planting sound.
- de_tides' A site's legacy `target` points at `bomb_b_mm` and B's at
  `bomb_a_mm` (map data; reproduce as is).
- Most hostage voice lines are defined but have no files, so hostages
  are nearly silent. Reproduce: play only the three entries with waves.
- The loose C4 can't be pushed with Use, though other dropped weapons can.

## Test cases

Ticks of 0.015 s. "(hyp.)" rows depend on an open question.

| Setup | Input | After | Expected |
|---|---|---|---|
| O1 de_dust2, 5 Ts, 5 CTs | round starts | – | exactly one living T has weapon_c4 in slot 4; no CT has one |
| O2 cs_office, any teams | round starts | – | nobody has a C4; 4 hostages at (1784, 734, −124), (1744, 802, −124), (2048, −344, −156), (1984, −344, −156) |
| O3 map without bomb targets or hostages (e.g. a test map) | round time runs out | – | draw/defenders win as now; no objective state |
| O4 dust2 A site trigger box (1072, 2336, 96)–(1248, 2624, 192) | T stands at (1160, 2480, 96.03) | – | in bomb target |
| O5 same box | T stands at (1000, 2480, 96.03) (hull x 984–1016) | – | not in bomb target; primary fire → "C4 must be planted at a bomb site.", no arming |
| O6 carrier in zone, on ground, C4 active, tick 0 | hold primary | 200 ticks (3.000 s) | planted bomb at his feet; C4 gone from inventory; `bomb_planted`; announcer `Event.BombPlanted`; explode_at = 3.0 + 45 = 48.0 s (tick 3200) |
| O7 same | hold primary, release at tick 190 | – | no bomb; `bomb_abortplant`; pressing again at tick 200 plants at tick 400 |
| O8 same | hold, at tick 100 walk out of the zone (if movement is possible) or jump | that tick | arming cancelled, "Arming sequence canceled. C4 can only be placed at a bomb target." (zone) |
| O9 arming view model | hold primary | 0.9 / 1.2333 / 2.2333 / 2.6 s | `c4.click`, screen "7" / "73" / "7355608" / "*******" |
| O10 carrier dies holding the C4 (knife active) | killed | – | weapon_c4 on the ground near the body; Ts told "<name> dropped the bomb." |
| O11 loose C4 | a CT walks over it | – | nothing |
| O12 loose C4 | a T walks over it | – | he has it in slot 4, active weapon unchanged; "You picked up the bomb." |
| O13 planted at t = 0, `mp_c4timer` 45, nobody defuses | – | t = 45.0 s (tick 3000) | explosion; T win "Target successfully bombed!"; every T +3500 (hyp.), every CT loss bonus |
| O14 planted at 0; CT with kit starts holding Use on it at 30.0 s | hold | 35.01 s (tick 2334, first tick ≥ 5 s) | defused, CT win, "The bomb has been defused.", every CT +3500, every T loss bonus + 800 (hyp.) |
| O15 as O14 without kit | start at 30.0 s | 40.005 s (tick 2667) | defused |
| O16 without kit, start at 36.0 s | hold | 45.0 s | explodes (defuse would end at 46.0) |
| O17 with kit, start at 30.0 | release Use at 34.0, press again at 34.5 | – | defuse at 39.51 s (full 5 s again) |
| O18 two CTs, CT1 defusing | CT2 uses the bomb | – | "The bomb is already being defused."; CT1 unaffected |
| O19 bomb planted, all Ts killed at 20 s | – | – | round continues; ends by defuse or explosion |
| O20 bomb planted, all CTs killed at 20 s | – | – | T win at once (hyp.) |
| O21 de_dust2, no plant, round time runs out | – | – | CT win "Target has been saved!", every CT +3250 (hyp.) |
| O22 de_dust2, all Ts killed before any plant | – | – | CT win "Counter-Terrorists Win!" (elimination) |
| O23 round time 0:00 reached 10 s after a plant | – | – | round continues until the bomb resolves |
| O24 H1 blast, default map (D 500, R 1750), unarmoured, clear line | player at d = 0 / 500 / 1000 / 1500 / 1750 in | – | 500 / 357.14 / 214.29 / 71.43 / 0 |
| O25 H2 blast, same | – | – | 500 / 346.28 / 115.03 / 18.33 / 0 (cut at R) |
| O26 H1 blast, de_nuke (D 400, R 1400) | d = 500 / 1000 / 1500 | – | 257.14 / 114.29 / 0 |
| O27 H1 blast, de_chateau (D 300, R 1050) | d = 250 / 500 / 1000 | – | 228.57 / 157.14 / 14.29 |
| O28 beep provisional rule, 45 s bomb | – | – | first interval 1.0 s, interval 0.55 s at 22.5 s remaining, 0.1 s at the end; ~115 beeps |
| O29 use search: CT eye (0, 0, 64) standing at origin, bomb on floor at (70, 0, 0), straight ray hits it at (70, 0, 1) | look at it | – | distance = 70 horizontal + 0 vertical (hit height inside 0–72) = 70 < 80 → found, though the eye is 94.2 in away |
| O30 use search, bomb at (90, 0, 0), looked at | – | – | 90 ≥ 80 → not found; "Use" deny sound |
| O31 use search sphere, bomb nearest point (40, 0, 0), view straight ahead (1, 0, 0) | Use | – | sphere: direction (40, 0, −64)/75.47 · view = 0.53 < 0.8 → not by sphere; the 45°-down probe may still hit it |
| O32 CT buys defuser with $800 in buy zone | `buy defuser` | – | $600, has kit, `defuser.equip`; buying again refused |
| O33 T | `buy defuser` | – | refused (CT only) |
| O34 CT with kit dies | – | – | kit on the ground; another CT without kit touching it gets it; a CT with a kit doesn't |
| O35 cs_italy, CT presses Use on an idle hostage | – | – | hostage follows him; `hostage_follows`; CT +150 once (hyp.) |
| O36 same CT presses Use again | – | – | hostage stays; `hostage_stops_following` |
| O37 T presses Use on a hostage | – | – | "Only Counter-Terrorists can move the hostages." |
| O38 following hostage enters cs_italy's rescue box (−896, −1712, −224)–(−432, −1504, −160) | – | – | rescued: removed, leader +1000 (hyp.), announcer `Event.HostageRescued` |
| O39 all 4 hostages rescued | – | – | CT win "All hostages have been rescued!" |
| O40 cs_ map, time runs out with hostages unrescued | – | – | T win "Hostages have not been rescued!" |
| O41 a player kills a hostage, `mp_hostagepenalty` 3, his 2nd kill | – | – | warning hint; 3rd kill → kicked |
| O42 hostage voice | any event | – | only Pain / StartFollowCT / StopFollowCT ever play |

## Open questions

Checks use the probe server (tools/css_probe: RCON, the SourceMod plugin's
per-tick runs on a bot via `mashup_wrun`, game-event logging, netprop
reads). Useful observable fields on a probe: the player's money, his
"has defuser" flag, the progress-bar duration and start time, the bomb's
defuse length/countdown and "blow" time, and the round's remaining
hostages (dump with `sm_dump_netprops` and read with `GetEntProp`).

1. **Q1 Answered (2026-10-16, probe server, dust2 B):** the plant completes
   200 ticks (3.0 s) after the attack press, the tick of `c4_plant.wav`
   (also Q14); with forward held the planter's speed stays 0 throughout;
   ducking still works while arming (duck pressed with attack: ducked 30
   ticks later, planted ducked); releasing attack before then aborts
   (no more clicks, the C4 stays in hand). Teleported out of the zone:
   not tried. Was: **Q1 Plant time and movement.** Give the probe bot a C4
   (`mashup_give weapon_c4`), place it inside dust2 A, `mashup_wrun` with
   primary held for 250 ticks: log the tick of `bomb_beginplant` and
   `bomb_planted` (expect 200 ticks). Repeat with forward held: does the bot
   move (velocity) while arming? With duck held? With a release at tick
   190 (expect `bomb_abortplant`)? Leaving the zone by being pushed
   (teleport): which message?
2. **Q2 Answered (2026-10-16, probe server):** `mp_c4timer` 45 (10–90),
   `mp_hostagepenalty` 13, `mp_roundtime` 2.5 (1–9), `mp_freezetime` 6
   (0–60), `mp_buytime` 1.5 (min 0.25), `mp_startmoney` 800 (800–16000),
   `mp_tkpunish` 0, `mp_limitteams` 2 (0–30), `mp_autoteambalance` 1
   (0–2: 2 asks volunteers), `mp_chattime` 10 (1–120), `mp_playerid` 0
   (0–2), `mp_fadetoblack` 0: all as ours. Not in ours: `mp_autokick` 1
   (kick idle and team-killing players) and `mp_spawnprotectiontime` 5
   (kick a player who team-kills within this many seconds of a round
   restart). Was: **Q2 Defaults and bounds.** RCON `cvarlist mp_c4`, `help mp_c4timer`,
   `help mp_hostagepenalty`, `help mp_roundtime`: defaults, min/max.
   Expect `mp_c4timer` 45 (10–90).
3. **Q3 Blast.** `mp_c4timer 10`, plant with the probe bot in an open part
   of a site, place `mashup_target`/`mashup_targetn` bots at 250, 500, 750,
   1000, 1500, 1800 in on a clear line, unarmoured, then armoured; log
   `player_hurt` (dmg_health, dmg_armor). Fit H1 (linear) vs H2 (bell).
   Repeat with a wall between bomb and target (does it block?), on
   de_nuke/de_chateau to confirm `bombradius` scaling, with
   `mp_friendlyfire` 0/1 for Ts. Also check screen shake (`shake` events
   are client-side: watch the client) and whether the planter is credited
   with kills in `player_death` (weapon "c4"?).
4. **Q4 Beep schedule.** Log the server time of every `bomb_beep` event
   for one 45 s bomb, then with `mp_c4timer 20` and 90. Does the interval
   depend on the fraction remaining or on seconds remaining? Record the
   first and last intervals and the count. Loudness: client `snd_show 1`
   or record audio at a fixed distance early vs late. Where on the model
   the LED glow sits (offset from the bomb origin): client screenshot or
   `ent_text`. Are `c4_exp_deb1/2.wav` played at the explosion?
5. **Q5 Money.** Read the account of every player before and after: a
   plant (planter), a defuse (defuser), each round-end reason, the +800
   T bonus after a defused bomb, a hostage first-use, a rescue, hostage
   damage (one shot of known damage) and a hostage kill. `mp_startmoney
   10000` so nobody hits 0 or 16000.
6. **Q6 Partly answered (2026-10-16, probe server, one T and one CT
   bot, `mp_ignore_round_win_conditions 0`):** after a plant, the last CT's
   death ends the round at once as a terrorist win by elimination (log
   `Terrorists_Win`, not `Target_Bombed`); the last terrorist's death
   doesn't: the round waits for the bomb (`Target_Bombed` when it went off
   10 s later). Both as ours (O19, O20). Who gets the bomb with humans
   present and the announcers on a defuse are open. Was: **Q6 Round edge rules.** Who gets the bomb: run 50 round restarts with
   2 human-like bots + 3 bots and count carriers; does a human get
   preference? After a plant: kill all CTs (`kill` via RCON) — does the
   round end at once? Defuse with the round already over? Which announcers
   play on a defuse (BombDefused then CTWin, or one)?
7. **Q7 Partly answered (2026-10-16, probe server, dust2 B, a CT bot
   40 units from the planted bomb holding use on it):** the defuser can't
   move (forward held: speed 0, no displacement over 250 ticks), can duck
   (ducked 30 ticks after the press, still defusing), and
   `weapons/c4/c4_disarm.wav` plays on the press tick; as ours. The
   planted bomb is **not solid**: a bot walking across it (two
   directions) passed within 3 units of its centre without stopping or
   stepping up, and a Desert Eagle shot aimed at its top struck the floor
   4 units beyond it; as ours. Riding moving platforms not measured. Was: **Q7 Planted bomb physics.** Is the planted bomb solid to players and
   bullets? Where exactly (origin vs player origin, angles)? Does it ride
   a moving platform? Dump its position and collision group after a plant.
   Can the defuser move or crouch while defusing (`mashup_wrun` with Use and
   forward held, looking at the bomb)?
8. **Q8 Answered (2026-10-16): see Constants, Hostages, "the rule" (hostage hitgroup multipliers, a second hit through the hostage).** Earlier: health 100; ten single shots from
   150 units and ours beside them under Constants, Hostages ("hostage
   damage, no rule found"): they don't follow the player multipliers
   and no single rule fits them; the re-measure needed is described
   there. Was: **Q8 Hostage health and damage.** Shoot a hostage once with a known
   weapon at a known hitgroup (`player_hurt` doesn't fire; use
   `hostage_hurt` and read its health netprop). Does armour/hitgroup
   apply?
9. **Q9 Answered (measured 2026-10-17, probe server, `mashup_hostages`
   printing each hostage_entity's `m_ModelName` and skin, several
   `mp_restartgame 1` rounds and map changes):** neither the map's model
   nor `HostageType` picks one. Each hostage spawned takes the next of
   `Hostage_01`, `02`, `hostage_03`, `04` in turn, in entity order, and the
   turn goes on over rounds and maps: cs_office's four got 01–04 every
   round (four a round), cs_compound's three got 04 01 02, then 03 04 01,
   02 03 04, 01 02 03 (its first round went on from where cs_office left
   it), and cs_office after it 03 04 01 02. The map's skin stays (cs_office
   1, cs_compound 0). Ours: `HostageModelCycle`, and the map's skin as
   `map::BodySkin` (every stock hostage and player model has one skin
   family, so skin 1 draws as skin 0, as in CS:S). Was: **Q9 Hostage model choice.** On cs_office (`models/hostage.mdl`, skin
   1) and cs_compound (`HostageType` 1): which of `hostage_01..04` does
   each hostage use (dump each `hostage_entity`'s model name) and does it
   change per round?
10. **Q10 Partly measured (2026-10-16, probe server, cs_office, a CT bot
    using hostage 121 from 60 units, then running or walking (the walk
    key) about 400 units away along a clear line; the hostage's origin
    logged every server frame with `mashup_htrack`):** the server moves a
    hostage only every 7 ticks (0.105 s; its origin steps, the client
    smooths it). Following, its step per update grows toward ≈ 26 units
    (≈ 250 u/s), each step closing about a fifth of the gap to that
    (6.1, 11.0, 14.9, 18.0, 20.5, 22.5, 24.1, 25.2, 26.0), the same whether
    the leader ran (250) or walked (130); near the leader each step is
    ≈ 0.8 of the last (26.0, 20.8, 16.7, 13.3, ...), and it stopped 40
    units from a runner and 77 from a walker. It started 0.1 s after the
    leader did. With the leader moved 1230 units away (straight line,
    another room, out of sight) the hostage still followed, found its way
    round and reached him in 7 s; the give-up distance and time are
    beyond that (ours: 1500 units or 10 s out of sight). Corners and
    jumps not measured. Was: **Q10 Follow behaviour.** With a probe CT leading a hostage, log the
    hostage's position and speed every tick while the CT walks away at
    250 in/s, stops, turns corners, jumps a crate, and teleports 2000 in
    away. Measure: stop distance, walk/run speeds and the threshold, the
    give-up rule (distance or time), and whether hostages block players or
    get pushed.
11. **Q11 Answered (measured 2026-10-17, probe server, cs_office, the
    probe bot made leader with `mashup_input <hostage> Use "" 1`, hostage
    and leader placed with `mashup_tp`, hostage and round events logged by
    mashup_ents):**
    - Only a following hostage on its way is rescued: one teleported idle
      into the middle of a zone stayed; one following but within its stop
      distance of the leader (70 units) stayed, even mid-zone; with the
      leader 300 units off it was rescued 9–12 ticks after it started
      following (its first moves).
    - Hull overlap, not the origin: the hostage's box is −13..13 × 0..72.
      Walking south into zone 428 (its face at y −1719), it was rescued at
      origin y −1710.3, −1709.8, −1712.5, −1718.3, −1710.5 (five runs), all
      outside the face, the box reaching 3–9 in; walking along the face
      with the origin at −1712 or −1700 it stayed (its path drifted away).
    - No rescue zones (both removed with `mashup_kill` mid-map, and a map
      load with them removed as they spawned, `mashup_norescue 1`; they
      aren't re-created by a round restart): hostages led straight over
      two CT spawns were never rescued. No radius: nobody is rescued.
    - Round end: 3 rescued and the 4th killed (killed first), and 2 and
      2: `hostage_rescued_all`, then the round ends at the last rescue, CT
      win (reason 10). All 4 killed: no round end.
    Ours now: rescue needs a leader and a walking hostage, a 26-wide box,
    no spawn fallback. Was: **Q11 Rescue rules.** Hostage at the rescue zone edge: hull overlap or
    origin inside? On a map with all rescue zones removed (`ent_remove`
    them, then `mp_restartgame 1`): do CT spawns rescue, and at what
    radius? Round end when 3 of 4 are rescued and the 4th is dead?
12. **Q12 HUD and radar.** On the client: does the bomb icon flash red only
    inside a bomb target? Which fill colour does the progress bar use?
    Is the round timer replaced by the scenario bomb icon after a plant,
    and does the hostage scenario icon show one glyph per remaining
    hostage? What does each team's radar show (carrier, loose bomb,
    planted bomb, hostages)?
13. **Q13 `info_bomb_target` / `info_hostage_rescue` radius.** Unused by
    stock maps; check with a custom map only if a community map needs it.
14. **Q14 Tick rounding.** Is a 3 s plant done on tick 200 or 201, a 5 s
    defuse on 333 or 334, and does a defuse finishing on the same tick as
    the explosion win? Log tick numbers of begin/complete events in Q1/Q3.
15. **Q15 Answered (2026-10-16, probe server, one bot alone on CT, killed
    with `bot_kill`):** the round ends at once as a draw (log `World
    triggered "Round_Draw"`, scores unchanged, then `Round_End`), and the
    server prints "Game will not start until both teams have players.";
    as ours. Was: **Q15 One side only.** (Added by an implementation session, not from
    the source.) With players on one team and nobody on the other (a lone
    player; mg_ maps with one team's spawns: mg_lego_course,
    mg_escape_prison_beta, mg_crazykart_v1_1 all T, mg_wipeout2 and
    mg_boatrace_scramble all CT), what happens when they are all dead:
    a draw and the next round at once, or nothing until the clock runs
    out? We end the round as a draw (our reading). Test: probe server
    alone on a team, `kill`, log the round end and its reason.
16. **Q16 The bar while planting.** (Added by an implementation session,
    not from the source.) A player remembers CS:S planting with no
    progress bar, only the keypad typing the code on the C4's screen;
    section 3 says the bar runs for 3 s, and `cl_c4progressbar`'s text
    names only defusing (and the bar's place, 300 of 480 down, would
    cover the view model's screen). We draw no bar while planting, with
    either value. Test: client screenshots 1.5 s into a plant and a
    defuse with `cl_c4progressbar` 1 and 0; is the planting bar there,
    and does the cvar hide it?

## Mapping to our engine

- **Rules layer (`rules::rounds`):** add the map type (bomb / hostage /
  neither) from the entity lump at load; extend `RoundEndReason` with
  `TargetBombed`, `BombDefused`, `TargetSaved`, `HostagesRescued`,
  `HostagesNotRescued`; make the time-out winner depend on the map type
  (CTs on bomb maps, Ts on hostage maps, today's defenders elsewhere);
  suspend the time-out and the "all Ts dead" rule while a bomb is
  planted; per-reason win bonuses and the +800 plant bonus in
  `RoundSettings`.
- **Weapon layer:** `weapon_c4` as a slot-4 weapon whose primary drives
  arming (not firing); can't be bought; only Ts may pick it up. The
  defusal kit is an equipment item like armour (`economy::buy` gets
  `defuser`, CT only). Drop rules from weapons.md.
- **A planted-bomb entity** (rules or weapon layer, outside `client/`):
  timer, beeps, defuse state, explosion via the existing radius-damage
  code (grenades.md) with the map's bomb radius.
- **Use:** a shared player use search (§5) is needed by both the bomb and
  hostages, and is also what buttons/doors use in CS:S; one function.
- **Hostages:** a simple NPC on the nav mesh (we have nav.md's mesh); the
  spawn list comes from `hostage_entity`; round restart re-creates them.
- **Map layer:** `func_bomb_target` and `func_hostage_rescue` brushes as
  trigger volumes (like `func_buyzone`, already loaded); `info_map_
  parameters` `bombradius` and `buying`; `BombExplode` outputs through the
  logic bridge.
- **Client:** HUD icons at the positions above, the progress bar, centre
  messages, announcer sounds through `map::RoundSounds`, the C4 view-model
  events (clicks) and its screen text.
