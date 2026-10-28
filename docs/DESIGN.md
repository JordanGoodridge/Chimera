# Chimera: design

The design behind Chimera and the games built on it: why the layers,
slots, mounts and weapon model look the way they do, and the legal rules
the project keeps. This was the README's content before the README became
the user-facing page; the code map is [ARCHITECTURE.md](ARCHITECTURE.md).

## Goal

Pull a "microcosm" out of an existing game (a map, its movement feel, a few
weapons, its HUD) and make it playable in Lucker Party. Then mix parts freely:

- Combat Arms rules, playing as a Modern Warfare 2 character with MW2 guns.
- A Combat Arms character in a Counter-Strike: Source defusal map.
- A Minecraft character on a CS:S minigame map.
- Smaller swaps: a Minecraft crosshair in CS:S, MapleStory damage numbers in
  Combat Arms.

Element swaps double as Roll the Dice modifiers.

## Vision

Layers, each using only those below it:

```
apps            one faithful app per game (apps/cs_source: CS:S as CS:S),
                mashup apps that mix several games, Lucker Party the rich one
games           each game a library (games/cs_source, later combat_arms, ...):
                its parts (loaders, movement, weapons, rules, HUD, menus)
loaders         a family's formats and shared behaviour (source/: Source's
                files, its map logic)
Chimera         the engine (chimera/): game-agnostic slots, simulation,
                presentation, network, client shell
Bevy, Rust
```

- **Chimera** knows no game. A game fills its slots and hooks (movement,
  weapons, rules, HUD, replicated parts) with data and registered code.
- **Games as libraries.** A game crate is the faithful game's parts; its
  own app runs them as the original (parity first), and any other app can
  take single parts: a Combat Arms character on a CS:S map is an app
  picking two games' parts, never one game reaching into another.
- **Apps on top.** The faithful apps are the parity baselines; mashup apps
  mix games; Lucker Party adds the party (lobby, rotation, minigames as
  data). Later, users mash games up themselves: minigame definitions pick
  parts by id at run time, no rebuild.
- **Performance holds as games are added**: a game costs only what an app
  uses of it (crates and features), the simulation runs without the
  presentation (the dedicated server), and tools sit at the lowest layer
  whose knowledge they need.

docs/ARCHITECTURE.md ("Crates") has the crates; the plan is
docs/plans/active/engine-split.md.

## Scope for now

- First-person games only.
- First pair: Counter-Strike: Source, then Combat Arms. They are similar enough
  to test the shared core without fighting it, and different enough to expose
  CS-shaped assumptions.
- A third game (MW2 or Minecraft) is the next test of the interfaces.

Out of scope for now: 2D games such as MapleStory. Bringing 3D content into 2D
is workable (side-locked 3D or sprites rendered from 3D models); 2D into 3D
needs information the source does not have and is a gimmick at best.

## Engine direction

Leaning Bevy (Rust) for this prototype. The idea is essentially "an ECS with a
plugin per game", which is what Bevy is. Rust also has Source format parsers
(`vbsp`, `vmdl`, `vtf`, `vpk`). Known costs: Bevy API churn, immature UI, no
editor (Blender-as-editor tooling partly covers it). The editor matters less
here because imported content comes from importers, not hand placement.

## Architecture

### Mount layer

One adapter per game. It finds the player's install, reads its archives, and
converts assets into neutral data: mesh, skeleton, animation, texture,
material, sound, map, sprite, font, UI layout. Each adapter also owns the
game's units and coordinate conventions (a Source unit is about an inch; a
Minecraft block is a meter; some games are Z-up).

The engine only ever sees neutral data.

Mounts target the official, current release of each game, never a specific old
or modified build: users can't legitimately obtain old builds, and asking them
to undermines the "load from your own install" model. To survive game updates:

- Parse at the format level, not the build level. Formats change far less
  often than content.
- Read assets straight from the game's own archives at runtime and convert
  only what is actually loaded. Results that are expensive to produce
  (decrypting and parsing a world, BSP to meshes) go into a local cache.
- The cache is content-addressed: entries are keyed by a hash of the source
  file's bytes plus our converter version, so a game update only re-converts
  files whose bytes changed, and unchanged files are shared across builds.
  Each build has a small manifest (asset ID to hash); entries no current
  manifest references are deleted, and the cache has a size cap.
- A "mount doctor" per game checks expected files and reports what is mounted,
  what changed and what failed. The multiplayer pre-match check uses it too.

Supported sources are the Steam releases. An install is identified by its
Steam build ID plus file hashes. That identifies which build is installed
(the mount doctor warns on builds we haven't tested); it never names cache
folders, so an update doesn't duplicate anything.

Where imported data lives:

- Converted files are still the publisher's assets. They follow the same rule
  as the originals: never in git, never in a distributed build.
- The cache is per user, outside the repository:
  `~/.local/share/chimera/cache/` on Linux, `%LOCALAPPDATA%\chimera\cache\` on
  Windows.
- Development also has a `dump` tool that extracts an install's raw files into
  a local folder outside the repo, for exploring formats and writing specs.
  It shares the archive readers with the runtime mount; the game itself never
  needs a full dump.
- Moving assets between our own machines means copying the install or the
  cache directly (rsync, Syncthing), never through git.
- The game finds the CS:S install itself: Steam's library folders
  (`libraryfolders.vdf`, the app's manifest, which also gives the build ID
  for the mount doctor). If it can't, a first-run dialog asks for the
  folder and remembers it in the user's settings file (outside the
  repository); `chimera_install` in the console shows or changes it. A path
  in the gitignored `chimera.local.toml` (see `chimera.local.example.toml`)
  always wins; that file also holds the Combat Arms keys.
- Tests that need a real install skip when it isn't present.
- Our own original assets (greybox textures, our sounds, UI) may be committed,
  with Git LFS if they get large.

How code refers to assets:

- Game code uses stable namespaced IDs we own (`combat_arms:weapon/m4a1`),
  never archive paths.
- The importer builds the catalog mapping each ID to where the asset lives in
  this install, following the game's own data (a weapon's data file names its
  model and sounds) rather than hard-coded paths.
- When a release moves something, a committed per-fingerprint alias file
  records it; content hashes catch pure renames. File names and hashes are
  facts about the game and may be committed; contents may not.
- Importing writes a catalog snapshot (IDs, source paths, hashes; no content)
  that we commit, so a game update shows up as a reviewable diff.

### Slots

A match is a loadout that picks one implementation per slot. Each game
contributes implementations as a plugin.

| Slot | Contents | Examples |
|---|---|---|
| Map | Geometry, collision, lighting, spawns, objectives | de_dust2, a Combat Arms map |
| Ruleset | Rounds, win conditions, economy, respawns | CS defusal, CA elimination |
| Movement | Speeds, acceleration, jump, crouch, sprint, air control | Source, MW2 |
| Character body | Third-person model, skeleton, animations | CA soldier, MW2 operator, Steve |
| Weapon | Trigger/cost/delivery/effect plus feel | CS AK-47, MW2 M4 |
| Viewmodel | First-person hands and gun, reload/draw/fire animations | MW2 hands with M4 |
| HUD pieces | Crosshair, health, ammo, killfeed, damage numbers | CS crosshair, MapleStory numbers |
| Audio | Footsteps, gunshots, announcer, hit sounds | CA announcer |

Slots can get more granular as needed (the HUD row already is).

### Intent

Nothing reads the keyboard directly. Local input, bot brains and (later) the
network all write the same `Intent` component: move direction, look, jump,
crouch, sprint, fire, reload. The Movement slot turns intent into motion and
publishes movement state; weapons read intent plus that state. A bot is just
another intent source, so it uses exactly the same movement and weapons as a
player, and this matches the multiplayer model (clients submit intent).

### Interfaces

Each slot declares what it provides and what it needs, so any implementation
that satisfies the contract can be swapped in:

- A crosshair needs the current spread and aim state. Any weapon providing
  spread works with any crosshair.
- Movement provides state (sprinting, crouching, airborne, landing). Weapons
  read it, so "can't fire while sprinting" is a weapon rule, not a hard link
  between MW2 movement and MW2 guns.
- A character body maps its animations onto a standard action set (run,
  crouch, jump, aim, reload, die). That mapping is the retargeting work and
  probably the hardest single piece.
- A ruleset declares required capabilities (defusal needs bomb sites and a
  plant/defuse interaction). Loadouts are validated before a match starts;
  invalid combinations fail cleanly or use a declared fallback.

### Bundles

Some parts belong together: MW2's M4 model, reload animation, hands and fire
sound. Pick the bundle by default; allow overriding individual pieces for
chaos combinations.

### Weapons and items

"Weapon" is not a type. Any use action is four composable parts:

| Part | Question | Examples |
|---|---|---|
| Trigger | How is it activated? | primary/secondary, hold, charge, tap, toggle |
| Cost | What does it consume? | magazine and ammo type, energy, mana, durability, cooldown, nothing |
| Delivery | How does it reach things? | hitscan, projectile, beam, melee swing, area, self |
| Effect | What happens on hit? | damage, break block, knockback, stun, heal, ignite |

Examples: CS AK-47 is primary, magazine, hitscan with spread, damage. A
Minecraft pickaxe is primary, durability, melee swing, break block plus small
damage.

Each host declares which effects it supports. Unsupported parts do nothing or
go through a bridge (a pickaxe in CS:S deals melee damage; mining has no
blocks to act on unless a bridge maps it to doors and glass).

Feel that doesn't decompose (CS recoil patterns, MW2 aim-down-sights) lives in
per-game components that travel with the weapon.

Prior art: Minecraft's item data components; Unreal's Gameplay Ability System.

### Components and categorization

- Core vocabulary: Transform, Velocity, Health, Team, Hitbox, Inventory,
  Weapon parts. Small and normalized.
- Per-game components are namespaced (`cs_source::RecoilPattern`,
  `mw2::Perk`).
- Bridges translate between per-game and core components.
- Rule of three: a concept moves into the core only after two or three games
  need it. A slot interface is not proven until two games implement it.
- Things are described with tags, not a class hierarchy (an AWP is `weapon`,
  `ranged`, `hitscan`, `scoped`).
- Every imported item keeps a namespaced ID and its source path
  (`cs_source:weapons/awp` from `scripts/weapon_awp.txt`).
- The catalog is generated by importers, never hand-edited, so changing the
  categorization means changing importer code and regenerating.
- LLM help is used offline for bulk tagging and proposing normalized values;
  the rules are saved as importer code and spot-checked by a human.

### Normalization and mixing rules

- The host game's rules win; imported parts adapt to the host.
- Shared quantities are normalized: health and damage as a fraction of a
  standard player's health, speed relative to the host's walk speed, fire rate
  in seconds.
- Cross-game questions (whose sprint rules apply, what an MW2 perk does to
  Steve) are design decisions, made explicit as bridges or as ruleset options.
  Many make good dice modifiers.
- Cross-game kits (a CS:S character's moveset in another game) are generated
  ahead of time as data, reviewed and playtested, never generated during a
  match. A runtime "chaos kit" could exist later as a server-generated mode.

### Impacts across games

Any game's gun works on any game's map; where a game lacks something,
Chimera has a generic fallback (`chimera_world::map::impact`,
`chimera_sim::weapon::impact`):

- **Impacts.** Surfaces map onto common classes (`SurfaceClass`:
  concrete, metal, wood, dirt, sand, grass, snow, glass, tile, plastic,
  cloth, flesh, water), each game registering how its names go to and
  from them. A map lists whose impact looks it carries (its loader's game;
  a guest's dressing adds its own). An impact is drawn by the gun's game
  when its looks are on the map, else the map's game, else any game whose
  looks are there, else Chimera's generic mark and puff; each game's
  impact code asks this route and draws only its own.
- **Pushes.** Every game's bullets tell a common `BulletImpulse` (N·s at
  the hit point; each game converts its own numbers: CS:S's measured
  damage force, Combat Arms' damage × 2 N·s per hit point), which Chimera
  gives to dynamic bodies times `phys_pushscale`.
- **Recoil.** The view punch is shared, the rules per game; `RecoilGame`
  names the game of the held weapon, and a switch to another game's
  weapon zeroes the punch: the old game's kick doesn't keep returning
  into the new game's view.

### Hits across games

Any game's gun hits any game's body by one game-agnostic interface
(`chimera_core::core`, `chimera_sim::weapon::hits`); nothing is written
per gun-body pair. Plain CS:S and plain Combat Arms keep their exact
numbers (pinned by `parity_pin_*` tests in each game).

- **Regions.** `BodyRegion` is the superset of what the games tell apart
  (head, neck, chest, stomach, groin, upper arm, forearm, hand, thigh,
  shin, foot by side, generic); `hitgroup()` gives Source's coarser
  hitgroup that `Damage` and everything downstream carry, `part()` the
  part armour covers (head, torso, arm, leg, groin).
- **Hitboxes, from the body's game.** Each body fills `Hitboxes` in its
  own game's way, converted into Chimera's shapes (boxes or spheres with
  region, priority and the body's own damage scale): CS:S from the
  model's hitbox set (regions by bone), Combat Arms its skeleton's hit
  spheres posed every tick. Lag compensation keeps them
  (`weapon::lagcomp::HitHistory`). A gun's rule (CS:S's nearest box
  entered, Combat Arms' client test and server walk) asks only shape
  queries (`ray_span`, `segment_passes`/`power`, `line_passes`), so it
  works on any body.
- **Damage per region, from the gun's game.** A gun's table scales by
  region (CS:S's five hitgroup multipliers; Combat Arms' zone factor, the
  body's own scale where its shape has one, else its male skeleton's).
- **Armour: one value, coverage by identity.** Every character has at
  most one `weapon::Armor` (normalized points, 1.0 = 100). What covers
  it is its game's: the default kit (CS:S's kevlar: torso, arms and
  groin half, every damage class; the helmet the head), or a game's own
  pieces worn with it (`weapon::ArmorPieces`: Combat Arms' gear, its
  protection fractions per damage class, used up when consumable). Every
  gun's hit splits between armour and health through one step,
  `hits::absorb`: P, the part's coverage for the damage class, lets
  `(1 - 2P) + P x ratio` through (the gun's armour ratio: CS:S's
  `WeaponArmorRatio`, Combat Arms' guns 1), armour loses the rest times
  the pieces' wear, and running out is the armour's rule (CS:S by what
  it would lose, Combat Arms by what it blocks). So a Combat Arms round
  on CS:S kevlar is halved, a CS:S AK-47 on Combat Arms' body gear loses
  0.15 x 0.45 of itself.
- **Armour when the identity switches** (the party apps, where a
  character's game follows the held gun): the armour value stays and
  what covers it follows the identity. A game gives armour only to its
  own characters (Combat Arms its default 100 points at the first life;
  CS:S by buying), puts its pieces on a character that becomes its own
  and takes them off one that becomes another game's; a character
  without pieces is covered by the default kit. So a CS:S-gun player's
  armour is kevlar by CS:S's rules and shows on CS:S's HUD, a Combat
  Arms-gun player's the same points as Combat Arms' gear on its HUD,
  and nobody carries armour their HUD doesn't show. (Combat Arms puts on
  its default gear at a switch, not a loadout's.)

### Announcers

Kill-streak and special-kill callouts (Combat Arms' DOUBLE KILL ...
UNBELIEVABLE+, FIRST KILL, HEADSHOT, NUTSHOT, REVENGE) are Chimera's, not
a game's HUD's (specs/combat_arms/kill_streaks.md: Part A the rules,
Part B Combat Arms' pack).

- **Counting is the server's, for any gun on any body**
  (`chimera_sim::announce`): from every enemy kill, the killer's
  multi-kill count (the pack's window), the round's first kill, the
  killing hit's place (head; groin, from the gun's or knife's full
  `BodyRegion`, `core::RegionHit`, since `Damage`'s hitgroup reads a groin as the
  stomach), revenge and the killer's body (`core::FemaleBodies`). These
  `KillFacts` travel with the kill (`net::Killed`); every client decides
  what to show from them: one event per kill by priority (first kill,
  revenge, then at count 1 a headshot or groin shot, else the step), at
  most one voice (the event's, else headshot, else groin by the killer's
  body), the skull or nut over a step, first kill or revenge. Which
  priority levels, art and voices is the pack's data.
- **An announcer pack is data** (`chimera_core::announcer::AnnouncerPack`,
  a TOML document): step names and thresholds, art and voice files (paths
  in the pack's game's install, loaded at run time), layout on its
  reference screen, the banner animation's timings, priority levels,
  weapon skins. A game contributes its pack as one of its parts
  (`GameParts::announcer`; Combat Arms' is `data/announcer.toml`).
- **The shell draws it** (`chimera_client::client::announcer`), above
  whichever HUD is up, so in the party apps a CS:S-identity player gets
  the same banner and voice as a Combat Arms one.
- **The server picks the pack**: `sv_announcer` (replicated) is
  `default` (the first registered: Combat Arms' wherever Combat Arms is
  composed, so the party apps and plain Combat Arms; none in plain CS:S,
  which has no pack), `none`, `random` (one per map) or a pack's id. More
  packs (a Quake-style one) are more data files; a minigame could set the
  cvar.
- **A minigame's moments are callouts too**: the gun game's rung up or
  down, the last rung (yours, and another player's to everyone), the
  lead taken, lost or tied, and the win are `MinigameEvent`s the server
  sends; each client's minigame HUD data turns the local player's into a
  `Callout` (`chimera_sim::minigame::hud`; text from the rules module,
  `RulesModule::callout`), which the shell draws centre screen on every
  HUD (`chimera_client::client::callouts`) with a cue's sound. The
  sounds are a **cue pack** (`chimera_core::cues::CuePack`, TOML: cue
  key to a file in a game's install, loaded at run time), a game's part
  (`GameParts::cues`; CS:S's is stock sounds, data/cues.toml); a
  definition picks one (`cues = "<id>"`, `none`; default: the first
  registered). Plain CS:S and Combat Arms play no minigame, so neither
  changes.

## Multiplayer

Same principles as the main game: the server owns the loadout, rules and
outcomes; clients submit intent. Every player must have each game the loadout
uses mounted; the server checks this before a match. Plan:
[docs/plans/active/multiplayer.md](plans/active/multiplayer.md).

## Assets, code and legal

- Never ship other games' assets. Load them at runtime from the player's own
  install (the Garry's Mod model). Check each publisher's terms before a
  public release, especially Nexon/Valofe titles.
- Reverse engineering formats for interoperability is generally lawful; check
  each game's EULA.
- Facts and ideas (file layouts, values, formulas, how a mechanic behaves) are
  free to reuse. The specific code expressing them is not, and a line-by-line
  translation into Rust is still a copy.
- Workflow: a spec session works from reference material and writes down
  what the game does as a plain spec in our own words (formats, values,
  rules, edge cases); a separate session implements from the spec in this
  project's ECS architecture rather than mirroring the original's structure.
- Never commit third-party code, assets, extracted files or keys. The goal
  is a faithful recreation built without distributing anything we don't
  own; players load assets from their own install.

### Per-game notes

- **Counter-Strike: Source.** Steam app 240, Linux build. Tested build
  25338318: `de_dust2.bsp` is BSP version 20 (loose in `cstrike/maps/`); VPK
  directories are version 2 (`cstrike/cstrike_pak_dir.vpk`, and
  `hl2/hl2_textures_dir.vpk`, `hl2/hl2_misc_dir.vpk` for shared content).
  Source 1 formats (VPK, BSP, MDL/VVD/VTX,
  VTF/VMT) are well documented. Valve's public Source SDK 2013 includes the
  shared movement code. Weapon stats live in `weapon_*.txt` scripts. Faithful
  rendering needs BSP lightmaps and Source material rules.
- **Combat Arms.** LithTech Jupiter (`.rez` archives, LithTech model and
  world formats). Specs and the `games/combat_arms` library are in the
  repository since 2026-10-18 (clean room, no keys or assets); the plan
  is [plans/active/combat-arms.md](plans/active/combat-arms.md).

## Specs

Behavior worked out from reference material reaches this repository only as
specs in `specs/`, written in our own words: constants with units, formulas
as math, per-tick order of operations, edge cases, deliberate quirks, and
test cases with expected numbers. Specs are written in sessions that read the
reference material; implementation happens in separate sessions that see only
the specs. The test cases become automated tests. See
[specs/README.md](../specs/README.md).

## First steps (the original plan, 2026-10; kept for the record)

MVP: a Combat Arms slice. Run, sprint, shoot and fight bots on one map, built
through the slots so another game's controller is a loadout change. Gameplay
never waits on asset formats.

1. ~~Create a Bevy project in this repository.~~
2. Foundation: core components, `Intent`, loadout resource with one plugin per
   slot, a greybox map built in code, collision queries, debug tools (inspector,
   free-fly camera), placeholder movement.
3. Combat Arms movement from its spec, as a kinematic character controller.
4. One Combat Arms weapon from its spec as trigger/cost/delivery/effect parts,
   plus hitscan, hit zones, health, crosshair and ammo HUD.
5. Bots (intent from a brain on a waypoint graph, navmesh later) and a simple
   deathmatch ruleset with respawns.
6. Real maps, in parallel with 2-5: `de_dust2` from a local CS:S install (BSP
   geometry, VPK textures, lightmaps), then a Combat Arms map once `.rez` and
   the world format are specced.
7. Prove the swap: Source movement (specced from Source SDK 2013) as a second
   Movement implementation, switchable live against Combat Arms movement on
   the same map. MW2 follows the same path.

## Open questions

- Which third game tests the interfaces best: MW2 or Minecraft?
- How do players pick loadouts: fixed per minigame, chosen in a lobby, rolled
  by dice, or a mix?
