# Combat Arms: bots

Source basis: the server-side AI (character AI object, its brain, goal
manager and goals, human states and strategies, target tracking, sense
recorder and the global stimulus manager, AI volumes and their neighbour
links, the volume path finder and path follower, patrol nodes), the
single-play bot deathmatch mission manager, the client's bot roster and bot
chat, the shared bot-profile reader, plus the shipped data: the single-play
AI attribute file (brains, attribute templates, stimuli, per-weapon AI
data), the single-play AI goal file, the bot profile file, the game-room
file and the per-map game-mode files of the bot maps (BR client build).
Status: draft

Sections: Summary; Does the game have bots; Units and conventions;
Constants; Difficulty, profiles and loadouts; Behavior (1 think loop,
2 goals and states, 3 navigation, 4 patrol, 5 perception, 6 target
selection, 7 reaction and aim, 8 weapon use, 9 movement in a fight,
10 team behaviour and mode rules, 11 other AI in the game); Per-tick order;
Edge cases; Quirks; Test cases; Open questions.

---

## Summary

Combat Arms has real bots: computer-controlled soldiers in two single-play
modes, **Bot Deathmatch** (one human against 5–8 bots on the other team)
and **Bot Training** (the same with tutorial captions). They are the
LithTech "AI human" of the engine's single-player games, re-tuned per
difficulty from data. A bot runs on a fixed set of goals (patrol, chase,
attack, investigate, dodge grenades), moves only inside hand-placed
navigation boxes ("AI volumes"), sees through a field-of-view ray test,
hears shots and footsteps through radius tests, and shoots straight at a
computed aim point: there is no weapon spread, no recoil and no turn speed.
Difficulty lives in a **reaction delay** before the first shot, a
**hit/miss cycle** (for example exactly one aimed shot in six on Easy and
one in two on the hardest Veteran archetype), the **distance at which it
notices you while patrolling**, how long it keeps chasing you, and how many
missed shots it takes before it throws a grenade. Bots know where you are
while they chase you; that is the behaviour players feel most.

## Does the game have bots

Yes. Where computer-controlled characters appear:

| Mode (game-room key) | Who | How added |
|---|---|---|
| `BDM` Bot Deathmatch (single play) | bots that play like soldiers; the subject of this spec | Room menu: player count 5–8 (step 1) = number of bots; difficulty Easy / Normal / Veteran (default Normal); kill goal 20–50 (step 10, default 40) |
| `BTRN` Bot Training (single play) | the same bots, Bot Deathmatch rules plus scripted tutorial captions | Training menu; default goal 20, about 6 bots |
| `SCOP` single-play Fireteam, `COOP` Fireteam and its override maps | scripted PvE enemies placed in the map (soldiers, zombies, bosses) | Placed by the map and the co-op mission script; difficulty 0–2 is the room goal |
| Quarantine-like and raid modes, infected ship, cabin fever | zombie and boss NPCs, pattern-driven bosses | Map scripts |
| Any mode | auto turrets | Placed by players' items |

Only `BDM` and `BTRN` have bots in the sense of "a computer opponent that
plays the game like a player". The PvE enemies share the same AI engine but
use other data files, goal sets and mission scripts; they are out of scope
here (see section 11 and Open questions).

**How a Bot Deathmatch game starts.** The player creates a single-play room
and starts it; the client hosts a local server for itself. The room's
player count, difficulty and goal are written to the client's console
variables `MaxPlayer` (bot count), `BotLevel` (0 Easy, 1 Normal, 2 Veteran)
and the kill goal. At level start the client builds the bot roster (see
Difficulty, profiles and loadouts): **every bot goes on the team opposite
the human**, so the match is always one human against N bots. The client
sends the roster to its local server, which spawns each bot at a start
point of its team. Bot names, clans, ranks and nations are cosmetic profile
data shown on the scoreboard; bots also post canned chat lines.

The bot maps are the maps that ship a bot game-mode file: Costa Recon,
Death Room, Downhill, Junk Flea, Neptune, Oil Rig, Short Fuse, and a
training variant of Junk Flea for Bot Training. Each such file redirects
the AI attribute and goal files to their single-play variants and lists the
patrol start nodes (below).

## Units and conventions

- Distance: LithTech world units (u), as in the other Combat Arms specs.
  +Y is up; X/Z horizontal.
- Angles: radians unless stated. "dot" is the dot product of unit vectors.
- Time: seconds. Bots run on the **server**, once per server frame. The
  shipped dedicated-server configuration runs 20 frames per second
  (dt = 0.05 s, see `grenades.md`); a Bot Deathmatch server is local and
  its frame rate is not confirmed (Open question 1). Formulas use dt.
- Random numbers: U(a, b) is uniform real in [a, b]. Ri(a, b) is a uniform
  integer in [a, b] inclusive. r100 is a uniform integer 0–99.
- "Percent rolls" in this spec all have the form "fails if r100 > P", so a
  roll against P succeeds with probability (⌊P⌋ + 1)/100 for 0 ≤ P < 100,
  and always for P ≥ 99.
- Difficulty columns: the data names each bot template
  `<Level>_<Archetype>[_<Variant>]` with Level in Normal (shown as Easy),
  Hard (shown as Normal), Extreme (shown as Veteran), Tutorial (training)
  and Archetype N, H or E. These names are data keys and are used as such.
  Tables below use the short column labels NN, NH, NE (Easy), HN, HH, HE
  (Normal), XN, XH, XE (Veteran), TN, TH, TE (training).

## Constants

Global (all bots, all levels):

| Name (ours) | Value | Unit | Meaning |
|---|---|---|---|
| think interval request | 0.01 | s | Bots ask for an update every 0.01 s, i.e. every server frame |
| sense pass rate | 0.01 | s | Nominal sense interval; in practice one sense pass per server frame (see 5.1) |
| sight range | 3000 | u | `SeeEnemyDistance`; also the range of the "visible from weapon" ray |
| sight stimulus radius | 5000 | u | Added to the sight range in the coarse radius test; never the binding limit |
| no-FOV radius | 256 | u | Inside this, the horizontal field of view is 180° even when relaxed |
| relaxed FOV | 140° horizontal, 120° vertical | — | dot thresholds 0.342 (forward) and 0.5 (up) |
| alert FOV | 180° horizontal, no vertical limit | — | dot threshold 0 |
| instant-see distance | 5000 | u | Larger than the sight range, so every successful sight ray is instant |
| personal bubble | 128 | u | Seen even if hidden / outside FOV (gradually, ×0.25, when relaxed) |
| darkness multiplier | 0.5 | — | Only affects the (irrelevant) gradual case |
| footstep stimulus radius | 256 × surface noise | u | Plus the template's hearing distance 1500 |
| footstep vertical window | ±128 | u | |
| gunfire stimulus radius | weapon `FireSndRadius` (silenced: silenced radius) | u | Template adds 0 |
| gunfire vertical window | ±512 | u | |
| ally-call radius | 512 | u | A bot that starts investigating calls allies within this |
| sight cooldown (patrolling) | 1.5 | s | `SeeEnemyUpdateTime`, after every sight roll |
| camping radius | 120 | u | `CharCoverStateResetDist` |
| camping onset | 10 | s | `CoverCharCheckTime` |
| camping accuracy step | 0.1 per 10 s | — | `CoverCharAccuracyVal` |
| camping reaction step | 0.2 s per 10 s | s | `CoverCharShootStartTimeVal` |
| reaction re-arm gap | 5 | s | A new reaction delay is rolled only if the attack goal was off for more than this |
| aim offset scale | 100 | u per unit | Movement inaccuracy is multiplied by 100 |
| miss offset vertical | U(−5, 15) | u | |
| grenade hang time | 0.5 | s | Ballistic throw solution time |
| far-weapon switch factor | 0.8 | — | On squared range (distance > 0.894 × mid weapon range) |
| shuffle check | 0.7 / roll 0.3 | — | Dodge action split |
| roll distance | 140 | u | |
| back-up / flank width / flank pass | 50 / 50 / 50 | u | `DynMove*Dist` |
| dodge check interval | 0.25 | s | |
| dodge delay after a dodge | 3.0 | s | |
| "aiming at me" | dot > 0.95 both ways | — | Player faces the bot within ~18° and the bot faces the player |
| attack chase timer | 10.5 + U(−10.5, 5.0) | s | Time a target may stay hidden before the bot leaves the attack state |
| crouch chance on engaging | 0.01 | — | Per entry into the attack state |
| crouch hold | 10 | s | |
| grenade-dodge search radius factor | 1.1 | — | Dodge point at 1.1 × blast radius |
| respawn delay | 7 | s | After death |
| body lifetime | 5 | s | |
| spawn protection | 3 | s | `GodModeTime` |
| path search volume limit | 1024 | volumes | More volumes: no path finding at all |
| minimum link width | 48 | u | Shorter shared edges are not links |
| reserved-volume penalty | +500 | weight | When paths should diverge |
| patrol straight-ahead bonus | 10000 × dot | ms | See 4 |
| difficulty factor | 1 | — | The engine's global AI difficulty scaling is identity in bot modes (see Quirk 14) |

## Difficulty, profiles and loadouts

### Choosing the bots

The bot profile file has four lists of **profiles**, one per level
(Easy 22, Normal 25, Veteran 26, Tutorial 6). A profile has a nickname and
clan (string-table keys), a rank, a fake kill/death ratio, clan mark,
nation, a base model index, a chat cooldown (15, 20 or 25 s) and one or more
**item-set** indices. Each profile also points to a **bot base** (one of 36)
that names the bot's brain, attribute template and goal set (`BDM_Bot` for
all).

For each of the N bots, in order:

1. Draw ρ = (milliseconds since system start) + C-library rand().
2. Profile index i = ρ mod (profiles of the level). If i was already used
   in this roster, step i ← (i + 1) mod count until free. When all are
   used the used-set is cleared first.
3. Item set: one of the profile's item-set indices, chosen by rand().
4. Fill the slots in this order with the same ρ and a second variable
   σ = ρ: model, backpack, head, goggle, then weapon slots 0–5 (mid-range
   primary, long-range primary, secondary, melee, grenade, mine). For a
   slot with use-percent P and list L (non-empty):
   - if P ≥ 100 or P > (σ mod 100): item = L[ρ mod |L|] mod 1000;
   - then, if P < 100, σ ← ⌊σ / 10⌋ (so each partial slot uses the next
     decimal digit of the random number).
   Slots with P = 0 or an empty list stay empty.
5. Model and face follow from the profile's base model and the chosen
   model; a missing head gets the face's default hair piece.

Because every slot uses the same ρ, list picks are correlated. To break
this, at load time any slot list whose length equals another list's length
in the same item set is padded with copies of its own first entries until
all lengths differ. Note the goggle slot reads the item set's `Face_*`
keys (Quirk 9).

The bot's object name is `AI_Alpha_<profile>` or `AI_Bravo_<profile>`; its
bot ID is the profile number.

Weapon IDs are item numbers mod 1000 and refer to the weapons file
(`attribute_data.md`). Examples from the Easy sets: mid-range primaries
M4A1_CQBR, AK-103, aug_a1_Black …; long-range primaries L96A1_Ghillie,
SR25_Sandfire …; secondaries USP_Tactical …; melee Machete, GI_Shovel …;
grenades M67_Frag_Pack, Dynamite_HE, Dynamite_Pack, RGN_Grenade_CA; mine
M16A1_Mine. Typical use-percents: long-range primary 60 % (100 % for the
sniper and launcher variants), grenade 45 % on Easy, 75 % on Normal and
Veteran (100 % flash XM84 for `_Flash`, 100 % from a smoke/gas list for
`_Smoke`), mine 50 % (100 % with a second mine type for `_Mine`). The
`_Launch` variant has an RPG-7 or M79 (Easy) or XM_25 / MZP_1 as its long
primary; `_SG` and `_MG` variants have shotgun or machine-gun mid primaries.

### What a level changes

Each attribute template points to its own row of **per-weapon AI data**,
which overrides the template's own accuracy numbers as soon as the bot
draws a weapon. Values are per gun type: Default (used for SMG, assault
rifle, double barrel, crossbow and any listed type without its own keys),
MG, SG (shotgun), SR (sniper), Pistol, Knife, ThrowWeapon, Mine. Rocket
launchers have **no row** (Quirk 6).

Template and brain values (variants inside an archetype share the column
unless a variant is named):

| Value | NN | NH | NE | HN | HH | HE | XN | XH | XE | TN | TH | TE |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Reaction min (s) | 0.6 | 0.5 | 0.4 | 0.4 | 0.3 | 0.2 | 0.2 | 0.1 | 0.1 | 0.8 | 0.8 | 0.8 |
| Reaction max (s) | 0.9 | 0.8 | 0.7 | 0.7 | 0.6 | 0.5 | 0.5 | 0.4 | 0.3 | 1.2 | 1.2 | 1.2 |
| Run speed (u/s) | 270 (`_Flash` 300) | 270 | 270 | 285 | 285 | 285 | 300 | 300 | 300 | 250 | 250 | 250 |
| Jump speed (u/s) | 300 | 300 | 300 | 315 | 315 | 315 | 315 | 315 | 315 | 300 | 300 | 300 |
| Sight roll full-distance (u) | 700 | 700 | 700 | 700 | 700 | 700 | 700 | 700 | 700 | 700 | 700 | 700 |
| Sight roll far distance (u) | 2400 | 2600 | 2800 | 2800 | 3000 | 3200 | 3200 | 3400 | 3600 | 2200 | 2200 | 2200 |
| Sight roll far percent | 10 | 15 | 20 | 20 | 25 | 30 | 30 | 35 | 40 | 10 | 15 | 20 |
| Sight cooldown min distance (u) | 450 | 500 | 550 | 550 | 550 | 600 | 600 | 650 | 700 | 350 | 350 | 350 |
| Camping sight step (percent points per 10 s) | 0.1 | 0.1 | 0.1 | 0.25 | 0.25 | 0.25 | 0.5 | 0.5 | 0.5 | 0.1 | 0.1 | 0.1 |
| Shots before grenade (base) | 18 | 18 | 18 | 15 | 15 | 15 | 12 | 12 | 12 | 18 | 18 | 18 |
| … `_Flash` / `_Smoke` | 50 / 15 | | | 40 / 12 | | | 30 / 9 | | | 50 / 15 | | |
| Attack-move interval (s) | 0.8–1.8 | 0.7–1.6 | 0.6–1.4 | 0.6–1.4 | 0.5–1.2 | 0.4–1.0 | 0.4–1.0 | 0.3–0.9 | 0.2–0.8 | 0.8–1.8 | 0.7–1.6 | 0.6–1.4 |
| Shuffle distance (u) | 30 | 40 | 50 | 30 | 40 | 50 | 30 | 40 | 50 | 10 | 20 | 30 |
| Chase endurance (s) | 12 | 12 | 12 | 16 | 16 | 16 | 22 | 22 | 22 | 12 | 12 | 12 |
| Grenade-dodge delay (s) | 0.3–0.7 | 0.3–0.7 | 0.3–0.7 | 0.1–0.7 | 0.1–0.7 | 0.1–0.7 | 0.1–0.2 | 0.1–0.2 | 0.1–0.2 | 0.6–1.4 | 0.6–1.4 | 0.6–1.4 |
| Safe distance to shoot a placed device (× its blast radius) | 0.3 | 0.3 | 0.3 | 0.4 | 0.4 | 0.4 | 0.5 | 0.5 | 0.5 | 0.3 | 0.3 | 0.3 |
| Patrol mine interval (s) | 8–12 | 5–12 | 3–12 | 8–12 | 5–12 | 3–12 | 8–12 | 5–12 | 3–12 | 8–12 | 5–12 | 3–12 |

Same everywhere: walk 70, ladder 200, fall 500 u/s; hearing roll for
footsteps 100 % up to 100 u falling linearly to 60 % at 800 u; for gunfire
100 % up to 100 u falling to 50 % at 1000 u.

Per-weapon AI data (accuracy a, movement inaccuracy cap c_max, burst size,
pause between bursts, engagement range):

| Value | NN | NH | NE | HN | HH | HE | XN | XH | XE | TN | TH | TE |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Default a | 0.17 | 0.20 | 0.23 | 0.23 | 0.27 | 0.31 | 0.36 | 0.41 | 0.46 | 0.14 | 0.14 | 0.14 |
| Default c_max | 0.020 | 0.017 | 0.014 | 0.014 | 0.011 | 0.009 | 0.009 | 0.006 | 0.003 | 0.020 | 0.017 | 0.014 |
| Default burst (rounds) | 3–6 | 3–6 | 3–6 | 3–7 | 3–7 | 3–7 | 3–7 | 3–7 | 3–7 | 3–6 | 3–6 | 3–6 |
| Default pause (s) | 0.3–0.6 | 0.3–0.6 | 0.3–0.6 | 0.3–0.4 | 0.3–0.4 | 0.3–0.4 | 0.1–0.3 | 0.1–0.3 | 0.1–0.3 | 0.3–0.6 | 0.3–0.6 | 0.3–0.6 |
| Default range max (u) | 1000 | 1000 | 1000 | 1500 | 1500 | 1500 | 1500 | 1500 | 1500 | 1000 | 1000 | 1000 |
| MG a | 0.10 | 0.13 | 0.15 | 0.15 | 0.17 | 0.19 | 0.19 | 0.21 | 0.23 | 0.08 | 0.08 | 0.08 |
| MG c_max | 0.15 | 0.14 | 0.13 | 0.13 | 0.12 | 0.11 | 0.11 | 0.10 | 0.09 | 0.15 | 0.14 | 0.13 |
| MG pause (s) | 1.2–2.5 | 1.1–2.4 | 1.0–2.3 | 1.0–2.3 | 0.9–2.2 | 0.8–2.1 | 0.8–2.1 | 0.7–2.0 | 0.6–1.9 | 1.2–2.5 | 1.1–2.4 | 1.0–2.3 |
| MG range max (u) | 1000 | 1000 | 1000 | 1500 | 1500 | 1500 | 1500 | 1500 | 1500 | 1000 | 1000 | 1000 |
| SG a | 0.50 | 0.55 | 0.60 | 0.60 | 0.65 | 0.70 | 0.75 | 0.80 | 0.85 | 0.40 | 0.40 | 0.40 |
| SG burst (rounds) | 1 | 1 | 1 | 20–30 | 20–30 | 20–30 | 20–30 | 20–30 | 20–30 | 1 | 1 | 1 |
| SG pause (s) | 1.5–2.5 | 1.3–2.3 | 1.1–2.1 | 1.1–2.1 | 0.9–1.9 | 0.7–1.7 | 0.7–1.7 | 0.5–1.5 | 0.3–1.3 | 1.5–2.5 | 1.3–2.3 | 1.1–2.1 |
| SG range max (u) | 600 | 600 | 600 | 1500 | 1500 | 1500 | 1500 | 1500 | 1500 | 600 | 600 | 600 |
| SG ammo multiplier | 10 | 10 | 10 | 4 | 6 | 8 | 8 | 10 | 12 | 10 | 10 | 10 |
| SR a | 0.50 | 0.55 | 0.60 | 0.60 | 0.65 | 0.70 | 0.70 | 0.75 | 0.80 | 0.40 | 0.40 | 0.40 |
| SR c_max | 0.30 | 0.28 | 0.32 | 0.32 | 0.36 | 0.40 | 0.40 | 0.44 | 0.48 | 0.30 | 0.28 | 0.32 |
| SR pause (s) | 1.0–1.5 | 0.9–1.4 | 0.8–1.3 | 0.8–1.3 | 0.7–1.2 | 0.6–1.1 | 0.6–1.1 | 0.5–1.0 | 0.4–0.9 | 1.0–1.5 | 0.9–1.4 | 0.8–1.3 |

Same in every column:

| Gun type | Full-accuracy value | Miss offset (u) | c decay (per s) | Burst | Pause (s) | Range min–max (u) | Ammo × | Fire-frame (s) |
|---|---|---|---|---|---|---|---|---|
| Default | 100 | 100 | 0.03 | above | above | 0–above | 10 | 0.1 |
| MG | 100 | 100 | 0.03 | 20–30 | above | 0–above | 10 | 0.01 |
| SG | 400 | 100 | 0.03 | above | above | 0–above | above | 0.4 |
| SR | 50 | 100 | 0.03 | 1–2 | above | 1000–5000 | 10 | 0.5 |
| Pistol | 50 | 100 | 0.04 | 1 | 0.8–1.3 | 0–500 | 10 | 0.5 |
| Knife (a = 0.95, c_max 0.05) | 50 | 2 | 0.03 | — | — | 0–130 | 1 | 0.7 |
| ThrowWeapon | — | — | — | — | — | 300–1200 | 1 (`_Smoke` 3) | — |
| Mine | — | — | — | — | — | default range | 1 (`_Mine` 3) | — |

Pistol a = 0.8 and c_max = 0.10; SG c_max = 0.01; all burst minimums are 3
for the default type. "Full-accuracy value" is compared with the **squared**
distance (Quirk 5), so it is effectively a radius of √value u. "Ammo ×"
multiplies the ammo's normal maximum to give the bot's total ammunition.
"Fire-frame" is the firing interval used only while a hit reaction is
playing (8.3).

## Behavior

### 1. Think loop

A bot is updated every server frame (it requests 0.01 s). Separately, the
global stimulus manager runs once per server frame and gives each bot one
**sense pass** (5.1). While the round is ending ("pre-end") or the mission
manager freezes AI, bots do nothing at all (no movement, no senses act, no
fire) and turrets stop firing. A dead bot only runs ground and animation
upkeep. The order of one update is in Per-tick order.

### 2. Goals and states

A bot has a fixed list of **goals**, each with a base importance:

| Goal | Importance | Decay time (s) | Triggered by |
|---|---|---|---|
| Dodge grenade | 100 | — | a nearby enemy grenade (5.4) |
| Draw weapon | 50 | — | any enemy/ally sense (only matters when holstered) |
| Alarm, Attack-from-view, Cover | 32 / 27 / 18 | 15 / 10 / 10 | need alarm, view or cover nodes in the map (Open question 9) |
| Attack ranged (dynamic) | 23 | 2 | seeing the enemy, in range |
| Attack melee | 22 | 2 | seeing the enemy, in knife range, no usable gun |
| Chase | 20 | — | seeing the enemy |
| Distress, Flee | 17 / 16 | 10 | see Open question 10 |
| Investigate | 13 | 10 | hearing footsteps or gunfire, an ally's call |
| Holster | 10 | — | — |
| Work | 4 | 10 | work-item nodes (none in bot maps) |
| Patrol | 2 | — | always (the idle behaviour) |

When a sense fires (5.1), every goal that lists that sense gets the chance
to accept it; a goal that accepts sets its current importance to its base
importance and records the stimulus source. The goal with the highest
current importance is active; a goal can lock itself in place while it runs
an uninterruptible action. Each update the **active** goal's importance
falls by base / decay-time per second (no decay when the decay time is 0);
any other sense-triggered goal that is not active is reset to 0 instead. So
a goal stays active only while its trigger keeps firing.

Goals drive **states** (patrol, investigate, chase, attack, attack-move,
goto, idle, aware, look-at), and states drive **strategies** (follow path,
shoot, dodge, throw grenade, drop mine).

### 3. Navigation

**What bots move on.** Bots never move on the world geometry directly. A
level designer places **AI volumes**: axis-aligned boxes, one per walkable
area, stored in the map's world file as ordinary objects of the volume
classes. Volume types: plain, ladder, stairs, ledge, jump-up, jump-over,
junction, teleport, ambient-life (bots may use ladders; ambient-life
volumes are excluded for them). A volume can carry a path weight (default
1) and up to two doors (doors inside it, found at load).

**Links are built at level load.** Two volumes are neighbours when their
boxes overlap or touch on all three axes (inclusive). The link is the shared
rectangle of their top faces: its corners are the max/min of the two boxes'
X and Z extents. Its connection point is the rectangle's centre, at a
height chosen from the two boxes' vertical ranges (the touching face if one
box is on top of the other, otherwise the middle of the overlapping height
range). The link is cut into **gates** of 48 u; a link shorter than 48 u is
invalid and dropped. A volume keeps at most a fixed number of neighbours.
More than 1024 volumes in a level disables path finding entirely.

**Path search.** A path request names a destination position, node or
volume. The source is the bot's current volume (or its last one). The
search is Dijkstra over volumes:

```math
\text{cost}(V_{next}) = \text{cost}(V) + w(V)\,\lVert e(V) - c(V,V_{next})\rVert^2
```

where e(V) is the point at which the path entered V (the start position for
the source), c is the link's connection point and w is the volume's path
weight: its own weight when the bot is not alert, 1 when alert, plus 500
when the volume is currently reserved by another bot and the request asks
for diverging paths (bots set "diverge" when chasing an unseen target).
Costs are **squared distances** (Quirk 4). A neighbour is skipped if it is
disabled, if it has a door locked for this bot that is not open, or if the
volume types refuse the transition (ladder, jump and teleport volumes only
connect in their allowed directions). The search stops expanding nodes
costlier than the destination's best cost. No path: the request fails; the
result (path found / not found per destination volume) is cached per bot
and reused for later requests to the same volume.

**Path following.** The volume chain is turned into waypoints: the start,
a point through each link (snapped inside the gate area, with curve
smoothing through consecutive links), and the destination; special
instructions are inserted for doors (open and walk through), ladders
(climb at the ladder speed 200 u/s), jump-up and jump-over volumes (a
parabolic jump at the jump speed), and teleports. Each update the bot moves
toward the current waypoint at its speed:

```math
s = s_{base}\cdot\big(1 - \operatorname{clamp}(p_{gear} + p_{weapon} + p_{stun}, -1, 1)\big) - p_{slow},\quad s \leftarrow s \cdot f_{volume}
```

s_base is the walk, run (patrol, chase, investigate and attack moves all
run), ladder or fall speed of the template; p_gear is the sum of the worn
model, head, goggle and backpack move penalties plus the figure's weight
minus its speed bonus (clamped to ±1), p_weapon the held weapon's
`MovePenalty`, p_stun the stun penalty while stunned, p_slow the
knock-back slow of the last hit (`attribute_data.md`, ×(1 + figure stiff
duration) in time), and f_volume the product of the speed factors of the
volume brushes the bot stands in. A new position that falls outside every
enabled AI volume is refused: the bot stops and the waypoint counts as
reached. This is what keeps bots on the navigation boxes.

**Doors** are passed if not locked for the bot; a closed unlocked door is
opened by walking into it. **Drops** happen only where a jump-up/ledge
volume pair or overlapping boxes of different heights connect; a bot never
walks off an edge that is not linked. **Getting stuck**: bots step aside
from other characters on their path (dynamic obstacle avoidance); a bot
that ends up below its volume's floor is popped back to the floor height
and 1 u toward its last valid position; there is no other unstick logic
(Open question 11).

### 4. Patrol

At level start the mission manager reads, from the map's bot game-mode
file, up to 8 **patrol start nodes** per team (`PatrolNode_Alpha0..7`,
`PatrolNode_Bravo0..7`) and up to 8 **guard nodes** per team. Each spawned
bot gets the next patrol start node of its team, round robin. Patrol nodes
are linked into a graph (next / previous links placed in the map). A bot
runs (not walks) from node to node; at a node with a wait action it waits
3 + U(0, 3) s; at a look-around node it plays look-around (head sweeps,
cosmetic). Choosing the next node when a node has several links: among
links other than the node it came from, score each linked node by

```math
\text{score} = \max\big(0,\ t_{used} - 10000\cdot\max(0, \hat f\cdot\hat d)\big)
```

with t_used the millisecond clock when that node was last reached by any
bot (0 if never), f̂ the bot's forward direction and d̂ the link direction;
the lowest score wins, ties to the first link. Unvisited nodes therefore
always win, then the least recently visited, with up to a 10 s bonus for
going straight on. At the end of a chain it turns back.

**Guards**: the data supports a guard role for bots carrying a long-range
primary, chosen by `AIGuradSpawnInfo<n>` sections (bot count limit, minimum
and maximum guard count, selection percent), but every shipped map sets the
maximum to 0, so **no bot ever guards** (Quirk 10).

**Mines while patrolling**: a bot that carries a mine re-arms a timer
U(min, 12) s (min per archetype above) each time it enters patrol and each
time the timer fires; when it fires, the bot drops the mine forward from
its grenade hand, using one from its stock.

### 5. Perception

#### 5.1 Senses and stimuli

Things in the world post **stimuli** to a global list, sorted by alarm
level. The single-play data only posts four kinds (all other kinds are
switched off): **enemy visible** (every character, persistent), **gunfire**
(per shot, lasts 1 s), **footstep** (per footstep, lasts 0.5 s) and **ally
call** (posted by a bot that starts investigating, lasts 1 s). Placed
devices (land mines, claymores, first-aid kits, auto turrets, caltrops,
nerve mines) post "enemy visible" too, if their effect data allows it.

Each server frame every bot gets one sense pass:

1. Try the **nearest enemy-visible stimulus** first: among enemy-visible
   records that pass the coarse test below, take the one closest to the
   bot (straight-line distance) and run the sight test on it. If it
   succeeds the pass is done.
2. Otherwise walk the stimulus list in alarm order and take the first
   record that passes the coarse test and its specific test. At most one
   failed ray test is allowed per pass (extra ones carry over when the
   bot used fewer last time).
3. Then update every sense record (stimulation rise/decay, reaction
   timers, triggers).

**Coarse test** (all must hold): the stimulus type is one the bot's current
state listens to; if the bot is patrolling or investigating, beyond its
sight cooldown min distance, and that sense's cooldown is running, refuse;
the source is not the bot itself; alignment: same team → ally, other team
→ enemy (a placed device's team counts); the bot is inside the stimulus's
vertical window; and the distance is below (template sense distance +
stimulus radius).

**Stimulation.** A sense record rises by 0.01 × rate × m per accepted
stimulus and falls by 0.01 × rate′ per pass without one (rates from the
stimulus data; m from the sight test). It fires when it reaches the
threshold maximum, after the stimulus's reaction delay. The **reaction
delay timer also counts 0.01 per sense pass**, not elapsed time
(Quirk 2). Values:

| Stimulus | Rise (alert / relaxed) | Fall (alert / relaxed) | Threshold | Reaction delay | Ignore older |
|---|---|---|---|---|---|
| Enemy visible | 10 / 0.5 × m | 0.125 / 0.5 | 0.5–1.0, 3 false = full | 0 | yes |
| Gunfire | 100 (default) | 100 | 1–1 | U(0.1, 0.3) "s" | yes |
| Footstep | 50 / 50 | 0.125 / 0.25 | 0.5–1.0, 3 false = full | 0 | yes |
| Ally call | 100 | 100 | 1–1 | U(0.3, 1.0) "s" | yes |

"Ignore older" means a record is used only if it is newer than the last one
this sense accepted. A "false stimulation" is a rise past the threshold
minimum that decays back below it without reaching the maximum; after 3 of
them the sense is treated as fully stimulated.

#### 5.2 Sight

Sight test on a character or device at position T (sight range R = 3000):

1. If the target is dead: not seen. If it is a placed device with blast
   radius ρ and the bot is closer than ρ × (safe factor, table above): not
   seen (the bot will not shoot a mine it stands next to).
2. The ray start is the bot's eye node. The aim spot cycles over a small
   grid on the target's facing plane (one grid step per failed pass); for
   characters it is then replaced by exact node tests below.
3. Distance² ≥ R²: not seen.
4. FOV: alert bots see 180° horizontally with no vertical limit; relaxed
   bots 140° horizontally (dot with eye forward ≥ 0.342) and not more than
   60° up from horizontal (dot with up ≤ 0.5); inside 256 u always 180°.
5. If a straight walk exists through AI volumes from the eye to the spot
   (ignoring ladder/stairs/jump/teleport volumes), it is visible without a
   ray (only when the target stands inside a volume, not under it).
6. Ray from eye to target centre. Ignored by the ray: the bot itself,
   bodies, hitboxes, **all characters** (teammates and enemies do not block
   sight), liquids, see-through surfaces, and anything non-solid (so
   **smoke never blocks sight**, as `grenades.md` says). Hit nothing, or hit
   the target: seen.
7. Otherwise one more ray to the target's **head node**: if clear, seen,
   and the bot remembers that only the head was visible (7.4).

Seen inside the 5000 u instant-see distance (always, given R = 3000)
means m is huge: the sight sense fires at once. A hidden target (scripted
hiding, not used in bot maps) is only seen inside 128 u.

**Flash grenades do not blind bots** (`grenades.md`), and darkness does not
matter.

**Sight while patrolling (the "notice" roll).** While a bot is in its
patrol or investigate state, each time a sight test would succeed it first
restarts its sight cooldown (1.5 s) and rolls against

```math
P_{see}(d) = \begin{cases}100 & d < d_0\\[2pt] 100 - (d - d_0)\dfrac{100 - P_{far}}{d_1 - d_0} & d_0 \le d \le d_1\\[2pt] P_{far} & d > d_1\end{cases} \;+\; B_{camp}
```

with d the bot-to-stimulus distance, d₀ = 700, d₁ and P_far from the
difficulty table, and B_camp the camping bonus (5.5) in percent points. A
failed roll means "not seen this pass". Because the cooldown blocks the
coarse test only beyond the cooldown min distance (350–700 u), a patrolling
bot at d > that distance gets one roll per 1.5 s; closer than 700 u it
always passes. Footstep and gunfire hearing roll the same way (their own
curves, no cooldown). In every other state (attack, chase, attack-move,
dodge) there is no roll: sight is deterministic.

#### 5.3 Hearing

- **Gunfire**: any character's shot posts a stimulus at the muzzle with
  radius = the weapon's `FireSndRadius` (silenced weapons: the silenced
  radius, `attachments.md` Q6) and a ±512 u vertical window. Walls do not
  block hearing. One accepted shot fills the sense; it fires after the
  reaction delay (Quirk 2). Teammates' shots are "ally gunfire", which
  the single-play data ignores.
- **Footsteps**: every footstep of a character on the ground or a ladder
  posts a stimulus with radius 256 × the surface's movement-noise
  modifier; the bot hears it within 1500 + that radius and ±128 u
  vertically. Each heard step adds 0.5; between steps it decays by about
  0.0013–0.0025 per pass, so in practice the **third** heard step (or the
  third "false" one) fires the sense.
- **Grenade bounces** play a sound with a radius (`grenades.md`), but no
  hearing stimulus is posted from it; the danger path (5.4) is how bots
  react to grenades.

What a heard sound does: the investigate goal takes it (chase is only
started by sight). The bot calls allies (5.1 ally call, 512 u), then:
if the sound's spot is directly visible or reachable in a straight line, it
may just turn and look (only when not already strongly alarmed and not
relaxed in the last 30 s); otherwise it runs a path to the spot and searches.
A gunshot carries a high alarm level and in practice always sends the bot
running to it (Open question 12).

#### 5.4 Grenade danger

Every server frame in which a **player-thrown** grenade touched something
(a bounce or the frame it comes to rest), if its effect data allows it, the
server tells every bot of another team:

1. Ignore if this bot is already reacting to a different grenade, or the
   grenade moved less than 5 u since the last notice, or the bot is farther
   than (blast radius + 100) u, where blast radius = max(area radius,
   lingering-effect radius), or a ray from the grenade to the bot hits
   world geometry (invisible polygons ignored).
2. Otherwise remember the grenade, and arm the dodge goal for
   now + U(delay min, delay max) (difficulty table).
3. When that time passes, the dodge goal (importance 100) takes over:
   - if the bot is already outside the radius: it stays (keeps fighting);
   - else it tries 6 directions away from the grenade, starting straight
     away and stepping 45° (skipping the three directions toward the
     grenade): the first point at 1.1 × radius from the grenade with a
     straight volume path and no bot in the way;
   - else the nearest scripted dodge node with a path;
   - if it sees its target from the weapon it moves while shooting
     (flank animation toward the side of the point), otherwise it runs
     there. Then it returns to attack or idle. No point: it does nothing.

The engine's older "flee from a grenade within 200 u" check also runs
during the attack state every 0.25 s (3 s after a dodge), with the same
effect of moving away.

#### 5.5 Camping bonus

Each human player tracks how long they have stayed within 120 u of an
anchor point: the timer T grows by the frame time while within 120 u and
resets (with a new anchor) when they move farther or die. Against that
player:

```math
B(v) = \begin{cases}0 & T < 10\\ \dfrac{T}{10}\,v & T \ge 10\end{cases}
```

is added to the bot's accuracy (v = 0.1), subtracted from its reaction
maximum (v = 0.2 s), and added to the notice roll (v = sensitivity step,
in percent points, so negligible). It is unbounded: a player camping for
50 s gives +0.5 accuracy and −1.0 s reaction.

#### 5.6 Memory of a target

- **While attacking**: if the target stops being completely visible, a
  chase timer of 10.5 + U(−10.5, 5) s starts counting down (it restarts
  whenever the target is seen again). When it runs out — or when the
  target is beyond the bot's ranged reach (8.2), but not before 1 s after
  the attack began — the attack ends and chase takes over (if sight still
  triggers it).
- **Chasing**: the bot paths to the target's **true current position**
  (its last AI-volume position, which is updated as the target moves),
  whether or not it can see it, re-planning every time it reaches the end
  of a path, with a rotating offset of ±96 u on X or Z (cycle −X, +X, −Z,
  +Z, shared by all bots). It never searches for you — it knows. This lasts
  **chase endurance** seconds (12 / 16 / 22) from the start of the pursuit;
  when it expires and the target is not completely visible, the chase is
  dropped and the bot returns to patrol; if the target is visible at that
  moment, the bot stops for a "catch breath" animation and then pursues
  with a fresh endurance. Seeing the target for more than 1 s during the
  chase only refreshes the remembered position. At junction volumes it may
  stop and peek or search.
- **Patrol** forgets the target. Entering patrol or investigate also resets
  the grenade counter (8.5).

### 6. Target selection

In Bot Deathmatch every bot is on the other team from the only human, so
the enemy set is: the human, and the human's placed devices. There is no
scoring by threat, angle or health. The target is simply the **source of
the last sight that fired**, and the sense pass always tries the **nearest**
enemy first (5.1). Consequences:

- A placed mine nearer than the player (but beyond the safe distance) is
  tested first; if visible it becomes the target and the bot shoots it.
- Targets change only when sight fires on another source. A dead target is
  dropped at once; observers are never targeted.
- Aiming at a placed device is always exact (no hit/miss cycle).

### 7. Reaction and aim

#### 7.1 Facing

There is no turn rate in combat. In the attack and attack-move states the
bot's body yaw is set straight to the target's direction every update, and
its look pitch to atan(−Δy / |Δ|). Outside combat, look-around and head
tracking are cosmetic.

#### 7.2 Reaction delay

When the ranged attack goal activates with a target, and it has been off
for more than 5 s:

```math
t_{max}' = \max\big(0,\ t_{max} - B(0.2)\big),\qquad
t_{fire} = t_{now} + \begin{cases}U(t_{min}, t_{max}') & t_{min} < t_{max}'\\ t_{max}' & \text{otherwise}\end{cases}
```

Until t_fire, and whenever the target is not visible from the weapon, the
bot holds the aim pose and does not fire. If the goal was off for 5 s or
less, the previous t_fire (already past) is kept: **no new delay**
(Quirk 3). The first shot then waits for the first "fire" key of the fire
animation (8.3).

The reaction delay of the senses (5.1) comes before this: sight has none,
a gunshot up to 30 sense passes.

#### 7.3 Aim point and the hit/miss cycle

Each shot aims at a point S:

1. Base point: the target's **torso node** position (the head node instead
   if only the head was visible in the last visibility test, 5.2).
2. If the squared distance to the target is below the weapon's
   full-accuracy value (Quirk 5: effectively 7–20 u), or the target is not a
   character: S is the base point. Done.
3. Accuracy

```math
a = \operatorname{clamp}\big((a_w + B(0.1))\cdot k_{head},\ 0,\ 1\big),\qquad k_{head} = \begin{cases}0.5 & \text{head only}\\ 1 & \text{otherwise}\end{cases}
```

   with a_w the per-weapon accuracy (table).
4. Cycle lengths: if a ≤ 0: 1 miss, 0 hits; a ≥ 1: 0 misses, 1 hit;
   a < 0.5: h = 1, m = ⌊(1 − a)/a + 0.5⌋; else m = 1,
   h = ⌊a/(1 − a) + 0.5⌋.
5. Counters i_h, i_m (per bot, kept across targets): if i_h ≥ h and
   i_m ≥ m, reset both to 0. Want-miss = (r100 > 100a) and i_m < m. If
   i_h < h and not want-miss: **hit**, i_h += 1. Else **miss**, i_m += 1.
   Every cycle of h + m shots therefore has exactly h aimed shots; their
   order is random.
6. Hit: S = base + 100 · (U(−c, c), U(−c/2, c/2), U(−c, c)) with c the
   current movement inaccuracy (7.5), when c > 0.
7. Miss: let D be the unit direction from the bot's origin to the base
   point and R = D × (0, 1, 0) (horizontal, length cos of the elevation,
   not renormalised). S = base ± 100·R (sign 50/50), then S.y is raised by
   U(−5, 15). A miss lands about 100 u to the side whatever the distance.

The shot leaves from the weapon socket toward S. **No weapon spread, no
recoil, no movement or stance penalty** apply to bot shots; one ray per
shot, shotguns included (Quirk 7). The ray, impact and damage rules are
the player rules of `weapon_firing.md` (AI shooters never get the head /
limb zone factor there).

Resulting hit cycles (ignoring camping and head-only):

| Default a | 0.14 | 0.17 | 0.20 | 0.23 | 0.27 | 0.31 | 0.36 | 0.41 | 0.46 |
|---|---|---|---|---|---|---|---|---|---|
| aimed shots per cycle | 1 of 7 | 1 of 6 | 1 of 5 | 1 of 4 | 1 of 4 | 1 of 3 | 1 of 3 | 1 of 2 | 1 of 2 |

Pistol (0.8): 4 of 5. Knife (0.95): 19 of 20. Shotgun 0.5–0.85: 1 of 2 up
to 6 of 7. Sniper 0.5–0.8: 1 of 2 up to 4 of 5. MG 0.08–0.23: 1 of 13 up
to 1 of 4.

#### 7.4 Head-only visibility

The head flag is cleared at the start of **every** visibility test the bot
makes (on any object) and set only when the centre ray failed and the head
ray succeeded. So behind waist-high cover a bot aims at your head with half
accuracy (Quirk 8).

#### 7.5 Movement inaccuracy (error that shrinks while tracking)

On every target visibility update (once per sense pass) with time step Δt
since the last one:

```math
\hat p = p_{prev} + v_{prev}\Delta t,\quad
c \leftarrow \max\big(\min(\lVert p - \hat p\rVert,\ c_{max}),\ c - k\,\Delta t\big),\quad
v_{prev} = \frac{p - p_{prev}}{\Delta t}
```

with p the target's position, c_max and k the weapon's cap and decay (table).
Selecting a new target sets c = c_max. A target moving in a straight line
at constant speed lets c decay at k per second (0.03/s, so it takes
c_max / 0.03 s to reach 0); any change of speed or direction larger than
c_max units per update snaps c back to c_max. In world terms the hit offset
is at most ±100·c_max horizontally: 2 u for an Easy rifle bot, 0.3 u for the
hardest, but 28–48 u for snipers (bigger on harder levels, Quirk 12).

### 8. Weapon use

#### 8.1 Inventory

Up to six weapons in fixed slots (mid primary, long primary, secondary,
melee, grenade, mine), each with total ammunition = ammo maximum × the
gun type's ammo multiplier. Bots use real ammo ("limited ammo"), drop
nothing on death and have no inventory switching rules from
`inventory.md`.

#### 8.2 Which weapon

The bot's ranged weapon is (re)chosen when it starts attacking, when the
target leaves the current weapon's [range min, range max], and when the
clip is empty with no reserve:

1. Start with "none".
2. Mid primary if it has ammo.
3. Long primary if it has ammo and (no mid primary chosen, or the target
   is farther than √0.8 × the mid weapon's range max).
4. If still none: the secondary if it has ammo.
5. Nothing found: the melee weapon.

The bot's ranged reach (when it may start or keep a ranged attack) is the
largest range max among its ranged weapons that had ammo **when it
spawned** (it is not recomputed later) — 5000 u with a sniper rifle,
1000 or 1500 u with a rifle; but it never sees beyond 3000 u. Out of reach
it chases (5.6) and fires on the way when in reach and visible. Inside
reach but beyond the **held** weapon's range max, the bot's next attack
move starts at once and is aimed at 0.8 × that range (9.1), so it closes
in while shooting.

#### 8.3 Firing cadence, bursts, reloads

- Shots are triggered by the "fire" keys of the bot's third-person fire
  animation (three fire variants, re-picked at random every 10 s). While a
  hit reaction plays, a shot is instead forced every fire-frame seconds
  (first one after half of it). The server still refuses shots closer than
  the weapon's `FireDelay` ms and **spends the round** (the same server
  check as for players, `weapon_firing.md`).
- **Bursts** (all ranged weapons except thrown): a burst of
  Ri(burst min, burst max) rounds, counted by the clip going down; then the
  fire animation stops for U(pause min, pause max) s; then a new burst. A
  weapon switch or a reload starts a new burst. Shooting at a placed device
  ignores bursts.
- **Reload** when the clip is empty and reserve remains: the clip is
  refilled at once, then the reload animation plays (if the weapon
  animates reloads) during which the bot does not fire. Starting a reload
  triggers an immediate attack move (9.1).
- Empty clip, no reserve: switch weapon (8.2).

#### 8.4 Knife

Used only when no ranged weapon can be used (all out of ammo, or none in
range is held) and the target is within 130 u. Each fire key of the melee
animation deals the knife ammo's instant damage directly to the target
(melee damage type), with no swing trace. Players' knife reach is 105 u
(`inventory.md`), the bot's 130 u.

#### 8.5 Grenades

Each bot has a counter G set to "shots before grenade" (table). It is
reset to that value when the bot gets a new target, when it enters patrol
or investigate, and **whenever it damages a player**; it drops by 1 for
every shot actually fired. While attacking a completely visible target, if
G = 0, the bot holds a thrown weapon with ammo, and the target is within
the bot's thrown reach (the largest range max of its thrown weapons at
spawn: 1200, or 1500 on Normal/Veteran if it also carries a mine,
Quirk 11), it throws
instead of shooting and resets G. So a grenade comes after G consecutive
shots that did not hurt you.

The throw: the grenade leaves the hand socket, aimed at the target's
object position T from fire position F with the hang-time solution

```math
\vec v = \frac{\vec T - \vec F - \tfrac12\,\vec g\,t_h^2}{t_h}\cdot k_{power},\qquad t_h = 0.5\ \text{s},\ \vec g = (0, -1600, 0)
```

so with k_power = 1, v = 2(T − F) + (0, 400, 0). The direction is then
rotated about the vertical axis by the throw angle carried by the animation
key, and k_power also comes from the key (defaults 0° and 1). The grenade
then flies by the grenade rules (`grenades.md`), whose own integration does
not match this solution exactly. Two throw animations are used: one when
the target is closer than 1000 u, one otherwise.

Flash and smoke bots use the same rule with their grenade (counts 50/40/30
and 15/12/9).

#### 8.6 Mines

Dropped while patrolling only (section 4). They are aimed along the bot's
eye forward from the hand socket.

#### 8.7 Shooting the player's devices

See 5.2 and 6: placed devices are targets like the player, aimed at
exactly, not seen when within the safe distance, and never the target of
grenades.

### 9. Movement in a fight

#### 9.1 Attack moves (strafing)

While attacking, every U(attack-move interval) s (and at once after a
reload starts or when the bot is hit), if the bot is not animation-locked
or jumping, it picks one of seven moves at random — shuffle left, back up,
shuffle right, back up, flank left, back up, flank right (so back-up is
3/7) — and tries them in order from there until one is possible:

- shuffle: to position ± right × (shuffle distance + radius);
- back up: to position − forward × (50 + radius); tried once per pick;
- flank: to position ± right × 50, then ± forward × 50 (50/50: back, or
  forward by min(d − range min, 50) when the weapon has a range min);
- if the target is beyond the weapon's range max: the destination is moved
  to 0.8 × range max from the target along the same line; a flank that
  would end inside range min is pushed out to range min.

A move is possible when a straight path through AI volumes exists (ladder,
stairs, jump and teleport volumes excluded) and no other bot is on the
line. Moves run (back-up runs backwards, turning around when the move
points within 20° of the target direction) and the bot keeps firing while
moving ("elite" movement is on for all bot brains). No possible move: it
stands and shoots.

#### 9.2 Dodging fire

When hit, or every 0.25 s when the player is "aiming at it" (5.4 table:
mutual facing within ~18°), the bot may dodge: 70 % shuffle, 30 % roll,
140 u for a roll, shuffle distance for a shuffle, random side, falling back
to the other side, then the other action. No room: no dodge. Starting a
dodge pushes the next check back by 3 s.

#### 9.3 Crouching

On entering the attack state, with probability 0.01 a bot crouches (if no
other bot is crouch-attacking the same target) for up to 10 s, then
stands. A bot whose line of fire is blocked by a teammate bot asks that
bot to crouch (if more than 48 u apart); otherwise it dodges sideways.

#### 9.4 Cover and retreat

There is **no retreat at low health** and no health-based behaviour at
all. Cover, alarm and attack-from-view behaviours exist but only use
map nodes of those kinds (Open question 9).

### 10. Team behaviour and mode rules

- **Teams**: all bots on the team opposite the human. Friendly fire is off
  in the room. Bots coordinate only through the ally call (5.1),
  diverging paths (3), and the crouch/dodge requests (9.3).
- **Spawning**: each bot spawns at a start point of its team (start-point
  selection as for players), with full health and armour from its model
  (the `MaxHP`/`MaxAmmor` keys of the bot base are never read), and is
  invulnerable for 3 s.
- **Death**: the body stays 5 s; 7 s after death the bot respawns with the
  same profile and a new patrol start node. All bots respawn when a new
  round starts.
- **Score**: Team Deathmatch rules: every death of the human counts a kill
  for the bot team (falls and suicides included); a bot's death counts for
  the human's team **only when the human killed it** (a bot killed by a
  fall, its own grenade or another bot scores nothing). The first team to
  the kill goal wins the round. Bot kills and deaths are kept in the roster
  for the scoreboard.
- **Chat**: on events (kill, death, respawn, battle start, patrol start,
  grenade dodge, mine sighted, and two unused ones) the server tells the
  client; if the bot's chat cooldown (15/20/25 s) has passed, it restarts
  the cooldown and shows a random line of that bot's chat set with
  probability (P + 1)/100 (P per event, e.g. kill 60, battle 25).
- **Bot Training** adds captions triggered by deaths (count, chain,
  body part, damage type) on top of the same rules.

### 11. Other AI in the game (out of scope)

PvE enemies in the Fireteam family, infected-ship, cabin-fever and raid
modes use the same engine with other data: other brains and templates,
skill lists for bosses (rush, strafe, stomp, multi-shot, throws), "pattern"
bosses, zombie movement that cuts straight at the target, and the engine's
global difficulty scaling (factor 1.0 / 1.2 / 1.5 for difficulty 0/1/2,
+0.3·log₂(players) above one player). They deserve their own spec.

## Per-tick order

One server frame:

1. Stimulus manager: drop expired stimuli; move tracked ones with their
   source; give each bot its sense pass (5.1), which may fire senses into
   the goals (Open question 2 on the order relative to step 2).
2. For each bot, its update:
   1. If AI is frozen: stop.
   2. If alive: grenade-danger timer check (5.4).
   3. Count down the sight cooldown by the frame time.
   4. Move along the current path (3).
   5. Goals: decay, attractor checks, choose and run the active goal (2).
   6. Apply any pending state change.
   7. Update the accuracy modifier timer.
   8. Update position, volume, rotation.
   9. Start spawn protection on the first update with a navigation mesh.
   10. If alive: target update (drop dead target, face it, visibility from
       the weapon, movement inaccuracy, head flag); then the state's
       pre-update, update and post-update (shoot, dodge, grenade, path
       strategies; firing happens when animation keys arrive).
   11. Ground check, second pending-state check, effects.
   12. Look-around and aim pose; volume-brush speed factor; mark whether the
       notice rolls apply (patrol or investigate) and reset the grenade
       counter on entering them.
3. Animation keys of this frame (fire, throw) are handled as they play.

## Edge cases

- **No path**: chase, investigate and patrol fail their state; chase falls
  back to "aware" (stands looking), investigate drops to 0 and the bot
  resumes patrol; the failure is cached per destination volume.
- **Target or bot outside all volumes**: a bot not in a volume cannot path;
  a target standing under a volume (or outside all) skips the volume-path
  shortcut in sight tests and needs a clear ray.
- **Target behind smoke**: seen and shot as if no smoke (5.2).
- **Teammate in the line of fire**: the "visible from weapon" ray uses a
  filter that ignores only bodies, hitboxes and liquids, so a teammate's
  solid character blocks it: the bot does not fire, asks the teammate to
  crouch, or dodges (9.3). Sight itself ignores characters, so the bot
  still "sees" you and keeps attacking.
- **Target visible but not from the gun** (eye clear, gun side blocked):
  the bot aims and waits; the weapon ray starts at the bot's side ± radius
  at half its height.
- **Head only**: half accuracy, aim at the head (7.4).
- **Shot faster than fire delay**: round spent, no shot (8.3).
- **Burst count with a shotgun on Normal/Veteran**: 20–30 rounds per burst
  means it fires until the clip is empty.
- **Rocket launcher held**: keeps the accuracy, bursts and ranges of the
  previously held weapon (Quirk 6).
- **Placed device too close**: not seen, so not shot; the bot ignores it.
- **Player observer / dead**: never a target; a target that dies is
  dropped the same update.
- **Round end**: bots freeze in place mid-action.
- **Two grenades**: a bot reacts only to the first until it is gone.
- **First 5 s of server time**: the reaction-delay re-arm test compares
  with a last-deactivation time of 0, so an attack that starts within 5 s
  of the server clock's start gets no reaction delay.
- **Weapons running dry**: the ranged and thrown reach keep their spawn
  values (8.2); a bot with an empty sniper still engages out to 5000 u
  reach, limited by its 3000 u sight.

## Quirks

1. **Bots know where you are while chasing.** Chase paths to the target's
   real position for the whole endurance time. Keep: this is the bot
   feel.
2. **Reaction timers count sense passes.** The 0.1–0.3 "s" gunshot delay is
   10–30 sense passes, i.e. 0.5–1.5 s at 20 Hz. Keep (depends on the
   server rate, Open question 1).
3. **No reaction delay within 5 s.** Re-engaging soon after losing you
   fires immediately. Keep.
4. **Squared-distance path costs** prefer chains of short hops and change
   path choices compared with plain distance. Keep for identical routes.
5. **Full-accuracy radius used unsquared**: the per-weapon value is compared
   with the squared distance, so the "perfect aim" zone is √value u
   (10 u for rifles, 20 u shotguns, ~7 u pistols). Keep.
6. **Rocket launchers have no AI weapon row**, so a launcher keeps the
   previous weapon's numbers and does not count toward ranged reach.
   Keep.
7. **One ray per shotgun shot.** Bot shotguns fire a single ray (the
   multi-ray loop is fixed at one). Keep; it makes bot shotguns weak.
8. **Head flag cleared by any visibility test**, including tests on other
   objects; behind low cover bots aim at the head at half accuracy. Keep.
9. **Goggle slot reads the face list** of the item set. Keep (cosmetic).
10. **Guard role disabled by data**, and the code gives the Bravo team's
    guard nodes a patrol command instead of a guard command. Keep (dead).
11. **Mine widens grenade reach**: mines count as thrown weapons with the
    default range max, so a Normal/Veteran bot carrying a mine throws
    grenades from up to 1500 u instead of 1200. Keep.
12. **Sniper aim error grows with difficulty** (c_max 0.28 → 0.48), the
    reverse of every other weapon. Keep.
13. **"Accuracy increase time/value" keys are never read** (the reader looks
    for differently spelt keys), so the templates' 15/10/6 s values do
    nothing. Omit.
14. **Engine difficulty scaling is identity** in bot modes (the server's
    difficulty is never set there), so all "raised/lowered by difficulty"
    numbers use factor 1. Keep.
15. **Camping bonus is unbounded** (5.5). Keep.
16. **Template accuracy values (0.715 etc.) are dead**: the first weapon
    draw replaces them. Omit.

## Test cases

Common setup unless stated: Bot Deathmatch, one bot, server dt = 0.05 s,
flat open floor covered by one AI volume, player standing still (camping
timer T = 0 unless stated), bot holding its mid primary (default gun-type
row), random sources replaced by the stated draws.

1. **Roster size and team.** Room player count 6, human on Alpha → 6 bots,
   all Bravo, 6 distinct profiles of the chosen level.
2. **Profile probing.** Easy level, ρ = 45, profiles 1 already used →
   index 45 mod 22 = 1 is taken, next free 2 is used.
3. **Partial slot digits.** Item set with grenade use-percent 45 as the
   first partial slot, σ = 1234: σ mod 100 = 34 < 45 → grenade given;
   σ becomes 123 for the next partial slot.
4. **List length padding.** An item set whose mid and secondary lists both
   have 7 entries → after load the second one has 8 (its first entry
   repeated).
5. **Reaction, Easy.** NN bot, target appears in view at 800 u, attack
   goal never active before, draw 0.5 → first fire allowed at
   0.6 + 0.5·0.3 = 0.75 s after the goal activates.
6. **Reaction, Veteran.** XE bot, draw 0 → 0.1 s; draw 1 → 0.3 s.
7. **Reaction with camping.** NN bot, player camping T = 20 s →
   t_max′ = 0.9 − 2·0.2 = 0.5 < 0.6 → delay exactly 0.5 s.
8. **Reaction floor.** XE bot, T = 20 s → t_max′ = max(0, 0.3 − 0.4) = 0;
   t_min 0.1 > 0 → delay 0.
9. **No re-arm.** Goal ends at t = 10 s, re-activates at t = 13 s → no new
   delay (fires at once); re-activates at t = 15.1 s → new delay rolled.
10. **Hit cycle, Easy rifle.** a = 0.17 → m = 5, h = 1: of any 6
    consecutive shots (from a reset) exactly 1 aims at the torso, 5 land
    100 u to the side.
11. **Hit cycle, Veteran E rifle.** a = 0.46 → h = 1, m = 1: shots
    alternate in random order, 1 aimed per pair.
12. **Hit cycle with draws.** a = 0.17, counters 0/0, r100 draws 10, 90:
    shot 1: 10 > 17 is false → hit (i_h = 1); shot 2: hits exhausted →
    miss.
13. **Pistol.** a = 0.8 → h = 4, m = 1: 4 aimed of 5.
14. **Head only.** XE rifle, player's centre ray blocked, head ray clear →
    a = 0.23 → 1 aimed of 4, aimed at the head node.
15. **Camping accuracy.** NN rifle, T = 30 s → a = 0.17 + 0.3 = 0.47 →
    1 of 2.
16. **Miss geometry.** Bot origin at (0, 0, 0), target torso at
    (0, 50, 1000), vertical draw 15 → D = (0, 0.04994, 0.99875),
    D × up = (−0.99875, 0, 0), aim point (∓99.9, 65, 1000) depending on the
    50/50 sign; the same 99.9 u sideways at 100 u or 3000 u.
17. **Full-accuracy zone.** Rifle, target at 9 u → every shot exact; at
    11 u → hit/miss cycle applies.
18. **Movement inaccuracy decay.** Sniper XE (c_max 0.48, k 0.03), target
    walking a straight line at constant speed after selection, 0.05 s per
    update → c falls 0.0015 per update, reaching 0 after 16 s; a strafe
    reversal snaps it back to 0.48 (aimed offsets up to ±48 u
    horizontally, ±24 u vertically).
19. **Notice roll, Easy.** NN bot patrolling, player at 1550 u in FOV with
    clear ray → P = 100 − 850·90/1700 = 55 → success on r100 ≤ 55
    (56 %); next roll no sooner than 1.5 s later.
20. **Notice roll, Veteran.** XE, player at 3000 u → P = 100 − 2300·60/2900
    ≈ 52.4 → 53 %.
21. **Notice roll, near.** Any level, player at 650 u → always seen; at
    3001 u → never (beyond sight range).
22. **FOV.** Relaxed bot facing +Z, player at 1000 u at 75° off its facing,
    same height → not seen (dot 0.259 < 0.342); same player at 200 u →
    seen (inside 256 u, 180°). Alert bot, player at 100° → not seen;
    at 89° → seen.
23. **Smoke.** Smoke cloud between bot and player at 800 u → seen and
    engaged as without smoke.
24. **Teammate in line.** Bot B2 standing on the line from bot B1's gun to
    the player → B1 does not fire; B1 asks B2 to crouch if they are more
    than 48 u apart.
25. **Hearing a shot.** Patrolling bot 900 u from a player firing an
    M4A1_CQBR (radius 2000), roll passes, gunshot delay draw 0.2 → the
    sense fills on the first pass and the investigate goal fires about 20
    sense passes later (≈ 1.0 s at 20 Hz), and the bot runs toward the shot
    position.
26. **Footsteps.** Relaxed bot, player walking past at 300 u, all rolls
    pass, one footstep every 0.4 s → the footstep sense fires on the third
    heard step.
27. **Chase knows.** Bot loses sight, player runs to another room through a
    door; chase endurance 12 s (Easy) → the bot's path ends at the
    player's current position (± 96 u offset) and is re-planned to their new
    position when reached; 12 s after the pursuit began, with the player
    still out of sight, it gives up and patrols.
28. **Attack chase timer.** Player ducks out of view during an attack,
    draw −10.5 → the attack state ends at once (timer 0); draw 5 → after
    15.5 s.
29. **Weapon choice.** Bot with mid AR (range 1000, Easy) and sniper:
    target at 850 u → AR; at 900 u (> 894.4) → sniper; sniper's range is
    1000–5000, so at 900 u it is "out of range" and the choice is
    re-evaluated each update but stays the sniper.
30. **Ranged reach is fixed at spawn.** Easy bot spawned with an AR
    (range 1000) and a pistol; the AR runs completely dry → it switches to
    the pistol (range 500) but its reach stays 1000: target at 800 u → it
    stays in the attack, and its attack moves aim at 400 u from the target
    (0.8 × 500) until it is within 500 u.
31. **Grenade after misses.** HN bot (G = 15), M67 in stock, player
    completely visible at 1000 u, 15 shots fired without damage → the 16th
    action is a throw; one of those shots damaging the player resets the
    count to 15.
32. **Grenade reach.** XN bot with a mine and a grenade, G = 0, player at
    1400 u → throws (reach 1500); without the mine → does not.
33. **Throw solution.** F = (0, 60, 0), T = (0, 60, 800), key angle 0,
    power 1 → initial velocity (0, 400, 1600), speed 1649.2 u/s.
34. **Grenade dodge.** Player's M67 (blast radius r) comes to rest 100 u
    from an HN bot with a clear line, delay draw 0.4 → at 0.4 s the bot
    starts moving to the point 1.1 r from the grenade straight away from
    it (if reachable).
35. **Grenade too far.** Grenade lands r + 101 u from the bot → no
    reaction.
36. **Mine as target.** XE bot (safe factor 0.5), player's mine with
    blast radius 300 at 140 u → not seen; at 160 u (> 150) → seen and shot
    at its exact position.
37. **Burst rhythm.** HN rifle bot: burst draw 5, pause draw 0.35 → fires
    5 rounds (by clip count), holds 0.35 s, repeats.
38. **Reload.** Clip empty, reserve 30 → clip refilled immediately, reload
    animation plays, an attack move is started at once.
39. **Attack move geometry.** Bot radius 16, NN (shuffle 30), facing +Z,
    pick "shuffle right" → destination 46 u to its right; pick "back up"
    → 66 u behind.
40. **Respawn.** Bot killed at t = 0 → body gone at 5 s, bot respawns at
    7 s at a Bravo start point, invulnerable until 10 s.
41. **Patrol link choice.** Node with links to A (never visited) and B
    (visited 5 s ago, dead ahead) → A. Both visited, A at clock 100000 ms
    sideways (dot 0), B at clock 104000 ms dead ahead (dot 1) → B's score
    94000 < 100000 → B.
42. **Mine drop timer.** NE bot with a mine enters patrol at t = 0, draw
    0.5 → drops a mine at 3 + 0.5·9 = 7.5 s.
43. **Path cost.** Volumes chained A→B→C: entry in A at distance 100 from
    the A–B link, B's entry to B–C link 300 → cost to C = 100² + 300² =
    100000 (weights 1).
44. **Freeze.** Round reaches the kill goal → all bots stop moving and
    firing on the next update.

## Open questions

1. **Local server frame rate.** Bot Deathmatch runs a local server inside
   the client. Its frame rate decides the sense-pass timers (Quirk 2) and
   all per-frame steps. Measure: count bot sense passes per second (e.g.
   time from a single gunshot to the bot turning, with known delay).
2. **Order of senses and bot updates** within a server frame (stimulus
   manager before or after the objects). Affects whether a sight fires
   into goals in the same frame.
3. **Fire key timing.** The bot fire rate comes from the fire keys of the
   NPC fire animations of each weapon animation set (not in this spec's
   reference), capped by the weapon's fire delay. Measure the bot's rate of
   fire per weapon in the live game and from the third-person animation
   data.
4. **Throw keys.** The throw angle and power carried by the throw
   animations' keys (defaults 0° and 1) were not read.
5. **Contradiction with `grenades.md`.** That spec says "a grenade at rest
   is announced to bots as a danger". The reference announces it on
   **every server frame in which the grenade touched something** (each
   bounce and the rest), only for player-thrown grenades whose effect data
   allows it, only to bots of other teams within radius + 100 u with a
   clear line, and re-announces only after it moved 5 u. `grenades.md`
   should be read as including bounces.
6. **The brief's premise vs the reference.** Bots do **not** move, aim or
   fire through `movement.md`, `weapon_firing.md` or `inventory.md`: they
   move at constant template speeds confined to AI volumes (no player
   physics, acceleration, stamina, crouch-spam or fall damage rules), their
   shots have no spread, recoil or zone factor and go straight to the aim
   point (one ray per shot), their fire rate comes from animation keys, and
   their inventory is the six fixed slots of section 8.1. Only the shared
   parts apply: the server fire-delay check, ray impact and damage
   (`weapon_firing.md`), grenade flight (`grenades.md`), ammo data. An
   implementation that drives bots through the player controller would
   behave differently; decide which one we want.
7. **Shotgun single ray** (Quirk 7): confirm in the live game that a bot
   shotgun blast deals one pellet's damage.
8. **Bot Training level.** Which profile level Bot Training uses (the
   Tutorial profiles exist; the path that selects them was not found) and
   how many bots it spawns.
9. **Cover, view and alarm nodes.** Whether the bot maps contain cover,
   view, vantage or alarm nodes (the goals exist in the bot goal set but
   need such nodes). If they do, those behaviours need their own detail.
10. **Distress and flee** goals are in the bot goal set; their trigger
    conditions in the bot context were not traced.
11. **Stuck bots.** Beyond obstacle avoidance and the fall-through pop-up,
    no unstick logic was found. Watch for bots stuck on volume edges in the
    live game.
12. **Investigate details.** The alarm-level bookkeeping that decides
    between "just look" and "go there" was not fully traced; the reading
    that gunshots always send the bot is an inference from the alarm
    levels (75 vs threshold 5).
13. **Room defaults.** The default player count shown when a Bot
    Deathmatch room is created (the room data gives 5–8 and a recommended
    8).
14. **Ally gunfire.** The single-play data switches off the ally-gunfire,
    ally-death and impact stimuli; confirm that bots do not react to their
    teammates being shot or killed.
15. **PvE AI.** Fireteam and other PvE enemies (section 11) need a separate
    spec, from the multiplayer AI data files.
