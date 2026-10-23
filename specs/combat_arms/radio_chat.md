# Combat Arms: radio and chat

Source basis: the reference material the user pointed the spec session to: the client's in-match chat panel and chat input, its radio menu and radio data reader, the text-command parser, the bad-word filter and its live word list, the per-account block list, the client's spam and whisper limiters, the client's handling of incoming chat, radio, whisper and clan messages, the lobby chat screen's sending path, the voice-chat channel switching and push-to-talk handling, the key-binding table and options screen bindings, a development seed player profile (default key bindings); the dedicated server's chat and radio relays and its per-player chat limiter; the regional build configurations that pick the limiter constants. The radio data file itself, the string table and the HUD layouts were not available in plain text (Open questions 1 to 3).
Status: draft

Sections: Summary; Units and conventions; Constants; Behavior (R: radio; C: chat channels and input; P: the chat panel; T: text commands; F: the bad-word filter; B: the block list (muting); L: spam and length limits; V: voice chat; S: what goes through the server); Per-tick order; Edge cases; Quirks; Answers to other specs' open questions; Test cases; Open questions.

---

## Summary

- Players talk in a match through four text channels and a radio. All and team chat go through the game server, which rate-limits them and stamps the sender's name; whisper and clan chat go through the online services (lobby and clan chat room) and never touch the game server.
- The radio is three numbered menus (report, answer, request) read from one attribute file. Picking an entry sends one small message; every receiver prints a bracketed line in the radio colour and plays a voice line in the *sender's* voice set, at the sender's position, out to a radius from the data.
- Filtering and muting are done by each receiving client, by its own options: the server relays the raw text. The bad-word filter replaces listed substrings with `**`, even inside other words, unless a whitelisted word in the same message contains them.
- Spam limits exist on both sides: the sender's client refuses the 21st line in 60 s (a 180 s lock) and the 11th line after ten identical ones (a 60 s lock); the server silently drops on the same rates.

## Units and conventions

- **Time:** the chat panel, repeat lock and radio cooldown use the millisecond wall clock (ms). The client's rate limiters use the engine's client clock in seconds (s). The server's limiter uses its millisecond wall clock. None of this is tied to the tick.
- **Distance:** world units (u), as in `movement.md`; only the radio's sound radius uses it.
- **Colours:** (R, G, B), alpha 255 unless stated. Values written in the code as 0xAARRGGBB are converted here.
- **Text:** UTF-16 internally; a "character" is one UTF-16 unit. Lengths below count characters, not bytes.
- **Keys:** named by their keyboard key; profile bindings store DirectInput scan codes (V = 47, B = 48, N = 49, F2 = 60, F3 = 61, F4 = 62, T = 20, Enter = 28, 1 to 0 = 2 to 11, Delete = 211).
- **Regions:** the code base compiles one set of limiter constants for the BR region and another for every other region. This spec gives the non-BR set as the default and lists the BR set beside it (Open question 4).

## Constants

| Name (ours) | Value (non-BR) | BR | Unit | Meaning |
|---|---|---|---|---|
| chat_input_max | 119 | 119 | characters | in-match input box (layout MaxLength 120 − 1) |
| server_chat_max | 119 | 119 | characters | the server reads at most 120 units including the terminator |
| lobby_chat_max | 120 | 120 | characters | lobby and room chat (`lobby.md`) |
| chat_line_life | 10000 | 10000 | ms | every player, team, radio, whisper, clan and most system lines |
| same_sentence_notice_life | 5000 | 5000 | ms | the "do not repeat yourself" notice |
| rate_slots | 20 | 5 | sends | client: sends allowed in one window |
| rate_window | 60 | 10 | s | client: how long one send holds a slot |
| rate_block | 180 | 60 | s | client: lock after a refused send |
| repeat_count | 10 | 10 | sends | client: identical consecutive sends before the lock |
| repeat_lock | 60000 | 60000 | ms | client: lock after `repeat_count` identical sends |
| server_rate_slots | 20 | 20 | messages | server: per player |
| server_rate_window | 60000 | 60000 | ms | server: a slot is free when older than this |
| server_rate_block | 180000 | 180000 | ms | server: silent drop period |
| server_repeat_count | 10 | 10 | messages | server: the n-th identical consecutive message is dropped |
| server_repeat_block | 60000 | 60000 | ms | server: silent drop period after a repeat |
| whisper_slots | 20 | 10 | whispers | client: whispers allowed in one window |
| whisper_window | 180 | 60 | s | client: how long one whisper holds a slot |
| whisper_repeat_limit | 3 | 3 | repeats | to one recipient, after the first copy |
| whisper_repeat_lock | 60 | 5 | s | to that recipient |
| whisper_recipients_tracked | 25 (+1) | 25 (+1) | names | per-recipient repeat records kept |
| recent_whisper_names | 5 | 5 | names | Up-arrow cycle list |
| lobby_repeat_count | 3 | 3 | sends | lobby: identical consecutive lines before the lock |
| lobby_repeat_lock | 60000 | 60000 | ms | lobby |
| radio_cooldown | 2000 | 2000 | ms | no radio menu after a radio send |
| radio_entries_shown | 9 | 9 | lines | per menu (keys 1 to 9) |
| radio_radius_default | 1000 | 1000 | u | `3DsoundRadius` default |
| radio_inner_ratio | 0.07 | 0.07 | | inner radius = 0.07 × outer |
| filter_replacement | `**` | `**` | | two asterisks, whatever the word's length |
| block_list_max | none | none | | no limit on muted names |
| whisper_ignore_max | 20 | 20 | names | dead feature (T2) |

## Behavior

### R1. Radio data (`RadioMsgSound.txt`, attributes archive)

- **Sound templates:** section `SoundTemplate`, keys `Template0`, `Template1`, ... until one is missing. A template is a voice set name (male, female, special models, infected voices). Its index in this list is what travels on the wire.
- **Classes:** section `RadioClass`, keys `RadioClass0`, `RadioClass1`, ... until one is missing; each value is a class name (trimmed). Class index 0 is the **report** menu, 1 the **answer** menu, 2 the **request** menu (fixed by the client). Later classes are read but never opened.
- **Entries:** for class name N, sections `N0`, `N1`, ... until one is missing; the section number is the entry index. Keys per entry:

| Key | Default | Meaning |
|---|---|---|
| `RadioClass` | 0 | read, unused |
| `SendToUs` | 1 | read, unused |
| `SendToEnemy` | 1 | 1: everyone hears it; 0: the sender's team only |
| `ListText` | (empty) | the line shown in the menu |
| `<Template>_Snd0`, `_Snd1`, ... | none | wave files for that template, until one is missing |
| `Chat0`, `Chat1`, ... | none | chat lines, until one is missing |
| `3Dsound` | 1 | 1: positional; 0: plain interface sound |
| `3DsoundRadius` | 1000 | outer radius in u |

- The menu title is the class name, and `ListText` and `Chat<k>` are shown verbatim (not looked up in the string table).

### R2. Opening and closing a radio menu

- **Keys** (profile bindings; options-screen labels in brackets): `RadioReport` [ReportMsg] opens report, `RadioAnswer` [TacticalMsg] opens answer, `RadioRequest` [SocialMsg] opens request. The development seed profile binds them to **B**, **V** and **N** (Open question 5).
- **Conditions** (all must hold, checked on the key press):
  - the local player is alive (any other state closes the menu instead);
  - at least `radio_cooldown` (2000 ms) since the last radio send;
  - the HUD is built;
  - weapon selection is possible (the respawn weapon-change prompt is not mid-way) and no vote-kick request or vote is on screen; a vote request window that cannot be used does not block.
- **Toggle:** the key of the menu already open closes it; another radio key switches to that menu.
- **Showing:** the title, then up to 9 entry lines; an entry index ≥ the class's entry count is hidden. There is no paging: entries past the ninth are never reachable.
- **Closing:** a pick, the same key again, the local player starting to die (dying hides it), entering the world.
- **Number keys while open:** keys bound to `Choose_1` to `Choose_9` pick an entry; `Choose_0` does nothing and leaves the menu open. Weapon-select keys are swallowed while a radio menu is open, so pressing 1 picks entry 1 and does not draw the primary.

### R3. Sending a radio message (sender's client)

1. Map the key to entry index i = 0 to 8; ignore if i ≥ the class's entry count (the menu stays open).
2. Find the sender's voice set: the base model record's `SoundTemplate` if not empty, else the figure type's `SoundTemplate` (a cosmetic character's figure type replaces the base one); while infected, the figure type's `QuarantineSoundTemplate` instead (a non-empty model `SoundTemplate` still wins). Its index t is the case-insensitive position of that name in the template list, or −1 if absent. If no name is found at all, t keeps its previous value (0 at start).
3. Hide the menu and stamp the radio cooldown.
4. Count n = number of `<template t>_Snd` files of the entry (0 if t is out of range). Draw r = rand() mod n, or r = 0 if n = 0.
5. Send one guaranteed message to the server: the sender's current position (compressed), class index, entry index i, r, the entry's `SendToEnemy`, and t (as a byte; −1 becomes 255).

### R4. Relay (server)

- The server reads the message and does **no** checks: not alive, not rate, not position.
- `SendToEnemy` = 1: one message to every client: position, class, entry, r, t, the sender's name, the sender's client id and the sender's name colour.
- `SendToEnemy` = 0: one message to each player whose team equals the sender's team (the sender included), with the same fields except that the client id and name colour are the **receiver's own** (Quirk 1). Spectators on another team id (255) get nothing.

### R5. Receiving a radio message (every receiving client)

1. Build the line: `[<class name>] <sender name> : <Chat r>`. If r ≥ the number of chat lines the text part is empty.
2. Show it as a team-flagged radio line (P2): block list, Hunted rule, the radio colour (107, 142, 242) for **every** receiver (enemies too), the name colour if non-zero, the filter if the receiver has it on.
3. Play the sound, even if step 2 dropped the line (Quirk 2):
   - file: the `<template t>_Snd` file number r; if r ≥ the number of **chat lines** there is no sound (Quirk 3); if r ≥ the number of that template's files, file 0; if t is out of range or the template has no files, no sound;
   - `3Dsound` = 1: a 3D sound at the position the sender sent, outer radius R = `3DsoundRadius`, inner radius 0.07 R, not attached (it does not follow the speaker);
   - `3Dsound` = 0: a local, non-positional sound at full volume.
- The sender receives and plays their own radio like everyone else.
- There is **no radar or HUD marker** for a radio message.

### R6. Emote menu (Hunted only)

- In Hunted (`HG`) the hard-wired **G** key opens a fourth page of the same menu if emotes are allowed and the player is alive (same vote gate as R2). Title `$Emotion_listClass_0`; lines `Emotion_list_1` to `Emotion_list_9` from the string table, a line hidden if its id does not resolve.
- Keys 1 to 4 pick insult, surrender, dance and salute (only if no emote is already playing); any number key closes the page. This is an animation, not a chat line; no radio message is sent. The animation itself belongs to a later spec.

### C1. Channels

| Channel | Opened by | Carried by | Who receives |
|---|---|---|---|
| All | Enter (if All is the current tab), `BroadCastChat` (F2 in the seed profile), F2 inside the input | game server | every client in the match, alive, dead or spectating |
| Team | `TeamChat` (F3), F3 inside the input | game server | every player whose team equals the sender's, plus every player on team 255 (GM spectators); alive or dead |
| Clan | `ClanChat` (F4), F4 inside the input, the clan text command | the clan chat room service | every online member of the sender's clan, in or out of matches |
| Whisper | the whisper text command, Up arrow | the lobby service | one named player anywhere online |
| Radio | radio keys (R2) | game server | R4 |
| Lobby / room | the lobby chat window | the lobby service | `lobby.md` K (room players, or the channel's players outside rooms) |

- The game server never sees clan, whisper or lobby text, and clan and whisper never pass through the server's limiter.
- **Dead players** can send and receive all channels in every mode, with three exceptions:
  - **Last Man Standing:** a player whose HUD is in the dead state cannot send all or team chat: Enter on a non-empty line closes the input and sends nothing, with no notice. Clan and whisper still work.
  - **Hunted:** a living receiver drops all and team lines (radio included) whose sender is dead. Dead receivers see everything.
  - **Voice** (V): dead players in Last Man Standing and Hunted leave the voice channel.
- GM accounts: the channel binds do nothing, and F3 or F4 inside the input pick All in GM spectator HUD mode.

### C2. The input bar

- **Opening with Enter:** only while playing, not during a screen fade, not if the local team is "nothing", not if the input is already open and not on the same frame a line was sent (the Enter that sends does not reopen). Any open weapon or ammo chooser closes. The input opens on the **current tab**, which persists between openings and is reset to All on entering the world.
- **Opening with a channel bind:** sets the tab, then opens.
- **While open** the chat panel takes every key:
  - F2, F3, F4: switch tab (the active tab's button is shown disabled);
  - Esc: close and clear;
  - Enter: send (C3);
  - Up: cycle the recent whisper partners (T3);
  - Page Up, Page Down, Down: swallowed;
  - anything else: typed into the box (IME supported; paste only for GM accounts).
- Nothing opens while the HUD is hidden (the HUD on/off bind) or in the referee spectator HUD.
- **Panel windows:** `Active` (input bar, tabs `Tab01` all, `Tab02` team, `Tab03` clan, `chat_edit` box) while open; `NonActive` otherwise. Both carry `VoiceChat_OnOff` and `VoiceChat_Talk`.

### C3. Sending a line (sender's client, in order)

1. Empty box: close, nothing else.
2. Copy the text (at most 119 characters).
3. Parse text commands (T1). A setting command, or a command with nothing after it, ends here: show its reply (red, 10000 ms) if it has one, close.
4. Route:
   - whisper command: take the first word of the remainder as the name, the rest (after one separator) as the body; no body: close; else W1;
   - "normal" command: the remainder is sent like a typed line on the current tab, except that a clan tab sends to **All** (step 5);
   - clan command: the remainder goes to clan (step 6);
   - anything else: the **whole typed line** goes to the current tab (All or Team: step 5; Clan: step 6).
5. **All or team:**
   1. Last Man Standing and dead: close (C1).
   2. Repeat lock (L2): if locked, show the same-sentence notice (5000 ms) and close.
   3. GM accounts only: a line starting with `/` is offered to the admin commands (`kick`, `ban`, `unban`, `hwban`, `hwunban`, `trace`, `join`, `punish`, `unpunish`, `restrict`, `unrestrict`, `userscreen`); if taken, close.
   4. Chat punishment (L4): if punished, show the notice and close.
   5. Rate limit (L1): if refused, show the notice and close.
   6. Send (team flag, text) to the server, take a rate slot, update the repeat counter, close.
   - The text goes out **unfiltered**; the sender sees their own line only when the server echoes it.
6. **Clan:** GM admin commands first; then the punishment check; then the text goes to the clan chat room service (UTF-8). No rate or repeat limit. The input closes. The sender sees their line when the room echoes it.

### W1. Whispers (sender)

1. **Global whisper rate:** if all `whisper_slots` are held (20 whispers sent in the last 180 s), show `$BlockWhisper` (red, 10000 ms) and stop. The input stays open (Quirk 6).
2. **Per-recipient repeat** (L3): if this recipient is locked, show `$UserSpamBlockWhisper` (red, 10000 ms) and stop; the input stays open.
3. Take a whisper slot; show the sender's own echo `<to-label> "<name>" : <body>` in (66, 158, 83), 10000 ms; send name and body to the lobby; close the input.
- The echo is local; the lobby replies only on failure, with a red 10000 ms line: `$UNAVAILABLE_NOW`, `$SERVER_IS_BUSY`, `$SEND_COUNT_LIMIT`, `$NOT_FOUND_RECEIVER`, `$RECEIVER_ONLINE_AND_SEND_BLOCKED`, `$RECEIVER_ONLINE_AND_CANNOT_RECEIVE_NOW`, otherwise the fixed text "Failed to send Whisper.".
- If the sender's own "accept whispers" option is off, the whisper command fails with an error line before step 1 (T2).

### W2. Whispers (receiver)

1. A sender on the block list (B1): drop silently (no auto-reply either).
2. Show `<from-label> "<name>" : <body>` in (60, 230, 92), 10000 ms, filtered by the receiver's option (the body, then the whole line again). In a match it goes to the HUD chat panel; in menus to the lobby chat window.
3. Remember the sender for the Up-arrow list (T3).
4. **Auto-reply:** if the receiver has an auto-reply text set, the sender is not the receiver, and the body does not start with the auto-reply title (a string-table prefix), whisper back `<title><auto text>` straight to the lobby, bypassing W1's limits. The title check stops two auto-replies from bouncing forever.
- In the Go4CA Cup mode (`ESL`) whisper lines, received or echoed, are not shown; sending still works.

### K1. Clan chat (receiver)

- In a match: `[<receiver's own clan name>]<sender> : <text>` (no space after the bracket; the sender is the part of the room id before `@`), clan colour (35, 206, 255), 10000 ms, filtered by the receiver's option. The block list does **not** apply to clan lines.
- Clan-room messages used by the "follow clan member" feature are consumed and not shown.

### M1. Megaphone lines

- A megaphone item broadcasts through the lobby: `$title_ChatMegaphone` + name + ` : ` + text, 10000 ms, white with the name in the buyer's name colour and the text in an item colour table. Blocked senders and the Go4CA Cup mode hide it. Its purchase and colours belong to a shop spec.

### P1. Chat panel

`ui_system.md` H gives the geometry, stacking, overflow, expiry and channel colours; this spec adds:

- **Line content:**
  - all and team: `<name> : <text>`, composed by the server from the sender's account name;
  - radio: R5;
  - whisper: W1, W2;
  - clan: K1;
  - bot lines: `bots.md` 10.
- **Plain text:** player lines are drawn as plain text, so markup and colour codes in a player's text are shown literally. System lines use the string table's markup.
- **System lines:** red (255, 0, 0) for spam refusals, whisper refusals and errors, voice messages and command replies, 10000 ms; the same-sentence notice in the HUD's same-sentence colour, 5000 ms; the punishment notice in yellow (255, 255, 0), 10000 ms; lobby notices in yellow (255, 255, 0), 10000 ms.
- **History:** the last 20 lines (any kind, after filtering) are kept for the in-game user report, which attaches them concatenated without separators.
- A GM screenshot flag can hide the panel.

### P2. Showing an incoming all, team or radio line (receiving client, in order)

1. **Block list:** sender name on the list (B1): drop.
2. **Server text** (client id "none"): shown as a bad-news progress message for 10000 ms instead (not in the chat panel).
3. Look up the client id; unknown: drop.
4. **Hunted:** if the receiver is alive and that client is dead: drop.
5. **Colour:**
   - radio: radio (107, 142, 242);
   - team: team (196, 196, 196), or your own team chat (255, 255, 255) when the sender name equals the receiver's own name;
   - all: teammate (63, 191, 87) if that client's team equals the receiver's team, else enemy (230, 87, 87). In one-team modes everyone is on Alpha, so every all-chat line is green;
   - then, whatever the channel, a sender name starting with `GM` (case-sensitive) gets the GM colour (255, 240, 0).
6. **Filter:** if the receiver's chat filter option is on, the whole line, name included, goes through F1.
7. **Name colour:** if the name colour is not 0, the first occurrence of the sender's name in the filtered line gets it; if the filter changed the name, nothing is recoloured.
8. Queue the line for 10000 ms (`ui_system.md` H: overflow, expiry).

### T1. Text commands (parsing)

- **Data:** nine string-table entries, one per command: `ChatCMD_Clan`, `ChatCMD_Normal`, `ChatCMD_Block`, `ChatCMD_UnBlock`, `ChatCMD_Whisper`, `ChatCMD_WhisperEnable`, `ChatCMD_WhisperIgnore`, `ChatCMD_WhisperUnignore`, `ChatCMD_WhisperAutoAnswer`. Each value is a list of aliases whose **first character is the separator** and also begins every alias: a value `/x/xyz` gives the aliases `/x` and `/xyz`. Values were not read (Open question 2).
- **Matching:**
  - The commands are tried in the order above; within a command, its aliases in order.
  - An alias matches when the typed line starts with it, compared case-insensitively. There is **no word boundary**: alias `/x` also matches `/xylophone`.
  - Within one command the first matching alias counts. A later command that matches **replaces** an earlier one, so the result is the last command in the list order with any matching alias.
  - The remainder is everything after the matched alias, its leading space included.
- **No command matched:** the whole line is ordinary chat.
- **A command with an empty remainder,** or a setting command (T2), never sends anything.

### T2. Setting commands (client only)

| Command | Remainder ≤ 1 character | Longer remainder (first word = name) |
|---|---|---|
| WhisperEnable | toggle your own "accept whispers" option (saved with the lobby); reply enabled or disabled | not a setting: the whole typed line is sent as chat (Quirk 4) |
| WhisperAutoAnswer | clear the auto-reply; reply "reset" | set the auto-reply to the whole remainder (leading space included); reply `"<remainder>" <set-text>` |
| Block | reply with help text | add the name to the block list (B1); success reply only if added |
| UnBlock | reply with help text | remove the name; success reply always |
| Whisper | nothing sent | remember the name as the whisper target; if your own whispers are off, reply with an error and send nothing; else continue to W1 |
| WhisperIgnore, WhisperUnignore | nothing sent | not settings: the whole typed line is sent as chat (Quirk 4); the ignore list they were meant for is never consulted |
| Normal | nothing sent | the remainder is sent to All or Team (C3 step 4) |
| Clan | nothing sent | the remainder is sent to clan |

- Replies are red system lines, 10000 ms, shown only if longer than 1 character.

### T3. Recent whisper partners

- A list of up to 5 names, most recent first, without duplicates. A name is added (pushed to the front, the oldest dropped) when a whisper is received or sent and it is not already listed; a name already listed **keeps its place**. Your own name is never added.
- **Up** in the input replaces the text with `/w <name> `, where name is the entry after the last one shown (wrapping to the first). The first press shows the first entry.

### F1. Bad-word filter

- **Where it lives:** compiled into the client: a convert list (each word becomes `**`), a whitelist of safe words, and a block list used for names and room titles (out of scope). Plus a **live list** pushed by the lobby service and edited by GMs (`/filter add`, `/filter del`, `/filter list <n>` in the lobby, GM accounts only), stored lower-cased.
- **Who filters:** each receiving client, only if its own chat-filter option is on (an account option saved with the lobby service). The server and the sender never filter.
- **What is filtered:** all, team and radio lines (the whole composed line), whispers, clan lines, lobby lines.
- **Algorithm** for one message m:
  1. Make b = m with ASCII A to Z lower-cased; other letters are not folded.
  2. S = every whitelist word that occurs anywhere in b (computed once).
  3. Sort the convert list by length, longest first (ties in no defined order). For k = 0, 1, ..., N − 1:
     - find the first occurrence of word w_k in b;
     - if found and w_k is not a substring of any word in S: replace that occurrence by `**` in both b and m (same position), then continue with k = 1 (Quirk 5);
     - if found but whitelisted: leave it and continue with k + 1.
  4. The live list the same way, with no whitelist, after the built-in list.
  5. The result is m.
- **Consequences:**
  - A listed word inside a longer word is replaced (`**` in the middle of the word).
  - A whitelisted word protects **every** occurrence of the bad words it contains, anywhere in the message, not only the one inside it.
  - Case is kept outside the replaced spans.
  - Accented and non-Latin letters are matched case-sensitively.

### B1. Block list (muting)

- **Storage:** one local file per account in the game's settings folder, entries `BlackUser0`, `BlackUser1`, ... in section `BlackUserList`. There is no size limit. It is never sent to a server.
- **Adding:** the Block text command, the player tooltip's block button (right-click a name), or the options screen. Your own name is refused, and so is a duplicate (exact string).
- **Removing:** the UnBlock command or the options screen.
- **Matching** an incoming name against the entries:
  - Each entry is a **wildcard pattern**, case-insensitive (`*` any run, `?` one character).
  - A name starting with `GM` (case-insensitive) is never blocked; nor is your own name.
- **Effect:** drops all, team and radio lines (the radio **sound still plays**, Quirk 2), whispers (no auto-reply), lobby lines and megaphone lines from matching names. Clan lines and voice are not affected. The sender is not told.

### L1. Rate limit

- **Client:** each all or team send, and each lobby line, takes one of `rate_slots` slots for `rate_window` seconds. A send at time t is refused when no slot is free:

```math
\mathrm{refused}(t) \iff t < t_b + T_{block} \;\lor\; \big|\{\, s \in \mathrm{sent} : t - s < T_{win} \,\}\big| \ge N_{slots}
```

  - Here t_b is the start of the last block. A refusal shows the spam notice (red, 10000 ms) and, if no block is running, starts one (t_b = t). Refusals during a block do not extend it.
  - Non-BR: N_slots = 20, T_win = 60 s, T_block = 180 s. BR: 5, 10 s, 60 s.
  - Refused attempts take no slot. Whispers, clan lines and radio never count.
- **Server**, per player, for all and team chat only, silent:
  1. While t < the block end: drop.
  2. Look for a slot, out of 20, that is empty or older than 60000 ms (strictly). If one is found, store t in it; if none, drop and set the block end to t + 180000 ms.
  3. Repeat check (L2). A message dropped by the repeat check has still taken its slot.
- Radio messages are not limited by the server or the sender's client, beyond the 2000 ms menu cooldown.

### L2. Repeat limit

- **Client:** a counter c and the last sent text x̂.
  - Before sending (C3 5.2): if c ≤ 0, then if at least 60000 ms have passed since the lock time L, set c = 10 and go on; else refuse.
  - After a successful send of x: if x = x̂, then c = c − 1, and if c ≤ 0, L = now. Otherwise x̂ = x and c = 9.
  - So ten identical consecutive lines are all sent; the next line, **of any text**, within 60000 ms of the tenth is refused. Whether c starts at 10 is Open question 7.
- **Server:** a count n and the last accepted text.
  - The same text as last: n = n + 1, and when n reaches 10, drop it, clear the last text, set n = 0 and block for 60000 ms.
  - A different text: last = it, n = 1.
  - Together with the client, the receivers see the first **nine** of ten identical lines.
- **Lobby:** the lobby window counts identical consecutive lines (GM and referee accounts exempt). At 3 it locks normal lobby chat for 60000 ms, showing the same-sentence notice on each attempt.

### L3. Whisper limits

- **Global:** a whisper takes one of `whisper_slots` slots for `whisper_window` seconds (non-BR 20 per 180 s; BR 10 per 60 s). With every slot held, the whisper is refused. There is no extra lock.
- **Per recipient:** a record (count k, last text, locked flag, lock time) for each recipient name.
  - **First whisper to a name:** a record with k = 0 and that text; sent. If more than 25 records exist before adding, the least recently used one is dropped first.
  - **Locked and the lock older than `whisper_repeat_lock`** (60 s; BR 5 s; measured on the wall clock, strictly greater): unlock, k = 0, last text cleared; sent.
  - **Not locked, same text as last:** k = k + 1; at k = 3 it locks now, and this whisper is refused.
  - **Not locked, different text:** k = 0, last = this text; sent.
  - **Locked, not expired:** refused, whatever the text.
  - The record moves to most-recently-used on every check.
  - So the 4th identical consecutive whisper to one person is refused, and so is everything to them for the next 60 s.

### L4. Chat punishment

- A GM can punish an account (`/punish`, `/unpunish`); the lobby keeps the remaining minutes M.
- While M > 0, all, team, clan and lobby sends are refused with a yellow 10000 ms notice. The notice says ⌊M / 60⌋ hours when M ≥ 60, else a "less than an hour" text. Whispers are not checked.

### L5. Length

- The input box accepts at most 119 characters; typing stops there.
- The server reads at most 119 characters and composes `<name> : <text>` (the name being at most 32 characters, the buffer 376).
- Clients read the composed line up to 375 characters.
- Whisper bodies are at most what is left of 119 after the command and the name.

### V1. Voice chat

- **Service:** an external voice service (not the game server), joined with an account-derived id. The lobby gives each room its channel addresses. Voice can be switched off for a room by the service.
- **Channels per mode** (switched on entering a match, on every team change and on infection changes):
  - One Man Army, Arms Race, Last Man Standing, Hunted: one shared channel ("Alpha") for everyone.
  - Spy Hunt: Alpha players join the Alpha channel; everyone else leaves voice.
  - Fireteam: a player moved to Bravo leaves voice.
  - All other modes: the Alpha or Bravo channel by team.
  - Last Man Standing and Hunted: a dead player leaves (also when joining dead).
  - GM spectators listen to Alpha and switch with F5 (Alpha) and F6 (Bravo, two-team modes only).
- **Keys:**
  - `VoiceChatToggle` (T in the seed profile) is push-to-talk. Pressing it unmutes the microphone and shows the `VoiceChat_Talk` icon; the frame it is no longer held, it mutes again.
  - If the player is not in a channel on press, a red line says why: disabled for non-Alpha Spy Hunt players; disabled in this mode for dead Last Man Standing or Hunted players; disconnected otherwise. The microphone is unmuted anyway (harmless: no channel).
  - **F8** toggles voice output on and off: it saves the option and prints an enabled or disabled red line (10000 ms) and `$VoiceChatEnable` / `$VoiceChatDisable` on the panel.
- **Profile:** `EnableVoiceChat` (seed FALSE), mic and speaker device and volume (50).

### S1. Server versus client

| Step | Where |
|---|---|
| Radio menu, entry choice, voice set, random line, position | sender's client |
| Radio relay, team selection | server (no checks) |
| Radio line text, colour, sound file, 3D playback | each receiving client |
| All and team: commands, rate and repeat refusal, punishment check | sender's client |
| All and team: rate and repeat drop, name stamping, chat log, team routing | server |
| Block list, Hunted dead filter, colours, bad-word filter, name colour, panel | each receiving client |
| Whisper limits and own echo | sender's client |
| Whisper and clan delivery, live filter list, punishment minutes, options | online services |
| Voice | external voice service; channel choice by each client |

## Per-tick order

There is no chat tick. Events are handled when they arrive, and the panel updates once per rendered frame:

1. **Input** (sender, on the key press): open, switch tab, type, or send (C3 steps in order: command parse, route, LMS check, repeat lock, GM command, punishment, rate, send, bookkeeping, close).
2. **Network** (server, when the message arrives): read the team flag and the text; rate (L1); repeat (L2); compose `name : text`; log; send to the chosen clients in one pass.
3. **Network** (receiver, when the message arrives): P2 for chat; R5 for radio (text first, then sound).
4. **Frame** (receiver): the panel's expiry pass (oldest line removed per expired line, `ui_system.md` Quirk 11), then push-to-talk release check, then the input's "sent this frame" flag reset once the input is closed.

## Edge cases

- **Dead player's team chat:** delivered to every teammate, alive or dead, in every mode except Last Man Standing (not sent) and Hunted (hidden from living receivers).
- **Spectators** (team 255) receive every team's team chat and every enemy-audible radio, but no team-only radio.
- **A message at the length limit:** 119 characters are sent and shown whole (wrapped). The 120th keystroke is ignored.
- **A line of spaces** is sent (length ≥ 1).
- **A filtered word inside another word:** replaced, unless a whitelisted word in the same message contains it (F1).
- **The sender's name contains a filtered word:** the name itself is changed to `**` on filtering receivers and loses its name colour.
- **Normal command:** its remainder keeps the leading space, so the line reads `Name :  text` (two spaces).
- **A radio entry with more voice files than chat lines:** the extra files are never heard (Quirk 3). With more chat lines than files, the extra lines are never chosen.
- **Unknown voice set** (template not in the list): t = 255 is sent; receivers show the text with chat line 0 and play nothing.
- **A radio key during the 2000 ms cooldown** does nothing; it does not queue.
- **Whisper to yourself** goes to the lobby, which decides; your own name never enters the recent list.
- **A blocked name typed with different case:** blocked (matching is case-insensitive). Adding the same name in different case creates a second entry (duplicate check is exact).

## Quirks

1. **Team radio carries the receiver's id and name colour.** The name text is right, but the name colour is the receiver's own, and the Hunted dead-sender check looks at the receiver (so it never hides team radio). Keep for parity (it is invisible unless name colours differ).
2. **Muting does not mute radio sounds:** the line is dropped, the voice still plays. Keep.
3. **Radio sounds are capped by the chat-line count:** the random index is drawn over the voice files, but a sound is played only if the index is also below the number of chat lines. Keep (data-dependent).
4. **Unhandled commands are broadcast:** `WhisperIgnore`, `WhisperUnignore` and `WhisperEnable` with a name are sent as ordinary chat with the command text included. Keep for parity; an implementation may later make them do what they say (opt-in).
5. **The filter's restart skips the longest word:** after any replacement the scan restarts at the second entry, so a second occurrence of the first (longest) listed word survives. Keep (rarely visible).
6. **Refused whispers leave the input open** with the text, while refused chat closes it. Keep.
7. **Repeat lock blocks everything:** after ten identical lines, a *different* line is refused for the rest of the 60 s too. Keep.
8. **The server's repeat drop is silent:** the tenth identical line is accepted by the sender's client, counted, and never shown to anyone. Keep.
9. **Clan lines are never muted or rate-limited.** Keep.
10. **Removing a live filter word that is not first in the list hangs the lobby client** (the search never advances). Do not keep.

## Answers to other specs' open questions

- **`lobby.md` open question 11 (chat routing):** whispers and clan chat go through the lobby and clan-room services (W1, W2, K1); lobby lines use the same client rate limiter as in-match chat plus a 3-line repeat lock (L1, L2). The room's team chat is not traced here.
- **`ui_system.md` constants ("system lines use 5000 or 15000"):** in a match most system lines (spam, voice, command replies, whisper errors, punishment, lobby notices) use 10000 ms; the same-sentence notice uses 5000. 15000 was not seen in the chat paths read (Open question 8).

## Test cases

Unless stated: non-BR constants; a two-team mode (Elimination); players A1, A2 on Alpha and B1 on Bravo; S is a GM spectator on team 255; filters off; empty block lists; every clock at 0 at the start; the HUD at 1024 x 768.

**Radio (given a radio file):** templates `Template0 = Male`, `Template1 = Female`; classes `RadioClass0 = Report`, `RadioClass1 = Answer`, `RadioClass2 = Request`; entry `Report0` with `ListText = Enemy spotted`, `Male_Snd0`, `Male_Snd1`, `Female_Snd0`, `Chat0 = Enemy spotted!`, `Chat1 = Contact!`, `SendToEnemy = 0`, no `3Dsound` or `3DsoundRadius`; entry `Answer0` with `SendToEnemy = 1`, `3Dsound = 0`, `Male_Snd0`, `Chat0 = Roger`.

1. A1 (Male, alive, at (100, 0, 0)) presses the report key at t = 0 → the menu shows title "Report" and one line "Enemy spotted"; lines 2 to 9 hidden.
2. A1 then presses 1, the draw gives r = 1 → A1 and A2 each show `[Report] A1 : Contact!` in (107, 142, 242) for 10000 ms and play `Male_Snd1` in 3D at (100, 0, 0), outer 1000 u, inner 70 u. B1 and S get nothing.
3. Same as 2 but A1's voice set is Female (one file, `Female_Snd0`), r = 0 → line `[Report] A1 : Enemy spotted!`; sound `Female_Snd0`.
4. A Female sender's message arrives with t = 1 and r = 1 (impossible from a real client, constructed) → line `[Report] A1 : Contact!`; sound falls back to `Female_Snd0`.
5. A1 presses the report key again at t = 1500 ms after case 2's send → nothing opens (cooldown). At t = 2000 ms → it opens.
6. A1 opens report, then presses the answer key → the answer menu replaces it. Pressing the answer key again → closed.
7. A1 opens the answer menu and presses 0 → nothing is sent; the menu stays open. Pressing 2 → nothing sent (only one entry); the menu stays open.
8. A1 picks `Answer0` → A1, A2, B1 and S all show `[Answer] A1 : Roger` in the radio colour, and all play `Male_Snd0` as a non-positional sound at full volume.
9. A1 is dead and presses a radio key → no menu.
10. A1 has the report menu open and is killed → the menu disappears when A1 starts dying.
11. A2 has A1 on the block list; case 2 → A2 shows no line but plays `Male_Snd1` at (100, 0, 0).
12. A1 opens report and presses 1 while holding the primary weapon key binding on the same key → entry 1 is sent; no weapon switch.
13. Unknown voice set: A1's template name is not in the list → t = 255 → receivers show `[Report] A1 : Enemy spotted!` and play no sound.

**Chat routing and colours**

14. A1 sends "hi" on All at t = 0 → every client shows `A1 : hi`: A1 and A2 in (63, 191, 87), B1 in (230, 87, 87), S in (230, 87, 87) (team 255 ≠ the sender's); 10000 ms.
15. A1 sends "push" on Team → A1 shows `A1 : push` in (255, 255, 255); A2 in (196, 196, 196); S in (196, 196, 196); B1 nothing.
16. A1 is dead in Elimination and sends Team "rush B" → A2 (alive) and S show it.
17. Last Man Standing, A1 dead (HUD dead), Enter on "gg" → input closes, nothing sent, no notice.
18. Hunted, A1 dead sends All "lol" → B1 (alive) shows nothing; A2 (dead) shows `A1 : lol`.
19. One Man Army (everyone on Alpha): A1 sends All "hi" → every receiver shows it in (63, 191, 87).
20. A player named `GMx` sends All "hello" → every receiver shows it in (255, 240, 0).
21. A1 has a name colour; A2 receives All "A1 rocks" from A1 → the line `A1 : A1 rocks` with the first `A1` in the name colour, the rest in the channel colour.
22. Chat input, F3 then Esc, then Enter → the input opens on Team (the tab persists).
23. Entering a new map → the tab is All.

**Commands (given `ChatCMD_Whisper = /w/whisper`, `ChatCMD_Normal = /a/all`, `ChatCMD_Block = /block`, `ChatCMD_WhisperIgnore = /ignore`; other commands absent)**

24. A1 types `/whisper Bob hi there` → whisper to "Bob" with body "hi there"; A1 shows `<to-label> "Bob" : hi there` in (66, 158, 83).
25. A1 types `/w Bob` → nothing sent; input closes.
26. A1 types `/wave` → it matches the alias `/w` (no word boundary): the remainder `ave` makes "ave" the whisper target with no body → nothing sent; input closes.
27. A1 on the Team tab types `/a go` → the remainder ` go` is sent; receivers on Alpha show `A1 :  go` (two spaces) as **team** chat (the Normal command keeps the current tab).
28. A1 on the Clan tab types `/a hi` → `A1 :  hi` goes to All.
29. A1 types `/ignore Bob` → every All-tab receiver shows `A1 : /ignore Bob`.
30. A1 types `/block A2` → reply line (red, 10000 ms); A2's later lines and A2's radio text are no longer shown to A1; A2 is not told.
31. Block list entry `bo?` → lines from `Bob` and `BOX` are dropped; `Bobby` is not.
32. Block list entry `*` → every player's lines are dropped except names starting `GM`/`gm` and A1's own.

**Filter (hypothetical lists: convert words `abc` and `xyzzy`, whitelist word `zabcz`; filter on for the receiver)**

33. "ABC def" → `** def`.
34. "fooabcbar" → `foo**bar`.
35. "zabcz abc" → unchanged (the whitelisted word protects both occurrences).
36. "xyzzy and xyzzy" (the longest word is first in the sorted list) → `** and xyzzy`.
37. "abc abc" (`abc` second in the list) → `** **`.
38. A sender named `abcman` sends All "hi" → filtering receivers show `**man : hi` with no name colour; non-filtering receivers show `abcman : hi`.

**Limits**

39. A1 sends 20 different All lines at t = 0, 1, ..., 19 s; the 21st at t = 20 s → refused (red spam notice); a block runs to t = 200 s. Lines at t = 100 s and t = 199 s → refused, notice each time. At t = 200 s → sent.
40. A1 sends 20 lines at t = 0 to 19 s, then waits; at t = 60 s → sent (the slot from t = 0 is free: 60 − 0 ≥ 60).
41. Server only (a client without limits): 21 different messages at t = 0, 1, ..., 20 s → the 21st is dropped and the block ends at t = 200 s; a message at t = 199.999 s is dropped, one at t = 200 s accepted (drop while t < end).
42. A1 sends "go" 11 times, 1 s apart, starting at t = 0 → sends 1 to 10 (t = 0 to 9 s) leave A1's client; receivers show the first 9 (the server drops the 10th and blocks A1 until t = 69 s); the 11th attempt at t = 10 s is refused with the same-sentence notice for 5000 ms; a line of any text at t = 68.999 s is refused; at t = 69 s it is sent and shown.
43. A1 sends "a", "b", "a", "b", ... alternating → never locked by repeat.
44. A1 whispers "hey" to Bob 4 times within 10 s → the first three are sent; the 4th shows `$UserSpamBlockWhisper`, input stays open. A different text to Bob at +30 s → refused. Any text to Bob more than 60 s after the 4th → sent.
45. A1 whispers 20 different people once each within 100 s; a 21st whisper at 120 s → `$BlockWhisper`; at 180 s after the first → sent.
46. BR build: A1 sends 5 All lines within 2 s; the 6th at 3 s → refused, blocked until 63 s.
47. Lobby: A1 sends "sell" three times → all three go out; a 4th line of any text within 60000 ms → same-sentence notice, not sent.
48. Punished account with 125 minutes left sends Team "x" → yellow notice with 2 hours; nothing sent. With 30 minutes → the "less than an hour" notice.

**Whisper receive and auto-reply**

49. A1 set the auto-reply with the command and the remainder ` afk`; Bob whispers A1 "yo" → A1 shows `<from-label> "Bob" : yo` in (60, 230, 92); Bob receives the whisper `<title> afk` (the title followed by the remainder, its leading space kept).
50. Bob also has an auto-reply set; his auto-reply arrives at A1 → A1 shows it, and sends no auto-reply back (it starts with the title).
51. Recent list after whispers from C, D, C, E (in that order) → E, D, C; Up shows `/w E `, Up again `/w D `, again `/w C `, again `/w E `.

**Voice**

52. Spy Hunt, a Bravo player presses push-to-talk → red line "disabled"; no voice transmitted.
53. Last Man Standing, A1 dies → A1 leaves the voice channel; pressing push-to-talk → red line "disabled in this mode".
54. Elimination, A1 holds push-to-talk for 3 s → the talk icon is shown for 3 s; only Alpha's channel hears A1.

## Open questions

1. **Radio data values:** the class names, entries, list texts, chat lines, voice files per template, `SendToEnemy`, `3Dsound` and radii of the stock `RadioMsgSound.txt` were not read (the attributes archive needs keys). Needed to fill the menus; check by decrypting the Steam attributes archive or by screenshotting each radio menu and noting who hears each entry.
2. **Command aliases:** the values of the nine `ChatCMD_*` string-table entries (and the auto-reply title, the whisper to/from labels and the notice texts) were not read. The Up-arrow text `/w ` suggests `/w` is a whisper alias. Check the Steam string table or type candidates in game.
3. **Layout values:** the radio window (`Hud_Radio.xml`: `Radio_Title`, `Radio_Msg1`, `Radio_Msg2`, the line gap) and the HUD's same-sentence and mouse-sensitivity colours were not read.
4. **Region:** which limiter set the Steam build uses (non-BR assumed; the reference's release configuration is the BR one). Check: send 6 lines in 3 s; refused means BR.
5. **Default keys:** B, V, N for report, answer and request, F2/F3/F4 for all/team/clan and T for push-to-talk come from a development seed profile (where `BuyWeapon` also sits on B, and the fallback default for `TeamChat` is Delete). Check the Steam options screen's defaults.
6. **Chat-filter option default** for a new account (an account option kept by the lobby service) and whether the Steam build exposes it in options.
7. **Repeat counter start:** the client's repeat counter is not explicitly initialised; the spec assumes it starts full (10). Check: send ten identical lines right after joining a match and see whether the eleventh is refused.
8. **System line lives:** `ui_system.md` lists 5000 or 15000 ms for system lines; the chat paths read here use 10000 for most and 5000 for one. Reconcile with `ui_system.md`'s source for 15000.
9. **Voice service in the Steam build:** whether in-match voice works at all on the revived service, and whether others' talking is shown anywhere in a match (scoreboard voice icon). Not traced.
10. **Emote page:** the Hunted emote animations, and when emotes are allowed, belong to a later spec.

## Corrections (2026-10-22)

From a comparison pass over the reference's radio menu, chat input and
text-command parser against this spec and our implementation
(`games/combat_arms/src/`, file:line relative to that folder). They change
R2, T1, T2 and test cases 24 and 26.

1. **Only radio entries 1 to 6 can be picked** (R2 "Number keys while
   open"; `options.md` K1 "Fixed keys").
   - Reference: a radio entry is picked by the menu-line commands
     `Choose_1` … `Choose_9`, and the only keys ever bound to them are the
     ones every bindings apply adds: keyboard 1 to 6 for `Choose_1` …
     `Choose_6` (the apply first clears every binding, and `Choose_7` …
     `Choose_0` are not in the bindable action set, so profile entries for
     them are never bound). Keys 7, 8, 9 and 0 therefore do nothing while
     a menu is open: the menu stays open and nothing is sent. Lines 7 to 9
     are still shown when the class has that many entries; they cannot be
     sent. (The weapon-select keys 1 to 6 are swallowed while the menu is
     open, as R2 says.)
   - Ours: `ui/talk.rs:264-280` reads the physical digit keys 1 to 9 and
     picks entries 7 to 9 with them.
   - Expect: a class with 9 entries; open its menu, press 7 → no message
     sent, the menu stays open; press 6 → entry 6 is sent and the menu
     closes. A player who rebinds `PrimaryWeapon` to another key still
     picks entry 1 with key 1 (the fixed binding).

2. **Within one command the first matching alias counts, in the order the
   string lists them** (T1; our deviation, and test cases 24 and 26).
   - Reference: T1's rule as written: for each command in list order, its
     aliases are tried in the order they appear in its string, and the
     first one the typed line starts with (case-insensitively) is taken;
     a later command that matches replaces it. A shorter alias listed
     before a longer one therefore always wins: with `/w/whisper`, the line
     `/whisper Bob hi there` matches `/w`, the remainder is
     `hisper Bob hi there`, and the whisper goes to "hisper" with the body
     "Bob hi there".
   - Ours: `game/chat/text.rs:111-123` takes the **longest** matching alias
     of each command (documented as a deliberate deviation at
     `game/chat.rs:39-41`, because test case 24 contradicted T1).
   - Expect: test case 24 is wrong as written. With `ChatCMD_Whisper =
     /w/whisper`: `/whisper Bob hi there` → whisper to "hisper", body "Bob
     hi there", echo `<to-label> "hisper" : Bob hi there`. With
     `ChatCMD_Whisper = /whisper/w`: `/whisper Bob hi there` → whisper to
     "Bob" with body "hi there", and `/wave` → matches `/w` (test 26's
     outcome). Implement the first-match rule; the stock aliases' order
     (Open question 2) then decides what players see.

3. **A setting command followed only by spaces is sent as chat** (T2).
   - Reference: when the remainder after a Block, UnBlock or
     WhisperAutoAnswer alias is longer than one character but holds no
     word (only spaces), the command is **not** treated as a setting: it
     falls through to C3 step 4's "anything else" route, and the **whole
     typed line** (alias and spaces included) is sent on the current tab.
     A Whisper alias followed only by spaces sends nothing (no target name)
     and closes the input, as now. A remainder of at most one character
     keeps T2's first column (help text, reset).
   - Ours: `game/chat/client.rs:320-330` (and the UnBlock and AutoAnswer
     arms below it) treats any remainder longer than one character as a
     setting: Block and UnBlock with an empty name, and AutoAnswer stores
     the spaces as the auto-reply; nothing is sent.
   - Expect: on the All tab, `/block` followed by three spaces → every
     receiver shows `A1 : /block   ` (with `ChatCMD_Block = /block`); the
     block list is unchanged; no reply line. The same with the auto-reply
     alias → sent as chat, the auto-reply unchanged.

Could not determine here: the stock alias strings (still Open question 2),
so whether correction 2 changes anything a player of the Steam build would
type.
