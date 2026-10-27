# Parity audit, 2026-10-27: specs vs code

A read-only pass over specs/ (CS:S, Source, Combat Arms) against the
code, looking for behaviour the originals have that we don't, which
neither docs/backlog.md nor docs/tech-debt.md (with the module `//!`
docs it is generated from) already lists. The top items are in the
backlog's "Gaps vs the originals (audit 2026-10-27)"; this file keeps the
method and what was checked and found matching. Delete it once the
backlog's items are done.

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
  movement, body, animation and HUD; nothing else at the time: the
  games' per-character sounds have followed it since).

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

High and medium: the backlog section (nine items; the two sound items and
the cross-game hit tests and shared armour ones done 2026-10-27:
[DESIGN.md "Hits across games"](../../DESIGN.md#hits-across-games)). The
low items (grenade water sounds, Combat Arms' 75 degree world FOV,
`sv_footsteps`, `env_entity_igniter`) and the two misdescribing `//!` docs
are done.
