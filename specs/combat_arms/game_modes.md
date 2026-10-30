# Combat Arms: game modes

Source basis: the dedicated server's match rules (one rule set per game mode, the shared match base, the player's spawn, respawn, damage filter and death handling, the spawn point, bomb zone, flag and flag zone objects), the client's respawn request, death camera, respawn gauge, action gauge and scoreboard, the room data of a US-region data set (mode list, goal ranges, mode names), the per-map mission files of a US-region client, the dedicated server's and client's server configuration files, and object lists read from plaintext maps of that client.
Status: draft

A follow-up session (Open questions 17–36) added the bomb zone, flag, pole, VIP, weapon-race, quarantine, spy-hunt and hunted rule code, the default mission files of those modes, the room-option item table, and object lists from plaintext maps of an older extracted client release.

Sections: Summary; Units and conventions; Modes found; Constants (shared); Shared rules (S1 teams; S2 settings; S3 match phases and clocks; S4 spawn points and spawn choice; S5 respawn; S6 spawn protection; S7 kill counting and assists; S8 friendly fire; S9 warm-up round; S10 end of a round and of the match, leaving players; S11 cleanup at a round's end; S12 item drop points; S13 scoreboard; S14 infected knock-back); Per-tick order; one section per mode (M1 Team Death Match ... M22); Edge cases; Quirks; Test cases; Open questions.

## Summary

Every mode is one server-side rule set on a shared skeleton: a match is a sequence of phases (start, round setup, live, round over, intermission, match over) driven once per server frame, with team scores, round win counters and per-player statistics kept by the server. Teams are fixed by the lobby (JSF = Alpha = 1, Creed = Bravo = 2, free-for-all modes give each player a team of their own). Respawn modes bring a dead player back automatically after a fixed wait; round modes keep the dead out until the next round. What makes the modes feel different is almost entirely in what a death adds to which counter, which condition ends a round, and who wins a time-out.

Team Death Match is shown to players as **Elimination**; the round mode without respawn that the brief calls "Elimination" is shown as **Elimination Pro**. Both are specified (M1, M2).

## Units and conventions

- Distances in LithTech world units (u), Y up, as in `world_dat.md` and `movement.md`.
- Server frame: 20 Hz (the shipped dedicated-server configuration locks the server to 20 frames per second, as `grenades.md` assumes). Every match phase change and every timer test happens in the server's mission update, once per frame, so a timer that expires between frames is noticed on the next frame.
- Timers: phase and round timers count server game time in seconds. The server's respawn-wait check and spawn-protection timer use the OS millisecond clock. The client's respawn wait uses the client's OS millisecond clock.
- "G" below is the room's **goal** value chosen in the lobby (kills, captures, rounds or minutes, depending on the mode). The lobby sends the same G both as the kill target and as the round count, so every mode reads the one value it needs.
- Team ids: 0 none, 1 Alpha (`JSF`), 2 Bravo (`Creed`), 255 observer or game master (never spawns, never counts). Free-for-all players carry a unique id per player, assigned by the lobby (see Open questions).
- **Box sizes** (Open question 30): every `Dims` of a map object (triggers, bomb zones, flag zones, use triggers, pick-up items) is a set of **half-extents** around the object's position, like a player's hull: a bomb zone with `Dims` (133.5, 64, 123.5) spans 267 × 128 × 247 u. The plant test (M3) is |Δx| ≤ 133.5 + 50 and so on.
- Map-side names (`GameStartPoint`, `ObjectSpawnPoint`, `BombZoneTrigger`, `FlagItem`, `FlagZoneTrigger`, `TDMGameControl`, `ActivateTrigger`, their properties) are the data the maps carry, as listed in `world_dat.md`. Per-map mode data (time limits, spawn areas, objective weapons) comes from the map's mission file, one per map and mode, whose field names are given as data.

## Modes found

Every mode the reference knows, by the room key the lobby uses, the game-type number, the name players see (US strings) and where it is specified. "Plays" is the room data's own on/off flag in the US data set.

| Section | Room key | Type | Shown as | Family | Plays (US) |
|---|---|---|---|---|---|
| M1 | `TDM` | 1 | Elimination | team kill race, respawn | yes |
| M2 | `TSV` | 12 | Elimination Pro | team rounds, no respawn | yes |
| M3 | `TMM` | 2 | Search & Destroy | team rounds, bomb, no respawn | yes |
| M4 | `CTF` | 7 | Capture the Flag | team captures, respawn | yes |
| M5 | `FFA` | 6 | One Man Army | free-for-all kill race | yes |
| M6 | `LMS` | 14 | Last Man Standing | free-for-all rounds | yes |
| M7 | `BNT` | 9 | Spy Hunt | intel hunt, two stages | yes |
| M8 | `QRT` | 11 | Quarantine Regen | infection rounds | yes |
| M9 | `SBT` | 15 | Bombing Run | one shared bomb, rounds, respawn | yes |
| M10 | `HQM` | 17 | Seize and Secure | flag raising, respawn | yes |
| M11 | `SQM` | 19 | Hired Guns | bullion rounds with AI, respawn | yes |
| M11 | `SQM2` | 20 | (no room entry) | flag raising variant | no room |
| M12 | `VTDM` | 25 | VIP Elimination | kill race with weighted kills | yes |
| M13 | `ESC`, `CESC` | 23, 24 | VIP Escort (and clan) | escort rounds, sides swap | yes |
| M14 | `VWRM` | 22 | Arms Race | free-for-all weapon ladder | yes |
| M15 | `HG` | 29 | Hunted | free-for-all scavenger rounds | yes |
| M16 | `SFM` | 10 | Snowball Fight | team ticket race | **no** |
| M17 | `ESL` | 21 | S&D for Go4CA Cup | S&D, fixed round total | yes |
| M18 | `CDM`, `CMM`, `CCTF`, `CTSV` | 4, 3, 8, 13 | clan Elimination, S&D, CTF, Elimination Pro | clan variants, sides swap | clan rooms |
| M19 | `COOP`, `IFS`, `COOP_CF` | 5, 26, 27 | Fireteam, Infected Ship, Cabin Fever 2 | co-operative against AI | `COOP` yes |
| M20 | `BDM`, `SCOP`, `BTRN`, `Tutorial` | 28, 5, 30, 16 | Elimination / Fireteam (single player), tutorials | offline | single-player builds |
| M21 | (none) | 18, 0 | Credit Office, Single | no rule set reachable | no |
| M22 | `BIG`, `REM`, `TRCM`, `LHG`, `LLHG`, `TDMSKILL` | 31–36 | Big Head, Race Combat, Team Race Combat, Lone Combat, Lone Combat Infected, Elimination (skill) | room data only | data only |

M1 to M4 are specified in full; the rest give only what differs, at the depth the reference supported.

## Constants (shared)

| Name (ours) | Value | Unit | Meaning |
|---|---|---|---|
| server_rate | 20 | Hz | Server frames per second; all phase changes are tested once per frame |
| team_alpha | 1 (`JSF`) | id | Alpha; HUD red |
| team_bravo | 2 (`Creed`) | id | Bravo; HUD blue |
| respawn_wait_client | 7.0 | s | Client waits this long after death before it asks to respawn (reference edit: 2.0) |
| respawn_wait_server | 5.0 | s | Server refuses a respawn request sooner than this after death (reference edit: 1.5) |
| respawn_wait_server_snowball | 1.0 | s | Same, Snowball Fight |
| respawn_wait_client_snowball | 1.5 | s | Client wait in the Snowball recovery variant (not built; see M16) |
| protect_bullets | 3.0 | s | Respawned player cannot be hit by instant-hit shots (reference edit: 0) |
| protect_all | 4.0 | s | Respawned player takes no damage at all; console variable `RespawnInvulnerabilityTime` (reference edit: 0) |
| protect_client_fx | 3.5 | s | Client shows no blood or flinch on a respawned player |
| occupied_scale | 2.0 | × | A spawn point is occupied when inside another player's box scaled by this |
| ff_scale | 0.25 | × | Teammate damage multiplier with friendly fire on (non-explosive types) |
| assist_share | 0.5 | of max HP | Damage share that earns an assist |
| warmup_time | `TrainingTimeLimit` (10 in every map) | s | Warm-up round length |
| warmup_revive | 5.0 | s | During a warm-up, every dead player is revived this often |
| late_join_wait | 3 | players | Round modes put a joiner on hold only when at least this many are in the game |
| dying_time | 3500 | ms | Death-camera hold, console variable `DyingTime` (Hunted only) |
| flag_return | 30.0 | s | A dropped CTF flag goes home after this |
| bomb_plant_gauge | 4 | s | Plant gauge length on the HUD |
| bomb_defuse_gauge | 7 | s | Defuse gauge length on the HUD; maps use `DefuseTime` 7.0 |
| bomb_fuse | 40 | s | Detonation time the stock S&D zones send when planted (`DetonateTime` 40.0) |
| plant_margin | 50 | u | A planter may stand this far outside the bomb zone's box on each axis |
| phase_round_over | `ObjectEnd`, 3 | s | Round-over hold (3 in the client's copy of the server file; 0 when absent) |
| phase_intermission | `MissionEnd`, 3 | s | Intermission after a round (same caveat) |
| phase_setup | `ObjectStart`, 0 | s | Round setup, players frozen |

## Shared rules

### S1. Teams

- **Two teams** in team modes: Alpha (`JSF`) and Bravo (`Creed`). The names, colours and win sounds come from the team data (`JSF` red (255, 0, 0), `Creed` (109, 207, 246), and an `Observer` entry for id 0). The HUD's "Alpha" row is JSF, "Bravo" is Creed (`hud_crosshair.md` D6).
- **Assignment** is the lobby's: the player arrives with a team id in their connection data and keeps it. The game server has **no auto-balance** (the server file's `Autobalance` is read by no live rule set) and **no team switch** during a match. The only team changes the server makes are mode rules: the clan modes and VIP Escort swap sides at half time (M13, M18), infection and Spy Hunt move players between teams (M7, M8).
- **Free-for-all** modes (M5, M6, M14, M15, Spy Hunt stage 1) give every player a different team id, so every other player is an enemy for damage, kill counting and the HUD's enemy test.
- Id 255 is an observer or game master: never spawned, never counted in team sizes, frozen out of every rule.
- **Joining a running match** (lobby setting "join in progress", room key `EnableJoinCombatSetting`): the joiner spawns at once in respawn modes. In round modes (M2, M3, M17, M18 S&D and Elimination Pro) the joiner is put on hold until the next round when the round is live and at least 3 players are in the game, and in S&D also while a bomb is ticking; a held player is spawned with everyone at the next round setup.
- **Late-join closing**: near the end of a match the server tells the lobby to close the room to joiners: TDM when either team is exactly 10 kills short of G; S&D and Bombing Run when the leading team is exactly 2 round wins short of G; Elimination Pro when 2 or fewer; CTF at 80 % of G captures or 80 % of the time; One Man Army when a player reaches 80 % of G; Seize and Secure in the last 15 % of the time; Quarantine 2 rounds before the end; and whenever a team is left with one player. This affects the lobby only.

### S2. Settings

Two kinds of setting reach the rules: the **room settings** a player picks in the lobby, and the **server file** of the dedicated server. The host-option list inherited from the engine's old multiplayer menu (`NetEndFrags`, `NetEndTime`, `NetEndScore`, `NetFragScore`, `NetIntelScore`, `NetMaxPlayers`, `NetRunSpeed`, `NetRespawnScale`, `NetDefaultWeapon`, `NetWeaponsStay`, `NetHitLocation`, `NetAudioTaunts`, `NetFallDamageScale`, `NetArmorHealthPercent`, `NetDMGameEnd`, `NetCAGameEnd`) is shipped in the data but **no Combat Arms rule reads it**. Implement only the settings below.

Room settings (per mode, from the room data; range = min..max in steps):

| Setting (room data name) | Type | Range / default | Read by |
|---|---|---|---|
| Goal G (`GoalMin`, `GoalMax`, `GoalInc`, `DefaultGoal`) | integer | per mode, table below | each mode's end condition |
| Players (`MinPlayers`, `MaxPlayers`, `PlayersInc`) | integer | per mode, table below | lobby; team sizes |
| Friendly fire (`EnableTeamKillSetting` says whether the room offers it) | on/off | default off | S8 |
| Join in progress (`EnableJoinCombatSetting`) | on/off | lobby default | lobby, S1 |
| Weapon restriction (`EnableWeaponUseSetting`) | primary and secondary class masks | all allowed | loadout (outside this spec) |
| Mutation (`EnableMutationSetting`) | on/off | off | S12 mutation boxes (TDM, One Man Army) |
| Enemy view camera (`EnableViewEnemyCameraSetting`) | on/off | – | death camera (outside this spec) |
| Perks (`EnablePerkSkillSetting`) | on/off | on | perk system (respawn-wait perk, S5) |

Goal ranges in the US data:

| Mode | G means | Min..max step | Default | Players |
|---|---|---|---|---|
| TDM | team kills | 30..140 step 10 | 100 | 2..16 step 2 |
| Elimination Pro | round wins | 5..9 step 2 | 7 | 6..16 step 2 |
| S&D | round wins | 3..9 step 2 | 5 | 2..16 step 2 |
| CTF | captures | 3..7 step 2 | 5 | 4..16 step 2 |
| One Man Army | player kills | 15..45 step 15 | 30 | 2..16 step 1 |
| Last Man Standing | rounds played | 3..9 step 2 | 5 | 2..16 step 1 |
| Spy Hunt | minutes | 8..16 step 4 | 12 | 4..16 step 1 |
| Quarantine Regen | rounds played | 5..9 step 2 | 7 | 8..16 step 1 |
| Bombing Run | round wins | 3..9 step 2 | 5 | 6..16 step 2 |
| Seize and Secure | minutes | 8..16 step 4 | 12 | 6..16 step 2 |
| Hired Guns | rounds played | 3..9 step 2 | 7 | 6..16 step 2 |
| VIP Elimination | team points | 100..180 step 10 | 140 | 2..16 step 2 |
| VIP Escort | round wins | 5..11 step 2 | 7 | 4..16 step 2 |
| Arms Race | player kills | 15..45 step 15 | 15 | 2..16 step 1 |
| Hunted | rounds played | 3..9 step 2 | 5 | 3..16 step 1 |
| Snowball Fight | team tickets | 30..90 step 20 | 50 | 2..16 step 2 |
| Go4CA Cup | rounds played (sum of wins) | 5 | 5 | 8..10 step 2 |
| Fireteam | difficulty 0..2 | 0..2 step 1 | 1 | 8 |

Server file settings (dedicated server, section `CombatMode`):

| Setting | Type | Default when absent | Value in the client's copy | Read by |
|---|---|---|---|---|
| `ObjectStart` | seconds (real) | 0 | 0 | S3 round setup length (TDM forces 0) |
| `ObjectEnd` | seconds (real) | 0 | 3 | S3 round-over length |
| `MissionEnd` | seconds (real) | 0 | 3 | S3 intermission length |
| `StartDelay` | seconds (real) | 0 | 0 | not applied: every mode starts with a 0 s start delay |
| `MissionTimeLimit` | minutes (integer) | 0 | 30 | starts a match clock that no live rule reads, except Seize and Secure, which restarts it and also never reads it |
| `RoundMax` | integer | 0 | 100 | fallback for G only in test builds |
| `FriendlyFire` | on/off | off | 0 | not used: the room setting decides |
| `MaxPlayer` | integer | 0 | 32 | server capacity |
| `MissionLimit`, `ObjectTimeLimit`, `FreezeTime`, `SpectorMode`, `Autobalance`, `KickRatio`, `BanRatio`, `MapRatio`, `Loop`, `RoundStart`, `RoundPlay`, `RoundPreend`, `RoundEnd`, `GamePreend`, `GameStart`, `MissionStart`, `ObjectPlay`, `GameEnd` | various | – | various | read into the rule set but change no rule (`ObjectPlay` is overwritten in Seize and Secure) |

Console variables: `NetFriendlyFire` (server; mirrors the room's friendly-fire choice, S8), `RespawnInvulnerabilityTime` (server; S6), `DyingTime` (client; S5).

Per-map mission data (one file per map and mode), fields by name: `MissionMode` (room key), `TimeLimit` (round length; seconds in most modes, minutes in Quarantine), `TrainingTimeLimit` (warm-up length), `JSFSpawn`, `CreedSpawn` (spawn area numbers, always 1 in the stock maps), `OffenceTeam`, `DefenceTeam` (`JSF` and `Creed` in every stock file), `JSFWeapon`, `CreedWeapon` (objective weapons handed out at round start), `JSFWin`, `CreedWin` (next objective after a win; −1 everywhere, meaning "the round ends the mission"), plus mode sections named in the mode sections below. Stock values: TDM and One Man Army files have no `TimeLimit` (no time limit); S&D, Elimination Pro, CTF and Last Man Standing use `TimeLimit` 180, Bombing Run 300, Quarantine 3 (minutes), VIP Escort 180; every round mode uses `TrainingTimeLimit` 10.

### S3. Match phases and clocks

A match runs through these phases (our names). All transitions are made by the server's mission update, once per server frame.

1. **Waiting.** The server has loaded the map. The first player to arrive (or the first player-info update with a live player) starts the match: the phase becomes **Starting** with a 0 s timer, and (unused) the match clock starts.
2. **Starting → Mission start**, next frame: the mission's controllers are switched on, counters reset.
3. **Round setup** (`ObjectStart` seconds, 0 by default; TDM always 0): every player on team 1 or 2 is respawned (S5; living players too: they are moved to a spawn point and re-equipped), map triggers for the round fire, objective weapons are handed out, and every living player is **frozen** (cannot move). Players who connect during this phase are frozen too.
4. **Live.** Players are unfrozen, the winner is cleared, the round timer starts with the round length (mode table) or is stopped when the mode has none. Kills count only in this phase.
5. **Round over** begins on the first frame where a rule has asked for it (a win condition, or the round timer reaching 0). The mode's time-out rule fills in a winner if none was set, round win counters are updated, end-of-round triggers fire, the result (winner and team scores) is sent to everyone, and a timer of `ObjectEnd` seconds starts. Players keep control, can still move, shoot and die, but nothing they do counts.
6. **Round end** (next frame after the timer): round results and rewards are computed and sent, objective items are dropped and removed, **every body and every weapon item on the ground is removed** (S11), held players are released, per-round statistics are cleared. Then the mission file's next objective for the winner is looked up (`JSFWin` / `CreedWin`); every stock file has −1 there, so **every round ends its mission** and **Intermission** (`MissionEnd` seconds) follows after **every** round, not only the last. At the end of the intermission the match-end test runs: if the match is over, the match result (step 7); otherwise the next **Mission start** (corrected: an earlier reading put the intermission only before the match result; the 6 s formula below already assumed every round; Open question 21). Only a file with a non-negative next objective would skip the intermission and go straight to the next round setup.
7. **Match over**: a reward roulette is played on the clients, then the server pauses and reports the result to the lobby. If every player leaves during the roulette, it ends at once.

```math
t_{\text{between rounds}} = \text{ObjectEnd} + \text{MissionEnd} + \text{ObjectStart} + \text{(frames)} \approx 3 + 3 + 0 = 6\ \text{s with the client's file}
```

The HUD's round timer (`hud_crosshair.md` D7) is fed the round timer's remaining time when the round goes live; TDM sends no time (no timer). Corrected (Corrections, HUD and scoreboard, 2): it is also fed the setup delay at each round setup (hidden when that is 0, the stock value), it is stopped and hidden at the round result, and in S&D and Bombing Run a plant hides it and shows the bomb timer counting the 40 s fuse instead (the round timer's own +40 s, M3, is not what the HUD shows).

### S4. Spawn points and spawn choice

**Which points.** At map load the server builds the spawn lists from the map's `GameStartPoint` objects:

- A point qualifies when its `GameType` string names the running mode's spawn family, `Locked` is false, and **`Spawn` is exactly 1**. Points with any other `Spawn` value are never used in versus modes. Team modes put points with `Teams` = `JSF` in the Alpha list and `Creed` in the Bravo list; `<none>` points are in neither.
- Spawn families (the `GameType` string a mode looks for):

| Running mode | Points it uses |
|---|---|
| TDM, Elimination Pro, Snowball, VIP Elimination, clan TDM, clan Elimination Pro, bot Elimination | `Team Death Match` |
| S&D, clan S&D, Go4CA Cup | `Doobic` |
| CTF, clan CTF | `Capturetheflag` |
| One Man Army, Last Man Standing, Arms Race | `FreeForAll` (one list, team ignored) |
| Spy Hunt | `BountyHunter` (one list) |
| Quarantine | `Quarantine` (one list) |
| Hunted | `HungerGame`: unlocked points form the warm-up list, locked points the round list |
| Bombing Run (M9) | `Sabotage`, per team (Ghost Town: 12 + 12) |
| Seize and Secure (M10) | `HeadQuaters` (spelled so), per team (Ghost Town: 12 + 12) |
| every other mode | its own game-type string, per team (VIP Escort `VIPEscort`, Hired Guns `SquadMatch`, …) |

- One-list modes (`FreeForAll`, `BountyHunter`, `Quarantine`) put every qualifying point in one list whatever its `Teams`. The `FreeForAll` list (and Hunted's two lists) are **shuffled once at map load**; team lists keep the map's object order.
- Co-operative modes pick, at each spawn, unlocked co-op points whose `Spawn` equals the current spawn area of the objective (and the team, when the player has one).
- A `GameStartPoint` can be locked and unlocked at run time by the map's `LOCK` and `UNLOCK` messages. The versus lists are built once, so a later lock does not remove a point from them.
- A typical map (Ghost Town) has 12 + 12 points per team mode and 22 `FreeForAll` points. Spawn points sit about 66 u above the floor; the spawned player is dropped to the floor.

**Which point (versus modes).** Each list has a **cursor**, starting at 0, shared by all players of that list (one cursor per team; free-for-all modes share one cursor for everybody). For a spawn:

1. Advance the cursor by one (wrapping at the end of the list) and test that point.
2. A point is **occupied** when any other player object (alive or dead player objects; bodies are not players) has its centre within twice its own half-extents of the point on every axis (strict): with the fallback hull (15, 50, 15), |dx| < 30, |dy| < 100, |dz| < 30. The half-extents are that player object's **current** physics box: the stand box (15, 50, 15) of every stock body, (15, 40, 15) while fully crouched (in between while crouching, `movement.md`); they come from the body's `LST` / `LC` animation boxes, not from the figure type.
3. If occupied, repeat from 1, at most as many times as the list has points. The last point tested is used even if occupied. The cursor stays where it stopped.
4. The player is placed at the point, faces its `Rotation` yaw, is dropped to the floor, and, if the point has a `Command` with `SendCommandOnRespawn`, that command runs (a map command string, S15, sent with the point as sender, once per spawn on that point). Every `GameStartPoint` of the stock maps read has an empty `Command` and `SendCommandOnRespawn` false.
5. **Telefrag**: every other player whose doubled box contains the new position is killed with infinite crush damage credited to themselves (a suicide, S7). This only happens when every point was occupied, and a spawn-protected player (S6) is immune.

So spawns cycle through a team's points in map order, skipping occupied ones; there is no "farthest from enemies" logic and no randomness for team points. If a list is empty the first `GameStartPoint` of the map is used.

### S5. Respawn

- **Which modes respawn**: TDM, VIP Elimination, clan TDM, CTF, clan CTF, One Man Army, Arms Race, Spy Hunt (stage 1 only), Bombing Run, Seize and Secure, Hired Guns, Squad Match 2, Snowball (tickets), the co-op modes (while lives remain) and bot Elimination. Every other mode keeps the dead out until the next round setup.
- **Automatic, no key.** On death the client starts its wait at once (Hunted only: after `DyingTime` 3.5 s of death camera). When the wait has passed (elapsed > 7000 ms; Snowball also needs its team's ticket count > 0), the client sends one respawn request and switches the camera back. A "fast respawn" item lets the client send at once; the server checks the player still has uses left and spends one. Details: the request carries a "fast" flag only in TDM-type and CTF-type respawn requests (not Snowball); with the flag the server skips the wait test entirely, refuses (as illegal) when the lobby says the player has no uses left (players with the premium pass have unlimited uses and spend none), otherwise spends one, and the player's previous body is removed at once instead of 5 s after the respawn (`characters.md` 10).
- **Server check**: the request is refused (and logged as illegal) if the mode does not respawn, or if fewer than 5.0 s (Snowball 1.0 s) have passed since death on the server's clock. A perk can lower the server limit: the limit is min(5.0, perk value). A refused request is not retried by the client (it has set its "requested" flag), so a refused player stays dead until the next round setup. The reference build carries edits that set the waits to 2.0 s and 1.5 s.
- **Respawn**: the player gets full health and armour, the loadout is rebuilt from scratch (`inventory.md` section 13: picked-up weapons gone, clips full, primary drawn), gear is re-applied, any carried flag, intel or gold is let go first, and the spawn point is chosen as in S4. The previous body is removed.
- At a round setup everyone on teams 1 and 2 is respawned this way, alive or not.

### S6. Spawn protection

At every respawn (including round setups):

- **All damage blocked for 4.0 s** (`RespawnInvulnerabilityTime`), except damage types a mode exempts (Spy Hunt's stage-2 respawn exempts falling out of the world). Explosions, falls and telefrags do nothing.
- **Instant-hit shots are absorbed for 3.0 s** (corrected; Open question 25): the shooter's ray still **stops** at the protected player's spheres (the ray test does not look at protection), and then nothing happens to the player: the shooter's client records no character hit (nothing is reported, no damage, no ding) and makes no blood. The ray's test ended at that player, so the round does not reach whatever is behind; no surface puff, decal or impact sound is made there either (Corrections 2026-10-28, 2). A server-traced impact on a protected player (projectile, bot shot) likewise does nothing. Clients draw no blood and no flinch on a player for 3.5 s after they see the respawn, which is also the window in which their own shots at that player are absorbed (the client uses its own flag, so the effective window is the client's).
- **What ends it early**: firing a weapon ends both parts at once, but only in One Man Army, Arms Race, Last Man Standing and Hunted; everyone is told and the client's no-blood flag clears too. In every other mode a protected player can shoot freely.
- **How it shows**: nothing marks a protected player; the only signs are the hit feedback that does not happen. Who sees what, window by window, is in Corrections (2026-10-28) below.
- The reference build carries edits setting both server times to 0 (no protection), but not the client's 3.5 s window; what that build does is in Corrections (2026-10-28) item 4. This spec uses 3.0 s and 4.0 s.

### S7. Kill counting and assists

The **killer** of a death is the player who dealt the killing damage. A player killed by their own damage (own grenade, telefrag) has themselves as killer; a death with no player damager (fall, world damage) has **no killer**. The server handles the death at the moment of the killing damage: first the death drop (`inventory.md` section 12), then the mode's kill counting, then perks.

Per-player statistics the server keeps for the whole match: **kills** (enemy kills), **deaths**, **suicides**, **friendly kills**, **assists**, plus mode statistics (captures, returns, plants, defuses, mission points, round wins). What a death adds depends on the mode; the table gives the rules for the main modes (Δ for victim V and killer K; "team" = the team counter that decides the mode).

| Death | TDM (M1) | Elim. Pro (M2) | S&D (M3) | CTF (M4) | One Man Army (M5) |
|---|---|---|---|---|---|
| Enemy kill | K kills +1, V deaths +1, **K's team +1** | K kills +1, V deaths +1, K's team round kills +1 | K kills +1, V deaths +1 | K kills +1, V deaths +1, K's team kills +1 (tie-break only) | K kills +1, V deaths +1 |
| Team kill | K friendly kills +1, V deaths +1, **V's enemy team +1** | K friendly kills +1, V deaths ±0 | K friendly kills +1, V deaths ±0 | K friendly kills +1, V deaths +1, enemy team kills +1 (tie-break) | – (no teammates) |
| Suicide (own damage) | V deaths +1, suicides +1, friendly kills +1, **V's enemy team +1** | V deaths ±0, suicides +1, friendly kills +1 | V deaths +1, suicides +1 | V deaths +1, suicides +1, friendly kills +1, enemy team kills +1 (tie-break) | V deaths +1, suicides +1 |
| No killer (fall) | V deaths +1, suicides +1, **V's enemy team +1** | V deaths +1 | V deaths +1 | V deaths +1, suicides +1 | V deaths +1, suicides +1 |
| Every death | – | V's team round deaths +1 | – | V's team deaths +1 (tie-break) | – |

- Deaths during any phase but Live, and during warm-up rounds, add **nothing** at all (no kill, no death, no score) in every mode.
- Kills, deaths and assists are what the scoreboard shows; the team counters are what the HUD's score panel shows.

**Assists** (TDM, VIP Elimination, Elimination Pro, S&D, VIP Escort, the clan versions of these and bot Elimination only):

- For each damager and victim pair the server sums the damage the damager dealt to the victim while the victim is alive. Heals by the victim's enemies reduce the sum; the sum is dropped when it goes below 0.
- When the victim dies from an enemy's damage, every *other* enemy damager whose sum is at least half the victim's maximum health gets one assist, and all sums on that victim are cleared.

```math
\text{assist if}\quad \frac{\sum \text{damage}_{A\to V}}{\text{HP}_{\max}(V)} \ge 0.5,\quad A \ne \text{killer},\ \text{team}(A) \ne \text{team}(V)
```

- A VIP victim (VIP Escort) gives no assists. Healing a teammate for at least half their maximum health, net of the healer's own damage to them, within bursts no more than 1 s apart, is also an assist (medic kits).
- Assists give no team score and no kill.

### S8. Friendly fire

Damage from a player to a player of the same team (or from or to a player with team id 0 or above 254) goes through the friendly-fire filter on the server, unless it is the victim's own damage or crush damage:

- **Friendly fire off** (room default): the damage becomes 0, for every damage type, explosions included.
- **Friendly fire on**, in TDM, S&D, Go4CA Cup, CTF, Snowball, Elimination Pro, clan Elimination Pro, VIP Elimination, VIP Escort and bot Elimination: bullets, melee and other direct damage are scaled by 0.25; explosions, flash, burn, poison and bleeding stay at full damage.
- **Friendly fire on** in any other team mode: full damage.
- **Own damage** (your own grenade) is never filtered: always full damage.
- A teammate hit also makes the victim call out (a "we are a team" voice, a different one after more than 2 such hits).
- Heals work only on teammates.

Details (Open question 26):

- **Order**: the filter runs on the raw damage **before** armour and the hit-zone factor are applied (the victim's damage handling does both afterwards), so a 0.25 teammate rifle hit of 40 enters the armour rules as 10.
- **Who is filtered**: the damager's team is read from the damaging player (or the auto turret, which never hurts its own team at all). A damager with team id 0 is never filtered. Otherwise the filter applies when the teams are equal, or the damager's team id is above 254 (255, observer or game master); an id below 1 cannot occur past the first test. So in free-for-all modes, where every player has a distinct id in 1..254, nobody is filtered.
- **Heals**: damage of the heal kinds (the medic kit's and the first-aid kit's ammo, available in any mode whose loadout allows them) is dropped entirely when the healer is on another team; on a teammate it heals in full whatever the friendly-fire setting. The Quarantine infected's standing regeneration (M8) is not a heal of this kind.
- **The voice**: on the first filtered teammate hit that still does damage above 0 (once; when this re-arms was not traced), an "I am hit" call-out to the victim's team (radius 500); on every filtered teammate hit (except ice-crush), a counter goes up and the victim's team hears "we are a team" (radius 500), or the angrier variant once the counter is above 2. The voice also plays with friendly fire **off** (damage 0), except for burn, poison and infection-poison ammo with friendly fire off, and never for heals. These are team call-outs: only the victim's team hears them (`effects_sounds.md`).

```math
d' = \begin{cases} 0 & \text{FF off} \\ 0.25\,d & \text{FF on, listed mode, type not explosive/flash/burn/poison/bleed} \\ d & \text{otherwise} \end{cases}
```

### S9. Warm-up round

Round modes start with a **warm-up round** (`TrainingTimeLimit`, 10 s in every stock map): S&D, Go4CA Cup, Elimination Pro, Last Man Standing, Hunted, Quarantine, Spy Hunt, Bombing Run, Seize and Secure, VIP Escort and the clan modes.

- It runs like a normal round but nothing counts: no kills, deaths or score; its round result has no winner and does not advance the round number; no objective weapons are handed out (no bombs in S&D); map item drops are skipped.
- In S&D, Elimination Pro, Last Man Standing, Bombing Run and the clan modes, every dead player is revived immediately at the start and then every 5 s.
- When it ends, the first real round follows through the normal round end and intermission.

### S10. End of a round and of the match; leaving players

- **No overtime, no sudden death** in any mode. Ties are resolved by each mode's tie-break list; when the list runs out, the round has no winner (draw) and no win counter moves, except where a mode says otherwise.
- **Order of a round's end** (server): the deciding event sets the winner and asks for the round end within the event; at the next mission update the round-over phase applies the time-out rule (if no winner), adds the win, sends the result; `ObjectEnd` s later the round results go out, items and bodies are removed (S11); then the next round or the match end.
- **Leaving players** (server, before the player is removed, so the leaver is still counted in team sizes):
  - TDM: if the leaver was the last player of their team, the other team wins the round (and so the match) at once.
  - S&D, Elimination Pro: if the leaver was alive and their team's last living player, the other team wins the round. If a team is empty when a round ends, the match ends (M2, M3).
  - CTF: if the leaver was the last of their team, the other team wins at once.
  - One Man Army and Arms Race: if 2 or fewer players (leaver included) remain, the match ends.
  - Others: see the mode sections.
- A leaving flag carrier drops the flag (M4); a leaving intel or bomb carrier drops it.

### S11. Cleanup at a round's end

At the round end phase (S3 step 6, `ObjectEnd` seconds after the deciding event):

- **Every weapon item on the ground is removed**: death drops, swap drops and also the map's own placed weapon items. They do not come back in the next round.
- **Every body is removed.**
- Objective items carried by players (S&D bombs and the like) are dropped and then removed.
- **Live grenades are not removed.** A grenade in flight, resting or released at death keeps its fuse and explodes normally, during round over, intermission or the next round's setup. Its damage applies, but the deaths it causes outside the Live phase count for nothing (S7). A player just respawned at a round setup is protected (S6), so such a blast cannot kill them in the first 4 s. This answers `grenades.md` open question 11.
- Mutation boxes and other timed items expire on their own lifetimes.

### S12. Item drop points (`ObjectSpawnPoint`)

`ObjectSpawnPoint` objects are **drop points for mode items**, not player spawns. A point exists only when its `GameType` names the running mode; the others are deleted at map load. Its `Spawn` property is not read. Stock maps carry 30 `Team Death Match`, 30 `FreeForAll`, 12 to 18 `BountyHunter` and 3 `Sabotage` points (60 in all). Uses:

- **Mutation boxes** (TDM and One Man Army with the room's mutation option on): every 30 s the point list is shuffled and a box is placed on the first N points, where N comes from the box data by player count (6 up to 6 players, then N = player count, up to 16). A box lives 10 s, is picked up by touch, and gives a random buff or nerf chosen by the toucher's team's score lead. What a box gives (Open question 27; data by role, effect keys are percent changes unless said): the toucher's "condition" is looked up first: TDM by the team score gap in percent (lose: −100..−15, even: −15..15, win: 15..100), One Man Army by the toucher's K / (K + D) in percent (0–45, 46–48, 49–100). Each item has one pick weight per condition and the pick is weighted among the ten box items:

| Item | Effect while it lasts | Lasts (s) | Weights lose / even / win |
|---|---|---|---|
| 0 | move speed +20, max stamina +200 | 20 | 10 / 10 / 10 |
| 1 | all spread and recoil values −50 | 20 | 5 / 10 / 20 |
| 2 | damage +20, fire rate +20 | 20 | 10 / 10 / 10 |
| 3 | damage taken −50, max HP +20, heal 100 | 20 | 10 / 10 / 10 |
| 4 | ammunition refilled continuously, fire rate +10 | 20 | 10 / 10 / 10 |
| 5 | max HP +200, max stamina +100 | 20 | 10 / 10 / 10 |
| 6 | instant heal 100 | 1 | 10 / 10 / 10 |
| 7 | damage taken −100 (invulnerable) | 10 | 5 / 10 / 20 |
| 8 | become an infected host: weapon 344, standing heal 10 % of max HP per 1 s after 3 s still | 30 | 5 / 10 / 20 |
| 9 | become the Spy Hunt spy model (model 6, backpack 20) with weapons 94, 86, 87 | 30 | 5 / 10 / 20 |

  (The weights as stored favour the **winning** side with items 1, 7, 8 and 9; this is the data.) Each item plays its effect group and its sound (radius 300) at the toucher. The detailed stat rules of each key belong to `gear_perks.md`.
- **Revenge buff** (TDM, event customisation on; M1): item 10 of the same table, not in boxes: damage +30, fire rate +10 and a "revenge chance" flag, for 30 s.
- **Spy Hunt intel** (M7): 5 intel items on the first 5 shuffled points at round start.
- **Bombing Run bomb** (M9): the bomb on the first shuffled point at each round start.
- **Hired Guns bullion** (M11) and Hunted's supply drops (M15), the same way.

This answers `world_dat.md` open question 3.

### S13. Scoreboard

Per player (rows): rank in the list, level icon, clan emblem, name, **kills**, **deaths**, **assists**, a mode column (mission points, captures, intel, spy kills or similar), ping. The local player's best kill streak, longest life and killer are shown on the death and result screens. Per team: the team score (`alpha_point`, `bravo_point`), the same number as the HUD's score panel.

Team boards list Alpha in the upper list and every other team in the lower list. Sorting (descending unless noted):

| Modes | Order |
|---|---|
| default (TDM, S&D, Elimination Pro, One Man Army, Arms Race, VIP Escort, Snowball, ...) | kills, then fewer deaths |
| CTF | captures, returns, kills, fewer deaths |
| Spy Hunt, Last Man Standing, Seize and Secure | mission points (intel, round wins, flag raises), kills, fewer deaths |
| Quarantine | mission points only |
| VIP Elimination | VIP rank, points, kills, fewer deaths |
| Hired Guns, Squad Match 2 | mission points, kills, fewer deaths |
| Fireteam, Cabin Fever 2 | score, kills, fewer deaths |
| Infected Ship | serum, score, kills, fewer deaths |
| Hunted | mission points, total time hunted, kills |

The scoreboard is re-sorted on every player-info update.

**HUD goal and mission column** (Open question 31; what the reference settles): the HUD's goal text is built from the room goal G by mode: round modes (S&D, Quarantine, Elimination Pro, Last Man Standing, Bombing Run, Hired Guns, Go4CA Cup, VIP Escort, VIP Elimination, Hunted) show "G Rounds" (`IDS_GAMEROOM_ROUND`, US text "%d Rounds"), with "current round / " in front when the HUD passes the round number; kill modes (TDM, One Man Army, Arms Race, Snowball, bot Elimination) "G Kills"; Spy Hunt and Seize and Secure "G Min."; CTF "G Points"; Squad Match 2 and VIP Elimination override with "G Points"; co-op modes the difficulty name. The scoreboard's mode column title is fixed per mode: S&D "Explode / Defuse", Bombing Run "Explode/Defuse", CTF "Thouch Down / Recover" (as spelled), Spy Hunt "Intels / Total", Quarantine "H./I. Survival", Last Man Standing "Last Man Points", Seize and Secure "HQ Point", Hunted "Hunted Points", co-op "Mission"; other modes have none. The S&D column shows the player's plant/detonation and defuse counts. The rest of the HUD layout (objective lines, gauge looks, icons, emblems, result screens) is `hud_crosshair.md`'s and `lobby.md`'s; it was not settled here.

**HUD panel per mode and the round result** (added 2026-10-19; Corrections, HUD and scoreboard): the goal texts above are the string table's entries (`IDS_GAMEROOM_KILL`, `IDS_GAMEROOM_ROUND`, `IDS_GAMEROOM_MIN` with the number filled in, or the number, a space and `IDS_POINTS`), never built by code. Each mode draws its own child of the mode indicator layout (`ui_system.md` Corrections 6: TDM `Eli`, Elimination Pro `Eli_pro`, S&D and Bombing Run `s&d`, CTF `CTF`, One Man Army `Indicator_OMA`, Last Man Standing `LMS`, Spy Hunt `Indicator_SPY`, Quarantine `QRT`, Seize and Secure `HQT`, Hired Guns `Squad`, Squad Match 2 `HG2`, VIP Elimination `VIP_Eli`, VIP Escort `VIP_Esct`, Arms Race `WeaponRace`, Go4CA `ESL`, Hunted `HungerGame`, Snowball `Eli`). In Elimination Pro, S&D and Bombing Run the two row titles show each team's living players. The counters on the panel are the server's team counters (TDM: kills per team; round modes: round wins). At a round's result the scoreboard shows the result banner until the next round's setup message (single-round modes: banner without lists) and the mode panel hides (`ui_system.md` Corrections 1 and 2). Players tied on every sort key come out in no defined order (the sort is not stable): any order is parity.

### S15. Map commands the modes run

Modes drive the map through **command strings** stored in object properties and in the mission file; each is a list of `msg <object> <message>` lines (optionally `delay <s> (…)`), run by the server's command handler with the owning object as sender (Open question 18). Where they come from:

- **Mission file**: each mission and each objective names a `StartTrigger` and an `EndTrigger` (map `Trigger` objects, triggered at mission start/end and objective start/end) and a `Controller` (the map's `TDMGameControl` for that mode). The stock maps keep one `LevelNNStartTrigger` / `LevelNNEndTrigger` pair per mode; a start trigger switches that mode's objects on and every other mode's off (bomb zones `on`/`off` and `lock`/`unlock`, indicator effects `on`/`off`, mode props `(visible 1)`/`(solid 1)`, hide-triggers for other modes, `Camera0 off`), an end trigger moves the shared map camera `Camera0` to a camera point and switches it on (the end-of-round view), defuses any planted bomb (`BombZone_X defuse`, delayed 0.3 s on some maps), switches the zones off and closes doors (`Latch_*` / `*_Shutter on`).
- **`TDMGameControl`**: `MissionStartCommand`, `MissionEndCommand`, `ObjectStartCommand` (stock: `msg memorize trigger;`, which tells the map's breakable props to remember their state for the round reset), `ObjectEndCommand`, `BombPlantedCommand` (stock: empty). Messages it understands from the map: `plantbomb <s>` (a plant: S&D/Bombing Run timer of s seconds; 0 means "already planted", the clients show that text), `detonate` (the bomb went off: the round is won as in M3/M9), `defusebomb` (defused), `OCCUPY` (M10), and mode-specific ones (bot respawn, Hunted's `RandomMapItem`, `SystemScan`).
- **`BombZoneTrigger`** (M3, M9): `PlantedCommand` (stock: `plantbomb 40` to the controller, `lock` this zone, `off` and `lock` the other zone, the green indicators off and this zone's red indicator on), `DetonatedCommand` (stock: `detonate` to the controller and `trigger` the site's explosion trigger, below), `DefusedCommand` (stock: `defusebomb`), `PowerOffCommand` (a plant attempt on a zone that is off: stock `plantbomb 0`), `CommandOn` / `CommandOff` (stock empty). Messages: `ON`, `OFF`, `LOCK` (no more plants here), `DETONATE`, `PLANTBOMB`, `DEFUSE`, `VISIBLE`, `HIDDEN`.
- **`ActivateTrigger`** (M10): `WorkCommand` while a player uses it, `ReleaseCommand` when they stop, `ActivateCommand` when the use completes (stock: lock own trigger, unlock the enemy's, `OCCUPY` to the controller, indicator effects).
- **`GameStartPoint`**: `Command` with `SendCommandOnRespawn` (S4; unused in stock maps). `SoundFX`, `SpecialFX`, `Camera`, doors and props take `on`/`off`/`trigger` and their own messages.

**The bomb's explosion is only these commands**: the site's explosion trigger runs a timed script of the map camera (`moveto` three camera points at about 0.15–0.2 s, 2.45–2.5 s and 4.95–5 s, `on` each time) and turns on six effect objects at the site (about 0.4, 0.5, 2.7, 2.8, 5.2 and 5.3 s; groups `Explode_S` / `Explode_L`, on a few maps `Explode_M`, `Grenade_Dirt`, glass blow-outs, `Nuke_Exp`, `S_Area_onFire`). Of the 13 bomb-site maps read (plaintext maps of an older extracted client release), none sends anything to an `Explosion` object: **the detonation hurts nobody** (M3).

### S16. Random draws

Every random choice of the rules (drop-point shuffles, Quarantine hosts, Arms Race rolls, mutation picks) uses the C library's single process-wide generator (the Microsoft linear congruential one), with no fixed seed: many places reseed it with the clock in seconds just before their draw (drop points, Quarantine hosts, VIP Escort), others with the millisecond tick count. Ranges are inclusive: "a random integer in [0, 100]" has 101 values. Nothing a player can see depends on the exact sequence, except that two draws reseeded in the same second repeat each other; **any uniform generator is parity** (Open question 32).

### S14. Infected knock-back

Grenade (and other) ammo with a knock-back value k (`DamageMovePenalty`) and a duration d (`DamageMovePenaltyDuration`; M67: k = 135, d = 0.4 s, `grenades.md` open question 14) affects **only an infected player** (Quarantine M8, and any other mode where the local player is infected), and only damage from another team. Client side, in the movement update:

1. On such a hit the knock-back timer restarts if d × (1 + s) is longer than what is left of the current one, where s is the victim figure type's stiffness value (`StiffDuration`, 0 for every standard figure). The new k replaces the old one.
2. While the timer runs, the player's movement acceleration for the tick is **replaced** by a horizontal push along the direction of the last damage:

```math
\vec a = \hat u \cdot k^{2}\,(1 - t), \qquad \hat u = \frac{(u_x, 0, u_z)}{\lVert (u_x, 0, u_z) \rVert}
```

   with t the seconds since the timer started (not divided by d), in u/s².
3. Crouching while moving scales the push by 0.25; crouching while not moving stops the player dead (velocity 0) and pushes nothing.

## Per-tick order

Server, one frame (20 Hz):

1. Client messages are handled as they arrive: respawn requests (S5), fire and use (plant, defuse, activate), pickups.
2. Objects update. Damage is applied as it happens; a death runs, in this order: grenade release and death drop (`inventory.md` 12), the mode's kill counting and win test (S7, mode sections), which may set the winner and ask for the round end; the frag is reported to everyone. Flag and bomb objects test touches and timers in their own updates.
3. The mission update runs once: the phase machine advances at most one phase (except where a phase starts the next one directly); a requested round end or an expired round timer enters round over (S3 step 5) and applies the time-out rule.

Deaths that happen in the same frame after a win was requested are still counted while the phase is Live (S3), because the phase only changes in step 3. Client, per rendered frame: the respawn wait and request (S5), knock-back (S14) in the movement update.

## M1. Team Death Match (shown as "Elimination")

- **Teams**: Alpha vs Bravo. **Respawn**: yes (S5). **Rounds**: one. **Time limit**: none (the stock mission files have no `TimeLimit`; the HUD shows no timer).
- **Score**: each team's kill counter, starting at 0. A death adds 1 to the **victim's enemy team**, whoever caused it: enemy kills, team kills, suicides and falls all feed the other side (S7 table).
- **Win**: the first death that brings either counter to G or more ends the round; the team with the higher counter wins. One death adds 1 to one counter, so the two cannot be equal at that moment. The match ends after the round (S3: round over, round end, intermission, match over).
- **Leaving**: last player of a team leaves → the other team wins at once.
- **Map objects**: `GameStartPoint` with `GameType` `Team Death Match`, `Teams` `JSF` / `Creed`, `Spawn` 1; `ObjectSpawnPoint` `Team Death Match` for mutation boxes; a `TDMGameControl` per mission (switched on by the mission file's `Controller`).
- **Round setup** is 0 s (forced): everyone is respawned at their team's points and frozen for one frame.
- **Revenge buff** (only when the event customisation is on): after every 20th team kill in total, every player whose kills / (kills + deaths) ≤ 0.45 gets a revenge buff.

## M2. Elimination Pro (`TSV`; the brief's "Elimination")

- **Teams**: Alpha vs Bravo. **Respawn**: no. **Rounds**: warm-up (S9) then rounds of `TimeLimit` 180 s. **G** = round wins needed.
- **Round win**: the first death that leaves one team with no living player gives the round to the other team. If the death leaves both teams with nobody alive, the victim's enemy team wins.
- **Time-out**: the team with more living players wins; if equal, more round kills (enemy kills this round); if equal, fewer round deaths (every death counts, falls and suicides included); if still equal, no winner and nobody scores.
- **Match end**: checked after each round: a team has G round wins, or a team is empty. Winner: the non-empty team if one is empty, else the team with more round wins, else (equal) **Bravo**.
- **Spawns**: `Team Death Match` points. Dead players spectate until the next round. **Whom the dead may watch** (Open question 35; the same in every no-respawn mode): the death camera first follows the **killer**, if that player is alive and the room's enemy-view option (`EnableViewEnemyCameraSetting`, the client's `Enemydeadcam`, on by default) is on; then, and whenever the watched player dies or the spectator switches, it cycles through the **living players of the spectator's own team** in list order; only when none is left alive, and the option is on, it moves to the living players of the other team. In Last Man Standing and Hunted the team filter is off: any living player. Views are a chase camera behind the target or the target's first-person view (the player's camera mode); there is no free camera for players (game masters follow the team of whoever they watch).
- Joining: held until the next round (S1).

## M3. Search & Destroy (`TMM`)

- **Teams**: JSF attack (`OffenceTeam`), Creed defend (`DefenceTeam`) on every stock map; **sides never swap** (Open question 33: the public S&D rule set has no swap at all; only the clan variants M18 and VIP Escort M13 swap). **Respawn**: no. **Rounds**: warm-up (S9), then rounds of `TimeLimit` 180 s. **G** = round wins needed.
- **Bombs**: at each real round's setup every living JSF player receives the bomb weapon named by the objective's `JSFWeapon` (`XK-2`, the mission-class weapon, `inventory.md` class 6). It is never dropped on death; leftover bombs are removed at the round's end. Order in the setup (Open question 33): everybody is respawned and given their own loadout first (S5), then the round's start triggers run, then the bomb is added to every **living team-1** player who does not already carry one (the hand-out ignores which team the objective names: it always looks at team 1) **without** being drawn (the player keeps the primary in hand), then everyone is frozen. Nothing is handed out in the warm-up.
- **Bomb sites**: `BombZoneTrigger` objects with `GameType` `Doobic`, `DefusableTeam` `Creed`, `CanDefuse` true, `DefuseTime` 7.0, `DetonateTime` 40.0, two per map (`ZoneNumber` 0 and 1, sites A and B), each a box (`Dims` half-extents, about 130 × 64 × 125 u).
- **Plant** (Open question 4, answered): the attacker draws the bomb and holds fire. The bomb's view model plays its fire animation (XK-2: `prefire` 0 ms, then `fire_0` **4000 ms** with `FIRE_KEY` at **3904 ms**, sound keys at 356, 822, 1644, 2534, 2603, 2699, 2822 and 3192 ms) and, while it plays, the client **locks the player's movement** (the player cannot walk, strafe or jump; turning still works). Releasing fire before the key cancels at once: the bomb returns to idle, movement is allowed again, and **nothing is kept**: the next attempt starts the 4 s from 0. At the key the shot is sent; the server checks the planter's position against the site's box widened by 50 u on each axis and, if inside and the site is on, plants **at once** (the site's own disable time is 0; the HUD's 4 s plant gauge is just the animation's length). The site's `PlantedCommand` then tells the mode controller "plant bomb 40": the bomb timer starts at 40 s, the round timer gets 40 s added to whatever is left, and the commands lock this site and switch the other off. Only one bomb can be planted per round. A plant on a site that is off or locked gives every client the "bomb already planted" text.
- **Defuse** (Open question 4, answered): a defender who looks at the site from within its use range holds the use key (or fire) for 7 s (`DefuseTime`; the 7 s gauge counts down with the client's frame time). Releasing the key, leaving the range or looking away ends the attempt and the server keeps **no progress**: every new attempt is sent the full 7 s again. Moving while defusing is allowed as long as the range test holds. A completed defuse wins the round for Creed. Only the defending team (`DefusableTeam`) can defuse.
- **Detonation** (Open question 17): when the bomb timer reaches 0 the round is won by JSF; the planter is credited with a detonation (a mission statistic, reward points) and **nothing else happens to anyone**: the stock sites' detonation commands only win the round and play the explosion show (S15): the map camera `Camera0` switches every client's view to three camera points in turn over about 5 s while six explosion effects play at the site. There is **no blast damage**, no kill, no suicide and no friendly-fire question, whoever stands at the site. The HUD shows "BOMB HAS BEEN DETONATED.", the bomb countdown goes to 0. A detonation or defuse only counts while the round is Live.
- **The planted bomb on the clients** (Open question 17): no bomb model is placed at the site (the code that would attach one is disabled). What players see and hear: the site's red indicator effect (`Mission_RED`) on and both green ones (`Mission_GRN`) off, by the planted command; every client's interface sound `UI_SND\BombPlanted.wav` and the text "BOMB HAS BEEN PLANTED."; a HUD bomb countdown that beeps once per whole second with an interface sound chosen by the time left: `UI_SND\T9COUNT1.wav` above 30 s, `T9COUNT2` above 20 s, `T9COUNT3` above 10 s, `T9COUNT4` above 5 s, `T9COUNT5` above 0 s (`BombTimerSound1..5`); and every client forces its dynamic-light options on (`DynamicLight`, `DynamicLightWorld` = 1) until a defuse restores them (Quirk 12). All clients get the same (no team difference). The defuse sound on the site is the gadget target's defusing loop and its "defused" sound (`GUNS_SND_ETC\T9\DEFUSING.WAV`, `DEFUSED.WAV`, radius 500); a defuse also shows "BOMB HAS BEEN DEFUSED.".
- **Round win by elimination**: the first death that leaves Creed with nobody alive gives the round to JSF, **even with the bomb planted**. A death that leaves JSF with nobody alive gives the round to Creed only if no bomb is ticking; with a bomb ticking the round goes on until it explodes or is defused. A death leaving both teams empty gives the round to the victim's enemy team.
- **Time-out**: Creed (defence) wins. Because a plant adds 40 s to the round timer, the round timer cannot run out before the bomb.
- **Match end** (after each round): a team has G round wins, or a team is empty. Winner: more round wins, else (equal) **Bravo**.
- **Spawns**: `Doobic` points per team.
- Statistics: plants, detonations (the planter is credited), defuses (the defuser is credited).

## M4. Capture the Flag (`CTF`)

- **Teams**: Alpha vs Bravo. **Respawn**: yes (S5). **Rounds**: one. **G** = captures to win. **Time limit**: the objective's `TimeLimit` × G:

```math
T = 180 \times G\ \text{s} \qquad (G = 5 \Rightarrow 900\ \text{s} = 15\ \text{min})
```

- **Objects**: one `FlagItem` per team (`Teams` `JSF` / `Creed`, `GameType` `Capturetheflag`, touch pickup, a box of `Dims` (10, 25, 10)); one `FlagZoneTrigger` per team (`Team`, `LinkedFlag` naming that team's own flag, a box of about 128 × 100 × 128) at the flag's base. Flags are visible only in CTF.
- **Taking**: a living player touching the **enemy** flag (at home or dropped) takes it. The flag disappears from the world while carried.
- **Returning**: a living player touching their **own** flag while it is away from home returns it to its base at once and earns a return (team return count +1, player returns +1). Touching your own flag at home does nothing.
- **Dropping**: when the carrier dies, the flag is dropped at the carrier's last living position, moved down to the floor, and a 30 s return timer starts (the remaining seconds are shown). After 30 s untouched, it goes home with no return credit. If the carrier died from falling out of the world or from crush damage, the flag goes home immediately. A carrier who leaves drops it the same way.
- **Capture**: a carrier of the enemy flag who touches their own team's flag zone **while their own flag is at home** scores: team captures +1, player captures +1, and the carried flag goes home. A capture during the warm-up does not count. Corrected (Open question 34): taking, returning and capturing work in **every** phase, and only the warm-up skips the credit; a capture or return made during round over or the intermission still adds to the team's and the player's counts (and a capture can still reach G and set the winner; Quirk 13).
- **The carried flag** (Open question 34): the carrier gets an attached model on the `1W_N` socket (the backpack socket, `characters.md` 3), zero offset, its own default rotation: `ATTACH_M\flag01_back.ltb` with the **other** team's colour (record `Flag0` for a team-1 carrier, skin `ATTACH_T\CTF_Flag_B.DTX`; `Flag1` for team 2, `CTF_Flag_R.DTX`; render style `RS\EdgeAmp01.ltb`). It exists from the carrier's spawn, hidden, and is shown only while the player holds a flag (checked every update); it is hidden in first person. The flag item itself is invisible while carried.
- **Assists**: CTF gives none (S7: not in the assist list).
- **Win**: the first capture that brings a team to G captures ends the round; that team wins.
- **Time-out**: more captures; else more returns; else more enemy kills (team kill counter, S7 table); else fewer deaths; else draw (no winner).
- **Leaving**: last player of a team leaves → the other team wins.
- **Spawns**: `Capturetheflag` points per team.

## M5. One Man Army (`FFA`)

- Everyone for themselves; each player has their own team id. **Respawn**: yes. **Rounds**: one. **Time limit**: none. **G** = kills.
- **Win**: the first kill that brings a player to G kills ends the round; that player wins. Suicides and falls add a death, not a negative kill.
- **Ending by leaving**: when 2 or fewer players remain (leaver included), the round ends; the remaining player is the winner if the total kills were above 10 % of G (rounded), otherwise nobody.
- **Spawns**: the shuffled `FreeForAll` list, one cursor (S4). **Spawn protection** ends when the protected player fires (S6). Mutation boxes when enabled (S12).

## M6. Last Man Standing (`LMS`)

- Free-for-all, **no respawn**, warm-up then G rounds of 180 s. G counts rounds **played**, not wins.
- **Round win**: when a death leaves at most one player alive, that player (if any) wins the round (round wins +1).
- **Time-out**: no winner.
- **Match winner**: chosen by comparing every other player with the **first player in the server's player list** only (see Quirks). For each test, walking the list: a player beating the first player makes them the candidate and clears a "tie" mark; a player equal to the first player sets the mark. Tests in order: round wins (used only if no tie mark is left and the first player has more than 0 round wins), then kills (same, first player's kills more than 0), then **more deaths** (used if no tie mark is left); else nobody. If 1 player or fewer remain, that player wins.
- Firing ends spawn protection. `FreeForAll` points.

## M7. Spy Hunt (`BNT`)

- **Stage 1** (free-for-all intel hunt, respawn allowed): everybody starts as a hunter (Alpha). At round start 5 intel items are placed on shuffled `BountyHunter` drop points (S12; count from the mission file's intel section). Picking up intel makes the carrier a **spy** with a team of their own (so everyone may shoot them); a spy's death drops their intel.
- **Stage 2**: the player who holds **all** intel becomes the **final spy** (Bravo); every dead player is revived and every living player's health and ammo are refilled; respawn stops. Hunters win by killing the final spy; the final spy wins when no hunter is left alive.
- **Time limit**: G minutes (default 12) for stage 1. Stage 2 (Open question 10, answered): when the final spy is made, every client's view is moved to the map camera at `CameraPoint00` for `AgentCameraHoldingTime` (5 s in the stock files) while the final spy is changed; then the camera goes off, the round timer is **restarted** with `Stage2PlayTime` (180 s; 180 when absent) and sent to every client, and the mission file's `AgentTrigger` runs. **Time-out winner**: the hunters (Alpha) in both stages; at a stage-2 time-out the final spy's detonation (mission) points for the round are cleared.
- **Stage-2 refill**: dead players are respawned (full loadout, S5); living players get full health, full ammunition for **every** carried weapon and their gear re-applied (armour), not health only.
- **Spy looks** (mission file): spies use `SpyModelSetting` (stock: model 6, no goggle, backpack 20), the final spy `AgentModelSetting` (model 10, backpack 20, primary weapons 94, 86, 87); the change effects `BountyFX` (to spy) and `FinalBountyFX` (to final spy); the intel item uses `ATTACH_T\Top_Secret.dtx` (`IntelObject`, `TotalNum` 5).
- Spawns: `BountyHunter` list. Leaving: in stage 2 the last of a side leaving ends the round; in stage 1 the round ends when one player would be left.
- **Intel on the ground** (Open question 30): each intel item has the same pick-up box as the Bombing Run bomb, half-extents (10, 25, 10) u, placed on its drop point and moved to the floor the same way (M9); a dropped intel goes to the carrier's last position, moved to the floor; out of the world or crushed, back to its drop point.

## M8. Quarantine Regen (`QRT`)

- **Teams**: everyone starts human (Alpha). **No respawn** request; dead humans killed by infected become infected. **Rounds**: warm-up then G rounds played, each `TimeLimit` 3 **minutes**.
- **Host selection**: 20 s after the round goes live, H living humans are turned into infected hosts (Bravo), chosen at random:

```math
H = \begin{cases} 1 & N \le 3 \\ \max\big(\mathrm{round}(N \cdot r),\ H_{\min}\big) & N \ge 4 \end{cases} \qquad r = 0.2,\ H_{\min} = 2
```

  (N players on teams, `SelectionRate` and `MinHostNum` from the mission file). If H or fewer humans are alive at that moment, the round is void (no winner).
- **Round win**: when no human is alive, infected win; when no infected is alive (after selection), humans win. **Time-out**: humans win.
- **Per-player points** (mission file): human kills infected +3, infected killed by a human −1, infected infects a human +2, human infected 0; living members of the winning side get +10 (humans) or +5 (infected).
- **Infected**: their own model and melee weapon from the mission file; after 3 s without moving they heal 0.1 every 1 s (`Infected_NoMovementHeal`); they take knock-back (S14).
- **Becoming infected** (Open question 12, answered): a human killed by an infected's attack (the infection damage kind) is turned **at once**, in the same server frame as the kill, after the kill is counted: they change to Bravo, keep their position (dropped to the floor below) and are back alive with full health, as an infected; an effect (`PlayerToInfected` / `PlayerToHost` of the mission file, stock `Quarantine_Exp_01`) and an infection sound play at that spot. There is no wait and no respawn point. A human who dies any other way (fall, own grenade, a human's shot) stays dead until the next round.
- **The infected's look and weapon**: the body becomes the model record's infection override (`InfectedOveridable`, or `HostInfectedOveridable` for a host; for example `Normal` → overrides 4 / 6; the override replaces render styles, armour, hit points and hands, `characters.md` 2); the head piece is the body's attached hair (or the event cap), goggle and backpack those of the mission file's `InfectedModelSetting` / `InfectedHostModelSetting` (stock: none); cosmetics are cleared. The weapons are replaced by the mission file's lists: stock infected melee weapon 191 (`Quarantine_Virus_Hand`), hosts 190 (`Quarantine_Host_Hand`); the server also removes that player's live projectiles of the kinds the mission file lists (grenades, mines, turrets …). A mercenary character uses its own infected model and weapon instead.
- **Host selection** runs only in real rounds, not in the warm-up: the 20 s timer is started when a real round goes live; the warm-up has no infected at all. The H hosts are the first H living players after a shuffle of the player list (S16).
- **The heal's unit**: `Heal` is a **fraction of the infected's maximum hit points** (0.1 = 10 % per `TickTime` 1.0 s), after `WaitingTIme` 3.0 s standing exactly still (any position change restarts the wait), interrupted by any change of hit points (damage) or by the fire posture; the first heal comes at once when the wait ends, then one per tick. A perk can scale it.
- **Match winner**: there is none. The match ends after G rounds played (or when one player or fewer is left) and is reported with no winning team; players are ranked by their mission points (S13).
- Spawns: `Quarantine` list (all `JSF`).

## M9. Bombing Run (`SBT`)

- **Teams**: Alpha vs Bravo, **respawn allowed**. Warm-up, then rounds of `TimeLimit` 300 s. **G** = round wins.
- One bomb (`JSFWeapon` and `CreedWeapon` are both `XK-2`, and the mission file's drop object) spawns at a random `Sabotage` drop point each round. Either team picks it up by touch and plants it at the **enemy's** `BombZoneTrigger` (`GameType` `Sabotage`); it is dropped when its carrier dies.
- **Round win**: detonation → the planting team; defuse → the other team. As in M3, the detonation is only the map's camera and effect show (S15): no blast damage.
- **The bomb on the ground** (Open question 30): a pick-up item with a box of half-extents (10, 25, 10) u, visible and solid to touches only; it is placed on its drop point and moved down to the floor (ray straight down from its centre, up to 10000 u, solid world and objects; centre set 25.1 u above the hit when the gap is larger than 25). A carrier's death drops it at their last position, again moved to the floor; a fall out of the world or crush death sends it back to its drop point at once.
- **Time-out**: **both teams get a round win**.
- **Match end**: a team has G wins or a team is empty; winner: more wins; if equal, the winner stays whatever the last round had (none after a time-out).

## M10. Seize and Secure (`HQM`)

- **Teams**: Alpha vs Bravo, respawn allowed. Warm-up, then **one** round of G minutes.
- A flag pole is created at the map's flag pole position (Open question 24): the position and rotation of the map object named `FlagPolePos` (a plain base-class object; Ghost Town (279, −1466, −79), yaw 180°). The pole is a non-solid, shadow-casting model from the prop table entry `HQ_Flag_Pole` (`PROPS\HeadQuaters\HQ_flagpole01.ltb`), with two sockets `HQ_pole_top` and `HQ_pole_bottom`; the two flags are prop models `HQ_Flag_ALPHA` / `HQ_Flag_BRAVO` (`PROPS\HeadQuaters\HQ_flag01.ltb`, skins `ATTACH_T\CTF_Flag_R.dtx` / `CTF_Flag_B.dtx`, transparent style), created at the top and the bottom socket respectively with the top socket's rotation and moved by the mode as a flag rises. The use triggers stand at the pole (Ghost Town `ActivateTrigger_A` / `_B`, box (48, 179, 48), map `TimeAmount` 10 replaced by the mission file's 5). A team seizes it by using its own `ActivateTrigger` (`ActivateTrigger_A` for JSF, `ActivateTrigger_B` for Creed; the mission file sets the use time to 5 s). Seizing raises that team's flag from the bottom; the other flag disappears.
- **Score**: the holding team gains 1 point per whole second its flag rises.
- **Win**: a flag that reaches the top (`FlagMoveTime` 120 s of uninterrupted holding) wins at once.
- **Time-out**: a flag still rising gets the `LastOccupyBonus` 50 points; then more points wins; else more enemy kills; else fewer deaths; else draw.

## M11. Hired Guns (`SQM`) and Squad Match 2 (`SQM2`)

- **Hired Guns**: Alpha vs Bravo with AI mercenaries, respawn allowed, warm-up then G rounds played (Elimination Pro timing). Bullion bars are dropped on drop points at round start; players carry bullion in a pocket and bank it; a death drops the pocket; kills and deaths pay bank bonuses from the map data. **Round winner**: more bullion banked; else more round kills; else fewer round deaths. **Match winner**: an empty team loses; else more round wins; else Bravo.
- **Squad Match 2**: a Seize and Secure variant (M10 rules) with no room entry in the US data.

## M12. VIP Elimination (`VTDM`)

- TDM (M1) in which a death is worth a number of points that depends on the victim's VIP rank, from the VIP data. The team counter is the sum of points; the first death that brings a team to G **points** ends the round. Personal VIP points are shown. Respawn as in TDM.
- **The VIP data** (Open question 28; the default file `GMS\Vtdm.GMS`): rank weights `VIPPointWeight0..3` = 1, 3, 5, 7 for no rank, bronze, silver and gold. A death adds the **victim's** weight (read before any re-ranking) to the victim's enemy team, as in TDM for every kind of death (enemy kill, team kill, suicide, fall); an enemy kill also adds it to the killer's own VIP points.
- **How many VIPs**: a table chosen by the goal G: the first `VIPElimination_Table` whose `TargetPoint` + 1 is above G (stock: `TargetPoint` 60 and 1000, so every stock goal 100..180 uses the second table). Its `VIPNumber<n>` gives the VIP count for a team of n players (n clamped to 0..8): second table 1 → 0, 2–4 → 1, 5–7 → 2, 8 or more → 3; first table 6 → 1, 7 → 1, 8 → 2, fewer → 0.
- **How a player gets a rank**: each team keeps its players sorted by VIP points (more first), then experience (more first), then name (alphabetical). The top k players of that order are the VIPs, where k is the VIP count above: the first gets rank k, the second k − 1, … (with 3 VIPs: gold, silver, bronze; with one VIP that VIP is only bronze, worth 3). The order and ranks are recomputed for a team whenever its score rises (after a kill counts for it) and when a player joins it; every client is sent the new VIP list.

## M13. VIP Escort (`ESC`, clan `CESC`)

- Rounds of `TimeLimit` 180 s, no respawn. The escort team protects a VIP (teleported to the mission's VIP spawn point; VIPs give no assists) to the exit; the other team eliminates. **Win**: VIP escaped → escort team; VIP killed or escort team eliminated → elimination team; **time-out** → elimination team.
- **Sides swap** every R / 2 rounds (R = G; clan rooms every R / 4), with a 3 s notice; spawns follow the swap. Match: first to G round wins.

## M14. Arms Race (`VWRM`)

- One Man Army (M5) with a weapon ladder: at given kill counts each player's weapon is replaced by the next weapon of the map's weapon table (a random roll chooses between the table and the player's own equipment). First to G kills wins. **No death drops** (`inventory.md` 12). Firing ends spawn protection.
- **The table** (Open question 29): the map's mission file, or, when it has none (every stock map), the default file `GMS\VWRM.GMS`: one `Weapon_Step_Table` per goal G (15, 30, 45) and per optional ticket item (`ItemNo` −1 = everyone; 9062 and 9063 are premium items with their own tables). Each step has a kill count `Kill<i>`, a slot `WeaponType<i>` and up to five weighted weapon ids `Set<i>_<j>` / `Set_rate<i>_<j>` (a weight of 0 removes the entry). The default steps (weapon id, all weight 100):

| G | kills: weapon id |
|---|---|
| 15 | 0: 325 (ACR), 4: 337 (Kriss), 7: 35 (M60), 10: 125 (R870), 12: 257 (M107CQ), 13: 110 (Desert Eagle), 14: 84 (M69HE frag) |
| 30 | 0: 325, 4: 255 (M417), 7: 337, 11: 203 (MP7), 15: 35, 18: 220 (RPK), 21: 295 (M3 Super 90), 23: 125, 25: 257, 27: 110, 29: 84 |
| 45 | 0: 325, 4: 255, 8: 351 (FS2000), 11: 337, 15: 203, 19: 152 (PP-19), 22: 35, 26: 220, 29: 386 (MG42), 32: 295, 35: 125, 38: 257, 41: 110, 44: 84 |

- **When it changes**: at spawn (kill count 0) and each time the player's enemy-kill count **equals** a step's kill count, the player's weapon for that step is chosen: one roll r uniform in 0..100 (101 values); if r < `Percent` of the everyone-table (80), the table's weapon (a weighted pick among the step's entries); otherwise the player's **own equipped** weapon of that slot. If the player owns a ticket item and r is also below that item's `Percent` (50), the ticket table's weapon for the same step is used instead. So the table wins 80 / 101 = 79.2 % of the time. Each player is also told the kill count of their next step; reaching the last step's count (14 / 29 / 44) announces "last stage" to everyone.

## M15. Hunted (`HG`)

- Last Man Standing (M6) with scavenging: players start with little, supply items and weapons drop on drop points, a death-camera hold of `DyingTime` 3.5 s, emotes, the time hunted is tracked and ranks the scoreboard. Spawns: unlocked `HungerGame` points in the warm-up, locked ones in rounds. Firing ends spawn protection.
- **Details** (Open question 36; the mode's fuller rules belong to `game_modes_2.md`):
  - **Start**: every player's loadout is replaced by a single weapon, id 934 (`fist`), with its normal ammunition; nothing else is carried.
  - **Supplies**: the map's command `RandomMapItem <spawner>` (sent by the map's triggers) spawns items from the mission file's tables: the spawners are named map objects (`EditMapSpawner<n>`: a base name and `MapSpawnerMaxCnt` numbered objects); per call `DefaultActItemSpawnCnt` + `AddActItemSpawnCntPerPlayer` × living players objects are picked at random without repeats (stock Dark Hills: 130 + 2 per player, at most 150), and each picks by weight between a weapon (`SubWeaponRate` 45), gear (0), a general item (20), the player's own equipment (5) or nothing (30), then an entry of the matching list. A weapon item lies on the ground with gravity off, moved to the floor, with total ammunition = its maximum × U(0, 0.5) (at least 3) and a clip no larger than that, for the mission file's item lifetime. Nothing spawns in the warm-up.
  - **`DyingTime`**: after death the client holds the death camera 3.5 s before starting its respawn wait (S5); in Hunted's rounds there is no respawn, so it is only the camera hold.
  - **Time hunted**: per round, from the player's round play start until death, or for survivors until the round's time-out or end, added to the player's total (capped at the time the game has run); the total is shown and ranks the scoreboard after mission points.
  - **System scan**: the map can switch a periodic radar scan on (`SystemScan 1`): every `SystemScanDelayTime` (10 s) it would show everyone's position for `SystemScanPlayTime` (5 s), but the periodic loop is disabled in this build and only the "off" message at the round's end remains.

## M16. Snowball Fight (`SFM`)

- Not playable in the US room data. Teams start with G **tickets** each; a respawn costs one ticket of the player's team, and the client does not ask while its team has none. Players are frozen by snowballs instead of killed. The round ends when a team has no tickets and nobody alive; winner: more tickets left, else more players alive. Server respawn wait 1.0 s. No death drops. A "recovery" variant (TDM-style kill race, assists for freezes, 1.5 s client wait) exists in the code but is not built.

## M17. S&D for Go4CA Cup (`ESL`)

- Search & Destroy (M3) except the match ends when the **total** round wins of both teams reaches G (5): a fixed number of rounds. Winner: more wins, else Bravo.

## M18. Clan variants

- Clan Elimination (`CDM`, G 100), clan S&D (`CMM`, G 6), clan CTF (`CCTF`), clan Elimination Pro (`CTSV`): the public rules plus a **side swap at half time**: TDM and CTF when either team reaches half the goal (the round ends, sides swap, play resumes), S&D and Elimination Pro at round R/2 + 1. Clan matches also start with a warm-up.

## M19. Co-operative modes (Fireteam `COOP`, Infected Ship `IFS`, Cabin Fever 2 `COOP_CF`)

- Players against AI on scripted missions. G is the **difficulty** (0 normal ... 2, clamped). Each objective gives every player a number of **lives** (`LifeCount` from the mission data, up to a maximum); a dead player respawns (by request, after the wait) while they have lives left; mission scripts can add lives. The mission fails when no player is left, no living player is left, time runs out or a protected object is destroyed. Spawns: co-op `GameStartPoint` objects by spawn area (S4). Details of the mission scripts are outside this spec.

## M20. Offline and tutorial modes

- **Bot Elimination** (`BDM`) and single-player Fireteam (`SCOP`) exist only in single-player builds: TDM and Fireteam against bots. **Tutorial**: every player has god mode. **Bot Tutorial** (`BTRN`): a scripted bot tutorial. Not needed for the versus slice.

## M21. Listed modes without reachable rules

- **Credit Office** (type 18) has a rule set in the code but no way to start it (the game-type list has no string for it). **Single** (type 0) is the engine's single-player default.

## M22. Room-data-only modes

- Big Head (`BIG`, 31), Race Combat (`REM`, 32), Team Race Combat (`TRCM`, 33), Lone Combat (`LHG`, 34), Lone Combat Infected (`LLHG`, 35) and a skill Elimination (`TDMSKILL`, 36) appear in the US room data with goals and player counts, but the reference code has no game types past 30, so their rules are unknown (Open questions).

## Edge cases

- **Last player of a team leaves**: handled before the player is removed (S10). In TDM and CTF the other team wins at once; in S&D and Elimination Pro only if the leaver was alive and their team's last living player, and the match then ends at the round's end because a team is empty.
- **Two objectives finishing on the same tick** (S&D: defuse completes and the last defender dies; detonation and defuse): the event handled first in the frame sets the winner; every later event that frame sees a winner already set and changes nothing (except TDM, below).
- **TDM, two deaths in one frame after G**: both are counted (the phase is still Live). The second may raise a counter past G (101). The winner is recomputed only if the second counter is strictly higher, so with Alpha at 100 and Bravo going from 99 to 100 Alpha stays the winner.
- **Killing yourself with a grenade after the round ends**: nothing is counted (phase is not Live). In the same frame as the deciding kill it still counts (TDM: one more point for the enemy team).
- **Death during the warm-up**: nothing counts; the player is revived within 5 s in the modes that revive.
- **Respawn request refused** (too early, S5): the client does not retry; the player stays dead until the next round setup. With the client's 7.0 s wait and the server's 5.0 s limit this only happens with a slow or skewed client clock.
- **All spawn points occupied**: the player spawns on the cursor's own point, telefragging anyone unprotected there (S4).
- **S&D, all attackers dead with a bomb ticking**: the round continues; defenders must defuse or lose.
- **CTF, carrier dies in a kill zone** (falling out of the world, crushed): the flag goes home at once.
- **CTF capture while your flag is away**: impossible; you must return it first.

## Quirks

1. **TDM team kills and suicides score for the enemy.** Every death adds a point to the victim's enemy team, so team killing and suicides help the opponents. Keep.
2. **A grenade suicide in TDM counts as a friendly kill too** (killer and victim share a team). Keep.
3. **S&D team kills cost the victim no death**, Elimination Pro team kills and suicides cost no death either; TDM and CTF count them. Keep.
4. **Ties at match end go to Bravo** in S&D, Elimination Pro and Hired Guns (only reachable when a team is empty or round wins are equal at the end). Keep.
5. **Last Man Standing's match winner** compares each player only with the first player in the list, and its third test prefers the player with **more** deaths. With three or more players the "best" player is not reliably found. Keep the behaviour (it is what players saw) but see Open questions.
6. **Seize and Secure's kill tie-break skips the first player in the list.** Keep.
7. **Bombing Run time-outs give a win to both teams**, so a match can end with both teams on G. Keep.
8. **Map-placed weapons vanish after the first round end** (all weapon items are cleared). Keep.
9. **Spawn protection does not stop protected players from shooting** except in the four free-for-all modes. Keep.
10. **No auto-balance and no team switching** during a match. Keep.
11. **The mission clock (`MissionTimeLimit`) starts but ends nothing.** Do not implement a match time limit for TDM. Confirmed (Open question 23): in every versus rule set the clock is only started and stopped; only the co-operative rule sets read it.
12. **A detonation leaves everyone's dynamic lights forced on.** A plant forces each client's `DynamicLight` and `DynamicLightWorld` options to 1 and a defuse restores the player's own values, but the restore after a detonation is never reached, so after a detonated round the lights stay forced on until a later defuse (or the player changes the option). Keep or not (cosmetic); noted.
13. **CTF counts captures and returns outside the Live phase.** Only the warm-up is excluded (M4). A capture during the 3 s round-over hold still adds to the score. Keep.
14. **The S&D bomb always goes to team 1.** The hand-out names the objective's team but gives the weapon to every living team-1 player; harmless on stock maps (JSF always attacks). Keep.
15. **The mutation box weights favour the winners** for the strongest items (the table in S12). Keep (data).

## Corrections (2026-10-19): HUD and scoreboard

Only the HUD and scoreboard parts of this spec (S3's HUD timer sentence, S13), checked against the reference's mode HUD panels, scoreboard and string use, and against our implementation. Test cases 87 to 91.

1. **The goal text is the string table's, per mode.**
   - Reference: S13's list was right; the texts are the string-table entries `IDS_GAMEROOM_KILL`, `IDS_GAMEROOM_ROUND`, `IDS_GAMEROOM_MIN` (formats filled with G), or G, a space and `IDS_POINTS` (CTF, Squad Match 2, VIP Elimination).
   - Ours: `games/combat_arms/src/ui/hud/score.rs:109-118` writes its own lower-case English units: S&D, Elimination Pro, Bombing Run and VIP Escort "wins" (should be the rounds text), CTF "captures" (points), Squad Match 2 "min" (points).
   - Why: the goal text sits in the top-centre panel all match long.
2. **The HUD timer in round modes** (S3).
   - Reference: setup delay countdown at each round setup (hidden at 0), the round length when live, hidden at the result; in S&D and Bombing Run the plant hides the round timer and shows the 40 s bomb timer in its own control.
   - Ours: `score.rs:200-205` shows the live round timer only, from the server's round end, so after a plant it shows remaining + 40 s. See `hud_crosshair.md` Corrections 8.
   - Why: the defuse race is timed off the top-centre clock.
3. **Round modes show living players, not team names.**
   - Reference: Elimination Pro, S&D and Bombing Run fill the panel's row titles with each team's living-player count.
   - Ours: `games/combat_arms/src/ui/hud/bound.rs:24-31` hides them and writes "Alpha" / "Bravo" (`score.rs:192-193`). See `hud_crosshair.md` Corrections 9.
4. **Each mode's own indicator panel.**
   - Reference: the table in S13 (and `ui_system.md` Corrections 6).
   - Ours: `bound.rs:202-219` gives Squad Match 2 `Squad`, and Bombing Run, VIP Escort and Hired Guns `Eli`.
5. **The round result on the scoreboard.**
   - Reference: shown from the result until the next round's setup message, banner only in single-round modes, the mode panel hidden meanwhile (`ui_system.md` Corrections 1 to 3).
   - Ours: `games/combat_arms/src/ui/scoreboard.rs:47` 3000 ms; lists always drawn; panel kept.

## Corrections (2026-10-22): match flow, scoring and leaving

A second comparison of the reference's rule sets (match phases, death counting, leaving, joining, assists) with our implementation after the modes moved onto the shared runtime (`chimera/sim/src/mode/flow.rs`, `games/combat_arms/src/game/rules/`). Most-noticed first. Paths below are relative to `games/combat_arms/src/game/rules/` unless given in full. Where a correction changes a rule stated above, the rule above is the older reading; the correction wins.

1. **The match-end test also runs after the warm-up.**
   - Reference: at the end of the intermission that follows the warm-up, the round modes run the same "is the match over" test as after a real round, minus the round-win goal: Elimination Pro, S&D, Go4CA Cup, Bombing Run, Seize and Secure, Squad Match 2, VIP Escort and Hired Guns end the match when a team has no players (winner as each mode's match-end rule: Elimination Pro the non-empty team; S&D, Go4CA, VIP Escort, Hired Guns more round wins else Bravo; Bombing Run more round wins else none); Last Man Standing and Hunted end it when one player or fewer is left (that player wins); Quarantine ends it when one player or fewer is on a team (no winner). The same holds after every later warm-up (VIP Escort's and the clan modes' post-swap warm-ups).
   - Ours: `chimera/sim/src/mode/flow.rs:302` never calls the mode's match end after a warm-up, so the first real round is played out with an empty team (Elimination Pro and S&D: nobody dies, the round runs its 180 s to the time-out; Last Man Standing: a lone player plays G rounds).
   - Expect: Elimination Pro 1 v 1, the Bravo player leaves at t = 4 s of the warm-up → the warm-up ends at the next frame (correction 2), round over 3 s, intermission 3 s, then match over at t ≈ 10 s with Alpha the winner; no 180 s round is played. Last Man Standing with 2 players, one leaves in the warm-up → match over after round over + intermission, the remaining player the winner.

2. **Leaving in Elimination Pro and S&D also counts in the warm-up; Elimination Pro ends the round when a team's last player leaves.**
   - Reference: the leaving rule has no phase or warm-up test. Elimination Pro: the leaver being the only player of their team asks for the round's end at once, alive or dead (the time-out rule then picks the other team, which has living players); otherwise a living leaver who was their team's last living player gives the round to the other team. S&D and Go4CA: only the second part (alive, last living). A request made during the warm-up ends the warm-up early (no winner, as every warm-up); one made during setup or the intermission has no effect (the next live phase clears it).
   - Ours: `elimination.rs:152` acts only when live, not in the warm-up, and only for a living leaver.
   - Expect: S&D warm-up at t = 3 s, the only living Creed player leaves while a Creed teammate is dead awaiting the 5 s revive → warm-up over at the next frame; then the normal round over, intermission and round 1. Elimination Pro real round, the last Bravo player (dead) leaves → round over at the next frame, Alpha wins the round (more living players).

3. **Late joiners: who counts, and which modes hold.**
   - Reference: the hold test counts every player on the server **including the joiner** (and observers and game masters): the joiner is held when that count is 3 or more, the round is live, it is not the warm-up and at least one whole second of the live round has passed (the elapsed time is truncated to whole seconds and must be above 0). S&D and Go4CA also hold while a bomb ticks, but only past the same "3 or more" test. Modes that hold this way: Elimination Pro (and clan), S&D (and clan), Go4CA, Last Man Standing, Hunted, VIP Escort. Never held: TDM, VIP Elimination, CTF, One Man Army, Arms Race, Spy Hunt, Bombing Run, Seize and Secure, Squad Match 2, Hired Guns. Quarantine: correction 4.
   - Ours: `flow.rs:327-331` counts the players already in the match without the joiner (`in_game >= late_join_wait`), holds from the first live instant, and the bomb hold (`search_destroy.rs:104`) skips the count.
   - Expect: S&D round live for 30 s with 2 players in; a third joins → held until the next setup (3 counted). With 1 player in, a second joins → spawns at once. A joiner 0.6 s after the round went live → spawns at once. This replaces test case 33's second sentence ("With only 2 players in the game → spawns at once" is wrong: the joiner makes 3).

4. **Quarantine holds joiners once the hosts are chosen, whatever the player count.**
   - Reference: in a real round a joiner is held unless the 20 s host-selection wait is running; the player count does not matter. So joiners during the first 20 s of a live round play at once (as humans); afterwards they wait for the next round. The warm-up never holds.
   - Ours: `quarantine.rs` has no hold hook, so the generic rule of correction 3 applies (held from 3 players, in the first 20 s too; never with fewer).
   - Expect: Quarantine, 2 players, a real round live for 40 s; a third joins → held until the next round. 10 players, round live for 10 s; an 11th joins → spawns at once, human.

5. **Assist sums are cleared between rounds, and only an enemy's or teammate's killing blow clears a victim's sums.**
   - Reference: the damage sums are dropped when TDM's (and VIP Elimination's) round goes live, and after every real round of the round modes (Elimination Pro, S&D, VIP Escort, clan modes: at the intermission's end, in the branch that also pays the round's rewards, which live servers take), so damage from an earlier round never earns an assist. A death clears the victim's sums only when the killing damage came from another player (enemy or teammate); a suicide (own grenade) or a death with no player damager (fall, world) leaves them, so in a respawn mode they still count toward an assist in the victim's next life. Only health actually lost is summed (armour-absorbed damage is not).
   - Ours: `scoring.rs:177` (and 143, 153) clears the victim's sums on every death; `CaDamageLog` is otherwise cleared only at a match reset (`flow.rs:267`), so a survivor's round-1 damage carries into round 2.
   - Expect: Elimination Pro, A deals 60 to B (100 HP) in round 1; both survive to the time-out. Round 2: C kills B with no further damage from A → no assist for A. TDM: A deals 60 to B, B kills himself with a grenade, respawns, C kills B → A gets the assist.

6. **Bombing Run and Seize and Secure count deaths as S&D does, not as TDM.**
   - Reference: in these modes (and Squad Match 2) an enemy kill gives killer kills +1 and victim deaths +1; a team kill gives the killer a friendly kill and the victim **no** death; a suicide gives the victim a death and a suicide and **no** friendly kill; a death with no killer gives a death and **no** suicide.
   - Ours: `bombing_run.rs:57` and `seize.rs:63` use `scoring::personal` (TDM's column: team-kill victim death +1, suicide friendly kill +1, fall suicide +1).
   - Expect: Bombing Run, an Alpha player's grenade kills himself and a teammate → victim teammate deaths +0, thrower friendly kills +1, deaths +1, suicides +1, friendly kills not +2. A fall → deaths +1, suicides +0.

7. **Bombing Run: leaving gives the round away.**
   - Reference: when a player leaves and either was the only player of their team, or no player of their team other than the leaver is alive at that moment (the others dead and waiting to respawn; the leaver may be alive or dead), the other team wins the round at once (if no winner was set).
   - Ours: `bombing_run.rs:83` has no leaving rule.
   - Expect: Bombing Run 3 v 3, two Bravo players dead (respawn countdown), the living Bravo player leaves → Alpha wins the round at the next frame.

8. **Seize and Secure (and Squad Match 2): the last player of a team leaving ends the match.**
   - Reference: the leaver being the only player of their team gives the other team the win at once (the flag stops rising, round over at the next frame); the match ends with it (one round).
   - Ours: `seize.rs:91` has no leaving rule: the round runs its G minutes.
   - Expect: Seize and Secure 1 v 2 at t = 100 s, the Alpha player leaves → Bravo wins at the next frame, whatever the points.

9. **Last Man Standing and Hunted: leaving.**
   - Reference: when 2 or fewer players remain (leaver counted), the round ends at once; and when the leaver's going leaves one living player or none (and no winner is set), the round ends at once with that living player (the first in the server's list, if any) the round winner (round wins +1). With one player or fewer left, the match ends at the next match-end test (correction 1), that player the winner.
   - Ours: `last_man.rs:57` has no leaving rule; `last_man.rs:82` ends the match only on rounds played.
   - Expect: LMS, 4 players, A and B alive, C and D dead; A leaves → B wins the round at the next frame. 2 players, one leaves → round over, then match over, the other the winner.

10. **Quarantine: one player left ends the round and the match.**
    - Reference: a leaver who leaves one player or fewer on the server ends the round at once (no side wins), and the match ends after it (no winner). The match also ends after any round (or the warm-up, correction 1) when one player or fewer is on a team.
    - Ours: `quarantine.rs:88` (no leaving rule) and `quarantine.rs:66` (match end on rounds only).
    - Expect: Quarantine with 2 players in a real round, one leaves → round over at the next frame, match over after the intermission.

11. **Spy Hunt stage 2: kills don't count; killing the final spy sets the intel score.**
    - Reference: in stage 2 an enemy kill adds no kill and no death to anyone (team kills, suicides and falls still count as in stage 1). When the final spy dies with a hunter's damage, that hunter's mission (intel) points are **set** to the total intel count (stock 5), and the final spy's are set to 0; the hunters win.
    - Ours: `spy_hunt.rs:217` counts every enemy kill (`scoring::free_for_all`) and changes no mission points.
    - Expect: stage 2, the final spy (5 intel points) kills two hunters then hunter H (2 intel points) kills the spy → the spy's kills unchanged by the two kills, the hunters' deaths unchanged; H's mission points 5, the spy's 0; Alpha wins.

12. **Spy Hunt leaving: the hunters win.**
    - Reference: in stage 2, the last player of either side leaving ends the round with no winner set, so the time-out rule gives it to the hunters (Alpha) and clears the final spy's points, even when the last hunter left. In stage 1, a leaver who leaves one player or fewer ends the round the same way (hunters win).
    - Ours: `spy_hunt.rs:230` gives the round to the side that did not leave in stage 2 (the spy wins when the last hunter leaves) and ends stage 1 without a winner.
    - Expect: stage 2, final spy against one hunter; the hunter leaves → round over, winner Alpha, the spy's mission points 0.

13. **S&D and Bombing Run: the "Explode / Defuse" column and the planter's credit.**
    - Reference: S&D keeps no plant statistic; its scoreboard column shows detonations and defuses. The planter is credited one detonation at the round's result when the bomb exploded, **and also** when the bomb was planted and the round was then decided any other way than a defuse (defenders eliminated, attackers' last player leaving); the defuser one defuse after a defuse. Never in the warm-up; a planter or defuser who left is not credited. Bombing Run credits the same way and also counts plants.
    - Ours: `bomb.rs:147` credits a detonation only when the bomb explodes; `bomb.rs:108` counts S&D plants; `scoreboard.rs:52` shows no value for S&D's column.
    - Expect: S&D, P plants at t = 100 s, the last Creed player dies at t = 110 s → JSF wins; at the round result P's detonations +1. A defuse round: the planter gets nothing, the defuser +1 defuse. The S&D row shows P's detonations and defuses.

14. **One Man Army and Arms Race: every kill that reaches G names the winner; the leave share counts every kill.**
    - Reference: each kill that brings its killer to G or more kills (re)sets the winner to that killer, so of two such kills in one frame the later wins. The "kills made" of the leaving rule (M5) counts every enemy kill of the round, including those of players who already left.
    - Ours: `free_for_all.rs:84` keeps the first; the leaving rule sums the kills of the players still in (`rules.rs:538`).
    - Expect: G = 30, A and B both at 29; in one frame A kills C, then B kills D → winner B. 3 players, a fourth who made 5 kills left earlier; 2 remain with 0 kills between them and one leaves → the kills made are 5 > round(0.1 × 30) = 3 → the remaining player wins.

15. **Capture the Flag: a later capture can change the winner.**
    - Reference: every capture that leaves a team at G or more re-picks the winner as the team with more captures (equal keeps the old one), in any phase where captures count (M4, Quirk 13).
    - Ours: `ctf.rs:306` sets the winner only when none is set.
    - Expect: rare; G = 5, a time-out with Alpha 4, Bravo 4 decided Alpha on returns; during the 3 s hold Bravo captures (5) → winner Bravo.

16. **Quarantine's round setup is always one frame.**
    - Reference: like TDM, Quarantine goes live at the frame after the setup whatever the server file's `ObjectStart`.
    - Ours: `quarantine.rs:62` has no forced setup (uses `object_start`, 0 by default, so only a non-zero `ObjectStart` shows the difference).
    - Expect: `ca_object_start 5`, Quarantine → setup lasts one frame; S&D → 5 s.

Could not determine (left open):

- Quarantine: whether a player joining during a real round's setup, round over or intermission is held through the whole next round (the hold test answers "hold" there, but the release and the next setup's respawn of a held player were not traced together).
- Seize and Secure's kill and death tie-breaks sum the players' statistics from wherever the server's player-list walk last stopped, not from a fresh start; in practice this may skip more than the first player (Quirk 6) or everyone (a draw). Measure: a Seize and Secure time-out with equal points.
- TDM, CTF and Seize and Secure: a last player of a team leaving during the round-over hold overwrites the winner the match reports; whether players ever saw this (the result screen was already sent) was not settled.

Test cases 92 to 103 below cover these corrections.

## Corrections (2026-10-28): how spawn protection shows

A second pass over S6 (the server's respawn, damage filter and update, the
shot handling of both the client and the server, the client's character
object, the HUD panels and the interface sounds), for an implementation
that has to draw a generic "spawn protected" state the way Combat Arms
does.

1. **Nothing marks a protected player.** Neither the server nor any client
   changes anything about a protected player's look: no tint, flash,
   shader, transparency, outline or glow on the body or its attachments,
   no effect at the spawn point, no icon, timer, bar or text on anyone's
   HUD (the protected player's own included), no line in the chat, no
   sound when protection starts, ends or is cut short by firing. (The
   engine's older code base mentions cycling the model's transparency
   while invulnerable; Combat Arms keeps an unused field for it and never
   does it.) The tutorial's permanent god mode and the bots' own 3 s
   window (`bots.md`) are equally invisible.
2. **What the hit feedback does instead.** On a shooter's client, an
   instant-hit trace that reaches a player inside that client's window
   (3.5 s, item 3) is dropped whole at that player: no blood, no surface
   puff or decal at the stopping point (this answers what Open question
   25 left), no impact sound, no hit indicator or name-tag flash
   (`hud_crosshair.md` A10, A11), no character hit reported to the
   server. The round still ends there: nothing behind the player is hit.
   The shot's own firing (muzzle flash, fire sound, recoil, ammunition)
   happens as usual. A server-traced impact (a projectile, a bot's shot,
   the server's own trace of a shot) on a player inside the server's
   instant-hit window likewise makes no impact effect. Damage the server
   blocks (any kind, inside the 4.0 s window) is dropped before anything
   follows from it: health and armour do not change, so there is no
   hurt voice, no damage-direction marker, no screen effect and no hit
   reaction on the victim (`characters.md`). An explosion near a
   protected player still explodes normally for everyone; only its
   damage to that player is missing.
3. **Three clocks.** The server's all-damage window (4.0 s,
   `RespawnInvulnerabilityTime`) and instant-hit window (3.0 s) both run
   from the respawn on the server. Every client runs its own 3.5 s window
   for every player character from the moment that client handles the
   respawn notice for it (its own millisecond clock, so the window starts
   later by that client's latency). The protected player's own client
   runs the same window on its own character, used only to skip hit
   reactions. So, for a shooter with a small ping:
   - 0 – 3.0 s: the shooter sees no hit at all (item 2), the server
     deals nothing;
   - 3.0 – 3.5 s: the shooter still sees no hit; the shooter reports no
     player, the server traces the shot itself and meets the player, but
     the all-damage window blocks it;
   - 3.5 – 4.0 s: the shooter sees blood and the red crosshair (a normal
     reported hit), but the server blocks the damage: these are the hits
     that look landed and do nothing;
   - after 4.0 s: normal.
   In One Man Army, Arms Race, Last Man Standing and Hunted the
   protected player's first shot ends all three at once (S6): the server
   clears its windows and tells every client, which clear theirs.
4. **The reference's newest values.** The reference's latest edits set the
   server's two windows to 0 (the console default of
   `RespawnInvulnerabilityTime` becomes 0, the instant-hit window becomes
   0 and is cleared on the player's first update), and leave the
   client's 3.5 s window as it was. If the Steam build matches those
   edits (Open question 2), a freshly spawned player there is not
   protected at all, yet for 3.5 s every shooter's client shows its own
   instant-hit hits on that player as misses (item 2) and reports no
   player; the server then traces the shot itself and **does** deal
   damage. The first-shot notice is never sent in that build (the server
   flag is already clear), so the client window always runs its full
   3.5 s. This spec keeps 3.0 s and 4.0 s as its values; an implementation
   that offers the newer behaviour should do so as an option.
5. **Per mode**: no mode adds a look of its own. Besides the first-shot
   rule (S6) and Spy Hunt's fall exemption, VIP Escort freezes every
   living player at its round-end hold and starts the server's instant-hit
   window on each of them (M13); no client is told, and nothing shows.

For Chimera this means Combat Arms' style of a protected state is an
**absence**: suppress blood, impact effects, the hit indicator, the
name-tag flash and the victim's hit reaction for hits on a protected
player, and draw nothing else. A visible marker (a tint, an icon) is not
Combat Arms' and belongs to whoever adds it as an option.

Test cases 104 to 108 below cover these corrections.

## Corrections (2026-10-30): the infected's look and moment

From the server player object's infection step, the shared model-attribute and auto-cast sound readers, the client's Quarantine message handler, its character effects, crosshair element and weapon hands. Answers the plan's "The infected's look and moment" questions. M8 "Becoming infected" is made precise here.

1. **The infection sounds.** Two, both at the moment of infection:
   - **Server, heard by all nearby**: the auto-cast 3D record `AutoCast3DSound12` (infected; a host uses `AutoCast3DSound11`), its file drawn uniformly from the list for the player's **sound template as it was before the change** (the human's, e.g. male or female), radius the record's `3DSoundRadius` (inner 0.07 R, volume 100), at the infection point (below), as a server sound (to clients within 2R). Like the hurt sounds it plays only if the previous sound of the same kind has finished anywhere on the server (`effects_sounds.md` 11). After the change the player's template becomes the body type's `QuarantineSoundTemplate` (`Q_Male`, `Q_Female`), which later hurt and voice sounds use.
   - **Local, on two clients only**: the newly infected player's client and the infecting killer's client each play, local (no position), the auto-cast **local** record for infection for the **local player's own** body type's quarantine template if the infected character is infected (it always is here). The victim also sees the red indicator line "infected" for 5 s when there was a killer.
2. **The change effect.** The mission's `PlayerToInfected` ImpactFX record (`PlayerToHost` for a host) is sent as an explosion event to every client at the **infection point**: the player's position dropped straight down onto the first solid surface below, plus 0.1 u (so at the floor, not the hull centre), with up +Y and the player's yaw, no parent (it does not follow the body), damage radius 0. The client makes that record's impact group and sound as for any impact (`effects_sounds.md` 6). Host and ordinary infected differ only by which record the mission names.
3. **The screen effect.** For the whole time the **local** player is infected (from the change until changed back or the round ends), every frame, the crosshair element first draws the HUD layout's `InfectedTex` texture (layout record `HUDLayout0`) at `InfectedPos` with size `InfectedSize` (1024 × 768 units, x and y scaled independently like A8), white vertices, ordinary alpha blending, no depth. No pulse, no fade, no tint or vignette at the moment of infection besides the red indicator line; it is drawn even when the crosshair is hidden (it comes before the crosshair's other tests, after only "no video, not frozen").
4. **Eyes and other character effects.** A character's looping effect groups come from data lists: the body record's `ClientFX#`, the head piece's and the face piece's `ClientFX#`. Each listed group is created looping, parented to the character, when its model is set up or changed (so at infection, with the infected face and head), and shut down when that piece changes again. The head, face and model lists are shown only in third-person points of view and hidden in first person (so you never see your own eyes; everyone else sees them); they also follow the character's visibility. Which infected face or head lists which `Eye_*` group is data, not code (Open question 37). `PolyTrail` is the effect file's name for the trail key type (`effects_sounds.md` "The effect file", `PolyTube`).
5. **The override's look and hands.** The body stays the player's own model record; the infected look is the model record's `InfectedOveridable` (`HostInfectedOveridable` for a host) override record, whose `RenderStyle0..2` replace the body's styles, `FaceIndex` its face and `HandIndex` the first-person hands. Cosmetic items are released (model colour −1). `RS\TFACTOR_BLEND_VIRUS.LTB` and `..._HOST.LTB` draw exactly like `RS\default.ltb` without its pixel shader: 2 × texture × lit colour, opaque (their texture factor is not used by their stage) (`models_ltb.md` "Render styles and sprite skins"). First-person hands: the override's `HandIndex` hand record; if the override sets none (−1) **no hand model** is built (the base model's hands are not used as a fallback).
6. **The mission's infected model.** `[InfectedModelSetting] Model` and `[InfectedHostModelSetting] Model` are read but **never used**: the override of item 5 wins, and model 10 has no effect. The same records' goggle, backpack and weapon lists are used. A player who chose a Quarantine mercenary takes that mercenary's model and its override instead.
7. **Which faces list which eye groups** (Steam build 26236938's model attributes; answers Open question 37). In the models file only `Face#` and `AIModel#` records have `ClientFX#` lists; no `Model#` (body) record and no head (`Hair#`) record has one, so an infected player's eyes come from the **face** alone (the override's `FaceIndex`, item 5).
   - `Eye_Red_Tail_L`: the stock infected faces 20 `Infected_M_Face`, 21 `Infected_W_Face`, 22 `Infected_Host_M_Face`, 23 `Infected_Host_W_Face`, 24 `Infected_Sw_Face`, 25 `Infected_Host_Sw_Face`, 60 and 61 (copies of 24 and 25), the character infected faces (75, 76, 78, 79, 99, 100, 175–178, 181–200 in virus/host pairs, 208, 209, 211, 212, 214, 215) and some non-infected faces (162 Cyborg Ingram, 173, 179, 217–221, 239, 242, 247), which show the red eyes whenever they are worn.
   - `Eye_Assasin_Tail_SH`: face 38 `SubType - S`; `Eye_Tanker_Tail_SH`: face 39 `SubType - P`; `Eye_NewType_Tail`: faces 55 and 56 (infected-ship faces no override uses).
   - Four faces (244, 254, 255, 256) list a group named `1`, which the effect file does not have: nothing.
   - Every one of the 264 `Overidable_Model#` records names a face with an eye group: 260 the red one (its `FaceIndex` among 20–25, 60, 61, 75, 76, 78, 79, 99, 100, 177–215), `Infected_Woman_Subtype_S` and its host (`FaceIndex` 38) the assassin one, `Infected_Woman_Subtype_P` and its host (39) the tanker one. The stock override 0 (`Infected_Man_Light`, face 20) shows `Eye_Red_Tail_L`.
   - `Eye_Red_Tail_L` (phase 3000 ms): two `PolyTrail` keys (trail lengths 50 and 25) and one `Sprite` key per eye, sockets `EYE_L` and `EYE_R` (`UpdatePos` SocketAttach), each for the whole 0–3 s; the assassin, tanker and new-type groups have the same six-key shape; `Eye_Abnormal_Tail` three keys.
   - The PvE models (`AIModel#`) list theirs on the model record: the zombies 8–11 and the infected-ship crew 40–70 an eye group (plus `NamedNPC_Top` / `BlockNPC_Top` markers on the named ones), most abnormal ones the tanker eyes (plus burning, blood or smoke groups; the abnormal boss `Eye_Abnormal_Tail`).
   - Expect: test case 114.

## Test cases

Common setup unless stated: Ghost Town–style map with 12 JSF and 12 Creed `Team Death Match` points (P0..P11 in map order per team), fallback hull (15, 50, 15), server 20 Hz, `ObjectEnd` 3 s, `MissionEnd` 3 s, `ObjectStart` 0 s, friendly fire off, spawn protection 3.0 / 4.0 s, respawn waits 7.0 / 5.0 s.

**Shared**

1. Spawn cycling. 4 JSF players spawn one after another at round setup, nobody near the points. → They get P1, P2, P3, P4; the JSF cursor ends at 4. The Creed cursor is unaffected.
2. Occupied skip. Cursor at 2, a living player standing exactly on P3. Next JSF spawn → P4 (P3 skipped); cursor 4.
3. Occupied test bounds. Player at (0, 0, 0); a point at (29.9, 0, 0) → occupied; at (30.0, 0, 0) → free; at (0, 99.9, 0) → occupied; at (0, 100.0, 0) → free.
4. All occupied. List of 2 points, cursor 0, both occupied by unprotected players. Next spawn → tries P1, P0 (both occupied), uses P0; the player on P0 dies by crush, credited as their own suicide; cursor 0.
5. Wrap. Cursor at 11 (last JSF point). Next spawn → P0.
6. Spawn family. On a map whose `Doobic` points have `Spawn` 2, S&D → those points are not used; the player goes to the map's first `GameStartPoint`.
7. Respawn timing. TDM, a player dies at t = 0. → The client sends its request at the first frame after 7.000 s; the server accepts (more than 5.0 s); the player is alive at t ≈ 7.0 s plus latency, at the next team point, with full health and the rebuilt loadout.
8. Early request. Server receives a request 4.9 s after the death. → Refused; the player stays dead until a round setup.
9. Protection. Player respawns at t = 0. A rifle hit at t = 2.9 s → no hit, no damage, no blood. An M67 blast at t = 3.5 s → no damage. A rifle hit at t = 4.05 s → normal damage.
10. Protection ended by firing. One Man Army: respawn at t = 0, fires at t = 1.0 s, is hit at t = 1.1 s → takes damage. Same in TDM → no damage until t = 4.0 s.
11. Friendly fire on, TDM. Teammate rifle hit of 40 → 10 damage. Teammate M67 of 80 → 80. Own M67 of 80 → 80.
12. Friendly fire off. Teammate rifle 40 → 0; teammate M67 80 → 0; own M67 80 → 80.
13. Friendly fire on, Seize and Secure. Teammate rifle 40 → 40.
14. Assist. B has 100 max HP. A deals 60 to B, then C kills B. → A assists +1; C kills +1; B deaths +1. If A had dealt 49 → no assist.
15. Assist cleared by heal. A deals 60 to B; an enemy of A heals B for 15 (sum 45); C kills B → no assist.
16. Round cleanup. TDM: 3 rifles on the ground and 2 bodies when the deciding kill happens at t = 0. → At t = 3.0 s (plus one frame) all 5 are gone, including a map-placed shotgun.
17. Grenade after round over. S&D round decided at t = 0; a grenade thrown at t = −0.5 s explodes at t = 1.0 s and kills a player. → No kill, no death, no score; round result unchanged.
18. Knock-back. Infected player hit by an enemy M67 (k = 135, d = 0.4 s, s = 0), damage direction (3, 1, 4). At t = 0.1 s → magnitude 135² × 0.9 = 16402.5 along (0.6, 0, 0.8), i.e. acceleration (9841.5, 0, 13122.0) u/s². At t = 0.4 s → timer over, normal movement. Crouched and still → velocity 0. A human hit by the same grenade → no knock-back.

**M1 TDM**

19. G = 100. Alpha 99, Bravo 50. An Alpha player kills a Bravo player at t = 0. → Alpha 100; round over at the next frame, winner Alpha; round end at t ≈ 3.05 s (bodies and weapons cleared); match result after `MissionEnd`.
20. Team kill. Alpha 10, Bravo 10; an Alpha player kills an Alpha teammate. → Alpha 10, Bravo 11; killer friendly kills +1; victim deaths +1; killer kills unchanged.
21. Fall death. An Alpha player falls to death. → Bravo +1; victim deaths +1, suicides +1.
22. Grenade suicide. Bravo player kills themselves. → Alpha +1; victim deaths +1, suicides +1, friendly kills +1.
23. Two kills in one frame at the limit. Alpha 99, Bravo 99; in one frame Alpha kills (Alpha 100) then Bravo kills (Bravo 100). → Winner Alpha; both show 100.
24. Last player leaves. Bravo has one player, who disconnects. → Alpha wins at once; round over next frame.
25. Kill during round over. After test 19's deciding kill, a Bravo player kills an Alpha player at t = 1.0 s. → Scores stay 100 – 50; no kill or death added.
26. No timer. TDM round live for 3600 s with no kills → round still live; HUD shows no timer.

**M2 Elimination Pro**

27. Warm-up. Match start at t = 0. → Warm-up live for 10 s; a death at t = 2 s adds nothing and the player is revived by t ≤ 5 s; after it ends, round 1 begins after `ObjectEnd` + `MissionEnd` + setup ≈ 6 s.
28. Elimination. 3 v 3; Bravo loses its third player at t = 60 s. → Alpha round wins +1; next round after ≈ 6 s.
29. Time-out by alive count. 180 s pass with Alpha 3 alive, Bravo 2. → Alpha wins the round.
30. Time-out tie-break. 2 v 2 alive; round kills 3 – 3; round deaths Alpha 3, Bravo 2. → Bravo wins.
31. Time-out draw. Alive 2 – 2, kills 1 – 1, deaths 1 – 1. → No winner; win counts unchanged; round number +1.
32. Match end. G = 7, Alpha reaches 7 round wins. → Match over, winner Alpha.
33. Late join. 6 players, round live at 30 s; a 7th joins. → Held, spawns at the next round setup. With only 2 players in the game → spawns at once.

**M3 Search & Destroy**

34. Bomb hand-out. Round 1 setup with 4 living JSF players. → All 4 hold the bomb. Warm-up setup → nobody holds a bomb.
35. Plant and detonate. Round live, plant completes at t = 100 s (80 s left). → Round timer 120 s left; bomb at 40 s; at t = 140 s the bomb detonates → JSF wins.
36. Defuse. As 35, a Creed player starts defusing at t = 128 s and holds for 7 s. → Defused at t = 135 s; Creed wins.
37. Interrupted defuse. Defuse held from t = 128 s to t = 131 s, released. → Not defused; the bomb still explodes at t = 140 s unless a new 7 s defuse completes.
38. Attackers wiped after plant. Plant at t = 100 s; the last JSF player dies at t = 110 s. → Round continues; detonation at t = 140 s → JSF wins.
39. Defenders wiped after plant. Plant at t = 100 s; the last Creed player dies at t = 110 s. → JSF wins at t = 110 s.
40. Time-out. No plant, 180 s pass. → Creed wins.
41. Plant zone margin. Site centre (0, 0, 0), half-extents (130, 64, 125). A planter at (175, 0, 0) → inside (≤ 130 + 50); at (181, 0, 0) → outside, no plant.
42. Match end tie. G = 5, wins 3 – 3, then every Creed player leaves. → The match ends after the round; S&D picks by wins only, 3 – 3 → winner **Bravo**, the empty team. (Elimination Pro in the same situation gives Alpha, the non-empty team.)

**M4 Capture the Flag**

43. Time limit. G = 5 → round time 900 s; G = 3 → 540 s.
44. Capture. Alpha carrier with the Bravo flag touches the Alpha flag zone, Alpha flag at home. → Alpha captures 1; the Bravo flag back at its base; carrier captures +1.
45. Capture blocked. Same, but the Alpha flag is dropped somewhere. → No capture until an Alpha player touches the dropped Alpha flag (Alpha returns +1, flag home), then the carrier can capture.
46. Drop and return timer. Carrier killed at (100, 0, 200) on flat ground at t = 0. → Flag at (100, floor, 200); untouched, it goes home at t = 30 s with no return credit.
47. Kill zone. Carrier falls out of the world. → Flag home at once.
48. Win. Alpha at 4 captures, G = 5, captures again. → Alpha wins; round over next frame.
49. Time-out tie-break. 2 – 2 captures, returns 3 – 1. → Alpha wins. 2 – 2 captures, 1 – 1 returns, kills 10 – 10, deaths 9 – 11 → Alpha (fewer deaths). All equal → draw.

**M5 One Man Army**

50. G = 30. A player reaches 30 kills. → Wins at once. A player at 29 kills who suicides → stays at 29 kills, deaths +1.
51. Spawns. 22 `FreeForAll` points shuffled at load; consecutive spawns take consecutive shuffled points with one shared cursor.
52. Leaving. G = 30. 3 players, one leaves → nothing happens (3 counted, leaver included). Then 2 players with 10 kills made in total, one leaves → round over; the remaining player wins (10 > round(0.1 × 30) = 3). With only 3 kills made in total → round over, no winner.

**M6 Last Man Standing**

53. Round. 5 players; deaths leave one alive. → That player round wins +1; round over. G = 5 → match over after round 5 (rounds played), winner by the comparison in M6.
54. Winner quirk. Players in list order A (2 round wins), B (3), C (3). → Each is compared with A only; the pick is the last player with more round wins than A: **C**. Order A (2), B (3), C (2) → C ties A, which blocks the round-win test; the kill test decides. Order A (0 round wins), B (4) → the round-win test is skipped because A has 0; the kill test decides.

**M8 Quarantine Regen**

55. Hosts. 8 players → round(1.6) = 2 hosts; 12 players → round(2.4) = 2; 13 → round(2.6) = 3; 3 players → 1 host; 20 → 4.
56. Time-out. 3 minutes pass with humans alive. → Humans win; living humans +10 points each.
57. Void round. 4 players, 2 hosts needed, only 2 humans alive at 20 s. → Round void, no winner.

**M9 Bombing Run**

58. Time-out. 300 s pass without detonation. → Alpha and Bravo each +1 round win.
59. Detonation. Bravo plants in Alpha's zone; it detonates. → Bravo wins the round.

**M10 Seize and Secure**

60. Hold. JSF seizes at t = 0 and holds 30 s, Creed seizes at t = 30 s. → JSF 30 points; JSF flag hidden; Creed's flag starts from the bottom.
61. Top. A team holds 120 s uninterrupted. → It wins at once.
62. Time-out bonus. G = 8 → 480 s; at the end JSF's flag is rising with JSF 200, Creed 220. → JSF +50 = 250 → JSF wins.

**Follow-up cases** (Open questions 4, 10, 12, 17–36; "old release" = plaintext maps of an older extracted client release, "data" = US attribute and mission files)

63. **Intermission after every round.** S&D round 1 decided at t = 0 (G = 5, score 1–0): round end at t ≈ 3.05 s (bodies and weapons cleared), intermission until ≈ 6.05 s, match-end test (not over), mission start and round 2 setup on the following frames. The last round's end runs the same 3 s + 3 s before the match result.
64. **Detonation hurts nobody (old release).** S&D Ghost Town, bomb planted at site A with one JSF and one Creed player standing on the site when the timer reaches 0 → both keep their health; JSF wins the round; the planter's detonations +1; no kill, death or suicide is counted; every client's view runs the site A camera script (first camera move at 0.15–0.2 s, effects from 0.4 s, last effect at 5.3 s).
65. **Plant timing (old release XK-2 files).** Attacker inside the widened site box presses fire at t = 0 and holds: movement locked; the plant completes when the `FIRE_KEY` at 3904 ms of `fire_0` is consumed (at 60 fps: firing state on the press's update 0, `prefire` on update 1, `fire_0` from update 2, the key fires in the advance of update 236 and the plant is sent on update 237, 3950 ms); the bomb timer starts at 40 s. Released at t = 3.0 s → no plant, movement back; pressed again at t = 3.5 s → the plant completes about 3.9 s after that press (t ≈ 7.4 s), not 0.9 s.
66. **Partial defuse.** Bomb ticking; a Creed player holds use on the site from t = 128 s to 131 s, lets go, and holds again from t = 132 s → defused at t = 139 s (7 s from the second press), not at 136 s.
67. **Bomb beeps (data).** One interface beep each time the whole-second count of the bomb timer changes; the file by the time left at that moment: more than 30 s `T9COUNT1`, more than 20 s `T9COUNT2`, more than 10 s `T9COUNT3`, more than 5 s `T9COUNT4`, more than 0 s `T9COUNT5`. Example: the change from 26 to 25 s left (25.99 s) plays `T9COUNT2`; from 6 to 5 (5.99 s) `T9COUNT4`; from 5 to 4 (4.99 s) `T9COUNT5`.
68. **Spawn families (old release).** Bombing Run on Ghost Town: the 12 + 12 `Sabotage` points; Seize and Secure: the 12 + 12 `HeadQuaters` points; neither uses `Team Death Match` points.
69. **Flag pole (old release).** Seize and Secure on Ghost Town: the pole model is created at (279, −1466, −79) with yaw 180° (the `FlagPolePos` object); the flags start at its `HQ_pole_top` (Alpha) and `HQ_pole_bottom` (Bravo) sockets.
70. **Arms Race (data).** G = 30, default table. A player reaches 7 kills: roll r = 50 → weapon 337; r = 79 → 337; r = 80 → the player's own equipped weapon of slot 1; reaching 8 kills → no change (no step at 8). G = 15, 14 kills → weapon 84 and "last stage" announced.
71. **VIP points (data).** G = 140, a team of 5 → 2 VIPs. Ranking by VIP points: the top player is silver (worth 5), the second bronze (3), the others 1. Killing the silver VIP adds 5 to the killer's team and to the killer's VIP points; a fall death of the bronze VIP adds 3 to the other team.
72. **VIP count (data).** G 100..180: team sizes 1, 2, 4, 5, 7, 8, 12 → 0, 1, 1, 2, 2, 3, 3 VIPs. G = 60 (first table): sizes 5, 6, 8 → 0, 1, 2.
73. **Spy Hunt stage 2 (data).** The final spy is made at t = 300 s → every view on the map camera at `CameraPoint00` until t = 305 s; then the round timer is set to 180 s; nobody wins by t = 485 s → time-out, hunters (Alpha) win. At the switch a living hunter at 30 HP with an empty magazine gets full HP, full magazines for every weapon and their armour back; a dead hunter is respawned.
74. **Quarantine infection (logic).** Real round live at t = 0; an infected (host since t = 20 s) kills a human with the infection hand at t = 50 s → in the same server frame the human is on Bravo, alive at full health at the spot where they died (on the floor), holding weapon 191. In the warm-up nobody is infected and no host is chosen.
75. **Infected regeneration (data).** An infected with maximum 100 HP at 40 HP stands still from t = 0 → +10 at t = 3.0 s, +10 at 4.0 s, +10 at 5.0 s; it moves at t = 5.5 s → no heal until 3 s after it stops again; hit at t = 4.5 s → the wait restarts.
76. **Mutation pick (data).** TDM, the toucher's team is behind by 20 % (condition 0): weights 10, 5, 10, 10, 10, 10, 10, 5, 5, 5 (sum 80) → item 0 with probability 10 / 80 = 0.125, item 7 (invulnerable 10 s) 5 / 80. Ahead by 20 % (condition 2): item 7 has weight 20 of 125.
77. **Friendly fire order.** FF on, TDM: a teammate rifle hit of 40 enters the armour and hit-zone rules as 10 (the 0.25 scale comes first).
78. **Protected player absorbs a shot.** Enemy P respawned 1 s ago (as the shooter's client sees it), enemy Q standing exactly behind P on the shot line: the shot hits neither P nor Q, no blood.
79. **CTF capture in round over (logic, Quirk 13).** After a time-out has been decided, an Alpha carrier touches their zone with their flag at home during the 3 s hold → Alpha captures +1 and the carrier's captures +1.
80. **Carried flag (data).** An Alpha (team 1) carrier: `ATTACH_M\flag01_back.ltb` with skin `ATTACH_T\CTF_Flag_B.DTX` on `1W_N`, visible to others while carrying, hidden after a capture, a return or the carrier's death.
81. **Spectating (logic).** Elimination Pro, A killed by B (alive), enemy view on → A watches B; B dies → A watches the next living player of A's team; no teammate alive → living enemies; enemy view off → only teammates (and nobody to watch when none is alive). Last Man Standing: any living player.
82. **Random draws (logic).** Two drop-point shuffles made in the same wall-clock second with the same point list give the same order (each reseeds with the clock in seconds).
83. **Fast respawn (logic).** TDM, death at t = 0, the fast respawn item used at t = 0.5 s → the server accepts at once (no 5 s test), the item count drops by 1 (not with the premium pass), the old body disappears at once. No uses left → refused as illegal.
84. **Hunted start (data).** Every player spawns into a round holding only weapon 934 (`fist`).
85. **Occupied test crouched (logic).** A fully crouched player (half-extents (15, 40, 15)) at (0, 0, 0): a point at (0, 79.9, 0) is occupied, at (0, 80, 0) free (standing: 100).
86. **Box half-extents (data).** Ghost Town site A at (284.7, −1618.8, −1419.5) with `Dims` (133.5, 64, 123.5): a planter at x = 284.7 + 183.4 = 468.1 is inside the plant test (≤ 133.5 + 50), at x = 468.3 outside.
87. **Goal text (HUD, Corrections 1).** Room goal 7: S&D, Elimination Pro, Bombing Run, VIP Escort -> the `IDS_GAMEROOM_ROUND` text with 7 (US "7 Rounds"); CTF goal 5 -> "5 " + `IDS_POINTS`; Squad Match 2 goal 300 -> "300 " + `IDS_POINTS`; TDM goal 100 -> the `IDS_GAMEROOM_KILL` text with 100; Seize and Secure goal 10 -> the `IDS_GAMEROOM_MIN` text with 10.
88. **Timer across a S&D round (HUD, Corrections 2).** Stock files (ObjectStart 0, TimeLimit 180): setup -> no timer; live -> "03 : 00"; plant with 75.4 s left -> round timer hidden, bomb timer "00 : 40"; defuse 7 s later -> round result: both hidden.
89. **Living players on the panel (HUD, Corrections 3).** Elimination Pro 4 v 4, one Bravo player dies -> row titles "4" (Alpha) and "3" (Bravo); next round's start -> "4" and "4".
90. **Indicator panel per mode (HUD, Corrections 4).** Bombing Run -> `s&d`; VIP Escort -> `VIP_Esct`; Squad Match 2 -> `HG2`; Hired Guns -> `Squad`.
91. **Round result (HUD, Corrections 5).** TDM: the deciding kill at t = 0 -> from the round result the banner alone (no lists) and no mode panel, until the match result screen. Elimination Pro: the round result at t = 0 -> banner over the lists until the next round's setup message (about t = 6 s with stock times).
92. **Empty team after the warm-up (Corrections 2026-10-22, 1).** Elimination Pro 1 v 1; the Bravo player leaves at t = 4 s of the warm-up → warm-up over at the next frame, round over until ≈ 7 s, intermission until ≈ 10 s, then match over, winner Alpha; round 1 never starts. S&D in the same case → match over, winner Bravo if round wins are 0–0 (more wins, else Bravo).
93. **Warm-up leaving (Corrections 2).** S&D warm-up, Creed has 2 players, one dead (revive pending), the living one leaves at t = 3 s → warm-up ends at the next frame (no winner).
94. **Late-join count (Corrections 3).** S&D live 30 s, 2 players in, a third joins → held. 1 player in, a second joins → spawns at once. 5 players in, a joiner 0.6 s after going live → spawns at once; 1.2 s after → held. TDM, CTF, Bombing Run, Seize and Secure, Spy Hunt, One Man Army → never held.
95. **Quarantine joiners (Corrections 4).** Real round live 10 s, 10 players → the joiner plays at once as a human; live 25 s, 2 players → held until the next round.
96. **Assists across rounds (Corrections 5).** Elimination Pro, A deals 60 to B (max 100) in round 1, both survive; round 2, C kills B untouched by A → no assist. TDM, A deals 60 to B, B dies by his own grenade, respawns, C kills him → A assist +1.
97. **Bombing Run counts (Corrections 6).** A team kill → victim deaths +0, killer friendly kills +1. A fall → deaths +1, suicides +0. Own grenade → deaths +1, suicides +1, friendly kills +0.
98. **Bombing Run leaving (Corrections 7).** 3 v 3, two Bravo dead awaiting respawn, the third Bravo leaves → Alpha round win at the next frame.
99. **Seize and Secure leaving (Corrections 8).** 1 v 2, the Alpha player leaves at t = 100 s → Bravo wins at once.
100. **Last Man Standing leaving (Corrections 9).** 4 players, A and B alive; A leaves → B round wins +1, round over at the next frame. 2 players, one leaves → match over after round over and intermission, the other the winner.
101. **Spy Hunt stage 2 (Corrections 11, 12).** The final spy kills two hunters (no kills or deaths added), hunter H kills the spy → H's mission points = 5, the spy's = 0, Alpha wins. Final spy against one hunter, the hunter leaves → Alpha wins, the spy's points 0.
102. **S&D credit (Corrections 13).** Plant at t = 100 s by P, last Creed dies at t = 110 s → JSF wins, P's detonations +1 at the round result. Warm-up: nothing credited.
103. **Two kills at G in a frame (Corrections 14).** One Man Army G = 30, A and B on 29; A's kill then B's kill in one frame → winner B.
104. **No marker (Corrections 2026-10-28, 1).** TDM, enemy P respawns at t = 0; an observer looks at P at t = 0.5, 2.0 and 3.9 s → P's model, materials, colour and opacity are exactly those of an unprotected player of the same character; no HUD element, chat line or sound appears for P on any client (P's own included) at t = 0, 3.0, 3.5 or 4.0 s.
105. **Protected hit feedback (Corrections 2026-10-28, 2).** P (respawned at t = 0) stands 2 u in front of a concrete wall; the shooter's rifle shot at P's chest is traced at t = 1.0 s → on the shooter's client: muzzle flash, fire sound, recoil and one round spent; no blood, no puff or decal on P or on the wall, no impact sound, the crosshair keeps its colour, no character hit is reported; P's health and armour unchanged, P plays no hit reaction and hears no hurt voice.
106. **The three windows (Corrections 2026-10-28, 3).** The shooter's client handles P's respawn notice 50 ms after the server's respawn. Shots traced at server time 2.9 s and 3.4 s → no hit shown, no damage; at 3.6 s (client window 3.55 s, over) → blood and the red crosshair on the shooter's client, no damage on the server; at 4.1 s → blood, red crosshair and normal damage.
107. **First shot ends it everywhere (Corrections 2026-10-28, 3).** Arms Race, P respawns at t = 0, fires at t = 1.0 s; a shot at P traced at t = 1.2 s → blood, red crosshair, damage. The same in Elimination Pro → no hit shown until 3.5 s, no damage until 4.0 s.
108. **Newest reference values (Corrections 2026-10-28, 4).** Server windows 0, client window 3.5 s: P respawns at t = 0; an M67 blast at t = 0.5 s → damage; a rifle shot at P traced at t = 1.0 s → the shooter's client shows no hit and reports no player, the server's own trace hits P and deals the damage; P's first shot sends no notice, and shots at P still show no hit until 3.5 s.
109. **Infection point (Corrections 2026-10-30, 2).** A human killed by an infected while standing on a floor at y = 0 (origin 40 u above it) → the change effect at y = 0.1 under the player, up +Y; on a 30 u ledge edge with the floor 30 u lower directly below the origin → at that lower floor + 0.1.
110. **Infection sounds (Corrections 2026-10-30, 1).** A male human infected while another infection sound is still playing elsewhere on the server → no server sound; the victim's and the killer's clients each still play their local infected sound; all other clients nothing local.
111. **Overlay (Corrections 2026-10-30, 3).** Local player infected and zoomed with a scope overlay → the infected texture is still drawn every frame; changed back to human → gone the next frame.
112. **No hands fallback (Corrections 2026-10-30, 5).** An override record without `HandIndex` → no first-person hand model while infected.
113. **Mission model unused (Corrections 2026-10-30, 6).** `[InfectedModelSetting] Model` changed from 10 to 3 → the infected's body unchanged.
114. **Eye groups by face (Corrections 2026-10-30, 7; data, Steam build 26236938).** Override 0 → face 20 → `ClientFX0` `Eye_Red_Tail_L`, six keys on `EYE_L`/`EYE_R`, looping in third person; override 58 → face 38 → `Eye_Assasin_Tail_SH`; override 60 → face 39 → `Eye_Tanker_Tail_SH`; face 244 → group `1`, not in the effect file → no effect; no body or head record has a `ClientFX#` list.

## Open questions

1. **Elimination naming.** The brief lists "Team Death Match" and "Elimination" as two modes. In this game they are one mode (`TDM`, shown as Elimination); this spec takes the brief's "Elimination" to mean Elimination Pro (`TSV`, M2). Confirm.
2. **Respawn and protection values.** The reference carries marked edits that change the respawn waits (7.0 → 2.0 s client, 5.0 → 1.5 s server) and turn off spawn protection (3.0 s and 4.0 s → 0). This spec uses the earlier values. Check on the live game: time death to respawn in TDM with a stopwatch, and shoot a freshly respawned enemy.
   Narrowed (2026-10-28): the edits leave the client's 3.5 s window alone, so the two builds can be told apart live: in both, a shot at an enemy within about 3 s of their respawn shows no blood; with the edits it still takes their health (watch the kill feed or the enemy's death), without them it does not (Corrections 2026-10-28, 4; case 108).
3. **Round phase lengths.** `ObjectEnd`, `MissionEnd` and `ObjectStart` default to 0 when the server file lacks them; only the client's copy of the server file has 3 / 3 / 0. Time the gap between a round's result and the next round's start in S&D (expect ≈ 6 s).
4. **Bomb plant time.** The HUD gauge is 4 s and the zone accepts the plant when the client reports it; the exact hold time and whether moving or releasing fire cancels it are decided by the client's bomb weapon, which was not traced. Measure a plant with a stopwatch, and whether a partial defuse keeps its progress.
   **Answered:** the plant is the bomb's fire animation (XK-2 `fire_0` 4000 ms, fire key at 3904 ms) with the planter's movement locked; releasing fire cancels and nothing is kept; the zone plants at once when the key's shot arrives inside the widened box. A partial defuse keeps no progress (every attempt gets the full 7 s); the plant's own "keep time" rule is moot because the zone's plant time is 0. Applied in M3; cases 65, 66. A live check on Steam (the XK-2 files may differ): time from pressing fire to the "planted" text.
5. **Order of objects and the mission update** inside a server frame was not confirmed; this spec assumes deaths are handled before the frame's mission update.
6. **Free-for-all team ids.** The lobby assigns them (Spy Hunt uses the player's room key + 1); confirm every player gets a different id in One Man Army. This also settles `hud_crosshair.md` open question 15 (everyone else is an enemy).
7. **Room-data-only modes** (M22): Big Head, Race Combat, Team Race Combat, Lone Combat, Lone Combat Infected and the skill Elimination have no rules in the reference. They need a later reference or live play.
8. **Last Man Standing winner** (Quirk 5): confirm on the live game whether the match winner really follows the first-player comparison (and the "more deaths" test), or whether a later build fixed it.
9. **Co-op modes** (M19): lives per objective, revive rules and mission scripts were not traced in detail.
10. **Spy Hunt stage 2 time** (`Stage2PlayTime` 180 s) and its time-out winner were not traced.
    **Answered:** the round timer is restarted with `Stage2PlayTime` after the 5 s camera hold; a time-out in either stage goes to the hunters (Alpha); the stage-2 refill is health, ammunition for every weapon and gear. Applied in M7; case 73.
11. **Join in progress off**: whether the lobby simply refuses joiners once the match has started was not traced (lobby side).
    Still open (lobby side, `lobby.md`): the game server only sends the "closing" notices of S1; whether the lobby then refuses joiners is outside the game server. Checked: every rule set's closing test (S1 values unchanged).
12. **Infected regeneration unit**: `Heal = 0.1` every 1 s after 3 s still; whether 0.1 is a fraction of maximum health or hit points was not traced.
    **Answered:** a fraction of the infected's maximum hit points (10 % per second), interrupted by movement, damage and firing. Also settled here (brief): a human killed by an infected becomes infected at once in place; the infected's model is the body's infection override, the weapon the mission file's (191, hosts 190); host selection never runs in the warm-up; there is no match winner. Applied in M8; cases 74, 75.
13. **Contradiction check with `hud_crosshair.md` D7**: it lists capture the flag among "round-based modes" with a timer. CTF is one round with a timer of 180 × G s; the timer is right, the label is loose. No change needed there, noted only.
14. **Contradiction check with `grenades.md`**: none. Its open question 11 (grenades at a round's end) is answered in S11 (they keep their fuse; their kills outside the Live phase do not count); its question 14 (M67 knock-back) is specified in S14.
15. **Contradiction check with `inventory.md`**: none; section 12's "no drop in snowball or the VIP weapon race" matches M14 and M16, and section 13's respawn matches S5. Addition: all ground weapons, including map-placed ones, are cleared at each round end (S11).
16. **Contradiction check with `world_dat.md`**: none. Its open question 3 is answered in S12 (`ObjectSpawnPoint` = drop points for mode items, filtered by `GameType`, `Spawn` unused). Note that `GameStartPoint.Spawn` must be exactly 1 for versus modes (S4).
17. **S&D detonation** (blast damage and who is hurt, credit, effect and sound, the planted bomb's object).
    **Answered:** no blast at all: the stock detonation commands win the round for the attackers and run a camera-and-effects show; the planter gets a detonation statistic, nobody is hurt or killed. No bomb model is placed; the site's red indicator, the HUD countdown with per-second beeps, the planted and defused interface sounds and texts, and the forced dynamic lights are what clients get, the same for both teams. Applied in M3 and S15 (the look of the explosion groups is `effects_sounds.md` 9); cases 64, 67.
18. **The sites' `PlantedCommand` and other command strings.**
    **Answered:** S15 lists them by role: the planted command starts the bomb and round timers and locks/switches the sites and indicators; the detonated, defused and power-off commands; the controller's per-phase commands; the mission file's start and end triggers that switch each mode's objects; the use triggers of Seize and Secure; the spawn point command.
19. **Spawn families of Bombing Run and Seize and Secure.**
    **Answered:** `Sabotage` and `HeadQuaters` per team, `Spawn` 1 (S4); case 68.
20. **A point's `Command` on respawn; the fast respawn item.**
    **Answered:** S4 step 4 (once per spawn, unused in stock maps) and S5 (TDM and CTF only, skips the wait test, spends a lobby-counted use, removes the body at once); case 83.
21. **Does the intermission come after every round?**
    **Answered:** yes, because every stock objective ends its mission (S3 step 6, corrected); case 63.
22. **Late-join closing in the elimination modes** (with Open question 11).
    Partly answered: the closing notices are as in S1; whether a closed room refuses joiners is the lobby's (`lobby.md`). Measure: in a live Elimination Pro room 2 rounds from the end, try to join from a second account.
23. **Does `MissionTimeLimit` end anything?**
    **Answered:** no (Quirk 11).
24. **Seize and Secure's flag pole.**
    **Answered:** M10 (the `FlagPolePos` object, the `HQ_Flag_*` props and their sockets); case 69.
25. **Spawn protection and instant-hit shots** (pass through, or stop and do nothing; blood).
    **Answered:** they stop and do nothing, and the client draws no blood (S6, corrected); case 78. Still open: whether a plain surface puff appears at the stopping point. Measure: shoot a freshly respawned enemy standing in front of a wall and watch for a puff on the enemy.
    **Answered (2026-10-28):** no puff, no decal, no impact sound: the shooter's client drops the whole impact at the protected player (Corrections 2026-10-28, 2); case 105.
26. **Scoring details** (damage scaled before or after armour, the team-id filter, heals, the teammate-hit voice).
    **Answered:** S8 Details; case 77.
27. **Mutation boxes and the revenge buff.**
    **Answered:** S12 (the item table, conditions and weights; revenge = item 10); case 76. Stat-by-stat rules: `gear_perks.md`.
28. **VIP Elimination data and ranks.**
    **Answered:** M12; cases 71, 72.
29. **Arms Race table and roll.**
    **Answered:** M14; case 70.
30. **Box sizes and pick-up boxes.**
    **Answered:** `Dims` are half-extents (Units and conventions); a player's box is the current hull, (15, 50, 15) standing (S4); the bomb and intel pick-ups are (10, 25, 10), dropped to the floor by a straight-down ray (M9, M7); cases 85, 86.
31. **HUD and scoreboard per mode.**
    Partly answered: the goal text and the scoreboard's mode-column title per mode (S13). Still open (the HUD's and lobby's specs own them): objective lines, gauge looks, column order beyond S13, level icon and clan emblem rules, tie order and the result screens. Measure: screenshots of the scoreboard and result screen in each mode with two players tied on kills.
    **Answered further (2026-10-19):** each mode's indicator panel, the row titles of the round modes (living players), the timers (setup countdown, bomb timer), the round result's length and look, tie order (undefined, the sort is not stable) and the action gauge's looks (`hud_crosshair.md` D9) are settled in S13's "HUD panel per mode and the round result" and in Corrections, HUD and scoreboard. Still open: Last Man Standing's and Spy Hunt's own panel fields, level icon and clan emblem rules.
32. **Random draws.**
    **Answered:** one C-library generator, reseeded from the clock; any uniform draw is parity (S16); case 82.
33. **S&D side swaps and the bomb hand-out's timing.**
    **Answered:** no swap outside the clan variants and VIP Escort; the bomb is added after the respawn and loadout and the start triggers, before the freeze, without drawing it (M3; Quirk 14).
34. **Capture the Flag details** (carried flag, phases, assists).
    **Answered:** M4 (the `1W_N` back flag, captures and returns in every phase but the warm-up, no assists); cases 79, 80.
35. **Whom the dead may spectate.**
    **Answered:** M2 (killer first with the enemy-view option, then living teammates, then enemies; free choice in Last Man Standing and Hunted); case 81.
36. **Hunted's scavenging, `DyingTime` and time hunted.**
    **Answered** at the depth this spec keeps (M15); the full mode belongs to `game_modes_2.md`; case 84.
37. **Answered (2026-10-30, build 26236938)**, Corrections (2026-10-30) 7: faces only; the stock infected faces list `Eye_Red_Tail_L`, subtypes S and P the assassin and tanker eyes. Was: **Which infected faces and heads list the eye groups.** The face, head and body records' `ClientFX#` lists in the install's model attributes decide which `Eye_*` group each infected shows (Corrections (2026-10-30) 4); read them on the PC with the keys.
