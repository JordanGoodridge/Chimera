# Console map names and map cycle per game, as a resource

**Done 2026-10-30** (ecs-cleanup.md, step 10).

## Context

`chimera_client::client::console` caches the maps a game can load (`map`
completion) in `static MAP_NAMES` and the map cycle in a function-local
`static CYCLE`, each holding one game at a time and dropping it when
another game asks. Step 10 of
[docs/plans/active/ecs-cleanup.md](../plans/active/ecs-cleanup.md).

## Files

- `chimera/client/src/client/console.rs` (`MAP_NAMES`, `forget_map_names`,
  `map_names`, `map_cycle`, `follow_map_cycle`)
- Guards: `chimera/client` tests; `checks/tests/it/determinism.rs`

## Definition of done

- [x] A resource keyed by game id holding both lists, filled lazily as now;
      `import` (`forget_map_names`) clears it.
- [x] Both statics gone; ratchet lowered.
- [x] A unit test with a fake `GameInfo` whose `map_names` counts calls:
      asked twice, the install is read once; two games keep separate lists.

## Constraints

No change to what `map` completes or which map the cycle picks.

## Out of bounds

The server browser and map downloads.

## Size and labels

- Size: S
- Area: area:ui, area:engine
- Needs install: no
- good-first-task
