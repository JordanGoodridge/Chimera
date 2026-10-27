# Backlog

Things to build, **in priority order** (top first; reprioritized
2026-10-06). Shortcuts already in the code live in
[tech-debt.md](tech-debt.md); the MVP plan is in
[plans/active/](plans/active/engine-split.md). Move items into a
plan when work starts; delete them when done.

## Architecture review (2026-10-20)

Address the feedback in
[plans/active/architecture-review.md](plans/active/architecture-review.md),
following its phased roadmap ("Now" N1–N7, "Next" X1–X10, "Later"
L1–L6) in that order; each item names its files (lanes), size and
guard tests, so items in one phase can go to parallel agents. Mark items
done in the review as they land. N5 (CI on GitHub Actions) is deferred (2026-10-24, the user's choice); it waits for
the user's go-ahead: no Actions runs unless asked.

- After the Bevy 0.20 upgrade (L5): move the menus from Bevy UI's
  deprecated `Interaction`/`Button` (`client::widgets::Interaction`) to
  picking's `Hovered` and `ui::Pressed` before Bevy removes them; the
  menus act on a press as the mouse goes down, so each one's input needs
  checking (docs/tech-debt.md). And `bevy_replicon_renet` back to a
  crates.io release once one supports Bevy 0.20.

## 0. Playtest feedback (2026-10-07), top priority

### Gun game playtest, 2026-10-25 (user; start after the usage reset)

- **Combat Arms character models and held guns: fixed, confirm in a
  playtest.** Heads looked thin and the head and backpack vanished up
  close: the body parts were culled by their bind pose's box, and the
  female bodies are bound standing on their origin, half a body above
  where the animation draws them, so the hair, backpack and upper body
  dropped out while still on screen (leaving the narrow face mask); now
  each part's bounds follow its joints (`render::bodies`,
  `body_bounds::body_parts_are_culled_by_their_posed_bounds`). The
  third-person guns were drawn in their bind pose; each `HHModel`'s own
  `Hand` animation turns it half a turn, so they pointed backwards; now
  they are drawn in that pose (`heavy::characters::
  held_weapons_point_forward`; one level per piece:
  `bodies_draw_one_level_per_piece`).
- **Clients don't see glass break properly: fixed, confirm online.** A
  broken window's panes (func_breakable_surf) stayed whole on clients and
  no shards, gibs or falling panes reached them (only a func_breakable's
  removal and the break sound did); now the panes replicate and what
  breaking throws is sent (`net::breakables`; guarded by
  `cs_source net_breakables`).
- **Combat Arms full-auto fire clips (painfully loud): fixed, confirm by
  ear.** The fire sound was already one voice per shooter (its fire
  channel); the pile was every round's impact and casing sound, a new
  voice each at almost full gain up close (Combat Arms' falloff is nearly
  flat near the source): 18 voices at once in 3 s of M4A1 fire with
  second-long stand-ins, where CS:S plays no casing sounds and its
  impacts fall off faster. Now each gun's impacts and casings replace
  their previous one (`effects::sounds::CHANNEL_IMPACT`/`CHANNEL_SHELL`),
  and Chimera's playback limits voices per source (8) and per sourceless
  entry (4) (`fire_voices::full_auto_keeps_a_few_voices`). Check in a
  playtest; if it still peaks, a limiter on the mix is next.
- **Combat Arms grenades: fixed, confirm in a playtest; pin and throw
  sounds need a spec answer** (user, 2026-10-27). The flight already
  matched grenades.md (arc, bounce, rest, fuse: `grenades::t01`-`t49`)
  but was drawn at the server's 20 frames a second, hopping 50 ms at a
  time; now it is drawn on its path between frames
  (`grenades::draw_between_frames`;
  `thrown_frag_flies_the_spec_arc_and_is_drawn_between_frames`). In
  flight it was the held model at its own size (plus a placeholder
  ball); now it is the projectile record's model (`FX.TXT`
  `ProjectileFX#`: `Model`, `Skin0`, `ModelScale` 2, `World` pose), and
  the record's `Velocity`, `Lifetime`, `DropVelocity`, `FireOffset` and
  `CanHitSameKind` reach the grenade hook (`data::grenade_values`;
  `attribute_data::grenade_projectile_records_from_the_install`,
  `heavy::characters::thrown_grenades_draw_their_projectile_model`). The
  view model's pin pull, throw and release already played
  (`view_model::a_throw_plays_pin_pull_throw_and_release`), and the
  bounce and blast sounds already fire. Left: the pin pull's view-model
  key is `sound_key 10` (misc sound 1, view_models.md 8), but no spec
  names the field misc sounds read and the install's grenade records
  have none (nor `FireSnd`/`PreFireSnd`), so the pin and throw are
  silent; a spec session should say which file (if any) plays there
  (the M67's sound folder has `reload1.wav`, `fire.wav`,
  `firetoss.wav`, which nothing names).

### Gaps vs the originals (audit 2026-10-27)

specs/ compared with the code, spec by spec. Plain CS:S and Combat Arms
match their specs wherever the docs don't already say otherwise; the
unlisted gaps are where the party apps (Lucker Party's gun game, the
Ultimate Gun Game: `apps/party_kit`) put the two games together. Most
noticeable first (impact H/M/L, size S/M/L). The rest, the method and
two misdescribed docs:
[plans/active/parity-audit-2026-10.md](plans/active/parity-audit-2026-10.md).

- **No CS:S hit slowdown or flinch in the party apps (H, S).** CS:S
  slows a hit player (0.5/0.65, back over 1/3 s) and kicks its view
  (weapons.md M18); `PartyPartsPlugin` (apps/party_kit/src/lib.rs) adds
  `CsGunsPlugin` but not `hit_reaction::HitReactionPlugin` (only
  `CsWeaponsPlugin` does), so CS:S-gun fights have no tagging.
- **Armour isn't shared across games (M, M).** Every character gets
  Combat Arms' `CaArmor` (100 points, games/combat_arms/src/game/weapons.rs),
  which CS:S guns ignore (they read `weapon::Armor`, weapons.md M7): a
  CA-gun player's HUD shows armour CS:S bullets go through, a CS:S-gun
  player (CS:S HUD: 0) carries unseen armour against CA guns. Plan:
  [plans/active/generic-hits.md](plans/active/generic-hits.md): the
  shared armour step (`weapon::hits::absorb`) and CS:S's kevlar on it are
  in; left are Combat Arms' hits through it and `CaArmor` becoming
  `weapon::Armor` (steps d3, d4), then the cross-game armour tests.
- **Water and ladders across movements (M, M).** CA movement swims only
  in its liquid volumes (movement.md "Volumes"); `volumes.rs` makes
  Source ladder brushes `Ladder` volumes but not Source water
  (`MapWater`, which CA grenades already treat as Water), so a CA-gun
  player walks the bottom of a CS:S map's water (the UGG pool). On CA
  maps CS:S movement has neither water nor ladders (CA's are volumes).
- **CS:S HUD pieces missing in the party apps (M, S).** With a CS:S gun
  the CS:S HUD is up, but `party_kit::client` adds only `GameHudPlugin`,
  `HudSpritesPlugin`, `WeaponSelectPlugin` and the minigame panel: no
  target ID (`ui::target_id`), radar, freeze cam or voice HUD (all in
  `CsSourceUiPlugin`); CA's name tags are its HUD's, so a CS:S-gun player
  sees no names at all.
- **No prop collision or scrape sounds in the party apps (M-L, S).**
  CS:S's `impacts::ImpactSoundsPlugin` (sounds.md 4) comes only with
  `CsWeaponsPlugin`; `PartyPartsPlugin` adds `BulletImpactsPlugin`
  alone, so props knocked about on CS:S maps are silent.
- **CS:S kill-feed icons for CA guns (L-M, S).** `ui::game_hud` looks an
  icon up as `d_<last part of the id>`: `combat_arms:m4a1` shows CS:S's
  M4A1 icon, the other CA guns none.

### Decisions 2026-10-13 (user)

- Refactor: go ahead with engine-split.md "Crates and compile time"
  steps 1-2 (tools package; move code to its future home in the one
  crate), then crates.
- Default binds follow CS:S's config_default.cfg (F1 autobuy, F2 rebuy,
  mouse wheel invprev/invnext, ...): move our debug keys (inspector, debug
  UI, wheel jump) off them. Done 2026-10-13: debug UI F8, inspector F11
  (`chimera_inspector`), collision F3 (`toggle chimera_drawcollision`; F3
  is free in CS:S); saved configs keep their binds.
- `con_enable` stays 1 (deliberate difference, ui-parity.md).
- SourceMod/Mani chat commands from maps stay ignored, as plain CS:S does.
  Later: consider making SourceMod plugins work on our engine (parity on
  community servers, good for users) — an investigation first.
- Networking: investigate whether Steam networking (relays/NAT traversal)
  is possible at all for our own client (expected not: it needs our own
  Steam app id; CS:S's can't be used), prototype if cheap; server size:
  as many players as we can make performant; stress-test it (parity
  first: 32 like CS:S as the first target). Done 2026-10-13: not
  possible without our own Steam app id; UPnP prototyped (`net_upnp`),
  recommendation in docs/plans/active/nat-traversal.md; 32 players fit
  the tick after a capacity pass (multiplayer.md "Capacity"), follow-ups
  in docs/tech-debt.md.
- No new release until the user says the group is testing again.
- Dark wall panels by "MAIN": skipped (map unknown). Skybox inside a
  player: waiting for repro details from the user.

### Playtest with friends, 2026-10-10 (Discord; build v0.1.0-playtest2 or 1)

- Water seen through other parts of the map (user, 2026-10-11,
  mg_jacks_multigames_v1, `setpos -12146.96 15343.41 -927.97; setang
  -29.57 184.04 0`): fixed 2026-10-13, to playtest. It was the flood's
  water (a func_water_analog, drawn under its mover's node) seen through
  the sky from another room: moving water had no visibility clusters, so
  it was never culled. It is now tagged by its moved bounds as brush
  entities are (`heavy::map_community::moving_water_is_culled_on_jacks`).
  Depth and pass order were fine.

- Done 2026-10-12: no weapon held draws no view model and the HUD's
  `crosshair_default` (five dots; `client::hud::crosshair_kind`).
  Check in a playtest that the T-posed hands are gone after a jacks
  strip (the single-player strip test shows no view model).

- Moving under a brush at head height (user, 2026-10-11,
  mg_jacks_multigames_v1 by the climb start: lamps on the pillar whose
  undersides are 64.7 units above the floor, over a 62-unit box with the
  eye at 64): fixed 2026-10-13, to playtest. The eye was kept under a
  ceiling found by a ray straight up from the box's centre, so a lamp
  over the box's top but not over the eye let the near plane cut into it;
  now a flat box as wide as the player's (15 units each side) is swept up
  (`client::view::below_ceiling`; `console_commands::eye_drops_under_a_brush_edge_over_the_box`).
  The movement spec doesn't cover the view against ceilings: our reading
  of the report. Capture to confirm: in CS:S, `setpos -12348.55 14205
  1408.03; setang 0 268.82 0` (feet on the floor, the box's top under the
  lamp's edge, the eye just short of it), `getpos` and a screenshot: does
  the view drop (by how much), or does CS:S do something else (duck)? The
  setpos in the report is a `getpos` line: it prints the eye, and setpos
  takes the feet (both games), so it lands 64 units higher, on top of
  the lamp.

- mg_jacks_multigames_v1 in real CS:S (user, 2026-10-11): done
  2026-10-13, to playtest (multiplayer.md, "mg_jacks_multigames_v1 in a
  network game"). (1) Each game's selection sound (`lj_sound` PlaySound,
  an ambient_generic) played only on the host: map logic's sounds now
  reach clients. (2) Its `say` lines ("<== Spawn: Surf == >", "[==> Map
  by Jackmaster <==]") show in chat, under the host's name on a listen
  server, "Console" on a dedicated one. Capture to confirm: on a CS:S
  listen server with the host dead, do living players read the map's
  lines (ours follow the host's dead-chat rule)? (3) `sv_airaccelerate
  750` (climb and surf spawn modes) is allowed and applied; ours puts it
  back when the map unloads (our rule; CS:S keeps it until a config sets
  it again). (4) Strips: StartTouch fires for anyone spawned or
  teleported into a trigger, and bots are stripped as players are
  (`heavy::map_flows::jacks_strips_everyone_between_games`); the
  "Normal" spawn mode goes to the lobby with no strip (the map's design,
  so survivors keep their guns there, in CS:S too).

- Done 2026-10-12: alt-tab back in grabs the cursor at once
  (`client::input::grab_cursor`).

- Weapons after a round on minigame maps (user, 2026-10-11): players walk
  into the lobby with e.g. a machine gun. In CS:S survivors keep their
  weapons into the next round; minigame maps strip them with map logic
  (`player_weaponstrip` Strip/StripWeaponsAndSuit or `game_player_equip`
  with the strip flag, fired by a trigger in the spawn/lobby or a
  logic_auto at round start). jacks has 4 player_weaponstrips fired by
  triggers' OnStartTouch. Checked on jacks 2026-10-13: a player spawning
  or teleported inside a trigger gets StartTouch, bots too, and the
  strip happens; jacks strips at the slide course's end teleports
  (`stripper_slide`) and on arrival in some arenas (`strippi`,
  `stripzomb`, `stripper_bw`), not in its lobby, and its "Normal" spawn
  mode goes to the lobby without a strip (so survivors walk in armed
  there, in CS:S too). Checked 2026-10-15 on the other maps
  (community-maps.md, round 6): all 29 strip triggers on the 7 mg_ maps
  with strips fire (`heavy::map_flows::every_strip_trigger_strips`);
  mg_lt_galaxy_v5's spawn hall strips survivors at each round start,
  swag strips on the drop from its spawn room; the other 10 mg_ maps have
  no strip, so survivors keep their guns there as in CS:S. Left: compare
  galaxy's hall with CS:S on the reference server.

- AK full-auto sounds choppy (user, 2026-10-11): fixed 2026-10-13, to
  playtest. `Weapon_AK47.Single` is one wave on CHAN_WEAPON at
  PITCH_NORM (CompatibilityAttenuation 0.52), so each shot replacing the
  last is parity; ours stopped the old one on whatever sample it was at
  (a click every shot). A replaced (or stopped) sound now fades out over
  5 ms (`audio::live_sound::fade_out_ramps_to_silence_then_ends`); the own
  sound plays centred through the same path. Capture to confirm: a CS:S
  recording of a full-auto AK burst against ours.

Connections and hosting: done 2026-10-10, to playtest (multiplayer.md,
"Playtest with friends"): Create Server's Game tab lists `settings.scr`'s
options on a fresh config (they were read when the main menu opened,
before the install's GameUI files came in); `net_maxfilesize` 512 on
both ends (150 MB map: `heavy::net_big_map`); a stopped server hangs
up on its clients (three copies of netcode's packet, Ctrl+C on the
dedicated server too) and they show "Server shutting down"; chat says
who joined and left and why. Left: whether the playtest's hang was a
lost packet, a killed process or something else (not reproduced here:
every stop path disconnected at once over loopback).

Gameplay and rules (done 2026-10-10, to playtest: a client's cheats run
on the server with its `sv_cheats`, noclip predicted; `chimera_noblock`
1/2, off by default; chat lines once in the console; `name` saved, a
"Player name" entry on Options > Multiplayer):
- "Gun does not reload after round end" (reporter THE GLITCH; fixed
  2026-10-10, to playtest): reproduced: R in the next round's freeze time
  did nothing (the freeze dropped every key but the look and weapon
  picks, `rules::rounds::hold_frozen`, and a network client the same,
  `net::predict::hold_local`); now reload and lastinv go through, moving
  and attacking stay held (`rounds::survivors_keep_their_clip_and_reload_in_the_freeze`).
  The clip isn't refilled at a round start: survivors keep their weapons
  and ammo as they are (parity, unchanged). Confirmed on the probe server
  2026-10-15: CS:S reloads in the freeze time at once, at the usual pace.

Looks:
- Third-person player models "not rigged right" (fixed 2026-10-10, to
  playtest): the gun hangs on the spine's animated `weapon_bone` (right)
  while the aim layers turn the arms with the look, so aiming up or down
  took the hands off the gun (a pistol's grip 4.8 units from the hand
  looking 40 degrees down). The player models carry IK chains (`rhand`,
  `lhand`) and per-animation IK rules (read from the files: hand poses
  lock the hands to the weapon, the knife's release them, dual pistols
  lock the right hand only, reloads lock to the grips); the hands now
  follow them onto `weapon_bone_RHand`/`_LHand` (where every weapon's
  authored pose has them, within 2 units and 9 degrees; not the
  grenades', whose world model hangs on the hand) with a two-bone IK
  (`map::anim::AnimSet::solve_ik`, `player_anim::lock_hands`;
  `chimera_hand_ik 0` turns it off to compare;
  `animation::hands_stay_on_the_weapon_while_aiming`). World models
  bone-merge: the dual pistols' left gun follows the left hand. Capture
  to confirm: CS:S third person (`thirdperson; cam_idealyaw 130`) with a
  pistol looking 30 degrees up and down, and crouched.
- Other players' weapon draw ("pull out") animation looks wrong in third
  person. Fixed 2026-10-10 (to playtest) what we found: a reload or shot
  gesture went on after drawing another weapon, played on the new
  weapon's arms (a reload up to 2 s); now a weapon change stops them
  (the grenade's throw still plays out;
  `animation::drawing_another_weapon_stops_the_old_ones_gestures`). The
  player models have no draw sequence (spec §14): the new weapon's upper
  body crosses over in its sequences' fade, as before. Capture to
  confirm: in CS:S, a bot switching rifle to pistol (and mid-reload) in
  third person, a few frames over the first half second; if CS:S shows a
  gesture we lack, it needs a spec.
- Standing inside another player (user: seen on mg_jacks_multigames_v1): the screen shows only the skybox, no
  world. Not reproduced on jacks either (2026-10-13): inside a bot
  standing, with `chimera_noblock` 0 and 1, moving and jumping in it; and
  every game's team teleports onto their shared destinations, players
  piled on one point, all eyes stay in the map
  (`heavy::map_flows::jacks_every_game_keeps_players_in_the_map`). The
  flood water (above) drawn over the sky looked like part of this from
  some spots. Not reproduced (2026-10-10, de_dust2): inside a bot standing and
  in noclip, and over the network (host `setpos` onto a client, the client
  walking, ducking and jumping inside it, both screens), the world draws
  and the body's insides show, as in CS:S; first-person spectating too.
  What the report describes matches the camera outside the map (the 3D
  skybox over black: what a `setpos` into the void shows), so something
  moved the player or camera out. Needed: the reporter's F9 bug report
  (`setpos` line, map) or the steps (map, team, spawn, noblock?).
- Crosshair off centre after Options > Multiplayer > Advanced: fixed
  2026-10-10, to playtest. On a scaled display (Windows at 125 %) the
  camera kept the in-game dialog picture's size (physical pixels) after
  going back to the window; the crosshair now uses the UI's size, and
  the camera's is worked out again (`--scale 1.25` reproduces it).
- de_dust2 (same screenshot, near "MAIN" on the wall by long/mid): large
  dark square panels on the wall. The stock CS:S de_dust2 has no "MAIN"
  graffiti and no grey stone there (its overlays: graffiti01-03 are other
  texts; checked against the install's BSP): the screenshot's map is
  another one (a remake or a multigame map's dust2 room). Found and fixed
  meanwhile (2026-10-10): DecalModulate decals and overlays (blood, dirt,
  scratches) had no texture, so their infodecals were never placed and
  their overlays drew untextured; now they multiply the surface 2x as in
  the game (`MapMesh::modulate`, world.wesl;
  `heavy::map_community::modulate_decals_multiply_the_surface`,
  gg_bk_warehouse_v1's blood). Needed: the map's name and a `setpos`
  there (F9 report) to identify the panels.

- Done 2026-10-09, checked against the coordinator's CS:S captures the
  same day: (1) other rooms showing through the sky
  (mg_creative_multigames_v8_ns): placed weapons, water surfaces (one
  mesh per material across rooms), beams/glows and moving props weren't
  PVS-culled; now they are (`vis`). Confirmed: CS:S at `setpos -6332.74
  -3047.50 -9638.97; setang -38.85 18.60 0` shows the same sky view as
  ours (nothing in the sky). Open: whether CS:S also hides things *in* the
  PVS behind a sky face; our `chimera_skyocclude 1` does
  (`render::sky_occluder`), off by default. Capture: a prop in view behind a
  sky face from a leaf whose PVS holds it. (2) The 2D sky's `dn` face
  turned half a turn (`cs_source::sky::FACES`). Not captured yet: CS:S
  looking straight down from noclip over a map with a pictured bottom
  (gg_lego_spacetower2's down-under sky), `refcmp skyconv` view. (3) The
  knife view model: confirmed (de_dust2, `setpos -295 1078 120; setang 0
  0 0`, both hands): the same blade (tan camouflage, serrated back edge),
  hands and placement. Its plain silver look in our earlier shot was the
  CT spawn's shade, not the model. Its idle (12.5 s, 151 frames) turns the
  blade sideways for a while; CS:S's two captures happened to be upright.
  Capture to confirm: the knife held 5-10 s after drawing it. (4) The 3D
  skybox's fog (sky_camera fog start/end divided by its scale, max
  density): confirmed, the distant hills' haze at `setpos -295 1078 500;
  setang 3 0 0` (noclip) matches within a few levels of 255; the
  start/end scale stays. Not `fogblend` (mg_kommando, cs_compound and
  de_port set it; other-maps.md). (5)
  Keyboard > Advanced (fast weapon switch, developer console =
  `con_enable`, ours defaults to 1, CS:S 0): not captured.

- Knife idle (done 2026-10-09, to playtest): the knife holds the draw's
  last frame and idles only `KNIFE_IDLE` (20 s, a guess) after its draw
  or last attack (its only idle, 12.5 s, is still for its first 3.4 s,
  then turns the blade); non-looping idles repeat after the script's
  `IdleInterval` (the TMP's 5 s sway, the only other idle that moves:
  `dump --sequences` prints each idle's motion). Capture to confirm: the
  knife 15, 20, 25 and 30 s after drawing it; the TMP 1-10 s after.
- cl_autowepswitch, the target ID (`hud_centerid`, `mp_playerid`) and
  the freeze cam (`cl_disablefreezecam`) (done 2026-10-09, to playtest):
  Multiplayer > Advanced's three check boxes work. Captures to confirm
  are listed in docs/tech-debt.md's rows for them.

- mg_item_battle_v4b's items (done 2026-10-09, to playtest): a picked-up
  knife's parented entities follow its carrier (`map::entities` anchors),
  OnPlayerPickup fires, and the entities the items drive work (game_ui,
  player_speedmod, point_template/env_entity_maker, func_physbox,
  phys_thruster/keepupright; docs/plans/active/community-maps.md, "Map
  entities"). The carried item sits at the middle of the carrier's box
  (31 units above the feet standing) turned to its yaw: CS:S's captures
  of the car knife (first person at `setang 30 0 0`, third person) match
  it exactly (the car on the floor, roof at the shoulders); at the feet
  the car sank to the knees. Left: playtest the cars and jetpack.
- Inside geometry, checked against CS:S (2026-10-09, mg_item_battle_v4b,
  reference captures in the coordinator's target/inside/): inside the stone
  wall ours already matches (world drawn, sky black). Inside the car
  (`setpos 2241.89 1311.06 79.39; setang 4.92 181.70 0`) CS:S shows the
  world through the car with only the hood's inside in view; ours showed
  the car's insides (wheels, chassis) and no world behind. Retake: our
  `setpos` put the eye 36 units lower than CS:S's then (below).
- `setpos`/`getpos` follow CS:S (2026-10-09, reference captures): `setpos`
  takes the feet (Source's origin; CS:S's camera was 64 above the z
  given, standing and in noclip, on two captures) and `getpos` prints the
  view's origin (the eye; in third person the camera: CS:S's third-person
  `getpos` was exactly its camera). So one `setpos` line gives the same
  view in both, and `setpos` with `getpos`'s numbers lands 64 higher, as
  in CS:S. Ours used the body's centre (36 above the feet) for both.
  Confirmed in CS:S's console: `noclip; setpos -295 1078 500; getpos`
  prints z 564.
- Options faithful to CS:S (done 2026-10-09, to playtest): the Voice
  tab, Multiplayer > Advanced from `cfg/user.scr`, Create Server's
  `< Random Map >`, the aspect ratio, and the cvars behind the greyed
  controls with CS:S's defaults (`fov_desired`, the crosshair's size,
  thickness, dot and custom colour, `snd_musicvolume`,
  `snd_mute_losefocus`, `m_filter`, `m_customaccel`, `cl_radar_locked`,
  `cl_downloadfilter`, water detail, `mp_decals`, `cl_c4progressbar`).
  What stays greyed and why, the deliberate differences (`con_enable`,
  `volume`) and the reference captures still needed (slider ranges,
  the colour list, `cl_crosshairusealpha`'s and `mp_decals`' defaults):
  docs/plans/active/ui-parity.md.

- Video > Advanced's and the Audio tab's remaining settings (shader
  detail, colour correction, motion blur; shadow and model detail done
  2026-10-10, characters' render-to-texture shadows left; speakers,
  sound quality and captions beyond being saved): one by one, per
  docs/plans/active/video-settings.md.
- Ladders (fixed 2026-10-10): standing on a ladder brush's top grabbed
  it as a ladder (its upward face carries the ladder flag): walking
  forward looking down stopped and a jump added the jump-off push. The
  probe now ignores ladder faces 0.7 or more vertical (our choice, the
  spec is silent; 36 stock ladder tops before, 0 now:
  `heavy::map_ladders`). Silent ladders: none found in the sim (every
  stock nav ladder climbed for more than a step period steps); a short
  climb right after a ground step hears none (the shared step timer,
  per the sounds spec), and holding walk or duck silences ladder steps
  (our CS:S walk rule). Confirmed in CS:S (de_train, feet on the ladder
  brush's top at (1607, -114, -136), pitch 30 down): +forward walks off
  the top (220 units in a second), a jump rises 52 units in 0.25 s (a
  normal 302 jump), as ours now. Left: a capture of a
  de_train car ladder's steps (docs/tech-debt.md, movement); walked
  straight at from the nav foot, 37 of de_train's 41 ladders don't
  attach in our sim (a clip face flush with them wins the probe; bots
  sidestep), worth comparing with CS:S.
- `kill` (fixed 2026-10-10): it moved you to a spawn alive; it is now
  CS:S's suicide, so a lone player can end a round. A network client's
  `kill` is a request to the server (2026-10-24, `net::cheats`,
  `PLAYER_COMMANDS`, no `sv_cheats` needed;
  `net_players::a_clients_kill_is_a_request_to_the_server`).

- Spectator mode (done 2026-10-10): CS:S's spectator team (team menu's
  Spectate, `spectate`, `jointeam 1`, `mp_allowspectators`): no body,
  free look first, chase and first person on anyone, `spec_next`/
  `spec_prev`/`spec_mode`/`spec_player`, duck opens our spectator menu
  (players, camera, join a team), the scoreboard's Spectators line,
  "*SPEC*" chat read by the dead and spectators, back on a team (in a
  freeze time at once), over the network (replicated, no team slot,
  not counted for round ends). Second pass (2026-10-10): the spectator
  menu is `bottomspectator.res`'s bar of drop-downs (Options from the
  install's `spectatormenu.res`, the players with `<` `>`, the camera
  from `spectatormodes.res`), the map overview (`overview_mode`: small
  or large, zoom, names, health, tracks), the Esc menu's Team dialog
  offers Spectate, and both team menus grey it under the server's
  `mp_allowspectators 0`. Left: CS:S's starting mode for a new spectator
  (no spec or capture; ours flies free), the overview's sizes and
  defaults against a capture (docs/tech-debt.md), the auto director.

- Decals persist across rounds (bullet holes, blood), as we believe CS:S
  does (players bind `r_cleardecals`, now there; `chimera_round_cleardecals
  1` clears them each round). Left: confirm the game keeps them.

## 1. Playtest essentials

A (settings, health bars, third person), B (penetration and collaterals,
docs/plans/active/weapons.md slice 5) and C (view models) are done. D and
E were paused on 2026-10-06 to free the machine; their partial work is in
agent worktrees under `.claude/worktrees/` (D: agent-a052e118eeb8d4c8d).

- **C. First-person view models**: done (docs/plans/active/view-models.md);
  remaining: check its open questions against CS:S (flash look, shell
  direction, near plane), view-model sound events, brass for other
  players.
- **E. Other maps.** In progress:
  [plans/active/other-maps.md](plans/active/other-maps.md) (catalog).
  Fixed: aztec's walls, props' ambient light on older maps, brush
  entities, additive glows, HDR skies, start-on switchable lights, murky
  water, decals on terrain, the Water shader (section 8), unplaced
  infodecals (131 -> 13 over the stock maps: a face was skipped when all
  its corners were far from the decal; brush entities the decal's probe
  hits take it), the 3D skybox's props unlit by the sun (nuke's cooling
  towers; 2026-10-18 survey of every other stock map in the plan). Left:
  reference views of the 13 stock maps never compared (windows-captures
  sitting 4), the 13 decals left (section 10).

## 2. Custom maps and minigames

Plan: [plans/active/custom-maps.md](plans/active/custom-maps.md).

- A test map with every supported entity (`mashup_logic_test`, generated
  `.vmf`, compiled with Valve's tools; plan section "Test map").
- Entity I/O, triggers and moving brushes are in (`source/logic/src/logic`, slices 3
  and 4 of the plan), and breakables (func_breakable, func_breakable_surf;
  source/logic/src/logic/breakables.rs), model doors (`prop_door_rotating`) and
  prop damage/outputs, skins, body groups and sequences
  (source/logic/src/logic/props.rs), sprites, dust and switchable lights
  (source/logic/src/logic/visuals.rs, lightmap styles relit at run time), env_global,
  trigger_soundscape through the touch code. Prop damage follows
  specs/source/prop_damage.md (source/logic/src/logic/prop_damage.rs: prop data,
  pieces, impact damage, explosive props, client props, Wake/Sleep/motion
  inputs, OnAwakened/OnMotionEnabled, pressure). Left: breakable follow-ups
  (section 7), train
  facing/banking and player train control. (Round restarts re-create
  entities: rounds plan.) Target: two
  real minigame maps from the user's downloads.
- First minigame map: `mg_lego_multigames_v2` (in the user's content
  cache). Fixed: impacts showing the sky on maps with water (decals were
  in the depth prepass), packed materials whose names differ in case.
  The breakable-block room went from 11-17 fps to the vsync cap (about
  240 fps at 720p uncapped; docs/performance.md, "Many brush entities").
  Impacts land on brush entities (doors, breakables) and go with them;
  unbroken brush entities draw merged (`map::merge`: 1768 -> 58 meshes
  at the blocks room).

- The user's ~89 community maps (surf_, bhop_, kz_, gg_, mg_): swept with
  `mapsweep` and their map logic audited with `mapsweep --audit`
  ([plans/active/community-maps.md](plans/active/community-maps.md),
  results and what's left, ranked). Round 2 (done 2026-10-10, to
  playtest): sprite trails, `.pcf` particle systems, smoke stacks, map
  ragdolls (drawn, not simulated), players' and brushes' render looks,
  water buoyancy (boats), motors and constraints, players riding karts,
  monitors, teslas, shooters, point_push, texture toggles, screen
  overlays; the audit's noise noted once per map. Round 3 (done
  2026-10-10, to playtest): map ragdolls simulated (and sent to network
  clients), moving water (func_water_analog, water on movers),
  color_correction (`mat_colorcorrection`), embers, fish pools, muzzle
  flashes, view punches, constraints breaking past their limits,
  monitors' sky, 3D-skybox particles, +use through a usable parent; the
  audit's +use misses were its own aim. Round 4 (2026-10-10, to
  playtest): surf_nebula's dead ramp stops (static props without a
  collision model block nothing), detail props (grass and weeds),
  monitors with their camera's own fog, bots stepping aside on maps
  without a navigation mesh. Top of what's left: particle and ember
  looks checked against CS:S, strike-generator env_beams, and three
  probes on the reference server (the plan's "Probes wanted": landing
  height, touch order of overlapping triggers, static props without a
  collision model). Round 6 (2026-10-15, to playtest): template copies of
  props and physics brushes drawn, solid and shootable (hidden on 12 mg_
  maps: block spawners, boats, karts, items, balls), func_clip_vphysics
  (balls kept on the pitch, boats held at the start), models with odd
  names, targeted func_wall/func_illusionary, server command lines,
  overlay V-flip; three spec questions in the plan.

## 2b. HUD and debug views

- CS:S HUD: health/armour/ammo/money/round-timer panels, death notices
  and the Tab scoreboard are in (`client/game_hud.rs`,
  `games/cs_source/ui/scoreboard.rs`), and the weapon selection and pickup history
  (`client/weapon_select.rs`; `hud_fastswitch`), matched to CS:S
  captures panel by panel (`refcmp hudcmp`). Left of that pass: the
  radar's look (zoom, translucency, markers) is ours. The team menu is in (M; joining a game shows the message of the day, then the team menu, then the class menu: `games/cs_source/ui/join_menus.rs`). The radio
  is in (`games/cs_source/ui/radio.rs`: Z/X/C menus, the calls as console commands,
  "Fire in the hole!" on throws, bots' enemy spotted/down and need
  backup, and bots' own commands and reports: go / stick together /
  follow me from the attackers' leader, cover me, sector clear, in
  position, regroup), with a chat area and hint text (`client/chat.rs`; the automatic
  `cl_autohelp` hints: `rules/hints.rs`, triggers ours until capture C26), the scoreboard
  in the game's `scoreboard.res` look (its status icons, "BOT" latency),
  the spectator bars (`spectator.res`), the radio icon over a teammate's
  head (`sprites/radio`, hidden by walls), and
  text chat (Y / U, `say`, `say_team`). The buy and team menus draw in the game's VGUI look from its `.res` files
  (`client/vgui.rs`), as do the message of the day and the class menu
  (`games/cs_source/ui/join_menus.rs`). Left:
  the scoreboard's row spacing (16 units, a guess) and dead rows' look
  checked against CS:S (its map time left beside the clock is in), the
  spectator menu (`bottomspectator.res`'s bar, done 2026-10-10; its
  values aligned as the file says 2026-10-15) and the freeze cam panel
  (`freezepanel_basic.res`: name, "KILLED YOU", "[F5] Save this moment",
  the killer's health gauge; 2026-10-15, timings now CS:S's server cvars
  and the death cam's measured: specs/cs_source/spectating.md) checked
  against a capture of a human's freeze cam (the avatar and nemesis lines
  are still out), the buy menu's favourites (autobuy and rebuy are in: `cl_autobuy`,
  `cl_rebuy`), checking the widescreen placement against
  the game, other game messages in the chat (team joins, bomb pickups
  and drops are in), bots' answers checked against a bot behaviour spec
  (`ignorerad`, bots answering and carrying out radio commands and
  issuing their own are in: `bot::radio::obey`, `speak`; checking
  when CS:S bots talk needs that spec). The radar is in (`client/radar.rs`: the map overview turning
  with you, team dots, your place name); its range (2200 units) is a guess.
- The game menu (Esc; `games/cs_source/ui/game_menu/`: Create Server; bots; team;
  options; bug report; quit) is in, in the game's GameUI look
  (`SourceScheme.res` frames and colours, `GameMenu.res` entries), its
  dialogs laid out from the install's `.res` files with VGUI's controls
  (`client/widgets.rs`; docs/plans/active/ui-parity.md). Left: what
  that plan lists as left (the console as a VGUI frame, the greyed
  controls whose features may come), binds for
  the keyboard actions greyed in the options (`impulse 100`, ...;
  `invnext`/`invprev`, `autobuy`, `rebuy`, `jpeg`, `nightvision`,
  `impulse 201` and `+voicerecord` are in), the Multiplayer tab's spray
  logo picture and a way to pick it there (sprays are in: `cl_logofile`
  in the console; Import Spray needs an image-to-VTF converter),
  comparing the look with CS:S's
  (refcmp has no menu views).
- The main menu is in: startup without a map shows CS:S's (its
  background picture, the title in its logo font, `GameMenu.res`'s
  entries; Resume and Disconnect only in a game, Find Servers greyed),
  the same menu over the game on Esc, `disconnect` back to it; none of
  our entries (done 2026-10-16: `chimera_quickstart`, `map greybox`,
  `menu bots`, `menu team`, `callvote`, `menu extras`, the console key
  instead). Left: the
  title's exact place and the entries' spacing checked against CS:S at a
  few resolutions (the loading dialog, `LoadingDialogNoBanner.res`,
  and the interface sounds are in; CS:S has no menu music: its install
  holds none), whether keyboard moves in the menu make the rollover
  sound, the server browser (needs networking). The Chimera debug-options page is in
  (`menu debug`, the overlay cvars as check boxes; 2026-10-24). A map
  changed from inside a game shows the loading dialog (2026-10-24).
- Debug overlays: `chimera_drawhitboxes`, `chimera_healthbars`,
  `chimera_drawnav`, `chimera_drawbots`, `chimera_drawsound` (sound
  radii) and `chimera_drawtriggers` exist (2026-10-24); add more as features
  need them.
- Debug UI (F8, `client/debug_ui.rs`) is in. Each bot's row has kick and freeze
  buttons (`bot_kick <name>`, `bot_freeze`/`bot_unfreeze`, 2026-10-24), a "Clear decals" button
  (`r_cleardecals`) and a Fog check box (`fog_enable`, CS:S's name; 2026-10-24).
  Left: starting a trace from the running game (needs
  the profile build), a fixed tick-rate control, a history graph per
  render pass.

## 2c. Multiplayer

Plan: [plans/active/multiplayer.md](plans/active/multiplayer.md)
(bevy_replicon + renet; Source-style prediction, interpolation and lag
compensation of our own; slices 0-9). Slices 0-4: a listen
server (maxplayers 4; map <name>) and the dedicated cs_source_server,
connect <ip[:port]>, characters replicated, usercmds bound to server
ticks with clock sync, the client's own movement predicted and
reconciled (net_graph-style readout, cl_showerror, net_fakelag;
docs/OBSERVABILITY.md, Network play), others interpolated, movers
and props replicated, weapons predicted with lag-compensated hits
(per-tick server hitbox poses), others' shots drawn from the server's
seeds, grenades, drops and pickups. Slices 5-7 done too: rounds, money,
buying, objectives, scoreboard, chat and radio; bots with `bot_quota`;
`changelevel` taking clients along, late joiners' full state, map
downloads (connection or `sv_downloadurl`, hash-checked, cached) and the
GameUI loading dialog for joining and map changes (detailed view:
`chimera_loading_details 1`). Their leftovers too: loading players out
of the game, predicted bomb arming and defusing, names (`name` over the
network), chat and radio flood limits, spawns never inside another
player, `changelevel` to the same map, bot profile names and `kick`,
decals for late joiners. Slice 8 done: Find Servers (Favorites,
History, Lan; Internet waits for a master server), server info queries,
LAN discovery, passwords. Slice 9 (lobby) dropped: standard CS:S. Left:
LAN broadcast between two machines (Windows) untested, CS:S's join rules
(empty team restarts the game) need a spec. Open questions 3 (NAT) and
4 (server size) answered 2026-10-13: Steam networking needs an app id of
our own (docs/plans/active/nat-traversal.md; UPnP mapping prototyped,
`net_upnp 1`), and a server holds 32 players within the tick
(multiplayer.md "Capacity", `heavy::net_capacity`); 1 is moot
(networking came first), 2 was taken as both.

Voice chat (2026-10-15, docs/plans/active/voice-chat.md): K held talks
(`+voicerecord`), Opus over the game's connection, `sv_alltalk`,
`sv_voiceenable`, `voice_*`, the HUD's talking icon and list. To
playtest with friends on Windows and Linux (real microphones and
speakers: only fakes are tested). Per-player mute is in (the game
menu's player list, `voice_mute`; client-side, for the session), and
the open microphone (`voice_vox`, `voice_threshold`), and the Voice
tab's microphone test, meter, boost and transmit volume
(`voice_transmit`, 2026-10-24). Left: icons over talking players' heads, a capture of
CS:S's voice HUD and client `voice_*` defaults (Windows), and checking
the dead-hear-the-dead rule with two clients on the probe server.

Votes (`callvote`, `net::vote`; CS:S's issues `game::votes`, panels
`ui::vote_hud`) are in: kick, change level, restart game, F1/F2, the
vote setup dialog (`ui::game_menu::vote_setup`: `callvote` alone or our
no menu entry, as CS:S), players joining a running vote, bots voting with
`sv_vote_bots_allowed 1` (our rule). Left: CS:S's rules, issues, look
and how its setup dialog opens, from capture C25 (parity-gaps.md).

- Missing server kicks (measured defaults 2026-10-16, objectives.md Q2):
  `mp_autokick` 1 (kick idle and team-killing players) and
  `mp_spawnprotectiontime` 5 (kick a player who team-kills within 5 s of a
  round restart). What counts as idle and how many team kills are
  unmeasured, and not measurable with bots on the probe server (kicks
  very likely spare bots; the probe plugin heals its targets): needs a
  human client (a Windows sitting).

## 2d. CS:S demos as a parity check (user idea, 2026-10-14)

Built: `demodump` reads CS:S `.dem` files (`source_formats::dem`; SourceTV
and, untested, client demos; OBSERVABILITY.md "CS:S demos").

- **`playdemo` in our engine.** Built: `playdemo <file>` / `--playdemo`
  (OBSERVABILITY.md "CS:S demos"). Left: a `demoui` panel, bodies' recorded
  animation where a game sends it, zoom through a sniper's eyes, physics
  props and doors from the demo, and comparing ticks against CS:S's own
  `playdemo` (refcmp). Was: drive our game from a CS:S demo: every
  player's position, angles, weapon, animation sequence, shots
  (`CTEFireBullets`), grenades, smoke clouds, sounds and the round/HUD
  state come from the file; our renderer, animation, effects, HUD and
  audio show them. Uses: compare the same tick against CS:S's own
  `playdemo` (screenshots via refcmp on the Windows PC or an unlocked
  desktop), watch any CS:S demo in our game (CS:S's `playdemo`/`demoui`
  are missing rows in parity-gaps.md), and record our own `.dem` later.
- **Re-simulate recorded input.** Built: `demosim` (OBSERVABILITY.md
  "Re-simulating a client demo"): a client demo's user commands decoded
  and played again through our movement and weapons, compared per
  command; the probe's logs as a real reference (`--probe`). Remaining:
  record a client demo on the Windows PC (bhop, ladders, surf,
  strafe-shooting; the doc says how) and run it, which also confirms the
  usercmd delta base and the own-client prop names
  (`cslocaldata.m_vecOrigin`, `m_nTickBase`) on a real file; other
  players as colliding puppets; doors and movers as recorded. Community
  demos on current map versions next; old pro demos (2005-2010) need the
  pre-Orange-Box network protocol and their own map versions, and often
  run at tick 100.

## 3. Weapons, remaining

In progress: [plans/active/weapons.md](plans/active/weapons.md). The
framework, the knife, every CS:S gun (zoom, silencers, bursts, pellets,
shell-by-shell reloads, the dual Elites), grenades, HUD, deathmatch and
a first bot are in.

- Probe the new guns' unmeasured rules (listed in the weapons plan):
  recoil (the automatics: measured 2026-10-15), the shotgun reload and pellets, the other ammo
  types' penetration. Ammo, box sizes, `primammo`'s fill-up and the
  spawn pistols' reserves: measured and matched 2026-10-14 (M19).
- The AWP's view model is in (MDL v48 reads like v44,
  specs/cs_source/mdl_v48.md); compare its fire and reload against the
  game. (MDL 45–48, sections, `.ani` blocks and the zero-frame cache
  are read: `anim.rs`.)
- Zoom: measure `zoom_sensitivity_ratio` and compare the scope overlay
  (the game's textures, laid out by eye) with CS:S.
- Rounds, money and buying: slice 1 done (`chimera_rounds 1`,
  [plans/active/rounds.md](plans/active/rounds.md)); ammo buying is in.
- Objectives, remaining (bomb and hostages are in: `games/cs_source/src/objectives/`,
  specs/cs_source/objectives.md): measure the spec's open questions on the
  probe (Q1 movement while arming, Q3 blast shape/walls/armour, Q4 beep
  schedule and loudness and where the LED sits, Q5 money, Q6 carrier
  choice, Q7 the bomb's solidity, Q9 hostage models, Q10 follow speeds,
  Q11 rescue rules, Q12 radar (no `sprites/c4` marker is drawn: CS:S
  shows none, playtest 2026-10-14),
  Q14 tick rounding); the planted bomb's own screen (`c4_panel`: the
  view model's is in, `games/cs_source/ui/bomb_fx.rs`), screen shake, the
  explosion's scale (it uses the HE's base explosion effect: the install
  names no C4 effect of its own, no explosion `.pcf` or sprite); hostage
  animation beyond idle/walk/run and a nod (`hostage_anim.rs`: compare
  with CS:S's hostages, which aren't measured; head/aim pose parameters
  toward the leader, flinch and cower), hostages avoiding
  "no hostages" nav areas, crouching and jumping; the game events' fields
  and the log's other lines (kills, chat, buys, connects: objectives.md
  Q17; the spec-named events and `triggered` lines are in,
  `rules/events.rs`); bots leading hostages, buying kits, guarding;
  which `Player.UseDeny` CS:S plays (doors_buttons.md Q2).
- Grenades, remaining (HE, flashbang and smoke are in: `weapon/grenade.rs`,
  `games/cs_source/grenades.rs`): measure the spec's open questions on the
  probe (fuse ticks, release timing, flash amounts and overlay curve, HE vs
  armour, smoke vs bots, HE shake and hearing) and replace the fits
  (tech-debt); bots throwing grenades, the `_thrown` models.
- Death animations on bodies (the grenade gesture is in; the reload gesture is in:
  `<Move>_Reload_<weapon>` by activity, an assumption for the spec's
  `reload_<suffix>`, with the shotguns' `_start/_loop/_end`).
- Impact effects, remaining (specs/cs_source/impact_effects.md; the
  section-by-section list is in `impact_effects.rs`'s `//!` doc):

  | Section | State |
  |---|---|
  | 1 who and when | in, others' shots to the clients in the shooter's PAS (`net::weapons`) |
  | 2 per impact, 3 letters | in, with the ragdoll check (no particles while one awake ragdoll is on the line) |
  | 4-8 old-style machinery, debris, dust, sparks | in; electric flares depth tested, no visibility test |
  | 9 glass | in: shards tumble on three angles and settle flat |
  | 10 blood | in from the spec's table, with the sprite cards' screen-size limits (2026-10-17, every sprite-card material), the goop's spin stop and yaw flip and the mist and goop light tint; `tint_perc` and the screen-size measure are our readings (Q7) |
  | 11 splash | in: water's own effect; slime's `slime_splash_03` from the install's `.pcf` (no stock map has slime) |
  | 12 knife | in; its decal: no spec names it (below) |

  Check the spec's open questions in the game. The muzzle flash's light on
  nearby walls is part of the view-model work (1C follow-up).
- Decals, remaining: on characters (blood; no spec covers model decals,
  overlays_decals.md's open questions); the knife's mark (`ManhackCut` is
  a guess and no spec names it: impact_effects.md 12 points to
  overlays_decals.md, which has no knife entry; a spec session or a
  capture) and lit decals (wood, glass) against the game; tracers are in (every 4th bullet, spec tracers.md; check N and
  the look in play).
- Dropping weapons, remaining (`drop`/G, pickup, death drops, bullets
  and blasts pushing loose weapons and the optional `chimera_usepickup`
  swap are in, `weapon/drop.rs`): the throw and slide are measured (M20, 2026-10-15; ours stops at 72 of 95 units); the re-pick delay, pickup
  reach and mass, friction (0.5) and the box collider are guesses
  (measure on the probe server: how far a shot weapon skids).
- Ragdolls, remaining (`map/ragdoll.rs` has the spec's bodies, joints,
  self-collision pairs, death poses, impulse, bone velocities, bullet and
  blast pushes, separation repair, settling and removal): check the
  spec's open questions (force scale, bone_dt, death-pose facing and
  crouch choice, joint limit convention) in the game.

## 4. Bots

Parked (user, 2026-10-13): bots are fine for now; focus on what players
see. The last pass's notes and open ideas (de_dust2 balance ~71% T,
de_nuke vent stuck spots, team eco) are in git history and
docs/tech-debt.md's bot rows.

## 5. Console, remaining

The console and overlays are in (chimera/core/src/console.rs, chimera/client/src/client/console.rs).

- `net_graph` (chimera/client/src/client/net_graph.rs): compare the
  layout, colours and graph with CS:S's own (a capture of `net_graph 1`
  to `3` on a server); `net_graph 3`'s per-message bands, `net_graph 4`,
  `net_scale`, `net_graphmsecs`, `net_graphtext`, `net_graphsolid`.
  At the default `net_graphheight` 64 its last lines overlap the HUD's
  ammo bottom right (1280x720); check where CS:S's sits.
- `cl_showpos 2`: what CS:S shows with it is unknown here (no public
  description found); look in the game.

## 6. Sound, remaining

docs/plans/active/sound.md.

- Scrapes by ear: tune `games::cs_source::scrapes::friction_energy`
  against CS:S (a crate shoved across concrete, a ragdoll sliding down a
  ramp) once spec open question 8 is measured.
- Room DSP by ear: tune `audio::room::PRESETS` against CS:S (dust2's
  tunnels, nuke's halls); soundscape `dsp_player` and `soundmixer` are
  read but not played (engine-side effects).
- Stock ambient_generics the bomb starts (de_nuke's alarm, dust2's
  fires) now play when it explodes (`BombExplode`); check them by ear. (Prop outputs are in: de_nuke's steam, cs_office's
  projector; env_steam's JetLength/SpreadSpeed/Speed inputs are in, for new
  puffs; heat-wave jets, Type 1, refract the scene: no stock map has one, so unseen.)
- Measure ambient_generic's level for script entries vs raw waves (spec
  open question 11).
- Measure on the probe server: the distance curves (replace the H1/H2
  guesses), CS:S footstep silence rules, the jump sound, wave choice.

## 7. Physics props, remaining

- Player physics shadow (games/cs_source/src/game/shadow.rs, games/cs_source/tests/it/heavy/map_physics_shadow.rs)
  follow-ups (docs/tech-debt.md "Physics shadow"): the push speed limit
  (spec Q7), the shadow's weight on a prop stood on, the controller's
  velocity target when not touching (spec 4.1 step 5), measuring the push
  feel against CS:S on cs_militia/de_inferno crates. CS:S has no +use
  pickup of props (no physgun; +use pushes only under `sv_turbophysics 1`,
  spec 4.2.4, not done).
- Prop damage follow-ups (docs/tech-debt.md "Prop damage"): stress crush, the
  velocity restore after an impact breaks a prop, the chunks' 90°
  realignment, the spec's open questions on the probe server
  (Q1-Q14: damage types, gas-can ignition, player impact rules, client
  break sounds, round restarts of client props).
- Model doors: the spec's open question 8 on the probe server (which
  entries the hardware and surface pick; we read the model's
  `door_options` skin and `hardwareN` blocks, docs/tech-debt.md "Model
  doors").
- Breakables, remaining (source/logic/src/logic/breakables.rs,
  games/cs_source/tests/it/heavy/map_breakables.rs): the GlassBreak/BulletProof
  decals and brush propdata templates are in (their readings in
  docs/tech-debt.md "Breakables"), and so are bulletproof glass's ricochet
  and the computer material's sparks on bullet traces; left: whether the
  ricochet has a look (the specs give it only `Bounce.Shrapnel`), the client's pane
  drawing (spec open question 8:
  edge piece selection, which side draws, the piece's offset; ours in
  docs/tech-debt.md "Breakables"), the spec's open questions on the probe
  server (bullet/knife damage types, broken brush visibility, shots after
  a window breaks).

## 8. Visual fidelity

- **Water surfaces** (specs/cs_source/water.md, `render::water`): refraction,
  planar reflection, the cheap cubemap pass, under-water fog (world,
  props, ropes, decals, particles; also in water of other shaders, de_nuke's
  pool, from its material's fog keys), bottom materials, the
  `$underwateroverlay` screen warp (with its `$bluramount` blur, which is
  what shows on de_port), the intersection view's height fog, the sky
  under water (through the surface from below when the water refracts,
  none in fog-only water), the water cvars, `$blurrefract` with `$refracttint` and the "Water
  detail" choices (`render::water::WATER_DETAIL`, for the Video Advanced
  menu) are in. Left: a Refract shader spec (the warp's
  strength and blur are guesses: de_port's `water_warp01` moves the
  screen about a pixel, its blur is 5 x 5 taps a pixel apart),
  the `$basetexture` variant (no stock material uses it), and a
  refcmp comparison of de_port/de_chateau water (no reference captures
  yet; it would settle de_port's dark speckles: refraction lookups at
  `$refractamount` 5 landing on barely submerged shore, which the spec's
  shore fade leaves unreflected). Water currents: no stock map has
  current contents (spec movement.md open question 15), so not applied.
- **HDR parity**: `mat_hdr_level` 1/2 (default 0, the reference's LDR)
  loads the HDR lightmaps, ambient cubes, world lights and `_hdr` sky,
  renders HDR with bloom and auto exposure within the map-start
  `env_tonemap_controller` bounds (client/hdr.rs). Not matched against the
  game (no HDR reference captures; refcmp runs LDR). Open questions for a
  spec session: the engine's HDR lightmap scale (taken as 1), the exposure
  target (`hdr::EXPOSURE_TARGET` 0.5), metering and adaptation speed
  (Bevy's histogram, 3/1 stops/s), the bloom filter and strength (Bevy's
  `OLD_SCHOOL` at 0.05 x bloom scale), whether bloom precedes exposure,
  HDR cubemaps/envmaps (still LDR), the `<sky>_hdr` material lookup rule,
  `mat_hdr_level` 1's exact look, and the controller's inputs (followed
  through the logic as they fire, `map::TonemapInputs`): what
  `SetTonemapRate` scales (taken as a multiplier on the adaptation speed),
  whether `UseDefaultAutoExposure` means the cvars' 0.5/2 bounds, and
  `SetBloomScaleRange`, `SetTonemapScale`, `BlendTonemapScale` (accepted,
  ignored; no cached map uses them). Then refcmp captures at
  mat_hdr_level 2.
- **Light styles and baked prop light, open questions** for a spec
  session: animated styles step ten pattern letters a second, unblended,
  'm' = as baked, scaling the style's lightmap share in linear light
  (Quake's convention; the engine's rate, interpolation and scaling space
  aren't specced), styles 13-31 without a light `pattern` stay steady,
  and props' probes ignore them. Baked per-vertex prop light (`.vhv`,
  `source/formats/src/vhv.rs`): its decode is taken as the spec's
  (2v)^2.2 (shaders.md 4) for both `sp_` and `sp_hdr_` files (the HDR
  files in the cached maps hold the same bytes as the LDR ones), each set
  falls back to the other, and a file whose checksum or vertex count
  doesn't fit the model is skipped; it comes out at about 0.45-0.75 of
  our probe on average (self-shadowing, or a different scale?). Compare
  a prop on kz_ancient_ruins or surf_demise with CS:S.
- **More refcmp views** across dust2 (mid, long, B, spawns) and other maps.
- **Material effects, open questions** for a spec session
  (specs/cs_source/shaders.md 16-19): texture transform composition and
  the TextureTransform proxy, `$emissiveblend*` (surf_demise's glowing
  banners aren't drawn), `$selfillummask`'s channel and
  `$selfillum_envmapmask_alpha`, detail modes 10/11.

## 9. Performance

Measured and culled (docs/performance.md): `chimera_perf`, `refcmp bench`,
PVS culling by world chunk, areaportals (closed doors hide what's behind
them; views clipped through openings; window brushes fade in and close
their portal), occluders (func_occluder), prop fade distances with
dithered fade bands. Left:
- Done: physics/animated props fade (fademindist/fademaxdist keys,
  `prop_fade` test); LODs for static, skinned (animated) props and
  CS:S player-model bodies (`render::lod`; levels share the skeleton, as
  the `.vvd`'s bone ids are the model's own at every level; tests in
  `prop_lods.rs`; a level with a negative switch point, as the player
  models' last, -1, is never drawn: `player_lods.rs`). Left: a near/far
  capture of a skinned model; LODs for
  Combat Arms' models and view models.
- cs_assault `refcmp vischeck` fails (57 of 316 views, worst 33.7k
  pixels on the roof): the PVS hides props and buildings seen over the
  walls through sky brushes, consistent with the map's own data
  (performance.md, "Checks"). Capture C27 says whether CS:S hides them
  too; if it shows them, find the engine's rule for static props seen
  past sky brushes. Until then cs_assault's vischeck failure is expected.
- `refcmp ordercheck` on cs_assault: passes (limit 100 pixels) but 8 of 316
  views still differ (re-run 2026-10-25, `capture-ours` works here, no
  reference captures needed): `nav1847_90` 56 pixels, `nav1567_180` 26,
  six others by 1 pixel. Looked at `nav1847_90`: the differing pixels
  are a strip along one edge (x 880-890, y 15 and below, in a 1280x720
  capture) where a dark surface meets lit wood, i.e. a depth tie at a
  geometry edge between two opaque draws, not a blended item. Most likely
  cause: opaque batches draw in pipeline-id order (made in the order the
  camera first saw each material kind), and where two opaque surfaces
  meet at the same depth the one drawn last wins. Next step: capture
  `nav1847_90` alone and after `nav1847_0` (performance.md, "Checks"),
  then query the two entities at that pixel (remote protocol, section 3)
  and compare their materials and pipeline keys; fix by giving opaque
  batches a stable order (as `render::blend_order` does for blended items)
  or by fixing the overlapping geometry. Not worth more than that while
  under the tolerance.
- Measure on the Windows PC (`refcmp bench` there) and set a budget.
- Frame-time follow-ups (performance.md, "Frame time pass"): before/after
  numbers on a quiet machine and on the Windows PC (`refcmp bench`, also
  `CHIMERA_EXECUTOR=multi`, `-- --view-size 3840x2160`); props as
  hierarchies of their own (cheaper collider propagation) only if a map
  shows it (stock maps walk 240-710 colliders a tick: performance.md,
  "Props that never sleep"); a render
  scale (`mat_render_scale`) only if a GPU turns out to be the limit (the
  dev box's RTX 3080 draws dust2 at 4K in about 1.3 ms). Done: props that
  never slept, ragdoll and gib hulls cut, ragdolls' forced sleep, the
  view-model camera off with nothing on its layer, broken props' pieces
  as debris that doesn't collide with debris, ragdoll limbs that jittered
  against the floor (joints now solved in the contacts' passes, no
  gyroscopic torque; performance.md, "Props that never sleep", "Fewer
  cameras per frame"), broken props' pieces with their `.phy` hulls
  (sleeping by props' stillness rule), a toy-scene test of ragdolls'
  forced sleep.
- `perfgate` in the pre-push hook (budgets per game app): deferred by
  the user (2026-10-15), pushes aren't blocked on performance for now.
  A Windows-side check needs another counter source (none there yet).

## 10. Long tail

Counts are from de_dust2's entity lump and static prop lump.

- **Remaining decals**: 13 infodecals over the stock maps find no
  surface (dust2 5, tides 3, compound 2, dust 2, train 1): by props
  (window frames) or farther than our reach from any face; the probe
  ignores props; whether the engine puts them on props is open
  (overlays_decals.md). Overlays with
  no geometry (train 8, assault 5, others 1) list no faces or faces
  away from their quad: nothing to draw (see plans/active/other-maps.md).
- **Verify inferred Source rules** with the comparison tool, using a local
  copy of a map with test entities added where dust2 has no example: floor
  and ceiling decal orientation, decal reach, overall brightness/tonemapping.
- **Brush entities**: movers (doors, buttons, func_rotating, trains,
  func_brush) move through the logic layer, breakables break; baked brush
  entities draw their render mode (colour/texture/alpha blend, glow/additive
  add, 10 hidden) and rendercolor; mode 1 draws the flat colour and mode 4
  alpha-tests (cs_italy's and de_piranesi's foliage and fences).
- **Fire** (specs/source/fire.md) is in: env_fire (heat, growth, burn damage box with line of sight, fuel, Extinguish, outputs, re-created unlit each round, Q4), env_firesource/env_firesensor, entity flames (Ignite inputs, burning props, the gas can), the `env_fire_large_smoke` and `burning_character` looks (docs/tech-debt.md "Fire"). Next: compare a lit de_dust2 fire with CS:S's (Q1, Q10 operator meanings), and the probe-server questions: burn vs armour, the kill icon and score (Q2), player ignition (Q3), the radius damage of a burning can (Q6), burning-prop light (Q7), buried fires (Q9), floors on de_dust/de_train (Q5).
