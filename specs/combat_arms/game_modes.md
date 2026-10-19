# Combat Arms: game modes

Source basis: the dedicated server's match rules (one rule set per game mode, the shared match base, the player's spawn, respawn, damage filter and death handling, the spawn point, bomb zone, flag and flag zone objects), the client's respawn request, death camera, respawn gauge, action gauge and scoreboard, the room data of a US-region data set (mode list, goal ranges, mode names), the per-map mission files of a US-region client, the dedicated server's and client's server configuration files, and object lists read from plaintext maps of that client.
Status: draft

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
6. **Round end** (next frame after the timer): round results and rewards are computed and sent, objective items are dropped and removed, **every body and every weapon item on the ground is removed** (S11), held players are released, per-round statistics are cleared. Then, if the mode has more rounds, the next **Mission start** follows; otherwise **Intermission** (`MissionEnd` seconds), then the match result.
7. **Match over**: a reward roulette is played on the clients, then the server pauses and reports the result to the lobby. If every player leaves during the roulette, it ends at once.

```math
t_{\text{between rounds}} = \text{ObjectEnd} + \text{MissionEnd} + \text{ObjectStart} + \text{(frames)} \approx 3 + 3 + 0 = 6\ \text{s with the client's file}
```

The HUD's round timer (`hud_crosshair.md` D7) is fed the round timer's remaining time when the round goes live; TDM sends no time (no timer).

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
| every other mode | its own game-type string, per team |

- One-list modes (`FreeForAll`, `BountyHunter`, `Quarantine`) put every qualifying point in one list whatever its `Teams`. The `FreeForAll` list (and Hunted's two lists) are **shuffled once at map load**; team lists keep the map's object order.
- Co-operative modes pick, at each spawn, unlocked co-op points whose `Spawn` equals the current spawn area of the objective (and the team, when the player has one).
- A `GameStartPoint` can be locked and unlocked at run time by the map's `LOCK` and `UNLOCK` messages. The versus lists are built once, so a later lock does not remove a point from them.
- A typical map (Ghost Town) has 12 + 12 points per team mode and 22 `FreeForAll` points. Spawn points sit about 66 u above the floor; the spawned player is dropped to the floor.

**Which point (versus modes).** Each list has a **cursor**, starting at 0, shared by all players of that list (one cursor per team; free-for-all modes share one cursor for everybody). For a spawn:

1. Advance the cursor by one (wrapping at the end of the list) and test that point.
2. A point is **occupied** when any other player object (alive or dead player objects; bodies are not players) has its centre within twice its own half-extents of the point on every axis (strict): with the fallback hull (15, 50, 15), |dx| < 30, |dy| < 100, |dz| < 30.
3. If occupied, repeat from 1, at most as many times as the list has points. The last point tested is used even if occupied. The cursor stays where it stopped.
4. The player is placed at the point, faces its `Rotation` yaw, is dropped to the floor, and, if the point has a `Command` with `SendCommandOnRespawn`, that command runs.
5. **Telefrag**: every other player whose doubled box contains the new position is killed with infinite crush damage credited to themselves (a suicide, S7). This only happens when every point was occupied, and a spawn-protected player (S6) is immune.

So spawns cycle through a team's points in map order, skipping occupied ones; there is no "farthest from enemies" logic and no randomness for team points. If a list is empty the first `GameStartPoint` of the map is used.

### S5. Respawn

- **Which modes respawn**: TDM, VIP Elimination, clan TDM, CTF, clan CTF, One Man Army, Arms Race, Spy Hunt (stage 1 only), Bombing Run, Seize and Secure, Hired Guns, Squad Match 2, Snowball (tickets), the co-op modes (while lives remain) and bot Elimination. Every other mode keeps the dead out until the next round setup.
- **Automatic, no key.** On death the client starts its wait at once (Hunted only: after `DyingTime` 3.5 s of death camera). When the wait has passed (elapsed > 7000 ms; Snowball also needs its team's ticket count > 0), the client sends one respawn request and switches the camera back. A "fast respawn" item lets the client send at once; the server checks the player still has uses left and spends one.
- **Server check**: the request is refused (and logged as illegal) if the mode does not respawn, or if fewer than 5.0 s (Snowball 1.0 s) have passed since death on the server's clock. A perk can lower the server limit: the limit is min(5.0, perk value). A refused request is not retried by the client (it has set its "requested" flag), so a refused player stays dead until the next round setup. The reference build carries edits that set the waits to 2.0 s and 1.5 s.
- **Respawn**: the player gets full health and armour, the loadout is rebuilt from scratch (`inventory.md` section 13: picked-up weapons gone, clips full, primary drawn), gear is re-applied, any carried flag, intel or gold is let go first, and the spawn point is chosen as in S4. The previous body is removed.
- At a round setup everyone on teams 1 and 2 is respawned this way, alive or not.

### S6. Spawn protection

At every respawn (including round setups):

- **All damage blocked for 4.0 s** (`RespawnInvulnerabilityTime`), except damage types a mode exempts (Spy Hunt's stage-2 respawn exempts falling out of the world). Explosions, falls and telefrags do nothing.
- **Instant-hit shots pass through for 3.0 s**: a bullet or melee ray that hits a protected player is ignored before it can hit (no hit, no penetration stop); clients draw no blood and no flinch on a player for 3.5 s after they see the respawn.
- **What ends it early**: firing a weapon ends both parts at once, but only in One Man Army, Arms Race, Last Man Standing and Hunted; everyone is told and the client's no-blood flag clears too. In every other mode a protected player can shoot freely.
- **How it shows**: no visual marker was found besides the missing blood and flinch.
- The reference build carries edits setting both times to 0 (no protection). This spec uses 3.0 s and 4.0 s.

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

- **Mutation boxes** (TDM and One Man Army with the room's mutation option on): every 30 s the point list is shuffled and a box is placed on the first N points, where N comes from the box data by player count (6 up to 6 players, then N = player count, up to 16). A box lives 10 s, is picked up by touch, and gives a random buff or nerf chosen by the toucher's team's score lead.
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
- **Spawns**: `Team Death Match` points. Dead players spectate until the next round.
- Joining: held until the next round (S1).

## M3. Search & Destroy (`TMM`)

- **Teams**: JSF attack (`OffenceTeam`), Creed defend (`DefenceTeam`) on every stock map; sides never swap. **Respawn**: no. **Rounds**: warm-up (S9), then rounds of `TimeLimit` 180 s. **G** = round wins needed.
- **Bombs**: at each real round's setup every living JSF player receives the bomb weapon named by the objective's `JSFWeapon` (`XK-2`, the mission-class weapon, `inventory.md` class 6). It is never dropped on death; leftover bombs are removed at the round's end.
- **Bomb sites**: `BombZoneTrigger` objects with `GameType` `Doobic`, `DefusableTeam` `Creed`, `CanDefuse` true, `DefuseTime` 7.0, `DetonateTime` 40.0, two per map (`ZoneNumber` 0 and 1, sites A and B), each a box (`Dims` half-extents, about 130 × 64 × 125 u).
- **Plant**: an attacker holding the bomb inside a site's box, widened by 50 u on each axis, holds fire; the HUD shows a 4 s plant gauge. When the plant completes, the site's `PlantedCommand` tells the mode controller "plant bomb 40": the bomb timer starts at 40 s, the round timer gets 40 s added to whatever is left, and the commands lock this site and switch the other off. Only one bomb can be planted per round.
- **Defuse**: a defender inside the site uses it (use key) for 7 s (`DefuseTime`; 7 s gauge). Stopping early cancels the defuse. A completed defuse wins the round for Creed; the bomb's detonation at 0 wins it for JSF. A detonation or defuse only counts while the round is Live.
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
- **Capture**: a carrier of the enemy flag who touches their own team's flag zone **while their own flag is at home** scores: team captures +1, player captures +1, and the carried flag goes home. A capture during the warm-up or outside the Live phase does not count.
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
- **Time limit**: G minutes (default 12) for the whole round; the mission file also carries a stage-2 time (`Stage2PlayTime` 180 s, use not traced).
- Spawns: `BountyHunter` list. Leaving: in stage 2 the last of a side leaving ends the round; in stage 1 the round ends when one player would be left.

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
- Spawns: `Quarantine` list (all `JSF`).

## M9. Bombing Run (`SBT`)

- **Teams**: Alpha vs Bravo, **respawn allowed**. Warm-up, then rounds of `TimeLimit` 300 s. **G** = round wins.
- One bomb (`JSFWeapon` and `CreedWeapon` are both `XK-2`, and the mission file's drop object) spawns at a random `Sabotage` drop point each round. Either team picks it up by touch and plants it at the **enemy's** `BombZoneTrigger` (`GameType` `Sabotage`); it is dropped when its carrier dies.
- **Round win**: detonation → the planting team; defuse → the other team.
- **Time-out**: **both teams get a round win**.
- **Match end**: a team has G wins or a team is empty; winner: more wins; if equal, the winner stays whatever the last round had (none after a time-out).

## M10. Seize and Secure (`HQM`)

- **Teams**: Alpha vs Bravo, respawn allowed. Warm-up, then **one** round of G minutes.
- A flag pole is created at the map's flag pole position. A team seizes it by using its own `ActivateTrigger` (`ActivateTrigger_A` for JSF, `ActivateTrigger_B` for Creed; the mission file sets the use time to 5 s). Seizing raises that team's flag from the bottom; the other flag disappears.
- **Score**: the holding team gains 1 point per whole second its flag rises.
- **Win**: a flag that reaches the top (`FlagMoveTime` 120 s of uninterrupted holding) wins at once.
- **Time-out**: a flag still rising gets the `LastOccupyBonus` 50 points; then more points wins; else more enemy kills; else fewer deaths; else draw.

## M11. Hired Guns (`SQM`) and Squad Match 2 (`SQM2`)

- **Hired Guns**: Alpha vs Bravo with AI mercenaries, respawn allowed, warm-up then G rounds played (Elimination Pro timing). Bullion bars are dropped on drop points at round start; players carry bullion in a pocket and bank it; a death drops the pocket; kills and deaths pay bank bonuses from the map data. **Round winner**: more bullion banked; else more round kills; else fewer round deaths. **Match winner**: an empty team loses; else more round wins; else Bravo.
- **Squad Match 2**: a Seize and Secure variant (M10 rules) with no room entry in the US data.

## M12. VIP Elimination (`VTDM`)

- TDM (M1) in which a death is worth a number of points that depends on the victim's VIP rank, from the VIP data. The team counter is the sum of points; the first death that brings a team to G **points** ends the round. Personal VIP points are shown. Respawn as in TDM.

## M13. VIP Escort (`ESC`, clan `CESC`)

- Rounds of `TimeLimit` 180 s, no respawn. The escort team protects a VIP (teleported to the mission's VIP spawn point; VIPs give no assists) to the exit; the other team eliminates. **Win**: VIP escaped → escort team; VIP killed or escort team eliminated → elimination team; **time-out** → elimination team.
- **Sides swap** every R / 2 rounds (R = G; clan rooms every R / 4), with a 3 s notice; spawns follow the swap. Match: first to G round wins.

## M14. Arms Race (`VWRM`)

- One Man Army (M5) with a weapon ladder: at given kill counts each player's weapon is replaced by the next weapon of the map's weapon table (a random roll chooses between the table and the player's own equipment). First to G kills wins. **No death drops** (`inventory.md` 12). Firing ends spawn protection.

## M15. Hunted (`HG`)

- Last Man Standing (M6) with scavenging: players start with little, supply items and weapons drop on drop points, a death-camera hold of `DyingTime` 3.5 s, emotes, the time hunted is tracked and ranks the scoreboard. Spawns: unlocked `HungerGame` points in the warm-up, locked ones in rounds. Firing ends spawn protection.

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
11. **The mission clock (`MissionTimeLimit`) starts but ends nothing.** Do not implement a match time limit for TDM.

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

## Open questions

1. **Elimination naming.** The brief lists "Team Death Match" and "Elimination" as two modes. In this game they are one mode (`TDM`, shown as Elimination); this spec takes the brief's "Elimination" to mean Elimination Pro (`TSV`, M2). Confirm.
2. **Respawn and protection values.** The reference carries marked edits that change the respawn waits (7.0 → 2.0 s client, 5.0 → 1.5 s server) and turn off spawn protection (3.0 s and 4.0 s → 0). This spec uses the earlier values. Check on the live game: time death to respawn in TDM with a stopwatch, and shoot a freshly respawned enemy.
3. **Round phase lengths.** `ObjectEnd`, `MissionEnd` and `ObjectStart` default to 0 when the server file lacks them; only the client's copy of the server file has 3 / 3 / 0. Time the gap between a round's result and the next round's start in S&D (expect ≈ 6 s).
4. **Bomb plant time.** The HUD gauge is 4 s and the zone accepts the plant when the client reports it; the exact hold time and whether moving or releasing fire cancels it are decided by the client's bomb weapon, which was not traced. Measure a plant with a stopwatch, and whether a partial defuse keeps its progress.
5. **Order of objects and the mission update** inside a server frame was not confirmed; this spec assumes deaths are handled before the frame's mission update.
6. **Free-for-all team ids.** The lobby assigns them (Spy Hunt uses the player's room key + 1); confirm every player gets a different id in One Man Army. This also settles `hud_crosshair.md` open question 15 (everyone else is an enemy).
7. **Room-data-only modes** (M22): Big Head, Race Combat, Team Race Combat, Lone Combat, Lone Combat Infected and the skill Elimination have no rules in the reference. They need a later reference or live play.
8. **Last Man Standing winner** (Quirk 5): confirm on the live game whether the match winner really follows the first-player comparison (and the "more deaths" test), or whether a later build fixed it.
9. **Co-op modes** (M19): lives per objective, revive rules and mission scripts were not traced in detail.
10. **Spy Hunt stage 2 time** (`Stage2PlayTime` 180 s) and its time-out winner were not traced.
11. **Join in progress off**: whether the lobby simply refuses joiners once the match has started was not traced (lobby side).
12. **Infected regeneration unit**: `Heal = 0.1` every 1 s after 3 s still; whether 0.1 is a fraction of maximum health or hit points was not traced.
13. **Contradiction check with `hud_crosshair.md` D7**: it lists capture the flag among "round-based modes" with a timer. CTF is one round with a timer of 180 × G s; the timer is right, the label is loose. No change needed there, noted only.
14. **Contradiction check with `grenades.md`**: none. Its open question 11 (grenades at a round's end) is answered in S11 (they keep their fuse; their kills outside the Live phase do not count); its question 14 (M67 knock-back) is specified in S14.
15. **Contradiction check with `inventory.md`**: none; section 12's "no drop in snowball or the VIP weapon race" matches M14 and M16, and section 13's respawn matches S5. Addition: all ground weapons, including map-placed ones, are cleared at each round end (S11).
16. **Contradiction check with `world_dat.md`**: none. Its open question 3 is answered in S12 (`ObjectSpawnPoint` = drop points for mode items, filtered by `GameType`, `Spawn` unused). Note that `GameStartPoint.Spawn` must be exactly 1 for versus modes (S4).
