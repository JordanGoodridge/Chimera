# Parity audit, 2026-10-27: specs vs code

A read-only pass over specs/ (CS:S, Source, Combat Arms) against the
code, looking for behaviour the originals have that we don't, which
neither docs/backlog.md nor docs/tech-debt.md (with the module `//!`
docs it is generated from) already lists. The top items are in the
backlog's "Gaps vs the originals (audit 2026-10-27)"; this file keeps the
method, what was checked and found matching, the low items and two docs
that say something the code doesn't do. Delete it once its items are
done or moved.

## Method

- Read each spec's section headings, then checked the player-facing
  sections against the code: distinctive names (sound entries, cvars,
  entity classes, constants) grepped across the workspace, then the
  module that implements the section, its `//!` doc and its Known
  shortcuts.
- Compared what each app composes (plugins) with what the plain game
  composes, since the party apps (apps/party_kit: Lucker Party, the
  Ultimate Gun Game) take each game's parts piecemeal.
- Checked identity gating: which systems of one game run on characters
  whose `GameIdentity` is the other's (`chimera_sim::identity` switches
  movement, body, animation and HUD; nothing else).

## Checked and matching (no unlisted gap)

CS:S: movement (landing roll, wall slam 1160/580, stuck nudges, jump
stamina and the bunny-hop cap, punch decay, ladders, water, drowning),
weapons (zoom speeds and steps, burst and silencer timings, empty
sounds, penetration, falloff, armour, hitgroups, refused hits), sounds
(footstep rules, wade, swim, pain sounds), view models (bob, sway,
handedness), impact effects (blood particles and wall stains behind a
hit), tracers, kill feed headshot icon. Parity left there is already in
plans/active/parity-gaps.md and the tech-debt rows.

Combat Arms: movement (volumes, ladders, crouch spam, stamina, fall
damage), inventory (no quick-knife, no drop key, death drop, pickup
rules), firing and spread (also with another game's movement: the run
flag and speed limit fall back sensibly), damage-taken feedback and the
heartbeat, name tags, the scope range readout, effects and sounds by
event, view models. The 2026-10-22 corrections listed in the specs are
in the code.

## All gaps found

High and medium: the backlog section (nine items; the cross-game hit
tests one done 2026-10-27, [generic-hits.md](generic-hits.md)). Low:

- **Grenade water splash sounds (CS:S, L, S).** grenades.md 4 step 8:
  entering water plays `BaseEntity.EnterWater`, leaving
  `BaseEntity.ExitWater`; `weapon::grenade` halves the speed but plays
  neither (no code names them).
- **Combat Arms' world FOV (L, S).** view_models.md 5.2: the world is 75
  degrees vertical (91.31 horizontal at 4:3). The Combat Arms app keeps
  the shell's `fov_desired` 90 (73.74 degrees vertical), and the view
  model's FOV follows it down (`view_model_fov`: 63.74 - (91.31 - 90) =
  62.43, about 49 degrees vertical instead of 50). Small, but every frame.
- **`sv_footsteps` (CS:S, L, S).** sounds.md 3: 0 silences steps, jump
  and landing sounds in multiplayer; no such cvar (community servers and
  map command lines set it).
- **env_entity_igniter (Source, L, S).** fire.md 4.2 specs it (ignite
  every match of `target` for `lifetime`); not handled. No stock map
  has one.

## Docs that misdescribe the code

- games/combat_arms/src/game/view_model.rs, Known shortcuts: "The world
  camera's FOV is the shell's (5.2: 75 degrees vertical)": the shell's is
  `fov_desired` 90 at 4:3, 73.74 vertical (the low item above).
- games/combat_arms/src/game/effects/mod.rs, Known shortcuts: "Others'
  footstep animation rate is 1 (no third-person animations yet:
  characters.md)": the stock bodies and their animations load now
  (`characters::models`); the rate stays 1 by the spec (characters.md 7),
  not for lack of animations.
