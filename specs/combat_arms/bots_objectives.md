# Combat Arms: bots and mode objectives

Source basis: the server's per-mode rule sets (which of them create, command or remove AI characters), the AI goal list the engine can build (every goal type it knows), the AI character object's leader and follow-point fields, the follow goal and follow state, the attack-tower goal, the squad-unit manager that player-bought mercenaries belong to, the Hired Guns and Squad Match 2 rule sets' unit messages, the client's mercenary order HUD, the pickup code of the objective items (bullion), the damage and kill-credit path for AI attackers; plus shipped data from a plaintext copy of the BR client's archives: the room data, every mission file of the bot maps (the bot game-mode files and the same maps' objective-mode files), the Hired Guns mission file, the single-play and multiplayer AI goal files, and the AI character records of the Hired Guns world. Continues `bots.md`.
Status: draft

Sections: Summary; Units and conventions; Constants; Which modes have bots (the answer per mode); What the bot game-mode files hold; Behavior (1 why no bot touches an objective; 2 per-mode sections: Search & Destroy, Capture the Flag, VIP Escort, Seize and Secure, Bombing Run, Spy Hunt, Hired Guns, Squad Match 2, the remaining modes; 3 Hired Guns mercenaries: buying, orders, goals, following, fighting, credit, removal); Per-tick order; Edge cases; Quirks; Test cases; Open questions.

---

## Summary

No computer-controlled character in Combat Arms plays a mode objective. The engine has no goal for bombs, flags, intel, flag poles, bullion or a VIP, and the objective items accept only human players. Bots in the `bots.md` sense (patrolling, chasing, shooting soldiers) exist only in Bot Deathmatch (`BDM`) and Bot Training (`BTRN`), which are Team Death Match with no objective at all. The one versus mode with AI on the players' sides is **Hired Guns** (`SQM`, and its variant Squad Match 2): each player may buy up to two **mercenaries** with banked bullion. Mercenaries follow their buyer, hold a spot, or fight whatever enemy they see, and that is all they do: they never pick up, carry or bank bullion. This spec states those facts per mode, so that an implementation does not invent objective AI, and specifies the mercenaries, which `bots.md` does not cover.

## Units and conventions

As in `bots.md`: LithTech world units (u), +Y up; seconds; server frames at 20 Hz (`game_modes.md`); U(a, b) uniform real. Goal importances, decay and "active goal" are the goal machinery of `bots.md` section 2. Mode keys and sections M1–M22 are those of `game_modes.md`; mission-file fields are those of `missions.md`. Players are "they".

## Constants

| Name (ours) | Value | Unit | Meaning |
|---|---|---|---|
| merc cap | 2 | units | Living mercenaries one player may own at once |
| merc cost, units 0–3 | 200 | bullion money | `SpawnCostAI0..3` in the Hired Guns mission file |
| merc cost, units 4–9 | 500 | bullion money | `SpawnCostAI4..9` |
| follow goal importance | 3 | — | Base importance, no decay, no trigger: always "on" |
| ranged attack importance | 23 | — | Decay 2 s, triggered by seeing an enemy (same as `bots.md`) |
| attack-tower importance | 24 | — | Decay 10 s, triggered by a visible enemy guard tower or auto turret |
| follow-only importance | largest finite float (≈3.4 × 10³⁸) | — | Follow goal's importance under the "follow me" order |
| follow arrive radius | 200 | u | 3D distance at which a following merc stops and holds |
| follow leave radius | √(200² + 1000) ≈ 202.5 | u | 3D distance at which a holding merc starts following again |
| follow leave delay | 0.25 | s | A holding merc moves only after the target has been outside the leave radius for this long |
| follow re-plan interval | 1.0 | s | Path to the follow target re-planned at least this often |
| "no follow point" sentinel | (0, 0, 0) | u | A follow point equal to the world origin means "follow the leader" |
| AI-versus-AI damage rate | 1.0 | × | Every `[AIRelations]` rate in the stock Hired Guns file |
| merc kill bonus (killing a merc) | 100 | money | `KillBonusAmount`, banked by the human who killed it |

## Which modes have bots

| Mode (key, `game_modes.md` section) | AI that plays it | Objective behaviour |
|---|---|---|
| Bot Deathmatch `BDM`, Bot Training `BTRN` (M20) | the bots of `bots.md` | none: the mode is Team Death Match, it has no objective |
| Search & Destroy `TMM`, clan `CMM`, Go4CA Cup `ESL` (M3, M17, M18) | none | none |
| Capture the Flag `CTF`, clan `CCTF` (M4, M18) | none | none |
| VIP Escort `ESC`, clan `CESC` (M13) | none (the VIP is a player) | none |
| Seize and Secure `HQM` (M10) | none | none |
| Bombing Run `SBT` (M9) | none | none |
| Spy Hunt `BNT` (M7) | none | none |
| Hired Guns `SQM` (M11) | player-bought mercenaries (section 3) | none: mercenaries ignore bullion |
| Squad Match 2 `SQM2` (M11) | player-bought mercenaries, bought at the team's flag shop | none: they ignore flags and the shop |
| Elimination, Elimination Pro, One Man Army, Last Man Standing, Quarantine, VIP Elimination, Arms Race, Hunted, Snowball (M1, M2, M5, M6, M8, M12, M14–M16) | none | (no objective items, or none relevant) |
| Fireteam, Infected Ship, Cabin Fever 2 and single-player Fireteam (M19, `SCOP`) | scripted PvE enemies | out of scope (`bots.md` 11): their "objectives" are mission scripts, not mode objectives |

How this was established:

1. **Who creates bots.** Bots are spawned only by the Bot Deathmatch rule set (and the Bot Training rule set built on it), which exist only in single-player builds. No other rule set creates an AI character for a player-versus-player mode, except Hired Guns and Squad Match 2 for mercenaries.
2. **What goals exist.** The engine's goal list is fixed: alarm, investigate, chase, attack and its variants (from view, melee, prop, tower, ranged, ranged dynamic), cover, distress, draw weapon, flee, follow, goto, guard, holster, patrol, proximity command, sniper, work, doubt, return, dodge grenade. None of them reads a bomb site, a flag, an intel item, a flag pole, a bullion bar, a bank or a VIP. The bot goal set `BDM_Bot` uses only the default goals plus patrol, work, attack melee and dodge grenade.
3. **What the items accept.** The objective items react to touch by players only: a bullion bar or bag ignores any toucher that is not a human player (the other items are human-only by the rules of `game_modes.md`).
4. **What the mission files say.** Only the bot game-mode files carry AI sections (next section). The bot maps' other mission files (their S&D, CTF, Bombing Run, Seize and Secure, Spy Hunt, VIP Escort and the rest) have no AI section, and the game type is always taken from the file's own `MissionMode` (`missions.md`), so a room on a bot map in an objective mode is an ordinary human-only match.

## What the bot game-mode files hold

There is one bot game-mode file per bot map, named with the suffix `_B`, for Costa Recon, Death Room, Downhill, Junk Flea, Neptune, Oil Rig and Short Fuse, plus the training file for Junk Flea and a Death Room training variant. The room data points `GMS_BDM` of each bot map at its file and `GMS_BTRN` of Junk Flea at the training file; the Death Room training variant's room entry is commented out, so that file is unreachable. The room data's recommended bot count (`ProperPlayer_BDM`) is 6 on Junk Flea, Death Room, Short Fuse, Downhill and Costa Recon and 8 on Oil Rig and Neptune.

Each bot file holds, and only holds, Team Death Match setup for bots:

| Section | Content |
|---|---|
| `[FileOverride]` | `AIGoalPath` and `AIButePath`: the single-play AI goal and attribute files (`bots.md`) |
| `[MapInfo]`, `[Mission0]`, `[Object0]`, `[MapExp]`, `[MapMoney]`, `[Bonus]` | Ordinary mission data (`missions.md`); `MissionMode` is `BDM` (`BTRN` in the training file); `JSFWeapon` and `CreedWeapon` are empty (no objective weapon); `[Object0]` names `OffenceTeam` JSF and `DefenceTeam` Creed, which Team Death Match never reads |
| `[AIStartNodeList]` | `PatrolNode_Bravo0..7` = `AI_Node_B_01..08` and `PatrolNode_Alpha0..7` = `AI_Node_A_01..08` on every map: the patrol start nodes (`bots.md` 4) |
| Oil Rig only | `GuardNode_Alpha0..1` and `GuardNode_Bravo0..1`, all four naming `AI_G_Node_B_01` / `AI_G_Node_B_02` (the same two nodes for both teams); four `[AIGuradSpawnInfo<n>]` records with `AIMax` 2, 6, 7, 8, `GuardSelectRatio` 50, `GuardAIMin` 0, `GuardAIMax` 0, so no bot guards (`bots.md` Quirk 10) |
| training file only | `[Caption<n>]` records (`CaptionType`, `Data`, `PlayTime`, `Message`) for Bot Training captions (`bots.md` 10) |

No section names a bomb site, flag, intel, pole, bullion or VIP; no section is per objective mode. "Per mode" there is exactly one bot file kind: Team Death Match.

## Behavior

### 1. Why no bot touches an objective

An implementation must reproduce the absence:

- A bot's goal choice (`bots.md` 2) never includes an objective. Bomb plants, flag drops, captures and pick-ups post no stimulus to the AI, so no bot reacts to them, not even by investigating: the single-play stimulus set is enemy visible, gunfire, footstep and ally call (`bots.md` 5.1), and none is posted by an objective event. A plant or defuse makes no gunfire stimulus.
- Bot modes cannot be combined with objective modes: a match's rule set comes from its mission file, and the only files that spawn bots declare Team Death Match.
- Bots do not move toward or away from objective volumes. Their paths depend only on AI volumes, patrol nodes and their target (`bots.md` 3, 4, 5.6).

### 2. Per mode

#### 2.1 Search & Destroy (and clan S&D, Go4CA Cup)

No AI. Nobody chooses a bomb carrier (every living attacker receives the bomb, `game_modes.md` M3), nobody but a player plants, guards or defuses. The mode reads no AI volumes or patrol nodes.

#### 2.2 Capture the Flag (and clan CTF)

No AI attacks, escorts, defends, returns or carries a flag. The flag item reacts to living players only.

#### 2.3 VIP Escort (and clan VIP Escort)

No AI. The VIP is a human player teleported to the VIP spawn point (`game_modes.md` M13); neither side has computer escorts or hunters.

#### 2.4 Seize and Secure

No AI uses the `ActivateTrigger_A` / `ActivateTrigger_B` seize triggers; using them needs a player's use key.

#### 2.5 Bombing Run

No AI picks up, carries or plants the shared bomb.

#### 2.6 Spy Hunt

No AI picks up intel or becomes a spy.

#### 2.7 Hired Guns

AI exists (section 3) but never picks up a bullion bar or a dropped bullion bag: bullion items ignore any toucher that is not a human player, so a mercenary walking over bullion leaves it on the floor. Mercenaries have no bank and no pocket. A mercenary killed drops nothing. Bullion scoring, banking and the carry penalty are player-only (`missions.md` F4.5).

#### 2.8 Squad Match 2

The Seize and Secure variant reached through the Hired Guns 2 map (`missions.md` Quirk 1) accepts the same mercenary orders as Hired Guns; buying needs the buyer to stand inside their own team's flag shop area and the unit to be on that flag's list (section 3.1). Mercenaries never seize the pole.

#### 2.9 The remaining modes

No AI in any other versus mode. Co-operative modes' PvE enemies are scripted by mission data and are not objective players in this sense (`bots.md` 11).

### 3. Hired Guns mercenaries

#### 3.1 Buying a mercenary

A player opens the mercenary shop on their HUD and buys unit i (0–9). The server, on receiving the request, in this order:

1. Refuses if the player already owns 2 mercenaries.
2. Looks up the cost `SpawnCostAI<i>` (stock 200 for i = 0–3, 500 for i = 4–9); refuses if there is none.
3. Takes the cost from the player's **bank** (not their pocket); refuses if the bank holds less than the cost. A purchase lowers the bank only: the round winner is decided by bars banked, not by the bank balance (`game_modes.md` M11), so buying does not cost the team score.
4. Picks the spawn object by the player's team: the single object named `UNITSPAWN_ALPHA` for Alpha, `UNITSPAWN_BRAVO` for Bravo.
5. Spawns the AI character template named `AI<i>` in `[AIRelations]` at that object's position and rotation, gives it the buyer as **leader** and the buyer's team, and records unit id i.

The checks of steps 4–5 come **after** the bank is charged (Quirk 2). Hired Guns does not check the round phase, the buyer being alive, or where the buyer stands. Squad Match 2 differs: the buyer must stand inside the shop box of a flag of their own team whose list contains unit i and which has at least one mercenary spawner; the bank must hold the cost; the same 2-unit cap applies; the mercenary appears at one of that flag's spawners chosen uniformly at random; and the bank is charged **only if** the spawn succeeded. A unit whose template name is `Weapon0` or `Weapon1` is not a mercenary but a placed weapon device created by the flag (Open question 4).

Stock Hired Guns templates (from the world's character records): unit 0 `Squad_1` and 1 `Squad_2` are assault riflemen (brain `Squad_AR`), 2 `Squad_3` and 3 `Squad_4` SMG gunners (`Squad_SMG`), 4 `Squad_5` and 5 `Squad_6` SMG gunners, 6 `Squad_7` and 7 `Squad_8` assault riflemen, 8 `Squad_9` and 9 `Squad_0` machine gunners (`Squad_MG`). All use the goal set `Squad`. Their attribute templates (accuracy, reaction, senses) come from the multiplayer AI attribute file and are not specified here (Open question 6).

#### 3.2 Orders

The squad (all mercenaries of one player) has one **order**, shown on the buyer's HUD; it starts as Default and returns to Default when the player's last mercenary is gone. Orders apply to every mercenary the player owns, and a newly bought one takes the current order. Each order sets two things per mercenary, the follow goal's importance and the **follow point**:

| Order | Sent by the shipped client | Follow importance | Follow point |
|---|---|---|---|
| Default | yes | 3 (data value) | none (0, 0, 0) |
| Follow me | yes | largest float | none |
| Hold position | yes | 3 | the mercenary's own position when the order arrives (a newly bought one: its spawn position) |
| Attack move to P | **no** (the server accepts it; nothing in the client sends it) | 3 | P |

The order HUD has three entries (hold, default, follow me), opened by the "NPC order" command.

#### 3.3 Goals and priorities

The `Squad` goal set has exactly three goals (it does not include the default goal lists):

| Goal | Importance | Trigger | Does |
|---|---|---|---|
| Attack tower | 24, decay 10 s | an enemy guard tower or an enemy player's auto turret visible from the eye within the mercenary's ranged attack range | shoot it until destroyed |
| Attack ranged (dynamic) | 23, decay 2 s | seeing an enemy | the `bots.md` attack: reaction delay, hit/miss cycle, bursts, attack moves |
| Follow | 3, no decay (or the largest float under "follow me") | none: always on | section 3.4 |

So with Default or Hold, a mercenary fights whatever enemy it sees, enemy players and enemy mercenaries alike, and turrets first; when the attack goal decays (2 s after the last sight trigger, `bots.md` 2), it goes back to following. There is no chase, investigate, patrol, dodge-grenade, draw-weapon or melee goal, so a mercenary never chases a target out of sight, never reacts to a sound, never dodges grenades and never knifes. Under **Follow me** no other goal can outrank the follow goal, so the mercenary only runs after its leader and **never fires** (Quirk 3).

Difficulty: Hired Guns has no difficulty setting for mercenaries. The mission file's `AIDamageFactor` (1.0 stock) scales damage done by AI; AI-versus-AI damage is scaled by the `[AIRelations]` rate for (attacker unit, victim unit), 1.0 for every stock pair.

#### 3.4 Following

Each update while the follow goal is active (state "follow", awareness alert, running):

1. The mercenary's target is forced to its leader.
2. The goal point G is the follow point if it is not (0, 0, 0), otherwise the leader's current position (its true position, not a remembered one).
3. With d = the 3D distance from the mercenary to G:
   - **Following**: if d < 200 u, stop (path cleared) and switch to **holding**; otherwise move along a volume path to G (`bots.md` 3), re-planning when the path ends or every 1.0 s, whichever comes first.
   - **Holding**: stand facing the leader (when following the leader). While d ≤ 202.5 u a 0.25 s timer keeps restarting; once d > 202.5 u and the timer has run out, plan a path to G and switch to following.
4. If no path to G can be planned, the mercenary holds where it is and tries again on later updates.

In world terms: mercenaries trail 200 u behind a moving leader, start moving about 0.25 s after the leader gets 202.5 u away, and re-aim their path at least once a second. Under Hold position they return to within 200 u of their hold spot after every fight.

#### 3.5 Credit, death and removal

- A kill made by a mercenary is credited to its **leader** as the killer (and the kill message marks it as a squad kill), so the leader's kill bonus and the team's round kills follow the player rules.
- A human who kills an enemy mercenary gets the kill counted (kills, enemy kills, round kills of their team, a headshot if the last hit was to the head outside the warm-up) and banks `KillBonusAmount` (100), and the same amount in mission points. Damage below 0.001 on the killing hit gives nothing.
- A mercenary leaves its owner's squad (freeing a slot) when its object is removed; when the last one goes, the order resets to Default.
- At every round's end all AI characters are removed (along with all bullion). Mercenaries do not carry over to the next round.
- When the owner leaves the match their mercenaries are removed at once.

## Per-tick order

Nothing in any mode's objective update reads or writes AI. For mercenaries, the bot update of `bots.md` (Per-tick order) applies unchanged, with the goal set above; the follow state runs in step 2.10 of that list. A buy or order message is handled when it arrives, between frames, and takes effect at the mercenary's next update.

## Edge cases

- **Bot map, objective mode** (e.g. Short Fuse S&D): no bots spawn; the match is human-only.
- **A bot (BDM) touching an objective item left in the world**: impossible in practice (objective items are visible and active only in their own game type); if one existed, its pickup ignores non-players.
- **Mercenary on bullion**: walks over it; the bar stays.
- **Leader dead**: the follow target is still the leader's character object; mercenaries keep following its position (Open question 3).
- **Hold position given while a mercenary is mid-fight** (moved by attack moves): its hold spot is where it stood when the order arrived, not where it was bought.
- **Attack-move point or hold spot exactly at the world origin**: treated as "no follow point", so the mercenary follows the leader instead (Quirk 4).
- **No `UNITSPAWN_*` object, or more than one, for the buyer's team**: no mercenary appears but the cost is already spent (Quirk 2). The stock Hired Guns world has exactly one of each.
- **Observer buys**: charged, then refused (Quirk 2).
- **Follow-only squad under fire**: does not shoot back.

## Quirks

1. **No objective AI anywhere.** Keep: do not add bot objective play when porting these modes; a Combat Arms objective match is human-only.
2. **Mercenary purchase charges before the spawn checks.** A purchase that then fails (no spawn object for the team, template not spawnable, buyer on no team) loses the money. Keep only if matching the original exactly; harmless with stock data.
3. **"Follow me" silences mercenaries.** The order gives the follow goal the largest float importance, which the attack goals (23, 24) can never beat, so mercenaries following only their leader never fire. Keep (players use it to run mercenaries past fights).
4. **Origin means "none".** A follow point of exactly (0, 0, 0) is read as "follow the leader". Keep.
5. **Attack-move unreachable.** The server implements an attack-move order but the shipped client never sends it. Keep the server side; no UI needed.
6. **The follow goal's own range keys are ignored.** The follow goal reads a range parameter (default 100 u) but the follow state always uses its own 200 u. Keep 200 u.

## Test cases

1. **Bot mode files.** Load the room data's `GMS_BDM` for each of the seven bot maps → each file's `MissionMode` is `BDM`, its objective weapons are empty, and it has `[AIStartNodeList]` with 8 Alpha and 8 Bravo patrol start nodes `AI_Node_A_01..08`, `AI_Node_B_01..08`.
2. **Training file.** `GMS_BTRN` of Junk Flea → `MissionMode` `BTRN`, the same 16 patrol start nodes, plus caption records.
3. **Oil Rig guards stay off.** Oil Rig bot file, 8 bots → guard selection: maximum 0 for every bot-count record (2, 6, 7, 8) → no bot gets a guard goal.
4. **Objective file has no AI.** Short Fuse S&D mission file (`_M`) → no `[AIStartNodeList]`, no `[FileOverride]`; starting it spawns no AI character.
5. **No bots in S&D.** Single-player build, Short Fuse, S&D room with the room's bot count set → the match runs the S&D rule set and no bot is created.
6. **Bot ignores a plant.** Hypothetical harness: BDM bot patrolling 300 u from a bomb site, a human plants a bomb there (rules of M3 forced on) → no stimulus is posted, the bot's active goal stays Patrol, its path is unchanged.
7. **Bot goal set.** Build `BDM_Bot` from the single-play goal file → the goal list contains no goal type other than those of `bots.md` 2; none reads an objective object.
8. **Merc buy, cheap unit.** Hired Guns, Alpha player with bank 1000, no mercenaries, buys unit 0 → bank 800, an assault-rifle mercenary (`Squad_1`) appears at `UNITSPAWN_ALPHA`, team Alpha, leader = that player, order Default.
9. **Merc buy, expensive unit.** Same player (bank 800, 1 mercenary) buys unit 9 → bank 300, a machine gunner (`Squad_0`) appears; a third purchase (unit 0) is refused, bank stays 300.
10. **Merc buy, too poor.** Bank 150, unit 0 (cost 200) → refused, bank 150, no mercenary.
11. **Merc ignores bullion.** Mercenary following its leader across a bullion bar → the bar remains, the leader's pocket and bank are unchanged.
12. **Default order, following.** Leader stands still 600 u away in an open AI volume, no enemy in view → the mercenary runs toward them and stops when it comes within 200 u (3D).
13. **Leave delay.** Mercenary holding at 190 u; the leader steps to 210 u at t = 0 → the mercenary starts moving at t ≈ 0.25 s (first update after the timer); at 201 u it would not move.
14. **Re-plan rate.** Leader running in a circle while the mercenary follows → the mercenary's path destination is refreshed at least every 1.0 s.
15. **Fighting.** Default order, an enemy player comes into view at 800 u → the ranged attack goal (23) beats follow (3); the mercenary attacks with the `bots.md` rules; 2 s after the enemy is last seen (decay) it resumes following.
16. **Turret first.** Default order, an enemy auto turret and an enemy player both visible within ranged attack range → the mercenary shoots the turret (24 > 23).
17. **Follow me.** Order "follow me", an enemy shoots the mercenary from 500 u in plain view → the mercenary keeps following its leader and fires no shot.
18. **Hold position.** Mercenary at (1000, 0, 500) when the hold order arrives; the leader runs 2000 u away → the mercenary stays within 200 u of (1000, 0, 500); after a fight that moved it 300 u, it walks back to within 200 u of the spot.
19. **Origin quirk.** Hold order given while the mercenary stands exactly at (0, 0, 0) → it follows the leader as if under Default.
20. **New unit takes the order.** Order "follow me" active with one mercenary; buy another → the new one also has follow-only importance.
21. **Kill credit.** A mercenary kills an enemy player → the kill is credited to its leader (the leader banks 100, the leader's team gets the round kill).
22. **Killing a merc.** A Bravo player kills an Alpha mercenary → the Bravo player banks 100, gains a kill and an enemy kill, and Bravo's round kills rise by 1.
23. **Round end.** Round ends with 2 mercenaries alive → both removed; next round the player's squad is empty and the order is Default.
24. **Owner leaves.** Player with 2 mercenaries leaves the match → both removed at once.

## Open questions

1. **Contradiction with `bots.md`.** Its "Does the game have bots" table lists only `BDM`/`BTRN` bots, PvE enemies and turrets, and its section 11 does not mention Hired Guns mercenaries; the reference also runs AI goals (attack, follow, attack tower) in the multiplayer Hired Guns and Squad Match 2 modes, on players' teams. `bots.md` should add a row: "Hired Guns, Squad Match 2: player-bought mercenaries, see `bots_objectives.md`". No other contradiction with `bots.md`, `game_modes.md` or `missions.md` was found; `game_modes_2.md` had not landed when this was written and was not checked.
2. **When a dead mercenary frees its slot.** A mercenary leaves its owner's squad when its object is destroyed, not at the moment of death. Whether the AI object is destroyed at death (body handed to a separate object) or after its body lifetime decides how soon a player can buy a replacement. Check in the live game: buy 2, lose one, try to buy at once.
3. **Leader dead or respawning.** Whether mercenaries keep following the dead leader's body position, stand still, or follow the respawned leader once they reappear at a spawn point was not traced.
4. **Squad Match 2 shop data.** Which units each Hired Guns 2 flag lists, its spawners and shop boxes, and what the `Weapon0` / `Weapon1` devices are, live in that map's mission file and were not read; they need a section in `missions.md` or here before Squad Match 2 mercenaries are implemented.
5. **Do mercenaries keep the sight "notice" rolls?** They have no patrol or investigate state, so by `bots.md` 5.2 their sight should be deterministic; confirm their reaction to an enemy appearing at long range.
6. **Mercenary attributes.** The `Squad_AR`, `Squad_SMG` and `Squad_MG` brains and the `Squad_0..9` attribute templates live in the multiplayer AI attribute file (not the single-play one `bots.md` reads); their reaction, accuracy and per-weapon rows need their own table before mercenaries are implemented.
7. **Spawn protection and respawn.** Mercenaries never respawn (none of the rule sets re-creates them); whether a freshly bought one is invulnerable for a moment like a bot (`bots.md` 10) was not traced.
8. **Bomb-site or flag AI in other builds.** Only the BR-era code and data were read. If a later build (for example the Steam one, whose mission files include extra suffixes `missions.md` lists as unread) added objective bots, this spec would not know; check its goal list for new goal types.
