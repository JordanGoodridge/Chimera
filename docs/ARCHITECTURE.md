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
  |                \
engine_core          source_formats (source/formats/)   formats as plain data: no Bevy
 (engine/core/)       core, console, slots, character, metrics
```

- `source_formats` turns bytes and text into Rust values (glam vectors,
  Source units); it never depends on Bevy, avian or `mashup` (checked).
  Conversion into engine types stays in `mashup` (`games/cs_source`),
  which re-exports the readers at their old paths.
- An edit in `source_formats` rebuilds it and `mashup`; `cargo test -p
  source_formats` builds and runs only its own tests (seconds).
- `engine_core` is the engine's stable base: the modules every layer
  uses and that rarely change (`core` but the build's `VERSION`,
  `console`, `slots`, `character`, `metrics`). It never depends on
  `mashup` or a game (checked); `mashup` re-exports its modules at their
  old paths (`mashup::core` adds `VERSION`, stamped in the app's crate
  so a commit doesn't rebuild the base). The layering table covers its
  modules too. `cargo test -p engine_core --features dev` tests it alone.

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
it); map uses only core. games/<name> uses core, console, slots,
character, movement, mount, map, weapon, objectives and rules (to register
its rule modules); its `ui/` uses those, client and the rest of the
simulation.

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

Drawn-part requests: what the simulation makes during play says in plain
data what it looks like, and `render` fulfils it; the simulation never
builds a mesh or material. A character's team, `BodyName` and class pick
its body, `Held` names what it holds, a `Ragdoll` takes the dead's body
over, `LooseItem`/`ShownItem` name a held model to lie or move as;
a gib carries its `GibLook` (which model of which gib list, skin, glass,
fade distances) and a window its `BrushPanes`; render's systems react to
them (added, changed, removed, without their drawn marker). Without `render` (a dedicated server) the requests stay
unfulfilled and the simulation's parts (bodies, colliders) work alone
(`tests/it/drawn_parts.rs`).

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
- A game's front end (`games/<name>/ui/`: menus, panels, HUD) sits on
  `client` (the shell: input, camera, console UI, VGUI-like widgets, debug
  tools, net glue); the rest of the game never uses it. The shell reaches
  it only through `client::front_end` (hooks the game registers), and the
  app adds it (main.rs). Client files that still name a game:
  `CLIENT_GAME_EXCEPTIONS` (tests/it/architecture.rs).
- Rule modules (`rules::module`): a game registers its modes by name
  (`cs_source:bomb`, ...), picked by the map and cvars; their code is
  still in `rules` and `objectives` (engine-split.md).

## Modules

One line per module or folder, grouped by layer. The detail lives in each
file's `//!` doc comment, which is the source of truth: when code changes
what a file does, change its `//!` doc in the same commit, and touch this
list only when a file is added, removed or changes its job. Keep each
entry to one short line (`tests/it/architecture.rs` checks the length and
that the paths exist).

### Foundations (core, console, slots, character, movement)

- `engine/core/src/core.rs`: the shared vocabulary: `Intent`, velocity and movement state, health and damage, teams, rounds, the tick sets (core)
- `src/core.rs`: `engine_core::core` re-exported, plus the build's `VERSION` (stamped by build.rs) (core)
- `engine/core/src/console.rs`: cvar and command registry, Source-style parsing, aliases, binds, exec and config files, built-ins, cvar scopes (console)
- `engine/core/src/slots.rs`: `MovementRegistry`, `Loadout`, `set_movement` (slots)
- `engine/core/src/character.rs`: `character_bundle`, `spawn_character`, capsule size (character)
- `src/movement/`: `placeholder` (stand-in walking) and `noclip` movement implementations (movement)
- `src/greybox.rs`: a map slot built in code: ramps, crates, a ladder, a water tank (greybox)
- `src/mount/`: the VFS and KeyValues (re-exported from `source_formats`), `mashup.local.toml`, finding game installs (mount)
- `engine/core/src/metrics/`: load-independent performance metrics: per-thread CPU time, hardware counters, distributions, tick metrics (metrics)

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
- `src/map/vis.rs`: visibility data: PVS clusters, area portals, occluders, chunking (map)
- `src/map/merge.rs`: brush entities drawn merged while at their spawn pose (map)
- `src/map/lod.rs`: `MapLod`: a model's lower levels of detail as data (map)
- `src/map/probe_lit.rs`: moving models lit from the baked light field (map)
- `src/map/surface_color.rs`: a surface's colour and light at a hit point, for tinted effects (map)
- `src/map/interp.rs`: drawing between simulation ticks (map)
- `src/map/bodies.rs`: which body and held model a character has, hitboxes (map)
- `src/map/props.rs`: placed props after spawning: looks, sequences (map)
- `src/map/anim.rs`: skeletal animation: `AnimSet`, blending, layers, IK (map)
- `src/map/view_model.rs`: first-person view models' data, animator, events and placement maths (map)
- `src/map/ragdoll.rs`: ragdolls on character models (map)
- `src/map/placed_ragdoll.rs`: map-placed ragdolls (prop_ragdoll) (map)
- `src/map/loose.rs`: loose items: dropped weapons' bodies, shown items, resting on the ground (map)
- `src/map/shells.rs`: spent shells' types and flight step (map)
- `src/map/breakables.rs`: gibs, broken panes and falling pieces: physics, and gibs' looks (map)
- `src/map/prop_physics.rs`: physics props' motion as the logic asks (map)
- `src/map/controllers.rs`: physics controllers and joints the logic switches (map)
- `src/map/buoyancy.rs`: bodies in water float (map)
- `src/map/copies.rs`: copies of map nodes made at run time (map)
- `src/map/monitor.rs`: monitor screen cameras' markers, point_camera fog as a view fog (map)
- `src/map/fish.rs`: func_fish_pool's fish and where they swim next (map)
- `src/map/color_correction.rs`: color_correction entities as data (map)
- `src/map/water.rs`: water materials, surfaces, moving water, the view's water state (map)
- `src/map/particles.rs`: the CPU particle pool (map)
- `src/map/psys.rs`: data-driven particle systems (`.pcf` effects) as data (map)
- `src/map/emitters.rs`: sprite trails, smoke stacks, tesla arcs, embers, muzzle flashes, strikes as data (map)
- `src/map/beams.rs`: beams and glows as data (map)
- `src/map/steam.rs`: steam jets as data (map)
- `src/map/fire.rs`: the fires and burning entities the logic says burn now (map)
- `src/map/tracer.rs`: bullet tracer requests and muzzle points (map)
- `src/map/decal.rs`: runtime decals as data, impacts' lift (map)
- `src/map/rope_material.rs`, `rope.wgsl`: ropes as camera-facing strips (map)
- `src/map/rope_sway.rs`: ropes' settling step and spline (map)
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
- `src/render/light_styles.rs`: switchable and animated light styles at run time (render)
- `src/render/lod.rs`: props' levels of detail as drawn (render)
- `src/render/glow.rs`: glow sprites faded by how much of their proxy is seen (render)
- `src/render/psys.rs`: running and drawing the particle systems (render)
- `src/render/shells.rs`: spent shells thrown, flying and drawn (render)
- `src/render/rope_sway.rs`: ropes in the wind (render)
- `src/render/fish.rs`: func_fish_pool's fish swimming (render)
- `src/render/parts.rs`: drawn parts the logic switches on and off (render)
- `src/render/dust.rs`: dust motes; the effects' dynamic meshes (render)
- `src/render/steam.rs`: steam jets' puffs (render)
- `src/render/tracer.rs`: bullet tracers drawn from the muzzle (render)
- `src/render/emitters.rs`: trails, smoke stacks, teslas, embers, muzzle flashes, strikes through the pool (render)
- `src/render/particles.rs`, `particle.wgsl`: drawing the particle pool (render)
- `src/render/fog.rs`: the scene's fog pushed to effect materials (render)
- `src/render/decal.rs`, `decal.wgsl`: runtime decals projected and drawn (render)
- `src/render/monitor.rs`: func_monitor screens drawn from point_camera views (render)
- `src/render/shadows.rs`, `shadow.wgsl`: dynamic shadows as the shadow-detail setting asks (render)
- `src/render/sky.rs`: the sky on the cameras, the 3D skybox's camera, the sky resources (render)
- `src/render/bodies.rs`: drawn character bodies, held models, a ragdoll's drawn body (render)
- `src/render/breakables.rs`: drawing gibs and windows' unbroken panes (render)
- `src/render/loose.rs`: drawn loose and shown items (render)
- `src/render/view_model.rs`: drawing view models, their screens and muzzle flashes (render)
- `src/render/vis.rs`: visibility culling, areaportal windows' fade, the culling cvars and stats (render)
- `src/render/water.rs`, `water.wgsl`, `screen_warp.wgsl`: drawing water: refraction, reflection, fog, overlays (render)

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
- `src/rules/module.rs`: rule modules a game registers by name; `mashup_rules` (rules)
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
- `src/games/cs_source/rules.rs`: CS:S's rule modules (games)
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
- `src/client/front_end.rs`: how the shell reaches a game's menus: `FrontEnd` hooks, `MenuStatus`, `Panels` (client)
- `src/games/cs_source/ui/`: CS:S's front end on the shell, `CsSourceUiPlugin` (games_ui)
- `src/client/net.rs`: the game's side of the network: joining, dropping, hosting (client)
- `src/client/view.rs`: third-person, free and chase cameras (client)
- `src/client/spectate.rs`: spectating while dead or on the spectator team (client)
- `src/games/cs_source/ui/freeze_cam.rs`: the freeze cam's picture (games_ui)
- `src/client/senses.rs`, `flash.wgsl`: flashes and blasts on the local player's senses (client)
- `src/client/hdr.rs`, `hdr_tonemap.wgsl`: `mat_hdr_level` and the tonemapper (client)
- `src/client/gamma.rs`, `gamma.wgsl`: `mat_monitorgamma` (client)
- `src/client/color_correction.rs`, `color_correction.wgsl`: `mat_colorcorrection` on the finished frame (client)
- `src/client/audio.rs`: the options' sound settings (client)
- `src/client/binds.rs`: every game key is a bind; default binds (client)
- `src/client/video.rs`: field of view, video and texture settings (client)
- `src/games/cs_source/ui/options.rs`: the options dialogs' settings (games_ui)
- `src/client/console.rs`: the in-game console UI (client)
- `src/client/debug_ui.rs`: the debug UI (F8) (client)
- `src/client/debug_views.rs`: gizmo debug overlays: nav, bots, ragdolls (client)
- `src/client/perf.rs`: the `mashup_perf` overlay (client)
- `src/client/frame_metrics.rs`: per-frame metrics of both worlds (client)
- `src/games/cs_source/ui/window_icon.rs`: the window and taskbar icon (games_ui)
- `src/games/cs_source/ui/first_run.rs`: finding CS:S on first run (games_ui)
- `src/client/fonts.rs`: the one font service (client)
- `src/client/widgets.rs`: VGUI control state machines and drawing (client)
- `src/client/vgui.rs`: drawing a game's own menu layouts (client)
- `src/games/cs_source/ui/game_menu/`: the main menu and game menu dialogs (games_ui)
- `src/games/cs_source/ui/server_browser.rs`: the server browser (games_ui)
- `src/games/cs_source/ui/team_menu.rs`: the team menu (games_ui)
- `src/games/cs_source/ui/join_menus.rs`: CS:S's join flow: MOTD, team and class menus (games_ui)
- `src/games/cs_source/ui/buy_menu.rs`: the buy menu (games_ui)
- `src/games/cs_source/ui/radio.rs`: the team radio menus and calls (games_ui)
- `src/client/chat.rs`: the chat area and hint text (client)
- `src/games/cs_source/ui/scoreboard.rs`: the scoreboard (games_ui)
- `src/client/win_panel.rs`: the round's win panel (client)
- `src/games/cs_source/ui/spectator_menu.rs`: the spectator menu (games_ui)
- `src/client/overview.rs`: the spectators' map overview (client)
- `src/client/hud.rs`: the fallback HUD: crosshair, scope, tints, health, ammo, killfeed (client)
- `src/client/game_hud.rs`: the game's own HUD panels (client)
- `src/client/hud_text.rs`, `hud_text.wgsl`: HUD text drawn as Source draws it (client)
- `src/client/hud_sprites.rs`: HUD sprite icons and damage indicators (client)
- `src/client/weapon_select.rs`: weapon selection and pickup history (client)
- `src/games/cs_source/ui/objectives_hud.rs`: bomb, kit, hostage and zone icons, centre prints on the HUD (games_ui)
- `src/games/cs_source/ui/bomb_fx.rs`: what the bomb shows: the code on the C4 screen, the LED glow (games_ui)
- `src/client/radar.rs`, `radar.wgsl`: the radar (client)
- `src/games/cs_source/ui/target_id.rs`: the target ID under the crosshair (games_ui)
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
