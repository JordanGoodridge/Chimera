# Combat Arms: mission data

Source basis: the shared per-map mission reader (the "map attribute" manager used by the dedicated server and the client), every server-side mode rule set where it reads that data (timers, teams, objective weapons, mode sections), the server's start-up path that chooses the mission file and the game type, the server configuration reader, the VIP Elimination table reader, the room data of a US-region data set (the map records that name each mission file), the plaintext mission archive of an older regional client (687 files, every value table below was measured on it), the mission archive directories of the US-region client and of the Steam install (names and sizes only; their payloads are encrypted), and the client's and the dedicated server's copies of the server configuration file.
Status: draft

Sections: Summary; Units and conventions; Where the data lives; Format; How a match finds its mission; Fields (F1 map info; F2 mission records; F3 objective records; F4 mode sections: Quarantine, Spy Hunt, Bombing Run, Seize and Secure, Hired Guns, VIP Escort, VIP Elimination, Arms Race, forced loadouts, world object overrides, Hunted, co-operative, rewards); Timers per mode; Spawn families and teams; The server file; Override order; Stock values; Edge cases; Quirks; Test cases; Open questions.

## Summary

Every playable (map, mode) pair has its own small mission file, written in the same text syntax as the attribute files (`attribute_data.md`). The lobby's room data says which file a room on a given map and mode opens; the file names the world to load, fixes the game type by its own mode key, and gives the round length, the warm-up length, which team attacks and defends, which objective weapon each side is handed, and the mode's extra tables (infected models, intel count, bullion bank, Arms Race weapon ladder and so on). The server's own configuration file adds the phase lengths between rounds, and the room's goal G overrides or scales a few mission values. Almost every stock value is identical across maps; what differs between maps is the world, the trigger and controller names, the minimap calibration and which modes exist at all.

## Units and conventions

- Times: `TimeLimit`, `TrainingTimeLimit`, `Stage2PlayTime` and the server file's phase lengths are **seconds**, except where a mode says otherwise: Quarantine's `TimeLimit` is **minutes**, Spy Hunt's fallback `TimeLimit` is minutes, the server file's `MissionTimeLimit` is minutes. Timers run on server game time and are tested once per server frame (20 Hz, `game_modes.md` units).
- Distances (minimap calibration) are world units in the world file's frame (`world_dat.md`): X right, Z forward; the minimap uses X and Z only.
- Team indices as in `game_modes.md` S1: 0 observer, 1 Alpha (`JSF`), 2 Bravo (`Creed`). Team-named fields are built from the team data's short names, so `JSF…` is team 1 and `Creed…` is team 2; a field `Observer…` would be team 0.
- G is the room's goal value (`game_modes.md` S2).
- "Integer", "number", "string", "bool", "vector", "point", "range" and the list kinds are the value types and typed reads of `attribute_data.md` ("Value types", "Typed reads", "Lists inside a record"). A field whose stored type does not match its read gets the field's default.
- "Name list" below means one string holding several world-object or weapon names separated by `;` (see Format).

## Where the data lives

| Role | Archive | Entry | Notes |
|---|---|---|---|
| Mission files | `GMS.rez` | `GMS\<MAP>_<SUFFIX>.GMS`, plus a few without suffix | One file per (map, mode). Directory `GMS` at the archive root holds every file. |
| Mode default files | `GMS.rez` | `GMS\VTDM.GMS`, `GMS\VWRM.GMS` | Mode-wide tables (VIP Elimination point table; Arms Race weapon ladders). |
| Room data | `ATTRIBUTES.rez` | `ATTRIBUTES\GAMEROOM.TXT` | `[Map<n>]` records name each mode's mission file; mode records give goal ranges (`game_modes.md` S2). |
| Server file (dedicated) | none | `server.cfg` in the folder above the server's working folder | Phase lengths, map cycle (see The server file). |
| Server file (single-player host, bots) | `ATTRIBUTES.rez` | `ATTRIBUTES\SERVER.CFG` | Same format; the client's copy. |
| Tutorial server file | `ATTRIBUTES.rez` | `ATTRIBUTES\Tutorial.cfg` | Used instead of the server file when the game type is Tutorial. |

- **Encryption.** The Steam install's `GMS.rez` has the V1 title (`V1_RESOURCES_KEY`) and V1-encrypted payloads, exactly like the attributes archive (`rez_archive.md`); only the last (size mod 16) bytes of each payload are plaintext. The US-region client's `GMS.rez` carries the plain title but its payloads are still encrypted (the older reference scheme, where the title selects nothing). The older regional client used for the value tables ships the same files in plaintext. Our loader decrypts exactly as for the attributes archive.
- **Counts.** Older regional client: 687 files (685 mission files with a `[MapInfo]`, plus the two mode default files). US client: 527 files. Steam build 25595979: 820 files. File sizes differ between builds (Steam files are typically about 100 bytes longer), so stock values may have changed; see Open questions.
- **Naming convention.** `<MAP>` is the map's key in capitals (`GHOSTTOWN`, `SHOWDOWN`), `<SUFFIX>` names the mode. The convention is only a convention: the room data gives the full path, and the game type comes from inside the file. Suffixes found:

| Suffix | Room key (`GMS_<key>`) | Mode |
|---|---|---|
| `T` | `TDM` | Team Death Match (shown as Elimination) |
| `TSKILL` | `TDMSKILL` | skill Elimination (Steam only) |
| `M`, `M_OFFICIAL` | `TMM`, `TMM_OFFICIAL` | Search & Destroy, official-room variant |
| `TSV`, `TSV_OFFICIAL` | `TSV`, `TSV_OFFICIAL` | Elimination Pro, official-room variant |
| `CT`, `CM`, `CCTF`, `CTSV` | `CDM`, `CMM`, `CCTF`, `CTSV` | clan Elimination, S&D, CTF, Elimination Pro |
| `CTF` | `CTF` | Capture the Flag |
| `F` | `FFA` | One Man Army |
| `LMS` | `LMS` | Last Man Standing |
| `BNT` | `BNT` | Spy Hunt |
| `QRT` | `QRT` | Quarantine |
| `SBT` | `SBT` | Bombing Run |
| `HQM` | `HQM` | Seize and Secure |
| `VTDM` | `VTDM` | VIP Elimination |
| `VWRM` | `VWRM` | Arms Race |
| `ESL` | `ESL` | S&D for Go4CA Cup |
| `ESC`, `CESC`, `H_ESC` | `ESC`, `CESC` | VIP Escort, clan VIP Escort (`H_ESC` is not referenced by the room data) |
| `S` | `SFM` | Snowball Fight |
| `HG` | `HG` | Hunted |
| `BH`, `BIG`, `BIG_OFFICIAL` | `BIG_OFFICIAL` | Big Head (room-data-only mode, `game_modes.md` M22) |
| `CP`, `SCP` | `COOP`, `SCOP` | Fireteam / Infected Ship / Cabin Fever 2, single-player variant |
| `B` | `BDM` | bot Elimination |
| `RTDM`, `BM`, `B2`, `BOSS`, `LHG`, `LLHG`, `REM`, `TRCM` | various | Steam-only files, contents not read (Open questions) |
| none | `SQM` | `BLOODMONEY.GMS` (Hired Guns) |
| none | `Tutorial`, `BTRN` | `TRAINING.GMS`, `TRAINING_JUNKFLEA_B.GMS` |
| none | none | `CREDITOFFICE.GMS` (Credit Office, `game_modes.md` M21) |

## Format

Mission files and the server file are attribute documents: everything in `attribute_data.md` "Encoding", "Document syntax", "Value types", "Tables, records and lookups" applies unchanged (byte-order mark skipped, `//` and `/* */` comments, sections and keys case-insensitive, first duplicate key wins, strict typed reads, records `Prefix0`, `Prefix1`, … ending at the first gap, list fields `Name0`, `Name1`, …). Shipped mission files rely on comments after values (`TimeLimit = 300 // 5min`) and on float spellings with `f` (`AIDamageFactor = 1.0f`), both of which are valid number reads. Only the differences:

- **Name lists in one string.** Weapon fields and trigger and controller fields hold one or more names separated by `;`, with no spaces trimmed (`"Level01StartTrigger"`, `"XK-2"`). Split on `;`; an empty piece is skipped only when it is the last one; an empty string gives no names.
- **`$` strings** (`MapName = "$GMS_Map_Name_26"`) are keys into the client's string table, kept as is by the server (`attribute_data.md`). Some files carry plain text instead (`"Dark Hills"`).
- **Built-in defaults** are per field, below. The reader never fails on a missing field. It fails the whole file (and the match does not start) only when the document does not parse, or when a mission file (not a mode default file) has no `[MapInfo]` section.
- **Mode default file.** When the reader loads a mission file whose path contains both a `\` and a `_`, it also loads the file named by everything after the last `_`, in the same folder (`GMS\GHOSTTOWN_VWRM.GMS` → `GMS\VWRM.GMS`; `GMS\SNOWVALLEY_M_OFFICIAL.GMS` → `GMS\OFFICIAL.GMS`). That file may be missing or lack `[MapInfo]`; either is silently accepted. Only one table is ever taken from it: the Arms Race weapon ladders (F4.8). VIP Elimination reads `GMS\VTDM.GMS` directly by that fixed name (F4.7), not through this rule.
- **No inheritance** between a mission file and anything else, except the ladder fallback above.

## How a match finds its mission

1. **Room → path.** The lobby gives the server a map index n and a mode (room key k, clan flag, official flag). The server reads `[Map<n>]` → `GMS_<k>` from the room data (the clan mode list supplies k when the room is a clan room; an official room reads `GMS_<k>_OFFICIAL` instead). The value is the mission file's path, case-insensitive (`"GMS\GhostTown_T.GMS"`). A map offers a mode in the lobby only if its record has that `GMS_<k>` field (and the lobby's open-mode flags allow it).
2. **Map cycle instead** (only when the server file's `[MapCycle]` `Enable` is true; never in the stock files): the path is the cycle's `Map<i>` string for the room's cycle index i, and the room data is not consulted (see The server file).
3. **Parse.** The mission file is parsed, then its mode default file (Format). Parse failure or a missing `[MapInfo]` → the match does not start.
4. **Game type.** The game type is taken from **`[Mission0]` `MissionMode`**, not from the room: the string is matched exactly (case-sensitive) against the mode keys of `game_modes.md` "Modes found" (`TDM`, `TMM`, `CTF`, `QRT`, `SQM2`, `COOP_CF`, …). An unknown key gives game type 0 (Single, no reachable rule set). A room key and a file key can therefore disagree, and the file wins (Quirks 1).
5. **World.** `[MapInfo]` `Map` names the world file to load (`"Worlds\GhostTown.dat"`, case-insensitive, `world_dat.md`).
6. **Objective index.** All versus modes play `[Object0]` for every round: the file's `StartObject` is read and ignored. After a round, `<WinnerTeam>Win` of the current objective is looked up (`JSFWin` for a JSF win, `CreedWin` for Creed, `ObserverWin` for a draw, always missing, so −1); a negative value sends the match through intermission (`MissionEnd`) and the mode then decides whether another round starts again at `[Object0]`; a value ≥ 0 would start that objective's setup directly after `ObjectStart` (no stock file uses it). Co-operative modes instead pick the objective by difficulty: index = clamp(G, 0, 2), so `[Object0]` is normal, `[Object1]` hard, `[Object2]` extreme.

## Fields

For each field: name as the data spells it, type, unit, default when missing (or when the stored type is wrong), and the rule that reads it. "GM" = `game_modes.md`.

### F1. `[MapInfo]`

| Field | Type | Unit | Default | Read by |
|---|---|---|---|---|
| `Map` | string | path | empty | world to load (step 5) |
| `Name`, `Comment`, `MaxPlayer` | string, string, integer | – | – | present in every file, read by nothing |

### F2. `[Mission<n>]` (only `[Mission0]` matters)

| Field | Type | Unit | Default | Read by |
|---|---|---|---|---|
| `MissionMode` | string | mode key | empty → game type 0 | game type (step 4) |
| `Controller` | name list | object names | empty | GM S3 step 2: each named object gets an `ON` message when the mission starts |
| `StartTrigger`, `EndTrigger` | name list | object names | empty | each named object gets a `TRIGGER` message at mission start / mission end |
| `VIPSpawn` | string | object name | empty | VIP Escort: a VIP who spawns is moved to the teleport point of this name (first object found), GM M13 |
| `StartObject` | integer | index | 0 | read, never used |
| `AIDefaultTeam` | integer | team | 0 | AI team of AI-assisted modes (Hired Guns), `[Mission0]` only |
| `JSFWeapon`, `CreedWeapon` | string | – | – | **never read** (Quirks 4); the objective weapons are the `[Object0]` fields |
| `Name`, `Comment`, `DefaultMoney`, `KillRewardMoney`, `RoundCounter`, `RoundCounterTrigger` | – | – | – | not read by the rules |

### F3. `[Object<n>]`

Core fields (every mode):

| Field | Type | Unit | Default | Read by |
|---|---|---|---|---|
| `TimeLimit` | integer | s (Quarantine, Spy Hunt: min) | 0 = no limit | round length, see Timers per mode; GM S3 step 4 |
| `TrainingTimeLimit` | integer | s | 0 | warm-up round length, GM S9; 0 → see Timers per mode |
| `OffenceTeam` | string | team short name | team 2 (Creed) | team that wins a detonation and an "objective accomplished" map message; GM M3, M13 |
| `DefenceTeam` | string | team short name | team 1 (JSF) | team that wins an S&D time-out, a defuse and an "objective failed" map message; GM M3 |
| `JSFWeapon`, `CreedWeapon` | name list | weapon `Name`s | empty | objective weapons, below |
| `JSFSpawn`, `CreedSpawn` | integer | spawn area | 0 | stored per team; no versus rule selects spawn points by it (Spawn families and teams) |
| `JSFWin`, `CreedWin` | integer | objective index | −1 | next objective after a win (step 6) |
| `StartTrigger`, `EndTrigger` | name list | object names | empty | `TRIGGER` messages at each round's setup / round end (GM S3 steps 3 and 6) |
| `Controller` | name list | object names | empty | `ON` messages at each round's setup |
| `RoundTrigger` | name list | object names | empty | VIP Escort round triggers |
| `EscapeZoneTrigger`, `TimerTrigger` | string | object names | empty | VIP Escort: the VIP's exit zone and its exit timer object (stock: `EscapeZoneTrigger`, `VipExitTimer`), GM M13 |
| `ActivateTriggerName<i>`, `ActivateTriggerTimeAmount<i>` | string list, number list | object name, s | empty, 0.0 | at round setup each named activate trigger (exactly one object of that name, of the activate-trigger class) gets its use time set to the amount; Seize and Secure flags, GM M10 |
| `JSFComment`, `CreedComment`, `TrainingComment`, `VIPComment`, `ClanRoundStartComment`, `ClanLastRoundComment`, `StartDeuceComment`, `MapName`, `Name` | string | `$` keys | empty | client messages and labels only |
| `LargeMap`, `LargeMapWidth`, `LargeMapHeight`, `LargeMapPoint0_2D`, `LargeMapPoint0_3D`, `LargeMapPoint1_2D`, `LargeMapPoint1_3D` | string, integer, integer, point, vector, point, vector | texture, px, px, px, u, px, u | empty, 0, 0, (0, 0), (0, 0, 0), … | minimap calibration, below |

Objective weapons (S&D, clan S&D, Go4CA Cup, Bombing Run, and every mode that runs the shared hand-out with empty fields):

- At each real round's setup (never in a warm-up), S&D-family modes split `JSFWeapon` on `;` and give each named weapon to **every living player of team 1** who does not already hold it. The same is done with `CreedWeapon`, and its weapons **also go to team 1** (Quirks 3). Each name must be a weapon `Name` from the weapons file (`attribute_data.md`); the stock value is `XK-2`, the bomb (`inventory.md` class 6).
- Bombing Run hands nothing out at setup. When a player picks up the dropped bomb item, that player receives the weapons of **their own team's** field (`JSFWeapon` for JSF, `CreedWeapon` for Creed; both `XK-2`).
- VIP Escort looks the `JSFWeapon` name up but gives nothing to anyone.
- TDM, CTF, Elimination Pro, Spy Hunt, Quarantine and the clan modes run the same hand-out; their stock fields are empty, so nothing is given.

Minimap calibration (client minimap, and the server's map-position bookkeeping): two reference points, each a pixel (x, y) on the `LargeMapWidth` × `LargeMapHeight` image and a world position (X, ·, Z), give the world rectangle the whole image covers:

```math
s_x = \frac{X_1 - X_0}{x_1 - x_0},\quad s_z = \frac{Z_0 - Z_1}{y_1 - y_0},\quad
\text{LeftTop} = (X_0 - x_0 s_x,\ 0,\ Z_0 + y_0 s_z),\quad
\text{RightBottom} = (X_1 + (W - x_1) s_x,\ 0,\ Z_1 - (H - y_1) s_z)
```

Image y grows down while world Z grows "up" on the map. Two points with equal x (or y) divide by zero; no stock file does that.

Co-operative and AI fields (read per objective; the co-op objective is the difficulty, step 6):

| Field | Type | Unit | Default | Read by |
|---|---|---|---|---|
| `LifeCount` | integer | lives | 0 | lives each player has on entering the match, GM M19 |
| `MaxLifeCount` | integer | lives | 0 | cap: a life is added (pickup or script) only if the result stays ≤ this |
| `RecoverRoundTotalHP` | number | HP | 0.0 | co-op health recovery per stage |
| `AIDamageFactor` | number | × | 0.5 | damage factor for AI attacks (server console variable) |
| `EnableAIVisionBlocker` | bool | – | true | AI vision blockers on/off |
| `ScoreFactor`, `RoundScoreFactor<i>` | number, number list | × | 1.0, empty | co-op score scaling; a non-zero per-round entry replaces `ScoreFactor` for round i ≥ 1 |
| `BeginSpecialAIShow`, `LevelCloseTime`, `ShowCoopResultDelayTime`, `CoopSuccessSpecialMark`, `CoopSpySelectRankRate`, `CoopSpySelectTriggerName`, `AlphaWinTriggerName`, `BravoWinTriggerName`, `AlphaWinScore`, `BravoWinScore`, `MissionList<i>`, `MissionGoalPoint<i>` | various | – | 0, 0, 1000 ms, empty, 1.0, empty, …, 0 | co-op scripting (outside this spec) |
| `AIDataTable<n>` section: `Name<i>`, `DamageRate<i>`, `Armor<i>`, `Health<i>` | string, numbers | – | stop at first empty name | per-objective AI overrides |

Hunted fields (`[Object0]` of `_HG` files): `SystemScanPlayTime` (number, s, default 0.0; stock 5.0), `SystemScanDelayTime` (number, s, 0.0; stock 10.0), `MaxMapItemSpawnCnt` (integer, 0; stock 150), `DefaultActItemSpawnCnt` (integer, 0; stock 130), `AddActItemSpawnCntPerPlayer` (integer, 0; stock 2). Read by Hunted's scan and supply drops (GM M15); their use is outside this spec.

### F4. Mode sections

Sections outside `[Object<n>]`, one per file unless numbered.

#### F4.1 Quarantine (`_QRT`)

| Section, field | Type | Unit | Default when the section exists | Stock | Read by |
|---|---|---|---|---|---|
| `[InfectedModelSetting]` `Model`, `Goggle`, `BackPack` | integer | model id, item ids | 0, −1, −1 | 10, −1, −1 | `Model`: read but never used (the model record's infection override decides the look, `game_modes.md` Corrections (2026-10-30) 6); `Goggle`, `BackPack`: the infected's goggle and backpack |
| `[InfectedModelSetting]` `PrimaryWeapon<i>`, `SecondaryWeapon<i>`, `MeleeWeapon<i>`, `ThrowWeapon<i>` | integer lists | weapon ids | empty (−1) | melee 191 only | infected loadout, below |
| `[InfectedHostModelSetting]` same model and weapon fields | as above | | as above | model 10, melee 190 | host loadout |
| `[InfectedHostModelSetting]` `MinHostNum` | integer | players | 2 | 2 | H_min, GM M8 |
| `[InfectedHostModelSetting]` `SelectionRate` | number | fraction | 0.2 | 0.2 | r, GM M8 |
| `[Infected_NoMovementHeal]` `TickTime`, `Heal`, `WaitingTIme` (spelled so) | number | s, HP?, s | 0.0 each | 1.0, 0.1, 3.0 | infected regeneration, GM M8 |
| `[ChangingFXSetting]` `PlayerToInfected`, `PlayerToHost` | string | effect names | empty | `Quarantine_Exp_01` | hit effects |
| `[ExpAndGPSetting]` `QRT_BasicPoint`, `QRT_KillPoint`, `QRT_DeadPoint`, `QRT_InfectPoint`, `QRT_InfectedPoint`, `QRT_HumanWinPoint`, `QRT_InfectWinPoint` | integer | points | 0 each | 20, 3, −1, 2, 0, 10, 5 | per-player points, GM M8 |
| `[DestroyProjectiles]` `Projectile<i>` | string list | object class names | empty | 15 names (grenades, mines, turrets, air strike, mission bomb, explosions, first-aid kit…) | projectiles removed when their owner turns infected |

Infected (and host) loadout: the player's whole weapon list is replaced by the slot-0 entry of each of the four lists in the order primary, secondary, melee, throw, followed by every further entry of each list in the same class order; entries of −1 are skipped. With the stock data an infected carries only weapon 191, a host only 190. Note that the integer lists must be stored as integers; a weapon id written as a string reads as −1 and is dropped.

#### F4.2 Spy Hunt (`_BNT`)

| Section, field | Type | Unit | Default | Stock | Read by |
|---|---|---|---|---|---|
| `[IntelObject]` `TotalNum` | integer | items | 0 | 5 | intel items placed at round start, GM M7 |
| `[IntelObject]` `FileName`, `Skin<i>`, `RenderStyle<i>` | string, string lists | model paths | empty | ground intel model and skin | intel item look |
| `[SpyModelSetting]` `Model`, `Goggle`, `BackPack` | integer | ids | 0, −1, −1 | 6, −1, 20 | model of a spy |
| `[AgentModelSetting]` `Model`, `Goggle`, `BackPack` | integer | ids | 0, −1, −1 | 10, −1, 20 | model of the agent (stage 2, see Open questions) |
| `[AgentModelSetting]` `PrimaryWeapon<i>`, `SecondaryWeapon<i>`, `MeleeWeapon<i>`, `ThrowWeapon<i>` | **string** lists, each converted to an integer | weapon ids | empty | primaries `"94"`, `"86"`, `"87"` | agent loadout, built like the infected loadout |
| `[AgentModelSetting]` `AgentTrigger`, `AgentCameraHoldingTime` | string, integer | object name, s | empty, 0 | `Initiaion_TRG01`, 5 | stage-2 start trigger and camera hold |
| `[BNT_Stage2_Comment]` `Stage2PlayTime` | integer | s | 180 | 180 | stage-2 time, GM M7 |
| `[BNT_Stage2_Comment]` `OperationName`, `HunterComment`, `BountyComment` | string | `$` keys | empty | – | messages |
| `[ChangingFXSetting]` `PlayerToSpy`, `SpyToSpy`, `SpyToAgent` | string | effect names | empty | `BountyFX`, `BountyFX`, `FinalBountyFX` | hit effects |
| `[PreLoadModels]` `Model<i>` | string list | model paths | empty | spy body and face | preloading only |
| `[ExpAndGPSetting]` `DefaultGP`, `DefaultExp`, `GPIndex`, `ExpIndex` | integer | – | 0 | 5, 4, 27, 12 | rewards |

A server-side event configuration can replace the spy and agent model ids; not needed.

#### F4.3 Bombing Run (`_SBT`)

`[DropObject]` `TotalNum` (integer, 0; stock 1), `FileName` (string; stock the sabotage bomb model), `Skin<i>`, `RenderStyle<i>`: the look of the one bomb item dropped on the first shuffled `Sabotage` drop point at each real round start (GM M9, S12). The section shares its storage with `[IntelObject]`; a file with both keeps the one read last (`[DropObject]`).

#### F4.4 Seize and Secure (`_HQM`)

| Field | Type | Unit | Default | Stock | Read by |
|---|---|---|---|---|---|
| `[HeadQuatersMode]` `FlagMoveTime` | number | s | 120.0 | 120.0 | flag rise time, GM M10 |
| `LastOccupyBonus` | integer | points | 50 | 50 | time-out bonus, GM M10 |
| `Abuse_PlayTime_Plus` | number | s | 30.0 | 10.0 | anti-abuse timer = `FlagMoveTime` + this |
| `Abuse_OccupyFlagTime_Plus` | number | s | 5.0 | 5.0 | anti-abuse margin per flag |
| `HQMissionRewardMax` | integer | – | 30 | 30 | rewards |
| `[Object0]` `ActivateTriggerName0/1`, `ActivateTriggerTimeAmount0/1` | F3 | | | `ActivateTrigger_A`, `ActivateTrigger_B`, 5.0, 5.0 | seize use time, GM M10 |

#### F4.5 Hired Guns (`BLOODMONEY.GMS`, mode key `SQM`)

| Field | Type | Unit | Default | Stock | Read by |
|---|---|---|---|---|---|
| `[BullionManage]` `DropBullionNum` | integer | bars | 0 | 80 | bars placed at each real round start on the first N shuffled drop points (GM S12, M11) |
| `BullionAmount` | integer | money | 0 | 100 | value of one placed bar |
| `BaseBankAmount` | integer | money | 0 | 1000 | each player's bank at every real round start |
| `RoundWinBonusAmount` | integer | money | 0 | 500 | added to that start bank for players of the previous round's winning team; also added to their mission points at the round end |
| `KillBonusAmount` | integer | money | 0 | 100 | banked by the killer for each enemy player kill and for each AI kill; also mission points |
| `DeathBonusAmount` | integer | money | 0 | 0 | banked by the victim at each death; also mission points |
| `BullionPropType`, `BullionBagPropType` | string | prop type names | empty | `Bullion`, `BullionBag` | placed bar, dropped pocket bag |
| `SpawnCostAI<i>` | integer list | money | empty | 200 ×4, 500 ×6 | cost to summon AI unit i |
| `MovePenaltyIncPerBullionNumSection<i>`, `MovePenaltyIncPerBullionNum<i>` | range, number list | bars, per bar | empty | [0, 10] 0.0; [11, 25] 0.03; [26, 80] 0.0 | carry penalty, below |
| `[AIRelations]` `AI<i>`, `<a>_Attack_<v>_DamageRate` | string list, number | – | 1.0 | 10 squads, all rates 1.0 | AI-versus-AI damage scale |
| `[Object0]` `TimeLimit`, `TrainingTimeLimit` | F3 | s | | 300, 10 | round and warm-up length |

Carry penalty for n bars in the pocket, with sections [a_i, b_i] (inclusive) and rates r_i: find every section containing n; for the last one found,

```math
P(n) = P(b_{i-1}) + r_i\,(n - b_{i-1}) \quad (i \ge 1), \qquad P(n) = r_0\, n \quad (i = 0), \qquad P(n) = 0 \text{ if no section contains } n
```

Stock: P(10) = 0, P(20) = 0.30, P(25) = 0.45, P(40) = 0.45, P(81) = 0. The value is applied as the player's custom move penalty (`movement.md`; unit in Open questions). `LifeCount 1`, `MaxLifeCount 10` and the co-op score sections in this file are not read by Hired Guns.

#### F4.6 VIP Escort (`_ESC`, `_CESC`)

`[Mission0]` `VIPSpawn` (F2); `[Object0]` `EscapeZoneTrigger`, `TimerTrigger` (F3); `[VIPChangeCharInfo]` `ModelIndex`, `WeaponIndex` (integers, 0) select `[VIPChangeCharModel_<m>]` (`ModelID` 0, `CamoIndex` 0, `GoggleID` −1, `BackpackID` −1, `HUDIndex` −1; stock `ModelID` 87, `HUDIndex` 0) and `[VIPChangeCharWeapon_<w>]` (integer lists `Primary<i>`, `Secondary<i>`, `Melee<i>`, `Throw<i>`; stock primary 91, secondary 0): the VIP's model and loadout. `[VIPEscortModeDialogMsg]` `VIPComment001…`, `EscortComment001…`, `ElimiComment001…` (numbered from 1, three digits) are messages. Stock world-object overrides set the exit timer `VipExitTimer` to `TimeAmount` 60 s with `InTimeCommandTime` 54 and the VIP activate trigger `AT_VIP` to 13 s (F4.10). Generic `[ChangeCharInfo_<n>]` / `[ChangeCharModel_<n>]` / `[ChangeCharWeapon_<n>]` records (same fields) exist for other character swaps.

#### F4.7 VIP Elimination (`GMS\VTDM.GMS`, one file for all maps)

Read when a VIP Elimination match starts; a parse failure is reported to the server application and the match continues with no table.

- `[VIPElimination_Table<t>]`: `TargetPoint` (integer, team points; tables with 0 are skipped) and `VIPNumber1` … `VIPNumber8` (integer, default 0): how many VIPs a team of 1…8 players has. A team of 0 players has 0; more than 8 players use `VIPNumber8`.
- **Table choice:** the first table, in increasing `TargetPoint`, whose `TargetPoint` ≥ G. With no such table, every team has 0 VIPs. Stock: table 0 `TargetPoint` 60 (VIPs 0, 0, 0, 0, 0, 1, 1, 2 for 1…8 players), table 1 `TargetPoint` 1000 (0, 1, 1, 1, 2, 2, 2, 3). Every room G (100…180) uses table 1.
- `[Point_Weight]` `VIPPointWeight0` … `VIPPointWeight3` (integer, read in order until one is missing): points a death is worth by the victim's VIP rank (0 none, 1 copper, 2 silver, 3 gold). Stock 1, 3, 5, 7. A missing weight should be 1.
- **Ranks:** with V VIPs for a team, the team's top V players in its ranking get ranks V, V−1, …, 1 (so the best player is gold only when V = 3); everyone else has rank 0. A death adds the victim's weight to the victim's enemy team (GM M12).

#### F4.8 Arms Race weapon table (`Weapon_Step_Table<t>`, normally only in `GMS\VWRM.GMS`)

| Field | Type | Default | Meaning |
|---|---|---|---|
| `Goal` | integer | 0 | the table is used only when G equals this exactly |
| `ItemNo` | integer | −1 | −1: the default ladder; otherwise the item number of a shop function item that switches the player to this ladder |
| `Percent` | integer | 0 | chance threshold, below |
| `Kill<s>`, `WeaponType<s>` | integer | −1 | step s: the kill count at which it starts, and the weapon class shown for it; the steps end at the first s where either is negative |
| `Set<s>_<j>`, `Set_rate<s>_<j>` | integer | −1 | candidate weapon ids of step s and their weights; candidates end at the first j where either is −1; weight 0 entries are dropped |

- **Loading.** Tables whose `Goal` ≠ G are skipped. If no table is left after that, the tables of the mode default file are used (filtered by G the same way). Every stock per-map Arms Race file has none, so `VWRM.GMS` always supplies them. Several tables may share an `ItemNo`; the first one kept for that `ItemNo` wins.
- **Choosing a weapon** when a player's enemy kills reach a step's `Kill` value exactly (and at spawn, with the current kill count): draw r uniformly from the integers 0…100. If r < `Percent` of the default ladder (80), the weapon is the default ladder's pick for this step; if additionally the player owns a function item whose ladder exists and r < that ladder's `Percent` (50), the pick comes from that ladder instead. Otherwise (r ≥ 80) the player keeps their own equipped weapon of that class.
- **Pick within a step:** draw q uniformly from the integers 0…W (W = sum of the kept weights, inclusive), and take the first candidate whose running weight sum is ≥ q.

```math
P(\text{default ladder}) = \tfrac{80}{101},\qquad P(\text{function ladder} \mid \text{item owned}) = \tfrac{50}{101},\qquad P(\text{own weapon}) = \tfrac{21}{101}
```

- **Next step** shown to a player: the first step whose `Kill` is greater than the current kill count; none left means the last stage (a "last stage" message).
- Stock default ladders (weapon id as stored, weapon named in the file's comment, `WeaponType`):

| G | Steps: kills → weapon (type) |
|---|---|
| 15 | 0 → 325 ACR (0); 4 → 337 Kriss (1); 7 → 35 M60 (3); 10 → 125 R870 (26); 12 → 257 M107CQ (2); 13 → 110 Desert Eagle (4); 14 → 84 M69HE Frag (6) |
| 30 | 0 → 325 ACR; 4 → 255 M417; 7 → 337 Kriss; 11 → 203 MP7; 15 → 35 M60; 18 → 220 RPK; 21 → 295 M3 Super 90; 23 → 125 R870; 25 → 257 M107CQ; 27 → 110 Desert Eagle; 29 → 84 M69HE Frag |
| 45 | 0 → 325 ACR; 4 → 255 M417; 8 → 351 FS2000; 11 → 337 Kriss; 15 → 203 MP7; 19 → 152 PP-19; 22 → 35 M60; 26 → 220 RPK; 29 → 386 MG42; 32 → 295 M3 Super 90; 35 → 125 R870; 38 → 257 M107CQ; 41 → 110 Desert Eagle; 44 → 84 M69HE Frag |

  Every default step has one candidate with weight 100. Function-item ladders exist for items 9062 and 9063 at the same kill steps, with two to five candidates per step.

#### F4.9 Forced loadout (`[WeaponList]`)

`Weapon<i>` (integer list, weapon ids): when present, the player's loadout is replaced by exactly these weapons, each with its default ammo's selection amount and no modifications. Read in Snowball Fight (`_S` files: 100, 101, 102, 103) and in the tutorial (`TRAINING.GMS`: 20, 24, 0, 143). This is the only mission-side loadout restriction; the room's weapon-class restriction is a room setting (GM S2).

#### F4.10 World object overrides (`[WorldObjectData<n>]`)

`ObjectName` (string) and the paired string lists `PropName<i>`, `PropValue<i>` (the shorter list sets the count): when a world object whose `Name` equals `ObjectName` (exact, case-sensitive) is created, each named property is replaced by the value text before the object reads its properties. Stock uses: CTF flag skins (`FlagA` → red flag texture, `FlagB` → blue), VIP Escort timers, Seize and Secure and co-op props.

#### F4.11 Hunted map items

`[EditMapSpawner<n>]` (`MapSpawnerName`, `MapSpawnerMaxCnt`, `AbleRateInfoNo<i>`), `[MapSpawnRateInfo<n>]` (`SelfRate`, `WeaponRateIndex`, `GearRateIndex`, `EtcRateIndex`, `SubWeaponRate`, `SubGearRate`, `SubEtcRate`, `SubMyEquipRate`, `SubNoneRate`), `[MapWeaponRateInfo<n>]` / `[MapGearRateInfo<n>]` / `[MapEtcRateInfo<n>]` (item-number and weight lists). Weighted tables for Hunted's supply drops; their behaviour belongs to a Hunted spec.

#### F4.12 Co-operative missions

`[AISpawnTable<n>]` (keyed by `Round`), `[AIHealthRateRoundTable<d>]` (per difficulty, list `Round<i>`, default 1.0), `[WeaponSpawn<n>]`, `[CooperativeModeNPCNameTable]`, `[CooperativeModeScoreTable]`, `[CooperativeModeDialogMsg]` (`Msg000`…`Msg999`, `Cap000`…`Cap999`), `[CooperativeEndingMessage]`, `[CreditNPCInfo<n>]`, `[FileOverride]` (single-player only). The number of co-op stages is the length of the difficulty's health table minus 1. Outside this spec beyond F3's lives and time.

#### F4.13 Rewards

`[MapExp]`, `[MapMoney]` (`level_correction` number, the multiplier is the stored value − 1.0; `basic_point`, `kill_point`, `death_point`, `mission_point`, `win_team_point`, `lose_team_point`, `capture_point`, `recover_point`, `clan_basicpoint`, `clan_winnerpoint` integers, 0), `[Bonus]` `MinPlayersForSEMBonus` (integer, default 999, stock 12). End-of-match experience and money only; no rule reads them.

## Timers per mode

How each rule set turns the data into its timers. "Setup" is the round setup phase length (GM S3 step 3): "server" = the server file's `ObjectStart`, "0" = forced to 0. Round over and intermission are always the server file's `ObjectEnd` and `MissionEnd` (GM S3 steps 5 and 6). TL = `TimeLimit`, TTL = `TrainingTimeLimit`.

| Mode | Warm-up length | Real round length | Setup |
|---|---|---|---|
| TDM, VIP Elimination | no warm-up | TL if TL ≠ 0, else no timer | 0 |
| clan Elimination (`CDM`) | TTL if > 0, else as a real round | TL if > 0, else no timer | 0 |
| S&D, clan S&D, Go4CA Cup, Elimination Pro, clan Elimination Pro, Hired Guns, Last Man Standing, Hunted, Bombing Run, VIP Escort | TTL if > 0, else as a real round | TL if > 0, else no timer | server |
| CTF | no warm-up | TL × G if TL ≠ 0, else no timer | 0 |
| clan CTF | TTL if > 0, else as a real round | TL if TL ≠ 0 (not × G), else no timer | 0 |
| One Man Army, Arms Race | no warm-up | TL if TL ≠ 0, else no timer | 0 |
| Quarantine | TTL if > 0, else as a real round | TL × 60 if TL > 0, else no timer | 0 |
| Spy Hunt | TTL if > 0, else as a real round | G × 60 if G > 0; else TL × 60 if TL > 0; else no timer | 0 |
| Seize and Secure | TTL if > 0, else 10 s | G × 60 (the server file's `ObjectPlay` is replaced) | server |
| Squad Match 2 | TTL if > 0, else 10 s | max(1, TL) | server |
| Snowball Fight | no warm-up | no timer (TL not read) | 0 |
| co-op modes | none | TL of the difficulty's objective | 0 |

```math
T_{\text{CTF}} = \text{TimeLimit}\cdot G,\qquad T_{\text{Quarantine}} = 60\,\text{TimeLimit},\qquad T_{\text{Spy Hunt}} = 60\,G,\qquad T_{\text{S\&S}} = 60\,G
```

The round timer's value is what the HUD shows (`hud_crosshair.md` D7); a mode with no timer sends none.

## Spawn families and teams

- **The spawn family is fixed by the game type, not by the mission file** (GM S4). Mission data never names a `GameStartPoint` `GameType`. The table, by game type and therefore by the file's `MissionMode`:

| `MissionMode` | `GameStartPoint` `GameType` used | Lists |
|---|---|---|
| `TDM`, `TSV`, `CTSV`, `CDM`, `VTDM`, `SFM`, `BDM` | `Team Death Match` | JSF, Creed |
| `TMM`, `CMM`, `ESL` | `Doobic` | JSF, Creed |
| `CTF`, `CCTF` | `Capturetheflag` | JSF, Creed |
| `FFA`, `LMS`, `VWRM` | `FreeForAll` | one, shuffled |
| `BNT` | `BountyHunter` | one |
| `QRT` | `Quarantine` | one |
| `HG` | `HungerGame` | unlocked = warm-up, locked = rounds |
| `SBT` | `Sabotage` | JSF, Creed |
| `HQM` | `HeadQuaters` (spelled so) | JSF, Creed |
| `SQM` | `SquadMatch` | JSF, Creed |
| `SQM2` | `SquadMatch2` | JSF, Creed |
| `ESC` | `VIPEscort` | JSF, Creed (swapped after a side swap) |
| `CESC` | `ClanVIPEscort` | JSF, Creed |
| `COOP`, `IFS`, `COOP_CF` | any co-op type (`CooperativeMission`, `InfectedShip`, `CabinFever2`) | by `Spawn`, below |

- Versus lists take only points whose `Spawn` is exactly 1 (GM S4). That 1 is a constant of the rules: `JSFSpawn` and `CreedSpawn` (1 in every stock file) are stored but select nothing in versus modes. Co-operative spawns take unlocked co-op points with `Spawn` equal to the spawn area the player's spawn asks for, which is the constant 1 in the respawn path observed (Open questions).
- **Teams.** JSF is always team 1 and Creed team 2 (team data); the mission file cannot swap them. It only says which of them is offence and which defence. Every stock file has `OffenceTeam "JSF"`, `DefenceTeam "Creed"`. A missing or unknown name keeps the built-in default, which is the opposite (offence 2, defence 1). Matching is case-insensitive against the team short names, so `"Observer"` would give team 0.
- Uses of offence and defence: S&D-family time-out winner = defence; detonation (when no winner is set yet) = offence; defuse = defence; the map's "accomplished" / "failed" messages make offence / defence win in TDM, S&D, Elimination Pro, LMS, Quarantine, Spy Hunt, Bombing Run, Seize and Secure, clan modes; VIP Escort's escape and elimination results. TDM, One Man Army and the clan modes only pass the defence team to the clients in the round result.

## The server file

The file GM S2 and S3 refer to. Attribute syntax (Format). Read once when the match is set up, before the mission file.

- **Location by role:** the dedicated server reads `server.cfg` from the folder above its working folder; a match whose game type is Tutorial reads `Tutorial.cfg` from the attributes archive; single-player and bot hosts read the attributes archive's `SERVER.CFG`.
- **Required:** sections `[Server]` and `[CombatMode]`. If the file fails to parse or either section is missing, the match does not start.
- **`[CombatMode]` fields** (all optional; default = the value when missing or of the wrong type):

| Field | Type | Unit | Default | Client copy | Dedicated copy | Read by |
|---|---|---|---|---|---|---|
| `ObjectStart` | number | s | 0.0 | 0 | absent | round setup length in the modes marked "server" above |
| `ObjectEnd` | number | s | 0.0 | 3 | absent | round-over length, every mode |
| `MissionEnd` | number | s | 0.0 | 3 | absent | intermission length, every mode |
| `ObjectPlay` | number | s | 0.0 | 600 | absent | stored by every mode, used by none (Seize and Secure replaces it with G × 60) |
| `MissionTimeLimit` | integer | min | 0 | 30 | absent | match clock nobody reads, GM Quirk 11 |
| `RoundMax` | integer | – | 0 | 100 | 5 | replaces G only when G ≤ 0, and only in non-final builds |
| `MaxPlayer` | integer | players | 0 | 32 | 24 | server capacity |
| `FriendlyFire` | – | – | – | 0 | 0 | not read (always off here; the room decides, GM S8) |
| `MissionLimit`, `ObjectTimeLimit`, `FreezeTime`, `KickRatio` (100), `BanRatio` (100), `StartDelay`, `RoundStart`, `RoundPlay`, `RoundPreend`, `RoundEnd`, `GamePreend`, `GameStart`, `MissionStart`, `GameEnd`, `TDMMap` | various | – | 0 unless noted | various | – | read, change no rule |
| `SpectatorMode` | integer | – | 0 | (the file spells it `SpectorMode`, which is never read) | – | – |
| `MapRatio` | integer | – | 100 | 60 | – | read from the **`MaxPlayer`** key, not from `MapRatio` (Quirks 8) |
| `Loop` | integer | – | the `MapRatio` value | 0 | – | not used |
| `Autobalance` | – | – | – | 1 | – | not read at all |

- **`[MapCycle]`:** `Enable` (bool, default false) and `Map0`, `Map1`, … (strings, mission file paths, read until the first missing index). When enabled, the room's cycle index i picks `Map<i>`; an index equal to the count is still accepted and gives an empty path (Edge cases). The mode and map ids reported to the lobby are then derived from the file's game type.
- `[Server]` holds the host name, password, ids and network settings, and `[GamePoint]`, `[Event…]`, `[WorldCoolTime]` and others hold reward and moderation settings; no mode rule here reads them. Do not copy their values.

## Override order

Lowest priority first; a later layer replaces an earlier one for the quantity named.

| Quantity | 1. built-in default | 2. mission file | 3. server file | 4. room |
|---|---|---|---|---|
| Round length | 0 (no limit) | `TimeLimit` | – | Spy Hunt: G minutes replaces it; CTF: multiplied by G; Seize and Secure: G minutes (instead of `ObjectPlay`) |
| Warm-up length | 0 | `TrainingTimeLimit` | – | – (0 falls back to the round length, or to 10 s in Seize and Secure and Squad Match 2) |
| Round setup, round over, intermission | 0 | – | `ObjectStart`, `ObjectEnd`, `MissionEnd` | – (setup forced to 0 in the modes marked 0) |
| Goal G | – | – | `RoundMax` if G ≤ 0 (test builds only) | room goal |
| Offence / defence | Creed / JSF | `OffenceTeam` / `DefenceTeam` | – | – (clan side swaps act on players, GM M18) |
| Co-op objective (difficulty) | 0 | – | – | clamp(G, 0, 2) |
| Arms Race ladder | none | per-map table with `Goal` = G, else `VWRM.GMS` table with `Goal` = G | – | G selects |
| VIP count table | none | `VTDM.GMS` table with smallest `TargetPoint` ≥ G | – | G selects |
| Friendly fire | off | – | ignored | room setting |
| Which mission file | – | – | `[MapCycle]` when enabled | room map and mode otherwise |

## Stock values

Measured on the older regional client's plaintext mission archive. Every file of a mode has the same values unless an exception is listed; all versus files have `JSFSpawn 1`, `CreedSpawn 1`, `JSFWin −1`, `CreedWin −1`, `OffenceTeam "JSF"`, `DefenceTeam "Creed"`, exactly one `[Object0]`. Spawn family from the previous section. Goal ranges from GM S2.

| Mode (`MissionMode`) | Files | `TimeLimit` | `TrainingTimeLimit` | Round length (default G) | Objective weapons | Spawn family |
|---|---|---|---|---|---|---|
| TDM (`TDM`) | 52 | – | – | none | – | `Team Death Match` |
| VIP Elimination (`VTDM`) | 52 | – | – | none | – | `Team Death Match` |
| clan Elimination (`CDM`) | 52 | – | 10 | none | – | `Team Death Match` |
| Elimination Pro (`TSV`) | 53 (+7 official) | 180 | 10 | 180 s | – | `Team Death Match` |
| clan Elimination Pro (`CTSV`) | 53 | 180 | 10 | 180 s | – | `Team Death Match` |
| S&D (`TMM`) | 16 (+14 official) | 180 | 10 | 180 s | JSF `XK-2` | `Doobic` |
| clan S&D (`CMM`) | 16 | 180 | 10 | 180 s | JSF `XK-2` | `Doobic` |
| Go4CA Cup (`ESL`) | 14 | 180 | 10 | 180 s | JSF `XK-2` | `Doobic` |
| CTF (`CTF`) | 45 | 180 | – | 180 × G = 900 s | – | `Capturetheflag` |
| clan CTF (`CCTF`) | 45 | 600 | 10 | 600 s | – | `Capturetheflag` |
| One Man Army (`FFA`) | 48 | – | – | none | – | `FreeForAll` |
| Arms Race (`VWRM`) | 48 | – | – | none | – | `FreeForAll` |
| Last Man Standing (`LMS`) | 46 | 180 | 10 | 180 s | – | `FreeForAll` |
| Spy Hunt (`BNT`) | 38 | 10 (0 on Cold Seed, Gray Hammer, Sand Hog, Snow Valley) | 10 | G min = 720 s | – | `BountyHunter` |
| Quarantine (`QRT`) | 21 | 3 (min) | 10 | 180 s | – | `Quarantine` |
| Bombing Run (`SBT`) | 9 | 300 | 10 | 300 s | JSF and Creed `XK-2` (on pickup) | `Sabotage` |
| Seize and Secure (`HQM`) | 15 | – | 10 | G min = 720 s | – | `HeadQuaters` |
| Squad Match 2 (`SQM2`, file `HIREDGUNS2_HQM.GMS`) | 1 | 1800 | 10 | 1800 s | – | `SquadMatch2` |
| Hired Guns (`SQM`, `BLOODMONEY.GMS`) | 1 | 300 | 10 | 300 s | – | `SquadMatch` |
| VIP Escort (`ESC`, `CESC`) | 2 + 2 (+1 `H_ESC`) | 180 | 10 | 180 s | JSF `XK-2` (not handed out) | `VIPEscort` / `ClanVIPEscort` |
| Hunted (`HG`, Dark Hills) | 1 | 270 | 20 | 270 s | – | `HungerGame` |
| Snowball Fight (`SFM`) | 6 | – | – | none | forced loadout 100, 101, 102, 103 | `Team Death Match` |
| Fireteam / Infected Ship / Cabin Fever 2 (`COOP`, `IFS`, `COOP_CF`) | 9 (+7 single-player) | per difficulty, below | – | – | – | co-op by `Spawn` |
| bot Elimination (`BDM`) | 8 | – | – | none | – | `Team Death Match` |

Co-op objectives (normal / hard / extreme), `LifeCount` 1 and `MaxLifeCount` 1 in every objective: Black Lung 1500 / 2100 / 2700 s; Cabin Fever 1800 / 2700 / 3600; Costa Recon 2100 / 1500 / 1200; Desert Fox 1200 / 1080 / 900; Nemexis HQ 1200 / 1080 / 900; Nemexis Lab 1200 / 1080 / 900; Sand Hog 1260 / 1080 / 900; Infected Ship (`IFS`) 1235 / 1550 / 2225; Snow Storm (`COOP_CF`) 1800 / 2700 / 3600. `AIDamageFactor` per difficulty, e.g. Desert Fox 0.15 / 0.4 / 0.8, Black Lung 1.0 / 1.0 / 1.0.

Maps and their mission files (older regional client; `MapInfo` world in brackets when the map name differs from the world):

| Map | Modes (suffixes) |
|---|---|
| Brush Wood | BNT CCTF CT CTF CTSV F T TSV VTDM VWRM |
| Cargo Ship, Castle, Junk Flea 2, Mouse Trap | BNT CCTF CT CTF CTSV F LMS QRT T TSV VTDM VWRM |
| City of Wings | BNT CESC CM CT CTSV ESC ESL F H_ESC LMS M T TSV VTDM VWRM |
| Cold Seed | BNT CCTF CT CTF CTSV F HQM LMS S T TSV VTDM VWRM |
| Costa Recon | B BNT CCTF CP CT CTF CTSV F LMS T TSV VTDM VWRM |
| Crystal Tower, Dam | BNT CCTF CT CTF CTSV F LMS T TSV TSV_OFFICIAL VTDM VWRM |
| Dark Forest | BNT CCTF CT CTF CTSV F HQM LMS SBT T TSV VTDM VWRM |
| Dark Hills | HG |
| Death Room, Death Room TR [both Death Room] | B CCTF CT CTF CTSV HQM T TSV VTDM |
| Desert Hole, Graveyard, Rural Estate, Tower Bridge, Last Vegas | BNT CCTF CT CTF CTSV F LMS T TSV VTDM VWRM |
| Downhill | B CCTF CT CTF CTSV HQM QRT T TSV VTDM |
| Dredge | BNT CCTF CM CT CTF CTSV ESL F LMS M M_OFFICIAL SBT T TSV VTDM VWRM |
| Freezer | CCTF CT CTF CTSV F LMS QRT S T TSV VTDM VWRM |
| Ghost Town | BNT CCTF CM CT CTF CTSV ESL F HQM LMS M M_OFFICIAL QRT SBT T TSV VTDM VWRM |
| Grand Canyon, Gray Hammer, Piazza | BNT CCTF CM CT CTF CTSV ESL F LMS M M_OFFICIAL T TSV VTDM VWRM |
| Grave Digger | CCTF CT CTF CTSV F LMS T TSV TSV_OFFICIAL VTDM VWRM |
| Gravity A | CCTF CT CTF CTSV F LMS QRT T TSV VTDM VWRM |
| Gravity B, Overdose | CT CTSV F LMS QRT T TSV VTDM VWRM |
| Hallow Ravine | CCTF CT CTF CTSV F HQM T TSV VTDM VWRM |
| Heavy Metal | BNT CCTF CM CT CTF CTSV F LMS M M_OFFICIAL QRT T TSV VTDM VWRM |
| Hell Gate | CTSV QRT TSV |
| Hired Guns 2 | BNT CT CTSV F HQM (= `SQM2`) LMS QRT T TSV VTDM VWRM |
| Istanbul | CCTF CT CTF CTSV F LMS T TSV VTDM VWRM |
| Junk Flea | B BNT CCTF CT CTF CTSV F LMS QRT T TSV VTDM VWRM |
| Nemexis War | BNT CCTF CM CT CTF CTSV ESL F LMS M M_OFFICIAL SBT T TSV VTDM VWRM |
| Neptune | B BNT CCTF CT CTF CTSV F LMS QRT T TSV VTDM VWRM |
| Oil Rig | B BNT CCTF CESC CT CTF CTSV ESC F LMS QRT T TSV TSV_OFFICIAL VTDM VWRM |
| Pump Jack | BNT CCTF CT CTF CTSV F LMS SBT T TSV VTDM VWRM |
| Rapid Stream | BNT CCTF CM CT CTF CTSV ESL F LMS M M_OFFICIAL T TSV TSV_OFFICIAL VTDM VWRM |
| Rattlesnake | CM CT CTSV ESL F HQM LMS M M_OFFICIAL SBT T TSV VTDM VWRM |
| Road Kill | CCTF CT CTF CTSV HQM S T TSV VTDM |
| Sand Hog | BNT CCTF CM CP CT CTF CTSV ESL F HQM LMS M M_OFFICIAL SBT SCP T TSV TSV_OFFICIAL VTDM VWRM |
| Sector 25 | CCTF CT CTF CTSV F HQM LMS T TSV VTDM VWRM |
| Short Fuse | B BNT CCTF CM CT CTF CTSV ESL F HQM LMS M M_OFFICIAL QRT SBT T TSV TSV_OFFICIAL VTDM VWRM |
| Showdown [`Worlds2\Warehouse.dat`; QRT uses `Warehouse_QRT.dat`] | CT CTSV F LMS QRT T TSV VTDM VWRM |
| Slaughterhouse | BNT CM CT CTSV ESL F LMS M M_OFFICIAL T TSV VTDM VWRM |
| Snowball Fight | S |
| Snow Valley | BNT CCTF CM CT CTF CTSV ESL F HQM LMS M M_OFFICIAL S T TSV VTDM VWRM |
| Square | BNT CCTF CM CT CTF CTSV F LMS M QRT T TSV VTDM VWRM |
| Two Towers | BNT CCTF CT CTF CTSV F HQM LMS QRT T TSV VTDM VWRM |
| Vertigo | BNT CCTF CT CTF CTSV F LMS S T TSV VTDM VWRM |
| Warhead | BNT CCTF CM CT CTF CTSV ESL F LMS M M_OFFICIAL QRT SBT T TSV VTDM VWRM |
| Wave Rider | BNT CCTF CT CTF CTSV F HQM LMS T TSV VTDM VWRM |
| co-op only | Black Lung, Cabin Fever, Desert Fox, Infected Ship, Nemexis HQ, Snow Storm (CP SCP); Nemexis Lab (CP) |

A file existing does not make a mode playable: the room data must name it. The US room data, for example, offers only `TDM`, `TDMSKILL` and `SFM` on Freezer although the CTF and Quarantine files exist.

## Edge cases

- **Map without an entry for the chosen mode.** The room record has no `GMS_<key>` (or no `GMS_<key>_OFFICIAL` for an official room): the path is empty, the parse fails and the match does not start. The lobby never offers such a pair, because a map is listed for a mode only when the field exists. A field naming a missing file (the US room data's `GMS_TDMSKILL` on maps with no `_TSKILL` file, `GMS_Tutorial` naming `Tutorial.GMS` while the file is `TRAINING.GMS`) fails the same way.
- **Mission file without `[Mission0]`:** there is no mode key; treat it as a load failure (the reference has no defined behaviour here).
- **`MissionMode` unknown or wrong case** (`"tdm"`): game type 0, no rule set; treat as a load failure.
- **Field out of range:**
  - `TimeLimit` < 0: modes that test "> 0" run with no timer; TDM, VIP Elimination, One Man Army, Arms Race, CTF and clan CTF, which test "≠ 0", start an already expired timer, so every round ends on the first frame of the live phase.
  - `TimeLimit` stored as a real (`180.0`): wrong type, default 0, no time limit.
  - `TrainingTimeLimit` ≤ 0 or missing in a mode with a warm-up: the warm-up runs for the real round length (Seize and Secure and Squad Match 2: 10 s).
  - `OffenceTeam` equal to `DefenceTeam`: both results go to that team; no check.
  - `JSFWeapon` naming a weapon that does not exist: the reference dereferences a missing weapon (crash). Skip unknown names and log.
  - `VIPNumber<n>` larger than the team: VIPs are capped at the team's player count.
  - A Seize and Secure activate-trigger name matching zero or two objects: the time is not changed.
  - Arms Race with G not equal to any `Goal` (only possible with a non-stock room goal): no ladder, every player keeps their own weapons for the whole match; the last-stage message is never sent.
- **Duplicate keys in a numbered map section.** `[WorldObjectData<n>]` records with the same `ObjectName`, `[AISpawnTable<n>]` with the same `Round`, and `[CreditNPCInfo<n>]` with the same `ID` make the reference reader loop forever (it numbers the next section by the count of distinct keys stored). Our loader: keep the first, continue with n + 1, and warn.
- **Duplicate keys in a room record** (US room data: Snow Valley lists `GMS_SFM` twice): first wins, as in `attribute_data.md`.
- **Map cycle index equal to the cycle length:** accepted by the reference, gives an empty path and a failed start; reject it.
- **Stock files carry fields their mode never reads** (`LifeCount`/`MaxLifeCount` in Hired Guns, `JSFWeapon` in VIP Escort, `TimeLimit` in Spy Hunt under a positive G). Do not invent rules for them.

## Quirks

1. **The file decides the mode, not the room.** The game type is the mission file's `MissionMode`. The US room data's `GMS_SQM` on Hired Guns 2 names `HiredGuns2_HQM.GMS`, whose key is `SQM2`, so a Hired Guns room on that map runs Squad Match 2. Keep: it is the only way Squad Match 2 is reached.
2. **CTF's time scales with the goal, clan CTF's does not.** CTF: 180 × G s; clan CTF: a flat 600 s plus a 10 s warm-up. Keep.
3. **S&D gives `CreedWeapon` to JSF.** The S&D-family hand-out ignores which team's field it is reading and arms every living team-1 player. Stock files leave `CreedWeapon` empty in S&D, so nothing visible happens. Keep (it only matters for custom data).
4. **`[Mission0]` `JSFWeapon` / `CreedWeapon` are never read** (the key name is never built), so per-mission default weapons do not exist. Keep.
5. **`StartObject` is ignored**; every versus round plays `[Object0]`. Keep.
6. **Built-in offence/defence are reversed** relative to every stock file (Creed attacks, JSF defends, when the fields are missing). Keep as the default.
7. **Spy Hunt's `TimeLimit` is dead data** whenever the room goal is positive (always, in live play): the round lasts G minutes. Keep.
8. **Server file `MapRatio` is read from `MaxPlayer`**, `Loop` defaults to it, and `SpectorMode` is misspelled in the shipped file so `SpectatorMode` is always 0. None of them change a rule. Keep or ignore.
9. **The mode default file is chosen by the text after the last `_`**, so official S&D files look for `OFFICIAL.GMS`, and files without a mode default file (all but Arms Race and VIP Elimination) simply get none. Only the Arms Race ladder ever falls back. Keep.
10. **Arms Race's random draws are inclusive** (0…100 against thresholds 80 and 50; 0…W against cumulative weights), so the first candidate of a step is one value more likely than its weight says. Keep.

## Test cases

Values from the older regional client's plaintext mission archive and the US room data, unless marked **[install]**. Install cases were measured on Combat Arms: the Classic, Steam build id 25595979 (the archive directory only; payloads are encrypted). Server file "client copy" means `ObjectStart` 0, `ObjectEnd` 3, `MissionEnd` 3.

| # | Setup | Input | Expected |
|---|---|---|---|
| 1 | Room data `[Map26]` (Ghost Town), public room | mode `TDM` | path `GMS\GhostTown_T.GMS`; file key `TDM` → game type 1; world `Worlds\GhostTown.dat` |
| 2 | Same map | mode `TMM`, official room | path read from `GMS_TMM_OFFICIAL` = `GMS\GhostTown_M_OFFICIAL.GMS`; game type 2 (S&D) |
| 3 | `[Map12]` (Warehouse, shown as Showdown) | mode `TSV` | `GMS\Showdown_TSV.GMS`, world `Worlds2\Warehouse.dat`; spawn family `Team Death Match`: on that world 12 JSF + 12 Creed points (`world_dat.md` case 2) |
| 4 | `[Map12]` | mode `FFA` | `Showdown_F.GMS`; family `FreeForAll`: the 16 `FreeForAll` points of `WAREHOUSE.DAT`, one shuffled list; no time limit; G default 30 |
| 5 | `[Map12]` | mode `TMM` | no `GMS_TMM` field: map not listed for S&D; forcing it gives an empty path → match does not start |
| 6 | Hired Guns 2 room record | mode `SQM` | `GMS\HiredGuns2_HQM.GMS`, `MissionMode` `SQM2` → game type 20 (Squad Match 2), not 19 |
| 7 | Ghost Town TDM, G 100 | round goes live | no round timer (TL missing); setup 0 s regardless of server `ObjectStart` 5 |
| 8 | Ghost Town S&D, server file client copy | match start | warm-up 10 s; then round setup 0 s, round 180 s; JSF players hold `XK-2` from round 1, none in the warm-up; time-out winner Creed (defence) |
| 9 | Ghost Town S&D, server file with `ObjectStart` 5 | each real round | 5 s frozen setup before each 180 s round |
| 10 | Freezer CTF | G 3, 5, 7 | round 540 s, 900 s, 1260 s; no warm-up |
| 11 | Freezer clan CTF (`CCTF`) | G 5 | warm-up 10 s, round 600 s |
| 12 | Ghost Town Quarantine | real round | 3 × 60 = 180 s; warm-up 10 s; hosts H from `MinHostNum` 2, `SelectionRate` 0.2 (GM case 55) |
| 13 | Ghost Town Spy Hunt | G 12 | round 720 s (`TimeLimit` 10 ignored); intel count 5; stage-2 time 180 s |
| 14 | Spy Hunt file, test build | G 0, `TimeLimit` 10 | round 600 s; with `TimeLimit` 0 → no timer |
| 15 | Ghost Town Seize and Secure | G 8, server `ObjectPlay` 600 | round 480 s; warm-up 10 s; both activate triggers' use time set to 5.0 s; flag rise 120 s, bonus 50, anti-abuse timer 130 s |
| 16 | Seize and Secure file without `TrainingTimeLimit` | warm-up | 10 s |
| 17 | S&D file with `TrainingTimeLimit` removed | warm-up | 180 s (the round length) |
| 18 | Dark Hills Hunted | match | warm-up 20 s, rounds 270 s, setup = server `ObjectStart` |
| 19 | Ghost Town Bombing Run | real round start | bomb item on the first shuffled `Sabotage` drop point; a Creed player picking it up receives `XK-2`; round 300 s |
| 20 | Oil Rig VIP Escort | VIP spawns | VIP moved to teleport point `VipTeleportPoint`; VIP model 87, primary weapon 91; nobody receives `XK-2`; round 180 s, warm-up 10 s |
| 21 | Blood Money Hired Guns | real round start after Alpha won the last round | 80 bars of 100 placed; each Alpha player's bank 1500, each Bravo player's 1000; an enemy kill banks +100 for the killer; a death banks 0 |
| 22 | Hired Guns carry penalty, stock sections | 10, 20, 25, 40, 81 bars | 0, 0.30, 0.45, 0.45, 0 |
| 23 | VIP Elimination, `VTDM.GMS` stock | G 140, team of 6 players | table 1 (`TargetPoint` 1000) → 2 VIPs: best player rank 2 (silver, worth 5), second rank 1 (copper, 3), others 1 |
| 24 | Same | G 140, team of 8; team of 1 | 3 VIPs (gold 7, silver 5, copper 3); 0 VIPs |
| 25 | Same, hypothetical G 60 | team of 8 | table 0 → 2 VIPs |
| 26 | Arms Race, any stock map, G 15 | ladder | per-map file has no table → `VWRM.GMS` table 0: steps at 0, 4, 7, 10, 12, 13, 14 kills; at 14 kills the stage is the M69HE frag and is the last stage |
| 27 | Arms Race G 30, player without function item | reaching 4 kills, draws r = 79, r = 80 | r 79 → weapon 255 (M417); r 80 → keeps own equipped weapon |
| 28 | Arms Race G 30, player owns item 9063, r = 49 | 4 kills | weapon from table 4 step 1 (one of 723, 877, 876, 847 by weight) |
| 29 | Arms Race, non-stock G 20 | any kill | no ladder; players keep their own weapons |
| 30 | Desert Fox co-op | G 0, 1, 2, 5 | objective 0 (1200 s, `AIDamageFactor` 0.15), 1 (1080 s, 0.4), 2 (900 s, 0.8), 2 (clamped); every player enters with 1 life, max 1 |
| 31 | Snowball Fight (Snow Valley `_S`) | spawn | loadout exactly weapons 100, 101, 102, 103; no round timer; family `Team Death Match` |
| 32 | Freezer CTF | flag objects created | `FlagA` skin replaced by the red flag texture, `FlagB` by the blue one (world object overrides) |
| 33 | Ghost Town minimap calibration | points (128, 128) ↔ (−5120, 4096), (895, 895) ↔ (5120, −6144), image 1024 × 1024 | s_x = s_z = 10240 / 767 = 13.35072; LeftTop (−6828.892, 0, 5804.892); RightBottom (6842.243, 0, −7866.243) |
| 34 | Missing fields | `[Object0]` without `OffenceTeam`/`DefenceTeam`, S&D time-out | winner JSF (built-in defence 1); a detonation → Creed |
| 35 | `TimeLimit = -1` in a TDM file | round goes live | round over on the next frame |
| 36 | `TimeLimit = 180.0` in an S&D file | round goes live | no timer (wrong type → 0) |
| 37 | Server file without `[CombatMode]` | match start | does not start |
| 38 | Dedicated server's own file (no phase keys) | S&D round end | round over 0 s, intermission 0 s, setup 0 s (GM Open question 3) |
| 39 | Mode default file rule | load `GMS\SnowValley_M_OFFICIAL.GMS` | also tries `GMS\OFFICIAL.GMS`; absent; nothing fails |
| 40 | Two `[WorldObjectData<n>]` with the same `ObjectName` | load | our loader keeps the first, reads the next index, warns (the reference hangs) |
| 41 | **[install]** Steam build 25595979, `GMS.rez` | open | title `V1_RESOURCES_KEY`; one root folder `GMS` with 820 entries; `GHOSTTOWN_T.GMS` is 1,404 bytes and its last 12 bytes (1404 mod 16) are plaintext `onus  = 12` followed by CR LF; `VTDM.GMS` 448 bytes; `SHOWDOWN_T.GMS` 1,390 bytes |
| 42 | **[install]** Steam build 25595979, after V1 decryption | every `.GMS` | parses as an attribute document; every per-map file has `[MapInfo]` and `[Mission0]` `MissionMode`; `GHOSTTOWN_M.GMS` `[Object0]` `TimeLimit` 180, `TrainingTimeLimit` 10, `JSFWeapon` `XK-2` (expected from the older client; unverified, Open questions) |

## Open questions

1. **Steam values.** The Steam mission files are V1-encrypted and could not be read here; they are about 100 bytes larger than the older client's files and 133 files are new (`_BH`, `_RTDM`, `_BM`, `_B2`, `_BOSS`, `_LHG`, `_LLHG`, `_REM`, `_TRCM`, `_TSKILL`, more `_QRT`, `_HG`, `_CP`). Decrypt them and re-measure the Stock values table (test 42), in particular whether TDM gained a `TimeLimit` and what the room-data-only modes of GM M22 carry.
2. **US client values.** The US client's mission payloads are encrypted too; the tables come from an older regional client. `game_modes.md` states the same stock values from "a US-region client", which agrees, but this session did not decrypt the US files.
3. **Co-op spawn area.** The co-op respawn path observed asks for spawn area 1 regardless of the objective's `JSFSpawn`; `game_modes.md` S4 says co-op picks points whose `Spawn` equals "the current spawn area of the objective". Every stock co-op file has area 1, so both readings agree on stock data. Mission scripts may move players by other means; check a co-op map with points of `Spawn` 2.
4. **Agent in Spy Hunt.** Who receives the `[AgentModelSetting]` model and three primaries (the stage-2 final spy, or the stage-2 hunters) was not traced.
5. **Units** of the Hired Guns carry penalty (0.03 per bar: a fraction of speed, or the movement spec's penalty units) and of `Infected_NoMovementHeal` `Heal` (GM Open question 12).
6. **Arms Race weapon ids.** Default-ladder ids (325 ACR, 84 M69HE) look like weapon record indices; function-item ladders use values up to 17133. Whether these are weapon ids or shop item numbers, and how `WeaponType` maps to an inventory slot, belongs to the inventory spec.
7. **`TDMSKILL`, `BIG_OFFICIAL`, `Tutorial` room keys** name files that do not exist in the older client; whether the Steam client resolves them differently is unknown.
8. **Contradictions and refinements of `game_modes.md`:**
   - Constants and S9 say `TrainingTimeLimit` is 10 in every map; Hunted (Dark Hills) uses 20.
   - M11 says Squad Match 2 has no room entry; the US room data reaches it through `GMS_SQM` on Hired Guns 2 (Quirks 1). M11's "Elimination Pro timing" for Hired Guns: its round is 300 s (`TimeLimit` of Blood Money), not Elimination Pro's 180 s; the rest of the Elimination Pro skeleton applies.
   - M18 implies clan CTF uses the public CTF rules; its round is a flat 600 s with a 10 s warm-up, not 180 × G.
   - S2/S3 say only TDM forces the round setup to 0; it is also forced to 0 in clan TDM, CTF, clan CTF, One Man Army, Arms Race, Quarantine, Spy Hunt, Snowball and the co-op modes (Timers per mode).
   - S2 calls `JSFSpawn`/`CreedSpawn` spawn area numbers; in versus modes they select nothing (the `Spawn` 1 filter is a constant).
   - S2 says the per-map data has "one file per map and mode"; true, and two mode-wide files (`VTDM.GMS`, `VWRM.GMS`) hold the VIP table and the weapon ladders.
9. **Contradiction check with `world_dat.md` and `attribute_data.md`:** none. The mission files use the attribute syntax unchanged; they live in their own archive (`GMS.rez`), not in the attributes archive. `world_dat.md`'s Warehouse map is the Showdown map of the room data.
