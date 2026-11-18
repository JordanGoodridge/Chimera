# Known technical debt

Small, current, honest. Remove entries when fixed.

## Without a file

- map: Glow sprite occlusion casts rays at the world, props and brush entities (colliders), not the depth buffer the game's occlusion query uses: props without a collision model don't hide glows. OK for now: Covers nearly every case; a GPU occlusion query would be exact.
- movement: Placeholder movement has no step-up (stairs block it) and doesn't shrink the collider when crouching. OK for now: It gets replaced by spec-based movement.
- movement: Ground check is one downward shape cast. OK for now: Good enough until a spec defines grounding.
- client: Mouse capture fights the inspector (`chimera_inspector`, F11, releases it; click recaptures). OK for now: Debug-only.
- tests: `no_game_assets_tracked` blocks all .wav/.ogg/.mp3, including our own future sounds. OK for now: Revisit when we have original audio.
- client: Local input overwrites the local player's `Intent` every frame, so remote tools can't drive it. OK for now: Scripted intent playback is planned (docs/OBSERVABILITY.md).
- map: Older maps' ambient light (one cube per leaf: BSP v19, early v20) is sampled at the leaf centre. OK for now: Props light sensibly; the engine's exact use isn't specced.
- map: Textures decode on every load (no cache yet); mips are box-filtered only for VTFs without a full chain. OK for now: dust2 loads in about a second, now in the background for `map`.
- map: 3D skybox content is found by the world's world_mins/world_maxs (computed by the map compiler without the skybox), with a 64-unit margin, not by BSP areas (the sky camera sits in area 0 with solid leaves). OK for now: Correct on dust2.
- map: Sky: the down face's orientation is assumed (never visible in the fitted views). OK for now: Rarely visible on dust2.
- weapon: A dropped gun carries its reserve (as CS:S's measured, M20; we have no per-player ammo pools); a refused touch is retried every tick. OK for now: Matches CS:S in every measured case.
- map, sound: Under water (`audio::hearing::UNDERWATER`): a steady low-pass (600 Hz, 0.8) on everything while the eye is in water; Source switches to its `dsp_water` room preset, engine-side and unmeasured. OK for now: Better than no change at all; capture wanted.
- sound: A prop surface name the surface scripts don't define (dust2's 19 "stone" rock props) steps as "default"; the sound spec doesn't say what the engine does with unknown names. OK for now: Assumed from "default" being every surface's base; check by ear in CS:S.
- map: A `--map` given at startup loads synchronously before the app starts (the `map` command and a joined server's map load and prepare their spawn in the background, `map::prepare`). OK for now: dust2 converts in well under a second.
- deps: `vbsp` pulls `binrw` 0.14, which rustc flags as future-incompatible. OK for now: Upstream; watch on Rust upgrades.
- deps: `bevy_replicon_renet`, `bevy_renet` and `renetcode` come from their authors' repositories (pinned in Cargo.lock): no crates.io release supports Bevy 0.20 yet. OK for now: Back to crates.io once released.
- build: Windows build not yet verified. OK for now: Needs the Windows PC set up (see plans).
