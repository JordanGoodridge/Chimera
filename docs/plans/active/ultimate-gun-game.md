# Plan: Ultimate Gun Game, games as optional parts

The gun game with guns and maps from every game an app composes: each
rung makes the player its gun's game (movement, body, HUD). A test bed
for splitting a game into parts it lends without a full app per game
(architecture-review.md A1/X1, N2, X7, L2, L3; "identity follows the
weapon").

Status (2026-10-24): stage 1 done (the contract as code, the capability
report, the automatic ladder, the apps, the fallbacks for HUD and body,
tests). What's left is at the end.

## The game parts contract

A game lends parts; a composed app reads each game's claim
(`chimera_core::game_parts::GameParts`, a game's `game::parts::PARTS`),
checks it against what it really composed
(`chimera_sim::minigame::auto::report` -> `Capabilities`, `parts` on the
console) and builds from what's there.

| Part | Required? | What provides it today | Fallback |
|---|---|---|---|
| Guns (weapon set, held/view models, fire/reload sounds, their rendering) | yes | the game's weapon set in `WeaponCatalog` (`CsGunsPlugin`, Combat Arms' data, `CsSourceGuns`/`CombatArmsGuns` for the weapons-only build), models via `MapSources::kept` and `guests` (X7), view models by the game's driver or Chimera's with `ViewAnimRules` | none: no guns, not in a ladder |
| Gun classes | yes (with the guns) | `GameParts::guns` (`GunClass`, the game's claim) | - |
| Knife | no | `GameParts::knife` (in the catalogue), given by game (`def::Kit::ByGame`, `runtime::refit_kit` on a rung's swap) | the base game's knife |
| Movement | yes | `slots::MovementRegistry` (`register_movement`; `of_game` for identity) | none: not in a ladder |
| Characters (bodies) | no | `map::CharacterModels` with `game` set: the map game's own, `MapSources::players`, `MapSources::guests` (`MapDressing`) | none of its game: the shell's capsule (`CharacterModels::for_identity` returns none, the body is removed) |
| HUD | no | the gameplay's `hud` slot (X1, `hud_is`), switched by `identity::hud_follows_identity`, kept up alone by the shell's `HudSwitchPlugin` | `PartsRegistry::fallback_hud` (the app's base game's; else `MINIMAL_HUD`, the shell's own) |
| Maps | no | the game's `MapLoader` in the app's `MapSources`, `GameParts::maps` | the other games' maps (the pool is every game's); none at all: the greybox |
| Bullet impacts (decals, debris, impact sounds) | no | the game's impact systems for what `weapon::impact::ImpactRoute` gives it (`register_impact_provider`), its looks on the map (`MapImpacts::looks`: its loader, a guest dressing), its surfaces by class (`register_surface_classes`, `map::impact::SurfaceClass`) | the map game's impact, else any game's whose looks the map carries, else Chimera's generic mark and puff (`client::effects::GenericImpactsPlugin`) |
| Pushes on physics objects | no | the game's bullets telling `weapon::impact::BulletImpulse` (N·s; CS:S its damage force, Combat Arms 2 N·s per hit point) | no push |
| Recoil | no | the game's own, on the shared `ViewPunch`, while `RecoilGame` (the held weapon's game, predicted) is it; a switch of game zeroes the punch | none |
| Install, binds, tick | the app's | `GameInfo` (`register_game`), `Gameplay` slots | the base game's |

Discovery is generic: `PartsRegistry` (a `Vec`, registration order,
`GamePartsAppExt::register_parts`), filled by the app that composes the
games (`lucker_party::GAME_PARTS` in `PartyPartsPlugin`); no game names
another, and a game with no `GameInfo` (the test kit's stand-in) takes
part too.

## The automatic gun game (`chimera_sim::minigame::auto`)

- Who plays: the games that are `playable` (guns and movement present in
  the app) and installed by every player (`chimera_net::minigame_net::
  installed_by_all`: each remote mount report, plus `LocalMounts`; a game
  a report doesn't list counts as there).
- Ladder: N rungs (`RUNGS` = 12), SplitMix64 from a seed; games taken in
  turn from a seeded start (a game gives a rung each turn while it has
  guns: spread), each rung's class the nearest to its place in the climb,
  no gun twice, sorted by class (stable).
- Pool: every installed game's maps, the games in turn (rotation
  alternates); none, the greybox.
- The definition is text (`MinigameDef`), so the network carries it as it
  carries any minigame's (L3's manifest: id, hash, text); `identity =
  "weapon"`, `chimera:ladder`, the base game's binds/tick/bots, the kit
  by game (each gun's game's knife, the base's for a game without one).
- The app: `apps/ultimate_gun_game` (`UltimateGunGamePlugin` on
  `lucker_party::client_app`) and `apps/ultimate_gun_game_server`
  (`lucker_party_server::run`). Made at `Plugin::finish`, again at
  `PostStartup` with this machine's installs, and on `ugg_new [seed]
  [rungs] [map]` for the players connected.
- Network (Chimera, generic): a definition made again under the same id
  is announced (`write_minigame` compares the definition, not only the
  id) and a client replays it (`apply_minigame`), so a new ladder reaches
  connected clients without a new id.

## Gaps this exposes

1. The party's glue is two app crates (`lucker_party`, `lucker_party_server`)
   that the Ultimate Gun Game's apps use as libraries: an app on an app,
   allowed by name in `checks/tests/it/architecture.rs`. The clean form is
   one party-kit crate (the two-game composition, map sources, client app,
   server run loop) under both; not done now because the gun-game agents
   edit `lucker_party` in parallel.
2. The composition still names both games (`PartyPartsPlugin`,
   `MAP_SOURCES`, `with_huds`): "whatever games are linked" means
   these two. A game-agnostic composer needs each game to export a
   bundle (its sim parts plugin, map loader, guest dressing, HUD plugins)
   behind one interface; `GameParts` describes, it doesn't build.
3. Gun classes are a hand list per game; weapon data has no class (`slot`
   numbers differ per game). Data-driven: a `class` key in the weapon TOML.
4. (Done after merging the per-game kit, `def::Kit::ByGame`.) The kit is
   only a knife: grenades or a class's sidearm per game aren't parts yet.
5. (Done in the shell: `client::hud_minimal::MinimalHudPlugin`, added by the
   `ultimate_gun_game` binary.) A drawn minimal HUD for `MINIMAL_HUD`: health,
   armour, held weapon id and clip/reserve, a crosshair, the minigame's lines,
   up only while the gameplay's HUD is the minimal one. The app's fallback
   stays the base game's (`BASE`); an app that sets `fallback_hud` to
   `MINIMAL_HUD` gets it for a game without a HUD. Weapon shown by id (no
   display-name registry in the shell).
6. Body fallback is the capsule only; "another game's body" needs
   `CharacterModels::for_identity` to take a fallback game.
7. Install filtering ignores the server's own installs and isn't re-run at
   rotation or when a player joins (only `ugg_new`); the client still
   stops without CS:S (`lucker_party::requirements`), the pool's first map
   being CS:S's.
8. Whether a claimed map really loads isn't checked (only that its game
   has a loader), and a guns-only game's guns on another game's map need
   a guest dressing to show models (X7's `guests` lists CS:S only).

## Next

- Rebuild the ladder at each rotation for the players then connected
  (gap 7).
- Weapon class in data (gap 3); display names in the minimal HUD.
- The party kit crate and a per-game bundle (gaps 1-2) when a third game
  arrives.
