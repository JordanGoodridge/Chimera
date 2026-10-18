# Counter-Strike: Source: spectating after death (measured)

Source basis: no source read. Measured from the game: the CS:S dedicated
server's cvar defaults (`help`/`cvarlist`, 2026-10-15), and a SourceTV demo
recorded on that server with six bots playing three rounds of de_dust2
(`mp_freezetime 3`, everything else default; 150 s, 66.67 ticks/s, a snapshot
every 4 ticks, so times below are good to about 0.06 s). The demo holds only
bots: what a human player sees (the freeze cam) is not in it.
Status: implemented (timings in `client::spectate`, `client::freeze_cam`)

## Summary
A killed player first shows the death cam, then (humans, killed by someone
still alive) the freeze cam, then watches a living teammate. The server owns
the timings (`spec_freeze_time`, `spec_freeze_traveltime` are replicated
server cvars), and the dead player's observer state is networked as
`m_iObserverMode` and `m_hObserverTarget` on its player entity.

## Units and conventions
Seconds; ticks at 66.67 per second (0.015 s). Observer modes as the game
numbers them (its `spec_mode` takes the last three): 0 none (alive),
1 death cam, 2 freeze cam, 3 fixed, 4 first person (in eye), 5 chase,
6 free look (roaming).

## Constants
| Name (ours) | Value | Unit | Meaning |
|---|---|---|---|
| `spec_freeze_time` | 5.0 | s | how long the freeze cam's picture holds (server cvar, game cheat, replicated) |
| `spec_freeze_traveltime` | 0.7 | s | how long the freeze cam takes to reach the killer (minimum 0.01; server cvar, cheat, replicated) |
| `mp_forcecamera` | 1 | | the dead watch only their own team |
| `mp_fadetoblack` | 0 | | the dead's view is not blacked out |
| `DEATH_CAM_ALONE` | 6.5 | s | death to watching a teammate when no freeze cam follows (bots: 432 to 436 ticks, 6.48 to 6.54 s, six deaths) |
| `TARGET_DEATH_HOLD` | 3.0 | s | a watched player's death to the watcher moving on (200 ticks, two cases) |

## Behavior

### Dying
On the tick of `player_death` the victim's `m_lifeState` goes to 1 (dying),
`pl.deadflag` to 1 and `m_iObserverMode` to 1 (death cam).
`m_hObserverTarget` stays unset (the killer is not networked as the death
cam's target).

### Death cam to watching (bots)
432 to 436 ticks (6.48 to 6.54 s) after the death, in every one of the six
bot deaths that lived that long, on the same snapshot: `m_lifeState` 2
(dead), `m_iObserverMode` 4 (first person) and `m_hObserverTarget` a living
teammate. Bots never showed mode 2 (no freeze cam for bots). In all six the
killer was still alive.

### The first target
Not the nearest teammate: one bot, dead at (-461, -760), was put on the
teammate 3350 units away rather than the one 2090 units away. In all three
cases with a choice the target was the living teammate with the lowest
entity index, which is also the next index after the dead player's own
(cyclically); the demo can't tell those apart. **Measured 2026-10-18**
(the probe server, four CT bots of entity indices 1, 3, 4, 5 and a T,
`mp_ignore_round_win_conditions 1`, a bot's `kill`; the dead observer's
`m_iObserverMode` read on the server, its target from its origin, which
the server keeps at the watched player's): the dead 4 watched 5, not 1:
the next index after its own. When 5 died, 4 moved on to 1 (the next
after 5, wrapping).

### When the watched player dies
3.0 s (200 ticks) after the watched player's death, its watchers changed to
mode 3 (fixed) with no target. Both cases were the last living player of the
watchers' team dying, which also ended the round; the other team's living
players were not taken as targets (`mp_forcecamera 1`). **Measured
2026-10-18** with no round end (`mp_ignore_round_win_conditions 1`): the
same mode 3 three seconds after the last teammate died, for every dead
teammate (also one still in its death cam once that ended), the camera
held at the last watched player's eye height over where it died: mode 3
comes from having nobody to watch, not from the round end.

### A new round
At `round_start` every player goes back to mode 0 and life state 0 on the
same tick; a player still in the death cam then (died 5.1 s before) never
reached watching.

## Test cases
| Setup | Input | After | Expected |
|---|---|---|---|
| defaults | killed, no freeze cam (killer dead or `cl_disablefreezecam 1`) | 6.4 s | still the death cam |
| defaults | same | 6.6 s | watching the first living teammate |
| defaults, freeze cam | killed by a living enemy | death cam, then 0.7 s travel | the picture holds 5.0 s |
| two living teammates, the far one first in order | killed | death cam over | watching the first in order, not the nearest |
| watching a teammate | it dies | 2.9 s | still on it |
| same | | 3.1 s | moved on |

## Open questions
- Human players' freeze cam: when it starts after the death (ours: after a
  2 s death cam, a guess), whether the travel and hold add to the 6.5 s or
  fit inside it, and the observer mode after it. Needs a client demo
  (`record`) of a human killed by a bot, or a capture.
- Whether a human without the freeze cam (`cl_disablefreezecam 1`, or the
  killer dead) waits the bots' 6.5 s. Ours assumes so.
- Answered 2026-10-18 (see "When the watched player dies"): mode 3 comes
  from nobody left to watch, the camera at the last watched player's eye
  where it died. Ours holds the camera where it was (`SpecMode::Fixed`).
- Answered 2026-10-18 (see "The first target"): the next index after the
  observer's own. Ours: the first in our cycling order, by name (bots join
  in name order; a listen server's host is index 1, where the two agree).
- The 6.5 s with a freeze cam disabled server-wide, and bots' mode 4 versus a
  human's remembered last mode.
