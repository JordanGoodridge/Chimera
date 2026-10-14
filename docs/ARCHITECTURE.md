# Architecture

A map of the code. Design rationale lives in the [README](../README.md); this
file says where things are and which way dependencies may point.

## Crates

A Cargo workspace (docs/plans/active/engine-split.md, "Crates and compile
time"); every member is a default member, so `cargo test` and `cargo
nextest run` at the root test them all (`tests/it/architecture.rs`,
`workspace_crates`).

```
mashup (repository root: src/, tests/it/)   the game, engine and tools
  |
source_formats (source/formats/)            file formats as plain data: no Bevy
```

- `source_formats` turns bytes and text into Rust values (glam vectors,
  Source units); it never depends on Bevy, avian or `mashup` (checked).
  Conversion into engine types stays in `mashup` (`games/cs_source`),
  which re-exports the readers at their old paths.
- An edit in `source_formats` rebuilds it and `mashup`; `cargo test -p
  source_formats` builds and runs only its own tests (seconds).

## Layers

```
            client            (window, input, camera, HUD, debug, agent tools)
              |
   harness    |    games/<name>   (one module per game)
      \       |       /    \
       net ---'      /      \       (net: transport, replication, join handshake)
        |           /        \
       bot   rules   logic  movement   greybox   mount   (mount: VFS, local config)
        |      |       |      |          |
        |  objectives  |      |          |      (objectives: bomb, hostages, zones)
        |      |       |      |          |
        |    weapon    |      |          |      (weapon: parts, inventory, weapon frame)
        |      |      /       |          |      (logic: entity I/O, triggers, movers, +use)
        |     map ---'        |          |      (map: neutral MapData + spawner, sound data)
         \     |       /          /
           character             (components every character has)
               |
             slots               (registries, loadout, swapping)
               |
            console              (cvars, commands, queue; games and client register)
               |
             core                (shared vocabulary)
```

Exact edges: net uses everything the simulation has (core, console,
slots, character, movement, map, weapon, objectives, rules, bot, logic),
and harness and client use net (games/<name> will, to register what they
replicate: add it to their `ALLOWED` entry then); bot uses core, console,
slots, character, map, weapon (grenades
it carries and throws), objectives (planting, defusing); rules uses core,
console, weapon, objectives; objectives uses core, console, slots,
character, map, weapon (games use it too, to fill in their bomb and
hostages); weapon uses core, console, map; logic uses core, console,
map (games feed it through `MapData::entities`; harness and client may use
it); map uses only core.

Presentation sits beside the simulation, above `map`: `audio` (sound
playback, live sounds, room DSP, soundscapes, hearing) uses core and map,
and only `client` and `map_plugin` use it; `render` (drawing what the map
spawns: the drawn phases of spawning a map, tints, detail sprites, sky
occluders, beams, colour correction, levels of detail) uses core and map,
and only `client` and `map_plugin` use it. `map_plugin` builds
`map::MapPlugin` (the type stays in `map`) from map's systems, audio's and
render's, in one fixed order. Materials, culling, water, shadows,
particles, decals, sprites, the sky and view models still live in `map`;
animation evaluation stays on the simulation's side (the server poses
hitboxes); see docs/plans/active/engine-split.md, "Crates and compile
time").

`metrics` (per-thread CPU time and hardware counters, distributions, the
tick's measurements) uses no other module of ours; any layer may use it
(so far harness and client; `SimPlugins` adds its tick plugin).

A module may use only the modules below it. Enforced by
`tests/it/architecture.rs` (`ALLOWED`); update both together.

- `core` never depends on anything in this crate.
- Simulation code (everything except `client`) never depends on `client`, so
  it runs headless in tests and, later, on a dedicated server.
- `games/<a>` never uses `games/<b>`. Shared needs go into `core` once two or
  three games need them (README, "rule of three"), or through a bridge.

## Modules

One line per module or folder, grouped by layer. The detail lives in each
file's `//!` doc comment, which is the source of truth: when code changes
what a file does, change its `//!` doc in the same commit, and touch this
list only when a file is added, removed or changes its job. Keep each
entry to one short line (`tests/it/architecture.rs` checks the length and
that the paths exist).

### Foundations (core, console, slots, character, movement)

- `src/core.rs`: the shared vocabulary: `Intent`, velocity and movement state, health and damage, teams, rounds, the tick sets (core)
- `src/console.rs`: cvar and command registry, Source-style parsing, aliases, binds, exec and config files, built-ins, cvar scopes (console)
- `src/slots.rs`: `MovementRegistry`, `Loadout`, `set_movement` (slots)
- `src/character.rs`: `character_bundle`, `spawn_character`, capsule size (character)
- `src/movement/`: `placeholder` (stand-in walking) and `noclip` movement implementations (movement)
- `src/greybox.rs`: a map slot built in code: ramps, crates, a ladder, a water tank (greybox)
- `src/mount/`: the VFS and KeyValues (re-exported from `source_formats`), `mashup.local.toml`, finding game installs (mount)
- `src/metrics/`: load-independent performance metrics: per-thread CPU time, hardware counters, distributions, tick metrics (metrics)

### Map (neutral map data, spawning, drawing, effects)

- `src/map/mod.rs`: `MapPlugin`, `unload_map`/`change_map`, `MapHooks`, the loaded map's resources, re-exports (map)
- `src/map/data.rs`: `MapData` and everything it holds; plain data (map)
- `src/map/state.rs`: what the logic and replication write and the map and its presentation apply (map)
- `src/map/spawn.rs`: `spawn_map` in fixed phases, the drawn parts through a `MapDrawer` (map)
- `src/map/entities.rs`: `MapEntity`, entity-space helpers, anchors, prop-data keys (map)
- `src/map/loading.rs`: how far a map load has come, for a loading screen (map)
- `src/map/materials.rs`, `images.rs`, `collision.rs`, `sky.rs`: materials, images, colliders and the sky built from map data (map)
- `src/map/world_material.rs`, `world.wgsl`: world surfaces: texture, baked light, bump, cubemap reflections (map)
- `src/map/prop_material.rs`, `prop.wgsl`: props lit by a light probe, plus point lights (map)
- `src/map/material_fx.rs`: what world and prop shaders share: UV transforms, detail modes, self-illumination (map)
- `src/map/fog.rs`, `fog.wgsl`: Source's range and height fog shared by map shaders (map)
- `src/map/vis.rs`: visibility culling: PVS clusters, area portals, occluders, `LogicHidden` (map)
- `src/map/merge.rs`: brush entities drawn merged while at their spawn pose (map)
- `src/map/lod.rs`: `MapLod`: a model's lower levels of detail as data (map)
- `src/map/shadows.rs`, `shadow.wgsl`: dynamic shadows as the shadow-detail setting asks (map)
- `src/map/light_styles.rs`: switchable and animated light styles at run time (map)
- `src/map/probe_lit.rs`: moving models lit from the baked light field (map)
- `src/map/surface_color.rs`: a surface's colour and light at a hit point, for tinted effects (map)
- `src/map/interp.rs`: drawing between simulation ticks (map)
- `src/map/bodies.rs`: character bodies, held models, hitboxes (map)
- `src/map/props.rs`: placed props after spawning: looks, sequences, shadows (map)
- `src/map/anim.rs`: skeletal animation: `AnimSet`, blending, layers, IK (map)
- `src/map/view_model.rs`: first-person view models (map)
- `src/map/ragdoll.rs`: ragdolls on character models (map)
- `src/map/placed_ragdoll.rs`: map-placed ragdolls (prop_ragdoll) (map)
- `src/map/loose.rs`: loose items: dropped weapons, shown items, resting on the ground (map)
- `src/map/shells.rs`: spent shells (map)
- `src/map/breakables.rs`: gibs, broken panes and falling pieces: drawing and physics (map)
- `src/map/prop_physics.rs`: physics props' motion as the logic asks (map)
- `src/map/controllers.rs`: physics controllers and joints the logic switches (map)
- `src/map/buoyancy.rs`: bodies in water float (map)
- `src/map/copies.rs`: copies of map nodes made at run time (map)
- `src/map/monitor.rs`: func_monitor screens and point_camera views (map)
- `src/map/fish.rs`: func_fish_pool's fish (map)
- `src/map/color_correction.rs`: color_correction entities as data (map)
- `src/map/water.rs`, `water.wgsl`: water surfaces (map)
- `src/map/particles.rs`: the CPU particle pool and its drawing (map)
- `src/map/psys.rs`: data-driven particle systems (`.pcf` effects) (map)
- `src/map/emitters.rs`: sprite trails, smoke stacks, tesla arcs, embers, muzzle flashes through the particle pool (map)
- `src/map/beams.rs`: beams and glows as data (map)
- `src/map/steam.rs`: steam jets (map)
- `src/map/fire.rs`: the fires and burning entities the logic says burn now (map)
- `src/map/tracer.rs`: bullet tracers (map)
- `src/map/decal.rs`, `decal.wgsl`: runtime decals (map)
- `src/map/rope_material.rs`, `rope.wgsl`: ropes as camera-facing strips (map)
- `src/map/rope_sway.rs`: ropes in the wind (map)
- `src/map/nav.rs`: `NavMesh`: areas, links, ladders, hiding spots, A* (map)
- `src/map/sound.rs`: the simulation's side of sounds (map)
- `src/map/radio.rs`: a game's team radio data and chat line format (map)
- `src/map/hud.rs`: a game's own HUD, menus and GameUI look as data (map)
- `src/map_plugin.rs`: builds `map::MapPlugin` from map's, audio's and render's systems (above map, audio, render)

### Render (presentation beside the simulation)

- `src/render/mod.rs`, `src/render/spawn.rs`: the drawn phases of spawning a map (`MapAssets`, a `map::MapDrawer`) (render)
- `src/render/tint.rs`: entities' render colour, alpha and mode; brush texture frames (render)
- `src/render/detail.rs`, `detail.wgsl`: detail sprites (grass and weeds) (render)
- `src/render/sky_occluder.rs`, `sky_occluder.wgsl`: opt-in depth-only sky faces (render)
- `src/render/beams.rs`, `beam.wgsl`: drawing beams, glows and spark bursts (render)
- `src/render/color_correction.rs`: color_correction volumes blended into one table (render)
- `src/render/lod.rs`: props' levels of detail as drawn (render)

### Audio (presentation beside the simulation)

- `src/audio/mod.rs`, `src/audio/sound.rs`: `SoundPlugin`, one-shot playback, the listener (audio)
- `src/audio/live_sound.rs`: long-lived sounds started and stopped by `map::SoundControl` (audio)
- `src/audio/room.rs`: room DSP presets and the shared reverb bus (audio)
- `src/audio/soundscape.rs`: soundscape selection and playback (audio)
- `src/audio/hearing.rs`: blast and flash muffle and ringing, the underwater muffle (audio)

### Simulation above map (logic, weapon, objectives, rules, bot)

- `src/logic/`: map logic for Source-style entities: entity I/O, triggers, movers, breakables, props, fire, the bridge to the world (logic)
- `src/weapon/`: weapons as parts, inventory, the weapon frame, economy, grenades, drops, lag compensation (weapon)
- `src/objectives/`: round objectives: bomb, hostages, zones, the use search (objectives)
- `src/rules.rs`: deathmatch and round rules: death, respawn, rounds, teams, score (rules)
- `src/bot/`: bots: aim, navigation, team plans, radio, buying, grenades, objectives (bot)

### Network (net)

- `src/net/mod.rs`: `NetPlugin`, the protocol, replicated components (net)
- `src/net/server.rs`: the server: listening, the join handshake, update pacing, own states (net)
- `src/net/client.rs`: the client: connecting, joining, the welcome (net)
- `src/net/predict.rs`: the client's command clock and prediction (net)
- `src/net/interp.rs`: others drawn in the past from snapshots (net)
- `src/net/ordered.rs`: replicated values applied in tick order (net)
- `src/net/delta.rs`: byte deltas (net)
- `src/net/quant.rs`: `NetBody`'s quantized wire form (net)
- `src/net/upnp.rs`: UPnP port mapping while hosting, `net_upnp` (net)
- `src/net/movers.rs`: moving brushes over the network (net)
- `src/net/props.rs`: physics props over the network (net)
- `src/net/ragdolls.rs`: map-placed ragdolls over the network (net)
- `src/net/anchors.rs`: placed weapons' anchor nodes over the network (net)
- `src/net/weapons.rs`: held weapons, loose items, grenades and weapon events over the network (net)
- `src/net/game.rs`: the round, scoreboard and game events over the network (net)
- `src/net/chat.rs`: chat and radio over the network (net)
- `src/net/cheats.rs`: cheat commands from clients (net)
- `src/net/cvars.rs`: replicated cvars (net)
- `src/net/mapfx.rs`: map effects for clients (net)
- `src/net/decals.rs`: decals for late joiners (net)
- `src/net/motd.rs`: the server's message of the day (net)
- `src/net/udp.rs`: the UDP transports and fake network conditions (net)
- `src/net/query.rs`: server queries and LAN discovery (net)
- `src/net/memory.rs`: an in-memory transport for `NetSim` (net)

### Games

- `src/games/mod.rs`: the `load_map("game:name")` dispatcher (games)
- `src/games/cs_source/`: Counter-Strike: Source from the install: BSP, materials, models, sounds, movement, weapons, HUD, menus (games)

### Source formats (crate `source_formats`, `source/formats/`)

- `source/formats/src/fs.rs`: the VFS core: `FileSource`, `Mount` (layers in order), `LooseDir`, `normalize` (re-exported by `mount`)
- `source/formats/src/keyvalues.rs`: KeyValues: one tokenizer (`Escapes::On`/`Off`), the `Kv` tree, `parse`/`parse_res`/`parse_pc`
- `source/formats/src/localization.rs`: localization files (UTF-16/UTF-8, their own escapes)
- `source/formats/src/vpk.rs`: VPK archives (v1, v2) as a `FileSource`
- `source/formats/src/pak.rs`: a BSP's pakfile (zip; stored and LZMA entries)
- `source/formats/src/lumps.rs`: LZMA-compressed BSP lumps inflated before anything reads the file
- `source/formats/src/detail.rs`: the `dprp` detail prop lump
- `source/formats/src/vhv.rs`: static props' baked per-vertex light (`.vhv`)
- `source/formats/src/phy.rs`: collision models (`.phy`): solids, ragdolls, break blocks
- `source/formats/src/pcf.rs`: particle files (binary DMX) and particle manifests
- `source/formats/src/nav.rs`: `.nav` file records (linked into a `NavMesh` by `games/cs_source/nav.rs`)
- `source/formats/src/wav.rs`: WAV (PCM, MS-ADPCM) and MP3 decoding to `SoundClip`

### Binaries and the test harness

- `src/lib.rs`: `SimPlugins`, `swap_map` (crate root)
- `src/main.rs`: the game binary (top)
- `src/harness.rs`: `Sim` and `NetSim`: headless apps stepped by exact ticks, for tests (harness)
- `src/bin/mashup_server.rs`: the dedicated server (top)
- `src/bin/mapsweep.rs`: dev tool: load every cached map headless and report problems
- `src/bin/perfgate.rs`: dev tool: instructions-per-tick regression gate
- `src/bin/refcmp.rs`: dev tool: compare views against real CS:S
- `src/bin/tracesum.rs`: dev tool: summarize a Chrome trace
- `src/bin/testmap.rs`: dev tool: write the entity test map
- `src/bin/dump.rs`: dev tool: summarize, list and extract a game install's files

### Client (window, input, camera, HUD, menus, debug)

- `src/client/`: local input, the camera, captures, the plugin that adds the rest (client)
- `src/client/net.rs`: the game's side of the network: joining, dropping, hosting (client)
- `src/client/view.rs`: third-person, free and chase cameras (client)
- `src/client/spectate.rs`: spectating while dead or on the spectator team (client)
- `src/client/freeze_cam.rs`: the freeze cam's picture (client)
- `src/client/senses.rs`, `flash.wgsl`: flashes and blasts on the local player's senses (client)
- `src/client/hdr.rs`, `hdr_tonemap.wgsl`: `mat_hdr_level` and the tonemapper (client)
- `src/client/gamma.rs`, `gamma.wgsl`: `mat_monitorgamma` (client)
- `src/client/color_correction.rs`, `color_correction.wgsl`: `mat_colorcorrection` on the finished frame (client)
- `src/client/audio.rs`: the options' sound settings (client)
- `src/client/binds.rs`: every game key is a bind; default binds (client)
- `src/client/options.rs`: the options dialogs' settings, field of view, video and texture settings (client)
- `src/client/console.rs`: the in-game console UI (client)
- `src/client/debug_ui.rs`: the debug UI (F8) (client)
- `src/client/debug_views.rs`: gizmo debug overlays: nav, bots, ragdolls (client)
- `src/client/perf.rs`: the `mashup_perf` overlay (client)
- `src/client/frame_metrics.rs`: per-frame metrics of both worlds (client)
- `src/client/window_icon.rs`: the window and taskbar icon (client)
- `src/client/first_run.rs`: finding CS:S on first run (client)
- `src/client/fonts.rs`: the one font service (client)
- `src/client/widgets.rs`: VGUI control state machines and drawing (client)
- `src/client/vgui.rs`: drawing a game's own menu layouts (client)
- `src/client/game_menu/`: the main menu and game menu dialogs (client)
- `src/client/server_browser.rs`: the server browser (client)
- `src/client/team_menu.rs`: the team menu (client)
- `src/client/join_menus.rs`: CS:S's join flow: MOTD, team and class menus (client)
- `src/client/buy_menu.rs`: the buy menu (client)
- `src/client/radio.rs`: the team radio menus and calls (client)
- `src/client/chat.rs`: the chat area and hint text (client)
- `src/client/scoreboard.rs`: the scoreboard (client)
- `src/client/win_panel.rs`: the round's win panel (client)
- `src/client/spectator_menu.rs`: the spectator menu (client)
- `src/client/overview.rs`: the spectators' map overview (client)
- `src/client/hud.rs`: the fallback HUD: crosshair, scope, tints, health, ammo, killfeed (client)
- `src/client/game_hud.rs`: the game's own HUD panels (client)
- `src/client/hud_text.rs`, `hud_text.wgsl`: HUD text drawn as Source draws it (client)
- `src/client/hud_sprites.rs`: HUD sprite icons and damage indicators (client)
- `src/client/weapon_select.rs`: weapon selection and pickup history (client)
- `src/client/objectives_hud.rs`: bomb, kit, hostage and zone icons on the HUD (client)
- `src/client/bomb_fx.rs`: what the bomb shows: the code on the C4 screen, the LED glow, the marker (client)
- `src/client/radar.rs`, `radar.wgsl`: the radar (client)
- `src/client/target_id.rs`: the target ID under the crosshair (client)
- `src/client/game_text.rs`: map HUD messages (game_text) (client)
- `src/client/map_screen.rs`: map fades, hints and screen overlays (client)

## Data flow per tick

```
PreUpdate:    (server) UserCmds from clients ──> CommandBuffer (by tick), DropRequest ──> drop;
              (client) replication ──> NetBody, NetMover, NetProp, NetItem and their Snapshots (by server
              tick); ticks heard ──> InterpClock, held snapshots; our NetBody ──> Transform (until
              predicted); OwnState ──> pending; NetHeld ──> others' held weapon; FireBullets ──>
              RemoteShot; WeaponFx, Killed, HitConfirm, Detonation, Senses ──> weapon events, Died,
              Detonated, Blinded, Deafened; NetSmoke ──> SmokeCloud
Update:       keyboard/mouse ──> Intent (local player)
FixedFirst:   (client) drawn transforms ──> the simulation's (map::interp::restore); command tick + SimClock;
              server state vs. predicted: on a mismatch restore it, re-run later commands (core::predict)
FixedUpdate:  bot brains, (server) each player's command for this tick ──> Intent, its view tick ──>
              lagcomp::ViewTick (before the rules, which hold it); (client) local Intent normalized
              (+ the render tick) ──> this tick's NetCmd
              SimSet::Rules     rounds (new round ──> RoundRestarts, objectives::round_start;
                                bomb/hostage state ──> round end; FreezeTime, RoundOpen),
                                respawn; the dead's and (freeze time) everyone's Intent held
              objectives        arming/defusing hold still; hostages follow ──> Intent
              MapControls ──> Intent (taken buttons, at controls, frozen; the buttons kept for game_ui)
              weapon selection/deploy ──> MaxSpeed           (core::Predict::Select)
              RoundRestarts changed ──> logic world re-created, nodes back; gibs, panes reset
              LogicSet::Pre     thinks, movers push/carry characters, +use (Intent.use_key);
                                (client) movers stepped to the command tick, our player carried
                                (net::movers::place, also in each replayed command)
              SimSet::Commands  Intent.command stamped (tick + Seed)
              SimSet::Movement  Intent (+ MaxSpeed) ──> Transform, Velocity, MovementState  (Predict::Movement)
              AnchorSet         placed weapons (loose or carried) ──> EntityAnchors, anchor nodes
              LogicSet::Post    trigger touches (push ──> BaseVelocity, teleport), untouch,
                                event queue (outputs ──> inputs), removals; effects ──> Damage, HUD text
              SimSet::Weapons   Intent + MovementState ──> traces, Damage, WeaponEvent, PlaySound
                                (inaccuracy, frame, recoil, timed sounds, zoom, grenade throws:
                                Predict::Weapons; (server) a remote player's traces against others
                                rewound to its view tick from HitHistory: weapon::lagcomp)
                                then the bomb: arming, plant, defuse (use search), beeps,
                                Explosion, FireEntityOutput; +use on hostages ──> ObjectiveEvent
              LogicSet::Damage  Damage on breakable and prop nodes ──> break, panes, gibs, sounds,
                                Explosion (next tick, in SimSet::Weapons)
              then              Damage ──> Health, Died ──> Score, Dead
                                WeaponEvent, Damage ──> impact sounds, decals, Particles (game effects)
FixedPostUpdate: physics step; new contacts of props ──> impact Damage (crush, next tick)
FixedLast:    (client) Transform, eye, look, punch ──> interpolation samples (last two ticks); predicted
              state ──> PredictionHistory; (server) characters ──> NetBody, NetHeld, OwnState (with
              what they carry); bodies animated from the tick's values (SimAnimator) ──> Hitboxes
              (map::SimPose) ──> HitHistory; movers ──> NetMover; physics props ──> NetProp; loose
              weapons, grenades ──> NetItem
after fixed loop: (client) samples + overstep fraction ──> drawn Transform, RenderedView; others,
              physics props and items (NetDrawn) from their snapshots at the render tick (net::interp,
              net::props, net::weapons)
Update:       (client) RemoteShot ──> others' rounds traced again (no damage) ──> WeaponEvent, PlaySound
PostUpdate:   (server) changes replicated (a mutate message with each client's updates: UpdatePacing),
              OwnStateDelta to each player due one;
              WeaponEvent ──> FireBullets, WeaponFx (to the others), HitConfirm (to the shooter);
              Detonated, Died, Blinded/Deafened ──> Detonation, Killed, Senses;
              (client) new NetCmds + backups ──> UserCmds; drop ──> DropRequest
              PlaySound ──> one-shots; SoundControl ──> LiveSounds (ambient loops) ──> audio
Update:       camera <── Intent (local look) + drawn Transform + RenderedView (eye offset, roll, punch)
              Particles stepped (frame time, at most 0.1 s) and drawn
              DriveAnimation  WeaponEvent ──> Animator (bodies), ViewAnimator (view models) ──> joints
                              view-model animation events ──> ViewModelEvent ──> flash, light, shells
                              Intent + Velocity ──> ViewModelOffset (bob, sway)
```

Intent buttons are levels, not edges. Movement runs at a fixed tick
(`DEFAULT_TICK_HZ` until a spec sets one); rendering draws between the last
two ticks (`map::interp`): gameplay, traces and shots use the simulation's
values, cameras, bodies and their animation the drawn ones.

## Slots

An implementation registers under a namespaced ID (`mashup:noclip`,
`combat_arms:movement`) with a marker component. Its systems act only on
entities with that marker, so swapping is removing one marker and inserting
another (`slots::set_movement`). Only Movement exists so far; the other slots
follow the same pattern when they arrive.
