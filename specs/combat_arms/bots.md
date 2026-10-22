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

A follow-up session (Open questions 16–24) added the bot volume, path,
patrol, look-around, sense-recorder, crouch-request and chat code, the
bot maps' AI objects from an older extracted client release and their
third-person fire animations.

Sections: Summary; Does the game have bots; Units and conventions;
Constants; Difficulty, profiles and loadouts; Behavior (1 think loop,
2 goals and states, 3 navigation, 4 patrol, 5 perception, 6 target
selection, 7 reaction and aim, 8 weapon use, 9 movement in a fight,
10 team behaviour and mode rules, 11 other AI in the game); Per-tick order;
Edge cases; Quirks; Corrections (2026-10-22); Test cases; Open questions.

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

**The profile file's fields by role** (Open question 22; the bot info file
of the attribute data):

- **Profile** (`Profile<n>`): `Difficulty` (`Normal`, `Hard`, `Extreme`,
  `Tutorial` = Easy, Normal, Veteran, Tutorial lists), `BotInfoIndex` (the
  bot base), `NickName` and `Clan` (string-table keys such as
  `$Normal_Profile0_Nick`), `Rank`, `ClanMarkEmblem`, `ClanMarkBackground`,
  `KDR` (shown only), `NationIndex`, `BaseModelID`, `NextChatTime` (chat
  cooldown, default 10 when absent), `ItemInfoIndex0..` (item sets).
- **Bot base** (`BotInfo<n>`, 36): `Brain`, `Template`, `BotTemplate` (the
  same name, for example `Normal_N_Flash`), `GoalSet` (`BDM_Bot`), `MaxHP`,
  `MaxAmmor` (100 / 100, unread).
- **Item set** (`ItemInfo<n>`, 36): per slot a `<slot>_UsePercent` and a
  list `<slot>_ItemNo<k>` of item numbers: `Model`, `Backpack`, `Head`,
  `Face` (read for the goggle slot, Quirk 9), `M_P_Weapon` (mid-range
  primary), `L_P_Weapon` (long-range primary), `S_S_Weapon` (secondary),
  `M_Weapon` (melee), `G_Weapon` (grenade), `M_S_Weapon` (mine).
- **Chat sets** (`AIChatMessage0..2`), see 10.

The draw ρ (Open question 22): milliseconds since the machine started plus
one C-library random number, made fresh for each bot and for each chat-set
choice; it is not seeded by the game. Nothing depends on its exact values
beyond the correlations described above, so any uniform seeded draw that
keeps "one number for all slots of a bot" is parity.

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

Speeds for an implementation that moves bots with its own mover
(Open question 6, answered for the speeds): jump-over speed equals the jump
speed (300 Normal and Tutorial, 315 Hard and Veteran); crawl 100 u/s
exists in every template but no bot state crawls; swim and fly 0. Patrol,
chase, investigate and attack moves all use the run speed; only scripted
walks use 70. These are ground speeds in u/s, constant (no acceleration,
no stamina, no sprint), multiplied by the gear and weapon penalties of 3.
At an AI volume's edge the next position is refused when it lies outside
every enabled volume: the bot stops for that update and the waypoint counts
as reached (3); it never slides along the edge. A mover that uses the
player's physics must therefore clamp bots to the volumes itself to match.
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
invalid and dropped. A volume keeps at most 32 neighbour candidates (the
link builder's table size); a candidate whose shared rectangle has a
diagonal below 0.71 u (squared length 0.5) is not a neighbour at all.
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

**What each volume type does** (Open question 16; the bot maps use only
three: every bot map has plain volumes, Death Room, Junk Flea, Oil Rig and
Short Fuse jump-up volumes, Downhill, Neptune, Oil Rig and Short Fuse ladder
volumes; none has stairs, ledge, jump-over, junction or teleport volumes,
of the seven bot maps' objects read on an older extracted client release):

- **Plain**: walkable box; links as above, both ways.
- **Ladder**: a bot may enter it only if its template allows ladders
  (`CanUseVolumeLadder`, true for every bot template); the path climbs it at
  the ladder speed (200 u/s) instead of walking; links both ways.
- **Jump-up**: a 64 u deep box in the jump's direction (a box not 64 u
  deep along the link it is entered from is refused), **entered only in the
  direction of its jump**; leaving it, the next volume must be **higher**
  (top of the next box above this one's top), unless the volume says
  `OnlyJumpDown`, in which case the next box's bottom must not be higher
  than this one's (a drop). `OnlyJumpWhenAlert` limits it to alert bots
  (unused: the stock volumes have it off). The bot crosses it with a
  parabolic jump at the jump speed. So jump-up links are **one-way**. Stock:
  Death Room's four are all `OnlyJumpDown`.
- **Jump-over**: entered and left only along its forward axis, with exactly
  one neighbour on each side (otherwise unusable); a jump at the jump-over
  speed (300 / 315 u/s). Not in bot maps.
- **Stairs**, **ledge**: plain volumes as far as linking goes; excluded
  from the "straight path" tests of the attack moves, dodges and sight
  short cut. Not in bot maps.
- **Junction**: up to four named exits (north, east, south, west) with up to
  four actions each (look, peek, search; each with a chance) that a bot
  passing through may play. Not in bot maps.
- **Teleport**: linked to a named destination volume, one-way. Not in bot
  maps.
- **Ambient-life**: never used by bots.

**Path weights**: every volume has `PreferredPath` and a weight, but every
volume of the seven bot maps has `PreferredPath` off and the default
weight, so in practice all weights are 1 and paths are the squared-distance
shortest ones. **Regions** (`AIRegion`): each bot map has exactly one
region holding all volumes, with no permission keys set; it changes
nothing. There are no `AINodeGoto` objects in the bot maps.

**Doors** are passed if not locked for the bot; a closed unlocked door is
opened by walking into it. **Drops** happen only where a jump-up/ledge
volume pair or overlapping boxes of different heights connect; a bot never
walks off an edge that is not linked. **Getting stuck**: bots step aside
from other characters on their path (dynamic obstacle avoidance); a bot
that ends up below its volume's floor is popped back to the floor height
and 1 u toward its last valid position; there is no other unstick logic
(Open question 11). The one retry rule found: a bot waiting for a door that
does not open for more than 2 updates gives up the door step and tries it
again from the start; if the door is blocked and the bot is alert with a
target, it shoots through the door when the door's surface can be shot
through.

### 4. Patrol

At level start the mission manager reads, from the map's bot game-mode
file, up to 8 **patrol start nodes** per team (`PatrolNode_Alpha0..7`,
`PatrolNode_Bravo0..7`) and up to 8 **guard nodes** per team. Each spawned
bot gets the next patrol start node of its team, round robin. Patrol nodes
are linked into a graph (next / previous links placed in the map). A bot
runs (not walks) from node to node; at a node with a wait action it waits
3 + U(0, 3) s; at a look-around node it plays look-around (head sweeps,
cosmetic). Data and details (Open question 17): the start nodes are the
mission file's `AIStartNodeList` section (`PatrolNode_Bravo0` =
`AI_Node_B_01` … `PatrolNode_Alpha7` = `AI_Node_A_08` on Death Room); every
patrol node in the seven bot maps (772 nodes) has the action `LookAround`,
no `Command` and a `Next` link, so there are no wait-only nodes. At a
look-around node, if the bot's template has `StopLookAroundPercentage` P
(25 for every bot template) and a 0..99 draw is **above** P, the bot does
not stop (it keeps running to the next node, 74 % of the time); otherwise it
stops, faces the node's forward if the node says so (`Face`), and looks
around for 3 + U(0, 3) s: repeated head turns to a random yaw in ±67.5°
and, one time in three, a random pitch in −45°..+22.5° (else level). While
**moving** on patrol it also looks around now and then: every
U(0, `NextLookAroundTime`) s it looks around for `NextLookAroundTime` s
(8, 10, 12 s for the `_N`, `_N_Flash`, `_N_Smoke` templates; 5, 4, 5
for `_H`, `_H_Mine`, `_H_SR`; 15, 11, 13, 20 for `_E`, `_E_Launch`,
`_E_SG`, `_E_MG`; the same at every level; the templates' separate
`LookAroundTime` is not used by this rule). Look-around turns only the
head and the eye forward used by sight (5.2), not the body. Choosing the next node when a node has several links: among
links other than the node it came from, score each linked node by

```math
\text{score} = \max\big(0,\ t_{used} - 10000\cdot\max(0, \hat f\cdot\hat d)\big)
```

with t_used the millisecond clock when that node was last reached by any
bot (0 if never), f̂ the bot's forward direction and d̂ the link direction;
the lowest score wins, ties to the first link. Unvisited nodes therefore
always win, then the least recently visited, with up to a 10 s bonus for
going straight on. At the end of a chain it turns back.

**Guards**: the maps' guard data is `AINodeGuard` objects with up to 16
linked `AINodeUseObject` nodes (Oil Rig: two guard nodes, five use-object
nodes with the action `SniperStand`, a 512 u radius, run movement and
commands that disable a node for a few seconds while used); the data supports a guard role for bots carrying a long-range
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
2. The ray start is the bot's **eye point**: its own hull centre's x and z
   with the height of its `Head` node (`Bip01 Head` on bodies without
   `Head`), re-evaluated once per sense pass (Open question 18). The aim spot cycles over a small
   grid on the target's facing plane (one grid step per failed pass); for
   characters it is then replaced by exact node tests below.
3. Distance² ≥ R²: not seen.
4. FOV: alert bots see 180° horizontally with no vertical limit; relaxed
   bots 140° horizontally (dot with eye forward ≥ 0.342) and not more than
   60° up from horizontal (dot with up ≤ 0.5); inside 256 u always 180°.
5. If a straight walk exists through AI volumes from the eye to the spot
   (ignoring ladder/stairs/jump/teleport volumes), it is visible without a
   ray (only when the target stands inside a volume, not under it).
6. Ray from eye to the target's **object position** (a player's hull
   centre, 50 u above the feet standing, 40 u crouched). Ignored by the ray: the bot itself,
   bodies, hitboxes, **all characters** (teammates and enemies do not block
   sight), liquids, see-through surfaces, and anything non-solid (so
   **smoke never blocks sight**, as `grenades.md` says). Hit nothing, or hit
   the target: seen.
7. Otherwise one more ray to the target's **head node** (`Head`, or
   `Bip01 Head`, at its current animated position): if clear, seen, and the
   bot remembers that only the head was visible (7.4). There is no torso or
   socket test besides these two points.

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
  modifier (Open question 18 for the details: the stimulus is posted at
  each `FOOTSTEP_KEY` of the character's **server-side** leg animation; the
  stock leg animations carry them only in the running (`LRF` 50 and
  350 ms, every 300 ms), sprinting (`LRRF` 63 and 312 ms, about every
  250 ms) and jump (`LSJ`, `LJJ` 600 ms, the landing) animations, while
  walking (`LW*`) and crouch-walking (`LC*`) carry only footprint keys, so
  **walking and crouching are silent to bots**; there is no speed
  threshold: the run animation plays whenever a movement key is held with
  the run flag, even against a wall; surface modifiers (`MoveNoiseMod`):
  1.0 for most (cement, wood, dirt, rock), 2.0 tile, tin, porcelain and
  untyped, 1.5 metal, liquid and water, 1.25 metal doors, 0.75 snow,
  grass, mud, sand, foliage, 0.5 carpet and cloth, 0.0 ladders and air); the bot hears it within 1500 + that radius and ±128 u
  vertically. Each heard step adds 0.5; between steps it decays by about
  0.0013–0.0025 per pass, so in practice the **third** heard step (or the
  third "false" one) fires the sense.
- **Grenade bounces** play a sound with a radius (`grenades.md`), but no
  hearing stimulus is posted from it; the danger path (5.4) is how bots
  react to grenades.
- **False stimulations** (Open question 18): for the two senses with a
  lower threshold (enemy visible, footsteps; threshold 0.5–1.0): a sense
  whose level rose past 0.5 without reaching 1.0 and then decays back below
  0.5 counts one false stimulation and is handled by the current state as a
  weak cue (the visible sense is reported as the "lean" sense, which the
  bot data switches off; a footstep as an unconfirmed footstep); the third
  one (`FalseStimulationLimit` 3) is turned into a full stimulation (level
  set to 1.0) and the counter restarts. With sight in practice always
  instant (5.2) this matters only for footsteps.
- **Ally gunfire** (Open question 14, answered): the bot data's ally
  stimuli (`AllyWeaponFireSound`, `AllyDeathVisible`, `AllyDeathSound`,
  `AllyPainSound`, `AllyDisturbanceVisible`, `AllyDistressVisible`,
  `AllySpecialDamageVisible`) all have `Activity` FALSE, and only four
  stimuli are active (`EnemyVisible`, `EnemyWeaponFireSound`,
  `EnemyFootstepSound`, `AllyDisturbanceSound`, the ally call). So bots do
  not react to teammates being shot, hurt or killed, whatever the
  templates' `CanHearAllyWeaponFire` and distances say.

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
   - else it tries **5** directions (corrected, Open question 19): straight
     away from the grenade, then 45° and 90° to one side, then 90° and 45°
     to the other side (the three directions toward the grenade, 135°,
     180° and 225°, are skipped): the first point at 1.1 × radius from the grenade with a
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
the player rules of `weapon_firing.md`. **Corrected** (Open question 24):
bots **do** get the hit-zone factor: the server tests the target's spheres
itself (`characters.md` 11, server test), records the struck node and
applies its zone factor (head ×6 on the stock tables) for bot damagers;
only AI that is not in bot mode (PvE enemies) gets none.

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
  animation (three fire variants, re-picked at random every 10 s). The fire
  animation loops while the bot fires, and every stock gun fire animation
  carries one fire key at 0 ms, so the bot's interval is the fire
  animation's **length** (Open question 3, answered). Standing lengths in
  the male library of an older extracted release (re-measure on Steam):
  rifle class (`AniType` 0: M4A1, AK-47, M16A3, M60 …) 100 ms; SMG and P90
  100; pistol 100; sniper 1000; automatic shotgun (`AniType` 6, R870)
  466; double barrel 1000; snowball gun 900; rocket launchers 1200; chain
  gun 133; knife 500 (key at 233 ms; stab 800, key at 366). The bot also
  measures its weapon's fire animation length the first time it fires it
  and uses that as the forced interval during hit reactions (else the
  data's fire-frame). While a
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
Details (Open question 20): the request is granted only if the blocking
bot is itself attacking with a ranged weapon, its crouch chance is above 0
(0.01) and it has crouched for less than its crouch time (10 s) so far;
the two must be at least 48 u apart. When granted, the asking bot does
nothing that update (no shot, no dodge) and the other crouches for the rest
of its 10 s; when refused, the asking bot forces a dodge on its next dodge
check.

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
  invulnerable for 3 s. (Open question 21:) the 3 s is the bot brain's own
  `GodModeTime` (3.0 in every bot brain), started at the bot's first update
  after each spawn; while it runs the bot takes no damage of any kind. The
  room's or mode's spawn protection (`game_modes.md` S6) applies only to
  the human. The 7 s respawn delay is a fixed delayed command the bot mode
  sends at each bot's death, whatever the mode's respawn waits; the human
  respawns by the TDM rules.
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
  Details (Open question 23): the events are a kill by the bot (the
  victim's death handler), the bot's death, its respawn, the start of its
  ranged attack ("battle"), the start of its patrol ("search"), the start of
  a grenade dodge, sighting a mine; "conceal" and "escape" are never sent.
  Each bot gets one of the three chat sets (`AIChatMessage0..2`) at roster
  time, by (milliseconds since start + C rand) mod 3. Per set and event:
  ten lines with string-table keys of the form
  `$Message<set>_<Event>_<0..9>` (events `Kill`, `Death`, `Respawn`,
  `Battle`, `Search`, `Grenade`, `Mine`, `Conceal`, `Escape`), and a percent
  P (`<Event>_Message_PrintPrecent`, as spelled): kill 60, death 60, respawn
  25, battle 25, search 25, grenade 25, mine 60, conceal 10, escape 10.
  The cooldown restarts **before** the percent roll, so a failed roll still
  silences the bot for the whole cooldown; the roll is (milliseconds since
  start mod 100) > P → nothing. The line is shown in the chat box for 10 s
  as "nickname : line", in the friendly colour if the bot is on the
  human's team, else the enemy colour. The first line is possible one
  cooldown after the roster is built.
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

## Corrections (2026-10-22)

From a comparison pass over the reference's bot mission rules, client bot
roster and chat, target tracking and visibility, shooting, attack-move,
grenade-dodge, investigate and chase behaviour, against this spec and our
implementation (`games/combat_arms/src/game/bots/`, at the commit this
section was written on; file:line below is relative to that folder).
Ordered by what a player notices. Test cases 58–69; Open questions 25–32.

1. **A heard sound sends the bot only halfway** (corrects 5.3 "runs a path
   to the spot and searches"; Open question 12).
   - Reference: when a bot starts investigating something it heard (an
     enemy's gunshot or footstep, or an ally's call), the investigation's
     arrival radius is **half the straight-line distance** from the bot to
     the sound's position at that moment. The bot runs its path toward the
     spot and the investigation is over on the first update it is inside
     that radius (or its path ends). The investigate goal then drops to
     zero and the bot goes back to patrol (it searches instead only when
     the conditions of Open question 30 hold). It never walks onto the
     spot and never stands there.
   - Ours: it walks until within 0.6 m of the spot, then stands looking at
     it until 10 s after the investigation began (brain.rs:949-957,
     1167-1171, 1311-1315).
   - Expect: patrolling bot 1000 u from a gunshot, open floor → it runs
     toward the shot and turns back to its patrol as soon as it is within
     500 u of the shot's position; at 300 u, within 150 u. It is never
     closer than half its starting distance when it gives up.

2. **When a sound is only looked at** (5.3, corrects the reading "only when
   not already strongly alarmed and not relaxed in the last 30 s").
   - Reference: the bot does not move (it only turns toward the sound)
     only if all of these hold, otherwise it runs (1):
     (a) it is not "majorly alarmed": every investigated stimulus adds its
     alarm level to a running total that is kept for the whole life and
     never goes down; at or above the brain's major-alarm threshold the bot
     is majorly alarmed. A gunshot's level is above the threshold, so after
     the first gunshot a bot hears in a life, every later sound in that
     life sends it running;
     (b) more than 30 s have passed since it last returned to relaxed
     awareness, and its **spawn counts as such a return**: for 30 s after
     each spawn, and for 30 s after it calms down again (an investigation
     or chase ending and its patrol resuming), it always runs;
     (c) then: if the spot is within 1000 u, inside its relaxed field of
     view (5.2) and visible (a straight walk through the AI volumes or a
     clear eye ray), it stands where it is (aware); else, if a
     straight walk through the volumes reaches the spot, it turns to look
     at it; else it runs.
   - Ours: a footstep or an ally's call is "look only" whenever a ray from
     the eye to the spot is clear, at any distance and angle; gunfire never
     (brain.rs:1153-1162).
   - Expect: a bot 20 s after spawning hears a footstep in plain view at
     400 u → runs (and stops at 200 u, 1). The same bot 40 s after
     spawning, no gunshot heard this life, nothing investigated in the last
     30 s → turns only. Footstep in plain view at 1200 u → runs. After one
     heard gunshot this life, any footstep → runs.

3. **An investigation is not restarted by every sound** (5.3).
   - Reference: while a bot is investigating (or is merely suspicious) it
     ignores allies' calls. A newly heard enemy sound replaces the current
     investigation only if its alarm level is not lower than the current
     one's and it is not the same stimulus already being followed. An ally's
     call counts as alarm level 1 for the bot that hears it (it raises the
     hearer's running total to at least the caller's). A bot answering a
     call goes toward the **caller's position** and, when it arrives (by the
     half-distance rule, 1), faces the direction the caller was facing.
   - Ours: every accepted footstep, gunshot or call during an investigation
     starts a new one at the new spot with a fresh 10 s, and a call is
     taken while investigating (brain.rs:1067-1127, 1148-1178).
   - Expect: bot investigating a gunshot hears an ally's call → keeps
     going to the gunshot. Bot investigating a footstep hears a gunshot
     elsewhere → switches to the gunshot.

4. **A grenade just outside the blast radius draws the bot onto it**
   (corrects 5.4 step 3, "if the bot is already outside the radius: it
   stays").
   - Reference: when the dodge time comes, the distance is taken from the
     grenade's **current** position. If the bot is farther than the blast
     radius, the point it is given is **the grenade's own position**, and
     it moves there: as an attack move while shooting when it sees its
     target from the weapon, otherwise running. Only a bot standing exactly
     on that point stays. Since the danger is noticed out to radius + 100 u
     (5.4 step 1), bots between r and r + 100 u from a grenade run onto it.
     The dodge (in either case) lasts until the grenade object is gone
     (exploded or removed) or the move ends; it sends the Grenade chat
     event (5). Afterwards the bot returns to its attack if it has a
     target, else stands idle until another goal takes over; a bot that was
     chasing when the dodge began resumes its chase.
   - Ours: outside the radius it stays, measured from where the grenade
     was noticed (brain.rs:1366-1374); a dodge ends after 3 s or on arrival
     and then goes to patrol unless the target is visible
     (brain.rs:1316-1323, 1383).
   - Keep the reference's behaviour (parity first; a "fix" would be an
     option later).
   - Expect: an M67 with blast radius r comes to rest r + 50 u from an HN
     bot with a clear line; after the delay draw the bot moves to the
     grenade's position. The same grenade at r − 50 u → the bot moves to
     the point 1.1 r away (5.4).

5. **Bot chat events: four are missing, and Kill is narrower** (10).
   - Reference: Kill is sent only when the bot kills a **player** (from the
     player's death handling), never when it kills another AI character.
     Battle: every time its attack state starts from another state (not
     when it returns to the attack after an attack move or a grenade
     dodge). Search: every time its patrol goal activates, including the
     patrol start at each spawn and every return to patrol after a chase,
     an investigation or a fight. Grenade: every grenade-dodge activation
     that found a point to go to (including 4). Mine: every time its sight
     sense fires on a placed device. Respawn: every spawn, the first one
     and the round-start respawns included. Each still passes the client's
     cooldown and percent roll (10).
   - Ours: Kill for any death the bot caused, bots included
     (chat.rs:163-165); Battle, Search, Grenade and Mine are never sent
     (chat.rs:12-15).
   - Expect: with the cooldown elapsed, a bot that begins attacking shows
     a `$Message<set>_Battle_<n>` line when the millisecond clock mod 100
     is at most 25; a bot that kills another bot shows no Kill line; a bot
     that turns back to patrol after losing you rolls a Search line.

6. **The chat cooldown is the profile's, not the level's** (10).
   - Reference: each bot's cooldown is its profile's `NextChatTime` (10 s
     when the profile has none); profiles of the same level may differ.
     The first line is possible one cooldown after the roster is built.
   - Ours: 15, 20 or 25 s by `BotLevel` (chat.rs:121-127).
   - Expect: when the profile file is read, a profile with
     `NextChatTime` 25 on Easy waits 25 s between lines. Until it is read,
     ours is a placeholder (the per-profile values are Open question 26).

7. **Each life is a new brain** (10, Death).
   - Reference: every respawn creates a new AI character from the same
     roster entry. Everything the brain learned is gone: hit/miss counters
     at 0/0, movement inaccuracy, every sense level and false-stimulation
     count, the alarm total of correction 2 (back to 0), the "last relaxed" time (= the
     spawn, 2b), the attack's last-deactivation time (0, so the first
     attack of a life always rolls a reaction delay, 7.2), the dodge and
     crouch timers, the grenade count (full) and the pending sense reaction
     timers. The profile, kit, team and bot ID stay.
   - Ours: the bot's brain state survives death; only the state, target,
     route, danger, pending reaction, aim offset and crouch are cleared
     (brain.rs:785-802): the hit/miss cycle, the footstep sense, the sight
     cooldown, the dodge timer and the last attack time carry over.
   - Expect: a bot dies with its cycle at 1 hit / 2 misses of (1, 5); after
     its respawn its first shot starts a fresh cycle (0/0). A bot that left
     an attack 1 s before dying rolls a reaction delay on its first attack
     after the respawn.

8. **Patrol stops at nodes; the first node comes from the mission file**
   (4; the spec already says this, ours lacks it).
   - Reference: every patrol node in the bot maps is a look-around node:
     with probability 0.74 the bot runs on, else it stops 3 + U(0, 3) s
     (head sweeps); while running it also starts a look-around every
     U(0, `NextLookAroundTime`) s. Each spawned bot's first patrol node is
     its team's next start node from the bot game-mode file (round robin
     over spawns).
   - Ours: never stops, never looks around, and starts from the nearest
     node (brain.rs:1687-1738, 1694).
   - Expect: over 100 node arrivals a bot stops at about 26 of them, each
     stop 3–6 s. Eight Bravo bots spawned in a row start at
     `PatrolNode_Bravo0` … `PatrolNode_Bravo7`; the ninth spawn starts at
     `PatrolNode_Bravo0` again.

9. **A grenade throw is an animated action from the hand** (8.5).
   - Reference: the bot switches to its grenade, plays its throw animation
     (one of two, chosen once by whether the target is closer than
     1000 u), and the grenade leaves the **hand socket** at the animation's
     throw key; until the throw animation has finished the bot cannot
     switch back, so it fires nothing from the moment the throw starts to
     the end of the animation; then it re-chooses its ranged weapon (8.2).
   - Ours: the grenade is spawned at the eye in the same tick the count
     reaches zero, and the bot keeps firing its gun (brain.rs:1568-1581).
   - Expect: from the throw decision to the throw key no round is fired;
     the grenade's first position is at the bot's hand, not its eye; the
     next round comes after the weapon switch back. (Animation and key
     times: Open question 27.)

10. **Firing while chasing is blind** (5.6, 8.2).
    - Reference: in the chase state the bot fires whenever its target is
      visible from the weapon and within its ranged reach, with no
      reaction delay and no burst pause from the per-weapon AI rows (its
      whole clip, then an **instant** reload, with no reload animation).
      While it is following a path its shots are "blind": each goes along
      the bot's **torso facing**, not to the aim point of 7.3, so the
      hit/miss cycle is not used. (The torso is turned toward the target's
      head by head-and-torso tracking, within limits not measured here,
      Open question 28.) In practice this matters for the frames before
      the attack goal takes over (section 2) and when the target is visible but
      the attack goal does not trigger.
    - Ours: chase fires through the attack's aim, with the hit/miss cycle,
      reaction delay and bursts (brain.rs:943-946).
    - Expect: a chasing bot running along its path with the target visible
      from the weapon fires along its torso direction; its rounds never
      go through the 100 u side-offset of a miss.

11. **The aim base point and the line of fire** (7.3, 7.4; small).
    - Reference: the base point is the target's `Torso` node as animated
      (5.14 u above the hull centre in the male bind pose standing; lower
      crouched). Whether the bot may fire ("visible from the weapon") is a
      ray from the bot's **right side** (one radius out, a quarter of its
      height above its centre) to that node, then to the `Head` node; the
      head-only flag of 7.4 comes from this ray. The eye test (5.2) only
      says whether the target is seen.
    - Ours: base point the hull centre (brain.rs:103), line of fire from the
      eye (29 u above the centre) to the base point (brain.rs:866-876),
      head-only from the eye test (brain.rs:1013-1016).
    - Expect: bot and target standing on one floor 500 u apart, a wall
      20 u in front of the target with its top 52 u above the floor: the
      target's hull centre (50 u) is hidden but its `Torso` node (about
      55 u) is not, and the side ray (from about 75 u) clears the wall
      (it passes it at about 55.8 u) → the reference aims at the torso
      with full accuracy; ours treats the target as head-only (half
      accuracy, aimed at the head). Wall top 60 u → both head-only.

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

**Follow-up cases** (Open questions 3, 6, 11, 14, 16–24). "Old release" = map
objects and animation files of an older extracted client release;
re-measure on the Steam install (build 25595979).

45. **Zone factor for bots (logic).** An Easy bot's aimed M4A1 shot whose
    ray passes within the radius of the target's `Face_Jaw` sphere (head,
    factor 6) on the server's pose → the damage is 6 × the base before
    armour, as for a player's head shot. A PvE (non-bot) AI's shot on the
    same line → factor 1.
46. **Bot fire interval (old release).** A bot firing an M4A1 (class 0)
    standing: one shot per 100 ms of its fire animation (plus frame
    rounding at the local server's rate); an R870 (class 6): 466 ms; an
    L96A1 (class 5): 1000 ms; a knife slash: 500 ms with the hit at 233 ms.
47. **Speeds (data).** Normal-level bot: run 270, jump 300, jump-over 300;
    Veteran: run 300, jump 315; `Normal_N_Flash`: run 300. A bot whose next
    step would leave every AI volume stops on that update and takes its
    next waypoint.
48. **Jump-down volume (old release).** Death Room `AIVolumeJump01`
    (`OnlyJumpDown`, box (40, 96, 24) half-extents): a path may use it only
    from its upper neighbour down to a lower one, never upward.
49. **Look-around stop (data).** Patrolling `Normal_N` bot reaching a
    `LookAround` node: with a 0..99 draw of 30 (> 25) it runs on; with 20 it
    stops for 3 + U(0, 3) s, head yaw targets in ±67.5°. While running it
    starts a look-around every U(0, 8) s lasting 8 s.
50. **Footsteps (old release, logic).** A player running forward (`LRF`):
    footstep stimuli at 50 and 350 ms of each 600 ms cycle (every 300 ms);
    walking (`LWF`) or crouch-walking (`LCF`): none; a jump: one at the
    landing key (600 ms of `LSJ` / `LJJ`). On carpet (0.5) the stimulus
    radius is 128 u, on tile (2.0) 512 u; a bot hears it within 1500 + that
    radius.
51. **False footsteps (logic).** Footstep sense fed one step (level 0.5),
    then nothing: it decays below 0.5 → one false stimulation (weak cue);
    after the third such false stimulation the sense is set to 1.0 and
    fires.
52. **Ally gunfire (data).** A bot's teammate fires a rifle 300 u away and is
    killed 200 u away, outside the bot's view → no stimulus at all; the bot
    keeps patrolling.
53. **Grenade dodge directions (logic).** Bot 100 u east of a grenade with
    blast radius 300: it tries the point 330 u from the grenade straight
    east first, then 45° and 90° to one side, then 90° and 45° to the other;
    never the three directions with a westward component of 135°–225°.
54. **Crouch request (logic).** Bot A's line of fire is blocked by teammate
    B 100 u ahead, B attacking with a rifle and having crouched 0 s: B
    crouches, A does nothing that update. B 40 u ahead → refused, A forces
    a dodge.
55. **Bot spawn protection (logic).** A bot spawns at t = 0; a frag blast at
    t = 2.9 s → no damage; at t = 3.1 s → damage. The bot dies at t = 10 s →
    respawns at t = 17 s whatever the room's respawn settings.
56. **Chat roll (data).** A bot with chat set 0 kills the player at t = 30 s
    with its cooldown (20 s) elapsed: the cooldown restarts; with a
    millisecond clock ending in 42 (≤ 60) a kill line `$Message0_Kill_<0..9>`
    is shown for 10 s; ending in 75 → no line, and no line from this bot
    until t = 50 s.
57. **Door retry (logic).** A bot waiting at a door that has not opened for
    3 updates restarts its door step; if alert with a target and the door's
    surface can be shot through, it shoots through it.

**Corrections (2026-10-22) cases.**

58. **Half-distance investigation (Correction 1).** Patrolling bot,
    gunshot 1000 u away with a volume path → the bot runs toward it and
    is back on its patrol goal on the first update it is within 500 u of
    the shot's position; with the shot 300 u away, within 150 u.
59. **Look only (Correction 2).** Bot 40 s after spawning, nothing heard
    this life: footstep in plain view and in its relaxed field of view at
    400 u → it turns to it and does not move; the same at 20 s after
    spawning → it runs (Case 58's rule); at 1200 u → it runs; after one
    gunshot heard earlier in the same life → it runs.
60. **Calls while investigating (Correction 3).** Bot investigating a
    gunshot; an ally's call arrives → ignored, the bot keeps its
    destination.
61. **Grenade just outside the radius (Correction 4).** Grenade of blast
    radius r at rest r + 50 u from an HN bot, clear line, delay draw
    0.4 s → at 0.4 s the bot starts moving to the grenade's position; at
    r − 50 u → to the point 1.1 r from the grenade, straight away from it.
62. **Kill chat (Correction 5).** A bot kills another bot → no Kill line;
    it kills a player with its cooldown elapsed and the clock mod 100 at
    42 → a `$Message<set>_Kill_<n>` line.
63. **Battle and Search chat (Correction 5).** Cooldown elapsed, clock mod
    100 at 20: a patrolling bot enters its attack → Battle line; later,
    cooldown elapsed again, it gives up a chase and resumes patrol →
    Search line; an attack move ending back in the attack → no Battle
    line.
64. **Chat cooldown (Correction 6).** Profile with `NextChatTime` 25 on
    Easy → two lines of that bot are at least 25 s apart.
65. **Fresh brain (Correction 7).** Bot dies with hit/miss counters 1/2
    (cycle 1, 5) → its first shot after the respawn is drawn from 0/0; a
    bot that left an attack 1 s before dying rolls a reaction delay in
    its next life's first attack.
66. **Patrol stops (Correction 8).** Node arrival with a 0..99 draw of 20
    → the bot stops 3 + U(0, 3) s; draw 30 → it runs on. First patrol
    node of the k-th Bravo spawn = `PatrolNode_Bravo<k mod 8>`.
67. **Throw (Correction 9).** Grenade count reaches 0 with the target at
    800 u → the near-throw animation plays; no round is fired until the
    weapon is switched back after it; the grenade starts at the hand.
68. **Blind chase fire (Correction 10).** Chasing bot following a path,
    target visible from the weapon and in reach → each round leaves along
    the bot's torso facing; no round is offset 100 u sideways (no miss of
    7.3).
69. **Base point (Correction 11).** Geometry of Correction 11 → wall top
    52 u: aimed at the `Torso` node with full accuracy; 60 u: head-only,
    half accuracy, aimed at the `Head` node.

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
   **Answered:** the fire animation loops and each stock gun fire
   animation has one fire key at 0, so the interval is its length (rifle,
   SMG, pistol 100 ms, sniper 1000, automatic shotgun 466, double barrel
   1000, rocket launchers 1200, chain gun 133, knife 500), measured on an
   older release's library; the server's `FireDelay` floor (30 ms default)
   never binds. Applied in 8.3; case 46. Re-measure the Steam library; a
   class whose fire name is missing from the library (the machine-gun `M`
   family in the US library, `characters.md` Quirk 1) plays no fire
   animation and so fires only through the hit-reaction path: check in game
   whether a bot with an M60-class gun of `AniType` 8 fires at all (stock
   M60 is class 0).
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
   **Answered for the numbers:** run, walk, jump, jump-over, ladder and fall
   speeds per template (Difficulty table and the paragraph after it), the
   volume-edge rule (stop, waypoint reached), and the zone factor (bots do
   get it, Open question 24). The design choice itself stays with us.
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
    Still open. Checked again: the path follower, the volume refusal and the
    states; the only retry found is the door rule (3, case 57). A bot that
    makes no progress against a wall stays until its goal changes (a new
    sense, the chase timer, the attack-move timer). Measure: on Short Fuse,
    block a narrow corridor with a crate-sized prop (or stand in it as a
    teammate in a modded room) and time how long a patrolling bot pushes
    against it before turning.
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
    **Answered:** confirmed from the data: every ally stimulus except the
    ally call has `Activity` FALSE; only four stimuli are active. Applied in
    5.3; case 52.
15. **PvE AI.** Fireteam and other PvE enemies (section 11) need a separate
    spec, from the multiplayer AI data files.
16. **Navigation volumes** (types, doors, path weights, neighbour cap).
    **Answered:** 3 (What each volume type does; 32 candidates; weights all
    1 in the bot maps; one region, no goto nodes); case 48.
17. **The bot game-mode file's patrol start nodes, node actions, guard and
    use-object nodes.**
    **Answered:** 4 (the `AIStartNodeList` section; all 772 patrol nodes are
    look-around nodes with the 25 % stop rule and the 3–6 s look; guard
    nodes with sniper-stand use-object nodes exist on Oil Rig only and are
    never used, Quirk 10); case 49.
18. **Senses: sight points, footsteps, false stimulations.**
    **Answered:** 5.2 (eye = own hull x/z at the `Head` node height; target
    = hull centre, then the head node), 5.3 (footsteps from the server's
    leg-animation keys only: running, sprinting and landings; surface
    multipliers; false stimulations); cases 50, 51.
19. **Grenade danger: bot-thrown grenades and the dodge directions.**
    **Answered:** only player-thrown grenades are announced (a bot's own
    grenades never warn other bots); the dodge tries 5 directions, not 6
    (5.4 corrected); case 53.
20. **Small timings** (catch breath, crouch request, roll, knife, reload
    move).
    **Answered:** the crouch request (9.3; case 54); the roll is 140 u
    (9.2); the knife: approach until within its 130 u range, every fire key
    of the melee animation then deals the damage at that range with no
    swing trace (8.4); the reload's attack move starts on the update the
    reload starts (the clip refill and the reload animation start are the
    same event, 8.3). **Still open:** the length of the "catch breath" stop:
    it is a one-shot action animation of the AI animation set, which the
    bots' player-style animation driver may not have (then it ends at
    once). Measure: let a bot chase you out of sight for its endurance time
    (12 s on Easy) and time its pause.
21. **Bots' spawn protection and respawn wait.**
    **Answered:** the brain's `GodModeTime` 3.0 s (all damage), and a fixed
    7 s respawn delay, independent of the mode (10); case 55.
22. **Profiles and the draw ρ.**
    **Answered:** field roles in "Choosing the bots"; ρ = milliseconds
    since start + rand(), any uniform draw is parity.
23. **Bot chat.**
    **Answered:** 10 (events, sets, string keys, percents, the cooldown
    before the roll); case 56.
24. **Hit-zone factor for AI shooters** (with Open question 6).
    **Answered:** bots get it, from the server's sphere test (7.3
    corrected); PvE AI does not; case 45.
25. **Alarm levels** (Corrections 2, 3). The brain's major-alarm threshold
    and each stimulus's alarm level (gunfire 75 by Open question 12;
    footsteps not read) are data in the install's single-play AI files,
    not in the reference code. They decide when a bot stops "looking only"
    (Correction 2a) and which sound replaces an investigation
    (Correction 3). Read them from the attribute data.
26. **Per-profile chat cooldowns** (Correction 6). Which profiles have
    `NextChatTime` 15, 20 or 25 (or none, 10) needs the install's profile
    file.
27. **Animation times** (Correction 9, 8.3). The throw animations' lengths
    and throw-key times, the bots' third-person reload and weapon-select
    animation lengths (no firing while they play). Our bots use the
    player's reload and switch times; measure the bots' in game.
28. **Torso tracking limits** (Correction 10). How far the torso turns
    toward the target while running (the direction of blind chase fire).
29. **Dodge check chance** (9.2). Each 0.25 s dodge check first rolls
    against a brain "check chance" before testing "aiming at me"; the
    spec's 0.7 / 0.3 is the shuffle/roll split. The check chance's value
    (1 would make 9.2 exact as written) was not read.
30. **Search after an investigation** (Correction 1). On reaching the
    half-distance point, a majorly alarmed bot whose volume lies in a
    searchable region targets the character that made the sound and
    starts a search instead of going back to patrol. Whether the bot maps'
    single region (3, "Regions") is searchable was not determined; if it
    is, a bot that has heard a gunshot this life turns toward the shooter
    at that point.
31. **"Has ammo" in the weapon choice** (8.2). Whether the test counts the
    rounds in the clip or only the reserve was not settled; it decides
    whether a bot switches away from a weapon with a loaded clip and no
    reserve.
32. **Firing during a grenade dodge** (Correction 4). The dodge forces the
    fire pose only when the target is **beyond** the held weapon's range
    max; whether the shooting itself still fires inside that range
    during the dodge's attack move was not settled. Watch a bot dodge a
    grenade while you stand 500 u away in view.
