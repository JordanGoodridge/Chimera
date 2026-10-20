# Combat Arms: lobby, rooms and menus (offline and LAN)

Source basis: the client's menu screens (connecting screen, top navigation bar, server and channel select, room list, room creation popup, waiting room, inventory, shop, result screen, in-game menu, and the developer-only LAN screens), the client's room data manager and its arms-option list, the client's local single-player hosting path, and the lobby service's room, channel, mode-rule, team-balance, timer, inventory and match-launch handling; checked against the room data file, layout attributes file, item and shop-list files and US string table of a US-region data set, and against the menu layouts of a classic-client data set. Where the lobby service's rules differ by region, the branch the reference marks as the US one is used and the others are noted. The Steam build was not read.
Status: draft

Sections: Summary; Units and conventions; Constants; Behavior (F: flow and what needs the online service; D: the data the lobby reads; N: connecting screen and top navigation; V: server and channel choice; L: room list; C: room creation; R: the waiting room; O: room options; T: teams; S: ready, start and countdown; J: joining a match in progress; K: host, kicking, chat; E: loadout; P: shop and item lifetime; G: result screen; X: in-game menu and leaving a match; Q: developer LAN screens; M: events); Order of events; Edge cases; Quirks; Answers to other specs' open questions; Test cases; Open questions.

---

## Summary

- The game has no classic main menu. After a connecting screen and the server and channel choice, the player sits in the **room list** ("Game Lobby"); a top navigation bar (Lobby/Gameroom, My Character, Shop, CLAN, Jobs, Quick Join, option, menu) is the menu. A match is always played from a **room** (the "waiting room"): one player is the host, everyone else presses READY, the host presses START, a 5-second countdown runs, and everyone who was ready is sent to a game server. After the match a result screen shows for up to 60 s, then everyone is back in the same room.
- Almost every rule about rooms is enforced twice: once by the client (which hides or disables what it will not allow) and once by the lobby service (which refuses requests). For offline and LAN play our implementation must play the service's part; this spec says for each rule which side decides. Login, the server and channel lists, shop prices, item ownership and expiry clocks, the open map/mode table, clans, friends, premium items and quick join all come from the service and have no offline equivalent in the reference except the single-player ("PLAY WITH BOTS") path and developer LAN screens.
- What a player carries into a match is decided in the lobby: the inventory screen's loadout (two separate sets, normal and "Pure Combat"), checked against the room's "1st Arms" restriction when the player readies, then filtered again by the game server when it builds the in-match inventory (`inventory.md` section 1).

## Units and conventions

- Screens are menu screens at 1024 x 768 (`ui_system.md` S1, M1). Layout files are named by their file name inside the game's archives (`ui_system.md` L1). Control names quoted in this spec (`button_game_ready`, `drop_mode` …) are names inside those layouts and are data.
- Displayed text is quoted from the US string table; the string id is given where a layout uses it (`ui_system.md` R1, R2).
- Time: the lobby service runs room timers on a **1 s tick**. The client uses a millisecond wall clock. "Count" means the room's start counter, incremented once per lobby tick while it runs.
- **Mode id** is the index of the `[Mode#]` section in the room data file (0 = `TDM` … 19 = `HG`, 20–25 room-data-only modes); clan rooms use the `[ClanMode#]` list instead. **Map id** is the index of the `[Map#]` section; 999 is the random-map pseudo-map. Room keys and game types are as in `game_modes.md` "Modes found".
- **Room teams** in the lobby are Alpha and Bravo. In the match they become team 1 (`JSF`) and team 2 (`Creed`) (`game_modes.md` S1).
- **Player states** in a room (our names): *waiting*, *ready*, *starting* (picked for a match that is counting down), *in game*. Independently a waiting player has an extra state: *none*, *outfitting* (in the inventory, shop, clan or jobs screen) or *idle* (no input for 60 s). The status column shows "Ready", "Starting", "In game", "Outfitting" (`IDS_SHOP_STATE`) or "Idle" (`IDS_SLEEP_STATE`), else nothing.
- **Room states**: *waiting*, *starting* (countdown and game-server launch), *in game*, *closing* (the game server has said the match is near its end, `game_modes.md` S1 late-join closing). The room list shows "Waiting", "Starting" and "In game" (closing also shows "In game").
- "Host" is the game's room master. "Pure Combat" (`des_LobbyFilter_official`, tag `[PCM]`, `IDS_OFFICIAL`) is the game's official-rules room variant.

## Constants

| Name (ours) | Value | Unit | Meaning |
|---|---|---|---|
| connect_timeout | 90000 | ms | Connecting screen gives up and quits with "failed to connect" |
| to_server_select_delay | 3000 | ms | After login succeeds, wait before showing server select |
| crowded_percent | 95 | % | A server at or above this fill is skipped by automatic server choice |
| rooms_per_channel | 100 | rooms | Room ids 0–99 per channel; the list shows id + 1 |
| room_players_max | 16 | players | Hard cap for any room; channels can set a lower room limit |
| room_name_max | 32 | characters | Longest name the service stores (longer: refused) |
| room_password_max | 8 | characters | Longest password the service stores |
| room_name_box | 15 | MaxLength | Name edit box limit in the creation layout (see Open questions) |
| room_password_box | 3 | MaxLength | Password edit box limit in the creation and join-password layouts (see Open questions) |
| team_rows | 8 | rows | Alpha list and Bravo list each show 8 rows of 25 ref px |
| one_side_alpha | 8 | players | One-team modes put the first 8 players on Alpha, the rest on Bravo |
| idle_after | 60000 | ms | No mouse or key input in the waiting room → *idle* |
| countdown_ticks | 5 | ticks | Countdown messages before the match may launch |
| countdown_display | 5 − count | s | Number shown: 5, 4, 3, 2, 1 |
| launch_timeout | 5 + 10 = 15 | ticks | At this count the service tells the launcher to stop the game server (the 10 is a service setting, default 10) |
| launch_give_up | 15 + 2 = 17 | ticks | At this count the room returns to *waiting* (the 2 is a service setting, default 2) |
| client_go_timeout | 15000 | ms | After the last countdown message, a *starting* player who got no "go" gives up |
| invite_max | 5 | players | Lobby players reached by one invite |
| result_timeout | 60000 | ms | Result screen returns to the room on its own (`ui_system.md` constants) |
| chat_max | 120 | characters | Longest lobby/room chat line the service accepts |
| backpack_slots | 4 | slots | Backpack slots A–D; how many are usable depends on the backpack and character |
| loadout_pages | 2 | sets | Normal set and Pure Combat set |
| ability_slots | 3 (4 with an extender item) | slots | Ability (perk) slots |
| permanent_period | 999 | days | Shop period that means "never expires" |
| rental_item_offset | 100000000 | item number | Rental copies of an item use item number + this |

## Behavior

### F. Flow and what needs the online service

Screens, in the order a player meets them (layout file of each in brackets):

1. **Connecting screen** [`LOGIN_PROGRESS.XML`]: shown at start while the launcher hand-off and the login run. **Service.**
2. **Server and channel select** [`SERVERSELECT.XML`]: pick a server, then a channel. **Service.**
3. **Room list** ("Game Lobby") [`GAMELOBBY.XML`, `POPUP_LOCKROOM.XML`]: browse, filter, sort, join or create rooms. Rooms live in the lobby service, one list per channel.
4. **Room creation popup** [`POPUP_CREATEROOM.XML`, `POPUP_CREATEROOM_EXPASS.XML` for premium accounts, `TOOLTIP_CREATEROOM.XML`].
5. **Waiting room** [`GAMEROOM.XML`]: teams, ready, room settings.
6. **Inventory** ("My Character") [`INVENTORY.XML` with `INVENTORY_CHA_WPN.XML`, `INVENTORY_CHA_GEA.XML`, `INVENTORY_CHA_COS.XML`, `INVENTORY_CHA_FNC.XML`, `INVENTORY_CHA_PERK.XML`, `INVENTORY_INVENTORY.XML` and their tooltips]: loadout.
7. **Shop** [`GUNSMITHSHOP.XML` and the `INVENTORY_SHOP*.XML` panels].
8. **Loading screen and match** (`ui_system.md` H).
9. **Result screen** [`RESULT.XML`, `RESULT_PLAYERCHART.XML`, `RESULT_NOR_TEAM.XML`, `RESULT_NOR_NOTEAM.XML`, `RESULT_FRT.XML`, `RESULT_MVP.XML`, `POPUP_RESULT_ABUSING.XML`], then back to 5.

What needs the online service and what our implementation must stand in for:

| Part | In the reference | Offline / LAN stand-in we need |
|---|---|---|
| Login, account, nickname | service | a local profile name |
| Server list, channel list, crowding, entry requirements (level, kill/death ratio, rank) | service | one local "server" with one channel, no requirements |
| Room list, rooms, teams, ready, start rules, countdown | lobby service | a lobby host (on the LAN host or in-process) implementing R, O, T, S, J, K |
| Which modes and maps are open, default map per mode, Pure Combat list | service table, overriding the room data | the room data file alone (D) |
| Launching a game server and handing out its address | service + launcher | our dedicated server or an in-process server |
| Item ownership, prices, purchases, expiry clock, gifts | service | a local item list; durations as data (P) |
| Loadout storage | service (per account, two pages) | local profile storage |
| Premium items (Elite Moderator kick, second arms, random map, gold/monthly passes) | service | none: treat as not owned (Open questions) |
| Clans, friends, invites across servers, quick join, ranks, rewards | service | not needed |
| Single play ("PLAY WITH BOTS") | client hosts a local server; the service only records the room and rewards | the same local path, without the service |

### D. The data the lobby reads

- **Room data file** (`GAMEROOM.TXT` in the attributes archive): `[Map#]` records (key, name id, icons, description, `Enable`, and one `GMS_<ModeKey>` entry per mode the map supports, naming its mission file; `DefaultProperPlayer`, `ProperPlayer_<ModeKey>`, `MapSize`, `Complexity`, `Weather` for the map panel), `[Mode#]`, `[ClanMode#]`, `[Tutorial#]` and `[OverrideMode#]` records. Mode fields used here: `Key`, `ModeFullName`, `GameType`, `SortingOrder`, `DefaultMapID`, `MinPlayers`, `MaxPlayers`, `PlayersInc`, `RecommendMaxPlayers`, `GoalMin`, `GoalMax`, `GoalInc`, `DefaultGoal`, `EnablePlay`, `SinglePlay`, `DefaultDifficulty`, `EnableWeaponUseSetting`, `EnableJoinCombatSetting`, `EnableTeamKillSetting`, `EnableViewEnemyCameraSetting`, `EnableMutationSetting`, `EnablePerkSkillSetting` (absent means on; the other `Enable…Setting` flags absent mean off), `Description`, `RoomIconCtrlName`, the `_OFFICIAL` variants. `StartCount` is present in some records and read by nothing.
- **Which modes are offered**: online, the service sends a table of open modes per map and the default map per mode; that replaces `EnablePlay` and `DefaultMapID`. Offline, use the file: a mode is offered when `EnablePlay` is TRUE (and `SinglePlay` matches the tab, C); the mode list is sorted by `SortingOrder`, ascending.
- **Which maps a mode offers**: maps whose record has `Enable` TRUE and a `GMS_<ModeKey>` entry for the mode's key. The map dropdown is sorted by the client's map order (by name).
- **Arms options**: the `[ArmsOption]` section of the layout attributes file, eleven entries 0–10 with a name, icon, item-type label, display order and option type (1 = "only" options, 2 = "except" options):

| Value | Name (US) | Option type | Display order |
|---|---|---|---|
| 0 | All Permitted | – | 0 |
| 1 | Pistols Only | only | 7 |
| 2 | Melee Only | only | 8 |
| 3 | No Sniper Rifles | except | 1 |
| 4 | No Explosives | except | 3 |
| 5 | No Backpack | except | 4 |
| 6 | Sniper Only | only | 5 |
| 7 | Shotguns Only | only | 6 |
| 8 | No Shotguns | except | 2 |
| 9 | Duty Roster | (never listed) | 9 |
| 10 | Only Go4CA Cup Item | only | 10 (listed only in the Go4CA Cup mode) |

  Dropdowns list the options by display order. "Duty Roster" is hidden from every list in this build.
- **Mode names** (US, `Mode_FullName_#`): 0 Elimination, 1 Search & Destroy, 2 Fireteam, 3 One Man Army, 4 Capture the Flag, 5 Spy Hunt, 6 Snowball Fight, 7 Quarantine Regen, 8 Elimination Pro, 9 Last Man Standing, 10 Bombing Run, 11 Seize and Secure, 12 Hired Guns, 13 S&D for Go4CA Cup, 14 Arms Race, 15 VIP Escort, 16 VIP Elimination, 17 Elimination (bot play), 18 Fireteam (single play), 19 Hunted, 20 Big Head, 21 Race Combat, 22 Team Race Combat, 23 Lone Combat, 24 Lone Combat Infected. Mode 25 (`TDMSKILL`) has no name string.

### N. Connecting screen and top navigation

- **Connecting screen**: shows "Connecting…" and appends log lines. If login has not finished 90000 ms after the screen gained focus, the client quits with a connection-failure message. When login succeeds the client waits 3000 ms, then shows server select. Offline: skip this screen.
- **Top navigation bar** [`BACKGROUND.XML`], drawn over every lobby screen except the connecting, nickname, result and login screens. On server and channel select its buttons are disabled.

| Control | Shown as | What it does |
|---|---|---|
| `button_lobby` | Lobby | From a room: send leave-room (K); the room list follows on the reply. Elsewhere: show the room list. |
| `button_gameroom` | Gameroom | Shown instead of Lobby while the player is in a room; returns to the waiting room |
| `button_character` | My Character | Inventory (E). From the waiting room first un-readies the player, then sets *outfitting* |
| `button_gunsmith` | Shop | Shop (P), same un-ready and *outfitting* as above |
| `button_clan` | CLAN | Clan screen (service; same *outfitting*) |
| `button_quest` | Jobs | Jobs screen (service; same *outfitting*) |
| `button_quickjoin` | Quick Join | Service matchmaking: leaves the channel, searches servers and rooms, creates a default room if none fits. Not needed offline |
| `button_option` | option | Quick-join options popup |
| `StartButton` | menu | Start-menu popup [`POPUP_STARTMENU.XML`] |
| `button_system` | (icon) | Exit popup [`POPUP_EXIT.XML`], quits the game on confirm |

- Returning to the waiting room from any of the *outfitting* screens sends "extra state none".

### V. Server and channel choice

- Layout [`SERVERSELECT.XML`]: tabs `tab_all` / `tab_regular` / `tab_clan` (All / Regular / Clan servers); server list rows (name, clearance level, status), a requirements panel (level, kill/death ratio lock, rank, GP and EXP bonus), a channel list (name, status), buttons Region list, Refresh (`button_server_refresh`), JOIN (`button_server_join`), and a friends panel.
- **Status words** come from fill = users × 100 / max (integer). Each server carries four thresholds packed by the service; the first threshold the fill does not exceed picks "Low", "Moderate", "Busy" or "FULL". A server or channel with no name or max 0 shows as not connectable or "Checking".
- **A server can be entered** when it is up, not full (users < max), the player's level, kill/death ratio and rank fit its limits, and (for automatic choice) fill < 95 %. **A channel can be entered** when it is open and users < max; otherwise "This channel is too full at this time. Please try to join a different channel." Entering a channel leads to the room list.
- Offline / LAN: one implicit server and channel; skip the screen and go straight to the room list. Room list's "Change Server" (`button_toServer`) leaves the channel and returns here.

### L. Room list

Layout [`GAMELOBBY.XML`]: window `gameLobby`, list `RoomInfoList` (741 x 515 ref px, rows 25 ref px tall, cloned from `column_RoomInfo` or, for rooms the player cannot enter, `column_RoomInfo_Disabled`; clan variants on clan servers), tabs `tab01` "Game Lobby" and `tab02` "Clan Match Lobby" (service), buttons `button_toServer` "Change Server", `button_create` "Create Room", `button_Join` "JOIN", and a hover panel `game setting`.

**Columns** (header buttons sort):

| Header control | Shown as | Cell | Content |
|---|---|---|---|
| `Sort_Num` | NO. | `Room_Num` | room id + 1 |
| `Sort_Title` | TITLE | `Room_Title` | room name; a lock icon `passLock` if it has a password |
| `Sort_Map` | Map | `Room_Map` | map name, or "Random Map" for random-map rooms |
| `Sort_Mode` | Mode | `Room_Mode` | mode name, prefixed "[PCM] " for Pure Combat rooms |
| `Sort_Mutation` | (icon) | `Icon_Mutation` | shown when Super Soldiers is on |
| `Sort_Arms` | Arms | `Room_Arms_icon`, `Room_Arms` | icon of the 1st Arms option; text "Multi" when both 1st and 2nd Arms are set |
| `Sort_Players` | Player(S) | `Room_Players` | "users/max", e.g. "3/16" |
| `Sort_Status` | Status | `Room_Status` | "Waiting" (cyan), "Starting" (blue), "In game" (green); grey when the room cannot be entered |

- **Can the player enter** (client test, drives the grey style and the Joinable filter): the room is not full (users < max), is not *closing*, and, if it is *starting* or *in game*, has Join Mid Combat on. A greyed row also shows a "no" status icon.
- **Filters** (checkboxes, any combination, applied on every re-sort): `Check_join` "Joinable" hides rooms the player cannot enter; `Check_pw` "No Password" hides locked rooms; `Check_official` "Pure Combat" shows only Pure Combat rooms; `mode_combo` shows one mode only (and only rooms whose Pure Combat flag matches the chosen entry); `map_combo` shows one map only (choosing a mode resets the map filter to all). Clicking the label next to a checkbox toggles it.
- **Sorting**: clicking a header sorts by that column. The sort direction is one shared value for the whole list, not per column: it starts at "none", and each header click turns "none" into ascending and then flips ascending and descending. Keys: NO. by id; TITLE by name, case-insensitive; Map by name as text; Mode by mode id (not by name); Super Soldiers off before on; Arms by display order; Player(S) by max, then by current count; Status by state (waiting, starting, in game, closing). Descending is the ascending order reversed. After sorting, rooms owned by an Elite Moderator host are moved to the top, higher moderator level first, keeping the other order (stable).
- **Refresh**: there is no refresh button. The full list is requested when the screen gains focus; after that, room-created, room-destroyed and room-changed notices from the lobby keep it live. A created room is inserted and the list re-sorted keeping the scroll position; a changed room is updated in place without re-sorting (except a Pure Combat flag change). Notices that arrive while another screen is current are ignored; the list is rebuilt on return.
- **Selecting and joining**: a click selects a row (JOIN becomes enabled); a double-click selects and joins. JOIN with no selection: "Select a game room that you wish to enter." JOIN on a room the client test refuses: "You cannot enter the room that you have currently selected. …". A locked room opens the password box [`LockGameRoom` window, `input_password`]; Enter or `button_ok` sends the join with the typed password, `button_cancel` closes it.
- **Join reply errors** (lobby decides, in this order): room *closing* → refused; password mismatch (exact, case-sensitive comparison) → "You enter the wrong password,please retry."; room full → refused; room not *waiting* and Join Mid Combat off → refused; joiner blocked by this host's Elite Moderator kick → "You have been kicked from this room. Please select another room to enter."; anything else → the generic "cannot enter" message. Success opens the waiting room.
- **Hover panel** (`game setting`): name, map and its description, mode, goal, players, 1st and 2nd Arms, Super Soldiers, and On/Off for Join Mid Combat, Swap Teams, Friendly Fire, Kill Cam, Abilities.
- Keys: Enter in the password box confirms. Mouse wheel scrolls the list.

### C. Room creation

Popup [`POPUP_CREATEROOM.XML`], window `CreateWnd`, title "Create Room", opened by "Create Room". Two tabs: `tab_multi` "MULTIPLAYER" and `tab_single` "PLAY WITH BOTS (Single)" (enabled only when at least one mode has `SinglePlay` TRUE).

Fields, top to bottom, with what each offers and its default when the popup opens:

| Field (control) | Shown as | Values | Default |
|---|---|---|---|
| Room name (`Edit_RoomName`) | Name | free text | one of five titles picked by the millisecond clock: "Welcome to Combat Arms: the Classic", "Weapons Hot!", "Nowhere to Hide", "Armed & Dangerous", "Engaging the Enemy" |
| Password (`UsePW` check + `Edit_RoomPW`) | Password | off, or text | off, empty |
| Mode (`drop_mode`, info button `mode_des`) | Mode | offered modes (D), then the Pure Combat variants the service lists | Elimination (mode 0); single tab: Elimination (bot play, mode 17) |
| Map (`drop_map`) | Map | maps of the mode | the mode's default map (`DefaultMapID`: Elimination → Oil Rig), or the previous map if the new mode also offers it |
| Goal (`drop_goal`) | Goal, or Difficulty for Fireteam | `GoalMin` to `GoalMax` in `GoalInc` steps, each labelled "N Kills", "N Rounds", "N Min." or "N Points" by mode; the default entry gets " (Default)" | `DefaultGoal` |
| Players (`Combo_Players`) | PLAYER(S) | `MinPlayers` to `MaxPlayers` in `PlayersInc` steps; team modes show "n/2 vs n/2", others "n Player(s)", bot play "1 vs n" | `RecommendMaxPlayers` (capped at 16, or the channel's limit); the previous choice is kept if it is valid and on the step grid of the new mode |
| 1st Arms (`drop_arms`) | 1st Arms | arms options 0–8 (and 10 in Go4CA Cup) | All Permitted; forced to All Permitted and disabled when the mode's `EnableWeaponUseSetting` is off |
| 2nd Arms (`drop_second_arms`) | 2nd Arms | "-" plus the options of the same type as 1st Arms, except the 1st Arms one | "-"; disabled unless 1st Arms is not All Permitted and the host owns the premium "super elite moderator" item, so always "-" offline |
| Random map (`Mutation_On` / `Mutation_Off` in the premium window) | – | on/off | off; premium only |

- Goal labels by mode: Kills for Elimination, One Man Army, Arms Race, Snowball Fight, bot Elimination; Rounds for Search & Destroy, Quarantine Regen, Elimination Pro, Last Man Standing, Bombing Run, Hired Guns, S&D for Go4CA Cup, VIP Escort, Hunted (and VIP Elimination by mode id, but see below); Points for Capture the Flag; Min. for Spy Hunt and Seize and Secure; "N Points" for any mode whose game type is VIP Elimination or Squad Match 2 (this test comes first, so VIP Elimination shows Points). Fireteam's goal list shows NORMAL / HARD / EXTREME for 0 / 1 / 2.
- **Ranges** offered (US room data; `game_modes.md` S2 has the same goal and player table): Elimination 30–140 step 10 (100), 2–16 step 2; Search & Destroy 3–9 step 2 (5), 2–16 step 2; Fireteam 0–2 (1), 8; One Man Army 15–45 step 15 (30), 2–16 step 1; Capture the Flag 3–7 step 2 (5), 4–16 step 2; Spy Hunt 8–16 step 4 (12), 4–16 step 1; Quarantine Regen 5–9 step 2 (7), 8–16 step 1; Elimination Pro 5–9 step 2 (7), 6–16 step 2; Last Man Standing 3–9 step 2 (5), 2–16 step 1; Bombing Run 3–9 step 2 (5), 6–16 step 2; Seize and Secure 8–16 step 4 (12), 6–16 step 2; Hired Guns 3–9 step 2 (7), 6–16 step 2 (on map 38 the goal is 15000–30000 step 5000, default 20000, by an override record); S&D for Go4CA Cup 5 (5), 8–10 step 2; Arms Race 15–45 step 15 (15), 2–16 step 1; VIP Escort 5–11 step 2 (7), 4–16 step 2; VIP Elimination 100–180 step 10 (140), 2–16 step 2; Hunted 3–9 step 2 (5), 3–16 step 1; Snowball Fight 30–90 step 20 (50), 2–16 step 2 (not offered, `EnablePlay` FALSE).
- **Single tab**: the mode list is the single-play modes (bot Elimination, single Fireteam). The password row, 1st and 2nd Arms rows are hidden; the second dropdown becomes **Difficulty** (NORMAL, HARD, EXTREME; default `DefaultDifficulty` 1 = HARD) and the arms row becomes the **Goal** for bot Elimination (20–50 step 10, default 40). Players for bot Elimination: 5–8 step 1, default 8, shown "1 vs 8". Single Fireteam uses the difficulty as its goal.
- **There is no time, team-balance or spectator option.** Round times come from the map's mission file (`game_modes.md` S2); teams are balanced by the lobby when players enter (T); spectating is only for game-master observer accounts. Friendly Fire, Join Mid Combat, Kill Cam, Abilities and Super Soldiers are set inside the room (O), starting at the mode's defaults.
- **OK** (`CreateRoom_ok`):
  1. The name must not be empty, must not consist only of spaces, and (unless it is still the offered default title) must pass the word filter; else "Please enter the room name." or "The room name is inappropriate or invalid. Please choose another." and the popup stays.
  2. With the password box unchecked the password is sent empty; on the single tab it is always empty.
  3. The request goes out once (further clicks are ignored until the reply), and the popup closes.
- **Lobby checks on create** (refusal → error box, the player stays in the room list): the mode must have rules (modes 20–25 have none and are refused); the map must be open for the mode (map 999 / random: the lobby picks a random open map; "no map": the mode's default map); requested max players must be ≤ the channel's room limit, within the mode's lobby range, and a multiple of 2 in two-team modes; the channel must have a free room id ("You cannot create any more rooms. Please use another channel."); Fireteam rooms are limited per channel ("This channel has reached capacity for Fireteam games…"). A goal outside the lobby's range is **silently replaced** by the mode's default; an invalid arms value is silently replaced by All Permitted; a name longer than 32 characters leaves the room unnamed.
- **On success** the creator enters as host, on Alpha, and the waiting room opens. The single tab creates no lobby room: the client builds the room locally (R, bot play).
- Keys: Esc closes the mode-description box; typing goes to the focused edit box (name or password; clicking a box focuses it).

### R. The waiting room

Layout [`GAMEROOM.XML`]: left window `gameroom` (room name bar with `button_change_roomName` "Change Room Name", password field and `button_change_pwd` "Change PW"; team header with `Btn_Alpha` "Alpha", `Btn_Bravo` "Bravo", arrows `Btn_Left` / `Btn_Right` and the mode name; the Alpha list `AlphaList` under header "Rank / Name", "CLAN", "STATUS" and the Bravo list `BravoList`, 8 rows each; `button_teamChange` "Team Change"; `button_game_ready` "READY [F5]" and `button_game_start` "START [F5]" in the same place; `button_lobby` "To Lobby"; `button_invite` "Invite"), and right window `game setting` "Game Setting" (mode, goal or difficulty, players, 1st Arms, 2nd Arms, Super Soldiers, the map dropdown with thumbnail, size, proper player count and level, and the option block, O).

- **Rows** (cloned from `UserInfoBar`): rank icon, name, clan emblem and name, nation flag, a host crown (`master`) on the host's row, voice icon, status text. In one-team modes (One Man Army, Last Man Standing, Arms Race, Spy Hunt, Quarantine, Fireteam, Hunted) the team buttons are hidden and the lists are filled in arrival order (Alpha up to 8, then Bravo).
- **On entering** the client asks for the room's settings and player list; every other member gets a player-entered notice with the newcomer's row, and the room list's user count updates for everyone in the lobby.
- **Who sees which button**: the host sees START; others see READY while the room is *waiting* and START (meaning "join the match") while it is *in game* or *closing*. READY's text turns red while the player is ready.
- **Keys**: F5 = START for the host, READY for others (only while the screen accepts input). Mouse: right-click a row opens a user menu (add friend, character info, entrust host, Elite Moderator kick); double-click a row shows character info.
- **Idle**: no mouse or key input for 60000 ms in this screen sends "extra state idle"; the next mouse move, click or key sends "extra state none".
- **Bot play room** (single tab): built by the client alone. The player is host on their chosen team, the bot roster is shown, no lobby messages are sent for settings or ready (except recording the single play), and START hosts a local server (S).

### O. Room options

Each option is a radio pair in the `game setting` window. Only the host can change an option (the controls are disabled for others), only while the room is *waiting*, and only if both the client data flag and the lobby's per-mode rule allow it. A change is a change-setting request to the lobby; the lobby re-checks and, on success, notifies the room (and the room list for fields it shows).

| Option (control pair) | Shown as | Default after create or mode change | Client enables it when | Lobby refuses it in | In-match effect |
|---|---|---|---|---|---|
| `Join_On` / `Join_Off` | Join Mid Combat | On | `EnableJoinCombatSetting` | (none) | J |
| `TeamChange_On` / `TeamChange_Off` | Swap Teams | display only: On in VIP Escort, else Off | never (always disabled) | – | none from the room (VIP Escort swaps by its rules, `game_modes.md` M13) |
| `FriendlyFire_On` / `FriendlyFire_Off` | Friendly Fire | Off | `EnableTeamKillSetting` | Fireteam, One Man Army, Quarantine Regen, Last Man Standing, Arms Race, VIP Escort, S&D for Go4CA Cup | `game_modes.md` S8 |
| `KillCam_On` / `KillCam_Off` | Kill Cam | Off; Hunted On | `EnableViewEnemyCameraSetting` | Fireteam, Quarantine Regen, S&D for Go4CA Cup, VIP Escort | enemy view in the death camera |
| `Perk_On` / `Perk_Off` | Abilities | On; Snowball Fight, S&D for Go4CA Cup, Hunted Off | `EnablePerkSkillSetting` (absent = on) | Snowball Fight, S&D for Go4CA Cup, Hunted | abilities (perks) active; with it on, READY and START first check the player's equipped abilities for the mode |
| `Mutation_On` / `Mutation_Off` (in `Mode`) | Super Soldiers | per mode, from the lobby's configuration (Open questions) | `EnableMutationSetting` and the channel is a "recreation" channel | per lobby configuration | mutation boxes (`game_modes.md` S12) |
| `drop_arms` | 1st Arms | All Permitted; S&D for Go4CA Cup: Only Go4CA Cup Item | `EnableWeaponUseSetting` | Fireteam, Spy Hunt, Snowball Fight, Quarantine Regen, S&D for Go4CA Cup | E (restriction) |
| `drop_second_arms` | 2nd Arms | "-" | premium only (C) | as 1st Arms | E |

- `EnableTeamKillSetting` is TRUE in the US data for Elimination, Search & Destroy, Capture the Flag, Spy Hunt, Snowball Fight, Elimination Pro, Bombing Run, Seize and Secure, Hired Guns and VIP Elimination; `EnableViewEnemyCameraSetting` for all playable modes except Fireteam, Quarantine Regen, S&D for Go4CA Cup and VIP Escort; `EnableWeaponUseSetting` for Elimination, Search & Destroy, One Man Army, Capture the Flag, Elimination Pro, Last Man Standing, Bombing Run, Seize and Secure, Hired Guns, VIP Escort and VIP Elimination; `EnableMutationSetting` for Elimination, One Man Army and VIP Elimination (and room-data-only modes).
- Pure Combat rooms: Abilities Off and Kill Cam Off, both locked; Friendly Fire locked (On in the BR and RU regions, Off elsewhere); Join Mid Combat On and changeable.
- **The other room settings**, same host-only and *waiting*-only rule:

| Setting | Control | Lobby check | Side effects |
|---|---|---|---|
| Mode | `drop_mode` | not the current mode; the mode has rules and is not single-play; the current map is open for it, else its default map is; the mode's default max players (16; One Man Army, Last Man Standing, Arms Race 8; Fireteam 8; Go4CA Cup 10; raised to the current player count, rounded up to even in two-team modes) fits; Fireteam per-channel room limit | map kept or reset to default; goal, Join Mid Combat, Friendly Fire, Kill Cam, Abilities, Super Soldiers and arms reset to the mode's defaults; max players reset to the default above; **every player un-readied**; in one-team modes Bravo players are moved to Alpha in join order until Alpha has 8; a chat line "The game has been changed to <mode> mode." |
| Map | `drop_map` | different map, open for the mode; while random map is on only "new random map" is accepted | goal reset to the mode's default for that map; ready states kept |
| Goal | `drop_goal` | within the lobby's range for the mode and map | ready states kept |
| Players | `drop_players` | ≤ channel limit; within the mode's lobby range; multiple of 2 in two-team modes; the bigger team (two-team) or the total (one-team) still fits | ready states kept |
| 1st / 2nd Arms | `drop_arms`, `drop_second_arms` | a real change; both values < 11 | **every player un-readied**; a chat line "Arms Setting changed to : <name>" |
| Room name | `button_change_roomName` | ≤ 32 characters; client applies the create-name rules first | – |
| Password | `button_change_pwd` | ≤ 8 characters; empty makes the room public | lock icon in the room list follows |
| Host | user menu, entrust host | target is another player in this room, not an observer | target un-readied if ready |

- Before sending a mode change the client refuses it itself when the mode's max players is below the current player count: "You have too many players to change the Mode to <mode> (Max: <n> players)".
- A change-setting request carries the mode the client believes the room has; the lobby refuses it if that differs (stale request). A field number outside the known list makes the lobby disconnect the sender.

### T. Teams

- **Entering a two-team room**: the lobby puts the player on the smaller team; on equal sizes, Alpha. **One-team modes**: Alpha while Alpha has fewer than 8, else Bravo.
- **Changing team** (`Btn_Alpha`, `Btn_Bravo`, the arrows, or `button_teamChange` which toggles): refused by the client in one-team modes (button disabled) and, while the room is *in game*, unless the player's current team without them has at least as many players as the other team ("Unable to switch teams due to team imbalance."). The lobby refuses when the player is not *waiting* (ready players cannot switch), the mode does not allow team change (one-team modes), or the target team already has max / 2 players. A refused switch shows a message box whose text is a single space (Quirks).
- Pure Combat rooms keep a per-team queue of players asking to switch: a request to a full team enters the queue; when a player of the target team asks the opposite way, the two are swapped. Asking again leaves the queue.
- The lobby has no automatic rebalancing at match start (`game_modes.md` S1). Teams in the match are exactly the room teams of the players who were sent in.

### S. Ready, start and countdown

- **READY** (non-host, room *waiting*): toggles. The client first checks the loadout against the room's arms (E), then (with Abilities on) the equipped abilities; if a check fails a message offers "Go to Shop". Then it sends "ready on" or "ready off". The lobby accepts "ready on" only from a *waiting* non-host (a host's ready is refused). The new state is broadcast; the room list does not show it.
- **START** (host, room *waiting*), client checks first:

| Modes | Client requires (ready players; the host counts as ready on their own team) |
|---|---|
| One Man Army, Last Man Standing, Arms Race | total ≥ 2 |
| Hunted | total ≥ 3 |
| Spy Hunt, Fireteam, Quarantine Regen | total ≥ 4 |
| every other mode | each team ≥ `MinPlayers` / 2 (integer) |

  then the arms and abilities checks for the host's own loadout. Failing the count shows "There aren't enough players to start the game."
- **Lobby start rule** (the host's team counts the host plus its ready players; "other" is the other team's ready players):

| Mode | Lobby requires | Effective with the client rule (US data) |
|---|---|---|
| Elimination, Snowball Fight, VIP Elimination | other ≥ 1 | each team ≥ 1 |
| Search & Destroy | host team ≥ 3 and other ≥ 3 (BR and EU: 1 and 1) | each team ≥ 3 |
| Fireteam, Spy Hunt, Quarantine Regen | total ≥ 4 (KR Fireteam: 2) | total ≥ 4 |
| One Man Army, Last Man Standing, Arms Race | total ≥ 2 | total ≥ 2 |
| Hunted | total ≥ 3 | total ≥ 3 |
| Capture the Flag | host team ≥ 2 and other ≥ 2 (BR: 1 and 1) | each team ≥ 2 |
| Elimination Pro | other ≥ 1 | each team ≥ 3 |
| Bombing Run, Hired Guns | 3 and 3 | each team ≥ 3 |
| Seize and Secure, VIP Escort | 2 and 2 | each team ≥ 3 / ≥ 2 |
| S&D for Go4CA Cup | 1 and 1, plus every player on the cup's allowed list | each team ≥ 4 |

  A lobby service configured for self-play skips the rule. Our implementation applies both rules (the effective column).
- **Start sequence**:
  1. Host's start request accepted: the host becomes ready, the room becomes *starting*, every ready player becomes *starting*, the start counter starts at 0, and the lobby asks the launcher for a game server with the room's settings (mode, map, goal, max players, options; `game_modes.md` S2). The room and the lobby's room list get the new state; the room gets the new player states. Players who were not ready stay *waiting* in the room.
  2. On each lobby tick while the counter is below 5, every room member gets a countdown notice carrying the counter; the client prints "Game starting in... N seconds." (N = 5 − counter; "second" for 1) as a red system line in the room chat and plays the countdown sound. *Starting* players' buttons are disabled.
  3. When the counter has reached 5 and the game server has reported ready, the room becomes *in game*, every *starting* player becomes *in game*, and each is sent a "go" notice with the game server's address. The client connects, switches to the loading screen and the match.
  4. Then the counter is incremented (so the countdown notices carry 0, 1, 2, 3, 4 and launch can happen on the sixth tick at the earliest).
  5. If the game server is not ready by count 15, the lobby tells the launcher to stop it; at count 17 the room returns to *waiting* and every player who was not *waiting* is set *waiting*.
  6. Client side, a *starting* player who has had no "go" 15000 ms after the last countdown notice gets "Failed to join the battle. Connection timed out.", is re-enabled, and sends return-to-room with a timeout reason. This timer is not cleared when the room returns to *waiting* at step 5, so the message still appears (Quirks); the lobby ignores the late return-to-room because the player is already *waiting*.
- **Bot play START**: the client checks nothing about other players, sends a single-play start (mode, map, goal, bot count, difficulty, optional reward ticket, team) to the service, and on the reply hosts a local server itself with max players = bot count, the mode, map, goal, Friendly Fire, Join Mid Combat, Super Soldiers, both arms and Abilities, then loads the map. Offline: skip the service round trip. Bot behaviour is in `bots.md`.

### J. Joining a match in progress

- **From the room list**: a room that is *starting* or *in game* can be entered only with Join Mid Combat on; a *closing* room never. Online the room list sends nothing more; the player lands in the waiting room as *waiting*.
- **From the waiting room**: a waiting player presses START (shown for non-hosts while the room is *in game* or *closing*), or READY while it is *starting*. The lobby accepts if the room is not *closing*, the mode allows joining in game and Join Mid Combat is on. Then:
  - room *starting*: the player becomes *starting* and goes in with the others at step 3 above;
  - room *in game*: the player becomes *in game* and the game server is told to expect them with their loadout; the player gets a "go" notice and connects.
  - Refused: "Please wait... the game is currently in the final round or is just ending."
- What happens to the joiner in the match (spawn at once or held until the next round) is in `game_modes.md` S1.

### K. Host, kicking, leaving, chat, invite

- **Leaving the room** ("To Lobby" or the Lobby navigation button): leave-room request; the player returns to the room list. The lobby removes them from their team, cancels any team-switch queue entry, and:
  - if they were the last player, destroys the room (room-destroyed notice to the lobby; observers inside are pushed out);
  - else, if they were the host, picks a new host: the first remaining player in join order whose extra state is *none*; if none, the first who is *outfitting*; if none, the first player (who must be *idle*). A ready new host is un-readied. The room gets a host-changed notice. The new host also inherits the host-bound premium settings, which are cleared when they lack them.
- The host leaving does not stop a countdown or a match in progress; the new host simply gets the host controls. A client that was counting down prints "Cancel..." and re-enables its buttons when the host changes (Quirks).
- **No idle-host takeover**: there is no rule that hands the host role to someone else while the host stays (the warning texts exist but are never shown in this build).
- **Kicking**: a host can kick only while they own an **Elite Moderator** item (premium), the room shows its moderator level, they have kicks left for the day, the target is not a game master, and (if the host is not in a match) the target is not in a match. The client also refuses targets that are *starting* or *in game* and names beginning with "GM". The kicked player is removed with a "kicked" exit, returns to the room list with a kick notice, and is blocked from re-entering any room this host hosts with moderator rights. Without the item there is no kick in normal rooms; in-match vote kick is separate (`ui_system.md` H). Kick reasons offered: team killing, bug abuse, abuse, hacking, other.
- **Chat**: the chat window (CHATWINDOW layouts) sends lines of at most 120 characters to the lobby; in a room they go to the room's players, in the room list to the channel's players who are not in a room. System lines (countdown, mode and arms changes) are added locally.
- **Invite** (host only): the lobby sends an invite (room id and password) to up to 5 players waiting in the channel's room list.

### E. Loadout

- **Where**: the inventory screen ("My Character"), reachable from the room list and the waiting room. Two tabs pick the set being edited: `tab_Normal` (normal set) and `tab_Official` (Pure Combat set). A Pure Combat room sends the Pure Combat set into the match; every other room the normal set.
- **Weapon slots** (`INVENTORY_CHA_WPN.XML`, list `EquipWeaponList`): `equip_primary` (Primary), `equip_secondary` (Secondary), `equip_Melee` (Melee), `equip_Support` (throw), and `equip_backpackA` … `equip_backpackD` (four backpack slots). Primary, secondary and backpack rows have five attachment buttons `btn_muzzle`, `btn_scope`, `btn_magazine`, `btn_bullet`, `btn_aim` (mechanical sight), one per attachment slot of `attachments.md`; attachments are chosen per weapon and per set.
- **Gear slots**: character (body), head, face (goggles), uniform, vest, backpack, plus five cosmetic slots (character, head, face, vest, backpack) and three ability slots (four with a slot-extender item).
- **What may go where** (lobby rule; the client enforces the same and explains refusals):

| Slot | Item types accepted |
|---|---|
| Primary | assault rifles, SMGs, sniper rifles, machine guns, rocket launchers, flame throwers, primary shotguns |
| Secondary | pistols, and the item type listed as shotgun (type 5) |
| Melee | knives |
| Support | throwables (grenades, mines and the like) |
| Backpack A–D | any weapon except knives; "special" items and assassin items go only here; a weapon marked `EquipOnlyBackpack` only here |
| Gear and cosmetic slots | items of that slot's type |

  - A backpack slot is usable only up to the number of slots given by the equipped backpack's `Slot` plus the character's own slots; with no backpack and no character slots, none. Specialist and assassin characters use their own special slot rules and cannot take gear.
  - The same item cannot be equipped twice in one set (unless its type allows duplicates): "You cannot equip two of the same item at the same time." Items listed in another equipped item's `CantWithEquip` list are refused: "There is an item already equipped that can't be equipped together. Cannot equip this item."
  - The Pure Combat set accepts only items on the service's Pure Combat list.
  - **Weapon slots can never be empty**: releasing a primary, secondary, melee or support weapon is refused, and releasing a base item is refused. Gear can be released. Releasing the backpack also releases the backpack-slot weapons; releasing the character releases what depends on it.
- **Equip gestures**: double-click an owned item, or select it and press the equip button; right-click opens a menu (equip, discard, extend). An "auto equip" button picks the best owned items (below).
- **Defaults for a new player** (base items, owned by everyone and never expiring): Primary M16A3 (item 20), Secondary Beretta M92FS (24), Melee M9 (0), Support M67 (7), head Black Cap (4007), backpack Standard Backpack (7001, one backpack slot) in the normal set. In the Pure Combat set the backpack's fallback is "none" (P); what a brand-new Pure Combat set holds was not traced (Open questions). The character body is the one chosen at character creation. These match `inventory.md`'s stock loadout.
- **Item numbers**: weapons use their weapon index; uniforms 2000 + id, vests 3000 + id, heads 4000 + id, face attachments 5000 + id, face models 6000 + id, backpacks 7000 + id, attachments 1000 + id, function items 9000 + id, characters 10000 + id (id = the record's `ItemID` in the item file). A rental copy is 100000000 + the item number.
- **The 1st Arms check at READY / START** (client): the player may ready when at least one of their equipped weapons passes the option: Pistols Only: a pistol; Melee Only: a knife (always true); No Sniper Rifles: a primary-class weapon that is not a sniper rifle; No Explosives: a primary-class weapon that is neither a rocket launcher nor a listed explosive; No Backpack: a weapon outside the backpack (always true); Sniper Only: a sniper rifle; Shotguns Only: a shotgun of either type; No Shotguns: a primary-class weapon that is not a primary shotgun. With a 2nd Arms set, "only" options pass if either option passes and "except" options need both. Go4CA Cup: every equipped item and attachment must be on the cup's list and the body must not be female. Fireteam additionally requires a weapon allowed in co-op. Failure shows the option's message ("You do not have a pistol equipped for 'Pistols Only' mode." …) with "Go to Shop".
- **What the game server keeps** (when it builds the in-match inventory, `inventory.md` section 1): each equipped weapon is dropped if the arms option forbids it: Pistols Only keeps pistols and knives; Melee Only keeps knives; Sniper Only keeps sniper rifles and knives; Shotguns Only keeps both shotgun types and knives; No Sniper Rifles drops sniper rifles; No Explosives drops throwables, rocket launchers and listed explosive weapons; No Shotguns drops both shotgun types; No Backpack drops everything in backpack slots. With a 2nd Arms: "only" options keep what either allows, "except" options drop what either forbids. A missing or invalid attachment is replaced by the weapon's default attachment (`attachments.md`). Co-op modes also drop weapons not allowed in co-op.
- **When the loadout is read**: the lobby sends the player's set to the game server when the player is sent into a match (start or join). Respawns use that copy (`inventory.md` section 13); changes made later take effect at the next match.
- Offline / LAN: the developer LAN path keeps the loadout in the local player profile (primary, secondary, melee, throw, four backpack items, character, head, face, uniform, vest, backpack, the five attachments of the primary and of the secondary, three abilities). Our implementation can do the same.

### P. Shop and item lifetime

Only what decides a player's equipment is specified; payment and prices are left out.

- **Shop data**: the shop list files (`SHOP_ITEM_LIST.TXT`, `SHOP_ITEM_LIST_RENTAL.TXT`): products `[Product#]` with `ItemNo0` (the item granted), `Period0`, `Consumetype0`, `Itemnum`, `Shopview` (listed or not) and sale type. The item file (`ITEMS.TXT`) gives each item's type and equip rules.
- **Duration** of a bought item, from the product:

| `Consumetype0` | Meaning of `Period0` |
|---|---|
| 0 (normal) | days; 999 = permanent |
| 3 (count) | number of uses |
| 5 (hours) | hours |
| 7 (minutes) | minutes |
| other values | boxes, GP, coupons and the like; not equipment durations |

  Typical periods in the US list: 1, 3, 7, 10, 15, 30, 90 days and 999. Rental products grant the rental copy (item number + 100000000) for 1, 7, 30 or 90 days; rentals show their own class icon and are listed apart in the inventory.
- **Expiry** is decided by the service's clock: checked at login, after each match result for each player, and after single play. A match never loses an item halfway: the in-match inventory is the copy sent at entry.
- **What an expired item does**: it leaves the inventory and every set. If it was equipped:
  - a weapon slot falls back to the player's chosen default weapon for that slot, else to the base item (E); a backpack-slot weapon is simply removed;
  - the backpack falls back to the Standard Backpack (normal set) or none (Pure Combat set), and backpack-slot weapons that no longer fit are released;
  - its attachments are released from the weapon; abilities bound to it are released.
- The client shows an expired-items notice [`POPUP_NOTICE_EXPIREDITEM.XML`] on the next visit to the waiting room. On the next visit to the inventory it re-equips the **normal** set's expired slots automatically with the best owned, unequipped item: first an item of the same weapon type as the expired one, then the higher grade, then by type priority (assault rifle, SMG, sniper rifle, primary shotgun, machine gun, rocket launcher, crossbow, special, assassin weapon, throwable, pistol), then at random.

### G. Result screen and return to the room

- Shown when the client leaves a finished match (`ui_system.md` H). It shows the match result per the mode's result layout: team result for team modes; no-team result for Spy Hunt, Quarantine Regen, One Man Army, Arms Race, Last Man Standing and Hunted; Fireteam result for co-op. Rewards, MVP and charts are online-only.
- **Controls**: `Exit_TopBtn` and `Exit_BottomBtn` "Exit" return to the room; tabs switch between the player list and the player's chart. Enter returns only while the abuse notice is shown. After 60000 ms the screen returns on its own.
- **Lobby side**: when the game server reports the match over, the room becomes *waiting* and every player who was *starting* or *in game* is set *waiting* (ready states are lost). A player who leaves the match early sends return-to-room and is set *waiting* at once.
- Returning lands in the **same waiting room**, not the room list. The host is whoever is host now.

### X. In-game menu and leaving mid-match

- Esc in a match opens the in-game menu [`POPUP_EXIT_INGAME.XML`], title "MENU", 220 x 145, centred and unscaled (`ui_system.md` S1): `button_lobby` "EXIT", `button_userreport` "Player & Bug Report Form", `button_record` (start/stop recording, with the bound key), `button_cancel` "CANCEL", and a close button `button_exit_title`. Esc or CANCEL closes it.
- While it (or the report box, a message box or the user tooltip) is open the match keeps running, but the player's controls are cut: newly pressed commands are ignored, mouse look is frozen, the movement flags are cleared every update except crouch, and an active sprint ends. The cursor is shown.
- **EXIT** asks for confirmation: "If you exit the game now, your GP, EXP, and job progress will be lost. Are you sure you want to exit?" (the Pure Combat text is the same). On Yes the client:
  1. tells the game server it is leaving,
  2. disconnects,
  3. sends return-to-room (reason normal) to the lobby,
  4. shows the waiting room.
  The game server applies its leaving-player rules (`game_modes.md` S10). The lobby sets the player *waiting*; they can rejoin the same match with START if the room still allows it (J).
- In tutorial modes EXIT does nothing after the confirmation. In bot play it ends the local game and returns to the result screen.
- A dropped connection (not via EXIT) also brings the client back to the waiting room with a disconnect message; the client then sends return-to-room itself.

### Q. Developer LAN screens

Non-release builds contain an older, text-menu path that works without the lobby service. It is described because it is the reference's only true LAN flow:

- **Multi menu**: entries Play (to Weapon Setting), and debug shortcuts to the other screens.
- **Weapon Setting** [`LANTEST_WEAPONSETTING.XML`]: tabs Weapon, Gear, Cosmetic, Function, Ability; slot buttons for primary, secondary, melee, throw, backpack 1–4, character, head, face, uniform, vest, backpack, the five attachment buttons and three abilities; every item in the item data can be picked (no ownership); a search box; buttons Join and Host. The choice is saved to the local profile.
- **Host**: host name, password toggle and password, port; the map list comes from the server configuration file's map cycle; picking a map launches a server on the chosen port with that map.
- **Join LAN**: Find Servers (LAN search on the chosen port), a server list with columns lock, name and address; a team selector that cycles Alpha, Bravo, game-master observer, referee observer; joining a locked server asks for the password.
- There are no rooms, ready or countdown in this path.

### M. Events

Events between client (C), lobby (L, the service or our stand-in) and game server (G). Names are ours; contents are listed, not bytes. "Notice" events are pushed to everyone concerned.

| Event | From → to | Contents | When / effect |
|---|---|---|---|
| channel enter / leave | C → L | channel | V |
| player-in-lobby / left-lobby notice | L → C (channel) | player row | someone enters or leaves the room list |
| room list request / reply | C → L / L → C | – / every room's id, name, settings, user count, state | L on focus |
| room-created notice | L → C (channel) | room summary | L |
| room-destroyed notice | L → C (channel) | room id | last player left |
| room-changed notice | L → C (room, channel) | room id, field, value or text | any setting, user count, state, host change |
| create-room request / reply | C ↔ L | name, password, mode, Pure Combat flag, map or random, goal, max players, 1st and 2nd arms, single-play flag / ok + room id + seat key, or error | C |
| enter-room request / reply | C ↔ L | room id, password / ok + seat key, or error | L |
| player-entered / player-left notice | L → C (room) | player row / player, exit kind (normal, moderator kick, GM kick) | R, K |
| leave-room request / reply | C ↔ L | – | K |
| room settings / player list request and reply | C ↔ L | – / all fields, all rows with team and states | on entering a room |
| change-setting request / reply | C ↔ L | field, value or text, mode the client believes | O |
| team request / reply / team notice | C ↔ L, L → room | wanted team / result / player, team | T |
| state request / reply / state notice | C ↔ L, L → room | ready on, ready off, start, or extra state none / outfitting / idle | S, J |
| countdown notice | L → C (room) | counter 0–4 | S |
| launch request / game-ready report | L → G / G → L | room settings / – | S |
| expect-player | L → G | player, team, loadout set | S step 3, J |
| go notice | L → C | game server address | S step 3, J |
| leaving | C → G | – | X |
| return-to-room request / reply | C ↔ L | reason (normal, timeout, failure) | G, X, S step 6 |
| closing report / match-over report | G → L | – / results | room *closing* / *waiting* |
| kick request / reply | C ↔ L | target, reason / kicks left | K |
| invite request / invite notice | C → L / L → C | – / host name, room id, password | K |
| chat | C → L → C | text | K |
| equip / release request and reply | C ↔ L | item, slot, set / ok or error | E |
| items-expired notice | L → C | item list | P |
| single-play start / end | C ↔ L | mode, map, goal, bots, difficulty, ticket, team / result | S |

## Order of events

Lobby tick (once per second), for each room with a running counter, in this order:

1. If the counter is below 5: send the countdown notice with the counter.
2. If the counter is 5 or more and the game server is ready: room *in game*, *starting* players *in game*, each gets the go notice and the game server is told to expect them; the counter stops (this happens at most once).
3. If the counter equals 15: tell the launcher to stop the game server.
4. If the counter is 17 or more: room back to *waiting*, players back to *waiting*.
5. Increment the counter (if still running).

Requests are handled as they arrive, between ticks. Within one request the lobby validates, changes the room, then sends the reply to the requester and the notices to the room and channel.

## Edge cases

- **Host leaves during a countdown or a match**: a new host is chosen (K); the countdown or match goes on. The new host cannot change settings until the room is *waiting* again.
- **Host leaves while everyone else is outfitting or idle**: the first *outfitting* player becomes host; if all are idle, the first in join order.
- **Player joins while the match is starting**: allowed only with Join Mid Combat on; a READY from them during *starting* puts them in with the others (J). With Join Mid Combat off, the room cannot even be entered while *starting*.
- **Ready player opens the inventory**: they are un-readied first, then shown as "Outfitting". The host opening the inventory just becomes *outfitting* (relevant only if they then leave).
- **Mode change with too many players**: refused by the client before sending; the lobby refuses it too if the player count does not fit the new mode's default size.
- **Map change**: ready players stay ready, even though the goal resets to the map's default.
- **Invalid loadout at READY**: READY is not sent; a message with "Go to Shop" appears. An invalid loadout that slips through (the client check is weaker than the server filter) loses the forbidden weapons in the match; with no weapon left but the knife the player fights with the knife.
- **Room full between list and join**: the client's test passed on stale data; the lobby refuses with the generic message.
- **Wrong password**: the box closes, the message appears, the player stays in the list.
- **Counter reaches 17 without a game server**: everyone is *waiting* again; the *starting* clients have already timed out on their own after 15000 ms without a go notice.
- **Room-data-only modes** (20–25): listed by an offline client that trusts `EnablePlay`, but the lobby has no rules for them; creation and mode changes to them are refused. Offline we should not list them.
- **Single play while in a lobby room**: refused by the service; a player in a bot-play room cannot enter a lobby room.
- **Expired equipped weapon**: the slot shows the default weapon at once; the client may replace it with a better owned item on the next inventory visit.
- **Item expires while in a match**: no effect until the match is over.

## Quirks

Keep unless a reviewer decides otherwise.

1. **The countdown says 5 but the minimum wait is five full ticks plus the time to the first tick**: the first countdown notice arrives on the first lobby tick after START (0–1 s), and launch can happen on the sixth tick. So START to "go" is between 5 and 6 s when the game server is quick.
2. **The room list's sort direction is shared by all columns.** Clicking TITLE (ascending) then Map gives Map descending.
3. **Elite Moderator rooms always float to the top of the room list**, whatever the sort.
4. **Map, goal, player-count and on/off option changes keep ready states; mode and arms changes clear them.**
5. **A refused team switch shows a message box whose text is a single space.**
6. **VIP Elimination's goal is labelled "Points"**, because the game-type test runs before the per-mode labels.
7. **"Swap Teams" is a dead control**: always disabled, only reflects whether the mode is VIP Escort.
8. **"Duty Roster" exists in the arms data but is never offered.**
9. **The host reminder ("More than half of the players in the room are ready to start the game!") and the idle-host warnings are never shown**; the code that would show them is not called.
10. **The client prints "Cancel..." when the host changes during a countdown** although the lobby does not cancel the countdown.
11. **"No Shotguns" readiness ignores secondary-slot shotguns** while the game server removes them; **"Pistols Only", "Sniper Only" and "Shotguns Only" keep the knife** in the match.
12. **A goal outside the range on room creation is silently replaced by the default**, while the same value in a later change is refused.
13. **A new host inherits nothing visible about readiness**: if they were ready they are quietly un-readied.
14. **The client's start timeout survives a failed launch**: after the room has gone back to *waiting*, formerly *starting* players still get the "Connection timed out" box 15000 ms after the last countdown notice.

## Answers to other specs' open questions

These are settled here; the other specs are not changed by this one.

- **`ui_system.md` question 6 (input while the Esc menu is open)**: movement and mouse look are **not** active. While the in-game menu, report box, message box or user tooltip is open, new command presses are ignored, mouse look is zeroed, the movement control flags are cleared each update except crouch, and sprint ends. The match itself does not pause (X).
- **`game_modes.md` question 11 (Join in progress off)**: with Join Mid Combat off the lobby refuses both entering a *starting* or *in game* room and a mid-match join from inside the room (J).
- **`game_modes.md` question 6 (free-for-all team ids)**, in part: the lobby itself gives one-team modes only Alpha (first 8) and Bravo (the rest); per-player team ids in the match are not the lobby's room team and must come from the game server's own numbering (the room seat key is a candidate). Not settled.
- **`bots.md` question 13 (bot room default player count)**: 8 bots, shown "1 vs 8" (`RecommendMaxPlayers` 8; the multiplayer default 16 does not fit the 5–8 range, so it is not kept).

## Test cases

Common setup unless stated: US room data and string table; one offline channel with a room limit of 16; lobby rules as in this spec (US branch); a fresh profile with the base loadout; times in ms from the action.

**Room list (L)**

1. Room list with rooms A (id 0, "Alpha", waiting, 4/16, no password), B (id 1, "bravo", in game, 16/16), C (id 2, "Charlie", in game, 3/16, Join Mid Combat off), D (id 3, "delta", waiting, 2/8, password). → Rows show NO. 1, 2, 3, 4; B and C greyed with status "In game", A and D "Waiting"; D has the lock icon; Players column "4/16", "16/16", "3/16", "2/8".
2. Same list, check "Joinable". → Only A and D remain. Also check "No Password". → Only A.
3. Same list, click TITLE once. → Order A, B, C, D (case-insensitive: "Alpha" < "bravo" < "Charlie" < "delta"). Click TITLE again. → D, C, B, A. Click Map next. → sorted by map, **descending** (shared direction).
4. Click Player(S) once with A 4/16, B 16/16, C 3/16, D 2/8. → D (max 8), C (16, 3), A (16, 4), B (16, 16).
5. Room E is created while the list is sorted by TITLE ascending, name "Bob". → E appears between "Alpha" and "bravo"; the scroll position is unchanged.
6. Double-click C. → "You cannot enter the room …" message; no request sent.
7. Double-click D, type "ab", press Enter. Lobby password is "ab". → enter-room request with "ab"; reply ok; waiting room opens. With password "AB" typed. → "You enter the wrong password,please retry."; still in the list.
8. Mode filter "Search & Destroy" with an S&D room and a Pure Combat S&D room present. → Only the non-Pure-Combat S&D room shows.

**Room creation (C)**

9. Open Create Room. → Mode "Elimination", Map "OIL RIG", Goal "100 Kills (Default)", Players "8 vs 8", 1st Arms "All Permitted", 2nd Arms "-" (disabled), password unchecked; the name is one of the five default titles.
10. Change Mode to One Man Army (Players was "8 vs 8", i.e. 16). → Map "JUNK FLEA", Goal list 15/30/45 Kills with "30 Kills (Default)", Players list 2 … 16 step 1 shown "N Player(s)"; the selection stays "16 Player(s)" because 16 is valid for the new mode. Opening the popup fresh with One Man Army picked first → "8 Player(s)".
11. Change Mode to Fireteam. → Goal row titled "Difficulty"; values NORMAL, HARD, EXTREME; default HARD; Players "8 Player(s)" only; 1st Arms disabled at All Permitted.
12. Clear the name to "   " and press OK. → "Please enter the room name."; popup stays open; nothing sent.
13. OK with mode Capture the Flag, Players 4. → Room created; creator is host on Alpha; Goal 5 Points; options Join Mid Combat On, Friendly Fire Off, Kill Cam Off, Abilities On.
14. Lobby receives a create with Elimination, goal 25. → Room created with goal 100.
15. Lobby receives a create with Elimination, max players 7. → Refused (odd in a two-team mode).
16. Lobby receives a create with mode 20 (Big Head). → Refused.
17. Single tab, mode Elimination (bot play). → Players "1 vs 5" … "1 vs 8", default "1 vs 8"; Difficulty default HARD; Goal 20–50 step 10 default "40 Kills"; password and arms rows hidden.

**Waiting room, teams and options (R, T, O)**

18. Room Elimination 16; host H on Alpha. P1 enters, then P2, then P3. → P1 Bravo, P2 Alpha, P3 Bravo (smaller team, ties Alpha).
19. One Man Army room, 10 players enter. → First 8 on Alpha, players 9 and 10 on Bravo; team buttons hidden; Team Change disabled.
20. Elimination room max 4 with Alpha 2 and Bravo 2; an Alpha player asks for Bravo. → Refused (Bravo has max/2 = 2).
21. P1 is ready and asks to change team. → Refused by the lobby.
22. Room in game: Alpha 3 in the room (P1 waiting among them), Bravo 4. P1 asks for Bravo. → Client refuses ("Unable to switch teams due to team imbalance.") because 3 − 1 = 2 < 4. With Alpha 5 and Bravo 4 → request sent.
23. Host changes Friendly Fire to On in Elimination. → Room-changed notice; all clients show On; ready players stay ready.
24. Host tries Friendly Fire in One Man Army. → Controls disabled; a forged request is refused by the lobby.
25. Host changes the map with P1 ready. → Map changes, goal reset to the mode default, P1 still ready.
26. Host changes 1st Arms to Pistols Only with P1 ready. → P1 un-readied; chat line "Arms Setting changed to : Pistols Only".
27. Host changes Mode from Elimination (12 players, 6 Alpha and 6 Bravo) to One Man Army. → Allowed (16 ≥ 12); everyone un-readied; Bravo players are moved to Alpha in join order until Alpha has 8, so 8 Alpha and 4 Bravo; max players becomes 12 (default 8 raised to the current count); chat line "The game has been changed to One Man Army mode."
28. Room with 10 players, host picks Fireteam. → Client refuses: "You have too many players to change the Mode to Fireteam (Max: 8 players)".
29. Non-host sends a change-setting request. → Refused.
30. Change-setting request carrying mode 1 while the room is mode 0. → Refused.

**Ready, start and countdown (S)**

31. Elimination, host H (Alpha) alone with P1 (Bravo, not ready). H presses START. → "There aren't enough players to start the game." P1 readies; H presses START. → Accepted.
32. Search & Destroy, Alpha H + 2 ready, Bravo 2 ready. → Client accepts (each ≥ 1), lobby refuses (needs 3 and 3). With 3 ready on Bravo → accepted.
33. Elimination Pro, Alpha H + 1 ready, Bravo 2 ready. → Refused by the client (each team needs 3).
34. Hunted with H + 1 ready. → Refused; with H + 2 ready → accepted.
35. START accepted at t = 0, lobby ticks at t = 400, 1400, … ms; game server ready at t = 2000. → Countdown notices at 400 (count 0, "Game starting in... 5 seconds."), 1400 (4), 2400 (3), 3400 (2), 4400 (count 4, "Game starting in... 1 second."); go notices at 5400; room *in game*.
36. Same, game server never ready. → Launcher told to stop at the tick with count 15 (t = 15400); room and players *waiting* at count 17 (t = 17400). Each formerly *starting* client still shows "Failed to join the battle. Connection timed out." at t = 19400 (15000 ms after its last countdown notice) and sends a return-to-room that the lobby refuses.
37. At START, P1 ready and P2 not ready. → P1 *starting* then *in game*; P2 stays *waiting* and sees START.
38. F5 pressed by the host → acts as START; by a non-host → toggles READY.

**Join in progress (J)**

39. Room in game, Join Mid Combat On, P2 waiting presses START. → P2 *in game*; game server told to expect P2 with their normal set; go notice to P2.
40. Same with Join Mid Combat Off. → Refused: "Please wait... the game is currently in the final round or is just ending."
41. Room *closing*, player in the room list joins. → Refused (room greyed).
42. Room *starting*, P3 enters (Join Mid Combat On) and presses READY. → P3 *starting*; goes in at the launch tick with the others.

**Host, leaving, chat (K)**

43. Room H (host), P1 (outfitting), P2 (none), P3 (idle) in join order H, P1, P2, P3. H leaves. → P2 becomes host.
44. Same with P2 outfitting. → P1 becomes host (first outfitting).
45. Only P3 (idle) remains after H leaves. → P3 becomes host. P3 leaves. → Room destroyed; room-destroyed notice to the lobby.
46. New host P2 was ready. → P2 un-readied; host crown moves to P2.
47. Host without an Elite Moderator item sends a kick request for P1. → Refused by the lobby; P1 stays.

**Loadout and items (E, P)**

48. New profile, normal set. → Primary M16A3, Secondary Beretta M92FS, Melee M9, Support M67, head Black Cap, backpack Standard Backpack, backpack A usable, B–D unusable.
49. Try to release the primary. → Refused; slot still M16A3.
50. Equip a knife into backpack A. → Refused. Equip a pistol into backpack A. → Accepted. Equip a second copy of the same pistol into the secondary slot. → "You cannot equip two of the same item at the same time."
51. Room Pistols Only, loadout M16A3 / M92FS / M9 / M67. → READY allowed (a pistol is equipped). In the match the player carries M92FS and M9 only.
52. Room Sniper Only, base loadout. → READY refused with the sniper message and "Go to Shop".
53. Room No Backpack, pistol in backpack A. → READY allowed; in the match the backpack pistol is missing.
54. Room No Explosives, base loadout. → READY allowed (M16A3 is a non-explosive primary); in the match the M67 is missing.
55. Product with `Consumetype0` "0" and `Period0` "7" bought at day 0. → Item expires at day 7. `Period0` "999" → never. `Consumetype0` "5", `Period0` "3" → 3 hours.
56. Equipped primary AK-47 (30-day) expires; player owns an M4A1 and an MP5A4. → Slot falls back to M16A3 at once; on the next inventory visit it is replaced by the M4A1 (same weapon type, assault rifle, wins over the SMG).
57. An equipped item's expiry passes during a match. → The match inventory is unchanged; after the result the item is gone and the expired-items notice shows on the next waiting-room visit.

**Result and leaving (G, X)**

58. Match ends; player does nothing on the result screen. → At 60000 ms the waiting room shows; the player is *waiting*, not ready.
59. Esc in a match while holding forward. → Menu opens; the player stops (movement flags cleared), mouse look frozen, crouch kept if held; the match continues. CANCEL → controls back.
60. EXIT, Yes. → Leaving event to the game server, disconnect, return-to-room (normal) to the lobby, waiting room shown; the lobby sets the player *waiting*; the game server runs its leaving rules (`game_modes.md` test 24 for TDM).
61. EXIT, then START again while the room is still in game with Join Mid Combat On. → The player is sent back into the same match.

## Open questions

1. **Steam build**: the client logic read is a BR-configured build and the layouts come from a classic-client data set; the US data set supplied names and ranges. Check on the Steam build that the room list columns, the room options and the countdown are the same.
2. **Region branches of the lobby rules**: the lobby rules used are the branch the reference marks as US (S&D 3 v 3 to start, minimum 6 players; CTF 2 v 2, minimum 4). The US room data instead lets S&D rooms be made with 2 players, which then can never start. Check what the US/Steam lobby required: try to start S&D with 1 v 1, then 2 v 2, then 3 v 3.
3. **Super Soldiers defaults**: the lobby takes each mode's default and whether it may be changed from its own configuration file, which was not available; the client also enables it only on "recreation" channels. Offline proposal: off by default, changeable in Elimination, One Man Army and VIP Elimination. Check live.
4. **Edit box lengths**: the creation and join-password layouts give MaxLength 15 (name) and 3 (password), while the service stores 32 and 8. Whether the typed limit is MaxLength or MaxLength − 1 (`ui_system.md` I4 says − 1 for chat) and whether the US layout really allows only 2–3 password characters needs a live check (type a long name and password).
5. **Kicking offline**: normal hosts cannot kick in the reference. For LAN we may want to give the host the Elite Moderator kick (and its re-entry block). Reviewer decision.
6. **Expiry while online but between matches**: the service may also expire items on a timer; only the login, after-match and after-single-play checks were traced.
7. **Rental item numbering for weapons with indices above 999** and the weapon item-number mapping past 999 were not traced (weapons below 1000 use their index).
8. **Pure Combat lists** (which modes, maps and items are allowed) come only from the service. Offline we have no data for them; proposal: do not offer Pure Combat rooms offline.
9. **New Pure Combat set**: the contents of a new player's Pure Combat set (base weapons, backpack or none) were not traced; only the expiry fallbacks are known.
10. **The function item "change weapon set"** exists (a set switch); whether it changes the set used in a match or allows switching sets mid-match was not traced.
11. **Chat routing details** (whisper, clan, team chat in the room) and the lobby chat layouts were not traced beyond scope.
12. **Contradiction with `bots.md`**: it says bot Elimination's difficulty is shown as "Easy / Normal / Veteran". The US string table shows **NORMAL / HARD / EXTREME** for the same three levels (default the middle one). The levels and the default index agree; only the displayed names differ. The ranges (5–8 bots, goal 20–50 step 10, default 40) agree.
13. **Contradiction check with `game_modes.md`**: no conflict in the goal and player ranges of S2 (they match the room data). Two additions: the lobby's own goal ranges are wider in places (Elimination Pro up to 11, Go4CA Cup 3–9, bot Elimination up to 60), so a forged request can set values the client never offers; and S2's "Join in progress: lobby default" is On for every mode. The brief's "room options of S12" are taken to mean the mutation boxes' room option (Super Soldiers, O).
14. **Contradiction check with `ui_system.md`**: H says team and loadout are chosen "in the waiting room"; the loadout is chosen in the separate inventory screen reached from it (E). Not a real conflict. Its question 6 is answered above.
15. **Contradiction check with `inventory.md` and `attachments.md`**: none. Addition: the game server drops weapons the room's arms option forbids before building the in-match list (E), which `inventory.md` section 1 does not mention; `attachments.md` question 6 (lobby fitting rules) is answered in part by E (attachments per weapon per set; ownership is the service's).
16. **Host change during a countdown**: the client prints "Cancel..." and re-enables buttons, but the lobby keeps counting. Check live whether the match still starts.
