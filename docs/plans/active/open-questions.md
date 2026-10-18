# Open questions: what can answer each

Started 2026-10-17. Every open question in the specs (`specs/cs_source/*.md`,
`specs/source/*.md`, their "Open questions" sections) and every "differs" or
"missing" row of [parity-gaps.md](parity-gaps.md), sorted by what would
answer it. Questions already marked answered in their spec are left out;
partly answered ones list only what is left.

Classes:

1. **Probe server**: answerable now on the CS:S dedicated server
   (`tools/css_probe`: server state, events, damage, entities, the sounds
   the server sends, movement), or offline from the server install's files
   and the maps with our own readers.
2. **CS:S client**: needs the game's client (screenshots, audio, RenderDoc,
   client demos): the Windows sittings in [windows-captures.md](windows-captures.md).
3. **Spec session**: needs someone reading the original source (or a
   design decision with no game behaviour to measure).
4. **Second human**: needs a second human player (bots behave differently).
5. **Answered, not marked**: the answer was already measured elsewhere;
   the spec or row is updated in the same commit as this list.

Counts (2026-10-17, 297 rows; a row grouping several questions counts
each, a row naming two classes counts in both): class 1: 137, class 2:
174, class 3: 56, class 4: 7, class 5: 25 (24 spec questions or parts and
one parity row, fixed with this list). Part B below works through the
class-1 items by player impact.

## Specs: specs/cs_source

| Spec | Question | Class | What would answer it |
|---|---|---|---|
| animation | ground speeds, walk/run threshold, max body yaw, playback scale | 2 | client demo of a bot walking/running (capture C14) |
| animation | reload sequence choice and timing by movement | 2 | client demo, frame-stepped |
| animation | grenade movement shoot layers; ELITES left/right rule | 2 | client demo |
| animation | jump sequence restart and air blending | 2 | client demo |
| animation | death sequences and when the ragdoll takes over | 2 | client demo |
| animation | weapon `PlayerAnimationExtension` per weapon | 3 | read the decrypted scripts in a spec session |
| animation | do the sequences' footstep events play anything | 2 | client audio while a bot walks (the server's steps follow movement, measured) |
| animation | IK rules and locks | 3 | spec session (out of scope so far) |
| animation | lag compensation's use of the layers | 3 | spec session (hitbox/lag-compensation spec) |
| fall_damage | `damagemodel 1` doubling | 1 | a spawned trigger_hurt with damagemodel 1 (no stock map uses it) |
| fall_damage | armour against trigger_hurt (drown type, others) | 1 | drown type done (Part B); other types: a spawned trigger_hurt, armoured bot |
| fall_damage | half-second timer per trigger or per toucher | 1 | two bots entering de_port's at different ticks (done, Part B) |
| fall_damage | `Player.FallDamage` on hurting landings only | 1 | sound hook on drops with and without damage (done, Part B) |
| fall_damage | landing view roll and its decay, HUD indicators, `HealthTookDamage` | 2 | client screenshots after a drop |
| grenades | Q2 redraw: the same grenade again or the best weapon | 1 → 2 | tried in Part B: the bot AI picks its own knife after a throw, so a human capture decides |
| grenades | Q3 switch with the pin pulled | 1 | a slot pick while attack is held (done, Part B) |
| grenades | Q6 spawn inside a wall | 1 | throw flush against a wall, log the projectile |
| grenades | Q7 bounce sounds per contact | 1 | sound hook while a grenade bounces (done, Part B) |
| grenades | Q9 HE visuals (fireball, shake, light) | 2 | client screenshots at `host_timescale 0.1` |
| grenades | Q10 left: crouched victims, walls and props blocking | 1 | `hemeas.py` with a crouched or covered victim |
| grenades | Q11 ear-ringing presets and who gets them | 2 | capture C31 (the server sends nothing, measured) |
| grenades | Q12 left: flash through walls, players, props, smoke | 1 | flash netprops with something between |
| grenades | Q13 flash overlay curve | 2 | client screenshots every 0.1 s |
| grenades | Q14 flash DSP presets | 2 | client console/audio |
| grenades | Q15 left: smoke on a moving lift, in-flight trail | 1, 2 | lift: probe (de_nuke has none moving; skip); trail: client |
| grenades | Q16 smoke against bots; target ID through smoke | 1, 2 | bots (parked); target ID: client |
| grenades | Q17 left: dying primed while moving; which grenades drop at death; `drop` with a grenade | 1 | death drops done (Part B); moving and `drop`: the same probe runs |
| impact_effects | Q1 CS:S's own impact callback (counts, colours, timings) | 2 | client screenshots at `host_timescale 0.1` |
| impact_effects | Q2 ricochet rate; no effect on refused teammates | 2 | client audio count; C30 |
| impact_effects | Q3 exit impact after penetration | 2 | client screenshot of a door's far side |
| impact_effects | Q4 particle throttle | 2 | client, a smoke-heavy scene |
| impact_effects | Q5 surface colour source | 2 | client screenshots of red and white walls |
| impact_effects | Q6 blood position and who makes it | 2 | client, `cl_predict` 0/1 |
| impact_effects | Q7 particle operator semantics | 2 | client recording at `host_timescale 0.05` |
| impact_effects | Q8 splash size per ammo; underwater impacts | 2 | client screenshots |
| impact_effects | Q9 surfaces without a decal | 2 | client: shoot a ladder |
| impact_effects | Q10 wood fleck fade | 2 | client at `host_timescale 0.2` |
| impact_effects | Q11 left: knife water check and client prediction (its damage type is measured: bullet) | 2 | client |
| impact_effects | Q12 `cl_new_impact_effects` | 2 | client console |
| impact_effects | Q13 effect detail setting | 2 | client video settings |
| mdl_v48 | zero-frame entries, v44 leftovers, `.ani` header | 3 | nothing reads them; spec session if ever needed |
| mdl_v48 | AWP poses against the game | 2 | view-model screenshot pair |
| movement | Q15 water currents and base velocity | 1 | no stock map has a current; a spawned `func_conveyor`-style test |
| movement | Q16 left: ladder jump-off 270, water jump 256/50 | 1 | movecmp scenarios (climb speeds and swimming already match tick-exact) |
| movement | Q18 a box exactly touching a floor | 1 | `setpos` onto a floor's height, `getpos`, jump |
| movement | Q19 displacement contact normals elsewhere | 1 | movecmp on more dust2 strip edges |
| nav | Q1 the per-area CS byte | 1 | `nav_analyze` and `nav_save` on the probe, read the bytes |
| nav | Q2-Q4 bot path cost, attribute handling, follower | 3 | bots are parked; a behaviour spec |
| nav | Q5 nearest-area distance | 1 | a bot placed off-mesh above a ledge (bots parked) |
| nav | Q6 stairs flags at load | 1 | `nav_edit` on dust2's stairs areas |
| nav | Q7 approach record fields (v9) | 3 | spec session |
| nav | Q8 `earliest_occupy` effect | 1 | bot round starts (bots parked) |
| objectives | Q1 left: leaving the zone by teleport while planting | 1 | `mashup_tp` mid-plant |
| objectives | Q3 left: walls between bomb and target; other radii | 1 | targets behind walls on de_nuke/de_chateau |
| objectives | Q4 beep schedule and loudness; LED place; debris sounds | 2 | the server emits no beep (measured): client audio, sitting 2 |
| objectives | Q5 money for plant, defuse, round-end reasons, hostages | 1 | `mashup_state` money around each event (done, Part B); left: a draw's pay, Ts winning after a rescue |
| objectives | Q6 left: who gets the bomb with humans; defuse announcers | 4, 2 | a human on T for 50 restarts; client audio |
| objectives | Q7 left: the planted bomb on a moving platform | 1 | no stock map; skip |
| objectives | Q10 left: hostages at corners and jumps; give-up rule | 1 | `mashup_htrack` with longer leads |
| objectives | Q12 HUD and radar | 2 | client screenshots |
| objectives | Q13 `info_bomb_target` radius | 1 | spawned entity (no stock map uses it) |
| objectives | Q14 plant/defuse tick rounding, same-tick defuse and blast | 1 | event ticks from Q1/Q3 runs |
| objectives | Q16 the bar while planting | 2 | client screenshots |
| overlays_decals | Q1-Q9 (decal orientation, reach, size, displacements/props, overlay order, overlays vs decals, corner mapping, deathmatch flag, fade) | 2 | client screenshots, one each (9 items) |
| physics_props | Q1 inertia tensor | 1 | spin-up of a prop from an off-centre shot, logged per frame |
| physics_props | Q2 rotational inertia limit | 3 | inside vphysics |
| physics_props | Q3 damping law | 1 | a prop with damping set, speed per tick |
| physics_props | Q4 friction and elasticity combination | 1 | bounce height and ramp slide of a spawned prop |
| physics_props | Q5 sleep thresholds | 1 | `mashup_watch` a rolling barrel |
| physics_props | Q6 air drag and density | 1 | density 2.00 read (done, Part B); the drag: a falling prop's speed |
| physics_props | Q7 player push limits | 1 | bot walking into props of 300 and 400 kg |
| physics_props | Q8 hull shrink | 1 | a prop's rest height on a flat floor |
| physics_props | Q9 static prop solid types 2 and 6 | 1 | traces and walks against rotated static props |
| physics_props | Q10 SOLID_VPHYSICS without `.phy` | 1 | `mashup_trace` through a prop_dynamic without one |
| physics_props | Q12 player push call sites (< 75 vs > 75 in/s) | 1 | bot walking into a barrel at two speeds |
| physics_props | Q13 `sv_turbophysics` default | 1 | `cvarlist` (done, Part B: 0) |
| physics_props | Q14 drag axis areas | 3 | units unknown; spec session |
| physics_props | Q15 model type 1 solids | 3 | none shipped |
| ragdolls | Q1 client-side ragdolls, per-client results | 4 | two clients watching one kill |
| ragdolls | Q2 left: does a player's death push use the ammo impulse | 2 | capture C13 |
| ragdolls | Q3 death pose choice | 2 | client demo |
| ragdolls | Q4 bone_dt | 2 | client demo |
| ragdolls | Q5 joint limit convention | 3 | inside vphysics |
| ragdolls | Q6 `cl_ragdoll_physics_enable` | 2 | client |
| ragdolls | Q7 collision with server props and movers | 2 | client |
| ragdolls | Q8 water | 2 | client |
| ragdolls | Q9 removal at restart and disconnect | 2 | client |
| ragdolls | Q10 bone derivative helper | 3 | spec session |
| ragdolls | Q11 physics position interpolation | 3 | spec session |
| ragdolls | Q12 damping, sleep, inertia, friction | 1 | as physics_props Q1-Q5 |
| ropes | strip builder (side vector, U edge, alpha, culling) | 2 | client screenshot edge-on |
| ropes | width = full width | 2 | client screenshot |
| ropes | engine point lighting | 2 | client screenshot |
| ropes | Hammer defaults in the FGD | 3 | the FGD |
| ropes | `_back` rope materials; `cable.vmt` blend | 1 | list `materials/cable/` in the server install's VPK (`vpkls.py`) |
| ropes | TextureScale network rounding | 3 | spec session |
| ropes | dangling rope's free end | 1 | sweep the maps' entity lumps for a rope without NextKey |
| ropes | CS:S-specific rope code; the 10-node cable's 11 px | 2 | client screenshots |
| shaders | Q1 lightmap scale constant (test map) | 2 | client test map |
| shaders | Q2 left: bump redistribution with coloured light; page borders | 2 | RenderDoc |
| shaders | Q3 linear light and envmap scale | 2 | RenderDoc |
| shaders | Q4 alpha-test reference (0.5 consistent) | 2 | RenderDoc of a ramp texture |
| shaders | Q5 brush tangents | 3 | engine code |
| shaders | Q6 `mat_fullbright` | 2 | client screenshot |
| shaders | Q7 model modulation parameter | 3 | engine code |
| shaders | Q8 translucent/additive blend states | 3 | engine code |
| shaders | Q9 decals on bump-lit brushes | 2 | client screenshot pair |
| shaders | Q10 bump weight sum 0 | 2 | client |
| shaders | Q11 vertex-lit inputs | 2 | capture C28, RenderDoc |
| shaders | Q12 pixel-fog depth | 3 | engine code |
| shaders | Q13 revision skew | 3 | spec session |
| shaders | Q14 `$phong` | 3 | spec session if content needs it |
| shaders | Q15 WindowImposter, ShatteredGlass, LightmappedReflective | 3 | shader source |
| shaders | Q16 texture transforms | 3 | material-system source |
| shaders | Q17 `$emissiveblend*` | 3 | shader source |
| shaders | Q18 `$selfillum` details | 3 | shader source |
| shaders | Q19 detail modes 10, 11 | 3 | shader source |
| shaders | Q20 displacement texture coordinates (measured) | 3 | confirm from the compiler/engine source |
| shaders_two_texture_blend | Q1, Q2 the ×2 factor, sampler sRGB | 2 | RenderDoc of a de_aztec wall |
| shaders_two_texture_blend | Q3 bump pages on de_aztec | 1 | the BSP faces' lightmap page counts, with our reader |
| shaders_two_texture_blend | Q4 mode 0 with detail and bump | 3 | unused by stock content |
| shaders_two_texture_blend | Q5 refcmp after implementing | 2 | refcmp views |
| shaders_two_texture_blend | Q6 2-vector `$detailscale` | 3 | material parser source |
| shadows_sky | Q1 shadow clipper; Q2 static props as casters; Q3 player shadows; Q5 2D sky order; Q6 shadow detail; Q7 blend space | 2 | RenderDoc/screenshots (6 items) |
| shadows_sky | Q4 the no-shadow_control ambient colour | 3 | engine code |
| sounds | Q1 distance falloff, pan law, occlusion | 1, 2 | the server's distance-gain query needs a plugin build (blocked here); else client recordings |
| sounds | Q2 left: other entries' wave picks | 1 | sound hook, 200 shots of a 3-wave entry |
| sounds | Q3 left: wading in knee-deep water | 1 | sound hook while running in shallow water |
| sounds | Q5 `Player.Swim` doubled for the owner | 2 | client trace |
| sounds | Q6 gravel surface override | 1 | sound hook walking on gravel |
| sounds | Q7 left: shot sound path, ricochets, whizz | 2 | client trace |
| sounds | Q8 scrape energy | 1 | sound hook while shoving a crate |
| sounds | Q9 weapon sound timing (reload parts, empty click, zoom, deploy) | 1 | sound hook during each action |
| sounds | Q10 ambient_generic presets and Hammer defaults | 1 | sweep the maps' entity lumps |
| sounds | Q11 ambient_generic level for a radius | 2 | client audibility test |
| sounds | Q12 channel stealing and voice limits | 2 | engine |
| sounds | Q13 remote footstep cadence | 2 | client |
| sounds | Q14 `dsp_player`, `soundmixer` | 2 | client |
| sounds | Q15 soundscape proxy radius; triggerable OnPlay | 1, 2 | OnPlay: spawned entities; what plays: client |
| spectating | freeze cam timing for humans | 4 | a human killed by a bot (client demo) |
| spectating | a human without the freeze cam waiting 6.5 s | 4 | a human, `cl_disablefreezecam 1` |
| spectating | mode 3 after the last teammate died | 1 | the dead bots' `m_iObserverMode` and target, read on the server |
| spectating | first target: lowest index or next after own | 1 | the dead bots' `m_hObserverTarget` |
| spectating | 6.5 s with the freeze cam off server-wide; bots' mode 4 vs a human's last mode | 4 | a human client |
| sprites_dust | Q1 scale rounding; Q2 undefined origin; Q3 vertex colours and `$color`; Q5 pixel diameter; Q7 Linux random range; Q8 proxy state; Q10 additive fog; Q11 global blend; Q12 HDR | 2 | client screenshots (9 items) |
| sprites_dust | Q6 network bit masking; Q9 dust brush contents | 3 | engine code |
| tracers | Q1-Q5 (frequency, own path, look, whizz, dormant shooters) | 2 | client recordings (5 items) |
| view_models | Q1-Q7 (bob, bob state, sway, mirroring, muzzle flash, brass, lighting point) | 2 | client screenshots, capture C15 (7 items) |
| view_models | Q8 sequence cross-fades | 3 | animation spec |
| water | Q1-Q12 (expensive water, reflect texture, target size, fog range, cheap pass, time base, fog material, patched proxies, `srgb?` fog colour, stale refraction, tangents, which volume) | 2 | client screenshots and RenderDoc (12 items) |
| weapons | Q1 random generator | 3 | the generator body (spec session) |
| weapons | Q2 left: the airborne inaccuracy target | 1 | `m_fAccuracyPenalty` through a long fall |
| weapons | Q3 left: snipers' recoil, silenced and burst modes | 1 | `recoilall.py` with those modes |
| weapons | Q6 hitbox reach outside the hull | 1 | M12 shots past the hull |
| weapons | Q7 left: penetration budget, player cost, other materials and ammo distances | 1 | M13 with more walls |
| weapons | Q11 left: shotgun reload (M9) | 1 | clip per tick during M3/XM1014 reloads (done, Part B) |
| weapons | Q12 left: pellet pattern, line or box traces | 1 | `bullet_impact` per pellet on a far wall, by seed |
| weapons | Q13 left: which weapon drops at death, dropped weapons' lifetime | 1 | death drops done (Part B); lifetime: watch a dropped gun for a round |

## Specs: specs/source

| Spec | Question | Class | What would answer it |
|---|---|---|---|
| breakables | Q6 gib gravity | 2 | client |
| breakables | Q7 de_nuke vent identification | 1 | our screenshot at those origins (no CS:S needed) |
| breakables | Q8 glass pane render details | 3 | client glass code |
| breakables | Q9 gib bounce sounds | 2 | client trace (gibs are client-side) |
| doors_buttons | Q1 CS:S use search distances | 1 | a spawned func_button, bot +use at 79/81 |
| doors_buttons | Q2 use deny sound | 2 | client audio |
| doors_buttons | Q3 use trace mask through grates | 1 | a spawned button behind a grate |
| doors_buttons | Q4 ride timing on elevators | 1 | a bot's z per tick on a mover |
| doors_buttons | Q5 func_tracktrain OnNextPoint per tick | 1 | spawned train and math_counter |
| doors_buttons | Q6 door move sound timing | 1 | sound hook on a stock func_door |
| doors_buttons | Q7 rotating door on its plane | 1 | probe placement |
| doors_buttons | Q9 touch-open door under a player | 1 | spawned func_door flag 1024 |
| entity_io | Q1 left: classes absent from stock maps | 1 | spawned entities across a restart |
| entity_io | Q3 delay at an exact tick multiple | 1 | relays with 0.015/0.03/0.3 delays |
| entity_io | Q4 Kill then input the same tick | 1 | a relay pair |
| entity_io | Q5 `sv_allow_point_servercommand` | 1 | `cvarlist sv_allow` (done, Part B: "always"); left: what maps send (the sweep) |
| entity_io | Q6 `cl_restrict_server_commands` | 2 | client console |
| entity_io | Q7, Q8 game_text scan-out and channels | 2 | client screenshots |
| entity_io | Q9 game_text newline | 1 | sweep the cached maps' entity lumps |
| entity_io | Q10 tracktrain and I/O `Use` | 1 | spawned train |
| fire | Q1 client fire visuals | 2 | client |
| fire | Q2 burn damage to players (armour, score) | 1 | a bot in dust2's A fire after a bomb |
| fire | Q3 player ignition | 1 | `Ignite` input on a bot |
| fire | Q4 fires at a round restart | 1 | `mashup_ents` before and after (env_fire is re-created: entity_io Q1) |
| fire | Q5 floors on de_dust and de_train | 1 | fire positions after their bombs |
| fire | Q6 burning gas can's radius damage | 1 | a bot against a burning can |
| fire | Q7 burning-prop light | 2 | client |
| fire | Q9 buried fires never damage | 1 | a bot in each after the bomb |
| fire | Q10 particle operators, colour fade | 2 | client |
| fire | Q11 fire sound level | 2 | client |
| game_entities | Q1 lagged movement (player_speedmod) | 1 | spawned player_speedmod, movement probe |
| game_entities | Q2 speedmod without an activator | 1 | spawned |
| game_entities | Q3 disabled buttons after respawn (HUD: client) | 1, 2 | spawned speedmod; HUD on the client |
| game_entities | Q4 game_ui "on train" flag | 1 | spawned game_ui |
| game_entities | Q5 key-hint panel | 2 | client |
| game_entities | Q6 env_fade modulate | 2 | client |
| game_entities | Q7 left: env_explosion against armour and friendly fire (the falloff is measured) | 1 | spawned env_explosion near an armoured bot |
| game_entities | Q8 game_score and team score | 1 | spawned game_score, team score netprop |
| game_entities | Q9 conveyor and physics props | 1 | spawned func_conveyor |
| game_entities | Q10 fades at round restart | 2 | client |
| particles_and_smoke | Q1, Q2, Q3, Q5, Q7, Q9, Q10, Q11 (filters, manifests, redefinition, kill, light, skybox, emittime 0, quantisation) | 2 | client test maps (8 items) |
| particles_and_smoke | Q4 operator semantics | 3 | its own spec |
| particles_and_smoke | Q6 unknown inputs logged | 1 | `developer 2` on the server, `ent_fire` |
| particles_and_smoke | Q8 env_smokestack Hammer defaults | 3 | the FGD |
| physics_brushes | Q1 physbox mass shrunk or not | 1 | a community map's physbox mass from its lump |
| physics_brushes | Q2 inertia and damping laws | 1 | as physics_props Q1, Q3 |
| physics_brushes | Q3 controller units | 1 | spawned phys_thruster |
| physics_brushes | Q4 +use pickup of a physbox | 1 | bot +use on a spawned physbox |
| physics_brushes | Q5 physbox_multiplayer on the client | 2 | client |
| physics_brushes | Q6 players and a map prop_ragdoll | 1 | spawned prop_ragdoll |
| physics_brushes | Q7 prop_ragdoll without a ragdoll | 1 | spawned |
| physics_brushes | Q8 thin-rod check on rotated brushes | 1 | spawned |
| physics_constraints | Q1-Q10 (break limit, motor units, hinge friction, spring law, deleted body, world fallback, equal limits, pulley, never-created joint, tie order) | 1 | spawned constraints, positions per frame (10 items) |
| prop_damage | Q4, Q7, Q8 and the client halves of Q2, Q9 | 2 | capture C29 |
| prop_damage | Q6 left: why the landing crush only at 1450-1500 in/s | 1 | more landing speeds |
| triggers | Q1 face contact | 1 | spawned trigger, bot placed at −16.0/−15.9 |
| triggers | Q2 swept boxes for carried players | 1 | spawned fast func_door through a thin trigger |
| triggers | Q3 touch order of overlapping triggers | 1 | spawned push + teleport |
| triggers | Q4 trigger_hurt armour, fractions, knockback | 1 | spawned trigger_hurt, armoured bot |
| triggers | Q5 gravity reset on respawn | 1 | spawned trigger_gravity |
| triggers | Q6 teleport flags 32 and 4096 | 1 | spawned trigger_teleport with a bot |
| triggers | Q7 trigger_push base velocity | 1 | spawned trigger_push, movement probe |
| triggers | Q8 first hurt pass timing | 1 | measured on de_port's (tick 4 after entering): the touch tick is still open |
| triggers | Q9 AddOutput basevelocity | 1 | spawned trigger_multiple with that output |
| viewcontrol_and_templates | Q1 HUD and view model under a camera | 2 | client |
| viewcontrol_and_templates | Q2 view reset with two players | 4 | two clients |
| viewcontrol_and_templates | Q3 camera path | 1 | spawned point_viewcontrol, origin per tick |
| viewcontrol_and_templates | Q4 ForceSpawnAtEntityOrigin | 1 | `ent_fire` on a spawned point_template |
| viewcontrol_and_templates | Q5 output separator in community maps | 1 | the sweep's entity lumps |
| viewcontrol_and_templates | Q6 instance counter start | 1 | one ForceSpawn, `mashup_ents` |
| viewcontrol_and_templates | Q7 templated logic_auto | 1 | spawned template |
| visual_entities | Q1, Q2, Q4-Q10 (beam width, dlight falloff, overrides, pixel visibility, trail repeat, steam roll, spark wrap, spotlight parent, scroll quirk) | 2 | client screenshots (9 items) |
| visual_entities | Q3 env_laser damage against armour | 1 | spawned env_laser through an armoured bot |

## parity-gaps.md rows marked differs or missing

| Row | Class | What would answer it |
|---|---|---|
| Surf ramps (rampbug patched out) | 3 | our choice |
| Base velocity (conveyors, currents) | 1 | movement Q15 |
| Camera under a low ceiling | 3 | other work (mg_jacks) |
| Tick rate 66.67 vs community 100 | 3 | a choice (CS:S servers always run 0.015 s) |
| Spread generator | 3 | weapons Q1 |
| Other automatics' recoil, scoped/silenced/burst | 1 | weapons Q3 |
| Penetration materials | 1 | weapons Q7 |
| Unmeasured gun values | 1 | M13 for buckshot and long reaches |
| Grenade redraw, pin rules | 1, 2 | Q3 done (Part B, as ours); Q2 needs a human |
| `cl_autowepswitch` while firing | 2 | a human client's userinfo |
| Knife idle timing | 2 | client view model |
| Flashlight | 1, 2 | `mp_flashlight` default is measured (0, C9); the look: client |
| Distance falloff curves | 1, 2 | sounds Q1 |
| Room DSP presets | 2 | client |
| `dsp_player`, `soundmixer` | 2 | sounds Q14 |
| Wave choice | 1 | sounds Q2 |
| ambient_generic levels | 2 | sounds Q11 |
| Scrapes | 1 | sounds Q8 |
| Drowning sounds and reset | 2 | client (the server plays none, measured) |
| Spray, night vision, flashlight sounds | 2 | C24 |
| Radar look | 2 | client |
| Scoreboard spacing, dead rows | 2 | client |
| Hint panel messages | 2 | C26 |
| Console look | 3 | a choice |
| `con_enable`, `volume` defaults | 3 | a choice |
| Options apply at once | 3 | a choice |
| `impulse 100` | 1, 2 | as the flashlight |
| `impulse 201`, Import Spray | 2 | client dialog |
| `record`, `stop`, `demoui` | 3 | ours to build |
| `commandmenu`, `showbriefing`, `cheer` | 2 | client |
| `cl_minmodels` | 3 | ours to build |
| Stats, achievements dialogs | 3 | ours to build |
| Impact effects beyond the shooter's PAS | 3 | network relevance (ours) |
| Knife slash mark | 2 | client screenshot |
| HDR | 2 | C17 |
| Player shadows | 2 | RenderDoc |
| World reflections | 3 | per-vertex tangents (shaders Q5) |
| Water refraction | 2 | water spec questions |
| Particles blend in linear space | 2 | client |
| Glow occlusion by rays | 3 | engine |
| LOD switch metric | 2 | C21 |
| Detail props layout | 2 | C22 |
| Player animation | 2 | C14 |
| Ragdoll push per weapon | 2 | C13 |
| Muzzle flash, view-model lighting | 2 | C15 |
| Prop light probes | 2, 3 | C28; the weighting is engine code |
| 3D skybox haze, sky down face | 2 | C10, C20 |
| 3D skybox fog distances | 2 | C10 |
| Static props lit in the 3D skybox | 2 | C10 |
| Blood decals on characters | 3 | a spec, then a client capture |
| Motion blur option | 3 | a spec |
| Breakables' damage types, broken-window drawing | 3 | breakables Q8 |
| Pushers | 1 | doors_buttons Q4, triggers Q2 |
| Physics shadow controller | 1 | physics_props Q7, Q12 |
| Prop damage left-outs | 1, 2 | prop_damage Q6; C29 |
| Steam heat-wave jets | 3 | no stock map; a spec |
| Community-map entity readings | 1 | the triggers and entity_io questions |
| Water and physics (buoyancy) | 1 | props floating in de_nuke's pool |
| LOD and fading of props | 2 | client |
| Model doors beyond the spec | 1 | doors_buttons questions |
| Bomb and hostage details | 1 | objectives Q5, Q10, Q14 |
| Draw rules on one-sided maps | 5 | measured (objectives.md Q15: a draw at once, as ours) |
| Movers mispredicted | 3 | ours (network) |
| `cl_cmdrate` | 3 | a choice |
| Demos | 3 | ours to build |
| Votes | 4 | C25 |

## Answered but not marked (class 5, fixed with this list)

- grenades.md Q1, Q4, Q5, Q8, Q10 (measured in "CS:S values"; the open
  questions now say so), and the measured parts of Q12, Q15, Q17.
- objectives.md Q3 (the blast, measured 2026-10-15) and Q4's server half
  (the server emits no beep).
- weapons.md Q12 (one `bullet_impact` per pellet, M16) and Q13 (the drop,
  pickup delay and reach, M20) in part.
- sounds.md Q3 and Q4 (the footstep bands and the jump step, "Footsteps").
- physics_props.md Q11 (ammo impulses, weapons.md Q8).
- movement.md Q6 (the air-duck shift: 8.5 units, item 5), Q9 (no delay),
  Q13 and Q14 (ladder climbing matches tick-exact in the movecmp fuzz),
  Q16 in part (the same fuzz), Q17 (CS:S servers ignore `-tickrate`:
  always 0.015 s ticks).
- entity_io.md Q2 (OnNewGame fires at a dedicated server's map load:
  measured with Q1).
- fire.md Q8 (CS:S's tick is always 0.015 s).
- sprites_dust.md Q4 (de_dust2's glows render in game: the spec says so).
- parity-gaps.md: "Draw rules on one-sided maps" (objectives.md Q15).

## Part B: class-1 items measured (2026-10-17)

In order of player impact. Method and numbers are in each spec, marked
measured.

| Question | CS:S answer | Ours before | Change |
|---|---|---|---|
| objectives Q5: when the round's bonuses are paid | as the next round starts (5 s after the end, a tick before the respawn) | at the round's end | paid at the next round's start (`RoundState::payouts`) |
| objectives Q5: planter's and defuser's reward | none | planter +300 | none |
| objectives Q5: bomb defused | CTs +3250 | +3500 | 3250 (`win_defused`) |
| objectives Q5: time-out losers still alive | nothing (Ts on Target_Saved, CTs on Hostages_Not_Rescued; the dead get the loss bonus) | the loss bonus | nothing |
| objectives Q5: hostage map wins | elimination 3000, all rescued 2500, not rescued 3250; plus 150 a living hostage, and for CT winners 100 a led one and 850 a rescued one | 3250 / 3500 / 3250 flat | as CS:S (`hostage_*_bonus`) |
| objectives Q5: hostage touch and rescue | +150 to the first leader of each hostage, +1000 to the rescuer | the same | none |
| objectives Q5: hurting and killing a hostage | −20 a whole point; the killing hit −40 a point −500 | −1500 a kill, nothing for a hurt | as CS:S |
| grenades Q3: switching with the pin pulled | allowed at once, nothing thrown, grenade kept | the same | none |
| grenades Q17 / weapons Q13: what the dead drop | the best gun whatever was held, and one grenade: HE, else flashbang, else smoke | the weapon in hand (or the lowest slot) | as CS:S (`drop::DeathDrop`, data `death_drop`) |
| weapons M9: shotgun reload | 0.5 s start, 0.5 s a shell (68 ticks to the first, 35 each after), a shot stops it | 0.45 s a shell (64, then 31) | 0.5 s a shell |
| fall_damage Q2: armour against de_port's trigger_hurt | takes nothing (health 25 a bite, armour 100 kept) | the same | none |
| fall_damage Q3: trigger_hurt timer | one per trigger; a newcomer waits for its next bite | the same | none |
| fall_damage Q4: `Player.FallDamage` | on every landing that takes health (with a step at 1.0), none otherwise (a step at 0.85) | the same | none |
| entity_io Q5: `sv_allow_point_servercommand` | exists, default "always"; a spawned one's `say` ran | our allowlist (a security choice) | none |
| physics_props Q13, Q6: `sv_turbophysics`, `air_density` | 0; 2.00 | not read | none |
| grenades Q7: bounce sounds | one a contact (the smoke's twice on the tick), the stopping contact too, none at rest | one a contact | none |
| grenades Q2: redraw after a throw | inconclusive: the bot ends up with its knife, but its AI picks the knife on its own | redraw of the same grenade | none (needs a human) |
